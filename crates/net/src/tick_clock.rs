//! Convert real elapsed time into bounded fixed simulation steps. Timer wakeups
//! are only opportunities to run: coarse OS timers must not slow gameplay.
use std::time::Duration;

#[derive(Default)]
pub(crate) struct TickClock {
    // Nanoseconds * ticks/second, preserving the fractional 1/120-second tick.
    remainder: u128,
    pub dropped: u64,
}
impl TickClock {
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        self.remainder += elapsed.as_nanos() * 120;
        let due = self.remainder / 1_000_000_000;
        self.remainder %= 1_000_000_000;
        // Bound catch-up to avoid a spiral of death after a long blocked task.
        let steps = due.min(8);
        self.dropped = self
            .dropped
            .saturating_add((due - steps).min(u64::MAX as u128) as u64);
        steps as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coarse_wakeups_preserve_real_simulation_rate_and_stalls_are_bounded() {
        let mut clock = TickClock::default();
        let mut ticks = 0;
        // Typical coarse wakeups, exactly one second of elapsed time.
        for _ in 0..62 {
            ticks += clock.advance(Duration::from_millis(16));
        }
        ticks += clock.advance(Duration::from_millis(8));
        assert_eq!(ticks, 120);
        assert_eq!(clock.dropped, 0);
        assert_eq!(clock.advance(Duration::from_millis(500)), 8);
        assert_eq!(clock.dropped, 52);
        assert_eq!(clock.advance(Duration::ZERO), 0);
        assert_eq!(clock.advance(Duration::from_millis(25)), 3);
    }
}
