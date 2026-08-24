//! Device TLS certificate.
//!
//! Cosync devices never touch a Certificate Authority. Each device
//! generates a self-signed certificate once and persists it; the SHA-256
//! fingerprint of that certificate is what gets embedded in the pairing
//! QR code ([`crate::pairing::PairingPayload`]) and pinned by the peer
//! that scans it (see [`crate::verifier`]).
//!
//! This is a closed pairwise trust model, not a CA-validated one — see
//! ADR-003. It's deliberate, not a shortcut: there is no third party to
//! trust in a two-device local mesh, so a CA would just be overhead.

use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};
use thiserror::Error;

const CERT_FILE_NAME: &str = "device_cert.der";
const KEY_FILE_NAME: &str = "device_cert.key.der";

#[derive(Debug, Error)]
pub enum CertError {
    #[error("failed to read/write certificate files: {0}")]
    Io(#[from] std::io::Error),
    #[error("certificate generation failed: {0}")]
    Generation(#[from] rcgen::RcgenError),
}

/// A device's persistent self-signed TLS identity: the certificate quinn
/// presents during the QUIC handshake, plus its private key.
pub struct DeviceCertificate {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
}

impl DeviceCertificate {
    /// Generate a brand-new self-signed certificate. Valid for a long
    /// time (10 years) since there's no revocation mechanism in a
    /// pairwise-trust model anyway — rotation would mean re-pairing.
    pub fn generate() -> Result<Self, CertError> {
        let mut params = rcgen::CertificateParams::new(vec!["cosync.local".to_string()]);
        let mut not_after = time::OffsetDateTime::now_utc();
        not_after += time::Duration::days(3650);
        params.not_after = not_after;

        let cert = rcgen::Certificate::from_params(params)?;
        Ok(Self {
            cert_der: cert.serialize_der()?,
            key_der: cert.serialize_private_key_der(),
        })
    }

    /// Load the persisted certificate from `dir`, generating and saving a
    /// new one on first run. Mirrors `DeviceIdentity::load_or_create` —
    /// same "first run creates it, every run after reuses it" contract.
    pub fn load_or_create(dir: &Path) -> Result<Self, CertError> {
        let cert_path = dir.join(CERT_FILE_NAME);
        let key_path = dir.join(KEY_FILE_NAME);

        if cert_path.exists() && key_path.exists() {
            Ok(Self {
                cert_der: fs::read(&cert_path)?,
                key_der: fs::read(&key_path)?,
            })
        } else {
            let generated = Self::generate()?;
            generated.save(dir)?;
            Ok(generated)
        }
    }

    pub fn save(&self, dir: &Path) -> Result<(), CertError> {
        fs::create_dir_all(dir)?;
        fs::write(dir.join(CERT_FILE_NAME), &self.cert_der)?;
        let key_path = dir.join(KEY_FILE_NAME);
        fs::write(&key_path, &self.key_der)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(&key_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o600);
                let _ = fs::set_permissions(&key_path, perms);
            }
        }

        Ok(())
    }

    /// The value embedded in the pairing QR code and pinned by the peer:
    /// SHA-256 of the DER-encoded certificate, hex-encoded. This is the
    /// standard "TLS certificate fingerprint" concept — the same shape
    /// LocalSend's protocol uses (see ADR-003), independent of whatever
    /// key algorithm the certificate happens to use.
    pub fn fingerprint(&self) -> String {
        fingerprint_of_der(&self.cert_der)
    }

    pub fn rustls_certificate(&self) -> rustls::Certificate {
        rustls::Certificate(self.cert_der.clone())
    }

    pub fn rustls_private_key(&self) -> rustls::PrivateKey {
        rustls::PrivateKey(self.key_der.clone())
    }
}

pub fn fingerprint_of_der(cert_der: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(cert_der);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_a_valid_der_certificate() {
        let cert = DeviceCertificate::generate().expect("generate");
        assert!(!cert.cert_der.is_empty());
        assert!(!cert.key_der.is_empty());
    }

    #[test]
    fn fingerprint_is_deterministic_for_the_same_cert() {
        let cert = DeviceCertificate::generate().expect("generate");
        assert_eq!(cert.fingerprint(), cert.fingerprint());
    }

    #[test]
    fn two_generated_certificates_have_different_fingerprints() {
        let a = DeviceCertificate::generate().expect("generate a");
        let b = DeviceCertificate::generate().expect("generate b");
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn persists_and_reloads_the_same_certificate_across_a_simulated_restart() {
        let tmp = tempfile::tempdir().expect("tempdir");

        let first_run = DeviceCertificate::load_or_create(tmp.path()).expect("first run");
        let fingerprint = first_run.fingerprint();

        let second_run = DeviceCertificate::load_or_create(tmp.path()).expect("second run");
        assert_eq!(second_run.fingerprint(), fingerprint);
    }
}
