//! Does a FastFlag actually do anything?
//!
//! Two independent facts decide that, and Chaser checks both:
//!
//! 1. **Is the flag real?** Guides and videos are full of invented flag names.
//!    We ship two independent symbol dumps and check against both:
//!    `data/flags.txt` (`PREFIX:Name`) from an unstripped Roblox Studio build,
//!    which is the only source that knows a flag's *prefix*; and
//!    `data/player-flags.txt` (bare names) from a current Player-client offset
//!    dump, which covers the ~1500 flags newer than that Studio build. A typo
//!    or fabrication is caught before it ever reaches Sober.
//! 2. **Is the flag allowlisted?** Since 2025-09-29 the client only honours
//!    flags on Roblox's published allowlist; everything else is silently
//!    ignored. A real flag can still be completely inert.
//!
//! The allowlist wins over the extracted set: the dump is from a macOS Studio
//! binary, so platform-specific entries (Windows' Alt+Enter handling) are
//! legitimately absent and must never be reported as fabricated.

use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashSet};
use std::sync::OnceLock;

/// Roblox's official allowlist for local client configuration via Fast Flags.
///
/// Source: <https://devforum.roblox.com/t/allowlist-for-local-client-configuration-via-fast-flags/3966569>
/// (announced 2025-09-29). Flags outside this list "will simply be ignored and
/// will not have an effect".
pub const ALLOWLIST: [&str; 18] = [
    // Geometry
    "DFIntCSGLevelOfDetailSwitchingDistance",
    "DFIntCSGLevelOfDetailSwitchingDistanceL12",
    "DFIntCSGLevelOfDetailSwitchingDistanceL23",
    "DFIntCSGLevelOfDetailSwitchingDistanceL34",
    // Rendering
    "FFlagHandleAltEnterFullscreenManually",
    "DFFlagTextureQualityOverrideEnabled",
    "DFIntTextureQualityOverride",
    "FIntDebugForceMSAASamples",
    "DFFlagDisableDPIScale",
    "FFlagDebugGraphicsPreferD3D11",
    "FFlagDebugSkyGray",
    "DFFlagDebugPauseVoxelizer",
    "DFIntDebugFRMQualityLevelOverride",
    "FIntFRMMaxGrassDistance",
    "FIntFRMMinGrassDistance",
    "FFlagDebugGraphicsPreferVulkan",
    "FFlagDebugGraphicsPreferOpenGL",
    // User Interface
    "FIntGrassMovementReducedMotionFactor",
];

/// What the Roblox client will actually do with a flag override.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Real and allowlisted. The client honours it.
    Live,
    /// A real engine flag, but not allowlisted, so the client ignores it.
    Ignored,
    /// The bare name exists, but only under other prefixes. Almost always a
    /// `FFlag`/`DFFlag` mix-up. Carries the prefixes that do exist.
    WrongPrefix { found: Vec<String> },
    /// No such symbol in the engine under any prefix. Fabricated or misspelt.
    Unknown,
}

impl Verdict {
    pub fn label(&self) -> &'static str {
        match self {
            Verdict::Live => "Live",
            Verdict::Ignored => "Ignored",
            Verdict::WrongPrefix { .. } => "Wrong prefix",
            Verdict::Unknown => "Not a real flag",
        }
    }

    /// True when the flag has no effect in-game, for any reason.
    pub fn is_inert(&self) -> bool {
        !matches!(self, Verdict::Live)
    }
}

fn ground_truth() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| include_str!("../data/flags.txt").lines().collect())
}

/// Bare flag names from a current Player-client dump. No prefixes, so this can
/// only answer "does this symbol exist", never "under which prefix".
fn player_bare() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| include_str!("../data/player-flags.txt").lines().collect())
}

/// Every prefix seen in the dump, longest first so matching is unambiguous.
fn prefixes() -> &'static Vec<&'static str> {
    static PREFIXES: OnceLock<Vec<&'static str>> = OnceLock::new();
    PREFIXES.get_or_init(|| {
        let mut set: BTreeSet<&'static str> = ground_truth()
            .iter()
            .filter_map(|l| l.split_once(':').map(|(p, _)| p))
            .collect();
        // Sober's own placeholder is not an engine symbol; never infer from it.
        set.remove("");
        let mut v: Vec<&'static str> = set.into_iter().collect();
        v.sort_by_key(|p| std::cmp::Reverse(p.len()));
        v
    })
}

/// Split `DFIntTextureQualityOverride` into `("DFInt", "TextureQualityOverride")`.
fn split_flag(flag: &str) -> Option<(&'static str, &str)> {
    prefixes()
        .iter()
        .find(|p| flag.starts_with(**p) && flag.len() > p.len())
        .map(|p| (*p, &flag[p.len()..]))
}

/// Is this exact flag on Roblox's allowlist?
pub fn is_allowlisted(flag: &str) -> bool {
    ALLOWLIST.contains(&flag)
}

/// Does this exact flag exist in the engine symbol dump?
pub fn is_real(flag: &str) -> bool {
    match split_flag(flag) {
        Some((prefix, bare)) => ground_truth().contains(format!("{prefix}:{bare}").as_str()),
        None => false,
    }
}

/// What the client will do with this flag.
pub fn verdict(flag: &str) -> Verdict {
    if is_allowlisted(flag) {
        return Verdict::Live;
    }
    if is_real(flag) {
        return Verdict::Ignored;
    }
    if let Some((_, bare)) = split_flag(flag) {
        // Checked before the Player fallback: a DFFlag/FFlag mix-up must be
        // reported as such, not quietly waved through as a real flag.
        let found: Vec<String> = prefixes()
            .iter()
            .filter(|p| ground_truth().contains(format!("{p}:{bare}").as_str()))
            .map(|p| p.to_string())
            .collect();
        if !found.is_empty() {
            return Verdict::WrongPrefix { found };
        }
        // Newer than our Studio dump, but shipping in the current Player
        // client. Real, and still not allowlisted.
        if player_bare().contains(bare) {
            return Verdict::Ignored;
        }
    }
    Verdict::Unknown
}

/// Audit a profile's (or Sober's) `fflags` object. Returns one verdict per
/// flag, in the map's own order.
pub fn audit(fflags: &Map<String, Value>) -> Vec<(String, Verdict)> {
    fflags
        .keys()
        .map(|name| (name.clone(), verdict(name)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn allowlisted_flag_is_live() {
        assert_eq!(verdict("DFIntTextureQualityOverride"), Verdict::Live);
        assert_eq!(verdict("FIntFRMMaxGrassDistance"), Verdict::Live);
    }

    #[test]
    fn real_but_not_allowlisted_is_ignored() {
        // Chaser's former headline flag: a genuine engine symbol the client
        // has ignored since the allowlist landed.
        assert!(is_real("DFIntTaskSchedulerTargetFps"));
        assert_eq!(verdict("DFIntTaskSchedulerTargetFps"), Verdict::Ignored);
    }

    #[test]
    fn fabricated_flags_are_unknown() {
        // All three shipped in Chaser's own catalog and exist in no build.
        for fake in [
            "FFlagDisablePostFx",
            "FIntRenderShadowIntensity",
            "DFFlagDebugRenderForceTechnologyVoxel",
        ] {
            assert_eq!(verdict(fake), Verdict::Unknown, "{fake} should be unknown");
        }
    }

    #[test]
    fn windows_only_allowlist_entry_is_not_called_fabricated() {
        // Absent from the macOS dump; the allowlist must still win.
        assert!(!is_real("FFlagHandleAltEnterFullscreenManually"));
        assert_eq!(verdict("FFlagHandleAltEnterFullscreenManually"), Verdict::Live);
    }

    #[test]
    fn wrong_prefix_suggests_the_real_one() {
        match verdict("FFlagTextureQualityOverrideEnabled") {
            Verdict::WrongPrefix { found } => assert!(found.contains(&"DFFlag".to_string())),
            other => panic!("expected WrongPrefix, got {other:?}"),
        }
    }

    #[test]
    fn every_allowlist_entry_is_live() {
        for f in ALLOWLIST {
            assert_eq!(verdict(f), Verdict::Live, "{f}");
        }
    }

    #[test]
    fn audit_reports_one_verdict_per_flag() {
        let mut m = Map::new();
        m.insert("DFIntTextureQualityOverride".into(), json!(0));
        m.insert("FFlagDisablePostFx".into(), json!(true));
        let out = audit(&m);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].1, Verdict::Live);
        assert_eq!(out[1].1, Verdict::Unknown);
        assert!(out[1].1.is_inert());
    }

    #[test]
    fn garbage_is_unknown_not_a_panic() {
        assert_eq!(verdict(""), Verdict::Unknown);
        assert_eq!(verdict("NotAFlagAtAll"), Verdict::Unknown);
        assert_eq!(verdict("FFlag"), Verdict::Unknown);
    }

    #[test]
    fn flag_newer_than_the_studio_dump_is_not_called_fabricated() {
        // Ships in the current Player client; absent from Studio 0.735.19.
        assert!(!is_real("FFlagAESAddCreatorIdToSearch3"));
        assert_eq!(verdict("FFlagAESAddCreatorIdToSearch3"), Verdict::Ignored);
    }

    #[test]
    fn wrong_prefix_beats_the_player_fallback() {
        // Bare name is in both dumps; the mix-up must still be reported.
        match verdict("FFlagTextureQualityOverrideEnabled") {
            Verdict::WrongPrefix { .. } => {}
            other => panic!("expected WrongPrefix, got {other:?}"),
        }
    }

    #[test]
    fn player_dump_loaded() {
        assert!(player_bare().len() > 14_000, "player dump looks truncated");
    }

    #[test]
    fn ground_truth_loaded() {
        assert!(ground_truth().len() > 15_000, "flag dump looks truncated");
    }
}
