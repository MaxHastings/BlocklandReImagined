//! The UI runtime: a Torque-style canvas (one content screen plus a dialog
//! stack), input routing, key binds, view-model updates and pending
//! requests. See `api.rs` for the host contract.

use crate::api::*;
use crate::binds::{BindMap, Platform, Repeater};
use crate::draw::DrawList;
use crate::geom::Rect;
use crate::input::{Chord, InputEvent, Key, Modifiers, MouseButton};
use crate::models::chat::ChatModel;
use crate::models::hud::{HudModel, HudPrefs, InvKey, Outbox};
use crate::models::selector::SelectorModel;
use crate::models::wrench::WrenchState;
use crate::pack::Pack;
use crate::prefs::Prefs;
use crate::screens::{self, Screen, ScreenId};
use crate::view::{EventKind, ViewEvent};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

/// Listener-local cosmetic audio. This is not a gameplay request and has no
/// request ID, acknowledgement or permission round trip. Names are original
/// profiles plus the semantic keys in the native audio bank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiSound {
    pub profile: &'static str,
    pub trigger: &'static str,
}

const MAX_QUEUED_SOUNDS: usize = 128;
/// Most scrollInventory steps one wheel event may take (a fast free spin).
const NUM_WHEEL_STEPS: usize = 10;
const AUDIO_ERROR: UiSound = UiSound {
    profile: "AudioError",
    trigger: "ui.error",
};

fn menu_hover_sound(screen: ScreenId, name: &str) -> Option<UiSound> {
    // allClientScripts-Vanilla.cs:9328–9388 and14226–14294. These are
    // onMouseEnter notes, not generic click sounds.
    let note = match (screen, name) {
        (ScreenId::MainMenu, "MM_TutorialButton") => 3,
        (ScreenId::MainMenu, "MM_StartButton") => 4,
        (ScreenId::MainMenu, "MM_JoinButton") => 5,
        (ScreenId::MainMenu, "MM_PlayerButton") => 6,
        (ScreenId::MainMenu, "MM_OptionsButton") => 7,
        (ScreenId::MainMenu, "MM_DemoButton") => 8,
        (ScreenId::MainMenu, "MM_QuitButton") => 0,
        (ScreenId::MainMenu, "MM_AboutButton") => 1,
        (ScreenId::MainMenu, "MM_CreditsButton") => 2,
        (ScreenId::EscapeMenu, "EM_Options") => 0,
        (ScreenId::EscapeMenu, "EM_PlayerList") => 1,
        (ScreenId::EscapeMenu, "EM_MiniGames") => 2,
        (ScreenId::EscapeMenu, "EM_AdminMenu") => 3,
        (ScreenId::EscapeMenu, "EM_SaveBricks") => 4,
        (ScreenId::EscapeMenu, "EM_LoadBricks") => 5,
        (ScreenId::EscapeMenu, "EM_Disconnect") => 6,
        (ScreenId::EscapeMenu, "EM_Quit") => 7,
        _ => return None,
    };
    let (profile, trigger) = [
        ("Note0Sound", "ui.menu_note.0"),
        ("Note1Sound", "ui.menu_note.1"),
        ("Note2Sound", "ui.menu_note.2"),
        ("Note3Sound", "ui.menu_note.3"),
        ("Note4Sound", "ui.menu_note.4"),
        ("Note5Sound", "ui.menu_note.5"),
        ("Note6Sound", "ui.menu_note.6"),
        ("Note7Sound", "ui.menu_note.7"),
        ("Note8Sound", "ui.menu_note.8"),
    ][note];
    Some(UiSound { profile, trigger })
}

/// Not a v20 setting: the interface size in percent (0 = automatic, the
/// largest whole scale that keeps 640x480 logical pixels).
pub const UI_SCALE: &str = "$pref::Gui::Scale";
/// The scale `$pref::Gui::Scale` asks for in a window of `size`, never so
/// large that fewer than 640x480 logical pixels remain (the layouts' floor).
pub fn preferred_scale(prefs: &crate::prefs::Prefs, size: (u32, u32)) -> Option<f32> {
    let percent = prefs.i64_or(UI_SCALE, 0);
    if percent <= 0 {
        return None;
    }
    let fit = (size.0 as f32 / 640.0).min(size.1 as f32 / 480.0).max(1.0);
    Some((percent as f32 / 100.0).clamp(1.0, fit))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiConfig {
    /// Window size in physical pixels.
    pub size: (u32, u32),
    /// Logical→physical scale. `None` = automatic: the largest integer
    /// scale that keeps at least 640x480 logical pixels (Torque's minimum
    /// canvas), so pixel art and bitmap fonts stay crisp. A window smaller
    /// than that scales down to fit it, so no dialog is cut off.
    pub scale: Option<f32>,
    pub platform: Platform,
}

impl UiConfig {
    pub fn effective_scale(&self) -> f32 {
        match self.scale {
            Some(s) if s.is_finite() && s > 0.0 => s.clamp(0.5, 8.0),
            _ => {
                let fit = (self.size.0 as f32 / 640.0).min(self.size.1 as f32 / 480.0);
                if fit >= 1.0 {
                    fit.floor()
                } else {
                    fit.max(0.5)
                }
            }
        }
    }
    pub fn logical(&self) -> (i32, i32) {
        let s = self.effective_scale();
        (
            (self.size.0 as f32 / s).floor() as i32,
            (self.size.1 as f32 / s).floor() as i32,
        )
    }
}

/// Deferred screen-stack edits (applied after the current handler).
#[derive(Debug, Clone, PartialEq)]
pub enum StackCmd {
    SetContent(ScreenId),
    Push(ScreenId),
    Pop(ScreenId),
    /// Show a message box (OK or Yes/No) with a callback.
    Message(Box<MessageBox>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageBox {
    pub title: String,
    pub text: String,
    pub yes_no: bool,
    pub on_yes: Callback,
    /// What NO (or Escape) does.
    pub on_no: Callback,
    /// Labels for YES and NO, when "Yes" and "No" would not say it.
    pub buttons: Option<[String; 2]>,
}

/// The web address a link opens, as `gotoWebPage` does: `http://` is added
/// when the link has no scheme, and only web pages open.
pub fn web_url(link: &str) -> Option<String> {
    let link = link.trim();
    let lower = link.to_ascii_lowercase();
    let url = if lower.starts_with("http://") || lower.starts_with("https://") {
        link.to_string()
    } else if link.contains("://") || lower.starts_with("javascript:") || lower.starts_with("file:") {
        return None;
    } else {
        format!("http://{link}")
    };
    (url.len() <= 256
        && url.len() > "http://".len()
        && url.bytes().all(|b| b.is_ascii_graphic() && !b"<>\"\\`".contains(&b)))
    .then_some(url)
}

/// What a message box's YES/OK does (v20 passed script strings).
#[derive(Debug, Clone, PartialEq)]
pub enum Callback {
    None,
    Quit,
    Disconnect,
    RemapForce {
        command: String,
        input: BindInput,
    },
    ClearBinds,
    DefaultBinds,
    OverwriteSave {
        name: String,
        description: String,
        events: bool,
        ownership: bool,
    },
    CloseEvents,
    MiniGame {
        game: crate::api::MiniGameId,
        operation: crate::api::MiniGameOperation,
    },
    /// `TrustInviteGui.ignore()`.
    IgnoreTrust {
        from: u64,
    },
    /// Turn a package on or off once the player confirmed what else changes.
    AddOn {
        id: String,
        enabled: bool,
    },
    /// Turn off every add-on outside the base game.
    DefaultAddOns,
    /// Send this request (a platform question answered YES).
    Request(Box<crate::api::UiAction>),
    /// Open a web page (a new release's download page).
    OpenUrl(String),
    /// Open a screen (the first-run name prompt opens Avatar).
    Push(ScreenId),
    /// First run: ask for a name once the Tutorial question is answered.
    NamePrompt,
    /// First run: play the Tutorial, then ask for a name back at the menu.
    TutorialThenName,
}

/// First run's name question: "after_tutorial" while it waits for the
/// player to come back from the Tutorial, "done" once asked.
pub const NAME_PROMPT: &str = "$pref::Player::NamePrompt";

/// How long a sound caption stays, and how many show at once.
const CAPTION_MS: u64 = 3000;
const MAX_CAPTIONS: usize = 4;
/// Keyboard look commands: (lowercase command, yaw sign, pitch sign). Pitch
/// follows mouse Y, so positive looks down.
const KEYBOARD_TURN: [(&str, f32, f32); 4] = [
    ("turnleft", -1.0, 0.0),
    ("turnright", 1.0, 0.0),
    ("panup", 0.0, -1.0),
    ("pandown", 0.0, 1.0),
];
/// Radians per second at `KeyboardTurnSpeed` 1.0. v20's getNextMove adds
/// the `$mvYaw*Speed`/`$mvPitch*Speed` values to every 32 ms move
/// (blocklandv20.exe 0x59571e), so the default 0.5 turns at 15.6 rad/s.
const KEYBOARD_TURN_RATE: f32 = 1.0 / 0.032;

/// What kind of answer a pending request is waiting for.
#[derive(Debug, Clone, PartialEq)]
pub enum Pending {
    Buy,
    Wrench,
    Events,
    Avatar,
    Save,
    Load,
    Print,
    Other,
    MiniGame(crate::api::MiniGameOperation),
}

impl Pending {
    /// How long a request may wait for its answer before the UI gives up on
    /// it. Every request the host receives is answered or timed out by the
    /// network worker well within this, so reaching it means a request was
    /// lost on the way; the screen must not wait on it forever.
    pub fn timeout_ms(&self) -> u64 {
        match self {
            // Writing or reading a whole build goes through the disk.
            Pending::Save | Pending::Load => 180_000,
            _ => 45_000,
        }
    }
}

/// The answer a request gets when nothing answered it in time.
pub const REQUEST_TIMED_OUT: &str =
    "The game did not answer this request in time. Please try again.";

/// Remembered ids of requests given up on, so a late answer is dropped
/// instead of reaching a screen that has moved on.
const ABANDONED_MEMORY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum HeldInput {
    Key(Key),
    Mouse(MouseButton),
}

/// A look edited in the avatar editor for another screen ([`Core::avatar_value`]).
#[derive(Debug, Clone, PartialEq)]
pub struct AvatarValue {
    /// The window's title (`Edit Uniform: Red Team`).
    pub title: String,
    pub look: crate::api::AvatarPrefs,
    /// What Reset To Default puts back.
    pub default: crate::api::AvatarPrefs,
    /// Which of the asking screen's looks it is.
    pub key: String,
    /// The look as Done left it, for the asking screen to take.
    pub done: Option<crate::api::AvatarPrefs>,
}

/// Everything screens can read and change. Owned by [`Ui`].
pub struct Core {
    pub pack: Rc<Pack>,
    pub platform: Platform,
    pub logical: (i32, i32),
    pub time_ms: u64,
    pub settings: Settings,
    pub prefs: Prefs,
    pub binds: BindMap,
    pub globals: BindMap,
    /// The Options remap list ([`crate::binds::remap_entries`]).
    pub remap: Vec<crate::schema::RemapEntry>,
    pub remap_commands: Vec<String>,
    pub remap_target: Option<usize>,
    pub remap_all: bool,
    pub options_open: bool,
    /// Options opens on the pane holding this section (an Add-On's
    /// Options key), once.
    pub options_section: Option<String>,
    /// HelpDlg is open (F1 closes it again).
    pub help_open: bool,
    /// The name question was put this run (at most once).
    pub name_asked: bool,
    /// The page `getHelp` asked HelpDlg to open on.
    pub help_page: Option<String>,
    /// A look the avatar editor edits for another screen instead of the
    /// player's own (a team uniform in Add-On Settings).
    pub avatar_value: Option<AvatarValue>,
    /// The splash the Splash screen shows.
    pub splash: Option<crate::api::SplashView>,
    /// Help pages of the server's running Add-Ons, listed after the
    /// game's own.
    pub addon_help: Vec<crate::api::AddOnHelpPage>,
    /// The report the host last showed (the Report window's).
    pub report: Option<crate::api::ReportView>,
    pub print_letters_visible: bool,
    // catalogs
    pub maps: Vec<MapInfo>,
    /// Start Game's game modes, from the enabled Add-Ons.
    pub game_modes: Vec<crate::api::GameModeInfo>,
    pub servers: Vec<ServerInfo>,
    pub lan_querying: bool,
    pub bricks: Vec<BrickInfo>,
    pub prints: BTreeMap<String, Vec<PrintInfo>>,
    pub events: EventCatalog,
    pub datablocks: DatablockMenus,
    pub menu_backgrounds: Vec<IconRef>,
    /// Music loop names for Start Game's Music Files.
    pub music_tracks: Vec<String>,
    /// Whether the save waiting in `LoadBricksColorGui` can add its colours.
    pub color_append_fits: bool,
    pub display_modes: Option<crate::api::DisplayModes>,
    pub avatar_preview: IconRef,
    pub save_maps: Vec<String>,
    pub save_files: Vec<SaveFileInfo>,
    pub save_context: Option<(String, IconRef)>,
    /// The last save picture the host sent: (map, file name, picture).
    pub save_preview: Option<(String, String, IconRef)>,
    /// Installed packages for the Add-Ons screen (host-prepared text).
    pub add_ons: crate::api::AddOnsView,
    /// Differing add-ons behind the last refused join (Can't Join dialog).
    pub add_on_mismatch: Option<crate::api::AddOnMismatch>,
    /// See `UiUpdate::FailureQuestion`.
    pub failure_question: Option<crate::api::Question>,
    /// See `UiUpdate::UnsavedChanges`.
    pub unsaved_changes: bool,
    /// This build's version (`UiUpdate::Version`).
    pub version: String,
    /// A newer release's name and page, once one is found.
    pub newer_version: Option<(String, String)>,
    // live state
    pub conn: ConnectionState,
    pub hud: HudModel,
    pub chat: ChatModel,
    /// Who is typing, shown above the chat (`chatWhosTalkingText`).
    pub talking: Vec<String>,
    pub selector: SelectorModel,
    pub wrench: WrenchState,
    pub players: Vec<PlayerRow>,
    pub admin: crate::models::admin::AdminModel,
    /// The host's environment and the Environment window's draft.
    pub environment: crate::models::environment::EnvironmentModel,
    pub minigames: MiniGameUiState,
    /// The mini-game whose Add-On Settings window is open (or opening).
    pub minigame_addons: Option<MiniGameId>,
    /// The Add-On Settings window shows the server-wide settings instead
    /// (opened from the Admin menu).
    pub server_addon_settings: bool,
    /// Open `TrustInviteGui` invitation.
    /// Open trust invitations, newest last, one per sender, like mini-game
    /// invitations: the dialog shows the newest and Escape leaves them open.
    pub trust_invites: Vec<crate::api::TrustInvitation>,
    /// Other players' names this frame (`GuiShapeNameHud`).
    pub name_tags: Vec<crate::api::NameTag>,
    /// HUD panels of enabled packages this frame (the `hud.overlay` slot).
    pub package_panels: Vec<crate::api::PackagePanel>,
    /// Keys package HUDs bind to package commands. Base game binds win.
    pub package_keys: Vec<crate::api::PackageKey>,
    /// Enabled packages' binds, listed in Controls after the game's own
    /// ([`Ui::set_package_binds`]).
    pub package_binds: Vec<crate::api::PackageBind>,
    pub server_name: String,
    pub max_players: u32,
    pub center_print: Option<(String, Option<u64>)>,
    pub bottom_print: Option<(String, Option<u64>, bool)>,
    pub plant_error: Option<(PlantError, u64)>,
    /// Sound captions on screen and when each one goes.
    pub captions: Vec<(String, u64)>,
    /// Current damage flash opacity (0..=0.75), fading over time.
    pub damage_flash: f32,
    pub energy: Option<f32>,
    /// Current whiteout opacity (0..=1), fading over time.
    pub whiteout: f32,
    /// `GameRenderFilters`' liquid tints for the camera, drawn in order.
    pub underwater: Vec<[f32; 4]>,
    /// `NetGraphGui` while it is on the canvas (`toggleNetGraph`).
    pub net_graph: Option<crate::models::perf::NetGraph>,
    /// The performance overlay (not in v20).
    pub perf: crate::models::perf::PerfOverlay,
    pub lagging: bool,
    pub shape_names: bool,
    /// The camera is a player's or vehicle's first-person eye.
    pub first_person: bool,
    /// The held weapon hides the crosshair.
    pub hide_crosshair: bool,
    pub super_shift: bool,
    super_shift_time: u64,
    pub zoom_on: bool,
    /// The held tool takes the mouse wheel while its trigger is held (an
    /// image's `wheel` command): the wheel goes to it, not the inventory.
    /// Whether the trigger is held is `held_controls`, this UI's own.
    pub wheel_tool: bool,
    /// An Add-On's orbit camera zooms on the wheel, in place of the
    /// inventory.
    pub wheel_camera: bool,
    /// Aimed through a scope with steps: the wheel zooms, trigger or not.
    pub aim_wheel: bool,
    /// The scope picture drawn over the screen while aiming, if any.
    pub scope_overlay: Option<(u64, f32)>,
    pub cursor_forced: bool,
    /// Open print selector aspect ratio and last print per aspect.
    pub print_aspect: Option<String>,
    pub last_print: BTreeMap<String, String>,
    // requests
    next_id: RequestId,
    session_request: Option<RequestId>,
    out: Vec<(RequestId, UiAction)>,
    pub pending: BTreeMap<RequestId, Pending>,
    /// When each answer-awaiting request ([`Core::pending`] and the admin
    /// model's) is given up on, in [`Core::time_ms`].
    deadlines: BTreeMap<RequestId, u64>,
    /// Requests timed out or abandoned; their late answers are dropped.
    abandoned: std::collections::VecDeque<RequestId>,
    pub cmds: Vec<StackCmd>,
    repeater: Repeater,
    held: BTreeMap<HeldInput, String>,
    held_controls: BTreeSet<HeldControl>,
    pub console: screens::console::ConsoleState,
}

impl Core {
    /// Queue a request; returns its id.
    pub fn request(&mut self, a: UiAction) -> RequestId {
        let open_admin = matches!(a, UiAction::OpenAdmin);
        if open_admin {
            self.push(if self.admin.is_admin() {
                ScreenId::Admin
            } else {
                ScreenId::AdminLogin
            });
        }
        let starts = matches!(
            a,
            UiAction::HostGame { .. }
                | UiAction::JoinServer { .. }
                | UiAction::TrustNewServerIdentity { .. }
                | UiAction::StartTutorial
        );
        let stops = matches!(a, UiAction::CancelConnect | UiAction::Disconnect);
        if starts || stops {
            let was_loading = matches!(self.conn, ConnectionState::Loading { .. });
            self.reset_session();
            if stops {
                self.session_request = None;
                self.pop(ScreenId::Connecting);
                if was_loading || matches!(a, UiAction::Disconnect) {
                    self.set_content(ScreenId::MainMenu);
                }
            }
        }
        self.next_id += 1;
        let id = self.next_id;
        if starts {
            self.session_request = Some(id);
        }
        self.out.push((id, a));
        if open_admin {
            self.deadlines
                .insert(id, self.time_ms.saturating_add(Pending::Other.timeout_ms()));
            self.admin
                .pending
                .insert(id, crate::models::admin::AdminAction::Refresh);
            self.admin.status = "Checking administration permissions...".into();
        }
        id
    }
    pub fn admin_request(
        &mut self,
        action: crate::models::admin::AdminAction,
    ) -> Option<RequestId> {
        if self.admin.busy() {
            return None;
        }
        if !self.admin.allowed(&action) {
            self.admin.status = "This action is unavailable or no longer permitted.".into();
            return None;
        }
        let id = self.request_pending(UiAction::Admin(action.clone()), Pending::Other);
        self.admin.pending.insert(id, action);
        self.admin.status = "Waiting for host...".into();
        Some(id)
    }
    /// Drop state owned by the previous host. User settings and favorites remain.
    fn reset_session(&mut self) {
        self.release_all();
        self.conn = ConnectionState::Idle;
        self.hud = HudModel::new(self.hud_prefs());
        self.chat = ChatModel::new(
            self.chat.cache_lines,
            self.chat.max_lines,
            self.chat.line_time_ms,
        );
        self.talking.clear();
        self.selector.cart = [None; 10];
        self.selector.clicked_brick = None;
        self.selector.clicked_slot = None;
        self.selector.tab = 0;
        self.selector.setting_favs = false;
        self.bricks.clear();
        self.prints.clear();
        self.events = EventCatalog::default();
        self.datablocks.clear();
        self.wrench = WrenchState::default();
        self.players.clear();
        self.minigames = MiniGameUiState::default();
        self.report = None;
        self.admin = Default::default();
        self.environment = Default::default();
        self.server_name.clear();
        self.max_players = 0;
        self.center_print = None;
        self.bottom_print = None;
        self.plant_error = None;
        self.damage_flash = 0.0;
        self.energy = None;
        self.hide_crosshair = false;
        self.whiteout = 0.0;
        self.underwater.clear();
        self.lagging = false;
        self.super_shift = false;
        self.zoom_on = false;
        self.wheel_tool = false;
        self.wheel_camera = false;
        self.aim_wheel = false;
        self.scope_overlay = None;
        self.cursor_forced = false;
        self.print_aspect = None;
        self.last_print.clear();
        self.save_context = None;
        self.save_files.clear();
        self.save_maps.clear();
        self.pending.clear();
        self.deadlines.clear();
        self.console.reset_session();
    }
    pub fn request_pending(&mut self, a: UiAction, kind: Pending) -> RequestId {
        let id = self.request(a);
        self.deadlines
            .insert(id, self.time_ms.saturating_add(kind.timeout_ms()));
        self.pending.insert(id, kind);
        id
    }
    /// Stop waiting for `id`: the player backed out of the screen that
    /// asked. Its answer, if one still comes, is dropped.
    pub fn abandon(&mut self, id: RequestId) {
        self.pending.remove(&id);
        self.admin.pending.remove(&id);
        self.deadlines.remove(&id);
        self.remember_abandoned(id);
    }
    fn remember_abandoned(&mut self, id: RequestId) {
        if self.abandoned.len() >= ABANDONED_MEMORY {
            self.abandoned.pop_front();
        }
        self.abandoned.push_back(id);
    }
    /// Requests whose deadline passed while still unanswered. They are
    /// forgotten here and remembered as abandoned.
    fn take_overdue(&mut self) -> Vec<RequestId> {
        let now = self.time_ms;
        let overdue: Vec<RequestId> = self
            .deadlines
            .iter()
            .filter(|&(_, &at)| at <= now)
            .map(|(&id, _)| id)
            .collect();
        let mut out = Vec::new();
        for id in overdue {
            self.deadlines.remove(&id);
            if self.pending.contains_key(&id) || self.admin.pending.contains_key(&id) {
                out.push(id);
            }
        }
        out
    }
    /// Queue a minigame operation only when the host explicitly advertises
    /// it for this session. Caller identity is resolved by the host.
    pub fn minigame_request(
        &mut self,
        operation: MiniGameOperation,
        action: UiAction,
    ) -> Option<RequestId> {
        use crate::models::minigames::Operation as Op;
        // The game a managing request acts on: the one it names, else the
        // player's own.
        let game = match &action {
            UiAction::ConfigureMiniGame { game, .. }
            | UiAction::ResetMiniGame { game }
            | UiAction::EndMiniGame { game }
            | UiAction::RespawnMiniGameMembers { game }
            | UiAction::SetMiniGameTeam { game, .. }
            | UiAction::EditMiniGameAddOns { game, .. }
            | UiAction::RemoveMiniGameMember {
                game: Some(game), ..
            } => Some(*game),
            _ => self.minigames.active_game,
        };
        let allowed = self.minigames.can_on(match operation {
            MiniGameOperation::List => Op::List,
            MiniGameOperation::Create => Op::Create,
            MiniGameOperation::Configure => Op::Configure,
            MiniGameOperation::Join => Op::Join,
            MiniGameOperation::Leave => Op::Leave,
            MiniGameOperation::Invite => Op::Invite,
            MiniGameOperation::AcceptInvite => Op::AcceptInvite,
            MiniGameOperation::RejectInvite => Op::RejectInvite,
            MiniGameOperation::IgnoreInvite => Op::IgnoreInvite,
            MiniGameOperation::RemoveMember => Op::RemoveMember,
            MiniGameOperation::Reset => Op::Reset,
            MiniGameOperation::RespawnAll => Op::RespawnAll,
            MiniGameOperation::End => Op::End,
            MiniGameOperation::AddOnSettings => Op::AddOnSettings,
        }, game);
        if !allowed {
            self.minigames.status =
                "This mini-game action is unavailable or no longer permitted.".into();
            return None;
        }
        if self
            .pending
            .values()
            .any(|p| matches!(p, Pending::MiniGame(_)))
        {
            self.minigames.status = "Waiting for the host...".into();
            return None;
        }
        self.minigames.status = "Waiting for the host...".into();
        Some(self.request_pending(action, Pending::MiniGame(operation)))
    }
    pub fn is_pending(&self, kind: &Pending) -> bool {
        self.pending.values().any(|p| p == kind)
    }
    /// `getHelp(name)`: HelpDlg on that page.
    pub fn get_help(&mut self, page: Option<String>) {
        self.help_page = page;
        self.push(ScreenId::Help);
    }
    /// `contextHelp` (F1): close HelpDlg if it is open, else open it.
    pub fn context_help(&mut self) {
        if self.help_open {
            self.pop(ScreenId::Help);
        } else {
            self.get_help(None);
        }
    }
    pub fn game(&mut self, g: GameAction) {
        self.request(UiAction::Game(g));
    }
    pub fn push(&mut self, s: ScreenId) {
        self.cmds.push(StackCmd::Push(s));
    }
    pub fn pop(&mut self, s: ScreenId) {
        self.cmds.push(StackCmd::Pop(s));
    }
    pub fn set_content(&mut self, s: ScreenId) {
        self.cmds.push(StackCmd::SetContent(s));
    }
    pub fn message_ok(&mut self, title: &str, text: &str) {
        self.cmds.push(StackCmd::Message(Box::new(MessageBox {
            title: title.into(),
            text: text.into(),
            yes_no: false,
            on_yes: Callback::None,
            on_no: Callback::None,
            buttons: None,
        })));
    }
    /// Ask before leaving a hosted game whose world changed since it was last
    /// saved: leaving loses those changes.
    pub fn confirm_unsaved(&mut self, on_yes: Callback) {
        self.message_yes_no(
            "Unsaved Changes",
            "Your build has changes you haven't saved. Leaving loses them.\n\nLeave anyway?",
            on_yes,
        );
    }
    /// First run, after the controls question: offer the Tutorial, then
    /// the name. v20 had no such welcome; everyone started as "Blockhead".
    pub fn first_run_welcome(&mut self) {
        self.cmds.push(StackCmd::Message(Box::new(MessageBox {
            title: "Welcome to Blockland ReImagined".into(),
            text: "New here? The Tutorial teaches moving, building, tools and driving in a \
                   few minutes. You can also start it later from the main menu.\n\nPlay the \
                   Tutorial now?"
                .into(),
            yes_no: true,
            on_yes: Callback::TutorialThenName,
            on_no: Callback::NamePrompt,
            buttons: Some(["Play Tutorial".into(), "Not Now".into()]),
        })));
    }
    /// Ask once for a name when the player still has the default one: the
    /// one name question (`ChooseName`), whichever way the first run went.
    /// Choosing or skipping it there settles it for good
    /// (`screens::name::PROMPTED`).
    pub fn name_prompt(&mut self) {
        if self.prefs.str_or(NAME_PROMPT, "") != "done" {
            self.prefs.set(NAME_PROMPT, "done");
            self.save_settings();
        }
        if !self.name_asked && crate::screens::name::should_prompt(self) {
            self.name_asked = true;
            self.push(ScreenId::ChooseName);
        }
    }
    pub fn message_yes_no(&mut self, title: &str, text: &str, on_yes: Callback) {
        self.cmds.push(StackCmd::Message(Box::new(MessageBox {
            title: title.into(),
            text: text.into(),
            yes_no: true,
            on_yes,
            on_no: Callback::None,
            buttons: None,
        })));
    }
    /// A question with its own button labels; each answer sends its request.
    pub fn ask(&mut self, q: crate::api::Question) {
        self.cmds.push(StackCmd::Message(Box::new(MessageBox {
            title: q.title,
            text: q.text,
            yes_no: true,
            on_yes: Callback::Request(q.on_yes),
            on_no: q.on_no.map_or(Callback::None, Callback::Request),
            buttons: Some([q.yes, q.no]),
        })));
    }
    /// `strupr(getWord(moveMap.getBinding(cmd), 1))` as used in HUD tips.
    pub fn key_name(&self, command: &str) -> String {
        match self.binds.binding_of(command) {
            Some(BindInput::Key(c)) => c.key.torque_name().to_ascii_uppercase(),
            Some(_) => self.binds.display(command),
            None => String::new(),
        }
    }
    pub fn center_print(&mut self, text: &str, seconds: f32) {
        let until = (seconds > 0.0).then(|| self.time_ms + (seconds * 1000.0) as u64);
        self.center_print = Some((text.to_string(), until));
    }
    /// Persist settings through the host.
    pub fn save_settings(&mut self) {
        self.settings.prefs = self.prefs.overrides();
        self.settings.binds = Some(self.binds.entries.clone());
        self.settings.brick_favorites = self.selector.favorites.clone();
        let s = Box::new(self.settings.clone());
        self.request(UiAction::SaveSettings(s));
    }
    /// `NetGraph::toggleNetGraph`: add `NetGraphGui` to the canvas, or
    /// remove it (and its history).
    pub fn toggle_net_graph(&mut self) {
        self.net_graph = match self.net_graph {
            Some(_) => None,
            None => Some(Default::default()),
        };
    }
    pub fn in_game(&self) -> bool {
        matches!(self.conn, ConnectionState::InGame { .. })
    }
    pub fn is_local(&self) -> bool {
        matches!(self.conn, ConnectionState::InGame { local: true, .. })
    }
    /// The live administrator snapshot decides once it has arrived; the flag
    /// copied at entry only covers the moments before it.
    pub fn is_admin(&self) -> bool {
        if self.admin.snapshot.is_some() {
            return self.admin.is_admin();
        }
        matches!(
            self.conn,
            ConnectionState::InGame { admin: true, .. }
                | ConnectionState::InGame { local: true, .. }
        )
    }
    /// Every writer of prefs (Options, the console) calls this after
    /// changing them, so the parts of the game that cache a pref see it.
    pub fn apply_prefs(&mut self) {
        use crate::screens::options::{VOLUMES, chat_lines, volume};
        self.hud.prefs = self.hud_prefs();
        self.selector.queue_brick_buying =
            self.prefs.bool_or("$pref::Input::QueueBrickBuying", true);
        self.chat.max_lines = chat_lines(&self.prefs);
        self.chat.line_time_ms = self
            .prefs
            .i64_or("$Pref::Chat::LineTime", 6500)
            .clamp(0, 30000);
        for &(_, pref, channel) in VOLUMES {
            self.request(UiAction::SetVolume {
                channel: channel.into(),
                value: volume(&self.prefs, pref),
            });
        }
    }
    pub fn hud_prefs(&self) -> HudPrefs {
        let p = &self.prefs;
        HudPrefs {
            hide_brick_box: p.bool_or("$pref::HUD::HideBrickBox", true),
            hide_paint_box: p.bool_or("$pref::HUD::HidePaintBox", true),
            hide_tool_box: p.bool_or("$pref::HUD::HideToolBox", true),
            show_tooltips: p.bool_or("$pref::HUD::showToolTips", true),
            reverse_brick_scroll: p.bool_or("$pref::Input::ReverseBrickScroll", false),
            recolor_brick_icons: p.bool_or("$pref::Hud::RecolorBrickIcons", true),
            show_slot_numbers: p.bool_or("$pref::Gui::ShowBrickSlotNumbers", true),
        }
    }
    fn apply_outbox(&mut self, o: Outbox) {
        for a in o.actions {
            self.request(a);
        }
        for (t, s) in o.center_prints {
            self.center_print(&t, s);
        }
    }
    /// Release every held gameplay control (dialog focus change, focus
    /// loss, leaving the game).
    pub fn release_all(&mut self) {
        let held: Vec<String> = std::mem::take(&mut self.held).into_values().collect();
        for c in held {
            self.run_command(&c, false);
        }
        let ctrls: Vec<HeldControl> = std::mem::take(&mut self.held_controls)
            .into_iter()
            .collect();
        for c in ctrls {
            self.game(GameAction::Held {
                control: c,
                down: false,
            });
        }
        self.repeater.cancel_all();
    }

    /// The held tool takes the wheel: its image has a `wheel` command and
    /// its trigger is held down right now, or it is aimed through a scope
    /// with steps.
    fn tool_takes_wheel(&self) -> bool {
        self.aim_wheel || self.wheel_tool && self.held_controls.contains(&HeldControl::Fire)
    }
    fn held_control(&mut self, control: HeldControl, down: bool) {
        // One-button jump and the dedicated jet input can overlap. Releasing
        // either physical input must not release the other's jet hold.
        if !down
            && control == HeldControl::Jet
            && self.held.values().any(|c| {
                c.eq_ignore_ascii_case("jet")
                    || (self.one_button_jet() && c.eq_ignore_ascii_case("jump"))
            })
        {
            return;
        }
        // Toggle crouch: each press flips it; releasing does nothing.
        let down = if control == HeldControl::Crouch
            && self
                .prefs
                .bool_or(crate::screens::options::TOGGLE_CROUCH, false)
        {
            if !down {
                return;
            }
            !self.held_controls.contains(&control)
        } else {
            down
        };
        let changed = if down {
            self.held_controls.insert(control)
        } else {
            self.held_controls.remove(&control)
        };
        if changed {
            self.game(GameAction::Held { control, down });
        }
    }

    /// turnLeft/turnRight/panUp/panDown: `$mvYaw*Speed =
    /// $pref::Input::KeyboardTurnSpeed` while held, as a look rate.
    fn keyboard_turn(&mut self, dt_ms: u64) {
        let (mut yaw, mut pitch) = (0.0, 0.0);
        for (cmd, dy, dp) in KEYBOARD_TURN {
            if self.held.values().any(|h| h.eq_ignore_ascii_case(cmd)) {
                yaw += dy;
                pitch += dp;
            }
        }
        if yaw == 0.0 && pitch == 0.0 {
            return;
        }
        let speed = self
            .prefs
            .f32_or(screens::options::KEYBOARD_TURN_SPEED, 0.5)
            .clamp(0.02, 1.0);
        let step = speed * KEYBOARD_TURN_RATE * (dt_ms.min(100) as f32 / 1000.0);
        self.game(GameAction::Look {
            yaw: yaw * step,
            pitch: pitch * step,
        });
    }

    fn one_button_jet(&self) -> bool {
        self.prefs.bool_or("$pref::Input::noobjet", false) || self.settings.mouse_type == 0
    }

    /// A package bind's screen: the player's own mini-game's Add-On
    /// Settings (the Mini-Game list when they are in none), the package's
    /// help pages, or Options where the Add-On client preferences are.
    fn open_bind_screen(&mut self, screen: &str, package: &str) {
        match screen {
            "minigame_addons" => match self.minigames.active_game {
                Some(game) => {
                    self.minigame_addons = Some(game);
                    self.push(ScreenId::MiniGameAddOns);
                }
                None => self.push(ScreenId::MiniGames),
            },
            "help" => {
                let page = self
                    .addon_help
                    .iter()
                    .find(|p| p.package == package)
                    .map(|p| p.name.clone());
                self.get_help(page);
            }
            "options" => {
                self.options_section = Some(crate::screens::options::ADDON_SECTION.into());
                self.push(ScreenId::Options);
            }
            _ => {}
        }
    }

    /// Run a bound command (`%val` = `down`). Returns false if unknown.
    pub fn run_command(&mut self, cmd: &str, down: bool) -> bool {
        let c = cmd.to_ascii_lowercase();
        if c.starts_with("package:") {
            // A package's bind: its command, as the key goes down (and
            // up again, when held).
            let Some(bind) = self
                .package_binds
                .iter()
                .find(|b| b.bind_command().eq_ignore_ascii_case(&c))
            else {
                return false;
            };
            if let Some(screen) = bind.screen.clone() {
                if down {
                    self.open_bind_screen(&screen, &bind.package.clone());
                }
                return true;
            }
            if bind.hold || down {
                let action = GameAction::Package {
                    package: bind.package.clone(),
                    command: bind.command.clone(),
                    pressed: bind.hold.then_some(down),
                };
                self.game(action);
            }
            return true;
        }
        let bsd_key = self.key_name("openBSD");
        let held = |c: &str| -> Option<HeldControl> {
            Some(match c {
                "moveforward" => HeldControl::Forward,
                "movebackward" => HeldControl::Backward,
                "moveleft" => HeldControl::Left,
                "moveright" => HeldControl::Right,
                "crouch" => HeldControl::Crouch,
                "jet" => HeldControl::Jet,
                "mousefire" => HeldControl::Fire,
                "walk" => HeldControl::Walk,
                "togglefreelook" => HeldControl::FreeLook,
                _ => return None,
            })
        };
        if let Some(h) = held(&c) {
            self.held_control(h, down);
            return true;
        }
        // Keyboard turning is applied while held, in `Ui::update`.
        if KEYBOARD_TURN.iter().any(|(cmd, _, _)| c == *cmd) {
            return true;
        }
        let shift = |c: &str| -> Option<(bool, i32, i32, i32)> {
            Some(match c {
                "shiftbrickaway" => (false, 1, 0, 0),
                "shiftbricktowards" => (false, -1, 0, 0),
                "shiftbrickleft" => (false, 0, 1, 0),
                "shiftbrickright" => (false, 0, -1, 0),
                "shiftbrickup" => (false, 0, 0, 3),
                "shiftbrickdown" => (false, 0, 0, -3),
                "shiftbrickthirdup" => (false, 0, 0, 1),
                "shiftbrickthirddown" => (false, 0, 0, -1),
                "supershiftbrickawayproxy" => (true, 1, 0, 0),
                "supershiftbricktowardsproxy" => (true, -1, 0, 0),
                "supershiftbrickleftproxy" => (true, 0, 1, 0),
                "supershiftbrickrightproxy" => (true, 0, -1, 0),
                "supershiftbrickupproxy" => (true, 0, 0, 1),
                "supershiftbrickdownproxy" => (true, 0, 0, -1),
                _ => return None,
            })
        };
        if let Some((proxy, x, y, z)) = shift(&c) {
            // superShift*Proxy: with the smart toggle on they act as the
            // normal shift keys (which honour `$SuperShift`).
            let smart = self
                .prefs
                .bool_or("$pref::Input::UseSuperShiftToggle", true)
                && self
                    .prefs
                    .bool_or("$pref::Input::UseSuperShiftSmartToggle", true);
            let super_mode = if proxy {
                !smart || self.super_shift
            } else {
                self.super_shift
            };
            let key = if super_mode {
                format!("super:{c}")
            } else {
                c.clone()
            };
            if down {
                self.shift_brick(super_mode, x, y, z);
                self.repeater.press(&key, self.time_ms);
            } else {
                self.repeater.release(&key);
                self.repeater.release(&c);
                self.repeater.release(&format!("super:{c}"));
            }
            return true;
        }
        match c.as_str() {
            "jump" => {
                if self.one_button_jet() {
                    self.held_control(HeldControl::Jet, down);
                }
                self.held_control(HeldControl::Jump, down);
            }
            "togglezoom" => {
                self.zoom_on = down;
                self.held_control(HeldControl::Zoom, down);
            }
            "plantbrick" => {
                if down {
                    self.game(GameAction::PlantBrick);
                    self.repeater.press("plantbrick", self.time_ms);
                } else {
                    self.repeater.release("plantbrick");
                }
            }
            "togglesupershift" => self.toggle_super_shift(down),
            _ if !down => {}
            "toggleconsole" => self.toggle_console(),
            "escapemenu.toggle();" => self.escape_toggle(),
            "togglefirstperson" => {
                let fast = self
                    .prefs
                    .bool_or("$pref::Input::FastFirstThirdPerson", false);
                self.game(GameAction::ToggleFirstPerson { fast });
            }
            "dropcameraatplayer" => self.game(GameAction::DropCameraAtPlayer),
            "dropplayeratcamera" => self.game(GameAction::DropPlayerAtCamera),
            "suicide" => self.game(GameAction::Suicide),
            "nextseat" => self.game(GameAction::NextSeat),
            "prevseat" => self.game(GameAction::PrevSeat),
            "uselight" => self.game(GameAction::UseLight),
            "droptool" => self.game(GameAction::DropTool),
            "rotatebrickcw" => self.game(GameAction::RotateBrick { dir: 1 }),
            "rotatebrickccw" => self.game(GameAction::RotateBrick { dir: -1 }),
            "cancelbrick" => self.game(GameAction::CancelBrick),
            "undobrick" => self.game(GameAction::UndoBrick),
            "doscreenshot" => self.game(GameAction::Screenshot {
                kind: ScreenshotKind::Normal,
            }),
            "dohudscreenshot" => self.game(GameAction::Screenshot {
                kind: ScreenshotKind::NoHud,
            }),
            "dodofscreenshot" => self.game(GameAction::Screenshot {
                kind: ScreenshotKind::DepthOfField,
            }),
            "togglenetgraph" => self.toggle_net_graph(),
            "toggleperfoverlay" => self.perf.cycle(),
            "saveperfcapture" => self.game(GameAction::SavePerfCapture),
            "togglefullscreen();" => self.game(GameAction::ToggleFullscreen),
            "togglebuildmacrorecording" => self.game(GameAction::ToggleBuildMacroRecording),
            "playbackbuildmacro" => self.game(GameAction::PlayBackBuildMacro),
            "emotesit" | "emotelove" | "emotehate" | "emoteconfusion" | "emotealarm" => {
                self.game(GameAction::Emote {
                    name: c.trim_start_matches("emote").to_string(),
                })
            }
            "globalchat" | "teamchat" => {
                if self.prefs.i64_or("$Pref::Chat::LineTime", 6500) > 0 {
                    let ch = if c == "globalchat" {
                        ChatChannel::Say
                    } else {
                        ChatChannel::Team
                    };
                    self.push(ScreenId::MessageInput(ch));
                }
            }
            "pageupnewchathud" => self.chat.page_up(),
            "pagedownnewchathud" => self.chat.page_down(),
            "togglecursor" => {
                let single = matches!(
                    self.conn,
                    ConnectionState::InGame {
                        single_player: true,
                        ..
                    }
                );
                if !single {
                    self.cursor_forced = !self.cursor_forced;
                }
            }
            "showplayerlist" => self.cmds.push(StackCmd::Push(ScreenId::PlayerList)),
            "openoptionswindow" => self.push(ScreenId::Options),
            "openadminwindow" => {
                self.request(UiAction::OpenAdmin);
            }
            "toggleshapenamehud" => self.shape_names = !self.shape_names,
            // `openBSD`: with building disabled it only says so, so the
            // selector (and its "Bricks" cue) never opens.
            "openbsd" if self.hud.building_disabled => {
                self.center_print("\u{E005}Building is currently disabled.", 2.0)
            }
            "openbsd" => self.push(ScreenId::BrickSelector),
            "usebricks" => {
                let mut o = Outbox::default();
                self.hud.use_bricks(&bsd_key, &mut o);
                self.apply_outbox(o);
            }
            "usetools" => {
                let mut o = Outbox::default();
                self.hud.use_tools(&mut o);
                self.apply_outbox(o);
            }
            "usespraycan" => {
                let mut o = Outbox::default();
                self.hud.use_spray_can(&mut o);
                self.apply_outbox(o);
            }
            "invup" | "invdown" | "invleft" | "invright" => {
                let k = match c.as_str() {
                    "invup" => InvKey::Up,
                    "invdown" => InvKey::Down,
                    "invleft" => InvKey::Left,
                    _ => InvKey::Right,
                };
                let mut o = Outbox::default();
                self.hud.inv_key(k, &bsd_key, &mut o);
                self.apply_outbox(o);
            }
            other => {
                const SLOTS: [&str; 10] = [
                    "usefirstslot",
                    "usesecondslot",
                    "usethirdslot",
                    "usefourthslot",
                    "usefifthslot",
                    "usesixthslot",
                    "useseventhslot",
                    "useeighthslot",
                    "useninthslot",
                    "usetenthslot",
                ];
                if let Some(i) = SLOTS.iter().position(|s| *s == other) {
                    let mut o = Outbox::default();
                    self.hud.direct_select_inv(i, &bsd_key, &mut o);
                    self.apply_outbox(o);
                } else if other == "contexthelp();" {
                    self.context_help();
                } else {
                    // Debug render modes are recognised but inert.
                    return other == "cycledebugrendermode";
                }
            }
        }
        true
    }

    fn shift_brick(&mut self, super_mode: bool, x: i32, y: i32, z: i32) {
        if super_mode {
            let z = z.signum();
            self.game(GameAction::SuperShiftBrick { x, y, z });
        } else {
            self.game(GameAction::ShiftBrick { x, y, z });
        }
    }

    /// `toggleSuperShift` (c:5219): press flips; with the smart toggle,
    /// releasing after more than 200 ms flips back.
    fn toggle_super_shift(&mut self, down: bool) {
        let toggle = self
            .prefs
            .bool_or("$pref::Input::UseSuperShiftToggle", true);
        let smart = self
            .prefs
            .bool_or("$pref::Input::UseSuperShiftSmartToggle", true);
        if toggle {
            if down {
                self.super_shift_time = self.time_ms;
                self.super_shift = !self.super_shift;
                self.repeater.cancel_all();
            } else if smart && self.time_ms.saturating_sub(self.super_shift_time) > 200 {
                self.super_shift = !self.super_shift;
                self.repeater.cancel_all();
            }
        } else {
            self.super_shift = down;
            self.repeater.cancel_all();
        }
    }

    fn escape_toggle(&mut self) {
        // In stock v20, Esc on the loading screen disconnects (c:9241).
        if matches!(
            self.conn,
            ConnectionState::Loading { .. } | ConnectionState::Connecting { .. }
        ) {
            self.request(UiAction::CancelConnect);
            return;
        }
        self.push(ScreenId::EscapeMenu);
    }

    fn wheel_scroll(&mut self, delta: f32) {
        // scrollInventory: %val < 0 → +1 (Torque positive = wheel up).
        let dir = if delta < 0.0 { 1 } else { -1 };
        if self.zoom_on {
            let mut fov = crate::screens::options::zoom_fov(&self.prefs);
            if dir > 0 {
                if fov > 5.0 {
                    fov -= 5.0;
                }
            } else if fov < 85.0 {
                fov += 5.0;
            }
            self.prefs
                .set(crate::screens::options::ZOOM_FOV, format!("{fov}"));
            self.save_settings();
            self.game(GameAction::SetZoomFov { fov });
            return;
        }
        let key = self.key_name("openBSD");
        let mut o = Outbox::default();
        self.hud.scroll_inventory(dir, &key, &mut o);
        self.apply_outbox(o);
    }
}

pub struct Ui {
    pub core: Core,
    pub content: Box<dyn Screen>,
    pub dialogs: Vec<Box<dyn Screen>>,
    cfg: UiConfig,
    mods: Modifiers,
    mouse: (f32, f32),
    /// Unused fraction of a wheel notch over menus.
    wheel_rest: f32,
    sounds: Vec<UiSound>,
    dropped_sounds: u64,
    /// A global bind consumed the last key press; drop the character it
    /// types (the `~` that opened the console must not appear in it).
    swallow_char: bool,
    /// The scale the host asked for; the player's UI size replaces it.
    host_scale: Option<f32>,
    /// `$pref::Gui::Scale` as last applied.
    applied_scale_pref: i64,
}

impl Ui {
    /// Create the UI. `settings` are the host-persisted values (use
    /// `Settings::default()` on first run: Default Controls will show).
    pub fn new(pack: Rc<Pack>, cfg: UiConfig, settings: Settings) -> Ui {
        let mut defaults = pack.data.data.prefs.clone();
        defaults.retain(|k, _| {
            !crate::screens::options::MACHINE_PREFS
                .iter()
                .any(|m| m.eq_ignore_ascii_case(k))
        });
        for (key, value) in crate::screens::options::NATIVE_DEFAULTS {
            defaults.retain(|k, _| !k.eq_ignore_ascii_case(key));
            defaults.insert((*key).into(), (*value).into());
        }
        let prefs = Prefs::new(&defaults, &settings.prefs);
        let platform = cfg.platform;
        let remap = crate::binds::remap_entries(&pack.data.data);
        let binds = match &settings.binds {
            Some(b) => {
                let mut binds = BindMap { entries: b.clone() };
                binds.add_missing_extras(&remap);
                binds
            }
            None => BindMap::defaults(
                &pack.data.data,
                crate::binds::DEFAULT_MOUSE,
                crate::binds::DEFAULT_KEYBOARD,
                platform,
            ),
        };
        let globals = BindMap::globals(&pack.data.data, platform);
        let mut selector = SelectorModel {
            favorites: settings.brick_favorites.clone(),
            ..Default::default()
        };
        if selector.favorites.is_empty() && settings.binds.is_none() {
            selector.favorites = pack.data.data.favorites.clone();
        }
        selector.queue_brick_buying = prefs.bool_or("$pref::Input::QueueBrickBuying", true);
        let first = prefs
            .i64_or("$Pref::Input::brickFirstRepeatTime", 200)
            .max(1) as u64;
        let rep = prefs.i64_or("$Pref::Input::brickRepeatTime", 50).max(1) as u64;
        let chat = ChatModel::new(
            prefs.i64_or("$Pref::Chat::CacheLines", 1000) as usize,
            crate::screens::options::chat_lines(&prefs),
            prefs.i64_or("$Pref::Chat::LineTime", 6500).clamp(0, 30000),
        );
        let remap_commands = remap.iter().map(|r| r.command.clone()).collect();
        let mut settings = settings;
        if settings.binds.is_none() {
            settings.mouse_type = crate::binds::DEFAULT_MOUSE;
            settings.keyboard_type = crate::binds::DEFAULT_KEYBOARD;
        }
        if settings.avatar.values.is_empty() {
            settings.avatar =
                AvatarPrefs::from_prefs(&prefs, &pack.data.data.prefs, &pack.data.data.avatar);
        }
        let mut core = Core {
            pack: pack.clone(),
            platform,
            logical: cfg.logical(),
            time_ms: 0,
            settings,
            prefs,
            binds,
            globals,
            remap,
            remap_commands,
            remap_target: None,
            remap_all: false,
            options_open: false,
            options_section: None,
            help_open: false,
            name_asked: false,
            help_page: None,
            addon_help: Vec::new(),
            avatar_value: None,
            splash: None,
            report: None,
            print_letters_visible: false,
            maps: Vec::new(),
            game_modes: Vec::new(),
            servers: Vec::new(),
            lan_querying: false,
            bricks: Vec::new(),
            prints: BTreeMap::new(),
            events: EventCatalog::default(),
            datablocks: DatablockMenus::new(),
            menu_backgrounds: Vec::new(),
            music_tracks: Vec::new(),
            color_append_fits: true,
            display_modes: None,
            avatar_preview: IconRef::None,
            save_maps: Vec::new(),
            save_files: Vec::new(),
            save_context: None,
            save_preview: None,
            add_ons: Default::default(),
            add_on_mismatch: None,
            failure_question: None,
            unsaved_changes: false,
            version: String::new(),
            newer_version: None,
            conn: ConnectionState::Idle,
            hud: HudModel::default(),
            chat,
            talking: Vec::new(),
            selector,
            wrench: WrenchState::default(),
            players: Vec::new(),
            admin: Default::default(),
            environment: Default::default(),
            minigames: MiniGameUiState::default(),
            minigame_addons: None,
            server_addon_settings: false,
            trust_invites: Vec::new(),
            name_tags: Vec::new(),
            package_panels: Vec::new(),
            package_keys: Vec::new(),
            package_binds: Vec::new(),
            server_name: String::new(),
            max_players: 0,
            center_print: None,
            bottom_print: None,
            plant_error: None,
            captions: Vec::new(),
            damage_flash: 0.0,
            energy: None,
            whiteout: 0.0,
            underwater: Vec::new(),
            net_graph: None,
            perf: Default::default(),
            lagging: false,
            shape_names: true,
            first_person: true,
            hide_crosshair: false,
            super_shift: false,
            super_shift_time: 0,
            zoom_on: false,
            wheel_tool: false,
            wheel_camera: false,
            aim_wheel: false,
            scope_overlay: None,
            cursor_forced: false,
            print_aspect: None,
            last_print: BTreeMap::new(),
            next_id: 0,
            session_request: None,
            out: Vec::new(),
            pending: BTreeMap::new(),
            deadlines: BTreeMap::new(),
            abandoned: std::collections::VecDeque::new(),
            cmds: Vec::new(),
            repeater: Repeater::new(first, rep),
            held: BTreeMap::new(),
            held_controls: BTreeSet::new(),
            console: Default::default(),
        };
        core.hud.prefs = core.hud_prefs();
        let content = screens::make(ScreenId::MainMenu, &mut core);
        let mut ui = Ui {
            core,
            content,
            dialogs: Vec::new(),
            cfg,
            mods: Modifiers::NONE,
            mouse: (-1.0, -1.0),
            wheel_rest: 0.0,
            sounds: Vec::new(),
            dropped_sounds: 0,
            swallow_char: false,
            host_scale: cfg.scale,
            applied_scale_pref: 0,
        };
        ui.apply_scale_pref();
        if ui.core.settings.binds.is_none() {
            ui.core.push(ScreenId::DefaultControls);
        }
        ui.content.on_wake(&mut ui.core);
        ui.flush();
        ui.relayout();
        ui
    }

    /// Commands the host runs itself (arriving as `UiAction::Console`), so
    /// the console lists, describes and completes them.
    pub fn set_console_commands(&mut self, commands: Vec<bri_console::CommandInfo>) {
        self.core.console.host_commands = commands;
    }

    pub fn config(&self) -> UiConfig {
        self.cfg
    }
    pub fn scale(&self) -> f32 {
        self.cfg.effective_scale()
    }
    pub fn logical_size(&self) -> (i32, i32) {
        self.core.logical
    }

    /// Window resized or UI scale changed.
    pub fn resize(&mut self, size: (u32, u32), scale: Option<f32>) {
        self.host_scale = scale;
        self.cfg.size = size;
        self.cfg.scale = preferred_scale(&self.core.prefs, size).or(scale);
        self.core.logical = self.cfg.logical();
        self.relayout();
    }
    /// The scale the host configured (the player's UI size aside).
    pub fn host_scale(&self) -> Option<f32> {
        self.host_scale
    }
    /// Follow a changed UI size preference (Options, console).
    fn apply_scale_pref(&mut self) {
        let pref = self.core.prefs.i64_or(UI_SCALE, 0);
        if pref != self.applied_scale_pref {
            self.applied_scale_pref = pref;
            self.resize(self.cfg.size, self.host_scale);
        }
    }

    fn relayout(&mut self) {
        let (w, h) = self.core.logical;
        self.content.layout(w, h, &mut self.core);
        for d in &mut self.dialogs {
            d.layout(w, h, &mut self.core);
        }
    }

    /// Requests produced since the last call.
    pub fn drain_actions(&mut self) -> Vec<(RequestId, UiAction)> {
        std::mem::take(&mut self.core.out)
    }

    /// Drain once per host frame and play at listener placement. Preference
    /// gates are evaluated when emitted from committed settings, never drafts.
    pub fn drain_sounds(&mut self) -> Vec<UiSound> {
        std::mem::take(&mut self.sounds)
    }

    /// Cosmetic intents discarded because the host did not drain the bounded
    /// outbox. No gameplay request or setting is affected.
    pub fn dropped_sounds(&self) -> u64 {
        self.dropped_sounds
    }

    fn queue_sound(&mut self, sound: UiSound) {
        if self.sounds.len() < MAX_QUEUED_SOUNDS {
            self.sounds.push(sound);
        } else {
            self.dropped_sounds = self.dropped_sounds.saturating_add(1);
        }
    }

    pub fn top_id(&self) -> ScreenId {
        self.dialogs.last().map_or(self.content.id(), |d| d.id())
    }
    /// Whether the top screen takes typed text (a focused text box or an
    /// open dropdown's search).
    pub fn takes_text(&self) -> bool {
        self.screen(self.top_id())
            .is_some_and(|s| s.view().takes_text())
    }
    pub fn is_open(&self, id: ScreenId) -> bool {
        self.content.id() == id || self.dialogs.iter().any(|d| d.id() == id)
    }
    pub fn screen(&self, id: ScreenId) -> Option<&dyn Screen> {
        if self.content.id() == id {
            return Some(self.content.as_ref());
        }
        self.dialogs
            .iter()
            .rev()
            .find(|d| d.id() == id)
            .map(|d| d.as_ref())
    }
    pub fn screen_mut(&mut self, id: ScreenId) -> Option<&mut Box<dyn Screen>> {
        if self.content.id() == id {
            return Some(&mut self.content);
        }
        self.dialogs.iter_mut().rev().find(|d| d.id() == id)
    }
    /// The stack from bottom (content) to top.
    pub fn stack(&self) -> Vec<ScreenId> {
        std::iter::once(self.content.id())
            .chain(self.dialogs.iter().map(|d| d.id()))
            .collect()
    }

    /// Whether the mouse cursor is visible (dialogs, menus, or M toggled).
    pub fn cursor_visible(&self) -> bool {
        self.content.cursor() || self.dialogs.iter().any(|d| d.cursor()) || self.core.cursor_forced
    }

    /// Gameplay binds require an established in-game session. Loading retains
    /// cancellation only; no movement/fire input leaks into the next session.
    fn game_input_active(&self) -> bool {
        self.content.id() == ScreenId::Play && self.core.in_game()
    }

    fn text_focus(&self) -> bool {
        self.dialogs.last().map_or_else(
            || self.content.view().focus.is_some(),
            |d| d.view().focus.is_some(),
        )
    }

    /// Apply queued stack commands until stable.
    fn flush(&mut self) {
        for _ in 0..32 {
            let cmds = std::mem::take(&mut self.core.cmds);
            if cmds.is_empty() {
                break;
            }
            for c in cmds {
                self.apply_cmd(c);
            }
        }
    }

    fn apply_cmd(&mut self, c: StackCmd) {
        let (w, h) = self.core.logical;
        match c {
            StackCmd::SetContent(id) => {
                if self.content.id() == id {
                    return;
                }
                self.core.release_all();
                let mut old =
                    std::mem::replace(&mut self.content, screens::make(id, &mut self.core));
                old.on_sleep(&mut self.core);
                // v20 pops every dialog when the content changes except
                // those the new content pushes itself. The console sits on
                // its own canvas layer (pushDialog(ConsoleDlg, 99)) and stays.
                for mut d in std::mem::take(&mut self.dialogs) {
                    if d.id() == ScreenId::Console {
                        self.dialogs.push(d);
                    } else {
                        d.on_sleep(&mut self.core);
                    }
                }
                self.content.layout(w, h, &mut self.core);
                self.content.on_wake(&mut self.core);
            }
            StackCmd::Push(id) => {
                if let Some(i) = self.dialogs.iter().position(|d| d.id() == id) {
                    // Already open: pushDialog brings it to the front.
                    let d = self.dialogs.remove(i);
                    self.dialogs.push(d);
                    return;
                }
                let mut s = screens::make(id, &mut self.core);
                if s.modal() || s.captures_keyboard() {
                    self.core.release_all();
                }
                s.layout(w, h, &mut self.core);
                s.on_wake(&mut self.core);
                self.dialogs.push(s);
            }
            StackCmd::Pop(id) => {
                if let Some(i) = self.dialogs.iter().rposition(|d| d.id() == id) {
                    let mut d = self.dialogs.remove(i);
                    d.on_sleep(&mut self.core);
                }
            }
            StackCmd::Message(m) => {
                self.core.release_all();
                let mut s = screens::menus::MessageScreen::new(&self.core, *m);
                s.layout(w, h, &mut self.core);
                self.dialogs.push(Box::new(s));
            }
        }
    }

    /// Filter all asynchronous updates by the originating HostGame/JoinServer/
    /// StartTutorial request ID. Cancellation and a new attempt invalidate it.
    /// The transport adapter must use this instead of untagged apply().
    pub fn apply_session(&mut self, connection_request: RequestId, update: UiUpdate) -> bool {
        if self.core.session_request != Some(connection_request) {
            return false;
        }
        self.apply(update);
        true
    }

    /// Current connection attempt/session token, not an authority credential.
    pub fn session_request(&self) -> Option<RequestId> {
        self.core.session_request
    }

    /// Apply synchronous local catalogs/settings, or a checked session update.
    pub fn apply(&mut self, u: UiUpdate) {
        if matches!(&u, UiUpdate::PlantError(_))
            && self
                .core
                .prefs
                .bool_or("$Pref::Audio::PlantErrorSound", false)
        {
            // handlePlantError, allClientScripts-Vanilla.cs:7193–7195.
            self.queue_sound(AUDIO_ERROR);
        }
        if matches!(
            &u,
            UiUpdate::Connection(ConnectionState::Idle | ConnectionState::Failed { .. })
        ) {
            self.core.reset_session();
            self.core.session_request = None;
        }
        let c = &mut self.core;
        match u {
            UiUpdate::Environment(view) => c.environment.apply(view),
            UiUpdate::Admin(update) => {
                let before: Vec<_> = c.admin.pending.keys().copied().collect();
                if let Err(reason) = c.admin.apply(update) {
                    c.admin.status = reason;
                }
                for id in before {
                    if !c.admin.pending.contains_key(&id) {
                        c.pending.remove(&id);
                    }
                }
            }
            UiUpdate::ActionResult { id, result } => {
                c.deadlines.remove(&id);
                if c.abandoned.contains(&id) {
                    bri_console::echo(format!(
                        "Dropped a late answer to request {id}: {}",
                        match &result {
                            Ok(()) => "accepted",
                            Err(reason) => reason.as_str(),
                        }
                    ));
                    return;
                }
                let kind = c.pending.remove(&id);
                let mut handled = c.admin.result(id, &result);
                for d in self.dialogs.iter_mut().rev().filter(|_| !handled) {
                    if d.on_result(id, kind.as_ref(), &result, &mut self.core) {
                        handled = true;
                        break;
                    }
                }
                if !handled
                    && !self
                        .content
                        .on_result(id, kind.as_ref(), &result, &mut self.core)
                    && let Err(reason) = result
                {
                    // During play a rejected action is a transient notice, like
                    // v20's server bottom prints, not a modal that steals input.
                    if self.core.in_game() && self.top_id() == ScreenId::Play {
                        let until = self.core.time_ms + 3000;
                        self.core.bottom_print = Some((reason, Some(until), false));
                    } else {
                        self.core.message_ok("Request Rejected", &reason);
                    }
                }
            }
            UiUpdate::Connection(state) => {
                let target = match &state {
                    ConnectionState::Idle | ConnectionState::Failed { .. } => ScreenId::MainMenu,
                    ConnectionState::Connecting { .. } => self.content.id(),
                    ConnectionState::Loading { .. } => ScreenId::Loading,
                    ConnectionState::DownloadingPackages(_) => ScreenId::PackageDownload,
                    ConnectionState::InGame { .. } => ScreenId::Play,
                };
                if let ConnectionState::InGame {
                    server_name,
                    max_players,
                    ..
                } = &state
                {
                    // Initialize slide positions once on session entry, before
                    // Play draws. Repeated metadata updates preserve animations.
                    if !c.in_game() {
                        c.hud.reset_layout();
                    }
                    c.server_name = server_name.clone();
                    c.max_players = *max_players;
                }
                let connecting = matches!(state, ConnectionState::Connecting { .. });
                let failed = match &state {
                    ConnectionState::Failed { reason } => Some(reason.clone()),
                    _ => None,
                };
                c.conn = state;
                if target != self.content.id() {
                    c.set_content(target);
                    if target == ScreenId::MainMenu {
                        c.cursor_forced = false;
                    }
                }
                if connecting {
                    c.push(ScreenId::Connecting);
                } else {
                    c.pop(ScreenId::Connecting);
                }
                if let Some(r) = failed {
                    bri_console::warn(format!("Connection failed: {r}"));
                    if let Some(question) = c.failure_question.take() {
                        c.ask(question);
                    } else if c.add_on_mismatch.is_some() {
                        c.push(ScreenId::AddOnMismatch);
                    } else {
                        c.message_ok("Connection Failed", &crate::models::disconnect::explain(&r));
                    }
                }
            }
            UiUpdate::Maps(m) => c.maps = m,
            UiUpdate::GameModes(m) => c.game_modes = m,
            UiUpdate::LanServers { servers, querying } => {
                c.servers = servers;
                c.lan_querying = querying;
            }
            UiUpdate::MainMenuBackgrounds(b) => c.menu_backgrounds = b,
            UiUpdate::Bricks(b) => {
                let remap = |old: Option<usize>| {
                    old.and_then(|i| c.bricks.get(i))
                        .and_then(|old| b.iter().position(|new| new.id == old.id))
                };
                c.selector.cart = c.selector.cart.map(remap);
                c.selector.clicked_brick = remap(c.selector.clicked_brick);
                c.pack
                    .prefetch(b.iter().filter_map(|brick| match &brick.icon {
                        IconRef::Pack(id) => Some(id.clone()),
                        _ => None,
                    }));
                c.bricks = b;
            }
            UiUpdate::Colorset(d) => c.hud.set_colorset(d),
            UiUpdate::Prints { aspect, prints } => {
                c.prints.insert(aspect, prints);
            }
            UiUpdate::Events(e) => c.events = e,
            UiUpdate::Datablocks(d) => c.datablocks = d,
            UiUpdate::BuildingAllowed(b) => c.hud.building_disabled = !b,
            UiUpdate::BrickInventory(ids) => {
                let inv = ids
                    .iter()
                    .map(|id| {
                        id.as_ref()
                            .and_then(|id| c.bricks.iter().find(|b| &b.id == id).cloned())
                    })
                    .collect();
                c.hud.set_bricks(inv);
            }
            UiUpdate::Tools(t) => c.hud.set_tools(t),
            UiUpdate::SetActiveTool(slot) => c.hud.apply_active_tool(slot),
            UiUpdate::ScrollMode(mode) => c.hud.apply_scroll_mode(mode),
            UiUpdate::ToolTakesPaint(takes) => c.hud.tool_takes_paint = takes,
            UiUpdate::SetActiveBrick(slot) => c.hud.apply_active_brick(slot),
            UiUpdate::FirstSpawn => {
                // BSD_ClickFav(1) + buy, only when favorites slot 1 exists.
                if c.selector.favorite_filled(1) || c.selector.favorites.contains_key(&1) {
                    let cat = c.bricks.clone();
                    c.selector.load_favorite(1, &cat);
                    let slots = c.selector.purchase(&cat);
                    c.hud.clear_bricks_for_buy();
                    c.request_pending(UiAction::BuyBricks { slots }, Pending::Buy);
                }
            }
            UiUpdate::Chat { text } => {
                let now = c.time_ms;
                // `newChatHud_AddLine`: Censor Chat (on in v20's defaults).
                let text = if c.prefs.bool_or("$Pref::Chat::CurseFilter", true) {
                    crate::models::chat::censor(&text, c.prefs.str_or("$Pref::Chat::CurseList", ""))
                } else {
                    text
                };
                c.chat.add(&text, now);
            }
            UiUpdate::Talking(names) => c.talking = names,
            UiUpdate::CenterPrint { text, seconds } => c.center_print(&text, seconds),
            UiUpdate::BottomPrint {
                text,
                seconds,
                hide_bar,
            } => {
                // An empty print clears the line (`clearBottomPrint`).
                let until = (seconds > 0.0).then(|| c.time_ms + (seconds * 1000.0) as u64);
                c.bottom_print = (!text.is_empty()).then_some((text, until, hide_bar));
            }
            UiUpdate::ClearPrints => {
                c.center_print = None;
                c.bottom_print = None;
            }
            UiUpdate::PlantError(e) => c.plant_error = Some((e, c.time_ms + 800)),
            UiUpdate::NetSample(sample) => {
                if let Some(graph) = &mut c.net_graph {
                    graph.add(sample);
                }
                if c.perf.wants_net() {
                    c.perf.net = Some(sample);
                }
            }
            UiUpdate::PerfFrame(frame) => c.perf.push_frame(frame),
            UiUpdate::PerfStats(stats) => {
                if c.perf.visible() {
                    c.perf.stats = stats;
                }
            }
            UiUpdate::Caption(text) => {
                if c.prefs.bool_or(crate::screens::options::CAPTIONS, false) {
                    // A repeated sound refreshes its line instead of stacking.
                    c.captions.retain(|(t, _)| *t != text);
                    c.captions.push((text, c.time_ms + CAPTION_MS));
                    let extra = c.captions.len().saturating_sub(MAX_CAPTIONS);
                    c.captions.drain(..extra);
                }
            }
            UiUpdate::FirstPerson(on) => c.first_person = on,
            UiUpdate::HideCrosshair(on) => c.hide_crosshair = on,
            UiUpdate::ToolWheel(on) => c.wheel_tool = on,
            UiUpdate::CameraWheel(on) => c.wheel_camera = on,
            UiUpdate::AimWheel(on) => c.aim_wheel = on,
            UiUpdate::ScopeOverlay(overlay) => {
                c.scope_overlay = overlay.filter(|(_, aspect)| aspect.is_finite() && *aspect > 0.0)
            }
            UiUpdate::Whiteout(amount) => {
                if amount.is_finite() {
                    c.whiteout = c.whiteout.max(amount.clamp(0.0, 1.0));
                }
            }
            UiUpdate::Underwater(tints) => {
                c.underwater = tints
                    .into_iter()
                    .filter(|t| t.iter().all(|v| v.is_finite()))
                    .take(2)
                    .collect();
            }
            UiUpdate::Energy(energy) => {
                c.energy = energy.filter(|e| e.is_finite()).map(|e| e.clamp(0.0, 1.0));
            }
            UiUpdate::DamageFlash(amount) => {
                if amount.is_finite() {
                    c.damage_flash = (c.damage_flash + amount.max(0.0)).min(0.75);
                }
            }
            UiUpdate::Players {
                rows,
                server_name,
                max_players,
            } => {
                c.players = rows;
                c.server_name = server_name;
                c.max_players = max_players;
            }
            UiUpdate::MiniGames(state) => {
                c.minigames = state;
            }
            UiUpdate::MessageBox { title, text } => c.message_ok(&title, &text),
            UiUpdate::Confirm {
                title,
                text,
                action,
            } => c.message_yes_no(&title, &text, Callback::Request(action)),
            UiUpdate::TrustInvite(invitation) => {
                c.trust_invites.retain(|i| i.from != invitation.from);
                c.trust_invites.push(invitation);
                c.pop(ScreenId::TrustInvitation);
                c.push(ScreenId::TrustInvitation);
            }
            UiUpdate::Report(report) => {
                // A new report opens a window sized to its columns, unless
                // the player hides them (Slayer's "Disable End of Round
                // Report").
                let hidden = c
                    .prefs
                    .bool_or(crate::screens::options::HIDE_REPORTS, false);
                c.pop(ScreenId::Report);
                if report.is_some() && !hidden {
                    c.push(ScreenId::Report);
                }
                c.report = report;
            }
            UiUpdate::MiniGameInvite(invitation) => {
                c.minigames
                    .invitations
                    .retain(|i| i.game != invitation.game);
                c.minigames.invitations.push(invitation);
                c.push(ScreenId::MiniGameInvitation);
            }
            UiUpdate::Lagging(l) => c.lagging = l,
            UiUpdate::OpenWrench {
                brick,
                variant,
                owner,
                data,
                admin_override,
                events_allowed,
            } => {
                c.pop(ScreenId::WrenchEvents);
                c.wrench
                    .open(brick, variant, owner, data, admin_override, events_allowed);
                for id in [
                    ScreenId::Wrench(WrenchVariant::Normal),
                    ScreenId::Wrench(WrenchVariant::Sound),
                    ScreenId::Wrench(WrenchVariant::VehicleSpawn),
                ] {
                    c.pop(id);
                }
                c.push(ScreenId::Wrench(variant));
            }
            UiUpdate::OpenFillWrench { bricks } => {
                c.pop(ScreenId::WrenchEvents);
                c.wrench.open_fill(bricks);
                for id in [
                    ScreenId::Wrench(WrenchVariant::Normal),
                    ScreenId::Wrench(WrenchVariant::Sound),
                    ScreenId::Wrench(WrenchVariant::VehicleSpawn),
                ] {
                    c.pop(id);
                }
                c.push(ScreenId::Wrench(WrenchVariant::Normal));
            }
            UiUpdate::OpenEvents {
                brick,
                rows,
                named_targets,
                allow_named,
            } => {
                c.pop(ScreenId::WrenchEvents);
                c.wrench
                    .open_events(brick, rows, named_targets, allow_named, &c.events);
                c.push(ScreenId::WrenchEvents);
            }
            UiUpdate::NamedTargetsInvalidated => {
                c.wrench.events_copy = None;
                if self.is_open(ScreenId::WrenchEvents) {
                    self.core.pop(ScreenId::WrenchEvents);
                    self.core.wrench.events = None;
                    self.core.message_ok("", "Named Target List Invalidated");
                }
            }
            UiUpdate::OpenPrintSelector { aspect, current } => {
                if let Some(p) = current {
                    c.last_print.insert(aspect.clone(), p);
                }
                c.print_aspect = Some(aspect);
                c.push(ScreenId::PrintSelector);
            }
            UiUpdate::ColorWarning { append } => {
                c.color_append_fits = append;
                c.push(ScreenId::LoadBricksColor);
            }
            UiUpdate::SaveFiles { maps, files } => {
                c.save_maps = maps;
                c.save_files = files;
            }
            UiUpdate::SaveContext { map, preview } => c.save_context = Some((map, preview)),
            UiUpdate::SavePreview { map, name, preview } => {
                c.save_preview = Some((map, name, preview))
            }
            UiUpdate::AvatarPreview(i) => c.avatar_preview = i,
            UiUpdate::Splash(s) => {
                c.splash = Some(s);
                c.push(ScreenId::Splash);
            }
            UiUpdate::AddOns(view) => c.add_ons = view,
            UiUpdate::AddOnMismatch(m) => c.add_on_mismatch = Some(m),
            UiUpdate::Question(q) => c.ask(q),
            UiUpdate::FailureQuestion(q) => c.failure_question = Some(q),
            UiUpdate::UnsavedChanges(unsaved) => c.unsaved_changes = unsaved,
            UiUpdate::Version(v) => c.version = v,
            UiUpdate::NewerVersion { name, url } => {
                c.message_yes_no(
                    "New Version Available",
                    &format!(
                        "A newer version of Blockland ReImagined, {name}, is available.\n\nOpen the download page?"
                    ),
                    Callback::OpenUrl(url.clone()),
                );
                c.newer_version = Some((name, url));
            }
            UiUpdate::SetPrefs(prefs) => {
                for (key, value) in prefs {
                    c.prefs.set(&key, value);
                }
                c.save_settings();
            }
            UiUpdate::DisplayModes(modes) => c.display_modes = Some(modes),
            UiUpdate::DisplayChanged {
                resolution,
                fullscreen,
            } => {
                crate::screens::options::record_display(&mut c.prefs, resolution, fullscreen);
                c.save_settings();
            }
        }
        self.content.on_update(&mut self.core);
        for d in &mut self.dialogs {
            d.on_update(&mut self.core);
        }
        self.flush();
    }

    fn logical_point(&self, x: f32, y: f32) -> (i32, i32) {
        let s = self.scale();
        ((x / s).floor() as i32, (y / s).floor() as i32)
    }

    /// Index of the screen that receives mouse input (topmost modal or
    /// mouse-accepting screen). `None` = content.
    fn mouse_target(&self) -> Option<usize> {
        self.dialogs
            .iter()
            .rposition(|d| d.modal() || d.wants_mouse())
    }

    fn with_target<R>(
        &mut self,
        idx: Option<usize>,
        f: impl FnOnce(&mut dyn Screen, &mut Core) -> R,
    ) -> R {
        match idx {
            Some(i) => f(self.dialogs[i].as_mut(), &mut self.core),
            None => f(self.content.as_mut(), &mut self.core),
        }
    }

    fn dispatch(&mut self, idx: Option<usize>, evs: Vec<ViewEvent>) {
        for ev in evs {
            let sound = self.with_target(idx, |s, c| {
                let sound = if ev.kind == EventKind::Hover
                    && c.prefs.bool_or("$Pref::Audio::MenuSounds", true)
                    && s.view().is_shown(ev.node)
                    && s.view().node(ev.node).state.active
                {
                    s.view()
                        .node(ev.node)
                        .ctrl
                        .name
                        .as_deref()
                        .and_then(|name| menu_hover_sound(s.id(), name))
                } else {
                    None
                };
                s.on_event(&ev, c);
                sound
            });
            if let Some(sound) = sound {
                self.queue_sound(sound);
            }
        }
        self.flush();
    }

    /// Feed one host input event.
    pub fn handle_input(&mut self, ev: InputEvent) {
        // Capture remaps before global shortcuts, widget dispatch or gameplay.
        let binding = match ev {
            InputEvent::KeyDown {
                key,
                mods,
                repeat: false,
            } => Some(BindInput::Key(if key.is_modifier() {
                Chord::plain(key)
            } else {
                Chord { key, mods }
            })),
            InputEvent::MouseDown { button, .. } => Some(BindInput::Mouse(button)),
            InputEvent::Wheel { delta } if delta.is_finite() && delta != 0.0 => {
                Some(BindInput::Wheel)
            }
            _ => None,
        };
        let top = self.dialogs.len().checked_sub(1);
        if let Some(input) = binding
            && self.with_target(top, |s, c| {
                s.captures_keyboard() && s.on_bind_input(input, c)
            })
        {
            self.flush();
            return;
        }
        match ev {
            InputEvent::MouseMove { x, y } => {
                self.mouse = (x, y);
                if self.cursor_visible() {
                    let (lx, ly) = self.logical_point(x, y);
                    let t = self.mouse_target();
                    if t.is_some() {
                        self.content.view_mut().mouse_leave();
                    }
                    for (i, dialog) in self.dialogs.iter_mut().enumerate() {
                        if t != Some(i) {
                            dialog.view_mut().mouse_leave();
                        }
                    }
                    let mut out = Vec::new();
                    self.with_target(t, |s, _| s.view_mut().mouse_move(lx, ly, &mut out));
                    self.dispatch(t, out);
                }
            }
            InputEvent::MouseDelta { dx, dy } => {
                // A captured pointer always drives the camera; any screen
                // that wants the mouse shows a cursor, which releases it.
                if !self.cursor_visible()
                    && self.game_input_active()
                    && dx.is_finite()
                    && dy.is_finite()
                {
                    let sens = crate::screens::options::mouse_sensitivity(&self.core.prefs);
                    let inv = if self.core.prefs.bool_or("$pref::Input::MouseInvert", false) {
                        -1.0
                    } else {
                        1.0
                    };
                    self.core.game(GameAction::Look {
                        yaw: dx * sens * 0.005,
                        pitch: dy * sens * 0.005 * inv,
                    });
                }
            }
            InputEvent::MouseDown { button, x, y } => {
                self.mouse = (x, y);
                if self.cursor_visible() {
                    let (lx, ly) = self.logical_point(x, y);
                    let t = self.mouse_target();
                    // `GuiMLTextCtrl::onURL` -> `gotoWebPage`, behind a
                    // confirmation: chat links come from other players.
                    if t.is_none()
                        && button == MouseButton::Left
                        && self.content.id() == ScreenId::Play
                        && let Some(url) =
                            crate::screens::play::chat_link_at(&self.core, lx, ly)
                                .and_then(|u| web_url(&u))
                    {
                        self.core.message_yes_no(
                            "Open Link",
                            &format!("Open this link in your web browser?\n\n{url}"),
                            Callback::OpenUrl(url),
                        );
                        self.flush();
                        return;
                    }
                    let pack = self.core.pack.clone();
                    let mut out = Vec::new();
                    self.with_target(t, |s, _| {
                        s.view_mut().mouse_move(lx, ly, &mut Vec::new());
                        s.view_mut().mouse_down(button, lx, ly, &pack, &mut out)
                    });
                    self.dispatch(t, out);
                } else if self.game_input_active() && self.dialogs.is_empty() {
                    if let Some(cmd) = self
                        .core
                        .binds
                        .command_for(&BindInput::Mouse(button))
                        .map(str::to_string)
                    {
                        self.core.held.insert(HeldInput::Mouse(button), cmd.clone());
                        self.core.run_command(&cmd, true);
                    }
                    self.flush();
                }
            }
            InputEvent::MouseUp { button, x, y } => {
                if let Some(cmd) = self.core.held.remove(&HeldInput::Mouse(button))
                    && !self
                        .core
                        .held
                        .values()
                        .any(|c| c.eq_ignore_ascii_case(&cmd))
                {
                    self.core.run_command(&cmd, false);
                }
                let (lx, ly) = self.logical_point(x, y);
                let t = self.mouse_target();
                let pack = self.core.pack.clone();
                let mut out = Vec::new();
                self.with_target(t, |s, _| {
                    s.view_mut().mouse_up(button, lx, ly, &pack, &mut out)
                });
                self.dispatch(t, out);
            }
            InputEvent::Wheel { delta } => {
                if !delta.is_finite() || delta == 0.0 {
                    return;
                }
                // High-resolution wheels and touchpads send fractions of a
                // notch; v20 (DirectInput, 120 per notch) acts once per notch,
                // so menus and scrollInventory both step on whole notches.
                if self.wheel_rest != 0.0 && self.wheel_rest.signum() != delta.signum() {
                    self.wheel_rest = 0.0;
                }
                self.wheel_rest += delta;
                let steps = self.wheel_rest.trunc();
                self.wheel_rest -= steps;
                if steps == 0.0 {
                    return;
                }
                if self.cursor_visible() {
                    let t = self.mouse_target();
                    if self.with_target(t, |s, _| s.view_mut().wheel(steps as i32)) {
                        return;
                    }
                }
                // scrollInventory: ignored while any dialog other than the
                // chat HUD is open (Canvas count > 2), and on LoadingGui.
                let dialogs = self.dialogs.iter().filter(|d| !d.passive()).count();
                if self.content.id() == ScreenId::Play && dialogs == 0 && self.core.tool_takes_wheel() {
                    let most = NUM_WHEEL_STEPS as i32;
                    let notches = (steps as i32).clamp(-most, most);
                    self.core.game(GameAction::ToolWheel { notches });
                    self.flush();
                    return;
                }
                if self.content.id() == ScreenId::Play && dialogs == 0 && self.core.wheel_camera {
                    let most = NUM_WHEEL_STEPS as i32;
                    let notches = (steps as i32).clamp(-most, most);
                    self.core.game(GameAction::CameraZoom { notches });
                    self.flush();
                    return;
                }
                if self.content.id() == ScreenId::Play && dialogs == 0 {
                    match self.core.binds.command_for(&BindInput::Wheel) {
                        Some(c) if c.eq_ignore_ascii_case("scrollInventory") => {
                            for _ in 0..(steps.abs() as usize).min(NUM_WHEEL_STEPS) {
                                self.core.wheel_scroll(steps.signum());
                            }
                        }
                        _ => {}
                    }
                    self.flush();
                }
            }
            InputEvent::KeyDown { key, mods, repeat } => self.key_down(key, mods, repeat),
            InputEvent::KeyUp { key, mods } => {
                self.mods = mods;
                if let Some(cmd) = self.core.held.remove(&HeldInput::Key(key))
                    && !self
                        .core
                        .held
                        .values()
                        .any(|c| c.eq_ignore_ascii_case(&cmd))
                {
                    self.core.run_command(&cmd, false);
                    self.flush();
                }
            }
            InputEvent::Char(_) if std::mem::take(&mut self.swallow_char) => {}
            InputEvent::Char(ch) => {
                let t = self.dialogs.len().checked_sub(1);
                let mut out = Vec::new();
                let used = self.with_target(t, |s, _| s.view_mut().char(ch, &mut out));
                if used {
                    self.dispatch(t, out);
                }
            }
            InputEvent::FocusLost => {
                self.core.release_all();
                self.wheel_rest = 0.0;
                self.mods = Modifiers::NONE;
                self.content.view_mut().pressed = None;
                self.content.view_mut().mouse_leave();
                for d in &mut self.dialogs {
                    d.view_mut().pressed = None;
                    d.view_mut().mouse_leave();
                    d.view_mut().close_popup();
                }
                self.flush();
            }
        }
    }

    fn key_down(&mut self, key: Key, mods: Modifiers, repeat: bool) {
        self.mods = mods;
        self.swallow_char = false;
        // 1. Global action map (console, fullscreen, help). While a text box
        // has focus, Shift or AltGr with a key types that key's character
        // (`~` on the console key), so only an exact chord matches: the
        // bare-key fallback is for play, where Shift is held to crouch.
        let typing = (mods.shift || mods.alt) && self.takes_text();
        let global = if typing {
            self.core.globals.command_for(&BindInput::Key(Chord { mods, key }))
        } else {
            self.core.globals.command_for_key(key, mods)
        };
        if !repeat
            && let Some(cmd) = global.map(str::to_string)
        {
            self.core.run_command(&cmd, true);
            self.swallow_char = true;
            self.flush();
            return;
        }
        let top = self.dialogs.len().checked_sub(1);
        // 2. An open dropdown list owns the keyboard (Escape closes only it).
        if self.with_target(top, |s, _| s.view().open_popup_node().is_some()) {
            let mut out = Vec::new();
            self.with_target(top, |s, _| s.view_mut().key(key, mods, &mut out));
            self.dispatch(top, out);
            return;
        }
        // The key that opens a dialog toggles it, like the console's.
        if !repeat
            && let Some(t) = top
            && let Some(opening) = self.dialogs[t].opening_command()
            && self
                .core
                .binds
                .command_for_key(key, mods)
                .is_some_and(|c| c.eq_ignore_ascii_case(opening))
        {
            let id = self.dialogs[t].id();
            self.core.pop(id);
            self.swallow_char = true;
            self.flush();
            return;
        }
        // 3. The top screen's own key handling (remap capture, text entry).
        if self.with_target(top, |s, c| s.on_key(key, mods, c)) {
            self.flush();
            return;
        }
        // 4. Focused text control.
        let mut out = Vec::new();
        let used = self.with_target(top, |s, _| s.view_mut().key(key, mods, &mut out));
        if used {
            self.dispatch(top, out);
            return;
        }
        // 5. Accelerators, topmost screen first (fire on repeat too).
        let n = self.dialogs.len();
        for i in (0..=n).rev() {
            let idx = i.checked_sub(1);
            let hit = self.with_target(idx, |s, _| s.view().accelerator(key, mods));
            if let Some(node) = hit {
                self.dispatch(
                    idx,
                    vec![ViewEvent {
                        node,
                        kind: EventKind::Click,
                    }],
                );
                return;
            }
            let modal = idx
                .is_some_and(|i| self.dialogs[i].modal() && self.dialogs[i].blocks_accelerators());
            if modal {
                break;
            }
        }
        // 6. Gameplay binds (key repeats are not delivered to bound
        //    functions; v20 repeats bricks itself).
        if repeat || self.core.held.contains_key(&HeldInput::Key(key)) || !self.game_input_active()
        {
            return;
        }
        if self.text_focus()
            || self
                .dialogs
                .iter()
                .any(|d| d.captures_keyboard() || d.modal())
        {
            return;
        }
        let Some(cmd) = self
            .core
            .binds
            .command_for_key(key, mods)
            .map(str::to_string)
        else {
            // Keys the base game leaves unbound may belong to a package HUD.
            if let Key::Letter(letter) = key
                && mods == Modifiers::NONE
                && let Some(k) = self.core.package_keys.iter().find(|k| k.key == letter)
            {
                let action = GameAction::Package {
                    package: k.package.clone(),
                    command: k.command.clone(),
                    pressed: None,
                };
                self.core.game(action);
                self.flush();
            }
            return;
        };
        // NoShiftMoveMap: while a wrench dialog or the chat input is open,
        // left shift is unbound (so typing capitals does not crouch).
        if key == Key::LShift && self.dialogs.iter().any(|d| d.no_shift()) {
            return;
        }
        self.core.held.insert(HeldInput::Key(key), cmd.clone());
        self.core.run_command(&cmd, true);
        self.flush();
    }

    /// Advance timers (animations, key repeat, print timeouts).
    pub fn update(&mut self, dt_ms: u64) {
        self.apply_scale_pref();
        let c = &mut self.core;
        c.time_ms = c.time_ms.saturating_add(dt_ms);
        c.keyboard_turn(dt_ms);
        c.damage_flash = (c.damage_flash - dt_ms as f32 / 1000.0).max(0.0);
        c.whiteout = (c.whiteout - dt_ms as f32 / 1000.0).max(0.0);
        let now = c.time_ms;
        for cmd in c.repeater.due(now) {
            let (super_mode, base) = match cmd.strip_prefix("super:") {
                Some(b) => (true, b.to_string()),
                None => (false, cmd.clone()),
            };
            if base == "plantbrick" {
                c.game(GameAction::PlantBrick);
                continue;
            }
            let dirs = [
                ("shiftbrickaway", 1, 0, 0),
                ("shiftbricktowards", -1, 0, 0),
                ("shiftbrickleft", 0, 1, 0),
                ("shiftbrickright", 0, -1, 0),
                ("shiftbrickup", 0, 0, 3),
                ("shiftbrickdown", 0, 0, -3),
                ("shiftbrickthirdup", 0, 0, 1),
                ("shiftbrickthirddown", 0, 0, -1),
                ("supershiftbrickawayproxy", 1, 0, 0),
                ("supershiftbricktowardsproxy", -1, 0, 0),
                ("supershiftbrickleftproxy", 0, 1, 0),
                ("supershiftbrickrightproxy", 0, -1, 0),
                ("supershiftbrickupproxy", 0, 0, 1),
                ("supershiftbrickdownproxy", 0, 0, -1),
            ];
            if let Some((_, x, y, z)) = dirs.iter().find(|d| d.0 == base) {
                c.shift_brick(super_mode, *x, *y, *z);
            }
        }
        if let Some((_, Some(t))) = &c.center_print
            && *t <= now
        {
            c.center_print = None;
        }
        if let Some((_, Some(t), _)) = &c.bottom_print
            && *t <= now
        {
            c.bottom_print = None;
        }
        if let Some((_, t)) = c.plant_error
            && t <= now
        {
            c.plant_error = None;
        }
        c.captions.retain(|(_, until)| *until > now);
        // A request nothing answered must not leave its screen waiting
        // forever: answer it with a timeout, as a lost reply would be.
        let overdue = c.take_overdue();
        for id in overdue {
            let kind = self
                .core
                .pending
                .get(&id)
                .map(|k| format!("{k:?}"))
                .unwrap_or_else(|| "Admin".into());
            bri_console::warn(format!(
                "Request {id} ({kind}) got no answer in time; giving up on it"
            ));
            self.apply(UiUpdate::ActionResult {
                id,
                result: Err(REQUEST_TIMED_OUT.into()),
            });
            self.core.remember_abandoned(id);
        }
        let c = &mut self.core;
        c.hud.tick(dt_ms);
        self.content.view_mut().tick(dt_ms);
        self.content.tick(dt_ms, &mut self.core);
        for d in &mut self.dialogs {
            d.view_mut().tick(dt_ms);
            d.tick(dt_ms, &mut self.core);
        }
        self.flush();
    }

    /// The welcome page of each running Add-On the player has not met yet
    /// (Slayer's start page on first run), once per Add-On.
    pub fn welcome_addons(&mut self) {
        let c = &mut self.core;
        // Shown once per Add-On, or on every join with the Options toggle.
        let always = c.prefs.bool_or(crate::screens::options::WELCOME_ALWAYS, false);
        let Some(page) = c.addon_help.iter().find(|p| {
            p.welcome
                && (always || !c.prefs.bool_or(&format!("$Pref::AddOnWelcome::{}", p.package), false))
        }) else {
            return;
        };
        let (package, name) = (page.package.clone(), page.name.clone());
        c.prefs.set(&format!("$Pref::AddOnWelcome::{package}"), "1");
        c.save_settings();
        if !c.help_open {
            c.get_help(Some(name));
        }
    }

    /// Enabled packages' binds (their `binds.json`): listed in Options →
    /// Controls after the game's own under their divisions, each bound to
    /// its default key while both the bind and the key are free. The list
    /// stays as it is while Options is open.
    pub fn set_package_binds(&mut self, binds: Vec<crate::api::PackageBind>) {
        let c = &mut self.core;
        if c.package_binds == binds || c.options_open {
            return;
        }
        let mut remap = crate::binds::remap_entries(&c.pack.data.data);
        let mut division: Option<&str> = None;
        for bind in &binds {
            let command = bind.bind_command();
            remap.push(crate::schema::RemapEntry {
                division: (division != Some(bind.division.as_str())).then(|| bind.division.clone()),
                name: bind.name.clone(),
                command: command.clone(),
            });
            division = Some(&bind.division);
            if let Some(input) = bind
                .key
                .as_deref()
                .and_then(|key| BindInput::parse(crate::schema::Device::Keyboard, key))
                && c.binds.binding_of(&command).is_none()
                && c.binds.command_for(&input).is_none()
            {
                c.binds.bind(input, &command);
            }
        }
        c.remap_commands = remap.iter().map(|r| r.command.clone()).collect();
        c.remap = remap;
        c.package_binds = binds;
    }

    /// Build the frame's draw list (logical pixels).
    pub fn draw(&self) -> DrawList {
        let (w, h) = self.core.logical;
        let mut dl = DrawList::new(Rect::new(0, 0, w, h));
        let pack = &self.core.pack;
        self.content.draw(pack, &mut dl, &self.core);
        for d in &self.dialogs {
            d.draw(pack, &mut dl, &self.core);
        }
        crate::screens::perf::draw(pack, &mut dl, &self.core);
        dl
    }

    /// Settings as they would be persisted now.
    pub fn settings(&self) -> Settings {
        let mut s = self.core.settings.clone();
        s.prefs = self.core.prefs.overrides();
        s.binds = Some(self.core.binds.entries.clone());
        s.brick_favorites = self.core.selector.favorites.clone();
        s
    }

    /// Current modifier state as last reported.
    pub fn modifiers(&self) -> Modifiers {
        self.mods
    }

    /// Test/host helper: physical position of a control's centre.
    pub fn control_center(&self, screen: ScreenId, name_or_command: &str) -> Option<(f32, f32)> {
        let s = self.screen(screen)?;
        let v = s.view();
        let id = v
            .id(name_or_command)
            .or_else(|| v.by_command(name_or_command))
            .or_else(|| {
                v.walk().find(|&n| {
                    v.is_shown(n)
                        && v.text_of(n).trim() == name_or_command.trim()
                        && !v.text_of(n).trim().is_empty()
                })
            })?;
        let r = v.node(id).rect;
        let sc = self.scale();
        Some((
            (r.x as f32 + r.w as f32 / 2.0) * sc,
            (r.y as f32 + r.h as f32 / 2.0) * sc,
        ))
    }

    /// Chord text for help strings.
    pub fn chord_label(c: &Chord) -> String {
        c.label()
    }
}

#[cfg(test)]
mod sound_tests {
    use super::*;
    use crate::schema::{Control, UiPack};

    fn fixture() -> Ui {
        let mut data = UiPack::default();
        for (layout, names) in [
            (
                "MainMenuGui",
                vec![
                    "MM_TutorialButton",
                    "MM_StartButton",
                    "MM_JoinButton",
                    "MM_PlayerButton",
                    "MM_OptionsButton",
                    "MM_DemoButton",
                    "MM_QuitButton",
                    "MM_AboutButton",
                    "MM_CreditsButton",
                ],
            ),
            (
                "escapeMenu",
                vec![
                    "EM_Options",
                    "EM_PlayerList",
                    "EM_MiniGames",
                    "EM_AdminMenu",
                    "EM_SaveBricks",
                    "EM_LoadBricks",
                    "EM_Disconnect",
                    "EM_Quit",
                ],
            ),
        ] {
            let mut root = Control {
                class: "GuiControl".into(),
                extent: [640, 480],
                visible: true,
                ..Default::default()
            };
            for (i, name) in names.into_iter().enumerate() {
                root.children.push(Control {
                    class: "GuiBitmapButtonCtrl".into(),
                    name: Some(name.into()),
                    position: [20, 20 + i as i32 * 40],
                    extent: [120, 30],
                    visible: true,
                    ..Default::default()
                });
            }
            data.layouts.insert(layout.into(), root);
        }
        Ui::new(
            Rc::new(Pack::from_parts(data, Default::default())),
            UiConfig {
                size: (640, 480),
                scale: Some(1.0),
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        )
    }
    fn hover(ui: &mut Ui, id: ScreenId, name: &str) {
        let (x, y) = ui.control_center(id, name).unwrap();
        ui.handle_input(InputEvent::MouseMove { x, y });
    }
    fn outside(ui: &mut Ui) {
        ui.handle_input(InputEvent::MouseMove { x: 600.0, y: 450.0 });
    }

    fn has_message(ui: &Ui) -> bool {
        ui.stack().contains(&ScreenId::MessageBox)
    }

    #[test]
    fn an_unanswered_request_times_out_once_and_its_late_answer_is_dropped() {
        let mut ui = fixture();
        let id = ui.core.request_pending(
            UiAction::SetPrint {
                print: "Letters/A".into(),
            },
            Pending::Print,
        );
        ui.drain_actions();
        ui.update(Pending::Print.timeout_ms() - 1);
        assert!(ui.core.is_pending(&Pending::Print), "still inside its time");
        ui.update(1);
        assert!(!ui.core.is_pending(&Pending::Print), "given up on");
        assert!(has_message(&ui), "the player is told");
        ui.core.pop(ScreenId::MessageBox);
        ui.update(0);
        assert!(!has_message(&ui));
        ui.apply(UiUpdate::ActionResult {
            id,
            result: Err("late refusal".into()),
        });
        ui.update(0);
        assert!(!has_message(&ui), "a late answer is dropped");
    }

    #[test]
    fn an_answered_request_never_times_out() {
        let mut ui = fixture();
        let id = ui.core.request_pending(
            UiAction::SetPrint {
                print: "Letters/A".into(),
            },
            Pending::Print,
        );
        ui.apply(UiUpdate::ActionResult { id, result: Ok(()) });
        ui.update(Pending::Print.timeout_ms() * 2);
        assert!(!has_message(&ui));
    }

    #[test]
    fn an_abandoned_request_is_forgotten_and_its_answer_is_dropped() {
        let mut ui = fixture();
        let id = ui.core.request_pending(
            UiAction::SetPrint {
                print: "Letters/A".into(),
            },
            Pending::Print,
        );
        ui.core.abandon(id);
        assert!(!ui.core.is_pending(&Pending::Print));
        ui.apply(UiUpdate::ActionResult {
            id,
            result: Err("refused after leaving".into()),
        });
        ui.update(Pending::Print.timeout_ms() * 2);
        assert!(!has_message(&ui));
    }

    #[test]
    fn source_note_map_main_and_escape_menus_is_exact() {
        let mut ui = fixture();
        ui.drain_actions();
        for (name, note) in [
            ("MM_TutorialButton", 3),
            ("MM_StartButton", 4),
            ("MM_JoinButton", 5),
            ("MM_PlayerButton", 6),
            ("MM_OptionsButton", 7),
            ("MM_DemoButton", 8),
            ("MM_QuitButton", 0),
            ("MM_AboutButton", 1),
            ("MM_CreditsButton", 2),
        ] {
            hover(&mut ui, ScreenId::MainMenu, name);
            let sounds = ui.drain_sounds();
            assert_eq!(sounds.len(), 1);
            assert_eq!(sounds[0].profile, format!("Note{note}Sound"));
            assert_eq!(sounds[0].trigger, format!("ui.menu_note.{note}"));
        }
        ui.core.push(ScreenId::EscapeMenu);
        ui.flush();
        for (name, note) in [
            ("EM_Options", 0),
            ("EM_PlayerList", 1),
            ("EM_MiniGames", 2),
            ("EM_AdminMenu", 3),
            ("EM_SaveBricks", 4),
            ("EM_LoadBricks", 5),
            ("EM_Disconnect", 6),
            ("EM_Quit", 7),
        ] {
            hover(&mut ui, ScreenId::EscapeMenu, name);
            let sounds = ui.drain_sounds();
            assert_eq!(sounds.len(), 1);
            assert_eq!(sounds[0].profile, format!("Note{note}Sound"));
        }
        assert!(ui.drain_actions().is_empty());
        assert!(ui.core.pending.is_empty());
    }

    #[test]
    fn hover_only_once_until_reentry_and_no_click_or_message_sound() {
        let mut ui = fixture();
        hover(&mut ui, ScreenId::MainMenu, "MM_StartButton");
        assert_eq!(ui.drain_sounds().len(), 1);
        hover(&mut ui, ScreenId::MainMenu, "MM_StartButton");
        ui.update(100);
        assert!(ui.drain_sounds().is_empty());
        outside(&mut ui);
        hover(&mut ui, ScreenId::MainMenu, "MM_StartButton");
        assert_eq!(ui.drain_sounds().len(), 1);
        let (x, y) = ui
            .control_center(ScreenId::MainMenu, "MM_StartButton")
            .unwrap();
        ui.handle_input(InputEvent::MouseDown {
            button: MouseButton::Left,
            x,
            y,
        });
        ui.handle_input(InputEvent::MouseUp {
            button: MouseButton::Left,
            x,
            y,
        });
        assert!(ui.drain_sounds().is_empty());
        ui.core
            .message_ok("Info", "No invented generic message cue");
        ui.flush();
        assert!(ui.drain_sounds().is_empty());
    }

    #[test]
    fn inactive_hidden_muted_and_modal_controls_do_not_emit_underlying_notes() {
        let mut ui = fixture();
        let n = ui.content.view().id("MM_StartButton").unwrap();
        ui.content.view_mut().set_active(n, false);
        hover(&mut ui, ScreenId::MainMenu, "MM_StartButton");
        assert!(ui.drain_sounds().is_empty());
        ui.content.view_mut().set_active(n, true);
        ui.content.view_mut().set_visible(n, false);
        hover(&mut ui, ScreenId::MainMenu, "MM_StartButton");
        assert!(ui.drain_sounds().is_empty());
        ui.content.view_mut().set_visible(n, true);
        ui.core.prefs.set_bool("$Pref::Audio::MenuSounds", false);
        hover(&mut ui, ScreenId::MainMenu, "MM_StartButton");
        assert!(ui.drain_sounds().is_empty());
        ui.core.prefs.set_bool("$Pref::Audio::MenuSounds", true);
        outside(&mut ui);
        hover(&mut ui, ScreenId::MainMenu, "MM_StartButton");
        ui.drain_sounds();
        ui.core.message_ok("Overlay", "Modal");
        ui.flush();
        hover(&mut ui, ScreenId::MainMenu, "MM_StartButton");
        assert!(ui.drain_sounds().is_empty());
        ui.core.pop(ScreenId::MessageBox);
        ui.flush();
        hover(&mut ui, ScreenId::MainMenu, "MM_StartButton");
        assert_eq!(ui.drain_sounds().len(), 1);
        ui.handle_input(InputEvent::FocusLost);
        hover(&mut ui, ScreenId::MainMenu, "MM_StartButton");
        assert_eq!(ui.drain_sounds().len(), 1);
    }

    /// A score report opens its window and a closing one shuts it; with
    /// "Hide end of round reports" (Slayer's client preference) it never
    /// opens.
    #[test]
    fn a_report_opens_its_window_unless_the_player_hides_reports() {
        let report = || {
            UiUpdate::Report(Some(crate::api::ReportView {
                title: "End of Round Report".into(),
                ..Default::default()
            }))
        };
        let mut ui = fixture();
        ui.apply(report());
        ui.flush();
        assert!(ui.is_open(ScreenId::Report));
        ui.apply(UiUpdate::Report(None));
        ui.flush();
        assert!(!ui.is_open(ScreenId::Report));
        ui.core
            .prefs
            .set_bool(crate::screens::options::HIDE_REPORTS, true);
        ui.apply(report());
        ui.flush();
        assert!(!ui.is_open(ScreenId::Report));
    }

    #[test]
    fn plant_errors_have_separate_preference_and_bounded_observable_outbox() {
        let mut ui = fixture();
        ui.drain_actions();
        ui.apply(UiUpdate::PlantError(PlantError::Overlap));
        assert!(ui.drain_sounds().is_empty());
        ui.core
            .prefs
            .set_bool("$Pref::Audio::PlantErrorSound", true);
        ui.core.prefs.set_bool("$Pref::Audio::MenuSounds", false);
        for _ in 0..MAX_QUEUED_SOUNDS + 7 {
            ui.apply(UiUpdate::PlantError(PlantError::Float));
        }
        assert_eq!(ui.dropped_sounds(), 7);
        let sounds = ui.drain_sounds();
        assert_eq!(sounds.len(), MAX_QUEUED_SOUNDS);
        assert!(sounds.iter().all(|s| *s == AUDIO_ERROR));
        assert!(ui.drain_sounds().is_empty());
        assert!(ui.drain_actions().is_empty());
        ui.apply(UiUpdate::PlantError(PlantError::Overlap));
        assert_eq!(ui.drain_sounds(), vec![AUDIO_ERROR]);
    }
}

#[cfg(test)]
mod hud_entry_tests {
    use super::*;
    use crate::models::hud::ScrollMode;

    #[test]
    fn first_game_entry_hides_trays_late_catalogs_resize_and_metadata_preserves_slide() {
        let mut ui = Ui::new(
            Rc::new(Pack::from_parts(Default::default(), Default::default())),
            UiConfig {
                size: (640, 480),
                scale: Some(1.0),
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        );
        let game = || ConnectionState::InGame {
            server_name: "Test".into(),
            max_players: 1,
            local: true,
            single_player: true,
            admin: false,
        };
        ui.apply(UiUpdate::Connection(game()));
        assert_eq!(ui.core.hud.brick_slide.offset, 64);
        assert_eq!(ui.core.hud.paint_slide.offset, 23);
        assert_eq!(ui.core.hud.tool_slide.offset, 320);
        ui.apply(UiUpdate::Colorset(vec![PaintDivision {
            name: "Standard".into(),
            colors: vec![[1.0; 4]; 8],
        }]));
        assert_eq!(ui.core.hud.paint_slide.offset, 40);
        ui.apply(UiUpdate::Tools(vec![None; 3]));
        assert_eq!(ui.core.hud.tool_slide.offset, 192);

        ui.core
            .hud
            .set_scroll_mode(ScrollMode::Paint, &mut Outbox::default());
        ui.core
            .hud
            .set_scroll_mode(ScrollMode::None, &mut Outbox::default());
        let running = ui.core.hud.paint_slide.clone();
        assert!(running.moving());
        ui.apply(UiUpdate::Connection(game()));
        assert_eq!(ui.core.hud.paint_slide, running);
        ui.core.hud.paint_slide.finish();
        assert_eq!(ui.core.hud.paint_slide.offset, 40);
        assert!(ui.drain_actions().is_empty());
    }
}
