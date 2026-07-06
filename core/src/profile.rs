//! Profile persistence (one JSON file per profile under Chaser's config dir)
//! and the built-in preset bundles.

use crate::models::{GraphicsMode, Profile, Renderer};
use crate::paths;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

/// On-disk pointer to the currently active profile.
#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    active: Option<String>,
}

/// Manages the set of saved profiles and which one is active.
pub struct Store {
    profiles_dir: PathBuf,
    state_path: PathBuf,
}

impl Store {
    /// Open the store at the default XDG location, creating directories.
    pub fn open() -> Result<Self> {
        let store = Self::open_at(paths::profiles_dir()?, paths::state_path()?)?;
        paths::backup_dir()?; // side effect: make sure the backup dir exists too
        Ok(store)
    }

    /// Open a store rooted at explicit paths (used by tests).
    pub fn open_at(profiles_dir: PathBuf, state_path: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&profiles_dir)?;
        Ok(Store {
            profiles_dir,
            state_path,
        })
    }

    fn profile_path(&self, slug: &str) -> PathBuf {
        self.profiles_dir.join(format!("{slug}.json"))
    }

    /// List all saved profiles, sorted by name (case-insensitive).
    pub fn list(&self) -> Result<Vec<Profile>> {
        let mut out = Vec::new();
        if self.profiles_dir.exists() {
            for entry in std::fs::read_dir(&self.profiles_dir)? {
                let path = entry?.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    match std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<Profile>(&s).ok())
                    {
                        Some(p) => out.push(p),
                        None => continue, // skip corrupt files rather than failing the whole list
                    }
                }
            }
        }
        out.sort_by_key(|p| p.name.to_lowercase());
        Ok(out)
    }

    pub fn exists(&self, slug: &str) -> bool {
        self.profile_path(slug).exists()
    }

    pub fn load(&self, slug: &str) -> Result<Profile> {
        let path = self.profile_path(slug);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading profile {}", path.display()))?;
        serde_json::from_str(&text).context("parsing profile JSON")
    }

    pub fn save(&self, profile: &Profile) -> Result<()> {
        let path = self.profile_path(&profile.slug());
        let text = serde_json::to_string_pretty(profile)?;
        std::fs::write(&path, text)
            .with_context(|| format!("writing profile {}", path.display()))?;
        Ok(())
    }

    pub fn delete(&self, slug: &str) -> Result<()> {
        let path = self.profile_path(slug);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        // If we deleted the active profile, clear the pointer.
        if self.active_slug()?.as_deref() == Some(slug) {
            self.write_state(&State { active: None })?;
        }
        Ok(())
    }

    /// Rename a profile, refusing to clobber a different existing profile
    /// whose slug matches the new name. Marks the renamed profile active.
    pub fn rename(&self, old_slug: &str, new_name: &str) -> Result<Profile> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(anyhow!("profile name cannot be empty"));
        }
        let mut p = self.load(old_slug)?;
        p.name = new_name.to_string();
        let new_slug = p.slug();
        if new_slug != old_slug && self.exists(&new_slug) {
            return Err(anyhow!("a profile named '{new_name}' already exists"));
        }
        self.save(&p)?;
        if new_slug != old_slug {
            let _ = std::fs::remove_file(self.profile_path(old_slug));
        }
        self.set_active(&new_slug)?;
        Ok(p)
    }

    pub fn active_slug(&self) -> Result<Option<String>> {
        Ok(self.read_state()?.active)
    }

    /// The active profile, falling back to the first profile if the pointer is
    /// unset or dangling.
    pub fn active(&self) -> Result<Option<Profile>> {
        if let Some(slug) = self.active_slug()? {
            if let Ok(p) = self.load(&slug) {
                return Ok(Some(p));
            }
        }
        Ok(self.list()?.into_iter().next())
    }

    pub fn set_active(&self, slug: &str) -> Result<()> {
        self.write_state(&State {
            active: Some(slug.to_string()),
        })
    }

    fn read_state(&self) -> Result<State> {
        if !self.state_path.exists() {
            return Ok(State::default());
        }
        let text = std::fs::read_to_string(&self.state_path)?;
        Ok(serde_json::from_str(&text).unwrap_or_default())
    }

    fn write_state(&self, state: &State) -> Result<()> {
        std::fs::write(&self.state_path, serde_json::to_string_pretty(state)?)?;
        Ok(())
    }

    /// First-run seeding: if no profiles exist, write the presets and mark
    /// "Balanced" active. Returns true if it seeded anything.
    pub fn ensure_defaults(&self) -> Result<bool> {
        if !self.list()?.is_empty() {
            return Ok(false);
        }
        for p in presets() {
            self.save(&p)?;
        }
        self.set_active("balanced")?;
        Ok(true)
    }
}

/// The four built-in presets. Slugs: competitive-fps, balanced, cinematic, potato.
pub fn presets() -> Vec<Profile> {
    vec![
        preset_competitive(),
        preset_balanced(),
        preset_cinematic(),
        preset_potato(),
    ]
}

fn preset_competitive() -> Profile {
    let mut p = Profile::new("Competitive FPS");
    p.description = "Max frames, minimum eye-candy. Uncapped FPS, low quality, no MSAA.".into();
    p.graphics_mode = Some(GraphicsMode::Performance);
    p.renderer = Some(Renderer::Vulkan);
    p.enable_gamemode = Some(true);
    p.fflags
        .insert("DFIntTaskSchedulerTargetFps".into(), json!(240));
    p.fflags
        .insert("DFIntDebugFRMQualityLevelOverride".into(), json!(3));
    p.fflags
        .insert("FIntDebugForceMSAASamples".into(), json!(0));
    p.fflags.insert("FFlagDisablePostFx".into(), json!(true));
    p
}

fn preset_balanced() -> Profile {
    let mut p = Profile::new("Balanced");
    p.description = "Sensible defaults with an uncapped framerate. A good starting point.".into();
    p.graphics_mode = Some(GraphicsMode::Balanced);
    p.renderer = Some(Renderer::Vulkan);
    p.enable_gamemode = Some(true);
    p.fflags
        .insert("DFIntTaskSchedulerTargetFps".into(), json!(144));
    p
}

fn preset_cinematic() -> Profile {
    let mut p = Profile::new("Cinematic");
    p.description = "Highest fidelity: quality mode, high render level, full effects.".into();
    p.graphics_mode = Some(GraphicsMode::Quality);
    p.renderer = Some(Renderer::Vulkan);
    p.enable_gamemode = Some(true);
    p.fflags
        .insert("DFIntTaskSchedulerTargetFps".into(), json!(120));
    p.fflags
        .insert("DFIntDebugFRMQualityLevelOverride".into(), json!(21));
    p
}

fn preset_potato() -> Profile {
    let mut p = Profile::new("Potato");
    p.description =
        "Rescue mode for very weak GPUs: voxel lighting, lowest textures, no shadows.".into();
    p.graphics_mode = Some(GraphicsMode::Performance);
    p.renderer = Some(Renderer::OpenGl);
    p.enable_gamemode = Some(true);
    p.fflags
        .insert("DFIntTaskSchedulerTargetFps".into(), json!(60));
    p.fflags
        .insert("DFIntDebugFRMQualityLevelOverride".into(), json!(1));
    p.fflags
        .insert("DFFlagDebugRenderForceTechnologyVoxel".into(), json!(true));
    p.fflags
        .insert("FIntRenderShadowIntensity".into(), json!(0));
    p.fflags
        .insert("DFFlagTextureQualityOverrideEnabled".into(), json!(true));
    p.fflags
        .insert("DFIntTextureQualityOverride".into(), json!(0));
    p.fflags
        .insert("FIntDebugForceMSAASamples".into(), json!(0));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store rooted in a unique temp dir so tests never touch the real
    /// ~/.config/chaser.
    fn temp_store(tag: &str) -> Store {
        let root = std::env::temp_dir().join(format!("chaser-store-test-{tag}"));
        let _ = std::fs::remove_dir_all(&root);
        Store::open_at(root.join("profiles"), root.join("state.json")).unwrap()
    }

    #[test]
    fn presets_have_unique_slugs() {
        let ps = presets();
        let mut slugs: Vec<String> = ps.iter().map(|p| p.slug()).collect();
        slugs.sort();
        let n = slugs.len();
        slugs.dedup();
        assert_eq!(n, slugs.len());
        assert!(ps.iter().any(|p| p.slug() == "balanced"));
    }

    #[test]
    fn competitive_uncaps_fps() {
        let c = preset_competitive();
        assert_eq!(
            c.fflags.get("DFIntTaskSchedulerTargetFps"),
            Some(&json!(240))
        );
    }

    #[test]
    fn save_load_delete_roundtrip() {
        let store = temp_store("roundtrip");
        let p = preset_balanced();
        store.save(&p).unwrap();
        assert!(store.exists("balanced"));
        let loaded = store.load("balanced").unwrap();
        assert_eq!(loaded.name, "Balanced");
        store.set_active("balanced").unwrap();
        assert_eq!(store.active_slug().unwrap().as_deref(), Some("balanced"));
        store.delete("balanced").unwrap();
        assert!(!store.exists("balanced"));
        // Deleting the active profile clears the pointer.
        assert_eq!(store.active_slug().unwrap(), None);
    }

    #[test]
    fn rename_moves_file_and_sets_active() {
        let store = temp_store("rename");
        store.save(&preset_potato()).unwrap();
        let renamed = store.rename("potato", "Ultra Potato").unwrap();
        assert_eq!(renamed.slug(), "ultra-potato");
        assert!(!store.exists("potato"));
        assert!(store.exists("ultra-potato"));
        assert_eq!(
            store.active_slug().unwrap().as_deref(),
            Some("ultra-potato")
        );
        // FFlags travelled with the rename.
        assert!(renamed
            .fflags
            .contains_key("DFFlagDebugRenderForceTechnologyVoxel"));
    }

    #[test]
    fn rename_refuses_to_clobber_existing_profile() {
        let store = temp_store("clobber");
        store.save(&preset_potato()).unwrap();
        store.save(&preset_balanced()).unwrap();
        let err = store.rename("potato", "Balanced").unwrap_err();
        assert!(err.to_string().contains("already exists"));
        // Both profiles still intact.
        assert!(store.exists("potato"));
        assert_eq!(store.load("balanced").unwrap().name, "Balanced");
    }

    #[test]
    fn rename_to_same_slug_is_fine() {
        let store = temp_store("samename");
        store.save(&preset_balanced()).unwrap();
        // Different display name, same slug — allowed.
        let renamed = store.rename("balanced", "BALANCED").unwrap();
        assert_eq!(renamed.slug(), "balanced");
        assert_eq!(store.load("balanced").unwrap().name, "BALANCED");
    }

    #[test]
    fn ensure_defaults_seeds_once() {
        let store = temp_store("defaults");
        assert!(store.ensure_defaults().unwrap());
        assert_eq!(store.list().unwrap().len(), 4);
        assert!(!store.ensure_defaults().unwrap()); // second call is a no-op
    }
}
