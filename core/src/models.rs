//! Shared data types: graphics modes, renderer, FastFlag catalog entries, and Profiles.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Sober's `graphics_optimization_mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphicsMode {
    Quality,
    Balanced,
    Performance,
}

impl GraphicsMode {
    pub fn as_key(self) -> &'static str {
        match self {
            GraphicsMode::Quality => "quality",
            GraphicsMode::Balanced => "balanced",
            GraphicsMode::Performance => "performance",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        match s {
            "quality" => Some(GraphicsMode::Quality),
            "balanced" => Some(GraphicsMode::Balanced),
            "performance" => Some(GraphicsMode::Performance),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GraphicsMode::Quality => "Quality",
            GraphicsMode::Balanced => "Balanced",
            GraphicsMode::Performance => "Performance",
        }
    }

    pub const ALL: [GraphicsMode; 3] = [
        GraphicsMode::Quality,
        GraphicsMode::Balanced,
        GraphicsMode::Performance,
    ];
}

/// Renderer choice, mapped onto Sober's `use_opengl` boolean.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Renderer {
    Vulkan,
    OpenGl,
}

impl Renderer {
    /// `use_opengl` value for this renderer.
    pub fn use_opengl(self) -> bool {
        matches!(self, Renderer::OpenGl)
    }

    pub fn from_use_opengl(b: bool) -> Self {
        if b {
            Renderer::OpenGl
        } else {
            Renderer::Vulkan
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Renderer::Vulkan => "Vulkan",
            Renderer::OpenGl => "OpenGL",
        }
    }
}

/// The value type a FastFlag expects. `Flag` is a boolean flag using the
/// `FFlag`/`DFFlag` naming convention; `Bool` is a plain boolean.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FFlagKind {
    Bool,
    Int,
    String,
}

/// How confident we are that a flag is real and safe to expose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    /// Well-known, widely used, unlikely to break anything.
    Safe,
    /// Real flag, but changes engine behaviour in ways some games dislike.
    Advanced,
    /// Can break rendering or get you odd behaviour; power users only.
    Risky,
}

impl Risk {
    pub fn label(self) -> &'static str {
        match self {
            Risk::Safe => "Safe",
            Risk::Advanced => "Advanced",
            Risk::Risky => "Risky",
        }
    }
}

/// A single curated FastFlag definition (catalog metadata, not a live value).
/// In-memory only — never serialised — so it can borrow `'static` strings.
#[derive(Clone, Debug)]
pub struct FFlagDef {
    pub name: &'static str,
    pub kind: FFlagKind,
    pub category: &'static str,
    pub risk: Risk,
    pub description: &'static str,
    /// A sensible value to seed the editor with when the user enables it.
    pub suggested: Value,
    /// Optional inclusive range hint for integer flags (min, max).
    pub range: Option<(i64, i64)>,
}

/// A named, switchable configuration bundle. Owns both the subset of Sober
/// config keys we manage and Chaser-side launch options that never touch
/// Sober's config file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub description: String,

    // --- Managed Sober config keys (None = leave whatever Sober has) ---
    #[serde(default)]
    pub graphics_mode: Option<GraphicsMode>,
    #[serde(default)]
    pub renderer: Option<Renderer>,
    #[serde(default)]
    pub enable_gamemode: Option<bool>,
    #[serde(default)]
    pub discord_rpc: Option<bool>,
    #[serde(default)]
    pub enable_hidpi: Option<bool>,
    #[serde(default)]
    pub allow_gamepad: Option<bool>,
    /// FastFlags this profile applies. Replaces Sober's `fflags` object on apply.
    #[serde(default)]
    pub fflags: Map<String, Value>,

    // --- Chaser-side launch options (never written to Sober config) ---
    /// Extra environment variables passed via `flatpak run --env=...`.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Overlay the MangoHud HUD (requires the MangoHud Flatpak extension).
    #[serde(default)]
    pub mangohud: bool,
}

impl Profile {
    pub fn new(name: impl Into<String>) -> Self {
        Profile {
            name: name.into(),
            description: String::new(),
            graphics_mode: None,
            renderer: None,
            enable_gamemode: None,
            discord_rpc: None,
            enable_hidpi: None,
            allow_gamepad: None,
            fflags: Map::new(),
            env: BTreeMap::new(),
            mangohud: false,
        }
    }

    /// A filesystem-safe slug derived from the profile name.
    pub fn slug(&self) -> String {
        slugify(&self.name)
    }
}

/// Lowercase, ascii-alnum + dashes; collapses runs and trims. Never empty.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "profile".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Competitive FPS"), "competitive-fps");
        assert_eq!(slugify("  Potato!! mode  "), "potato-mode");
        assert_eq!(slugify("///"), "profile");
        assert_eq!(slugify("Cinematic (4K)"), "cinematic-4k");
    }

    #[test]
    fn graphics_mode_roundtrip() {
        for m in GraphicsMode::ALL {
            assert_eq!(GraphicsMode::from_key(m.as_key()), Some(m));
        }
        assert_eq!(GraphicsMode::from_key("nonsense"), None);
    }

    #[test]
    fn renderer_maps_to_use_opengl() {
        assert!(Renderer::OpenGl.use_opengl());
        assert!(!Renderer::Vulkan.use_opengl());
        assert_eq!(Renderer::from_use_opengl(true), Renderer::OpenGl);
    }
}
