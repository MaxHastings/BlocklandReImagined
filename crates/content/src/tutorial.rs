//! Native tutorial pack index (`tutorial.json`): the Tutorial map's brick
//! layouts, its target practice schedule and the target models, converted
//! from `Map_Tutorial`.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Schema 2 adds the target models and their textures.
pub const PACK_SCHEMA: u32 = 2;
pub const PACK_INDEX: &str = "tutorial.json";

/// `TutorialTarget`, `TutorialTargetM` and the `...Hit` datablocks
/// `ProjectileData::onCollision` swaps in (`tutorial.cs`, "Target
/// Shooting"): each names one of these shapes.
pub const TARGET_SHAPE: &str = "v20/add-ons/map_tutorial/target.dts";
pub const TARGET_HIT_SHAPE: &str = "v20/add-ons/map_tutorial/targethit.dts";
pub const TARGET_M_SHAPE: &str = "v20/add-ons/map_tutorial/targetm.dts";
pub const TARGET_M_HIT_SHAPE: &str = "v20/add-ons/map_tutorial/targetmhit.dts";
pub const TARGET_SHAPES: [&str; 4] = [
    TARGET_SHAPE,
    TARGET_HIT_SHAPE,
    TARGET_M_SHAPE,
    TARGET_M_HIT_SHAPE,
];
/// `setSkinName` replaces the `base.` prefix of a skinnable material:
/// `launchTarget` gives the marked target these skins.
pub const TARGET_SKINS: [&str; 4] = ["base", "m1", "m2", "m3"];
const MAX_TEXTURES: usize = 64;

/// `tutorial.json` in the tutorial pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackIndex {
    pub schema_version: u32,
    /// Native world loaded whenever the tutorial starts (`Tutorial_Part1.bls`).
    pub part1: String,
    /// Native world that replaces it at the Light room (`Tutorial_Part2.bls`).
    pub part2: String,
    /// `targetSetup.txt` resolved to launch times.
    pub targets: Vec<TargetLaunch>,
    /// When the schedule reaches the end of `targetSetup.txt`.
    pub targets_end_ms: u32,
    /// Native shape file of each of [`TARGET_SHAPES`], by asset id.
    pub shapes: BTreeMap<String, String>,
    /// The target shapes' textures and skins, by lower-case material file
    /// name as the shapes name them (`gray50.png`, `m1.target.png`).
    pub textures: BTreeMap<String, String>,
}
/// One `launchTarget(row, speed, type)` call of the target practice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetLaunch {
    pub at_ms: u32,
    /// Lane 1-3.
    pub row: u8,
    /// Scroll speed 1-5.
    pub speed: u8,
    /// Original target type word: empty for the plain target, `m`/`m1`-`m4`
    /// for the marked target and its skins.
    pub kind: String,
}
impl TargetLaunch {
    /// `launchTarget`: a type containing `m` is `TutorialTargetM`, and `m1`
    /// to `m4` pick the skins `base`, `m1`, `m2` and `m3`. Returns whether
    /// the target is marked and its skin (`base` when none is set).
    pub fn look(&self) -> (bool, &'static str) {
        let marked = self.kind.contains('m');
        let skin = match self.kind.as_str() {
            "m4" => "m3",
            "m3" => "m2",
            "m2" => "m1",
            _ => "base",
        };
        (marked, skin)
    }
}

fn plain_file(file: &str, suffix: &[&str]) -> bool {
    suffix.iter().any(|s| file.ends_with(s))
        && !file.contains(['/', '\\', ':'])
        && !file.starts_with('.')
        && file.len() <= 128
}

impl PackIndex {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == PACK_SCHEMA,
            "Unsupported tutorial pack schema {}",
            self.schema_version
        );
        for file in [&self.part1, &self.part2] {
            ensure!(
                plain_file(file, &[".world.json"]),
                "Invalid tutorial world filename"
            );
        }
        ensure!(self.targets.len() <= 1024, "Too many tutorial targets");
        let mut last = 0;
        for target in &self.targets {
            ensure!(
                (1..=3).contains(&target.row)
                    && (1..=5).contains(&target.speed)
                    && ["", "m", "m1", "m2", "m3", "m4"].contains(&target.kind.as_str())
                    && target.at_ms >= last
                    && target.at_ms <= self.targets_end_ms,
                "Invalid tutorial target launch"
            );
            last = target.at_ms;
        }
        ensure!(
            self.targets_end_ms <= 3_600_000,
            "Tutorial target schedule too long"
        );
        ensure!(
            self.shapes.len() == TARGET_SHAPES.len()
                && TARGET_SHAPES.iter().all(|id| self.shapes.contains_key(*id))
                && self
                    .shapes
                    .values()
                    .all(|f| plain_file(f, &[".shape.json"])),
            "The tutorial pack needs exactly the four target shapes"
        );
        ensure!(
            self.textures.len() <= MAX_TEXTURES
                && self.textures.iter().all(|(name, file)| {
                    name.len() <= 64
                        && *name == name.to_ascii_lowercase()
                        && !name.contains(['/', '\\', ':'])
                        && plain_file(file, &[".png", ".jpg"])
                }),
            "Invalid tutorial target textures"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_types_pick_the_datablock_and_skin_like_launch_target() {
        let look = |kind: &str| {
            TargetLaunch {
                at_ms: 0,
                row: 1,
                speed: 1,
                kind: kind.into(),
            }
            .look()
        };
        assert_eq!(look(""), (false, "base"));
        assert_eq!(look("m"), (true, "base"));
        assert_eq!(look("m1"), (true, "base"));
        assert_eq!(look("m2"), (true, "m1"));
        assert_eq!(look("m4"), (true, "m3"));
    }
}
