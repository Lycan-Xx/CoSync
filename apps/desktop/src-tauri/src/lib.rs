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
    accept_pairing_connection, default_app_data_dir, DeviceCertificate, DeviceIdentity,
    PairedDevice, PairedDeviceStore, PairingPayload,
};
use rand::RngCore;
use tauri::{Emitter, Manager};
use tokio::sync::Mutex as AsyncMutex;

struct AppState {
    identity: DeviceIdentity,
    cert: DeviceCertificate,
    store: PairedDeviceStore,
    pairing_addr: SocketAddr,
    current_pairing_token: AsyncMutex<String>,
    connection_status: AsyncMutex<HashMap<String, bool>>,
}

fn random_token() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Best-effort local IPv4 address to embed in the pairing QR as an
/// `ip_hint`. Falls back to loopback if nothing better is found — better
/// to hand the scanning device *something* than fail QR generation
/// outright; mDNS discovery (once Milestone 4 exists) can still find the
/// real address independently.
fn local_ip_hint() -> String {
    if_addrs::get_if_addrs()
        .ok()
        .and_then(|addrs| {
            addrs
                .into_iter()
                .find(|iface| !iface.is_loopback() && iface.addr.ip().is_ipv4())
        })
        .map(|iface| iface.addr.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

#[tauri::command]
async fn get_pairing_qr(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    let token = random_token();
    *state.current_pairing_token.lock().await = token.clone();

    let payload = PairingPayload {
        device_name: "Sani's Desktop".to_string(),
        public_key_fingerprint: state.cert.fingerprint(),
        ip_hint: local_ip_hint(),
        port: state.pairing_addr.port(),
        pairing_token: token,
    };

    payload.to_json().map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_paired_devices(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<PairedDeviceWithStatus>, String> {
    let devices = state.store.list_all().map_err(|e| e.to_string())?;
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
    loop {
        let expected_token = state.current_pairing_token.lock().await.clone();
        match accept_pairing_connection(
            state.pairing_addr,
            &state.cert,
            &expected_token,
            Duration::from_secs(300),
        )
        .await
        {
            Ok((device, _session)) => {
                if let Err(err) = state.store.upsert(&device) {
                    eprintln!("cosync: failed to persist paired device: {err}");
                    continue;
                }
                state
                    .connection_status
                    .lock()
                    .await
                    .insert(device.device_id.clone(), true);

                let _ = app.emit("paired-device-connected", &device);
                update_tray_tooltip(&app, &state).await;
            }
            Err(err) => {
                // Timeout, token mismatch, or a dropped connection —
                // none of these should kill the listener. Just try again.
                eprintln!("cosync: pairing attempt did not complete: {err}");
            }
        }
    }
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
        .setup(|app| {
            let data_dir = default_app_data_dir().expect("resolve app data dir");
            std::fs::create_dir_all(&data_dir).expect("create app data dir");

            let identity = DeviceIdentity::load_or_create(&data_dir).expect("load/create identity");
            let cert = DeviceCertificate::load_or_create(&data_dir).expect("load/create cert");
            let store = PairedDeviceStore::open(&data_dir.join("paired_devices.sqlite"))
                .expect("open paired-device store");

            // Bind the pairing listener's UDP socket now (synchronously)
            // so we know the real port before anything (QR, mDNS, tray)
            // needs it — the actual QUIC endpoint is (re)built per pairing
            // attempt by `accept_pairing_connection`, but it needs to bind
            // the *same* address every time, so we resolve that address
            // once up front.
            let probe = std::net::UdpSocket::bind("0.0.0.0:0").expect("bind pairing port probe");
            let pairing_addr = probe.local_addr().expect("pairing addr");
            drop(probe);

            let state = Arc::new(AppState {
                identity,
                cert,
                store,
                pairing_addr,
                current_pairing_token: AsyncMutex::new(random_token()),
                connection_status: AsyncMutex::new(HashMap::new()),
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

            // Background pairing listener.
            let app_handle = app.handle().clone();
            let listener_state = state.clone();
            tauri::async_runtime::spawn(async move {
                run_pairing_listener(app_handle, listener_state).await;
            });

            let _ = identity_and_cert_are_ready(&state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_pairing_qr,
            list_paired_devices,
            get_connection_status
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
