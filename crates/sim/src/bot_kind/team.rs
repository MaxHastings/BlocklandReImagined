//! A kind's `team` weights (`docs/architecture/bots.md`, "Coordination"):
//! how much teammates' current intents move its own scores. All zero, the
//! default, is the brain without coordination.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The terms a teammate's intent adds to an option's score; a callout
/// template may be keyed by each.
pub const TERMS: [&str; 3] = ["overlap", "uses", "harm"];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BotTeam {
    /// Cost per teammate already going for the same target, or for a place
    /// within `overlap_radius` (less the farther apart).
    pub overlap: f32,
    pub overlap_radius: f32,
    /// Bonus for an option that uses what a teammate's intent exposes.
    pub uses: f32,
    /// Cost of an option whose place lies where a teammate's weapon will hit.
    pub harm: f32,
    /// How much more often it takes up an idle flavour as the share of bots
    /// within `mood_radius` (either side) doing one rises, at most `mood_cap`.
    pub mood: f32,
    pub mood_radius: f32,
    pub mood_cap: f32,
    /// How much a person counts in that share against a bot's 1.
    pub mood_human: f32,
    /// At most one callout this often, in seconds.
    pub callout_seconds: f32,
    /// Team chat lines by the term that changed its choice ([`TERMS`]).
    pub callouts: BTreeMap<String, String>,
}
impl BotTeam {
    pub fn validate(&self) -> Result<()> {
        let weight = |v: f32| v.is_finite() && (0.0..=2.0).contains(&v);
        ensure!(
            weight(self.overlap)
                && weight(self.uses)
                && weight(self.harm)
                && [self.mood, self.mood_cap, self.mood_human]
                    .iter()
                    .all(|m| m.is_finite() && (0.0..=50.0).contains(m))
                && [self.overlap_radius, self.mood_radius]
                    .iter()
                    .all(|r| r.is_finite() && (0.0..=64.0).contains(r))
                && self.callout_seconds.is_finite()
                && (0.0..=3600.0).contains(&self.callout_seconds),
            "team weights out of range"
        );
        for (term, line) in &self.callouts {
            ensure!(
                TERMS.contains(&term.as_str()),
                "Unknown team callout `{term}`; one of {}",
                TERMS.join(", ")
            );
            ensure!(
                !line.trim().is_empty()
                    && line.chars().count() <= 64
                    && !line.chars().any(char::is_control),
                "Team callout `{term}` must be 1-64 characters"
            );
        }
        Ok(())
    }
}
