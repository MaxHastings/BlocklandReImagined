//! `ConsoleDlg`, v20's `~` console, without TorqueScript.
//!
//! The authored window, GuiConsole log and entry line come from the UI pack;
//! input runs through a [`bri_console::Registry`] of typed commands and cvars
//! instead of `eval`. As in `ConsoleEntry::eval`, every submitted line is
//! echoed as `==>line` first. Commands that change shared state send the same
//! requests as the menus (admin actions, chat `/commands`, joins), so the host
//! applies its usual trust checks; the console grants nothing by itself.
//! Commands the client app owns (`stats`, `version`) are forwarded to it as
//! [`UiAction::Console`].

use super::{Screen, ScreenId, layout_view};
use crate::api::{ChatChannel, GameAction, RequestId, ScreenshotKind, UiAction};
use crate::input::{Key, Modifiers};
use crate::models::admin::{AdminAction, AdminSecret};
use crate::ui::{Core, Pending};
use crate::view::{EventKind, NodeId, View, ViewEvent};
use bri_console::registry::{split_statements, tokenize};
use bri_console::{CommandInfo, Kind, Output, Registry, Store};
use std::collections::BTreeMap;

/// `ConsoleEntry.historySize`.
pub const HISTORY: usize = 20;
/// `toggleConsole` ignores a second press within 100 ms.
const TOGGLE_DEBOUNCE_MS: u64 = 100;

/// Console state that outlives the window (it is rebuilt on every open).
#[derive(Debug, Default)]
pub struct ConsoleState {
    pub open: bool,
    last_toggle: Option<u64>,
    /// Submitted lines, oldest first.
    pub history: Vec<String>,
    /// Commands the host app runs, published with `Ui::set_console_commands`.
    pub host_commands: Vec<CommandInfo>,
    /// Requests whose answer the console reports.
    requests: BTreeMap<RequestId, String>,
    admin: BTreeMap<RequestId, AdminAction>,
    /// `changemap` waiting for the host's map list.
    pending_map: Option<String>,
}

impl ConsoleState {
    /// Forget requests that belonged to the previous host.
    pub(crate) fn reset_session(&mut self) {
        self.requests.clear();
        self.admin.clear();
        self.pending_map = None;
    }
}

impl Core {
    /// `toggleConsole`.
    pub fn toggle_console(&mut self) {
        if self
            .console
            .last_toggle
            .is_some_and(|t| self.time_ms.saturating_sub(t) < TOGGLE_DEBOUNCE_MS)
        {
            return;
        }
        self.console.last_toggle = Some(self.time_ms);
        if self.console.open {
            self.pop(ScreenId::Console);
        } else {
            self.push(ScreenId::Console);
        }
    }
}

/// Cvars are views of `$pref::` values; setting one persists it like the
/// Options menus do.
impl Store for Core {
    fn get(&self, key: &str) -> Option<String> {
        self.prefs.get(key).map(str::to_string)
    }
    fn set(&mut self, key: &str, value: &str) {
        let key = self.prefs.canonical(key).unwrap_or(key).to_string();
        self.prefs.set(&key, value);
        self.hud.prefs = self.hud_prefs();
        self.save_settings();
    }
    fn keys(&self) -> Vec<String> {
        self.prefs.keys()
    }
}

/// The UI's commands and cvars, plus the host's forwarded commands.
pub fn registry(core: &Core) -> Registry<Core> {
    let mut r: Registry<Core> = Registry::new();
    r.command("echo", "<text>", "Print text to the console.", |_, a, out| {
        out.echo(a.join(" "));
        Ok(())
    });
    r.command("cls", "", "Clear the console.", |_, _, _| {
        bri_console::log::clear();
        Ok(())
    });
    r.command("quit", "", "Quit the game.", |core, _, _| {
        core.request(UiAction::Quit);
        Ok(())
    });
    r.command("connect", "<address> [password]", "Join a server by address (Connect to IP).", |core, a, out| {
        let address = a[0].trim().to_string();
        let password = a.get(1).cloned().unwrap_or_default();
        out.echo(format!("Connecting to {address}..."));
        core.request(UiAction::JoinServer { address, password });
        Ok(())
    })
    .min_args(1)
    // A join password must not reach the log or history.
    .secret_from(1);
    r.command("disconnect", "", "Leave the game, or stop hosting.", |core, _, _| {
        use crate::api::ConnectionState as C;
        match core.conn {
            C::InGame { .. } => core.request(UiAction::Disconnect),
            C::Connecting { .. } | C::Loading { .. } => core.request(UiAction::CancelConnect),
            C::Idle | C::Failed { .. } => return Err("Not connected.".into()),
        };
        Ok(())
    });
    r.command("say", "<text>", "Send a chat message.", |core, a, _| {
        in_game(core)?;
        let text = a.join(" ");
        if text.starts_with('/') {
            return Err("Type /commands directly, without say.".into());
        }
        core.request(UiAction::Chat { channel: ChatChannel::Say, text });
        Ok(())
    })
    .min_args(1);
    r.command("players", "", "List players on this server.", |core, _, out| {
        in_game(core)?;
        for p in &core.players {
            let role = if p.super_admin {
                " (super admin)"
            } else if p.admin {
                " (admin)"
            } else {
                ""
            };
            out.echo(format!("  {}{role}  score {}  trust {}", p.name, p.score, p.trust));
        }
        out.echo(format!("{} player(s).", core.players.len()));
        Ok(())
    });
    r.command("maps", "", "List maps.", |core, _, out| {
        let maps: Vec<(String, String)> = if core.in_game() && !core.admin.maps.is_empty() {
            core.admin.maps.iter().map(|m| (m.id.clone(), m.name.clone())).collect()
        } else {
            core.maps.iter().map(|m| (m.id.clone(), m.name.clone())).collect()
        };
        for (id, name) in &maps {
            out.echo(format!("  {name} ({id})"));
        }
        out.echo(format!("{} map(s).", maps.len()));
        Ok(())
    });
    r.command("netgraph", "", "Toggle the FPS and ping display.", |core, _, _| {
        core.game(GameAction::ToggleNetGraph);
        Ok(())
    });
    r.command("screenshot", "", "Save a screenshot.", |core, _, _| {
        core.game(GameAction::Screenshot { kind: ScreenshotKind::Normal });
        Ok(())
    });
    r.command("prefs", "[filter]", "List $pref:: values (type a name to read or set one).", |core, a, out| {
        let filter = a.first().map(|f| f.to_ascii_lowercase());
        let mut n = 0;
        for key in core.prefs.keys() {
            if filter.as_ref().is_some_and(|f| !key.to_ascii_lowercase().contains(f.as_str())) {
                continue;
            }
            out.echo(format!("  {key} = \"{}\"", core.prefs.get(&key).unwrap_or_default()));
            n += 1;
        }
        out.echo(format!("{n} pref(s)."));
        Ok(())
    });
    // Administration: the same requests as the Admin window; the host checks
    // the connection's role for every one.
    r.command("admin", "", "Show your administration status.", |core, _, out| {
        in_game(core)?;
        match &core.admin.snapshot {
            Some(s) => out.echo(format!("Role: {:?}{}", s.role, if s.local_host { " (host)" } else { "" })),
            None => out.echo("Administration status not received yet."),
        }
        admin(core, AdminAction::Refresh).map(|_| ())
    });
    r.command("adminlogin", "<password>", "Log in as an administrator.", |core, a, _| {
        in_game(core)?;
        admin(core, AdminAction::Login { password: AdminSecret(a.join(" ")) }).map(|_| ())
    })
    .min_args(1)
    .secret();
    r.command("kick", "<player>", "Kick a player (admin).", |core, a, _| {
        let target = player(core, &a.join(" "))?;
        admin(core, AdminAction::Kick { target }).map(|_| ())
    })
    .min_args(1)
    .complete(player_names);
    r.command("ban", "<player> [minutes|-1] [reason]", "Ban a player; -1 or no minutes is permanent (admin).", |core, a, _| {
        let target = player(core, &a[0])?;
        let minutes = match a.get(1).map(|m| m.parse::<i64>()) {
            None => None,
            Some(Ok(m)) if m < 0 => None,
            Some(Ok(m)) => Some(u32::try_from(m).map_err(|_| "Ban length is too long.".to_string())?),
            Some(Err(_)) => return Err("Minutes must be a whole number.".into()),
        };
        let reason = a.get(2..).map(|r| r.join(" ")).unwrap_or_default();
        admin(core, AdminAction::Ban { target, minutes, reason }).map(|_| ())
    })
    .min_args(1)
    .complete(player_names);
    r.command("clearbricks", "", "Clear every brick on the server (admin).", |core, _, _| {
        admin(core, AdminAction::ClearAllBricks).map(|_| ())
    });
    r.command("changemap", "<map>", "Change the server's map (admin).", |core, a, out| {
        let wanted = a.join(" ");
        if core.admin.maps.is_empty() {
            admin(core, AdminAction::RequestMaps)?;
            core.console.pending_map = Some(wanted);
            out.echo("Asking the host for its map list...");
            return Ok(());
        }
        change_map(core, &wanted)
    })
    .min_args(1)
    .complete(map_names);

    // Typed views of settings the Options menus already own.
    r.cvar("volume", "$pref::Audio::masterVolume", Kind::Float { min: 0.0, max: 1.0 }, "Master volume.");
    r.cvar("music", "$pref::Audio::PlayMusic", Kind::Bool, "Play music.");
    r.cvar("menusounds", "$pref::Audio::MenuSounds", Kind::Bool, "Menu button sounds.");
    r.cvar("mousesensitivity", "$pref::Input::MouseSensitivity", Kind::Float { min: 0.0, max: 10.0 }, "Mouse look speed.");
    r.cvar("invertmouse", "$pref::Input::MouseInvert", Kind::Bool, "Invert mouse look.");
    r.cvar("keyboardturnspeed", super::options::KEYBOARD_TURN_SPEED, Kind::Float { min: 0.02, max: 1.0 }, "Keyboard turn rate.");
    // The Options FOV slider's pref and range; the camera reads it each frame.
    r.cvar(
        "fov",
        super::options::DEFAULT_FOV,
        Kind::Int { min: super::options::FOV_RANGE.0 as i64, max: super::options::FOV_RANGE.1 as i64 },
        "Camera field of view in degrees (Options slider).",
    );
    r.cvar("chatsize", super::options::CHAT_SIZE, Kind::Int { min: 0, max: 10 }, "Chat text size.");
    r.cvar("chatlines", "$Pref::Chat::MaxDisplayLines", Kind::Int { min: 1, max: 64 }, "Chat lines shown.");
    r.cvar("shadows", "$pref::ShadowQuality", Kind::Int { min: 0, max: 4 }, "Shadow quality: 0 best .. 4 off.");
    r.cvar("antialiasing", "$pref::Video::AntiAliasing", Kind::Bool, "Multisample anti-aliasing.");
    r.cvar("anisotropy", "$pref::OpenGL::anisotropy", Kind::Float { min: 0.0, max: 1.0 }, "Anisotropic filtering, 0..1.");
    r.cvar("precipitation", "$pref::precipitationOn", Kind::Bool, "Rain and snow.");
    r.cvar("tooltips", "$pref::HUD::showToolTips", Kind::Bool, "HUD tool tips.");

    for info in &core.console.host_commands {
        r.forward(info);
    }
    r
}

fn in_game(core: &Core) -> Result<(), String> {
    if core.in_game() { Ok(()) } else { Err("Not in a game.".into()) }
}

/// Send an admin request and report its answer in the console.
fn admin(core: &mut Core, action: AdminAction) -> Result<RequestId, String> {
    in_game(core)?;
    if core.admin.busy() {
        return Err("Waiting for the host to answer an earlier admin request.".into());
    }
    match core.admin_request(action.clone()) {
        Some(id) => {
            core.console.admin.insert(id, action);
            Ok(id)
        }
        None => Err(if core.admin.snapshot.is_none() {
            "Administration status not received yet; try again.".into()
        } else if core.admin.is_admin() {
            core.admin.status.clone()
        } else {
            "You are not an administrator (use adminlogin <password>).".into()
        }),
    }
}

fn player_names(core: &Core) -> Vec<String> {
    match &core.admin.snapshot {
        Some(s) => s.players.iter().map(|p| p.name.clone()).collect(),
        None => core.players.iter().map(|p| p.name.clone()).collect(),
    }
}

fn map_names(core: &Core) -> Vec<String> {
    core.admin.maps.iter().map(|m| m.name.clone()).collect()
}

/// A player by exact name, else by a unique name prefix.
fn player(core: &Core, name: &str) -> Result<u64, String> {
    in_game(core)?;
    let Some(s) = &core.admin.snapshot else {
        return Err("Administration status not received yet; try again.".into());
    };
    let low = name.to_ascii_lowercase();
    if let Some(p) = s.players.iter().find(|p| p.name.eq_ignore_ascii_case(name)) {
        return Ok(p.connection);
    }
    let found: Vec<_> = s
        .players
        .iter()
        .filter(|p| p.name.to_ascii_lowercase().starts_with(&low))
        .collect();
    match found.as_slice() {
        [p] => Ok(p.connection),
        [] => Err(format!("No player named {name}.")),
        _ => Err(format!("{name} matches more than one player.")),
    }
}

fn change_map(core: &mut Core, wanted: &str) -> Result<(), String> {
    let map = core
        .admin
        .maps
        .iter()
        .find(|m| m.id.eq_ignore_ascii_case(wanted) || m.name.eq_ignore_ascii_case(wanted))
        .map(|m| m.id.clone())
        .ok_or_else(|| format!("The host has no map named {wanted} (type maps)."))?;
    admin(core, AdminAction::ChangeMap { map }).map(|_| ())
}

pub struct Console {
    view: View,
    scroll: Option<NodeId>,
    log: Option<NodeId>,
    entry: Option<NodeId>,
    revision: Option<u64>,
    registry: Registry<Core>,
    /// Index into the history while browsing it with Up/Down.
    browsing: Option<usize>,
    /// What was typed before browsing started.
    draft: String,
}

impl Console {
    pub fn new(core: &mut Core) -> Self {
        let mut layout = core.pack.data.layouts.get("ConsoleDlg").cloned().unwrap_or_default();
        // GuiConsoleEditCtrl is a GuiTextEditCtrl with history (handled here).
        fn edit_class(c: &mut crate::schema::Control) {
            if c.class == "GuiConsoleEditCtrl" {
                c.class = "GuiTextEditCtrl".into();
            }
            c.children.iter_mut().for_each(edit_class);
        }
        edit_class(&mut layout);
        let mut view = if layout.class.is_empty() {
            layout_view(core, "ConsoleDlg")
        } else {
            let mut v = View::new(&layout);
            v.measure(&core.pack);
            v
        };
        let log = view.id("testArrayCtrl");
        let entry = view.id("ConsoleEntry");
        let scroll = log.and_then(|l| view.node(l).parent);
        view.focus = entry;
        Console {
            view,
            scroll,
            log,
            entry,
            revision: None,
            registry: registry(core),
            browsing: None,
            draft: String::new(),
        }
    }

    fn refresh_log(&mut self) {
        let revision = bri_console::log::revision();
        if self.revision == Some(revision) {
            return;
        }
        self.revision = Some(revision);
        let Some(log) = self.log else { return };
        self.view.state(log).items = bri_console::log::lines()
            .into_iter()
            .map(|l| {
                let level = match l.level {
                    bri_console::Level::Normal => 0,
                    bri_console::Level::Warning => 1,
                    bri_console::Level::Error => 2,
                };
                (l.text, level)
            })
            .collect();
        self.view.relayout();
        // GuiConsole keeps the newest line in view.
        if let Some(s) = self.scroll {
            self.view.scroll_to(s, i32::MAX);
        }
    }

    fn entry_text(&self) -> String {
        self.entry.map(|e| self.view.edit_text(e)).unwrap_or_default()
    }

    fn set_entry(&mut self, text: &str) {
        if let Some(e) = self.entry {
            self.view.set_text(e, text);
            self.view.state(e).cursor = text.chars().count();
        }
    }

    fn submit(&mut self, core: &mut Core) {
        let text = self.entry_text();
        self.set_entry("");
        self.browsing = None;
        if text.trim().is_empty() {
            return;
        }
        let (shown, secret) = self.registry.redact(&text);
        bri_console::echo(format!("==>{shown}"));
        if !secret && core.console.history.last() != Some(&text) {
            core.console.history.push(text.clone());
            let extra = core.console.history.len().saturating_sub(HISTORY);
            core.console.history.drain(..extra);
        }
        for statement in split_statements(&text) {
            // `/command args` is a chat command, as in the chat box.
            if let Some(rest) = statement.strip_prefix('/') {
                let mut words = tokenize(rest).into_iter();
                let Some(name) = words.next() else { continue };
                if !core.in_game() {
                    bri_console::error("Not in a game.");
                    continue;
                }
                let id = core.request(UiAction::ChatCommand { name: name.clone(), args: words.collect() });
                core.console.requests.insert(id, format!("/{name}"));
                continue;
            }
            let mut out = Output::default();
            let forwarded = self.registry.exec(core, &statement, &mut out);
            out.flush();
            for line in forwarded {
                let id = core.request(UiAction::Console { line: line.clone() });
                core.console.requests.insert(id, line);
            }
        }
    }

    fn history_step(&mut self, back: bool, core: &Core) {
        let h = &core.console.history;
        if h.is_empty() {
            return;
        }
        let next = match (self.browsing, back) {
            (None, true) => {
                self.draft = self.entry_text();
                Some(h.len() - 1)
            }
            (None, false) => return,
            (Some(i), true) => Some(i.saturating_sub(1)),
            (Some(i), false) if i + 1 < h.len() => Some(i + 1),
            (Some(_), false) => None,
        };
        self.browsing = next;
        let text = match next {
            Some(i) => h[i].clone(),
            None => std::mem::take(&mut self.draft),
        };
        self.set_entry(&text);
    }

    fn complete(&mut self, core: &Core) {
        let c = self.registry.complete(core, &self.entry_text());
        if c.candidates.len() > 1 {
            bri_console::echo(c.candidates.join("  "));
        }
        self.set_entry(&c.line);
    }

    /// Report answers to the console's admin requests.
    fn admin_answers(&mut self, core: &mut Core) {
        let done: Vec<RequestId> = core
            .console
            .admin
            .keys()
            .copied()
            .filter(|id| !core.admin.pending.contains_key(id))
            .collect();
        for id in done {
            let Some(action) = core.console.admin.remove(&id) else { continue };
            let status = core.admin.status.clone();
            if status.starts_with("Rejected") {
                bri_console::error(status);
                continue;
            }
            match action {
                AdminAction::Login { .. } if core.admin.is_admin() => {
                    bri_console::echo("You are now an administrator.");
                }
                AdminAction::Refresh | AdminAction::RequestMaps | AdminAction::Login { .. } => {}
                _ => bri_console::echo(if status.is_empty() { "Done.".to_string() } else { status }),
            }
        }
        if let Some(map) = core.console.pending_map.clone()
            && !core.admin.busy()
        {
            core.console.pending_map = None;
            if let Err(e) = change_map(core, &map) {
                bri_console::error(e);
            }
        }
    }
}

impl Screen for Console {
    fn id(&self) -> ScreenId {
        ScreenId::Console
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    /// `sinkAllKeyEvents`: no gameplay bind fires while typing here.
    fn captures_keyboard(&self) -> bool {
        true
    }
    fn on_wake(&mut self, core: &mut Core) {
        core.console.open = true;
        self.refresh_log();
        // Fetch the admin state once so kick/ban can resolve names.
        if core.in_game() && core.admin.snapshot.is_none() && !core.admin.busy() {
            core.admin_request(AdminAction::Refresh);
        }
    }
    fn on_sleep(&mut self, core: &mut Core) {
        core.console.open = false;
    }
    fn layout(&mut self, w: i32, h: i32, _core: &mut Core) {
        self.view.layout(w, h);
        if let Some(s) = self.scroll {
            self.view.scroll_to(s, i32::MAX);
        }
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        match key {
            Key::Escape => core.pop(ScreenId::Console),
            Key::Up => self.history_step(true, core),
            Key::Down => self.history_step(false, core),
            Key::Tab => self.complete(core),
            // `useSiblingScroller`: page the log from the entry line.
            Key::PageUp | Key::PageDown => {
                if let Some(s) = self.scroll {
                    let page = (self.view.node(s).rect.h - 16).max(16);
                    self.view.scroll_by(s, if key == Key::PageUp { -page } else { page });
                }
            }
            _ => return false,
        }
        true
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        match ev.kind {
            EventKind::Close => core.pop(ScreenId::Console),
            EventKind::Submit if Some(ev.node) == self.entry => self.submit(core),
            _ => {}
        }
    }
    fn on_result(
        &mut self,
        id: RequestId,
        _kind: Option<&Pending>,
        result: &Result<(), String>,
        core: &mut Core,
    ) -> bool {
        let Some(what) = core.console.requests.remove(&id) else {
            return false;
        };
        if let Err(e) = result {
            bri_console::error(format!("{what}: {e}"));
        }
        true
    }
    fn tick(&mut self, _dt_ms: u64, core: &mut Core) {
        self.admin_answers(core);
        self.refresh_log();
    }
}
