//! Clear All Bricks from the Admin menu on a big build, in single player:
//! the menu must hear back from the host and the bricks must go. Max saw it
//! stay on "Waiting for host" and freeze.
//! Run with: cargo test -p bri-client --test clear_bricks_flow --release -- --ignored --nocapture
//! Requires converted v20 content and loopback QUIC; never opens a window.
use anyhow::{Context, Result, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_ui::{api::*, models::admin::AdminAction, screens::ScreenId};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SIZE: (u32, u32) = (640, 480);
const KITCHEN: &str = "v20/add-ons/map_kitchen/kitchen.mis";
/// Bricks in the test build: 100 by 100, two layers.
const BRICKS: usize = 20_000;

fn step(app: &mut App, dt: Duration) -> Result<()> {
    app.tick(dt)?;
    app.ui.update(dt.as_millis() as u64);
    ensure!(app.pump()?.is_empty(), "Unexpected native window command");
    Ok(())
}

fn until(
    app: &mut App,
    what: &str,
    limit: Duration,
    ready: impl Fn(&App) -> bool,
) -> Result<Duration> {
    let start = Instant::now();
    let mut previous = start;
    loop {
        let now = Instant::now();
        step(app, now.duration_since(previous))?;
        previous = now;
        if ready(app) {
            let took = start.elapsed();
            eprintln!("{what}: {:.2} s", took.as_secs_f32());
            return Ok(took);
        }
        ensure!(
            start.elapsed() < limit,
            "Timed out waiting for {what}: admin status {:?}, pending {:?}, bricks {:?}, screens {:?}",
            app.ui.core.admin.status,
            app.ui.core.admin.pending,
            app.network_view().map(|v| v.world.bricks.len()),
            app.ui.stack()
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn bricks(app: &App) -> usize {
    app.network_view().map_or(0, |v| v.world.bricks.len())
}

#[test]
#[ignore = "requires converted native v20 content and loopback QUIC; no window/audio device"]
fn clear_all_bricks_from_the_admin_menu_finishes_on_a_big_build() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let artifact = workspace.join("artifacts/clear-bricks");
    let state = artifact.join(format!(
        "state-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    std::fs::create_dir_all(&state)?;
    let mut app = App::load(&workspace.join("content"), &state, SIZE)?;
    app.ui.core.pop(ScreenId::DefaultControls);
    app.ui.core.request(UiAction::HostGame {
        map: KITCHEN.into(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Clear bricks".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    until(&mut app, "Kitchen host", Duration::from_secs(120), |a| {
        matches!(a.ui.core.conn, ConnectionState::InGame { .. })
            && a.network_view()
                .is_some_and(|v| v.poses.contains_key(&v.owner))
    })?;
    until(&mut app, "admin state", Duration::from_secs(30), |a| {
        a.ui.core.admin.snapshot.is_some()
    })?;
    // A big build beside the player, loaded like any save.
    let (player, _) = app.local_motion().context("local player")?;
    let feet = glam::Vec3::from(player.feet);
    let view = app.network_view().context("view")?;
    let map_id = view.world.map_id.clone();
    let mut world = bri_world::World::new("Big".into(), map_id.clone(), view.world.palette.clone());
    let base = [
        (feet.x * 2.0).round() / 2.0 + 5.0,
        (feet.y / 0.2).ceil() * 0.2 + 0.3,
        (feet.z * 2.0).round() / 2.0 + 5.0,
    ];
    let mut id = 1;
    for layer in 0..2 {
        for x in 0..100 {
            for z in 0..100 {
                let brick = bri_world::Brick::new(
                    bri_world::ContentRef::Resolved("v20/brick/brick2x2data".into()),
                    [
                        base[0] + x as f32,
                        base[1] + layer as f32 * 0.6,
                        base[2] + z as f32,
                    ],
                    view.owner,
                );
                world.bricks.insert(id, brick);
                id += 1;
            }
        }
    }
    world.next_brick_id = id;
    let build = bri_world::build::SavedBuild::new(world);
    use sha2::Digest;
    let folder = state
        .join("saves")
        .join(format!("map-{:x}", sha2::Sha256::digest(map_id.as_bytes())));
    std::fs::create_dir_all(&folder)?;
    std::fs::write(folder.join("big.world.json"), serde_json::to_vec(&build)?)?;
    app.ui.core.request(UiAction::LoadBricks {
        map: "Kitchen".into(),
        name: "big.world.json".into(),
        ownership: true,
    });
    until(
        &mut app,
        "the build to load",
        Duration::from_secs(600),
        |a| bricks(a) >= BRICKS,
    )?;
    // Admin menu > Clear Bricks >> lists the brick groups, then Clear All.
    app.ui.core.push(ScreenId::Admin);
    app.ui.core.push(ScreenId::AdminBricks);
    until(&mut app, "the brick groups", Duration::from_secs(30), |a| {
        !a.ui.core.admin.busy() && !a.ui.core.admin.groups.is_empty()
    })?;
    ensure!(
        app.ui
            .core
            .admin_request(AdminAction::ClearAllBricks)
            .is_some(),
        "Clear All Bricks was refused: {:?}",
        app.ui.core.admin.status
    );
    let took = until(
        &mut app,
        "Clear All Bricks",
        Duration::from_secs(120),
        |a| !a.ui.core.admin.busy() && bricks(a) == 0,
    )?;
    ensure!(
        took < Duration::from_secs(10),
        "clearing {BRICKS} bricks took {took:?}"
    );
    eprintln!("status after clearing: {:?}", app.ui.core.admin.status);
    Ok(())
}
