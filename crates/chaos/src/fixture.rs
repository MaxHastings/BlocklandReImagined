//! Worlds for chaos runs. The synthetic one needs no v20 content: a handful
//! of brick shapes (including water and an indestructible one), every
//! vehicle family, the core tools and a small arsenal built from the Bubble
//! Blaster sample (bouncing, exploding, sticking, fast and brick-breaking
//! projectiles). With `BRI_CONTENT` the real game content is served instead.
use anyhow::{Context, Result};
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions, Special},
    session::{Session, ToolInventory},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// A chaos-ready session and what the bots need to know about it.
pub struct Fixture {
    pub session: Session,
    pub spawn_points: Vec<Vec3>,
    /// Brick definitions bots may plant.
    pub bricks: Vec<String>,
    /// Vehicle (and bot) kinds a spawn brick may hold.
    pub vehicles: Vec<String>,
    /// Items bots are given on joining.
    pub items: Vec<String>,
    /// Whole builds an administrator may load over the current one.
    pub saves: Vec<World>,
    /// Half-size of the area bots play in.
    pub extent: f32,
    /// The packages a joining client must hold.
    pub environment: bri_package::environment::Environment,
}

/// `BRI_CONTENT` when set: the generated content folder of a v20 install.
pub fn content_root() -> Option<PathBuf> {
    std::env::var_os("BRI_CONTENT").map(PathBuf::from)
}

pub const PLATE: &str = "chaos/brick/plate";
pub const BRICK: &str = "chaos/brick/brick2x4";
pub const TALL: &str = "chaos/brick/tall";
pub const BASEPLATE: &str = "chaos/brick/baseplate";
pub const WATER: &str = "chaos/brick/water";
pub const STONE: &str = "chaos/brick/stone";
/// The spawn-brick kind that makes a wandering bot (`bots::BOT_KINDS`).
pub const BOT: &str = "bot.blockhead";

fn definition(
    id: &str,
    studs: [u8; 2],
    plates: u16,
    special: Special,
    stone: bool,
) -> Result<Definition> {
    let size = [
        f32::from(studs[0]) * 0.5,
        f32::from(plates) * 0.2,
        f32::from(studs[1]) * 0.5,
    ];
    let collision = CollisionBody {
        id: id.into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size,
        }],
    };
    let shape = bri_physics::content::collider(&collision)?
        .build()
        .shared_shape()
        .clone();
    Ok(Definition {
        mesh: Mesh {
            schema_version: 1,
            id: id.into(),
            footprint_studs: [studs[0].into(), studs[1].into()],
            height_plates: plates.into(),
            // Per stud row, top plate first: studs on top, sockets below.
            attachment_rows: (0..studs[1])
                .flat_map(|_| {
                    (0..plates).map(move |y| {
                        let cell = match (y, plates) {
                            (_, 1) => "b",
                            (0, _) => "u",
                            (y, h) if y == h - 1 => "d",
                            _ => "x",
                        };
                        cell.repeat(studs[0].into())
                    })
                })
                .collect(),
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![],
        },
        collision,
        shape,
        indestructible: stone,
        special,
        reflection: None,
        link: None,
        glass: [0.0; 4],
    })
}

pub fn synthetic_definitions() -> Result<Definitions> {
    let entries = [
        definition(PLATE, [1, 1], 1, Special::None, false)?,
        definition(BRICK, [4, 2], 3, Special::None, false)?,
        definition(TALL, [1, 1], 15, Special::None, false)?,
        definition(BASEPLATE, [16, 16], 1, Special::None, false)?,
        definition(WATER, [4, 4], 3, Special::Water, false)?,
        definition(STONE, [2, 2], 3, Special::None, true)?,
    ];
    Ok(Definitions {
        entries: entries
            .into_iter()
            .map(|d| (d.mesh.id.clone(), d))
            .collect(),
    })
}

/// The Bubble Blaster sample's weapons file, the template for every item.
fn bubble_pack() -> Result<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/samples/sample-bubble-blaster/assets/weapons.json");
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

/// Content-free weapons: the core tools, the bubble blaster and variants of
/// it covering each projectile behaviour the runtime has.
pub fn synthetic_weapons() -> Result<(bri_weapons::Pack, Vec<String>)> {
    let mut pack = bubble_pack()?;
    let bubble_item = pack["items"]["sample-bubble-blaster:weapon/bubble_blaster"].clone();
    let bubble_image = pack["images"]["sample-bubble-blaster:image/bubble_blaster"].clone();
    let bubble_projectile = pack["projectiles"]["sample-bubble-blaster:projectile/bubble"].clone();
    let mut given = vec!["sample-bubble-blaster:weapon/bubble_blaster".to_string()];
    let add = |pack: &mut Value,
               name: &str,
               item_id: String,
               image_id: String,
               projectile: Option<Value>| {
        let mut item = bubble_item.clone();
        item["id"] = json!(item_id);
        item["name"] = json!(format!("{name}Item"));
        item["image"] = json!(image_id);
        let mut image = bubble_image.clone();
        image["id"] = json!(image_id);
        image["name"] = json!(format!("{name}Image"));
        match projectile {
            Some(mut p) => {
                let id = format!("chaos:projectile/{name}");
                p["id"] = json!(id);
                p["name"] = json!(format!("{name}Projectile"));
                image["projectile"] = json!(id);
                pack["projectiles"][&id] = p;
            }
            None => image["projectile"] = Value::Null,
        }
        pack["items"][&item_id] = item;
        pack["images"][&image_id] = image;
    };
    // The core tools: their images' `onFire` is performed by the host.
    for (item, image) in [
        (bri_weapons::HAMMER, "v20.image.hammerimage"),
        (bri_weapons::WRENCH, "v20.image.wrenchimage"),
        (bri_weapons::PRINTER, "v20.image.printgunimage"),
        (bri_weapons::WAND, "v20.image.wandimage"),
    ] {
        let name = image.rsplit('.').next().unwrap().to_string();
        add(&mut pack, &name, item.into(), image.into(), None);
    }
    let variants: [(&str, Value); 5] = [
        // Fast, heavy hitscan-like rounds that eject casings.
        (
            "gun",
            json!({"speed": 400.0, "gravity": 0.0, "ballistic": false, "damage": 25.0, "impulse": 50.0}),
        ),
        // Explodes on everything: player, brick knockouts, vehicle damage.
        (
            "rocket",
            json!({"speed": 60.0, "ballistic": false, "gravity": 0.0, "damage": 30.0,
            "explode_player": true, "explode_death": true, "lifetime_ticks": 600,
            "explosion": {"effect": "bubblePop", "damage": 60.0, "radius": 5.0, "impulse": 2000.0,
                "impulse_radius": 6.0, "impulse_vertical": 500.0, "burn_seconds": 2.0},
            "brick": {"radius": 4.0, "direct": true, "force": 20.0, "max_volume": 400.0, "max_floating_volume": 100.0}}),
        ),
        // Bounces forever until it times out.
        (
            "bouncer",
            json!({"speed": 30.0, "gravity": 1.0, "elasticity": 1.0, "friction": 0.5,
            "bounce_angle": 10.0, "rest_speed": 0.5}),
        ),
        // Sticks where it lands, then blows up.
        (
            "sticky",
            json!({"speed": 25.0, "gravity": 1.0, "min_stick_speed": 1.0, "explode_death": true,
            "lifetime_ticks": 240, "explosion": {"effect": "bubblePop", "damage": 20.0, "radius": 3.0,
            "impulse": 300.0, "impulse_radius": 3.0, "impulse_vertical": 0.0, "burn_seconds": 0.0}}),
        ),
        // Pushes hard and knocks bricks loose without exploding.
        (
            "shove",
            json!({"speed": 80.0, "ballistic": false, "gravity": 0.0, "impulse": 5000.0, "vertical": 3000.0,
            "brick": {"radius": 0.5, "direct": true, "force": 50.0, "max_volume": 100.0, "max_floating_volume": 100.0}}),
        ),
    ];
    for (name, overrides) in variants {
        let mut projectile = bubble_projectile.clone();
        merge(&mut projectile, overrides);
        let item = format!("chaos:weapon/{name}");
        add(
            &mut pack,
            name,
            item.clone(),
            format!("chaos:image/{name}"),
            Some(projectile),
        );
        given.push(item);
    }
    // The gun ejects a shell each shot, the path of the a16 casing crash.
    pack["images"]["chaos:image/gun"]["casing"] = json!("gunShellDebris");
    for state in pack["images"]["chaos:image/gun"]["states"]
        .as_array_mut()
        .context("states")?
    {
        if state["name"] == "Fire" {
            state["eject_shell"] = json!(true);
            state["ticks"] = json!(6);
        }
    }
    let pack = bri_weapons::Pack::from_json(&serde_json::to_vec(&pack)?)?;
    Ok((pack, given))
}

fn merge(target: &mut Value, overrides: Value) {
    match (target, overrides) {
        (Value::Object(t), Value::Object(o)) => {
            for (k, v) in o {
                merge(t.entry(k).or_insert(Value::Null), v);
            }
        }
        (t, o) => *t = o,
    }
}

const FAMILIES: [&str; 8] = [
    "Wheeled",
    "FlyingWheeled",
    "Flying",
    "Horse",
    "Ball",
    "Cannon",
    "Rowboat",
    "Turret",
];

/// One content-free vehicle per spawnable family.
pub fn synthetic_vehicles() -> Result<(bri_vehicles::Pack, Vec<String>)> {
    let hull: Vec<[f32; 3]> = (0..8)
        .map(|i| {
            [
                if i & 1 == 0 { -1.0 } else { 1.0 },
                if i & 2 == 0 { 0.0 } else { 1.2 },
                if i & 4 == 0 { -2.0 } else { 2.0 },
            ]
        })
        .collect();
    let seat = |controls: bool, weapon: bool, z: f32| {
        json!({"node": "mount0", "transform": {"position": [0.0, 1.0, z], "rotation": [0.0, 0.0, 0.0, 1.0]},
            "pose": "sit", "controls": controls, "weapon": weapon})
    };
    let wheel = |x: f32, z: f32, steering: f32, powered: bool| {
        json!({"position": [x, 0.2, z], "radius": 0.5, "rest_length": 0.4, "spring": 60.0, "damping": 8.0,
            "anti_sway": 1.0, "tire": {"static_friction": 1.5, "kinetic_friction": 1.0,
                "lateral_force": 600.0, "lateral_damping": 60.0, "lateral_relaxation": 1.0,
                "longitudinal_force": 600.0, "longitudinal_damping": 60.0,
                "longitudinal_relaxation": 1.0},
            "steering": steering, "powered": powered, "model": "chaos/tire.dts",
            "model_rotation": [0.0, 0.0, 0.0, 1.0]})
    };
    let mut ids = Vec::new();
    let mut definitions = Vec::new();
    for family in FAMILIES {
        let id = format!("v20.vehicle.chaos{}", family.to_ascii_lowercase());
        let wheeled = matches!(family, "Wheeled" | "FlyingWheeled");
        // A flying wheeled vehicle is a wheeled one with the flying fields.
        let (name, family) = (
            family,
            if family == "FlyingWheeled" {
                "Wheeled"
            } else {
                family
            },
        );
        let armed = matches!(family, "Cannon" | "Turret");
        definitions.push(json!({
            "id": id, "datablock": format!("Chaos{name}Vehicle"), "name": format!("Chaos {name}"),
            "family": family,
            "energy": {"maximum": 100.0, "minimum_jet": 10.0, "drain_per_32ms": 1.0, "recharge_per_32ms": 1.0, "jet_force": 500.0},
            "flight": if family == "Flying" { json!({"hover_height": 2.0, "create_hover_height": 2.0, "min_drag": 1.0,
                "max_auto_speed": 20.0, "auto_linear_force": 100.0, "auto_angular_force": 100.0, "auto_input_damping": 0.8,
                "horizontal_surface_force": 10.0, "vertical_surface_force": 10.0, "steering_force": 50.0,
                "steering_roll_force": 20.0, "vertical_thrust_multiple": 2.0}) } else { Value::Null },
            "model": "chaos/body.dts",
            "seats": [seat(true, armed, -0.5), seat(false, false, 0.5), seat(false, armed, 1.0)],
            "wheels": if wheeled { vec![wheel(-1.0, -1.5, 1.0, true), wheel(1.0, -1.5, 1.0, true),
                wheel(-1.0, 1.5, 0.0, true), wheel(1.0, 1.5, 0.0, true)] } else { vec![] },
            "weapon": if armed { json!({"projectile": "chaos:projectile/rocket",
                "muzzle": {"position": [0.0, 1.5, -2.0], "rotation": [0.0, 0.0, 0.0, 1.0]},
                "pivot": [0.0, 1.0, 0.0], "cooldown_ticks": 30, "speed": 60.0, "charge_ticks": 0,
                "charge_steps": 1, "sound": "", "effect": ""}) } else { Value::Null },
            "attachment_model": null, "attachment_collision_hulls": [], "attachment_mount": null,
            "attachment_fallback_seat": null,
            "collision_hulls": [hull.clone()],
            "bounds_min": [-1.0, 0.0, -2.0], "bounds_max": [1.0, 1.2, 2.0],
            "mass": 200.0, "mass_center": [0.0, 0.3, 0.0], "inertia_box": [2.0, 1.2, 4.0],
            "density": 1.0, "drag": 0.5, "friction": 0.8, "restitution": 0.3,
            "max_damage": 120.0, "burn_ticks": 240, "invulnerable_ticks": 60,
            "initial_explosion": null, "final_explosion": null,
            "initial_explosion_offset": 0.0, "final_explosion_offset": 0.0,
            "mount_distance": 3.0, "engine_force": 4000.0, "engine_brake": 500.0, "brake_force": 2000.0,
            "max_speed": 30.0, "reverse_speed": 10.0, "max_steering": 0.6,
            "thrust": 3000.0, "reverse_thrust": 1000.0, "lift": 50.0,
            "yaw_force": 500.0, "pitch_force": 500.0, "roll_force": 500.0, "angular_drag": 2.0,
            "jump_speed": 8.0, "max_side_speed": 5.0, "run_surface_angle": 50.0,
            "impact_threshold": 5.0, "impact_damage": 1.0, "strafe_steering": family == "Wheeled",
            "look_pitch": [-1.5, 1.5], "underwater_speeds": [5.0, 3.0, 3.0],
            "camera": {"max_dist": 8.0, "offset": 2.0, "tilt": 0.1, "lag": 0.1, "decay": 0.5},
            "look_limits": [0.0, 1.0],
            "wheeled_flight": if name == "FlyingWheeled" { json!({"max_forward_vel": 40.0, "max_reverse_vel": 20.0,
                "horizontal_surface_force": 100.0, "vertical_surface_force": 100.0, "stall_speed": 10.0,
                "sled": false}) } else { Value::Null },
            "runover_speed": 5.0, "runover_damage": 20.0, "runover_push": 5.0,
            "protect_direct": false, "protect_radius": false, "protect_burn": false,
            "authored": {}, "adaptations": [],
        }));
        ids.push(id);
    }
    let pack: bri_vehicles::Pack = serde_json::from_value(json!({
        "schema_version": bri_vehicles::schema::SCHEMA_VERSION,
        "definitions": definitions, "assets": [], "evidence": [], "unresolved": [], "animation_aliases": {},
    }))?;
    pack.validate()?;
    Ok((pack, ids))
}

/// The synthetic map, with `bricks` already built on it: a 200 x 200
/// floor, two walls and a pillar to get stuck in.
pub fn synthetic_simulation(bricks: &[bri_world::Brick]) -> Result<Simulation> {
    let mut world = World::new(
        "Chaos".into(),
        "chaos/map".into(),
        vec![
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 0.5],
            [1.0; 4],
        ],
    );
    for (i, brick) in bricks.iter().enumerate() {
        world.bricks.insert(i as u64 + 1, brick.clone());
    }
    world.next_brick_id = bricks.len() as u64 + 1;
    let colliders = vec![
        ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
        ColliderBuilder::cuboid(0.5, 4.0, 20.0).translation(Vector::new(12.0, 4.0, 0.0)),
        ColliderBuilder::cuboid(20.0, 4.0, 0.5).translation(Vector::new(0.0, 4.0, -15.0)),
        ColliderBuilder::cuboid(1.0, 10.0, 1.0).translation(Vector::new(-6.0, 10.0, 6.0)),
    ];
    Simulation::new(world, synthetic_definitions()?, colliders)
}

/// A synthetic session: a 200 x 200 floor, a few walls and a pillar to get
/// stuck in, and the synthetic content above.
pub fn synthetic() -> Result<Fixture> {
    let mut session = Session::new(synthetic_simulation(&[])?);
    let (weapons, items) = synthetic_weapons()?;
    session.set_weapon_pack(weapons)?;
    let mut loadout = ToolInventory::default();
    loadout.slots[3] = Some("chaos:weapon/rocket".into());
    loadout.slots[4] = Some("chaos:weapon/gun".into());
    session.set_spawn_loadout(loadout)?;
    let (vehicles, mut kinds) = synthetic_vehicles()?;
    session.set_vehicle_pack(vehicles)?;
    kinds.push(BOT.into());
    // A grid of spawn points like a map's candidates, one inside the pillar.
    let spawn_points: Vec<Vec3> = (0..16)
        .map(|i| Vec3::new((i % 4) as f32 * 3.0 - 4.5, 0.05, (i / 4) as f32 * 3.0 - 4.5))
        .chain([Vec3::new(-6.0, 1.0, 6.0)])
        .collect();
    session.set_item_bounds(
        items
            .iter()
            .cloned()
            .chain(bri_weapons::CORE_TOOLS.iter().map(|s| s.to_string()))
            .map(|id| {
                (
                    id,
                    bri_weapons::ItemBounds {
                        min: [-0.3, -0.1, -0.5],
                        max: [0.3, 0.2, 0.5],
                    },
                )
            })
            .collect(),
    )?;
    session.set_spawn_points(spawn_points.clone())?;
    let bricks = [PLATE, BRICK, TALL, BASEPLATE, WATER, STONE]
        .map(String::from)
        .to_vec();
    Ok(Fixture {
        session,
        spawn_points,
        bricks,
        vehicles: kinds,
        items,
        saves: Vec::new(),
        environment: bri_package::environment::Environment::empty(),
        extent: 30.0,
    })
}

/// The real game on `map` (a map id such as `v20/add-ons/map_slate/slate.mis`),
/// with the stock saves for overlapping loads.
pub fn content(root: &Path, map: &str) -> Result<Fixture> {
    let palette = (0..64)
        .map(|i| {
            let f = i as f32 / 63.0;
            [
                f,
                1.0 - f,
                (f * 7.0).fract(),
                if i % 9 == 0 { 0.5 } else { 1.0 },
            ]
        })
        .collect();
    let world = World::new("Chaos".into(), map.into(), palette);
    let packages = bri_package::packages::PackageSet::load_root(root)?;
    let weapons =
        bri_net::content_identity::WeaponContent::load(&packages.role_dir(root, "weapons")?)?;
    let dedicated = bri_net::dedicated::load_packages(root, &packages, world)?;
    let mut session = dedicated.session;
    let vehicles = session
        .vehicle_choices()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let items = weapons.item_choices.into_iter().map(|(id, _)| id).collect();
    let bricks = session
        .simulation()
        .definitions
        .entries
        .keys()
        .cloned()
        .collect();
    let mut saves = Vec::new();
    if let Ok(report) = std::fs::read(root.join("worlds-pass-005/report.json")) {
        let report: Value = serde_json::from_slice(&report)?;
        for save in report["saves"].as_array().into_iter().flatten() {
            if let Some(file) = save["file"].as_str()
                && let Ok(world) =
                    bri_world::persistence::load(&root.join("worlds-pass-005").join(file))
            {
                saves.push(world);
            }
        }
    }
    session.set_lan_host(true);
    Ok(Fixture {
        session,
        spawn_points: dedicated.spawn_points,
        bricks,
        vehicles,
        items,
        saves,
        extent: 60.0,
        environment: dedicated.environment,
    })
}
