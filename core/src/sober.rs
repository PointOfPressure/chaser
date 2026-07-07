//! The only module that knows Sober's on-disk layout and launch mechanics.
//!
//! Sober's `config.json` is JSONC (it ships with a `// !!! STOP !!!` comment
//! header) and its schema grows across versions, so we:
//!   * strip comments before parsing,
//!   * preserve the leading comment block verbatim on write,
//!   * keep the config as an order-preserving object and only touch the keys
//!     we manage — unknown keys are never dropped.

use crate::models::Profile;
use crate::paths;
use anyhow::{anyhow, Context, Result};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const APP_ID: &str = "org.vinegarhq.Sober";

/// True when Chaser itself is running inside a Flatpak sandbox (in which case
/// `flatpak` on the host must be reached via `flatpak-spawn --host`).
pub fn in_sandbox() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// A `flatpak` command that works both natively and from inside a sandbox.
fn host_flatpak_command() -> Command {
    if in_sandbox() {
        let mut c = Command::new("flatpak-spawn");
        c.args(["--host", "flatpak"]);
        c
    } else {
        Command::new("flatpak")
    }
}

/// Default header written when we have to create a fresh config file.
const DEFAULT_PREAMBLE: &str = "\
// !!! STOP !!!
// This file is not meant to be edited by hand unless you know what you're doing. You are encouraged to use the settings menu instead (Right click Sober in your apps menu, then hit \"Settings\")
// You can prevent Sober from launching by improperly formatting this file's JSON, or possibly mess up the Roblox engine by toggling certain flags intended for Roblox engineers.
// Incase you mess up, you can reset this file by deleting it. It will be recreated the next time you launch Sober.
// -------------------------------------------
// Documentation is available at https://vinegarhq.org/Sober/Configuration/index.html - We encourage you to read it before toggling anything.
// (This file is currently managed by Chaser.)
";

/// A detected Sober installation.
#[derive(Clone, Debug)]
pub struct SoberInstall {
    pub version: Option<String>,
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl SoberInstall {
    /// The canonical config path under the Flatpak per-app home.
    pub fn config_path() -> PathBuf {
        paths::home()
            .join(".var/app")
            .join(APP_ID)
            .join("config/sober/config.json")
    }

    pub fn data_dir() -> PathBuf {
        paths::home()
            .join(".var/app")
            .join(APP_ID)
            .join("data/sober")
    }

    /// True if the Sober Flatpak is installed (checked via `flatpak info`).
    pub fn is_installed() -> bool {
        host_flatpak_command()
            .args(["info", APP_ID])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Detect the installation, returning paths and the reported version.
    /// Succeeds even if the config file doesn't exist yet (Sober never launched).
    pub fn detect() -> Result<Self> {
        if !Self::is_installed() {
            return Err(anyhow!(
                "Sober does not appear to be installed. Install it with:\n    flatpak install flathub {APP_ID}"
            ));
        }
        Ok(SoberInstall {
            version: detect_version(),
            config_path: Self::config_path(),
            data_dir: Self::data_dir(),
            log_dir: Self::data_dir().join("sober_logs"),
        })
    }
}

/// Parse `flatpak info` output for the `Version:` field.
fn detect_version() -> Option<String> {
    let out = host_flatpak_command()
        .args(["info", APP_ID])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Version:") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

/// A loaded Sober config: the preserved comment header plus the JSON object.
#[derive(Clone, Debug)]
pub struct SoberConfig {
    /// Leading comment/blank lines, re-emitted verbatim on save.
    preamble: String,
    /// The top-level JSON object (order-preserving via serde_json preserve_order).
    root: Map<String, Value>,
}

impl SoberConfig {
    /// Load and parse the config at `path`.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading Sober config at {}", path.display()))?;
        Self::parse(&raw)
    }

    /// Load the config, or synthesise an empty one (with our default header)
    /// if the file does not exist yet.
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::load(path)
        } else {
            Ok(SoberConfig {
                preamble: DEFAULT_PREAMBLE.to_string(),
                root: Map::new(),
            })
        }
    }

    /// Parse raw JSONC text into a `SoberConfig`.
    pub fn parse(raw: &str) -> Result<Self> {
        let (preamble, body) = split_preamble(raw);
        let stripped = strip_jsonc_comments(body);
        let value: Value = serde_json::from_str(&stripped)
            .context("Sober config is not valid JSON (after stripping comments)")?;
        let root = match value {
            Value::Object(map) => map,
            other => {
                return Err(anyhow!(
                    "Sober config root is not a JSON object (found {})",
                    kind_name(&other)
                ))
            }
        };
        Ok(SoberConfig { preamble, root })
    }

    /// Serialise back to text: preserved header + 4-space-indented JSON body.
    pub fn to_text(&self) -> Result<String> {
        let mut out = String::new();
        if !self.preamble.is_empty() {
            out.push_str(&self.preamble);
            if !self.preamble.ends_with('\n') {
                out.push('\n');
            }
        }
        let body = to_pretty_4space(&Value::Object(self.root.clone()))?;
        out.push_str(&body);
        out.push('\n');
        Ok(out)
    }

    /// Atomically write the config to `path`, backing up any existing file first.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating config directory {}", parent.display()))?;
        }
        if path.exists() {
            backup_config(path).context("backing up existing Sober config")?;
        }
        let text = self.to_text()?;
        let tmp = path.with_extension("json.chaser-tmp");
        std::fs::write(&tmp, text.as_bytes())
            .with_context(|| format!("writing temp config {}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .with_context(|| format!("moving temp config into place at {}", path.display()))?;
        Ok(())
    }

    // --- typed accessors over the managed keys ---

    pub fn raw(&self) -> &Map<String, Value> {
        &self.root
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.root.get(key).and_then(Value::as_bool)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.root.get(key).and_then(Value::as_str)
    }

    pub fn set_bool(&mut self, key: &str, val: bool) {
        self.root.insert(key.to_string(), Value::Bool(val));
    }

    pub fn set_str(&mut self, key: &str, val: &str) {
        self.root
            .insert(key.to_string(), Value::String(val.to_string()));
    }

    /// The current `fflags` object (empty if absent or wrong type).
    pub fn fflags(&self) -> Map<String, Value> {
        match self.root.get("fflags") {
            Some(Value::Object(m)) => m.clone(),
            _ => Map::new(),
        }
    }

    pub fn set_fflags(&mut self, flags: Map<String, Value>) {
        self.root.insert("fflags".to_string(), Value::Object(flags));
    }

    /// Merge a profile's managed keys into this config. Only keys the profile
    /// sets (Some / non-empty) are touched; everything else is left intact.
    pub fn apply_profile(&mut self, profile: &Profile) {
        if let Some(mode) = profile.graphics_mode {
            self.set_str("graphics_optimization_mode", mode.as_key());
        }
        if let Some(r) = profile.renderer {
            self.set_bool("use_opengl", r.use_opengl());
        }
        if let Some(v) = profile.enable_gamemode {
            self.set_bool("enable_gamemode", v);
        }
        if let Some(v) = profile.discord_rpc {
            self.set_bool("discord_rpc_enabled", v);
        }
        if let Some(v) = profile.enable_hidpi {
            self.set_bool("enable_hidpi", v);
        }
        if let Some(v) = profile.allow_gamepad {
            self.set_bool("allow_gamepad_permission", v);
        }
        // A profile fully owns the fflags set it declares.
        self.set_fflags(profile.fflags.clone());
    }
}

/// Everything needed to spawn Sober: argv plus (unused, env goes via argv).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl LaunchSpec {
    /// A shell-ish preview string for dry-run display.
    pub fn preview(&self) -> String {
        let mut parts = vec![self.program.clone()];
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }

    pub fn to_command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        cmd
    }
}

/// Build the `flatpak run` invocation for a profile, optionally opening a
/// Roblox deep-link URI. Environment (MangoHud, custom vars) is injected via
/// `flatpak run --env=` so it reaches the sandboxed app.
pub fn build_launch(profile: &Profile, uri: Option<&str>) -> LaunchSpec {
    build_launch_for(profile, uri, in_sandbox())
}

/// Like [`build_launch`], but with the sandbox decision made explicit
/// (`sandboxed` = Chaser itself runs inside Flatpak and must escape via
/// `flatpak-spawn --host`). Split out so it is unit-testable.
pub fn build_launch_for(profile: &Profile, uri: Option<&str>, sandboxed: bool) -> LaunchSpec {
    let mut args: Vec<String> = Vec::new();
    let program = if sandboxed {
        args.push("--host".to_string());
        args.push("flatpak".to_string());
        "flatpak-spawn"
    } else {
        "flatpak"
    };
    args.push("run".to_string());

    // Deterministic env ordering: BTreeMap iterates sorted; MangoHud appended last.
    for (k, v) in &profile.env {
        args.push(format!("--env={k}={v}"));
    }
    if profile.mangohud {
        args.push("--env=MANGOHUD=1".to_string());
    }

    args.push(APP_ID.to_string());
    if let Some(u) = uri {
        args.push(u.to_string());
    }

    LaunchSpec {
        program: program.to_string(),
        args,
    }
}

// --- JSONC helpers ---

/// Split leading blank/`//`-comment lines (the header) from the JSON body.
fn split_preamble(s: &str) -> (String, &str) {
    let mut idx = 0;
    for line in s.split_inclusive('\n') {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with("//") {
            idx += line.len();
        } else {
            break;
        }
    }
    (s[..idx].to_string(), &s[idx..])
}

/// Remove `//` line comments and `/* */` block comments, respecting string
/// literals (so a `//` inside a JSON string value is preserved).
fn strip_jsonc_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' => match chars.peek() {
                Some('/') => {
                    // line comment: consume to end of line
                    while let Some(&n) = chars.peek() {
                        if n == '\n' {
                            break;
                        }
                        chars.next();
                    }
                }
                Some('*') => {
                    chars.next(); // consume '*'
                    let mut prev = '\0';
                    for n in chars.by_ref() {
                        if prev == '*' && n == '/' {
                            break;
                        }
                        prev = n;
                    }
                }
                _ => out.push(c),
            },
            _ => out.push(c),
        }
    }
    out
}

/// Serialise a JSON value with 4-space indentation to match Sober's style.
fn to_pretty_4space(value: &Value) -> Result<String> {
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    serde::Serialize::serialize(value, &mut ser)?;
    Ok(String::from_utf8(buf)?)
}

/// How many timestamped config backups to keep around.
const MAX_BACKUPS: usize = 20;

/// Copy the existing config into Chaser's timestamped backup directory,
/// then prune old backups so the directory doesn't grow forever.
fn backup_config(path: &Path) -> Result<PathBuf> {
    let dir = paths::backup_dir()?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = dir.join(format!("config-{stamp}.json"));
    std::fs::copy(path, &dest)
        .with_context(|| format!("copying {} to {}", path.display(), dest.display()))?;
    prune_backups(&dir, MAX_BACKUPS)?;
    Ok(dest)
}

/// Delete the oldest `config-*.json` backups beyond `keep`. The fixed-width
/// timestamp in the filename makes lexical order chronological.
fn prune_backups(dir: &Path, keep: usize) -> Result<()> {
    let mut backups: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("config-") && n.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect();
    if backups.len() <= keep {
        return Ok(());
    }
    backups.sort(); // lexical == chronological for our fixed-width stamps
    let excess = backups.len() - keep;
    for old in backups.into_iter().take(excess) {
        let _ = std::fs::remove_file(old);
    }
    Ok(())
}

fn kind_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{GraphicsMode, Profile, Renderer};
    use serde_json::json;

    const SAMPLE: &str = r#"// !!! STOP !!!
// Do not edit by hand.
{
    "allow_gamepad_permission": false,
    "enable_mobile_home_screen": false,
    "fflags": {
        "FFlagExample": true
    },
    "graphics_optimization_mode": "balanced",
    "touch_mode": "fake_off",
    "use_opengl": true
}
"#;

    #[test]
    fn parse_preserves_unknown_keys_and_order() {
        let cfg = SoberConfig::parse(SAMPLE).unwrap();
        // Unknown-to-us key survives.
        assert_eq!(cfg.get_bool("enable_mobile_home_screen"), Some(false));
        assert_eq!(cfg.get_str("touch_mode"), Some("fake_off"));
        // Order preserved: first key stays first.
        let keys: Vec<&String> = cfg.raw().keys().collect();
        assert_eq!(
            keys.first().map(|s| s.as_str()),
            Some("allow_gamepad_permission")
        );
    }

    #[test]
    fn roundtrip_keeps_header_and_reparses() {
        let cfg = SoberConfig::parse(SAMPLE).unwrap();
        let text = cfg.to_text().unwrap();
        assert!(text.starts_with("// !!! STOP !!!"));
        // Re-parsing the output yields the same known values.
        let again = SoberConfig::parse(&text).unwrap();
        assert_eq!(
            again.get_str("graphics_optimization_mode"),
            Some("balanced")
        );
        assert_eq!(again.get_bool("enable_mobile_home_screen"), Some(false));
    }

    #[test]
    fn strip_respects_strings_with_slashes() {
        let input = r#"{ "url": "https://example.com//x", "a": 1 }"#;
        let out = strip_jsonc_comments(input);
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["url"], json!("https://example.com//x"));
        assert_eq!(v["a"], json!(1));
    }

    #[test]
    fn apply_profile_sets_only_managed_keys() {
        let mut cfg = SoberConfig::parse(SAMPLE).unwrap();
        let mut p = Profile::new("Test");
        p.graphics_mode = Some(GraphicsMode::Performance);
        p.renderer = Some(Renderer::Vulkan);
        p.fflags
            .insert("DFIntTaskSchedulerTargetFps".into(), json!(240));

        cfg.apply_profile(&p);

        assert_eq!(
            cfg.get_str("graphics_optimization_mode"),
            Some("performance")
        );
        assert_eq!(cfg.get_bool("use_opengl"), Some(false)); // Vulkan
                                                             // Untouched key preserved.
        assert_eq!(cfg.get_str("touch_mode"), Some("fake_off"));
        // fflags replaced by the profile's set.
        let flags = cfg.fflags();
        assert_eq!(flags.get("DFIntTaskSchedulerTargetFps"), Some(&json!(240)));
        assert!(!flags.contains_key("FFlagExample"));
    }

    #[test]
    fn prune_keeps_newest_backups() {
        let dir = std::env::temp_dir().join("chaser-prune-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..25 {
            std::fs::write(dir.join(format!("config-20260101-0000{i:02}.json")), "{}").unwrap();
        }
        // A non-backup file must never be touched.
        std::fs::write(dir.join("unrelated.txt"), "keep me").unwrap();

        prune_backups(&dir, 20).unwrap();

        let remaining: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().and_then(|e| e.file_name().into_string().ok()))
            .collect();
        let backups: Vec<&String> = remaining
            .iter()
            .filter(|n| n.starts_with("config-"))
            .collect();
        assert_eq!(backups.len(), 20);
        // The oldest five are gone, the newest survive.
        assert!(!remaining.contains(&"config-20260101-000000.json".to_string()));
        assert!(!remaining.contains(&"config-20260101-000004.json".to_string()));
        assert!(remaining.contains(&"config-20260101-000024.json".to_string()));
        assert!(remaining.contains(&"unrelated.txt".to_string()));
    }

    #[test]
    fn launch_builds_flatpak_argv() {
        let mut p = Profile::new("Perf");
        p.mangohud = true;
        p.env
            .insert("__GL_THREADED_OPTIMIZATIONS".into(), "1".into());
        let spec = build_launch_for(&p, Some("roblox://experiences/start?placeId=1"), false);
        assert_eq!(spec.program, "flatpak");
        assert_eq!(spec.args[0], "run");
        assert!(spec.args.iter().any(|a| a == "--env=MANGOHUD=1"));
        assert!(spec
            .args
            .iter()
            .any(|a| a == "--env=__GL_THREADED_OPTIMIZATIONS=1"));
        assert_eq!(
            spec.args.last().unwrap(),
            "roblox://experiences/start?placeId=1"
        );
        // App id present before the URI.
        let app_pos = spec.args.iter().position(|a| a == APP_ID).unwrap();
        let uri_pos = spec.args.len() - 1;
        assert!(app_pos < uri_pos);
    }

    #[test]
    fn sandboxed_launch_escapes_via_flatpak_spawn() {
        let p = Profile::new("Any");
        let spec = build_launch_for(&p, None, true);
        assert_eq!(spec.program, "flatpak-spawn");
        assert_eq!(spec.args[0], "--host");
        assert_eq!(spec.args[1], "flatpak");
        assert_eq!(spec.args[2], "run");
        assert_eq!(spec.args.last().unwrap(), APP_ID);
    }
}
