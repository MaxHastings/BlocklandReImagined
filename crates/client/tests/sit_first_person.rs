//! First person while sitting with `/sit`: the camera follows the body down
//! into the sit pose, as v20's does. `Player::getCameraTransform` at `pos` 0
//! (blocklandv20.exe 0x5ab7d0) sees from the posed `eye` node, through every
//! thread the body plays: the held `sit` action and, crouching while
//! sitting, the crouch thread beside it. So first and third person show
//! the same body: the view drops with the sit, crouching while sitting
//! keeps it wherever the third-person body's eye is (on v20's rig the sit
//! holds the eye and only the arms rise), and standing up hands the eye back
//! to the moving body. Before, the view stayed at standing height
//! while the body sat.
//! Runs on the made-up content root; the ignored variant on the generated
//! v20 content (`-- --ignored`, BRI_CONTENT or content/). A hosted game over
//! loopback, no window, no GPU.
use anyhow::{Context, Result, ensure};
use bri_client::{app::App, platform::PlatformApp};
use bri_ui::{api::*, screens::ScreenId};
use glam::Vec3;
use std::time::Duration;

#[macro_use]
mod support;
use support::{content_root::ContentRoot, wait};

synthetic_and_content!(ContentRoot: the_first_person_eye_sits_and_crouches_with_the_body);

const SLATE: &str = "v20/add-ons/map_slate/slate.mis";

fn request(app: &mut App, action: UiAction) -> Result<()> {
    app.ui.core.request(action);
    ensure!(app.pump()?.is_empty(), "Unexpected window command");
    app.ui.update(0);
    Ok(())
}
fn step(apps: &mut [&mut App], dt: Duration) -> Result<()> {
    for app in apps.iter_mut() {
        app.tick(dt)?;
        app.ui.update(dt.as_millis() as u64);
        ensure!(app.pump()?.is_empty(), "Unexpected window command");
    }
    Ok(())
}
fn run_for(app: &mut App, seconds: f32) -> Result<()> {
    wait::run_for(&mut [app], Duration::from_secs_f32(seconds), step)
}
fn in_game(app: &App) -> bool {
    matches!(app.ui.core.conn, ConnectionState::InGame { .. })
        && app
            .network_view()
            .is_some_and(|v| v.poses.contains_key(&v.owner))
}
fn sitting(app: &App) -> bool {
    app.network_view()
        .and_then(|v| v.vitals.get(&v.owner).map(|v| v.sitting))
        .unwrap_or(false)
}
fn held(app: &mut App, control: HeldControl, down: bool) -> Result<()> {
    request(app, UiAction::Game(GameAction::Held { control, down }))
}
/// The first-person eye and the drawn body's `Eye` node, over the feet.
fn eyes(app: &App) -> Result<(Vec3, Vec3, Vec3)> {
    let (state, eye) = app.local_motion().context("no local player")?;
    let owner = app.network_view().context("view")?.owner;
    let node = app
        .avatar_node(owner, "Eye")
        .context("no Eye node")?
        .w_axis
        .truncate();
    let feet = Vec3::from(state.feet);
    Ok((eye.context("no eye")? - feet, node - feet, feet))
}

fn the_first_person_eye_sits_and_crouches_with_the_body(f: &ContentRoot) -> Result<()> {
    let scratch = f.state()?;
    let state = scratch.path().join("Host");
    let mut app = App::load(&f.root, &state, (320, 240))?;
    app.ui.core.pop(ScreenId::DefaultControls);
    app.host_on_any_port();
    request(
        &mut app,
        UiAction::HostGame {
            map: SLATE.into(),
            mode: ServerMode::Lan,
            game_mode: None,
            max_players: 1,
            server_name: "Sit first person".into(),
            password: String::new(),
            admin_password: String::new(),
            super_admin_password: String::new(),
        },
    )?;
    wait::until(
        &mut [&mut app],
        "host in game",
        Duration::from_secs(180),
        step,
        |a| Ok(in_game(a[0])),
    )?;
    if app.controls.third_person_view() {
        request(
            &mut app,
            UiAction::Game(GameAction::ToggleFirstPerson { fast: true }),
        )?;
    }
    run_for(&mut app, 2.0)?;
    let (standing, _, _) = eyes(&app)?;

    request(
        &mut app,
        UiAction::Game(GameAction::Emote { name: "sit".into() }),
    )?;
    wait::until(
        &mut [&mut app],
        "sitting",
        Duration::from_secs(10),
        step,
        |a| Ok(sitting(a[0])),
    )?;
    run_for(&mut app, 1.0)?;
    let (sat, node, _) = eyes(&app)?;
    println!("standing eye {standing}, sitting eye {sat}, sitting Eye node {node}");
    ensure!(
        sat.distance(node) < 1e-3,
        "sitting first-person eye {sat} is not the sitting body's Eye node {node}"
    );
    ensure!(
        sat.y < standing.y - 0.1,
        "sitting first-person eye {sat} stayed near standing height {standing}"
    );

    // Crouching while sitting plays the crouch thread under the held sit,
    // in both views: the eye is still the drawn body's. On v20's rig the
    // sit owns the Eye node, so the view holds while the arms come up.
    held(&mut app, HeldControl::Crouch, true)?;
    run_for(&mut app, 1.0)?;
    ensure!(sitting(&app), "crouching stood the player up");
    let (crouched, node, _) = eyes(&app)?;
    println!("crouched while sitting: eye {crouched}, Eye node {node}");
    ensure!(
        crouched.distance(node) < 1e-3,
        "crouched sitting eye {crouched} is not the body's Eye node {node}"
    );
    held(&mut app, HeldControl::Crouch, false)?;
    run_for(&mut app, 1.0)?;

    // Moving stands the player up, and the eye goes back to standing height.
    held(&mut app, HeldControl::Forward, true)?;
    wait::until(
        &mut [&mut app],
        "standing up",
        Duration::from_secs(10),
        step,
        |a| Ok(!sitting(a[0])),
    )?;
    held(&mut app, HeldControl::Forward, false)?;
    run_for(&mut app, 1.0)?;
    let (stood, _, _) = eyes(&app)?;
    println!("stood up: eye {stood}");
    ensure!(
        (stood.y - standing.y).abs() < 0.05,
        "standing eye {stood} did not return to {standing}"
    );
    request(&mut app, UiAction::Disconnect)?;
    Ok(())
}
