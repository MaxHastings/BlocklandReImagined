//! Recording a match and replaying it: the same calls into a session set
//! up the same way play out the same, tick for tick, and a changed input
//! is caught at the tick it changes the match.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
    shape::{Node, Shape},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    replay::{Call, Frame, FrameReader, FrameWriter, Recorder, Report, Secrets, replay},
    session::{Command, MiniGameRequest, Session, shape_mount_points},
    simulation::Simulation,
};
use bri_world::{OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// Bots in the recorded match, one per spawn brick.
const BOTS: usize = 3;
/// Ticks the recorded match plays: long enough for bots to spawn, wander,
/// find the players and fight.
const MATCH_TICKS: u64 = 1800;
/// The tick from which the tampered replay's one input changes.
const TAMPERED_TICK: u64 = 900;

fn definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: "brick".into(),
        footprint_studs: [2, 2],
        height_plates: 3,
        attachment_rows: vec!["bb".into(); 6],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "brick".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.6, 1.0],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definitions {
        entries: [(
            "brick".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
                reflection: None,
                link: None,
                glass: [0.0; 4],
                bot: None,
            },
        )]
        .into(),
    }
}

fn node(name: &str, parent: Option<usize>, translation: [f32; 3]) -> Node {
    Node {
        name: name.into(),
        parent,
        translation,
        rotation: [0.0, 0.0, 0.0, 1.0],
    }
}
fn blockhead() -> Shape {
    Shape {
        schema_version: 1,
        id: "v20.shape.m".into(),
        nodes: vec![
            node("root", None, [0.0; 3]),
            node("chest", Some(0), [0.0, 1.5, 0.0]),
            node("Mount0", Some(1), [0.5, 0.2, -0.3]),
        ],
        objects: vec![],
        details: vec![],
        meshes: vec![],
        materials: vec![],
        animations: vec![],
    }
}

/// A flat field with a Blockhead Bot spawn brick per bot.
fn world() -> World {
    let mut world = World::new("Replay".into(), "replay".into(), vec![[1.0; 4]]);
    world
        .owners
        .insert(1, bri_world::OwnerRecord::new([1; 32], "Builder".into()));
    for i in 0..BOTS {
        let id = i as u64 + 1;
        let mut brick = bri_world::Brick::new(
            bri_world::ContentRef::Resolved("brick".into()),
            [i as f32 * 8.0 - 8.0, 0.3, -12.0],
            1,
        );
        brick.vehicle = Some(Box::new(bri_world::VehicleSpawn {
            vehicle: bri_world::ContentRef::Resolved("bot.blockhead".into()),
            recolor: false,
            team: None,
        }));
        world.bricks.insert(id, brick);
    }
    world.next_brick_id = BOTS as u64 + 1;
    world
}

fn packages() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages")
}

/// The session every run starts from: built the same way each time, as a
/// host builds it from the same content.
fn session(admin_password: &str) -> Session {
    let mut s = Session::new(
        Simulation::new(
            world(),
            definitions(),
            vec![
                ColliderBuilder::cuboid(200.0, 0.5, 200.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s.set_body_mount_points("v20.shape.m", shape_mount_points(&blockhead()))
        .unwrap();
    s.set_vehicle_pack(
        bri_vehicles::Pack::load(packages().join("showcase/steel-ball-kit/assets/vehicles.json"))
            .unwrap(),
        Vec::new(),
    )
    .unwrap();
    s.set_bot_kinds(
        bri_sim::bot_kind::BotPack::from_json(include_bytes!(
            "../../../packages/blockhead_bot/assets/bots.json"
        ))
        .unwrap()
        .bots,
    )
    .unwrap();
    s.set_admin_passwords(
        bri_admin::Secret::new(admin_password.into()).unwrap(),
        bri_admin::Secret::new(String::new()).unwrap(),
    )
    .unwrap();
    s
}

/// What a recording is written into, readable once the match is over.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for Shared {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

const ADMIN_PASSWORD: &str = "hunter2";

/// Two players run, turn, jump and plant around three bots, one logs in as
/// admin (once wrongly), and the host takes what it takes; all recorded
/// through the host's [`Recorder`]. Returns the recording.
fn record_match() -> Vec<u8> {
    let mut s = session(ADMIN_PASSWORD);
    let file = Shared::default();
    let mut secrets = Secrets::default();
    secrets.redact(&bri_admin::Secret::new(ADMIN_PASSWORD.into()).unwrap());
    let mut host = Recorder::start(&mut s, secrets, Box::new(file.clone())).unwrap();
    let a = host
        .join_verified(
            &mut s,
            "Alice".into(),
            Vec3::ZERO,
            false,
            Some(bri_admin::Principal([1; 32])),
        )
        .unwrap();
    let b = host
        .join_verified(&mut s, "Bob".into(), Vec3::new(3.0, 0.0, 0.0), false, None)
        .unwrap();
    let mut sequence = [0u64; 2];
    let mut moves = [0u64; 2];
    for tick in 0..MATCH_TICKS {
        for (i, owner) in [a, b].into_iter().enumerate() {
            moves[i] += 1;
            let t = tick as f32 / 120.0 + i as f32;
            let input = MoveInput {
                forward: (t * 0.7).sin(),
                right: (t * 0.3).cos() * 0.5,
                yaw: (t * 0.4).sin() * 3.0,
                pitch: 0.0,
                jump: tick % 200 == 50 * i as u64,
                ..MoveInput::default()
            };
            host.movement(&mut s, owner, moves[i], input).unwrap();
        }
        // Alice's bricks' bots play in her minigame, and Bob joins it.
        if tick == 30 {
            sequence[0] += 1;
            host.command(
                &mut s,
                a,
                sequence[0],
                Command::MiniGame(MiniGameRequest::Create {
                    color: 0,
                    settings: bri_minigames::Settings::default(),
                }),
                None,
                |_| Ok(()),
            )
            .unwrap();
        }
        if tick == 40 {
            sequence[1] += 1;
            let game = s.minigame_views()[0].id;
            host.command(
                &mut s,
                b,
                sequence[1],
                Command::MiniGame(MiniGameRequest::Join { game }),
                None,
                |_| Ok(()),
            )
            .unwrap();
        }
        if tick == 200 || tick == 260 {
            sequence[0] += 1;
            let password = if tick == 200 { "wrong" } else { ADMIN_PASSWORD };
            let _ = host.command(
                &mut s,
                a,
                sequence[0],
                Command::Admin(bri_admin::Request::new(bri_admin::Action::Login {
                    password: bri_admin::Secret::new(password.into()).unwrap(),
                })),
                None,
                |_| Ok(()),
            );
        }
        if tick % 240 == 120 {
            sequence[1] += 1;
            let _ = host.command(
                &mut s,
                b,
                sequence[1],
                Command::Plant {
                    definition: "brick".into(),
                    position: [tick as f32 / 60.0, 0.3, 6.0],
                    quarter_turns: 0,
                    color: 0,
                },
                None,
                |_| Ok(()),
            );
        }
        host.step(&mut s, Session::step).unwrap();
        // Replication's cadence, as the network host takes it.
        if tick % 6 == 0 {
            host.take_dirty(&mut s);
            host.take_cues(&mut s);
            host.take_private_notices(&mut s);
        }
    }
    host.disconnect(&mut s, b).unwrap();
    host.step(&mut s, Session::step).unwrap();
    let bots = s.names().keys().filter(|o| s.is_bot(**o)).count();
    assert_eq!(bots, BOTS, "every bot spawned and stayed");
    // They went after the players, far from their bricks.
    for (p, _) in s
        .motion_states()
        .into_iter()
        .filter(|(p, _)| s.is_bot(p.owner))
    {
        let from_bricks = Vec3::from(p.feet).distance(Vec3::new(0.0, 0.0, -12.0));
        assert!(
            from_bricks > 20.0,
            "bot {} stayed home: {:?}",
            p.owner,
            p.feet
        );
    }
    host.finish();
    file.0.lock().unwrap().clone()
}

fn frames(bytes: &[u8]) -> Vec<Frame> {
    let mut reader = FrameReader::new(bytes);
    let mut frames = Vec::new();
    while let Some(frame) = reader.next_frame().unwrap() {
        frames.push(frame);
    }
    assert_eq!(reader.cut_off, None);
    frames
}

fn replay_of(frames: &[Frame]) -> Report {
    let mut bytes = Vec::new();
    let mut writer = FrameWriter::new(&mut bytes);
    for frame in frames {
        writer.write(frame).unwrap();
    }
    // The recording's password is its stand-in.
    let mut s = session("replay-secret-1");
    replay(
        &mut s,
        &mut FrameReader::new(bytes.as_slice()),
        &mut |map, _| anyhow::bail!("This match never changes to {map}"),
    )
    .unwrap()
}

#[test]
fn a_recorded_match_replays_tick_for_tick() {
    let recording = record_match();
    let frames = frames(&recording);
    let report = replay_of(&frames);
    assert_eq!(report.divergence, None, "{report:#?}");
    assert_eq!(
        report.ticks,
        MATCH_TICKS + 2,
        "the start and every tick checked"
    );
    assert_eq!(report.cut_off, None);
    // Twice, the same: nothing outside the recording leaks in.
    assert_eq!(replay_of(&frames), report);
}

#[test]
fn a_recording_keeps_no_password() {
    let recording = record_match();
    let text = String::from_utf8_lossy(&recording);
    assert!(
        !text.contains(ADMIN_PASSWORD),
        "the admin password is a stand-in"
    );
    assert!(text.contains("replay-secret-1"));
}

#[test]
fn a_changed_input_is_caught_where_it_changes_the_match() {
    let mut frames = frames(&record_match());
    // From the tampered tick on, Alice (player 1) holds still.
    let mut tick = 0;
    for frame in &mut frames {
        match frame {
            Frame::Tick { tick: t, .. } => tick = *t,
            Frame::Call {
                call: Call::Movement { owner, input, .. },
                ..
            } if *owner == 1 && tick >= TAMPERED_TICK => *input = MoveInput::default(),
            _ => {}
        }
    }
    let report = replay_of(&frames);
    let divergence = report.divergence.expect("the change is caught");
    // Her next tick moves her differently; the check after it sees that.
    assert_eq!(divergence.tick, TAMPERED_TICK + 1, "{divergence:#?}");
    let later = divergence.later.expect("a full check follows");
    assert!(later.parts.contains(&"players"), "{later:#?}");
}

#[test]
fn a_cut_off_recording_replays_what_it_holds() {
    let recording = record_match();
    let cut = &recording[..recording.len() / 2];
    let mut s = session("replay-secret-1");
    let report = replay(&mut s, &mut FrameReader::new(cut), &mut |map, _| {
        anyhow::bail!("This match never changes to {map}")
    })
    .unwrap();
    assert!(report.cut_off.is_some());
    assert_eq!(report.divergence, None, "{report:#?}");
    assert!(report.ticks > MATCH_TICKS / 4);
}

/// A recorder that is off changes nothing: the same match, called through
/// it, ends as the same match called directly.
#[test]
fn an_off_recorder_passes_calls_straight_through() {
    let mut direct = session(ADMIN_PASSWORD);
    let mut through = session(ADMIN_PASSWORD);
    let mut host = Recorder::off();
    let a = direct.join("Alice".into(), Vec3::ZERO, false).unwrap();
    assert_eq!(
        host.join_verified(&mut through, "Alice".into(), Vec3::ZERO, false, None)
            .unwrap(),
        a
    );
    let input = MoveInput {
        forward: 1.0,
        ..MoveInput::default()
    };
    for n in 1..=240 {
        direct.movement(a, n, input).unwrap();
        direct.step().unwrap();
        host.movement(&mut through, a, n, input).unwrap();
        host.step(&mut through, Session::step).unwrap();
    }
    let state = |s: &Session, owner: OwnerId| {
        s.motion_states()
            .into_iter()
            .find(|(p, _)| p.owner == owner)
            .unwrap()
            .0
    };
    assert_eq!(state(&direct, a), state(&through, a));
}
