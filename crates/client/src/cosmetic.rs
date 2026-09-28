//! Faults in cosmetic presentation: gun casings, brick and explosion debris,
//! particles, weather, held-item and avatar posing. A bad frame there loses
//! one effect, not the game (a16 closed on an invalid casing collision), so
//! the frame counts the fault, logs its first occurrence per subsystem and
//! carries on. Simulation, network and content integrity errors stay fatal.
use anyhow::Result;
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct CosmeticFaults {
    counts: BTreeMap<&'static str, u64>,
}

impl CosmeticFaults {
    /// The value, or `None` after counting (and, the first time, logging)
    /// the subsystem's error.
    pub fn absorb<T>(&mut self, subsystem: &'static str, result: Result<T>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                let count = self.counts.entry(subsystem).or_default();
                if *count == 0 {
                    eprintln!("{subsystem}: {error:#} (cosmetic; the game continues)");
                }
                *count += 1;
                None
            }
        }
    }
    pub fn count(&self, subsystem: &str) -> u64 {
        self.counts.get(subsystem).copied().unwrap_or(0)
    }
    pub fn total(&self) -> u64 {
        self.counts.values().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cosmetic_errors_are_counted_per_subsystem_and_never_raised() {
        let mut faults = CosmeticFaults::default();
        assert_eq!(faults.absorb("gun casings", Ok(3)), Some(3));
        for _ in 0..3 {
            assert_eq!(
                faults.absorb::<()>(
                    "gun casings",
                    Err(anyhow::anyhow!("invalid debris collision result"))
                ),
                None
            );
        }
        assert_eq!(
            faults.absorb::<u8>("weather", Err(anyhow::anyhow!("bad frame"))),
            None
        );
        assert_eq!(faults.count("gun casings"), 3);
        assert_eq!(faults.count("weather"), 1);
        assert_eq!(faults.total(), 4);
    }
}
