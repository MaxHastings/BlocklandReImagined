//! Vehicle trails: wing-tip contrails like the Stunt Plane's (its
//! stuntplane_Contrail.cs mounts contrailImage1/2 at mount3/mount4 while
//! `vectorLen(%obj.getVelocity()) >= minContrailSpeed`, 30; the importer
//! turns that into `trails`). Flown in the vehicles runtime and presented as
//! the host's driver sees it and as a guest does, after the pose crosses the
//! wire. The plane is a synthetic one of the same shape, since the original
//! is never in the repository; `addon-import`'s tests check the original
//! imports to these trails.
// The synthetic plane is one large `json!`.
#![recursion_limit = "512"]
use anyhow::Result;
use bri_client::actor_effects::{ActorEffects, vehicle_trails, with_vehicle_effects};
use bri_client::vehicles::ClientVehicles;
use bri_content::effects::Library;
use bri_fx_runtime::{
    Camera, EffectsPack,
    pack::{Manifest, TextureImage},
};
use bri_sim::session::{VehicleInfo, VehiclePose};
use bri_vehicles::{VehiclesWorld, schema::Pack};
use glam::{Mat4, Quat, Vec3};
use std::{collections::BTreeMap, path::Path, sync::Arc};

const PLANE: &str = "test_plane:vehicle/contrailplane";
const EMITTER: &str = "test_plane:emitter/contrail";

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}
/// A flying wheeled plane with an emitter at each wing tip, 4.5 either side,
/// from speed 30: a particle a millisecond that lives half a second.
fn plane_pack() -> Pack {
    let wheel = |x: f32, z: f32, steering: f32, powered: bool| {
        serde_json::json!({"position": [x, 0.2, z], "radius": 0.5, "rest_length": 0.4, "spring": 60.0,
            "damping": 8.0, "anti_sway": 1.0, "tire": {"static_friction": 1.5, "kinetic_friction": 1.0,
                "lateral_force": 600.0, "lateral_damping": 60.0, "lateral_relaxation": 1.0,
                "longitudinal_force": 600.0, "longitudinal_damping": 60.0, "longitudinal_relaxation": 1.0},
            "steering": steering, "powered": powered, "model": "test/tire.dts",
            "model_rotation": [0.0, 0.0, 0.0, 1.0]})
    };
    let tip = |node: &str, x: f32| {
        serde_json::json!({"node": node, "transform": {"position": [x, 0.6, -0.5], "rotation": [0.0, 0.0, 0.0, 1.0]},
            "emitter": EMITTER, "min_speed": 30.0, "max_speed": null})
    };
    let hull: Vec<[f32; 3]> = (0..8)
        .map(|i| {
            [
                if i & 1 == 0 { -1.0 } else { 1.0 },
                if i & 2 == 0 { 0.0 } else { 1.2 },
                if i & 4 == 0 { -2.0 } else { 2.0 },
            ]
        })
        .collect();
    let plane = serde_json::json!({
        "id": PLANE, "datablock": "ContrailPlaneVehicle", "name": "Contrail Plane", "family": "Wheeled",
        "energy": {"maximum": 100.0, "minimum_jet": 10.0, "drain_per_32ms": 1.0, "recharge_per_32ms": 1.0, "jet_force": 500.0},
        "flight": null, "model": "test/plane.dts",
        "seats": [{"node": "mount0", "transform": {"position": [0.0, 1.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0]},
            "pose": "sit", "controls": true, "weapon": false}],
        "wheels": [wheel(-1.0, -1.5, 1.0, false), wheel(1.0, 1.5, 0.0, true), wheel(-1.0, 1.5, 0.0, true)],
        "weapon": null, "attachment_model": null, "attachment_collision_hulls": [], "attachment_mount": null,
        "attachment_fallback_seat": null, "collision_hulls": [hull],
        "bounds_min": [-1.0, 0.0, -2.0], "bounds_max": [1.0, 1.2, 2.0],
        "mass": 200.0, "mass_center": [0.0, 0.3, 0.0], "inertia_box": [2.0, 1.2, 4.0],
        "density": 1.0, "drag": 0.5, "friction": 0.8, "restitution": 0.3,
        "max_damage": 120.0, "burn_ticks": 240, "invulnerable_ticks": 60,
        "initial_explosion": null, "final_explosion": null,
        "initial_explosion_offset": 0.0, "final_explosion_offset": 0.0,
        "mount_distance": 3.0, "engine_force": 4000.0, "engine_brake": 500.0, "brake_force": 2000.0,
        "max_speed": 45.0, "reverse_speed": 10.0, "max_steering": 0.6,
        "thrust": 3000.0, "reverse_thrust": 1000.0, "lift": 50.0,
        "yaw_force": 500.0, "pitch_force": 500.0, "roll_force": 500.0, "angular_drag": 2.0,
        "jump_speed": 8.0, "max_side_speed": 5.0, "run_surface_angle": 50.0,
        "impact_threshold": 5.0, "impact_damage": 1.0, "strafe_steering": false,
        "look_pitch": [-1.5, 1.5], "underwater_speeds": [5.0, 3.0, 3.0],
        "camera": {"max_dist": 8.0, "offset": 2.0, "tilt": 0.1, "lag": 0.1, "decay": 0.5},
        "look_limits": [0.0, 1.0],
        "wheeled_flight": {"max_forward_vel": 40.0, "max_reverse_vel": 20.0,
            "horizontal_surface_force": 100.0, "vertical_surface_force": 100.0, "stall_speed": 10.0, "sled": false},
        "runover_speed": 5.0, "runover_damage": 20.0, "runover_push": 5.0,
        "protect_direct": false, "protect_radius": false, "protect_burn": false,
        "trails": [tip("mount3", 4.5), tip("mount4", -4.5)],
        "effects": {
            "particles": [{"id": "test_plane:particle/contrail", "texture": "base/data/particles/cloud",
                "alpha_blend": false, "lifetime": 0.5, "lifetime_variance": 0.0, "drag": 0.0, "wind": 0.0,
                "gravity": 0.0, "inherited_velocity": 0.0, "acceleration": 0.0, "spin_degrees": 0.0,
                "random_spin": [0.0, 0.0],
                "keys": [{"time": 0.0, "color": [1.0, 1.0, 1.0, 1.0], "size": 0.1},
                         {"time": 1.0, "color": [1.0, 1.0, 1.0, 0.0], "size": 0.1}]}],
            "emitters": [{"id": EMITTER, "name": "", "particles": ["test_plane:particle/contrail"],
                "period": 0.001, "period_variance": 0.0, "speed": 0.0, "speed_variance": 0.0,
                "offset": 0.0, "offset_variance": 0.0, "theta_degrees": [0.0, 180.0], "phi_rate_degrees": 0.0,
                "phi_variance_degrees": 360.0, "lifetime": 0.0, "lifetime_variance": 0.0, "orient": false,
                "orient_on_velocity": true, "override_advance": false, "use_emitter_colors": false,
                "use_emitter_sizes": false, "use_placement_velocity": false, "node_time_scale": 1.0,
                "point_node_time_scale": 1.0}]
        },
        "authored": {}, "adaptations": [],
    });
    let pack: Pack = serde_json::from_value(serde_json::json!({
        "schema_version": bri_vehicles::schema::SCHEMA_VERSION,
        "definitions": [plane], "assets": [], "evidence": [], "unresolved": [], "animation_aliases": {},
    }))
    .unwrap();
    pack.validate().unwrap();
    pack
}

/// An effects pack with the base game's cloud texture and nothing else.
fn cloud_only() -> Arc<EffectsPack> {
    EffectsPack::from_parts(
        Library {
            schema_version: 1,
            lights: vec![],
            particles: vec![],
            emitters: vec![],
            textures: BTreeMap::from([("base/data/particles/cloud".into(), "cloud.png".into())]),
        },
        Manifest {
            schema_version: 1,
            library_sha256: String::new(),
            textures: BTreeMap::new(),
            emitter_alpha: BTreeMap::new(),
            bindings: vec![],
            composites: vec![],
            unresolved: vec![],
        },
        vec![TextureImage {
            id: "base/data/particles/cloud".into(),
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        }],
    )
    .unwrap()
}

fn actor_effects(pack: Arc<EffectsPack>, vehicles: &Pack) -> ActorEffects {
    let (pack, notes) = with_vehicle_effects(pack, vehicles).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let weapons = bri_weapons::Pack::from_json(
        format!(
            r#"{{ "schema_version": {}, "id": "test_plane" }}"#,
            bri_weapons::SCHEMA
        )
        .as_bytes(),
    )
    .unwrap();
    ActorEffects::new(pack, Arc::new(weapons), Default::default()).unwrap()
}

/// The plane 45 up over a floor, a driver seated, full throttle from rest.
fn flying() -> (VehiclesWorld, rapier3d::prelude::PhysicsWorld) {
    use bri_vehicles::*;
    use rapier3d::prelude::*;
    let mut v = VehiclesWorld::new(plane_pack()).unwrap();
    let mut w = bri_physics::new_world();
    w.insert(
        RigidBodyBuilder::fixed().translation(Vec3::new(0., -0.5, 0.)),
        ColliderBuilder::cuboid(2000., 0.5, 2000.),
    );
    v.spawn(
        &mut w,
        Spawn {
            scale: 1.,
            id: VehicleId(1),
            owner: OwnerId(10),
            definition: PLANE.into(),
            transform: Transform {
                position: [0., 45., 0.],
                ..Default::default()
            },
            spawn_id: None,
            respawn_ticks: None,
        },
    )
    .unwrap();
    w.detect_collisions(&(), &());
    let seat = v.snapshot(&w).vehicles[0].seats[0].transform.position;
    v.mount(
        &w,
        VehicleId(1),
        0,
        Occupant {
            id: OccupantId(20),
            owner: OwnerId(10),
            body: [1.25, 2.65],
        },
        seat,
    )
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
    (v, w)
}

/// What one viewer sees each tick: how many trails emit, and the particles.
struct Viewer {
    vehicles: ClientVehicles,
    effects: ActorEffects,
    driven: Option<u64>,
    wire: bool,
}
impl Viewer {
    fn tick(&mut self, d: &bri_vehicles::Definition, pose: &VehiclePose) -> Result<usize> {
        let pose = if self.wire {
            // A guest's copy, as the host's datagram carries it.
            let bytes = bri_net::codec::encode_datagram(&bri_net::protocol::Datagram::Vehicle(
                pose.clone(),
            ))?;
            match bri_net::codec::decode_datagram(&bytes)? {
                bri_net::protocol::Datagram::Vehicle(p) => p,
                other => anyhow::bail!("decoded {other:?}"),
            }
        } else {
            pose.clone()
        };
        let infos = BTreeMap::from([(
            1,
            VehicleInfo {
                id: 1,
                definition: PLANE.into(),
                color: None,
                occupants: vec![Some(10)],
                destroyed: false,
                turret_broken: false,
                scale: 1.0,
            },
        )]);
        let tick = pose.tick as f64;
        self.vehicles.update(
            &infos,
            &BTreeMap::from([(1, pose)]),
            Some(tick),
            self.driven,
            &Default::default(),
        );
        let frame = self.vehicles.frame(1).expect("presented").clone();
        let trails = vehicle_trails(1, d, &frame);
        self.effects.update_trails(&trails)?;
        self.effects
            .advance(bri_physics::FIXED_DT, |_| None::<Mat4>, &[], &[], &[])?;
        Ok(trails.len())
    }
}

fn pose(tick: u64, v: &bri_vehicles::world::VehicleSnapshot) -> VehiclePose {
    VehiclePose {
        passage_frame: Default::default(),
        id: 1,
        tick,
        position: v.transform.position,
        rotation: v.transform.rotation,
        velocity: v.velocity,
        steering: v.steering,
        wheel_suspension: v.wheel_suspension.clone(),
        wheel_rotation: v.wheel_rotation.clone(),
        wheel_contact: v.wheel_contact.clone(),
        wheel_tire: v.wheel_tire.clone(),
        turret_aim: v.turret_aim,
        jetting: v.jetting,
        angular_velocity: [0.0; 3],
        mouse_steering: [0.0; 2],
        driver_input: 0,
        driver_steering: (false, false),
        steering_quiet: 0,
        actor: None,
    }
}

#[test]
fn a_plane_streams_contrails_off_its_wing_tips_past_speed_30() -> Result<()> {
    let pack = plane_pack();
    let d = pack.definitions.iter().find(|d| d.id == PLANE).unwrap();
    assert_eq!(d.trails.len(), 2, "{:?}", d.trails);
    // The host's driver (the single player too) and a guest watching.
    let mut viewers: Vec<Viewer> = [(Some(1), false), (None, true)]
        .into_iter()
        .map(|(driven, wire)| Viewer {
            vehicles: ClientVehicles::default(),
            effects: actor_effects(cloud_only(), &pack),
            driven,
            wire,
        })
        .collect();
    let (mut v, mut w) = flying();
    let mut crossed = None;
    let mut emitted = [false; 2];
    for tick in 1..=600u64 {
        v.pre_step(&mut w, &[])?;
        w.step();
        v.post_step(&mut w)?;
        let snapshot = v.snapshot(&w);
        let s = &snapshot.vehicles[0];
        let speed = Vec3::from(s.velocity).length();
        let pose = pose(tick, s);
        for (i, viewer) in viewers.iter_mut().enumerate() {
            let emitting = viewer.tick(d, &pose)?;
            let presented = viewer.vehicles.frame(1).unwrap().velocity.length();
            // Nothing below 30; both wing tips from 30 (the guest sees the
            // plane a few ticks late, so it crosses on its own frame).
            assert_eq!(
                emitting,
                if presented >= 30. { 2 } else { 0 },
                "viewer {i} tick {tick}"
            );
            emitted[i] |= emitting > 0;
            if !emitted[i] {
                assert_eq!(
                    viewer.effects.world().particle_count(),
                    0,
                    "viewer {i} tick {tick}"
                );
            }
        }
        if speed >= 30. && crossed.is_none() {
            crossed = Some(tick);
        }
    }
    let crossed = crossed.expect("the plane never reached speed 30");
    assert!(crossed < 480, "reached 30 only at tick {crossed}");
    let snapshot = v.snapshot(&w);
    let s = &snapshot.vehicles[0];
    let body = Mat4::from_rotation_translation(
        Quat::from_array(s.transform.rotation),
        Vec3::from(s.transform.position),
    );
    for (i, viewer) in viewers.iter().enumerate() {
        assert_eq!(viewer.effects.trail_count(), 2, "viewer {i}");
        // ContrailEmitter ejects one particle a millisecond that lives half
        // a second: about 500 per wing tip in flight.
        let count = viewer.effects.world().particle_count();
        assert!(
            (800..=1100).contains(&count),
            "viewer {i}: {count} particles"
        );
        // They hang where the tips passed, still in the air (no velocity or
        // gravity): each lies on a line behind a tip, 4.5 either side.
        let frame = viewer.effects.world().snapshot(&Camera {
            view_projection: Mat4::IDENTITY,
            position: Vec3::new(0., 100., 0.),
            right: Vec3::X,
            up: Vec3::Y,
        });
        let local: Vec<Vec3> = frame
            .particles
            .iter()
            .map(|p| body.inverse().transform_point3(p.position))
            .collect();
        assert!(!local.is_empty());
        let right = local.iter().filter(|p| (p.x - 4.5).abs() < 0.5).count();
        let left = local.iter().filter(|p| (p.x + 4.5).abs() < 0.5).count();
        assert!(
            right + left == local.len() && right > local.len() / 3 && left > local.len() / 3,
            "viewer {i}: {right} right, {left} left of {}",
            local.len()
        );
        // Behind the plane: up to half a second of flight.
        assert!(local.iter().all(|p| p.z > -1. && p.z < 25.), "viewer {i}");
    }
    Ok(())
}

#[test]
fn trails_stop_below_their_speed_and_their_particles_drain() -> Result<()> {
    let pack = plane_pack();
    let d = pack.definitions.iter().find(|d| d.id == PLANE).unwrap();
    let mut viewer = Viewer {
        vehicles: ClientVehicles::default(),
        effects: actor_effects(cloud_only(), &pack),
        // Its driver: drawn at the newest pose, so the speed shows at once.
        driven: Some(1),
        wire: false,
    };
    let at = |tick: u64, speed: f32| VehiclePose {
        passage_frame: Default::default(),
        id: 1,
        tick,
        position: [0., 50., -(tick as f32) * speed / 120.],
        rotation: [0., 0., 0., 1.],
        velocity: [0., 0., -speed],
        steering: 0.,
        wheel_suspension: vec![0.2; 3],
        wheel_rotation: vec![0.; 3],
        wheel_contact: vec![false; 3],
        wheel_tire: vec![Default::default(); 3],
        turret_aim: [0.; 2],
        jetting: false,
        angular_velocity: [0.0; 3],
        mouse_steering: [0.0; 2],
        driver_input: 0,
        driver_steering: (false, false),
        steering_quiet: 0,
        actor: None,
    };
    for tick in 1..=60 {
        assert_eq!(viewer.tick(d, &at(tick, 35.))?, 2);
    }
    assert!(viewer.effects.world().particle_count() > 0);
    // Slower than 30: the emitters stop and what they left fades out.
    for tick in 61..=150 {
        assert_eq!(viewer.tick(d, &at(tick, 29.))?, 0);
    }
    assert_eq!(viewer.effects.trail_count(), 0);
    assert_eq!(viewer.effects.world().particle_count(), 0);
    Ok(())
}

#[test]
#[ignore = "requires the generated effects pack"]
fn the_base_effects_pack_draws_the_plane_contrails() -> Result<()> {
    // The contrail particle uses base/data/particles/cloud, which the base
    // game's effects pack carries; merging adds the Add-On's emitter.
    let effects = EffectsPack::load(bri_package::testing::pack_dir(
        &root().join("content"),
        "effects_runtime",
    ))?;
    let (merged, notes) = with_vehicle_effects(effects, &plane_pack())?;
    assert!(notes.is_empty(), "{notes:?}");
    assert!(merged.emitter_ids().any(|id| id == EMITTER));
    Ok(())
}
