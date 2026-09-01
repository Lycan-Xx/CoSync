//! Trusted-session resumption after the one-time QR pairing has completed.
//!
//! TLS rejects certificates outside the desktop's persisted trust set. A
//! tiny request/ack exchange then proves that both peers have reached the
//! steady-state Cosync protocol before either UI reports Connected.

use std::net::SocketAddr;
use std::time::Duration;

use thiserror::Error;

use crate::framing::{read_envelope, write_envelope, FramingError};
use crate::proto::{envelope::Payload, Ack, Envelope, Heartbeat};
use crate::transport::{Session, TransportError};

const RECONNECT_ACK_REF_ID: &str = "reconnect";

#[derive(Debug, Error)]
pub enum ReconnectError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Framing(#[from] FramingError),
    #[error("peer did not present a certificate")]
    NoPeerCertificate,
    #[error("timed out completing trusted reconnect")]
    Timeout,
    #[error("first message on a reconnect must be a heartbeat")]
    UnexpectedMessage,
    #[error("reconnect device identity did not match its TLS certificate")]
    PeerIdentityMismatch,
    #[error("trusted reconnect was rejected: {0}")]
    Rejected(String),
}

async fn write_ack(
    send: &mut quinn::SendStream,
    success: bool,
    error: &str,
) -> Result<(), ReconnectError> {
    write_envelope(
        send,
        &Envelope {
            device_id: String::new(),
            logical_time: 0,
            physical_time_ms: 0,
            payload: Some(Payload::Ack(Ack {
                ref_id: RECONNECT_ACK_REF_ID.to_string(),
                success,
                error: error.to_string(),
            })),
        },
    )
    .await?;
    send.finish().await.map_err(FramingError::from)?;
    Ok(())
}

/// Complete a reconnect accepted by a TLS endpoint whose client verifier is
/// backed by the desktop's persisted certificate trust set.
pub async fn accept_reconnect_incoming(
    incoming: quinn::Connecting,
    timeout: Duration,
) -> Result<(String, Session), ReconnectError> {
    tokio::time::timeout(timeout, async move {
        let connection = incoming.await.map_err(TransportError::from)?;
        let session = Session { connection };
        let fingerprint = session
            .peer_fingerprint()
            .ok_or(ReconnectError::NoPeerCertificate)?;
        let (mut send, mut recv) = session
            .connection
            .accept_bi()
            .await
            .map_err(TransportError::from)?;
        let envelope = read_envelope(&mut recv).await?;

        if !matches!(envelope.payload, Some(Payload::Heartbeat(Heartbeat {}))) {
            let _ = write_ack(&mut send, false, "invalid reconnect request").await;
            return Err(ReconnectError::UnexpectedMessage);
        }
        if envelope.device_id != fingerprint {
            let _ = write_ack(&mut send, false, "device identity did not match").await;
            return Err(ReconnectError::PeerIdentityMismatch);
        }

        write_ack(&mut send, true, "").await?;
        Ok((fingerprint, session))
    })
    .await
    .map_err(|_| ReconnectError::Timeout)?
}

/// Dial a previously paired desktop. The caller owns `endpoint` for the
/// returned session's lifetime and has already configured it to pin the
/// desktop certificate fingerprint.
pub async fn dial_reconnect(
    endpoint: &quinn::Endpoint,
    addr: SocketAddr,
    my_device_id: &str,
    timeout: Duration,
) -> Result<Session, ReconnectError> {
    tokio::time::timeout(timeout, async move {
        let session = Session::connect(endpoint, addr, "cosync.local").await?;
        let (mut send, mut recv) = session
            .connection
            .open_bi()
            .await
            .map_err(TransportError::from)?;
        write_envelope(
            &mut send,
            &Envelope {
                device_id: my_device_id.to_string(),
                logical_time: 0,
                physical_time_ms: 0,
                payload: Some(Payload::Heartbeat(Heartbeat {})),
            },
        )
        .await?;
        send.finish().await.map_err(FramingError::from)?;

        let envelope = read_envelope(&mut recv).await?;
        match envelope.payload {
            Some(Payload::Ack(ack)) if ack.ref_id == RECONNECT_ACK_REF_ID && ack.success => {
                Ok(session)
            }
            Some(Payload::Ack(ack)) if ack.ref_id == RECONNECT_ACK_REF_ID => {
                Err(ReconnectError::Rejected(ack.error))
            }
            _ => Err(ReconnectError::UnexpectedMessage),
        }
    })
    .await
    .map_err(|_| ReconnectError::Timeout)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cert::DeviceCertificate;
    use crate::transport::{build_client_endpoint, build_trusted_server_endpoint};
    use crate::verifier::TrustedClientFingerprints;

    #[tokio::test]
    async fn trusted_peer_completes_reconnect_probe() {
        let outcome = tokio::time::timeout(Duration::from_secs(10), async {
            let server_cert = DeviceCertificate::generate().expect("server cert");
            let client_cert = DeviceCertificate::generate().expect("client cert");
            let client_fingerprint = client_cert.fingerprint();
            let trusted = TrustedClientFingerprints::new([client_fingerprint.clone()]);
            let server = build_trusted_server_endpoint(
                "127.0.0.1:0".parse().unwrap(),
                &server_cert,
                trusted,
            )
            .expect("server endpoint");
            let server_addr = server.local_addr().expect("server address");
            let client = build_client_endpoint(
                "127.0.0.1:0".parse().unwrap(),
                &client_cert,
                &server_cert.fingerprint(),
            )
            .expect("client endpoint");

            let server_task = tokio::spawn(async move {
                let incoming = server.accept().await.expect("incoming");
                accept_reconnect_incoming(incoming, Duration::from_secs(5))
                    .await
                    .expect("accept reconnect")
            });
            let client_session = dial_reconnect(
                &client,
                server_addr,
                &client_fingerprint,
                Duration::from_secs(5),
            )
            .await
            .expect("dial reconnect");
            let (accepted_fingerprint, server_session) = server_task.await.expect("server task");

            assert_eq!(accepted_fingerprint, client_fingerprint);
            assert!(client_session.connection.close_reason().is_none());
            assert!(server_session.connection.close_reason().is_none());
        })
        .await;
        outcome.expect("test must not hang");
    }
}
