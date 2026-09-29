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
    // Password login needs a durable identity (failed guesses follow it).
    let owner = s
        .join_verified(
            "Player".into(),
            Vec3::Y,
            false,
            Some(bri_admin::Principal([7; 32])),
        )
        .unwrap();
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
    assert_eq!(
        s.take_admin_disconnect_message(target),
        "You were banned from this server permanently. Reason: fixture"
    );
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
    // The press is held (v20's move trigger) but nothing in hand fires.
    s.command(owner, 4, Command::WeaponTrigger { down: true })
        .unwrap();
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
fn build_load_keeps_unknown_bricks_aside_and_preserves_existing_players() {
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
    let saved = SavedBuild::capture(&world, true, true).unwrap();
    let before = s.snapshot();
    let cmd = |b: SavedBuild| Command::LoadBuild {
        build: Box::new(b),
        ownership: true,
    };
    assert!(s.command(guest, 1, cmd(saved.clone())).is_err());
    assert_eq!(s.snapshot(), before, "Only the host may load");
    // The brick with no definition here is kept aside; the rest loads.
    assert_eq!(
        s.command(host, 1, cmd(saved)).unwrap(),
        Reply::Loaded { bricks: 1 }
    );
    assert_eq!(s.snapshot().players, before.players);
    s.step().unwrap();
    assert!(!s.build_loading(), "Small saves finish in one batch");
    let after = s.snapshot();
    assert_eq!(after.world.bricks.len(), 1);
    assert_eq!(after.world.bricks[&1].owner, 3);
    assert_eq!(after.world.unloaded.len(), 1);
    assert_eq!(
        after.world.unloaded[0].definition,
        ContentRef::Resolved("missing".into())
    );
    assert_eq!(after.world.unloaded[0].owner, 3);
    assert!(
        s.chat()
            .iter()
            .any(|l| l.text.starts_with("1 bricks were not loaded") && l.text.contains("1 missing")),
        "The load says what it skipped"
    );
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
    assert_eq!(build.world.unloaded, after.world.unloaded, "Saved again");
    assert!(build.world.owners.is_empty(), "No principals built here");
    for _ in 0..30 {
        s.step().unwrap();
    }
    assert_eq!(s.snapshot().players.len(), 3);
}
#[test]
fn leaving_and_rejoining_keeps_the_same_owner_number() {
    use bri_admin::Principal;
    let max = Principal([1; 32]);
    let mut s = session();
    let first = s
        .join_verified("Maxwell".into(), Vec3::Y, false, Some(max))
        .unwrap();
    s.disconnect(first).unwrap();
    // A fresh join (not a resume) by the same principal takes the dropped
    // connection's number back, so its bricks stay editable.
    let again = s
        .join_verified("Maxwell".into(), Vec3::Y, false, Some(max))
        .unwrap();
    assert_eq!(again, first);
    assert!(s.resume(first, Vec3::Y).is_err(), "The dropped connection is replaced");
    // Someone else never gets it.
    s.disconnect(again).unwrap();
    let other = s
        .join_verified("Guest".into(), Vec3::Y, false, Some(Principal([2; 32])))
        .unwrap();
    assert_ne!(other, first);
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
        .contains("needs full trust")
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
        s.command(a, seq, Command::Chat(format!("hello {seq}"))).unwrap();
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
    for i in 0..2100 {
        world.bricks.insert(
            i + 1,
            Brick::new(
                ContentRef::Resolved("plate".into()),
                [0.5 + (i % 60) as f32, 0.1, -3.25 - (i / 60) as f32],
                1,
            ),
        );
    }
    world.next_brick_id = 2101;
    let saved = SavedBuild::capture(&world, false, false).unwrap();
    let cmd = || Command::LoadBuild {
        build: Box::new(saved.clone()),
        ownership: false,
    };
    s.set_load_pace(bri_sim::session::LoadPace::Bricks(1024));
    assert_eq!(s.command(host, 1, cmd()).unwrap(), Reply::Loaded { bricks: 2100 });
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
    assert_eq!(counts, [1024, 2048, 2100], "Bricks arrive batch by batch");
    assert_eq!(tags(&s), [MessageTag::UploadStart, MessageTag::ProcessComplete]);
    let done = s.chat().last().unwrap().text.clone();
    assert!(done.starts_with("2100 / 2100 bricks created in 0:00.02"), "{done}");
}
#[test]
fn loading_over_a_build_skips_overlapping_bricks_like_v20() {
    use bri_world::{Brick, ContentRef, build::SavedBuild};
    let mut s = session();
    let host = s
        .join("Host".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let save = |xs: &[f32]| {
        let mut world = World::new("Build".into(), "source".into(), vec![[0.2, 0.3, 0.4, 1.0]]);
        for (i, x) in xs.iter().enumerate() {
            world.bricks.insert(
                i as u64 + 1,
                Brick::new(ContentRef::Resolved("plate".into()), [*x, 0.1, -3.25], 1),
            );
        }
        world.next_brick_id = xs.len() as u64 + 1;
        SavedBuild::capture(&world, false, false).unwrap()
    };
    let load = |s: &mut Session, build: SavedBuild, seq| {
        s.command(
            host,
            seq,
            Command::LoadBuild {
                build: Box::new(build),
                ownership: false,
            },
        )
        .unwrap();
        while s.build_loading() {
            s.step().unwrap();
        }
        s.chat().last().unwrap().text.clone()
    };
    let first: Vec<f32> = (0..10).map(|i| 0.5 + i as f32).collect();
    let done = load(&mut s, save(&first), 1);
    assert!(done.starts_with("10 / 10 bricks created"), "{done}");
    // Half of the second save lands on the first, and its last brick
    // repeats one of its own. v20 plants each loaded brick and deletes the
    // ones that overlap, reporting them as not created.
    let mut second: Vec<f32> = (5..15).map(|i| 0.5 + i as f32).collect();
    second.push(14.5);
    let done = load(&mut s, save(&second), 2);
    assert!(done.starts_with("5 / 11 bricks created"), "{done}");
    let mut xs: Vec<f32> = s
        .snapshot()
        .world
        .bricks
        .values()
        .map(|b| b.position[0])
        .collect();
    xs.sort_by(f32::total_cmp);
    let expected: Vec<f32> = (0..15).map(|i| 0.5 + i as f32).collect();
    assert_eq!(xs, expected, "One brick in each spot");
}
#[test]
fn a_brick_that_cannot_be_planted_is_skipped_and_the_load_carries_on_like_v20() {
    use bri_world::{Brick, ContentRef, build::SavedBuild};
    let mut s = session();
    let host = s
        .join("Host".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let mut world = World::new("Build".into(), "source".into(), vec![[0.2, 0.3, 0.4, 1.0]]);
    for i in 0..10u64 {
        let mut brick = Brick::new(
            ContentRef::Resolved("plate".into()),
            [0.5 + i as f32, 0.1, -3.25],
            1,
        );
        // Off the stud grid, and a colour its save does not have.
        if i == 3 {
            brick.position[0] += 0.3;
        }
        if i == 6 {
            brick.color = 9;
        }
        world.bricks.insert(i + 1, brick);
    }
    world.next_brick_id = 11;
    s.command(
        host,
        1,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        },
    )
    .unwrap();
    while s.build_loading() {
        s.step().unwrap();
    }
    let done = s.chat().last().unwrap().text.clone();
    assert!(done.starts_with("8 / 10 bricks created"), "{done}");
    assert_eq!(s.snapshot().world.bricks.len(), 8);
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
            target_collision: Default::default(),
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
    // `serverCmdTeamMessageSent` talks before it looks for a team; outside a
    // mini-game it only tells the sender team chat is disabled.
    s.command(a, 2, Command::TeamChat("hi".into())).unwrap();
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

#[test]
#[ignore = "uses converted native weapons pack; headless server only"]
fn native_akimbo_fires_two_bullets_per_click_over_seconds() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/weapons-pack-009/weapons.json");
    let pack = bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap();
    let mut s = session();
    s.set_weapon_pack(pack).unwrap();
    let actor = s
        .join("Akimbo".into(), Vec3::new(0., 0.05, 0.), false)
        .unwrap();
    let slot = s.give_item(actor, "v20.weapon.akimbogunitem").unwrap();
    let mut sequence = 1;
    s.command(actor, sequence, Command::EquipTool { slot: Some(slot) })
        .unwrap();
    let mut moves = 0;
    let mut held = false;
    let mut seen = std::collections::BTreeSet::new();
    // Holds the trigger per `down(tick)` with a live client sending one move per
    // tick, and counts distinct host bullets.
    let mut drive = |s: &mut Session, ticks: usize, down: &dyn Fn(usize) -> bool| {
        for t in 0..ticks {
            if down(t) != held {
                held = down(t);
                sequence += 1;
                s.command(actor, sequence, Command::WeaponTrigger { down: held })
                    .unwrap();
            }
            moves += 1;
            s.movement(actor, moves, MoveInput::default()).unwrap();
            s.step().unwrap();
            // Not the join's spawn effect, itself a projectile.
            seen.extend(s.weapon_view().fired().map(|p| p.id));
        }
        seen.len()
    };
    assert_eq!(drive(&mut s, 60, &|_| false), 0);
    // Held for five seconds: one bullet, plus the left gun's on release.
    assert_eq!(drive(&mut s, 600, &|_| true), 1);
    assert_eq!(drive(&mut s, 240, &|_| false), 2);
    // Four clicks a second for five seconds: exactly two bullets per click.
    assert_eq!(drive(&mut s, 600, &|t| t % 30 < 15), 42);
}

#[test]
fn kicked_and_banned_players_are_told_why_and_for_how_long() {
    use bri_admin::{BanId, BanRecord, DisconnectReason, DurableState, Principal};
    use bri_sim::session::disconnect_message;
    let now = 1_000_000;
    let ban = |id, minutes: Option<u64>, reason: &str| BanRecord {
        id: BanId(id),
        principal: Principal([id as u8; 32]),
        victim_name: "Victim".into(),
        issued_by: "Admin".into(),
        reason: reason.into(),
        created_unix_seconds: now,
        expires_unix_seconds: minutes.map(|m| now + m * 60),
    };
    let durable = DurableState {
        bans: vec![
            ban(1, Some(10), "spam"),
            ban(2, Some(3 * 60), ""),
            ban(3, Some(5 * 24 * 60), "griefing\nthe spawn"),
        ],
        ..Default::default()
    };
    let say = |reason| disconnect_message(&reason, &durable, now);
    assert_eq!(
        say(DisconnectReason::Kicked),
        "You were kicked from the server by an admin."
    );
    assert_eq!(
        say(DisconnectReason::Banned(BanId(1))),
        "You were banned from this server for 10 minutes. Reason: spam"
    );
    assert_eq!(
        say(DisconnectReason::Banned(BanId(2))),
        "You were banned from this server for 3 hours."
    );
    assert_eq!(
        say(DisconnectReason::Banned(BanId(3))),
        "You were banned from this server for 5 days. Reason: griefingthe spawn"
    );
    assert!(say(DisconnectReason::FailedPasswords).contains("wrong admin passwords"));
}

#[test]
fn sitting_is_replicated_state_that_moving_ends() {
    let mut s = session();
    let owner = s
        .join("Sitter".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..60 {
        s.step().unwrap();
    }
    s.command(owner, 1, Command::Emote("sit".into())).unwrap();
    s.step().unwrap();
    // Vitals reach every client, including one who joins after the emote.
    let late = s
        .join("Latecomer".into(), Vec3::new(4.0, 0.05, 0.0), false)
        .unwrap();
    s.step().unwrap();
    assert!(s.vitals()[&owner].sitting);
    assert!(!s.vitals()[&late].sitting);
    walk(&mut s, owner, 1, 30);
    assert!(!s.vitals()[&owner].sitting);
}

#[test]
fn drop_player_at_camera_lands_at_the_camera_like_v20() {
    use bri_admin::{Action, Request};
    use bri_sim::presentation::CueKind;
    use bri_sim::session::{CameraView, ControlObject};
    let mut s = session();
    let admin = s
        .join("Admin".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let guest = s
        .join("Guest".into(), Vec3::new(4.0, 0.05, 0.0), false)
        .unwrap();
    for _ in 0..60 {
        s.step().unwrap();
    }
    let f8 = || Command::Admin(Request::new(Action::DropCameraAtPlayer));
    assert!(s.command(guest, 1, Command::DropPlayerAtCamera(None)).is_err());
    // F8: the camera starts at the eye and everyone else sees its orb there.
    let standing = body(&s, admin);
    // `getEyePoint() - getPosition()` at the body's facing when F7 is
    // pressed: the Eye node's height and its lead ahead of the body.
    let offset = standing.eye(&bri_sim::player::PlayerTuning::default()) - Vec3::from(standing.feet);
    let eye_height = offset.y;
    s.command(admin, 1, f8()).unwrap();
    assert_eq!(s.camera_orbs().len(), 1);
    assert!((s.camera_orbs()[0].1[1] - (standing.feet[1] + eye_height)).abs() < 1e-4);
    let high = CameraView {
        eye: [5.0, 20.0, 3.0],
        yaw: 1.0,
        pitch: 0.3,
    };
    s.camera_report(admin, high).unwrap();
    assert_eq!(s.camera_orbs(), vec![(admin, high.eye)]);
    s.take_cues();
    // F7 in the air: `serverCmdDropPlayerAtCamera` sets the feet to the
    // camera minus that offset, lead included.
    s.command(admin, 2, Command::DropPlayerAtCamera(None)).unwrap();
    let dropped = body(&s, admin);
    assert!(
        Vec3::from(dropped.feet).distance(Vec3::new(5.0, 20.0, 3.0) - offset) < 1e-3,
        "{:?}",
        dropped.feet
    );
    assert!((dropped.yaw - 1.0).abs() < 1e-6);
    assert_eq!(s.vitals()[&admin].control, ControlObject::Player);
    assert!(s.camera_orbs().is_empty(), "the orb goes with the camera");
    assert!(s.take_cues().iter().any(|c| matches!(
        c.kind,
        CueKind::Teleport { actor, player: true, .. } if actor == admin
    )));
    // Reports from the body do not move the camera.
    s.camera_report(admin, CameraView { eye: [50.0; 3], ..high }).unwrap();
    // Closer to the ground than the eye's height: the feet stand on it.
    s.command(admin, 3, f8()).unwrap();
    let low = CameraView {
        eye: [2.0, 1.0, -2.0],
        yaw: -0.5,
        pitch: -0.2,
    };
    // The ray runs from the camera along -offset and the feet go where it
    // meets the ground (y = 0).
    let ground = |s: &Session| {
        let b = body(s, admin);
        let offset = b.eye(&bri_sim::player::PlayerTuning::default()) - Vec3::from(b.feet);
        low.eye() - offset * (low.eye[1] / offset.y)
    };
    let expected = ground(&s);
    s.command(admin, 4, Command::DropPlayerAtCamera(Some(low))).unwrap();
    let landed = body(&s, admin);
    assert!(
        Vec3::from(landed.feet).distance(expected) < 1e-3,
        "{:?} {expected:?}",
        landed.feet
    );
    // F7 without flying goes back to where the camera was left.
    walk(&mut s, admin, 1, 60);
    let expected = ground(&s);
    s.command(admin, 5, Command::DropPlayerAtCamera(None)).unwrap();
    assert!(Vec3::from(body(&s, admin).feet).distance(expected) < 1e-3);
    // A dead administrator respawns at once.
    s.command(admin, 6, Command::Suicide).unwrap();
    assert!(!s.is_alive(admin));
    s.command(admin, 7, Command::DropPlayerAtCamera(None)).unwrap();
    assert!(s.is_alive(admin));
    assert_eq!(s.vitals()[&admin].control, ControlObject::Player);
}

#[test]
fn join_admin_team_chat_and_emote_lines_use_v20_colors() {
    use bri_admin::{Action, BanDuration, ConnectionId, Principal, Request};
    use bri_sim::{presentation::CueKind, session::Notice};

    let mut s = session();
    let host = s
        .join_verified("Host".into(), Vec3::Y, true, Some(Principal([1; 32])))
        .unwrap();
    let bob = s
        .join_verified("Bob".into(), Vec3::new(4., 1., 0.), false, Some(Principal([2; 32])))
        .unwrap();
    let chat = |n: &[(u64, Notice)], to: u64| -> Vec<String> {
        n.iter()
            .filter_map(|(o, n)| match n {
                Notice::Chat(text) if *o == to => Some(text.clone()),
                _ => None,
            })
            .collect()
    };
    let joined = s.take_private_notices();
    assert_eq!(
        chat(&joined, host),
        [
            "\u{E002}Welcome to Blockland Host.",
            "\u{E002}Host has become Super Admin (Host)",
            "\u{E001}Bob connected.",
            "\u{E001}Bob spawned.",
        ]
    );
    assert_eq!(chat(&joined, bob), ["\u{E002}Welcome to Blockland Bob."]);

    // `chatMessageTeam` outside a mini-game.
    s.command(bob, 1, Command::TeamChat("hi".into())).unwrap();
    assert_eq!(
        chat(&s.take_private_notices(), bob),
        ["\u{E005}Team chat disabled - You are not in a mini-game."]
    );

    // `Player::emote`: /bsd's explosion plays at the m.dts eye node.
    s.take_cues();
    s.command(bob, 2, Command::Emote("bsd".into())).unwrap();
    s.command(bob, 3, Command::Emote("hug".into())).unwrap();
    let cues = s.take_cues();
    let bsd = cues
        .iter()
        .find(|c| matches!(&c.kind, CueKind::WeaponEffect { definition, .. } if definition == "BSDExplosion"))
        .expect("BSD explosion cue");
    let feet = s.snapshot().players.iter().find(|p| p.owner == bob).unwrap().feet;
    assert!((bsd.position[1] - feet[1] - 2.156).abs() < 1e-4);
    assert!(cues
        .iter()
        .any(|c| matches!(&c.kind, CueKind::Emote { actor, name } if *actor == bob && name == "hug")));

    let connection = |s: &Session, name: &str| {
        ConnectionId(
            s.admin_state(host)
                .unwrap()
                .players
                .into_iter()
                .find(|p| p.name == name)
                .unwrap()
                .connection,
        )
    };
    let target = connection(&s, "Bob");
    s.command(host, 1, Command::Admin(Request::new(Action::Kick { target })))
        .unwrap();
    assert_eq!(
        chat(&s.take_private_notices(), host),
        ["\u{E003}Host\u{E002} kicked \u{E003}Bob"]
    );
    s.disconnect(bob).unwrap();
    s.take_private_notices();

    let cat = s
        .join_verified("Cat".into(), Vec3::new(8., 1., 0.), false, Some(Principal([3; 32])))
        .unwrap();
    s.take_private_notices();
    let target = connection(&s, "Cat");
    s.command_with_aim_and_admin_persistence(
        host,
        2,
        Command::Admin(Request::new(Action::Ban {
            target,
            duration: BanDuration::Minutes(5),
            reason: "griefing".into(),
        })),
        None,
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(
        chat(&s.take_private_notices(), host),
        ["\u{E003}Host\u{E002} banned \u{E003}Cat\u{E002} (ID: 03030303) for 5 minutes - \u{E002}\"griefing\""]
    );
    let _ = cat;
}

#[test]
#[ignore = "uses converted native weapons pack; headless server only"]
fn deploying_a_brick_swings_the_brick_image_and_puffs_where_it_lands() {
    use bri_sim::{presentation::CueKind, session::BrickHand};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/weapons-pack-009/weapons.json");
    let pack = bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap();
    let mut s = session();
    s.set_weapon_pack(pack).unwrap();
    let a = s.join("Builder".into(), Vec3::Y, false).unwrap();
    s.command(
        a,
        1,
        Command::BrickHand(BrickHand {
            stocked: true,
            equipped: true,
            ghost: false,
        }),
    )
    .unwrap();
    // Look down at the floor a few units ahead.
    for sequence in 1..=30 {
        s.movement(
            a,
            sequence,
            MoveInput {
                pitch: -0.6,
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
    }
    s.take_cues();
    s.command(a, 2, Command::WeaponTrigger { down: true }).unwrap();
    s.command(a, 3, Command::WeaponTrigger { down: false }).unwrap();
    for sequence in 31..=90 {
        s.movement(
            a,
            sequence,
            MoveInput {
                pitch: -0.6,
                ..Default::default()
            },
        )
        .unwrap();
        s.step().unwrap();
    }
    let cues = s.take_cues();
    let effects: Vec<String> = cues
        .iter()
        .filter_map(|c| match &c.kind {
            CueKind::WeaponEffect { definition, .. } => Some(definition.to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    let sequences: Vec<String> = cues
        .iter()
        .filter_map(|c| match &c.kind {
            CueKind::WeaponAnimation { sequence, .. } => Some(sequence.to_ascii_lowercase()),
            _ => None,
        })
        .collect();

    assert!(effects.contains(&"brickdeployexplosion".into()), "{effects:?}");
    assert!(effects.contains(&"bricktrailemitter".into()), "{effects:?}");
    assert!(sequences.contains(&"fire".into()), "{sequences:?}");
}

#[test]
fn duplicate_names_get_numbers_and_live_rename_updates_everywhere() {
    use bri_sim::session::Notice;
    let mut s = session();
    let a = s.join("Blockhead".into(), Vec3::Y, false).unwrap();
    let b = s.join("blockhead".into(), Vec3::new(4., 1., 0.), false).unwrap();
    let c = s.join("Blockhead".into(), Vec3::new(8., 1., 0.), false).unwrap();
    let names = s.names();
    assert_eq!(names[&a], "Blockhead");
    assert_eq!(names[&b], "blockhead 2");
    assert_eq!(names[&c], "Blockhead 3");

    assert!(matches!(
        s.command(b, 1, Command::SetName("  Builder  ".into())).unwrap(),
        Reply::Accepted
    ));
    assert_eq!(s.names()[&b], "Builder");
    assert!(
        s.chat()
            .iter()
            .any(|l| l.owner == 0 && l.text == "blockhead 2 is now known as Builder.")
    );
    // Taking someone else's name is numbered; keeping your own is a no-op.
    s.command(c, 1, Command::SetName("builder".into())).unwrap();
    assert_eq!(s.names()[&c], "builder 2");
    let lines = s.chat().len();
    s.command(c, 2, Command::SetName("builder".into())).unwrap();
    assert_eq!(s.names()[&c], "builder 2");
    assert_eq!(s.chat().len(), lines);
    // A blank name stays the default and an over-long one is shortened,
    // with a note to that player, instead of being refused.
    s.command(a, 1, Command::SetName("   ".into())).unwrap();
    assert_eq!(s.names()[&a], "Blockhead");
    let _ = s.take_private_notices();
    s.command(a, 2, Command::SetName("é".repeat(30))).unwrap();
    assert_eq!(s.names()[&a], "é".repeat(23));
    assert!(
        s.take_private_notices()
            .iter()
            .any(|(owner, notice)| *owner == a
                && matches!(notice, Notice::Chat(text) if text.contains("shortened")))
    );
    s.command(a, 3, Command::SetName("Blockhead".into()))
        .unwrap();

    // The freed name is available again.
    let d = s.join("Blockhead".into(), Vec3::new(12., 1., 0.), false).unwrap();
    assert_eq!(s.names()[&d], "Blockhead 2");
}

#[test]
fn joins_take_a_cleaned_name_instead_of_being_refused() {
    use bri_sim::session::{Notice, clean_player_name};
    assert_eq!(clean_player_name(""), "Blockhead");
    assert_eq!(clean_player_name(" \u{7}\t "), "Blockhead");
    assert_eq!(clean_player_name("a\nb"), "ab");
    assert_eq!(clean_player_name("  Builder  "), "Builder");
    // v20's `trim(getSubStr(StripMLControlChars(%LANname), 0, 23))`: ML
    // tags go, cut to 23 characters, then trimmed.
    assert_eq!(clean_player_name(&"é".repeat(30)), "é".repeat(23));
    assert_eq!(
        clean_player_name(&format!("{} x", "y".repeat(22))),
        "y".repeat(22)
    );
    assert_eq!(clean_player_name("<color:ff0000>Red<br>"), "Red");
    assert_eq!(clean_player_name("<3 you"), "<3 you");
    assert_eq!(clean_player_name("<b>"), "Blockhead");
    let mut s = session();
    let long = s.join("x".repeat(200), Vec3::Y, false).unwrap();
    assert_eq!(s.names()[&long], "x".repeat(23));
    assert!(
        s.take_private_notices()
            .iter()
            .any(|(owner, notice)| *owner == long
                && matches!(notice, Notice::Chat(text) if text.contains("shortened")))
    );
    let blank = s.join("\n".into(), Vec3::new(4., 1., 0.), false).unwrap();
    assert_eq!(s.names()[&blank], "Blockhead");
}

#[test]
fn clan_tags_are_cleaned_and_carried_on_chat_lines() {
    use bri_sim::session::{Clan, MAX_CLAN_TAG, Notice};
    let mut s = session();
    let host = s.join("Host".into(), Vec3::Y, true).unwrap();
    let guest = s.join("Guest".into(), Vec3::new(4., 1., 0.), false).unwrap();
    // Taken at join (`onConnectRequest`), as a guest with default trust.
    let tags = Clan {
        prefix: "[BL]".into(),
        suffix: "~".into(),
    };
    s.set_clan(guest, &tags).unwrap();
    assert_eq!(s.clans()[&guest], tags);
    assert!(!s.clans().contains_key(&host));
    s.command(guest, 1, Command::Chat("hi".into())).unwrap();
    let line = s.chat().last().cloned().unwrap();
    assert_eq!((line.owner, line.name.as_str()), (guest, "Guest"));
    assert_eq!(line.clan, tags);

    // Avatar screen Done: v20's `trim(getSubStr(StripMLControlChars(..),
    // 0, 4))`, the player told once when more than spaces went.
    let _ = s.take_private_notices();
    let wanted = Clan {
        prefix: format!("\u{e003}\n<color:ff0000>{}", "é".repeat(40)),
        suffix: String::new(),
    };
    s.command(guest, 2, Command::SetClan(wanted.clone())).unwrap();
    let taken = &s.clans()[&guest];
    assert_eq!(taken.prefix, "é".repeat(MAX_CLAN_TAG));
    assert_eq!(taken.suffix, "");
    let told = |s: &mut bri_sim::session::Session| {
        s.take_private_notices().iter().any(|(owner, notice)| {
            *owner == guest && matches!(notice, Notice::Chat(text) if text.contains("clan tags"))
        })
    };
    assert!(told(&mut s));
    s.command(guest, 3, Command::SetClan(wanted)).unwrap();
    assert!(!told(&mut s), "the same tags again change nothing");
    // Surrounding spaces are trimmed quietly, as in v20.
    let spaced = Clan {
        prefix: " [A] ".into(),
        suffix: String::new(),
    };
    s.command(guest, 4, Command::SetClan(spaced)).unwrap();
    assert_eq!(s.clans()[&guest].prefix, "[A]");
    assert!(!told(&mut s));
    // Clearing both tags leaves the name bare.
    s.command(guest, 5, Command::SetClan(Clan::default())).unwrap();
    assert!(!s.clans().contains_key(&guest));
}

/// The client's own click: an aimed trigger, the ghost report that follows
/// it, and the release, all before the next tick.
#[test]
#[ignore = "uses converted native weapons pack; headless server only"]
fn an_aimed_click_with_a_ghost_report_still_fires_the_brick_image() {
    use bri_sim::{
        presentation::CueKind,
        session::{ActionAim, BrickHand, GhostBrick},
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/weapons-pack-009/weapons.json");
    let pack = bri_weapons::Pack::from_json(&std::fs::read(path).unwrap()).unwrap();
    let mut s = session();
    s.set_weapon_pack(pack).unwrap();
    let a = s.join("Builder".into(), Vec3::Y, false).unwrap();
    let hand = |ghost| {
        Command::BrickHand(BrickHand {
            stocked: true,
            equipped: true,
            ghost,
        })
    };
    s.command(a, 1, hand(false)).unwrap();
    let look = MoveInput {
        pitch: -1.0,
        ..Default::default()
    };
    for sequence in 1..=30 {
        s.movement(a, sequence, look).unwrap();
        s.step().unwrap();
    }
    s.take_cues();
    let aim = Some(ActionAim { yaw: 0.0, pitch: -1.0 });
    let replies = [
        s.command_with_aim(a, 2, Command::WeaponTrigger { down: true }, aim),
        s.command_with_aim(a, 3, hand(true), aim),
        s.command_with_aim(
            a,
            4,
            Command::GhostBrick(Some(GhostBrick {
                definition: "plate".into(),
                position: [0.0, 0.1, -1.5],
                quarter_turns: 0,
                color: 0,
                print: None,
            })),
            aim,
        ),
        s.command_with_aim(a, 5, Command::WeaponTrigger { down: false }, aim),
    ];
    println!("replies {replies:?}");
    let mut states = Vec::new();
    for sequence in 31..=60 {
        s.movement(a, sequence, look).unwrap();
        s.step().unwrap();
        let view = s.weapon_view();
        let state: Vec<_> = view.images.get(&a).into_iter().flatten().map(|i| i.state.clone()).collect();
        if states.last() != Some(&state) {
            println!("tick {sequence}: {state:?} projectiles {}", view.projectiles.len());
            states.push(state);
        }
    }
    let cues = s.take_cues();
    let effects: Vec<String> = cues
        .iter()
        .filter_map(|c| match &c.kind {
            CueKind::WeaponEffect { definition, .. } => Some(definition.to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    println!("effects {effects:?}\nnotices {:?}", s.take_notices());
    assert!(effects.contains(&"bricktrailemitter".into()), "{effects:?}");
    assert!(effects.contains(&"brickdeployexplosion".into()), "{effects:?}");
}
