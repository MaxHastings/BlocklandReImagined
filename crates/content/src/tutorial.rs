//! Native tutorial pack index (`tutorial.json`): the Tutorial map's brick
//! layouts and its target practice schedule, converted from `Map_Tutorial`.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const PACK_SCHEMA: u32 = 1;
pub const PACK_INDEX: &str = "tutorial.json";

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
impl PackIndex {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == PACK_SCHEMA,
            "Unsupported tutorial pack schema {}",
            self.schema_version
        );
        for file in [&self.part1, &self.part2] {
            ensure!(
                file.ends_with(".world.json")
                    && !file.contains(['/', '\\', ':'])
                    && !file.starts_with('.'),
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
        Ok(())
    }
}
