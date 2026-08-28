//! `chaser`, a small headless front-end over `chaser-core`.
//!
//! Useful on its own (scriptable) and as the verification vehicle for the core
//! logic against a real Sober install without needing the GTK GUI.

use anyhow::{anyhow, bail, Context, Result};
use chaser_core::validate::{self, Verdict};
use chaser_core::{build_launch, fflags, Profile, SoberConfig, SoberInstall, Store};

fn main() {
    reset_sigpipe();
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

/// Restore the default SIGPIPE handler.
///
/// Rust ignores SIGPIPE at startup, which turns `chaser sessions | head` into a
/// panic on a broken pipe instead of a quiet exit. Every subcommand prints, so
/// this belongs at the entry point rather than around individual writes.
fn reset_sigpipe() {
    #[cfg(unix)]
    // SAFETY: `signal` with SIG_DFL is async-signal-safe and this runs before
    // any threads are spawned.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let rest = &args[args.len().min(1)..];

    match cmd {
        "status" => cmd_status(),
        "config" => cmd_config(rest),
        "profiles" => cmd_profiles(),
        "show" => cmd_show(rest),
        "apply" => cmd_apply(rest),
        "launch" => cmd_launch(rest),
        "fflags" => cmd_fflags(),
        "sessions" => cmd_sessions(),
        "doctor" => cmd_doctor(),
        "import" => cmd_import(rest),
        "init" => cmd_init(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => {
            eprintln!("unknown command: {other}\n");
            print_help();
            std::process::exit(2);
        }
    }
}

fn cmd_status() -> Result<()> {
    println!("Chaser {}", env!("CARGO_PKG_VERSION"));
    match SoberInstall::detect() {
        Ok(s) => {
            println!("Sober:        installed");
            println!(
                "  version:    {}",
                s.version.as_deref().unwrap_or("(unknown)")
            );
            println!("  config:     {}", s.config_path.display());
            println!(
                "  config file:{}",
                if s.config_path.exists() {
                    " present"
                } else {
                    " not created yet (launch Sober once)"
                }
            );
            println!("  logs:       {}", s.log_dir.display());
        }
        Err(e) => println!("Sober:        {e}"),
    }

    let store = Store::open()?;
    let profiles = store.list()?;
    let active = store.active_slug()?;
    println!("Profiles:     {} saved", profiles.len());
    println!(
        "  active:     {}",
        active.as_deref().unwrap_or("(none, run `chaser init`)")
    );
    Ok(())
}

fn cmd_config(rest: &[String]) -> Result<()> {
    let path = SoberInstall::config_path();
    if rest.iter().any(|a| a == "--path") {
        println!("{}", path.display());
        return Ok(());
    }
    if !path.exists() {
        bail!(
            "Sober config not found at {}, launch Sober once to create it",
            path.display()
        );
    }
    let cfg = SoberConfig::load(&path)?;
    println!("{}", serde_json::to_string_pretty(cfg.raw())?);
    Ok(())
}

fn cmd_profiles() -> Result<()> {
    let store = Store::open()?;
    let active = store.active_slug()?;
    let profiles = store.list()?;
    if profiles.is_empty() {
        println!("No profiles yet. Run `chaser init` to create the built-in presets.");
        return Ok(());
    }
    for p in profiles {
        let marker = if active.as_deref() == Some(&p.slug()) {
            "*"
        } else {
            " "
        };
        println!("{marker} {:<16} {}", p.slug(), p.name);
        if !p.description.is_empty() {
            println!("    {}", p.description);
        }
    }
    Ok(())
}

fn cmd_show(rest: &[String]) -> Result<()> {
    let slug = rest
        .first()
        .ok_or_else(|| anyhow!("usage: chaser show <slug>"))?;
    let store = Store::open()?;
    let p = store.load(slug)?;
    println!("{}", serde_json::to_string_pretty(&p)?);
    Ok(())
}

fn cmd_apply(rest: &[String]) -> Result<()> {
    let slug = rest
        .first()
        .ok_or_else(|| anyhow!("usage: chaser apply <slug>"))?;
    let store = Store::open()?;
    let profile = store.load(slug)?;
    apply_profile_to_sober(&profile)?;
    store.set_active(&profile.slug())?;
    println!(
        "Applied profile '{}' to Sober config. Restart Sober for changes to take effect.",
        profile.name
    );
    Ok(())
}

fn cmd_launch(rest: &[String]) -> Result<()> {
    let mut dry_run = false;
    let mut profile_slug: Option<String> = None;
    let mut uri: Option<String> = None;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dry-run" => dry_run = true,
            "--profile" => {
                profile_slug = Some(
                    it.next()
                        .cloned()
                        .ok_or_else(|| anyhow!("--profile needs a slug"))?,
                )
            }
            other if other.starts_with("--") => bail!("unknown flag: {other}"),
            other => uri = Some(other.to_string()),
        }
    }

    let store = Store::open()?;
    let profile = match profile_slug {
        Some(s) => store.load(&s)?,
        None => store
            .active()?
            .ok_or_else(|| anyhow!("no active profile; run `chaser init` or pass --profile"))?,
    };

    apply_profile_to_sober(&profile)?;
    store.set_active(&profile.slug())?;
    let spec = build_launch(&profile, uri.as_deref());

    if dry_run {
        println!("{}", spec.preview());
        return Ok(());
    }
    println!("Launching Sober with profile '{}'...", profile.name);
    spec.to_command()
        .spawn()
        .context("failed to spawn `flatpak run`")?;
    Ok(())
}

fn cmd_fflags() -> Result<()> {
    println!("Curated FastFlag catalog (unsupported by VinegarHQ, use at your own risk):\n");
    for f in fflags::catalog() {
        println!(
            "  {:<8} {:<42} [{:<13} {:<8}] {}",
            validate::verdict(f.name).label().to_uppercase(),
            f.name,
            f.category,
            f.risk.label(),
            first_line(f.description)
        );
    }
    println!(
        "\n  LIVE = on Roblox's allowlist. IGNORED = a real flag the client discards.\n  \
         Only allowlisted flags have any effect via a config file."
    );
    Ok(())
}

/// Audit the FastFlags actually sitting in Sober's config right now.
///
/// This is the command that pays for the whole `validate` module: it tells you
/// which of your flags the client honours, which it silently discards, and
/// which were never real flags to begin with.
fn cmd_doctor() -> Result<()> {
    let path = SoberInstall::config_path();
    let cfg = SoberConfig::load_or_default(&path)?;
    let flags = cfg.fflags();

    if flags.is_empty() {
        println!("No FastFlags set in {}", path.display());
        return Ok(());
    }

    println!("Auditing {} FastFlags in {}\n", flags.len(), path.display());

    let (mut live, mut ignored, mut broken) = (0, 0, 0);
    for (name, verdict) in validate::audit(&flags) {
        let value = flags.get(&name).map(|v| v.to_string()).unwrap_or_default();
        let (tag, note) = match &verdict {
            Verdict::Live => {
                live += 1;
                ("LIVE", String::new())
            }
            Verdict::Ignored => {
                ignored += 1;
                ("IGNORED", "not on Roblox's allowlist, so the client discards it".into())
            }
            Verdict::WrongPrefix { found } => {
                broken += 1;
                ("WRONG", format!("wrong prefix; try {}{}", found[0], strip_prefix(&name)))
            }
            Verdict::Unknown => {
                broken += 1;
                ("FAKE", "no such flag in any Roblox build".into())
            }
        };
        println!("  {tag:<8} {name:<44} = {value:<8} {note}");
    }

    println!("\n  {live} live, {ignored} ignored, {broken} not real.");
    let inert = ignored + broken;
    if inert > 0 {
        println!(
            "  {inert} of {} flags in this config do nothing.",
            flags.len()
        );
    }
    Ok(())
}

/// Merge a shared FastFlag blob into the active profile.
///
/// Flag presets get traded around as raw JSON and are full of invented names.
/// Every incoming flag is checked, and anything that is not a real engine
/// symbol is dropped rather than written into the profile - `--all` keeps them.
fn cmd_import(rest: &[String]) -> Result<()> {
    let mut keep_all = false;
    let mut file: Option<String> = None;
    for a in rest {
        match a.as_str() {
            "--all" => keep_all = true,
            other if other.starts_with("--") => bail!("unknown flag: {other}"),
            other => file = Some(other.to_string()),
        }
    }
    let file = file.ok_or_else(|| anyhow!("usage: chaser import <file.json> [--all]"))?;

    let raw = std::fs::read_to_string(&file).with_context(|| format!("reading {file}"))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&raw).with_context(|| format!("{file} is not valid JSON"))?;

    // Accept either a bare {"FFlagX": true} blob or a whole Chaser profile.
    let incoming = parsed
        .get("fflags")
        .unwrap_or(&parsed)
        .as_object()
        .ok_or_else(|| anyhow!("expected a JSON object of flag names to values"))?
        .clone();

    if incoming.is_empty() {
        bail!("no flags found in {file}");
    }

    let store = Store::open()?;
    let mut profile = store
        .active()?
        .ok_or_else(|| anyhow!("no active profile; run `chaser init` first"))?;

    let (mut added, mut dropped) = (0, 0);
    for (name, value) in incoming {
        let verdict = validate::verdict(&name);
        let fabricated = matches!(verdict, Verdict::Unknown | Verdict::WrongPrefix { .. });
        if fabricated && !keep_all {
            println!("  dropped  {name:<44} {}", verdict.label());
            dropped += 1;
            continue;
        }
        println!("  kept     {name:<44} {}", verdict.label());
        profile.fflags.insert(name, value);
        added += 1;
    }

    store.save(&profile)?;
    println!("\n  {added} flags merged into '{}', {dropped} dropped.", profile.name);
    if dropped > 0 {
        println!("  Re-run with --all to keep the dropped ones anyway.");
    }
    println!("  Run `chaser apply {}` to write them to Sober.", profile.slug());
    Ok(())
}

/// Best-effort strip of a flag's prefix, for "did you mean" suggestions.
fn strip_prefix(name: &str) -> String {
    for p in ["DFString", "SFString", "FString", "DFFlag", "SFFlag", "DFInt", "DFLog", "FFlag", "FInt", "FLog"] {
        if let Some(rest) = name.strip_prefix(p) {
            return rest.to_string();
        }
    }
    name.to_string()
}

fn cmd_sessions() -> Result<()> {
    let s = SoberInstall::detect()?;
    let games = chaser_core::activity::recent_games(&s.log_dir, 20)?;
    if games.is_empty() {
        println!("No play sessions found in {}", s.log_dir.display());
        return Ok(());
    }
    println!("Recent play sessions:\n");
    for g in games {
        let when = g
            .joined
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|| "unknown".into());
        let dur = match g.duration() {
            Some(d) if d.num_hours() > 0 => format!("{}h{:02}m", d.num_hours(), d.num_minutes() % 60),
            Some(d) => format!("{}m", d.num_minutes()),
            None => "in progress".into(),
        };
        let universe = g
            .universe_id
            .map(|u| u.to_string())
            .unwrap_or_else(|| "-".into());
        println!(
            "  {when}  place {:<16} universe {:<13} {:<12} {}",
            g.place_id,
            universe,
            dur,
            g.server.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

fn cmd_init() -> Result<()> {
    let store = Store::open()?;
    if store.ensure_defaults()? {
        println!("Created built-in presets: competitive-fps, balanced, cinematic, potato");
        println!("Active profile set to 'balanced'.");
    } else {
        println!("Profiles already exist, nothing to do.");
    }
    Ok(())
}

/// Load-or-create the Sober config, merge the profile, and save (with backup).
fn apply_profile_to_sober(profile: &Profile) -> Result<()> {
    if !SoberInstall::is_installed() {
        bail!("Sober is not installed; cannot apply a profile");
    }
    let path = SoberInstall::config_path();
    let mut cfg = SoberConfig::load_or_default(&path)?;
    cfg.apply_profile(profile);
    cfg.save(&path)?;
    Ok(())
}

fn first_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn print_help() {
    println!(
        "chaser, manage Sober (Roblox on Linux) profiles and FastFlags\n\n\
USAGE:\n  chaser <command> [args]\n\n\
COMMANDS:\n  \
status                  Show Sober install + active profile\n  \
init                    Create the built-in preset profiles\n  \
profiles                List saved profiles (* = active)\n  \
show <slug>             Print a profile as JSON\n  \
apply <slug>            Write a profile into Sober's config\n  \
launch [opts] [uri]     Apply active profile and launch Sober\n    \
    --profile <slug>    Use this profile instead of the active one\n    \
    --dry-run           Print the flatpak command without running it\n  \
config [--path]         Print Sober's parsed config (or just its path)\n  \
fflags                  List the curated FastFlag catalog\n  \
sessions                List recent Sober play sessions\n  \
doctor                  Check which FastFlags in Sober's config actually work\n  \
import <file> [--all]   Merge a shared FastFlag JSON into the active profile\n    \
    --all               Keep fabricated flags too, instead of dropping them\n  \
help                    Show this help"
    );
}
