//! A kind's `team` dials (`docs/architecture/bots.md`, "Coordination"):
//! how much teammates' current intents, the mood of the players it sees and
//! its team's score move its own scores.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The terms a teammate's intent adds to an option's score, and `clear`
/// (going to knock an opponent off a body a teammate works); a callout
/// template may be keyed by each.
pub const TERMS: [&str; 4] = ["overlap", "uses", "harm", "clear"];

/// What each term is worth at `teamwork` 1, on the 0-1 scale behaviour
/// scores use. Fixed in code: their ratio is the mechanism, the dial is
/// how much of it a kind shows. An ally on the same target or spot costs
/// a third of a score at most (each further one half the last), a seat or sightline it offers adds as much,
/// and standing in its line of fire costs twice that, since it is a risk
/// to both.
const OVERLAP: f32 = 0.3;
const USES: f32 = 0.3;
const HARM: f32 = 0.6;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BotTeam {
    /// 0-1: how much allies' intents count, for crowding (overlap) and
    /// for using or keeping clear of what they do (interaction) together.
    pub teamwork: f32,
    /// How much more often it takes up an idle flavour by the share of the
    /// players it sees doing one, at most `mood_cap` times.
    pub mood: f32,
    pub mood_cap: f32,
    /// How much a person counts in that share against a bot's 1.
    pub mood_human: f32,
    /// Objectives are worth this much more as its team falls behind: by
    /// `1 + pressure * behind / (behind + 1)`, `behind` in points.
    pub pressure: f32,
    /// Seeing a teammate's option work (a hit) makes the same option score
    /// up to this much more, fading over the surprise `effectiveness_seconds`.
    pub copy: f32,
    /// Team chat lines by the term that changed its choice ([`TERMS`]).
    pub callouts: BTreeMap<String, String>,
}
impl Default for BotTeam {
    fn default() -> Self {
        Self {
            teamwork: 0.6,
            mood: 16.0,
            mood_cap: 10.0,
            mood_human: 3.0,
            pressure: 0.3,
            copy: 0.15,
            callouts: BTreeMap::new(),
        }
    }
}
impl BotTeam {
    /// Cost per earlier ally on the same target or spot.
    pub fn overlap(&self) -> f32 {
        self.teamwork * OVERLAP
    }
    /// Bonus for using what an ally's intent exposes.
    pub fn uses(&self) -> f32 {
        self.teamwork * USES
    }
    /// Cost of standing where an ally's weapon will hit.
    pub fn harm(&self) -> f32 {
        self.teamwork * HARM
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (0.0..=1.0).contains(&self.teamwork)
                && (0.0..=2.0).contains(&self.pressure)
                && (0.0..=1.0).contains(&self.copy)
                && [self.mood, self.mood_cap, self.mood_human]
                    .iter()
                    .all(|m| (0.0..=50.0).contains(m)),
            "team dials out of range"
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
