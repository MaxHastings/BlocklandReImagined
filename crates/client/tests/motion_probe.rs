//! Headless probe: the predicted local player must settle exactly where the
//! authoritative server has it, on a real converted map.
use anyhow::{Result, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_ui::api::*;
use std::{
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";
/// Samples in a row the server must hold the player's feet unchanged.
const STILL_SAMPLES: usize = 120;
/// Only a hang takes this long.
const HANG: Duration = Duration::from_secs(300);

fn step(app: &mut App, dt: Duration) -> Result<()> {
    app.tick(dt)?;
    app.ui.update(dt.as_millis() as u64);
    ensure!(app.pump()?.is_empty(), "Unexpected window command");
    Ok(())
}

#[test]
#[ignore = "requires converted native v20 content and loopback QUIC; no window"]
fn predicted_local_player_settles_on_the_authoritative_pose() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let state = std::env::temp_dir().join(format!(
        "bri-motion-probe-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    let mut app = App::load(&workspace.join("content"), &state, (640, 480))?;
    app.ui.core.pop(bri_ui::screens::ScreenId::DefaultControls);
    app.ui.core.request(UiAction::HostGame {
        map: BEDROOM.into(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Motion probe".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    // Sample until the server has held the player still for a while, however
    // long loading and landing take on a busy PC; the deadline only catches
    // a hang. A fixed wall-clock window here once ended before the player
    // had landed under load.
    let start = Instant::now();
    let mut previous = start;
    let mut samples = Vec::new();
    let mut still = 0;
    while still < STILL_SAMPLES {
        ensure!(
            start.elapsed() < HANG,
            "the player never settled: {:?}; {} samples",
            app.ui.core.conn,
            samples.len()
        );
        let now = Instant::now();
        step(&mut app, now.duration_since(previous))?;
        previous = now;
        if let (Some(view), Some((local, eye))) = (app.network_view(), app.local_motion()) {
            let server = view.poses[&view.owner].player.feet;
            let moved = samples
                .last()
                .is_none_or(|last: &(f32, [f32; 3], [f32; 3], _)| {
                    glam::Vec3::from(last.2).distance(glam::Vec3::from(server)) > 1e-4
                });
            still = if moved || !local.grounded {
                0
            } else {
                still + 1
            };
            samples.push((start.elapsed().as_secs_f32(), local.feet, server, eye));
        }
        thread::sleep(Duration::from_millis(5));
    }
    for s in samples.iter().step_by(40) {
        eprintln!(
            "{:6.2}s local {:?} server {:?} eye {:?}",
            s.0, s.1, s.2, s.3
        );
    }
    let last = samples.last().expect("sampled the player");
    let gap = glam::Vec3::from(last.1).distance(glam::Vec3::from(last.2));
    ensure!(gap < 0.01, "predicted player drifted {gap} from the server");
    let _ = std::fs::remove_dir_all(&state);
    Ok(())
}
