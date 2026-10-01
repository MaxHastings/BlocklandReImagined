//! Operations behind the `brick_events` capability.
use super::*;

/// Keep `value` on a brick as this package's `key`, or clear it with
/// `None`: a v20 script's dynamic field on a brick (Slayer's
/// `isLocked[color]`). Every package reads it with `brick_field`; it
/// goes with the brick. Keys are 1 to [`MAX_BRICK_FIELD_KEY`] letters,
/// digits or `_`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetBrickField {
    pub brick: u64,
    pub key: String,
    pub value: Option<serde_json::Value>,
}
impl ScriptOp for SetBrickField {
    const CAPABILITY: &str = "brick_events";
    const NAME: &str = "set_brick_field";
    fn bounded(&self) -> bool {
        let SetBrickField { key, .. } = self;
        is_brick_field_key(key)
    }
}

/// Fire one of this package's wrench event inputs on a brick
/// (`processInputEvent`): the rows its builder wired to it run, as
/// theirs. `player` fills the Player, Client and MiniGame targets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FireBrickInput {
    pub brick: u64,
    pub input: String,
    pub player: Option<u64>,
}
impl ScriptOp for FireBrickInput {
    const CAPABILITY: &str = "brick_events";
    const NAME: &str = "fire_brick_input";
    fn bounded(&self) -> bool {
        let FireBrickInput { input, .. } = self;
        input.len() <= 64
    }
}

/// Fire one of this package's inputs on every brick of mini-game `game`
/// wired to it (Slayer's `processMultiSourceInputEvent`:
/// `onMinigameDeath`, `onMinigameRoundStart`). `player` fills the
/// Player and Client targets, `killer` the `Player(Killer)` and
/// `Client(Killer)` ones, and the game the MiniGame target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FireGameInput {
    pub game: u64,
    pub input: String,
    pub player: Option<u64>,
    pub killer: Option<u64>,
}
impl ScriptOp for FireGameInput {
    const CAPABILITY: &str = "brick_events";
    const NAME: &str = "fire_game_input";
    fn bounded(&self) -> bool {
        let FireGameInput { input, .. } = self;
        input.len() <= 64
    }
}
