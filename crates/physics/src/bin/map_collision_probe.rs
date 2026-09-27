//! Native interior placement and real solver-floor checks at original spawn points.
use anyhow::{Context, Result, ensure};
use bri_content::{
    interior::Interior,
    scene::{Kind, Scene},
};
use rapier3d::prelude::*;
use std::path::PathBuf;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 2,
        "Usage: map_collision_probe <map-bundle-dir> <report.json>"
    );
    let root = PathBuf::from(&args[0]);
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("bundle.json"))?)?;
    let mut reports = Vec::new();
    for map in bundle["maps"].as_array().context("Missing maps")? {
        let scene: Scene = serde_json::from_slice(&std::fs::read(
            root.join(map["file"].as_str().context("Missing scene file")?),
        )?)?;
        let interiors: Vec<_> = scene
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, Kind::Interior))
            .collect();
        if interiors.is_empty() {
            continue;
        }
        let mut world = bri_physics::new_world();
        let mut triangles = 0;
        for node in interiors {
            let id = node.asset.as_ref().context("Missing interior asset")?;
            let interior: Interior = serde_json::from_slice(&std::fs::read(
                root.join(
                    bundle["assets"][id]
                        .as_str()
                        .context("Missing native interior")?,
                ),
            )?)?;
            interior.validate()?;
            let matrix = glam::Mat4::from_cols_array(&node.transform);
            let geometry = &interior.details[0].collision_triangles;
            triangles += geometry.len();
            world.insert(
                RigidBodyBuilder::fixed(),
                bri_physics::content::interior_collider(&interior.details[0], matrix)?,
            );
        }
        world.detect_collisions(&(), &());
        let spawn = scene
            .nodes
            .iter()
            .find(|n| matches!(n.kind, Kind::Spawn))
            .context("Missing spawn")?;
        let m = &spawn.transform;
        let position = Vector::new(m[12], m[13], m[14]);
        let ray = Ray::new(position + Vector::Y * 5.0, -Vector::Y);
        let (_, intersection) = world
            .query_pipeline()
            .cast_ray_and_get_normal(&ray, 50.0, true)
            .context("No authored floor near original spawn")?;
        let floor = ray.point_at(intersection.time_of_impact);
        ensure!(intersection.normal.y > 0.7, "Spawn floor normal incorrect");
        let (body, _) = world.insert(
            RigidBodyBuilder::dynamic()
                .translation(floor + Vector::Y * 3.0)
                .ccd_enabled(true),
            ColliderBuilder::ball(0.5).restitution(0.0),
        );
        for _ in 0..600 {
            world.step();
        }
        let rest = world.bodies[body].translation();
        let horizontal_drift = ((rest.x - floor.x).powi(2) + (rest.z - floor.z).powi(2)).sqrt();
        ensure!(
            horizontal_drift < 0.001,
            "Spurious flat-floor drift in {}: {horizontal_drift}",
            scene.name
        );
        ensure!(
            (rest.y - floor.y - 0.5).abs() < 0.03,
            "Imported floor contact failed for {}",
            scene.name
        );
        reports.push(serde_json::json!({"map":scene.name,"collision_triangles":triangles,"original_spawn":position.to_array(),"floor":floor.to_array(),"resting_body":rest.to_array(),"horizontal_drift":((rest.x-floor.x).powi(2)+(rest.z-floor.z).powi(2)).sqrt()}));
    }
    ensure!(
        reports.len() >= 2,
        "Expected Bedroom and Kitchen interior checks"
    );
    let output = PathBuf::from(&args[1]);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        output,
        serde_json::to_vec_pretty(
            &serde_json::json!({"status":"passed","maps":reports,"scope":"transformed native interior floor at original spawns and Rapier contacts; no traversal or vehicle behavior acceptance"}),
        )?,
    )?;
    println!(
        "{} native map spawn-floor and solver checks passed",
        reports.len()
    );
    Ok(())
}
