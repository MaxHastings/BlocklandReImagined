//! A kind's `surprise` tunables (`docs/architecture/bots.md`, "Surprise"):
//! how far its bots stray from the single best choice, how their
//! preferences drift, tire and adapt, how long a pick is held, and how
//! often they do something idle at a pause. `strength` 0 is the brain
//! without any of it: every choice is the plain best, as before.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Flavour interrupts a kind may weigh, each an ordinary player action:
/// look at a player, strike an emote, hop, run a small circle, walk a
/// short detour, look round, crouch, spray paint toward a player, take
/// out another tool for a moment, drop the weapon in hand, or flick the
/// light.
pub const INTERRUPTS: [&str; 11] = [
    "stare", "emote", "hop", "circle", "detour", "look", "crouch", "spray", "tool", "drop", "light",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BotSurprise {
    /// 0 to 1: how much of everything below applies. 0 is the plain brain.
    pub strength: f32,
    /// Options scoring within this share of the best are eligible.
    pub band: f32,
    /// How far a drive's weight drifts either way (a factor of e^drift).
    pub drift: f32,
    /// About how many seconds a drift takes to cross its range.
    pub drift_seconds: f32,
    /// Weight an option loses per second in use (boredom).
    pub boredom: f32,
    /// Seconds for boredom to halve once an option is out of use.
    pub boredom_seconds: f32,
    /// Share of effectiveness one failed outcome takes away.
    pub failure: f32,
    /// Share of the gap to full effectiveness one success gives back.
    pub success: f32,
    /// Seconds for lost effectiveness to halve on its own.
    pub effectiveness_seconds: f32,
    /// A pick is held at least this long (randomly up to half again).
    pub commit_seconds: f32,
    /// The pause before a switch the variation causes.
    pub tell_seconds: f32,
    /// Under this share of its health a bot is urgent: no variation.
    pub urgent_health: f32,
    /// Hurt by an enemy this close...
    pub urgent_range: f32,
    /// ...this recently, a bot is urgent: no variation.
    pub urgent_seconds: f32,
    /// How far to the side a flanking chase aims.
    pub flank_distance: f32,
    /// Flavour interrupts a minute at natural pauses.
    pub interrupts_per_minute: f32,
    /// No interrupt until this long after the last one ended.
    pub interrupt_cooldown_seconds: f32,
    /// How long an interrupt lasts (randomly half again).
    pub interrupt_seconds: f32,
    /// Weights by name ([`INTERRUPTS`]); an unnamed one weighs 1.
    pub interrupts: BTreeMap<String, f32>,
}
impl Default for BotSurprise {
    fn default() -> Self {
        Self {
            strength: 0.0,
            band: 0.15,
            drift: 0.4,
            drift_seconds: 90.0,
            boredom: 0.02,
            boredom_seconds: 20.0,
            failure: 0.25,
            success: 0.5,
            effectiveness_seconds: 30.0,
            commit_seconds: 3.0,
            tell_seconds: 0.35,
            urgent_health: 0.3,
            urgent_range: 8.0,
            urgent_seconds: 1.5,
            flank_distance: 5.0,
            interrupts_per_minute: 2.0,
            interrupt_cooldown_seconds: 12.0,
            interrupt_seconds: 2.0,
            interrupts: BTreeMap::new(),
        }
    }
}
impl BotSurprise {
    pub fn validate(&self) -> Result<()> {
        let unit = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
        let positive = |v: f32| v.is_finite() && v > 0.0 && v <= 3600.0;
        let span = |v: f32| v.is_finite() && (0.0..=3600.0).contains(&v);
        ensure!(
            unit(self.strength)
                && unit(self.band)
                && unit(self.failure)
                && unit(self.success)
                && unit(self.urgent_health)
                && self.drift.is_finite()
                && (0.0..=3.0).contains(&self.drift)
                && self.boredom.is_finite()
                && (0.0..=10.0).contains(&self.boredom)
                && positive(self.drift_seconds)
                && positive(self.boredom_seconds)
                && positive(self.effectiveness_seconds)
                && positive(self.interrupt_seconds)
                && span(self.commit_seconds)
                && span(self.tell_seconds)
                && span(self.urgent_seconds)
                && span(self.interrupt_cooldown_seconds)
                && self.urgent_range.is_finite()
                && (0.0..=1000.0).contains(&self.urgent_range)
                && self.flank_distance.is_finite()
                && (0.0..=50.0).contains(&self.flank_distance)
                && self.interrupts_per_minute.is_finite()
                && (0.0..=60.0).contains(&self.interrupts_per_minute),
            "surprise tunables out of range"
        );
        for (name, weight) in &self.interrupts {
            ensure!(
                INTERRUPTS.contains(&name.as_str()),
                "Unknown surprise interrupt `{name}`; one of {}",
                INTERRUPTS.join(", ")
            );
            ensure!(
                weight.is_finite() && (0.0..=100.0).contains(weight),
                "Surprise interrupt `{name}` weight out of range"
            );
        }
        Ok(())
    }
    /// The weight of interrupt `name`.
    pub fn interrupt_weight(&self, name: &str) -> f32 {
        self.interrupts.get(name).copied().unwrap_or(1.0)
    }
}
