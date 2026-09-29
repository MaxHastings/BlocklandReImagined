//! A plate planted on the floor under each stock map's spawn rests on it:
//! every map's spawn floor lies on the brick plate lattice.
use anyhow::{Context, Result};
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    map::NativeMap,
    simulation::{Builder, PlantFailure, Simulation},
};
use bri_world::{Brick, ContentRef, World, authority::Actor};
use glam::Vec3;
use std::path::PathBuf;

const BUNDLE: &str = "map-bundle-017";

fn content() -> PathBuf {
    std::env::var_os("BRI_CONTENT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    )
}

/// A 1x1 plate.
fn plate() -> Result<Definitions> {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [1, 1],
        height_plates: 1,
        attachment_rows: vec!["b".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [0.5, 0.2, 0.5],
        }],
    };
    let shape = bri_physics::content::collider(&collision)?
        .build()
        .shared_shape()
        .clone();
    Ok(Definitions {
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
    })
}

#[test]
#[ignore = "needs generated content (map-bundle-017); set BRI_CONTENT"]
fn a_plate_on_each_stock_spawn_floor_rests_flush() -> Result<()> {
    let root = content().join(BUNDLE);
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("bundle.json"))?)?;
    let maps: Vec<_> = bundle["maps"]
        .as_array()
        .context("maps")?
        .iter()
        .filter_map(|m| m["id"].as_str())
        .map(str::to_owned)
        .collect();
    assert_eq!(maps.len(), 14, "{maps:?}");
    let owner = Actor {
        owner: 1,
        ..Default::default()
    };
    for map in &maps {
        let native = NativeMap::load(&root, map)?;
        let spawn = native
            .scene
            .nodes
            .iter()
            .find(|n| matches!(n.kind, bri_content::scene::Kind::Spawn))
            .context("spawn")?;
        let spawn = Vec3::new(
            spawn.transform[12],
            spawn.transform[13],
            spawn.transform[14],
        );
        let mut simulation = Simulation::new(
            World::new("Floor".into(), "floor".into(), vec![[1.0; 4]]),
            plate()?,
            native.colliders,
        )?;
        // The build tool deploys onto the first plate plane at or above the
        // floor it hits (0.002 tolerance); see `Building::fire`.
        let x = ((spawn.x / 0.5).floor() + 0.5) * 0.5;
        let z = ((spawn.z / 0.5).floor() + 0.5) * 0.5;
        let Some(hit) = simulation.target(Vec3::new(x, spawn.y + 0.5, z), Vec3::NEG_Y, 20.0)?
        else {
            // The Slopes: terrain only, streamed separately. Terrain is
            // continuous, so there is no single floor to align.
            assert!(
                map.ends_with("/slopes.mis"),
                "{map}: no interior floor under spawn"
            );
            assert!(!native.terrain.is_empty());
            continue;
        };
        let floor = hit.position.y;
        let bottom = ((floor - 0.002) / 0.2).ceil() * 0.2;
        eprintln!("{map}: floor {floor:.4}, plate bottom {bottom:.4}");
        assert!(
            (bottom - floor).abs() < 0.003,
            "{map}: a plate floats {:.3} above the floor at {floor}",
            bottom - floor
        );
        let builder = Builder {
            actor: &owner,
            position: Vec3::new(x, floor + 2.0, z),
            reach: 50.0,
        };
        let at = |bottom: f32| {
            Brick::new(
                ContentRef::Resolved("plate".into()),
                [x, bottom + 0.1, z],
                1,
            )
        };
        // One plane up it floats: the floor plane is the one it rests on.
        let error = simulation.plant(&builder, at(bottom + 0.2)).unwrap_err();
        assert_eq!(
            error.downcast_ref::<PlantFailure>(),
            Some(&PlantFailure::Float),
            "{map}: {error:#}"
        );
        simulation
            .plant(&builder, at(bottom))
            .with_context(|| format!("{map}: plate on the floor"))?;
    }
    Ok(())
}
