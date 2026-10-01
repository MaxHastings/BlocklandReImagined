//! Whether the connection to the host has gone quiet (v20's lag icon).
//!
//! Torque's `GameConnection::detectLag` compares the time since the last
//! packet from the server with the lag threshold every client tick and calls
//! `setLagIcon` when the answer changes; v20 sets the threshold from
//! `$Pref::Net::LagThreshold` (400 ms). Here any datagram from the host
//! counts, transport acknowledgements included: the client sends movement
//! every tick, so a live host answers well inside the threshold even when
//! nothing in the world moves.
use std::time::{Duration, Instant};

/// `$Pref::Net::LagThreshold`'s v20 default.
pub const DEFAULT_LAG_THRESHOLD: Duration = Duration::from_millis(400);

/// Tracks the last time the host was heard from.
#[derive(Debug, Clone)]
pub struct LagWatch {
    threshold: Duration,
    /// Datagrams received so far, and when that count last changed.
    heard: Option<(u64, Instant)>,
    lagging: bool,
}
impl Default for LagWatch {
    fn default() -> Self {
        Self::new(DEFAULT_LAG_THRESHOLD)
    }
}
impl LagWatch {
    pub fn new(threshold: Duration) -> Self {
        Self {
            threshold,
            heard: None,
            lagging: false,
        }
    }
    pub fn set_threshold(&mut self, threshold: Duration) {
        self.threshold = threshold;
    }
    pub fn lagging(&self) -> bool {
        self.lagging
    }
    /// Record the transport's received datagram count at `now`. Returns the
    /// new state when it changed.
    pub fn observe(&mut self, now: Instant, received: u64) -> Option<bool> {
        let since = match self.heard {
            Some((count, at)) if count == received => now.saturating_duration_since(at),
            _ => {
                self.heard = Some((received, now));
                Duration::ZERO
            }
        };
        let lagging = since > self.threshold;
        (lagging != self.lagging).then(|| {
            self.lagging = lagging;
            lagging
        })
    }
    /// Forget the connection: the next observation starts fresh.
    pub fn reset(&mut self) {
        self.heard = None;
        self.lagging = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn quiet_past_the_threshold_lags_until_the_host_is_heard_again() {
        let start = Instant::now();
        let mut watch = LagWatch::default();
        assert_eq!(watch.observe(start, 10), None);
        // Packets every 32 ms: never lagging.
        for i in 1..=20 {
            assert_eq!(watch.observe(start + ms(32 * i), 10 + i), None);
        }
        let last = start + ms(32 * 20);
        // Silence up to the threshold is not lag.
        assert_eq!(watch.observe(last + ms(400), 30), None);
        assert!(!watch.lagging());
        // Past it is, reported once.
        assert_eq!(watch.observe(last + ms(401), 30), Some(true));
        assert_eq!(watch.observe(last + ms(2000), 30), None);
        assert!(watch.lagging());
        // The first datagram back clears it.
        assert_eq!(watch.observe(last + ms(2001), 31), Some(false));
        assert_eq!(watch.observe(last + ms(2100), 32), None);
    }

    #[test]
    fn a_stalled_frame_that_finds_new_datagrams_is_not_lag() {
        // The client froze for a second, but the transport kept receiving.
        let start = Instant::now();
        let mut watch = LagWatch::default();
        watch.observe(start, 5);
        assert_eq!(watch.observe(start + ms(1000), 40), None);
        assert!(!watch.lagging());
    }

    #[test]
    fn the_threshold_follows_the_pref_and_reset_starts_over() {
        let start = Instant::now();
        let mut watch = LagWatch::new(ms(100));
        watch.observe(start, 1);
        assert_eq!(watch.observe(start + ms(150), 1), Some(true));
        watch.set_threshold(ms(400));
        assert_eq!(watch.observe(start + ms(200), 1), Some(false));
        watch.reset();
        assert!(!watch.lagging());
        // A new connection's first observation is a fresh start, whatever
        // its count.
        assert_eq!(watch.observe(start + ms(5000), 1), None);
        assert_eq!(watch.observe(start + ms(5300), 1), None);
        assert_eq!(watch.observe(start + ms(5401), 1), Some(true));
    }
}
