//! Chaser core: all Sober-facing logic, UI-independent and unit-tested.
//!
//! Layers:
//!   * [`sober`]   — detect Sober, read/write its JSONC config, build launches
//!   * [`profile`] — saved profiles + built-in presets
//!   * [`fflags`]  — curated FastFlag catalog
//!   * [`activity`]— best-effort session history from Sober logs
//!   * [`models`]  — shared types

pub mod activity;
pub mod fflags;
pub mod models;
pub mod profile;
pub mod sober;

pub use models::{FFlagDef, FFlagKind, GraphicsMode, Profile, Renderer, Risk};
pub use profile::Store;
pub use sober::{build_launch, LaunchSpec, SoberConfig, SoberInstall};

/// Filesystem locations Chaser uses. Directory getters create the directory.
pub mod paths {
    use anyhow::{anyhow, Result};
    use std::path::PathBuf;

    /// The user's home directory (falls back to `/` only if truly undetectable).
    pub fn home() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
    }

    /// `~/.config/chaser`, created if missing.
    pub fn config_dir() -> Result<PathBuf> {
        let d = dirs::config_dir()
            .ok_or_else(|| anyhow!("could not determine the XDG config directory"))?
            .join("chaser");
        std::fs::create_dir_all(&d)?;
        Ok(d)
    }

    /// `~/.config/chaser/profiles`, created if missing.
    pub fn profiles_dir() -> Result<PathBuf> {
        let d = config_dir()?.join("profiles");
        std::fs::create_dir_all(&d)?;
        Ok(d)
    }

    /// `~/.config/chaser/backups`, created if missing.
    pub fn backup_dir() -> Result<PathBuf> {
        let d = config_dir()?.join("backups");
        std::fs::create_dir_all(&d)?;
        Ok(d)
    }

    /// `~/.config/chaser/state.json` (not created; may not exist yet).
    pub fn state_path() -> Result<PathBuf> {
        Ok(config_dir()?.join("state.json"))
    }
}
