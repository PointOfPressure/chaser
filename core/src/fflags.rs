//! The curated FastFlag catalog, built from Roblox's official allowlist.
//!
//! Since 2025-09-29 the client honours only the flags on Roblox's published
//! allowlist and silently ignores everything else, so the catalog *is* that
//! allowlist plus a clearly-marked legacy entry people still go looking for.
//!
//! Every entry here is checked by tests against [`crate::validate`], which
//! knows the real engine symbols. A fabricated flag cannot reach this list
//! without failing the build — three of them previously did.

use crate::models::{FFlagDef, FFlagKind, Risk};
use serde_json::json;

/// The built-in curated catalog.
pub fn catalog() -> Vec<FFlagDef> {
    vec![
        // ---- Grass: the cheapest real wins on weak GPUs ----
        FFlagDef {
            name: "FIntFRMMaxGrassDistance",
            kind: FFlagKind::Int,
            category: "Grass",
            risk: Risk::Safe,
            description:
                "How far out grass is drawn. 0 removes grass entirely, which is one of the \
                 largest allowlisted wins on integrated graphics. Set this at or above the \
                 minimum distance below.",
            suggested: json!(0),
            range: Some((0, 1000)),
        },
        FFlagDef {
            name: "FIntFRMMinGrassDistance",
            kind: FFlagKind::Int,
            category: "Grass",
            risk: Risk::Safe,
            description:
                "Distance at which grass starts fading in. Set to 0 alongside the maximum \
                 above to switch grass off cleanly.",
            suggested: json!(0),
            range: Some((0, 1000)),
        },
        FFlagDef {
            name: "FIntGrassMovementReducedMotionFactor",
            kind: FFlagKind::Int,
            category: "Grass",
            risk: Risk::Safe,
            description:
                "Damps grass sway. Higher means less movement; it is Roblox's reduced-motion \
                 accessibility control, so it also helps if animated grass is distracting. \
                 Irrelevant once grass distance is 0.",
            suggested: json!(100),
            range: Some((0, 100)),
        },
        // ---- Rendering ----
        FFlagDef {
            name: "DFIntDebugFRMQualityLevelOverride",
            kind: FFlagKind::Int,
            category: "Rendering",
            risk: Risk::Safe,
            description:
                "Pin the render quality level (roughly 1-21, mirroring the in-game graphics \
                 slider) instead of letting the engine adjust it. Lower is faster and, unlike \
                 the slider, it will not creep back up mid-game.",
            suggested: json!(10),
            range: Some((1, 21)),
        },
        FFlagDef {
            name: "DFFlagTextureQualityOverrideEnabled",
            kind: FFlagKind::Bool,
            category: "Rendering",
            risk: Risk::Safe,
            description:
                "Turn on manual texture-quality control. Does nothing on its own - pair it \
                 with the override level below.",
            suggested: json!(true),
            range: None,
        },
        FFlagDef {
            name: "DFIntTextureQualityOverride",
            kind: FFlagKind::Int,
            category: "Rendering",
            risk: Risk::Safe,
            description:
                "Texture quality from 0 (lowest) to 3 (highest). Only read when the override \
                 above is enabled. Lower saves VRAM, which matters most on shared-memory \
                 integrated GPUs.",
            suggested: json!(1),
            range: Some((0, 3)),
        },
        FFlagDef {
            name: "FIntDebugForceMSAASamples",
            kind: FFlagKind::Int,
            category: "Rendering",
            risk: Risk::Safe,
            description:
                "Force the anti-aliasing sample count. 0 disables MSAA for a straightforward \
                 frame-rate win; 1, 2 and 4 are the usual steps upward.",
            suggested: json!(0),
            range: Some((0, 4)),
        },
        FFlagDef {
            name: "FFlagDebugSkyGray",
            kind: FFlagKind::Bool,
            category: "Rendering",
            risk: Risk::Advanced,
            description:
                "Replace the skybox with flat grey. Skips skybox shading and removes a bright \
                 distraction, at the obvious cost of atmosphere.",
            suggested: json!(true),
            range: None,
        },
        FFlagDef {
            name: "DFFlagDebugPauseVoxelizer",
            kind: FFlagKind::Bool,
            category: "Rendering",
            risk: Risk::Risky,
            description:
                "Freeze the voxel lighting voxelizer. Large win in dynamically-lit places \
                 because lighting stops being recomputed - but lighting then stops updating, \
                 so moving lights and day/night cycles visibly break.",
            suggested: json!(true),
            range: None,
        },
        FFlagDef {
            name: "DFFlagDisableDPIScale",
            kind: FFlagKind::Bool,
            category: "Rendering",
            risk: Risk::Advanced,
            description:
                "Stop the client scaling for high-DPI displays. On a HiDPI panel this renders \
                 fewer pixels and gains performance, at the cost of a smaller, sharper-edged \
                 UI. Prefer Sober's own HiDPI toggle first.",
            suggested: json!(true),
            range: None,
        },
        FFlagDef {
            name: "FFlagDebugGraphicsPreferVulkan",
            kind: FFlagKind::Bool,
            category: "Rendering",
            risk: Risk::Advanced,
            description:
                "Ask the engine to prefer Vulkan. On Sober the renderer normally follows the \
                 profile's Renderer setting; reach for this only when that is not taking \
                 effect.",
            suggested: json!(true),
            range: None,
        },
        FFlagDef {
            name: "FFlagDebugGraphicsPreferOpenGL",
            kind: FFlagKind::Bool,
            category: "Rendering",
            risk: Risk::Advanced,
            description:
                "Ask the engine to prefer OpenGL/GLES. Sometimes more stable on older Intel \
                 and Mesa drivers, usually at a cost in frame rate.",
            suggested: json!(true),
            range: None,
        },
        // ---- Geometry ----
        FFlagDef {
            name: "DFIntCSGLevelOfDetailSwitchingDistance",
            kind: FFlagKind::Int,
            category: "Geometry",
            risk: Risk::Advanced,
            description:
                "Base distance controlling when solid-modelled (union/negate) parts drop to \
                 simpler geometry. Lower switches sooner, drawing less detail in the distance.",
            suggested: json!(100),
            range: Some((0, 10000)),
        },
        FFlagDef {
            name: "DFIntCSGLevelOfDetailSwitchingDistanceL12",
            kind: FFlagKind::Int,
            category: "Geometry",
            risk: Risk::Advanced,
            description: "Distance for the first CSG detail step (level 1 to 2). Lower is faster.",
            suggested: json!(100),
            range: Some((0, 10000)),
        },
        FFlagDef {
            name: "DFIntCSGLevelOfDetailSwitchingDistanceL23",
            kind: FFlagKind::Int,
            category: "Geometry",
            risk: Risk::Advanced,
            description: "Distance for the second CSG detail step (level 2 to 3). Lower is faster.",
            suggested: json!(200),
            range: Some((0, 10000)),
        },
        FFlagDef {
            name: "DFIntCSGLevelOfDetailSwitchingDistanceL34",
            kind: FFlagKind::Int,
            category: "Geometry",
            risk: Risk::Advanced,
            description: "Distance for the third CSG detail step (level 3 to 4). Lower is faster.",
            suggested: json!(400),
            range: Some((0, 10000)),
        },
        // ---- Allowlisted, but not useful under Sober ----
        FFlagDef {
            name: "FFlagDebugGraphicsPreferD3D11",
            kind: FFlagKind::Bool,
            category: "Windows only",
            risk: Risk::Advanced,
            description:
                "Prefer the Direct3D 11 renderer. Allowlisted by Roblox but meaningless under \
                 Sober, which runs the Android build against Vulkan or GLES. Listed for \
                 completeness.",
            suggested: json!(true),
            range: None,
        },
        FFlagDef {
            name: "FFlagHandleAltEnterFullscreenManually",
            kind: FFlagKind::Bool,
            category: "Windows only",
            risk: Risk::Advanced,
            description:
                "Changes how the Windows client handles Alt+Enter fullscreen. Allowlisted by \
                 Roblox, but absent from non-Windows builds. Listed for completeness.",
            suggested: json!(true),
            range: None,
        },
        // ---- Legacy ----
        FFlagDef {
            name: "DFIntTaskSchedulerTargetFps",
            kind: FFlagKind::Int,
            category: "Legacy (ignored)",
            risk: Risk::Risky,
            description:
                "The old frame-rate cap, and still the most-recommended flag online. It is a \
                 real engine symbol but is NOT on Roblox's allowlist, so the client ignores \
                 it. Kept here so Chaser can tell you that outright instead of letting you \
                 wonder why nothing changed. Use Sober's own frame-rate handling instead.",
            suggested: json!(240),
            range: Some((30, 1000)),
        },
    ]
}

/// Look up a catalog entry by flag name.
pub fn find(name: &str) -> Option<FFlagDef> {
    catalog().into_iter().find(|f| f.name == name)
}

/// All distinct categories, in first-seen order.
pub fn categories() -> Vec<&'static str> {
    let mut seen = Vec::new();
    for f in catalog() {
        if !seen.contains(&f.category) {
            seen.push(f.category);
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::{verdict, Verdict, ALLOWLIST};

    #[test]
    fn catalog_is_nonempty_and_unique() {
        let cat = catalog();
        assert!(!cat.is_empty());
        let mut names: Vec<&str> = cat.iter().map(|f| f.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate flag names in catalog");
    }

    /// The guard that matters: nothing fabricated can ever be listed again.
    #[test]
    fn no_catalog_entry_is_fabricated() {
        for f in catalog() {
            match verdict(f.name) {
                Verdict::Live | Verdict::Ignored => {}
                other => panic!(
                    "catalog lists {} which is not a real engine flag ({:?})",
                    f.name, other
                ),
            }
        }
    }

    /// Anything not on the allowlist must be filed under a category that says
    /// so, so the UI can never present an inert flag as a working one.
    #[test]
    fn inert_entries_are_labelled_as_such() {
        for f in catalog() {
            if verdict(f.name) == Verdict::Ignored {
                assert!(
                    f.category.contains("ignored"),
                    "{} is ignored by the client but filed under '{}'",
                    f.name,
                    f.category
                );
            }
        }
    }

    #[test]
    fn every_allowlisted_flag_is_offered() {
        let names: Vec<&str> = catalog().iter().map(|f| f.name).collect();
        for f in ALLOWLIST {
            assert!(names.contains(&f), "allowlisted flag {f} missing from catalog");
        }
    }

    #[test]
    fn integer_suggestions_sit_inside_their_range() {
        for f in catalog() {
            if let (Some((lo, hi)), Some(v)) = (f.range, f.suggested.as_i64()) {
                assert!(v >= lo && v <= hi, "{} suggests {v}, outside {lo}..={hi}", f.name);
            }
        }
    }

    #[test]
    fn grass_flag_present_and_safe() {
        let f = find("FIntFRMMaxGrassDistance").expect("grass distance should exist");
        assert_eq!(f.kind, FFlagKind::Int);
        assert_eq!(f.risk, Risk::Safe);
    }
}
