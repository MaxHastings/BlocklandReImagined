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
                // Chat commands only; the Admin menu has no buttons for them.
                Capability::Teleport | Capability::Vehicles | Capability::TimeScale => {
                    return None;
                }
                })
            })
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
    }
}
/// The Server Settings dialog's values over the host's current settings
/// (keeping those the dialog does not show).
fn settings(o: &ui::AdminOptions, current: &bri_admin::ServerSettings) -> bri_admin::ServerSettings {
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
        ..current.clone()
    }
}

/// None means refresh from the latest authenticated connection snapshot.
/// Never infer host authority from a UI action, target, name or legacy ID.
pub fn command(action: &ui::AdminAction, snapshot: &AdminSnapshot) -> Result<Option<Command>> {
    let (capability, action) = match action {
        ui::AdminAction::Refresh => return Ok(None),
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
        ui::AdminAction::ChangeMap { map } => {
            (Capability::ChangeMap, Action::ChangeMap { map: map.clone() })
        }
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
    let (capability, action) = match name.to_ascii_lowercase().as_str() {
        "fetch" => (Capability::Teleport, Action::Fetch { target: find_player(snapshot, &joined)? }),
        "find" => (Capability::Teleport, Action::Find { target: find_player(snapshot, &joined)? }),
        "warp" => (Capability::Teleport, Action::Warp),
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
            | ui::AdminAction::SetPassword { .. },
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
