//! `chaser` — a small headless front-end over `chaser-core`.
//!
//! Useful on its own (scriptable) and as the verification vehicle for the core
//! logic against a real Sober install without needing the GTK GUI.

use anyhow::{anyhow, bail, Context, Result};
use chaser_core::{build_launch, fflags, Profile, SoberConfig, SoberInstall, Store};

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
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
        active.as_deref().unwrap_or("(none — run `chaser init`)")
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
            "Sober config not found at {} — launch Sober once to create it",
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
    println!("Curated FastFlag catalog (unsupported by VinegarHQ — use at your own risk):\n");
    for f in fflags::catalog() {
        println!(
            "  {:<38} [{:<10} {:<8}] {}",
            f.name,
            f.category,
            f.risk.label(),
            first_line(f.description)
        );
    }
    Ok(())
}

fn cmd_sessions() -> Result<()> {
    let s = SoberInstall::detect()?;
    let sessions = chaser_core::activity::sessions(&s.log_dir)?;
    if sessions.is_empty() {
        println!("No Sober sessions found in {}", s.log_dir.display());
        return Ok(());
    }
    for sess in sessions.iter().take(20) {
        println!("  {}   {} KiB", sess.label, sess.size_bytes / 1024);
    }
    Ok(())
}

fn cmd_init() -> Result<()> {
    let store = Store::open()?;
    if store.ensure_defaults()? {
        println!("Created built-in presets: competitive-fps, balanced, cinematic, potato");
        println!("Active profile set to 'balanced'.");
    } else {
        println!("Profiles already exist — nothing to do.");
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
        "chaser — manage Sober (Roblox on Linux) profiles and FastFlags\n\n\
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
help                    Show this help"
    );
}
