//! Wire framing.
//!
//! Protobuf messages don't self-delimit, so every `Envelope` sent over a
//! QUIC stream is prefixed with its length: a 4-byte big-endian `u32`
//! followed by that many bytes of encoded `Envelope`. Nothing fancier is
//! needed — QUIC streams are already ordered and reliable.

use prost::Message;
use thiserror::Error;

use crate::proto::Envelope;

/// Above this, refuse to even attempt an allocation — a malformed or
/// hostile length prefix should error out immediately, not try to
/// allocate a multi-gigabyte buffer. Individual payloads (file chunks
/// etc.) are always well under this in the actual protocol design.
const MAX_ENVELOPE_BYTES: u32 = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum FramingError {
    #[error("failed to write a framed envelope: {0}")]
    Write(#[from] quinn::WriteError),
    #[error("failed to read a framed envelope: {0}")]
    Read(#[from] quinn::ReadExactError),
    #[error("declared envelope length {0} exceeds the maximum of {MAX_ENVELOPE_BYTES}")]
    TooLarge(u32),
    #[error("failed to decode envelope: {0}")]
    Decode(#[from] prost::DecodeError),
}

pub async fn write_envelope(
    send: &mut quinn::SendStream,
    envelope: &Envelope,
) -> Result<(), FramingError> {
    let bytes = envelope.encode_to_vec();
    let len = bytes.len() as u32;
    send.write_all(&len.to_be_bytes()).await?;
    send.write_all(&bytes).await?;
    Ok(())
}

pub async fn read_envelope(recv: &mut quinn::RecvStream) -> Result<Envelope, FramingError> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_ENVELOPE_BYTES {
        return Err(FramingError::TooLarge(len));
    }

    let mut body = vec![0u8; len as usize];
    recv.read_exact(&mut body).await?;
    Ok(Envelope::decode(body.as_slice())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{envelope::Payload, Heartbeat};

    #[tokio::test]
    async fn round_trips_an_envelope_over_a_real_quic_stream() {
        use crate::cert::DeviceCertificate;
        use crate::transport::{build_client_endpoint, build_server_endpoint, Session};

        let server_cert = DeviceCertificate::generate().expect("server cert");
        let client_cert = DeviceCertificate::generate().expect("client cert");

        let server_endpoint = build_server_endpoint(
            "127.0.0.1:0".parse().unwrap(),
            &server_cert,
            &client_cert.fingerprint(),
        )
        .expect("server endpoint");
        let server_addr = server_endpoint.local_addr().unwrap();

        let client_endpoint = build_client_endpoint(
            "127.0.0.1:0".parse().unwrap(),
            &client_cert,
            &server_cert.fingerprint(),
        )
        .expect("client endpoint");

        let server_task = tokio::spawn(async move {
            let session = Session::accept(&server_endpoint)
                .await
                .unwrap()
                .expect("handshake ok");
            let (_send, mut recv) = session.connection.accept_bi().await.expect("accept stream");
            read_envelope(&mut recv).await.expect("read envelope")
        });

        let client_session = Session::connect(&client_endpoint, server_addr, "cosync.local")
            .await
            .expect("client connects");
        let (mut send, _recv) = client_session
            .connection
            .open_bi()
            .await
            .expect("open stream");

        let sent = Envelope {
            device_id: "device-abc".to_string(),
            logical_time: 1,
            physical_time_ms: 1_725_000_000_000,
            payload: Some(Payload::Heartbeat(Heartbeat {})),
        };
        write_envelope(&mut send, &sent).await.expect("write envelope");
        send.finish().await.ok();

        let received = tokio::time::timeout(std::time::Duration::from_secs(5), server_task)
            .await
            .expect("no hang")
            .expect("server task");
        assert_eq!(received, sent);
    }
}
