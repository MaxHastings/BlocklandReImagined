//! `TrustInviteGui`: accept, reject or ignore a trust invitation.
use super::*;
use crate::api::{TrustAnswer, UiAction};
use crate::ui::Callback;
use crate::view::EventKind;

pub struct TrustInvite {
    view: View,
}
impl TrustInvite {
    pub fn new(core: &Core) -> Self {
        let mut s = Self {
            view: layout_view(core, "TrustInviteGui"),
        };
        s.refresh(core);
        s
    }
    fn refresh(&mut self, core: &Core) {
        let Some(invite) = core.trust_invites.last() else {
            return;
        };
        let build = invite.level == 1;
        for (name, text) in [
            ("TI_Name", invite.name.as_str()),
            ("TI_BL_ID", invite.bl_id.as_str()),
        ] {
            if let Some(n) = self.view.id(name) {
                self.view.set_text(n, text);
            }
        }
        for (name, shown) in [
            ("TI_BuildMessageA", build),
            ("TI_BuildMessageB", build),
            ("TI_FullMessageA", !build),
            ("TI_FullMessageB", !build),
        ] {
            if let Some(n) = self.view.id(name) {
                self.view.set_visible(n, shown);
            }
        }
    }
}
/// Answer one invitation; the dialog closes once none are left, and shows
/// the next one otherwise.
pub fn answer(core: &mut Core, from: u64, answer: TrustAnswer) {
    if core.trust_invites.iter().any(|i| i.from == from) {
        core.trust_invites.retain(|i| i.from != from);
        core.request(UiAction::AnswerTrustInvite { from, answer });
    }
    core.pop(ScreenId::TrustInvitation);
    if !core.trust_invites.is_empty() {
        core.push(ScreenId::TrustInvitation);
    }
}
impl Screen for TrustInvite {
    fn id(&self) -> ScreenId {
        ScreenId::TrustInvitation
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_update(&mut self, core: &mut Core) {
        self.refresh(core);
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
        if ev.kind != EventKind::Click || !self.view.node(ev.node).state.active {
            return;
        }
        let Some(from) = core.trust_invites.last().map(|i| i.from) else {
            core.pop(self.id());
            return;
        };
        match command_of(&self.view, ev.node)
            .to_ascii_lowercase()
            .as_str()
        {
            "trustinvitegui.clickaccept();" => answer(core, from, TrustAnswer::Accept),
            "trustinvitegui.clickreject();" => answer(core, from, TrustAnswer::Reject),
            "trustinvitegui.clickignore();" => core.message_yes_no(
                "Ignore User?",
                "Ignore all future trust invites from this person?",
                Callback::IgnoreTrust { from },
            ),
            _ => {}
        }
    }
}
