//! Headless probe: the predicted local player must settle exactly where the
//! authoritative server has it, on a real converted map.
use anyhow::{Result, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_ui::api::*;
use std::{path::Path, thread, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

const BEDROOM: &str = "v20/add-ons/map_bedroom/bedroom.mis";

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
        max_players: 1,
        server_name: "Motion probe".into(),
        password: String::new(),
        admin_password: String::new(),
        super_admin_password: String::new(),
    });
    let start = Instant::now();
    let mut previous = start;
    let mut samples = Vec::new();
    while start.elapsed() < Duration::from_secs(20) {
        let now = Instant::now();
        step(&mut app, now.duration_since(previous))?;
        previous = now;
        if let (Some(view), Some((local, eye))) = (app.network_view(), app.local_motion()) {
            let server = view.poses[&view.owner].player.feet;
            samples.push((start.elapsed().as_secs_f32(), local.feet, server, eye));
            if samples.len() > 600 {
                break;
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
    for s in samples.iter().step_by(40) {
        eprintln!("{:6.2}s local {:?} server {:?} eye {:?}", s.0, s.1, s.2, s.3);
    }
    let last = samples.last().expect("entered the game");
    let gap = glam::Vec3::from(last.1).distance(glam::Vec3::from(last.2));
    ensure!(gap < 0.01, "predicted player drifted {gap} from the server");
    let _ = std::fs::remove_dir_all(&state);
    Ok(())
}
