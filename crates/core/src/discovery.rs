//! mDNS discovery.
//!
//! Devices advertise themselves as `_cosync._udp.local.` on the LAN and
//! listen for the same service type to find peers, with no central
//! server involved. `mdns-sd` runs its own background thread and hands
//! back a plain `Receiver`, so this module doesn't need `tokio` at all —
//! only [`crate::transport`] (the actual QUIC connection) does.
//!
//! **Android note for Milestone 4:** before starting a `Discovery`
//! listener on Android, the caller must hold a `WifiManager.MulticastLock`
//! for the listener's lifetime. Android silently drops multicast/mDNS
//! packets without it. This crate can't take that lock itself (it's an
//! Android API, not something reachable from pure Rust) — it's the
//! mobile bridge's responsibility to acquire it before calling
//! `Discovery::start` and release it after `stop`.

use std::net::IpAddr;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use thiserror::Error;

pub const SERVICE_TYPE: &str = "_cosync._udp.local.";

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("mDNS daemon error: {0}")]
    Mdns(#[from] mdns_sd::Error),
}

/// One discovered peer on the LAN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPeer {
    pub device_id: String,
    pub device_name: String,
    pub addresses: Vec<IpAddr>,
    pub port: u16,
}

/// A running discovery session: advertising this device and/or listening
/// for others, depending on which methods are called.
pub struct Discovery {
    daemon: ServiceDaemon,
    instance_name: String,
}

impl Discovery {
    pub fn new() -> Result<Self, DiscoveryError> {
        Ok(Self {
            daemon: ServiceDaemon::new()?,
            instance_name: String::new(),
        })
    }

    /// Advertise this device on the LAN so other Cosync devices can find
    /// it. `device_id` becomes part of the mDNS TXT record so a listener
    /// scoped to a specific known device (reconnection, Milestone 2 step
    /// 7) can filter for it without connecting to every peer it sees.
    pub fn advertise(
        &mut self,
        device_id: &str,
        device_name: &str,
        port: u16,
    ) -> Result<(), DiscoveryError> {
        let hostname = format!("{device_id}.local.");
        self.instance_name = device_id.to_string();

        let properties = [("device_id", device_id), ("device_name", device_name)];

        let service = ServiceInfo::new(
            SERVICE_TYPE,
            &self.instance_name,
            &hostname,
            "", // let mdns-sd fill in this host's local IPs automatically
            port,
            &properties[..],
        )?
        .enable_addr_auto();

        self.daemon.register(service)?;
        Ok(())
    }

    pub fn stop_advertising(&self) -> Result<(), DiscoveryError> {
        if !self.instance_name.is_empty() {
            self.daemon
                .unregister(&format!("{}.{}", self.instance_name, SERVICE_TYPE))?;
        }
        Ok(())
    }

    /// Start listening for other Cosync devices. Returns a channel of
    /// `DiscoveredPeer`s as they're found — callers (Tauri commands,
    /// UniFFI callbacks) adapt this into whatever event stream their
    /// platform expects.
    pub fn browse(&self) -> Result<std::sync::mpsc::Receiver<DiscoveredPeer>, DiscoveryError> {
        let receiver = self.daemon.browse(SERVICE_TYPE)?;
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            while let Ok(event) = receiver.recv() {
                if let ServiceEvent::ServiceResolved(info) = event {
                    let device_id = info
                        .get_property_val_str("device_id")
                        .unwrap_or_default()
                        .to_string();
                    let device_name = info
                        .get_property_val_str("device_name")
                        .unwrap_or_default()
                        .to_string();

                    let peer = DiscoveredPeer {
                        device_id,
                        device_name,
                        addresses: info.get_addresses().iter().copied().collect(),
                        port: info.get_port(),
                    };

                    if tx.send(peer).is_err() {
                        break; // receiver dropped, stop forwarding
                    }
                }
            }
        });

        Ok(rx)
    }

    pub fn shutdown(self) -> Result<(), DiscoveryError> {
        self.daemon.shutdown()?;
        Ok(())
    }
}

/// Reconnection backoff schedule (Milestone 2, step 7): how long to wait
/// before the next discovery attempt for a known-but-currently-unreachable
/// paired device. Exponential with a cap, so a device that's been off the
/// network for hours doesn't get hammered every second forever.
pub fn reconnect_backoff(attempt: u32) -> std::time::Duration {
    const BASE_MS: u64 = 500;
    const MAX_MS: u64 = 60_000;
    let backoff_ms = BASE_MS.saturating_mul(1u64 << attempt.min(10));
    std::time::Duration::from_millis(backoff_ms.min(MAX_MS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_backoff_grows_and_caps() {
        let first = reconnect_backoff(0);
        let second = reconnect_backoff(1);
        let third = reconnect_backoff(2);
        assert!(second > first);
        assert!(third > second);

        let far_future = reconnect_backoff(100);
        assert_eq!(far_future, std::time::Duration::from_millis(60_000));
    }

    #[test]
    fn discovery_daemon_starts_and_shuts_down_cleanly() {
        // Doesn't assert actual peer discovery — this sandbox's container
        // networking may not route real multicast — but proves the
        // daemon itself starts, can be told to advertise, and shuts down
        // without hanging or panicking.
        let mut discovery = Discovery::new().expect("daemon starts");
        discovery
            .advertise("test-device-id", "Test Device", 53317)
            .expect("advertise succeeds");
        discovery.stop_advertising().expect("stop advertising");
        discovery.shutdown().expect("clean shutdown");
    }
}
