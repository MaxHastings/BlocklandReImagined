//! The soccer field Maxwell loads from Load Bricks: listed under Slate from
//! the generated worlds pack (the repository's `saves/Slate/`, which
//! `tools/bootstrap.py` carries into it), read as the menu reads it, and
//! loaded by the host's Load Bricks command on the real v20 bricks. Every
//! brick is placed where it was saved, and every pad, goal event and the
//! game with its teams survive.
//! Run: cargo test -p bri-client --test bundled_soccer_field -- --ignored
use anyhow::{Context, Result, ensure};
use bri_sim::{player::MoveInput, session::Command};
use std::path::{Path, PathBuf};

fn generated_content() -> PathBuf {
    std::env::var_os("BRI_CONTENT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    )
}

/// What a brick of the field is, for comparing saved and placed bricks
/// (where it stands is compared apart: a load may move a brick saved off
/// the real brick's grid by under half a cell).
fn shape(b: &bri_world::Brick) -> String {
    format!(
        "{:?} {} {:?} {:?} {} {:?}",
        b.definition,
        b.quarter_turns,
        b.name,
        b.vehicle.as_ref().map(|v| (&v.vehicle, v.team)),
        b.events.len(),
        b.rule_region
    )
}

/// Whether `placed` stands where `saved` was saved, within half a stud
/// and half a plate.
fn near(placed: &bri_world::Brick, saved: &bri_world::Brick) -> bool {
    (0..3).all(|a| (placed.position[a] - saved.position[a]).abs() <= [0.25, 0.1, 0.25][a] + 1e-4)
}

#[test]
#[ignore = "requires generated v20 content with the bundled builds (python tools/bootstrap.py)"]
fn the_bundled_soccer_field_loads_whole_from_load_bricks() -> Result<()> {
    let root = generated_content();
    let content = bri_client::content::ClientContent::load(&root)?;
    let state = std::env::temp_dir().join(format!("bri-bundled-field-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    let store = bri_client::saves::Store::new(&state, &content, None);
    // Load Bricks lists it under the Slate, as the menu shows it.
    let entry = store
        .list()?
        .into_iter()
        .find(|e| e.info.name == "Soccer 2v2.world.json" && e.info.map == "Slate")
        .context("Load Bricks lists Soccer 2v2 under Slate: rerun python tools/bootstrap.py")?;
    let build = bri_client::saves::Store::read(&entry)?;
    let saved_bricks: Vec<bri_world::Brick> = build.world.bricks.values().cloned().collect();
    let mut saved: Vec<String> = saved_bricks.iter().map(shape).collect();
    saved.sort();
    ensure!(!saved.is_empty(), "the field has bricks");
    // The host's server on the Slate, as single player starts it.
    let packages = bri_package::packages::PackageSet::load_root(&root)?;
    let schema = bri_world::World::new(
        "Soccer".into(),
        entry.map_id.clone(),
        build.world.palette.clone(),
    );
    let dedicated = bri_net::dedicated::load_packages(&root, &packages, schema)?;
    let spawn = dedicated.spawn_points[0];
    let mut session = dedicated.session;
    let host = session.join("Host".into(), spawn, true)?;
    session.command(
        host,
        1,
        Command::LoadBuild {
            build: Box::new(build),
            ownership: false,
        },
    )?;
    for seq in 1..=600u64 {
        session.movement(host, seq, MoveInput::default())?;
        session.step()?;
        if !session.build_loading() && seq > 10 {
            break;
        }
    }
    let mut placed: Vec<String> = session
        .simulation()
        .state()
        .bricks
        .values()
        .map(shape)
        .collect();
    placed.sort();
    let chat: Vec<String> = session
        .snapshot()
        .chat
        .iter()
        .map(|c| format!("{c:?}"))
        .collect();
    ensure!(
        placed == saved,
        "every brick placed as saved ({} of {}); load said {chat:?}\nmissing: {:?}",
        placed.len(),
        saved.len(),
        saved
            .iter()
            .filter(|s| !placed.contains(s))
            .collect::<Vec<_>>()
    );
    let world = session.simulation().state();
    for b in &saved_bricks {
        ensure!(
            world
                .bricks
                .values()
                .any(|p| shape(p) == shape(b) && near(p, b)),
            "{:?} stands where it was saved",
            b.name
        );
    }
    let pads = placed.iter().filter(|s| s.contains("Some((")).count();
    ensure!(pads == 5, "the ball pad and four bot pads: {pads}");
    let games = session.minigame_views();
    ensure!(games.len() == 1, "the save brings its game");
    let teams: Vec<_> = games[0].teams.iter().map(|t| t.name.clone()).collect();
    ensure!(teams == ["Blue", "Red"], "with its teams: {teams:?}");
    let _ = std::fs::remove_dir_all(&state);
    Ok(())
}
