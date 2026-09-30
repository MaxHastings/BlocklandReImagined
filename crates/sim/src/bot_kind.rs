//! Bot kinds Add-Ons provide (`assets/bots.json`).
//!
//! v20 has no brain for the player objects a Vehicle Spawn brick makes:
//! they stand where they spawn until ridden. Bots that walk, find their way
//! and fight are the engine's bot mechanism; which ones exist, what the
//! spawn list calls them and how they play is each Add-On's data. The base
//! game provides none.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
/// Bot kinds one server knows, over every Add-On.
pub const MAX_KINDS: usize = 64;

/// One bot kind: its spawn list entry and how its brain plays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BotKind {
    /// Stable id a spawn brick stores.
    pub id: String,
    /// What the Vehicle Spawn list shows, and the bot's player name.
    pub name: String,
    /// How far, in world units, it sees other players.
    pub sight: f32,
    /// How far from its spawn brick it strolls when nothing is going on.
    pub wander_radius: f32,
    /// How far from its spawn brick it follows a fight before heading back.
    pub chase_radius: f32,
    /// Seconds from first seeing an enemy to its first shot.
    pub reaction_seconds: f32,
    /// Degrees a second its aim turns.
    pub turn_degrees: f32,
    /// Aim error in degrees when a fight starts; it narrows to a third of
    /// this while the bot keeps its enemy in sight.
    pub aim_error_degrees: f32,
    /// Seconds it remembers where it last saw, or was hurt by, an enemy.
    pub memory_seconds: f32,
    /// Whether it fights bots on the other side as well as players.
    pub fights_bots: bool,
}
impl Default for BotKind {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            sight: 80.0,
            wander_radius: 12.0,
            chase_radius: 48.0,
            reaction_seconds: 0.35,
            turn_degrees: 300.0,
            aim_error_degrees: 5.0,
            memory_seconds: 8.0,
            fights_bots: true,
        }
    }
}
impl BotKind {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.trim().is_empty()
                && self.id.len() <= 96
                && self
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._:/-".contains(c)),
            "Bot id `{}` must be 1-96 letters, digits or ._:/-",
            self.id
        );
        ensure!(
            !self.name.trim().is_empty()
                && self.name.chars().count() <= 32
                && !self.name.chars().any(char::is_control),
            "Bot `{}` needs a name of 1-32 characters",
            self.id
        );
        let ranges = [
            ("sight", self.sight, 1.0, 400.0),
            ("wander_radius", self.wander_radius, 0.0, 64.0),
            ("chase_radius", self.chase_radius, 0.0, 256.0),
            ("reaction_seconds", self.reaction_seconds, 0.0, 5.0),
            ("turn_degrees", self.turn_degrees, 10.0, 3600.0),
            ("aim_error_degrees", self.aim_error_degrees, 0.0, 45.0),
            ("memory_seconds", self.memory_seconds, 0.0, 60.0),
        ];
        for (name, value, min, max) in ranges {
            ensure!(
                value.is_finite() && (min..=max).contains(&value),
                "Bot `{}`: {name} must be {min} to {max}",
                self.id
            );
        }
        Ok(())
    }
}

/// A `bots.json` file.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BotPack {
    pub schema_version: u32,
    pub bots: Vec<BotKind>,
}
impl BotPack {
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        let pack: Self = serde_json::from_slice(bytes).context("bots.json")?;
        ensure!(
            pack.schema_version == SCHEMA_VERSION,
            "bots.json schema_version must be {SCHEMA_VERSION}"
        );
        ensure!(pack.bots.len() <= MAX_KINDS, "Too many bot kinds");
        for bot in &pack.bots {
            bot.validate()?;
        }
        Ok(pack)
    }
    /// Every provider's kinds in order; a later id replaces an earlier one.
    pub fn merge(packs: impl IntoIterator<Item = BotPack>) -> Result<Vec<BotKind>> {
        let mut out: Vec<BotKind> = Vec::new();
        for pack in packs {
            for bot in pack.bots {
                match out.iter_mut().find(|b| b.id == bot.id) {
                    Some(slot) => *slot = bot,
                    None => out.push(bot),
                }
            }
        }
        ensure!(out.len() <= MAX_KINDS, "Too many bot kinds");
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packs_validate_and_later_ids_replace_earlier_ones() {
        let a = BotPack::from_json(br#"{"schema_version":1,"bots":[{"id":"bot.a","name":"A"}]}"#)
            .unwrap();
        let b = BotPack::from_json(
            br#"{"schema_version":1,"bots":[{"id":"bot.a","name":"A2","sight":20},{"id":"bot.b","name":"B"}]}"#,
        )
        .unwrap();
        let merged = BotPack::merge([a, b]).unwrap();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].name, "A2");
        assert_eq!(merged[0].sight, 20.0);
        assert!(
            BotPack::from_json(br#"{"schema_version":1,"bots":[{"id":"x","name":""}]}"#).is_err()
        );
        assert!(
            BotPack::from_json(
                br#"{"schema_version":1,"bots":[{"id":"x","name":"X","sight":-1}]}"#
            )
            .is_err()
        );
        assert!(
            BotPack::from_json(br#"{"schema_version":1,"bots":[{"id":"x","name":"X","speed":2}]}"#)
                .is_err()
        );
    }
}
