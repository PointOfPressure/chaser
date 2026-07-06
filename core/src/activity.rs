//! Best-effort reading of Sober's session logs for a play-history view.
//!
//! Sober writes timestamped logs to `data/sober/sober_logs/` plus a
//! `latest.log`. We surface them as "sessions"; this is intentionally light —
//! log formats change and we never depend on their internals.

use anyhow::Result;
use chrono::TimeZone;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct Session {
    pub path: PathBuf,
    /// Parsed from the filename when possible, else the file's modified time.
    pub started: SystemTime,
    /// Human label, e.g. "2026-07-06 22:43:25".
    pub label: String,
    pub size_bytes: u64,
}

/// List play sessions found in Sober's log directory, newest first.
/// Returns an empty vec (not an error) if the directory doesn't exist.
pub fn sessions(log_dir: &Path) -> Result<Vec<Session>> {
    let mut out = Vec::new();
    if !log_dir.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(log_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if path.extension().and_then(|e| e.to_str()) != Some("log") {
            continue;
        }
        if name == "latest.log" {
            continue; // symlink/dupe of the newest real log
        }
        let meta = entry.metadata()?;
        let started = parse_stamp_from_name(name)
            .or_else(|| meta.modified().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        out.push(Session {
            label: label_for(name, started),
            path,
            started,
            size_bytes: meta.len(),
        });
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.started));
    Ok(out)
}

/// Parse a `YYYY-MM-DD_HH-MM-SS` filename stem into a SystemTime (local).
fn parse_stamp_from_name(name: &str) -> Option<SystemTime> {
    let stem = name.strip_suffix(".log").unwrap_or(name);
    let dt = chrono::NaiveDateTime::parse_from_str(stem, "%Y-%m-%d_%H-%M-%S").ok()?;
    let local = chrono::Local.from_local_datetime(&dt).single()?;
    Some(SystemTime::from(local))
}

fn label_for(name: &str, fallback: SystemTime) -> String {
    let stem = name.strip_suffix(".log").unwrap_or(name);
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(stem, "%Y-%m-%d_%H-%M-%S") {
        return dt.format("%Y-%m-%d %H:%M:%S").to_string();
    }
    let secs = fallback
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("log ({secs})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_dir_is_not_an_error() {
        let dir = std::env::temp_dir().join("chaser-activity-test-missing");
        let _ = std::fs::remove_dir_all(&dir);
        let s = sessions(&dir).unwrap();
        assert!(s.is_empty());
    }

    #[test]
    fn label_parses_timestamp() {
        assert_eq!(
            label_for("2026-07-06_22-43-25.log", SystemTime::UNIX_EPOCH),
            "2026-07-06 22:43:25"
        );
    }
}
