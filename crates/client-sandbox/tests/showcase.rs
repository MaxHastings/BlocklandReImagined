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

fn beam(world: &mut World, player: u64, beam: [f64; 7]) {
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
fn gun_world(beam_state: [f64; 7], crate_at_: [f32; 3]) -> World {
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
    let idle = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    assert!(draws(&mut addon, 0.0, gun_world(idle, [0.0, 2.0, -5.0])).is_empty());
    // The trigger held with nothing caught: a thinner beam (glow and
    // core) out to where it points, and a glow at the muzzle.
    let reaching = draws(
        &mut addon,
        0.05,
        gun_world([0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 8.0], [0.0, 2.0, -25.0]),
    );
    assert_eq!(reaching.len(), 3, "{reaching:#?}");
    assert!(close(&reaching[0].params.unwrap()[1][..3], &[0.0, 2.1, -8.0]));
    // Holding the crate, grabbed a unit left of its middle: two beam
    // passes, the grip and muzzle glows, the bubble, the orbiting sparks,
    // and the grab heard at the crate.
    let grab = [1.0, 7.0, 1.0, 0.0, 0.0, 0.0, 5.0];
    let grabbed = addon
        .frame(frame(0.1, &Arc::new(gun_world(grab, [1.0, 2.1, -5.0]))))
        .unwrap()
        .clone();
    let held = grabbed.draws.clone();
    assert_eq!(held.len(), 6, "{held:#?}");
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
    let beam_params = draws(&mut addon, 0.2, turned)[0].params.unwrap();
    assert!(
        close(&beam_params[1][..3], &[1.0, 2.1, -4.0]),
        "the grip turned with the crate: {:?}",
        beam_params[1]
    );
    let from = glam::Vec3::from_slice(&beam_params[0][..3]);
    let bend = glam::Vec3::from_slice(&beam_params[2][..3]);
    let aim = (bend - from).normalize();
    assert!(close(&aim.to_array(), &[0.0, 0.0, -1.0]), "the bend is on the aim: {aim}");
    // The throw: the beam goes, a shockwave and sparks mark it, and it
    // booms (not the drop's fall-away)...
    let launched = addon
        .frame(frame(
            0.6,
            &Arc::new(gun_world(
                [0.0, 0.0, 0.0, 1.0, 1.0, 7.0, 5.0],
                [0.0, 2.0, -5.0],
            )),
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
            gun_world([0.0, 0.0, 0.0, 1.0, 1.0, 7.0, 5.0], [0.0, 2.0, -25.0])
        )
        .is_empty()
    );
    // Let go without a throw: the drop is heard.
    draws(
        &mut addon,
        1.6,
        gun_world([1.0, 7.0, 1.0, 1.0, 1.0, 7.0, 5.0], [0.0, 2.1, -5.0]),
    );
    let dropped = addon
        .frame(frame(
            1.7,
            &Arc::new(gun_world(
                [0.0, 0.0, 0.0, 1.0, 1.0, 7.0, 5.0],
                [0.0, 2.1, -5.0],
            )),
        ))
        .unwrap()
        .clone();
    let names: Vec<&str> = dropped.sounds.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["client/sounds/drop.wav"]);
    // A player seen for the first time with throws already made shows no
    // stale burst.
    let (_, mut fresh) = start("gravity-gun-fx");
    assert!(
        draws(
            &mut fresh,
            0.0,
            gun_world([0.0, 0.0, 0.0, 5.0, 1.0, 7.0, 5.0], [0.0, 2.0, -5.0])
        )
        .is_empty()
    );
}

#[test]
fn the_beam_comes_out_of_the_drawn_guns_muzzle_and_the_gun_wears_its_skin() {
    let (_, mut addon) = start("gravity-gun-fx");
    let muzzle = [0.4, 1.75, -1.1];
    let mut world = gun_world([1.0, 7.0, 1.0, 0.0, 0.0, 0.0, 5.0], [0.0, 2.1, -5.0]);
    let transform = holding_the_gun(&mut world, muzzle);
    let drawn = addon.frame(frame(0.0, &Arc::new(world))).unwrap().clone();
    assert_eq!(drawn.draws.len(), 7, "the skin, then the beam and the rest");
    let skin = drawn.draws[0];
    assert_eq!(skin.model, transform, "drawn where the game draws the gun");
    assert_eq!(skin.params.unwrap()[0][3], 1.0, "its veins flare while the beam is on");
    let beam_params = drawn.draws[1].params.unwrap();
    assert_eq!(&beam_params[0][..3], &muzzle, "from the muzzle");
    // At rest the skin stays, its veins dimmed; nothing else is drawn.
    let mut world = gun_world([0.0; 7], [0.0, 2.1, -5.0]);
    holding_the_gun(&mut world, muzzle);
    let drawn = addon.frame(frame(0.1, &Arc::new(world))).unwrap().clone();
    assert_eq!(drawn.draws.len(), 1);
    assert_eq!(drawn.draws[0].params.unwrap()[0][3], 0.0);
    // Someone holding another weapon: no skin.
    let mut world = gun_world([0.0; 7], [0.0, 2.1, -5.0]);
    holding_the_gun(&mut world, muzzle);
    world.players[0].image = "other:image/rifle".into();
    assert!(addon.frame(frame(0.2, &Arc::new(world))).unwrap().draws.is_empty());
}

/// Needs a GPU: renders reaching, holding, a swing and a throw to PNGs.
#[test]
#[ignore = "needs a GPU adapter"]
fn the_gravity_gun_renders_offscreen() {
    let (_, mut addon) = start("gravity-gun-fx");
    let world = |t: f32| {
        let (state, at) = if t < 0.3 {
            ([0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 6.0], [0.4, 2.2, -30.0])
        } else if t < 1.0 {
            ([1.0, 7.0, 1.0, 0.0, 0.0, 0.0, 4.5], [0.0, 2.1, -4.5])
        } else if t < 2.0 {
            // Swung: the crate lags off to the side, the beam bends.
            ([1.0, 7.0, 1.0, 0.0, 0.0, 0.0, 4.5], [2.5, 2.6, -3.8])
        } else {
            (
                [0.0, 0.0, 0.0, 1.0, 1.0, 7.0, 4.5],
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
    assert!(lit(5) > 500, "the throw's shockwave shows");
}

#[test]
fn a_held_creature_gets_the_beam_and_bubble_too() {
    let (_, mut addon) = start("gravity-gun-fx");
    let mut world = gun_world([3.0, 5.0, 1.0, 0.0, 0.0, 0.0, 6.0], [0.0, 2.0, -5.0]);
    world.entities = vec![Entity {
        id: 5,
        kind: "zoo:entity/cow".into(),
        feet: [1.0, 0.0, -6.0],
        yaw: 0.0,
    }];
    let drawn = addon.frame(frame(0.0, &Arc::new(world))).unwrap().clone();
    assert_eq!(drawn.draws.len(), 6, "beam, core, glows, bubble and sparks");
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
        let mut w = gun_world([2.0, 2.0, 1.0, 0.0, 0.0, 0.0, 5.0], [0.0, 2.0, -25.0]);
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
        .frame(frame(0.0, &Arc::new(gun_world([0.0; 7], [0.0, 2.0, -25.0]))))
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
    let mut dropped = gun_world([0.0; 7], [0.0, 2.0, -25.0]);
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
