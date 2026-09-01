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
use crate::proto::{envelope::Payload, Ack, Envelope, PairingRequest};
use crate::transport::{build_client_endpoint, Session, TransportError};

const PAIRING_ACK_REF_ID: &str = "pairing";
const PAIRING_ACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

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
    #[error("pairing was rejected: {0}")]
    Rejected(String),
    #[error("pairing peer returned an invalid acknowledgement")]
    UnexpectedAcknowledgement,
    #[error("timed out waiting for the desktop to acknowledge pairing")]
    AcknowledgementTimeout,
    #[error("pairing request certificate fingerprint did not match the TLS peer certificate")]
    PeerFingerprintMismatch,
}

/// A validated pairing request that is waiting for the desktop to persist
/// the device before success is reported to the client.
pub struct PendingPairing {
    device: PairedDevice,
    session: Session,
    response: quinn::SendStream,
}

impl PendingPairing {
    pub fn device(&self) -> &PairedDevice {
        &self.device
    }

    pub async fn acknowledge(mut self) -> Result<(PairedDevice, Session), PairingError> {
        write_pairing_ack(&mut self.response, true, "").await?;
        self.response.finish().await.map_err(FramingError::from)?;
        Ok((self.device, self.session))
    }

    pub async fn reject(mut self, error: &str) -> Result<(), PairingError> {
        write_pairing_ack(&mut self.response, false, error).await?;
        self.response.finish().await.map_err(FramingError::from)?;
        Ok(())
    }
}

async fn write_pairing_ack(
    send: &mut quinn::SendStream,
    success: bool,
    error: &str,
) -> Result<(), PairingError> {
    write_envelope(
        send,
        &Envelope {
            device_id: String::new(),
            logical_time: 0,
            physical_time_ms: 0,
            payload: Some(Payload::Ack(Ack {
                ref_id: PAIRING_ACK_REF_ID.to_string(),
                success,
                error: error.to_string(),
            })),
        },
    )
    .await?;
    Ok(())
}

pub async fn read_pairing_ack(recv: &mut quinn::RecvStream) -> Result<(), PairingError> {
    read_pairing_ack_with_timeout(recv, PAIRING_ACK_TIMEOUT).await
}

async fn read_pairing_ack_with_timeout(
    recv: &mut quinn::RecvStream,
    timeout: std::time::Duration,
) -> Result<(), PairingError> {
    let envelope = tokio::time::timeout(timeout, read_envelope(recv))
        .await
        .map_err(|_| PairingError::AcknowledgementTimeout)??;
    match envelope.payload {
        Some(Payload::Ack(ack)) if ack.ref_id == PAIRING_ACK_REF_ID && ack.success => Ok(()),
        Some(Payload::Ack(ack)) if ack.ref_id == PAIRING_ACK_REF_ID => {
            let error = if ack.error.is_empty() {
                "desktop rejected the pairing request".to_string()
            } else {
                ack.error
            };
            Err(PairingError::Rejected(error))
        }
        _ => Err(PairingError::UnexpectedAcknowledgement),
    }
}

/// Server side (desktop, typically): listen once for a single incoming
/// pairing attempt, validate its token, and return the now-known
/// `PairedDevice` record — ready to be `upsert`ed into a
/// `PairedDeviceStore` — plus the live `Session` in case the caller wants
/// to keep talking immediately rather than reconnect.
///
/// The caller owns `endpoint` for the lifetime of the listener. Keeping one
/// endpoint alive across QR-token changes avoids racing an asynchronous UDP
/// socket shutdown against an immediate rebind of the same fixed port.
pub async fn accept_pairing_connection(
    endpoint: &quinn::Endpoint,
    expected_token: &str,
    timeout: std::time::Duration,
) -> Result<PendingPairing, PairingError> {
    let accept_and_verify = async {
        let incoming = endpoint.accept().await.ok_or(PairingError::Timeout)?;
        validate_pairing_incoming(incoming, expected_token).await
    };

    tokio::time::timeout(timeout, accept_and_verify)
        .await
        .map_err(|_| PairingError::Timeout)?
}

/// Complete and validate one connection already accepted by a long-lived
/// pairing endpoint. Keeping endpoint acceptance separate lets desktop hosts
/// process several bounded handshakes concurrently, so a peer that connects
/// and then stalls cannot monopolize the listener.
pub async fn accept_pairing_incoming(
    incoming: quinn::Connecting,
    expected_token: &str,
    timeout: std::time::Duration,
) -> Result<PendingPairing, PairingError> {
    tokio::time::timeout(timeout, validate_pairing_incoming(incoming, expected_token))
        .await
        .map_err(|_| PairingError::Timeout)?
}

async fn validate_pairing_incoming(
    incoming: quinn::Connecting,
    expected_token: &str,
) -> Result<PendingPairing, PairingError> {
    let connection = incoming.await.map_err(TransportError::from)?;
    let session = Session { connection };

    let (mut send, mut recv) = session
        .connection
        .accept_bi()
        .await
        .map_err(TransportError::from)?;
    let envelope = read_envelope(&mut recv).await?;

    let request = match envelope.payload {
        Some(Payload::PairingRequest(request)) => request,
        _ => {
            let _ = write_pairing_ack(&mut send, false, "invalid pairing request").await;
            let _ = send.finish().await;
            return Err(PairingError::UnexpectedMessage);
        }
    };

    if request.pairing_token != expected_token {
        let _ = write_pairing_ack(&mut send, false, "pairing token did not match").await;
        let _ = send.finish().await;
        return Err(PairingError::TokenMismatch);
    }

    let fingerprint = session
        .peer_fingerprint()
        .ok_or(PairingError::NoPeerCertificate)?;

    if request.public_key_fingerprint != fingerprint {
        let _ = write_pairing_ack(
            &mut send,
            false,
            "pairing certificate fingerprint did not match",
        )
        .await;
        let _ = send.finish().await;
        return Err(PairingError::PeerFingerprintMismatch);
    }

    let device = PairedDevice {
        // The transport certificate is the trust anchor (ADR-005), so it
        // is also the collision-resistant device key. The request's
        // device_id is descriptive protocol data, not trusted identity.
        device_id: fingerprint.clone(),
        device_name: request.device_name,
        cert_fingerprint: fingerprint,
        last_known_ip: Some(session.connection.remote_address().ip().to_string()),
        last_known_port: Some(session.connection.remote_address().port()),
    };

    Ok(PendingPairing {
        device,
        session,
        response: send,
    })
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
    dial_and_send_pairing_request_with_ack_timeout(
        my_device_id,
        my_device_name,
        my_cert,
        payload,
        PAIRING_ACK_TIMEOUT,
    )
    .await
}

async fn dial_and_send_pairing_request_with_ack_timeout(
    my_device_id: &str,
    my_device_name: &str,
    my_cert: &DeviceCertificate,
    payload: &PairingPayload,
    ack_timeout: std::time::Duration,
) -> Result<Session, PairingError> {
    let server_addr: std::net::SocketAddr = format!("{}:{}", payload.ip_hint, payload.port)
        .parse()
        .map_err(|_| PairingError::UnexpectedMessage)?;

    let endpoint = build_client_endpoint(
        "0.0.0.0:0".parse().unwrap(),
        my_cert,
        &payload.public_key_fingerprint,
    )?;

    let session = Session::connect(&endpoint, server_addr, "cosync.local").await?;

    let (mut send, mut recv) = session
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
    send.finish().await.map_err(FramingError::from)?;
    read_pairing_ack_with_timeout(&mut recv, ack_timeout).await?;

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

            let server_endpoint = crate::transport::build_pairing_server_endpoint(
                "127.0.0.1:0".parse().unwrap(),
                &server_cert,
            )
            .expect("pairing server endpoint");
            let bind_addr = server_endpoint.local_addr().unwrap();

            let token = "one-time-token-xyz".to_string();
            let server_task = tokio::spawn({
                let server_endpoint = server_endpoint.clone();
                let token = token.clone();
                async move {
                    let pending = accept_pairing_connection(
                        &server_endpoint,
                        &token,
                        std::time::Duration::from_secs(5),
                    )
                    .await?;
                    pending.acknowledge().await
                }
            });

            let payload = PairingPayload {
                device_name: "Sani's Desktop".to_string(),
                public_key_fingerprint: server_cert.fingerprint(),
                ip_hint: bind_addr.ip().to_string(),
                port: bind_addr.port(),
                pairing_token: token,
            };

            let _client_session = dial_and_send_pairing_request(
                "phone-device-id",
                "Sani's Phone",
                &client_cert,
                &payload,
            )
            .await
            .expect("client dial + pairing request send");

            let (paired_device, session) = server_task
                .await
                .expect("server task")
                .expect("pairing succeeds");

            assert_eq!(paired_device.device_id, client_cert.fingerprint());
            assert_eq!(paired_device.device_name, "Sani's Phone");
            // The crucial assertion: the server learned the CLIENT's real
            // fingerprint from the live connection, not from anything the
            // client merely claimed in the message body.
            assert_eq!(paired_device.cert_fingerprint, client_cert.fingerprint());
            assert!(session.connection.close_reason().is_none());
        })
        .await;
        outcome.expect("test must complete within 10s, not hang");
    }

    #[tokio::test]
    async fn a_wrong_token_is_rejected() {
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let server_cert = DeviceCertificate::generate().expect("server cert");
            let client_cert = DeviceCertificate::generate().expect("client cert");

            let server_endpoint = crate::transport::build_pairing_server_endpoint(
                "127.0.0.1:0".parse().unwrap(),
                &server_cert,
            )
            .expect("pairing server endpoint");
            let bind_addr = server_endpoint.local_addr().unwrap();

            let server_task = tokio::spawn({
                let server_endpoint = server_endpoint.clone();
                async move {
                    accept_pairing_connection(
                        &server_endpoint,
                        "the-real-token",
                        std::time::Duration::from_secs(5),
                    )
                    .await
                }
            });

            let payload = PairingPayload {
                device_name: "Sani's Desktop".to_string(),
                public_key_fingerprint: server_cert.fingerprint(),
                ip_hint: bind_addr.ip().to_string(),
                port: bind_addr.port(),
                pairing_token: "a-guessed-wrong-token".to_string(),
            };

            let client_result = dial_and_send_pairing_request(
                "phone-device-id",
                "Sani's Phone",
                &client_cert,
                &payload,
            )
            .await;
            assert!(matches!(client_result, Err(PairingError::Rejected(_))));

            let result = server_task.await.expect("server task");
            assert!(matches!(result, Err(PairingError::TokenMismatch)));
        })
        .await;
        outcome.expect("test must complete within 10s, not hang");
    }

    #[tokio::test]
    async fn desktop_can_reject_after_validation_before_reporting_success() {
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let server_cert = DeviceCertificate::generate().expect("server cert");
            let client_cert = DeviceCertificate::generate().expect("client cert");
            let server_endpoint = crate::transport::build_pairing_server_endpoint(
                "127.0.0.1:0".parse().unwrap(),
                &server_cert,
            )
            .expect("pairing server endpoint");
            let bind_addr = server_endpoint.local_addr().unwrap();
            let token = "valid-token".to_string();

            let server_task = tokio::spawn({
                let server_endpoint = server_endpoint.clone();
                let token = token.clone();
                async move {
                    let pending = accept_pairing_connection(
                        &server_endpoint,
                        &token,
                        std::time::Duration::from_secs(5),
                    )
                    .await?;
                    pending.reject("desktop persistence failed").await
                }
            });
            let payload = PairingPayload {
                device_name: "Desktop".to_string(),
                public_key_fingerprint: server_cert.fingerprint(),
                ip_hint: bind_addr.ip().to_string(),
                port: bind_addr.port(),
                pairing_token: token,
            };

            let client_result =
                dial_and_send_pairing_request("phone", "Phone", &client_cert, &payload).await;
            assert!(matches!(
                client_result,
                Err(PairingError::Rejected(error)) if error == "desktop persistence failed"
            ));
            server_task
                .await
                .expect("server task")
                .expect("server sends rejection acknowledgement");
        })
        .await;
        outcome.expect("test must complete within 10s, not hang");
    }

    #[tokio::test]
    async fn cancelling_an_accept_keeps_the_pairing_endpoint_reusable() {
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let server_cert = DeviceCertificate::generate().expect("server cert");
            let client_cert = DeviceCertificate::generate().expect("client cert");
            let server_endpoint = crate::transport::build_pairing_server_endpoint(
                "127.0.0.1:0".parse().unwrap(),
                &server_cert,
            )
            .expect("pairing server endpoint");
            let bind_addr = server_endpoint.local_addr().unwrap();

            {
                let stale_accept = accept_pairing_connection(
                    &server_endpoint,
                    "stale-token",
                    std::time::Duration::from_secs(5),
                );
                tokio::pin!(stale_accept);
                tokio::select! {
                    _ = &mut stale_accept => panic!("accept should still be waiting"),
                    _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
                }
            }

            let token = "fresh-token".to_string();
            let server_task = tokio::spawn({
                let server_endpoint = server_endpoint.clone();
                let token = token.clone();
                async move {
                    let pending = accept_pairing_connection(
                        &server_endpoint,
                        &token,
                        std::time::Duration::from_secs(5),
                    )
                    .await?;
                    pending.acknowledge().await
                }
            });
            let payload = PairingPayload {
                device_name: "Desktop".to_string(),
                public_key_fingerprint: server_cert.fingerprint(),
                ip_hint: bind_addr.ip().to_string(),
                port: bind_addr.port(),
                pairing_token: token,
            };

            dial_and_send_pairing_request("phone", "Phone", &client_cert, &payload)
                .await
                .expect("fresh token should pair on the same endpoint");
            server_task
                .await
                .expect("server task")
                .expect("server accepts fresh token");
        })
        .await;
        outcome.expect("test must complete within 10s, not hang");
    }

    #[tokio::test]
    async fn pairing_acknowledgement_has_a_deadline_even_with_a_live_connection() {
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let server_cert = DeviceCertificate::generate().expect("server cert");
            let client_cert = DeviceCertificate::generate().expect("client cert");
            let server_endpoint = crate::transport::build_pairing_server_endpoint(
                "127.0.0.1:0".parse().unwrap(),
                &server_cert,
            )
            .expect("pairing server endpoint");
            let bind_addr = server_endpoint.local_addr().unwrap();
            let token = "ack-timeout-token".to_string();

            let server_task = tokio::spawn({
                let server_endpoint = server_endpoint.clone();
                let token = token.clone();
                async move {
                    accept_pairing_connection(
                        &server_endpoint,
                        &token,
                        std::time::Duration::from_secs(5),
                    )
                    .await
                }
            });
            let client_endpoint = build_client_endpoint(
                "127.0.0.1:0".parse().unwrap(),
                &client_cert,
                &server_cert.fingerprint(),
            )
            .expect("client endpoint");
            let session = Session::connect(&client_endpoint, bind_addr, "cosync.local")
                .await
                .expect("client connects");
            let (mut send, mut recv) = session
                .connection
                .open_bi()
                .await
                .expect("open pairing stream");
            write_envelope(
                &mut send,
                &Envelope {
                    device_id: client_cert.fingerprint(),
                    logical_time: 0,
                    physical_time_ms: 0,
                    payload: Some(Payload::PairingRequest(PairingRequest {
                        device_name: "Phone".to_string(),
                        public_key_fingerprint: client_cert.fingerprint(),
                        pairing_token: token,
                    })),
                },
            )
            .await
            .expect("write request");
            send.finish().await.expect("finish request");

            let pending = server_task
                .await
                .expect("server task")
                .expect("server validates request");
            let client_result = read_pairing_ack_with_timeout(
                &mut recv,
                std::time::Duration::from_millis(25),
            )
            .await;
            assert!(matches!(
                client_result,
                Err(PairingError::AcknowledgementTimeout)
            ));

            // The pending server response deliberately stays alive until the
            // client deadline fires, proving QUIC keep-alives cannot hang it.
            drop(pending);
        })
        .await;
        outcome.expect("test must complete within 10s, not hang");
    }

    #[tokio::test]
    async fn a_stalled_peer_does_not_block_the_next_incoming_pairing() {
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let server_cert = DeviceCertificate::generate().expect("server cert");
            let stalled_cert = DeviceCertificate::generate().expect("stalled client cert");
            let valid_cert = DeviceCertificate::generate().expect("valid client cert");
            let server_endpoint = crate::transport::build_pairing_server_endpoint(
                "127.0.0.1:0".parse().unwrap(),
                &server_cert,
            )
            .expect("pairing server endpoint");
            let bind_addr = server_endpoint.local_addr().unwrap();
            let token = "concurrent-token".to_string();

            let server_task = tokio::spawn({
                let server_endpoint = server_endpoint.clone();
                let token = token.clone();
                async move {
                    let stalled_incoming = server_endpoint
                        .accept()
                        .await
                        .expect("stalled incoming connection");
                    let stalled_task = tokio::spawn({
                        let token = token.clone();
                        async move {
                            accept_pairing_incoming(
                                stalled_incoming,
                                &token,
                                std::time::Duration::from_millis(200),
                            )
                            .await
                        }
                    });

                    let valid_incoming = server_endpoint
                        .accept()
                        .await
                        .expect("valid incoming connection");
                    let pending = accept_pairing_incoming(
                        valid_incoming,
                        &token,
                        std::time::Duration::from_secs(5),
                    )
                    .await?;
                    let accepted = pending.acknowledge().await?;
                    assert!(matches!(
                        stalled_task.await.expect("stalled task"),
                        Err(PairingError::Timeout)
                    ));
                    Ok::<_, PairingError>(accepted)
                }
            });

            let stalled_endpoint = build_client_endpoint(
                "127.0.0.1:0".parse().unwrap(),
                &stalled_cert,
                &server_cert.fingerprint(),
            )
            .expect("stalled client endpoint");
            let _stalled_session = Session::connect(
                &stalled_endpoint,
                bind_addr,
                "cosync.local",
            )
            .await
            .expect("stalled client connects without opening a stream");

            let payload = PairingPayload {
                device_name: "Desktop".to_string(),
                public_key_fingerprint: server_cert.fingerprint(),
                ip_hint: bind_addr.ip().to_string(),
                port: bind_addr.port(),
                pairing_token: token,
            };
            dial_and_send_pairing_request("request-id", "Valid Phone", &valid_cert, &payload)
                .await
                .expect("valid peer pairs while first peer is stalled");

            let (paired_device, _) = server_task
                .await
                .expect("server task")
                .expect("valid pairing succeeds");
            assert_eq!(paired_device.device_id, valid_cert.fingerprint());
        })
        .await;
        outcome.expect("test must complete within 10s, not hang");
    }

    #[tokio::test]
    async fn claimed_certificate_fingerprint_cannot_replace_authenticated_identity() {
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let server_cert = DeviceCertificate::generate().expect("server cert");
            let client_cert = DeviceCertificate::generate().expect("client cert");
            let server_endpoint = crate::transport::build_pairing_server_endpoint(
                "127.0.0.1:0".parse().unwrap(),
                &server_cert,
            )
            .expect("pairing server endpoint");
            let bind_addr = server_endpoint.local_addr().unwrap();
            let token = "fingerprint-token".to_string();

            let server_task = tokio::spawn({
                let server_endpoint = server_endpoint.clone();
                let token = token.clone();
                async move {
                    accept_pairing_connection(
                        &server_endpoint,
                        &token,
                        std::time::Duration::from_secs(5),
                    )
                    .await
                }
            });
            let client_endpoint = build_client_endpoint(
                "127.0.0.1:0".parse().unwrap(),
                &client_cert,
                &server_cert.fingerprint(),
            )
            .expect("client endpoint");
            let session = Session::connect(&client_endpoint, bind_addr, "cosync.local")
                .await
                .expect("client connects");
            let (mut send, mut recv) = session
                .connection
                .open_bi()
                .await
                .expect("open pairing stream");
            write_envelope(
                &mut send,
                &Envelope {
                    device_id: "existing-device-id".to_string(),
                    logical_time: 0,
                    physical_time_ms: 0,
                    payload: Some(Payload::PairingRequest(PairingRequest {
                        device_name: "Impostor".to_string(),
                        public_key_fingerprint: "00".repeat(32),
                        pairing_token: token,
                    })),
                },
            )
            .await
            .expect("write request");
            send.finish().await.expect("finish request");

            assert!(matches!(
                read_pairing_ack(&mut recv).await,
                Err(PairingError::Rejected(_))
            ));
            assert!(matches!(
                server_task.await.expect("server task"),
                Err(PairingError::PeerFingerprintMismatch)
            ));
        })
        .await;
        outcome.expect("test must complete within 10s, not hang");
    }
}
