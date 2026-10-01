//! A small made-up vehicle catalog for tests that have no converted content:
//! one vehicle per role the engine's code paths distinguish (a strafe-steered
//! car, a tank with an attached turret, a flying car, skis, a flying carpet,
//! the player-type mounts, a ball and the tumble body). Every value is
//! invented, round and plausible, tuned only so the behaviour each family's
//! code path implements shows; none comes from Blockland's own vehicles.
//!
//! Ids are `test:vehicle/...`, except where the engine itself spawns or
//! looks up a vehicle by its v20 id (the skis item, the tumble a wreck
//! starts, the tutorial's horse and jeep pads, the tank gunner's look
//! limits): those stand-ins carry that id so the same code path runs.
//!
//! Tests read a vehicle's numbers back from its [`Definition`] rather than
//! repeating them, and change one with [`pack_with`] or by editing
//! `Pack::definitions`.
use crate::schema::*;
use glam::Quat;
use std::collections::BTreeMap;

/// A strafe-steered four-wheel car: a driver and three passengers.
pub const CAR: &str = "v20.vehicle.jeepvehicle";
/// A mouse-steered tank: four powered wheels, the rear pair steering against
/// the front, and an attached turret (its own model and hull) whose gunner
/// sits in seat 2 with a gun.
pub const TANK: &str = "test:vehicle/tank";
/// A mouse-steered car with Blockland's wheeled flying forces, and jets.
pub const FLYING_CAR: &str = "test:vehicle/flying-car";
/// Skis: a sled on frictionless wheels that only its flying forces move.
pub const SKIS: &str = "v20.vehicle.skivehicle";
/// A hovering `Flying` vehicle.
pub const CARPET: &str = "test:vehicle/carpet";
/// A player-type mount that runs and jumps.
pub const HORSE: &str = "v20.vehicle.horsearmor";
/// A player-type mount that floats and rows, with a passenger seat.
pub const ROWBOAT: &str = "test:vehicle/rowboat";
/// A player-type gun that charges its shot while the trigger is held.
pub const CANNON: &str = "test:vehicle/cannon";
/// A standalone player-type gun turret.
pub const TURRET: &str = "v20.vehicle.tankturretplayer";
/// A ball with no seats.
pub const BALL: &str = "test:vehicle/ball";
/// The body a wrecked skier tumbles in: a seat with no controls.
pub const TUMBLE: &str = "v20.vehicle.deathvehicle";

/// Every vehicle here, in catalog order.
pub const ALL: [&str; 11] = [
    CAR, TANK, FLYING_CAR, SKIS, CARPET, HORSE, ROWBOAT, CANNON, TURRET, BALL, TUMBLE,
];

/// The catalog, validated.
pub fn pack() -> Pack {
    pack_with(|_| {})
}

/// The catalog with `edit` applied to every definition (match on
/// `Definition::id` to change one), validated.
pub fn pack_with(edit: impl FnMut(&mut Definition)) -> Pack {
    let mut definitions = definitions();
    definitions.iter_mut().for_each(edit);
    let pack = Pack {
        schema_version: SCHEMA_VERSION,
        definitions,
        assets: vec![],
        evidence: vec![],
        unresolved: vec![],
        animation_aliases: BTreeMap::new(),
    };
    pack.validate().expect("the synthetic vehicles validate");
    pack
}

/// Every definition, in [`ALL`] order.
pub fn definitions() -> Vec<Definition> {
    vec![
        car(),
        tank(),
        flying_car(),
        skis(),
        carpet(),
        horse(),
        rowboat(),
        cannon(),
        turret(),
        ball(),
        tumble(),
    ]
}

/// One definition by id.
pub fn definition(id: &str) -> Definition {
    definitions()
        .into_iter()
        .find(|d| d.id == id)
        .unwrap_or_else(|| panic!("no synthetic vehicle {id}"))
}

/// The eight corners of the box from `min` to `max`, as a collision hull.
pub fn box_hull(min: [f32; 3], max: [f32; 3]) -> Vec<[f32; 3]> {
    (0..8)
        .map(|i| {
            [
                if i & 1 == 0 { min[0] } else { max[0] },
                if i & 2 == 0 { min[1] } else { max[1] },
                if i & 4 == 0 { min[2] } else { max[2] },
            ]
        })
        .collect()
}

fn at(position: [f32; 3]) -> Transform {
    Transform {
        position,
        ..Default::default()
    }
}

fn seat(node: &str, position: [f32; 3], controls: bool, weapon: bool) -> Seat {
    Seat {
        node: node.into(),
        transform: at(position),
        pose: "sit".into(),
        controls,
        weapon,
    }
}

/// A car tyre: grips, springs back and gives a little.
fn tire() -> Tire {
    Tire {
        static_friction: 3.,
        kinetic_friction: 1.5,
        lateral_force: 3000.,
        lateral_damping: 300.,
        lateral_relaxation: 1.,
        longitudinal_force: 3000.,
        longitudinal_damping: 300.,
        longitudinal_relaxation: 1.,
    }
}

/// A wheel whose hub mounts at `position`, its tyre model turned so its
/// outer face points away from the chassis.
fn wheel(position: [f32; 3], steering: f32, powered: bool) -> Wheel {
    let outward = if position[0] < 0. { 1. } else { -1. };
    Wheel {
        position,
        radius: 0.5,
        rest_length: 0.5,
        spring: 3000.,
        damping: 300.,
        anti_sway: 500.,
        tire: tire(),
        steering,
        powered,
        model: "test/tire.dts".into(),
        model_rotation: Quat::from_rotation_y(outward * std::f32::consts::FRAC_PI_2).to_array(),
    }
}

fn four_wheels(x: f32, y: f32, z: f32, rear_steering: f32) -> Vec<Wheel> {
    vec![
        wheel([-x, y, -z], 1., true),
        wheel([x, y, -z], 1., true),
        wheel([-x, y, z], rear_steering, true),
        wheel([x, y, z], rear_steering, true),
    ]
}

fn impact_sounds(hard: &str, hard_speed: f32, soft_speed: f32) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("hardimpactsound".into(), hard.into()),
        ("hardimpactspeed".into(), hard_speed.to_string()),
        ("softimpactsound".into(), "test:sound/soft-impact".into()),
        ("softimpactspeed".into(), soft_speed.to_string()),
    ])
}

/// What every vehicle starts from: a 2 x 1 x 4 box of 100 kg that does
/// nothing by itself.
fn base(id: &str, name: &str, family: Family) -> Definition {
    let (min, max) = ([-1., 0., -2.], [1., 1., 2.]);
    Definition {
        id: id.into(),
        datablock: name.replace(' ', ""),
        name: name.into(),
        family,
        energy: EnergySettings {
            maximum: 0.,
            minimum_jet: 0.,
            drain_per_32ms: 0.,
            recharge_per_32ms: 0.,
            jet_force: 0.,
        },
        flight: None,
        model: format!("test/{}.dts", name.to_ascii_lowercase().replace(' ', "-")),
        seats: vec![],
        wheels: vec![],
        weapon: None,
        attachment_model: None,
        attachment_collision_hulls: vec![],
        attachment_mount: None,
        attachment_fallback_seat: None,
        collision_hulls: vec![box_hull(min, max)],
        bounds_min: min,
        bounds_max: max,
        mass: 100.,
        mass_center: [0., 0.5, 0.],
        inertia_box: [2., 1., 4.],
        density: 2.,
        drag: 0.5,
        friction: 0.6,
        restitution: 0.2,
        max_damage: 100.,
        burn_ticks: 0,
        invulnerable_ticks: 0,
        initial_explosion: None,
        final_explosion: None,
        initial_explosion_offset: 0.,
        final_explosion_offset: 0.,
        mount_distance: 4.,
        engine_force: 0.,
        engine_brake: 0.,
        brake_force: 0.,
        max_speed: 0.,
        reverse_speed: 0.,
        max_steering: 0.6,
        thrust: 0.,
        reverse_thrust: 0.,
        lift: 0.,
        yaw_force: 0.,
        pitch_force: 0.,
        roll_force: 0.,
        angular_drag: 1.,
        jump_speed: 0.,
        max_side_speed: 0.,
        run_surface_angle: 45.,
        impact_threshold: 0.,
        impact_damage: 0.,
        strafe_steering: false,
        steering: SteeringSettings::default(),
        wheeled_flight: None,
        threads: vec![],
        look_pitch: [-1., 1.],
        underwater_speeds: [0.; 3],
        camera: VehicleCamera {
            max_dist: 10.,
            offset: 3.,
            tilt: 0.2,
            lag: 0.,
            decay: 0.,
        },
        look_limits: [0., 1.],
        runover_speed: 0.,
        runover_damage: 0.,
        runover_push: 0.,
        protect_direct: false,
        protect_radius: false,
        protect_burn: false,
        smash: None,
        shove: false,
        harms_only_in_minigames: false,
        per_player: None,
        blast_scale: None,
        trails: vec![],
        effects: VehicleEffects::default(),
        authored: BTreeMap::new(),
        adaptations: vec![],
    }
}

/// A wheeled body over four wheels, the body `size` wide, tall and long
/// from `bottom` up.
fn wheeled(id: &str, name: &str, size: [f32; 3], bottom: f32) -> Definition {
    let [x, y, z] = size.map(|s| s * 0.5);
    let (min, max) = ([-x, bottom, -z], [x, bottom + 2. * y, z]);
    Definition {
        collision_hulls: vec![box_hull(min, max)],
        bounds_min: min,
        bounds_max: max,
        mass_center: [0., bottom, 0.],
        inertia_box: size,
        density: 4.,
        drag: 1.5,
        max_damage: 400.,
        burn_ticks: 240,
        invulnerable_ticks: 12,
        initial_explosion: Some("test:projectile/vehicle-explosion".into()),
        final_explosion: Some("test:projectile/vehicle-final-explosion".into()),
        final_explosion_offset: 1.,
        reverse_speed: 10.,
        runover_speed: 4.,
        runover_damage: 6.,
        runover_push: 1.5,
        authored: impact_sounds("test:sound/hard-impact", 20., 10.),
        ..base(id, name, Family::Wheeled)
    }
}

pub fn car() -> Definition {
    Definition {
        seats: vec![
            seat("mount0", [-0.5, 1.2, 0.], true, false),
            seat("mount1", [0.5, 1.2, 0.], false, false),
            seat("mount2", [-0.5, 1.2, 1.2], false, false),
            seat("mount3", [0.5, 1.2, 1.2], false, false),
        ],
        wheels: four_wheels(1., 0.4, 1.6, 0.),
        mass: 200.,
        engine_force: 3000.,
        engine_brake: 500.,
        brake_force: 2000.,
        max_speed: 30.,
        max_steering: 0.8,
        strafe_steering: true,
        ..wheeled(CAR, "Test Car", [2.4, 1.2, 4.8], 0.4)
    }
}

/// Every wheel with this spring and damper.
fn sprung(wheels: Vec<Wheel>, spring: f32, damping: f32) -> Vec<Wheel> {
    wheels
        .into_iter()
        .map(|w| Wheel {
            spring,
            damping,
            ..w
        })
        .collect()
}

pub fn tank() -> Definition {
    let turret = ([-1., 0., -1.], [1., 0.8, 1.]);
    Definition {
        seats: vec![
            seat("mount0", [0., 1.4, -1.], true, false),
            seat("mount1", [0., 1.4, 1.], false, false),
            seat("mount2", [0., 2.4, 0.3], false, true),
        ],
        wheels: sprung(four_wheels(1.3, 0.4, 1.8, -0.5), 6000., 600.),
        weapon: Some(Weapon {
            projectile: "test:projectile/shell".into(),
            muzzle: at([0., 2.6, -3.]),
            pivot: [0., 2.4, 0.],
            cooldown_ticks: 180,
            speed: 80.,
            charge_ticks: 0,
            charge_steps: 1,
            sound: "test:sound/shell-fire".into(),
            effect: "test:effect/shell-smoke".into(),
            look_muzzle: vec![],
        }),
        attachment_model: Some("test/tank-turret.dts".into()),
        attachment_collision_hulls: vec![box_hull(turret.0, turret.1)],
        attachment_mount: Some(at([0., 1.6, 0.])),
        attachment_fallback_seat: Some(at([0., 1.6, 1.5])),
        mass: 400.,
        engine_force: 6000.,
        engine_brake: 1000.,
        brake_force: 4000.,
        max_speed: 20.,
        ..wheeled(TANK, "Test Tank", [3., 1.2, 5.], 0.4)
    }
}

pub fn flying_car() -> Definition {
    Definition {
        energy: EnergySettings {
            maximum: 100.,
            minimum_jet: 10.,
            drain_per_32ms: 2.,
            recharge_per_32ms: 0.,
            jet_force: 500.,
        },
        seats: vec![
            seat("mount0", [0., 1.2, -0.4], true, false),
            seat("mount1", [0., 1.2, 0.8], false, false),
        ],
        wheels: sprung(four_wheels(1., 0.4, 1.5, 0.), 2250., 250.),
        mass: 150.,
        engine_force: 2000.,
        engine_brake: 300.,
        brake_force: 1500.,
        max_speed: 20.,
        thrust: 1500.,
        reverse_thrust: 500.,
        // Carries the weight (150 x 20) at the top speed of 30.
        lift: 100.,
        yaw_force: 1500.,
        pitch_force: 2500.,
        roll_force: 1000.,
        wheeled_flight: Some(WheeledFlightSettings {
            max_forward_vel: 30.,
            max_reverse_vel: 10.,
            horizontal_surface_force: 10.,
            vertical_surface_force: 10.,
            stall_speed: 8.,
            sled: false,
        }),
        ..wheeled(FLYING_CAR, "Test Flying Car", [2.4, 1., 4.4], 0.4)
    }
}

pub fn skis() -> Definition {
    let frictionless = Tire {
        static_friction: 0.,
        kinetic_friction: 0.,
        lateral_force: 0.,
        lateral_damping: 0.,
        lateral_relaxation: 0.,
        longitudinal_force: 0.,
        longitudinal_damping: 0.,
        longitudinal_relaxation: 0.,
    };
    // Springs far too soft for the weight: the skis ride on their body.
    let ski = |x: f32, z: f32| Wheel {
        position: [x, 0.2, z],
        radius: 0.3,
        rest_length: 0.4,
        spring: 100.,
        damping: 20.,
        anti_sway: 0.,
        tire: frictionless.clone(),
        steering: 0.,
        powered: false,
        model: String::new(),
        model_rotation: [0., 0., 0., 1.],
    };
    let (min, max) = ([-0.5, 0., -1.2], [0.5, 0.4, 1.2]);
    let authored = BTreeMap::from([
        (
            "hardimpactsound".to_owned(),
            "test:sound/ski-impact".to_owned(),
        ),
        ("hardimpactspeed".into(), "10".into()),
        ("softimpactspeed".into(), "5".into()),
        ("minimpactspeed".into(), "10".into()),
    ]);
    Definition {
        seats: vec![seat("mount0", [0., 0.4, 0.], true, false)],
        wheels: vec![
            ski(-0.3, -0.8),
            ski(0.3, -0.8),
            ski(-0.3, 0.8),
            ski(0.3, 0.8),
        ],
        collision_hulls: vec![box_hull(min, max)],
        bounds_min: min,
        bounds_max: max,
        mass: 80.,
        mass_center: [0., 0.2, 0.],
        inertia_box: [1., 0.4, 2.4],
        friction: 0.15,
        restitution: 0.,
        max_speed: 30.,
        thrust: 400.,
        reverse_thrust: 100.,
        yaw_force: 200.,
        pitch_force: 50.,
        roll_force: 50.,
        wheeled_flight: Some(WheeledFlightSettings {
            max_forward_vel: 30.,
            max_reverse_vel: 5.,
            horizontal_surface_force: 40.,
            vertical_surface_force: 5.,
            stall_speed: 2.,
            sled: true,
        }),
        authored,
        ..base(SKIS, "Test Skis", Family::Skis)
    }
}

pub fn carpet() -> Definition {
    let (min, max) = ([-1., 0., -1.5], [1., 0.3, 1.5]);
    Definition {
        energy: EnergySettings {
            jet_force: 2000.,
            ..base(CARPET, "", Family::Flying).energy
        },
        flight: Some(FlightSettings {
            // Low enough for a player to jump on.
            hover_height: 1.5,
            create_hover_height: 1.5,
            min_drag: 30.,
            max_auto_speed: 10.,
            auto_linear_force: 50.,
            auto_angular_force: 100.,
            auto_input_damping: 0.5,
            horizontal_surface_force: 50.,
            vertical_surface_force: 50.,
            steering_force: 600.,
            steering_roll_force: 50.,
            vertical_thrust_multiple: 2.,
        }),
        seats: vec![seat("mount0", [0., 0.5, 0.], true, false)],
        collision_hulls: vec![box_hull(min, max)],
        bounds_min: min,
        bounds_max: max,
        mass_center: [0., 0.15, 0.],
        inertia_box: [2., 0.3, 3.],
        angular_drag: 2.,
        thrust: 1500.,
        roll_force: 2.,
        ..base(CARPET, "Test Carpet", Family::Flying)
    }
}

/// A player-type mount whose box is `size` wide, tall and long.
fn actor(id: &str, name: &str, family: Family, size: [f32; 3]) -> Definition {
    let (x, z) = (size[0] * 0.5, size[2] * 0.5);
    let (min, max) = ([-x, 0., -z], [x, size[1], z]);
    Definition {
        collision_hulls: vec![box_hull(min, max)],
        bounds_min: min,
        bounds_max: max,
        mass_center: [0., size[1] * 0.5, 0.],
        inertia_box: size,
        ..base(id, name, family)
    }
}

pub fn horse() -> Definition {
    Definition {
        seats: vec![seat("mount0", [0., 2.2, 0.], true, false)],
        mass: 200.,
        engine_force: 4000.,
        max_speed: 10.,
        reverse_speed: 4.,
        max_side_speed: 4.,
        jump_speed: 10.,
        underwater_speeds: [4., 3., 3.],
        density: 1.5,
        ..actor(HORSE, "Test Horse", Family::Horse, [2.4, 2., 2.4])
    }
}

pub fn rowboat() -> Definition {
    Definition {
        seats: vec![
            seat("mount0", [0., 0.8, -0.6], true, false),
            seat("mount1", [0., 0.8, 0.8], false, false),
        ],
        engine_force: 1500.,
        max_speed: 6.,
        reverse_speed: 3.,
        max_side_speed: 3.,
        underwater_speeds: [6., 3., 3.],
        density: 0.3,
        drag: 0.05,
        ..actor(ROWBOAT, "Test Rowboat", Family::Rowboat, [3.2, 1., 3.2])
    }
}

pub fn cannon() -> Definition {
    Definition {
        seats: vec![seat("mount0", [0., 1., 1.], true, true)],
        weapon: Some(Weapon {
            projectile: "test:projectile/cannonball".into(),
            muzzle: at([0., 1.2, -1.5]),
            pivot: [0., 1., 0.],
            cooldown_ticks: 240,
            speed: 6.,
            charge_ticks: 20,
            charge_steps: 4,
            sound: "test:sound/cannon-fire".into(),
            effect: "test:effect/cannon-smoke".into(),
            look_muzzle: vec![],
        }),
        mass: 200.,
        look_pitch: [-0.5, 1.],
        ..actor(CANNON, "Test Cannon", Family::Cannon, [2., 1.5, 2.])
    }
}

pub fn turret() -> Definition {
    Definition {
        seats: vec![seat("mount0", [0., 1.5, 0.], true, true)],
        weapon: Some(Weapon {
            projectile: "test:projectile/turret-shell".into(),
            muzzle: at([0., 1.8, -1.5]),
            pivot: [0., 1.8, 0.],
            cooldown_ticks: 120,
            speed: 60.,
            charge_ticks: 0,
            charge_steps: 1,
            sound: "test:sound/turret-fire".into(),
            effect: String::new(),
            look_muzzle: vec![],
        }),
        mass: 150.,
        ..actor(TURRET, "Test Turret", Family::Turret, [1.6, 2., 1.6])
    }
}

pub fn ball() -> Definition {
    let (min, max) = ([-2.; 3], [2.; 3]);
    Definition {
        collision_hulls: vec![box_hull(min, max)],
        bounds_min: min,
        bounds_max: max,
        mass_center: [0.; 3],
        inertia_box: [3.; 3],
        max_damage: 200.,
        friction: 0.8,
        restitution: 0.5,
        angular_drag: 0.5,
        authored: impact_sounds("test:sound/ball-bounce", 15., 8.),
        ..base(BALL, "Test Ball", Family::Ball)
    }
}

pub fn tumble() -> Definition {
    let (min, max) = ([-0.5, 0., -0.4], [0.5, 1.8, 0.4]);
    Definition {
        seats: vec![seat("mount0", [0., 0.9, 0.], false, false)],
        collision_hulls: vec![box_hull(min, max)],
        bounds_min: min,
        bounds_max: max,
        mass: 80.,
        mass_center: [0., 0.9, 0.],
        inertia_box: [1., 1.8, 0.8],
        density: 1.,
        restitution: 0.3,
        ..base(TUMBLE, "Test Tumble", Family::Tumble)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_synthetic_catalog_validates_with_one_vehicle_per_id() {
        let pack = super::pack();
        let ids: Vec<_> = pack.definitions.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, super::ALL);
    }
}
