//! Replicated minigame state → the original Mini-Games dialogs, and dialog
//! actions → authoritative minigame requests.
use anyhow::{Result, ensure};
use bri_minigames::Settings;
use bri_package::setting::{SettingEditor, SettingScope, SettingType, SettingValue, ShownWhen};
use bri_sim::session::{
    AddOnSetting, Command, MiniGameRequest, MiniGameView, SettingEdit, TeamEdit, Vitals,
};
use bri_ui::api::*;
use bri_world::OwnerId;
use std::collections::BTreeMap;

/// `$MiniGameColorName[0..9]` with `$MiniGameColor` values.
const COLORS: [(&str, [u8; 3]); 10] = [
    ("Red", [255, 0, 0]),
    ("Orange", [255, 128, 0]),
    ("Yellow", [255, 255, 0]),
    ("Green", [0, 255, 0]),
    ("Dark Green", [0, 128, 0]),
    ("Cyan", [0, 255, 255]),
    ("Dark Cyan", [0, 128, 128]),
    ("Blue", [0, 128, 255]),
    ("Pink", [255, 128, 255]),
    ("Black", [0, 0, 0]),
];

/// `$MiniGameColorI[index]`, the colour a member's name takes.
pub fn color_rgb(index: u8) -> Option<[u8; 3]> {
    COLORS.get(usize::from(index)).map(|(_, rgb)| *rgb)
}

pub fn settings(rules: &MiniGameRules) -> Result<Settings> {
    ensure!(
        (1..=30).contains(&rules.respawn_seconds)
            && rules.vehicle_respawn_seconds <= 300
            && (2..=300).contains(&rules.brick_respawn_seconds),
        "Invalid mini-game timers"
    );
    Ok(Settings {
        title: rules.title.clone(),
        invite_only: rules.invite_only,
        use_all_players_bricks: rules.use_all_players_bricks,
        players_use_own_bricks: rules.players_use_own_bricks,
        use_spawn_bricks: rules.use_spawn_bricks,
        points_break_brick: rules.points_break_brick,
        points_plant_brick: rules.points_plant_brick,
        points_kill_player: rules.points_kill_player,
        points_kill_self: rules.points_kill_self,
        points_die: rules.points_die,
        respawn_ms: rules.respawn_seconds * 1000,
        vehicle_respawn_ms: rules.vehicle_respawn_seconds * 1000,
        brick_respawn_ms: rules.brick_respawn_seconds * 1000,
        falling_damage: rules.falling_damage,
        weapon_damage: rules.weapon_damage,
        self_damage: rules.self_damage,
        vehicle_damage: rules.vehicle_damage,
        brick_damage: rules.brick_damage,
        enable_wand: rules.enable_wand,
        enable_building: rules.enable_building,
        enable_painting: rules.enable_painting,
        player_type: rules.player_type.clone(),
        loadout: rules.loadout.clone(),
        lives: bri_minigames::Lives::Unlimited,
    })
}

fn rules(settings: &Settings) -> MiniGameRules {
    MiniGameRules {
        title: settings.title.clone(),
        invite_only: settings.invite_only,
        use_all_players_bricks: settings.use_all_players_bricks,
        players_use_own_bricks: settings.players_use_own_bricks,
        use_spawn_bricks: settings.use_spawn_bricks,
        points_break_brick: settings.points_break_brick,
        points_plant_brick: settings.points_plant_brick,
        points_kill_player: settings.points_kill_player,
        points_kill_self: settings.points_kill_self,
        points_die: settings.points_die,
        respawn_seconds: settings.respawn_ms / 1000,
        vehicle_respawn_seconds: settings.vehicle_respawn_ms / 1000,
        brick_respawn_seconds: settings.brick_respawn_ms / 1000,
        falling_damage: settings.falling_damage,
        weapon_damage: settings.weapon_damage,
        self_damage: settings.self_damage,
        vehicle_damage: settings.vehicle_damage,
        brick_damage: settings.brick_damage,
        enable_wand: settings.enable_wand,
        enable_building: settings.enable_building,
        enable_painting: settings.enable_painting,
        player_type: settings.player_type.clone(),
        loadout: settings.loadout.clone(),
    }
}

/// Build the dialog state for `local` from replicated listings.
pub fn state(
    local: OwnerId,
    games: &[MiniGameView],
    vitals: &BTreeMap<OwnerId, Vitals>,
    names: &BTreeMap<OwnerId, String>,
    items: &[(String, String)],
    archetypes: &bri_sim::archetype::Archetypes,
    revision: u64,
) -> MiniGameUiState {
    // Owner 0 is the server: a game mode's own mini-game.
    let name = |owner: &OwnerId| match owner {
        0 => "Server".to_string(),
        _ => names.get(owner).cloned().unwrap_or_default(),
    };
    let mine = vitals.get(&local);
    let active = mine.and_then(|v| v.minigame);
    let active_view = active.and_then(|id| games.iter().find(|g| g.id == id));
    let used: Vec<u8> = games.iter().map(|g| g.color).collect();
    let server_game = games.iter().any(|g| g.owner == 0);
    let invitations = mine
        .and_then(|v| v.invite)
        .and_then(|id| games.iter().find(|g| g.id == id))
        .map(|g| MiniGameInvitation {
            game: MiniGameId(g.id),
            title: g.settings.title.clone(),
            owner: MiniGamePlayerId(g.owner),
            owner_name: name(&g.owner),
            owner_display_id: g.owner.to_string(),
        })
        .into_iter()
        .collect();
    MiniGameUiState {
        ready: true,
        revision,
        // A game mode's mini-game is the only one: nobody starts, joins or
        // leaves another while it runs.
        capabilities: MiniGameCapabilities {
            list: true,
            create: !server_game,
            configure: true,
            join: !server_game,
            leave: !server_game,
            invite: true,
            respond_invite: true,
            remove_member: true,
            reset: true,
            respawn_all: true,
            end: true,
            scoreboard: true,
        },
        games: games
            .iter()
            .map(|g| MiniGameSummary {
                id: MiniGameId(g.id),
                title: g.settings.title.clone(),
                owner: MiniGamePlayerId(g.owner),
                owner_name: name(&g.owner),
                color: g.color,
                member_count: g.members.len() as u32,
                invite_only: g.settings.invite_only,
                rules: rules(&g.settings),
                teams: g
                    .teams
                    .iter()
                    .map(|t| MiniGameTeam {
                        id: t.id.0,
                        name: t.name.clone(),
                        color: t.color,
                        settings: t
                            .addon_settings
                            .iter()
                            .map(|(k, v)| (k.clone(), ui_value(v)))
                            .collect(),
                    })
                    .collect(),
                addon_settings: g
                    .addon_settings
                    .iter()
                    .map(|(k, v)| (k.clone(), ui_value(v)))
                    .collect(),
                default: g.default,
                paint_color: g.paint_color,
                members: g
                    .members
                    .iter()
                    .map(|m| MiniGameTeamMember {
                        id: MiniGamePlayerId(*m),
                        name: name(m),
                        team: vitals.get(m).and_then(|v| v.team),
                    })
                    .collect(),
            })
            .collect(),
        colors: COLORS
            .iter()
            .enumerate()
            .filter(|(i, _)| !used.contains(&(*i as u8)))
            .map(|(i, (name, rgb))| MiniGameColor {
                index: i as u8,
                name: (*name).into(),
                rgb: *rgb,
            })
            .collect(),
        active_game: active.map(MiniGameId),
        owns_active_game: active_view.is_some_and(|g| g.owner == local),
        local_player: Some(MiniGamePlayerId(local)),
        members: names
            .keys()
            .map(|owner| MiniGameMemberRow {
                id: MiniGamePlayerId(*owner),
                name: name(owner),
                score: vitals.get(owner).map_or(0, |v| v.score),
                is_owner: active_view.is_some_and(|g| g.owner == *owner),
                admin: false,
                in_local_game: active_view.is_some_and(|g| g.members.contains(owner)),
            })
            .collect(),
        invitations,
        player_types: bri_sim::player_types::PlayerType::ALL
            .map(|t| MiniGameChoice {
                id: t.id().into(),
                name: t.name().into(),
            })
            .into_iter()
            .chain(
                archetypes
                    .iter()
                    .skip(bri_sim::player_types::PlayerType::EVERY.len())
                    .filter(|(_, a)| !a.name.is_empty())
                    .map(|(_, a)| MiniGameChoice {
                        id: a.id.clone(),
                        name: a.name.clone(),
                    }),
            )
            .collect(),
        items: items
            .iter()
            .map(|(id, name)| MiniGameChoice {
                id: id.clone(),
                name: name.clone(),
            })
            .collect(),
        status: String::new(),
        addon_settings: Vec::new(),
        addon_editable: Vec::new(),
        palette: Vec::new(),
        addon_locked: Vec::new(),
        teams_shown_when: None,
    }
}

/// What the local player is on the server, to tell which Add-On settings
/// they may change (the host checks again).
#[derive(Debug, Clone, Default)]
pub struct Rank {
    pub admin: bool,
    pub super_admin: bool,
    pub host: bool,
    /// Their trust with each player (3: themselves).
    pub trust: BTreeMap<OwnerId, u8>,
}
impl Rank {
    fn allows(&self, editor: SettingEditor, creator: Option<OwnerId>) -> bool {
        let trust = creator
            .and_then(|c| self.trust.get(&c))
            .copied()
            .unwrap_or(0);
        match editor {
            SettingEditor::Owner => true,
            SettingEditor::Admin => self.admin || self.super_admin || self.host,
            SettingEditor::SuperAdmin => self.super_admin || self.host,
            SettingEditor::Host => self.host,
            SettingEditor::Creator => self.host || trust >= 3,
            SettingEditor::FullTrust => self.host || trust >= 2,
            SettingEditor::BuildTrust => self.host || trust >= 1,
        }
    }
}

pub(crate) fn ui_value(v: &SettingValue) -> MiniGameSettingValue {
    match v {
        SettingValue::Bool(b) => MiniGameSettingValue::Bool(*b),
        SettingValue::Int(n) => MiniGameSettingValue::Int(*n),
        SettingValue::Text(t) => MiniGameSettingValue::Text(t.clone()),
    }
}
fn ui_shown_when(setting: String, w: &ShownWhen) -> MiniGameShownWhen {
    MiniGameShownWhen {
        setting,
        is: w.is.iter().map(ui_value).collect(),
        is_not: w.is_not.iter().map(ui_value).collect(),
    }
}
pub(crate) fn host_value(v: &MiniGameSettingValue) -> SettingValue {
    match v {
        MiniGameSettingValue::Bool(b) => SettingValue::Bool(*b),
        MiniGameSettingValue::Int(n) => SettingValue::Int(*n),
        MiniGameSettingValue::Text(t) => SettingValue::Text(t.clone()),
    }
}
fn choice(c: &MiniGameChoice) -> (MiniGameSettingValue, String) {
    (MiniGameSettingValue::Text(c.id.clone()), c.name.clone())
}
fn edits(list: &[(String, Option<MiniGameSettingValue>)]) -> Vec<SettingEdit> {
    list.iter()
        .map(|(key, value)| SettingEdit {
            key: key.clone(),
            value: value.as_ref().map(host_value),
        })
        .collect()
}

/// Add the running Add-Ons' settings to the dialog state: what each is,
/// which games `local` may change (theirs, or any as an admin; the host
/// checks again) and the paint colours teams take.
pub fn with_addon_settings(
    mut state: MiniGameUiState,
    games: &[MiniGameView],
    settings: &[AddOnSetting],
    local: OwnerId,
    rank: &Rank,
    teams_shown_when: Option<&ShownWhen>,
    palette: &[[f32; 4]],
) -> MiniGameUiState {
    let admin = rank.admin || rank.super_admin || rank.host;
    state.addon_settings = settings
        .iter()
        .map(|s| MiniGameAddOnSetting {
            key: s.key(),
            add_on: s.package_name.clone(),
            category: s.def.category.clone(),
            title: s.def.title.clone(),
            team: s.def.scope == SettingScope::Team,
            server: s.def.scope == SettingScope::Server,
            restart: s.def.restart,
            kind: match s.def.kind {
                SettingType::Bool => MiniGameSettingKind::Bool,
                SettingType::Int => MiniGameSettingKind::Int {
                    min: s.def.min.unwrap_or(0),
                    max: s.def.max.unwrap_or(0),
                },
                SettingType::List => MiniGameSettingKind::List {
                    items: s
                        .items
                        .iter()
                        .map(|i| (ui_value(&i.value), i.name.clone()))
                        .collect(),
                },
                SettingType::Text => MiniGameSettingKind::Text {
                    max_length: s.def.max_length.unwrap_or(0),
                },
                // The server's items and player types, as the mini-game's
                // own loadout offers them.
                SettingType::Item => MiniGameSettingKind::List {
                    items: std::iter::once((
                        MiniGameSettingValue::Text(String::new()),
                        "NONE".into(),
                    ))
                    .chain(state.items.iter().map(choice))
                    .collect(),
                },
                SettingType::PlayerType => MiniGameSettingKind::List {
                    items: state.player_types.iter().map(choice).collect(),
                },
                SettingType::PaintColor => MiniGameSettingKind::PaintColor {
                    min: s.def.min.unwrap_or(0),
                    max: s.def.max.unwrap_or(63),
                },
            },
            default: ui_value(&s.def.default),
            help: s.def.help.clone(),
            avatar: s.def.avatar.clone(),
            shown_when: s.def.shown_when.as_ref().map(|w| {
                // The host names a same-Add-On setting by its bare key.
                let key = if w.setting.contains(':') {
                    w.setting.clone()
                } else {
                    format!("{}:{}", s.package, w.setting)
                };
                ui_shown_when(key, w)
            }),
        })
        .collect();
    // Named whether or not Add-Ons have settings: the rules, Reset, End,
    // invites and removals of these games are the player's to manage.
    state.addon_editable = games
        .iter()
        .filter(|g| admin || g.owner == local)
        .map(|g| MiniGameId(g.id))
        .collect();
    // A shared or game mode's game is the host's own.
    state.addon_locked = games
        .iter()
        .filter(|g| state.addon_editable.contains(&MiniGameId(g.id)))
        .map(|g| {
            let creator = (g.owner != 0 && !g.shared).then_some(g.owner);
            let keys: Vec<String> = settings
                .iter()
                .filter(|s| !rank.allows(s.def.editor, creator))
                .map(AddOnSetting::key)
                .collect();
            (MiniGameId(g.id), keys)
        })
        .filter(|(_, keys)| !keys.is_empty())
        .collect();
    state.teams_shown_when = teams_shown_when.map(|w| ui_shown_when(w.setting.clone(), w));
    state.palette = palette
        .iter()
        .map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
        .map(|[r, g, b, _]| [r, g, b])
        .collect();
    state
}

/// Map a dialog request to a server command. `None` for local-only actions.
/// `own` is the local player's game and `owner` whether they own it: a
/// request about another game (its editor's, from the Mini-Game list), or
/// about their own game by a player who does not own it (an admin), names
/// that game, and the host decides whether they may.
pub fn command(action: &UiAction, own: Option<u64>, owner: bool) -> Result<Option<Command>> {
    let manage = |game: &MiniGameId, request: MiniGameRequest| {
        if Some(game.0) == own && owner {
            request
        } else {
            MiniGameRequest::Manage {
                game: game.0,
                request: Box::new(request),
            }
        }
    };
    let request = match action {
        UiAction::RequestMiniGameList => return Ok(None),
        UiAction::CreateMiniGame { color, rules } => MiniGameRequest::Create {
            color: *color,
            settings: settings(rules)?,
        },
        UiAction::ConfigureMiniGame { game, rules } => manage(
            game,
            MiniGameRequest::Configure {
                settings: settings(rules)?,
            },
        ),
        UiAction::JoinMiniGame { game } => MiniGameRequest::Join { game: game.0 },
        UiAction::LeaveMiniGame { .. } => MiniGameRequest::Leave,
        UiAction::InviteMiniGame { target } => {
            let invite = MiniGameRequest::Invite { target: target.0 };
            match own {
                Some(game) => manage(&MiniGameId(game), invite),
                None => invite,
            }
        }
        UiAction::AcceptMiniGameInvite { game } => MiniGameRequest::Accept { game: game.0 },
        UiAction::RejectMiniGameInvite { game, ignore_owner } => MiniGameRequest::Reject {
            game: game.0,
            ignore_owner: *ignore_owner,
        },
        UiAction::RemoveMiniGameMember { target, game } => {
            let kick = MiniGameRequest::Kick { target: target.0 };
            match game {
                Some(game) => manage(game, kick),
                None => kick,
            }
        }
        UiAction::SetMiniGameTeam { game, target, team } => MiniGameRequest::SetTeam {
            game: game.0,
            target: target.0,
            team: *team,
        },
        UiAction::ResetMiniGame { game } => manage(game, MiniGameRequest::Reset),
        UiAction::RespawnMiniGameMembers { game } => manage(game, MiniGameRequest::RespawnAll),
        UiAction::EndMiniGame { game } => manage(game, MiniGameRequest::End),
        UiAction::EditMiniGameAddOns {
            game,
            settings,
            teams,
            quiet,
            reset,
        } => MiniGameRequest::AddOnSettings {
            game: game.0,
            quiet: *quiet,
            reset: *reset,
            settings: edits(settings),
            teams: teams.as_ref().map(|list| {
                list.iter()
                    .map(|t| TeamEdit {
                        id: t.id,
                        name: t.name.clone(),
                        color: t.color,
                        settings: edits(&t.settings),
                    })
                    .collect()
            }),
        },
        _ => anyhow::bail!("Not a mini-game action"),
    };
    Ok(Some(Command::MiniGame(request)))
}

pub fn is_minigame_action(action: &UiAction) -> bool {
    matches!(
        action,
        UiAction::RequestMiniGameList
            | UiAction::CreateMiniGame { .. }
            | UiAction::ConfigureMiniGame { .. }
            | UiAction::JoinMiniGame { .. }
            | UiAction::LeaveMiniGame { .. }
            | UiAction::InviteMiniGame { .. }
            | UiAction::AcceptMiniGameInvite { .. }
            | UiAction::RejectMiniGameInvite { .. }
            | UiAction::RemoveMiniGameMember { .. }
            | UiAction::SetMiniGameTeam { .. }
            | UiAction::ResetMiniGame { .. }
            | UiAction::RespawnMiniGameMembers { .. }
            | UiAction::EndMiniGame { .. }
            | UiAction::EditMiniGameAddOns { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rules_round_trip_and_color_availability() {
        let rules_in = MiniGameRules {
            respawn_seconds: 5,
            ..Default::default()
        };
        let settings = settings(&rules_in).unwrap();
        assert_eq!(settings.respawn_ms, 5000);
        assert_eq!(rules(&settings), rules_in);
        let view = MiniGameView {
            id: 1,
            owner: 2,
            color: 3,
            settings,
            members: vec![2],
            teams: Vec::new(),
            addon_settings: Default::default(),
            default: false,
            paint_color: None,
            shared: false,
            name_distance: None,
        };
        let names: BTreeMap<_, _> = [(2, "Host".to_string()), (3, "Guest".to_string())].into();
        let state = state(
            3,
            &[view],
            &BTreeMap::new(),
            &names,
            &[],
            &Default::default(),
            1,
        );
        assert_eq!(state.colors.len(), 9);
        assert!(state.colors.iter().all(|c| c.index != 3));
        assert!(!state.owns_active_game);
        assert_eq!(state.games[0].owner_name, "Host");
        assert!(settings_invalid());
    }
    #[test]
    fn a_player_managing_a_game_they_do_not_own_names_it() {
        let reset = UiAction::ResetMiniGame {
            game: MiniGameId(4),
        };
        let invite = UiAction::InviteMiniGame {
            target: MiniGamePlayerId(9),
        };
        let kick = UiAction::RemoveMiniGameMember {
            target: MiniGamePlayerId(9),
            game: Some(MiniGameId(4)),
        };
        let named = |action: &UiAction, owner: bool| {
            matches!(
                command(action, Some(4), owner).unwrap(),
                Some(Command::MiniGame(MiniGameRequest::Manage { game: 4, .. }))
            )
        };
        // Its owner acts on their own game directly.
        for action in [&reset, &invite, &kick] {
            assert!(!named(action, true));
        }
        // An admin playing in it names the game, so the host checks them
        // as its editor rather than as its owner.
        for action in [&reset, &invite, &kick] {
            assert!(named(action, false));
        }
    }
    #[test]
    fn games_an_admin_may_manage_are_named_without_add_on_settings() {
        let view = MiniGameView {
            id: 1,
            owner: 2,
            color: 3,
            settings: Settings::default(),
            members: vec![2, 3],
            teams: Vec::new(),
            addon_settings: Default::default(),
            default: false,
            paint_color: None,
            shared: false,
            name_distance: None,
        };
        let names: BTreeMap<_, _> = [(2, "Host".to_string()), (3, "Admin".to_string())].into();
        let views = [view];
        let base = state(
            3,
            &views,
            &BTreeMap::new(),
            &names,
            &[],
            &Default::default(),
            1,
        );
        let admin = Rank {
            admin: true,
            ..Default::default()
        };
        let state = with_addon_settings(base.clone(), &views, &[], 3, &admin, None, &[]);
        assert_eq!(state.addon_editable, vec![MiniGameId(1)]);
        let plain = with_addon_settings(base, &views, &[], 3, &Rank::default(), None, &[]);
        assert!(plain.addon_editable.is_empty());
    }
    fn settings_invalid() -> bool {
        settings(&MiniGameRules {
            respawn_seconds: 0,
            ..Default::default()
        })
        .is_err()
    }
}
