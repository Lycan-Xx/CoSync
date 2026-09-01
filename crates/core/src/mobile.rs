//! Android-facing connection object for Milestone 4A.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::cert::DeviceCertificate;
use crate::diagnostics::{recent_pairing_stages, record_pairing_stage};
use crate::framing::write_envelope;
use crate::pairing::PairingPayload;
use crate::pairing_session::read_pairing_ack;
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
            runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("create mobile Tokio runtime"),
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
            record_pairing_stage(&self.data_dir, "certificate_loading");
            let cert = DeviceCertificate::load_or_create(&self.data_dir)
                .map_err(|error| error.to_string())?;
            let server_addr = format!("{}:{}", payload.ip_hint, payload.port)
                .parse::<std::net::SocketAddr>()
                .map_err(|error| format!("invalid desktop address: {error}"))?;
            let device_id = cert.fingerprint();
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
                let session = Session::connect(&endpoint, server_addr, "cosync.local")
                    .await
                    .map_err(|error| {
                        record_pairing_stage(&self.data_dir, "quic_connect_failed");
                        format!("quic_connect: {error}")
                    })?;
                record_pairing_stage(&self.data_dir, "quic_connected");
                let (mut send, mut recv) = session.connection.open_bi().await.map_err(|error| {
                    record_pairing_stage(&self.data_dir, "pairing_stream_open_failed");
                    format!("pairing_stream_open: {error}")
                })?;
                let request = Envelope {
                    device_id,
                    logical_time: 0,
                    physical_time_ms: 0,
                    payload: Some(Payload::PairingRequest(PairingRequest {
                        device_name,
                        public_key_fingerprint: cert.fingerprint(),
                        pairing_token: payload.pairing_token,
                    })),
                };
                write_envelope(&mut send, &request).await.map_err(|error| {
                    record_pairing_stage(&self.data_dir, "pairing_request_write_failed");
                    format!("pairing_request_write: {error}")
                })?;
                send.finish().await.map_err(|error| {
                    record_pairing_stage(&self.data_dir, "pairing_request_finish_failed");
                    format!("pairing_request_finish: {error}")
                })?;
                record_pairing_stage(&self.data_dir, "pairing_request_sent");
                record_pairing_stage(&self.data_dir, "pairing_ack_waiting");
                read_pairing_ack(&mut recv).await.map_err(|error| {
                    record_pairing_stage(&self.data_dir, "pairing_ack_failed");
                    format!("pairing_ack: {error}")
                })?;
                record_pairing_stage(&self.data_dir, "pairing_ack_received");
                Ok::<(quinn::Endpoint, Session), String>((endpoint, session))
            })?;
            let mut session_slot = self.session.lock().map_err(|error| error.to_string())?;
            let mut endpoint_slot = self.endpoint.lock().map_err(|error| error.to_string())?;
            let replaced_session = session_slot.replace(session);
            let replaced_endpoint = endpoint_slot.replace(endpoint);
            drop(endpoint_slot);
            drop(session_slot);

            // Disconnect displaced handles explicitly. A disconnect monitor
            // intentionally owns a connection clone, so dropping only the
            // stored Session would otherwise keep the old QUIC tunnel alive.
            if let Some(session) = replaced_session {
                session
                    .connection
                    .close(0u32.into(), b"superseded pairing session");
            }
            if let Some(endpoint) = replaced_endpoint {
                endpoint.close(0u32.into(), b"superseded pairing endpoint");
            }
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
        self.session
            .lock()
            .ok()
            .and_then(|session| {
                session
                    .as_ref()
                    .map(|session| session.connection.close_reason().is_none())
            })
            .unwrap_or(false)
    }

    /// Block until the current QUIC connection closes. Android runs this on
    /// a dedicated monitor executor, giving it an event-driven disconnect
    /// signal without periodic JNI/UniFFI polling.
    pub fn wait_for_disconnect(&self) {
        let connection = self.session.lock().ok().and_then(|session| {
            session
                .as_ref()
                .map(|session| session.connection.clone())
        });
        if let Some(connection) = connection {
            self.runtime.block_on(async {
                connection.closed().await;
            });
        }
    }

    pub fn disconnect(&self) {
        if let Ok(mut session) = self.session.lock() {
            if let Some(session) = session.take() {
                session.connection.close(0u32.into(), b"user disconnect");
            }
        }
        if let Ok(mut endpoint) = self.endpoint.lock() {
            if let Some(endpoint) = endpoint.take() {
                endpoint.close(0u32.into(), b"user disconnect");
            }
        }
    }
}
