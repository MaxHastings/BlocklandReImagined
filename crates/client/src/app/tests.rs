#[test]
fn body_threads_hold_until_replaced_or_root() {
    let mut threads = std::collections::BTreeMap::new();
    // An image's own thread 0 and the image-bound thread 2 are not body
    // threads.
    assert!(!super::play_free_thread(
        &mut threads,
        7,
        0,
        "fire",
        Some(0),
        1.0
    ));
    assert!(!super::play_free_thread(
        &mut threads,
        7,
        2,
        "plant",
        None,
        1.0
    ));
    assert!(threads.is_empty());
    assert!(super::play_free_thread(
        &mut threads,
        7,
        0,
        "jump",
        None,
        1.0
    ));
    assert!(super::play_free_thread(
        &mut threads,
        7,
        3,
        "talk",
        None,
        1.5
    ));
    assert!(super::play_free_thread(
        &mut threads,
        7,
        0,
        "plant",
        None,
        2.0
    ));
    let sequences = |threads: &std::collections::BTreeMap<u64, super::AvatarThreads>| {
        threads[&7]
            .iter()
            .map(|t| t.as_ref().map(|t| (t.sequence.clone(), t.started_at)))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        sequences(&threads),
        vec![
            Some(("plant".into(), 2.0)),
            None,
            None,
            Some(("talk".into(), 1.5)),
        ]
    );
    assert!(super::play_free_thread(
        &mut threads,
        7,
        0,
        "Root",
        None,
        3.0
    ));
    assert!(super::play_free_thread(
        &mut threads,
        7,
        3,
        "root",
        None,
        3.0
    ));
    assert!(threads.is_empty());
}
/// A host's report reaches the Report window as plain text, a cell for
/// every column in order and team names in their paint.
#[test]
fn a_score_report_shows_in_column_order_as_plain_text() {
    use bri_package_runtime::report::{Report, ReportColumn, ReportRow, ReportSection};
    let report = Report {
        title: "End of Round Report".into(),
        banner: Some("VICTORY".into()),
        columns: ["score", "kills"]
            .map(|k| ReportColumn {
                key: k.into(),
                title: k.to_uppercase(),
            })
            .into(),
        sections: vec![ReportSection {
            title: "Teams:".into(),
            rows: vec![ReportRow {
                key: "team:1".into(),
                name: "<b>Blue".into(),
                color: Some(1),
                cells: [("kills".to_string(), "2".to_string())].into(),
            }],
        }],
    };
    let view = super::report_view(&report, &[[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]]);
    assert_eq!(view.columns, ["SCORE", "KILLS"]);
    let row = &view.sections[0].rows[0];
    assert_eq!(row.name, "‹b›Blue");
    assert_eq!(row.color, Some([0, 0, 255, 255]));
    assert_eq!(row.cells, ["", "2"]);
}
/// A first-person image sits in the view's frame, so it stays put on
/// screen however a seat pitches, rolls or loops: the frame's axes are
/// the rendered camera's.
#[test]
fn a_first_person_image_stays_on_screen_through_a_loop() {
    use super::{Vec3, rolled_view_basis};
    // A held item's eye offset: right, forward and down of the eye.
    let offset = Vec3::new(0.5, -0.4, -1.1);
    let eye = Vec3::new(3.0, 40.0, -7.0);
    for (yaw, pitch, roll) in [
        (0.0, 0.0, 0.0),
        (0.7, 1.2, 0.0),
        (-2.1, 0.3, 2.8),
        (1.4, -1.5, -3.1),
        (0.2, 0.1, std::f32::consts::PI),
    ] {
        let frame = crate::controls::view_frame(eye, yaw, pitch, roll).unwrap();
        let (forward, right, up) = rolled_view_basis(yaw, pitch, roll);
        let placed = frame.transform_point3(offset) - eye;
        let on_screen = Vec3::new(placed.dot(right), placed.dot(up), -placed.dot(forward));
        assert!(
            on_screen.abs_diff_eq(offset, 1e-4),
            "yaw {yaw} pitch {pitch} roll {roll}: {on_screen} vs {offset}"
        );
    }
}
#[test]
fn a_broken_bulb_switches_off_its_lights_and_rules_tint_the_rest() {
    use super::{BTreeSet, Vec3, map_light_tints};
    use bri_render::map_lighting::MapLight;
    use bri_sim::session::MapLightRule;
    let light = |x: f32, z: f32| MapLight {
        position: [x, 10.0, z],
        color: [1.0; 3],
        inner: 0.0,
        outer: 30.0,
        channel: Some(0),
    };
    // The bulb at x = 0 and the positions v20's Bedroom fit gives its
    // lights: 5.9, 11.8 and 19.9 units off. One light across the room.
    // Two tubes at x = 100 and 107 fit as one light between them.
    let lights = [
        light(5.9, 0.0),
        light(0.0, 11.8),
        light(-19.9, 0.0),
        light(60.0, 0.0),
        light(104.0, 14.0),
    ];
    let shapes = [
        (7u32, Vec3::new(0.0, 10.0, 0.0)),
        (8, Vec3::new(100.0, 10.0, 0.0)),
        (9, Vec3::new(107.0, 10.0, 0.0)),
    ];
    let rule = MapLightRule {
        position: [60.0, 10.0, 0.0],
        radius: 2.0,
        tint: [1.0, 0.0, 0.0],
    };
    let whole = map_light_tints(&lights, &shapes, &BTreeSet::new(), &[rule]);
    assert_eq!(whole, [Vec3::ONE, Vec3::ONE, Vec3::ONE, Vec3::X, Vec3::ONE]);
    let broken = map_light_tints(&lights, &shapes, &BTreeSet::from([7, 8]), &[rule]);
    assert_eq!(
        broken,
        [
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::X,
            Vec3::splat(0.5)
        ]
    );
    let both = map_light_tints(&lights, &shapes, &BTreeSet::from([8, 9]), &[]);
    assert_eq!(both[4], Vec3::ZERO);
    // An Add-On cannot light a broken bulb again.
    let lit = MapLightRule {
        position: [0.0, 10.0, 0.0],
        radius: 30.0,
        tint: [2.0; 3],
    };
    assert_eq!(
        map_light_tints(&lights, &shapes, &BTreeSet::from([7]), &[lit])[0],
        Vec3::ZERO
    );
}
/// Max, v0.1.9: holding a jeep with the Gravity Gun, the wheel
/// switched tools instead of reeling. Fire on foot goes to the
/// building path, which never told `controls` the trigger was down, so
/// the tool never got the wheel. The trigger is noted before routing.
#[test]
fn the_trigger_is_noted_whichever_path_takes_the_click() {
    use bri_ui::api::{GameAction, HeldControl, UiAction};
    let mut c = super::Controls::default();
    let fire = |down| {
        UiAction::Game(GameAction::Held {
            control: HeldControl::Fire,
            down,
        })
    };
    assert!(
        super::building_action(&fire(true)),
        "on foot, building takes the click"
    );
    super::note_trigger(&mut c, &fire(true));
    assert!(c.held(HeldControl::Fire));
    super::note_trigger(&mut c, &UiAction::Game(GameAction::DropTool));
    assert!(c.held(HeldControl::Fire), "other actions leave it");
    super::note_trigger(&mut c, &fire(false));
    assert!(!c.held(HeldControl::Fire));
}
/// Max, v0.1.10: "gravity gun scrolling still switches tool instead of
/// letting me reel in or out whatever i am currently grabbed on to".
/// Every frame `follow_control` told `controls` the player was in
/// control of their body, which dropped the held trigger, so the tool
/// never claimed the wheel. Here the real UI takes the mouse, and each
/// frame runs as the game's does: actions drained and the trigger
/// noted, control followed, the held tool's wheel claimed.
#[test]
fn rolling_the_wheel_with_the_trigger_held_reels_and_never_switches_tools() {
    use super::{PathBuf, Ui, UiUpdate};
    use bri_ui::{
        api::{BindInput, GameAction, HeldControl, UiAction},
        binds::Platform,
        geom::Rect,
        input::{InputEvent, MouseButton},
        schema::UiPack,
        screens::ctrl,
        ui::UiConfig,
    };
    let mut pack = UiPack::default();
    for name in ["PlayGui", "LoadingGui"] {
        pack.layouts.insert(
            name.into(),
            ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480)),
        );
    }
    let mut ui = Ui::new(
        std::rc::Rc::new(bri_ui::pack::Pack::from_parts(pack, PathBuf::new())),
        UiConfig {
            size: (1280, 960),
            scale: Some(2.0),
            platform: Platform::Windows,
        },
        bri_ui::api::Settings {
            binds: Some(vec![]),
            mouse_type: 2,
            ..Default::default()
        },
    );
    ui.core.binds.bind(BindInput::Wheel, "scrollInventory");
    ui.core
        .binds
        .bind(BindInput::Mouse(MouseButton::Left), "mouseFire");
    ui.apply(UiUpdate::Connection(bri_ui::api::ConnectionState::InGame {
        server_name: "Test".into(),
        max_players: 8,
        local: true,
        single_player: true,
        admin: true,
    }));
    ui.drain_actions();
    let mut controls = super::Controls::default();
    let mut tool_wheel = None;
    let mut frame = |ui: &mut Ui, controls: &mut super::Controls| -> Vec<UiAction> {
        let actions: Vec<_> = ui.drain_actions().into_iter().map(|(_, a)| a).collect();
        for action in &actions {
            super::note_trigger(controls, action);
            if let UiAction::Game(action) = action {
                controls.action(action);
            }
        }
        controls.follow(bri_sim::session::ControlObject::Player, 1, None);
        super::claim_wheel(ui, &mut tool_wheel, Some("gravity-gun:reel".into()));
        actions
    };
    let reels = |actions: &[UiAction]| {
        actions
            .iter()
            .filter(|a| matches!(a, UiAction::Game(GameAction::ToolWheel { .. })))
            .count()
    };
    let (x, y) = (640.0, 480.0);
    let button = MouseButton::Left;
    // Grab: the trigger held over many frames stays held.
    ui.handle_input(InputEvent::MouseDown { button, x, y });
    for _ in 0..10 {
        frame(&mut ui, &mut controls);
    }
    assert!(
        controls.held(HeldControl::Fire),
        "the trigger is still held"
    );
    // Rolled forward and back: each notch reels, nothing else moves.
    for delta in [1.0, 1.0, -1.0] {
        ui.handle_input(InputEvent::Wheel { delta });
        let actions = frame(&mut ui, &mut controls);
        assert_eq!(
            actions,
            vec![UiAction::Game(GameAction::ToolWheel {
                notches: delta as i32
            })],
            "only the tool sees the wheel"
        );
    }
    // Let go: the wheel is the inventory's again.
    ui.handle_input(InputEvent::MouseUp { button, x, y });
    frame(&mut ui, &mut controls);
    assert!(!controls.held(HeldControl::Fire));
    ui.handle_input(InputEvent::Wheel { delta: 1.0 });
    assert_eq!(reels(&frame(&mut ui, &mut controls)), 0);
}
#[test]
fn only_a_steering_seat_drives_its_vehicle() {
    let steers = |yes: bool| move |_: u64, seat: usize| yes && seat == 0;
    assert_eq!(super::driven_vehicle(Some((7, 0)), steers(true)), Some(7));
    assert_eq!(
        super::driven_vehicle(Some((7, 1)), steers(true)),
        None,
        "a passenger"
    );
    assert_eq!(
        super::driven_vehicle(Some((7, 2)), |_, seat| seat == 2),
        Some(7),
        "an authored control seat need not be first"
    );
    // A tumble's seat: its rider is drawn from the host's poses.
    assert_eq!(
        super::driven_vehicle(Some((7, 0)), steers(false)),
        None,
        "a tumble"
    );
    assert_eq!(super::driven_vehicle(None, steers(true)), None);
}
#[test]
fn the_own_body_hides_only_once_the_camera_reaches_the_eye() {
    use bri_ui::api::GameAction;
    let mut c = super::Controls::default();
    c.action(&GameAction::ToggleFirstPerson { fast: false });
    c.advance_view(1.0);
    assert!(super::draws_third_person(&c, true));
    c.action(&GameAction::ToggleFirstPerson { fast: false });
    c.advance_view(0.1);
    assert!(
        super::draws_third_person(&c, true),
        "halfway in, the body still draws"
    );
    c.advance_view(0.1);
    assert!(!super::draws_third_person(&c, true));
    assert!(
        super::draws_third_person(&c, false),
        "the dead see their body"
    );
    c.action(&GameAction::ToggleFirstPerson { fast: false });
    c.advance_view(1.0 / 60.0);
    assert!(
        super::draws_third_person(&c, true),
        "the body shows as soon as the camera starts out"
    );
}
/// Max, a16: in the Tutorial's horse lesson (no jet on foot) the jet
/// key never reached the horse, so the rider could not get off.
#[test]
fn a_tutorial_rider_still_sends_jet_and_jump_to_the_mount() {
    let no_jet = bri_sim::session::Abilities {
        run: true,
        jump: false,
        jet: false,
    };
    let pressed = bri_sim::player::MoveInput {
        forward: 1.0,
        jump: true,
        jet: true,
        ..Default::default()
    };
    let riding = super::rider_input(no_jet, pressed, true);
    assert!(
        riding.jet && riding.jump,
        "dismount and horse jump reach the mount"
    );
    let walking = super::rider_input(no_jet, pressed, false);
    assert!(
        !walking.jet && !walking.jump,
        "the lesson's limits still hold on foot"
    );
    assert_eq!(walking.forward, 1.0);
}
/// Found by the screen harness: a joined guest's Player List read
/// "127.0.0.1:28000 - 2/64 Players" instead of the host's name and size.
#[test]
fn a_joined_server_goes_by_its_listed_name_and_size() {
    let listing = |name: &str, max_players| bri_net::protocol::Listing {
        name: name.into(),
        map: "Bedroom".into(),
        players: 1,
        max_players,
    };
    assert_eq!(
        super::joined_server(&listing("Max's Build Server", 12), "127.0.0.1:28000", 64),
        ("Max's Build Server".to_string(), 12)
    );
    // A listing without a name or size keeps what the join had.
    assert_eq!(
        super::joined_server(&listing("  ", 0), "10.0.0.5:28000", 64),
        ("10.0.0.5:28000".to_string(), 64)
    );
}
#[test]
fn looking_straight_down_or_past_it_keeps_turning_with_the_yaw() {
    use glam::Vec3;
    use std::f32::consts::FRAC_PI_2;
    // v20's look limits are exactly +-90 degrees; the chase camera adds
    // cameraTilt (0.261) past that.
    for pitch in [-FRAC_PI_2 - 0.261, -FRAC_PI_2, -1.2, 0.0, FRAC_PI_2] {
        for yaw in [0.0f32, 1.0, -2.5] {
            let (forward, right, up) = super::view_basis(yaw, pitch);
            for v in [forward, right, up] {
                assert!(v.is_finite() && (v.length() - 1.0).abs() < 1e-5);
            }
            assert!(forward.dot(right).abs() < 1e-5 && forward.dot(up).abs() < 1e-5);
            assert!(right.y.abs() < 1e-6, "the horizon stays level");
            let camera = bri_render::scene::Camera::oriented(
                [0.0; 3],
                forward.to_array(),
                up.to_array(),
                1.5,
                1.0,
                0.05,
                100.0,
            );
            assert!(camera.view_projection.iter().all(|v| v.is_finite()));
        }
    }
    // Straight down, turning spins the view (no snap to a fixed roll).
    let (_, a, _) = super::view_basis(0.0, -FRAC_PI_2);
    let (_, b, _) = super::view_basis(1.0, -FRAC_PI_2);
    assert!(a.angle_between(b) > 0.99);
    // The chase camera passes over the head smoothly.
    let (before, _, _) = super::view_basis(0.3, -FRAC_PI_2 + 0.01);
    let (after, _, _) = super::view_basis(0.3, -FRAC_PI_2 - 0.01);
    assert!(before.angle_between(after) < 0.021);
    assert!(after.dot(Vec3::new(0.3f32.sin(), 0.0, -0.3f32.cos())) < 0.0);
}
#[test]
fn fov_is_horizontal_like_torque() {
    let aspect = 16.0 / 9.0;
    let fov_y = super::vertical_fov(90f32.to_radians(), aspect);
    // The projected width at fov_y and this aspect spans 90 degrees.
    let across = 2.0 * ((fov_y * 0.5).tan() * aspect).atan();
    assert!((across.to_degrees() - 90.0).abs() < 1e-3, "{across}");
    assert!(fov_y.to_degrees() < 60.0);
}
#[test]
fn saved_pins_are_keyed_by_the_typed_host() {
    assert_eq!(
        super::target_host("play.example.com:28000"),
        "play.example.com"
    );
    assert_eq!(super::target_host("[2001:db8::1]:28000"), "2001:db8::1");
    assert_eq!(super::target_host("203.0.113.10:28001"), "203.0.113.10");
}
#[test]
fn small_state_files_update_in_place() {
    let dir = std::env::temp_dir().join(format!("bri-recent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("state.json");
    for address in ["a.example.com", "b.example.com", "A.example.com"] {
        super::update_small_json(&file, |list: &mut Vec<String>| {
            list.retain(|a| !a.eq_ignore_ascii_case(address));
            list.insert(0, address.to_string());
        })
        .unwrap();
    }
    let list: Vec<String> = super::read_small_json(&file).unwrap();
    assert_eq!(list, ["A.example.com", "b.example.com"]);
    std::fs::remove_dir_all(&dir).unwrap();
}
use crate::testing::content_root::ContentRoot;
/// Run the app until `ready`. Waits follow the game, not the wall clock:
/// one fails when the server has run ten seconds of game time without
/// `ready`, or when neither loading nor the server has moved for two
/// minutes (a stopped game, not a machine busy building something else).
fn until(
    app: &mut super::App,
    what: &str,
    ready: impl Fn(&super::App) -> bool,
) -> anyhow::Result<()> {
    use super::*;
    const TICKS: u64 = 1200;
    const STALL: std::time::Duration = std::time::Duration::from_secs(120);
    let moved = |app: &super::App| (app.loading_revision(), app.network_view().map(|v| v.tick));
    let mut previous = std::time::Instant::now();
    let mut seen = moved(app);
    let mut since = previous;
    let mut first_tick = None;
    loop {
        let now = std::time::Instant::now();
        app.tick(now.duration_since(previous))?;
        app.ui
            .update(now.duration_since(previous).as_millis() as u64);
        previous = now;
        ensure!(app.pump()?.is_empty(), "Unexpected native window command");
        if let ConnectionState::Failed { reason } = &app.ui.core.conn {
            anyhow::bail!("{what} failed: {reason}");
        }
        if ready(app) {
            return Ok(());
        }
        let now_seen = moved(app);
        if now_seen != seen {
            seen = now_seen;
            since = now;
        }
        let tick = seen.1;
        first_tick = first_tick.or(tick);
        ensure!(
            tick.zip(first_tick).is_none_or(|(t, f)| t - f < TICKS),
            "{what} timed out after {TICKS} server ticks: {:?}",
            app.ui.core.conn
        );
        ensure!(
            since.elapsed() < STALL,
            "{what} stopped advancing: {:?}",
            app.ui.core.conn
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
crate::testing::synthetic_and_content!(
    ContentRoot: app_weapon_effect_path_consumes_cues_once_and_syncs_projectile_trails,
    native_weapon_catalog_startup_and_headless_host,
    a_save_covers_the_world_as_the_host_took_it,
);

/// Save Bricks' build is the world when the host answered; a brick placed
/// while the file is still being written is not saved, so leaving still
/// asks about it.
fn a_save_covers_the_world_as_the_host_took_it(f: &ContentRoot) -> anyhow::Result<()> {
    use super::*;
    let state = f.state()?;
    let mut app = App::load(&f.root, state.path(), (320, 240))?;
    app.ui.core.request(UiAction::HostGame {
        map: "v20/add-ons/map_bedroom/bedroom.mis".into(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Save revision test".into(),
        password: String::new(),
        admin_password: "headless-admin-fixture".into(),
        super_admin_password: "headless-super-fixture".into(),
    });
    until(&mut app, "Hosting", |app| {
        app.net
            .attempt
            .as_ref()
            .is_some_and(|a| a.entered && a.view.is_some())
    })?;
    let mut a = app.net.attempt.take().context("no game")?;
    let taken = a.view.as_ref().context("no view")?.world_revision;
    let map = a.view.as_ref().context("no view")?.world.map_id.clone();
    // The host's answer came at `taken`; the write is queued.
    app.files.file_jobs.enqueue(crate::saves::Request {
        id: 9001,
        session: Some(a.id),
        action: UiAction::SaveBricks {
            name: "Covered.world.json".into(),
            description: String::new(),
            events: true,
            ownership: true,
            overwrite: false,
        },
        build: Some(Box::new(bri_world::build::SavedBuild::new(
            bri_world::World::new("Covered".into(), map, vec![[1.0; 4]]),
        ))),
        revision: Some(taken),
    })?;
    // A brick lands before the file is written.
    a.view.as_mut().context("no view")?.world_revision = taken + 1;
    a.settling = None;
    app.net.attempt = Some(a);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while app.files.save_picture.is_none() {
        ensure!(
            std::time::Instant::now() < deadline,
            "the save was never written"
        );
        app.poll_files();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let mut a = app.net.attempt.take().context("no game")?;
    assert_eq!(a.saved_revision, Some(taken));
    app.track_unsaved(&mut a);
    app.net.attempt = Some(a);
    assert!(
        app.ui.core.unsaved_changes,
        "the brick placed while saving counts as saved"
    );
    Ok(())
}

fn app_weapon_effect_path_consumes_cues_once_and_syncs_projectile_trails(
    f: &ContentRoot,
) -> anyhow::Result<()> {
    use super::*;
    let state = f.state()?;
    let mut app = App::load(&f.root, state.path(), (320, 240))?;
    let trail = app
        .content
        .weapons
        .pack
        .projectiles
        .values()
        .find(|projectile| !projectile.trail.is_empty())
        .context("Native weapon pack has no projectile trails")?;
    let mut view = bri_sim::session::WeaponView::default();
    view.projectiles.push(bri_weapons::Projectile {
        paint: None,
        heading: None,
        bounces: 0,
        spawned: 0,
        id: 1,
        definition: trail.id.clone(),
        source: bri_weapons::ActorId(1),
        position: Vec3::new(0., 1., 0.),
        velocity: Vec3::NEG_Z,
        scale: 1.,
        age: 0,
        bounced: false,
        stuck: false,
        origin: Vec3::ZERO,
        was_thrown: false,
    });
    let emitter = app
        .fx
        .weapon_effects
        .world()
        .pack()
        .library
        .emitters
        .iter()
        // Finite, and sure to emit within the first 0.1 s step.
        .find(|emitter| emitter.lifetime > 0. && emitter.period + emitter.period_variance <= 0.1)
        .context("Native effects pack has no finite emitter")?
        .id
        .clone();
    let cue = bri_sim::presentation::Cue {
        id: 1,
        tick: 1,
        position: [0., 1., 0.],
        kind: bri_sim::presentation::CueKind::WeaponEffect {
            source: bri_weapons::TargetId::Map(0),
            definition: emitter,
            node: String::new(),
            seconds: 0.,
            image: None,
            hand: None,
            direction: None,
            scale: 1.,
        },
    };
    app.queue_weapon_cue(cue.clone());
    app.update_weapon_effects(&view, 0.1)?;
    assert_eq!(app.fx.weapon_effects.cue_cursor(), 1);
    assert_eq!(app.weapon_effect_diagnostics().accepted_cues, 1);
    assert!(app.fx.weapon_effects.world().particle_count() > 0);
    let accepted = app.weapon_effect_diagnostics().accepted_cues;
    app.queue_weapon_cue(cue.clone());
    app.update_weapon_effects(&view, 0.)?;
    assert_eq!(app.weapon_effect_diagnostics().accepted_cues, accepted);
    assert_eq!(app.weapon_effect_diagnostics().duplicate_cues, 1);
    assert_eq!(app.fx.weapon_effects.attachment_count(), 1);

    let attached = bri_sim::presentation::Cue {
        id: 2,
        tick: 2,
        position: [0., 1., 0.],
        kind: bri_sim::presentation::CueKind::WeaponEffect {
            source: bri_weapons::TargetId::Actor(bri_weapons::ActorId(1)),
            definition: app
                .fx
                .weapon_effects
                .world()
                .pack()
                .library
                .emitters
                .iter()
                .find(|emitter| emitter.lifetime > 0.)
                .unwrap()
                .id
                .clone(),
            node: "muzzlePoint".into(),
            seconds: 0.2,
            image: Some("v20.image.missing-pose-test".into()),
            hand: Some(0),
            direction: None,
            scale: 1.,
        },
    };
    app.queue_weapon_cue(attached.clone());
    app.update_weapon_effects(&view, 0.1)?;
    assert_eq!(
        app.weapon_effect_backlog().0,
        1,
        "cue waits for a sampled mount"
    );
    assert_eq!(app.fx.weapon_effects.cue_cursor(), 1);
    app.update_weapon_effects(&view, 0.25)?;
    app.update_weapon_effects(&view, 0.25)?;
    assert_eq!(app.weapon_effect_backlog().0, 0);
    assert_eq!(app.fx.weapon_effects.cue_cursor(), 2);
    assert_eq!(app.weapon_effect_diagnostics().missing_poses, 1);
    app.queue_weapon_cue(attached);
    app.update_weapon_effects(&view, 0.)?;
    assert_eq!(app.weapon_effect_diagnostics().missing_poses, 1);
    assert_eq!(app.weapon_effect_backlog().0, 1);
    app.update_weapon_effects(&view, 0.25)?;
    app.update_weapon_effects(&view, 0.25)?;
    assert_eq!(app.weapon_effect_diagnostics().duplicate_cues, 2);

    app.reset_weapon_effect_session(999, 2);
    app.queue_weapon_cue(cue);
    app.update_weapon_effects(&view, 0.)?;
    assert_eq!(app.fx.weapon_effects.cue_cursor(), 2);
    assert_eq!(app.weapon_effect_diagnostics().duplicate_cues, 1);
    assert_eq!(app.fx.weapon_effects.attachment_count(), 1);
    app.disconnect();
    assert_eq!(app.fx.weapon_effects.cue_cursor(), 0);
    assert_eq!(app.fx.weapon_effects.world().source_count(), 0);
    Ok(())
}

/// The base game's items (ids outside any Add-On's namespace) the
/// root's base weapons package carries.
fn base_weapon_items(root: &std::path::Path) -> anyhow::Result<usize> {
    let dir = bri_package::packages::PackageSet::load_root(root)?.role_dir(root, "weapons")?;
    let pack = bri_weapons::Pack::from_json(&std::fs::read(dir.join("weapons.json"))?)?;
    Ok(pack.items.keys().filter(|id| !id.contains(':')).count())
}

#[test]
#[ignore = "requires generated v20 content"]
fn the_stock_weapons_pack_has_v20s_21_items() -> anyhow::Result<()> {
    assert_eq!(base_weapon_items(&ContentRoot::content()?.root)?, 21);
    Ok(())
}

fn native_weapon_catalog_startup_and_headless_host(f: &ContentRoot) -> anyhow::Result<()> {
    use super::*;
    let state_dir = f.state()?;
    let state = state_dir.path().to_path_buf();
    let mut app = App::load(&f.root, &state, (960, 720))?;
    assert_eq!(
        app.content.paths.effects_runtime.canonicalize()?,
        bri_package::packages::PackageSet::load_root(&f.root)?
            .role_dir(&f.root, "effects_runtime")?
            .canonicalize()?
    );
    // The base pack's items, plus any a loaded Add-On adds (the default
    // Add-Ons, once a checkout's content has them installed).
    let items = &app.content.weapons.pack.items;
    let base = items.keys().filter(|id| !id.contains(':')).count();
    assert_eq!(base, base_weapon_items(&f.root)?);
    let all = items.len();
    assert_eq!(app.build.tool_ui.server_catalog().items.len(), all);
    assert_eq!(app.content.datablocks["ItemData"].len(), all);
    assert_eq!(app.content.item_physics.bounds.len(), all);
    app.ui.core.request(UiAction::HostGame {
        map: "v20/add-ons/map_bedroom/bedroom.mis".into(),
        mode: ServerMode::SinglePlayer,
        game_mode: None,
        max_players: 1,
        server_name: "Weapon catalog test".into(),
        password: String::new(),
        admin_password: "headless-admin-fixture".into(),
        super_admin_password: "headless-super-fixture".into(),
    });
    // Waits follow the game, not the wall clock: one fails when the
    // server has run ten seconds of game time without `ready`, or when
    // neither loading nor the server has moved for two minutes (a stopped
    // game, not a machine busy building something else).
    until(&mut app, "Headless host", |app| {
        let Some(view) = app.network_view() else {
            return false;
        };
        let Some(inventory) = view.tools.get(&view.owner) else {
            return false;
        };
        for (slot, expected) in bri_weapons::CORE_TOOLS[..3].iter().enumerate() {
            assert_eq!(inventory.slots[slot].as_deref(), Some(*expected));
        }
        true
    })?;
    until(&mut app, "Inventory action", |a| {
        a.ui.core.admin.snapshot.is_some()
    })?;
    assert_eq!(
        app.ui.core.admin.snapshot.as_ref().unwrap().role,
        bri_ui::models::admin::AdminRole::SuperAdmin
    );
    assert!(
        app.ui
            .core
            .admin
            .snapshot
            .as_ref()
            .unwrap()
            .players
            .iter()
            .any(|player| player.owner && player.persistent_identity)
    );
    let stored_identity =
        bri_identity::ClientIdentity::load_or_create(state.join("client.identity"))?;
    let original_public_key = *stored_identity.public_key();
    app.ui.core.request(UiAction::OpenAdmin);
    until(&mut app, "Inventory action", |a| {
        !a.ui.core.admin.busy() && a.ui.stack().contains(&ScreenId::Admin)
    })?;
    app.ui
        .core
        .admin_request(bri_ui::models::admin::AdminAction::RequestBrickGroups)
        .context("Host could not request original brick management list")?;
    until(&mut app, "Inventory action", |a| {
        !a.ui.core.admin.busy() && a.pending_requests() == 0
    })?;
    assert!(app.ui.core.admin.groups.is_empty());
    app.ui
        .core
        .admin_request(bri_ui::models::admin::AdminAction::RequestBans)
        .context("Host could not request persistent ban list")?;
    until(&mut app, "Inventory action", |a| {
        !a.ui.core.admin.busy() && a.pending_requests() == 0
    })?;
    assert!(app.ui.core.admin.bans.is_empty());
    assert!(
        app.ui.core.admin.status.is_empty(),
        "Ban query was not accepted: {}",
        app.ui.core.admin.status
    );
    app.ui
        .core
        .admin_request(bri_ui::models::admin::AdminAction::SetPassword {
            slot: bri_ui::models::admin::AdminPasswordSlot::Admin,
            password: bri_ui::models::admin::AdminSecret("changed-admin-fixture".into()),
        })
        .context("Host could not change administrator password")?;
    until(&mut app, "Inventory action", |a| {
        !a.ui.core.admin.busy() && a.pending_requests() == 0
    })?;
    assert!(app.ui.core.admin.status.contains("accepted"));
    assert!(
        !app.ui
            .core
            .admin
            .available(bri_ui::models::admin::AdminFeature::Ban)
    );
    assert_eq!(app.ui.core.hud.tools.len(), 5);
    assert_eq!(
        app.ui.core.hud.tools[2].as_ref().unwrap().id,
        bri_weapons::PRINTER
    );
    assert!(matches!(
        app.ui.core.hud.tools[2].as_ref().unwrap().icon,
        IconRef::External(_)
    ));
    app.ui.core.request(UiAction::UseTool { slot: 2 });
    until(&mut app, "Inventory action", |a| {
        a.network_view()
            .is_some_and(|v| v.tools[&v.owner].selected == Some(2))
            && a.pending_requests() == 0
    })?;
    // The HUD names a tool by its uiName as the pack writes it.
    let ui_name = |app: &App, id: &str| app.content.weapons.pack.items[id].ui_name.clone();
    assert_eq!(
        app.ui.core.hud.tool_name,
        ui_name(&app, bri_weapons::PRINTER)
    );
    app.ui.core.request(UiAction::UseTool { slot: 1 });
    until(&mut app, "Inventory action", |a| {
        a.network_view()
            .is_some_and(|v| v.tools[&v.owner].selected == Some(1))
            && a.pending_requests() == 0
    })?;
    // As written: v20 names it "wrench" (wrenchItem uiName), lower case.
    assert_eq!(
        app.ui.core.hud.tool_name,
        ui_name(&app, bri_weapons::WRENCH)
    );
    let owner = app.network_view().unwrap().owner;
    assert!(
        app.world_items.instances().any(|(identity, _)| {
            identity == crate::world_items::ItemIdentity::Mounted(owner, 0)
        })
    );
    assert_eq!(app.world_item_stats().missing_poses, 0);
    assert_eq!(app.world_item_stats().missing_bindings, 0);
    // A state-machine transition on the SAME mounted image must not cancel
    // the authored avatar thread; changing the actual equipment must.
    let mut animation_view = app.network_view().unwrap().clone();
    animation_view.tools.get_mut(&owner).unwrap().selected = None;
    animation_view.weapons.images.insert(
        owner,
        vec![bri_sim::session::MountedImage {
            paint: None,
            image: "v20.image.gunimage".into(),
            state: "Fire".into(),
            hand: 0,
        }],
    );
    let mut animations = BTreeMap::new();
    let mut identities = BTreeMap::new();
    let mut cues = VecDeque::from([(
        bri_sim::presentation::Cue {
            id: 1,
            tick: animation_view.tick,
            position: [0.; 3],
            kind: bri_sim::presentation::CueKind::WeaponAnimation {
                actor: owner,
                thread: 2,
                sequence: "armattack".into(),
                image_hand: None,
            },
        },
        0.,
        10.,
    )]);
    let mut discarded = 0;
    App::update_avatar_animation_inputs(
        &mut animations,
        &mut BTreeMap::new(),
        &mut identities,
        &mut cues,
        &mut discarded,
        &animation_view,
        0.01,
    );
    assert_eq!(animations[&owner].started_at, 10.);
    animation_view.weapons.images.get_mut(&owner).unwrap()[0].state = "Smoke".into();
    App::update_avatar_animation_inputs(
        &mut animations,
        &mut BTreeMap::new(),
        &mut identities,
        &mut cues,
        &mut discarded,
        &animation_view,
        0.01,
    );
    assert_eq!(animations[&owner].started_at, 10.);
    animation_view.weapons.images.get_mut(&owner).unwrap()[0].image = "v20.image.bowimage".into();
    App::update_avatar_animation_inputs(
        &mut animations,
        &mut BTreeMap::new(),
        &mut identities,
        &mut cues,
        &mut discarded,
        &animation_view,
        0.01,
    );
    assert!(animations.is_empty());
    app.ui.core.request(UiAction::Game(GameAction::DropTool));
    until(&mut app, "Inventory action", |a| {
        a.network_view()
            .is_some_and(|v| v.tools[&v.owner].slots[1].is_none())
            && a.pending_requests() == 0
    })?;
    assert!(app.ui.core.hud.tools[1].is_none());
    assert!(
        app.network_view()
            .unwrap()
            .weapons
            .drops
            .iter()
            .any(|d| d.item == bri_weapons::WRENCH)
    );
    let dropped = app.network_view().unwrap().weapons.drops.last().unwrap().id;
    assert!(
        app.world_items
            .instances()
            .any(|(identity, _)| { identity == crate::world_items::ItemIdentity::Drop(dropped) })
    );
    assert!(
        !app.world_items.instances().any(|(identity, _)| {
            identity == crate::world_items::ItemIdentity::Mounted(owner, 0)
        })
    );
    assert_eq!(app.world_item_stats().missing_poses, 0);
    assert_eq!(app.world_item_stats().missing_bindings, 0);
    app.disconnect();
    assert!(app.network_view().is_none());
    assert!(app.ui.core.admin.snapshot.is_none());
    assert_eq!(
        *bri_identity::ClientIdentity::load_or_create(state.join("client.identity"))?.public_key(),
        original_public_key,
    );
    assert_eq!(app.world_item_stats().cached_models, 0);
    assert_eq!(app.world_item_stats().geometry_slots, 0);
    assert!(app.world_items.instances().next().is_none());
    assert!(app.ui.core.hud.tools.iter().all(Option::is_none));
    assert!(!app.ui.core.hud.tool_active);
    assert!(app.build.building.is_none());
    Ok(())
}
#[test]
fn ghost_matches_v20_temp_brick_shells() {
    let mut scene = bri_render::scene::SceneData::default();
    scene
        .materials
        .push(bri_render::scene::Material::vertex_lit("literal", 0));
    for x in [0.0, 1.0, 0.0] {
        scene.vertices.push(bri_render::scene::SceneVertex {
            position: [x, 0.0, x - 1.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0; 2],
            lightmap_uv: [0.0; 2],
            color: [0.4, 0.6, 0.2, 0.5],
            fx: [0.; 4],
        });
    }
    scene.indices = vec![0, 1, 2];
    scene.batches.push(bri_render::scene::MeshBatch {
        indices: 0..3,
        material: 0,
        center: [0.0; 3],
    });
    super::translucent_ghost(&mut scene, &Default::default());
    // Outside: paint x1.5, pushed 0.02 along the normal, forward winding.
    assert_eq!(scene.vertices[0].position, [0.0, 0.02, -1.0]);
    assert!((scene.vertices[0].color[0] - 0.6).abs() < 1e-6);
    assert!((scene.vertices[0].color[1] - 0.9).abs() < 1e-6);
    // Inside: black copy drawn first with reversed winding.
    assert_eq!(scene.vertices[3].color, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(scene.indices, vec![3, 5, 4, 0, 1, 2]);
    assert_eq!(scene.batches[0].indices, 0..6);
    let material = &scene.materials[0];
    assert_eq!(material.alpha, bri_render::scene::AlphaMode::Blend);
    assert!(material.temp_brick_flash);
    assert_eq!(
        material.parameters.map(|p| p[0]),
        Some([0.8, 0.3, 0.3, 0.0])
    );
}
/// A driver steers, and is predicted, by the steering prefs the host
/// uses (its copy, in the pose), never by a copy the host lacks; with
/// no pose yet, by their own, which the host assumes too.
#[test]
fn a_driver_is_predicted_with_the_hosts_steering_prefs() {
    let mut prefs = bri_ui::prefs::Prefs::default();
    assert_eq!(
        super::steering_in_use(None, &prefs),
        bri_sim::session::DEFAULT_STEERING,
        "the shipped prefs are the host's default"
    );
    prefs.set("$pref::Input::UseStrafeSteering", "1");
    assert_eq!(super::steering_in_use(None, &prefs), (true, false));
    let pose = bri_sim::session::VehiclePose {
        passage_frame: Default::default(),
        id: 1,
        tick: 3,
        position: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        velocity: [0.0; 3],
        steering: 0.0,
        wheel_suspension: vec![],
        wheel_rotation: vec![],
        wheel_contact: vec![],
        wheel_tire: vec![],
        turret_aim: [0.0; 2],
        jetting: false,
        angular_velocity: [0.0; 3],
        mouse_steering: [0.0; 2],
        driver_input: 0,
        driver_steering: (false, false),
        steering_quiet: 0,
        actor: None,
    };
    // The host has not heard (or lost) the change: it still steers by
    // the mouse, so the client predicts the mouse too.
    assert_eq!(super::steering_in_use(Some(&pose), &prefs), (false, false));
}
#[test]
fn temp_brick_options_colour_and_flash_the_ghost() {
    let mut prefs = bri_ui::prefs::Prefs::default();
    prefs.set("$pref::HUD::tempBrickOutsideUsePaintColor", "0");
    prefs.set("$pref::HUD::tempBrickOutsideGreen", "1");
    prefs.set("$pref::HUD::tempBrickInsideUsePaintColor", "1");
    prefs.set("$pref::HUD::tempBrickFlashTime", "2000");
    let look = crate::world_scene::TempBrickLook::from_prefs(&prefs);
    assert_eq!(look.outside, Some([0.0, 1.0, 0.0]));
    assert_eq!(look.inside, None);
    assert_eq!(look.flash_ms, 2000.0);
    assert_eq!(
        crate::world_scene::TempBrickLook::from_prefs(&Default::default()),
        Default::default()
    );
}
#[test]
fn world_and_weapon_effects_share_depth_order_and_nearest_light_budget() {
    use glam::Vec3;
    let particle = |z, texture| bri_fx_runtime::ParticleInstance {
        position: Vec3::new(0., 0., z),
        size: 1.,
        color: Vec3::ONE.extend(1.),
        spin: 0.,
        axis: Vec3::ZERO,
        texture,
        blend: bri_fx_runtime::BlendMode::Alpha,
        depth_test: true,
    };
    let light = |id, x| bri_fx_runtime::LightSnapshot {
        handle: bri_fx_runtime::EffectHandle(id),
        position: Vec3::new(x, 0., 0.),
        color: Vec3::ONE,
        radius: 1.,
    };
    let world = bri_fx_runtime::FrameEffects {
        particles: vec![particle(-3., 2)],
        lights: (0..bri_render::scene::MAX_POINT_LIGHTS)
            .map(|i| light(i as u64, 1000. + i as f32))
            .collect(),
    };
    let weapon = bri_fx_runtime::FrameEffects {
        particles: vec![particle(-1., 7)],
        lights: vec![light(9000, 1.)],
    };
    let actor = bri_fx_runtime::FrameEffects {
        particles: vec![],
        lights: vec![],
    };
    let others = [weapon.clone(), actor.clone()];
    let (combined, deferred) = super::combine_effect_frames(world.clone(), others, &[Vec3::ZERO]);
    assert_eq!(combined.particles[0].texture, 2);
    assert_eq!(combined.particles[1].texture, 7);
    assert_eq!(combined.lights.len(), bri_render::scene::MAX_POINT_LIGHTS);
    assert_eq!(combined.lights[0].handle.0, 9000);
    assert_eq!(deferred, 1);
    // A mirror's eye far down the row keeps the lights beside it: the
    // farthest from the player is kept, the next nearest dropped.
    let mirror = Vec3::new(1000. + bri_render::scene::MAX_POINT_LIGHTS as f32, 0., 0.);
    let (combined, _) = super::combine_effect_frames(world, [weapon, actor], &[Vec3::ZERO, mirror]);
    let kept = |id: u64| combined.lights.iter().any(|l| l.handle.0 == id);
    assert!(kept(9000) && kept(bri_render::scene::MAX_POINT_LIGHTS as u64 - 1));
    assert!(!kept(0), "the light nearest neither eye goes");
}
#[test]
fn remote_chat_cannot_inject_color_stack_or_markup() {
    assert_eq!(
        super::plain_chat("<color:ff0000>A\u{e003}B\u{e00b}C\u{e00c}\n"),
        "‹color:ff0000›ABC"
    );
}
#[test]
fn server_prints_keep_ml_markup_for_the_shared_renderer() {
    let binds = bri_ui::binds::BindMap::default();
    let event = "<color:FFFFFF>It's no longer Badspot's' Birthday.<br>Attempts\u{7} ignored";
    assert_eq!(
        super::print_markup(&binds, event),
        "<color:FFFFFF>It's no longer Badspot's' Birthday.<br>Attempts ignored"
    );
    assert_eq!(
        super::print_markup(
            &binds,
            "Press \u{E003}<key:jump>\u{E000} now\n<bitmap:base/client/ui/CI/trophy>"
        ),
        "Press \u{E003}(unbound)\u{E000} now\n<bitmap:base/client/ui/CI/trophy>"
    );
}
#[test]
fn chat_links_like_v20() {
    assert_eq!(
        super::player_chat(
            &Default::default(),
            "Max",
            "see https://blockland.us/x<y now"
        ),
        "\u{e007}\u{e003}Max\u{e007}\u{e006}: see <a:blockland.us/xy>blockland.us/xy</a>\u{e006} now"
    );
    assert_eq!(super::linked_chat("no link <b>", '\u{e006}'), "no link ‹b›");
}
#[test]
fn chat_lines_carry_v20_colors() {
    // `'\c7%1\c3%2\c7%3\c6: %4'`: the name is yellow, the text white.
    assert_eq!(
        super::player_chat(&Default::default(), "Max", "hi \u{e003}<b>"),
        "\u{e007}\u{e003}Max\u{e007}\u{e006}: hi ‹b›"
    );
    // Clan tags sit grey around the name, stripped of colour escapes.
    let clan = bri_sim::session::Clan {
        prefix: "[B\u{e003}]".into(),
        suffix: "~".into(),
    };
    assert_eq!(
        super::player_chat(&clan, "Max", "hi"),
        "\u{e007}[B]\u{e003}Max\u{e007}~\u{e006}: hi"
    );
    // Server lines keep markup and colour escapes around a death icon.
    assert_eq!(
        bri_ui::ml::sanitize("\u{e003}Max<bitmap:base/client/ui/CI/skull>\u{e000}!"),
        "\u{e003}Max<bitmap:base/client/ui/CI/skull>\u{e000}!"
    );
}
#[test]
fn a_player_camera_pivots_over_the_middle_of_the_box() {
    use glam::Vec3;
    let feet = Vec3::new(3.0, 1.0, -2.0);
    // PlayerStandardArmor: feet + 2.65 / 2 + 0.75, 8 back, tilted 0.261.
    let (distance, pivot, tilt) = super::pivot_camera(2.65, 1.0, super::PLAYER_CAMERA, feet, 1.0);
    assert_eq!(distance, 8.0);
    assert!(pivot.distance(feet + Vec3::Y * 2.075) < 1e-5, "{pivot}");
    assert_eq!(tilt, 0.261);
    // Sliding in, the offset eases to 0.75 and the distance to nothing.
    let (distance, pivot, _) = super::pivot_camera(2.4, 1.0, (8.0, 2.3, 0.261), feet, 0.0);
    assert_eq!(distance, 0.0);
    assert!(pivot.distance(feet + Vec3::Y * 1.95) < 1e-5, "{pivot}");
}
/// A vehicle pack on disk, and the roles its vehicles play.
struct Mounts {
    assets: crate::vehicles::VehicleAssets,
    /// Each vehicle, and whether the client predicts its first seat.
    predicted: Vec<(String, bool)>,
    car: String,
    tank: String,
    horse: String,
    /// The player-type mount the tank carries as its turret.
    tank_turret: String,
}
impl Mounts {
    fn synthetic() -> anyhow::Result<Self> {
        use bri_vehicles::testing as vt;
        let scratch = crate::testing::ScratchDir::new("app-mounts")?;
        crate::testing::vehicles::write_pack(scratch.path())?;
        let assets = crate::vehicles::VehicleAssets::load(scratch.path())?;
        Ok(Self {
            assets,
            // The ball has no seat; the tumble body's seat has no controls.
            predicted: vt::ALL
                .map(|id| (id.to_string(), ![vt::BALL, vt::TUMBLE].contains(&id)))
                .into(),
            car: vt::CAR.into(),
            tank: vt::TANK.into(),
            horse: vt::HORSE.into(),
            tank_turret: crate::testing::vehicles::TANK_TURRET.into(),
        })
    }
    fn content() -> anyhow::Result<Self> {
        let root = bri_package::testing::pack_dir(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
            "vehicles",
        );
        Ok(Self {
            assets: crate::vehicles::VehicleAssets::load(&root)?,
            predicted: [
                ("v20.vehicle.jeepvehicle", true),
                ("v20.vehicle.tankvehicle", true),
                ("v20.vehicle.flyingwheeledjeepvehicle", true),
                ("v20.vehicle.magiccarpetvehicle", true),
                ("v20.vehicle.skivehicle", true),
                ("v20.vehicle.horsearmor", true),
                ("v20.vehicle.rowboatarmor", true),
                ("v20.vehicle.cannonturret", true),
                ("v20.vehicle.tankturretplayer", true),
                ("v20.vehicle.deathvehicle", false),
            ]
            .map(|(id, p)| (id.to_string(), p))
            .into(),
            car: "v20.vehicle.jeepvehicle".into(),
            tank: "v20.vehicle.tankvehicle".into(),
            horse: "v20.vehicle.horsearmor".into(),
            tank_turret: "v20.vehicle.tankturretplayer".into(),
        })
    }
}
crate::testing::synthetic_and_content!(
    Mounts: the_client_predicts_the_live_vehicles_and_mounts_it_controls,
    a_horse_rider_sees_the_horse_player_camera
);
/// Which first seats the client predicts, and when it starts again: a
/// live vehicle a player steers or a player-type mount they control; a
/// respawn (new id), a new definition or scale restarts it; the tumble
/// body (no controls) and a destroyed vehicle show the host's poses.
fn the_client_predicts_the_live_vehicles_and_mounts_it_controls(fx: &Mounts) -> anyhow::Result<()> {
    let assets = &fx.assets;
    let info = |definition: &str| bri_sim::session::VehicleInfo {
        id: 7,
        definition: definition.into(),
        color: None,
        occupants: vec![Some(1)],
        destroyed: false,
        scale: 1.0,
    };
    let target = |info: &bri_sim::session::VehicleInfo, strafe: bool| {
        super::drive_target(info, assets.definition(&info.definition).unwrap(), strafe)
    };
    for (definition, predicted) in &fx.predicted {
        for strafe in [false, true] {
            assert_eq!(
                target(&info(definition), strafe).is_some(),
                *predicted,
                "{definition}, strafe steering {strafe}"
            );
        }
    }
    let jeep = info(&fx.car);
    let base = target(&jeep, false).unwrap();
    let destroyed = bri_sim::session::VehicleInfo {
        destroyed: true,
        ..jeep.clone()
    };
    assert_eq!(target(&destroyed, false), None, "a wreck is the host's");
    for changed in [
        bri_sim::session::VehicleInfo {
            id: 8,
            ..jeep.clone()
        },
        bri_sim::session::VehicleInfo {
            scale: 2.0,
            ..jeep.clone()
        },
        bri_sim::session::VehicleInfo {
            definition: fx.tank.clone(),
            ..jeep.clone()
        },
    ] {
        assert_ne!(target(&changed, false), Some(base.clone()), "{changed:?}");
    }
    Ok(())
}
/// Max, a21: riding a horse, the chase camera sat 2.3 over the horse's
/// feet. v20's rider looks through the horse's own player camera: the
/// middle of its 2.4 tall box plus `cameraVerticalOffset` 2.3, 8 back.
fn a_horse_rider_sees_the_horse_player_camera(fx: &Mounts) -> anyhow::Result<()> {
    use glam::Vec3;
    let assets = &fx.assets;
    // v20's camera pivot: the middle of the mount's box plus its
    // `cameraVerticalOffset`, `cameraMaxDist` back, tilted `cameraTilt`.
    let pivot_height = |d: &bri_vehicles::Definition| {
        let (low, high) = d
            .collision_hulls
            .iter()
            .flatten()
            .fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(p[1]), h.max(p[1])));
        (high - low) * 0.5 + d.camera.offset
    };
    let horse = assets.definition(&fx.horse).unwrap();
    assert_eq!(
        horse.seat_role(0),
        bri_vehicles::schema::SeatRole::Actor,
        "the horse's rider takes the actor path"
    );
    let feet = Vec3::new(10.0, 4.0, -6.0);
    let (distance, pivot, tilt) = super::mount_camera(horse, feet, 1.0);
    assert_eq!(distance, horse.camera.max_dist);
    assert!(
        pivot.distance(feet + Vec3::Y * pivot_height(horse)) < 1e-4,
        "{pivot}"
    );
    assert!((tilt - horse.camera.tilt).abs() < 1e-6);
    // The other player-type mounts use their own boxes and offsets.
    let turret = assets.definition(&fx.tank_turret).unwrap();
    let (_, pivot, _) = super::mount_camera(turret, feet, 1.0);
    assert!(
        pivot.distance(feet + Vec3::Y * pivot_height(turret)) < 1e-4,
        "{pivot}"
    );
    // The Tank's gunner looks through that turret, not the Tank.
    let tank = assets.definition(&fx.tank).unwrap();
    assert_eq!(tank.seat_role(2), bri_vehicles::schema::SeatRole::Gunner);
    let carried = assets.attachment_definition(tank).unwrap();
    assert_eq!(carried.id, fx.tank_turret);
    Ok(())
}
/// v20's own numbers: the horse's 2.4 tall box and 2.3 offset, 8 back,
/// tilted 0.261; the tank turret's 0.85 half height; its 8 distance.
#[test]
#[ignore = "requires generated v20 content"]
fn original_mount_cameras() -> anyhow::Result<()> {
    use glam::Vec3;
    let fx = Mounts::content()?;
    let feet = Vec3::new(10.0, 4.0, -6.0);
    let horse = fx.assets.definition(&fx.horse).unwrap();
    let (distance, pivot, tilt) = super::mount_camera(horse, feet, 1.0);
    assert_eq!(distance, 8.0);
    assert!(pivot.distance(feet + Vec3::Y * 3.5) < 1e-4, "{pivot}");
    assert!((tilt - 0.261).abs() < 1e-6);
    let turret = fx.assets.definition(&fx.tank_turret).unwrap();
    let (_, pivot, _) = super::mount_camera(turret, feet, 1.0);
    assert!(
        pivot.distance(feet + Vec3::Y * (0.85 + 2.3)) < 1e-4,
        "{pivot}"
    );
    let tank = fx.assets.definition(&fx.tank).unwrap();
    let carried = fx.assets.attachment_definition(tank).unwrap();
    assert_eq!(carried.camera.max_dist, 8.0);
    Ok(())
}

crate::testing::synthetic_and_content!(
    ContentRoot: leaving_a_game_forgets_its_seat_eyes_and_liquids
);
fn leaving_a_game_forgets_its_seat_eyes_and_liquids(f: &ContentRoot) -> anyhow::Result<()> {
    use super::*;
    let scratch = f.state()?;
    let mut app = App::load(&f.root, scratch.path(), (320, 240))?;
    // What a game in progress leaves behind: a seat, a rider's eye, a
    // tumble, eyes the camera drew from and the map's liquids.
    app.mounts.seated_on = Some((7, 1));
    app.mounts.mount_heading = Some(1.0);
    app.mounts.takes_turret = true;
    app.mounts.rider_eye = Some(Vec3::ONE);
    app.mounts.tumble = Some(9);
    app.mounts
        .rider_rotations
        .insert(3, glam::Quat::from_rotation_y(1.0));
    app.view.observer_eye = Some(Vec3::ONE);
    app.view.rendered_camera = Some((Vec3::ONE, 1.0, 0.5));
    app.view.rendered_roll = 0.3;
    app.view.drawn_controls = Some(app.controls.clone());
    app.scene.liquid_cache = Some(LiquidCache {
        generation: 1,
        palette: Vec::new(),
        liquids: Arc::from(Vec::new()),
        waters: Arc::from(Vec::new()),
    });
    app.disconnect();
    // The next game starts on foot, with nothing of the last one's view.
    assert_eq!(app.mounts.seated_on, None);
    assert_eq!(app.mounts.mount_heading, None);
    assert!(!app.mounts.takes_turret);
    assert_eq!(app.mounts.rider_eye, None);
    assert_eq!(app.mounts.tumble, None);
    assert!(app.mounts.rider_rotations.is_empty());
    assert_eq!(app.mounts.seat_report, None);
    assert_eq!(app.view.observer_eye, None);
    assert_eq!(app.view.rendered_camera, None);
    assert_eq!(app.view.rendered_roll, 0.0);
    assert!(app.view.drawn_controls.is_none());
    assert!(app.scene.liquid_cache.is_none());
    Ok(())
}
