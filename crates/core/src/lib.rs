//! cosync-core: the shared Rust logic behind Cosync's desktop (Tauri) and
//! mobile (React Native, via UniFFI) apps.
//!
//! Milestone 1 scope: pure data types and logic only — the wire protocol
//! (`proto`), the Hybrid Logical Clock (`hlc`), device identity
//! (`identity`), and the pairing QR payload (`pairing`). No networking
//! yet; that starts in Milestone 2.

pub mod cert;
pub mod discovery;
pub mod diagnostics;
pub mod framing;
pub mod hlc;
pub mod identity;
pub mod paired_devices;
pub mod pairing;
pub mod pairing_session;
pub mod transport;
pub mod verifier;

#[cfg(feature = "mobile-bindings")]
pub mod mobile;

#[cfg(feature = "mobile-bindings")]
pub use mobile::ConnectionClient;

#[cfg(feature = "mobile-bindings")]
uniffi::include_scaffolding!("cosync_mobile");

/// Generated from `proto/cosync.proto` by `build.rs` (prost-build).
pub mod proto {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/cosync.rs"));
}

pub use cert::DeviceCertificate;
pub use discovery::{Discovery, DiscoveredPeer};
pub use hlc::{should_apply_update, HlcTimestamp, HybridLogicalClock};
pub use identity::{default_app_data_dir, DeviceIdentity, IdentityError};
pub use paired_devices::{PairedDevice, PairedDeviceStore};
pub use pairing::PairingPayload;
pub use pairing_session::{accept_pairing_connection, dial_and_send_pairing_request, PairingError};
pub use transport::Session;

#[cfg(test)]
mod envelope_tests {
    //! Round-trip tests for every `Envelope` payload variant. These exist
    //! to catch schema mistakes early — a field that doesn't round-trip
    //! cleanly here will silently corrupt data once real networking
    //! (Milestone 2+) is layered on top.

    use crate::proto::{
        envelope::Payload, Ack, ClipboardUpdate, Envelope, FileChunk, FileMeta, Heartbeat,
        PairingRequest,
    };
    use prost::Message;

    fn base_envelope(payload: Payload) -> Envelope {
        Envelope {
            device_id: "device-abc123".to_string(),
            logical_time: 7,
            physical_time_ms: 1_725_000_000_000,
            payload: Some(payload),
        }
    }

    fn round_trip(envelope: &Envelope) -> Envelope {
        let bytes = envelope.encode_to_vec();
        Envelope::decode(bytes.as_slice()).expect("decode must succeed for a value we just encoded")
    }

    #[test]
    fn pairing_request_round_trips() {
        let envelope = base_envelope(Payload::PairingRequest(PairingRequest {
            device_name: "Sani's Desktop".to_string(),
            public_key_fingerprint: "ab".repeat(32),
            pairing_token: "token-123".to_string(),
        }));
        assert_eq!(envelope, round_trip(&envelope));
    }

    #[test]
    fn clipboard_update_round_trips() {
        let envelope = base_envelope(Payload::ClipboardUpdate(ClipboardUpdate {
            source_device_id: "device-abc123".to_string(),
            content: b"copied text with \0 a null byte".to_vec(),
            content_type: "text/plain".to_string(),
        }));
        assert_eq!(envelope, round_trip(&envelope));
    }

    #[test]
    fn file_meta_round_trips() {
        let envelope = base_envelope(Payload::FileMeta(FileMeta {
            transfer_id: "transfer-1".to_string(),
            filename: "vacation.jpg".to_string(),
            size_bytes: 12_345_678,
            sha256: "0".repeat(64),
            chunk_count: 188,
        }));
        assert_eq!(envelope, round_trip(&envelope));
    }

    #[test]
    fn file_chunk_round_trips_including_binary_data() {
        let envelope = base_envelope(Payload::FileChunk(FileChunk {
            transfer_id: "transfer-1".to_string(),
            chunk_index: 42,
            data: (0u8..=255).collect(), // exercise every byte value, not just ASCII
        }));
        assert_eq!(envelope, round_trip(&envelope));
    }

    #[test]
    fn heartbeat_round_trips() {
        let envelope = base_envelope(Payload::Heartbeat(Heartbeat {}));
        assert_eq!(envelope, round_trip(&envelope));
    }

    #[test]
    fn ack_round_trips_success_and_failure() {
        let success = base_envelope(Payload::Ack(Ack {
            ref_id: "transfer-1".to_string(),
            success: true,
            error: String::new(),
        }));
        assert_eq!(success, round_trip(&success));

        let failure = base_envelope(Payload::Ack(Ack {
            ref_id: "transfer-1".to_string(),
            success: false,
            error: "checksum mismatch".to_string(),
        }));
        assert_eq!(failure, round_trip(&failure));
    }
}
