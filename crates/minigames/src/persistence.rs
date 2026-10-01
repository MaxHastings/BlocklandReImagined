use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub schema_version: u32,
    pub settings: Settings,
}
impl Preset {
    pub fn new(settings: Settings, catalog: &Catalog) -> Result<Self, Error> {
        settings.validate(catalog)?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            settings,
        })
    }
    pub fn to_json(&self, catalog: &Catalog) -> Result<Vec<u8>, Error> {
        self.settings.validate(catalog)?;
        if self.schema_version != SCHEMA_VERSION {
            return Err(Error::InvalidSnapshot);
        }
        serde_json::to_vec_pretty(self).map_err(|_| Error::InvalidSnapshot)
    }
    pub fn from_json(bytes: &[u8], catalog: &Catalog) -> Result<Self, Error> {
        if bytes.len() > 65536 {
            return Err(Error::Capacity);
        }
        let preset: Self = serde_json::from_slice(bytes).map_err(|_| Error::InvalidSnapshot)?;
        if preset.schema_version != SCHEMA_VERSION {
            return Err(Error::InvalidSnapshot);
        }
        preset.settings.validate(catalog)?;
        Ok(preset)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema_version: u32,
    mode: PolicyMode,
    clear_events: bool,
    tick: u64,
    next_session: u64,
    next_game: u64,
    next_life: u64,
    players: Vec<PlayerState>,
    games: Vec<MiniGame>,
}
impl MinigamesWorld {
    /// A trusted server snapshot, not a network message. Persist alongside host world state.
    pub fn save(&self) -> Result<Vec<u8>, Error> {
        let snapshot = Snapshot {
            schema_version: SCHEMA_VERSION,
            mode: self.mode,
            clear_events: self.clear_events,
            tick: self.tick,
            next_session: self.next_session,
            next_game: self.next_game,
            next_life: self.next_life,
            players: self.players.values().cloned().collect(),
            games: self.games.values().cloned().collect(),
        };
        let bytes = serde_json::to_vec_pretty(&snapshot).map_err(|_| Error::InvalidSnapshot)?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err(Error::Capacity);
        }
        Ok(bytes)
    }
    /// Restore only with the same authenticated connection bindings, or disconnect saved players
    /// before accepting new connections. A client cannot claim a saved PlayerId.
    pub fn restore(bytes: &[u8], catalog: Catalog) -> Result<Self, Error> {
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err(Error::Capacity);
        }
        let s: Snapshot = serde_json::from_slice(bytes).map_err(|_| Error::InvalidSnapshot)?;
        if s.schema_version != SCHEMA_VERSION
            || s.players.len() > MAX_PLAYERS
            || s.games.len() > MAX_GAMES
            || s.next_session == 0
            || s.next_game == 0
            || s.next_life == 0
            || s.next_life > u64::MAX - MAX_PLAYERS as u64 * 3
            || s.tick > u64::MAX - 36001
        {
            return Err(Error::InvalidSnapshot);
        }
        let mut world = Self::new(catalog, s.mode, s.clear_events)?;
        world.tick = s.tick;
        world.next_session = s.next_session;
        world.next_game = s.next_game;
        world.next_life = s.next_life;
        let mut accounts = BTreeSet::new();
        let mut sessions = BTreeSet::new();
        let mut lives = BTreeSet::new();
        for p in s.players {
            let life = match p.life {
                LifeState::Alive { life } | LifeState::Dead { life, .. } => life,
            };
            if p.id.session == 0
                || p.id.session >= s.next_session
                || life.0 == 0
                || life.0 >= s.next_life
                || !accounts.insert(p.id.account)
                || !sessions.insert(p.id.session)
                || !lives.insert(life)
                || p.name.is_empty()
                || p.name.chars().count() > 64
                || p.name.chars().any(char::is_control)
                || p.last_join.is_some_and(|t| t > s.tick)
                || p.ignored_owners.len() > MAX_PLAYERS
                || matches!(p.life, LifeState::Dead { ready_at, .. } if ready_at > s.tick.saturating_add(3601))
            {
                return Err(Error::InvalidSnapshot);
            }
            world.players.insert(p.id, p);
        }
        let mut colors = BTreeSet::new();
        for g in s.games {
            g.settings.validate(&world.catalog)?;
            g.teams.validate()?;
            if g.id.0 == 0
                || g.id.0 >= s.next_game
                || g.color >= 10
                || !colors.insert(g.color)
                || (!g.is_server() && (g.members.is_empty() || !g.members.contains(&g.owner)))
                || (g.is_server() && world.games.values().any(MiniGame::is_server))
                || g.round == 0
                || g.members
                    .iter()
                    .any(|p| world.players.get(p).is_none_or(|p| p.game != Some(g.id)))
                || g.last_reset.is_some_and(|t| t > s.tick)
                || g.ball_update_at
                    .is_some_and(|t| t <= s.tick || t > s.tick.saturating_add(6))
                || world.games.contains_key(&g.id)
            {
                return Err(Error::InvalidSnapshot);
            }
            world.games.insert(g.id, g);
        }
        for p in world.players.values() {
            if p.game.is_some_and(|id| {
                world
                    .games
                    .get(&id)
                    .is_none_or(|g| !g.members.contains(&p.id))
            }) || p.team.is_some_and(|t| {
                p.game
                    .is_none_or(|id| world.games[&id].teams.get(t).is_none())
            }) || p
                .invite
                .is_some_and(|id| !world.games.contains_key(&id) || p.game == Some(id))
            {
                return Err(Error::InvalidSnapshot);
            }
            if let LifeState::Dead { ready_at, .. } = p.life {
                let ms = p
                    .game
                    .map_or(1000, |id| world.games[&id].settings.respawn_ms);
                if ready_at < manual_respawn_ticks(ms) {
                    return Err(Error::InvalidSnapshot);
                }
            }
        }
        Ok(world)
    }
}
