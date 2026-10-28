//! Live player list; identity/trust are host-provided data, never authority.
use super::*;
use crate::api::UiAction;
use crate::view::EventKind;

/// The Trust column's start inside NPL_List.
const TRUST_COLUMN_X: i32 = 310;
/// The Voice column's start inside NPL_List, after Trust.
const VOICE_COLUMN_X: i32 = 372;
/// How much wider the window gets for it; off, the list keeps its width.
const VOICE_EXTRA_WIDTH: i32 = 104;
/// Left edge, in the window, from which the right-hand action column starts
/// (Trust Invite, Trust Demote, Mini-Game, Un-Ignore, Close, Mute). The
/// sortable header buttons all start left of it; the column itself starts
/// near 385.
const RIGHT_COLUMN_X: i32 = 340;

pub struct Players {
    view: View,
    ids: Vec<u64>,
    sort: usize,
    descending: bool,
    /// `showTrustMessage`: the "trust invite sent" window, in ms left.
    trust_message_ms: u64,
    /// The Voice column is laid out (the host runs voice chat).
    voice_laid_out: bool,
}
/// The Voice column's text for one player.
fn voice_state(core: &Core, id: u64) -> &'static str {
    if core.voice_muted.contains(&id) || core.server_voice_muted.contains(&id) {
        "Muted"
    } else if core.voice_speaking.contains(&id) {
        "On"
    } else {
        "Off"
    }
}
impl Players {
    pub fn new(core: &Core) -> Self {
        let mut s = Self {
            view: layout_view(core, "NewPlayerListGui"),
            ids: vec![],
            sort: 1,
            descending: false,
            trust_message_ms: 0,
            voice_laid_out: false,
        };
        for n in s.view.walk().collect::<Vec<_>>() {
            let command = s
                .view
                .node(n)
                .ctrl
                .command
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase();
            if command.contains("clickminigame") {
                s.view.set_active(n, false);
            }
            if s.view.node(n).ctrl.name.as_deref() == Some("NPL_TrustWindow") {
                s.view.set_visible(n, false);
            }
        }
        // The voice chat's mute button: top of the right-hand action column,
        // above the trust buttons. It shows only when the host runs voice
        // chat (its Add-On on), which refresh() keeps in step.
        if let Some(parent) = s.view.id("NPL_Window") {
            let mut b = button(
                "BlockButtonProfile",
                Rect::new(405, 33, 76, 21),
                "base/client/ui/button1",
                "Mute",
                "NativeVoiceMute",
            );
            b.name = Some("NativeVoiceMute".into());
            s.view.add(parent, b);
        }
        // The Voice column, when the host runs voice chat; refresh() keeps
        // it in step if the state arrives after the screen opened.
        s.apply_voice_layout(core);
        s.refresh(core);
        s
    }
    /// Widen the window for the Voice column and add its header, once per
    /// opened screen.
    ///
    /// A control's size follows its parent only by the difference between the
    /// parent's authored extent and its laid-out one (`GuiControl::
    /// parentResized`), and the window's authored extent is what is changed
    /// here, so that difference is zero and nothing inside would follow. The
    /// window's contents are therefore moved and stretched directly: the
    /// scroll area and its list gain the width, the right-hand action column
    /// (and the Mute button) shifts right by it, and the rows carry a sixth
    /// field: On / Off / Muted.
    fn apply_voice_layout(&mut self, core: &Core) {
        if !core.voice_on_server || self.voice_laid_out {
            return;
        }
        self.voice_laid_out = true;
        let Some(window) = self.view.id("NPL_Window") else {
            return;
        };
        let dx = VOICE_EXTRA_WIDTH;
        self.view.nodes[window].ctrl.extent[0] += dx;

        // The right-hand action column moves clear of the wider list. The
        // runtime-built Mute button sits in it too.
        for k in self.view.nodes[window].children.clone() {
            let c = &mut self.view.nodes[k].ctrl;
            if c.position[0] >= RIGHT_COLUMN_X {
                c.position[0] += dx;
            }
        }

        // The list, and every control between it and the window (its scroll
        // area), gain the width.
        if let Some(list) = self.view.id("NPL_List") {
            // v20's `fitParentWidth`: inside a scroll area the list fills it
            // up to the scroll bar's lane, on every layout pass.
            let gutter = self.view.nodes[list]
                .parent
                .filter(|&p| p != window)
                .map(|p| {
                    (self.view.nodes[p].ctrl.extent[0] - self.view.nodes[list].ctrl.extent[0])
                        .max(0)
                });
            if let Some(g) = gutter {
                self.view.nodes[list]
                    .ctrl
                    .fields
                    .insert("fitParentWidth".into(), g.to_string());
            }
            let mut path = vec![];
            let mut n = list;
            while n != window {
                path.push(n);
                match self.view.nodes[n].parent {
                    Some(p) => n = p,
                    None => {
                        path.clear();
                        break;
                    }
                }
            }
            for n in path {
                self.view.nodes[n].ctrl.extent[0] += dx;
            }
            self.view.nodes[list]
                .ctrl
                .fields
                .insert("columns".into(), format!("0 33 190 245 {TRUST_COLUMN_X} {VOICE_COLUMN_X}"));
        }

        // The Voice header: the Trust header's button, one column over, so it
        // looks like its neighbours. It does not sort. A layout without that
        // button gets a plain label.
        let trust_header = self.view.walk().find(|&n| {
            self.view.node(n).ctrl.command.as_deref().is_some_and(|cmd| {
                let cmd = cmd.to_ascii_lowercase();
                cmd.starts_with("newplayerlistgui.sort") && cmd.ends_with("(4);")
            })
        });
        if let Some(t) = trust_header
            && let Some(parent) = self.view.node(t).parent
        {
            let mut header = self.view.node(t).ctrl.clone();
            header.name = Some("NativeVoiceColumn".into());
            header.command = None;
            header.alt_command = None;
            header.text = Some("Voice".into());
            header.position[0] += VOICE_COLUMN_X - TRUST_COLUMN_X;
            // A header row inside a container needs the width too.
            let mut up = Some(parent);
            while let Some(p) = up.filter(|&p| p != window) {
                if self.view.nodes[p].ctrl.position[0] < RIGHT_COLUMN_X {
                    self.view.nodes[p].ctrl.extent[0] += dx;
                }
                up = self.view.nodes[p].parent;
            }
            self.view.add(parent, header);
        } else if let Some(parent) = self.view.id("NPL_Window") {
            let mut label = text(
                "GuiTextProfile",
                Rect::new(VOICE_COLUMN_X, 33, 84, 19),
                "Voice",
            );
            label.name = Some("NativeVoiceColumn".into());
            self.view.add(parent, label);
        }
        self.view.relayout();
    }
    fn refresh(&mut self, core: &Core) {
        let selected = self
            .view
            .id("NPL_List")
            .and_then(|n| self.view.selected(n))
            .and_then(|i| self.ids.get(i as usize))
            .copied();
        let mut rows = core.players.iter().collect::<Vec<_>>();
        rows.sort_by(|a, b| {
            let order = match self.sort {
                0 => (a.super_admin, a.admin).cmp(&(b.super_admin, b.admin)),
                2 => a.score.cmp(&b.score),
                3 => a.bl_id.cmp(&b.bl_id),
                4 => a
                    .trust
                    .to_ascii_lowercase()
                    .cmp(&b.trust.to_ascii_lowercase()),
                _ => a
                    .name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase()),
            }
            .then_with(|| a.id.cmp(&b.id));
            if self.descending {
                order.reverse()
            } else {
                order
            }
        });
        self.ids = rows.iter().map(|p| p.id).collect();
        if let Some(n) = self.view.id("NPL_List") {
            // The Voice column's state, when the host runs voice chat: Muted
            // (by this player or by the host), else On while they speak, else
            // Off. With voice chat off, rows keep their five fields and the
            // list its original width.
            let voice = core.voice_on_server;
            self.view.state(n).items = rows
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let member = core.minigames.members.iter().find(|m| m.id.0 == p.id);
                    let same_game = member.is_some_and(|m| m.in_local_game);
                    let mut line = format!(
                        "{}{}\t{}\t{}\t{}\t{}",
                        if same_game { "\u{e005}" } else { "" },
                        if p.super_admin {
                            "S"
                        } else if p.admin {
                            "A"
                        } else {
                            "-"
                        },
                        p.name.replace(['\t', '\n', '\r'], " ")
                            + if p.loading { " (loading)" } else { "" },
                        member
                            .filter(|m| m.in_local_game)
                            .map_or(i64::from(p.score), |m| m.score),
                        p.bl_id.map(|id| id.to_string()).unwrap_or_default(),
                        p.trust.replace(['\t', '\n', '\r'], " ")
                    );
                    if voice {
                        line.push('\t');
                        line.push_str(voice_state(core, p.id));
                    }
                    (line, i as i64)
                })
                .collect();
            self.view.select(
                n,
                selected
                    .and_then(|id| self.ids.iter().position(|p| *p == id))
                    .map(|i| i as i64),
            );
        }
        if let Some(n) = self.view.id("NPL_Window") {
            self.view.set_text(
                n,
                format!(
                    "{} - {}/{} Players",
                    core.server_name,
                    core.players.len(),
                    core.max_players
                ),
            );
        }
        let selected_id = self
            .view
            .id("NPL_List")
            .and_then(|n| self.view.selected(n))
            .and_then(|i| self.ids.get(i as usize))
            .copied();
        let selected_member =
            selected_id.and_then(|id| core.minigames.members.iter().find(|m| m.id.0 == id));
        let invite = core
            .minigames
            .can(crate::models::minigames::Operation::Invite)
            && selected_id.is_some()
            && selected_id != core.minigames.local_player.map(|p| p.0)
            && selected_member.is_none_or(|m| !m.in_local_game);
        let remove = core
            .minigames
            .can(crate::models::minigames::Operation::RemoveMember)
            && selected_member.is_some_and(|m| m.in_local_game && !m.is_owner)
            && selected_id != core.minigames.local_player.map(|p| p.0);
        for (command, active) in [
            ("NewPlayerListGui.clickMiniGameInvite();", invite),
            ("NewPlayerListGui.clickMiniGameRemove();", remove),
        ] {
            if let Some(n) = self.view.by_command(command) {
                self.view.set_active(n, active);
            }
        }
        if let Some(n) = self.view.id("NPL_MiniGameInviteBlocker") {
            self.view.set_visible(n, !invite);
        }
        if let Some(n) = self.view.id("NPL_MiniGameRemoveBlocker") {
            self.view.set_visible(n, !remove);
        }
        // `NewPlayerListGui::clickList` trust and ignore blockers.
        let row = selected_id.and_then(|id| core.players.iter().find(|p| p.id == id));
        let lan = core.players.iter().any(|p| p.trust == "LAN");
        let trust = row.map_or("", |p| p.trust.as_str());
        let (invite_build, invite_full, remove_build, remove_full) = match trust {
            _ if lan || row.is_none() => (false, false, false, false),
            "Build" => (false, true, true, false),
            "Full" => (false, false, true, true),
            "You" => (false, false, false, false),
            _ => (true, true, false, false),
        };
        let unignore = row.is_some_and(|p| p.ignoring);
        for (command, blocker, active) in [
            (
                "NewPlayerListGui.clickTrustInviteBuild();",
                "NPL_TrustInviteBuildBlocker",
                invite_build,
            ),
            (
                "NewPlayerListGui.clickTrustInviteFull();",
                "NPL_TrustInviteFullBlocker",
                invite_full,
            ),
            (
                "NewPlayerListGui.ClickTrustDemoteNONE();",
                "NPL_TrustRemoveBuildBlocker",
                remove_build,
            ),
            (
                "NewPlayerListGui.ClickTrustDemoteBUILD();",
                "NPL_TrustRemoveFullBlocker",
                remove_full,
            ),
            (
                "NewPlayerListGui.clickUnIgnore();",
                "NPL_UnIgnoreBlocker",
                unignore,
            ),
        ] {
            if let Some(n) = self.view.by_command(command) {
                self.view.set_active(n, active);
            }
            if let Some(n) = self.view.id(blocker) {
                self.view.set_visible(n, !active);
            }
        }
        // The Voice column, in case the state arrived after opening.
        self.apply_voice_layout(core);
        // The voice chat's mute button: only when the host runs voice chat,
        // and it toggles by whether the selected player is muted locally.
        if let Some(n) = self.view.id("NativeVoiceMute") {
            let voice = core.voice_on_server;
            self.view.set_visible(n, voice);
            if voice && selected_id.is_some() {
                let muted = selected_id.is_some_and(|id| core.voice_muted.contains(&id));
                self.view.set_text(n, if muted { "Unmute" } else { "Mute" });
            }
            // Muting yourself would only silence you for no one: off.
            let self_ = row.is_some_and(|p| p.trust == "You");
            self.view
                .set_active(n, voice && selected_id.is_some() && !self_);
        }
    }
    fn selected(&self) -> Option<u64> {
        self.view
            .id("NPL_List")
            .and_then(|n| self.view.selected(n))
            .and_then(|i| self.ids.get(i as usize))
            .copied()
    }
}
impl Screen for Players {
    fn id(&self) -> ScreenId {
        ScreenId::PlayerList
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
    fn opening_command(&self) -> Option<&'static str> {
        Some("showPlayerList")
    }
    fn on_update(&mut self, core: &mut Core) {
        self.refresh(core);
    }
    fn tick(&mut self, dt_ms: u64, _core: &mut Core) {
        if self.trust_message_ms > 0 {
            self.trust_message_ms = self.trust_message_ms.saturating_sub(dt_ms);
            if self.trust_message_ms == 0
                && let Some(n) = self.view.id("NPL_TrustWindow")
            {
                self.view.set_visible(n, false);
            }
        }
    }
    fn on_result(
        &mut self,
        _id: RequestId,
        kind: Option<&Pending>,
        result: &Result<(), String>,
        core: &mut Core,
    ) -> bool {
        if !matches!(kind, Some(Pending::MiniGame(_))) {
            return false;
        }
        core.minigames.status = result
            .as_ref()
            .map_or_else(|e| e.clone(), |_| "Mini-game request completed.".into());
        self.refresh(core);
        // A refusal falls through to the shared notice every screen uses.
        result.is_ok()
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        if key == Key::Escape {
            core.pop(self.id());
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
            core.pop(self.id());
            return;
        }
        if ev.kind == EventKind::Changed
            && self.view.node(ev.node).ctrl.name.as_deref() == Some("NPL_List")
        {
            self.refresh(core);
            return;
        }
        if ev.kind != EventKind::Click {
            return;
        }
        let cmd = command_of(&self.view, ev.node).to_ascii_lowercase();
        match cmd.as_str() {
            "canvas.popdialog(newplayerlistgui);" => core.pop(self.id()),
            "newplayerlistgui.clicklist();" => self.refresh(core),
            "newplayerlistgui.clicktrustinvitebuild();"
            | "newplayerlistgui.clicktrustinvitefull();" => {
                if let Some(target) = self.selected() {
                    let level = if cmd.ends_with("build();") { 1 } else { 2 };
                    core.request(UiAction::TrustInvite { target, level });
                    // `showTrustMessage`.
                    if let Some(n) = self.view.id("NPL_TrustWindow") {
                        self.view.set_visible(n, true);
                        self.trust_message_ms = 800;
                    }
                }
            }
            "newplayerlistgui.clicktrustdemotenone();"
            | "newplayerlistgui.clicktrustdemotebuild();" => {
                if let Some(target) = self.selected() {
                    let level = if cmd.ends_with("none();") { 0 } else { 1 };
                    core.request(UiAction::TrustDemote { target, level });
                }
            }
            "newplayerlistgui.clickunignore();" => {
                if let Some(target) = self.selected() {
                    core.request(UiAction::UnIgnore { target });
                }
            }
            // Local voice mute: this player's ears only, never the host's.
            "nativevoicemute" => {
                if let Some(target) = self.selected() {
                    if core.voice_muted.contains(&target) {
                        core.request(UiAction::VoiceUnmute { target });
                    } else {
                        core.request(UiAction::VoiceMute { target });
                    }
                }
            }
            "newplayerlistgui.clickminigameinvite();" => {
                if core
                    .minigames
                    .can(crate::models::minigames::Operation::Invite)
                    && let Some(id) = self
                        .view
                        .id("NPL_List")
                        .and_then(|n| self.view.selected(n))
                        .and_then(|i| self.ids.get(i as usize))
                        .copied()
                {
                    core.minigame_request(
                        MiniGameOperation::Invite,
                        UiAction::InviteMiniGame {
                            target: MiniGamePlayerId(id),
                        },
                    );
                }
            }
            "newplayerlistgui.clickminigameremove();" => {
                if core
                    .minigames
                    .can(crate::models::minigames::Operation::RemoveMember)
                    && let Some(id) = self
                        .view
                        .id("NPL_List")
                        .and_then(|n| self.view.selected(n))
                        .and_then(|i| self.ids.get(i as usize))
                        .copied()
                {
                    core.minigame_request(
                        MiniGameOperation::RemoveMember,
                        UiAction::RemoveMiniGameMember {
                            target: MiniGamePlayerId(id),
                            game: core.minigames.active_game,
                        },
                    );
                }
            }
            _ => {
                let column = cmd
                    .strip_prefix("newplayerlistgui.sortlist(")
                    .or_else(|| cmd.strip_prefix("newplayerlistgui.sortnumlist("))
                    .and_then(|s| s.strip_suffix(");"))
                    .and_then(|s| s.parse::<usize>().ok());
                if let Some(column) = column.filter(|c| *c <= 4) {
                    self.descending = if column == self.sort {
                        !self.descending
                    } else {
                        false
                    };
                    self.sort = column;
                    self.refresh(core);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{PlayerRow, Settings};
    use crate::binds::Platform;
    use crate::schema::UiPack;
    use crate::ui::{Ui, UiConfig};
    use std::rc::Rc;
    #[test]
    fn live_player_rows_preserve_identity_sort_and_remove_selection() {
        let mut pack = UiPack::default();
        let mut c = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        let mut list = ctrl(
            "GuiTextListCtrl",
            "GuiDefaultProfile",
            Rect::new(0, 0, 300, 200),
        );
        list.name = Some("NPL_List".into());
        c.children.push(list);
        let mut trust = ctrl("GuiButtonCtrl", "GuiButtonProfile", Rect::new(0, 0, 70, 20));
        trust.command = Some("NewPlayerListGui.clickTrustInviteBuild();".into());
        trust.name = Some("trust".into());
        c.children.push(trust);
        pack.layouts.insert("NewPlayerListGui".into(), c);
        let mut ui = Ui::new(
            Rc::new(Pack::from_parts(pack, Default::default())),
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
        ui.core.players = vec![
            PlayerRow {
                id: u64::MAX,
                name: "Zed".into(),
                score: 3,
                admin: true,
                super_admin: false,
                bl_id: Some(12),
                trust: "Build".into(),
                ignoring: false,
                loading: false,
            },
            PlayerRow {
                id: 2,
                name: "Ada".into(),
                score: 8,
                admin: false,
                super_admin: false,
                bl_id: None,
                trust: "None".into(),
                ignoring: false,
                loading: true,
            },
        ];
        let mut s = Players::new(&ui.core);
        let n = s.view.id("NPL_List").unwrap();
        assert_eq!(s.ids, vec![2, u64::MAX]);
        // A player still loading is listed, and says so.
        assert!(s.view.state(n).items[0].0.contains("\tAda (loading)\t"));
        assert!(s.view.state(n).items[1].0.contains("\tZed\t"));
        s.view.select(n, Some(1));
        s.sort = 2;
        s.refresh(&ui.core);
        assert_eq!(s.ids, vec![u64::MAX, 2]);
        assert_eq!(s.view.selected(n), Some(0));
        let trust = s.view.id("trust").unwrap();
        assert!(!s.view.node(trust).state.active);
        s.on_event(
            &ViewEvent {
                node: trust,
                kind: EventKind::Click,
            },
            &mut ui.core,
        );
        assert!(ui.drain_actions().is_empty());
        ui.core.players.retain(|p| p.id != u64::MAX);
        s.on_update(&mut ui.core);
        assert_eq!(s.view.selected(n), None);
        assert_eq!(s.ids, vec![2]);
    }
    /// The NewPlayerListGui as the content pack authors it: window 489 wide,
    /// list columns after Trust (310).
    fn player_list_ui() -> Ui {
        let mut pack = UiPack::default();
        let mut window = ctrl(
            "GuiControl",
            "GuiDefaultProfile",
            Rect::new(75, 74, 489, 332),
        );
        window.name = Some("NPL_Window".into());
        let mut list = ctrl(
            "GuiTextListCtrl",
            "GuiDefaultProfile",
            Rect::new(11, 54, 357, 140),
        );
        list.name = Some("NPL_List".into());
        list.fields
            .insert("columns".into(), "0 33 190 245 310".into());
        window.children.push(list);
        // The Trust header button, and a control of the right-hand action
        // column, as the content pack authors them.
        let mut header = ctrl(
            "GuiBitmapButtonCtrl",
            "BlockButtonProfile",
            Rect::new(315, 33, 46, 19),
        );
        header.command = Some("NewPlayerListGui.sortList(4);".into());
        header.text = Some("Trust".into());
        header.name = Some("trust_header".into());
        window.children.push(header);
        let mut close = ctrl(
            "GuiBitmapButtonCtrl",
            "BlockButtonProfile",
            Rect::new(386, 290, 94, 30),
        );
        close.name = Some("close".into());
        window.children.push(close);
        // As authored, a full-screen root holds the window.
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        root.children.push(window);
        pack.layouts.insert("NewPlayerListGui".into(), root);
        Ui::new(
            Rc::new(Pack::from_parts(pack, Default::default())),
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
    fn three_players(ui: &mut Ui) {
        let row = |id: u64, name: &str| PlayerRow {
            id,
            ignoring: false,
            name: name.into(),
            score: 0,
            admin: false,
            super_admin: false,
            bl_id: Some(id * 1000),
            trust: "-".into(),
            loading: false,
        };
        ui.core.players = vec![row(1, "Hosty"), row(7, "Guesty"), row(9, "Walker")];
    }
    #[test]
    fn voice_on_widens_the_list_and_states_each_players_mute() {
        let mut ui = player_list_ui();
        three_players(&mut ui);
        // The host runs voice chat: Walker is muted locally, Guesty by an
        // admin, and Hosty is talking (so is Guesty, to no one's ears).
        ui.core.voice_on_server = true;
        ui.core.voice_muted.insert(9);
        ui.core.server_voice_muted.insert(7);
        ui.core.voice_speaking.insert(1);
        ui.core.voice_speaking.insert(7);
        let mut s = Players::new(&ui.core);

        // The window widens for the Voice column, with its header...
        let w = s.view.id("NPL_Window").unwrap();
        assert_eq!(s.view.node(w).ctrl.extent[0], 489 + VOICE_EXTRA_WIDTH);
        let h = s.view.id("NativeVoiceColumn").unwrap();
        assert_eq!(s.view.text_of(h), "Voice");
        // ...and the list's columns reach it.
        let n = s.view.id("NPL_List").unwrap();
        assert!(s.view.node(n).ctrl.fields["columns"].ends_with(&format!(" {VOICE_COLUMN_X}")));

        // Each row carries its voice state as a sixth field: Muted (locally
        // or by the host) before On (speaking), and Off otherwise.
        let items: Vec<&str> = s
            .view
            .state(n)
            .items
            .iter()
            .map(|(t, _)| t.as_str())
            .collect();
        assert_eq!(items.len(), 3);
        for (name, want) in [("Guesty", "Muted"), ("Hosty", "On"), ("Walker", "Muted")] {
            let row = items.iter().find(|t| t.contains(name)).unwrap();
            assert!(
                row.ends_with(&format!("\t{want}")),
                "\"{row}\" ends with \"{want}\""
            );
        }
        // Hosty stops talking.
        ui.core.voice_speaking.remove(&1);
        s.refresh(&ui.core);
        let row = s
            .view
            .state(n)
            .items
            .iter()
            .find(|(t, _)| t.contains("Hosty"))
            .unwrap()
            .0
            .clone();
        assert!(row.ends_with("\tOff"), "{row}");

        // The mute button follows the selected row's local state.
        let b = s.view.id("NativeVoiceMute").unwrap();
        s.view.select(n, Some(0)); // Guesty: not locally muted -> Mute
        s.refresh(&ui.core);
        assert!(s.view.node(b).state.active);
        assert_eq!(s.view.text_of(b), "Mute");
        s.view.select(n, Some(2)); // Walker: locally muted -> Unmute
        s.refresh(&ui.core);
        assert_eq!(s.view.text_of(b), "Unmute");
    }
    #[test]
    fn voice_on_moves_the_action_column_and_stretches_the_list_when_laid_out() {
        let mut ui = player_list_ui();
        three_players(&mut ui);
        ui.core.voice_on_server = true;
        let mut s = Players::new(&ui.core);
        s.view.layout(640, 480);
        let rect = |s: &Players, name: &str| s.view.node(s.view.id(name).unwrap()).rect;
        let window = rect(&s, "NPL_Window");
        assert_eq!(window.w, 489 + VOICE_EXTRA_WIDTH);
        // The list is as wide as it was plus the Voice column's width...
        let list = rect(&s, "NPL_List");
        assert_eq!(list.w, 357 + VOICE_EXTRA_WIDTH);
        // ...and the Voice column fits inside it.
        assert!(VOICE_COLUMN_X + 84 <= list.w);
        // The action column stays inside the wider window, and the Mute button
        // sits in that column, clear of the list.
        let close = rect(&s, "close");
        assert_eq!(close.x - window.x, 386 + VOICE_EXTRA_WIDTH);
        assert!(close.right() <= window.right());
        let mute = rect(&s, "NativeVoiceMute");
        assert_eq!(mute.x - window.x, 405 + VOICE_EXTRA_WIDTH);
        assert!(mute.x >= list.right());
        // The Voice header is a button like Trust's, one column over.
        let trust = rect(&s, "trust_header");
        let voice = rect(&s, "NativeVoiceColumn");
        assert_eq!(voice.x - trust.x, VOICE_COLUMN_X - TRUST_COLUMN_X);
        assert_eq!((voice.w, voice.h), (trust.w, trust.h));
        assert_eq!(s.view.text_of(s.view.id("NativeVoiceColumn").unwrap()), "Voice");
    }
    #[test]
    fn voice_on_fills_a_scroll_area_up_to_its_bar() {
        // As authored: the list sits in a scroll area, a bar's lane narrower.
        let mut pack = UiPack::default();
        let mut window = ctrl(
            "GuiControl",
            "GuiDefaultProfile",
            Rect::new(75, 74, 489, 332),
        );
        window.name = Some("NPL_Window".into());
        let mut scroll = ctrl(
            "GuiScrollCtrl",
            "GuiDefaultProfile",
            Rect::new(11, 54, 372, 200),
        );
        let mut list = ctrl("GuiTextListCtrl", "GuiDefaultProfile", Rect::new(0, 0, 357, 140));
        list.name = Some("NPL_List".into());
        scroll.children.push(list);
        window.children.push(scroll);
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        root.children.push(window);
        pack.layouts.insert("NewPlayerListGui".into(), root);
        let mut ui = Ui::new(
            Rc::new(Pack::from_parts(pack, Default::default())),
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
        ui.core.voice_on_server = true;
        let mut s = Players::new(&ui.core);
        s.view.layout(640, 480);
        // A relayout from inside the view keeps it too.
        s.view.relayout();
        let n = s.view.id("NPL_List").unwrap();
        let list = s.view.node(n).rect;
        let scroll = s.view.node(s.view.node(n).parent.unwrap()).rect;
        assert_eq!(scroll.w, 372 + VOICE_EXTRA_WIDTH);
        assert_eq!(list.w, scroll.w - 15);
    }
    #[test]
    fn voice_off_keeps_the_list_its_original_width_and_fields() {
        let mut ui = player_list_ui();
        three_players(&mut ui);
        // The host does not run voice chat: nothing changes.
        assert!(!ui.core.voice_on_server);
        let mut s = Players::new(&ui.core);

        let w = s.view.id("NPL_Window").unwrap();
        assert_eq!(s.view.node(w).ctrl.extent[0], 489);
        assert!(s.view.id("NativeVoiceColumn").is_none());
        let n = s.view.id("NPL_List").unwrap();
        assert_eq!(
            s.view.node(n).ctrl.fields["columns"].as_str(),
            "0 33 190 245 310"
        );
        // The action column stays where it was authored.
        s.view.layout(640, 480);
        let close = s.view.node(s.view.id("close").unwrap()).rect;
        let window = s.view.node(w).rect;
        assert_eq!(close.x - window.x, 386);
        for (t, _) in &s.view.state(n).items {
            assert_eq!(t.matches('\t').count(), 4, "five fields: {t}");
        }
        // Even with a row selected, the button stays off.
        let b = s.view.id("NativeVoiceMute").unwrap();
        s.view.select(n, Some(0));
        s.refresh(&ui.core);
        assert!(!s.view.node(b).state.active);
    }
}
