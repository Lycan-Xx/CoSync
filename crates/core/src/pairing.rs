//! The pairing QR payload.
//!
//! This is deliberately *not* part of the protobuf wire protocol
//! (`proto::Envelope`). It's small, short-lived, and only ever exists
//! long enough to be rendered as a QR code and scanned once — plain JSON
//! is more useful here than protobuf, since it's human-readable if you
//! ever need to debug a pairing failure by eyeballing a decoded QR.
//!
//! See Milestone 2 for how this gets generated (desktop) and consumed
//! (mobile scan) to bootstrap the QUIC tunnel.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingPayload {
    pub device_name: String,
    /// SHA-256 hex fingerprint of the advertising device's TLS certificate —
    /// see `cert::DeviceCertificate::fingerprint()`. This is what the scanning
    /// device pins as the trust anchor for this peer (ADR-005).
    pub public_key_fingerprint: String,
    /// Best-effort IP hint so the scanning device can dial directly
    /// instead of waiting for mDNS discovery to find it independently.
    pub ip_hint: String,
    pub port: u16,
    /// One-time token proving "this QR scan happened," not a long-term
    /// secret — the fingerprint pinning is what actually secures the
    /// connection afterwards.
    pub pairing_token: String,
}

impl PairingPayload {
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PairingPayload {
        PairingPayload {
            device_name: "Sani's Desktop".to_string(),
            public_key_fingerprint: "deadbeef".repeat(8),
            ip_hint: "192.168.1.42".to_string(),
            port: 53317,
            pairing_token: "one-time-token-abc123".to_string(),
        }
    }

    #[test]
    fn round_trips_through_json() {
        let original = sample();
        let json = original.to_json().expect("serialize");
        let decoded = PairingPayload::from_json(&json).expect("deserialize");
        assert_eq!(original, decoded);
    }

    #[test]
    fn json_is_human_readable() {
        // Not a strict requirement of the type, but the whole reason this
        // is JSON and not protobuf — assert it actually looks like JSON,
        // not a base64 blob, so a future debugging session can eyeball it.
        let json = sample().to_json().expect("serialize");
        assert!(json.contains("device_name"));
        assert!(json.contains("Sani's Desktop"));
    }
}
