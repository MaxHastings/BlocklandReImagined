//! Screens: authored v20 layouts driven by models. Each screen maps the
//! original control `command` strings (kept for provenance, never executed)
//! to typed behaviour.

pub mod addons;
pub mod admin;
pub mod avatar;
pub mod colorwarn;
pub mod console;
pub mod help;
pub mod menus;
pub mod modes;
pub mod music;
pub mod minigames;
pub mod options;
pub mod perf;
pub mod play;
pub mod players;
pub mod saveload;
pub mod selector;
pub mod trust;
pub mod wrench;

use crate::api::{ChatChannel, MiniGameOperation, MiniGamePlayerId, RequestId, WrenchVariant};
use crate::draw::DrawList;
use crate::geom::{Rect, Rgba};
use crate::input::{Key, Modifiers};
use crate::pack::Pack;
use crate::schema::{Control, HSizing, VSizing};
use crate::ui::{Core, Pending};
use crate::view::{NodeId, View, ViewEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenId {
    MainMenu,
    DefaultControls,
    StartMission,
    JoinServer,
    ManualJoin,
    Connecting,
    Loading,
    Play,
    MessageInput(ChatChannel),
    EscapeMenu,
    Options,
    Remap,
    PlayerList,
    MiniGames,
    MiniGameSettings,
    MiniGameInvitation,
    TrustInvitation,
    Admin,
    AdminLogin,
    AdminBan,
    AdminUnban,
    AdminBricks,
    AdminMaps,
    AdminOptions,
    AdminCredentials,
    /// The saved ranks (v20's auto-admin lists).
    AdminRanks,
    AdminConfirm,
    BrickSelector,
    PrintSelector,
    Wrench(WrenchVariant),
    WrenchEvents,
    Avatar,
    SaveBricks,
    LoadBricks,
    MessageBox,
    About,
    /// v20 `ConsoleDlg` (`~`).
    Console,
    /// Installed packages: turn them on and off (native; no v20 layout).
    AddOns,
    /// Fetching a server's packages before joining (native).
    PackageDownload,
    /// A join refused over differing add-ons (native).
    AddOnMismatch,
    /// Start Game's game mode picker (native; v20 had none).
    GameModes,
    /// Start Game's Advanced Config (`serverConfigGui` over the saved
    /// `$Pref::Server::*`), before a game is hosted.
    ServerConfig,
    /// v20 `HelpDlg`: the main menu's Credits button and F1.
    Help,
    /// Start Game's Music Files: the loops a hosted game offers.
    MusicFiles,
    /// v20 `LoadBricksColorGui`: how to load a save's differing colours.
    LoadBricksColor,
}

pub trait Screen {
    fn id(&self) -> ScreenId;
    fn view(&self) -> &View;
    fn view_mut(&mut self) -> &mut View;
    /// Modal screens take all mouse input and stop accelerator search.
    fn modal(&self) -> bool {
        true
    }
    /// Non-modal screens that still take mouse input when on top.
    fn wants_mouse(&self) -> bool {
        false
    }
    fn cursor(&self) -> bool {
        true
    }
    /// Overlays that do not count as open dialogs (chat HUD).
    fn passive(&self) -> bool {
        false
    }
    fn blocks_accelerators(&self) -> bool {
        true
    }
    /// The bound command that opens this dialog; its key closes it again.
    fn opening_command(&self) -> Option<&'static str> {
        None
    }
    /// Screens that take every key (remap capture).
    fn captures_keyboard(&self) -> bool {
        false
    }
    /// `NoShiftMoveMap` while open (wrench dialogs, chat input).
    fn no_shift(&self) -> bool {
        false
    }
    fn on_wake(&mut self, _core: &mut Core) {}
    fn on_sleep(&mut self, _core: &mut Core) {}
    fn on_event(&mut self, _ev: &ViewEvent, _core: &mut Core) {}
    /// Optional exclusive binding capture, before global shortcuts/widgets.
    fn on_bind_input(&mut self, _input: crate::api::BindInput, _core: &mut Core) -> bool {
        false
    }
    fn on_key(&mut self, _key: Key, _mods: Modifiers, _core: &mut Core) -> bool {
        false
    }
    /// View models changed.
    fn on_update(&mut self, _core: &mut Core) {}
    /// A request answered; return true if this screen handled it.
    fn on_result(
        &mut self,
        _id: RequestId,
        _kind: Option<&Pending>,
        _result: &Result<(), String>,
        _core: &mut Core,
    ) -> bool {
        false
    }
    fn tick(&mut self, _dt_ms: u64, _core: &mut Core) {}
    fn layout(&mut self, w: i32, h: i32, _core: &mut Core) {
        self.view_mut().layout(w, h);
    }
    fn draw(&self, pack: &Pack, dl: &mut DrawList, _core: &Core) {
        self.view().draw(pack, dl);
    }
}

pub fn make(id: ScreenId, core: &mut Core) -> Box<dyn Screen> {
    match id {
        ScreenId::SaveBricks | ScreenId::LoadBricks => {
            return Box::new(saveload::SaveLoad::new(id, core));
        }
        ScreenId::PlayerList => return Box::new(players::Players::new(core)),
        ScreenId::MiniGames => return Box::new(minigames::MiniGameScreen::list(core)),
        ScreenId::MiniGameSettings => return Box::new(minigames::MiniGameScreen::settings(core)),
        ScreenId::MiniGameInvitation => return Box::new(minigames::MiniGameScreen::invitation(core)),
        ScreenId::TrustInvitation => return Box::new(trust::TrustInvite::new(core)),
        ScreenId::Admin
        | ScreenId::AdminLogin
        | ScreenId::AdminBan
        | ScreenId::AdminUnban
        | ScreenId::AdminBricks
        | ScreenId::AdminMaps
        | ScreenId::AdminOptions
        | ScreenId::AdminCredentials
        | ScreenId::AdminRanks
        | ScreenId::AdminConfirm => return Box::new(admin::AdminScreen::new(id, core)),
        ScreenId::Wrench(variant) => return Box::new(wrench::Wrench::new(core, variant)),
        ScreenId::WrenchEvents => return Box::new(wrench::WrenchEvents::new(core)),
        ScreenId::Avatar => return Box::new(avatar::Avatar::new(core)),
        ScreenId::Play => return Box::new(play::Play::new(core)),
        ScreenId::Console => return Box::new(console::Console::new(core)),
        ScreenId::AddOns => return Box::new(addons::AddOns::new(core)),
        ScreenId::PackageDownload => return Box::new(addons::PackageDownload::new(core)),
        ScreenId::AddOnMismatch => return Box::new(addons::Mismatch::new(core)),
        ScreenId::GameModes => return Box::new(modes::GameModes::new(core)),
        ScreenId::ServerConfig => return Box::new(admin::ServerConfig::new(core)),
        ScreenId::Help => return Box::new(help::Help::new(core)),
        ScreenId::MusicFiles => return Box::new(music::MusicFiles::new(core)),
        ScreenId::LoadBricksColor => return Box::new(colorwarn::ColorWarning::new(core)),
        ScreenId::Options => return Box::new(options::Options::new(core)),
        ScreenId::Remap => return Box::new(options::Remap::new(core)),
        ScreenId::BrickSelector => return Box::new(selector::BrickSelector::new(core)),
        ScreenId::PrintSelector => return Box::new(selector::PrintSelector::new(core)),
        _ => {}
    }
    if id == ScreenId::MessageBox {
        return Box::new(menus::MessageScreen::new(
            core,
            crate::ui::MessageBox {
                title: String::new(),
                text: String::new(),
                yes_no: false,
                on_yes: crate::ui::Callback::None,
                on_no: crate::ui::Callback::None,
                buttons: None,
            },
        ));
    }
    Box::new(menus::NativeScreen::new(id, core))
}

// ---------------------------------------------------------------- helpers

/// A view of an authored layout (an empty full-screen control if missing).
pub fn layout_view(core: &Core, name: &str) -> View {
    let c = core
        .pack
        .data
        .layouts
        .get(name)
        .cloned()
        .unwrap_or_else(|| Control {
            class: "GuiControl".into(),
            name: Some(name.into()),
            extent: [640, 480],
            style: "GuiDefaultProfile".into(),
            visible: true,
            ..Default::default()
        });
    let mut v = View::new(&c);
    v.measure(&core.pack);
    v
}

/// A plain control for runtime-built widgets.
pub fn ctrl(class: &str, style: &str, r: Rect) -> Control {
    Control {
        class: class.into(),
        style: style.into(),
        position: [r.x, r.y],
        extent: [r.w, r.h],
        visible: true,
        h_sizing: HSizing::Right,
        v_sizing: VSizing::Bottom,
        ..Default::default()
    }
}

pub fn bitmap(style: &str, r: Rect, image: &str) -> Control {
    let mut c = ctrl("GuiBitmapCtrl", style, r);
    c.bitmap = Some(image.into());
    c
}

pub fn swatch(r: Rect, color: Rgba) -> Control {
    let mut c = ctrl("GuiSwatchCtrl", "GuiDefaultProfile", r);
    c.color = Some(color);
    c
}

pub fn text(style: &str, r: Rect, t: &str) -> Control {
    let mut c = ctrl("GuiTextCtrl", style, r);
    c.text = Some(t.into());
    c
}

pub fn button(style: &str, r: Rect, image: &str, label: &str, command: &str) -> Control {
    let mut c = ctrl("GuiBitmapButtonCtrl", style, r);
    c.bitmap = Some(image.into());
    c.text = Some(label.into());
    c.command = Some(command.into());
    c
}

/// Float colour (0..1) → bytes.
pub fn rgba(c: [f32; 4]) -> Rgba {
    crate::geom::from_f32(c)
}

/// Id of a named control, or the first control with this command.
pub fn find(v: &View, key: &str) -> Option<NodeId> {
    v.id(key).or_else(|| v.by_command(key))
}

/// Command string of the event's control (or its first ancestor with one).
pub fn command_of(v: &View, mut id: NodeId) -> String {
    loop {
        let n = v.node(id);
        if let Some(c) = &n.ctrl.command {
            return c.clone();
        }
        match n.parent {
            Some(p) => id = p,
            None => return String::new(),
        }
    }
}

/// Visible window (first GuiWindowCtrl) of a dialog.
pub fn window(v: &View) -> Option<NodeId> {
    v.walk().find(|&n| v.node(n).ctrl.class == "GuiWindowCtrl")
}
