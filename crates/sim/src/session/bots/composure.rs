//! How steady a bot stays under fire (`bot_kind::BotComposure`): the events
//! the brain already perceives take points off one number, time brings them
//! back, and a bot at or below its kind's panic line runs from what it knows
//! of its enemies (`intents::bot_flee_from`). The numbers are the kind's.
use super::*;

impl Brain {
    /// `event` (`bot_kind::composure::EVENTS`) meets this bot.
    pub(super) fn nervous(&mut self, event: &str, tick: u64) {
        let table = &self.kind.composure;
        if !table.is_on() {
            return;
        }
        let after = table.after(self.composure, event);
        if after != self.composure {
            self.composure = after;
            self.nervous_at = tick;
        }
    }
    /// Time brings composure back toward where it started: one point each
    /// `recover_seconds`, once a second has passed since the last blow.
    pub(super) fn recover(&mut self, tick: u64) {
        let table = &self.kind.composure;
        let since = tick.saturating_sub(self.composure_tick);
        self.composure_tick = tick;
        if !table.is_on()
            || table.recover_seconds <= 0.0
            || self.composure >= self.composure_start.unwrap_or(table.start)
            || tick < self.nervous_at.saturating_add(ticks(1.0))
        {
            return;
        }
        let back = since as f32 / ticks(table.recover_seconds).max(1) as f32;
        self.composure = (self.composure + back).min(self.composure_start.unwrap_or(table.start));
    }
    /// At or below its kind's panic line.
    pub(super) fn panicked(&self) -> bool {
        let table = &self.kind.composure;
        table.is_on() && self.composure <= table.panic
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot_kind::{Blow, BotComposure};

    fn brain() -> Brain {
        let mut kind = BotKind::default();
        kind.composure = BotComposure {
            start: 25.0,
            recover_seconds: 2.0,
            panic: -60.0,
            events: std::collections::BTreeMap::from([
                (
                    "hurt".into(),
                    Blow {
                        delta: 30.0,
                        floor: -99.0,
                        ..Blow::default()
                    },
                ),
                (
                    "seen".into(),
                    Blow {
                        delta: 2.0,
                        floor: -10.0,
                        ..Blow::default()
                    },
                ),
            ]),
        };
        let mut brain = Brain::new(None, kind, Vec3::ZERO, 1, 0);
        brain.composure = 25.0;
        brain
    }

    #[test]
    fn hits_shake_it_to_panic_and_time_steadies_it() {
        let mut b = brain();
        for tick in [0, 10, 20] {
            b.nervous("hurt", tick);
        }
        assert_eq!(b.composure, -65.0);
        assert!(b.panicked());
        // Nothing comes back in the second after a blow.
        b.recover(60);
        assert_eq!(b.composure, -65.0);
        // Then one point each 2 s: 11 s is five and a half points.
        b.recover(60 + 11 * 120);
        assert!(
            b.composure > -60.0 && b.composure < -59.0,
            "{}",
            b.composure
        );
        assert!(!b.panicked());
    }

    #[test]
    fn composure_stops_at_where_it_started() {
        let mut b = brain();
        b.nervous("seen", 0);
        b.recover(1_000_000);
        assert_eq!(b.composure, 25.0);
    }

    #[test]
    fn a_kind_without_events_is_never_shaken() {
        let mut b = Brain::new(None, BotKind::default(), Vec3::ZERO, 2, 0);
        b.nervous("hurt", 0);
        b.recover(500);
        assert!(!b.panicked());
    }
}
