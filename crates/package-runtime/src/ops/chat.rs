//! Operations behind the `chat` capability.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tell {
    pub player: u64,
    pub text: String,
}
impl ScriptOp for Tell {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "tell";
    fn bounded(&self) -> bool {
        let Tell { text, .. } = self;
        chat(text)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Broadcast {
    pub text: String,
}
impl ScriptOp for Broadcast {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "broadcast";
    fn bounded(&self) -> bool {
        let Broadcast { text } = self;
        chat(text)
    }
}

/// A chat line to every member of a mini-game (`MiniGameSO::messageAll`).
/// One line of the package's chat share, however many members;
/// `except` leaves one member out (`messageAllExcept`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TellMinigame {
    pub game: u64,
    pub text: String,
    pub except: Option<u64>,
}
impl ScriptOp for TellMinigame {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "tell_minigame";
    fn bounded(&self) -> bool {
        let TellMinigame { text, .. } = self;
        chat(text)
    }
}

/// One chat line to each of some players (Slayer's dead-only and
/// team-only messages): one line of the share.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TellPlayers {
    pub players: Vec<u64>,
    pub text: String,
}
impl ScriptOp for TellPlayers {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "tell_players";
    fn bounded(&self) -> bool {
        let TellPlayers { players, text } = self;
        players.len() <= MAX_TELL_PLAYERS && chat(text)
    }
}

/// A center or bottom print to every member of a mini-game
/// (`centerPrintAll`, `bottomPrintAll`): one print of the share.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrintMinigame {
    pub game: u64,
    pub text: String,
    pub seconds: f32,
    pub bottom: bool,
}
impl ScriptOp for PrintMinigame {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "bottom_print_minigame";
    fn bounded(&self) -> bool {
        let PrintMinigame { text, seconds, .. } = self;
        text.chars().count() <= MAX_PRINT_CHARS
            && !text.chars().any(|c| c.is_control() && c != '\n')
            && seconds.is_finite()
            && (0.0..=600.0).contains(seconds)
    }
}

/// Text in the middle of the screen (`centerPrint`), or above the
/// bottom edge (`bottomPrint`), for `seconds`: one player's, or
/// everyone's when `player` is `None`. Empty text clears it.
/// `hide_bar` hides the bottom print's bar (`bottomPrint`'s third
/// argument).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Print {
    pub player: Option<u64>,
    pub text: String,
    pub seconds: f32,
    pub bottom: bool,
    #[serde(default)]
    pub hide_bar: bool,
}
impl ScriptOp for Print {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "bottom_print";
    fn bounded(&self) -> bool {
        let Print { text, seconds, .. } = self;
        text.chars().count() <= MAX_PRINT_CHARS
            && !text.chars().any(|c| c.is_control() && c != '\n')
            && seconds.is_finite()
            && (0.0..=600.0).contains(seconds)
    }
}

/// One player's plant-error icon and sound, as v20's
/// `messageClient(%client, 'MsgPlantError_…')`: one of
/// [`PLANT_ERRORS`]. `flood` (planting too soon) shows what the
/// engine's own plant rate shows when a player plants too fast.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlantError {
    pub player: u64,
    pub error: String,
}
impl ScriptOp for PlantError {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "plant_error";
    fn bounded(&self) -> bool {
        let PlantError { error, .. } = self;
        PLANT_ERRORS.contains(&error.as_str())
    }
}

/// Tell one player something in a message box they close with OK
/// (v20's `MessageBoxOK` from the server).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageBox {
    pub player: u64,
    pub title: String,
    pub text: String,
}
impl ScriptOp for MessageBox {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "message_box";
    fn bounded(&self) -> bool {
        let MessageBox { title, text, .. } = self;
        title.chars().count() <= 64
            && text.chars().count() <= MAX_PRINT_CHARS
            && ![title, text]
                .iter()
                .any(|t| t.chars().any(|c| c.is_control() && c != '\n'))
    }
}

/// Ask one player a yes or no question (v20's `MessageBoxYesNo` from
/// the server): yes sends the package's own `command`, which takes no
/// arguments, as if they had typed it; no does nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ask {
    pub player: u64,
    pub title: String,
    pub text: String,
    pub command: String,
}
impl ScriptOp for Ask {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "ask";
    fn bounded(&self) -> bool {
        let Ask {
            title,
            text,
            command,
            ..
        } = self;
        title.chars().count() <= 64
            && text.chars().count() <= MAX_PRINT_CHARS
            && ![title, text]
                .iter()
                .any(|t| t.chars().any(|c| c.is_control() && c != '\n'))
            && !command.is_empty()
            && command.len() <= 64
            && command
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    }
}

/// Show `player` a score report in its own window (Slayer's End of
/// Round Report), with the columns Add-Ons changed for their game; or
/// close it with `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowReport {
    pub player: u64,
    pub report: Option<Box<crate::report::Report>>,
}
impl ScriptOp for ShowReport {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "hide_report";
    fn bounded(&self) -> bool {
        let ShowReport { report, .. } = self;
        report.as_ref().is_none_or(|r| r.is_bounded())
    }
}
