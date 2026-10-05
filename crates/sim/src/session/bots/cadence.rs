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
    pub(crate) const RESPAWN: u64 = 6;
    pub(crate) const HOP: u64 = 9;
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
