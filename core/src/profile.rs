//! Profile persistence (one JSON file per profile under Chaser's config dir)
//! and the built-in preset bundles.

use crate::models::{GraphicsMode, Profile, Renderer};
use crate::paths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// On-disk pointer to the currently active profile.
#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    active: Option<String>,
}

/// Manages the set of saved profiles and which one is active.
pub struct Store;

impl Store {
    /// Ensure Chaser's directories exist and return the manager.
    pub fn open() -> Result<Self> {
        std::fs::create_dir_all(paths::profiles_dir()?)?;
        paths::backup_dir()?; // side effect: create it too
        Ok(Store)
    }

    /// List all saved profiles, sorted by name (case-insensitive).
    pub fn list(&self) -> Result<Vec<Profile>> {
        let dir = paths::profiles_dir()?;
        let mut out = Vec::new();
        if dir.exists() {
            for entry in std::fs::read_dir(&dir)? {
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
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(out)
    }

    pub fn load(&self, slug: &str) -> Result<Profile> {
        let path = paths::profiles_dir()?.join(format!("{slug}.json"));
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading profile {}", path.display()))?;
        Ok(serde_json::from_str(&text).context("parsing profile JSON")?)
    }

    pub fn save(&self, profile: &Profile) -> Result<()> {
        let path = paths::profiles_dir()?.join(format!("{}.json", profile.slug()));
        let text = serde_json::to_string_pretty(profile)?;
        std::fs::write(&path, text).with_context(|| format!("writing profile {}", path.display()))?;
        Ok(())
    }

    pub fn delete(&self, slug: &str) -> Result<()> {
        let path = paths::profiles_dir()?.join(format!("{slug}.json"));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        // If we deleted the active profile, clear the pointer.
        if self.active_slug()?.as_deref() == Some(slug) {
            self.write_state(&State { active: None })?;
        }
        Ok(())
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
        let path = paths::state_path()?;
        if !path.exists() {
            return Ok(State::default());
        }
        let text = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&text).unwrap_or_default())
    }

    fn write_state(&self, state: &State) -> Result<()> {
        let path = paths::state_path()?;
        std::fs::write(&path, serde_json::to_string_pretty(state)?)?;
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
    vec![preset_competitive(), preset_balanced(), preset_cinematic(), preset_potato()]
}

fn preset_competitive() -> Profile {
    let mut p = Profile::new("Competitive FPS");
    p.description = "Max frames, minimum eye-candy. Uncapped FPS, low quality, no MSAA.".into();
    p.graphics_mode = Some(GraphicsMode::Performance);
    p.renderer = Some(Renderer::Vulkan);
    p.enable_gamemode = Some(true);
    p.fflags.insert("DFIntTaskSchedulerTargetFps".into(), json!(240));
    p.fflags.insert("DFIntDebugFRMQualityLevelOverride".into(), json!(3));
    p.fflags.insert("FIntDebugForceMSAASamples".into(), json!(0));
    p.fflags.insert("FFlagDisablePostFx".into(), json!(true));
    p
}

fn preset_balanced() -> Profile {
    let mut p = Profile::new("Balanced");
    p.description = "Sensible defaults with an uncapped framerate. A good starting point.".into();
    p.graphics_mode = Some(GraphicsMode::Balanced);
    p.renderer = Some(Renderer::Vulkan);
    p.enable_gamemode = Some(true);
    p.fflags.insert("DFIntTaskSchedulerTargetFps".into(), json!(144));
    p
}

fn preset_cinematic() -> Profile {
    let mut p = Profile::new("Cinematic");
    p.description = "Highest fidelity: quality mode, high render level, full effects.".into();
    p.graphics_mode = Some(GraphicsMode::Quality);
    p.renderer = Some(Renderer::Vulkan);
    p.enable_gamemode = Some(true);
    p.fflags.insert("DFIntTaskSchedulerTargetFps".into(), json!(120));
    p.fflags.insert("DFIntDebugFRMQualityLevelOverride".into(), json!(21));
    p
}

fn preset_potato() -> Profile {
    let mut p = Profile::new("Potato");
    p.description =
        "Rescue mode for very weak GPUs: voxel lighting, lowest textures, no shadows.".into();
    p.graphics_mode = Some(GraphicsMode::Performance);
    p.renderer = Some(Renderer::OpenGl);
    p.enable_gamemode = Some(true);
    p.fflags.insert("DFIntTaskSchedulerTargetFps".into(), json!(60));
    p.fflags.insert("DFIntDebugFRMQualityLevelOverride".into(), json!(1));
    p.fflags.insert("DFFlagDebugRenderForceTechnologyVoxel".into(), json!(true));
    p.fflags.insert("FIntRenderShadowIntensity".into(), json!(0));
    p.fflags.insert("DFFlagTextureQualityOverrideEnabled".into(), json!(true));
    p.fflags.insert("DFIntTextureQualityOverride".into(), json!(0));
    p.fflags.insert("FIntDebugForceMSAASamples".into(), json!(0));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
