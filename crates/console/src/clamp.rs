//! A float clamp that names its caller when its bounds are wrong.
//!
//! `f32::clamp` and `f64::clamp` panic when `min > max` or a bound is NaN,
//! and the panic names `core/src/num/f32.rs`, never the code that computed
//! the bounds: they are not `#[track_caller]`. [`Clamp::clamped`] is the
//! same clamp for ordered bounds. For others it warns once per call site,
//! naming the caller's file and line and the operands, and returns a defined
//! value (each bound that is a number applies, the lower first), so a bad
//! bound costs a wrong value instead of the game. Debug builds (tests)
//! panic at the caller instead, so such a fault is never missed there.
//!
//! The workspace `clippy.toml` disallows `f32::clamp` and `f64::clamp`.
use std::{collections::BTreeSet, panic::Location, sync::Mutex};

/// [`f32::clamp`] and [`f64::clamp`], reporting a caller whose bounds are
/// out of order instead of panicking inside `core`.
pub trait Clamp: Copy {
    /// `self` restricted to `min..=max`, exactly as `clamp` for ordered
    /// bounds; see the [module docs](self) for the others.
    #[track_caller]
    fn clamped(self, min: Self, max: Self) -> Self;
}

macro_rules! clamp_float {
    ($($t:ty),+) => {$(
        impl Clamp for $t {
            #[track_caller]
            #[inline]
            fn clamped(self, min: Self, max: Self) -> Self {
                // False for a NaN bound as well as for reversed ones.
                if min <= max {
                    #[allow(clippy::disallowed_methods, reason = "the bounds are ordered")]
                    return self.clamp(min, max);
                }
                bad_bounds(
                    Location::caller(),
                    format_args!("{self}.clamped({min}, {max})"),
                );
                self.max(min).min(max)
            }
        }
    )+};
}
clamp_float!(f32, f64);

/// Call sites already reported, by file, line and column.
static REPORTED: Mutex<BTreeSet<(&'static str, u32, u32)>> = Mutex::new(BTreeSet::new());

#[cold]
#[inline(never)]
fn bad_bounds(at: &'static Location<'static>, operands: std::fmt::Arguments<'_>) {
    let first = REPORTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert((at.file(), at.line(), at.column()));
    if first {
        crate::warn(format!("Clamp bounds out of order at {at}: {operands}"));
    }
    debug_assert!(false, "Clamp bounds out of order at {at}: {operands}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::disallowed_methods, reason = "the reference behaviour")]
    fn ordered_bounds_clamp_as_the_standard_library_does() {
        for (v, lo, hi) in [
            (5.0f32, 0.0, 1.0),
            (-5.0, 0.0, 1.0),
            (0.5, 0.0, 1.0),
            (2.0, 2.0, 2.0),
        ] {
            assert_eq!(v.clamped(lo, hi), v.clamp(lo, hi));
            let (v, lo, hi) = (f64::from(v), f64::from(lo), f64::from(hi));
            assert_eq!(v.clamped(lo, hi), v.clamp(lo, hi));
        }
        assert!(f32::NAN.clamped(0.0, 1.0).is_nan());
        assert_eq!(3.0f32.clamped(f32::NEG_INFINITY, f32::INFINITY), 3.0);
    }

    /// A NaN bound names this file and line, not `core`'s: in the panic of
    /// a debug build, and in the one console warning however often the
    /// call runs.
    #[test]
    fn a_nan_bound_reports_the_callers_location_once() {
        let line = line!() + 2;
        for _ in 0..3 {
            let caught = std::panic::catch_unwind(|| 0.5f32.clamped(f32::NAN, 1.0));
            if cfg!(debug_assertions) {
                let message = caught.expect_err("debug builds fail loudly");
                let message = message.downcast_ref::<String>().cloned();
                let message = message.unwrap_or_default();
                let at = format!("{}:{line}:", file!());
                assert!(message.contains(&at), "{message}");
            } else {
                assert_eq!(caught.ok(), Some(0.5), "the bound that is a number applies");
            }
        }
        let reported = REPORTED.lock().unwrap();
        let here: Vec<_> = reported
            .iter()
            .filter(|(file, at, _)| *file == file!() && *at == line)
            .collect();
        assert_eq!(here.len(), 1, "reported once: {here:?}");
    }
}
