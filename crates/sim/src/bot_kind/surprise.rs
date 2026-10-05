//! A kind's `surprise` (`docs/architecture/bots.md`, "Surprise"): one
//! dial for how far its bots stray from the single best choice and how
//! often they goof, and the weight of each goof. Every other number is
//! fixed in `session::bots::surprise`.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Goofs a kind may weigh, each an ordinary player action: look at a
/// player, strike an emote, hop, run a small circle, walk a short detour,
/// look round, crouch, spray paint toward a player, take out another tool
/// for a moment, drop the weapon in hand, or flick the light.
pub const FLAVOURS: [&str; 11] = [
    "stare", "emote", "hop", "circle", "detour", "look", "crouch", "spray", "tool", "drop", "light",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BotSurprise {
    /// 0 to 1: how much its choices vary and how often it goofs. 0 is the
    /// plain brain (the hold rule still holds its choices).
    pub strength: f32,
    /// Goof weights by name ([`FLAVOURS`]); an unnamed one weighs 1, 0
    /// never happens.
    pub flavours: BTreeMap<String, f32>,
}
impl Default for BotSurprise {
    fn default() -> Self {
        Self {
            strength: 0.5,
            flavours: BTreeMap::new(),
        }
    }
}
impl BotSurprise {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.strength.is_finite() && (0.0..=1.0).contains(&self.strength),
            "surprise strength is 0 to 1"
        );
        for (name, weight) in &self.flavours {
            ensure!(
                FLAVOURS.contains(&name.as_str()),
                "Unknown surprise flavour `{name}`; one of {}",
                FLAVOURS.join(", ")
            );
            ensure!(
                weight.is_finite() && (0.0..=100.0).contains(weight),
                "Surprise flavour `{name}` weight out of range"
            );
        }
        Ok(())
    }
    /// The weight of goof `name`.
    pub fn flavour_weight(&self, name: &str) -> f32 {
        self.flavours.get(name).copied().unwrap_or(1.0)
    }
}
