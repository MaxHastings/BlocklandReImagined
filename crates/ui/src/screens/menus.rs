//! Initial native menu/connection screens. Remaining gameplay dialogs are
//! explicit development notices until their models are wired to controls.
use super::*;
use crate::api::*;
use crate::binds::{BindMap, DEFAULT_KEYBOARD, DEFAULT_MOUSE};
use crate::models::chat::{ChatSend, chat_send};
use crate::ui::{Callback, MessageBox};
use crate::view::EventKind;

const SERVER_TYPE: &str = "$Pref::Net::ServerType";
const MAX_PLAYERS: &str = "$Pref::Server::MaxPlayers";

pub struct NativeScreen {
    id: ScreenId,
    view: View,
    map_ids: Vec<String>,
    server_addresses: Vec<String>,
    request: Option<RequestId>,
}

impl NativeScreen {
    pub fn new(id: ScreenId, core: &Core) -> Self {
        let name = match id {
            ScreenId::MainMenu => "MainMenuGui",
            ScreenId::DefaultControls => "defaultControlsGui",
            ScreenId::StartMission => "startMissionGui",
            ScreenId::JoinServer => "JoinServerGui",
            ScreenId::ManualJoin => "ManualJoin",
            ScreenId::Connecting => "connectingGui",
            ScreenId::Loading => "LoadingGui",
            ScreenId::EscapeMenu => "escapeMenu",
            ScreenId::Play => "PlayGui",
            ScreenId::MessageInput(_) => "newMessageHud",
            ScreenId::About => "aboutDlg",
            _ => "MessageBoxOKDlg",
        };
        let mut s = Self {
            id,
            view: layout_view(core, name),
            map_ids: vec![],
            server_addresses: vec![],
            request: None,
        };
        // Preferences are data; script strings are never evaluated.
        for n in s.view.walk().collect::<Vec<_>>() {
            if let Some(var) = s.view.node(n).ctrl.variable.clone()
                && let Some(value) = core.prefs.get(&var)
            {
                s.view.set_text(n, value);
            }
        }
        match id {
            ScreenId::MainMenu => {
                for name in ["MM_AuthBar", "DemoBanner", "buyNowButton_W", "buyNowButton_B", "mm_Fade"] { s.visible(name, false); }
                s.set("MM_Version", "ReImagined — development");
                if core.pack.has_image("screenshots/icepalace") { s.icon("MM_BG", &IconRef::Pack("screenshots/icepalace".into())); }
            }
            ScreenId::DefaultControls => {
                let mouse = if core.settings.binds.is_none() { DEFAULT_MOUSE } else { core.settings.mouse_type };
                s.radio(&format!("OPT_Mouse{}", mouse.min(3)));
                s.radio(&format!("OPT_Keyboard{}", core.settings.keyboard_type.min(1)));
                s.visible("DefaultControls_CancelBlocker", false);
            }
            ScreenId::StartMission => {
                for name in ["SM_demoBanner1", "SM_demoBanner2"] { s.visible(name, false); }
                // Public legacy services are excluded. LAN and direct IP remain.
                s.active("SM_OptInternet", false);
                if let Some(n) = s.view.id("SM_PlayerCountMenu") {
                    s.view.state(n).items = (1..=32).map(|i| (i.to_string(), i)).collect();
                }
                let lan = core.prefs.str_or(SERVER_TYPE, "SinglePlayer").eq_ignore_ascii_case("LAN");
                s.radio(if lan { "SM_OptLAN" } else { "SM_OptSinglePlayer" });
                s.server_type(core, lan);
            }
            ScreenId::JoinServer => {
                s.visible("JSG_demoBanner", false); s.visible("JSG_demoBanner2", false);
                s.visible("JS_QueryInternetBlocker", false);
                if let Some(n) = s.view.by_command("JoinServerGui.queryWebMaster();") { s.view.set_active(n, false); }
            }
            ScreenId::About => s.set("aboutText", "Blockland ReImagined\nOriginal Blockland by Eric Hartman and contributors.\nNative engine rewrite — development build."),
            ScreenId::MessageInput(ch) => {
                s.set("NMH_Channel", if ch == ChatChannel::Say { "SAY:" } else { "TEAM:" });
                let size = super::options::chat_size(&core.prefs);
                for (name, style) in [("NMH_Type", "HUDChatTextEditSize"), ("NMH_Channel", "BlockChatChannelSize")] {
                    if let Some(n) = s.view.id(name) {
                        s.view.nodes[n].ctrl.style = format!("{style}{size}Profile");
                    }
                }
                s.view.focus = s.view.id("NMH_Type");
            }
            ScreenId::ManualJoin => s.view.focus = s.view.id("MJ_txtIP"),
            ScreenId::Connecting | ScreenId::Loading | ScreenId::EscapeMenu | ScreenId::Play => {}
            _ => {
                s.set("MBOKFrame", "Interface under construction");
                s.set("MBOKText", &format!("{id:?} is not connected yet. It remains required before the alpha handoff."));
            }
        }
        s.refresh(core);
        s
    }
    fn set(&mut self, name: &str, text: &str) {
        if let Some(n) = self.view.id(name) {
            self.view.set_text(n, text);
        }
    }
    fn visible(&mut self, name: &str, value: bool) {
        if let Some(n) = self.view.id(name) {
            self.view.set_visible(n, value);
        }
    }
    fn active(&mut self, name: &str, value: bool) {
        if let Some(n) = self.view.id(name) {
            self.view.set_active(n, value);
        }
    }
    fn radio(&mut self, name: &str) {
        if let Some(n) = self.view.id(name) {
            self.view.select_radio(n);
        }
    }
    fn edit(&self, name: &str) -> String {
        self.view
            .id(name)
            .map(|n| self.view.edit_text(n))
            .unwrap_or_default()
    }
    fn checked(&self, name: &str) -> bool {
        self.view.id(name).is_some_and(|n| self.view.bool_value(n))
    }
    fn selected(&self, name: &str) -> Option<usize> {
        self.view
            .id(name)
            .and_then(|n| self.view.selected(n))
            .and_then(|i| i.try_into().ok())
    }
    fn icon(&mut self, name: &str, icon: &IconRef) {
        if let Some(n) = self.view.id(name) {
            self.view.state(n).external_texture = match icon {
                IconRef::External(id) => Some(*id),
                _ => None,
            };
            self.view.state(n).bitmap = match icon {
                IconRef::Pack(p) => Some(p.clone()),
                _ => None,
            };
        }
    }
    /// startMissionGui::ClickSinglePlayer/ClickLAN: single player greys the
    /// server options out and plays alone.
    fn server_type(&mut self, core: &Core, lan: bool) {
        self.visible("SM_OptionsBlocker", !lan);
        if let Some(n) = self.view.id("SM_PlayerCountMenu") {
            let players = if lan {
                core.prefs.i64_or(MAX_PLAYERS, 8).clamp(1, 32)
            } else {
                1
            };
            self.view.select(n, Some(players));
        }
    }
    fn refresh(&mut self, core: &Core) {
        match self.id {
            ScreenId::MainMenu => {
                if let Some(i) = core.menu_backgrounds.first() {
                    self.icon("MM_BG", i);
                }
            }
            ScreenId::StartMission => {
                let selected = self
                    .selected("SM_missionList")
                    .and_then(|i| self.map_ids.get(i))
                    .cloned();
                let mut maps: Vec<_> = core.maps.iter().collect();
                maps.sort_by_key(|m| m.name.to_ascii_lowercase());
                self.map_ids = maps.iter().map(|m| m.id.clone()).collect();
                if let Some(n) = self.view.id("SM_missionList") {
                    self.view.state(n).items = maps
                        .iter()
                        .enumerate()
                        .map(|(i, m)| (m.name.clone(), i as i64))
                        .collect();
                    let i = selected
                        .and_then(|id| self.map_ids.iter().position(|x| *x == id))
                        .or_else(|| (!maps.is_empty()).then_some(0));
                    self.view.select(n, i.map(|i| i as i64));
                }
                self.map_preview(core);
                if let Some(n) = self.view.by_command("SM_StartMission();") {
                    self.view
                        .set_active(n, !maps.is_empty() && self.request.is_none());
                }
            }
            ScreenId::JoinServer => {
                let selected = self
                    .selected("JS_serverList")
                    .and_then(|i| self.server_addresses.get(i))
                    .cloned();
                self.server_addresses = core.servers.iter().map(|s| s.address.clone()).collect();
                if let Some(n) = self.view.id("JS_serverList") {
                    self.view.state(n).items = core
                        .servers
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            (
                                format!(
                                    "{}\t{}\t{}\t{}\t{}\t/\t{}\t{}\t{}",
                                    if s.password { "Yes" } else { "" },
                                    if s.dedicated { "Yes" } else { "" },
                                    s.name,
                                    s.ping_ms.map(|p| p.to_string()).unwrap_or_default(),
                                    s.players,
                                    s.max_players,
                                    s.bricks,
                                    s.map
                                ),
                                i as i64,
                            )
                        })
                        .collect();
                    self.view.select(
                        n,
                        selected
                            .and_then(|s| self.server_addresses.iter().position(|x| *x == s))
                            .map(|i| i as i64),
                    );
                }
                self.visible("JS_queryStatus", core.lan_querying);
                self.set(
                    "JS_statusText",
                    if core.lan_querying {
                        "Querying LAN..."
                    } else {
                        ""
                    },
                );
            }
            ScreenId::Connecting => {
                if let ConnectionState::Connecting { text } = &core.conn {
                    self.set("Connecting_Text", text);
                }
            }
            ScreenId::Loading => {
                if let ConnectionState::Loading {
                    map,
                    preview,
                    phase,
                    progress,
                } = &core.conn
                {
                    self.set("LOAD_MapName", map);
                    self.icon("LOAD_MapPicture", preview);
                    self.set(
                        "LOAD_MapDescription",
                        core.maps
                            .iter()
                            .find(|m| m.id == *map || m.name == *map)
                            .map_or("", |m| m.description.as_str()),
                    );
                    self.set(
                        "LoadingProgressTxt",
                        match phase {
                            LoadPhase::WaitingForServer => "WAITING FOR SERVER",
                            LoadPhase::LoadingObjects => "LOADING OBJECTS",
                            LoadPhase::LightingMission => "LIGHTING MISSION",
                            LoadPhase::Ghosting => "RECEIVING WORLD",
                        },
                    );
                    if let Some(n) = self.view.id("LoadingProgress") {
                        self.view.set_num(
                            n,
                            if progress.is_finite() {
                                progress.clamp(0.0, 1.0)
                            } else {
                                0.0
                            },
                        );
                    }
                }
            }
            _ => {}
        }
    }
    fn map_preview(&mut self, core: &Core) {
        let map = self
            .selected("SM_missionList")
            .and_then(|i| self.map_ids.get(i))
            .and_then(|id| core.maps.iter().find(|m| m.id == *id));
        self.set("SM_MapName", map.map_or("", |m| m.name.as_str()));
        self.set(
            "SM_MapDescription",
            map.map_or("", |m| m.description.as_str()),
        );
        self.icon(
            "SM_MapPreview",
            &map.map_or(IconRef::None, |m| m.preview.clone()),
        );
    }
    fn cancel(&mut self, core: &mut Core) {
        match self.id {
            ScreenId::Loading | ScreenId::Connecting => {
                core.request(UiAction::CancelConnect);
            }
            ScreenId::MainMenu | ScreenId::Play => return,
            _ if self.request.is_some() => {
                core.request(UiAction::CancelConnect);
            }
            _ => {}
        }
        core.pop(self.id);
    }
    fn submit_chat(&mut self, core: &mut Core, ch: ChatChannel) {
        match chat_send(ch, &self.edit("NMH_Type")) {
            ChatSend::Close => core.pop(self.id),
            ChatSend::Send(a) => {
                core.request(a);
                core.pop(self.id);
            }
            ChatSend::Blocked { title, text } => core.message_ok(&title, &text),
        }
    }
    fn host(&mut self, core: &mut Core) {
        if self.request.is_some() {
            return;
        }
        let Some(id) = self
            .selected("SM_missionList")
            .and_then(|i| self.map_ids.get(i))
            .cloned()
        else {
            return;
        };
        if !core.maps.iter().any(|m| m.id == id) {
            return;
        }
        let pref = |key: &str| {
            self.view
                .walk()
                .find(|&n| {
                    self.view
                        .node(n)
                        .ctrl
                        .variable
                        .as_deref()
                        .is_some_and(|v| v.eq_ignore_ascii_case(key))
                })
                .map(|n| self.view.edit_text(n))
                .unwrap_or_default()
        };
        let lan = self.checked("SM_OptLAN");
        let max_players = self
            .selected("SM_PlayerCountMenu")
            .unwrap_or(1)
            .clamp(1, 32) as u32;
        core.prefs
            .set(SERVER_TYPE, if lan { "LAN" } else { "SinglePlayer" });
        if lan {
            core.prefs.set(MAX_PLAYERS, max_players.to_string());
        }
        core.save_settings();
        let action = UiAction::HostGame {
            map: id,
            mode: if lan {
                ServerMode::Lan
            } else {
                ServerMode::SinglePlayer
            },
            max_players,
            server_name: self.edit("TxtServerName"),
            password: self.edit("TxtServerPassword"),
            admin_password: pref("$Pref::Server::AdminPassword"),
            super_admin_password: pref("$Pref::Server::SuperAdminPassword"),
        };
        self.request = Some(core.request_pending(action, Pending::Other));
        self.refresh(core);
    }
    fn join(&mut self, core: &mut Core) {
        if self.request.is_some() {
            return;
        }
        let (address, password) = if self.id == ScreenId::ManualJoin {
            (
                self.edit("MJ_txtIP").trim().to_string(),
                self.edit("MJ_txtJoinPass"),
            )
        } else {
            let Some(address) = self
                .selected("JS_serverList")
                .and_then(|i| self.server_addresses.get(i))
                .cloned()
            else {
                return;
            };
            // Passworded entries use the same direct-IP dialog, already filled.
            if core
                .servers
                .iter()
                .any(|s| s.address == address && s.password)
            {
                core.prefs.set("$pref::Join::Address", &address);
                core.push(ScreenId::ManualJoin);
                return;
            }
            (address, String::new())
        };
        if address.is_empty() {
            core.message_ok("Connect to IP", "Enter a server address.");
            return;
        }
        self.request =
            Some(core.request_pending(UiAction::JoinServer { address, password }, Pending::Other));
    }
}

impl Screen for NativeScreen {
    fn id(&self) -> ScreenId {
        self.id
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn cursor(&self) -> bool {
        !matches!(self.id, ScreenId::Play | ScreenId::MessageInput(_))
    }
    fn modal(&self) -> bool {
        self.id != ScreenId::Play
    }
    fn blocks_accelerators(&self) -> bool {
        true
    }
    fn captures_keyboard(&self) -> bool {
        self.id != ScreenId::Play
    }
    fn on_wake(&mut self, core: &mut Core) {
        if matches!(self.id, ScreenId::MessageInput(_)) {
            core.request(UiAction::StartTyping);
        }
        if self.id == ScreenId::ManualJoin {
            self.set("MJ_txtIP", core.prefs.str_or("$pref::Join::Address", ""));
        }
    }
    fn on_sleep(&mut self, core: &mut Core) {
        if matches!(self.id, ScreenId::MessageInput(_)) {
            core.request(UiAction::StopTyping);
        }
        if let Some(id) = self.request.take() {
            core.pending.remove(&id);
        }
    }
    fn on_update(&mut self, core: &mut Core) {
        self.refresh(core);
    }
    fn on_result(
        &mut self,
        id: RequestId,
        _kind: Option<&Pending>,
        result: &Result<(), String>,
        core: &mut Core,
    ) -> bool {
        if self.request != Some(id) {
            return false;
        }
        self.request = None;
        if let Err(reason) = result {
            core.message_ok("Request Rejected", reason);
        }
        self.refresh(core);
        true
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            self.cancel(core);
            return self.id != ScreenId::Play;
        }
        if matches!(key, Key::Return | Key::NumpadEnter) {
            if let ScreenId::MessageInput(ch) = self.id {
                self.submit_chat(core, ch);
                return true;
            }
            if self.id == ScreenId::ManualJoin {
                self.join(core);
                return true;
            }
        }
        false
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if ev.kind == EventKind::Close {
            self.cancel(core);
            return;
        }
        if ev.kind == EventKind::Changed {
            if self.view.node(ev.node).ctrl.name.as_deref() == Some("SM_missionList") {
                self.map_preview(core);
            }
            return;
        }
        if !matches!(
            ev.kind,
            EventKind::Click | EventKind::Submit | EventKind::DoubleClick
        ) {
            return;
        }
        let command = command_of(&self.view, ev.node).to_ascii_lowercase();
        // Exact allowlist only: never evaluate script or infer arbitrary actions.
        match command.as_str() {
            "quit();" => {
                core.request(UiAction::Quit);
            }
            "quitgame();" => {
                core.message_yes_no("Quit", "Quit Blockland ReImagined?", Callback::Quit)
            }
            "canvas.pushdialog(startmissiongui);" => core.push(ScreenId::StartMission),
            "canvas.pushdialog(joinservergui);" => core.push(ScreenId::JoinServer),
            "canvas.pushdialog(optionsdlg);" => core.push(ScreenId::Options),
            "canvas.pushdialog(avatargui);" => core.push(ScreenId::Avatar),
            "canvas.pushdialog(aboutdlg);" => core.push(ScreenId::About),
            "canvas.pushdialog(\"manualjoin\");" => core.push(ScreenId::ManualJoin),
            "mm_tutorial();" => {
                core.request(UiAction::StartTutorial);
            }
            "sm_startmission();" => self.host(core),
            "sm_missionlist.select();" => self.map_preview(core),
            "startmissiongui.clicklan();" => self.server_type(core, true),
            "startmissiongui.clicksingleplayer();" => self.server_type(core, false),
            "defaultcontrolsgui.apply();" => {
                let mouse = (0..=3)
                    .find(|i| self.checked(&format!("OPT_Mouse{i}")))
                    .unwrap_or(DEFAULT_MOUSE);
                let keyboard = (0..=1)
                    .find(|i| self.checked(&format!("OPT_Keyboard{i}")))
                    .unwrap_or(DEFAULT_KEYBOARD);
                core.settings.mouse_type = mouse;
                core.settings.keyboard_type = keyboard;
                core.binds =
                    BindMap::defaults(&core.pack.data.data, mouse, keyboard, core.platform);
                if !core.options_open {
                    core.save_settings();
                }
                core.pop(self.id);
            }
            "joinservergui.querylan();" => {
                core.request(UiAction::QueryLan);
            }
            "mj_connect();" | "joinservergui.join();" => self.join(core),
            "connectinggui::cancel();" => self.cancel(core),
            "disconnect();" if self.id == ScreenId::Loading => self.cancel(core),
            "escapefromgame();" => {
                core.message_yes_no("Disconnect", "Leave this game?", Callback::Disconnect)
            }
            "escapemenu::clicksavebricks();" => core.push(ScreenId::SaveBricks),
            "escapemenu::clickloadbricks();" => core.push(ScreenId::LoadBricks),
            "escapemenu::clickadmin();" => {
                core.request(UiAction::OpenAdmin);
            }
            "canvas.pushdialog(newplayerlistgui);canvas.popdialog(escapemenu);" => {
                core.pop(self.id);
                core.push(ScreenId::PlayerList);
            }
            "escapemenu::clickminigames();" => {
                core.pop(self.id);
                core.push(if core.minigames.owns_active_game {
                    ScreenId::MiniGameSettings
                } else {
                    ScreenId::MiniGames
                });
            }
            "canvas.popdialog(startmissiongui);"
            | "canvas.popdialog(defaultcontrolsgui);"
            | "canvas.popdialog(joinservergui);"
            | "canvas.popdialog(manualjoin);"
            | "canvas.popdialog(aboutdlg);"
            | "canvas.popdialog(newmessagehud);" => self.cancel(core),
            "messagecallback(messageboxokdlg,messageboxokdlg.callback);" => self.cancel(core),
            "" => {}
            _ => core.message_ok(
                "Interface under construction",
                "This action still needs its native implementation before the alpha handoff.",
            ),
        }
    }
}

pub struct MessageScreen {
    view: View,
    message: MessageBox,
}
impl MessageScreen {
    pub fn new(core: &Core, message: MessageBox) -> Self {
        let mut view = layout_view(
            core,
            if message.yes_no {
                "MessageBoxYesNoDlg"
            } else {
                "MessageBoxOKDlg"
            },
        );
        let prefix = if message.yes_no { "MBYesNo" } else { "MBOK" };
        if let Some(n) = view.id(&format!("{prefix}Frame")) {
            view.set_text(n, &message.title);
        }
        if let Some(n) = view.id(&format!("{prefix}Text")) {
            view.set_text(n, &message.text);
        }
        Self { view, message }
    }
    fn answer(&self, yes: bool, core: &mut Core) {
        core.pop(ScreenId::MessageBox);
        if !yes {
            return;
        }
        match &self.message.on_yes {
            Callback::None => {}
            Callback::Quit => {
                core.request(UiAction::Quit);
            }
            Callback::Disconnect => {
                core.request(UiAction::Disconnect);
            }
            Callback::RemapForce { command, input } => {
                core.binds.force_remap(command, *input);
                core.save_settings();
            }
            // optionsDlg.clearAllBinds: only the remappable controls; Options
            // saves them when it closes.
            Callback::ClearBinds => {
                for c in core.remap_commands.clone() {
                    core.binds.unbind_command(&c);
                }
            }
            Callback::DefaultBinds => core.push(ScreenId::DefaultControls),
            Callback::OverwriteSave {
                name,
                description,
                events,
                ownership,
            } => {
                core.request_pending(
                    UiAction::SaveBricks {
                        name: name.clone(),
                        description: description.clone(),
                        events: *events,
                        ownership: *ownership,
                        overwrite: true,
                    },
                    Pending::Save,
                );
            }
            Callback::CloseEvents => core.pop(ScreenId::WrenchEvents),
            Callback::MiniGame { game, operation } => {
                let valid = match operation {
                    MiniGameOperation::AcceptInvite | MiniGameOperation::RejectInvite | MiniGameOperation::IgnoreInvite => core.minigames.invitations.iter().any(|i| i.game == *game),
                    _ => core.minigames.active_game == Some(*game),
                };
                if !valid { return; }
                let action = match operation {
                    MiniGameOperation::Reset => UiAction::ResetMiniGame { game: *game },
                    MiniGameOperation::End => UiAction::EndMiniGame { game: *game },
                    MiniGameOperation::Leave => UiAction::LeaveMiniGame { game: *game },
                    MiniGameOperation::RejectInvite => UiAction::RejectMiniGameInvite { game: *game, ignore_owner: false },
                    MiniGameOperation::IgnoreInvite => UiAction::RejectMiniGameInvite { game: *game, ignore_owner: true },
                    _ => return,
                };
                core.minigame_request(*operation, action);
            }
        }
    }
}
impl Screen for MessageScreen {
    fn id(&self) -> ScreenId {
        ScreenId::MessageBox
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn blocks_accelerators(&self) -> bool {
        true
    }
    fn captures_keyboard(&self) -> bool {
        true
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        match key {
            Key::Escape => self.answer(false, core),
            Key::Return | Key::NumpadEnter => self.answer(true, core),
            _ => return false,
        }
        true
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if ev.kind == EventKind::Click {
            let c = command_of(&self.view, ev.node);
            self.answer(!c.contains("noCallback"), core);
        }
    }
}
