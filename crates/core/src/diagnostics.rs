//! Local, privacy-safe diagnostics for connection setup.
//!
//! Pairing failures need enough context to distinguish a bad payload, an
//! unreachable QUIC listener, and a rejected stream. These records contain
//! stage names only: never QR payloads, pairing tokens, clipboard content, or
//! device names.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const FILE_NAME: &str = "pairing-diagnostics.log";
const MAX_BYTES: u64 = 32 * 1024;
const MAX_RECENT_LINES: usize = 16;

pub fn record_pairing_stage(data_dir: &Path, stage: &str) {
    if fs::create_dir_all(data_dir).is_err() {
        return;
    }

    let path = data_dir.join(FILE_NAME);
    if path
        .metadata()
        .map(|metadata| metadata.len() > MAX_BYTES)
        .unwrap_or(false)
    {
        let _ = fs::write(&path, "");
    }

    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{timestamp_ms} {stage}");
    }
}

pub fn recent_pairing_stages(data_dir: &Path) -> String {
    let contents = fs::read_to_string(data_dir.join(FILE_NAME)).unwrap_or_default();
    let mut lines = contents
        .lines()
        .rev()
        .take(MAX_RECENT_LINES)
        .collect::<Vec<_>>();
    lines.reverse();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{recent_pairing_stages, record_pairing_stage};

    #[test]
    fn records_only_the_recent_pairing_stages() {
        let directory = tempfile::tempdir().expect("create temporary diagnostics directory");

        for stage in 0..20 {
            record_pairing_stage(directory.path(), &format!("stage_{stage}"));
        }

        let stages = recent_pairing_stages(directory.path());
        assert!(!stages.contains("stage_0\n"));
        assert!(stages.contains("stage_4"));
        assert!(stages.contains("stage_19"));
    }
}
