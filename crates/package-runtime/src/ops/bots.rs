//! Operations behind the `bots` capability.
use super::*;

/// Put a bot in a mini-game (Slayer's `addBotToGame`): a player body
/// without a connection, of a bot `kind` an enabled Add-On provides
/// (`bot_kinds()`), playing with the engine's bot brain. It joins
/// `game`, and `team` when given, and spawns as a member does; the
/// rules hear it join as any member. It is this package's until
/// `RemoveBot` or its game ends, and counts toward the server's bot
/// limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddBot {
    pub game: u64,
    pub team: Option<u64>,
    pub kind: String,
    pub name: String,
}
impl ScriptOp for AddBot {
    const CAPABILITY: &str = "bots";
    const NAME: &str = "add_bot";
    fn bounded(&self) -> bool {
        let AddBot { kind, name, .. } = self;
        !kind.is_empty()
            && kind.len() <= 96
            && kind
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._:/-".contains(c))
            && !name.trim().is_empty()
            && name.chars().count() <= MAX_BOT_NAME_CHARS
            && !name.chars().any(char::is_control)
    }
}

/// Take away a bot this package added.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoveBot {
    pub bot: u64,
}
impl ScriptOp for RemoveBot {
    const CAPABILITY: &str = "bots";
    const NAME: &str = "remove_bot";
    fn bounded(&self) -> bool {
        true
    }
}

/// Stop or restart the brain of a bot this package added (Slayer's
/// `stopHoleLoop` and `resetHoleLoop`): a resting bot stands still and
/// holds its fire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestBot {
    pub bot: u64,
    pub rest: bool,
}
impl ScriptOp for RestBot {
    const CAPABILITY: &str = "bots";
    const NAME: &str = "rest_bot";
    fn bounded(&self) -> bool {
        true
    }
}

/// Put a tool slot in the hand of a bot this package added, or put its
/// tools away with `None` (`AiPlayer::setWeapon`, Slayer's
/// `useRandomTool`). Its brain fights with what it holds, arming
/// itself only when its hand holds no weapon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BotTool {
    pub bot: u64,
    pub slot: Option<u8>,
}
impl ScriptOp for BotTool {
    const CAPABILITY: &str = "bots";
    const NAME: &str = "bot_tool";
    fn bounded(&self) -> bool {
        let BotTool { slot, .. } = self;
        slot.is_none_or(|s| usize::from(s) < MAX_TOOL_SLOTS)
    }
}
