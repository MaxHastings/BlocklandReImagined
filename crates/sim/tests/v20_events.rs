//! Wrench event rules checked against v20's scripts. Each test names the
//! v20 line (`.research/bl-decompiled/v20/server/scripts/allGameScripts.cs`)
//! it follows; docs/audits/v20-behaviour.md lists them all.
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_events::{Catalog, InputDef, OutputDef, Param};
use bri_minigames::Settings;
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    presentation::CueKind,
    session::{Command, MiniGameRequest, Session},
    simulation::Simulation,
};
use bri_world::{EventRow, EventTarget, EventValue, OwnerId, World, authority::Edit};
use glam::Vec3;
use rapier3d::prelude::*;

/// The vanilla inputs and the outputs these tests use, in v20's shape.
fn catalog() -> Catalog {
    let input = |name: &str| InputDef {
        id: format!("in/{name}"),
        class_name: "fxDTSBrick".into(),
        name: name.into(),
        targets: [
            ("Self", "fxDTSBrick"),
            ("Player", "Player"),
            ("Client", "GameConnection"),
            ("MiniGame", "MiniGame"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect(),
        source: "v20".into(),
        source_line: 17122,
    };
    let output = |class: &str, name: &str, params| OutputDef {
        id: format!("out/{class}/{name}"),
        class_name: class.into(),
        name: name.into(),
        params,
        append_client: true,
        source: "v20".into(),
        source_line: 17368,
    };
    Catalog {
        schema_version: 1,
        inputs: vec![input("onActivate"), input("onPlayerTouch")],
        outputs: vec![
            output("fxDTSBrick", "setColor", vec![Param::PaintColor { default: 0 }]),
            output("fxDTSBrick", "setRendering", vec![Param::Bool]),
            output("fxDTSBrick", "setRayCasting", vec![Param::Bool]),
            output(
                "fxDTSBrick",
                "spawnExplosion",
                vec![
                    Param::Datablock {
                        class_name: "ProjectileData".into(),
                    },
                    Param::Float {
                        min: 0.2,
                        max: 2.0,
                        step: 0.1,
                        default: 1.0,
                    },
                ],
            ),
            output(
                "fxDTSBrick",
                "radiusImpulse",
                vec![
                    Param::Int { min: 1, max: 100, default: 5 },
                    Param::Int { min: -50000, max: 50000, default: 50 },
                    Param::Int { min: -50000, max: 50000, default: 10 },
                ],
            ),
            output(
                "fxDTSBrick",
                "fakeKillBrick",
                vec![
                    Param::Vector { max_length: 200.0 },
                    Param::Int { min: 0, max: 300, default: 5 },
                ],
            ),
            output("Player", "Kill", vec![]),
            output("MiniGame", "Reset", vec![]),
        ],
        sources: vec![],
        scope: serde_json::Value::Null,
    }
}

fn session(lan: bool) -> Session {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 2],
        height_plates: 3,
        attachment_rows: vec!["bb".into(); 6],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
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
    let definitions = Definitions {
        entries: [(
            "plate".into(),
            Definition {
                mesh,
                collision,
                shape,
                indestructible: false,
                special: Default::default(),
            },
        )]
        .into(),
    };
    let mut s = Session::new(
        Simulation::new(
            World::new("V20".into(), "v20".into(), vec![[1.0; 4], [0.0; 4]]),
            definitions,
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.set_lan_host(lan);
    s.set_event_catalog(catalog(), Vec::new()).unwrap();
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s
}

fn row(input: &str, target: bri_events::Slot, output: &str, params: Vec<EventValue>) -> EventRow {
    EventRow {
        preserved: None,
        enabled: true,
        input: input.into(),
        delay_ms: 0,
        target: EventTarget::Slot(target),
        output: output.into(),
        params,
    }
}

/// Plant a brick for `owner` at `position` with these event rows.
fn evented_brick(s: &mut Session, owner: OwnerId, seq: u64, position: [f32; 3], rows: Vec<EventRow>) -> u64 {
    let bri_sim::session::Reply::Planted(id) = s
        .command(
            owner,
            seq,
            Command::Plant {
                definition: "plate".into(),
                position,
                quarter_turns: 0,
                color: 0,
            },
        )
        .unwrap()
    else {
        panic!("expected a plant")
    };
    s.edit_brick(owner, id, Edit::Events(rows)).unwrap();
    id
}

/// Step with every player standing still, so none is dropped as starved.
fn steps(s: &mut Session, players: &[OwnerId], n: u64) {
    for _ in 0..n {
        for &p in players {
            let seq = 1_000_000 + s.simulation().state().tick;
            s.movement(p, seq, MoveInput::default()).unwrap();
        }
        s.step().unwrap();
    }
}

/// allGameScripts.cs:9527 `Player::kill` is `Damage(self, ..., 10000)`, and
/// `Armor::Damage` (9201) has no minigame check: a "kill brick" kills
/// whoever sets it off, in or out of a minigame, on any server, once their
/// spawn protection is over.
#[test]
fn a_kill_brick_kills_whoever_sets_it_off_outside_minigames() {
    for lan in [true, false] {
        let mut s = session(lan);
        let builder = s.join("Builder".into(), Vec3::new(5.0, 0.05, 0.0), false).unwrap();
        let walker = s.join("Walker".into(), Vec3::new(-5.0, 0.05, 0.0), false).unwrap();
        steps(&mut s, &[builder, walker], 10);
        let brick = evented_brick(
            &mut s,
            builder,
            1,
            [5.0, 0.3, -3.0],
            vec![row("onActivate", bri_events::Slot::Player, "Kill", vec![])],
        );
        // `Armor::Damage` (9207) spares a player for the 2.5 s after spawning.
        s.fire_brick_input(brick, "onActivate", Some(walker));
        steps(&mut s, &[builder, walker], 2);
        assert!(s.is_alive(walker), "lan {lan}: killed while spawn-protected");
        steps(&mut s, &[builder, walker], 300);
        s.fire_brick_input(brick, "onActivate", Some(walker));
        steps(&mut s, &[builder, walker], 2);
        assert!(!s.is_alive(walker), "lan {lan}: the kill brick did nothing");
        assert!(s.is_alive(builder));
    }
}

/// allGameScripts.cs:17136-17139 (and every input): on single-player and
/// LAN servers the MiniGame target is the activator's own game, and
/// `MiniGameSO::Reset` (22236) lets the game's owner reset it from any
/// brick. On internet servers the brick must share the game.
#[test]
fn a_lan_minigame_owner_resets_their_game_from_anyones_brick() {
    for lan in [true, false] {
        let mut s = session(lan);
        let builder = s.join("Builder".into(), Vec3::new(5.0, 0.05, 0.0), false).unwrap();
        let host = s.join("Host".into(), Vec3::new(-5.0, 0.05, 0.0), false).unwrap();
        s.command(
            host,
            1,
            Command::MiniGame(MiniGameRequest::Create {
                color: 0,
                settings: Settings::default(),
            }),
        )
        .unwrap();
        steps(&mut s, &[builder, host], 700); // past the 5 s reset cooldown
        let brick = evented_brick(
            &mut s,
            builder,
            1,
            [5.0, 0.3, -3.0],
            vec![row("onActivate", bri_events::Slot::MiniGame, "Reset", vec![])],
        );
        s.take_private_notices();
        s.fire_brick_input(brick, "onActivate", Some(host));
        steps(&mut s, &[builder, host], 2);
        let reset = s
            .take_private_notices()
            .iter()
            .any(|(_, n)| format!("{n:?}").contains("reset the mini-game"));
        assert_eq!(reset, lan, "lan {lan}");
    }
}

/// allGameScripts.cs:17660 `fxDTSBrick::spawnExplosion` (and spawnItem
/// 17514, spawnProjectile 17600) do nothing from a brick that is neither
/// drawn nor hit by rays.
#[test]
fn a_hidden_brick_spawns_no_explosion() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/weapons-pack-009/weapons.json");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("\n**** skipped: a_hidden_brick_spawns_no_explosion needs content/weapons-pack-009 ****\n");
        return;
    };
    let pack = bri_weapons::Pack::from_json(&bytes).unwrap();
    let rocket = pack
        .projectiles
        .keys()
        .find(|id| id.contains("rocket"))
        .expect("a rocket projectile")
        .clone();
    for hidden in [false, true] {
        let mut s = session(true);
        s.set_weapon_pack(pack.clone()).unwrap();
        let builder = s.join("Builder".into(), Vec3::new(5.0, 0.05, 0.0), false).unwrap();
        steps(&mut s, &[builder], 10);
        let mut rows = vec![];
        if hidden {
            rows.push(row("onActivate", bri_events::Slot::SelfBrick, "setRendering", vec![EventValue::Bool(false)]));
            rows.push(row("onActivate", bri_events::Slot::SelfBrick, "setRayCasting", vec![EventValue::Bool(false)]));
        }
        let mut blast = row(
            "onActivate",
            bri_events::Slot::SelfBrick,
            "spawnExplosion",
            vec![EventValue::Datablock(Some(rocket.clone())), EventValue::Float(1.0)],
        );
        blast.delay_ms = 100;
        rows.push(blast);
        let brick = evented_brick(&mut s, builder, 1, [5.0, 0.3, -3.0], rows);
        s.take_cues();
        s.fire_brick_input(brick, "onActivate", Some(builder));
        steps(&mut s, &[builder], 60);
        let exploded = s
            .take_cues()
            .iter()
            .any(|c| {
                matches!(
                    c.kind,
                    CueKind::Explosion { .. } | CueKind::WeaponEffect { .. } | CueKind::WeaponSound { .. }
                )
            });
        assert_eq!(exploded, !hidden, "hidden {hidden}");
    }
}

/// allGameScripts.cs:17868 `fxDTSBrick::radiusImpulse`: outside minigames an
/// internet server pushes only the player who set it off (`%searchObj.client
/// != %client` skips the rest, 17886); a LAN server pushes everyone in reach.
#[test]
fn a_radius_impulse_pushes_only_the_activator_on_internet_servers() {
    for lan in [true, false] {
        let mut s = session(lan);
        let builder = s.join("Builder".into(), Vec3::new(20.0, 0.05, 0.0), false).unwrap();
        let walker = s.join("Walker".into(), Vec3::new(1.0, 0.05, 0.0), false).unwrap();
        let bystander = s.join("Bystander".into(), Vec3::new(-1.0, 0.05, 0.0), false).unwrap();
        steps(&mut s, &[builder, walker, bystander], 30);
        let brick = evented_brick(
            &mut s,
            builder,
            1,
            [0.0, 0.3, -4.0],
            vec![row(
                "onActivate",
                bri_events::Slot::SelfBrick,
                "radiusImpulse",
                vec![EventValue::Int(10), EventValue::Int(0), EventValue::Int(2000)],
            )],
        );
        s.fire_brick_input(brick, "onActivate", Some(walker));
        steps(&mut s, &[builder, walker, bystander], 1);
        let up = |s: &Session, owner| {
            s.snapshot()
                .players
                .into_iter()
                .find(|p| p.owner == owner)
                .unwrap()
                .velocity[1]
        };
        assert!(up(&s, walker) > 1.0, "lan {lan}: the activator was not pushed");
        assert_eq!(up(&s, bystander) > 1.0, lan, "lan {lan}: the bystander");
    }
}

fn slash(s: &mut Session, owner: OwnerId, seq: u64, command: &str) {
    s.command(
        owner,
        seq,
        Command::Package(bri_sim::session::PackageCommand {
            package: String::new(),
            command: command.into(),
            args: vec![],
        }),
    )
    .unwrap();
}

fn chat_to(s: &mut Session, owner: OwnerId) -> Vec<String> {
    s.take_private_notices()
        .into_iter()
        .filter(|(to, _)| *to == owner)
        .filter_map(|(_, n)| match n {
            bri_sim::session::Notice::Chat(text) => Some(text),
            _ => None,
        })
        .collect()
}

/// allGameScripts.cs:4958 `serverCmdCancelEvents`: any player on an internet
/// server stops their own pending events, once every five seconds; on LAN
/// servers only administrators may.
#[test]
fn cancel_events_stops_a_players_own_pending_events() {
    for (lan, administrator) in [(false, false), (true, false), (true, true)] {
        let mut s = session(lan);
        let builder = s
            .join("Builder".into(), Vec3::new(5.0, 0.05, 0.0), administrator)
            .unwrap();
        steps(&mut s, &[builder], 10);
        let mut later = row(
            "onActivate",
            bri_events::Slot::SelfBrick,
            "setColor",
            vec![EventValue::Color(1)],
        );
        later.delay_ms = 1000;
        let brick = evented_brick(&mut s, builder, 1, [5.0, 0.3, -3.0], vec![later]);
        s.fire_brick_input(brick, "onActivate", Some(builder));
        steps(&mut s, &[builder], 2);
        s.take_private_notices();
        slash(&mut s, builder, 2, "cancelEvents");
        steps(&mut s, &[builder], 200);
        let cancelled = !lan || administrator;
        let color = s.simulation().state().bricks[&brick].color;
        assert_eq!(color == 0, cancelled, "lan {lan} admin {administrator}");
        let said = chat_to(&mut s, builder);
        assert_eq!(
            said.iter().any(|t| t.contains("Deleting all events")),
            cancelled,
            "{said:?}"
        );
        if cancelled {
            slash(&mut s, builder, 3, "cancelEvents");
            steps(&mut s, &[builder], 1);
            let said = chat_to(&mut s, builder);
            assert!(said.iter().any(|t| t.starts_with("You must wait")), "{said:?}");
        }
    }
}


/// allGameScripts.cs:17459 `fxDTSBrick::fakeKillBrick` clamps its time to
/// 0-300 s and schedules the respawn that far ahead: a time of 0 brings the
/// brick back at once instead of after a second.
#[test]
fn a_zero_second_fake_kill_comes_back_at_once() {
    for seconds in [0, 1] {
        let mut s = session(true);
        let builder = s.join("Builder".into(), Vec3::new(5.0, 0.05, 0.0), false).unwrap();
        steps(&mut s, &[builder], 10);
        let brick = evented_brick(
            &mut s,
            builder,
            1,
            [5.0, 0.3, -3.0],
            vec![row(
                "onActivate",
                bri_events::Slot::SelfBrick,
                "fakeKillBrick",
                vec![EventValue::Vector(Vec3::new(0.0, 0.0, 5.0)), EventValue::Int(seconds)],
            )],
        );
        s.fire_brick_input(brick, "onActivate", Some(builder));
        steps(&mut s, &[builder], 1);
        assert!(!s.simulation().state().bricks[&brick].visible, "{seconds} s: not killed");
        steps(&mut s, &[builder], 2);
        let back = s.simulation().state().bricks[&brick].visible;
        assert_eq!(back, seconds == 0, "{seconds} s");
        steps(&mut s, &[builder], 2 * bri_world::TICKS_PER_SECOND);
        assert!(s.simulation().state().bricks[&brick].visible, "{seconds} s: never came back");
    }
}
