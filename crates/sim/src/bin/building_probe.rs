//! Headless native-content integration. No input/window or Torque readers.
use anyhow::{Context, Result, ensure};
use bri_content::scene::Kind;
use bri_sim::{
    definitions::Definitions,
    map::NativeMap,
    simulation::{Builder, Simulation},
};
use bri_world::{Brick, ContentRef, World, authority::Actor};
use glam::Vec3;
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

fn json(path: impl AsRef<Path>) -> Result<serde_json::Value> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 5,
        "Usage: building_probe <catalog-dir> <native-content> <world-dir> <map-bundle> <report.json>"
    );
    let start = Instant::now();
    let definitions = Definitions::load(&args[0], &args[1])?;
    let definition_count = definitions.entries.len();
    let world_report = json(args[2].join("report.json"))?;
    let mut worlds = Vec::new();
    let mut total = 0;
    // Reuse cooked templates across loads; each instance shares each template's shape.
    let mut available = Some(definitions);
    for entry in world_report["saves"].as_array().context("Missing saves")? {
        let path = args[2].join(entry["file"].as_str().context("Missing world filename")?);
        let load_start = Instant::now();
        let world = bri_world::persistence::load(&path)?;
        let count = world.bricks.len();
        let sim = Simulation::new(world, available.take().unwrap(), vec![])
            .with_context(|| format!("Loading {}", entry["source"]))?;
        let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;
        ensure!(
            sim.physics.colliders.len() == count,
            "World/collider identity count mismatch"
        );
        total += count;
        worlds.push(serde_json::json!({"source":entry["source"],"bricks":count,"load_and_collision_index_ms":load_ms}));
        available = Some(sim.definitions);
    }
    let bundle = json(args[3].join("bundle.json"))?;
    let mut catalog_checks = 0;
    let mut pending_bricks = Vec::new();
    let mut catalog_sim = Simulation::new(
        World::new(
            "Catalog placement".into(),
            "fixture/floor".into(),
            vec![[1.0; 4]],
        ),
        available.take().unwrap(),
        vec![
            rapier3d::prelude::ColliderBuilder::cuboid(1024.0, 0.5, 1024.0)
                .translation(rapier3d::prelude::Vector::new(0.0, -0.5, 0.0)),
        ],
    )?;
    let ids: Vec<_> = catalog_sim.definitions.entries.keys().cloned().collect();
    let administrator = Actor {
        owner: 1,
        administrator: true,
    };
    for name in ids {
        let definition = &catalog_sim.definitions.entries[&name];
        if definition.requires_behavior_adapter {
            pending_bricks.push(name);
            continue;
        }
        let [width, depth] = definition.mesh.footprint_studs.map(|v| v as f32);
        let height = definition.mesh.height_plates as f32;
        for turn in 0..4 {
            let (x, z) = if turn % 2 == 0 {
                (width, depth)
            } else {
                (depth, width)
            };
            let position = [x * 0.25, height * 0.1, z * 0.25];
            let builder = Builder {
                actor: &administrator,
                position: Vec3::from(position) + Vec3::Y,
                reach: 50.0,
            };
            let mut brick = Brick::new(ContentRef::Resolved(name.clone()), position, 1);
            brick.quarter_turns = turn;
            let id = catalog_sim
                .plant(&builder, brick)
                .with_context(|| format!("Catalog placement {name}, turn {turn}"))?;
            catalog_sim.remove(&administrator, id)?;
            catalog_checks += 1;
        }
    }
    available = Some(catalog_sim.definitions);
    let mut map_checks = Vec::new();
    for map in bundle["maps"].as_array().context("Missing maps")? {
        let native = NativeMap::load(&args[3], map["id"].as_str().context("Missing map ID")?)?;
        let anchors = native.spawn_anchors()?;
        let scene = native.scene;
        let colliders = native.colliders;
        let spawn = scene
            .nodes
            .iter()
            .find(|n| matches!(n.kind, Kind::Spawn))
            .context("Missing spawn")?;
        let spawn = glam::Mat4::from_cols_array(&spawn.transform).transform_point3(Vec3::ZERO);
        let world = World::new("Building integration".into(), scene.id, vec![[1.0; 4]]);
        let mut sim = Simulation::new(world, available.take().unwrap(), colliders)?;
        sim.attach_terrain(native.terrain, anchors)?;
        let x = (spawn.x / 0.5).floor() * 0.5 + 0.25;
        let z = (spawn.z / 0.5).floor() * 0.5 + 0.25;
        let ray_origin = Vec3::new(x, spawn.y + 5.0, z);
        let hit = sim
            .target(ray_origin, -Vec3::Y, 50.0)?
            .context("No original map floor")?;
        ensure!(
            hit.brick.is_none() && hit.normal.y > 0.7,
            "Incorrect original floor"
        );
        // A 1x1 brick is three plates tall. Snap its bottom upward to a plate.
        // Sample the footprint corners too: terrain can rise above the center.
        let mut high = hit.position.y;
        for dx in [-0.25, 0.25] {
            for dz in [-0.25, 0.25] {
                let corner = sim
                    .target(ray_origin + Vec3::new(dx, 0.0, dz), -Vec3::Y, 50.0)?
                    .context("Missing footprint support")?;
                high = high.max(corner.position.y);
            }
        }
        let position = [x, (high / 0.2).ceil() * 0.2 + 0.3, z];
        let owner = Actor {
            owner: 1,
            administrator: false,
        };
        let builder = Builder {
            actor: &owner,
            position: ray_origin,
            reach: 50.0,
        };
        let brick = Brick::new(
            ContentRef::Resolved("v20/brick/brick1x1data".into()),
            position,
            1,
        );
        let id = sim
            .plant(&builder, brick.clone())
            .with_context(|| format!("Planting on {} at {position:?}", scene.name))?;
        ensure!(
            sim.target(ray_origin, -Vec3::Y, 50.0)?
                .context("Missing planted brick")?
                .brick
                == Some(id),
            "Planted brick targeting mismatch"
        );
        ensure!(
            sim.plant(&builder, brick.clone()).is_err(),
            "Duplicate planting accepted"
        );
        sim.remove(&owner, id)?;
        ensure!(
            sim.target(ray_origin, -Vec3::Y, 50.0)?
                .context("Missing floor after removal")?
                .brick
                .is_none(),
            "Stale removed brick target"
        );
        sim.plant(&builder, brick)?;
        map_checks.push(serde_json::json!({"map":scene.name,"original_spawn":spawn.to_array(),"floor":hit.position.to_array(),"planted":position,"terrain_tiles":sim.terrain_tiles(),"pending_object_collision":native.pending_objects,"checks":["floor support","native brick target","overlap rejection","remove releases target and occupancy","replant"]}));
        available = Some(sim.definitions);
    }
    ensure!(
        map_checks.len() == 3,
        "Expected Bedroom, Kitchen and Slopes map integration"
    );
    if let Some(parent) = args[4].parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &args[4],
        serde_json::to_vec_pretty(
            &serde_json::json!({"status":"passed","definitions":definition_count,"catalog_plant_remove_cases":catalog_checks,"pending_brick_behaviors":pending_bricks,"worlds":worlds,"total_bricks":total,"maps":map_checks,"elapsed_seconds":start.elapsed().as_secs_f64(),"scope":"Native corpus grid/collider loading, catalog placement in four orientations on a fixture floor, and building near original Bedroom/Kitchen/Slopes spawns. Terrain coverage is the explicit finite cell region; streaming beyond it and listed object adapters remain pending. Headless CPU timings include parsing and collision cooking/indexing; not game frame rates. Player feel is not accepted by this probe."}),
        )?,
    )?;
    println!(
        "{definition_count} definitions, {total} placed bricks across {} worlds, {} map building checks passed",
        worlds.len(),
        map_checks.len()
    );
    Ok(())
}
