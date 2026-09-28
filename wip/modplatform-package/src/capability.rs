//! Named capabilities a package's server behaviour may request. The server
//! owner sees the plain-language line for each when enabling a package, and
//! the behaviour host refuses any call whose capability was not declared.

pub const CAPABILITIES: &[(&str, &str)] = &[
    ("chat.read", "read chat messages"),
    ("chat.send", "send chat messages to players"),
    ("players.read", "see who is online and where they are"),
    ("players.move", "teleport players"),
    ("players.health", "damage, heal or kill players"),
    ("world.read", "read bricks in the world"),
    ("world.build", "place and remove its own bricks"),
    ("commands", "add chat /commands"),
    ("storage", "keep its own saved data on the server"),
];

pub fn describe(name: &str) -> Option<&'static str> {
    CAPABILITIES
        .iter()
        .find(|(capability, _)| *capability == name)
        .map(|(_, text)| *text)
}

pub fn names() -> Vec<&'static str> {
    CAPABILITIES.iter().map(|(name, _)| *name).collect()
}
