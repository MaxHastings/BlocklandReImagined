//! Surface kinds: what a body stands on or a shot strikes, a short name a
//! map gives its materials (`dirt`, `metal`, `grass`). The engine gives
//! kinds no meaning; a body's footsteps and a projectile's impacts say what
//! each one sounds and looks like (`bri_sim::map::MapSurfaces`).

/// Anything a map names no kind for.
pub const DEFAULT: &str = "default";
/// Every brick.
pub const BRICK: &str = "brick";
/// v20 heightfield terrain.
pub const TERRAIN: &str = "terrain";

/// A surface kind's name: 1 to 32 ASCII letters, digits, `_` or `-`.
pub fn valid_name(kind: &str) -> bool {
    (1..=32).contains(&kind.len())
        && kind
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[cfg(test)]
mod tests {
    #[test]
    fn kind_names_are_short_plain_words() {
        for good in ["dirt", "metal_grate", "snow-2", super::DEFAULT] {
            assert!(super::valid_name(good), "{good}");
        }
        for bad in ["", "two words", "métal", &"x".repeat(33)] {
            assert!(!super::valid_name(bad), "{bad}");
        }
    }
}
