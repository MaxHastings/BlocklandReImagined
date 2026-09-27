use bri_vehicles::*;
use glam::Vec3;
use rapier3d::prelude::*;
fn pack() -> Pack {
    Pack::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../content/vehicles-pack-010/vehicles.json"
    ))
    .unwrap()
}
fn setup() -> (VehiclesWorld, PhysicsWorld) {
    let v = VehiclesWorld::new(pack()).unwrap();
    let mut w = bri_physics::new_world();
    w.insert(
        RigidBodyBuilder::fixed().translation(Vec3::new(0., -0.5, 0.)),
        ColliderBuilder::cuboid(500., 0.5, 500.),
    );
    (v, w)
}
fn spawn(v: &mut VehiclesWorld, w: &mut PhysicsWorld, name: &str, y: f32) {
    v.spawn(
        w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: format!("v20.vehicle.{name}"),
            transform: Transform {
                position: [0., y, 0.],
                ..Default::default()
            },
            spawn_id: Some(SpawnId(4)),
            respawn_ticks: Some(600),
        },
    )
    .unwrap();
    w.detect_collisions(&(), &());
}
fn mount(v: &mut VehiclesWorld, w: &PhysicsWorld, seat: usize) {
    let p = v.snapshot(w).vehicles[0].seats[seat].transform.position;
    v.mount(
        w,
        VehicleId(1),
        seat,
        Occupant {
            id: OccupantId(20 + seat as u64),
            owner: OwnerId(10),
        },
        p,
    )
    .unwrap();
}
fn step(v: &mut VehiclesWorld, w: &mut PhysicsWorld, n: usize, water: Option<f32>) {
    for _ in 0..n {
        v.pre_step(w, |_| water).unwrap();
        w.step();
        v.post_step(w).unwrap();
    }
}
#[test]
fn native_catalog_assets_and_authored_values() {
    let p = pack();
    p.verify_assets(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../content/vehicles-pack-010"
    ))
    .unwrap();
    assert_eq!(p.definitions.len(), 11);
    assert_eq!(p.animation_aliases.len(), 39);
    let jeep = p
        .definitions
        .iter()
        .find(|d| d.datablock == "JeepVehicle")
        .unwrap();
    assert_eq!(jeep.seats.len(), 7);
    assert_eq!(jeep.engine_force, 12000.);
    assert_eq!(jeep.mass, 300.);
    assert_eq!(jeep.wheels.len(), 4);
    let horse = p
        .definitions
        .iter()
        .find(|d| d.datablock == "HorseArmor")
        .unwrap();
    assert_eq!(horse.jump_speed, 17.);
    assert_eq!(horse.seats[0].node, "mount2");
    assert!(
        p.definitions
            .iter()
            .find(|d| d.family == Family::Ball)
            .unwrap()
            .seats
            .is_empty()
    );
}
#[test]
fn seats_authority_and_serialization() {
    let (mut v, mut w) = setup();
    spawn(&mut v, &mut w, "jeepvehicle", 2.);
    mount(&mut v, &w, 0);
    assert!(
        v.set_controls(OwnerId(11), OccupantId(20), Controls::default())
            .is_err()
    );
    mount(&mut v, &w, 1);
    assert!(
        v.set_controls(OwnerId(10), OccupantId(21), Controls::default())
            .is_err()
    );
    assert!(
        v.set_controls(
            OwnerId(10),
            OccupantId(20),
            Controls {
                throttle: f32::NAN,
                ..Default::default()
            }
        )
        .is_err()
    );
    let s = v.snapshot(&w);
    let bytes = serde_json::to_vec(&s).unwrap();
    let r: Snapshot = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(r.vehicles[0].seats.len(), 7);
    v.disconnect(&w, OwnerId(10));
    assert!(
        v.snapshot(&w).vehicles[0]
            .seats
            .iter()
            .all(|s| s.occupant.is_none())
    );
}
#[test]
fn wheels_drive_brake_and_world_wall() {
    for name in ["jeepvehicle", "tankvehicle"] {
        let (mut v, mut w) = setup();
        spawn(&mut v, &mut w, name, 2.);
        mount(&mut v, &w, 0);
        step(&mut v, &mut w, 240, None);
        v.set_controls(
            OwnerId(10),
            OccupantId(20),
            Controls {
                throttle: 1.,
                ..Default::default()
            },
        )
        .unwrap();
        step(&mut v, &mut w, 300, None);
        let s = v.snapshot(&w);
        println!(
            "{name} after drive {:?} {:?}",
            s.vehicles[0].transform.position, s.vehicles[0].velocity
        );
        assert!(s.vehicles[0].transform.position[2] < -3., "must drive -Z");
        assert!(Vec3::from_array(s.vehicles[0].velocity).length() < 35.);
        v.set_controls(
            OwnerId(10),
            OccupantId(20),
            Controls {
                brake: true,
                ..Default::default()
            },
        )
        .unwrap();
        step(&mut v, &mut w, 240, None);
        let slow = v.snapshot(&w).vehicles[0].velocity;
        assert!(
            Vec3::from_array(slow).length() < 3.,
            "brake failed {slow:?}"
        );
    }
}
#[test]
fn horse_run_jump_and_collision() {
    let (mut v, mut w) = setup();
    spawn(&mut v, &mut w, "horsearmor", 0.2);
    mount(&mut v, &w, 0);
    w.insert(
        RigidBodyBuilder::fixed().translation(Vec3::new(0., 2., -9.)),
        ColliderBuilder::cuboid(10., 2., 0.2),
    );
    step(&mut v, &mut w, 120, None);
    v.set_controls(
        OwnerId(10),
        OccupantId(20),
        Controls {
            throttle: 1.,
            ..Default::default()
        },
    )
    .unwrap();
    step(&mut v, &mut w, 180, None);
    let p = v.snapshot(&w).vehicles[0].transform.position;
    println!("horse {p:?}");
    assert!(p[2] < -2. && p[2] > -9.);
    v.set_controls(
        OwnerId(10),
        OccupantId(20),
        Controls {
            jump: true,
            ..Default::default()
        },
    )
    .unwrap();
    step(&mut v, &mut w, 30, None);
    assert!(v.snapshot(&w).vehicles[0].transform.position[1] > 1.);
}
#[test]
fn flight_and_water_families() {
    for name in ["magiccarpetvehicle", "flyingwheeledjeepvehicle"] {
        let (mut v, mut w) = setup();
        spawn(&mut v, &mut w, name, 5.);
        mount(&mut v, &w, 0);
        if name == "flyingwheeledjeepvehicle" {
            v.set_energy(VehicleId(1), 100.).unwrap();
        }
        v.set_controls(
            OwnerId(10),
            OccupantId(20),
            Controls {
                throttle: 0.5,
                vertical: 1.,
                ..Default::default()
            },
        )
        .unwrap();
        step(&mut v, &mut w, 120, None);
        let p = v.snapshot(&w).vehicles[0].transform.position;
        println!("flight {name} {p:?}");
        assert!(p[1] > 1. && p[2] < -1.);
    }
    let (mut v, mut w) = setup();
    spawn(&mut v, &mut w, "rowboatarmor", 3.);
    mount(&mut v, &w, 0);
    v.set_controls(
        OwnerId(10),
        OccupantId(20),
        Controls {
            throttle: 1.,
            ..Default::default()
        },
    )
    .unwrap();
    step(&mut v, &mut w, 360, Some(5.));
    let p = v.snapshot(&w).vehicles[0].transform.position;
    println!("rowboat {p:?}");
    assert!(p[1] > 2. && p[1] < 6. && p[2] < -3.);
}
#[test]
fn cannon_authored_charge_and_cooldown() {
    let (mut v, mut w) = setup();
    spawn(&mut v, &mut w, "cannonturret", 0.1);
    mount(&mut v, &w, 0);
    v.drain_intents();
    v.set_controls(
        OwnerId(10),
        OccupantId(20),
        Controls {
            fire: true,
            ..Default::default()
        },
    )
    .unwrap();
    step(&mut v, &mut w, 49, None);
    assert_eq!(v.snapshot(&w).vehicles[0].charge, 3);
    assert!(
        !v.drain_intents()
            .iter()
            .any(|i| matches!(i, Intent::Fire(_)))
    );
    v.set_controls(OwnerId(10), OccupantId(20), Controls::default())
        .unwrap();
    step(&mut v, &mut w, 1, None);
    let intents = v.drain_intents();
    let fire = intents
        .iter()
        .find_map(|i| {
            if let Intent::Fire(f) = i {
                Some(f)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(fire.projectile, "v20.projectile.cannonballprojectile");
    assert!((Vec3::from_array(fire.velocity).length() - 16.5).abs() < 0.001);
    v.set_controls(
        OwnerId(10),
        OccupantId(20),
        Controls {
            fire: true,
            ..Default::default()
        },
    )
    .unwrap();
    step(&mut v, &mut w, 100, None);
    assert_eq!(v.snapshot(&w).vehicles[0].charge, 0);
}
#[test]
fn tank_gunner_and_turret() {
    for name in ["tankvehicle", "tankturretplayer"] {
        let (mut v, mut w) = setup();
        spawn(&mut v, &mut w, name, 3.);
        let seat = if name == "tankvehicle" { 2 } else { 0 };
        mount(&mut v, &w, seat);
        v.drain_intents();
        v.set_controls(
            OwnerId(10),
            OccupantId(20 + seat as u64),
            Controls {
                fire: true,
                ..Default::default()
            },
        )
        .unwrap();
        step(&mut v, &mut w, 301, None);
        let fires: Vec<_> = v
            .drain_intents()
            .into_iter()
            .filter_map(|i| {
                if let Intent::Fire(f) = i {
                    Some(f)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(fires.len(), 1);
        assert_eq!(fires[0].projectile, "v20.projectile.tankshellprojectile");
        assert!((Vec3::from_array(fires[0].velocity).length() - 140.).abs() < 0.01);
    }
}
#[test]
fn ball_rolls_without_mounts() {
    let (mut v, mut w) = setup();
    spawn(&mut v, &mut w, "ballvehicle", 4.);
    assert!(
        v.mount(
            &w,
            VehicleId(1),
            0,
            Occupant {
                id: OccupantId(20),
                owner: OwnerId(10)
            },
            [0., 4., 0.]
        )
        .is_err()
    );
    let (_, b) = w.bodies.iter_mut().find(|(_, b)| b.is_dynamic()).unwrap();
    b.apply_impulse(Vec3::X * 1000., true);
    step(&mut v, &mut w, 240, None);
    let p = v.snapshot(&w).vehicles[0].transform.position;
    assert!(p[0] > 1. && p[1] > 1.5);
}
#[test]
fn destruction_cleanup_respawn_and_cancel() {
    let (mut v, mut w) = setup();
    spawn(&mut v, &mut w, "jeepvehicle", 2.);
    mount(&mut v, &w, 0);
    v.damage(&w, VehicleId(1), 500., OwnerId(11)).unwrap();
    assert!(!v.snapshot(&w).vehicles[0].destroyed);
    step(&mut v, &mut w, 12, None);
    v.damage(&w, VehicleId(1), 500., OwnerId(11)).unwrap();
    assert!(v.snapshot(&w).vehicles[0].destroyed);
    step(&mut v, &mut w, 480, None);
    assert!(v.snapshot(&w).vehicles.is_empty());
    assert_eq!(w.bodies.len(), 1);
    step(&mut v, &mut w, 120, None);
    let intents = v.drain_intents();
    assert!(intents.iter().any(|i| matches!(
        i,
        Intent::RespawnDue {
            spawn_id: SpawnId(4),
            ..
        }
    )));
    assert!(
        intents
            .iter()
            .any(|i| matches!(i, Intent::Dismounted { forced: true, .. }))
    );
    spawn(&mut v, &mut w, "tankvehicle", 2.);
    v.cancel_spawn(&mut w, SpawnId(4)).unwrap();
    assert_eq!(w.bodies.len(), 1);
}
#[test]
fn blocked_dismount_is_atomic() {
    let (mut v, mut w) = setup();
    spawn(&mut v, &mut w, "jeepvehicle", 2.);
    mount(&mut v, &w, 0);
    w.insert(
        RigidBodyBuilder::fixed().translation(Vec3::new(0., 4., 0.)),
        ColliderBuilder::cuboid(10., 10., 10.),
    );
    w.detect_collisions(&(), &());
    assert!(v.dismount(&w, OwnerId(10), OccupantId(20), false).is_err());
    assert!(v.snapshot(&w).vehicles[0].seats[0].occupant.is_some());
    v.dismount(&w, OwnerId(10), OccupantId(20), true).unwrap();
}

#[test]
fn turret_damage_removes_weapon_and_preserves_hull() {
    let (mut v, mut w) = setup();
    spawn(&mut v, &mut w, "tankvehicle", 3.);
    mount(&mut v, &w, 2);
    let before = v.snapshot(&w).vehicles[0].seats[2].transform.position;
    v.damage_turret(&mut w, VehicleId(1), 250., OwnerId(11))
        .unwrap();
    let snap = v.snapshot(&w);
    assert_eq!(snap.vehicles[0].turret_damage, Some(250.));
    assert_ne!(snap.vehicles[0].seats[2].transform.position, before);
    assert!(!snap.vehicles[0].destroyed);
    v.drain_intents();
    v.set_controls(
        OwnerId(10),
        OccupantId(22),
        Controls {
            fire: true,
            ..Default::default()
        },
    )
    .unwrap();
    step(&mut v, &mut w, 2, None);
    assert!(
        !v.drain_intents()
            .iter()
            .any(|i| matches!(i, Intent::Fire(_)))
    );
}
#[test]
fn original_shapes_collide_tip_and_slope() {
    for name in ["jeepvehicle", "tankvehicle", "ballvehicle"] {
        let (mut v, mut w) = setup();
        spawn(&mut v, &mut w, name, 8.);
        let (_, body) = w.bodies.iter_mut().find(|(_, b)| b.is_dynamic()).unwrap();
        body.set_rotation(glam::Quat::from_rotation_z(0.6), true);
        body.set_linvel(Vec3::new(5., 0., 0.), true);
        w.insert(
            RigidBodyBuilder::fixed()
                .translation(Vec3::new(5., 1., 0.))
                .rotation(Vec3::new(0., 0., 0.2)),
            ColliderBuilder::cuboid(8., 0.2, 8.),
        );
        step(&mut v, &mut w, 480, None);
        let s = &v.snapshot(&w).vehicles[0];
        assert!(
            s.transform
                .position
                .iter()
                .chain(s.velocity.iter())
                .all(|x| x.is_finite())
        );
        assert!(s.transform.position[1] > 0. && s.transform.position[1] < 8.);
        assert_ne!(s.transform.rotation, [0., 0., 0., 1.]);
    }
}
#[test]
fn rejects_invalid_native_data_and_fixed_rate() {
    let mut p = pack();
    p.definitions[0].mass = f32::NAN;
    assert!(p.validate().is_err());
    let mut p = pack();
    p.definitions[0].drag = -1.;
    assert!(p.validate().is_err());
    let mut p = pack();
    p.definitions[0].id = p.definitions[1].id.clone();
    assert!(p.validate().is_err());
    let (mut v, mut w) = setup();
    w.integration_parameters.dt = 1. / 60.;
    assert!(v.pre_step(&mut w, |_| None).is_err());
}
#[test]
fn velocity_transfer_and_runover_intents() {
    let (mut v, mut w) = setup();
    spawn(&mut v, &mut w, "jeepvehicle", 3.);
    mount(&mut v, &w, 0);
    let (_, b) = w.bodies.iter_mut().find(|(_, b)| b.is_dynamic()).unwrap();
    b.set_linvel(Vec3::new(10., 0., 0.), true);
    v.player_contact(&w, VehicleId(1), OccupantId(99), [0.; 3])
        .unwrap();
    assert!(
        v.drain_intents()
            .iter()
            .any(|i| matches!(i,Intent::RunOver{damage,..} if *damage==80.))
    );
    v.dismount(&w, OwnerId(10), OccupantId(20), false).unwrap();
    let intents = v.drain_intents();
    let vel = intents
        .iter()
        .find_map(|i| {
            if let Intent::Dismounted { velocity, .. } = i {
                Some(velocity)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(vel[0], 10.);
    assert!(vel[1] > 2.);
}

#[test]
fn skis_drive_simple_dismount_and_wreck_transition() {
    let (mut v, mut w) = setup();
    spawn(&mut v, &mut w, "skivehicle", 2.);
    mount(&mut v, &w, 0);
    v.set_velocity(&mut w, VehicleId(1), [0., 0., -15.])
        .unwrap();
    v.set_controls(
        OwnerId(10),
        OccupantId(20),
        Controls {
            throttle: 1.,
            ..Default::default()
        },
    )
    .unwrap();
    step(&mut v, &mut w, 180, None);
    let p = v.snapshot(&w).vehicles[0].transform.position;
    assert!(p[2] < -5.);
    v.set_velocity(&mut w, VehicleId(1), [0., 0., -50.])
        .unwrap();
    v.drain_intents();
    v.wreck_skis(&mut w, VehicleId(1)).unwrap();
    let intents = v.drain_intents();
    assert!(matches!(
        intents.last(),
        Some(Intent::TumbleRequested {
            requested_ticks: 792,
            ..
        })
    ));
    assert!(v.snapshot(&w).vehicles.is_empty());
    assert_eq!(w.bodies.len(), 1);
    spawn(&mut v, &mut w, "skivehicle", 2.);
    mount(&mut v, &w, 0);
    v.set_velocity(&mut w, VehicleId(1), [3., 1., 2.]).unwrap();
    v.dismount(&w, OwnerId(10), OccupantId(20), false).unwrap();
    assert!(
        v.drain_intents()
            .iter()
            .any(|i| matches!(i,Intent::Dismounted{velocity,..}if *velocity==[3.,1.,2.]))
    );
    step(&mut v, &mut w, 1, None);
    assert!(v.snapshot(&w).vehicles.is_empty());
}
#[test]
fn tumble_blocks_controls_and_releases_on_water_at_two_seconds() {
    let (mut v, mut w) = setup();
    spawn(&mut v, &mut w, "deathvehicle", 2.);
    mount(&mut v, &w, 0);
    assert!(v.dismount(&w, OwnerId(10), OccupantId(20), false).is_err());
    assert!(
        v.set_controls(OwnerId(10), OccupantId(20), Controls::default())
            .is_err()
    );
    step(&mut v, &mut w, 239, Some(10.));
    assert_eq!(v.snapshot(&w).vehicles.len(), 1);
    step(&mut v, &mut w, 1, Some(100.));
    assert!(v.snapshot(&w).vehicles.is_empty());
    assert_eq!(w.bodies.len(), 1);
}

fn save(v: &mut VehiclesWorld, w: &PhysicsWorld) -> Checkpoint {
    v.drain_intents();
    let c = v.checkpoint(w).unwrap();
    Checkpoint::decode(&c.encode().unwrap()).unwrap()
}
#[test]
fn checkpoint_mid_charge_preserves_edges_and_next_fire() {
    let (mut a, mut aw) = setup();
    spawn(&mut a, &mut aw, "cannonturret", 40.);
    mount(&mut a, &aw, 0);
    a.set_controls(
        OwnerId(10),
        OccupantId(20),
        Controls {
            fire: true,
            ..Default::default()
        },
    )
    .unwrap();
    step(&mut a, &mut aw, 49, None);
    let checkpoint = save(&mut a, &aw);
    let (mut b, mut bw) = setup();
    b.restore_checkpoint(&mut bw, checkpoint, |o, _, _| {
        o.owner == OwnerId(10) && o.id == OccupantId(20)
    })
    .unwrap();
    assert_eq!(
        serde_json::to_value(a.snapshot(&aw)).unwrap(),
        serde_json::to_value(b.snapshot(&bw)).unwrap()
    );
    for v in [&mut a, &mut b] {
        v.set_controls(OwnerId(10), OccupantId(20), Controls::default())
            .unwrap();
    }
    step(&mut a, &mut aw, 1, None);
    step(&mut b, &mut bw, 1, None);
    let ai = a.drain_intents();
    let bi = b.drain_intents();
    assert_eq!(
        serde_json::to_value(ai).unwrap(),
        serde_json::to_value(bi).unwrap()
    );
}
#[test]
fn checkpoint_pending_respawn_and_tumble_keep_deadlines() {
    let (mut a, mut aw) = setup();
    spawn(&mut a, &mut aw, "jeepvehicle", 3.);
    step(&mut a, &mut aw, 12, None);
    a.damage(&aw, VehicleId(1), 1000., OwnerId(11)).unwrap();
    step(&mut a, &mut aw, 480, None);
    let cp = save(&mut a, &aw);
    assert_eq!(cp.pending_respawns.len(), 1);
    let (mut b, mut bw) = setup();
    b.restore_checkpoint(&mut bw, cp, |_, _, _| true).unwrap();
    step(&mut a, &mut aw, 120, None);
    step(&mut b, &mut bw, 120, None);
    assert_eq!(
        serde_json::to_value(a.drain_intents()).unwrap(),
        serde_json::to_value(b.drain_intents()).unwrap()
    );
    let (mut a, mut aw) = setup();
    spawn(&mut a, &mut aw, "deathvehicle", 30.);
    mount(&mut a, &aw, 0);
    step(&mut a, &mut aw, 239, None);
    let cp = save(&mut a, &aw);
    let (mut b, mut bw) = setup();
    b.restore_checkpoint(&mut bw, cp, |_, _, _| true).unwrap();
    step(&mut a, &mut aw, 1, Some(100.));
    step(&mut b, &mut bw, 1, Some(100.));
    assert!(a.snapshot(&aw).vehicles.is_empty() && b.snapshot(&bw).vehicles.is_empty());
    assert_eq!(
        serde_json::to_value(a.drain_intents()).unwrap(),
        serde_json::to_value(b.drain_intents()).unwrap()
    );
}
#[test]
fn checkpoint_rejection_is_atomic_for_shared_world() {
    let (mut v, mut w) = setup();
    spawn(&mut v, &mut w, "tankvehicle", 3.);
    mount(&mut v, &w, 0);
    let cp = save(&mut v, &w);
    let unrelated = w.insert_body(RigidBodyBuilder::dynamic().translation(Vec3::new(90., 9., 0.)));
    let before = serde_json::to_value(v.snapshot(&w)).unwrap();
    let count = w.bodies.len();
    let mut cases = vec![];
    let mut c = cp.clone();
    c.vehicles[0].sleeping = true;
    c.vehicles[0].angular_velocity[0] = 1.;
    cases.push(c);
    let mut c = cp.clone();
    c.vehicles[0].energy_phase = 96;
    cases.push(c);
    let mut c = cp.clone();
    c.vehicles[0].charge = 1;
    cases.push(c);
    let mut c = cp.clone();
    c.vehicles.push(c.vehicles[0].clone());
    cases.push(c);
    let mut c = cp.clone();
    c.vehicles[0].born_tick = c.tick + 1;
    cases.push(c);
    let mut c = cp.clone();
    c.content_fingerprint = "bad".into();
    cases.push(c);
    let mut c = cp.clone();
    c.vehicles[0].velocity[0] = f32::NAN;
    cases.push(c);
    let mut c = cp.clone();
    c.vehicles[0].spawn.scale = 0.;
    cases.push(c);
    for c in cases {
        assert!(v.restore_checkpoint(&mut w, c, |_, _, _| true).is_err());
        assert_eq!(w.bodies.len(), count);
        assert_eq!(w.bodies[unrelated].translation(), Vec3::new(90., 9., 0.));
        assert_eq!(serde_json::to_value(v.snapshot(&w)).unwrap(), before);
    }
    assert!(
        v.restore_checkpoint(&mut w, cp.clone(), |_, _, _| false)
            .is_err()
    );
    assert_eq!(w.bodies.len(), count);
    v.restore_checkpoint(&mut w, cp, |_, _, _| true).unwrap();
    assert_eq!(w.bodies.len(), count);
    assert!(w.bodies.get(unrelated).is_some());
}
#[test]
fn scaled_geometry_mounts_wheels_and_restored_motion_agree() {
    let (mut v, mut w) = setup();
    for (id, scale) in [(1, 1.), (2, 2.)] {
        v.spawn(
            &mut w,
            Spawn {
                id: VehicleId(id),
                owner: OwnerId(10),
                definition: "v20.vehicle.jeepvehicle".into(),
                transform: Transform {
                    position: [0., 30., 0.],
                    ..Default::default()
                },
                spawn_id: None,
                respawn_ticks: None,
                scale,
            },
        )
        .unwrap();
    }
    w.detect_collisions(&(), &());
    let s = v.snapshot(&w);
    let a = Vec3::from_array(s.vehicles[0].seats[0].transform.position) - Vec3::new(0., 30., 0.);
    let b = Vec3::from_array(s.vehicles[1].seats[0].transform.position) - Vec3::new(0., 30., 0.);
    assert!((b - a * 2.).length() < 0.0001);
    let mut widths = vec![];
    for (h, c) in w.colliders.iter() {
        if let Some((id, _)) = v.classify_collider(h) {
            let bb = c.compute_aabb();
            widths.push((id.0, bb.maxs.x - bb.mins.x));
        }
    }
    widths.sort_by_key(|x| x.0);
    assert!((widths[1].1 - widths[0].1 * 2.).abs() < 0.001);
    v.set_velocity(&mut w, VehicleId(2), [4., 5., 6.]).unwrap();
    let cp = save(&mut v, &w);
    assert_eq!(cp.vehicles[1].spawn.scale, 2.);
    let (mut restored, mut rw) = setup();
    restored
        .restore_checkpoint(&mut rw, cp, |_, _, _| true)
        .unwrap();
    assert_eq!(
        serde_json::to_value(v.snapshot(&w)).unwrap(),
        serde_json::to_value(restored.snapshot(&rw)).unwrap()
    );
}
#[test]
fn checkpoint_requires_completed_tick_and_consumed_intents() {
    let (mut v, mut w) = setup();
    spawn(&mut v, &mut w, "jeepvehicle", 3.);
    mount(&mut v, &w, 0);
    assert!(v.checkpoint(&w).is_err());
    v.drain_intents();
    v.pre_step(&mut w, |_| None).unwrap();
    assert!(v.checkpoint(&w).is_err());
    w.step();
    v.post_step(&mut w).unwrap();
    v.drain_intents();
    assert!(v.checkpoint(&w).is_ok());
}
#[test]
fn carpet_hover_uses_geometry_and_no_free_jet_energy() {
    let mut heights = vec![];
    for y in [1., 5.] {
        let (mut v, mut w) = setup();
        spawn(&mut v, &mut w, "magiccarpetvehicle", y);
        mount(&mut v, &w, 0);
        v.set_controls(
            OwnerId(10),
            OccupantId(20),
            Controls {
                jet: true,
                ..Default::default()
            },
        )
        .unwrap();
        step(&mut v, &mut w, 1, None);
        let s = &v.snapshot(&w).vehicles[0];
        assert_eq!(s.energy, 0.);
        assert!(!s.jetting);
        heights.push(s.velocity[1]);
    }
    assert!(heights[0] > 0., "below hover height gets restoring lift");
    assert!(heights[1] < 0., "above hover height gets reduced support");
}
#[test]
fn jet_resource_cadence_and_checkpoint_phase_survive_restore() {
    let (mut a, mut aw) = setup();
    spawn(&mut a, &mut aw, "flyingwheeledjeepvehicle", 80.);
    mount(&mut a, &aw, 0);
    a.set_energy(VehicleId(1), 100.).unwrap();
    a.set_controls(
        OwnerId(10),
        OccupantId(20),
        Controls {
            jet: true,
            ..Default::default()
        },
    )
    .unwrap();
    step(&mut a, &mut aw, 17, None);
    let cp = save(&mut a, &aw);
    assert_eq!(cp.vehicles[0].energy_phase, 41);
    assert_eq!(cp.vehicles[0].energy, 92.);
    let (mut b, mut bw) = setup();
    b.restore_checkpoint(&mut bw, cp, |_, _, _| true).unwrap();
    step(&mut a, &mut aw, 7, None);
    step(&mut b, &mut bw, 7, None);
    assert_eq!(
        serde_json::to_value(a.snapshot(&aw)).unwrap(),
        serde_json::to_value(b.snapshot(&bw)).unwrap()
    );
    step(&mut a, &mut aw, 200, None);
    assert_eq!(a.snapshot(&aw).vehicles[0].energy, 0.);
    assert!(!a.snapshot(&aw).vehicles[0].jetting);
}
#[test]
fn scaled_turret_pose_and_angular_velocity_restore_to_shared_geometry() {
    let (mut a, mut aw) = setup();
    spawn(&mut a, &mut aw, "tankvehicle", 30.);
    mount(&mut a, &aw, 2);
    a.set_controls(
        OwnerId(10),
        OccupantId(22),
        Controls {
            aim_yaw: 1.,
            ..Default::default()
        },
    )
    .unwrap();
    step(&mut a, &mut aw, 1, None);
    let mut cp = save(&mut a, &aw);
    cp.vehicles[0].spawn.scale = 2.;
    cp.vehicles[0].angular_velocity = [0.2, 0.3, 0.4];
    a.restore_checkpoint(&mut aw, cp, |_, _, _| true).unwrap();
    let turret = aw
        .colliders
        .iter()
        .find(|(h, _)| matches!(a.classify_collider(*h), Some((_, VehiclePart::Turret))))
        .unwrap()
        .1;
    let rotation = turret.position_wrt_parent().unwrap().rotation;
    let cp = save(&mut a, &aw);
    let (mut b, mut bw) = setup();
    b.restore_checkpoint(&mut bw, cp, |_, _, _| true).unwrap();
    assert_eq!(
        serde_json::to_value(a.snapshot(&aw)).unwrap(),
        serde_json::to_value(b.snapshot(&bw)).unwrap()
    );
    let other = bw
        .colliders
        .iter()
        .find(|(h, _)| matches!(b.classify_collider(*h), Some((_, VehiclePart::Turret))))
        .unwrap()
        .1;
    assert_eq!(rotation, other.position_wrt_parent().unwrap().rotation);
}
/// Every stock wheeled vehicle must rest upright on its tires, drive toward
/// its nose on W without pitching over, and turn right on D.
#[test]
fn wheeled_vehicles_settle_upright_and_drive_forward() {
    let wheeled: Vec<_> = pack()
        .definitions
        .into_iter()
        .filter(|d| matches!(d.family, Family::Wheeled | Family::FlyingWheeled))
        .collect();
    assert_eq!(wheeled.len(), 3);
    for d in wheeled {
        let name = d.id.trim_start_matches("v20.vehicle.");
        for wheel in &d.wheels {
            let outer = glam::Quat::from_array(wheel.model_rotation) * Vec3::NEG_Z;
            assert!(
                (outer - Vec3::X * wheel.position[0].signum()).length() < 1e-4,
                "{name} tire must face outward"
            );
        }
        let (mut v, mut w) = setup();
        spawn(&mut v, &mut w, name, 2.);
        mount(&mut v, &w, 0);
        step(&mut v, &mut w, 240, None);
        let s = &v.snapshot(&w).vehicles[0];
        let rotation = glam::Quat::from_array(s.transform.rotation);
        assert!((rotation * Vec3::Y).y > 0.999, "{name} must settle upright");
        for (wheel, extension) in d.wheels.iter().zip(&s.wheel_suspension) {
            assert!(
                *extension > 0.01 && *extension < wheel.rest_length - 0.01,
                "{name} wheel must carry load, extension {extension}"
            );
            let hub = Vec3::from_array(s.transform.position)
                + rotation * (Vec3::from_array(wheel.position) - Vec3::Y * *extension);
            assert!(
                (hub.y - wheel.radius).abs() < 0.05,
                "{name} tire must touch the ground, bottom {}",
                hub.y - wheel.radius
            );
        }
        let start = Vec3::from_array(s.transform.position);
        v.set_controls(
            OwnerId(10),
            OccupantId(20),
            Controls {
                throttle: 1.,
                ..Default::default()
            },
        )
        .unwrap();
        for _ in 0..36 {
            step(&mut v, &mut w, 10, None);
            let s = &v.snapshot(&w).vehicles[0];
            let up = glam::Quat::from_array(s.transform.rotation) * Vec3::Y;
            assert!(up.y > 0.98, "{name} tipped while accelerating: up {up}");
        }
        let s = &v.snapshot(&w).vehicles[0];
        let travel = Vec3::from_array(s.transform.position) - start;
        assert!(
            travel.z < -20. && travel.x.abs() < 1.,
            "{name} must drive toward its nose, moved {travel}"
        );
        v.set_controls(
            OwnerId(10),
            OccupantId(20),
            Controls {
                throttle: 1.,
                steer: 1.,
                // Mouse-steered vehicles turn by accumulating mouse motion.
                look_delta: if d.strafe_steering { [0.; 2] } else { [0.02, 0.] },
                ..Default::default()
            },
        )
        .unwrap();
        step(&mut v, &mut w, 90, None);
        let s = &v.snapshot(&w).vehicles[0];
        let rotation = glam::Quat::from_array(s.transform.rotation);
        assert!((rotation * Vec3::Y).y > 0.95, "{name} rolled over turning");
        assert!(
            (rotation * Vec3::NEG_Z).x > 0.2,
            "{name} must turn right on positive steer"
        );
    }
}
