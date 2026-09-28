//! Live player list; identity/trust are host-provided data, never authority.
use super::*;
use crate::api::UiAction;
use crate::view::EventKind;

pub struct Players {
    view: View,
    ids: Vec<u64>,
    sort: usize,
    descending: bool,
    /// `showTrustMessage`: the "trust invite sent" window, in ms left.
    trust_message_ms: u64,
}
impl Players {
    pub fn new(core: &Core) -> Self {
        let mut s = Self {
            view: layout_view(core, "NewPlayerListGui"),
            ids: vec![],
            sort: 1,
            descending: false,
            trust_message_ms: 0,
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
        s.refresh(core);
        s
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
            self.view.state(n).items = rows
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let member = core.minigames.members.iter().find(|m| m.id.0 == p.id);
                    let same_game = member.is_some_and(|m| m.in_local_game);
                    (
                        format!(
                            "{}{}\t{}\t{}\t{}\t{}",
                            if same_game { "\u{e005}" } else { "" },
                            if p.super_admin {
                                "S"
                            } else if p.admin {
                                "A"
                            } else {
                                ""
                            },
                            p.name.replace(['\t', '\n', '\r'], " "),
                            member.filter(|m| m.in_local_game).map_or(i64::from(p.score), |m| m.score),
                            p.bl_id.map(|id| id.to_string()).unwrap_or_default(),
                            p.trust.replace(['\t', '\n', '\r'], " ")
                        ),
                        i as i64,
                    )
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
        let selected_id = self.view.id("NPL_List").and_then(|n| self.view.selected(n))
            .and_then(|i| self.ids.get(i as usize)).copied();
        let selected_member = selected_id.and_then(|id| core.minigames.members.iter().find(|m| m.id.0 == id));
        let invite = core.minigames.can(crate::models::minigames::Operation::Invite)
            && selected_id.is_some() && selected_id != core.minigames.local_player.map(|p| p.0)
            && selected_member.is_none_or(|m| !m.in_local_game);
        let remove = core.minigames.can(crate::models::minigames::Operation::RemoveMember)
            && selected_member.is_some_and(|m| m.in_local_game && !m.is_owner)
            && selected_id != core.minigames.local_player.map(|p| p.0);
        for (command, active) in [
            ("NewPlayerListGui.clickMiniGameInvite();", invite),
            ("NewPlayerListGui.clickMiniGameRemove();", remove),
        ] {
            if let Some(n) = self.view.by_command(command) { self.view.set_active(n, active); }
        }
        if let Some(n) = self.view.id("NPL_MiniGameInviteBlocker") { self.view.set_visible(n, !invite); }
        if let Some(n) = self.view.id("NPL_MiniGameRemoveBlocker") { self.view.set_visible(n, !remove); }
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
            ("NewPlayerListGui.clickTrustInviteBuild();", "NPL_TrustInviteBuildBlocker", invite_build),
            ("NewPlayerListGui.clickTrustInviteFull();", "NPL_TrustInviteFullBlocker", invite_full),
            ("NewPlayerListGui.ClickTrustDemoteNONE();", "NPL_TrustRemoveBuildBlocker", remove_build),
            ("NewPlayerListGui.ClickTrustDemoteBUILD();", "NPL_TrustRemoveFullBlocker", remove_full),
            ("NewPlayerListGui.clickUnIgnore();", "NPL_UnIgnoreBlocker", unignore),
        ] {
            if let Some(n) = self.view.by_command(command) { self.view.set_active(n, active); }
            if let Some(n) = self.view.id(blocker) { self.view.set_visible(n, !active); }
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
    fn on_result(&mut self, _id: RequestId, kind: Option<&Pending>, result: &Result<(), String>, core: &mut Core) -> bool {
        if !matches!(kind, Some(Pending::MiniGame(_))) { return false; }
        core.minigames.status = result.as_ref().map_or_else(|e| e.clone(), |_| "Mini-game request completed.".into());
        self.refresh(core);
        true
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
            && self.view.node(ev.node).ctrl.name.as_deref() == Some("NPL_List") {
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
            "newplayerlistgui.clicktrustinvitebuild();" | "newplayerlistgui.clicktrustinvitefull();" => {
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
            "newplayerlistgui.clicktrustdemotenone();" | "newplayerlistgui.clicktrustdemotebuild();" => {
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
            "newplayerlistgui.clickminigameinvite();" => {
                if core.minigames.can(crate::models::minigames::Operation::Invite)
                    && let Some(id) = self.view.id("NPL_List").and_then(|n| self.view.selected(n))
                    .and_then(|i| self.ids.get(i as usize)).copied() {
                    core.minigame_request(MiniGameOperation::Invite, UiAction::InviteMiniGame { target: MiniGamePlayerId(id) });
                }
            }
            "newplayerlistgui.clickminigameremove();" => {
                if core.minigames.can(crate::models::minigames::Operation::RemoveMember)
                    && let Some(id) = self.view.id("NPL_List").and_then(|n| self.view.selected(n))
                    .and_then(|i| self.ids.get(i as usize)).copied() {
                    core.minigame_request(MiniGameOperation::RemoveMember, UiAction::RemoveMiniGameMember { target: MiniGamePlayerId(id) });
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
            },
        ];
        let mut s = Players::new(&ui.core);
        let n = s.view.id("NPL_List").unwrap();
        assert_eq!(s.ids, vec![2, u64::MAX]);
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
}
