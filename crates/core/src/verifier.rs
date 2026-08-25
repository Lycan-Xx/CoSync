//! Pinned-fingerprint certificate verification.
//!
//! Cosync devices don't validate certificates against a CA — there isn't
//! one. Instead, each `Session` is created already knowing the exact
//! fingerprint of the peer it's willing to talk to (learned from the
//! pairing QR code the first time, or from the `paired_devices` table on
//! every reconnect after). These verifiers implement that: "accept this
//! one specific certificate, reject everything else," rather than "accept
//! anything a CA vouches for."
//!
//! This intentionally uses rustls's `dangerous_configuration` escape
//! hatch. That name describes *disabling* validation, which is not what
//! this does — it substitutes a stricter, narrower check (one exact
//! certificate) for the broader one (any CA-issued certificate). See
//! ADR-003 for why a CA doesn't make sense in a two-device local mesh.

use crate::cert::fingerprint_of_der;

/// Verifies that the server's presented certificate matches one specific
/// expected fingerprint. Used on the client side (the device dialing a
/// connection) during pairing and reconnection.
pub struct PinnedServerVerifier {
    expected_fingerprint: String,
}

impl PinnedServerVerifier {
    pub fn new(expected_fingerprint: impl Into<String>) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            expected_fingerprint: expected_fingerprint.into(),
        })
    }
}

impl rustls::client::ServerCertVerifier for PinnedServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        let actual = fingerprint_of_der(&end_entity.0);
        if actual == self.expected_fingerprint {
            Ok(rustls::client::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(format!(
                "certificate fingerprint mismatch: expected {}, got {}",
                self.expected_fingerprint, actual
            )))
        }
    }
}

/// Verifies that the client's presented certificate matches one specific
/// expected fingerprint. Used on the server side (the device accepting an
/// incoming connection) — Cosync's QUIC endpoints require mutual TLS, so
/// both directions get pinned, not just the dialer's view of the callee.
pub struct PinnedClientVerifier {
    expected_fingerprint: String,
}

impl PinnedClientVerifier {
    pub fn new(expected_fingerprint: impl Into<String>) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            expected_fingerprint: expected_fingerprint.into(),
        })
    }
}

impl rustls::server::ClientCertVerifier for PinnedClientVerifier {
    fn client_auth_root_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _now: std::time::SystemTime,
    ) -> Result<rustls::server::ClientCertVerified, rustls::Error> {
        let actual = fingerprint_of_der(&end_entity.0);
        if actual == self.expected_fingerprint {
            Ok(rustls::server::ClientCertVerified::assertion())
        } else {
            Err(rustls::Error::General(format!(
                "client certificate fingerprint mismatch: expected {}, got {}",
                self.expected_fingerprint, actual
            )))
        }
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }
}

/// Accepts *any* client certificate, without checking its fingerprint
/// against anything. This exists for exactly one purpose: the server side
/// of a brand-new pairing connection, where the desktop hasn't learned
/// the phone's certificate fingerprint yet (the QR code only carries the
/// *desktop's* fingerprint to the phone — nothing flows back the other
/// way until the phone dials in).
///
/// This is **not** "no security." The TLS tunnel is still fully
/// encrypted; what's missing at this point is peer *authentication*, not
/// confidentiality. Authentication for the pairing exchange itself comes
/// from the one-time `pairing_token` carried inside the first
/// `PairingRequest` envelope sent over that encrypted tunnel (see
/// `pairing_session.rs`) — the same trust model as typing a PIN or
/// scanning a QR code to pair a Bluetooth device or use AirDrop. Once
/// that token is verified, the server captures the peer's *actual*
/// certificate fingerprint from the now-authenticated exchange and pins
/// it going forward via `PinnedClientVerifier` for every future
/// reconnection. This verifier is only ever used for the single,
/// short-lived pairing-mode listener — never for a steady-state session
/// endpoint.
pub struct AcceptAnyClientVerifier;

impl rustls::server::ClientCertVerifier for AcceptAnyClientVerifier {
    fn client_auth_root_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _now: std::time::SystemTime,
    ) -> Result<rustls::server::ClientCertVerified, rustls::Error> {
        Ok(rustls::server::ClientCertVerified::assertion())
    }

    fn client_auth_mandatory(&self) -> bool {
        // Still require *a* certificate be presented (so we have
        // something to fingerprint once the token checks out) — just
        // don't check it against a pin yet.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cert::DeviceCertificate;
    use rustls::client::ServerCertVerifier;
    use rustls::server::ClientCertVerifier;

    fn dummy_server_name() -> rustls::ServerName {
        rustls::ServerName::try_from("cosync.local").unwrap()
    }

    #[test]
    fn accepts_the_exact_pinned_certificate() {
        let cert = DeviceCertificate::generate().expect("generate");
        let verifier = PinnedServerVerifier::new(cert.fingerprint());

        let result = verifier.verify_server_cert(
            &cert.rustls_certificate(),
            &[],
            &dummy_server_name(),
            &mut std::iter::empty(),
            &[],
            std::time::SystemTime::now(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_a_different_certificate_even_though_its_otherwise_valid() {
        // This is the core security property: an attacker with a
        // perfectly legitimate, well-formed self-signed cert of their own
        // must still be rejected, because it's not the one the pairing
        // QR pinned.
        let legitimate_peer_cert = DeviceCertificate::generate().expect("generate legit");
        let attacker_cert = DeviceCertificate::generate().expect("generate attacker");

        let verifier = PinnedServerVerifier::new(legitimate_peer_cert.fingerprint());

        let result = verifier.verify_server_cert(
            &attacker_cert.rustls_certificate(), // wrong cert presented
            &[],
            &dummy_server_name(),
            &mut std::iter::empty(),
            &[],
            std::time::SystemTime::now(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn client_verifier_mirrors_the_same_accept_reject_behavior() {
        let cert = DeviceCertificate::generate().expect("generate");
        let other = DeviceCertificate::generate().expect("generate other");
        let verifier = PinnedClientVerifier::new(cert.fingerprint());

        assert!(verifier
            .verify_client_cert(&cert.rustls_certificate(), &[], std::time::SystemTime::now())
            .is_ok());
        assert!(verifier
            .verify_client_cert(&other.rustls_certificate(), &[], std::time::SystemTime::now())
            .is_err());
    }
}
