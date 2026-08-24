//! QUIC transport.
//!
//! Builds quinn client/server endpoints configured with mutual TLS 1.3 and
//! [`crate::verifier`]'s pinned-fingerprint verification instead of CA
//! validation. A [`Session`] is what Milestone 5+ actually sends
//! `Envelope`s over — this module only gets two devices to a live,
//! authenticated connection; it doesn't know about clipboard, files, or
//! anything else on top.

use std::net::SocketAddr;
use std::sync::Arc;

use thiserror::Error;

use crate::cert::DeviceCertificate;
use crate::verifier::{PinnedClientVerifier, PinnedServerVerifier};

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("TLS configuration error: {0}")]
    Tls(#[from] rustls::Error),
    #[error("QUIC endpoint construction failed: {0}")]
    Endpoint(#[from] std::io::Error),
    #[error("QUIC connection failed: {0}")]
    Connect(#[from] quinn::ConnectError),
    #[error("QUIC connection lost: {0}")]
    Connection(#[from] quinn::ConnectionError),
}

/// Both endpoints use this transport config: a bounded idle timeout so a
/// handshake that's going to fail (wrong pinned fingerprint, unreachable
/// peer) fails within a few seconds instead of hanging on QUIC's
/// otherwise-generous defaults.
fn bounded_transport_config() -> Arc<quinn::TransportConfig> {
    let mut config = quinn::TransportConfig::default();
    config.max_idle_timeout(Some(
        std::time::Duration::from_secs(5)
            .try_into()
            .expect("5s fits in a VarInt"),
    ));
    config.keep_alive_interval(Some(std::time::Duration::from_secs(2)));
    Arc::new(config)
}

/// Build a QUIC server endpoint bound to `bind_addr`, presenting `cert`
/// and requiring the connecting peer to present a certificate matching
/// `expected_peer_fingerprint`.
///
/// One endpoint is reused for every paired peer this device accepts
/// connections from — the fingerprint check happens per-connection
/// inside quinn's TLS handshake, not by running a separate endpoint per
/// peer.
pub fn build_server_endpoint(
    bind_addr: SocketAddr,
    cert: &DeviceCertificate,
    expected_peer_fingerprint: &str,
) -> Result<quinn::Endpoint, TransportError> {
    let client_verifier = PinnedClientVerifier::new(expected_peer_fingerprint.to_string());

    let mut tls_config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(vec![cert.rustls_certificate()], cert.rustls_private_key())?;
    tls_config.alpn_protocols = vec![b"cosync".to_vec()];

    let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(tls_config));
    server_config.transport_config(bounded_transport_config());
    let endpoint = quinn::Endpoint::server(server_config, bind_addr)?;
    Ok(endpoint)
}

/// Build a QUIC client endpoint that will only accept a server whose
/// certificate matches `expected_peer_fingerprint`. `cert` is this
/// device's own certificate, presented for the server's mutual-TLS check.
pub fn build_client_endpoint(
    bind_addr: SocketAddr,
    cert: &DeviceCertificate,
    expected_peer_fingerprint: &str,
) -> Result<quinn::Endpoint, TransportError> {
    let server_verifier = PinnedServerVerifier::new(expected_peer_fingerprint.to_string());

    let mut tls_config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(server_verifier)
        .with_client_auth_cert(vec![cert.rustls_certificate()], cert.rustls_private_key())?;
    tls_config.alpn_protocols = vec![b"cosync".to_vec()];

    let mut client_config = quinn::ClientConfig::new(Arc::new(tls_config));
    client_config.transport_config(bounded_transport_config());
    let mut endpoint = quinn::Endpoint::client(bind_addr)?;
    endpoint.set_default_client_config(client_config);
    Ok(endpoint)
}

/// A live, mutually-authenticated connection to one specific paired
/// device. This is deliberately thin at Milestone 2 — `send`/`on_receive`
/// of real `Envelope`s is Milestone 5's job; this just proves the tunnel
/// itself works.
pub struct Session {
    pub connection: quinn::Connection,
}

impl Session {
    /// Client side: dial a peer we already know the address and pinned
    /// fingerprint for (from a fresh QR scan, or from the `paired_devices`
    /// table on reconnect).
    pub async fn connect(
        endpoint: &quinn::Endpoint,
        addr: SocketAddr,
        server_name: &str,
    ) -> Result<Self, TransportError> {
        let connection = endpoint.connect(addr, server_name)?.await?;
        Ok(Self { connection })
    }

    /// Server side: accept the next incoming connection on this endpoint.
    /// The certificate check already happened during the handshake (via
    /// `PinnedClientVerifier`) by the time this returns `Ok`.
    pub async fn accept(endpoint: &quinn::Endpoint) -> Option<Result<Self, TransportError>> {
        let incoming = endpoint.accept().await?;
        Some(async move {
            let connection = incoming.await?;
            Ok(Self { connection })
        }.await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cert::DeviceCertificate;

    /// End-to-end proof that two independently-generated certificates,
    /// each pinning the other's fingerprint, can complete a real QUIC
    /// handshake over loopback — this is Milestone 2's actual acceptance
    /// criterion, minus the QR scan and mDNS discovery (which need two
    /// physical devices to mean anything; the cryptographic handshake
    /// this test exercises is identical either way).
    #[tokio::test]
    async fn two_pinned_devices_establish_a_mutually_authenticated_tunnel() {
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let server_cert = DeviceCertificate::generate().expect("server cert");
        let client_cert = DeviceCertificate::generate().expect("client cert");

        let server_endpoint = build_server_endpoint(
            "127.0.0.1:0".parse().unwrap(),
            &server_cert,
            &client_cert.fingerprint(), // server pins the client
        )
        .expect("build server endpoint");
        let server_addr = server_endpoint.local_addr().expect("server addr");

        let client_endpoint = build_client_endpoint(
            "127.0.0.1:0".parse().unwrap(),
            &client_cert,
            &server_cert.fingerprint(), // client pins the server
        )
        .expect("build client endpoint");

        let server_task = tokio::spawn(async move {
            Session::accept(&server_endpoint)
                .await
                .expect("connection attempted")
                .expect("handshake succeeds")
        });

        let client_session = Session::connect(&client_endpoint, server_addr, "cosync.local")
            .await
            .expect("client handshake succeeds");

        let server_session = server_task.await.expect("server task");

        assert_eq!(
            client_session.connection.remote_address().port(),
            server_addr.port()
        );
        assert!(server_session.connection.remote_address().port() > 0);
        })
        .await;
        outcome.expect("test must complete within 10s, not hang");
    }

    #[tokio::test]
    async fn handshake_fails_when_the_client_is_not_the_pinned_device() {
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let server_cert = DeviceCertificate::generate().expect("server cert");
        let client_cert = DeviceCertificate::generate().expect("client cert");
        let attacker_cert = DeviceCertificate::generate().expect("attacker cert");

        // Server expects `client_cert`'s fingerprint, but an attacker
        // holding a different (also perfectly valid) cert tries to
        // connect instead.
        let server_endpoint = build_server_endpoint(
            "127.0.0.1:0".parse().unwrap(),
            &server_cert,
            &client_cert.fingerprint(),
        )
        .expect("build server endpoint");
        let server_addr = server_endpoint.local_addr().expect("server addr");

        let attacker_endpoint = build_client_endpoint(
            "127.0.0.1:0".parse().unwrap(),
            &attacker_cert,
            &server_cert.fingerprint(), // attacker correctly pins the real server
        )
        .expect("build attacker endpoint");

        let server_task = tokio::spawn(async move { Session::accept(&server_endpoint).await });

        let client_result =
            Session::connect(&attacker_endpoint, server_addr, "cosync.local").await;

        // TLS 1.3's client-authentication flow means the *dialer's* side
        // of the handshake can resolve successfully before the server
        // has finished validating the client's certificate (the
        // server's rejection arrives slightly later, as the client
        // cert is the second flight). So `Session::connect` returning
        // `Ok` here does NOT by itself prove the attacker was accepted —
        // it has to be confirmed by actually trying to use the
        // connection and observing it get torn down.
        let attacker_was_rejected = match client_result {
            Err(_) => true, // rejected during the handshake itself
            Ok(session) => {
                // `open_bi()` merely allocates a local stream id — it
                // doesn't by itself prove the network round trip
                // succeeded. Waiting on `connection.closed()` is the
                // actual signal: it resolves once the peer tears the
                // connection down, which is what the server does the
                // moment it finishes rejecting an unpinned client cert.
                tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    session.connection.closed(),
                )
                .await
                .is_ok() // closed within the window -> rejected; stayed open -> real failure
            }
        };
        assert!(
            attacker_was_rejected,
            "attacker's connection must be rejected by the server's client-cert pin"
        );

        // The server side should also observe the handshake failing
        // (never completing to a usable Session), not silently succeeding.
        let server_outcome = server_task.await.expect("server task");
        if let Some(Ok(_)) = server_outcome {
            panic!("server must not accept a connection from an unpinned client certificate");
        }
        })
        .await;
        outcome.expect("test must complete within 10s, not hang");
    }
}
