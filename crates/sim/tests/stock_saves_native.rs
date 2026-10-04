//! Every stock v20 save still loads onto its map once the map's floor is
//! moved onto the plate lattice, and the bricks that rest on the map floor
//! stay resting on it: none sinks in or is left floating.
use anyhow::{Context, Result};
use bri_sim::{definitions::Definitions, map::NativeMap, simulation::Simulation};
use bri_world::World;
use glam::Vec3;
use std::path::PathBuf;

fn content() -> PathBuf {
    std::env::var_os("BRI_CONTENT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    )
}

/// (floor, gap) for each brick with map floor within a quarter unit of its
/// bottom: the floor height under its centre and how far its bottom sits
/// above it. Gaps of a plate or more are bricks standing on other bricks.
fn floor_gaps(
    map: &Simulation,
    world: &World,
    definitions: &Definitions,
) -> Result<Vec<(f32, f32, bri_world::Brick)>> {
    let mut gaps = vec![];
    for brick in world.bricks.values() {
        let height = definitions.get(brick)?.mesh.height_plates as f32 * 0.2;
        let bottom = brick.position[1] - height * 0.5;
        let [x, _, z] = brick.position;
        let Some(hit) = map.target(Vec3::new(x, bottom + 0.25, z), Vec3::NEG_Y, 0.5)? else {
            continue;
        };
        if hit.normal.y > 0.9 {
            gaps.push((hit.position.y, bottom - hit.position.y, brick.clone()));
        }
    }
    Ok(gaps)
}

#[test]
#[ignore = "requires generated v20 content (map-bundle-017, worlds-pass-006, or BRI_CONTENT)"]
fn stock_saves_load_and_rest_on_the_lifted_floors() -> Result<()> {
    let root = content();
    let definitions = Definitions::load(
        &bri_package::testing::pack_dir(&root, "brick_catalog"),
        &bri_package::testing::pack_dir(&root, "geometry"),
    )?;
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(
        bri_package::testing::pack_dir(&root, "worlds").join("report.json"),
    )?)?;
    let mut saves: Vec<(String, String, World)> = vec![];
    for save in report["saves"].as_array().context("saves")? {
        let source = save["source"]
            .as_str()
            .context("source")?
            .replace('\\', "/");
        let map = match source
            .split('/')
            .next()
            .unwrap()
            .to_ascii_lowercase()
            .as_str()
        {
            "bedroom" => "v20/add-ons/map_bedroom/bedroom.mis",
            "kitchen" => "v20/add-ons/map_kitchen/kitchen.mis",
            "slate" => "v20/add-ons/map_slate/slate.mis",
            other => panic!("unexpected save folder {other}"),
        };
        let world = bri_world::persistence::load(
            &bri_package::testing::pack_dir(&root, "worlds")
                .join(save["file"].as_str().context("file")?),
        )?;
        assert_eq!(
            world.bricks.len() as u64,
            save["bricks"].as_u64().unwrap(),
            "{source}"
        );
        saves.push((source, map.into(), world));
    }
    assert_eq!(saves.len(), 35);
    let (_, part1, part2) =
        bri_sim::tutorial::load_pack(&bri_package::testing::pack_dir(&root, "tutorial"))?;
    for (name, world) in [("Tutorial part 1", part1), ("Tutorial part 2", part2)] {
        saves.push((name.into(), bri_sim::tutorial::MAP_ID.into(), world));
    }
    let bundle = bri_package::testing::pack_dir(&root, "map_bundle");
    let mut maps = std::collections::BTreeMap::new();
    for (source, map_id, world) in saves {
        if !maps.contains_key(&map_id) {
            let native = NativeMap::load(&bundle, &map_id)?;
            let map = Simulation::new(
                World::new("Map".into(), "map".into(), vec![[1.0; 4]]),
                definitions.clone(),
                native.colliders.clone(),
            )?;
            maps.insert(map_id.clone(), (native, map));
        }
        let (native, map) = maps.get_mut(&map_id).unwrap();
        let count = world.bricks.len();
        let loaded = Simulation::new(world.clone(), definitions.clone(), native.colliders.clone())
            .with_context(|| format!("{source}: load"))?;
        assert_eq!(loaded.state().bricks.len(), count, "{source}: bricks lost");
        assert!(
            loaded.state().unloaded.is_empty(),
            "{source}: unloaded bricks"
        );
        // A load plants every brick and skips those that overlap one already
        // placed, as v20 does; no stock save loses any. (With the ramp grid
        // read back to front, five lost 43 bricks.)
        let placed = map.drop_overlapping(world.bricks.values().cloned().collect())?;
        assert_eq!(
            placed.len(),
            count,
            "{source}: bricks skipped as overlapping"
        );
        let gaps = floor_gaps(map, &world, &definitions)?;
        // Placed again by hand, every brick resting on the map floor (within
        // half a plate, as v20 rests them) plants: no Buried or Float refusal.
        let owner = bri_world::authority::Actor {
            owner: 1,
            ..Default::default()
        };
        let mut refused = std::collections::BTreeMap::<String, usize>::new();
        for (_, gap, brick) in gaps.iter().filter(|(_, g, _)| g.abs() <= 0.1 + 1e-4) {
            let builder = bri_sim::simulation::Builder {
                actor: &owner,
                position: Vec3::from(brick.position) + Vec3::Y * 2.0,
                reach: 1000.0,
            };
            let mut brick = brick.clone();
            brick.owner = 1;
            brick.color = 0;
            brick.print = None;
            brick.name = None;
            match map.plant(&builder, brick) {
                Ok(id) => map.remove(&owner, id)?,
                Err(error) => {
                    *refused
                        .entry(format!("{error} (gap {gap:.3})"))
                        .or_default() += 1;
                }
            }
        }
        assert!(
            refused
                .keys()
                .all(|k| !k.contains("buried") && !k.contains("floating")),
            "{source}: {refused:?}"
        );
        let flush = gaps.iter().filter(|(_, g, _)| g.abs() < 0.003).count();
        let deepest = gaps.iter().map(|(_, g, _)| *g).fold(0.0f32, f32::min);
        eprintln!(
            "{source}: {count} bricks load; {} over the map floor, {flush} flush, deepest dip {deepest:.3}",
            gaps.len()
        );
        if map_id.contains("bedroom") {
            // The reported gap: nothing over the carpet hovers above it.
            let carpet: Vec<_> = gaps
                .iter()
                .filter(|(floor, _, _)| (floor - 286.4).abs() < 1e-3)
                .map(|(_, g, _)| *g)
                .collect();
            assert!(
                carpet.iter().all(|g| g.abs() < 0.003 || *g > 0.19),
                "{source}: a brick hovers over the carpet: {carpet:?}"
            );
            assert!(carpet.iter().any(|g| g.abs() < 0.003) || carpet.is_empty());
        }
    }
    Ok(())
}
