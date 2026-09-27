//! Native Slopes collision, periodic seams and solver contact; no gameplay input.
use anyhow::{Context, Result, ensure};
use bri_content::{
    Terrain,
    scene::{Kind, Scene},
    terrain_mesh,
};
use glam::{Mat4, Vec3};
use rapier3d::prelude::*;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 2,
        "Usage: terrain_collision_probe <native-map-bundle> <report.json>"
    );
    let root = PathBuf::from(&args[0]);
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("bundle.json"))?)?;
    let map = bundle["maps"]
        .as_array()
        .context("Missing maps")?
        .iter()
        .find(|m| m["id"].as_str().is_some_and(|s| s.ends_with("/slopes.mis")))
        .context("Missing Slopes")?;
    let scene: Scene = serde_json::from_slice(&std::fs::read(
        root.join(map["file"].as_str().context("Missing scene")?),
    )?)?;
    let node = scene
        .nodes
        .iter()
        .find(|n| matches!(n.kind, Kind::Terrain))
        .context("Missing terrain")?;
    let id = node.asset.as_ref().context("Missing terrain ID")?;
    let terrain: Terrain = serde_json::from_slice(&std::fs::read(
        root.join(
            bundle["assets"][id]
                .as_str()
                .context("Missing terrain file")?,
        ),
    )?)?;
    terrain.validate()?;
    let spacing: f32 = node
        .properties
        .get("squaresize")
        .context("Missing spacing")?
        .parse()?;
    let transform = Mat4::from_cols_array(&node.transform);
    let origin = transform.transform_point3(Vec3::ZERO);
    let region = [-64, -64, 384, 384];
    let mut world = bri_physics::new_world();
    world.insert(
        RigidBodyBuilder::fixed(),
        bri_physics::content::terrain_collider(&terrain, spacing, region, transform)?,
    );
    world.detect_collisions(&(), &());
    let mut coordinates: Vec<f32> = (-63..319).step_by(7).map(|v| v as f32 + 0.37).collect();
    coordinates.extend([-0.001, 0.0, 0.001, 255.999, 256.0, 256.001]);
    let mut max_error = 0.0_f32;
    let mut rays = 0;
    for column in &coordinates {
        for row in &coordinates {
            let x = column * spacing;
            let z = -row * spacing;
            let expected = terrain_mesh::height(&terrain, spacing, x, z) + origin.y;
            let ray = Ray::new(
                Vector::new(x + origin.x, expected + 100.0, z + origin.z),
                -Vector::Y,
            );
            let (_, hit) = world
                .query_pipeline()
                .cast_ray_and_get_normal(&ray, 200.0, true)
                .context("Terrain ray missed, including periodic seam")?;
            let error = (ray.point_at(hit.time_of_impact).y - expected).abs();
            max_error = max_error.max(error);
            rays += 1;
            ensure!(
                error < 0.002,
                "Terrain interpolation/collision disagreement: {error} at {column},{row}"
            );
            ensure!(hit.normal.y > 0.0, "Terrain winding reversed");
        }
    }
    let spawn = scene
        .nodes
        .iter()
        .find(|n| matches!(n.kind, Kind::Spawn))
        .context("Missing spawn")?;
    let spawn = Mat4::from_cols_array(&spawn.transform).transform_point3(Vec3::ZERO);
    let ray = Ray::new(
        Vector::from_array((spawn + Vec3::Y * 5.0).to_array()),
        -Vector::Y,
    );
    let (_, hit) = world
        .query_pipeline()
        .cast_ray_and_get_normal(&ray, 50.0, true)
        .context("No floor near original Slopes spawn")?;
    let floor = ray.point_at(hit.time_of_impact);
    ensure!(
        (spawn.y - floor.y).abs() < 5.0,
        "Original Slopes spawn far from terrain"
    );
    // Restrict horizontal motion only for this solver test: a free ball should
    // roll downhill, so asserting zero drift would test the wrong behavior.
    let (body, _) = world.insert(
        RigidBodyBuilder::dynamic()
            .translation(floor + Vector::Y * 3.0)
            .enabled_translations(false, true, false)
            .lock_rotations()
            .ccd_enabled(true),
        ColliderBuilder::ball(0.5).restitution(0.0),
    );
    for _ in 0..600 {
        world.step();
    }
    let rest = world.bodies[body].translation();
    let expected_rest = floor.y + 0.5 / hit.normal.y;
    ensure!(
        (rest.y - expected_rest).abs() < 0.03,
        "Sloped terrain solver contact disagrees with plane normal"
    );
    let output = PathBuf::from(&args[1]);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        output,
        serde_json::to_vec_pretty(
            &serde_json::json!({"status":"passed","map":scene.name,"region":region,"rays":rays,"maximum_height_error":max_error,"origin":origin.to_array(),"original_spawn":spawn.to_array(),"floor":floor.to_array(),"floor_normal":hit.normal.to_array(),"rest":rest.to_array(),"expected_rest_y":expected_rest,"scope":"native terrain triangle interpolation vs Rapier queries across repeated borders; vertical-only solver drop at original spawn; no player traversal acceptance"}),
        )?,
    )?;
    println!(
        "Slopes: {rays} terrain rays passed, maximum error {max_error}, spawn floor {}",
        floor.y
    );
    Ok(())
}
