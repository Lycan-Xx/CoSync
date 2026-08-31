//! Android-facing connection object for Milestone 4A.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::cert::DeviceCertificate;
use crate::diagnostics::{recent_pairing_stages, record_pairing_stage};
use crate::framing::write_envelope;
use crate::identity::DeviceIdentity;
use crate::pairing::PairingPayload;
use crate::proto::{envelope::Payload, Envelope, PairingRequest};
use crate::transport::{build_client_endpoint, Session};

pub struct ConnectionClient {
    data_dir: PathBuf,
    runtime: tokio::runtime::Runtime,
    endpoint: Mutex<Option<quinn::Endpoint>>,
    session: Mutex<Option<Session>>,
}

impl ConnectionClient {
    pub fn new(data_dir: String) -> Self {
        Self {
            data_dir: PathBuf::from(data_dir),
            runtime: tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("create mobile Tokio runtime"),
            endpoint: Mutex::new(None),
            session: Mutex::new(None),
        }
    }

    pub fn pair(&self, payload_json: String, device_name: String) -> String {
        record_pairing_stage(&self.data_dir, "pair_requested");
        let payload = match PairingPayload::from_json(&payload_json) {
            Ok(payload) => payload,
            Err(error) => {
                record_pairing_stage(&self.data_dir, "payload_parse_failed");
                return format!("pairing failed at payload_parse: {error}");
            }
        };
        let result = (|| {
            std::fs::create_dir_all(&self.data_dir).map_err(|error| error.to_string())?;
            record_pairing_stage(&self.data_dir, "identity_loading");
            let identity = DeviceIdentity::load_or_create(&self.data_dir).map_err(|error| error.to_string())?;
            let cert = DeviceCertificate::load_or_create(&self.data_dir).map_err(|error| error.to_string())?;
            let server_addr = format!("{}:{}", payload.ip_hint, payload.port).parse::<std::net::SocketAddr>().map_err(|error| format!("invalid desktop address: {error}"))?;
            let device_id = identity.fingerprint();
            let (endpoint, session) = self.runtime.block_on(async {
                record_pairing_stage(&self.data_dir, "client_endpoint_building");
                let endpoint = build_client_endpoint(
                    "0.0.0.0:0".parse().expect("valid mobile bind address"),
                    &cert,
                    &payload.public_key_fingerprint,
                )
                .map_err(|error| {
                    record_pairing_stage(&self.data_dir, "client_endpoint_build_failed");
                    format!("client_endpoint: {error}")
                })?;
                record_pairing_stage(&self.data_dir, "quic_connect_started");
                let session = Session::connect(&endpoint, server_addr, "cosync.local").await.map_err(|error| {
                    record_pairing_stage(&self.data_dir, "quic_connect_failed");
                    format!("quic_connect: {error}")
                })?;
                record_pairing_stage(&self.data_dir, "quic_connected");
                let (mut send, _recv) = session.connection.open_bi().await.map_err(|error| {
                    record_pairing_stage(&self.data_dir, "pairing_stream_open_failed");
                    format!("pairing_stream_open: {error}")
                })?;
                let request = Envelope { device_id, logical_time: 0, physical_time_ms: 0, payload: Some(Payload::PairingRequest(PairingRequest { device_name, public_key_fingerprint: cert.fingerprint(), pairing_token: payload.pairing_token })) };
                write_envelope(&mut send, &request).await.map_err(|error| {
                    record_pairing_stage(&self.data_dir, "pairing_request_write_failed");
                    format!("pairing_request_write: {error}")
                })?;
                send.finish().await.map_err(|error| {
                    record_pairing_stage(&self.data_dir, "pairing_request_finish_failed");
                    format!("pairing_request_finish: {error}")
                })?;
                record_pairing_stage(&self.data_dir, "pairing_request_sent");
                Ok::<(quinn::Endpoint, Session), String>((endpoint, session))
            })?;
            *self.endpoint.lock().map_err(|error| error.to_string())? = Some(endpoint);
            *self.session.lock().map_err(|error| error.to_string())? = Some(session);
            Ok::<(), String>(())
        })();
        match result {
            Ok(()) => {
                record_pairing_stage(&self.data_dir, "pairing_connected");
                "connected".to_string()
            }
            Err(error) => {
                record_pairing_stage(&self.data_dir, "pairing_failed");
                format!("pairing failed: {error}")
            }
        }
    }

    pub fn recent_diagnostics(&self) -> String {
        recent_pairing_stages(&self.data_dir)
    }

    pub fn is_connected(&self) -> bool {
        self.session.lock().ok().and_then(|session| session.as_ref().map(|session| session.connection.close_reason().is_none())).unwrap_or(false)
    }

    pub fn disconnect(&self) {
        if let Ok(mut session) = self.session.lock() { if let Some(session) = session.take() { session.connection.close(0u32.into(), b"user disconnect"); } }
        if let Ok(mut endpoint) = self.endpoint.lock() { if let Some(endpoint) = endpoint.take() { endpoint.close(0u32.into(), b"user disconnect"); } }
    }
}
