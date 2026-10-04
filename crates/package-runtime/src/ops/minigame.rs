//! Operations behind the `minigame` capability.
use super::*;

/// Change a column of the reports a game's players are shown, from now
/// on: retitle and fill it, add it, or take it out (`title: None`).
/// Capture the Flag's Flag Pick-ups in place of Slayer's Kills.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportColumn {
    pub game: u64,
    pub change: crate::report::ColumnChange,
}
impl ScriptOp for ReportColumn {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "report_column";
    fn bounded(&self) -> bool {
        let ReportColumn { change, .. } = self;
        change.is_bounded()
    }
}

/// Set a mini-game's teams and team rules, as Slayer's team list does:
/// a team with an `id` keeps it and its members, one without is new,
/// and teams left out are removed (their members are left on none).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetTeams {
    pub game: u64,
    pub teams: Vec<TeamOp>,
    pub friendly_fire: bool,
    pub ally_same_color: bool,
}
impl ScriptOp for SetTeams {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "set_teams";
    fn bounded(&self) -> bool {
        let SetTeams { teams, .. } = self;
        teams.len() <= MAX_TEAMS
            && teams.iter().all(|t| {
                !t.name.trim().is_empty()
                    && t.name.chars().count() <= MAX_TEAM_NAME
                    && !t.name.chars().any(char::is_control)
            })
    }
}

/// Put a member of a mini-game on one of its teams, or on none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetTeam {
    pub player: u64,
    pub team: Option<u64>,
}
impl ScriptOp for SetTeam {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "set_team";
    fn bounded(&self) -> bool {
        true
    }
}

/// Set a player's mini-game score, or with `add` change it by `value`
/// (`incScore`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetScore {
    pub player: u64,
    pub value: i64,
    pub add: bool,
}
impl ScriptOp for SetScore {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "add_score";
    fn bounded(&self) -> bool {
        let SetScore { value, .. } = self;
        value.abs() <= MAX_SCORE
    }
}

/// Set a team's own points, or with `add` change them by `value`
/// (Slayer's `Slayer_TeamSO::incScore`): the points a team scored itself,
/// apart from its members'. The engine keeps them with the game, so a
/// rule's Team Score check, the scripts' `points` and the game's resets
/// all read and clear the same points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetTeamPoints {
    pub game: u64,
    pub team: u64,
    pub value: i64,
    pub add: bool,
}
impl ScriptOp for SetTeamPoints {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "add_team_points";
    fn bounded(&self) -> bool {
        let SetTeamPoints { value, .. } = self;
        value.abs() <= MAX_SCORE
    }
}

/// Reset a mini-game (`MiniGameSO::reset`): every member respawns with
/// a score of 0 and the game's bricks come back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResetMinigame {
    pub game: u64,
}
impl ScriptOp for ResetMinigame {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "reset_minigame";
    fn bounded(&self) -> bool {
        true
    }
}

/// Change how the engine runs a mini-game for these rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetGameRule {
    pub game: u64,
    pub rule: GameRule,
}
impl ScriptOp for SetGameRule {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "set_default_minigame";
    fn bounded(&self) -> bool {
        let SetGameRule { rule, .. } = self;
        match rule {
            GameRule::PaintColor(c) => c.is_none_or(|c| c < 64),
            GameRule::Region(r) => r.is_none_or(|[lo, hi]| {
                (0..3).all(|i| lo[i].is_finite() && hi[i].is_finite() && lo[i] <= hi[i])
            }),
            GameRule::Settings(v) => settings_json_ok(v),
            GameRule::NameDistance(d) => d.is_none_or(|d| d <= 8192),
            _ => true,
        }
    }
}

/// Make a mini-game for these rules, owned by player `owner` or, with
/// none, by the server (one players come to and leave as they like):
/// the defaults with `settings` over them (the Mini-Game window's
/// fields, as `minigame(game).settings` reads them) and paint colour
/// `paint`. Rules hear `on_minigame`'s `created`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateMinigame {
    pub owner: Option<u64>,
    pub settings: serde_json::Value,
    pub paint: Option<u8>,
}
impl ScriptOp for CreateMinigame {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "create_minigame";
    fn bounded(&self) -> bool {
        let CreateMinigame {
            settings, paint, ..
        } = self;
        settings_json_ok(settings) && paint.is_none_or(|c| c < 64)
    }
}

/// Put a snapshot (`minigame_snapshot`) into mini-game `game`: its
/// settings, Add-On settings, teams and per-game state (Slayer's
/// configs and Auto Start).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestoreMinigame {
    pub game: u64,
    pub snapshot: serde_json::Value,
}
impl ScriptOp for RestoreMinigame {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "restore_minigame";
    fn bounded(&self) -> bool {
        let RestoreMinigame { snapshot, .. } = self;
        snapshot.is_object()
            && serde_json::to_vec(snapshot).is_ok_and(|b| b.len() <= MAX_HOST_VALUE)
    }
}

/// Bring back now every knocked-out brick whose builder's bricks are
/// mini-game `game`'s (Slayer's reset: `respawn` on fake-dead bricks).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviveBricks {
    pub game: u64,
}
impl ScriptOp for ReviveBricks {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "revive_bricks";
    fn bounded(&self) -> bool {
        true
    }
}

/// Put a player in a mini-game, or in none, whatever its invitations
/// and join wait (Slayer's `addMember` and `removeMember`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceMember {
    pub player: u64,
    pub game: Option<u64>,
}
impl ScriptOp for PlaceMember {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "place_member";
    fn bounded(&self) -> bool {
        true
    }
}

/// End a mini-game's round (Slayer's `endRound`), won by these teams
/// and players, or by nobody. Every rule hears `on_minigame` with
/// `kind == "round_end"`; the round stays over until a reset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndRound {
    pub game: u64,
    pub teams: Vec<u64>,
    pub players: Vec<u64>,
}
impl ScriptOp for EndRound {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "end_round";
    fn bounded(&self) -> bool {
        let EndRound { teams, players, .. } = self;
        teams.len() <= MAX_TEAMS && players.len() <= MAX_ROUND_WINNERS
    }
}

/// Change an Add-On setting of a mini-game, or of one of its teams
/// (`Slayer_MiniGameSO::setPref`): `key` is the package's own or
/// `namespace:key`; `None` puts it back to its default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetSetting {
    pub game: u64,
    pub team: Option<u64>,
    pub key: String,
    pub value: Option<SettingValue>,
}
impl ScriptOp for SetSetting {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "set_team_setting";
    fn bounded(&self) -> bool {
        let SetSetting { key, value, .. } = self;
        bri_package::setting::is_setting_ref(key)
            && !matches!(value, Some(SettingValue::Text(t)) if t.len() > bri_package::setting::MAX_TEXT)
    }
}

/// How often one of this package's zones (`behaviour.zones`, by index)
/// is checked from now on, 10 to 10000 ms, as a script setting
/// `TriggerData.tickPeriodMS` did (Slayer's capture point Tick Time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetZonePeriod {
    pub zone: u32,
    pub period_ms: u32,
}
impl ScriptOp for SetZonePeriod {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "set_zone_period";
    fn bounded(&self) -> bool {
        let SetZonePeriod { zone, period_ms } = self;
        (*zone as usize) < crate::content::MAX_ZONES && (10..=10_000).contains(period_ms)
    }
}

/// Keep a mini-game member from respawning until their mini-game resets
/// or a rule lets them (Slayer's `setDead`: out of lives, between
/// rounds). The client hides its respawn prompt while held.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoldRespawn {
    pub player: u64,
    pub held: bool,
}
impl ScriptOp for HoldRespawn {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "hold_respawn";
    fn bounded(&self) -> bool {
        true
    }
}

/// How long a mini-game member waits to respawn after dying, in ms,
/// in place of their mini-game's time (`setRespawnTime`; Slayer's team
/// Respawn Time), or `None` for the mini-game's again. Kept until they
/// leave the mini-game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetRespawnTime {
    pub player: u64,
    pub ms: Option<u32>,
}
impl ScriptOp for SetRespawnTime {
    const CAPABILITY: &str = "minigame";
    const NAME: &str = "set_respawn_time";
    fn bounded(&self) -> bool {
        let SetRespawnTime { ms, .. } = self;
        ms.is_none_or(|ms| ms <= MAX_RESPAWN_MS)
    }
}
