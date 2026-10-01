//! Session-fed vanilla minigame list, rules and membership presentation.
//! The host supplies authority capabilities and stable session selectors.
use crate::api::{MiniGameCapabilities, MiniGameId, MiniGameRules, MiniGameUiState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation { List, Create, Configure, Join, Leave, Invite, AcceptInvite, RejectInvite, IgnoreInvite, RemoveMember, Reset, RespawnAll, End, AddOnSettings }

impl MiniGameUiState {
    /// Whether `op` is offered on the local player's own game.
    pub fn can(&self, op: Operation) -> bool { self.can_on(op, self.active_game) }
    /// Whether `op` is offered on `game` (a managing operation's target).
    pub fn can_on(&self, op: Operation, game: Option<MiniGameId>) -> bool {
        if !self.ready { return false; }
        let c: MiniGameCapabilities = self.capabilities;
        let manage = game.is_some_and(|g| self.can_manage(g));
        match op {
            Operation::List => c.list,
            Operation::Create => c.create,
            Operation::Configure => c.configure && manage,
            Operation::Join => c.join,
            Operation::Leave => c.leave && self.active_game.is_some() && !self.owns_active_game,
            Operation::Invite => c.invite && manage,
            Operation::AcceptInvite | Operation::RejectInvite | Operation::IgnoreInvite => c.respond_invite,
            Operation::RemoveMember => c.remove_member && manage,
            Operation::Reset => c.reset && manage,
            Operation::RespawnAll => c.respawn_all && manage,
            Operation::End => c.end && manage,
            // The host names the games this player may edit; it checks again.
            Operation::AddOnSettings => !self.addon_settings.is_empty() && !self.addon_editable.is_empty(),
        }
    }
    /// Whether the local player may manage `game` (edit its rules, reset,
    /// end, invite to and remove from it): its owner, or a player the host
    /// names as its editor (an admin). The host checks again; this only
    /// decides what the windows offer.
    pub fn can_manage(&self, game: MiniGameId) -> bool {
        self.ready
            && ((self.owns_active_game && self.active_game == Some(game))
                || self.local_player.is_some_and(|me| self.games.iter().any(|g| g.id == game && g.owner == me))
                || self.addon_editable.contains(&game))
    }
    pub fn rules_draft(&self) -> MiniGameRules {
        self.active_game.and_then(|id| self.games.iter().find(|g| g.id == id))
            .map(|g| g.rules.clone()).unwrap_or_default()
    }
    /// Retain a selected target only while its session identity remains present.
    pub fn retain_player_target(&self, selected: Option<crate::api::MiniGamePlayerId>) -> Option<crate::api::MiniGamePlayerId> {
        selected.filter(|id| self.members.iter().any(|m| m.id == *id))
    }
    pub fn retain_game_target(&self, selected: Option<MiniGameId>) -> Option<MiniGameId> {
        selected.filter(|id| self.games.iter().any(|g| g.id == *id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::*;
    #[test]
    fn default_capabilities_fail_closed_and_admin_is_not_ownership() {
        let mut state=MiniGameUiState{ready:true,..Default::default()};
        assert!(!state.can(Operation::Create));
        state.capabilities.create=true;
        assert!(state.can(Operation::Create));
        state.capabilities.configure=true;
        assert!(!state.can(Operation::Configure));
        state.owns_active_game=true;
        state.active_game=Some(MiniGameId(3));
        assert!(state.can(Operation::Configure));
        assert_eq!(MiniGameRules::default().respawn_seconds,1);
        assert_eq!(MiniGameRules::default().loadout[0].as_deref(),Some("v20.weapon.hammeritem"));
    }
    #[test]
    fn an_editor_the_host_names_manages_a_game_they_do_not_own() {
        let mut state=MiniGameUiState{ready:true,..Default::default()};
        state.capabilities=MiniGameCapabilities{configure:true,reset:true,end:true,invite:true,remove_member:true,..Default::default()};
        state.local_player=Some(MiniGamePlayerId(5));
        state.active_game=Some(MiniGameId(7));
        state.games.push(MiniGameSummary{default:false,paint_color:None,members:vec![],id:MiniGameId(7),title:"Round".into(),owner:MiniGamePlayerId(90),owner_name:"Host".into(),color:0,member_count:2,invite_only:false,rules:MiniGameRules::default(),teams:vec![],addon_settings:Default::default()});
        for op in [Operation::Configure,Operation::Reset,Operation::End,Operation::Invite,Operation::RemoveMember] {
            assert!(!state.can(op), "{op:?} offered to a plain member");
        }
        assert!(!state.can_manage(MiniGameId(7)));
        // An admin: the host names the game as theirs to edit.
        state.addon_editable=vec![MiniGameId(7)];
        assert!(state.can_manage(MiniGameId(7)));
        for op in [Operation::Configure,Operation::Reset,Operation::End,Operation::Invite,Operation::RemoveMember] {
            assert!(state.can(op), "{op:?} refused to the game's editor");
            assert!(!state.can_on(op,Some(MiniGameId(8))), "{op:?} offered on a game they may not edit");
        }
        // Not the owner: leaving is still theirs, ownership is not claimed.
        assert!(!state.owns_active_game);
    }
    #[test]
    fn selected_game_and_player_targets_are_retained_only_while_live(){
        let mut state=MiniGameUiState::default();
        state.games.push(MiniGameSummary{default:false,paint_color:None,members:vec![],id:MiniGameId(7),title:"Round".into(),owner:MiniGamePlayerId(90),owner_name:"Host".into(),color:0,member_count:1,invite_only:false,rules:MiniGameRules::default(),teams:vec![],addon_settings:Default::default()});
        state.members.push(MiniGameMemberRow{id:MiniGamePlayerId(90),name:"Host".into(),score:0,is_owner:true,admin:false,in_local_game:true});
        assert_eq!(state.retain_game_target(Some(MiniGameId(7))),Some(MiniGameId(7)));
        assert_eq!(state.retain_player_target(Some(MiniGamePlayerId(90))),Some(MiniGamePlayerId(90)));
        state.games.clear(); state.members.clear();
        assert_eq!(state.retain_game_target(Some(MiniGameId(7))),None);
        assert_eq!(state.retain_player_target(Some(MiniGamePlayerId(90))),None);
    }
}
