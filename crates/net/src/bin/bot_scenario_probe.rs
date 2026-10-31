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

fn static_route_audit(session: &Session, target: Vec3) -> Result<Value> {
    use bri_sim::nav::{Body, Found, Ground, Nav, Search};
    let states = session.motion_states();
    let state = states
        .iter()
        .map(|(state, _)| state)
        .find(|state| session.is_bot(state.owner) && state.grounded)
        .context("Static route audit needs a grounded bot")?;
    let tuning = session
        .player_tuning(state.owner)
        .context("Missing bot tuning")?;
    let body = Body::of(tuning, 1.0);
    let simulation = session.simulation();
    let terrain = |o, d, r| simulation.terrain_ray(o, d, r);
    let waters = simulation.liquids();
    let passable = simulation.bot_passable();
    let solid = |handle, _: &_| !passable.contains(&handle);
    let ground = Ground {
        physics: &simulation.physics,
        terrain: &terrain,
        passages: simulation.passages(),
        waters: &waters,
        bodies: &[],
        motions: &[],
        solid: &solid,
    };
    let mut nav = Nav::default();
    let mut search = Search::with(
        Vec3::from(state.feet),
        target,
        72.0,
        bri_sim::route::Costs {
            swim: Some(bri_sim::route::Swim::of(tuning)),
            jets: None,
            veer: 0,
        },
    );
    let start = Instant::now();
    let mut stages = Vec::new();
    let mut resumes = 0;
    let mut finished = false;
    for _ in 0..1024 {
        nav.begin_tick();
        if let Some(found) = search.step(&mut nav, &ground, &body) {
            let reached = matches!(found, Found::Path(_));
            let path = match found {
                Found::Path(path) | Found::Partial(path) => path,
                Found::Nowhere => Vec::new(),
            };
            let resumed = !reached && resumes < 8 && search.resume();
            stages.push(json!({"reached": reached, "resumed": resumed,
                "explored": search.explored(), "path": path.iter().map(|way| way.feet.to_array()).collect::<Vec<_>>() }));
            if !resumed {
                finished = true;
                break;
            }
            resumes += 1;
        }
        if start.elapsed().as_secs_f64() > 10.0 {
            break;
        }
    }
    Ok(
        json!({"owner": state.owner, "from": state.feet, "target": target.to_array(),
        "static_only": true, "jets": false, "body": format!("{body:?}"),
        "finished": finished, "stages": stages}),
    )
}

fn sample(session: &Session) -> Value {
    let tick = session.simulation().state().tick;
    json!({
        "tick": tick,
        "motion": session.motion_states(),
        "vitals": session.vitals(),
        "bots": session.bot_thoughts().into_iter().map(|thought| json!({
            "owner": thought.bot,
            "visible": thought.visible,
            "remembered": thought.remembered.map(|e| json!({
                "subject": e.subject, "position": e.position,
                "observed": e.observed, "expires": e.expires,
            })),
            "behaviour": thought.behaviour,
            "goal": thought.goal,
            "next": thought.next,
            "route_short": thought.route_short,
            "route_end": thought.route_end,
            "path_steps": thought.path_steps,
            "searching": thought.searching,
            "search_nodes": thought.search_nodes,
            "search_phase": thought.search_phase,
            "input": thought.input,
            "why": thought.why(tick),
        })).collect::<Vec<_>>(),
        "bot_failures": session.bot_step_failures(),
    })
}

fn main() -> Result<()> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let audit = args.last().is_some_and(|arg| arg == "--static-route-audit");
    let trace = args.last().is_some_and(|arg| arg == "--every-tick");
    if audit || trace {
        args.pop();
    }
    ensure!(
        args.len() == 7,
        "Usage: bot_scenario_probe <content> <save> <feet-x> <feet-y> <feet-z> <ticks> <report.json> [--static-route-audit|--every-tick]"
    );
    let content = PathBuf::from(&args[0]);
    let save = PathBuf::from(&args[1]);
    let target = Vec3::new(args[2].parse()?, args[3].parse()?, args[4].parse()?);
    let ticks: u64 = args[5].parse()?;
    ensure!(
        target.is_finite() && (1..=14400).contains(&ticks),
        "Invalid target/tick budget"
    );
    ensure!(
        !trace || ticks <= 3600,
        "Every-tick trace is bounded to 3600 ticks"
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
    let route_audit = audit
        .then(|| static_route_audit(session, target))
        .transpose()?;
    for tick in 1..=ticks {
        sequence += 1;
        session.movement(owner, sequence, MoveInput::default())?;
        session.step()?;
        if trace || tick % 120 == 0 || tick == ticks {
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
        "static_route_audit": route_audit,
    });
    std::fs::write(&args[6], serde_json::to_vec_pretty(&report)?)?;
    println!(
        "Recorded {} bot snapshots to {}",
        report["snapshots"].as_array().unwrap().len(),
        args[6]
    );
    Ok(())
}
