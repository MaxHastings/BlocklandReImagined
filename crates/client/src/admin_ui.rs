//! Typed administration boundary. Connection identity and permissions remain host-owned.
use anyhow::{Result, bail, ensure};
use bri_admin::{Action, BanDuration, BanId, ConnectionId, Request, Role, Secret};
use bri_sim::session::{
    AdminCapability as Capability, AdminData, AdminReply, AdminSnapshot, Command,
};
use bri_ui::{
    api::{RequestId, UiUpdate},
    models::admin as ui,
};

fn role(value: Role) -> ui::AdminRole {
    match value {
        Role::Player => ui::AdminRole::Player,
        Role::Admin => ui::AdminRole::Admin,
        Role::SuperAdmin => ui::AdminRole::SuperAdmin,
    }
}
fn plain(value: &str) -> String {
    plain_bounded(value, 128)
}
fn plain_bounded(value: &str, limit: usize) -> String {
    let mut bytes = 0;
    value
        .chars()
        .filter(|c| !c.is_control() && !('\u{e000}'..='\u{e0ff}').contains(c))
        .map(|c| match c {
            '<' => '‹',
            '>' => '›',
            _ => c,
        })
        .take_while(|c| {
            bytes += c.len_utf8();
            bytes <= limit
        })
        .collect()
}
pub fn state(snapshot: &AdminSnapshot) -> ui::AdminSnapshot {
    ui::AdminSnapshot {
        revision: snapshot.revision,
        role: role(snapshot.role),
        local_host: snapshot.local_host,
        legacy_lan: snapshot.legacy_lan,
        supported: snapshot
            .supported
            .iter()
            .filter_map(|capability| {
                Some(match capability {
                    Capability::Login => ui::AdminFeature::Login,
                    Capability::Kick => ui::AdminFeature::Kick,
                    Capability::Ban => ui::AdminFeature::Ban,
                    Capability::Unban => ui::AdminFeature::Unban,
                    Capability::ClearBricks => ui::AdminFeature::ClearBricks,
                    Capability::AdminPassword => ui::AdminFeature::AdminPassword,
                    Capability::HighlightBricks => ui::AdminFeature::HighlightBricks,
                    Capability::WorldCommands => ui::AdminFeature::ClearBricks,
                    Capability::DestructoWand => ui::AdminFeature::Wand,
                    Capability::Spy => ui::AdminFeature::Spy,
                    Capability::ChangeMap => ui::AdminFeature::Maps,
                    Capability::HostOptions => ui::AdminFeature::HostOptions,
                    Capability::Environment => ui::AdminFeature::Environment,
                    // Chat commands only; the Admin menu has no buttons for them.
                    Capability::Teleport | Capability::Vehicles | Capability::TimeScale => {
                        return None;
                    }
                })
            })
            // The host and Super Admins hand out ranks.
            .chain(
                (snapshot.local_host || snapshot.role == Role::SuperAdmin)
                    .then_some(ui::AdminFeature::Ranks),
            )
            .collect(),
        players: snapshot
            .players
            .iter()
            .map(|player| ui::AdminPlayer {
                connection: player.connection,
                name: plain(&player.name),
                identity_label: plain(&player.identity_label),
                role: role(player.role),
                owner: player.owner,
                local: player.local,
                bot: player.bot,
                persistent_identity: player.persistent_identity,
            })
            .collect(),
        options: snapshot.options.as_ref().map(options),
    }
}
fn quotas(q: &bri_admin::Quotas) -> ui::AdminQuotas {
    ui::AdminQuotas {
        schedules: q.schedules,
        misc: q.misc,
        projectiles: q.projectiles,
        items: q.items,
        environment: q.environment,
        players: q.players,
        vehicles: q.vehicles,
    }
}
fn server_quotas(q: &ui::AdminQuotas) -> bri_admin::Quotas {
    bri_admin::Quotas {
        schedules: q.schedules,
        misc: q.misc,
        projectiles: q.projectiles,
        items: q.items,
        environment: q.environment,
        players: q.players,
        vehicles: q.vehicles,
    }
}
fn options(s: &bri_admin::ServerSettings) -> ui::AdminOptions {
    ui::AdminOptions {
        name: s.name.clone(),
        port: s.port,
        max_players: s.max_players,
        brick_limit: s.brick_limit,
        bricks_per_second: s.bricks_per_second,
        max_chat_length: s.max_chat_length,
        physics_vehicles: s.physics_vehicles,
        player_vehicles: s.player_vehicles,
        random_brick_color: s.random_brick_color,
        chat_filter: s.chat_filter,
        falling_damage: s.falling_damage,
        public_domain_timeout_minutes: s.public_domain_timeout_minutes,
        too_far_distance: s.too_far_distance,
        per_player: quotas(&s.per_player),
        lan: quotas(&s.lan),
        addon_settings: s
            .addon_settings
            .iter()
            .map(|(k, v)| (k.clone(), crate::minigame_ui::ui_value(v)))
            .collect(),
    }
}
/// A new host's Server Settings: the saved Advanced Config (`$Pref::Server::*`)
/// with this game's name and player limit. Saved values the host would
/// refuse fall back to v20's defaults.
pub fn host_settings(
    o: &ui::AdminOptions,
    name: &str,
    max_players: u16,
) -> bri_admin::ServerSettings {
    let base = bri_admin::ServerSettings {
        name: name.into(),
        max_players,
        ..Default::default()
    };
    let saved = settings(
        &ui::AdminOptions {
            name: name.into(),
            max_players,
            ..o.clone()
        },
        &base,
    );
    if saved.validate().is_ok() {
        return saved;
    }
    // Saved Add-On settings the host would refuse are left out first.
    let saved = bri_admin::ServerSettings {
        addon_settings: Default::default(),
        ..saved
    };
    if saved.validate().is_ok() {
        saved
    } else {
        base
    }
}
/// The Server Settings dialog's values over the host's current settings
/// (keeping those the dialog does not show).
fn settings(
    o: &ui::AdminOptions,
    current: &bri_admin::ServerSettings,
) -> bri_admin::ServerSettings {
    bri_admin::ServerSettings {
        name: o.name.clone(),
        port: o.port,
        max_players: o.max_players,
        brick_limit: o.brick_limit,
        bricks_per_second: o.bricks_per_second,
        max_chat_length: o.max_chat_length,
        physics_vehicles: o.physics_vehicles,
        player_vehicles: o.player_vehicles,
        random_brick_color: o.random_brick_color,
        chat_filter: o.chat_filter,
        falling_damage: o.falling_damage,
        public_domain_timeout_minutes: o.public_domain_timeout_minutes,
        too_far_distance: o.too_far_distance,
        per_player: server_quotas(&o.per_player),
        lan: server_quotas(&o.lan),
        addon_settings: o
            .addon_settings
            .iter()
            .map(|(k, v)| (k.clone(), crate::minigame_ui::host_value(v)))
            .collect(),
        ..current.clone()
    }
}

/// None means refresh from the latest authenticated connection snapshot.
/// Never infer host authority from a UI action, target, name or legacy ID.
pub fn command(action: &ui::AdminAction, snapshot: &AdminSnapshot) -> Result<Option<Command>> {
    let (capability, action) = match action {
        ui::AdminAction::Refresh => return Ok(None),
        ui::AdminAction::SetRole { target, role } => {
            return set_role(snapshot, ConnectionId(*target), *role).map(Some);
        }
        ui::AdminAction::RequestRanks => {
            return ranks_request(snapshot, Action::RequestAutoRoles).map(Some);
        }
        ui::AdminAction::ForgetRank { key } => {
            return ranks_request(
                snapshot,
                Action::HostSetAutoRole {
                    principal: principal(key)?,
                    role: Role::Player,
                },
            )
            .map(Some);
        }
        ui::AdminAction::Login { password } => (
            Capability::Login,
            Action::Login {
                password: Secret::new(password.0.clone())?,
            },
        ),
        ui::AdminAction::Kick { target } => (
            Capability::Kick,
            Action::Kick {
                target: ConnectionId(*target),
            },
        ),
        ui::AdminAction::Ban {
            target,
            minutes,
            reason,
        } => (
            Capability::Ban,
            Action::Ban {
                target: ConnectionId(*target),
                duration: minutes.map_or(BanDuration::Forever, BanDuration::Minutes),
                reason: reason.clone(),
            },
        ),
        ui::AdminAction::Spy { target } => (
            Capability::Spy,
            Action::Spy {
                target: ConnectionId(*target),
            },
        ),
        ui::AdminAction::RequestBans => (Capability::Unban, Action::RequestBanList),
        ui::AdminAction::Unban { ban } => (Capability::Unban, Action::Unban { ban: BanId(*ban) }),
        ui::AdminAction::RequestBrickGroups => {
            (Capability::ClearBricks, Action::RequestBrickGroups)
        }
        ui::AdminAction::ClearBrickGroup { group } => (
            Capability::ClearBricks,
            Action::ClearBrickGroup { group: *group },
        ),
        ui::AdminAction::ClearAllBricks => (Capability::ClearBricks, Action::ClearAllBricks),
        ui::AdminAction::HighlightBrickGroup { group } => (
            Capability::HighlightBricks,
            Action::HighlightBrickGroup { group: *group },
        ),
        ui::AdminAction::Wand => (Capability::DestructoWand, Action::DestructoWand),
        ui::AdminAction::RequestMaps => (Capability::ChangeMap, Action::RequestMaps),
        ui::AdminAction::ChangeMap { map } => (
            Capability::ChangeMap,
            Action::ChangeMap { map: map.clone() },
        ),
        ui::AdminAction::SetPassword {
            slot: ui::AdminPasswordSlot::Admin,
            password,
        } => (
            Capability::AdminPassword,
            Action::SetAdminPassword {
                password: Secret::new(password.0.clone())?,
            },
        ),
        ui::AdminAction::ConfigureHost { options } => (
            Capability::HostOptions,
            Action::HostConfigure {
                settings: settings(
                    options,
                    snapshot
                        .options
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("Host settings unavailable"))?,
                ),
            },
        ),
        ui::AdminAction::SetEnvironment { settings } => (
            Capability::Environment,
            Action::SetEnvironment {
                settings: (**settings).clone(),
            },
        ),
        _ => bail!("This administration operation is not connected to gameplay yet"),
    };
    ensure!(
        snapshot.supported.contains(&capability),
        "Host does not support this administration operation"
    );
    let allowed = match capability {
        Capability::Login => true,
        Capability::AdminPassword => snapshot.local_host || snapshot.role == Role::SuperAdmin,
        Capability::HostOptions => snapshot.local_host,
        _ => snapshot.role.is_admin(),
    };
    ensure!(allowed, "Administration permission has changed");
    let request = Request::new(action);
    request.validate()?;
    Ok(Some(Command::Admin(request)))
}

/// Make `target` Admin or Super Admin, or a plain player again. The host
/// checks the rank again; this only keeps players from asking in vain.
fn set_role(
    snapshot: &AdminSnapshot,
    target: ConnectionId,
    rank: ui::AdminRole,
) -> Result<Command> {
    ensure!(
        snapshot.local_host || snapshot.role == Role::SuperAdmin,
        "Only a Super Admin can change ranks"
    );
    let request = Request::new(Action::HostSetRole {
        target,
        role: match rank {
            ui::AdminRole::Player => Role::Player,
            ui::AdminRole::Admin => Role::Admin,
            ui::AdminRole::SuperAdmin => Role::SuperAdmin,
        },
    });
    request.validate()?;
    Ok(Command::Admin(request))
}

/// Read or change the saved rank list (the host and Super Admins).
fn ranks_request(snapshot: &AdminSnapshot, action: Action) -> Result<Command> {
    ensure!(
        snapshot.local_host || snapshot.role == Role::SuperAdmin,
        "Only a Super Admin can change ranks"
    );
    let request = Request::new(action);
    request.validate()?;
    Ok(Command::Admin(request))
}

fn hex(key: &bri_admin::Principal) -> String {
    key.0.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn principal(key: &str) -> Result<bri_admin::Principal> {
    ensure!(key.len() == 64 && key.is_ascii(), "Invalid player key");
    let mut out = [0; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&key[i * 2..i * 2 + 2], 16)?;
    }
    Ok(bri_admin::Principal(out))
}

/// v20 `findClientByName`: the name that contains `partial` earliest.
fn find_player(snapshot: &AdminSnapshot, partial: &str) -> Result<ConnectionId> {
    let partial = partial.to_lowercase();
    snapshot
        .players
        .iter()
        .filter(|p| !p.bot)
        .filter_map(|p| p.name.to_lowercase().find(&partial).map(|at| (at, p)))
        .min_by_key(|(at, _)| *at)
        .map(|(_, p)| ConnectionId(p.connection))
        .ok_or_else(|| anyhow::anyhow!("No player named {partial}"))
}

/// Administrator chat commands (`/fetch`, `/find`, `/warp`, `/timeScale`,
/// `/resetVehicles`, `/clearVehicles`, `/realBrickCount`, `/cancelAllEvents`,
/// `/clearBots`). `None` when `name` is not one of them.
pub fn chat_command(
    name: &str,
    args: &[String],
    snapshot: &AdminSnapshot,
) -> Result<Option<Command>> {
    let joined = args.join(" ");
    // `/admin`, `/superAdmin` and `/deAdmin <name>` change a player's rank.
    let rank = match name.to_ascii_lowercase().as_str() {
        "admin" => Some(ui::AdminRole::Admin),
        "superadmin" => Some(ui::AdminRole::SuperAdmin),
        "deadmin" => Some(ui::AdminRole::Player),
        _ => None,
    };
    if let Some(rank) = rank {
        ensure!(!joined.trim().is_empty(), "Usage: /{name} <player name>");
        return set_role(snapshot, find_player(snapshot, joined.trim())?, rank).map(Some);
    }
    let (capability, action) = match name.to_ascii_lowercase().as_str() {
        "fetch" => (
            Capability::Teleport,
            Action::Fetch {
                target: find_player(snapshot, &joined)?,
            },
        ),
        "find" => (
            Capability::Teleport,
            Action::Find {
                target: find_player(snapshot, &joined)?,
            },
        ),
        "warp" => (Capability::Teleport, Action::Warp),
        // `serverCmdSpy`: watch a player through a corpse camera; `/ret`
        // returns (any player may return to their own body).
        "spy" => (
            Capability::Spy,
            Action::Spy {
                target: find_player(snapshot, &joined)?,
            },
        ),
        "timescale" => (
            Capability::TimeScale,
            Action::TimeScale {
                // `mClampF` of a non-number is 0, clamped up to 0.2.
                scale: args.first().and_then(|a| a.parse().ok()).unwrap_or(0.0),
            },
        ),
        "resetvehicles" => (Capability::Vehicles, Action::ResetVehicles),
        "clearvehicles" => (Capability::Vehicles, Action::ClearVehicles),
        "realbrickcount" => (Capability::WorldCommands, Action::RealBrickCount),
        // `ServerCmdClearAllBricks`; `/clearBricks` (one's own) goes to the host.
        "clearallbricks" => (Capability::ClearBricks, Action::ClearAllBricks),
        "cancelallevents" => (Capability::WorldCommands, Action::CancelAllEvents),
        "clearbots" => (Capability::WorldCommands, Action::ClearBots),
        _ => return Ok(None),
    };
    // Stock servers ignore these from non-administrators.
    ensure!(
        snapshot.role.is_admin() && snapshot.supported.contains(&capability),
        "You are not an administrator"
    );
    let request = Request::new(action);
    request.validate()?;
    Ok(Some(Command::Admin(request)))
}

pub fn reply_updates(
    id: RequestId,
    action: &ui::AdminAction,
    reply: &AdminReply,
) -> Result<Vec<UiUpdate>> {
    let mut updates = vec![UiUpdate::Admin(ui::AdminUpdate::State(state(
        &reply.snapshot,
    )))];
    match (&reply.data, action) {
        (AdminData::LoginRejected { attempts }, ui::AdminAction::Login { .. }) => {
            bail!("Administrator password rejected (attempt {attempts} of 4)")
        }
        (AdminData::AutoRoles(rows), ui::AdminAction::RequestRanks) => {
            updates.push(UiUpdate::Admin(ui::AdminUpdate::Ranks {
                request: id,
                revision: reply.snapshot.revision,
                rows: rows
                    .iter()
                    .map(|row| ui::AdminSavedRank {
                        key: hex(&row.principal),
                        name: plain(&row.name),
                        role: role(row.role),
                    })
                    .collect(),
            }));
        }
        (AdminData::BrickGroups(rows), ui::AdminAction::RequestBrickGroups) => {
            updates.push(UiUpdate::Admin(ui::AdminUpdate::BrickGroups {
                request: id,
                revision: reply.snapshot.revision,
                rows: rows
                    .iter()
                    .map(|row| ui::AdminBrickGroup {
                        id: row.id,
                        name: plain(&row.name),
                        identity_label: plain(&row.identity_label),
                        bricks: row.bricks,
                    })
                    .collect(),
            }));
        }
        (
            AdminData::BanList {
                rows,
                now_unix_seconds,
            },
            ui::AdminAction::RequestBans,
        ) => {
            updates.push(UiUpdate::Admin(ui::AdminUpdate::Bans {
                request: id,
                revision: reply.snapshot.revision,
                rows: rows
                    .iter()
                    .map(|row| ui::AdminBan {
                        id: row.id.0,
                        administrator: plain(&row.issued_by),
                        name: plain(&row.victim_name),
                        // This is a public-key fingerprint, never a legacy BL_ID.
                        identity_label: format!(
                            "Native {}",
                            row.principal
                                .0
                                .iter()
                                .map(|byte| format!("{byte:02x}"))
                                .collect::<String>()
                        ),
                        address: None,
                        reason: plain_bounded(&row.reason, 512),
                        remaining_minutes: row
                            .expires_unix_seconds
                            .map(|end| end.saturating_sub(*now_unix_seconds).div_ceil(60)),
                    })
                    .collect(),
            }));
        }
        (AdminData::Maps(rows), ui::AdminAction::RequestMaps) => {
            updates.push(UiUpdate::Admin(ui::AdminUpdate::Maps {
                request: id,
                revision: reply.snapshot.revision,
                rows: rows
                    .iter()
                    .map(|row| ui::AdminMap {
                        id: row.id.clone(),
                        name: plain(&row.name),
                    })
                    .collect(),
            }));
        }
        (AdminData::None, ui::AdminAction::Login { .. }) => ensure!(
            reply.snapshot.role.is_admin(),
            "Host did not grant administrator permission"
        ),
        (
            AdminData::None,
            ui::AdminAction::Kick { .. }
            | ui::AdminAction::Ban { .. }
            | ui::AdminAction::Spy { .. }
            | ui::AdminAction::Unban { .. }
            | ui::AdminAction::ClearBrickGroup { .. }
            | ui::AdminAction::ClearAllBricks
            | ui::AdminAction::ChangeMap { .. }
            | ui::AdminAction::SetPassword { .. }
            | ui::AdminAction::SetRole { .. }
            | ui::AdminAction::ForgetRank { .. }
            | ui::AdminAction::SetEnvironment { .. },
        ) => {}
        _ => bail!("Host returned an unexpected administration reply"),
    }
    Ok(updates)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(role: Role) -> AdminSnapshot {
        AdminSnapshot {
            revision: 3,
            role,
            local_host: false,
            legacy_lan: false,
            supported: [
                Capability::Login,
                Capability::Kick,
                Capability::ClearBricks,
                Capability::AdminPassword,
            ]
            .into(),
            players: vec![],
            options: None,
        }
    }
    #[test]
    fn requests_use_connection_targets_without_actor_or_claimed_roles() -> Result<()> {
        let s = snapshot(Role::Admin);
        let Some(Command::Admin(request)) = command(&ui::AdminAction::Kick { target: 73 }, &s)?
        else {
            panic!("missing request")
        };
        assert_eq!(
            request.action,
            Action::Kick {
                target: ConnectionId(73)
            }
        );
        assert!(
            command(
                &ui::AdminAction::Kick { target: 73 },
                &snapshot(Role::Player)
            )
            .is_err()
        );
        assert!(
            command(
                &ui::AdminAction::SetPassword {
                    slot: ui::AdminPasswordSlot::Admin,
                    password: ui::AdminSecret("secret".into())
                },
                &s
            )
            .is_err()
        );
        assert!(command(&ui::AdminAction::Wand, &snapshot(Role::SuperAdmin)).is_err());
        let mapped = state(&s);
        assert!(!mapped.supported.contains(&ui::AdminFeature::Ban));
        assert!(!mapped.supported.contains(&ui::AdminFeature::HostOptions));
        Ok(())
    }
    #[test]
    fn spy_watches_a_player_named_in_chat_for_admins_only() -> Result<()> {
        let mut s = snapshot(Role::Admin);
        s.supported.insert(Capability::Spy);
        s.players.push(bri_sim::session::AdminPlayer {
            connection: 9,
            name: "Builder".into(),
            identity_label: String::new(),
            role: Role::Player,
            owner: false,
            local: false,
            bot: false,
            persistent_identity: true,
        });
        let Some(Command::Admin(request)) = chat_command("spy", &["buil".into()], &s)? else {
            panic!("missing request")
        };
        assert_eq!(
            request.action,
            Action::Spy {
                target: ConnectionId(9)
            }
        );
        s.role = Role::Player;
        assert!(chat_command("spy", &["buil".into()], &s).is_err());
        Ok(())
    }
    #[test]
    fn super_admins_change_ranks_from_chat_and_the_menu() -> Result<()> {
        let mut s = snapshot(Role::SuperAdmin);
        s.players.push(bri_sim::session::AdminPlayer {
            connection: 9,
            name: "Builder".into(),
            identity_label: String::new(),
            role: Role::Player,
            owner: false,
            local: false,
            bot: false,
            persistent_identity: true,
        });
        assert!(state(&s).supported.contains(&ui::AdminFeature::Ranks));
        for (name, role) in [
            ("admin", Role::Admin),
            ("superAdmin", Role::SuperAdmin),
            ("deAdmin", Role::Player),
        ] {
            let Some(Command::Admin(request)) = chat_command(name, &["buil".into()], &s)? else {
                panic!("missing request")
            };
            assert_eq!(
                request.action,
                Action::HostSetRole {
                    target: ConnectionId(9),
                    role
                }
            );
        }
        let Some(Command::Admin(request)) = command(
            &ui::AdminAction::SetRole {
                target: 9,
                role: ui::AdminRole::Admin,
            },
            &s,
        )?
        else {
            panic!("missing request")
        };
        assert_eq!(
            request.action,
            Action::HostSetRole {
                target: ConnectionId(9),
                role: Role::Admin
            }
        );
        assert!(chat_command("admin", &[], &s).is_err());
        // An Admin cannot hand out ranks; the host always can.
        s.role = Role::Admin;
        assert!(!state(&s).supported.contains(&ui::AdminFeature::Ranks));
        assert!(chat_command("admin", &["buil".into()], &s).is_err());
        s.local_host = true;
        assert!(chat_command("superadmin", &["buil".into()], &s)?.is_some());
        Ok(())
    }
    #[test]
    fn clear_all_bricks_from_chat_is_for_admins_and_clear_bricks_goes_to_the_host() -> Result<()> {
        let s = snapshot(Role::Admin);
        let Some(Command::Admin(request)) = chat_command("clearAllBricks", &[], &s)? else {
            panic!("missing request")
        };
        assert_eq!(request.action, Action::ClearAllBricks);
        assert!(chat_command("clearallbricks", &[], &snapshot(Role::Player)).is_err());
        // A player's own `/clearBricks` is the host's typed command.
        assert!(chat_command("clearBricks", &[], &snapshot(Role::Player))?.is_none());
        Ok(())
    }
    #[test]
    fn the_saved_rank_list_round_trips_player_keys() -> Result<()> {
        let s = snapshot(Role::SuperAdmin);
        let key = bri_admin::Principal([0xa7; 32]);
        let reply = AdminReply {
            snapshot: s.clone(),
            data: AdminData::AutoRoles(vec![bri_admin::AutoRole {
                principal: key,
                role: Role::Admin,
                name: "Builder".into(),
            }]),
        };
        let updates = reply_updates(5, &ui::AdminAction::RequestRanks, &reply)?;
        let Some(UiUpdate::Admin(ui::AdminUpdate::Ranks { rows, .. })) = updates.last() else {
            panic!("no rank list: {updates:?}")
        };
        assert_eq!(rows[0].name, "Builder");
        let Some(Command::Admin(request)) = command(
            &ui::AdminAction::ForgetRank {
                key: rows[0].key.clone(),
            },
            &s,
        )?
        else {
            panic!("missing request")
        };
        assert_eq!(
            request.action,
            Action::HostSetAutoRole {
                principal: key,
                role: Role::Player
            }
        );
        assert!(command(&ui::AdminAction::ForgetRank { key: "zz".into() }, &s).is_err());
        assert!(command(&ui::AdminAction::RequestRanks, &snapshot(Role::Admin)).is_err());
        // A rank change is answered with no data, and that is success.
        let done = AdminReply {
            snapshot: s,
            data: AdminData::None,
        };
        reply_updates(
            6,
            &ui::AdminAction::SetRole {
                target: 9,
                role: ui::AdminRole::Admin,
            },
            &done,
        )?;
        Ok(())
    }
    #[test]
    fn environment_changes_need_the_host_capability_and_an_admin() -> Result<()> {
        let settings = bri_content::atmosphere::Settings {
            sun_azimuth: Some(120.0),
            ..Default::default()
        };
        let action = ui::AdminAction::SetEnvironment {
            settings: Box::new(settings.clone()),
        };
        let mut s = snapshot(Role::Admin);
        assert!(command(&action, &s).is_err(), "the host does not offer it");
        s.supported.insert(Capability::Environment);
        assert!(state(&s).supported.contains(&ui::AdminFeature::Environment));
        match command(&action, &s)? {
            Some(Command::Admin(request)) => {
                assert_eq!(request.action, Action::SetEnvironment { settings });
            }
            other => panic!("{other:?}"),
        }
        assert!(command(&action, &snapshot(Role::Player)).is_err());
        Ok(())
    }
    #[test]
    fn advanced_config_defaults_are_the_hosts() {
        let d = bri_admin::ServerSettings::default();
        assert_eq!(options(&d), ui::AdminOptions::default());
        assert_eq!(
            host_settings(&ui::AdminOptions::default(), &d.name, d.max_players),
            d
        );
        let mut saved = ui::AdminOptions {
            max_chat_length: 40,
            random_brick_color: true,
            ..Default::default()
        };
        let s = host_settings(&saved, "Build", 12);
        assert_eq!((s.max_chat_length, s.random_brick_color), (40, true));
        assert_eq!((s.name.as_str(), s.max_players), ("Build", 12));
        saved.max_chat_length = 5000;
        assert_eq!(
            host_settings(&saved, "Build", 12).max_chat_length,
            d.max_chat_length
        );
    }
    #[test]
    fn rejected_password_and_wrong_reply_never_report_success() {
        let action = ui::AdminAction::Login {
            password: ui::AdminSecret("private".into()),
        };
        let mut reply = AdminReply {
            snapshot: snapshot(Role::Player),
            data: AdminData::LoginRejected { attempts: 2 },
        };
        assert!(
            reply_updates(1, &action, &reply)
                .unwrap_err()
                .to_string()
                .contains("attempt 2")
        );
        reply.data = AdminData::None;
        assert!(reply_updates(1, &action, &reply).is_err());
        reply.snapshot.role = Role::Admin;
        assert!(reply_updates(1, &action, &reply).is_ok());
        assert!(reply_updates(1, &ui::AdminAction::RequestBrickGroups, &reply).is_err());
    }
    #[test]
    fn group_reply_retains_request_identity_and_unowned_group() -> Result<()> {
        let reply = AdminReply {
            snapshot: snapshot(Role::SuperAdmin),
            data: AdminData::BrickGroups(vec![bri_sim::session::AdminBrickGroup {
                id: 0,
                name: "Unowned <color:ff0000>".into(),
                identity_label: "Unavailable".into(),
                bricks: 9,
            }]),
        };
        let mut model = ui::AdminModel::default();
        model.pending.insert(7, ui::AdminAction::RequestBrickGroups);
        for update in reply_updates(7, &ui::AdminAction::RequestBrickGroups, &reply)? {
            let UiUpdate::Admin(update) = update else {
                unreachable!()
            };
            model.apply(update).map_err(anyhow::Error::msg)?;
        }
        assert_eq!(model.groups[0].id, 0);
        assert_eq!(model.groups[0].name, "Unowned ‹color:ff0000›");
        assert!(!model.pending.contains_key(&7));
        assert!(model.allowed(&ui::AdminAction::ClearBrickGroup { group: 0 }));
        assert!(!model.allowed(&ui::AdminAction::HighlightBrickGroup { group: 0 }));
        Ok(())
    }
    #[test]
    fn ban_requests_require_host_capability_and_validate_duration() -> Result<()> {
        let action = ui::AdminAction::Ban {
            target: 91,
            minutes: Some(60),
            reason: "Repeated griefing".into(),
        };
        let mut s = snapshot(Role::Admin);
        assert!(command(&action, &s).is_err());
        s.supported.insert(Capability::Ban);
        let Some(Command::Admin(request)) = command(&action, &s)? else {
            panic!("missing ban request")
        };
        assert_eq!(
            request.action,
            Action::Ban {
                target: ConnectionId(91),
                duration: BanDuration::Minutes(60),
                reason: "Repeated griefing".into(),
            }
        );
        let invalid = ui::AdminAction::Ban {
            target: 91,
            minutes: Some(0),
            reason: "invalid".into(),
        };
        assert!(command(&invalid, &s).is_err());
        s.role = Role::Player;
        assert!(command(&action, &s).is_err());
        Ok(())
    }
    #[test]
    fn ban_reply_uses_host_time_and_preserves_reason_and_stable_ids() -> Result<()> {
        let mut s = snapshot(Role::Admin);
        s.supported.insert(Capability::Unban);
        let reason = "é".repeat(200);
        let row = bri_admin::BanRecord {
            id: BanId(18),
            principal: bri_admin::Principal([0xab; 32]),
            victim_name: "<name>".into(),
            issued_by: "Host".into(),
            reason: reason.clone(),
            created_unix_seconds: 100,
            expires_unix_seconds: Some(221),
        };
        let mut reply = AdminReply {
            snapshot: s,
            data: AdminData::BanList {
                rows: vec![row],
                now_unix_seconds: 160,
            },
        };
        let mut model = ui::AdminModel::default();
        model.pending.insert(19, ui::AdminAction::RequestBans);
        for update in reply_updates(19, &ui::AdminAction::RequestBans, &reply)? {
            let UiUpdate::Admin(update) = update else {
                unreachable!()
            };
            model.apply(update).map_err(anyhow::Error::msg)?;
        }
        assert_eq!(model.bans[0].id, 18);
        assert_eq!(model.bans[0].name, "‹name›");
        assert_eq!(model.bans[0].reason, reason);
        assert_eq!(model.bans[0].remaining_minutes, Some(2));
        assert_eq!(
            model.bans[0].identity_label,
            format!("Native {}", "ab".repeat(32))
        );
        assert!(model.bans[0].address.is_none());
        assert!(!model.pending.contains_key(&19));
        let Some(Command::Admin(request)) =
            command(&ui::AdminAction::Unban { ban: 18 }, &reply.snapshot)?
        else {
            panic!("missing unban request")
        };
        assert_eq!(request.action, Action::Unban { ban: BanId(18) });
        assert!(reply_updates(20, &ui::AdminAction::RequestBrickGroups, &reply).is_err());
        if let AdminData::BanList { rows, .. } = &mut reply.data {
            rows[0].expires_unix_seconds = None;
        }
        let updates = reply_updates(21, &ui::AdminAction::RequestBans, &reply)?;
        let UiUpdate::Admin(ui::AdminUpdate::Bans { rows, .. }) = &updates[1] else {
            panic!("missing ban rows")
        };
        assert_eq!(rows[0].remaining_minutes, None);
        Ok(())
    }
}
