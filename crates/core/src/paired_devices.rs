//! Paired-device persistence.
//!
//! Once a QR scan completes a pairing handshake, the peer's identity is
//! recorded here so future runs can reconnect without scanning again.
//! This is the only piece of Milestone 2 that touches disk state beyond
//! the device's own cert/key (see `cert.rs`).

use std::path::Path;

use rusqlite::Connection;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PairedDevice {
    pub device_id: String,
    pub device_name: String,
    pub cert_fingerprint: String,
    pub last_known_ip: Option<String>,
    pub last_known_port: Option<u16>,
}

pub struct PairedDeviceStore {
    conn: Connection,
}

impl PairedDeviceStore {
    pub fn open(db_path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(db_path)?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    /// In-memory store — used by tests, and useful for anything that
    /// wants the same API without touching disk.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    fn init_schema(conn: &Connection) -> Result<(), StoreError> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS paired_devices (
                device_id         TEXT PRIMARY KEY,
                device_name       TEXT NOT NULL,
                cert_fingerprint  TEXT NOT NULL,
                last_known_ip     TEXT,
                last_known_port   INTEGER,
                paired_at         INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            )",
            [],
        )?;
        Ok(())
    }

    /// Record a newly-paired device, or update an existing one (e.g. its
    /// IP changed since the last connection — the fingerprint is what
    /// actually identifies the device, not the address).
    pub fn upsert(&self, device: &PairedDevice) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO paired_devices (device_id, device_name, cert_fingerprint, last_known_ip, last_known_port)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(device_id) DO UPDATE SET
                device_name = excluded.device_name,
                cert_fingerprint = excluded.cert_fingerprint,
                last_known_ip = excluded.last_known_ip,
                last_known_port = excluded.last_known_port",
            rusqlite::params![
                device.device_id,
                device.device_name,
                device.cert_fingerprint,
                device.last_known_ip,
                device.last_known_port,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, device_id: &str) -> Result<Option<PairedDevice>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT device_id, device_name, cert_fingerprint, last_known_ip, last_known_port
             FROM paired_devices WHERE device_id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![device_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(PairedDevice {
                device_id: row.get(0)?,
                device_name: row.get(1)?,
                cert_fingerprint: row.get(2)?,
                last_known_ip: row.get(3)?,
                last_known_port: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_all(&self) -> Result<Vec<PairedDevice>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT device_id, device_name, cert_fingerprint, last_known_ip, last_known_port
             FROM paired_devices ORDER BY paired_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(PairedDevice {
                device_id: row.get(0)?,
                device_name: row.get(1)?,
                cert_fingerprint: row.get(2)?,
                last_known_ip: row.get(3)?,
                last_known_port: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    pub fn remove(&self, device_id: &str) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM paired_devices WHERE device_id = ?1", rusqlite::params![device_id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(device_id: &str) -> PairedDevice {
        PairedDevice {
            device_id: device_id.to_string(),
            device_name: "Sani's Desktop".to_string(),
            cert_fingerprint: "ab".repeat(32),
            last_known_ip: Some("192.168.1.42".to_string()),
            last_known_port: Some(53317),
        }
    }

    #[test]
    fn upsert_then_get_round_trips() {
        let store = PairedDeviceStore::open_in_memory().expect("open");
        let device = sample("device-1");
        store.upsert(&device).expect("upsert");

        let loaded = store.get("device-1").expect("get").expect("present");
        assert_eq!(loaded, device);
    }

    #[test]
    fn get_returns_none_for_an_unknown_device() {
        let store = PairedDeviceStore::open_in_memory().expect("open");
        assert_eq!(store.get("nonexistent").expect("get"), None);
    }

    #[test]
    fn upsert_updates_an_existing_device_rather_than_duplicating_it() {
        let store = PairedDeviceStore::open_in_memory().expect("open");
        let mut device = sample("device-1");
        store.upsert(&device).expect("first upsert");

        device.last_known_ip = Some("10.0.0.5".to_string());
        store.upsert(&device).expect("second upsert");

        assert_eq!(store.list_all().expect("list").len(), 1);
        let loaded = store.get("device-1").expect("get").expect("present");
        assert_eq!(loaded.last_known_ip, Some("10.0.0.5".to_string()));
    }

    #[test]
    fn list_all_returns_every_paired_device() {
        let store = PairedDeviceStore::open_in_memory().expect("open");
        store.upsert(&sample("device-1")).expect("upsert 1");
        store.upsert(&sample("device-2")).expect("upsert 2");

        let all = store.list_all().expect("list");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn remove_deletes_a_paired_device() {
        let store = PairedDeviceStore::open_in_memory().expect("open");
        store.upsert(&sample("device-1")).expect("upsert");
        store.remove("device-1").expect("remove");
        assert_eq!(store.get("device-1").expect("get"), None);
    }

    #[test]
    fn persists_across_a_simulated_restart_via_a_real_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("paired_devices.sqlite");

        {
            let store = PairedDeviceStore::open(&db_path).expect("open");
            store.upsert(&sample("device-1")).expect("upsert");
        } // store (and its connection) dropped here — simulates app restart

        let reopened = PairedDeviceStore::open(&db_path).expect("reopen");
        assert!(reopened.get("device-1").expect("get").is_some());
    }
}
