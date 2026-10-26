//! A kind's `composure` (`docs/architecture/bots.md`, "Composure"): how steady a
//! bot stays under fire. One number per bot, from `start` down by what it meets
//! (the events table) and back up with time; low enough, it runs.
//!
//! The mechanism is generic and the numbers are the Add-On's: a kind with no
//! `events` has no composure and the brain behaves as before.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The events the brain reports to a bot's composure, from what it already
/// perceives: an enemy newly seen, fighting heard, a hit taken.
pub const EVENTS: [&str; 3] = ["seen", "heard", "hurt"];

/// Composure's range: a bot never rises above or sinks below it.
pub const LIMIT: f32 = 99.0;

/// One event's blow: it takes `delta` off composure while composure is at or
/// above `floor` (a bot already shaken below the floor is not shaken further by it).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blow {
    pub delta: f32,
    pub floor: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BotComposure {
    /// Where composure starts, and the most it recovers to.
    pub start: f32,
    /// Seconds for one point to come back, after the last blow has gone 1 s
    /// unrepeated; 0 never recovers.
    pub recover_seconds: f32,
    /// At or below this it runs from what it knows of its enemies, as it
    /// does when hurt past `flee_below`.
    pub panic: f32,
    /// What each event takes ([`EVENTS`]); none, no composure.
    pub events: BTreeMap<String, Blow>,
}
impl Default for BotComposure {
    fn default() -> Self {
        Self {
            start: 0.0,
            recover_seconds: 0.0,
            panic: -LIMIT,
            events: BTreeMap::new(),
        }
    }
}
impl BotComposure {
    /// Whether this kind has composure at all.
    pub fn is_on(&self) -> bool {
        !self.events.is_empty()
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.start, self.panic]
                .iter()
                .all(|v| v.is_finite() && (-LIMIT..=LIMIT).contains(v))
                && self.recover_seconds.is_finite()
                && (0.0..=600.0).contains(&self.recover_seconds),
            "composure dials out of range"
        );
        for (event, blow) in &self.events {
            ensure!(
                EVENTS.contains(&event.as_str()),
                "composure event `{event}` is not one of {EVENTS:?}"
            );
            ensure!(
                blow.delta.is_finite()
                    && (-LIMIT..=LIMIT).contains(&blow.delta)
                    && blow.floor.is_finite()
                    && (-LIMIT..=LIMIT).contains(&blow.floor),
                "composure event `{event}` is out of range"
            );
        }
        Ok(())
    }
    /// `current` after `event`, or unchanged when the kind has no blow for it
    /// or composure is already below the blow's floor.
    pub fn after(&self, current: f32, event: &str) -> f32 {
        match self.events.get(event) {
            Some(blow) if current >= blow.floor => (current - blow.delta).clamp(-LIMIT, LIMIT),
            _ => current,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind() -> BotComposure {
        BotComposure {
            start: 25.0,
            recover_seconds: 2.0,
            panic: -60.0,
            events: BTreeMap::from([
                (
                    "seen".into(),
                    Blow {
                        delta: 2.0,
                        floor: -10.0,
                    },
                ),
                (
                    "hurt".into(),
                    Blow {
                        delta: 30.0,
                        floor: -60.0,
                    },
                ),
            ]),
        }
    }

    #[test]
    fn a_kind_without_events_has_no_composure() {
        assert!(!BotComposure::default().is_on());
        assert!(kind().is_on());
    }

    #[test]
    fn a_blow_applies_only_above_its_floor_and_never_past_the_limit() {
        let c = kind();
        assert_eq!(c.after(25.0, "seen"), 23.0);
        assert_eq!(c.after(-20.0, "seen"), -20.0, "already below the floor");
        assert_eq!(c.after(-50.0, "hurt"), -80.0);
        assert_eq!(c.after(-70.0, "hurt"), -70.0);
        assert_eq!(
            c.after(-50.0, "heard"),
            -50.0,
            "an event the kind has no blow for"
        );
        let hard = BotComposure {
            events: BTreeMap::from([(
                "hurt".into(),
                Blow {
                    delta: 500.0,
                    floor: -99.0,
                },
            )]),
            ..kind()
        };
        assert!(hard.validate().is_err());
    }

    #[test]
    fn an_unknown_event_is_refused() {
        let c = BotComposure {
            events: BTreeMap::from([(
                "dazed".into(),
                Blow {
                    delta: 1.0,
                    floor: 0.0,
                },
            )]),
            ..kind()
        };
        assert!(c.validate().is_err());
    }
}
