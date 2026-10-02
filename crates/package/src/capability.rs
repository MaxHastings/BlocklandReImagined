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
    "lighting",
    "environment",
    "minigame",
    "brick_events",
    "bots",
    "storage",
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
        "player" => {
            "move and respawn players, change their bodies and view, and give them items and ammo"
        }
        // Copy a build for a player to place under the plant rules.
        "build" => "copy builds for players to place again",
        // Push, hold and throw players, vehicles and entities (within the
        // minigame and trust rules), spawn its own vehicles, and put away
        // its kinds of vehicles at their owner's request.
        "physics" => {
            "grab, push and throw players and vehicles, and spawn and put away its own kinds of vehicles"
        }
        // Presentation only: sounds in the world or to one player, beams,
        // and animations on players. Nothing here changes the game.
        "effects" => "play sounds and show effects",
        // Switch the map's lights off and on, dim them or change their
        // colour, for everyone, until the map changes.
        "lighting" => "switch, dim and recolour the map's lights",
        // The sun, light, fog, sky and time of day, for everyone, until
        // the map changes.
        "environment" => "change the sun, sky, fog and time of day",
        // Teams, scores and round resets in mini-games (Slayer's team
        // games, Capture the Flag).
        "minigame" => "set up teams in mini-games, keep score and reset rounds",
        // Its own wrench event inputs and outputs (Capture the Flag's
        // onFlagPickedUp, Slayer's setTeamControl): the rows builders wired
        // to them run as those builders' own.
        "brick_events" => "add its own wrench events and run the rows builders wire to them",
        // Bots of an enabled bot Add-On's kinds that play in mini-games
        // (Slayer's Preferred Player Count): added, rested and removed.
        "bots" => "add bots to mini-games and take them away again",
        // Small values kept on the host between games and restarts
        // (Slayer's saved configs and its server-wide settings).
        "storage" => "keep its own settings and saved games on the host",
        _ => return None,
    })
}

/// The capability an earlier name became, for a clear refusal of old
/// manifests (alpha keeps no aliases).
pub fn renamed(name: &str) -> Option<&'static str> {
    match name {
        "sound" => Some("effects"),
        _ => None,
    }
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
        let now = renamed("sound").unwrap();
        assert!(CAPABILITIES.contains(&now) && !CAPABILITIES.contains(&"sound"));
        assert!(renamed("effects").is_none());
    }
}
