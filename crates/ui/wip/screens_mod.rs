//! Screens: authored v20 layouts driven by models. Each screen maps the
//! original control `command` strings (kept for provenance, never executed)
//! to typed behaviour.

pub mod avatar;
pub mod menus;
pub mod options;
pub mod play;
pub mod saveload;
pub mod selector;
pub mod wrench;

use crate::api::{ChatChannel, RequestId, WrenchVariant};
use crate::draw::DrawList;
use crate::geom::{Rect, Rgba};
use crate::input::{Key, Modifiers};
use crate::pack::Pack;
use crate::schema::{Control, HSizing, VSizing};
use crate::ui::{Core, Pending};
use crate::view::{NodeId, View, ViewEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    BrickSelector,
    PrintSelector,
    Wrench(WrenchVariant),
    WrenchEvents,
    Avatar,
    SaveBricks,
    LoadBricks,
    MessageBox,
    About,
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
        false
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
    fn on_key(&mut self, _key: Key, _mods: Modifiers, _core: &mut Core) -> bool {
        false
    }
    /// View models changed.
    fn on_update(&mut self, _core: &mut Core) {}
    /// A request answered; return true if this screen handled it.
    fn on_result(&mut self, _id: RequestId, _kind: Option<&Pending>, _result: &Result<(), String>, _core: &mut Core) -> bool {
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
        ScreenId::MainMenu => Box::new(menus::MainMenu::new(core)),
        ScreenId::DefaultControls => Box::new(menus::DefaultControls::new(core)),
        ScreenId::StartMission => Box::new(menus::StartMission::new(core)),
        ScreenId::JoinServer => Box::new(menus::JoinServer::new(core)),
        ScreenId::ManualJoin => Box::new(menus::ManualJoin::new(core)),
        ScreenId::Connecting => Box::new(menus::Connecting::new(core)),
        ScreenId::Loading => Box::new(menus::Loading::new(core)),
        ScreenId::EscapeMenu => Box::new(menus::EscapeMenu::new(core)),
        ScreenId::PlayerList => Box::new(menus::PlayerList::new(core)),
        ScreenId::About => Box::new(menus::About::new(core)),
        ScreenId::Play => Box::new(play::Play::new(core)),
        ScreenId::MessageInput(ch) => Box::new(play::MessageInput::new(core, ch)),
        ScreenId::Options => Box::new(options::Options::new(core)),
        ScreenId::Remap => Box::new(options::Remap::new(core)),
        ScreenId::BrickSelector => Box::new(selector::BrickSelector::new(core)),
        ScreenId::PrintSelector => Box::new(selector::PrintSelector::new(core)),
        ScreenId::Wrench(v) => Box::new(wrench::Wrench::new(core, v)),
        ScreenId::WrenchEvents => Box::new(wrench::WrenchEvents::new(core)),
        ScreenId::Avatar => Box::new(avatar::Avatar::new(core)),
        ScreenId::SaveBricks => Box::new(saveload::SaveBricks::new(core)),
        ScreenId::LoadBricks => Box::new(saveload::LoadBricks::new(core)),
        ScreenId::MessageBox => Box::new(menus::MessageScreen::new(core, crate::ui::MessageBox {
            title: String::new(),
            text: String::new(),
            yes_no: false,
            on_yes: crate::ui::Callback::None,
        })),
    }
}

// ---------------------------------------------------------------- helpers

/// A view of an authored layout (an empty full-screen control if missing).
pub fn layout_view(core: &Core, name: &str) -> View {
    let c = core.pack.data.layouts.get(name).cloned().unwrap_or_else(|| Control {
        class: "GuiControl".into(),
        name: Some(name.into()),
        extent: [640, 480],
        style: "GuiDefaultProfile".into(),
        visible: true,
        ..Default::default()
    });
    let mut v = View::new(&c);
    v.row_height_hint = 16;
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
