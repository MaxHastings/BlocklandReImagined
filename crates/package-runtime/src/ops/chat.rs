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

/// A line on a HUD message feed (`hud_line`) for `seconds`: one player's,
/// or everyone's when `player` is `None`. A layout's `message_feed` element
/// of the same `feed` name draws it, with the optional `color` and one of its
/// `icons` by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudLine {
    pub player: Option<u64>,
    pub feed: String,
    pub text: String,
    pub seconds: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}
impl ScriptOp for HudLine {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "hud_line";
    fn bounded(&self) -> bool {
        let HudLine {
            feed,
            text,
            seconds,
            color,
            icon,
            ..
        } = self;
        let name = |n: &str| {
            !n.is_empty() && n.len() <= 32 && !n.chars().any(|c| c.is_control())
        };
        feed.len() <= 32
            && feed
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
            && !feed.is_empty()
            && text.chars().count() <= MAX_PRINT_CHARS
            && !text.chars().any(char::is_control)
            && seconds.is_finite()
            && (0.5..=60.0).contains(seconds)
            && color
                .iter()
                .all(|c| c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)))
            && icon.as_deref().is_none_or(name)
    }
}

/// The screen tinted `color` (RGBA 0..1) for one player, or everyone with
/// no player, fading to nothing over `seconds`: a shield breaking, a hit
/// taken, a power-up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenFlash {
    pub player: Option<u64>,
    pub color: [f32; 4],
    pub seconds: f32,
}
impl ScriptOp for ScreenFlash {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "screen_flash";
    fn bounded(&self) -> bool {
        self.color
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && self.seconds.is_finite()
            && (0.05..=10.0).contains(&self.seconds)
    }
}

/// A full-screen overlay for one player, or everyone with no player, that
/// follows a curve of seconds (`bri_content::screen_overlay::Overlay`): light
/// while its value is above zero, dark below. Overlays running together add.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenOverlay {
    pub player: Option<u64>,
    pub overlay: bri_content::screen_overlay::Overlay,
}
impl ScriptOp for ScreenOverlay {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "screen_overlay";
    fn bounded(&self) -> bool {
        self.overlay.bounded()
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

/// Show `player` a panel ([`crate::panel::Panel`]), in place of any panel
/// they have open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowPanel {
    pub player: u64,
    pub panel: Box<crate::panel::Panel>,
}
impl ScriptOp for ShowPanel {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "show_panel";
    fn bounded(&self) -> bool {
        self.panel.is_bounded()
    }
}

/// Close `player`'s panel `id`, when it is this Add-On's and open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClosePanel {
    pub player: u64,
    pub id: String,
}
impl ScriptOp for ClosePanel {
    const CAPABILITY: &str = "chat";
    const NAME: &str = "close_panel";
    fn bounded(&self) -> bool {
        crate::panel::is_panel_id(&self.id)
    }
}
