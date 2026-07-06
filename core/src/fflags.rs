//! A small, deliberately conservative catalog of real FastFlags.
//!
//! VinegarHQ explicitly does not support FastFlag usage and warns that many
//! flags floating around guides are fabricated. So we ship only flags we're
//! confident are real engine flags, tag each with a risk level, and make it
//! trivial for power users to add their own via the raw editor.

use crate::models::{FFlagDef, FFlagKind, Risk};
use serde_json::json;

/// The built-in curated catalog.
pub fn catalog() -> Vec<FFlagDef> {
    vec![
        FFlagDef {
            name: "DFIntTaskSchedulerTargetFps",
            kind: FFlagKind::Int,
            category: "Frame Rate",
            risk: Risk::Safe,
            description:
                "Target/maximum framerate. Set to your monitor's refresh rate, or high (e.g. 240) \
                 to effectively uncap FPS. The single most useful flag on Linux.",
            suggested: json!(240),
            range: Some((30, 1000)),
        },
        FFlagDef {
            name: "FFlagDebugGraphicsPreferVulkan",
            kind: FFlagKind::Bool,
            category: "Renderer",
            risk: Risk::Advanced,
            description:
                "Ask the engine to prefer the Vulkan renderer. On Sober the renderer is normally \
                 chosen by the 'use_opengl' setting; use this only if you know you need it.",
            suggested: json!(true),
            range: None,
        },
        FFlagDef {
            name: "FFlagDebugGraphicsPreferOpenGL",
            kind: FFlagKind::Bool,
            category: "Renderer",
            risk: Risk::Advanced,
            description:
                "Ask the engine to prefer the OpenGL/GLES renderer. Sometimes more stable on older \
                 Intel/Mesa GPUs at the cost of performance.",
            suggested: json!(true),
            range: None,
        },
        FFlagDef {
            name: "DFIntDebugFRMQualityLevelOverride",
            kind: FFlagKind::Int,
            category: "Graphics",
            risk: Risk::Advanced,
            description:
                "Force the render quality level (roughly 1-21, mirroring the in-game graphics \
                 slider). Lower = faster. Overrides automatic quality.",
            suggested: json!(10),
            range: Some((1, 21)),
        },
        FFlagDef {
            name: "FIntDebugForceMSAASamples",
            kind: FFlagKind::Int,
            category: "Graphics",
            risk: Risk::Advanced,
            description:
                "Force anti-aliasing sample count (0 = off, or 1/2/4). 0 gives a small FPS win on \
                 weak GPUs.",
            suggested: json!(0),
            range: Some((0, 4)),
        },
        FFlagDef {
            name: "DFFlagTextureQualityOverrideEnabled",
            kind: FFlagKind::Bool,
            category: "Graphics",
            risk: Risk::Advanced,
            description:
                "Enable manual texture-quality override. Pair with DFIntTextureQualityOverride.",
            suggested: json!(true),
            range: None,
        },
        FFlagDef {
            name: "DFIntTextureQualityOverride",
            kind: FFlagKind::Int,
            category: "Graphics",
            risk: Risk::Advanced,
            description:
                "Texture quality level 0-3 (0 = lowest, 3 = highest). Only used when the override \
                 above is enabled. Lower saves VRAM and helps weak GPUs.",
            suggested: json!(1),
            range: Some((0, 3)),
        },
        FFlagDef {
            name: "FFlagDisablePostFx",
            kind: FFlagKind::Bool,
            category: "Graphics",
            risk: Risk::Advanced,
            description:
                "Disable post-processing effects (bloom, blur, colour correction). Cleaner, \
                 faster, but games lose intended atmosphere.",
            suggested: json!(true),
            range: None,
        },
        FFlagDef {
            name: "DFFlagDebugRenderForceTechnologyVoxel",
            kind: FFlagKind::Bool,
            category: "Graphics",
            risk: Risk::Risky,
            description:
                "Force the older Voxel lighting technology instead of Future/ShadowMap lighting. \
                 Big FPS gains on weak GPUs, but lighting looks flat and some games look wrong.",
            suggested: json!(true),
            range: None,
        },
        FFlagDef {
            name: "FIntRenderShadowIntensity",
            kind: FFlagKind::Int,
            category: "Graphics",
            risk: Risk::Advanced,
            description:
                "Global shadow intensity (0-100). 0 effectively removes dynamic shadows for a \
                 performance win.",
            suggested: json!(0),
            range: Some((0, 100)),
        },
        FFlagDef {
            name: "FLogNetwork",
            kind: FFlagKind::Int,
            category: "Debug",
            risk: Risk::Risky,
            description:
                "Network log verbosity (0-7). Diagnostic only; leave alone unless you're chasing \
                 a connection bug.",
            suggested: json!(7),
            range: Some((0, 7)),
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

    #[test]
    fn fps_flag_present() {
        let f = find("DFIntTaskSchedulerTargetFps").expect("FPS flag should exist");
        assert_eq!(f.kind, FFlagKind::Int);
        assert_eq!(f.risk, Risk::Safe);
    }
}
