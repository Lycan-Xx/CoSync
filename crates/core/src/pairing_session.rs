//! Pairing handshake.
//!
//! Bridges the gap [`crate::transport::build_pairing_server_endpoint`]
//! leaves open: a connection accepted in pairing mode is encrypted but
//! not yet peer-*authenticated*. This module completes that — the client
//! sends a `PairingRequest` envelope carrying the one-time token from the
//! QR code; once the server confirms it, the connection's actual peer
//! certificate fingerprint becomes the value pinned for every future
//! reconnection.

use thiserror::Error;

use crate::cert::DeviceCertificate;
use crate::framing::{read_envelope, write_envelope, FramingError};
use crate::paired_devices::PairedDevice;
use crate::pairing::PairingPayload;
use crate::proto::{envelope::Payload, Envelope, PairingRequest};
use crate::transport::{
    build_client_endpoint, build_pairing_server_endpoint, Session, TransportError,
};

#[derive(Debug, Error)]
pub enum PairingError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Framing(#[from] FramingError),
    #[error("pairing token did not match")]
    TokenMismatch,
    #[error("first message on a pairing connection must be a PairingRequest")]
    UnexpectedMessage,
    #[error("peer did not present a certificate")]
    NoPeerCertificate,
    #[error("timed out waiting for a pairing request")]
    Timeout,
}

/// Server side (desktop, typically): listen once for a single incoming
/// pairing attempt, validate its token, and return the now-known
/// `PairedDevice` record — ready to be `upsert`ed into a
/// `PairedDeviceStore` — plus the live `Session` in case the caller wants
/// to keep talking immediately rather than reconnect.
///
/// `bind_addr` should be the same address advertised via mDNS and
/// embedded (as `ip_hint`/`port`) in the pairing QR payload.
pub async fn accept_pairing_connection(
    bind_addr: std::net::SocketAddr,
    cert: &DeviceCertificate,
    expected_token: &str,
    timeout: std::time::Duration,
) -> Result<(PairedDevice, Session), PairingError> {
    let endpoint = build_pairing_server_endpoint(bind_addr, cert)?;

    let accept_and_verify = async {
        let session = Session::accept(&endpoint)
            .await
            .ok_or(PairingError::Timeout)??;

        let (_send, mut recv) = session
            .connection
            .accept_bi()
            .await
            .map_err(TransportError::from)?;
        let envelope = read_envelope(&mut recv).await?;

        let request = match envelope.payload {
            Some(Payload::PairingRequest(request)) => request,
            _ => return Err(PairingError::UnexpectedMessage),
        };

        if request.pairing_token != expected_token {
            return Err(PairingError::TokenMismatch);
        }

        let fingerprint = session
            .peer_fingerprint()
            .ok_or(PairingError::NoPeerCertificate)?;

        let device = PairedDevice {
            device_id: envelope.device_id,
            device_name: request.device_name,
            cert_fingerprint: fingerprint,
            last_known_ip: Some(session.connection.remote_address().ip().to_string()),
            last_known_port: Some(session.connection.remote_address().port()),
        };

        Ok((device, session))
    };

    tokio::time::timeout(timeout, accept_and_verify)
        .await
        .map_err(|_| PairingError::Timeout)?
}

/// Client side (phone, typically): dial the device described by a scanned
/// `PairingPayload`, present this device's own identity, and send the
/// `PairingRequest` proving we hold the token from the QR code.
///
/// The client already knows (from the QR) the server's fingerprint, so
/// this reuses the normal, strict [`build_client_endpoint`] — only the
/// *server's* side of a first-time pairing is TOFU; the client's view is
/// pinned from the very first packet.
pub async fn dial_and_send_pairing_request(
    my_device_id: &str,
    my_device_name: &str,
    my_cert: &DeviceCertificate,
    payload: &PairingPayload,
) -> Result<Session, PairingError> {
    let server_addr: std::net::SocketAddr =
        format!("{}:{}", payload.ip_hint, payload.port)
            .parse()
            .map_err(|_| PairingError::UnexpectedMessage)?;

    let endpoint = build_client_endpoint(
        "0.0.0.0:0".parse().unwrap(),
        my_cert,
        &payload.public_key_fingerprint,
    )?;

    let session = Session::connect(&endpoint, server_addr, "cosync.local").await?;

    let (mut send, _recv) = session
        .connection
        .open_bi()
        .await
        .map_err(TransportError::from)?;

    let request = Envelope {
        device_id: my_device_id.to_string(),
        logical_time: 0,
        physical_time_ms: 0,
        payload: Some(Payload::PairingRequest(PairingRequest {
            device_name: my_device_name.to_string(),
            public_key_fingerprint: my_cert.fingerprint(),
            pairing_token: payload.pairing_token.clone(),
        })),
    };
    write_envelope(&mut send, &request).await?;
    send.finish().await.ok();

    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_correct_token_completes_pairing_and_captures_the_real_fingerprint() {
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let server_cert = DeviceCertificate::generate().expect("server cert");
            let client_cert = DeviceCertificate::generate().expect("client cert");

            // Bind the server first so we know the real port for the payload.
            let probe = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            let bind_addr = probe.local_addr().unwrap();
            drop(probe);

            let token = "one-time-token-xyz".to_string();
            let server_task = tokio::spawn({
                let server_cert = server_cert.clone();
                let token = token.clone();
                async move {
                    accept_pairing_connection(
                        bind_addr,
                        &server_cert,
                        &token,
                        std::time::Duration::from_secs(5),
                    )
                    .await
                }
            });

            // Give the server a moment to actually bind before the client dials.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let payload = PairingPayload {
                device_name: "Sani's Desktop".to_string(),
                public_key_fingerprint: server_cert.fingerprint(),
                ip_hint: bind_addr.ip().to_string(),
                port: bind_addr.port(),
                pairing_token: token,
            };

            dial_and_send_pairing_request("phone-device-id", "Sani's Phone", &client_cert, &payload)
                .await
                .expect("client dial + pairing request send");

            let (paired_device, _session) = server_task
                .await
                .expect("server task")
                .expect("pairing succeeds");

            assert_eq!(paired_device.device_id, "phone-device-id");
            assert_eq!(paired_device.device_name, "Sani's Phone");
            // The crucial assertion: the server learned the CLIENT's real
            // fingerprint from the live connection, not from anything the
            // client merely claimed in the message body.
            assert_eq!(paired_device.cert_fingerprint, client_cert.fingerprint());
        })
        .await;
        outcome.expect("test must complete within 10s, not hang");
    }

    #[tokio::test]
    async fn a_wrong_token_is_rejected() {
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let server_cert = DeviceCertificate::generate().expect("server cert");
            let client_cert = DeviceCertificate::generate().expect("client cert");

            let probe = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            let bind_addr = probe.local_addr().unwrap();
            drop(probe);

            let server_task = tokio::spawn({
                let server_cert = server_cert.clone();
                async move {
                    accept_pairing_connection(
                        bind_addr,
                        &server_cert,
                        "the-real-token",
                        std::time::Duration::from_secs(5),
                    )
                    .await
                }
            });

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let payload = PairingPayload {
                device_name: "Sani's Desktop".to_string(),
                public_key_fingerprint: server_cert.fingerprint(),
                ip_hint: bind_addr.ip().to_string(),
                port: bind_addr.port(),
                pairing_token: "a-guessed-wrong-token".to_string(),
            };

            dial_and_send_pairing_request("phone-device-id", "Sani's Phone", &client_cert, &payload)
                .await
                .expect("client can still dial and send — rejection happens server-side");

            let result = server_task.await.expect("server task");
            assert!(matches!(result, Err(PairingError::TokenMismatch)));
        })
        .await;
        outcome.expect("test must complete within 10s, not hang");
    }
}
