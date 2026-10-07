//! Per-bot timing. Bot cadences (refire pulses, alerts, dismount checks,
//! weave legs, footwork) must not key off the shared game tick directly, or
//! every bot acts on the same tick. Each cadence goes through [`beat`]: the
//! bot gets its own seeded phase, and each cycle fires at a seeded point, so
//! intervals vary between 3/4 and 5/4 of the period and bots drift apart.
//!
//! All of it is a pure function of (bot, salt, tick): no state, no RNG draws,
//! so replays and tests stay deterministic.

use super::OwnerId;

/// Mixes a value into a well-spread 64-bit hash (splitmix64 finaliser).
pub(crate) fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A seeded hash of the bot and a cadence name, so two cadences of one bot
/// are not in step either.
pub(crate) fn seed(bot: OwnerId, salt: u64) -> u64 {
    mix(bot ^ mix(salt))
}

/// The bot's own phase for a cadence: add it to the tick before any modulo.
pub(crate) fn bot_phase(bot: OwnerId, salt: u64) -> u64 {
    seed(bot, salt) & 0xFFFF
}

/// A uniform value in [0, 1) from a seed.
pub(crate) fn unit(seed: u64) -> f32 {
    (mix(seed) >> 40) as f32 / (1u64 << 24) as f32
}

/// The cycle the tick falls in, and the tick inside the cycle the beat
/// lands on. Each cycle of `period` ticks fires once, at a seeded offset
/// within its first half, so consecutive beats are 1/2 to 3/2 periods apart
/// (mean `period`).
fn landing(bot: OwnerId, salt: u64, tick: u64, period: u64) -> (u64, u64) {
    let period = period.max(1);
    let shifted = tick + bot_phase(bot, salt);
    let cycle = shifted / period;
    let span = (period / 2).max(1);
    let at = mix(seed(bot, salt) ^ cycle) % span;
    (shifted % period, at)
}

/// True on the bot's beat for a cadence of about `period` ticks.
pub(crate) fn beat(bot: OwnerId, salt: u64, tick: u64, period: u64) -> bool {
    let (inside, at) = landing(bot, salt, tick, period);
    inside == at
}

/// Where the tick falls in the bot's own cycles of `period` ticks: the
/// cycle's index and how far through it the tick is, in [0, 1). For a
/// motion that runs through a cycle (footwork, aim drift), not a one-tick
/// beat.
pub(crate) fn cycle(bot: OwnerId, salt: u64, tick: u64, period: u64) -> (u64, f32) {
    let period = period.max(1);
    let shifted = tick + bot_phase(bot, salt);
    (shifted / period, (shifted % period) as f32 / period as f32)
}

/// A smooth seeded signal in [-1, 1] for one bot: a new seeded point every
/// `period` ticks, eased from one to the next, so it never jumps and never
/// settles. `channel` gives independent signals for one salt (yaw, pitch).
pub(crate) fn drift(bot: OwnerId, salt: u64, channel: u64, tick: u64, period: u64) -> f32 {
    let (k, f) = cycle(bot, salt, tick, period);
    let point = |k: u64| spread(bot, salt, mix(k) ^ channel, -1.0, 1.0);
    let eased = f * f * (3.0 - 2.0 * f);
    point(k) + (point(k + 1) - point(k)) * eased
}

/// Ticks in one burst-and-pause cycle of fire.
const BURST_TICKS: u64 = 150;
/// The shortest and longest pause, as a share of the cycle.
const PAUSE: (f32, f32) = (0.12, 0.4);

/// Whether the bot is in a burst (true) or a pause between bursts: each
/// cycle of about 1.25 s ends in a seeded pause of 0.15 to 0.5 s.
pub(crate) fn bursting(bot: OwnerId, tick: u64) -> bool {
    let (k, f) = cycle(bot, salt::BURST, tick, BURST_TICKS);
    f < 1.0 - spread(bot, salt::BURST, k, PAUSE.0, PAUSE.1)
}

/// A seeded value in [lo, hi) for one bot, cadence and occasion (a death,
/// a leg, a burst). The same arguments give the same value.
pub(crate) fn spread(bot: OwnerId, salt: u64, occasion: u64, lo: f32, hi: f32) -> f32 {
    lo + (hi - lo) * unit(seed(bot, salt) ^ mix(occasion))
}

/// Cadence names. Each is a salt, so cadences of one bot stay independent.
pub(crate) mod salt {
    pub(crate) const FIRE: u64 = 1;
    pub(crate) const ALERT: u64 = 2;
    pub(crate) const DISMOUNT: u64 = 3;
    pub(crate) const FOOTWORK: u64 = 5;
    pub(crate) const RESPAWN: u64 = 6;
    pub(crate) const BURST: u64 = 7;
    pub(crate) const AIM: u64 = 8;
    pub(crate) const LEAD: u64 = 10;
    /// Looking round for the mood (`team`).
    pub(crate) const MOOD: u64 = 21;
    /// A reflex's drift (`surprise::Domain::reflex`).
    pub(crate) const REFLEX: u64 = 22;
    /// Where a search's sweep looks.
    pub(crate) const SWEEP: u64 = 23;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beats(bot: OwnerId, period: u64, ticks: u64) -> Vec<u64> {
        (0..ticks)
            .filter(|t| beat(bot, salt::FIRE, *t, period))
            .collect()
    }

    #[test]
    fn bots_do_not_fire_on_the_same_ticks() {
        let firsts: Vec<u64> = (1..=8).map(|bot| beats(bot, 40, 200)[0]).collect();
        let mut unique = firsts.clone();
        unique.sort_unstable();
        unique.dedup();
        assert!(unique.len() >= 6, "first beats {firsts:?}");
        for a in 1..=8 {
            for b in (a + 1)..=8 {
                assert_ne!(
                    beats(a, 40, 2400),
                    beats(b, 40, 2400),
                    "{a} and {b} in step"
                );
            }
        }
    }

    #[test]
    fn a_cadence_keeps_its_mean_period_but_its_intervals_vary() {
        for bot in 1..=6 {
            let ticks = beats(bot, 40, 40 * 400);
            let gaps: Vec<f32> = ticks.windows(2).map(|w| (w[1] - w[0]) as f32).collect();
            let mean = gaps.iter().sum::<f32>() / gaps.len() as f32;
            assert!((mean - 40.0).abs() < 2.0, "bot {bot} mean {mean}");
            assert!(
                gaps.iter().all(|g| *g >= 1.0 && *g < 60.0),
                "bot {bot} {gaps:?}"
            );
            let var = gaps.iter().map(|g| (g - mean).powi(2)).sum::<f32>() / gaps.len() as f32;
            assert!(var.sqrt() / mean > 0.1, "bot {bot} is a metronome");
        }
    }

    #[test]
    fn drift_is_smooth_and_keeps_moving() {
        for bot in 1..=6 {
            let values: Vec<f32> = (0..4800).map(|t| drift(bot, salt::AIM, 0, t, 48)).collect();
            assert!(values.iter().all(|v| (-1.0..=1.0).contains(v)));
            let steps = values.windows(2).map(|w| (w[1] - w[0]).abs());
            assert!(steps.clone().all(|d| d < 0.1), "bot {bot} jumps");
            // It never rests: every half second it has moved.
            for w in values.windows(60) {
                let (lo, hi) = w
                    .iter()
                    .fold((1.0f32, -1.0f32), |(a, b), v| (a.min(*v), b.max(*v)));
                assert!(hi - lo > 0.01, "bot {bot} sat still");
            }
            let wide = values.iter().filter(|v| v.abs() > 0.5).count();
            assert!(wide > 600, "bot {bot} hugs zero: {wide}");
        }
        assert_ne!(
            drift(1, salt::AIM, 0, 100, 48),
            drift(1, salt::AIM, 1, 100, 48)
        );
    }

    #[test]
    fn fire_comes_in_bursts_with_pauses() {
        for bot in 1..=6 {
            let on: Vec<bool> = (0..12_000).map(|t| bursting(bot, t)).collect();
            let duty = on.iter().filter(|b| **b).count() as f32 / on.len() as f32;
            assert!((0.6..0.9).contains(&duty), "bot {bot} duty {duty}");
            let pauses = on.windows(2).filter(|w| w[0] && !w[1]).count();
            assert!(pauses >= 70, "bot {bot} {pauses} pauses");
        }
        let at = |bot| (0..400).find(|t| !bursting(bot, *t));
        assert!(
            (1..=8)
                .map(at)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                > 4
        );
    }

    #[test]
    fn the_aim_error_drifts_within_its_size() {
        // The error keeps drifting within its size, never parked at one
        // offset and not hugging zero.
        let size = 2f32.to_radians();
        for bot in 1..=4 {
            let errors: Vec<(f32, f32)> = (0..2400)
                .map(|t| super::super::aim_error(bot, 600 + t, size))
                .collect();
            assert!(
                errors
                    .iter()
                    .all(|e| e.0.abs() <= size * super::super::DRIFT_SPREAD + 1e-6)
            );
            for w in errors.windows(60) {
                let (lo, hi) = w
                    .iter()
                    .fold((1f32, -1f32), |(a, b), e| (a.min(e.0), b.max(e.0)));
                assert!(hi - lo > size * 0.005, "bot {bot} parked");
            }
            let wide = errors.iter().filter(|e| e.0.abs() > size * 0.5).count();
            assert!(wide > 300, "bot {bot} aims too true: {wide}");
        }
    }

    #[test]
    fn spread_is_deterministic_and_in_range() {
        for occasion in 0..200 {
            let v = spread(3, salt::RESPAWN, occasion, 0.4, 2.5);
            assert!((0.4..2.5).contains(&v));
            assert_eq!(v, spread(3, salt::RESPAWN, occasion, 0.4, 2.5));
        }
        assert_ne!(
            spread(3, salt::RESPAWN, 1, 0.0, 1.0),
            spread(4, salt::RESPAWN, 1, 0.0, 1.0)
        );
    }
}
