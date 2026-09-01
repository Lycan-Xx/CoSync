//! Cosync desktop hub (Milestone 3).
//!
//! Wires `cosync-core` into a Tauri app: pairing QR generation, a
//! background listener that accepts incoming pairing connections, the
//! paired-device list, and a system tray reflecting connection state.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use cosync_core::{
    accept_pairing_incoming, accept_reconnect_incoming, build_pairing_server_endpoint,
    build_trusted_server_endpoint, default_app_data_dir, DeviceCertificate, DeviceIdentity,
    Discovery, PairedDevice, PairedDeviceStore, PairingPayload, Session, TrustedClientFingerprints,
};
use rand::RngCore;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use tokio::sync::{watch, Mutex as AsyncMutex, Semaphore};

struct AppState {
    identity: DeviceIdentity,
    cert: DeviceCertificate,
    store: AsyncMutex<PairedDeviceStore>,
    pairing_addr: SocketAddr,
    session_addr: SocketAddr,
    current_pairing_token: watch::Sender<String>,
    trusted_clients: TrustedClientFingerprints,
    connection_status: AsyncMutex<HashMap<String, bool>>,
    sessions: AsyncMutex<HashMap<String, Session>>,
    pairing_commit: AsyncMutex<()>,
    _discovery: Option<Discovery>,
}

/// Stable UDP port used by the pairing listener and advertised in the QR.
/// A fixed port lets local firewalls permit only Cosync pairing traffic
/// instead of requiring a broad inbound UDP exception for an ephemeral port.
const PAIRING_PORT: u16 = 48_215;
const SESSION_PORT: u16 = 48_216;
const PAIRING_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const RECONNECT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_CONCURRENT_PAIRING_ATTEMPTS: usize = 8;

fn random_token() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn desktop_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "THIS PC".to_string())
}

#[tauri::command]
fn get_hostname() -> String {
    desktop_name()
}

/// Best-effort LAN IPv4 address to embed in the pairing QR as an `ip_hint`.
///
/// Prefer the address selected by the operating system's default route.
/// Enumerating adapters alone is not reliable on Windows: virtual adapters
/// and link-local addresses can appear before the active Wi-Fi interface,
/// yielding a QR address an Android device cannot route to.
fn local_ip_hint() -> String {
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        // UDP connect only asks the OS to select a route; it sends no packet.
        if socket.connect("1.1.1.1:443").is_ok() {
            if let Ok(std::net::SocketAddr::V4(addr)) = socket.local_addr() {
                let ip = *addr.ip();
                if !ip.is_unspecified() && !ip.is_loopback() && !ip.is_link_local() {
                    return ip.to_string();
                }
            }
        }
    }

    if_addrs::get_if_addrs()
        .ok()
        .and_then(|addrs| {
            addrs.into_iter().find(|iface| match iface.addr.ip() {
                std::net::IpAddr::V4(ip) => {
                    !iface.is_loopback() && !ip.is_unspecified() && !ip.is_link_local()
                }
                std::net::IpAddr::V6(_) => false,
            })
        })
        .map(|iface| iface.addr.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

#[tauri::command]
async fn get_pairing_qr(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    let commit = state.pairing_commit.lock().await;
    let token = random_token();
    state.current_pairing_token.send_replace(token.clone());
    drop(commit);

    let payload = PairingPayload {
        device_name: desktop_name(),
        public_key_fingerprint: state.cert.fingerprint(),
        ip_hint: local_ip_hint(),
        session_port: Some(state.session_addr.port()),
        port: state.pairing_addr.port(),
        pairing_token: token,
    };

    payload.to_json().map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_paired_devices(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<PairedDeviceWithStatus>, String> {
    let devices = state
        .store
        .lock()
        .await
        .list_all()
        .map_err(|e| e.to_string())?;
    let status = state.connection_status.lock().await;

    Ok(devices
        .into_iter()
        .map(|device| {
            let connected = status.get(&device.device_id).copied().unwrap_or(false);
            PairedDeviceWithStatus { device, connected }
        })
        .collect())
}

#[tauri::command]
async fn get_connection_status(
    state: tauri::State<'_, Arc<AppState>>,
    device_id: String,
) -> Result<bool, String> {
    Ok(state
        .connection_status
        .lock()
        .await
        .get(&device_id)
        .copied()
        .unwrap_or(false))
}

#[derive(serde::Serialize)]
struct PairedDeviceWithStatus {
    #[serde(flatten)]
    device: PairedDevice,
    connected: bool,
}

/// Runs for the lifetime of the app: repeatedly accepts pairing attempts
/// on the fixed pairing port. Each accepted device is persisted and
/// broadcast to the frontend via a `paired-device-connected` event. A
/// failed/expired attempt (wrong token, nobody connects within the
/// window) just loops back around rather than tearing anything down.
async fn run_pairing_listener(app: tauri::AppHandle, state: Arc<AppState>) {
    let endpoint = match build_pairing_server_endpoint(state.pairing_addr, &state.cert) {
        Ok(endpoint) => endpoint,
        Err(err) => {
            eprintln!("cosync: pairing listener failed to start: {err}");
            return;
        }
    };
    let pairing_slots = Arc::new(Semaphore::new(MAX_CONCURRENT_PAIRING_ATTEMPTS));

    while let Some(incoming) = endpoint.accept().await {
        let permit = match pairing_slots.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                // Bound memory and handshake work under a burst or hostile
                // LAN peer. Dropping the incoming connection rejects it.
                drop(incoming);
                continue;
            }
        };
        let mut token_updates = state.current_pairing_token.subscribe();
        let expected_token = token_updates.borrow_and_update().clone();
        let attempt_app = app.clone();
        let attempt_state = state.clone();

        tauri::async_runtime::spawn(async move {
            let _permit = permit;
            let result = {
                let pairing_attempt =
                    accept_pairing_incoming(incoming, &expected_token, PAIRING_REQUEST_TIMEOUT);
                tokio::pin!(pairing_attempt);
                tokio::select! {
                    result = &mut pairing_attempt => Some(result),
                    // Invalidate every in-flight attempt for an older QR token.
                    _ = token_updates.changed() => None,
                }
            };

            match result {
                Some(Ok(pending)) => {
                    complete_pairing(attempt_app, attempt_state, pending, expected_token).await;
                }
                Some(Err(err)) => {
                    eprintln!("cosync: pairing attempt did not complete: {err}");
                }
                None => {}
            }
        });
    }
}

/// Accept steady-state sessions on a separate endpoint whose TLS verifier
/// only permits certificates already present in the paired-device store.
async fn run_session_listener(app: tauri::AppHandle, state: Arc<AppState>) {
    let endpoint = match build_trusted_server_endpoint(
        state.session_addr,
        &state.cert,
        state.trusted_clients.clone(),
    ) {
        Ok(endpoint) => endpoint,
        Err(err) => {
            eprintln!("cosync: trusted session listener failed to start: {err}");
            return;
        }
    };
    let session_slots = Arc::new(Semaphore::new(MAX_CONCURRENT_PAIRING_ATTEMPTS));

    while let Some(incoming) = endpoint.accept().await {
        let permit = match session_slots.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                drop(incoming);
                continue;
            }
        };
        let session_app = app.clone();
        let session_state = state.clone();
        tauri::async_runtime::spawn(async move {
            let _permit = permit;
            match accept_reconnect_incoming(incoming, RECONNECT_REQUEST_TIMEOUT).await {
                Ok((device_id, session)) => {
                    let device = session_state.store.lock().await.get(&device_id);
                    match device {
                        Ok(Some(device)) => {
                            register_session(session_app, session_state, device, session).await;
                        }
                        Ok(None) => {
                            session
                                .connection
                                .close(0u32.into(), b"device is no longer paired");
                        }
                        Err(err) => {
                            eprintln!("cosync: failed to load reconnecting device: {err}");
                            session
                                .connection
                                .close(0u32.into(), b"paired-device store failed");
                        }
                    }
                }
                Err(err) => {
                    eprintln!("cosync: trusted reconnect did not complete: {err}");
                }
            }
        });
    }
}

async fn complete_pairing(
    app: tauri::AppHandle,
    state: Arc<AppState>,
    pending: cosync_core::PendingPairing,
    expected_token: String,
) {
    // Only one validated attempt may commit a given one-time token. This
    // closes the race where two concurrent peers validate just before the
    // first successful attempt rotates the token.
    let commit = state.pairing_commit.lock().await;
    if state.current_pairing_token.borrow().as_str() != expected_token.as_str() {
        drop(commit);
        let _ = pending.reject("pairing code is no longer valid").await;
        return;
    }

    let device = pending.device().clone();
    if let Err(err) = state.store.lock().await.upsert(&device) {
        eprintln!("cosync: failed to persist paired device: {err}");
        drop(commit);
        let _ = pending
            .reject("desktop could not persist the paired device")
            .await;
        return;
    }
    state
        .trusted_clients
        .insert(device.cert_fingerprint.clone());

    let (device, session) = match pending.acknowledge().await {
        Ok(accepted) => accepted,
        Err(err) => {
            eprintln!("cosync: failed to acknowledge paired device: {err}");
            return;
        }
    };
    state.current_pairing_token.send_replace(random_token());
    drop(commit);

    register_session(app, state, device, session).await;
}

async fn register_session(
    app: tauri::AppHandle,
    state: Arc<AppState>,
    device: PairedDevice,
    session: Session,
) {
    let device_id = device.device_id.clone();
    let connection = session.connection.clone();
    let connection_id = connection.stable_id();

    if let Some(replaced) = state
        .sessions
        .lock()
        .await
        .insert(device_id.clone(), session)
    {
        replaced
            .connection
            .close(0u32.into(), b"superseded trusted session");
    }
    state
        .connection_status
        .lock()
        .await
        .insert(device_id.clone(), true);

    let _ = app.emit("paired-device-connected", &device);
    update_tray_tooltip(&app, &state).await;

    let monitor_app = app.clone();
    let monitor_state = state.clone();
    tauri::async_runtime::spawn(async move {
        connection.closed().await;
        let removed_current_session = {
            let mut sessions = monitor_state.sessions.lock().await;
            let is_current = sessions
                .get(&device_id)
                .map(|session| session.connection.stable_id() == connection_id)
                .unwrap_or(false);
            if is_current {
                sessions.remove(&device_id);
            }
            is_current
        };

        if removed_current_session {
            monitor_state
                .connection_status
                .lock()
                .await
                .insert(device_id.clone(), false);
            let _ = monitor_app.emit("paired-device-disconnected", &device_id);
            update_tray_tooltip(&monitor_app, &monitor_state).await;
        }
    });
}

async fn update_tray_tooltip(app: &tauri::AppHandle, state: &Arc<AppState>) {
    let connected_count = state
        .connection_status
        .lock()
        .await
        .values()
        .filter(|&&connected| connected)
        .count();

    let tooltip = if connected_count == 0 {
        "Cosync — no devices connected".to_string()
    } else {
        format!("Cosync — {connected_count} device(s) connected")
    };

    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let data_dir = default_app_data_dir().expect("resolve app data dir");
            std::fs::create_dir_all(&data_dir).expect("create app data dir");

            let identity = DeviceIdentity::load_or_create(&data_dir).expect("load/create identity");
            let cert = DeviceCertificate::load_or_create(&data_dir).expect("load/create cert");
            let store = PairedDeviceStore::open(&data_dir.join("paired_devices.sqlite"))
                .expect("open paired-device store");
            let trusted_clients = TrustedClientFingerprints::new(
                store
                    .list_all()
                    .expect("load paired-device trust set")
                    .into_iter()
                    .map(|device| device.cert_fingerprint),
            );

            // Reserve both fixed ports before QR generation. Pairing accepts
            // a new certificate plus one-time token; steady-state sessions use
            // a separate TLS listener restricted to persisted fingerprints.
            let pairing_probe = std::net::UdpSocket::bind(("0.0.0.0", PAIRING_PORT))
                .expect("bind fixed pairing port");
            let pairing_addr = pairing_probe.local_addr().expect("pairing addr");
            let session_probe = std::net::UdpSocket::bind(("0.0.0.0", SESSION_PORT))
                .expect("bind fixed trusted-session port");
            let session_addr = session_probe.local_addr().expect("session addr");
            drop(pairing_probe);
            drop(session_probe);
            let (current_pairing_token, _) = watch::channel(random_token());
            let discovery = match Discovery::new() {
                Ok(mut discovery) => match discovery.advertise(
                    &cert.fingerprint(),
                    &desktop_name(),
                    session_addr.port(),
                ) {
                    Ok(()) => Some(discovery),
                    Err(err) => {
                        eprintln!("cosync: failed to advertise trusted session: {err}");
                        let _ = discovery.shutdown();
                        None
                    }
                },
                Err(err) => {
                    eprintln!("cosync: failed to start LAN discovery: {err}");
                    None
                }
            };

            let state = Arc::new(AppState {
                identity,
                cert,
                store: AsyncMutex::new(store),
                pairing_addr,
                session_addr,
                current_pairing_token,
                trusted_clients,
                connection_status: AsyncMutex::new(HashMap::new()),
                sessions: AsyncMutex::new(HashMap::new()),
                pairing_commit: AsyncMutex::new(()),
                _discovery: discovery,
            });
            app.manage(state.clone());

            // System tray.
            use tauri::menu::{MenuBuilder, MenuItemBuilder};
            use tauri::tray::TrayIconBuilder;

            let show_qr = MenuItemBuilder::with_id("show_qr", "Show Pairing QR").build(app)?;
            let show_devices =
                MenuItemBuilder::with_id("show_devices", "Paired Devices").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show_qr, &show_devices, &quit])
                .build()?;

            TrayIconBuilder::with_id("main")
                .menu(&menu)
                .tooltip("Cosync — no devices connected")
                .icon(app.default_window_icon().unwrap().clone())
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show_qr" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                        let _ = app.emit("navigate", "pairing");
                    }
                    "show_devices" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                        let _ = app.emit("navigate", "devices");
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            let clipboard_shortcut =
                Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyV);
            app.global_shortcut().register(clipboard_shortcut)?;

            // Background pairing listener.
            let app_handle = app.handle().clone();
            let listener_state = state.clone();
            tauri::async_runtime::spawn(async move {
                run_pairing_listener(app_handle, listener_state).await;
            });
            let app_handle = app.handle().clone();
            let listener_state = state.clone();
            tauri::async_runtime::spawn(async move {
                run_session_listener(app_handle, listener_state).await;
            });

            let _ = identity_and_cert_are_ready(&state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_pairing_qr,
            list_paired_devices,
            get_connection_status,
            get_hostname
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Trivial sanity check kept as a named function (rather than an inline
/// `let _ =`) so it shows up clearly in a stack trace if identity/cert
/// generation ever silently produces something unusable.
fn identity_and_cert_are_ready(state: &AppState) -> bool {
    !state.identity.fingerprint().is_empty() && !state.cert.fingerprint().is_empty()
}
