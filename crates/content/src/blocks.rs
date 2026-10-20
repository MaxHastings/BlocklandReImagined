//! What a collider blocks: the query types a map's collision answers to.
//!
//! A collider with no mask blocks every query. A map Add-On can mark a collision mesh (`bri_blocks` on its glTF node)
//! with the query types it blocks, so a surface that stops bodies need not stop shots, or the reverse. The engine gives
//! the names no gameplay meaning; callers say which query they are.
//!
//! - `move`: a body or mover moving (a player walking, a sliding door).
//! - `ray`: a zero-extent trace (line of sight, a pointer ray).
//! - `sweep`: a trace with an extent (a projectile's flight, a grab).
//! - `shot`: a bullet-like trace; it also needs `ray`.

/// Moving bodies and movers.
pub const MOVE: &str = "move";
/// Zero-extent traces.
pub const RAY: &str = "ray";
/// Traces with an extent.
pub const SWEEP: &str = "sweep";
/// Bullet-like traces.
pub const SHOT: &str = "shot";
/// Every query type, in a fixed order.
pub const ALL: [&str; 4] = [MOVE, RAY, SWEEP, SHOT];

/// Whether every name is one of [`ALL`] (an empty list blocks nothing and is allowed).
pub fn valid(names: &[String]) -> bool {
    names.iter().all(|n| ALL.contains(&n.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_four_names_are_valid() {
        assert!(valid(&["move".into(), "shot".into()]));
        assert!(valid(&[]));
        assert!(!valid(&["walk".into()]));
    }
}
