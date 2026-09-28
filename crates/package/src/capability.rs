//! What a package's server behaviour may do to the world, by name. The
//! runtime refuses any operation whose capability the manifest does not
//! declare (`bri_package_runtime::ops::authorize`); players see each
//! declared capability in plain words before turning an Add-On on.

/// Every capability a manifest may declare.
pub const CAPABILITIES: &[&str] = &["world.edit", "damage", "entity", "chat"];

/// The plain-language line a player reads for `name`, completing
/// "This Add-On can ...".
pub fn describe(name: &str) -> Option<&'static str> {
    Some(match name {
        // Remove bricks from the world.
        "world.edit" => "change the world's bricks",
        // Explosions and direct damage to players and bricks.
        "damage" => "hurt players and break bricks",
        // Spawn, steer and remove the package's own entities.
        "entity" => "spawn and move its own creatures and objects",
        // Send chat lines to players.
        "chat" => "send chat messages",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_capability_has_plain_words() {
        for name in CAPABILITIES {
            assert!(describe(name).is_some(), "{name} has no description");
        }
        assert!(describe("players.teleport").is_none());
    }
}
