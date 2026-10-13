//! Stock v20 mini-game list and invitation dialog (the rules are the
//! Mini-Game window's, [`super::minigame_window`]).
//! Views use the original pack layouts; controls are typed and host-gated.
use super::*;
use crate::{api::*, geom::Rect, ui::Callback, view::EventKind};

#[derive(Clone, Copy)]
enum Kind {
    List,
    Invite,
}
/// Slayer's Default column header (`JMG_Slayer_Default`), shown while a
/// listed game is the server's default one.
const DEFAULT_HEADER: &str = "JMG_DefaultHeader";
/// `JMG_List.columns` with the Default column (Slayer's).
const DEFAULT_COLUMNS: &str = "0 102 165 490 450";
/// Beside the status: the picked game's settings in the Mini-Game window,
/// to change when the player may, else to read.
const SETTINGS_BUTTON: &str = "NativeMiniGameSettings";

pub struct MiniGameScreen {
    id: ScreenId,
    kind: Kind,
    view: View,
    selected_game: Option<MiniGameId>,
    game_ids: Vec<MiniGameId>,
    request: Option<RequestId>,
    list_columns: Option<String>,
}
impl MiniGameScreen {
    pub fn list(core: &Core) -> Self {
        Self::new(core, Kind::List)
    }
    pub fn invitation(core: &Core) -> Self {
        Self::new(core, Kind::Invite)
    }
    fn new(core: &Core, kind: Kind) -> Self {
        let (id, layout) = match kind {
            Kind::List => (ScreenId::MiniGames, "joinMiniGameGui"),
            Kind::Invite => (ScreenId::MiniGameInvitation, "MiniGameInviteGui"),
        };
        let mut s = Self {
            id,
            kind,
            view: layout_view(core, layout),
            selected_game: None,
            game_ids: vec![],
            request: None,
            list_columns: None,
        };
        if let Some(n) = s.view.id("JMG_List") {
            s.list_columns = s.view.node(n).ctrl.field("columns").map(str::to_owned);
        }
        if matches!(kind, Kind::List) {
            let parent = window(&s.view).unwrap_or(s.view.root);
            let mut b = button(
                "BlockButtonProfile",
                Rect::new(445, 33, 57, 19),
                "base/client/ui/button1",
                "Default",
                "JoinMiniGameGui.sortList(3);",
            );
            b.name = Some(DEFAULT_HEADER.into());
            b.visible = false;
            s.view.add(parent, b);
        }
        s.refresh(core);
        s
    }
    fn selected_game(&self) -> Option<MiniGameId> {
        self.view
            .id("JMG_List")
            .and_then(|n| self.view.selected(n))
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| self.game_ids.get(i))
            .copied()
    }
    fn set_active(&mut self, command: &str, active: bool) {
        if let Some(n) = self.view.by_command(command) {
            self.view.set_active(n, active);
        }
    }
    fn refresh(&mut self, core: &Core) {
        match self.kind {
            Kind::List => {
                let old = self.selected_game;
                let games = &core.minigames.games;
                self.selected_game = core
                    .minigames
                    .retain_game_target(old)
                    .or_else(|| games.first().map(|g| g.id));
                // Slayer's Default column, while a listed game is the default.
                let defaults = games.iter().any(|g| g.default);
                if let Some(n) = self.view.id(DEFAULT_HEADER) {
                    self.view.set_visible(n, defaults);
                }
                if let Some(n) = self.view.id("JMG_List") {
                    self.game_ids = games.iter().map(|g| g.id).collect();
                    let columns = if defaults {
                        Some(DEFAULT_COLUMNS.to_owned())
                    } else {
                        self.list_columns.clone()
                    };
                    match columns {
                        Some(c) => {
                            self.view.nodes[n].ctrl.fields.insert("columns".into(), c);
                        }
                        None => {
                            self.view.nodes[n].ctrl.fields.remove("columns");
                        }
                    }
                    // `MiniGameSO::getLine` in the game's colour: creator, BL_ID, title, invite-only.
                    self.view.nodes[n].state.items = games
                        .iter()
                        .enumerate()
                        .map(|(i, g)| {
                            let color = char::from_u32(
                                crate::text::COLOR_CODE_BASE + u32::from(g.color.min(9)),
                            )
                            .unwrap_or(' ');
                            let bl_id = core
                                .players
                                .iter()
                                .find(|p| p.id == g.owner.0)
                                .and_then(|p| p.bl_id)
                                .map(|id| id.to_string())
                                .unwrap_or_default();
                            let mut line = format!(
                                "{color}{}\t{bl_id}\t{}\t{}",
                                g.owner_name,
                                g.title,
                                u8::from(g.invite_only)
                            );
                            if defaults {
                                line.push('\t');
                                if g.default {
                                    line.push_str("Yes");
                                }
                            }
                            (line, i as i64)
                        })
                        .collect();
                    self.view.select(
                        n,
                        self.selected_game
                            .and_then(|id| self.game_ids.iter().position(|g| *g == id))
                            .map(|i| i as i64),
                    );
                }
                let selected = games.iter().find(|g| Some(g.id) == self.selected_game);
                let join = selected.is_some_and(|g| !g.invite_only)
                    && core
                        .minigames
                        .can(crate::models::minigames::Operation::Join)
                    && core.minigames.active_game.is_none()
                    && self.request.is_none();
                self.set_active("JoinMiniGameGui.clickJoin();", join);
                self.set_active(
                    "JoinMiniGameGui.clickLeave();",
                    core.minigames.active_game.is_some() && self.request.is_none(),
                );
                self.set_active(
                    "JoinMiniGameGui.clickCreate();",
                    (core
                        .minigames
                        .can(crate::models::minigames::Operation::Create)
                        || core
                            .minigames
                            .can(crate::models::minigames::Operation::Configure))
                        && self.request.is_none(),
                );
                for (name, shown) in [
                    ("JMG_JoinBlocker", !join),
                    ("JMG_LeaveBlocker", core.minigames.active_game.is_none()),
                    ("JMG_CreateBlocker", false),
                ] {
                    if let Some(n) = self.view.id(name) {
                        self.view.set_visible(n, shown);
                    }
                }
                self.status(core);
            }
            Kind::Invite => {
                if core.minigames.invitations.is_empty() {
                    return;
                }
                if let Some(i) = core.minigames.invitations.last() {
                    for (name, value) in [
                        ("MGI_Title", i.title.as_str()),
                        ("MGI_Name", i.owner_name.as_str()),
                        ("MGI_BL_ID", i.owner_display_id.as_str()),
                    ] {
                        if let Some(n) = self.view.id(name) {
                            self.view.set_text(n, value);
                        }
                    }
                }
                for cmd in [
                    "MiniGameInviteGui.clickAccept();",
                    "MiniGameInviteGui.clickReject();",
                    "MiniGameInviteGui.clickIgnore();",
                ] {
                    self.set_active(
                        cmd,
                        core.minigames
                            .can(crate::models::minigames::Operation::AcceptInvite)
                            && self.request.is_none(),
                    );
                }
            }
        }
    }
    fn status(&mut self, core: &Core) {
        let key = match self.kind {
            Kind::List => "NativeMiniGameListStatus",
            Kind::Invite => "NativeMiniGameInviteStatus",
        };
        let status = if !core.minigames.ready {
            "Waiting for the server's mini-games.".to_string()
        } else {
            core.minigames.status.clone()
        };
        if let Some(n) = self.view.id(key) {
            self.view.set_text(n, &status);
        } else {
            // Below the authored window content, inside the window: the
            // window grows by the row, never past v20's 480-high canvas.
            let parent = window(&self.view).unwrap_or(self.view.root);
            let [w, h] = self.view.nodes[parent].ctrl.extent;
            let grow = 26.min((480 - h).max(0));
            self.view.nodes[parent].ctrl.extent[1] = h + grow;
            let list = matches!(self.kind, Kind::List);
            let mut c = text(
                "GuiTextProfile",
                Rect::new(
                    12,
                    h + grow - 28,
                    (w - 24 - if list { 88 } else { 0 }).max(0),
                    24,
                ),
                &status,
            );
            c.name = Some(key.into());
            c.class = "GuiMLTextCtrl".into();
            self.view.add(parent, c);
            if list {
                let mut b = button(
                    "BlockButtonProfile",
                    Rect::new(w - 12 - 76, h + grow - 30, 76, 26),
                    "base/client/ui/button1",
                    "Settings",
                    SETTINGS_BUTTON,
                );
                b.name = Some(SETTINGS_BUTTON.into());
                self.view.add(parent, b);
            }
        }
        if let Some(n) = self.view.id(SETTINGS_BUTTON) {
            let picked = self.selected_game();
            self.view.set_active(n, picked.is_some());
        }
    }
}

impl Screen for MiniGameScreen {
    fn id(&self) -> ScreenId {
        self.id
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
    fn on_wake(&mut self, core: &mut Core) {
        if matches!(self.kind, Kind::List)
            && core
                .minigames
                .can(crate::models::minigames::Operation::List)
        {
            self.request =
                core.minigame_request(MiniGameOperation::List, UiAction::RequestMiniGameList);
        }
        self.refresh(core);
    }
    fn on_sleep(&mut self, core: &mut Core) {
        if let Some(id) = self.request.take() {
            core.pending.remove(&id);
        }
    }
    fn on_update(&mut self, core: &mut Core) {
        if matches!(self.kind, Kind::Invite) && core.minigames.invitations.is_empty() {
            core.pop(self.id);
            return;
        }
        self.refresh(core);
    }
    fn on_result(
        &mut self,
        id: RequestId,
        kind: Option<&Pending>,
        result: &Result<(), String>,
        core: &mut Core,
    ) -> bool {
        if self.request != Some(id) || !matches!(kind, Some(Pending::MiniGame(_))) {
            return false;
        }
        self.request = None;
        core.minigames.status = result
            .as_ref()
            .map_or_else(|e| e.clone(), |_| "Mini-game request completed.".into());
        self.refresh(core);
        true
    }
    fn on_key(&mut self, key: Key, _: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            core.pop(self.id);
            true
        } else {
            false
        }
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if !self.view.node(ev.node).state.active {
            return;
        }
        if ev.kind == EventKind::Close {
            core.pop(self.id);
            return;
        }
        if ev.kind == EventKind::Changed {
            if self.view.node(ev.node).ctrl.name.as_deref() == Some("JMG_List") {
                self.selected_game = self.selected_game();
                self.refresh(core);
            }
            return;
        }
        if !matches!(
            ev.kind,
            EventKind::Click | EventKind::Submit | EventKind::DoubleClick
        ) {
            return;
        }
        let cmd = command_of(&self.view, ev.node).to_ascii_lowercase();
        if cmd == SETTINGS_BUTTON.to_ascii_lowercase() {
            if let Some(game) = self.selected_game() {
                core.minigame_addons = Some(game);
                core.push(ScreenId::MiniGameSettings);
            }
            return;
        }
        match self.kind {
            Kind::List => match cmd.as_str() {
                "canvas.popdialog(joinminigamegui);" => core.pop(self.id),
                "joinminigamegui.clicklist();" => {
                    self.selected_game = self.selected_game();
                    self.refresh(core);
                }
                "joinminigamegui.clickjoin();" => {
                    if let Some(game) = self.selected_game() {
                        self.request = core.minigame_request(
                            MiniGameOperation::Join,
                            UiAction::JoinMiniGame { game },
                        );
                        self.refresh(core);
                    }
                }
                "joinminigamegui.clickleave();" => {
                    if let Some(game) = core.minigames.active_game {
                        if core.minigames.owns_active_game {
                            core.message_yes_no(
                                "End Mini-Game?",
                                "Are you sure you want to end the mini-game?",
                                Callback::MiniGame {
                                    game,
                                    operation: MiniGameOperation::End,
                                },
                            );
                        } else if let Some(id) = core.minigame_request(
                            MiniGameOperation::Leave,
                            UiAction::LeaveMiniGame { game },
                        ) {
                            self.request = Some(id);
                            self.refresh(core);
                        }
                    }
                }
                "joinminigamegui.clickcreate();" => {
                    core.pop(self.id);
                    core.push(ScreenId::MiniGameSettings);
                }
                _ => {
                    let col = cmd
                        .strip_prefix("joinminigamegui.sortlist(")
                        .or_else(|| cmd.strip_prefix("joinminigamegui.sortnumlist("))
                        .and_then(|s| s.strip_suffix(");"))
                        .and_then(|s| s.parse::<usize>().ok());
                    if col.is_some() {
                        self.refresh(core);
                    }
                }
            },
            Kind::Invite => {
                let Some(invite) = core.minigames.invitations.last().cloned() else {
                    return;
                };
                match cmd.as_str() {
                    "minigameinvitegui.clickaccept();" => {
                        self.request = core.minigame_request(
                            MiniGameOperation::AcceptInvite,
                            UiAction::AcceptMiniGameInvite { game: invite.game },
                        );
                    }
                    "minigameinvitegui.clickreject();" => {
                        self.request = core.minigame_request(
                            MiniGameOperation::RejectInvite,
                            UiAction::RejectMiniGameInvite {
                                game: invite.game,
                                ignore_owner: false,
                            },
                        );
                    }
                    "minigameinvitegui.clickignore();" => core.message_yes_no(
                        "Ignore User?",
                        "Are you sure you want to ignore mini-game invites from this user?",
                        Callback::MiniGame {
                            game: invite.game,
                            operation: MiniGameOperation::IgnoreInvite,
                        },
                    ),
                    _ => {}
                }
            }
        }
        self.refresh(core);
    }
}
