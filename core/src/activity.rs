//! Best-effort reading of Sober's session logs for a play-history view.
//!
//! Sober writes timestamped logs to `data/sober/sober_logs/` plus a
//! `latest.log`. We surface them as "sessions"; this is intentionally light -
//! log formats change and we never depend on their internals.

use anyhow::Result;
use chrono::{DateTime, TimeZone, Utc};
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

/// One visit to a place, reconstructed from a Sober log.
///
/// Sober forwards the Roblox client's own log lines, so this is the same
/// information a Windows launcher reads - place, universe, job (server
/// instance) and the public server address.
#[derive(Clone, Debug, PartialEq)]
pub struct GameSession {
    pub place_id: u64,
    pub universe_id: Option<u64>,
    /// Server instance GUID, Roblox's "job id".
    pub job_id: String,
    /// Public server address as Roblox reports it, `ip|port`. The address on
    /// the join line itself is an internal one and is deliberately not used.
    pub server: Option<String>,
    pub joined: Option<DateTime<Utc>>,
    /// The leave line carries no timestamp of its own, so this is the last
    /// timestamp seen before the client reported leaving - accurate to within
    /// a log line, not exact.
    pub left: Option<DateTime<Utc>>,
}

impl GameSession {
    /// How long the visit lasted, when both ends are known.
    pub fn duration(&self) -> Option<chrono::Duration> {
        match (self.joined, self.left) {
            (Some(a), Some(b)) if b >= a => Some(b - a),
            _ => None,
        }
    }
}

/// The remainder of `line` after `needle`, if present.
fn after<'a>(line: &'a str, needle: &str) -> Option<&'a str> {
    line.find(needle).map(|i| &line[i + needle.len()..])
}

/// Leading run of ASCII digits, parsed.
fn leading_u64(s: &str) -> Option<u64> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// `... Roblox: 2026-08-28T10:54:39.831Z,1611.83,... ` -> the timestamp.
fn timestamp(line: &str) -> Option<DateTime<Utc>> {
    let rest = after(line, "Roblox: ")?;
    let stamp = rest.split(',').next()?;
    DateTime::parse_from_rfc3339(stamp)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Reconstruct the play sessions recorded in one log's text.
///
/// Deliberately tolerant: log formats change, so every field beyond the place
/// id is optional and an unrecognised line is simply skipped.
pub fn games_in_log(text: &str) -> Vec<GameSession> {
    let mut out: Vec<GameSession> = Vec::new();
    let mut current: Option<GameSession> = None;
    let mut last_ts: Option<DateTime<Utc>> = None;

    for line in text.lines() {
        if let Some(ts) = timestamp(line) {
            last_ts = Some(ts);
        }

        // "! Joining game '<jobid>' place <placeid> at <ip>"
        if let Some(rest) = after(line, "! Joining game '") {
            if let Some((job_id, tail)) = rest.split_once('\'') {
                if let Some(place_id) = after(tail, " place ").and_then(leading_u64) {
                    if let Some(prev) = current.take() {
                        out.push(prev);
                    }
                    current = Some(GameSession {
                        place_id,
                        universe_id: None,
                        job_id: job_id.to_string(),
                        server: None,
                        joined: timestamp(line).or(last_ts),
                        left: None,
                    });
                }
            }
            continue;
        }

        let Some(cur) = current.as_mut() else { continue };

        if let Some(rest) = after(line, "universeid:") {
            cur.universe_id = leading_u64(rest);
        } else if let Some(rest) = after(line, "serverId: ") {
            cur.server = rest.split_whitespace().next().map(str::to_string);
        } else if line.contains("onGameLeaveBegin()") {
            cur.left = last_ts;
            out.push(current.take().expect("checked above"));
        }
    }

    if let Some(last) = current {
        out.push(last);
    }
    out
}

/// Play sessions across Sober's logs, newest first.
pub fn recent_games(log_dir: &Path, limit: usize) -> Result<Vec<GameSession>> {
    let mut out = Vec::new();
    for s in sessions(log_dir)? {
        let Ok(text) = std::fs::read_to_string(&s.path) else {
            continue; // logs can contain invalid UTF-8; skip rather than fail
        };
        let mut games = games_in_log(&text);
        games.reverse(); // newest first within the log
        out.append(&mut games);
        if out.len() >= limit {
            break;
        }
    }
    out.truncate(limit);
    Ok(out)
}

#[cfg(test)]
mod game_tests {
    use super::*;

    /// Verbatim lines from a real Sober log.
    const LOG: &str = concat!(
        "info: Roblox: 2026-08-28T10:54:39.831Z,1611.831543,865986c0,6 [FLog::Output] ! Joining game 'ceea2e61-2e21-4fc6-8238-b090b805473c' place 129391875250493 at 10.30.9.15\n",
        "info: Roblox: 2026-08-28T10:54:39.831Z,1611.831909,865986c0,6 [FLog::GameJoinLoadTime] Report game_join_loadtime: sid:c8b4250d, clienttime:1787914478.0, join_time:0.456, referral_page:, placeid:129391875250493, userid:1715677889, universeid:10071500390, \n",
        "info: Roblox: 2026-08-28T10:54:40.065Z,1612.065552,86f996c0,7 [FLog::Network] serverId: 128.116.13.34|57069\n",
        "debug: rbx.jni: onGameLoaded() SessionReporterState_GameLoaded placeId:129391875250493\n",
        "info: Roblox: 2026-08-28T11:20:00.000Z,3000.0,86f996c0,7 [FLog::Output] something later\n",
        "debug: rbx.jni: onGameLeaveBegin() SessionReporterState_GameExitRequested placeId:129391875250493\n",
        "info: Roblox: 2026-08-28T11:44:41.184Z,4613.184570,54a2c6c0,6 [FLog::Output] ! Joining game '1f2a3aed-db97-4c8a-a0f8-ccbdbe3396fe' place 189707 at 10.34.1.16\n",
        "info: Roblox: 2026-08-28T11:44:41.186Z,4613.186523,54a2c6c0,6 [FLog::GameJoinLoadTime] Report game_join_loadtime: placeid:189707, userid:1715677889, universeid:65241, \n",
    );

    #[test]
    fn parses_both_visits_from_a_real_log() {
        let games = games_in_log(LOG);
        assert_eq!(games.len(), 2);
    }

    #[test]
    fn first_visit_has_place_universe_job_and_public_server() {
        let g = &games_in_log(LOG)[0];
        assert_eq!(g.place_id, 129391875250493);
        assert_eq!(g.universe_id, Some(10071500390));
        assert_eq!(g.job_id, "ceea2e61-2e21-4fc6-8238-b090b805473c");
        // The public serverId, not the internal 10.30.9.15 on the join line.
        assert_eq!(g.server.as_deref(), Some("128.116.13.34|57069"));
    }

    #[test]
    fn duration_spans_join_to_last_line_before_leaving() {
        let g = &games_in_log(LOG)[0];
        assert_eq!(g.duration().map(|d| d.num_minutes()), Some(25));
    }

    #[test]
    fn a_visit_still_open_at_end_of_log_is_kept() {
        let g = &games_in_log(LOG)[1];
        assert_eq!(g.place_id, 189707);
        assert_eq!(g.universe_id, Some(65241));
        assert!(g.left.is_none());
        assert!(g.duration().is_none());
    }

    #[test]
    fn junk_is_not_a_panic() {
        assert!(games_in_log("").is_empty());
        assert!(games_in_log("nothing to see\nhere at all").is_empty());
        // Truncated join line: no place id, so no session.
        assert!(games_in_log("! Joining game 'abc' place ").is_empty());
    }
}
