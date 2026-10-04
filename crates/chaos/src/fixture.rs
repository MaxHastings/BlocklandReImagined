//! Worlds for chaos runs. The synthetic one needs no v20 content: a handful
//! of brick shapes (including water and an indestructible one), and the
//! engine crates' made-up test content: every vehicle family
//! ([`bri_vehicles::testing`]), the core tools and an arsenal covering each
//! weapon behaviour ([`bri_weapons::testing`]). With `BRI_CONTENT` the real
//! game content is served instead.
use anyhow::Result;
use bri_sim::{
    definitions::Definitions,
    session::{Session, ToolInventory},
    simulation::Simulation,
};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::Value;
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

pub use bri_sim::testing::{BASEPLATE, BRICK, PLATE, STONE, TALL, WATER};
/// The spawn-brick kind that makes a wandering bot (the Blockhead Bot Add-On's).
pub const BOT: &str = "bot.blockhead";

/// The made-up bricks of [`bri_sim::testing`].
pub fn synthetic_definitions() -> Result<Definitions> {
    Ok(bri_sim::testing::definitions())
}

/// Content-free weapons: [`bri_weapons::testing`]'s made-up pack, and the
/// items that fire a projectile from the hand, which bots are given.
pub fn synthetic_weapons() -> Result<(bri_weapons::Pack, Vec<String>)> {
    use bri_weapons::testing;
    Ok((
        testing::pack(),
        testing::PROJECTILE_WEAPONS
            .iter()
            .map(|id| id.to_string())
            .collect(),
    ))
}

/// Content-free vehicles: [`bri_vehicles::testing`]'s made-up catalog, every
/// gun firing the made-up rocket and naming no sound or effect (which a
/// pack may leave out), and the kinds a spawn brick may hold.
pub fn synthetic_vehicles() -> Result<(bri_vehicles::Pack, Vec<String>)> {
    use bri_vehicles::testing;
    let pack = testing::pack_with(|d| {
        if let Some(weapon) = &mut d.weapon {
            weapon.projectile = bri_weapons::testing::ROCKET_PROJECTILE.into();
            weapon.sound.clear();
            weapon.effect.clear();
        }
    });
    pack.validate()?;
    // Skis come from the skis item and the tumble body from a wreck, never
    // from a spawn brick.
    let ids = testing::ALL
        .iter()
        .filter(|id| ![testing::SKIS, testing::TUMBLE].contains(id))
        .map(|id| id.to_string())
        .collect();
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
    loadout.slots[3] = Some(bri_weapons::testing::ROCKET_ITEM.into());
    loadout.slots[4] = Some(bri_weapons::testing::GUN_ITEM.into());
    session.set_spawn_loadout(loadout)?;
    let (vehicles, mut kinds) = synthetic_vehicles()?;
    session.set_vehicle_pack(
        vehicles,
        bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))?
        .bots,
    )?;
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
    if let Ok(worlds) = packages.role_dir(root, "worlds")
        && let Ok(report) = std::fs::read(worlds.join("report.json"))
    {
        let report: Value = serde_json::from_slice(&report)?;
        for save in report["saves"].as_array().into_iter().flatten() {
            if let Some(file) = save["file"].as_str()
                && let Ok(world) = bri_world::persistence::load(&worlds.join(file))
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
