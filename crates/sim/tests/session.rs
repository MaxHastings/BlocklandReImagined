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
use bri_world::{EventRow, EventTarget, EventValue, World, authority::Edit};
use glam::Vec3;
use rapier3d::prelude::*;
fn session() -> Session {
    session_on("test")
}
fn session_on(map_id: &str) -> Session {
    session_with(World::new(
        "Session".into(),
        map_id.into(),
        vec![[1.0; 4], [0.0; 4]],
    ))
}
fn session_with(world: World) -> Session {
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
                special: Default::default(),
            },
        )]
        .into(),
    };
    Session::new(
        Simulation::new(
            world,
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
    // v20 allows a second copy; only a full inventory refuses.
    assert_eq!(s.give_item(a, wand).unwrap(), 4);
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

    assert!(
        s.join_verified("Guest".into(), Vec3::splat(f32::NAN), false, principal)
            .is_err()
    );
    let guest = s
        .join_verified("Guest".into(), Vec3::new(5.0, 1.0, 0.0), false, principal)
        .unwrap();
    let rows = s.admin_state(host).unwrap().players;
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .any(|row| row.name == "Host" && row.connection == 1)
    );
    assert!(
        rows.iter()
            .any(|row| row.name == "Guest" && row.connection == 3)
    );

    s.disconnect(guest).unwrap();
    assert!(
        s.resume_verified(guest, Vec3::splat(f32::NAN), false, principal)
            .is_err()
    );
    s.resume_verified(guest, Vec3::new(9.0, 1.0, 0.0), false, principal)
        .unwrap();
    let rows = s.admin_state(host).unwrap().players;
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .any(|row| row.name == "Host" && row.connection == 1)
    );
    assert!(
        rows.iter()
            .any(|row| row.name == "Guest" && row.connection == 5)
    );
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
    assert!(
        s.set_admin_passwords(
            Secret::new("replacement".into()).unwrap(),
            Secret::new(String::new()).unwrap(),
        )
        .is_err()
    );

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

    assert!(
        s.command_with_aim_and_admin_persistence(host, 1, ban.clone(), None, |_| {
            anyhow::bail!("disk full")
        })
        .is_err()
    );
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
    assert!(
        s.command_with_aim_and_admin_persistence(host, 3, unban, None, |_| {
            anyhow::bail!("disk full")
        })
        .is_err()
    );
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
        .join("../../content/weapons-pack-009/weapons.json");
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
        .join("../../content/weapons-pack-009/weapons.json");
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
    let mut saved = SavedBuild::capture(&world, true, true).unwrap();
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
    assert_eq!(s.snapshot().players, before.players);
    s.step().unwrap();
    assert!(!s.build_loading(), "Small saves finish in one batch");
    let after = s.snapshot();
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
    assert!(build.world.owners.is_empty(), "No principals built here");
    for _ in 0..30 {
        s.step().unwrap();
    }
    assert_eq!(s.snapshot().players.len(), 3);
}
#[test]
fn returning_players_get_their_bricks_back_after_a_restart() {
    use bri_admin::Principal;
    use bri_world::{Brick, ContentRef, OwnerRecord, build::SavedBuild};
    let (max, guest) = (Principal([1; 32]), Principal([2; 32]));
    let mut world = World::new("Saved".into(), "test".into(), vec![[1.0; 4]]);
    world.bricks.insert(
        1,
        Brick::new(ContentRef::Resolved("plate".into()), [0.5, 0.1, -3.25], 7),
    );
    world.next_brick_id = 2;
    world.owners.insert(7, OwnerRecord::new(max.0, "Maxwell".into()));
    // The server restarts on the saved world.
    let mut s = session_with(world);
    let host = s.join("Host".into(), Vec3::Y, true).unwrap();
    assert_eq!(host, 8, "Recorded numbers are never handed to anyone else");
    let back = s
        .join_verified("Maxwell".into(), Vec3::new(3.0, 1.0, 0.0), false, Some(max))
        .unwrap();
    assert_eq!(back, 7, "A returning principal owns their bricks again");
    let other = s
        .join_verified("Guest".into(), Vec3::new(6.0, 1.0, 0.0), false, Some(guest))
        .unwrap();
    assert_eq!(other, 9);
    assert_eq!(s.snapshot().world.owners[&9].name, "Guest");
    // A second connection of a principal already in the game gets its own
    // number; the world keeps the first.
    let twice = s
        .join_verified("Maxwell".into(), Vec3::new(9.0, 1.0, 0.0), false, Some(max))
        .unwrap();
    assert_eq!(twice, 10);
    assert_eq!(s.snapshot().world.owner_of(&"01".repeat(32)), Some(7));

    // Saving with ownership and loading on another server keeps the builder.
    let Reply::Saved(build) = s
        .command(
            host,
            1,
            Command::SaveBuild {
                events: true,
                ownership: true,
            },
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(build.world.owners.len(), 1, "Only owners with saved bricks");
    let mut fresh = session();
    let loader = fresh.join("Host".into(), Vec3::Y, true).unwrap();
    let saved: SavedBuild = *build;
    assert_eq!(
        fresh
            .command(
                loader,
                1,
                Command::LoadBuild {
                    build: Box::new(saved),
                    ownership: true,
                },
            )
            .unwrap(),
        Reply::Loaded { bricks: 1 }
    );
    fresh.step().unwrap();
    let returning = fresh
        .join_verified("Maxwell".into(), Vec3::new(3.0, 1.0, 0.0), false, Some(max))
        .unwrap();
    let world = fresh.snapshot().world;
    assert_eq!(world.bricks.values().next().unwrap().owner, returning);
    assert_eq!(world.owners[&returning].principal, "01".repeat(32));
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
    s.movement(a, 1, a_look).unwrap();
    s.movement(b, 1, b_look).unwrap();
    s.step().unwrap();
    let before = s.simulation().state().clone();
    assert!(
        s.edit_brick(b, id, Edit::Color(1))
        .unwrap_err()
        .to_string()
        .contains("denied")
    );
    assert_eq!(*s.simulation().state(), before);
    s.edit_brick(a, id, Edit::Color(1)).unwrap();
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
    let mut accepted = 0;
    for seq in 1..=240 {
        if s.movement(
            a,
            seq,
            MoveInput {
                forward: 1.0,
                ..Default::default()
            },
        )
        .is_ok()
        {
            accepted += 1;
        }
    }
    // Queuing inputs never advances time; a flood exhausts the input budget.
    assert_eq!(s.snapshot().players[0], before);
    assert!(accepted <= 48, "{accepted}");
    for _ in 0..180 {
        s.step().unwrap();
    }
    let after = &s.snapshot().players[0];
    assert!(after.feet[2] < -1.0 && after.feet[2] > -5.0, "{after:?}");
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
    let Reply::Planted(_id) = s
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
        s.movement(
            a,
            tick + 1,
            MoveInput {
                right: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
        s.movement(
            b,
            tick + 1,
            MoveInput {
                right: -1.0,
                ..Default::default()
            },
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
    s.set_event_catalog(bri_events::testing::catalog(), Vec::new())
        .unwrap();
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
    s.movement(a, 1, aim(&s, a)).unwrap();
    s.step().unwrap();
    s.equip_tool(a, Some(1)).unwrap();
    s.edit_brick(a, id, Edit::Events(vec![EventRow {
                preserved: None,
                enabled: true,
                input: "onPlayerTouch".into(),
                delay_ms: 100,
                target: EventTarget::Slot(bri_events::Slot::SelfBrick),
                output: "setColor".into(),
                params: vec![EventValue::Color(1)],
            }])).unwrap();
    s.join("Visitor".into(), Vec3::new(0.5, 0.25, -3.25), false)
        .unwrap();
    for _ in 0..120 {
        s.step().unwrap();
    }
    assert_eq!(s.simulation().state().bricks[&id].color, 1);
}
fn body(s: &Session, owner: u64) -> bri_sim::player::PlayerState {
    s.snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap()
}
fn walk(s: &mut Session, owner: u64, from: u64, ticks: u64) {
    for seq in from..from + ticks {
        s.movement(
            owner,
            seq,
            MoveInput {
                forward: 1.0,
                yaw: 1.3,
                pitch: -0.4,
                jump: true,
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
    }
}
#[test]
fn camera_control_parks_the_body_until_control_returns() {
    use bri_admin::{Action, Request};
    use bri_sim::session::ControlObject;
    let mut s = session();
    let admin = s
        .join("Admin".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    for _ in 0..60 {
        s.step().unwrap();
    }
    let parked = body(&s, admin);
    let camera = Command::Admin(Request::new(Action::DropCameraAtPlayer));
    s.command(admin, 1, camera).unwrap();
    assert_eq!(s.vitals()[&admin].control, ControlObject::Camera);
    walk(&mut s, admin, 1, 120);
    let still = body(&s, admin);
    assert_eq!(
        (still.feet, still.yaw, still.pitch),
        (parked.feet, parked.yaw, parked.pitch)
    );
    // Moves are still consumed and acknowledged so prediction stays in step.
    assert_eq!(
        s.motion_states()
            .iter()
            .find(|(p, _)| p.owner == admin)
            .unwrap()
            .1,
        120
    );
    s.command(admin, 2, Command::ControlPlayer).unwrap();
    assert_eq!(s.vitals()[&admin].control, ControlObject::Player);
    walk(&mut s, admin, 121, 60);
    let moved = body(&s, admin);
    assert!(Vec3::from(moved.feet).distance(Vec3::from(parked.feet)) > 1.0);
    assert!((moved.yaw - 1.3).abs() < 1e-4);
}
#[test]
fn spy_is_admin_only_follows_its_target_and_ends_when_they_leave() {
    use bri_admin::{Action, ConnectionId, Request};
    use bri_sim::session::ControlObject;
    let mut s = session();
    let admin = s
        .join("Admin".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let guest = s
        .join("Guest".into(), Vec3::new(4.0, 0.05, 0.0), false)
        .unwrap();
    let connection = |s: &Session, name: &str| {
        ConnectionId(
            s.admin_state(admin)
                .unwrap()
                .players
                .iter()
                .find(|p| p.name == name)
                .unwrap()
                .connection,
        )
    };
    let spy = |target| Command::Admin(Request::new(Action::Spy { target }));
    assert!(s.command(guest, 1, spy(connection(&s, "Admin"))).is_err());
    assert_eq!(s.vitals()[&guest].control, ControlObject::Player);
    s.command(admin, 1, spy(connection(&s, "Guest"))).unwrap();
    assert_eq!(s.vitals()[&admin].control, ControlObject::Spy(guest));
    assert!(s.command(admin, 2, spy(connection(&s, "Admin"))).is_err());
    for _ in 0..60 {
        s.step().unwrap();
    }
    let parked = body(&s, admin);
    walk(&mut s, admin, 1, 60);
    assert_eq!(body(&s, admin).feet, parked.feet);
    s.disconnect(guest).unwrap();
    assert_eq!(s.vitals()[&admin].control, ControlObject::Player);
}
#[test]
fn death_hands_control_to_the_corpse_camera_until_respawn() {
    use bri_sim::session::ControlObject;
    let mut s = session();
    let owner = s
        .join("Player".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..60 {
        s.step().unwrap();
    }
    s.command(owner, 1, Command::Suicide).unwrap();
    assert_eq!(s.vitals()[&owner].control, ControlObject::Corpse);
    let corpse = body(&s, owner);
    walk(&mut s, owner, 1, 60);
    assert_eq!(body(&s, owner).yaw, corpse.yaw);
    // Leaving a camera while dead lands back on the corpse camera.
    s.command(owner, 2, Command::ControlPlayer).unwrap();
    assert_eq!(s.vitals()[&owner].control, ControlObject::Corpse);
    for _ in 0..600 {
        s.step().unwrap();
    }
    s.command(owner, 3, Command::Respawn).unwrap();
    assert_eq!(s.vitals()[&owner].control, ControlObject::Player);
}
#[test]
fn saves_stream_in_batches_with_v20_load_messages() {
    use bri_sim::session::MessageTag;
    use bri_world::{Brick, ContentRef, build::SavedBuild};
    let mut s = session();
    let host = s
        .join("Host".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let mut world = World::new("Build".into(), "source".into(), vec![[0.2, 0.3, 0.4, 1.0]]);
    for i in 0..60 {
        world.bricks.insert(
            i + 1,
            Brick::new(
                ContentRef::Resolved("plate".into()),
                [0.5 + i as f32, 0.1, -3.25],
                1,
            ),
        );
    }
    world.next_brick_id = 61;
    let saved = SavedBuild::capture(&world, false, false).unwrap();
    let cmd = || Command::LoadBuild {
        build: Box::new(saved.clone()),
        ownership: false,
    };
    assert_eq!(s.command(host, 1, cmd()).unwrap(), Reply::Loaded { bricks: 60 });
    assert!(s.command(host, 2, cmd()).is_err(), "One load at a time");
    let tags = |s: &Session| s.chat().iter().filter_map(|l| l.tag).collect::<Vec<_>>();
    assert_eq!(tags(&s), [MessageTag::UploadStart]);
    let mut counts = Vec::new();
    while s.build_loading() {
        s.step().unwrap();
        counts.push(s.snapshot().world.bricks.len());
        assert!(counts.len() < 1000);
    }
    counts.dedup();
    assert_eq!(counts, [25, 50, 60], "Bricks arrive batch by batch");
    assert_eq!(tags(&s), [MessageTag::UploadStart, MessageTag::ProcessComplete]);
    let done = s.chat().last().unwrap().text.clone();
    assert!(done.starts_with("60 / 60 bricks created in 0:00.2"), "{done}");
}
#[test]
fn tutorial_keeps_the_wand_and_cans_for_their_rooms() {
    use bri_sim::tutorial::{MAP_ID, TutorialMap, Zone, ZoneKind};
    let zone = |kind, min: Vec3| Zone {
        kind,
        goal: String::new(),
        bind: String::new(),
        task: String::new(),
        min,
        max: min + Vec3::splat(4.0),
    };
    let tutorial = |at: Vec3| {
        let mut s = session_on(MAP_ID);
        let world = || World::new("Tutorial".into(), MAP_ID.into(), vec![[1.0; 4]]);
        s.set_tutorial(TutorialMap {
            zones: vec![zone(ZoneKind::Wand, at), zone(ZoneKind::Spray, at)],
            look_target: Vec3::ZERO,
            part1: world(),
            part2: world(),
            targets: vec![],
            targets_end_ms: 0,
        })
        .unwrap();
        let owner = s.join("Pupil".into(), Vec3::new(0.0, 0.05, 0.0), true).unwrap();
        for _ in 0..12 {
            s.step().unwrap();
        }
        (s, owner)
    };
    let held = |s: &Session, owner| {
        s.weapon_view()
            .images
            .get(&owner)
            .map(|i| i.iter().map(|m| m.image.clone()).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    // Outside the rooms `/wand` and the spray can do nothing, as in v20.
    let (mut s, owner) = tutorial(Vec3::new(50.0, 0.0, 50.0));
    s.command(owner, 1, Command::Wand).unwrap();
    s.command(owner, 2, Command::UseSprayCan { color: 0 }).unwrap();
    s.command(owner, 3, Command::UseFxCan { fx: 0 }).unwrap();
    assert!(held(&s, owner).is_empty(), "{:?}", held(&s, owner));
    // Inside them the request goes on to mount the image, which this
    // content-free test session does not have.
    let (mut s, owner) = tutorial(Vec3::new(-2.0, -1.0, -2.0));
    for (sequence, command) in [
        (1, Command::Wand),
        (2, Command::UseSprayCan { color: 0 }),
        (3, Command::UseFxCan { fx: 0 }),
    ] {
        let error = s.command(owner, sequence, command).unwrap_err();
        assert!(error.to_string().contains("Unknown image"), "{error:#}");
    }
}

#[test]
#[ignore = "uses converted native weapons pack; headless server only"]
fn bricks_in_hand_mount_the_grey_brick_image_in_the_right_hand() {
    use bri_sim::session::BrickHand;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/weapons-pack-009/weapons.json");
    let pack = bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap();
    let image = &pack.images["v20.image.brickimage"];
    assert!(image.arm_ready && image.color_shift);
    assert_eq!(image.color, [0.647, 0.647, 0.647, 1.0]);
    assert_eq!(image.model, "base/data/shapes/brickWeapon.dts");
    let mut s = session();
    s.set_weapon_pack(pack).unwrap();
    let a = s.join("Builder".into(), Vec3::Y, false).unwrap();
    let held = |s: &Session| {
        s.weapon_view()
            .images
            .get(&a)
            .map(|images| {
                images
                    .iter()
                    .map(|i| (i.image.clone(), i.hand))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let brick = vec![("v20.image.brickimage".to_owned(), 0)];
    let hand = |equipped| {
        Command::BrickHand(BrickHand {
            stocked: true,
            equipped,
            ghost: false,
        })
    };
    // Choosing a brick sends both reports; either order leaves it in hand.
    s.command(a, 1, hand(true)).unwrap();
    s.command(a, 2, Command::EquipTool { slot: None }).unwrap();
    assert_eq!(held(&s), brick);
    // A tool replaces it, and putting bricks away then leaves the tool alone.
    s.command(a, 3, Command::EquipTool { slot: Some(0) }).unwrap();
    s.command(a, 4, hand(false)).unwrap();
    assert_eq!(held(&s)[0].0, "v20.image.hammerimage");
    s.command(a, 5, Command::EquipTool { slot: None }).unwrap();
    s.command(a, 6, hand(true)).unwrap();
    assert_eq!(held(&s), brick);
    // `Armor::onNewDataBlock` swaps in HorseArmor's `horseBrickImage` and back.
    let horse = bri_minigames::Settings {
        player_type: bri_sim::player_types::PlayerType::Horse.id().into(),
        ..Default::default()
    };
    let create = bri_sim::session::MiniGameRequest::Create {
        color: 0,
        settings: horse,
    };
    s.command(a, 7, Command::MiniGame(create)).unwrap();
    s.step().unwrap();
    assert_eq!(held(&s), [("v20.image.horsebrickimage".to_owned(), 0)]);
    let leave = bri_sim::session::MiniGameRequest::Leave;
    s.command(a, 8, Command::MiniGame(leave)).unwrap();
    s.step().unwrap();
    assert_eq!(held(&s), brick);
    s.command(a, 9, hand(false)).unwrap();
    assert!(held(&s).is_empty());
}

#[test]
fn builder_animations_play_on_thread_three_and_bricks_raise_the_arm() {
    use bri_sim::{
        presentation::CueKind,
        session::{BrickHand, BuildGesture, ToolAction},
    };
    let mut s = session();
    let a = s
        .join("Maxwell".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..60 {
        s.step().unwrap();
    }
    s.take_cues();
    let mut seq = 0;
    let mut run = |s: &mut Session, command: Command| {
        seq += 1;
        s.command(a, seq, command).unwrap();
        s.take_cues()
            .into_iter()
            .filter_map(|cue| match cue.kind {
                CueKind::WeaponAnimation {
                    actor,
                    thread: 3,
                    sequence,
                    image_hand: None,
                } if actor == a => Some(sequence),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    run(
        &mut s,
        Command::BrickHand(BrickHand {
            stocked: true,
            equipped: true,
            ghost: true,
        }),
    );
    assert_eq!(
        run(&mut s, Command::BuildGesture(BuildGesture::ShiftTowards)),
        ["shiftTO"]
    );
    assert_eq!(
        run(&mut s, Command::BuildGesture(BuildGesture::RotateCcw)),
        ["rotCCW"]
    );
    let plant = Command::Plant {
        definition: "plate".into(),
        position: [0.5, 0.1, -3.25],
        quarter_turns: 0,
        color: 0,
    };
    assert_eq!(run(&mut s, plant), ["plant"]);
    assert_eq!(run(&mut s, Command::Tool(ToolAction::UndoBrick)), ["undo"]);
    // Nothing left to undo: v20 plays nothing.
    assert!(run(&mut s, Command::Tool(ToolAction::UndoBrick)).is_empty());
    // `activateLevel` climbs on clicks within 320 ms; the fifth repeat swings harder.
    let swings: Vec<_> = (0..6).flat_map(|_| run(&mut s, Command::Activate)).collect();
    assert_eq!(
        swings,
        [
            "activate",
            "activate",
            "activate",
            "activate",
            "activate",
            "activate2"
        ]
    );
    for _ in 0..40 {
        s.step().unwrap();
    }
    assert_eq!(run(&mut s, Command::Activate), ["activate"]);
    assert_eq!(BuildGesture::shift(1, -1, 1), Some(BuildGesture::ShiftUp));
    assert_eq!(BuildGesture::shift(1, -1, 0), Some(BuildGesture::ShiftRight));
    assert_eq!(BuildGesture::shift(0, 0, 0), None);
}

#[test]
fn chat_talks_on_thread_three_for_fifty_ms_per_character() {
    use bri_sim::presentation::CueKind;
    let mut s = session();
    let a = s
        .join("Maxwell".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..60 {
        s.step().unwrap();
    }
    s.take_cues();
    let talk = |s: &mut Session| {
        s.take_cues()
            .into_iter()
            .filter_map(|cue| match cue.kind {
                CueKind::WeaponAnimation {
                    actor,
                    thread: 3,
                    sequence,
                    ..
                } if actor == a => Some(sequence),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    // Ten characters: 500 ms, 60 ticks.
    s.command(a, 1, Command::Chat("hello blox".into())).unwrap();
    assert_eq!(talk(&mut s), ["talk"]);
    for _ in 0..60 {
        s.step().unwrap();
    }
    assert!(talk(&mut s).is_empty());
    s.step().unwrap();
    assert_eq!(talk(&mut s), ["root"]);
    // `serverCmdTeamMessageSent` talks before it looks for a team.
    assert!(s.command(a, 2, Command::TeamChat("hi".into())).is_err());
    assert_eq!(talk(&mut s), ["talk"]);
    for _ in 0..13 {
        s.step().unwrap();
    }
    assert_eq!(talk(&mut s), ["root"]);
}
#[test]
fn admin_fetch_find_warp_and_time_scale_follow_v20() {
    use bri_admin::{Action, ConnectionId, Request};
    let mut s = session();
    let admin = s
        .join("Admin".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let guest = s
        .join("Guest".into(), Vec3::new(20.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..60 {
        s.step().unwrap();
    }
    let connection = |s: &Session, name: &str| {
        ConnectionId(
            s.admin_state(admin)
                .unwrap()
                .players
                .iter()
                .find(|p| p.name == name)
                .unwrap()
                .connection,
        )
    };
    let admin_cmd = |action| Command::Admin(Request::new(action));
    let target = connection(&s, "Admin");
    assert!(s.command(guest, 1, admin_cmd(Action::Fetch { target })).is_err());
    assert!(s.command(guest, 2, admin_cmd(Action::TimeScale { scale: 0.5 })).is_err());
    let near = |a: [f32; 3], b: [f32; 3]| Vec3::from(a).distance(Vec3::from(b)) < 0.2;

    let target = connection(&s, "Guest");
    s.command(admin, 1, admin_cmd(Action::Fetch { target })).unwrap();
    assert!(near(body(&s, guest).feet, body(&s, admin).feet));

    s.command(guest, 3, Command::Suicide).unwrap();
    assert!(s.command(admin, 2, admin_cmd(Action::Find { target })).is_err());

    // Look down at the floor ahead and warp onto it.
    let before = body(&s, admin);
    s.movement(
        admin,
        1,
        MoveInput {
            yaw: 0.0,
            pitch: -0.5,
            ..Default::default()
        },
    )
    .unwrap();
    s.step().unwrap();
    s.command(admin, 3, admin_cmd(Action::Warp)).unwrap();
    let after = body(&s, admin);
    let moved = Vec3::from(after.feet) - Vec3::from(before.feet);
    assert!(moved.length() > 1.0, "{moved}");
    assert!(after.feet[1].abs() < 0.2);

    s.command(admin, 4, admin_cmd(Action::TimeScale { scale: 5.0 })).unwrap();
    assert_eq!(s.time_scale(), 2.0);
    s.command(admin, 5, admin_cmd(Action::TimeScale { scale: 0.5 })).unwrap();
    assert_eq!(s.time_scale(), 0.5);
    assert!(s.chat().iter().any(|l| l.text == "Admin changed the timescale to 0.5"));
}
#[test]
fn trust_invites_uploads_demotion_and_lan_follow_v20() {
    use bri_admin::Principal;
    use bri_sim::session::{Notice, TrustEntry, TrustLevel};
    let mut s = session();
    s.set_lan_host(false);
    let spawn = Vec3::new(0.0, 0.05, 0.0);
    let a = s.join_verified("Ann".into(), spawn - Vec3::X * 4.0, false, Some(Principal([1; 32]))).unwrap();
    let b = s.join_verified("Bob".into(), spawn, false, Some(Principal([2; 32]))).unwrap();
    let notices = |s: &mut Session| s.take_private_notices();
    let level = |n: &[(u64, Notice)], viewer: u64, other: u64| {
        n.iter()
            .rev()
            .find_map(|(o, n)| match n {
                Notice::PlayerTrust(rows) if *o == viewer => Some(rows[&other].level),
                _ => None,
            })
    };
    let first = notices(&mut s);
    assert!(first.iter().any(|(o, n)| *o == a && *n == Notice::Chat("\u{E001}Bob connected.".into())));
    assert_eq!(level(&first, a, b), Some(TrustLevel::None));
    assert_eq!(level(&first, a, a), Some(TrustLevel::You));

    s.command(a, 1, Command::TrustInvite { target: b, level: 2 }).unwrap();
    let invited = notices(&mut s);
    assert!(invited.iter().any(|(o, n)| *o == b
        && matches!(n, Notice::TrustInvite { from, level: 2, .. } if *from == a)));
    // A second invite while the first is pending is refused with a message.
    s.command(a, 2, Command::TrustInvite { target: b, level: 1 }).unwrap();
    assert!(notices(&mut s).iter().any(|(o, n)| *o == a && matches!(n, Notice::MessageBox { .. })));

    s.command(b, 1, Command::AcceptTrust { from: a }).unwrap();
    let accepted = notices(&mut s);
    assert_eq!(level(&accepted, a, b), Some(TrustLevel::Full));
    assert_eq!(level(&accepted, b, a), Some(TrustLevel::Full));
    assert!(accepted.iter().any(|(o, n)| *o == a
        && matches!(n, Notice::TrustSaved { principal, level: 2, .. } if *principal == [2; 32])));

    s.command(a, 3, Command::DemoteTrust { target: b, level: 1 }).unwrap();
    assert_eq!(level(&notices(&mut s), b, a), Some(TrustLevel::Build));

    // Saved lists: both sides listing each other become mutual trust at the
    // uploader's level.
    let c = s.join_verified("Cat".into(), spawn + Vec3::X * 4.0, false, Some(Principal([3; 32]))).unwrap();
    s.command(c, 1, Command::TrustList(vec![TrustEntry { principal: [1; 32], level: 2 }])).unwrap();
    assert_eq!(level(&notices(&mut s), a, c), Some(TrustLevel::None));
    s.command(a, 4, Command::TrustList(vec![TrustEntry { principal: [3; 32], level: 1 }])).unwrap();
    let uploaded = notices(&mut s);
    assert_eq!(level(&uploaded, a, c), Some(TrustLevel::Build));
    // A's upload replaced its earlier trust with Bob.
    assert_eq!(level(&uploaded, a, b), Some(TrustLevel::None));

    // Ignored invites are refused.
    s.command(c, 2, Command::TrustInvite { target: b, level: 1 }).unwrap();
    s.command(b, 2, Command::IgnoreTrust { from: c }).unwrap();
    notices(&mut s);
    s.command(c, 3, Command::TrustInvite { target: b, level: 1 }).unwrap();
    assert!(!notices(&mut s).iter().any(|(o, n)| *o == b && matches!(n, Notice::TrustInvite { .. })));

    s.set_lan_host(true);
    assert_eq!(level(&notices(&mut s), a, b), Some(TrustLevel::Lan));
    s.disconnect(b).unwrap();
    assert!(notices(&mut s).iter().any(|(o, n)| *o == a && *n == Notice::Chat("\u{E001}Bob has left the game.".into())));
}
