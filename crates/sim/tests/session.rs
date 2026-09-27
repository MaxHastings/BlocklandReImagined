use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{Command, Reply, Session, Snapshot},
    simulation::Simulation,
};
use bri_world::{Action, World, authority::Edit};
use glam::Vec3;
use rapier3d::prelude::*;
fn session() -> Session {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 1],
        height_plates: 1,
        attachment_rows: vec!["bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.2, 0.5],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    let defs = Definitions {
        entries: [(
            "plate".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                requires_behavior_adapter: false,
            },
        )]
        .into(),
    };
    Session::new(
        Simulation::new(
            World::new("Session".into(), "test".into(), vec![[1.0; 4], [0.0; 4]]),
            defs,
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    )
}

#[test]
fn inventories_are_owned_by_connections_and_reconnect_spawns_stock_tools() {
    let mut s = session();
    let a = s.join("A".into(), Vec3::Y, false).unwrap();
    let b = s.join("B".into(), Vec3::new(4., 1., 0.), false).unwrap();
    let stock = bri_sim::session::ToolInventory::default();
    assert_eq!(s.tool_inventories()[&a], stock);
    assert_eq!(s.tool_inventories()[&b], stock);
    s.command(a, 1, Command::EquipTool { slot: Some(1) })
        .unwrap();
    assert_eq!(s.tool_inventories()[&a].selected, Some(1));
    assert_eq!(s.tool_inventories()[&b].selected, None);
    assert!(s.command(a, 1, Command::EquipTool { slot: None }).is_err());
    assert!(
        s.command(a, 2, Command::EquipTool { slot: Some(4) })
            .is_err()
    );
    assert_eq!(s.tool_inventories()[&a].selected, Some(1));
    let wand = "v20.weapon.wanditem";
    assert_eq!(s.give_item(a, wand).unwrap(), 3);
    assert!(s.give_item(a, wand).is_err());
    assert!(s.give_item(b + 99, wand).is_err());
    assert!(
        serde_json::from_str::<Command>(
            r#"{"kind":"give_item","value":{"item":"v20.weapon.wanditem"}}"#
        )
        .is_err()
    );
    let checkpoint = s.snapshot();
    assert_eq!(checkpoint.tools[&a].slots[3].as_deref(), Some(wand));
    s.disconnect(a).unwrap();
    assert!(!s.tool_inventories().contains_key(&a));
    s.resume(a, Vec3::Y).unwrap();
    assert_eq!(s.tool_inventories()[&a], stock);
}

#[test]
fn failed_spawn_attempts_do_not_leave_admin_rows_on_join_or_resume() {
    use bri_admin::Principal;

    let mut s = session();
    let host = s
        .join_verified("Host".into(), Vec3::Y, true, Some(Principal([1; 32])))
        .unwrap();
    let principal = Some(Principal([2; 32]));

    assert!(s
        .join_verified("Guest".into(), Vec3::splat(f32::NAN), false, principal)
        .is_err());
    let guest = s
        .join_verified("Guest".into(), Vec3::new(5.0, 1.0, 0.0), false, principal)
        .unwrap();
    let rows = s.admin_state(host).unwrap().players;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|row| row.name == "Host" && row.connection == 1));
    assert!(rows.iter().any(|row| row.name == "Guest" && row.connection == 3));

    s.disconnect(guest).unwrap();
    assert!(s
        .resume_verified(guest, Vec3::splat(f32::NAN), false, principal)
        .is_err());
    s.resume_verified(guest, Vec3::new(9.0, 1.0, 0.0), false, principal)
        .unwrap();
    let rows = s.admin_state(host).unwrap().players;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|row| row.name == "Host" && row.connection == 1));
    assert!(rows.iter().any(|row| row.name == "Guest" && row.connection == 5));
}

#[test]
fn setup_admin_passwords_are_prejoin_only_and_login_grants_authoritative_role() {
    use bri_admin::{Action, Request, Role, Secret};

    let mut s = session();
    s.set_admin_passwords(
        Secret::new("admin-pass".into()).unwrap(),
        Secret::new("super-pass".into()).unwrap(),
    )
    .unwrap();
    let owner = s.join("Player".into(), Vec3::Y, false).unwrap();
    assert!(s
        .set_admin_passwords(
            Secret::new("replacement".into()).unwrap(),
            Secret::new(String::new()).unwrap(),
        )
        .is_err());

    let command = Command::Admin(Request::new(Action::Login {
        password: Secret::new("admin-pass".into()).unwrap(),
    }));
    let Reply::Admin(reply) = s.command(owner, 1, command).unwrap() else {
        panic!("Expected admin reply");
    };
    assert_eq!(reply.snapshot.role, Role::Admin);
    assert!(s.is_administrator(owner));
}

#[test]
fn ban_and_unban_publish_only_after_durable_commit() {
    use bri_admin::{Action, BanDuration, DurableState, Principal, Request};

    let mut s = session();
    let host = s
        .join_verified("Host".into(), Vec3::Y, true, Some(Principal([1; 32])))
        .unwrap();
    let target = s
        .join_verified(
            "Same name".into(),
            Vec3::new(4., 1., 0.),
            false,
            Some(Principal([2; 32])),
        )
        .unwrap();
    let connection = s
        .admin_state(host)
        .unwrap()
        .players
        .into_iter()
        .find(|player| player.name == "Same name")
        .unwrap()
        .connection;
    let ban = Command::Admin(Request::new(Action::Ban {
        target: bri_admin::ConnectionId(connection),
        duration: BanDuration::Forever,
        reason: "fixture".into(),
    }));

    assert!(s
        .command_with_aim_and_admin_persistence(host, 1, ban.clone(), None, |_| {
            anyhow::bail!("disk full")
        })
        .is_err());
    assert!(s.admin_durable_state().bans.is_empty());
    assert!(s.names().contains_key(&target));
    assert!(s.take_admin_disconnects().is_empty());

    let mut saved = DurableState::default();
    let Reply::Admin(reply) = s
        .command_with_aim_and_admin_persistence(host, 2, ban, None, |candidate| {
            saved = candidate.clone();
            Ok(())
        })
        .unwrap()
    else {
        panic!("ban returns authoritative admin reply");
    };
    assert_eq!(reply.snapshot.role, bri_admin::Role::SuperAdmin);
    assert_eq!(s.admin_durable_state().bans.len(), 1);
    assert_eq!(saved.bans, s.admin_durable_state().bans);
    assert_eq!(s.take_admin_disconnects(), vec![target]);
    s.disconnect(target).unwrap();

    let unban = Command::Admin(Request::new(Action::Unban {
        ban: saved.bans[0].id,
    }));
    assert!(s
        .command_with_aim_and_admin_persistence(host, 3, unban, None, |_| {
            anyhow::bail!("disk full")
        })
        .is_err());
    assert_eq!(s.admin_durable_state().bans, saved.bans);
}

#[test]
fn release_after_core_switch_is_idempotent_but_cannot_start_a_weapon() {
    let mut s = session();
    let owner = s.join("Player".into(), Vec3::Y, false).unwrap();
    s.command(owner, 1, Command::WeaponTrigger { down: false })
        .unwrap();
    s.command(owner, 2, Command::EquipTool { slot: Some(0) })
        .unwrap();
    s.command(owner, 3, Command::WeaponTrigger { down: false })
        .unwrap();
    assert!(
        s.command(owner, 4, Command::WeaponTrigger { down: true })
            .is_err()
    );
    s.step().unwrap();
    assert!(s.weapon_view().projectiles.is_empty());
}

#[test]
#[ignore = "requires converted native weapons pack; host only"]
fn full_trigger_queue_always_accepts_release_and_cancels_pending_fire_observably() {
    let mut s = session();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/weapons-pack-003/weapons.json");
    s.set_weapon_pack(bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap())
        .unwrap();
    let owner = s.join("Player".into(), Vec3::Y, false).unwrap();
    s.give_item(owner, "v20.weapon.gunitem").unwrap();
    s.command(owner, 1, Command::EquipTool { slot: Some(3) })
        .unwrap();
    for seq in 2..=33 {
        s.command(owner, seq, Command::WeaponTrigger { down: true })
            .unwrap();
    }
    assert!(
        s.command(owner, 34, Command::WeaponTrigger { down: true })
            .is_err()
    );
    s.command(owner, 35, Command::WeaponTrigger { down: false })
        .unwrap();
    assert_eq!(
        s.weapon_adapter_gaps()["trigger backlog cancelled for release"],
        32
    );
    for _ in 0..60 {
        s.step().unwrap();
    }
    assert!(s.weapon_view().projectiles.is_empty());
    assert!(!s.take_cues().iter().any(|c| matches!(&c.kind,bri_sim::presentation::CueKind::WeaponSound {profile} if profile.eq_ignore_ascii_case("gunShot1Sound"))));
}

#[test]
#[ignore = "uses converted native weapons pack; headless server only"]
fn native_gun_quick_trigger_edges_use_host_tick_pose_and_reliable_sound() {
    use bri_sim::{presentation::CueKind, session::ActionAim};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/weapons-pack-003/weapons.json");
    let pack = bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap();
    let mut s = session();
    s.set_weapon_pack(pack).unwrap();
    let actor = s
        .join("Shooter".into(), Vec3::new(0., 0.05, 0.), false)
        .unwrap();
    let slot = s.give_item(actor, "v20.weapon.gunitem").unwrap();
    assert_eq!(slot, 3);
    s.command(actor, 1, Command::EquipTool { slot: Some(slot) })
        .unwrap();
    for _ in 0..40 {
        s.step().unwrap();
    }
    s.take_cues();
    let aim = Some(ActionAim {
        yaw: std::f32::consts::FRAC_PI_2,
        pitch: 0.,
    });
    s.command_with_aim(actor, 2, Command::WeaponTrigger { down: true }, aim)
        .unwrap();
    s.command_with_aim(actor, 3, Command::WeaponTrigger { down: false }, aim)
        .unwrap();
    s.step().unwrap();
    assert_eq!(s.weapon_view().projectiles.len(), 1);
    let projectile = s.weapon_view().projectiles.remove(0);
    assert_eq!(projectile.source.0, actor);
    assert!(projectile.velocity.x > 80. && projectile.velocity.z.abs() < 0.01);
    assert!(s.take_cues().iter().any(|c| matches!(&c.kind, CueKind::WeaponSound { profile } if profile.eq_ignore_ascii_case("gunShot1Sound"))));
    s.step().unwrap();
    for _ in 0..12 {
        s.step().unwrap();
    }
    assert_eq!(s.weapon_view().projectiles.len(), 1);
    assert_eq!(s.weapon_view().projectiles[0].id, projectile.id);
    s.disconnect(actor).unwrap();
    assert!(s.weapon_view().projectiles.is_empty());
}

#[test]
fn host_loadouts_preserve_empty_slots_and_reject_live_or_unknown_changes() {
    let mut s = session();
    let mut loadout = bri_sim::session::ToolInventory::default();
    loadout.slots[1] = None;
    loadout.slots[4] = Some("v20.weapon.wanditem".into());
    s.set_spawn_loadout(loadout.clone()).unwrap();
    let mut bad = loadout.clone();
    bad.slots[3] = Some("v20.weapon.unknown".into());
    assert!(s.set_spawn_loadout(bad).is_err());
    let owner = s.join("Loadout".into(), Vec3::Y, false).unwrap();
    assert_eq!(s.tool_inventories()[&owner], loadout);
    assert!(s.set_spawn_loadout(Default::default()).is_err());
}
fn aim(s: &Session, owner: u64) -> MoveInput {
    let p = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap();
    let d = Vec3::new(0.5, 0.1, -3.25) - (Vec3::from(p.feet) + Vec3::Y * 2.4);
    MoveInput {
        yaw: d.x.atan2(-d.z),
        pitch: d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()),
        ..Default::default()
    }
}
#[test]
fn build_load_preflights_every_definition_and_preserves_existing_players() {
    use bri_world::{Brick, ContentRef, build::SavedBuild};
    let mut s = session();
    s.set_ownership_scope("session".into()).unwrap();
    let host = s
        .join("Host".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let guest = s
        .join("Guest".into(), Vec3::new(3.0, 0.05, 0.0), false)
        .unwrap();
    let mut world = World::new("Build".into(), "source".into(), vec![[0.2, 0.3, 0.4, 1.0]]);
    world.bricks.insert(
        1,
        Brick::new(ContentRef::Resolved("plate".into()), [0.5, 0.1, -3.25], 1),
    );
    world.bricks.insert(
        2,
        Brick::new(ContentRef::Resolved("missing".into()), [2.5, 0.1, -3.25], 1),
    );
    world.next_brick_id = 3;
    let mut saved = SavedBuild::capture(&world, None, true, true).unwrap();
    let before = s.snapshot();
    let cmd = |b: SavedBuild| Command::LoadBuild {
        build: Box::new(b),
        ownership: true,
    };
    assert!(s.command(host, 1, cmd(saved.clone())).is_err());
    assert_eq!(
        s.snapshot(),
        before,
        "Partial publication after invalid second brick"
    );
    saved.world.bricks.get_mut(&2).unwrap().definition = ContentRef::Resolved("plate".into());
    assert!(s.command(guest, 1, cmd(saved.clone())).is_err());
    assert_eq!(
        s.command(host, 2, cmd(saved)).unwrap(),
        Reply::Loaded { bricks: 2 }
    );
    let after = s.snapshot();
    assert_eq!(after.players, before.players);
    assert_eq!(after.world.bricks.len(), 2);
    assert_eq!(after.world.bricks[&1].owner, 3);
    assert_eq!(after.world.bricks[&2].owner, 3);
    let newcomer = s
        .join("Newcomer".into(), Vec3::new(6.0, 0.05, 0.0), false)
        .unwrap();
    assert_eq!(newcomer, 4, "Imported owners must stay unclaimed");
    let Reply::Saved(build) = s
        .command(
            guest,
            2,
            Command::SaveBuild {
                events: true,
                ownership: true,
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(build.world.bricks, after.world.bricks);
    assert_eq!(build.ownership_scope.as_deref(), Some("session"));
    for _ in 0..30 {
        s.step().unwrap();
    }
    assert_eq!(s.snapshot().players.len(), 3);
}
#[test]
fn two_players_build_edit_and_late_join_share_authoritative_state() {
    let mut s = session();
    let a = s
        .join("Maxwell".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let b = s
        .join("Guest".into(), Vec3::new(3.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..60 {
        s.step().unwrap();
    }
    let Reply::Planted(id) = s
        .command(
            a,
            1,
            Command::Plant {
                definition: "plate".into(),
                position: [0.5, 0.1, -3.25],
                quarter_turns: 0,
                color: 0,
            },
        )
        .unwrap()
    else {
        panic!()
    };
    let a_look = aim(&s, a);
    let b_look = aim(&s, b);
    s.command(a, 2, Command::Move(a_look)).unwrap();
    s.command(b, 1, Command::Move(b_look)).unwrap();
    s.step().unwrap();
    let before = s.simulation().state().clone();
    assert!(
        s.command(
            b,
            2,
            Command::Edit {
                brick: id,
                edit: Edit::Action(Action::Color(1))
            }
        )
        .unwrap_err()
        .to_string()
        .contains("denied")
    );
    assert_eq!(*s.simulation().state(), before);
    s.command(
        a,
        3,
        Command::Edit {
            brick: id,
            edit: Edit::Action(Action::Color(1)),
        },
    )
    .unwrap();
    s.command(a, 4, Command::Chat("Hello".into())).unwrap();
    let c = s
        .join("Late join".into(), Vec3::new(-3.0, 0.05, 0.0), false)
        .unwrap();
    let late: Snapshot =
        serde_json::from_slice(&serde_json::to_vec(&s.snapshot()).unwrap()).unwrap();
    assert_eq!(late, s.snapshot());
    assert_eq!(late.world.bricks[&id].color, 1);
    assert_eq!(late.players.len(), 3);
    assert_eq!(late.chat[0].owner, a);
    assert_eq!(late.names[&c], "Late join");
    assert!(s.command(a, 3, Command::Remove { brick: id }).is_err());
    s.equip_tool(a, Some(0)).unwrap();
    s.command(a, 5, Command::Remove { brick: id }).unwrap();
    assert!(s.snapshot().world.bricks.is_empty());
    s.disconnect(b).unwrap();
    assert!(
        s.command(b, 3, Command::Chat("stale connection".into()))
            .is_err()
    );
    let rejoin = s
        .join("Guest".into(), Vec3::new(3.0, 0.05, 0.0), false)
        .unwrap();
    assert_ne!(rejoin, b);
}
#[test]
fn packet_frequency_cannot_advance_time_or_forge_positions_and_authority() {
    let mut s = session();
    let a = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..60 {
        s.step().unwrap();
    }
    let before = s.snapshot().players[0].clone();
    for seq in 1..=240 {
        s.command(
            a,
            seq,
            Command::Move(MoveInput {
                forward: 1.0,
                ..Default::default()
            }),
        )
        .unwrap();
    }
    assert_eq!(s.snapshot().players[0], before);
    assert!(
        s.command(a, 241, Command::Move(MoveInput::default()))
            .is_err()
    );
    for _ in 0..180 {
        s.step().unwrap();
    }
    let after = &s.snapshot().players[0];
    assert!(after.feet[2] < -2.0 && after.feet[2] > -5.0);
    assert!(Vec3::from(after.velocity).length() < 0.01);
    assert!(serde_json::from_str::<Command>(r#"{"kind":"move","value":{"forward":0,"right":0,"yaw":0,"pitch":0,"jump":false,"crouch":false,"jet":false,"position":[0,9999,0]}}"#).is_err());
    assert!(serde_json::from_str::<Command>(r#"{"kind":"plant","value":{"definition":"plate","position":[0.5,0.1,-3.25],"quarter_turns":0,"color":0,"owner":1,"administrator":true}}"#).is_err());
}
#[test]
fn tool_actions_require_server_eye_visibility_and_chat_is_bounded() {
    let mut s = session();
    let a = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let Reply::Planted(id) = s
        .command(
            a,
            1,
            Command::Plant {
                definition: "plate".into(),
                position: [0.5, 0.1, -3.25],
                quarter_turns: 0,
                color: 0,
            },
        )
        .unwrap()
    else {
        panic!()
    };
    s.equip_tool(a, Some(0)).unwrap();
    assert!(
        s.command(a, 2, Command::Remove { brick: id })
            .unwrap_err()
            .to_string()
            .contains("out of reach or obstructed")
    );
    for seq in 3..=6 {
        s.command(a, seq, Command::Chat("hello".into())).unwrap();
    }
    assert!(s.command(a, 7, Command::Chat("too many".into())).is_err());
    assert_eq!(s.snapshot().chat.len(), 4);
    let before = s.snapshot().world;
    assert!(
        s.command(
            a,
            8,
            Command::Plant {
                definition: "plate".into(),
                position: [1000.5, 0.1, -3.25],
                quarter_turns: 0,
                color: 0
            }
        )
        .is_err()
    );
    assert_eq!(before, s.snapshot().world);
}

#[test]
fn converging_players_do_not_pass_through_each_other() {
    let mut s = session();
    let a = s
        .join("A".into(), Vec3::new(-2.0, 0.05, 0.0), false)
        .unwrap();
    let b = s
        .join("B".into(), Vec3::new(2.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..60 {
        s.step().unwrap();
    }
    for tick in 0..120 {
        s.command(
            a,
            tick + 1,
            Command::Move(MoveInput {
                right: 1.0,
                ..Default::default()
            }),
        )
        .unwrap();
        s.command(
            b,
            tick + 1,
            Command::Move(MoveInput {
                right: -1.0,
                ..Default::default()
            }),
        )
        .unwrap();
        s.step().unwrap();
        let p = s.snapshot().players;
        assert!(p[1].feet[0] - p[0].feet[0] >= 1.24, "{tick}: {:?}", p);
    }
}

#[test]
fn physical_touch_enters_event_scheduler_once() {
    let mut s = session();
    let a = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..60 {
        s.step().unwrap();
    }
    let Reply::Planted(id) = s
        .command(
            a,
            1,
            Command::Plant {
                definition: "plate".into(),
                position: [0.5, 0.1, -3.25],
                quarter_turns: 0,
                color: 0,
            },
        )
        .unwrap()
    else {
        panic!()
    };
    s.command(a, 2, Command::Move(aim(&s, a))).unwrap();
    s.step().unwrap();
    s.equip_tool(a, Some(1)).unwrap();
    s.command(
        a,
        3,
        Command::Edit {
            brick: id,
            edit: Edit::Events(vec![bri_world::Event {
                enabled: true,
                input: bri_world::Input::Touch,
                delay_ms: 100,
                target: bri_world::Target::ThisBrick,
                action: Action::Color(1),
            }]),
        },
    )
    .unwrap();
    s.join("Visitor".into(), Vec3::new(0.5, 0.25, -3.25), false)
        .unwrap();
    for _ in 0..120 {
        s.step().unwrap();
    }
    assert_eq!(s.simulation().state().bricks[&id].color, 1);
    assert!(s.simulation().state().pending.is_empty());
    assert_eq!(s.simulation().state().next_event_order, 2);
}
