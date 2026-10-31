//! Reproduce a private saved bot encounter through the normal headless host,
//! Load Bricks, saved minigame and admin placement paths. Never writes inputs,
//! opens a window or invents a bot objective. The report records actual outcomes;
//! a successful probe command is not an assertion that bots reached the player.
use anyhow::{Context, Result, ensure};
use bri_sim::{
    player::MoveInput,
    session::{CameraView, Command, Session},
};
use glam::Vec3;
use serde_json::{Value, json};
use std::{path::PathBuf, time::Instant};

fn sample(session: &Session) -> Value {
    let tick = session.simulation().state().tick;
    json!({
        "tick": tick,
        "motion": session.motion_states(),
        "vitals": session.vitals(),
        "bots": session.bot_thoughts().into_iter().map(|thought| json!({
            "owner": thought.bot,
            "visible": thought.visible,
            "behaviour": thought.behaviour,
            "goal": thought.goal,
            "next": thought.next,
            "route_short": thought.route_short,
            "route_end": thought.route_end,
            "path_steps": thought.path_steps,
            "search_phase": thought.search_phase,
            "input": thought.input,
            "why": thought.why(tick),
        })).collect::<Vec<_>>(),
        "bot_failures": session.bot_step_failures(),
    })
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 7,
        "Usage: bot_scenario_probe <content> <save> <feet-x> <feet-y> <feet-z> <ticks> <report.json>"
    );
    let content = PathBuf::from(&args[0]);
    let save = PathBuf::from(&args[1]);
    let target = Vec3::new(args[2].parse()?, args[3].parse()?, args[4].parse()?);
    let ticks: u64 = args[5].parse()?;
    ensure!(
        target.is_finite() && (1..=14400).contains(&ticks),
        "Invalid target/tick budget"
    );
    let build = bri_world::persistence::load_startup(&save)?;
    let saved_bricks = build.world.bricks.len();
    let mut world = bri_world::World::new(
        "Bot scenario".into(),
        build.world.map_id.clone(),
        build.world.palette.clone(),
    );
    // A new host then Load Bricks, matching the playable save-loading path.
    world.tick = 0;
    let start = Instant::now();
    let mut host = bri_net::dedicated::load(&content, world)?;
    ensure!(
        host.unresolved_items == 0,
        "Host has {} unresolved items",
        host.unresolved_items
    );
    // The stock Bedroom currently reports unsupported clock schedules. Keep
    // that fidelity limit explicit; it does not prevent observing bot routes.
    let pending_objects = host.pending_objects.clone();
    let environment = format!("{:?}", host.environment.digest());
    let session = &mut host.session;
    let owner = session.join("Scenario target".into(), target, true)?;
    session.command(
        owner,
        1,
        Command::LoadBuild {
            build: Box::new(build),
            ownership: false,
        },
    )?;
    let mut sequence = 0;
    while session.build_loading() {
        sequence += 1;
        session.movement(owner, sequence, MoveInput::default())?;
        session.step()?;
        ensure!(
            sequence <= 3600,
            "Load Bricks exceeded its bounded setup budget"
        );
    }
    // Let the saved game's initial spawn/reset effects settle before placing
    // the target. Otherwise its queued spawn can undo the diagnostic placement.
    for _ in 0..120 {
        sequence += 1;
        session.movement(owner, sequence, MoveInput::default())?;
        session.step()?;
    }
    let before = session
        .motion_states()
        .into_iter()
        .find(|(state, _)| state.owner == owner)
        .context("Missing target body")?
        .0;
    let offset = before.eye(
        session
            .player_tuning(owner)
            .context("Missing target tuning")?,
    ) - Vec3::from(before.feet);
    session.command(
        owner,
        2,
        Command::DropPlayerAtCamera(Some(CameraView {
            eye: (target + offset).to_array(),
            yaw: 0.0,
            pitch: 0.0,
        })),
    )?;
    let mut snapshots = vec![sample(session)];
    for tick in 1..=ticks {
        sequence += 1;
        session.movement(owner, sequence, MoveInput::default())?;
        session.step()?;
        if tick % 120 == 0 || tick == ticks {
            snapshots.push(sample(session));
        }
    }
    let report = json!({
        "schema_version": 1,
        "save": save, "environment": environment, "target": target.to_array(),
        "owner": owner, "saved_bricks": saved_bricks,
        "placed_bricks": session.simulation().state().bricks.len(),
        "set_aside": session.simulation().state().unloaded.len(),
        "merge_notes": host.merge_notes, "pending_map_behaviour": pending_objects, "ticks": ticks,
        "elapsed_seconds": start.elapsed().as_secs_f64(), "snapshots": snapshots,
    });
    std::fs::write(&args[6], serde_json::to_vec_pretty(&report)?)?;
    println!(
        "Recorded {} bot snapshots to {}",
        report["snapshots"].as_array().unwrap().len(),
        args[6]
    );
    Ok(())
}
