//! Device identity.
//!
//! Every device (phone or desktop) has exactly one long-lived Ed25519
//! keypair, generated once on first run and persisted locally. The
//! public key's fingerprint (a SHA-256 hash, hex-encoded) is what gets
//! embedded in the pairing QR code and pinned by the peer — see
//! Milestone 2. There is no CA involved; this is a closed pairwise trust
//! model (see ADR-003 for why).

use std::fs;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use thiserror::Error;

const KEY_FILE_NAME: &str = "identity.key";

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("failed to read/write identity key file: {0}")]
    Io(#[from] std::io::Error),
    #[error("stored identity key is corrupt (expected 32 bytes, got {0})")]
    CorruptKeyFile(usize),
    #[error("no app data directory available for this platform")]
    NoAppDataDir,
}

pub struct DeviceIdentity {
    signing_key: SigningKey,
}

impl DeviceIdentity {
    /// Generate a brand-new random identity. Does not touch disk — pair
    /// this with `save()` if it should persist.
    pub fn generate() -> Self {
        let mut secret_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut secret_bytes);
        Self {
            signing_key: SigningKey::from_bytes(&secret_bytes),
        }
    }

    /// Load the identity from `dir/identity.key`, generating and saving a
    /// new one if none exists yet. This is the function every real
    /// caller (desktop, mobile bridge) should use — "first run generates
    /// a key, every run after that reuses it" is the whole point.
    pub fn load_or_create(dir: &Path) -> Result<Self, IdentityError> {
        let key_path = dir.join(KEY_FILE_NAME);

        if key_path.exists() {
            let bytes = fs::read(&key_path)?;
            let bytes: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| IdentityError::CorruptKeyFile(bytes.len()))?;
            Ok(Self {
                signing_key: SigningKey::from_bytes(&bytes),
            })
        } else {
            let identity = Self::generate();
            identity.save(dir)?;
            Ok(identity)
        }
    }

    /// Persist this identity's secret key to `dir/identity.key`, creating
    /// `dir` if necessary.
    pub fn save(&self, dir: &Path) -> Result<(), IdentityError> {
        fs::create_dir_all(dir)?;
        let key_path = dir.join(KEY_FILE_NAME);
        fs::write(&key_path, self.signing_key.to_bytes())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Best-effort: this is a private key, keep it owner-read-only.
            if let Ok(metadata) = fs::metadata(&key_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o600);
                let _ = fs::set_permissions(&key_path, perms);
            }
        }

        Ok(())
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.signing_key.sign(message).to_bytes()
    }

    pub fn verify(public_key: &VerifyingKey, message: &[u8], signature_bytes: &[u8; 64]) -> bool {
        let signature = ed25519_dalek::Signature::from_bytes(signature_bytes);
        public_key.verify(message, &signature).is_ok()
    }

    /// The value that gets embedded in the pairing QR code and pinned by
    /// the peer as this device's trust anchor: SHA-256 of the raw public
    /// key, hex-encoded.
    pub fn fingerprint(&self) -> String {
        fingerprint_of(&self.verifying_key())
    }
}

pub fn fingerprint_of(public_key: &VerifyingKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(public_key.as_bytes());
    hex::encode(hasher.finalize())
}

/// The standard, cross-platform location for `identity.key` and any other
/// per-device state (paired-device SQLite DB, etc.). Uses the `directories`
/// crate so this resolves correctly on Windows/macOS/Linux/Android without
/// per-platform branching in every caller.
pub fn default_app_data_dir() -> Result<PathBuf, IdentityError> {
    directories::ProjectDirs::from("dev", "cosync", "cosync")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .ok_or(IdentityError::NoAppDataDir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_a_usable_keypair() {
        let identity = DeviceIdentity::generate();
        let message = b"hello cosync";
        let signature = identity.sign(message);
        assert!(DeviceIdentity::verify(
            &identity.verifying_key(),
            message,
            &signature
        ));
    }

    #[test]
    fn fingerprint_is_deterministic_for_the_same_key() {
        let identity = DeviceIdentity::generate();
        assert_eq!(identity.fingerprint(), identity.fingerprint());
    }

    #[test]
    fn two_generated_identities_have_different_fingerprints() {
        let a = DeviceIdentity::generate();
        let b = DeviceIdentity::generate();
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn persists_and_reloads_the_same_identity_across_a_simulated_restart() {
        let tmp = tempfile::tempdir().expect("tempdir");

        let first_run = DeviceIdentity::load_or_create(tmp.path()).expect("first run");
        let fingerprint_after_first_run = first_run.fingerprint();

        // Simulate the app restarting: load again from the same dir.
        let second_run = DeviceIdentity::load_or_create(tmp.path()).expect("second run");
        assert_eq!(second_run.fingerprint(), fingerprint_after_first_run);
    }

    #[test]
    fn save_creates_a_file_that_load_or_create_reuses() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let identity = DeviceIdentity::generate();
        identity.save(tmp.path()).expect("save");

        let key_path = tmp.path().join(KEY_FILE_NAME);
        assert!(key_path.exists());

        let reloaded = DeviceIdentity::load_or_create(tmp.path()).expect("reload");
        assert_eq!(reloaded.fingerprint(), identity.fingerprint());
    }
}
