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
        package: None,
    };
    Catalog {
        schema_version: 1,
        inputs: vec![
            input("onActivate"),
            input("onPlayerTouch"),
            InputDef {
                targets: [
                    ("Self", "fxDTSBrick"),
                    ("Bot", "Player"),
                    ("Driver", "Player"),
                    ("Client", "GameConnection"),
                    ("MiniGame", "MiniGame"),
                ]
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
                ..input("onBotTouch")
            },
        ],
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
            output(
                "fxDTSBrick",
                "spawnItem",
                vec![
                    Param::Vector { max_length: 200.0 },
                    Param::Datablock {
                        class_name: "ItemData".into(),
                    },
                ],
            ),
            output("Player", "Kill", vec![]),
            output("MiniGame", "Reset", vec![]),
        ],
        targets: vec![],
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
                reflection: None,
                link: None,
                glass: [0.0; 4],
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
        // The engine's `applyImpulse` divides by the player's mass (90):
        // 2000 at about 82% strength is about 18 units a second, not 1600.
        assert!(up(&s, walker) < 25.0, "lan {lan}: pushed {} without the mass", up(&s, walker));
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

/// A Blockhead Bot from a vehicle spawn brick, and a plate whose rows fire
/// on `onBotTouch`, loaded as `builder`'s build.
fn bot_and_plate(s: &mut Session, builder: OwnerId, rows: Vec<EventRow>) -> (OwnerId, u64) {
    s.set_tool_catalog(bri_sim::session::ToolCatalog {
        vehicles: ["bot.blockhead".to_string()].into(),
        vehicle_bricks: ["plate".to_string()].into(),
        ..Default::default()
    })
    .unwrap();
    let mut world = World::new("Bots".into(), "v20".into(), vec![[1.0; 4], [0.0; 4]]);
    let mut spawn = bri_world::Brick::new(bri_world::ContentRef::Resolved("plate".into()), [8.0, 0.3, 8.0], builder);
    spawn.vehicle = Some(bri_world::VehicleSpawn {
        vehicle: bri_world::ContentRef::Resolved("bot.blockhead".into()),
        recolor: false,
    });
    let mut plate = bri_world::Brick::new(bri_world::ContentRef::Resolved("plate".into()), [-8.0, 0.3, 8.0], builder);
    plate.events = rows;
    world.bricks.insert(1, spawn);
    world.bricks.insert(2, plate);
    world.next_brick_id = 3;
    s.command(
        builder,
        50,
        Command::LoadBuild {
            build: Box::new(bri_world::build::SavedBuild::new(world)),
            ownership: false,
        },
    )
    .unwrap();
    let players: Vec<OwnerId> = s.names().keys().copied().collect();
    steps(s, &players, 90);
    let bot = *s.names().keys().find(|o| s.is_bot(**o)).expect("the brick spawned a bot");
    let plate = *s
        .simulation()
        .state()
        .bricks
        .iter()
        .find(|(_, b)| !b.events.is_empty())
        .unwrap()
        .0;
    (bot, plate)
}

/// allGameScripts.cs:17157-17234 `fxDTSBrickData::onPlayerTouch` for a bot:
/// its rows run as the bot's spawn brick owner; with that owner gone, as
/// the first player on LAN, and not at all on an internet server.
#[test]
fn bot_touch_rows_run_as_the_spawn_brick_owner() {
    for lan in [true, false] {
        let mut s = session(lan);
        s.set_vehicle_pack(bri_vehicles::Pack {
            schema_version: bri_vehicles::schema::SCHEMA_VERSION,
            definitions: vec![],
            assets: vec![],
            evidence: vec![],
            unresolved: vec![],
            animation_aliases: Default::default(),
        }, bri_sim::bot_kind::BotPack::from_json(include_bytes!("../../../packages/blockhead_bot/assets/bots.json")).unwrap().bots)
        .unwrap();
        let builder = s.join("Builder".into(), Vec3::new(5.0, 0.05, 0.0), true).unwrap();
        let other = s.join("Other".into(), Vec3::new(-5.0, 0.05, 0.0), false).unwrap();
        let (bot, plate) = bot_and_plate(
            &mut s,
            builder,
            vec![row("onBotTouch", bri_events::Slot::SelfBrick, "setColor", vec![EventValue::Color(1)])],
        );
        s.fire_brick_input(plate, "onBotTouch", Some(bot));
        steps(&mut s, &[builder, other], 2);
        let notes = s.take_event_diagnostics();
        assert_eq!(s.simulation().state().bricks[&plate].color, 1, "lan {lan}: as the owner {notes:?}");
        // With the owner gone: LAN falls back to the first player.
        s.edit_brick(builder, plate, Edit::Color(0)).unwrap();
        s.disconnect(builder).unwrap();
        steps(&mut s, &[other], 2);
        s.fire_brick_input(plate, "onBotTouch", Some(bot));
        steps(&mut s, &[other], 2);
        assert_eq!(s.simulation().state().bricks[&plate].color == 1, lan, "lan {lan}: owner gone");
    }
}

/// allGameScripts.cs:17868 `fxDTSBrick::radiusImpulse` searches items too
/// (`$TypeMasks::ItemObjectType`): on a LAN server a dropped item in reach
/// is thrown up; on an internet server, outside minigames, it is not (an
/// item has no client).
#[test]
fn a_radius_impulse_throws_items_on_lan_servers() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/weapons-pack-009/weapons.json");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("\n**** skipped: a_radius_impulse_throws_items_on_lan_servers needs content/weapons-pack-009 ****\n");
        return;
    };
    let pack = bri_weapons::Pack::from_json(&bytes).unwrap();
    let item = pack
        .items
        .keys()
        .find(|id| id.contains("hammer"))
        .expect("a hammer item")
        .clone();
    for lan in [true, false] {
        let mut s = session(lan);
        s.set_weapon_pack(pack.clone()).unwrap();
        // The item is on the server's item list, as `spawnItem` requires.
        s.set_tool_catalog(bri_sim::session::ToolCatalog {
            items: [item.clone()].into(),
            ..Default::default()
        })
        .unwrap();
        let builder = s.join("Builder".into(), Vec3::new(20.0, 0.05, 0.0), false).unwrap();
        steps(&mut s, &[builder], 10);
        let brick = evented_brick(
            &mut s,
            builder,
            1,
            [0.0, 0.3, -2.0],
            vec![row(
                "onActivate",
                bri_events::Slot::SelfBrick,
                "spawnItem",
                vec![EventValue::Vector(Vec3::ZERO), EventValue::Datablock(Some(item.clone()))],
            )],
        );
        s.fire_brick_input(brick, "onActivate", Some(builder));
        steps(&mut s, &[builder], 240); // the item settles
        let height = |s: &Session| s.weapon_view().drops.iter().map(|d| d.position.y).fold(f32::MIN, f32::max);
        let resting = height(&s);
        s.edit_brick(
            builder,
            brick,
            Edit::Events(vec![row(
                "onActivate",
                bri_events::Slot::SelfBrick,
                "radiusImpulse",
                vec![EventValue::Int(10), EventValue::Int(0), EventValue::Int(20)],
            )]),
        )
        .unwrap();
        s.fire_brick_input(brick, "onActivate", Some(builder));
        steps(&mut s, &[builder], 20);
        assert_eq!(height(&s) > resting + 0.5, lan, "lan {lan}: item at {} from {resting}", height(&s));
    }
}

/// mainServer.cs:1102-1116 `serverCmdMessageSent`: the same line (any case)
/// within 15 s of the sender's last one warns them "Do not repeat
/// yourself." and fills their spam allowance; the line still goes out.
#[test]
fn repeating_a_chat_line_within_15_seconds_is_warned() {
    let mut s = session(false);
    let talker = s.join("Talker".into(), Vec3::new(5.0, 0.05, 0.0), false).unwrap();
    steps(&mut s, &[talker], 10);
    let said = |s: &Session| s.chat().iter().filter(|l| l.owner == talker).count();
    s.command(talker, 1, Command::Chat("hello".into())).unwrap();
    steps(&mut s, &[talker], 130);
    s.take_private_notices();
    s.command(talker, 2, Command::Chat("  HELLO ".into())).unwrap();
    assert_eq!(said(&s), 2, "the repeat still goes out");
    assert!(chat_to(&mut s, talker).iter().any(|t| t.contains("Do not repeat yourself.")));
    // The allowance is spent: the next line this second is held.
    assert!(s.command(talker, 3, Command::Chat("other".into())).is_err());
    // A different line, or the same one 15 s later, is fine.
    steps(&mut s, &[talker], 130);
    s.command(talker, 4, Command::Chat("other".into())).unwrap();
    steps(&mut s, &[talker], 15 * 120 + 10);
    s.command(talker, 5, Command::Chat("other".into())).unwrap();
    assert!(!chat_to(&mut s, talker).iter().any(|t| t.contains("repeat")));
}

/// allGameScripts.cs:4733 `serverCmdTripOut`: an administrator sets every
/// brick to the Rainbow colour effect (6) and Undulo shape effect (1);
/// anyone else is ignored, silently.
#[test]
fn trip_out_is_an_administrators_rainbow_undulo() {
    for administrator in [false, true] {
        let mut s = session(true);
        let player = s
            .join("Player".into(), Vec3::new(5.0, 0.05, 0.0), administrator)
            .unwrap();
        steps(&mut s, &[player], 10);
        let brick = evented_brick(&mut s, player, 1, [5.0, 0.3, -3.0], vec![]);
        s.take_private_notices();
        slash(&mut s, player, 2, "tripOut");
        steps(&mut s, &[player], 1);
        let b = &s.simulation().state().bricks[&brick];
        assert_eq!(
            (b.color_effect, b.shape_effect),
            if administrator { (6, 1) } else { (0, 0) },
            "admin {administrator}"
        );
        let said = chat_to(&mut s, player);
        assert!(said.is_empty(), "{said:?}");
    }
}
