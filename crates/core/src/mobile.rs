//! Android-facing connection object for Milestone 4A.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::cert::DeviceCertificate;
use crate::diagnostics::{recent_pairing_stages, record_pairing_stage};
use crate::discovery::Discovery;
use crate::framing::write_envelope;
use crate::paired_devices::{PairedDevice, PairedDeviceStore};
use crate::pairing::PairingPayload;
use crate::pairing_session::read_pairing_ack;
use crate::proto::{envelope::Payload, Envelope, PairingRequest};
use crate::reconnect_session::dial_reconnect;
use crate::transport::{build_client_endpoint, Session};

const RECONNECT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const DISCOVERY_WINDOW: Duration = Duration::from_secs(4);

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
        let reconnect_port = payload.session_port.unwrap_or(payload.port);
        let desktop = PairedDevice {
            device_id: payload.public_key_fingerprint.clone(),
            device_name: payload.device_name.clone(),
            cert_fingerprint: payload.public_key_fingerprint.clone(),
            last_known_ip: Some(payload.ip_hint.clone()),
            last_known_port: Some(reconnect_port),
        };

        let result = (|| {
            std::fs::create_dir_all(&self.data_dir).map_err(|error| error.to_string())?;
            record_pairing_stage(&self.data_dir, "certificate_loading");
            let cert = DeviceCertificate::load_or_create(&self.data_dir)
                .map_err(|error| error.to_string())?;
            let server_addr = format!("{}:{}", payload.ip_hint, payload.port)
                .parse::<SocketAddr>()
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

            self.open_store()?
                .upsert(&desktop)
                .map_err(|error| error.to_string())?;
            self.replace_connection(endpoint, session)?;
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

    /// Whether Android has a persisted desktop trust record. This decides
    /// whether app startup should resume the foreground connection service.
    pub fn has_paired_device(&self) -> bool {
        self.open_store()
            .and_then(|store| store.list_all().map_err(|error| error.to_string()))
            .map(|devices| !devices.is_empty())
            .unwrap_or(false)
    }

    /// Make one bounded reconnect attempt. Android schedules this method with
    /// exponential backoff and wakes it immediately from network callbacks;
    /// there is no idle status polling.
    pub fn reconnect(&self) -> String {
        if self.is_connected() {
            return "connected".to_string();
        }

        record_pairing_stage(&self.data_dir, "reconnect_requested");
        let devices = match self
            .open_store()
            .and_then(|store| store.list_all().map_err(|error| error.to_string()))
        {
            Ok(devices) if !devices.is_empty() => devices,
            Ok(_) => return "not paired".to_string(),
            Err(_) => {
                record_pairing_stage(&self.data_dir, "reconnect_store_failed");
                return "reconnect failed".to_string();
            }
        };

        // The saved address is the low-latency path for the common case.
        for device in &devices {
            if let (Some(ip), Some(port)) = (&device.last_known_ip, device.last_known_port) {
                if let Ok(ip) = ip.parse::<IpAddr>() {
                    if self
                        .connect_paired_device(device, SocketAddr::new(ip, port))
                        .is_ok()
                    {
                        record_pairing_stage(&self.data_dir, "reconnect_connected");
                        return "connected".to_string();
                    }
                }
            }
        }

        // If DHCP changed the desktop address, discover only already-trusted
        // device IDs on the LAN. Kotlin holds WifiManager.MulticastLock while
        // this bounded method runs.
        record_pairing_stage(&self.data_dir, "reconnect_discovery_started");
        if self.reconnect_via_discovery(&devices) {
            record_pairing_stage(&self.data_dir, "reconnect_connected");
            "connected".to_string()
        } else {
            record_pairing_stage(&self.data_dir, "reconnect_failed");
            "reconnect failed".to_string()
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
        let connection = self
            .session
            .lock()
            .ok()
            .and_then(|session| session.as_ref().map(|session| session.connection.clone()));
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

    fn open_store(&self) -> Result<PairedDeviceStore, String> {
        PairedDeviceStore::open(&self.data_dir.join("paired_devices.sqlite"))
            .map_err(|error| error.to_string())
    }

    fn replace_connection(
        &self,
        endpoint: quinn::Endpoint,
        session: Session,
    ) -> Result<(), String> {
        let mut session_slot = self.session.lock().map_err(|error| error.to_string())?;
        let mut endpoint_slot = self.endpoint.lock().map_err(|error| error.to_string())?;
        let replaced_session = session_slot.replace(session);
        let replaced_endpoint = endpoint_slot.replace(endpoint);
        drop(endpoint_slot);
        drop(session_slot);

        // A disconnect monitor owns a connection clone, so explicitly close
        // displaced handles instead of relying on Drop.
        if let Some(session) = replaced_session {
            session
                .connection
                .close(0u32.into(), b"superseded connection");
        }
        if let Some(endpoint) = replaced_endpoint {
            endpoint.close(0u32.into(), b"superseded endpoint");
        }
        Ok(())
    }

    fn connect_paired_device(
        &self,
        device: &PairedDevice,
        server_addr: SocketAddr,
    ) -> Result<(), String> {
        if !server_addr.is_ipv4() {
            return Err("IPv6 reconnect is not enabled yet".to_string());
        }
        let cert =
            DeviceCertificate::load_or_create(&self.data_dir).map_err(|error| error.to_string())?;
        let endpoint = build_client_endpoint(
            "0.0.0.0:0".parse().expect("valid mobile bind address"),
            &cert,
            &device.cert_fingerprint,
        )
        .map_err(|error| error.to_string())?;
        let session = self
            .runtime
            .block_on(dial_reconnect(
                &endpoint,
                server_addr,
                &cert.fingerprint(),
                RECONNECT_HANDSHAKE_TIMEOUT,
            ))
            .map_err(|error| error.to_string())?;

        let mut updated = device.clone();
        updated.last_known_ip = Some(server_addr.ip().to_string());
        updated.last_known_port = Some(server_addr.port());
        self.open_store()?
            .upsert(&updated)
            .map_err(|error| error.to_string())?;
        self.replace_connection(endpoint, session)?;
        Ok(())
    }

    fn reconnect_via_discovery(&self, devices: &[PairedDevice]) -> bool {
        let discovery = match Discovery::new() {
            Ok(discovery) => discovery,
            Err(_) => return false,
        };
        let receiver = match discovery.browse() {
            Ok(receiver) => receiver,
            Err(_) => {
                let _ = discovery.shutdown();
                return false;
            }
        };
        let deadline = Instant::now() + DISCOVERY_WINDOW;
        let mut connected = false;

        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            let peer = match receiver.recv_timeout(remaining) {
                Ok(peer) => peer,
                Err(_) => break,
            };
            let Some(device) = devices
                .iter()
                .find(|device| device.device_id == peer.device_id)
            else {
                continue;
            };

            for ip in peer.addresses.into_iter().filter(IpAddr::is_ipv4) {
                if self
                    .connect_paired_device(device, SocketAddr::new(ip, peer.port))
                    .is_ok()
                {
                    connected = true;
                    break;
                }
            }
            if connected {
                break;
            }
        }

        let _ = discovery.shutdown();
        connected
    }
}
