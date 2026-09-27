//! Original-map player motor checks without windows or user input automation.
use anyhow::{Context, Result, ensure};
use bri_content::scene::Kind;
use bri_sim::{
    map::{NativeMap, TerrainStream},
    player::{MoveInput, Player, PlayerTuning},
};
use glam::{Mat4, Vec3};
use std::path::PathBuf;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 2,
        "Usage: player_probe <map-bundle> <report.json>"
    );
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(args[0].join("bundle.json"))?)?;
    let mut reports = Vec::new();
    for entry in bundle["maps"].as_array().context("Missing maps")? {
        let native = NativeMap::load(&args[0], entry["id"].as_str().context("Missing map ID")?)?;
        let anchors = native.spawn_anchors()?;
        let mut physics = bri_physics::new_world();
        for c in native.colliders {
            physics.insert_collider(c, None);
        }
        physics.detect_collisions(&(), &());
        let mut terrain = TerrainStream::new(native.terrain, 0, anchors)?;
        terrain.update(&mut physics);
        let node = native
            .scene
            .nodes
            .iter()
            .find(|n| matches!(n.kind, Kind::Spawn))
            .context("Missing original spawn")?;
        let authored = Mat4::from_cols_array(&node.transform).transform_point3(Vec3::ZERO);
        // Players join at the host's collision-checked candidates, never at the
        // raw marker, which may touch or sit inside authored geometry.
        let spawn = bri_sim::spawn::candidates(&physics, &native.scene, &PlayerTuning::default())
            .with_context(|| format!("Spawn candidates in {}", native.scene.name))?[0];
        let mut player = Player::spawn(&mut physics, 1, spawn, PlayerTuning::default())
            .with_context(|| format!("Player spawn in {}", native.scene.name))?;
        for _ in 0..240 {
            player.step(&mut physics, MoveInput::default())?;
            physics.step();
            terrain.update(&mut physics);
        }
        ensure!(
            player.state().grounded,
            "Player did not settle in {}: {:?}",
            native.scene.name,
            player.state()
        );
        let settled = player.state().clone();
        let origin = Vec3::from(settled.feet) + Vec3::Y * 0.5;
        let filter = rapier3d::prelude::QueryFilter::exclude_kinematic().exclude_sensors();
        let (_, hit) = physics
            .query_pipeline_with_filter(filter)
            .cast_ray(
                &rapier3d::prelude::Ray::new(
                    rapier3d::prelude::Vector::from_array(origin.to_array()),
                    -rapier3d::prelude::Vector::Y,
                ),
                2.0,
                true,
            )
            .context("Player has no floor")?;
        let clearance = hit - 0.5;
        ensure!(
            (-0.02..0.7).contains(&clearance),
            "Player is buried or hovering in {}: {clearance}",
            native.scene.name
        );
        let mut highest = settled.feet[1];
        let mut jumped = false;
        let mut landed = false;
        for i in 0..240 {
            let events = player.step(
                &mut physics,
                MoveInput {
                    jump: i == 0,
                    ..Default::default()
                },
            )?;
            jumped |= events.jumped;
            landed |= events.landed;
            physics.step();
            highest = highest.max(player.state().feet[1]);
        }
        ensure!(
            jumped && landed && highest > settled.feet[1] + 2.0,
            "Original map jump/landing failed in {}",
            native.scene.name
        );
        for _ in 0..120 {
            player.step(
                &mut physics,
                MoveInput {
                    forward: 1.0,
                    crouch: true,
                    ..Default::default()
                },
            )?;
            physics.step();
        }
        let crouched = player.state().clone();
        ensure!(crouched.crouched, "Crouch failed");
        // Measure from the lowest point: the crouch walk may end mid-fall off a ledge.
        let mut jet_rise = 0.0_f32;
        let mut lowest = crouched.feet[1];
        for _ in 0..120 {
            player.step(
                &mut physics,
                MoveInput {
                    jet: true,
                    ..Default::default()
                },
            )?;
            physics.step();
            lowest = lowest.min(player.state().feet[1]);
            jet_rise = jet_rise.max(player.state().feet[1] - lowest);
        }
        ensure!(
            jet_rise > 1.0,
            "Original map jet failed in {}: rose {jet_rise}",
            native.scene.name
        );
        let camera = player.camera(&physics, true);
        ensure!(camera.is_finite(), "Invalid camera");
        reports.push(serde_json::json!({"map":native.scene.name,"original_spawn":spawn.to_array(),"settled":settled,"floor_clearance_at_center":clearance,"jump_rise":highest-settled.feet[1],"crouch_end":crouched,"jet_rise":jet_rise,"authored_spawn":authored.to_array(),"camera":camera.to_array()}));
    }
    ensure!(
        reports.len() == bundle["maps"].as_array().map_or(0, Vec::len),
        "Every original map must be probed"
    );
    if let Some(parent) = args[1].parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &args[1],
        serde_json::to_vec_pretty(
            &serde_json::json!({"status":"passed","maps":reports,"scope":"Headless motor at original spawns: settle, jump/land, crouch movement, jet and bounded camera. Not interactive fidelity acceptance; tuning assumptions and environment adapters remain."}),
        )?,
    )?;
    println!("{} original-map player motor checks passed", reports.len());
    Ok(())
}
