//! Screen-independent view state. UI visibility is never server authorization.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Login,
    Administrator,
    Ban,
    Unban,
    BrickManagement,
    ChangeMap,
    HostConfiguration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Kick,
    Ban,
    Unban,
    Spy,
    DestructoWand,
    ChangeMap,
    ClearBricks,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    pub control: Control,
    pub label: &'static str,
    pub enabled: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerSort {
    Name,
    Connection,
}
#[derive(Clone, Debug)]
pub struct ViewModel {
    pub screen: Screen,
    pub players: Vec<PlayerRow>,
    pub selected: Option<ConnectionId>,
    pub bans: Vec<BanRecord>,
    pub selected_ban: Option<BanId>,
    pub can_administer: bool,
    pub can_set_admin_password: bool,
    pub can_configure_host: bool,
    pub legacy_lan: bool,
    pending: Option<Request>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BanDraft {
    pub days: u8,
    pub hours: u8,
    pub minutes: u8,
    pub forever: bool,
    pub reason: String,
}
impl BanDraft {
    /// The original dialog offers 0..15 days, 0..23 hours, 0..59 minutes.
    pub fn duration(&self) -> Result<BanDuration, Error> {
        if self.days > 15 || self.hours > 23 || self.minutes > 59 {
            return Err(Error::InvalidTime);
        }
        validate_text(&self.reason, 512)?;
        if self.forever {
            return Ok(BanDuration::Forever);
        }
        let m = u32::from(self.days) * 1440 + u32::from(self.hours) * 60 + u32::from(self.minutes);
        if m == 0 {
            return Err(Error::InvalidTime);
        }
        Ok(BanDuration::Minutes(m))
    }
}
impl ViewModel {
    pub fn from_server(
        server: &Administration,
        origin: Origin,
        legacy_lan: bool,
    ) -> Result<Self, Error> {
        server.actor(origin)?;
        let can_administer = server.permission(origin, &Action::RequestBanList).is_ok();
        let can_set_admin_password = server
            .permission(
                origin,
                &Action::SetAdminPassword {
                    password: Secret::new(String::new())?,
                },
            )
            .is_ok();
        Ok(Self {
            screen: if can_administer {
                Screen::Administrator
            } else {
                Screen::Login
            },
            players: server.rows(),
            selected: None,
            bans: Vec::new(),
            selected_ban: None,
            can_administer,
            can_set_admin_password,
            can_configure_host: server.host_authority(origin)?,
            legacy_lan,
            pending: None,
        })
    }
    pub fn buttons(&self) -> Vec<Button> {
        let selected = self
            .selected
            .and_then(|id| self.players.iter().find(|p| p.connection == id));
        let kick = selected.is_some_and(|p| {
            p.is_bot || (!p.is_owner && !p.is_local && p.role != Role::SuperAdmin)
        });
        let ban = selected.is_some_and(|p| {
            p.durable_identity_available && !p.is_owner && !p.is_local && p.role != Role::SuperAdmin
        });
        [
            (Control::Kick, "Kick", kick),
            (Control::Ban, "Ban >>", ban && !self.legacy_lan),
            (Control::Unban, "Un-Ban >>", !self.legacy_lan),
            (Control::Spy, "Spy", selected.is_some()),
            (Control::DestructoWand, "Destructo Wand", true),
            (Control::ChangeMap, "Change Map >>", true),
            (Control::ClearBricks, "Clear Bricks >>", true),
        ]
        .into_iter()
        .map(|(control, label, enabled)| Button {
            control,
            label,
            enabled: enabled && self.can_administer,
        })
        .collect()
    }
    pub fn select_player(&mut self, id: ConnectionId) -> Result<(), Error> {
        if !self.players.iter().any(|p| p.connection == id) {
            return Err(Error::UnknownConnection);
        }
        self.selected = Some(id);
        self.pending = None;
        Ok(())
    }
    pub fn sort_players(&mut self, key: PlayerSort, ascending: bool) {
        self.players.sort_by(|a, b| match key {
            PlayerSort::Name => a
                .display_name
                .to_lowercase()
                .cmp(&b.display_name.to_lowercase())
                .then(a.connection.cmp(&b.connection)),
            PlayerSort::Connection => a.connection.cmp(&b.connection),
        });
        if !ascending {
            self.players.reverse();
        }
    }
    pub fn apply_bans(&mut self, bans: Vec<BanRecord>) -> Result<(), Error> {
        if bans.len() > MAX_BANS {
            return Err(Error::Budget);
        }
        let max = bans.iter().map(|b| b.id.0).max().unwrap_or(0);
        DurableState {
            schema_version: 1,
            next_ban_id: max.checked_add(1).ok_or(Error::Budget)?,
            bans: bans.clone(),
            auto_roles: Vec::new(),
        }
        .validate()?;
        self.bans = bans;
        if self
            .selected_ban
            .is_some_and(|id| !self.bans.iter().any(|b| b.id == id))
        {
            self.selected_ban = None;
        }
        self.pending = None;
        Ok(())
    }
    /// Confirmation is native data, never an eval-able command string.
    pub fn request_kick_confirmation(&mut self) -> Result<String, Error> {
        if !self
            .buttons()
            .iter()
            .any(|b| b.control == Control::Kick && b.enabled)
        {
            return Err(Error::Denied);
        }
        let id = self.selected.ok_or(Error::UnknownConnection)?;
        let name = &self
            .players
            .iter()
            .find(|p| p.connection == id)
            .ok_or(Error::UnknownConnection)?
            .display_name;
        self.pending = Some(Request::new(Action::Kick { target: id }));
        Ok(format!("Kick {name}?"))
    }
    pub fn request_unban_confirmation(&mut self, id: BanId) -> Result<String, Error> {
        if !self.can_administer || self.legacy_lan {
            return Err(Error::Denied);
        }
        let ban = self
            .bans
            .iter()
            .find(|b| b.id == id)
            .ok_or(Error::UnknownBan)?;
        self.selected_ban = Some(id);
        self.pending = Some(Request::new(Action::Unban { ban: id }));
        Ok(format!("Un-ban {}?", ban.victim_name))
    }
    pub fn confirm(&mut self, yes: bool) -> Option<Request> {
        let request = self.pending.take();
        if yes { request } else { None }
    }
    pub fn ban_request(&self, draft: &BanDraft) -> Result<Request, Error> {
        if !self
            .buttons()
            .iter()
            .any(|b| b.control == Control::Ban && b.enabled)
        {
            return Err(Error::Denied);
        }
        Ok(Request::new(Action::Ban {
            target: self.selected.ok_or(Error::UnknownConnection)?,
            duration: draft.duration()?,
            reason: draft.reason.clone(),
        }))
    }
    /// The caller clears its edit box on both wake and sleep, as the stock dialog does.
    pub fn login_request(edit_box: &mut String) -> Result<Request, Error> {
        Ok(Request::new(Action::Login {
            password: Secret::new(std::mem::take(edit_box))?,
        }))
    }
    pub fn open_clear_bricks(&mut self) -> Result<Option<String>, Error> {
        if !self.can_administer {
            return Err(Error::Denied);
        }
        if self.legacy_lan {
            self.pending = Some(Request::new(Action::ClearAllBricks));
            Ok(Some("Delete all bricks?".into()))
        } else {
            self.screen = Screen::BrickManagement;
            Ok(None)
        }
    }
    pub fn request_clear_group(&mut self, group: u64, display_name: &str) -> Result<String, Error> {
        if !self.can_administer {
            return Err(Error::Denied);
        }
        validate_text(display_name, 128)?;
        self.pending = Some(Request::new(Action::ClearBrickGroup { group }));
        Ok(format!("Destroy all of {display_name}'s bricks?"))
    }
}
