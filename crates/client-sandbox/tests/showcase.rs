//! The showcase Add-Ons' client code (`packages/showcase/*-fx`): each
//! module is built from its WebAssembly text, loads under the sandbox's
//! rules, and draws what the world it is shown asks for. With a GPU
//! (`--ignored`), each renders offscreen to PNGs for a look.
use bri_client_sandbox::{
    AddOn, AddOnCode, Budgets, Capability, FrameInput, Sandbox, TrustLevel, World,
    world::{Entity, Held, Player, Vehicle},
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
        // What the effects draw, not how fast: a loaded machine or a
        // software renderer gives the same result.
        .start(&code, Budgets::untimed(), TrustLevel::Sandboxed)
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
fn the_steel_ball_sounds_clank_where_a_steel_ball_hits_and_draw_nothing() {
    let (code, mut addon) = start("steel-ball-fx");
    assert_eq!(code.name, "Steel Ball Sounds");
    assert!(code.capabilities.contains(&Capability::WorldRead));
    let mut other = ball(9, 0.0, 0.0);
    other.definition = "v20.vehicle.jeepvehicle".into();
    let world = Arc::new(World {
        vehicles: vec![ball(3, -3.0, 0.0), other.clone(), ball(4, 3.0, 1.0)],
        ..Default::default()
    });
    let first = addon.frame(frame(0.0, &world)).unwrap().clone();
    assert!(first.draws.is_empty(), "the game draws the ball itself");
    assert!(first.sounds.is_empty(), "nothing hit anything yet");
    // Next frame the first ball has stopped dead against something: it
    // clanks where it is. The jeep stopping as sharply makes no sound.
    let mut hit = ball(3, -3.0, 0.0);
    hit.velocity = [-12.0, 0.0, 0.0];
    other.velocity = [-12.0, 0.0, 0.0];
    let struck = Arc::new(World {
        vehicles: vec![hit, other, ball(4, 3.0, 1.0)],
        ..Default::default()
    });
    let heard = addon.frame(frame(0.05, &struck)).unwrap().sounds.clone();
    assert_eq!(heard.len(), 1, "{heard:?}");
    assert_eq!(heard[0].name, "client/sounds/clank.wav");
    assert_eq!(heard[0].at, Some([-3.0, 1.25, 0.0]));
    assert!(heard[0].volume > 0.5);
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

fn beam(world: &mut World, player: u64, beam: [f64; 4]) {
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
const GUN: &str = "gravity-gun-tool:image/gravitygun";

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

/// Player 1 at the origin looking along -z (eye 2.1 up), a crate ahead.
fn gun_world(beam_state: [f64; 4], crate_at_: [f32; 3]) -> World {
    let mut world = World {
        local: 1,
        players: vec![player(1, [0.0, 0.0, 0.0], [0.0, 0.0, -1.0])],
        vehicles: vec![crate_at(7, crate_at_)],
        ..Default::default()
    };
    beam(&mut world, 1, beam_state);
    world
}

/// The gun drawn in player 1's hand, its muzzle at `muzzle`, with a
/// stand-in model for it.
fn holding_the_gun(world: &mut World, muzzle: [f32; 3]) -> [f32; 16] {
    let transform = glam::Mat4::from_translation(glam::Vec3::new(0.35, 1.7, -0.4)).to_cols_array();
    let me = &mut world.players[0];
    me.image = GUN.into();
    me.held = vec![Held {
        hand: 0,
        transform,
        muzzle: Some(muzzle),
    }];
    world.image_meshes.insert(GUN.into(), Arc::new(stand_in_gun()));
    transform
}

/// A box about the Printer's size, pointing along +y as Torque models
/// do, standing in for its model.
fn stand_in_gun() -> bri_client_sandbox::host::Mesh {
    let half = glam::Vec3::new(0.12, 0.45, 0.16);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let mut n = glam::Vec3::ZERO;
            n[axis] = sign;
            let (u, v) = (n.cross(glam::Vec3::Y), glam::Vec3::Y);
            let (u, v) = if u.length() < 0.5 {
                (glam::Vec3::X, n.cross(glam::Vec3::X))
            } else {
                (u, v)
            };
            // Counter-clockwise seen from outside.
            let (u, v) = if u.cross(v).dot(n) < 0.0 { (v, u) } else { (u, v) };
            let base = vertices.len() as u32;
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let p = (n + u * a + v * b) * half;
                vertices.push(bri_client_sandbox::host::Vertex {
                    position: p.to_array(),
                    normal: n.to_array(),
                    uv: [a * 0.5 + 0.5, b * 0.5 + 0.5],
                });
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    bri_client_sandbox::host::Mesh { vertices, indices }
}

fn close(a: &[f32], b: &[f32]) -> bool {
    a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4)
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
    let idle = [0.0, 0.0, 0.0, 0.0];
    assert!(draws(&mut addon, 0.0, gun_world(idle, [0.0, 2.0, -5.0])).is_empty());
    // The trigger held with nothing caught: a thinner beam (glow and
    // core) out to where it points, and a glow at the muzzle.
    let reaching = draws(
        &mut addon,
        0.05,
        gun_world([0.0, 0.0, 1.0, 8.0], [0.0, 2.0, -25.0]),
    );
    assert_eq!(reaching.len(), 3, "{reaching:#?}");
    assert!(close(&reaching[0].params.unwrap()[1][..3], &[0.0, 2.1, -8.0]));
    // And it whirrs: once as the trigger goes down, then every half
    // second while it keeps reaching.
    let (_, mut quiet) = start("gravity-gun-fx");
    let whirrs = |addon: &mut AddOn, t: f32| {
        let reach = Arc::new(gun_world([0.0, 0.0, 1.0, 8.0], [0.0, 2.0, -25.0]));
        let f = addon.frame(frame(t, &reach)).unwrap();
        f.sounds.iter().filter(|s| s.name == "client/sounds/reach.wav").count()
    };
    let heard: Vec<usize> = [0.0, 0.2, 0.4, 0.5, 0.7, 1.0].iter().map(|&t| whirrs(&mut quiet, t)).collect();
    assert_eq!(heard, [1, 0, 0, 1, 0, 1]);
    // Holding the crate, grabbed a unit left of its middle: two beam
    // passes, the grip glow and the catch's flash, the muzzle glow, the
    // bubble, the orbiting sparks, and the grab heard at the crate.
    let grab = [1.0, 7.0, 1.0, 5.0];
    let grabbed = addon
        .frame(frame(0.1, &Arc::new(gun_world(grab, [1.0, 2.1, -5.0]))))
        .unwrap()
        .clone();
    let held = grabbed.draws.clone();
    assert_eq!(held.len(), 7, "{held:#?}");
    assert_eq!(grabbed.sounds.len(), 1);
    assert_eq!(grabbed.sounds[0].name, "client/sounds/grab.wav");
    assert_eq!(grabbed.sounds[0].at, Some([1.0, 2.1, -5.0]));
    let beam_params = held[0].params.unwrap();
    assert!(
        close(&beam_params[1][..3], &[0.0, 2.1, -5.0]),
        "the beam ends where it grabbed: {:?}",
        beam_params[1]
    );
    assert!(
        beam_params[0][2] < -0.5 && beam_params[0][0] > 0.2,
        "without a drawn gun it starts ahead and to the right: {:?}",
        beam_params[0]
    );
    // The crate turns a quarter round and drifts: the grip turns with it,
    // and the beam still leaves along the aim, bending into it.
    let (s, c) = std::f32::consts::FRAC_PI_4.sin_cos();
    let mut turned = gun_world(grab, [1.0, 2.1, -5.0]);
    turned.vehicles[0].rotation = [0.0, s, 0.0, c];
    let turned = draws(&mut addon, 0.4, turned);
    assert_eq!(turned.len(), 6, "the flash is over");
    let beam_params = turned[0].params.unwrap();
    assert!(
        close(&beam_params[1][..3], &[1.0, 2.1, -4.0]),
        "the grip turned with the crate: {:?}",
        beam_params[1]
    );
    let from = glam::Vec3::from_slice(&beam_params[0][..3]);
    let bend = glam::Vec3::from_slice(&beam_params[2][..3]);
    let aim = (bend - from).normalize();
    assert!(close(&aim.to_array(), &[0.0, 0.0, -1.0]), "the bend is on the aim: {aim}");
    // Let go of it flying: only the drop is heard, and the beam snaps back
    // from the grip into the muzzle (two passes and a fading glow). No
    // burst marks it; it just flies on.
    let let_go = addon
        .frame(frame(0.6, &Arc::new(gun_world([0.0, 0.0, 0.0, 5.0], [0.0, 2.0, -5.0]))))
        .unwrap()
        .clone();
    let names: Vec<&str> = let_go.sounds.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["client/sounds/drop.wav"]);
    assert_eq!(let_go.draws.len(), 3, "{:#?}", let_go.draws);
    let snap = |t: f32, addon: &mut AddOn| {
        draws(addon, t, gun_world([0.0, 0.0, 0.0, 5.0], [0.0, 2.0, -25.0]))
    };
    let halfway = snap(0.69, &mut addon)[0].params.unwrap();
    let end = glam::Vec3::from_slice(&halfway[1][..3]);
    assert!(end.z > -4.0 && end.z < -1.0, "halfway back to the muzzle: {end}");
    assert!(snap(0.8, &mut addon).is_empty(), "and gone");
}

#[test]
fn the_beam_comes_out_of_the_drawn_guns_muzzle_and_the_gun_wears_its_skin() {
    let (_, mut addon) = start("gravity-gun-fx");
    let muzzle = [0.4, 1.75, -1.1];
    let mut world = gun_world([1.0, 7.0, 1.0, 5.0], [0.0, 2.1, -5.0]);
    let transform = holding_the_gun(&mut world, muzzle);
    let drawn = addon.frame(frame(0.0, &Arc::new(world))).unwrap().clone();
    assert_eq!(drawn.draws.len(), 8, "the skin, then the beam and the rest");
    let skin = drawn.draws[0];
    assert_eq!(skin.model, transform, "drawn where the game draws the gun");
    assert_eq!(skin.params.unwrap()[0][3], 1.0, "its veins flare while the beam is on");
    let beam_params = drawn.draws[1].params.unwrap();
    assert_eq!(&beam_params[0][..3], &muzzle, "from the muzzle");
    // At rest, once the beam has snapped back, the skin stays, its veins
    // dimmed; nothing else is drawn.
    let mut world = gun_world([0.0; 4], [0.0, 2.1, -5.0]);
    holding_the_gun(&mut world, muzzle);
    let world = Arc::new(world);
    addon.frame(frame(0.1, &world)).unwrap();
    let drawn = addon.frame(frame(0.5, &world)).unwrap().clone();
    assert_eq!(drawn.draws.len(), 1);
    assert_eq!(drawn.draws[0].params.unwrap()[0][3], 0.0);
    // Someone holding another weapon: no skin.
    let mut world = gun_world([0.0; 4], [0.0, 2.1, -5.0]);
    holding_the_gun(&mut world, muzzle);
    world.players[0].image = "other:image/rifle".into();
    assert!(addon.frame(frame(0.6, &Arc::new(world))).unwrap().draws.is_empty());
}

/// Needs a GPU: renders reaching, holding, a swing and letting go to PNGs.
#[test]
#[ignore = "needs a GPU adapter"]
fn the_gravity_gun_renders_offscreen() {
    let (_, mut addon) = start("gravity-gun-fx");
    let world = |t: f32| {
        let (state, at) = if t < 0.3 {
            ([0.0, 0.0, 1.0, 6.0], [0.4, 2.2, -30.0])
        } else if t < 1.0 {
            ([1.0, 7.0, 1.0, 4.5], [0.0, 2.1, -4.5])
        } else if t < 2.0 {
            // Swung: the crate lags off to the side, the beam bends.
            ([1.0, 7.0, 1.0, 4.5], [2.5, 2.6, -3.8])
        } else {
            (
                [0.0, 0.0, 0.0, 4.5],
                [0.4, 2.6 + (t - 2.0) * 2.0, -4.5 - (t - 2.0) * 30.0],
            )
        };
        let mut world = gun_world(state, at);
        holding_the_gun(&mut world, [0.35, 1.7, -0.85]);
        Arc::new(world)
    };
    let (adapter, images) = bri_client_sandbox::gpu::render_offscreen_scene(
        &mut addon,
        640,
        384,
        &[0.0, 0.4, 0.8, 1.2, 2.0, 2.08, 2.2],
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
    assert!(lit(0) > 300, "the reaching beam shows");
    assert!(lit(1) > 2000, "the beam and bubble show");
    assert_ne!(images[2].pixels, images[3].pixels, "the swing bends it");
    assert!(lit(6) < lit(3), "the snap-back is over and leaves no burst behind");
}

#[test]
fn a_held_creature_gets_the_beam_and_bubble_too() {
    let (_, mut addon) = start("gravity-gun-fx");
    let mut world = gun_world([3.0, 5.0, 1.0, 6.0], [0.0, 2.0, -5.0]);
    world.entities = vec![Entity {
        id: 5,
        kind: "zoo:entity/cow".into(),
        feet: [1.0, 0.0, -6.0],
        yaw: 0.0,
    }];
    let drawn = addon.frame(frame(0.0, &Arc::new(world))).unwrap().clone();
    assert_eq!(drawn.draws.len(), 7, "beam, core, glows, the catch's flash, bubble and sparks");
    let beam_params = drawn.draws[0].params.unwrap();
    assert_eq!(
        &beam_params[1][..3],
        &[1.0, 1.3, -6.0],
        "to the creature's middle"
    );
}

#[test]
fn a_ragdoll_dangles_from_the_limb_the_beam_grabbed() {
    use bri_client_sandbox::bodies::{BodyState, PhysicsCommand, body_ref};
    let (code, mut addon) = start("gravity-gun-fx");
    assert!(code.capabilities.contains(&Capability::PhysicsLocal));
    // Player 1 holds player 2's corpse; the Ragdoll Add-On's arm is where
    // the beam meets it.
    let arm = body_ref(5, 1).unwrap();
    let limb = |at: [f32; 3]| {
        Arc::new(
            [(
                arm,
                BodyState {
                    position: at,
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    velocity: [0.0; 3],
                    spin: [0.0; 3],
                    resting: false,
                    shared: true,
                    group: 1,
                    mass: 1.0,
                    radius: 0.4,
                },
            )]
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>(),
        )
    };
    let world = |look: [f32; 3]| {
        let mut w = gun_world([2.0, 2.0, 1.0, 5.0], [0.0, 2.0, -25.0]);
        w.players[0].look = look;
        let mut corpse = player(2, [0.0, 0.0, -5.0], [0.0, 0.0, 1.0]);
        corpse.alive = false;
        w.players.push(corpse);
        Arc::new(w)
    };
    let run = |addon: &mut AddOn, t: f32, look: [f32; 3], at: [f32; 3]| {
        addon
            .frame(FrameInput {
                shared: limb(at),
                ..frame(t, &world(look))
            })
            .unwrap()
            .clone()
    };
    // Seen before the grab, so the grab is seen happen.
    addon
        .frame(frame(0.0, &Arc::new(gun_world([0.0; 4], [0.0, 2.0, -25.0]))))
        .unwrap();
    let grabbed = run(&mut addon, 0.01, [0.0, 0.0, -1.0], [0.0, 2.1, -5.0]);
    let holds: Vec<_> = grabbed
        .physics
        .iter()
        .filter_map(|c| match c {
            PhysicsCommand::Hold { body, point, target, .. } => Some((*body, *point, *target)),
            _ => None,
        })
        .collect();
    assert_eq!(holds.len(), 1, "{:?}", grabbed.physics);
    let (body, point, target) = holds[0];
    assert_eq!(body, arm);
    assert!(close(&point, &[0.0, 0.0, 0.4]), "gripped where the beam met it: {point:?}");
    assert!(close(&target, &[0.0, 2.1, -5.0]), "pulled to the beam's end: {target:?}");
    let beam_params = grabbed.draws[0].params.unwrap();
    assert!(close(&beam_params[1][..3], &[0.0, 2.1, -4.6]), "{:?}", beam_params[1]);
    // The arm swings aside: the beam still ends on it, and the pull moves
    // with the aim.
    let swung = run(&mut addon, 0.03, [0.1, 0.0, -0.995], [1.0, 2.1, -5.0]);
    let beam_params = swung.draws[0].params.unwrap();
    assert!(close(&beam_params[1][..3], &[1.0, 2.1, -4.6]), "{:?}", beam_params[1]);
    let moving = swung.physics.iter().any(|c| {
        matches!(c, PhysicsCommand::Hold { velocity, .. } if velocity[0] > 1.0)
    });
    assert!(moving, "{:?}", swung.physics);
    // Let go: no more pull, so the arm flies on as it was moving.
    let mut dropped = gun_world([0.0; 4], [0.0, 2.0, -25.0]);
    dropped.players.push(player(2, [0.0, 0.0, -5.0], [0.0, 0.0, 1.0]));
    let after = addon
        .frame(FrameInput {
            shared: limb([1.0, 2.1, -5.0]),
            ..frame(0.05, &Arc::new(dropped))
        })
        .unwrap()
        .clone();
    assert!(after.physics.is_empty());
}

// ---- Grapple Rope Effects ----

const LAUNCHER: &str = "grapple-rope-tool:image/grapplerope";

fn rope_state(world: &mut World, player: u64, rope: [f64; 5]) {
    world
        .state
        .entry("grapple-rope".into())
        .or_default()
        .players
        .insert(
            player,
            [("rope".to_string(), serde_json::json!(rope))].into(),
        );
}

/// Player 1 at `feet` looking along -z (eye 2.1 up), their rope as given.
fn rope_world(feet: [f32; 3], rope: [f64; 5]) -> World {
    let mut world = World {
        local: 1,
        players: vec![player(1, feet, [0.0, 0.0, -1.0])],
        ..Default::default()
    };
    rope_state(&mut world, 1, rope);
    world
}

/// The launcher drawn in player 1's hand, its muzzle 0.85 ahead of the
/// hands, with a stand-in model for the Printer.
fn holding_the_launcher(world: &mut World) -> [f32; 3] {
    let feet = glam::Vec3::from(world.players[0].feet);
    let hands = feet + glam::Vec3::new(0.35, 1.7, -0.4);
    let muzzle = (hands + glam::Vec3::new(0.0, 0.0, -0.45)).to_array();
    let me = &mut world.players[0];
    me.image = LAUNCHER.into();
    me.held = vec![Held {
        hand: 0,
        transform: glam::Mat4::from_translation(hands).to_cols_array(),
        muzzle: Some(muzzle),
    }];
    world.image_meshes.insert(LAUNCHER.into(), Arc::new(stand_in_gun()));
    muzzle
}

fn names(frame: &bri_client_sandbox::host::Frame) -> Vec<&str> {
    frame.sounds.iter().map(|s| s.name.as_str()).collect()
}

#[test]
fn the_grapple_rope_module_is_built_from_its_source() {
    built_from_source("grapple-rope-fx");
}

#[test]
fn the_grapple_rope_throws_bites_hangs_twangs_and_zips_back() {
    let (code, mut addon) = start("grapple-rope-fx");
    assert_eq!(code.name, "Grapple Rope Effects");
    let run = |addon: &mut AddOn, t: f32, world: World| {
        addon.frame(frame(t, &Arc::new(world))).unwrap().clone()
    };
    let at = [0.0, 20.0, -16.0];
    let anchor = glam::Vec3::from(at);
    // Nobody has a rope out: nothing drawn, nothing heard.
    let idle = run(&mut addon, 0.0, rope_world([0.0; 3], [0.0; 5]));
    assert!(idle.draws.is_empty() && idle.sounds.is_empty());
    // Thrown at a spot 25 away: a whoosh at the muzzle, the hook's four
    // parts leaving it, then the rope paying out behind the hook.
    let thrown = [1.0, 0.0, 20.0, -16.0, 25.0];
    let out = run(&mut addon, 0.1, rope_world([0.0; 3], thrown));
    assert_eq!(names(&out), ["client/sounds/throw.wav"]);
    assert_eq!(out.draws.len(), 4, "{:#?}", out.draws);
    let later = run(&mut addon, 0.18, rope_world([0.0; 3], thrown));
    assert!(later.sounds.is_empty());
    assert_eq!(later.draws.len(), 5);
    let crown = glam::Vec3::from_slice(&later.draws[1].params.unwrap()[0][..3]);
    let flown = crown.distance(glam::Vec3::new(0.0, 1.8, -0.9));
    assert!(flown > 9.0 && flown < 16.0, "about 13 units out after 0.08 s: {crown}");
    for part in 0..4 {
        assert_eq!(later.draws[1 + part].params.unwrap()[1][3], part as f32);
    }
    // It gets there and bites with a clank at the spot.
    let bitten = run(&mut addon, 0.3, rope_world([0.0; 3], thrown));
    assert_eq!(names(&bitten), ["client/sounds/bite.wav"]);
    assert_eq!(bitten.sounds[0].at, Some(at));
    // Hooked, the player hanging on a taut rope: straight, no sag, no
    // second clank.
    let hanging = anchor - glam::Vec3::new(0.0, 12.0, -3.0) - glam::Vec3::Y * 1.8;
    let rope_length = f64::from((anchor - hanging - glam::Vec3::Y * 1.8).length());
    let hooked = [2.0, 0.0, 20.0, -16.0, rope_length];
    let taut = run(&mut addon, 0.35, rope_world(hanging.to_array(), hooked));
    assert!(taut.sounds.is_empty(), "{:?}", names(&taut));
    let rope = taut.draws[0].params.unwrap();
    assert!(rope[1][3] < 0.05, "a taut rope is straight: sag {}", rope[1][3]);
    let tie = glam::Vec3::from_slice(&rope[1][..3]);
    assert!(tie.distance(anchor) > 0.5 && tie.distance(anchor) < 0.7, "tied behind the crown");
    // Swinging in close, the rope goes slack and hangs in a curve.
    let close_in = hanging + glam::Vec3::new(0.0, 6.0, -2.0);
    let slack = run(&mut addon, 0.5, rope_world(close_in.to_array(), hooked));
    let sag = slack.draws[0].params.unwrap()[1][3];
    assert!(sag > 1.0, "slack rope sags: {sag}");
    // Falling back out, it snaps tight: a twang, and the rope shivers.
    let snapped = run(&mut addon, 0.6, rope_world(hanging.to_array(), hooked));
    assert_eq!(names(&snapped), ["client/sounds/twang.wav"]);
    let shiver = |out: &bri_client_sandbox::host::Frame| out.draws[0].params.unwrap()[2][3].abs();
    let wobbling = (1..8)
        .map(|k| shiver(&run(&mut addon, 0.6 + k as f32 * 0.03, rope_world(hanging.to_array(), hooked))))
        .fold(0.0_f32, f32::max);
    assert!(wobbling > 0.05, "it shivers: {wobbling}");
    let settled = run(&mut addon, 1.5, rope_world(hanging.to_array(), hooked));
    assert_eq!(shiver(&settled), 0.0, "and settles");
    // Reeled in: the rope's strands run along as it shortens.
    let climbing = [2.0, 0.0, 20.0, -16.0, rope_length - 5.0];
    let before = settled.draws[0].params.unwrap()[3][3];
    let mut reeled = settled;
    for k in 0..6 {
        reeled = run(&mut addon, 1.6 + k as f32 / 60.0, rope_world(hanging.to_array(), climbing));
    }
    let run_along = reeled.draws[0].params.unwrap()[3][3];
    assert!(run_along < before - 2.0, "{before} to {run_along}");
    // Let go: the zip, the hook flying back in, then nothing.
    let let_go = run(&mut addon, 2.0, rope_world(hanging.to_array(), [0.0; 5]));
    assert_eq!(names(&let_go), ["client/sounds/zip.wav"]);
    assert_eq!(let_go.draws.len(), 5);
    let back = run(&mut addon, 2.1, rope_world(hanging.to_array(), [0.0; 5]));
    let crown = glam::Vec3::from_slice(&back.draws[1].params.unwrap()[0][..3]);
    assert!(crown.distance(anchor) > 3.0, "on its way back: {crown}");
    assert!(run(&mut addon, 2.3, rope_world(hanging.to_array(), [0.0; 5])).draws.is_empty());
    // A miss: out as far as it goes and back in.
    let missed = [3.0, 0.0, 2.0, -64.0, 64.0];
    let out = run(&mut addon, 3.0, rope_world([0.0; 3], missed));
    assert_eq!(names(&out), ["client/sounds/throw.wav"]);
    assert_eq!(run(&mut addon, 3.3, rope_world([0.0; 3], missed)).draws.len(), 5);
    assert!(run(&mut addon, 3.8, rope_world([0.0; 3], missed)).draws.is_empty());
}

#[test]
fn the_rope_comes_out_of_the_drawn_launchers_muzzle_and_the_launcher_is_carved_wood() {
    let (_, mut addon) = start("grapple-rope-fx");
    let mut world = rope_world([0.0; 3], [2.0, 0.0, 20.0, -16.0, 25.0]);
    let muzzle = holding_the_launcher(&mut world);
    let drawn = addon.frame(frame(0.0, &Arc::new(world))).unwrap().clone();
    assert_eq!(drawn.draws.len(), 6, "the skin, then the rope and the hook");
    let skin = drawn.draws[0];
    assert_eq!(skin.params.unwrap()[0][0], 1.0, "its brass glints while the hook is out");
    assert_eq!(&drawn.draws[1].params.unwrap()[0][..3], &muzzle, "from the muzzle");
    // Put away, once the hook has zipped back: just the launcher.
    let mut world = rope_world([0.0; 3], [0.0; 5]);
    holding_the_launcher(&mut world);
    let world = Arc::new(world);
    addon.frame(frame(1.0, &world)).unwrap();
    let rest = addon.frame(frame(1.5, &world)).unwrap().clone();
    assert_eq!(rest.draws.len(), 1);
    assert_eq!(rest.draws[0].params.unwrap()[0][0], 0.0);
    // Someone holding another weapon: no skin.
    let mut world = rope_world([0.0; 3], [0.0; 5]);
    holding_the_launcher(&mut world);
    world.players[0].image = "other:image/rifle".into();
    assert!(addon.frame(frame(1.6, &Arc::new(world))).unwrap().draws.is_empty());
}

/// Needs a GPU: renders a throw, a slack hang, a taut swing and the
/// launcher up close to PNGs.
#[test]
#[ignore = "needs a GPU adapter"]
fn the_grapple_rope_renders_offscreen() {
    let (_, mut addon) = start("grapple-rope-fx");
    let world = |t: f32| {
        let (feet, rope) = if t < 0.5 {
            ([0.0, 0.0, 0.0], [1.0, 0.0, 9.0, -8.0, 12.0])
        } else if t < 1.5 {
            ([0.0, 2.0, -2.0], [2.0, 0.0, 9.0, -8.0, 12.0])
        } else {
            ([0.0, 0.0, 0.0], [2.0, 0.0, 9.0, -8.0, 7.0])
        };
        let mut world = rope_world(feet, rope);
        holding_the_launcher(&mut world);
        Arc::new(world)
    };
    let (adapter, images) = bri_client_sandbox::gpu::render_offscreen_scene(
        &mut addon,
        640,
        384,
        &[0.0, 0.05, 1.0, 2.0],
        glam::Vec3::new(5.0, 4.0, 3.0),
        glam::Vec3::new(0.0, 4.0, -4.0),
        world,
    )
    .unwrap();
    save("grapple-rope", &images);
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
    assert!(lit(1) > lit(0), "the rope pays out as the hook flies");
    assert_ne!(images[2].pixels, images[3].pixels, "slack sags, taut is straight");
}

// ---- Grappling Hook Effects ----

const WINCH: &str = "grappling-hook-tool:image/grapplinghook";

/// Player 1 at `feet` looking along -z (eye 2.1 up), their hook as given:
/// [phase, x, y, z, distance, kind, id].
fn hook_world(feet: [f32; 3], hook: [f64; 7]) -> World {
    let mut world = World {
        local: 1,
        players: vec![player(1, feet, [0.0, 0.0, -1.0])],
        ..Default::default()
    };
    world
        .state
        .entry("grappling-hook".into())
        .or_default()
        .players
        .insert(1, [("hook".to_string(), serde_json::json!(hook))].into());
    world
}

/// The winch gun drawn in player 1's hand, as `holding_the_launcher`.
fn holding_the_winch(world: &mut World) -> [f32; 3] {
    let muzzle = holding_the_launcher(world);
    world.players[0].image = WINCH.into();
    world.image_meshes.insert(WINCH.into(), Arc::new(stand_in_gun()));
    muzzle
}

#[test]
fn the_grappling_hook_module_is_built_from_its_source() {
    built_from_source("grappling-hook-fx");
}

#[test]
fn the_grappling_hook_fires_bites_winches_and_zips_back() {
    let (code, mut addon) = start("grappling-hook-fx");
    assert_eq!(code.name, "Grappling Hook Effects");
    let run = |addon: &mut AddOn, t: f32, world: World| {
        addon.frame(frame(t, &Arc::new(world))).unwrap().clone()
    };
    let at = [0.0, 20.0, -16.0];
    let anchor = glam::Vec3::from(at);
    let idle = run(&mut addon, 0.0, hook_world([0.0; 3], [0.0; 7]));
    assert!(idle.draws.is_empty() && idle.sounds.is_empty());
    // Fired at a spot 25 away: a crack at the hands (nothing in them),
    // the grapnel's five parts leaving, then the cable paying out.
    let fired = [1.0, 0.0, 20.0, -16.0, 25.0, 0.0, 0.0];
    let out = run(&mut addon, 0.1, hook_world([0.0; 3], fired));
    assert_eq!(names(&out), ["client/sounds/fire.wav"]);
    assert_eq!(out.sounds[0].at, Some([0.0, 2.25, 0.0]), "from the hands");
    assert_eq!(out.draws.len(), 5, "{:#?}", out.draws);
    let later = run(&mut addon, 0.16, hook_world([0.0; 3], fired));
    assert_eq!(later.draws.len(), 6);
    let crown = glam::Vec3::from_slice(&later.draws[1].params.unwrap()[0][..3]);
    let flown = crown.distance(glam::Vec3::new(0.0, 2.25, 0.0));
    assert!(flown > 9.0 && flown < 15.0, "12 units out after 0.06 s: {crown}");
    for part in 0..5 {
        assert_eq!(later.draws[1 + part].params.unwrap()[1][3], part as f32);
    }
    // It bites with a clang at the spot.
    let bitten = run(&mut addon, 0.3, hook_world([0.0; 3], fired));
    assert_eq!(names(&bitten), ["client/sounds/clamp.wav"]);
    assert_eq!(bitten.sounds[0].at, Some(at));
    // Hooked: the winch whirs, the claws spring open, the cable is taut
    // and buzzes, then settles.
    let hooked = [2.0, 0.0, 20.0, -16.0, 25.0, 0.0, 0.0];
    let winching = run(&mut addon, 0.35, hook_world([0.0; 3], hooked));
    assert_eq!(names(&winching), ["client/sounds/winch.wav"]);
    let open = |out: &bri_client_sandbox::host::Frame| out.draws[1].params.unwrap()[3][3];
    let opened = run(&mut addon, 0.5, hook_world([0.0, 4.0, -3.0], hooked));
    assert_eq!(open(&opened), 1.0);
    let cable = opened.draws[0].params.unwrap();
    assert!(cable[1][3] < 0.1, "taut: sag {}", cable[1][3]);
    let tie = glam::Vec3::from_slice(&cable[1][..3]);
    assert!((tie.distance(anchor) - 0.6).abs() < 0.01, "tied behind the crown");
    let buzz = |out: &bri_client_sandbox::host::Frame| out.draws[0].params.unwrap()[2][3].abs();
    let buzzing = (1..8)
        .map(|k| buzz(&run(&mut addon, 0.35 + k as f32 * 0.02, hook_world([0.0, 4.0, -3.0], hooked))))
        .fold(0.0_f32, f32::max);
    assert!(buzzing > 0.03, "it buzzes: {buzzing}");
    // Pulled in, the strands run along as the cable shortens.
    let before = run(&mut addon, 1.2, hook_world([0.0, 4.0, -3.0], hooked));
    assert_eq!(buzz(&before), 0.0, "and settles");
    let pulled = run(&mut addon, 1.25, hook_world([0.0, 12.0, -10.0], hooked));
    let ran = pulled.draws[0].params.unwrap()[3][3] - before.draws[0].params.unwrap()[3][3];
    assert!(ran < -5.0, "{ran}");
    // Let go: a whirr, the grapnel folding back in, then nothing.
    let let_go = run(&mut addon, 2.0, hook_world([0.0, 12.0, -10.0], [0.0; 7]));
    assert_eq!(names(&let_go), ["client/sounds/release.wav"]);
    assert_eq!(let_go.draws.len(), 6);
    assert!(run(&mut addon, 2.3, hook_world([0.0; 3], [0.0; 7])).draws.is_empty());
    // A miss: out and back.
    let missed = [3.0, 0.0, 2.0, -80.0, 80.0, 0.0, 0.0];
    let out = run(&mut addon, 3.0, hook_world([0.0; 3], missed));
    assert_eq!(names(&out), ["client/sounds/fire.wav"]);
    assert_eq!(run(&mut addon, 3.3, hook_world([0.0; 3], missed)).draws.len(), 6);
    assert!(run(&mut addon, 3.9, hook_world([0.0; 3], missed)).draws.is_empty());
}

#[test]
fn a_grapnel_in_a_vehicle_or_player_rides_along_with_it() {
    let (_, mut addon) = start("grappling-hook-fx");
    let crown = |out: &bri_client_sandbox::host::Frame| {
        glam::Vec3::from_slice(&out.draws[1].params.unwrap()[0][..3])
    };
    // Bitten into a vehicle 2 units right of its middle.
    let truck = |x: f32, turn: f32| Vehicle {
        id: 7,
        definition: "v20.vehicle.jeep".into(),
        position: [x, 1.0, -10.0],
        rotation: glam::Quat::from_rotation_y(turn).to_array(),
        velocity: [0.0; 3],
        radius: 3.0,
    };
    let hooked = [2.0, 2.0, 1.0, -10.0, 10.0, 2.0, 7.0];
    let world = |x: f32, turn: f32| {
        let mut w = hook_world([0.0; 3], hooked);
        w.vehicles = vec![truck(x, turn)];
        w
    };
    let first = addon.frame(frame(0.0, &Arc::new(world(0.0, 0.0)))).unwrap().clone();
    assert!(crown(&first).distance(glam::Vec3::new(2.0, 1.0, -10.0)) < 0.01);
    // It drives 5 on and turns a quarter: the grapnel is on the same spot.
    let moved = addon
        .frame(frame(0.1, &Arc::new(world(5.0, std::f32::consts::FRAC_PI_2))))
        .unwrap()
        .clone();
    let spot = glam::Vec3::new(5.0, 1.0, -10.0)
        + glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * glam::Vec3::new(2.0, 0.0, 0.0);
    assert!(crown(&moved).distance(spot) < 0.01, "{} vs {spot}", crown(&moved));
    // Bitten into player 2's chest: it follows them.
    let (_, mut addon) = start("grappling-hook-fx");
    let hooked = [2.0, 0.0, 1.5, -8.0, 8.0, 1.0, 2.0];
    let world = |z: f32| {
        let mut w = hook_world([0.0; 3], hooked);
        w.players.push(player(2, [0.0, 0.0, z], [0.0, 0.0, 1.0]));
        w
    };
    addon.frame(frame(0.0, &Arc::new(world(-8.0)))).unwrap();
    let ran = addon.frame(frame(0.1, &Arc::new(world(-14.0)))).unwrap().clone();
    assert!(crown(&ran).distance(glam::Vec3::new(0.0, 1.5, -14.0)) < 0.01, "{}", crown(&ran));
}

#[test]
fn the_cable_comes_out_of_the_winch_guns_muzzle_and_the_gun_wears_its_skin() {
    let (_, mut addon) = start("grappling-hook-fx");
    let mut world = hook_world([0.0; 3], [2.0, 0.0, 20.0, -16.0, 25.0, 0.0, 0.0]);
    let muzzle = holding_the_winch(&mut world);
    let drawn = addon.frame(frame(0.0, &Arc::new(world))).unwrap().clone();
    assert_eq!(drawn.draws.len(), 7, "the skin, then the cable and the grapnel");
    assert_eq!(drawn.draws[0].params.unwrap()[0][0], 1.0, "its gauge glows while the grapnel is out");
    assert_eq!(&drawn.draws[1].params.unwrap()[0][..3], &muzzle, "from the muzzle");
    // Holding the Grapple Rope's launcher instead: no winch skin, and the
    // cable comes from the hands.
    let mut world = hook_world([0.0; 3], [2.0, 0.0, 20.0, -16.0, 25.0, 0.0, 0.0]);
    holding_the_launcher(&mut world);
    let other = addon.frame(frame(0.1, &Arc::new(world))).unwrap().clone();
    assert_eq!(other.draws.len(), 6);
    assert_eq!(&other.draws[0].params.unwrap()[0][..3], &[0.0, 2.25, 0.0]);
}

/// Needs a GPU: renders a shot, the grapnel biting, the hang and the
/// winch gun up close to PNGs.
#[test]
#[ignore = "needs a GPU adapter"]
fn the_grappling_hook_renders_offscreen() {
    let (_, mut addon) = start("grappling-hook-fx");
    let world = |t: f32| {
        let (feet, hook) = if t < 0.5 {
            ([0.0, 0.0, 0.0], [1.0, 0.0, 9.0, -8.0, 12.0, 0.0, 0.0])
        } else {
            ([0.0, 3.0, -4.0], [2.0, 0.0, 9.0, -8.0, 12.0, 0.0, 0.0])
        };
        let mut world = hook_world(feet, hook);
        holding_the_winch(&mut world);
        Arc::new(world)
    };
    let (adapter, images) = bri_client_sandbox::gpu::render_offscreen_scene(
        &mut addon,
        640,
        384,
        &[0.0, 0.04, 1.0, 2.0],
        glam::Vec3::new(5.0, 4.0, 3.0),
        glam::Vec3::new(0.0, 4.0, -4.0),
        world,
    )
    .unwrap();
    save("grappling-hook", &images);
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
    assert!(lit(1) > lit(0), "the cable pays out as the grapnel flies");
}
