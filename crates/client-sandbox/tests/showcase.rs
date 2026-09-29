//! The showcase Add-Ons' client code (`packages/showcase/*-fx`): each
//! module is built from its WebAssembly text, loads under the sandbox's
//! rules, and draws what the world it is shown asks for. With a GPU
//! (`--ignored`), each renders offscreen to PNGs for a look.
use bri_client_sandbox::{
    AddOn, AddOnCode, Budgets, Capability, FrameInput, Sandbox, TrustLevel, World,
    world::{Entity, Player, Vehicle},
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const BALL: &str = "steel-ball-kit:vehicle/steelball";

fn showcase(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/showcase")
        .join(name)
}

fn built_from_source(name: &str) {
    let dir = showcase(name);
    let built = wat::parse_file(dir.join("client/main.wat")).unwrap();
    let path = dir.join("client/main.wasm");
    if std::env::var_os("BRI_BLESS").is_some() {
        std::fs::write(&path, &built).unwrap();
    }
    assert_eq!(
        std::fs::read(&path).unwrap_or_default(),
        built,
        "{name}/client/main.wasm is stale; rerun with BRI_BLESS=1"
    );
}

fn start(name: &str) -> (AddOnCode, AddOn) {
    let code = match AddOnCode::load(&showcase(name)) {
        Ok(Some(code)) => code,
        Ok(None) => panic!("{name} has no client code"),
        Err(problems) => panic!("{problems:#?}"),
    };
    let addon = Sandbox::new()
        .unwrap()
        .start(&code, Budgets::default(), TrustLevel::Sandboxed)
        .unwrap_or_else(|e| panic!("{name} stopped: {e}"));
    (code, addon)
}

fn frame(t: f32, world: &Arc<World>) -> FrameInput {
    FrameInput {
        time: t,
        dt: 1.0 / 60.0,
        eye: [0.0, 3.0, 8.0],
        forward: [0.0, -0.3, -1.0],
        world: world.clone(),
        ..Default::default()
    }
}

fn ball(id: u64, x: f32, turn: f32) -> Vehicle {
    let (s, c) = (turn * 0.5).sin_cos();
    Vehicle {
        id,
        definition: BALL.into(),
        position: [x, 1.25, 0.0],
        rotation: [0.0, 0.0, s, c],
        velocity: [4.0, 0.0, 0.0],
        radius: 1.25 * 3f32.sqrt(),
    }
}

// ---- Steel Ball Shine ----

#[test]
fn the_steel_ball_module_is_built_from_its_source() {
    built_from_source("steel-ball-fx");
}

#[test]
fn the_steel_ball_shine_draws_each_steel_ball_and_nothing_else() {
    let (code, mut addon) = start("steel-ball-fx");
    assert_eq!(code.name, "Steel Ball Shine");
    assert!(code.capabilities.contains(&Capability::WorldRead));
    let mut other = ball(9, 0.0, 0.0);
    other.definition = "v20.vehicle.jeepvehicle".into();
    let world = Arc::new(World {
        vehicles: vec![ball(3, -3.0, 0.0), other, ball(4, 3.0, 1.0)],
        ..Default::default()
    });
    let drawn = addon.frame(frame(0.0, &world)).unwrap().clone();
    assert_eq!(drawn.draws.len(), 2, "one sphere per Steel Ball");
    let params = drawn.draws[0].params.expect("its own parameters");
    assert_eq!(&params[0][..3], &[-3.0, 1.25, 0.0], "centred on the ball");
    assert!(
        (params[0][3] - 1.25).abs() < 0.01 && params[0][3] > 1.25,
        "a hair over the ball's radius: {}",
        params[0][3]
    );
    assert_eq!(params[1], [0.0, 0.0, 0.0, 1.0], "unturned");
    let turned = drawn.draws[1].params.unwrap();
    assert_eq!(
        turned[1],
        ball(4, 3.0, 1.0).rotation,
        "turned as the ball rolls"
    );
    assert!(drawn.sounds.is_empty(), "nothing hit anything yet");
    // Next frame the first ball has stopped dead against something: it
    // clanks where it is.
    let mut hit = ball(3, -3.0, 0.0);
    hit.velocity = [-12.0, 0.0, 0.0];
    let struck = Arc::new(World {
        vehicles: vec![hit, ball(4, 3.0, 1.0)],
        ..Default::default()
    });
    let heard = addon.frame(frame(0.05, &struck)).unwrap().sounds.clone();
    assert_eq!(heard.len(), 1, "{heard:?}");
    assert_eq!(heard[0].name, "client/sounds/clank.wav");
    assert_eq!(heard[0].at, Some([-3.0, 1.25, 0.0]));
    assert!(heard[0].volume > 0.5);
    // No balls, nothing drawn.
    let empty = Arc::new(World::default());
    assert!(addon.frame(frame(0.1, &empty)).unwrap().draws.is_empty());
}

/// Needs a GPU: renders two rolling steel balls to PNGs.
#[test]
#[ignore = "needs a GPU adapter"]
fn the_steel_ball_renders_offscreen() {
    let (_, mut addon) = start("steel-ball-fx");
    let world = |t: f32| {
        Arc::new(World {
            vehicles: vec![ball(3, -1.6, t * 2.0), ball(4, 1.6, -t)],
            ..Default::default()
        })
    };
    let (adapter, images) = bri_client_sandbox::gpu::render_offscreen_scene(
        &mut addon,
        640,
        360,
        &[0.0, 0.8],
        glam::Vec3::new(0.0, 2.6, 6.5),
        glam::Vec3::new(0.0, 1.2, 0.0),
        world,
    )
    .unwrap();
    save("steel-ball", &images);
    let background = images[0].pixels[0..4].to_vec();
    let at = |x: u32, y: u32| {
        let i = ((y * images[0].width + x) * 4) as usize;
        images[0].pixels[i..i + 4].to_vec()
    };
    assert_ne!(
        at(200, 180),
        background,
        "the left ball is drawn on {adapter}"
    );
    assert_ne!(images[0].pixels, images[1].pixels, "it rolls");
}

fn save(name: &str, images: &[bri_client_sandbox::gpu::Image]) {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("showcase-preview");
    std::fs::create_dir_all(&out).unwrap();
    for (i, image) in images.iter().enumerate() {
        let path = out.join(format!("{name}-{i:02}.png"));
        bri_client_sandbox::gpu::write_png(image, &path).unwrap();
        println!("{}", path.display());
    }
}

fn player(id: u64, feet: [f32; 3], look: [f32; 3]) -> Player {
    Player {
        id,
        alive: true,
        feet,
        eye: [feet[0], feet[1] + 2.1, feet[2]],
        look,
        velocity: [0.0; 3],
        ..Default::default()
    }
}

fn beam(world: &mut World, player: u64, beam: [i64; 6]) {
    world
        .state
        .entry("gravity-gun".into())
        .or_default()
        .players
        .insert(
            player,
            [("beam".to_string(), serde_json::json!(beam))].into(),
        );
}

// ---- Gravity Gun Effects ----

const CRATE: &str = "test:vehicle/crate";

fn crate_at(id: u64, at: [f32; 3]) -> Vehicle {
    Vehicle {
        id,
        definition: CRATE.into(),
        position: at,
        rotation: [0.0, 0.0, 0.0, 1.0],
        velocity: [0.0; 3],
        radius: 3f32.sqrt(),
    }
}

/// Player 1 at the origin looking along -z, a crate 5 units ahead.
fn gun_world(beam_state: [i64; 6], crate_at_: [f32; 3]) -> World {
    let mut world = World {
        local: 1,
        players: vec![player(1, [0.0, 0.0, 0.0], [0.0, 0.0, -1.0])],
        vehicles: vec![crate_at(7, crate_at_)],
        ..Default::default()
    };
    beam(&mut world, 1, beam_state);
    world
}

#[test]
fn the_gravity_gun_module_is_built_from_its_source() {
    built_from_source("gravity-gun-fx");
}

#[test]
fn the_gravity_gun_effects_follow_the_guns_state() {
    let (code, mut addon) = start("gravity-gun-fx");
    assert_eq!(code.name, "Gravity Gun Effects");
    let draws = |addon: &mut AddOn, t: f32, world: World| {
        addon
            .frame(frame(t, &Arc::new(world)))
            .unwrap()
            .draws
            .clone()
    };
    // Nobody holds anything: nothing drawn.
    assert!(
        draws(
            &mut addon,
            0.0,
            gun_world([0, 0, 0, 0, 0, 0], [0.0, 2.0, -5.0])
        )
        .is_empty()
    );
    // Holding the crate: two beam passes, the bubble, the orbiting sparks,
    // and the grab heard at the crate.
    let grabbed = addon
        .frame(frame(
            0.1,
            &Arc::new(gun_world([1, 7, 0, 0, 0, 0], [0.0, 2.0, -5.0])),
        ))
        .unwrap()
        .clone();
    let held = grabbed.draws.clone();
    assert_eq!(held.len(), 4, "{held:#?}");
    assert_eq!(grabbed.sounds.len(), 1);
    assert_eq!(grabbed.sounds[0].name, "client/sounds/grab.wav");
    assert_eq!(grabbed.sounds[0].at, Some([0.0, 2.0, -5.0]));
    let beam_params = held[0].params.unwrap();
    assert_eq!(
        &beam_params[1][..3],
        &[0.0, 2.0, -5.0],
        "the beam ends at the crate"
    );
    assert!(
        beam_params[0][2] < -0.5 && beam_params[0][0] > 0.2,
        "it starts at the gun, ahead and to the right: {:?}",
        beam_params[0]
    );
    // Charging adds the orb and the sparks drawn into it.
    let charging = draws(
        &mut addon,
        0.2,
        gun_world([1, 7, 1, 0, 0, 0], [0.0, 2.0, -5.0]),
    );
    assert_eq!(charging.len(), 6);
    // The throw: the beam goes, a shockwave and sparks mark it, and it
    // booms (not the drop's fall-away)...
    let launched = addon
        .frame(frame(
            0.6,
            &Arc::new(gun_world([0, 0, 0, 1, 1, 7], [0.0, 2.0, -5.0])),
        ))
        .unwrap()
        .clone();
    let names: Vec<&str> = launched.sounds.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["client/sounds/launch.wav"]);
    let thrown = launched.draws.clone();
    assert_eq!(thrown.len(), 3, "{thrown:#?}");
    let ring = thrown[0].params.unwrap();
    assert_eq!(&ring[0][..3], &[0.0, 2.0, -5.0], "where the crate was");
    // ...and fade out.
    assert!(
        draws(
            &mut addon,
            1.5,
            gun_world([0, 0, 0, 1, 1, 7], [0.0, 2.0, -25.0])
        )
        .is_empty()
    );
    // A player seen for the first time with shots already fired shows no
    // stale burst.
    let (_, mut fresh) = start("gravity-gun-fx");
    assert!(
        draws(
            &mut fresh,
            0.0,
            gun_world([0, 0, 0, 5, 1, 7], [0.0, 2.0, -5.0])
        )
        .is_empty()
    );
}

/// Needs a GPU: renders holding, charging and a throw to PNGs.
#[test]
#[ignore = "needs a GPU adapter"]
fn the_gravity_gun_renders_offscreen() {
    let (_, mut addon) = start("gravity-gun-fx");
    let world = |t: f32| {
        let (state, at) = if t < 1.0 {
            ([1, 7, 0, 0, 0, 0], [0.4, 2.2, -4.5])
        } else if t < 2.0 {
            ([1, 7, 1, 0, 0, 0], [0.4, 2.2, -4.5])
        } else {
            (
                [0, 0, 0, 1, 1, 7],
                [0.4, 2.6 + (t - 2.0) * 2.0, -4.5 - (t - 2.0) * 30.0],
            )
        };
        Arc::new(gun_world(state, at))
    };
    let (adapter, images) = bri_client_sandbox::gpu::render_offscreen_scene(
        &mut addon,
        640,
        384,
        &[0.0, 0.4, 1.2, 1.8, 2.0, 2.08, 2.2],
        glam::Vec3::new(3.5, 3.4, 2.5),
        glam::Vec3::new(0.2, 1.8, -3.0),
        world,
    )
    .unwrap();
    save("gravity-gun", &images);
    let background = images[0].pixels[0..4].to_vec();
    let lit = |i: usize| {
        images[i]
            .pixels
            .chunks_exact(4)
            .filter(|p| *p != background.as_slice())
            .count()
    };
    println!(
        "lit pixels per frame on {adapter}: {:?}",
        (0..images.len()).map(lit).collect::<Vec<_>>()
    );
    assert!(lit(1) > 2000, "the beam and bubble show");
    assert!(lit(3) > lit(1), "charging adds the orb");
    assert!(lit(5) > 500, "the throw's shockwave shows");
}

#[test]
fn a_held_creature_gets_the_beam_and_bubble_too() {
    let (_, mut addon) = start("gravity-gun-fx");
    let mut world = gun_world([3, 5, 0, 0, 0, 0], [0.0, 2.0, -5.0]);
    world.entities = vec![Entity {
        id: 5,
        kind: "zoo:entity/cow".into(),
        feet: [1.0, 0.0, -6.0],
        yaw: 0.0,
    }];
    let drawn = addon.frame(frame(0.0, &Arc::new(world))).unwrap().clone();
    assert_eq!(drawn.draws.len(), 4, "beam, core, bubble and sparks");
    let beam_params = drawn.draws[0].params.unwrap();
    assert_eq!(
        &beam_params[1][..3],
        &[1.0, 1.3, -6.0],
        "to the creature's middle"
    );
}
