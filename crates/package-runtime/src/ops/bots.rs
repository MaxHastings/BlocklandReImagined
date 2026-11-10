//! Operations behind the `bots` capability.
use super::*;

/// Put a bot in a mini-game (Slayer's `addBotToGame`): a player body
/// without a connection, of a bot `kind` an enabled Add-On provides
/// (`bot_kinds()`), playing with the engine's bot brain. It joins
/// `game`, and `team` when given, and spawns as a member does; the
/// rules hear it join as any member. It is this package's until
/// `RemoveBot` or its game ends, and counts toward the server's bot
/// limit. When `muted`, it is born already exempt from NPC reaction
/// Add-Ons (the flag those check before cueing a line or sound for it,
/// as with `set_bot_voice_muted`) — so no spawn-time hook can queue one
/// before its rules have run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddBot {
    pub game: u64,
    pub team: Option<u64>,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub muted: bool,
    /// Suppress stock join/spawn presentation for this spawn only.
    #[serde(default)]
    pub quiet: bool,
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

/// Stop or restart a bot this package added, or a bot kind this package
/// provides (including its declared host-rule companion). This supports
/// `stopHoleLoop` and `resetHoleLoop` without granting deletion of another
/// package's bot. A resting bot stands still and holds its fire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestBot {
    pub bot: u64,
    pub rest: bool,
    /// Explicitly execute anchored intents while native thinking is suspended.
    #[serde(default)]
    pub locomotion: bool,
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

/// Hang a cloud at `at` that bots cannot see through for `seconds`
/// (smoke): a bot whose line of sight to anything passes inside it sees
/// nothing along that line and fights from what it last knew.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Obscure {
    pub at: [f32; 3],
    pub radius: f32,
    pub seconds: f32,
}
impl ScriptOp for Obscure {
    const CAPABILITY: &str = "bots";
    const NAME: &str = "obscure";
    fn bounded(&self) -> bool {
        let Obscure { at, radius, seconds } = self;
        finite(at)
            && (0.1..=64.0).contains(radius)
            && seconds.is_finite()
            && (0.0..=MAX_OBSCURE_SECONDS).contains(seconds)
    }
}

/// Longest a cloud hangs, in seconds.
pub const MAX_OBSCURE_SECONDS: f32 = 120.0;

/// Dazzle a bot (a flash): it sees nothing for `seconds`, the way a
/// person in a white-out does, while its other senses go on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DazeBot {
    pub bot: u64,
    pub seconds: f32,
}
impl ScriptOp for DazeBot {
    const CAPABILITY: &str = "bots";
    const NAME: &str = "daze_bot";
    fn bounded(&self) -> bool {
        self.seconds.is_finite() && (0.0..=MAX_OBSCURE_SECONDS).contains(&self.seconds)
    }
}

/// Mark a bot this package added, or one of a kind it provides, as muted
/// for NPC reaction Add-Ons (`set_bot_voice_muted`): they check the flag
/// before cueing a voice line or sound for it (the pop-culture lines,
/// spawn and death cues). The body itself is not silenced: its own sounds
/// and chat go on. `false` unmutes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetBotVoiceMute {
    pub bot: u64,
    pub muted: bool,
}
impl ScriptOp for SetBotVoiceMute {
    const CAPABILITY: &str = "bots";
    const NAME: &str = "set_bot_voice_muted";
    fn bounded(&self) -> bool {
        true
    }
}

/// Most points a patrol visits.
pub const MAX_PATROL_POINTS: usize = 16;

/// What a bot's rules want of it, carried out by the shared brain
/// (`bot_intent`): the brain walks, jumps, rides and fights; the intent
/// only says where it keeps to and when it runs. A fight still comes
/// first, within the intent's bounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum BotIntent {
    /// Keep to `at`: stroll and fight within `radius` of it, and go back.
    Guard { at: [f32; 3], radius: f32 },
    /// Walk from point to point in turn, round and round, fighting what
    /// comes within `radius` of where it is on the round.
    Patrol { points: Vec<[f32; 3]>, radius: f32 },
    /// Keep within `distance` of a player or bot, fighting near them.
    Follow { leader: u64, distance: f32 },
    /// Run from `from` (or from every enemy it knows, with none) for
    /// `seconds`.
    Flee { from: Option<u64>, seconds: f32 },
}
impl BotIntent {
    fn bounded(&self) -> bool {
        match self {
            BotIntent::Guard { at, radius } => finite(at) && (0.5..=256.0).contains(radius),
            BotIntent::Patrol { points, radius } => {
                (1..=MAX_PATROL_POINTS).contains(&points.len())
                    && points.iter().all(|p| finite(p))
                    && (0.5..=256.0).contains(radius)
            }
            BotIntent::Follow { distance, .. } => (0.5..=64.0).contains(distance),
            BotIntent::Flee { seconds, .. } => seconds.is_finite() && (0.0..=600.0).contains(seconds),
        }
    }
}

/// Give a bot this package added, or one of a kind it provides, an
/// intent, or take its intent away with `None`: what it does when it is
/// not fighting, and when it runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetBotIntent {
    pub bot: u64,
    pub intent: Option<BotIntent>,
}
impl ScriptOp for SetBotIntent {
    const CAPABILITY: &str = "bots";
    const NAME: &str = "bot_intent";
    fn bounded(&self) -> bool {
        self.intent.as_ref().is_none_or(BotIntent::bounded)
    }
}

/// What a held-still bot keeps turning toward (`set_bot_facing`),
/// re-resolved by the engine each tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FacingTarget {
    /// A loose object to track as it moves: `"player:N"`, `"vehicle:N"`
    /// or `"entity:N"`. When it is gone (destroyed, removed, left), the
    /// facing goes with it and the bot keeps its last heading.
    Object(ObjectRef),
    /// A fixed point in the world.
    Point([f32; 3]),
}

/// Hold a rested bot's facing (`set_bot_facing`): while the rules hold it
/// still, it keeps turning toward `target`, re-resolved each tick for a
/// moving object, instead of frozen where it stood when it rested. `None`
/// releases the hold: it keeps its current heading. A free brain turns on
/// its own; this only steers one held by `rest_bot`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetBotFacing {
    pub bot: u64,
    pub target: Option<FacingTarget>,
}
impl ScriptOp for SetBotFacing {
    const CAPABILITY: &str = "bots";
    const NAME: &str = "set_bot_facing";
    fn bounded(&self) -> bool {
        self.target.as_ref().is_none_or(|t| match t {
            FacingTarget::Object(_) => true,
            FacingTarget::Point(at) => finite(at),
        })
    }
}
