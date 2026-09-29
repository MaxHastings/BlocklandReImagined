//! What a package's server behaviour may do to the world, by name. The
//! runtime refuses any operation whose capability the manifest does not
//! declare (`bri_package_runtime::ops::authorize`); players see each
//! declared capability in plain words before turning an Add-On on.

/// Every capability a manifest may declare.
pub const CAPABILITIES: &[&str] = &[
    "world.edit",
    "damage",
    "entity",
    "chat",
    "player",
    "build",
    "physics",
    "effects",
];

/// The plain-language line a player reads for `name`, completing
/// "This Add-On can ...".
pub fn describe(name: &str) -> Option<&'static str> {
    Some(match name {
        // Remove bricks from the world.
        "world.edit" => "change the world's bricks",
        // Explosions and direct damage to players and bricks.
        "damage" => "hurt and heal players and break bricks",
        // Spawn, steer and remove the package's own entities.
        "entity" => "spawn and move its own creatures and objects",
        // Send chat lines to players, and print text on their screens.
        "chat" => "send chat messages and put text on players' screens",
        // Move players, respawn them, change their body, hand them an
        // entity to drive, give them an item or ammo, change what they hold
        // or how wide they see.
        "player" => "move and respawn players, change their bodies and view, and give them items and ammo",
        // Copy a build for a player to place under the plant rules.
        "build" => "copy builds for players to place again",
        // Push, hold and throw players, vehicles and entities (within the
        // minigame and trust rules), and spawn its own vehicles.
        "physics" => "grab, push and throw players and vehicles, and spawn its own vehicles",
        // Presentation only: sounds in the world or to one player, beams,
        // and animations on players. Nothing here changes the game.
        "effects" => "play sounds and show effects",
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
