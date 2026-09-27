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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiConfig {
    /// Window size in physical pixels.
    pub size: (u32, u32),
    /// Logical→physical scale. `None` = automatic: the largest integer
    /// scale that keeps at least 640x480 logical pixels (Torque's minimum
    /// canvas), so pixel art and bitmap fonts stay crisp.
    pub scale: Option<f32>,
    pub platform: Platform,
}

impl UiConfig {
    pub fn effective_scale(&self) -> f32 {
        match self.scale {
            Some(s) if s > 0.0 => s,
            _ => {
                let s = (self.size.0 / 640).min(self.size.1 / 480).max(1);
                s as f32
            }
        }
    }
    pub fn logical(&self) -> (i32, i32) {
        let s = self.effective_scale();
        ((self.size.0 as f32 / s).floor() as i32, (self.size.1 as f32 / s).floor() as i32)
    }
}

/// Deferred screen-stack edits (applied after the current handler).
#[derive(Debug, Clone, PartialEq)]
pub enum StackCmd {
    SetContent(ScreenId),
    Push(ScreenId),
    Pop(ScreenId),
    /// Show a message box (OK or Yes/No) with a callback.
    Message(MessageBox),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageBox {
    pub title: String,
    pub text: String,
    pub yes_no: bool,
    pub on_yes: Callback,
}

/// What a message box's YES/OK does (v20 passed script strings).
#[derive(Debug, Clone, PartialEq)]
pub enum Callback {
    None,
    Quit,
    Disconnect,
    RemapForce { command: String, input: BindInput },
    ClearBinds,
    DefaultBinds,
    OverwriteSave { name: String, description: String, events: bool, ownership: bool },
    CloseEvents,
}

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum HeldInput {
    Key(Key),
    Mouse(MouseButton),
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
    pub remap_commands: Vec<String>,
    // catalogs
    pub maps: Vec<MapInfo>,
    pub servers: Vec<ServerInfo>,
    pub lan_querying: bool,
    pub bricks: Vec<BrickInfo>,
    pub prints: BTreeMap<String, Vec<PrintInfo>>,
    pub events: EventCatalog,
    pub datablocks: DatablockMenus,
    pub menu_backgrounds: Vec<IconRef>,
    pub avatar_preview: IconRef,
    pub save_maps: Vec<String>,
    pub save_files: Vec<SaveFileInfo>,
    pub save_context: Option<(String, IconRef)>,
    // live state
    pub conn: ConnectionState,
    pub hud: HudModel,
    pub chat: ChatModel,
    pub selector: SelectorModel,
    pub wrench: WrenchState,
    pub players: Vec<PlayerRow>,
    pub server_name: String,
    pub max_players: u32,
    pub center_print: Option<(String, Option<u64>)>,
    pub bottom_print: Option<(String, Option<u64>, bool)>,
    pub plant_error: Option<(PlantError, u64)>,
    pub lagging: bool,
    pub shape_names: bool,
    pub super_shift: bool,
    super_shift_time: u64,
    pub zoom_on: bool,
    pub cursor_forced: bool,
    /// Open print selector aspect ratio and last print per aspect.
    pub print_aspect: Option<String>,
    pub last_print: BTreeMap<String, String>,
    // requests
    next_id: RequestId,
    out: Vec<(RequestId, UiAction)>,
    pub pending: BTreeMap<RequestId, Pending>,
    pub cmds: Vec<StackCmd>,
    repeater: Repeater,
    held: BTreeMap<HeldInput, String>,
    held_controls: BTreeSet<HeldControl>,
}

impl Core {
    /// Queue a request; returns its id.
    pub fn request(&mut self, a: UiAction) -> RequestId {
        self.next_id += 1;
        self.out.push((self.next_id, a));
        self.next_id
    }
    pub fn request_pending(&mut self, a: UiAction, kind: Pending) -> RequestId {
        let id = self.request(a);
        self.pending.insert(id, kind);
        id
    }
    pub fn is_pending(&self, kind: &Pending) -> bool {
        self.pending.values().any(|p| p == kind)
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
        self.cmds.push(StackCmd::Message(MessageBox {
            title: title.into(),
            text: text.into(),
            yes_no: false,
            on_yes: Callback::None,
        }));
    }
    pub fn message_yes_no(&mut self, title: &str, text: &str, on_yes: Callback) {
        self.cmds.push(StackCmd::Message(MessageBox { title: title.into(), text: text.into(), yes_no: true, on_yes }));
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
    pub fn in_game(&self) -> bool {
        matches!(self.conn, ConnectionState::InGame { .. })
    }
    pub fn is_local(&self) -> bool {
        matches!(self.conn, ConnectionState::InGame { local: true, .. })
    }
    pub fn is_admin(&self) -> bool {
        matches!(self.conn, ConnectionState::InGame { admin: true, .. } | ConnectionState::InGame { local: true, .. })
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
        let ctrls: Vec<HeldControl> = std::mem::take(&mut self.held_controls).into_iter().collect();
        for c in ctrls {
            self.game(GameAction::Held { control: c, down: false });
        }
        self.repeater.cancel_all();
    }

    fn held_control(&mut self, control: HeldControl, down: bool) {
        let changed = if down { self.held_controls.insert(control) } else { self.held_controls.remove(&control) };
        if changed {
            self.game(GameAction::Held { control, down });
        }
    }

    fn one_button_jet(&self) -> bool {
        self.prefs.bool_or("$pref::Input::noobjet", false) || self.settings.mouse_type == 0
    }

    /// Run a bound command (`%val` = `down`). Returns false if unknown.
    pub fn run_command(&mut self, cmd: &str, down: bool) -> bool {
        let c = cmd.to_ascii_lowercase();
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
                "turnleft" => HeldControl::TurnLeft,
                "turnright" => HeldControl::TurnRight,
                "panup" => HeldControl::LookUp,
                "pandown" => HeldControl::LookDown,
                _ => return None,
            })
        };
        if let Some(h) = held(&c) {
            self.held_control(h, down);
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
            let smart = self.prefs.bool_or("$pref::Input::UseSuperShiftToggle", true)
                && self.prefs.bool_or("$pref::Input::UseSuperShiftSmartToggle", true);
            let super_mode = if proxy { !smart || self.super_shift } else { self.super_shift };
            let key = if super_mode { format!("super:{c}") } else { c.clone() };
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
            "escapemenu.toggle();" => self.escape_toggle(),
            "togglefirstperson" => {
                let fast = self.prefs.bool_or("$pref::Input::FastFirstThirdPerson", false);
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
            "doscreenshot" => self.game(GameAction::Screenshot { kind: ScreenshotKind::Normal }),
            "dohudscreenshot" => self.game(GameAction::Screenshot { kind: ScreenshotKind::NoHud }),
            "dodofscreenshot" => self.game(GameAction::Screenshot { kind: ScreenshotKind::DepthOfField }),
            "togglenetgraph" => self.game(GameAction::ToggleNetGraph),
            "togglefullscreen();" => self.game(GameAction::ToggleFullscreen),
            "togglebuildmacrorecording" => self.game(GameAction::ToggleBuildMacroRecording),
            "playbackbuildmacro" => self.game(GameAction::PlayBackBuildMacro),
            "emotesit" | "emotelove" | "emotehate" | "emoteconfusion" | "emotealarm" => {
                self.game(GameAction::Emote { name: c.trim_start_matches("emote").to_string() })
            }
            "globalchat" | "teamchat" => {
                if self.prefs.i64_or("$Pref::Chat::LineTime", 6500) > 0 {
                    let ch = if c == "globalchat" { ChatChannel::Say } else { ChatChannel::Team };
                    self.push(ScreenId::MessageInput(ch));
                }
            }
            "pageupnewchathud" => self.chat.page_up(),
            "pagedownnewchathud" => self.chat.page_down(),
            "togglecursor" => {
                let single = matches!(self.conn, ConnectionState::InGame { single_player: true, .. });
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
                } else {
                    // Console, help and debug render modes are excluded from the
                    // alpha (no script console); they are recognised but inert.
                    return matches!(other, "toggleconsole" | "contexthelp();" | "cycledebugrendermode");
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
        let toggle = self.prefs.bool_or("$pref::Input::UseSuperShiftToggle", true);
        let smart = self.prefs.bool_or("$pref::Input::UseSuperShiftSmartToggle", true);
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
        if matches!(self.conn, ConnectionState::Loading { .. } | ConnectionState::Connecting { .. }) {
            self.request(UiAction::CancelConnect);
            return;
        }
        self.push(ScreenId::EscapeMenu);
    }

    fn wheel_scroll(&mut self, delta: f32) {
        // scrollInventory: %val < 0 → +1 (Torque positive = wheel up).
        let dir = if delta < 0.0 { 1 } else { -1 };
        if self.zoom_on {
            let mut fov = self.prefs.f32_or("$Pref::player::CurrentFOV", 45.0);
            if dir > 0 {
                if fov > 5.0 {
                    fov -= 5.0;
                }
            } else if fov < 85.0 {
                fov += 5.0;
            }
            self.prefs.set("$Pref::player::CurrentFOV", format!("{fov}"));
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
}

impl Ui {
    /// Create the UI. `settings` are the host-persisted values (use
    /// `Settings::default()` on first run: Default Controls will show).
    pub fn new(pack: Rc<Pack>, cfg: UiConfig, settings: Settings) -> Ui {
        let prefs = Prefs::new(&pack.data.prefs, &settings.prefs);
        let platform = cfg.platform;
        let binds = match &settings.binds {
            Some(b) => BindMap { entries: b.clone() },
            None => BindMap::defaults(&pack.data, crate::binds::DEFAULT_MOUSE, crate::binds::DEFAULT_KEYBOARD, platform),
        };
        let globals = BindMap::globals(&pack.data, platform);
        let mut selector = SelectorModel { favorites: settings.brick_favorites.clone(), ..Default::default() };
        if selector.favorites.is_empty() && settings.binds.is_none() {
            selector.favorites = pack.data.favorites.clone();
        }
        selector.queue_brick_buying = prefs.bool_or("$pref::Input::QueueBrickBuying", true);
        let first = prefs.i64_or("$Pref::Input::brickFirstRepeatTime", 200).max(1) as u64;
        let rep = prefs.i64_or("$Pref::Input::brickRepeatTime", 50).max(1) as u64;
        let chat = ChatModel::new(
            prefs.i64_or("$Pref::Chat::CacheLines", 1000) as usize,
            prefs.i64_or("$Pref::Chat::MaxDisplayLines", 8) as usize,
            prefs.i64_or("$Pref::Chat::LineTime", 6500),
        );
        let remap_commands = pack.data.remap.iter().map(|r| r.command.clone()).collect();
        let mut settings = settings;
        if settings.avatar.values.is_empty() {
            settings.avatar = AvatarPrefs::from_prefs(&prefs, &pack.data.prefs);
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
            remap_commands,
            maps: Vec::new(),
            servers: Vec::new(),
            lan_querying: false,
            bricks: Vec::new(),
            prints: BTreeMap::new(),
            events: EventCatalog::default(),
            datablocks: DatablockMenus::new(),
            menu_backgrounds: Vec::new(),
            avatar_preview: IconRef::None,
            save_maps: Vec::new(),
            save_files: Vec::new(),
            save_context: None,
            conn: ConnectionState::Idle,
            hud: HudModel::default(),
            chat,
            selector,
            wrench: WrenchState::default(),
            players: Vec::new(),
            server_name: String::new(),
            max_players: 0,
            center_print: None,
            bottom_print: None,
            plant_error: None,
            lagging: false,
            shape_names: true,
            super_shift: false,
            super_shift_time: 0,
            zoom_on: false,
            cursor_forced: false,
            print_aspect: None,
            last_print: BTreeMap::new(),
            next_id: 0,
            out: Vec::new(),
            pending: BTreeMap::new(),
            cmds: Vec::new(),
            repeater: Repeater::new(first, rep),
            held: BTreeMap::new(),
            held_controls: BTreeSet::new(),
        };
        core.hud.prefs = core.hud_prefs();
        let content = screens::make(ScreenId::MainMenu, &mut core);
        let mut ui = Ui { core, content, dialogs: Vec::new(), cfg, mods: Modifiers::NONE, mouse: (-1.0, -1.0) };
        if ui.core.settings.binds.is_none() {
            ui.core.push(ScreenId::DefaultControls);
        }
        ui.flush();
        ui.relayout();
        ui
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
        self.cfg.size = size;
        self.cfg.scale = scale;
        self.core.logical = self.cfg.logical();
        self.relayout();
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

    pub fn top_id(&self) -> ScreenId {
        self.dialogs.last().map_or(self.content.id(), |d| d.id())
    }
    pub fn is_open(&self, id: ScreenId) -> bool {
        self.content.id() == id || self.dialogs.iter().any(|d| d.id() == id)
    }
    pub fn screen(&self, id: ScreenId) -> Option<&dyn Screen> {
        if self.content.id() == id {
            return Some(self.content.as_ref());
        }
        self.dialogs.iter().rev().find(|d| d.id() == id).map(|d| d.as_ref())
    }
    pub fn screen_mut(&mut self, id: ScreenId) -> Option<&mut Box<dyn Screen>> {
        if self.content.id() == id {
            return Some(&mut self.content);
        }
        self.dialogs.iter_mut().rev().find(|d| d.id() == id)
    }
    /// The stack from bottom (content) to top.
    pub fn stack(&self) -> Vec<ScreenId> {
        std::iter::once(self.content.id()).chain(self.dialogs.iter().map(|d| d.id())).collect()
    }

    /// Whether the mouse cursor is visible (dialogs, menus, or M toggled).
    pub fn cursor_visible(&self) -> bool {
        self.content.cursor() || self.dialogs.iter().any(|d| d.cursor()) || self.core.cursor_forced
    }

    /// Gameplay binds are active while the play (or loading) screen is the
    /// content (v20 pushes `moveMap` in PlayGui/LoadingGui::onWake).
    fn game_input_active(&self) -> bool {
        matches!(self.content.id(), ScreenId::Play | ScreenId::Loading)
    }

    fn text_focus(&self) -> bool {
        self.dialogs.last().map_or_else(|| self.content.view().focus.is_some(), |d| d.view().focus.is_some())
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
                let was_game = self.game_input_active();
                let mut old = std::mem::replace(&mut self.content, screens::make(id, &mut self.core));
                old.on_sleep(&mut self.core);
                // v20 pops every dialog when the content changes except
                // those the new content pushes itself.
                for mut d in self.dialogs.drain(..) {
                    d.on_sleep(&mut self.core);
                }
                if was_game && !self.game_input_active() {
                    self.core.release_all();
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
                let mut s = screens::menus::MessageScreen::new(&self.core, m);
                s.layout(w, h, &mut self.core);
                self.dialogs.push(Box::new(s));
            }
        }
    }

    /// Apply an authoritative update from the host.
    pub fn apply(&mut self, u: UiUpdate) {
        let c = &mut self.core;
        match u {
            UiUpdate::ActionResult { id, result } => {
                let kind = c.pending.remove(&id);
                let mut handled = false;
                for d in self.dialogs.iter_mut().rev() {
                    if d.on_result(id, kind.as_ref(), &result, &mut self.core) {
                        handled = true;
                        break;
                    }
                }
                if !handled
                    && !self.content.on_result(id, kind.as_ref(), &result, &mut self.core)
                    && let Err(reason) = result
                {
                    self.core.message_ok("Request Rejected", &reason);
                }
            }
            UiUpdate::Connection(state) => {
                let target = match &state {
                    ConnectionState::Idle | ConnectionState::Failed { .. } => ScreenId::MainMenu,
                    ConnectionState::Connecting { .. } => self.content.id(),
                    ConnectionState::Loading { .. } => ScreenId::Loading,
                    ConnectionState::InGame { .. } => ScreenId::Play,
                };
                if let ConnectionState::InGame { server_name, max_players, .. } = &state {
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
                    c.message_ok("Connection Failed", &r);
                }
            }
            UiUpdate::Maps(m) => c.maps = m,
            UiUpdate::LanServers { servers, querying } => {
                c.servers = servers;
                c.lan_querying = querying;
            }
            UiUpdate::MainMenuBackgrounds(b) => c.menu_backgrounds = b,
            UiUpdate::Bricks(b) => c.bricks = b,
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
                    .map(|id| id.as_ref().and_then(|id| c.bricks.iter().find(|b| &b.id == id).cloned()))
                    .collect();
                c.hud.set_bricks(inv);
            }
            UiUpdate::Tools(t) => c.hud.set_tools(t),
            UiUpdate::SetActiveTool(slot) => {
                let mut o = Outbox::default();
                match slot {
                    Some(s) => c.hud.server_set_active_tool(s, &mut o),
                    None => {
                        c.hud.set_scroll_mode(crate::models::hud::ScrollMode::None, &mut o);
                    }
                }
                c.apply_outbox(o);
            }
            UiUpdate::SetActiveBrick(slot) => {
                let key = c.key_name("openBSD");
                let mut o = Outbox::default();
                if let Some(s) = slot {
                    c.hud.server_set_active_brick(s, &key, &mut o);
                }
                c.apply_outbox(o);
            }
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
                c.chat.add(&text, now);
            }
            UiUpdate::CenterPrint { text, seconds } => c.center_print(&text, seconds),
            UiUpdate::BottomPrint { text, seconds, hide_bar } => {
                let until = (seconds > 0.0).then(|| c.time_ms + (seconds * 1000.0) as u64);
                c.bottom_print = Some((text, until, hide_bar));
            }
            UiUpdate::ClearPrints => {
                c.center_print = None;
                c.bottom_print = None;
            }
            UiUpdate::PlantError(e) => c.plant_error = Some((e, c.time_ms + 800)),
            UiUpdate::Players { rows, server_name, max_players } => {
                c.players = rows;
                c.server_name = server_name;
                c.max_players = max_players;
            }
            UiUpdate::Lagging(l) => c.lagging = l,
            UiUpdate::OpenWrench { brick, variant, owner, data, admin_override, events_allowed } => {
                c.wrench.open(brick, variant, owner, data, admin_override, events_allowed);
                for id in [ScreenId::Wrench(WrenchVariant::Normal), ScreenId::Wrench(WrenchVariant::Sound), ScreenId::Wrench(WrenchVariant::VehicleSpawn)] {
                    c.pop(id);
                }
                c.push(ScreenId::Wrench(variant));
            }
            UiUpdate::OpenEvents { brick, rows, named_targets, allow_named } => {
                c.wrench.events = Some(crate::models::events::EventsModel::open(brick, rows, named_targets, allow_named, &c.events));
                c.push(ScreenId::WrenchEvents);
            }
            UiUpdate::NamedTargetsInvalidated => {
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
            UiUpdate::SaveFiles { maps, files } => {
                c.save_maps = maps;
                c.save_files = files;
            }
            UiUpdate::SaveContext { map, preview } => c.save_context = Some((map, preview)),
            UiUpdate::AvatarPreview(i) => c.avatar_preview = i,
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
        self.dialogs.iter().rposition(|d| d.modal() || d.wants_mouse())
    }

    fn with_target<R>(&mut self, idx: Option<usize>, f: impl FnOnce(&mut dyn Screen, &mut Core) -> R) -> R {
        match idx {
            Some(i) => f(self.dialogs[i].as_mut(), &mut self.core),
            None => f(self.content.as_mut(), &mut self.core),
        }
    }

    fn dispatch(&mut self, idx: Option<usize>, evs: Vec<ViewEvent>) {
        for ev in evs {
            self.with_target(idx, |s, c| s.on_event(&ev, c));
        }
        self.flush();
    }

    /// Feed one host input event.
    pub fn handle_input(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::MouseMove { x, y } => {
                self.mouse = (x, y);
                if self.cursor_visible() {
                    let (lx, ly) = self.logical_point(x, y);
                    let t = self.mouse_target();
                    let mut out = Vec::new();
                    self.with_target(t, |s, _| s.view_mut().mouse_move(lx, ly, &mut out));
                    self.dispatch(t, out);
                }
            }
            InputEvent::MouseDelta { dx, dy } => {
                if !self.cursor_visible() && self.game_input_active() {
                    let sens = self.core.prefs.f32_or("$pref::Input::MouseSensitivity", 0.75);
                    let inv = if self.core.prefs.bool_or("$pref::Input::MouseInvert", false) { -1.0 } else { 1.0 };
                    self.core.game(GameAction::Look { yaw: dx * sens * 0.005, pitch: dy * sens * 0.005 * inv });
                }
            }
            InputEvent::MouseDown { button, x, y } => {
                self.mouse = (x, y);
                if self.cursor_visible() {
                    let (lx, ly) = self.logical_point(x, y);
                    let t = self.mouse_target();
                    let pack = self.core.pack.clone();
                    let mut out = Vec::new();
                    self.with_target(t, |s, _| {
                        s.view_mut().mouse_move(lx, ly, &mut Vec::new());
                        s.view_mut().mouse_down(button, lx, ly, &pack, &mut out)
                    });
                    self.dispatch(t, out);
                } else if self.game_input_active() {
                    if let Some(cmd) = self.core.binds.command_for(&BindInput::Mouse(button)).map(str::to_string) {
                        self.core.held.insert(HeldInput::Mouse(button), cmd.clone());
                        self.core.run_command(&cmd, true);
                    }
                    self.flush();
                }
            }
            InputEvent::MouseUp { button, x, y } => {
                if let Some(cmd) = self.core.held.remove(&HeldInput::Mouse(button)) {
                    self.core.run_command(&cmd, false);
                }
                let (lx, ly) = self.logical_point(x, y);
                let t = self.mouse_target();
                let pack = self.core.pack.clone();
                let mut out = Vec::new();
                self.with_target(t, |s, _| s.view_mut().mouse_up(button, lx, ly, &pack, &mut out));
                self.dispatch(t, out);
            }
            InputEvent::Wheel { delta } => {
                if self.cursor_visible() {
                    let t = self.mouse_target();
                    let used = self.with_target(t, |s, _| s.view_mut().wheel(delta.round() as i32));
                    if used {
                        return;
                    }
                }
                // scrollInventory: ignored while any dialog other than the
                // chat HUD is open (Canvas count > 2), and on LoadingGui.
                let dialogs = self.dialogs.iter().filter(|d| !d.passive()).count();
                if self.content.id() == ScreenId::Play && dialogs == 0 {
                    match self.core.binds.command_for(&BindInput::Wheel) {
                        Some(c) if c.eq_ignore_ascii_case("scrollInventory") => self.core.wheel_scroll(delta),
                        _ => {}
                    }
                    self.flush();
                }
            }
            InputEvent::KeyDown { key, mods, repeat } => self.key_down(key, mods, repeat),
            InputEvent::KeyUp { key, mods } => {
                self.mods = mods;
                if let Some(cmd) = self.core.held.remove(&HeldInput::Key(key)) {
                    self.core.run_command(&cmd, false);
                    self.flush();
                }
            }
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
                self.flush();
            }
        }
    }

    fn key_down(&mut self, key: Key, mods: Modifiers, repeat: bool) {
        self.mods = mods;
        // 1. Global action map (console, fullscreen, help).
        if !repeat
            && let Some(cmd) = self.core.globals.command_for_key(key, mods).map(str::to_string)
        {
            self.core.run_command(&cmd, true);
            self.flush();
            return;
        }
        // 2. The top screen's own key handling (remap capture, text entry).
        let top = self.dialogs.len().checked_sub(1);
        if self.with_target(top, |s, c| s.on_key(key, mods, c)) {
            self.flush();
            return;
        }
        // 3. Focused text control.
        let mut out = Vec::new();
        let used = self.with_target(top, |s, _| s.view_mut().key(key, mods, &mut out));
        if used {
            self.dispatch(top, out);
            return;
        }
        // 4. Accelerators, topmost screen first (fire on repeat too).
        let n = self.dialogs.len();
        for i in (0..=n).rev() {
            let idx = i.checked_sub(1);
            let hit = self.with_target(idx, |s, _| s.view().accelerator(key, mods));
            if let Some(node) = hit {
                self.dispatch(idx, vec![ViewEvent { node, kind: EventKind::Click }]);
                return;
            }
            let modal = idx.is_some_and(|i| self.dialogs[i].modal() && self.dialogs[i].blocks_accelerators());
            if modal {
                break;
            }
        }
        // 5. Gameplay binds (key repeats are not delivered to bound
        //    functions; v20 repeats bricks itself).
        if repeat || !self.game_input_active() {
            return;
        }
        if self.text_focus() || self.dialogs.iter().any(|d| d.captures_keyboard()) {
            return;
        }
        let Some(cmd) = self.core.binds.command_for_key(key, mods).map(str::to_string) else { return };
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
        let c = &mut self.core;
        c.time_ms += dt_ms;
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
        c.hud.tick(dt_ms);
        self.content.view_mut().tick(dt_ms);
        self.content.tick(dt_ms, &mut self.core);
        for d in &mut self.dialogs {
            d.view_mut().tick(dt_ms);
            d.tick(dt_ms, &mut self.core);
        }
        self.flush();
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

    /// Test/host helper: logical position of a control's centre.
    pub fn control_center(&self, screen: ScreenId, name_or_command: &str) -> Option<(f32, f32)> {
        let s = self.screen(screen)?;
        let v = s.view();
        let id = v.id(name_or_command).or_else(|| v.by_command(name_or_command)).or_else(|| {
            v.walk().find(|&n| v.is_shown(n) && v.text_of(n).trim() == name_or_command.trim() && !v.text_of(n).trim().is_empty())
        })?;
        let r = v.node(id).rect;
        let sc = self.scale();
        Some(((r.x as f32 + r.w as f32 / 2.0) * sc, (r.y as f32 + r.h as f32 / 2.0) * sc))
    }

    /// Chord text for help strings.
    pub fn chord_label(c: &Chord) -> String {
        c.label()
    }
}
