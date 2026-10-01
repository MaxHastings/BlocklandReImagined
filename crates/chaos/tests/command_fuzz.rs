//! Every command a client can send, damaged: one well-formed example of each
//! `Command` variant, with values swapped for extremes, keys dropped and
//! lists cut or repeated, sent by a guest with default trust and by the
//! host. A damaged command may fail to decode or be refused; it must never
//! panic the host, fail its step, or put NaN into what it replicates.
//!
//! A new `Command` variant does not compile here until it has an example
//! (`variant`), so the fuzzer never silently skips a command.
use bri_admin::{Action, BanDuration, ConnectionId, Request, Role};
use bri_chaos::{
    fixture,
    local::check_replicated,
    mutate::{apply, change},
};
use bri_events::{Slot, testing};
use bri_sim::session::{
    ActionAim, BrickHand, BuildGesture, CameraView, Command, GhostBrick, InspectMode,
    MiniGameRequest, PackageArg, PackageCommand, Session, ToolAction, TrustEntry, WrenchProperties,
};
use bri_world::{EventRow, EventTarget, EventValue, World, build::SavedBuild};
use proptest::prelude::*;
use serde_json::Value;
use std::collections::BTreeMap;

/// The name of `command`'s variant. Exhaustive on purpose: see the module
/// comment.
fn variant(command: &Command) -> &'static str {
    match command {
        Command::Admin(_) => "admin",
        Command::Plant { .. } => "plant",
        Command::Tool(_) => "tool",
        Command::PlaceBlueprint { .. } => "place_blueprint",
        Command::UseSprayCan { .. } => "use_spray_can",
        Command::UseFxCan { .. } => "use_fx_can",
        Command::EquipTool { .. } => "equip_tool",
        Command::DropTool { .. } => "drop_tool",
        Command::WeaponTrigger { .. } => "weapon_trigger",
        Command::Avatar(_) => "avatar",
        Command::SaveBuild { .. } => "save_build",
        Command::LoadBuild { .. } => "load_build",
        Command::Activate => "activate",
        Command::Chat(_) => "chat",
        Command::Suicide => "suicide",
        Command::Respawn => "respawn",
        Command::ToggleLight => "toggle_light",
        Command::CancelBrick => "cancel_brick",
        Command::Emote(_) => "emote",
        Command::MiniGame(_) => "mini_game",
        Command::SwitchSeat(_) => "switch_seat",
        Command::TeamChat(_) => "team_chat",
        Command::ClearCheckpoint => "clear_checkpoint",
        Command::TreasureStatus => "treasure_status",
        Command::TrustInvite { .. } => "trust_invite",
        Command::AcceptTrust { .. } => "accept_trust",
        Command::RejectTrust { .. } => "reject_trust",
        Command::IgnoreTrust { .. } => "ignore_trust",
        Command::DemoteTrust { .. } => "demote_trust",
        Command::UnIgnore { .. } => "un_ignore",
        Command::TrustList(_) => "trust_list",
        Command::DropPlayerAtCamera(_) => "drop_player_at_camera",
        Command::ControlPlayer => "control_player",
        Command::BrickHand(_) => "brick_hand",
        Command::GhostBrick(_) => "ghost_brick",
        Command::Wand => "wand",
        Command::Talking(_) => "talking",
        Command::SteeringPrefs { .. } => "steering_prefs",
        Command::BuildGesture(_) => "build_gesture",
        Command::Package(_) => "package",
        Command::SetName(_) => "set_name",
        Command::SetClan(_) => "set_clan",
    }
}

const VARIANTS: usize = 42;

/// Owners in the fuzzed session: the host (an administrator) and a guest.
const HOST: u64 = 1;
const GUEST: u64 = 2;

fn row(output: &str, params: Vec<EventValue>, delay_ms: u32) -> EventRow {
    EventRow {
        preserved: None,
        enabled: true,
        input: "onActivate".into(),
        delay_ms,
        target: EventTarget::Slot(Slot::SelfBrick),
        output: output.into(),
        params,
    }
}

/// One ordinary example of every command.
fn examples() -> Vec<Command> {
    let camera = CameraView {
        eye: [0.0, 3.0, 4.0],
        yaw: 0.5,
        pitch: -0.2,
    };
    let admin = |action| Command::Admin(Request::new(action));
    vec![
        admin(Action::Kick {
            target: ConnectionId(GUEST),
        }),
        admin(Action::Ban {
            target: ConnectionId(GUEST),
            duration: BanDuration::Minutes(5),
            reason: "fuzz".into(),
        }),
        admin(Action::TimeScale { scale: 0.5 }),
        admin(Action::HostSetRole {
            target: ConnectionId(GUEST),
            role: Role::Admin,
        }),
        admin(Action::ClearBrickGroup { group: GUEST }),
        Command::Plant {
            definition: fixture::BRICK.into(),
            position: [1.0, 0.3, 1.0],
            quarter_turns: 1,
            color: 2,
        },
        Command::Tool(ToolAction::Inspect {
            mode: InspectMode::Events,
        }),
        Command::Tool(ToolAction::SetPrint {
            brick: 1,
            print: Some("Letters/A".into()),
        }),
        Command::Tool(ToolAction::SetWrench {
            brick: 1,
            properties: WrenchProperties {
                name: Some("door".into()),
                raycast: true,
                colliding: true,
                visible: true,
                ..Default::default()
            },
        }),
        Command::Tool(ToolAction::SetEvents {
            brick: 1,
            events: vec![
                row("setColor", vec![EventValue::Color(1)], 0),
                row(
                    "fakeKillBrick",
                    vec![
                        EventValue::Vector([0.0, 0.0, 5.0].into()),
                        EventValue::Int(3),
                    ],
                    100,
                ),
                row("fireRelay", vec![], 0),
            ],
        }),
        Command::Tool(ToolAction::UndoBrick),
        Command::Tool(ToolAction::RespawnVehicle { brick: 1 }),
        Command::PlaceBlueprint {
            position: [2.0, 0.0, 2.0],
            quarter_turns: 3,
            mirrored: true,
        },
        Command::UseSprayCan { color: 1 },
        Command::UseFxCan { fx: 3 },
        Command::EquipTool { slot: Some(3) },
        Command::DropTool { slot: 3 },
        Command::WeaponTrigger { down: true },
        Command::Avatar(bri_content::avatar::Appearance {
            parts: BTreeMap::from([("hat".into(), "helmet".into())]),
            colors: BTreeMap::from([("hat".into(), [0.5, 0.2, 0.1, 1.0])]),
            face: "smiley".into(),
            decal: "AAA-None".into(),
        }),
        Command::SaveBuild {
            events: true,
            ownership: true,
        },
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(World::new(
                "Fuzz".into(),
                "chaos/map".into(),
                vec![[1.0; 4], [0.5, 0.5, 0.5, 1.0]],
            ))),
            ownership: true,
        },
        Command::Activate,
        Command::Chat("hello /brickcount".into()),
        Command::Suicide,
        Command::Respawn,
        Command::ToggleLight,
        Command::CancelBrick,
        Command::Emote("love".into()),
        Command::MiniGame(MiniGameRequest::Create {
            color: 2,
            settings: bri_minigames::Settings::default(),
        }),
        Command::MiniGame(MiniGameRequest::Reject {
            game: 1,
            ignore_owner: true,
        }),
        Command::SwitchSeat(1),
        Command::TeamChat("team".into()),
        Command::ClearCheckpoint,
        Command::TreasureStatus,
        Command::TrustInvite {
            target: HOST,
            level: 2,
        },
        Command::AcceptTrust { from: HOST },
        Command::RejectTrust { from: HOST },
        Command::IgnoreTrust { from: HOST },
        Command::DemoteTrust {
            target: HOST,
            level: 1,
        },
        Command::UnIgnore { target: HOST },
        Command::TrustList(vec![TrustEntry {
            principal: [7; 32],
            level: 2,
        }]),
        Command::DropPlayerAtCamera(Some(camera)),
        Command::ControlPlayer,
        Command::BrickHand(BrickHand {
            stocked: true,
            equipped: true,
            ghost: true,
        }),
        Command::GhostBrick(Some(GhostBrick {
            definition: fixture::PLATE.into(),
            position: [0.0, 1.0, 0.0],
            quarter_turns: 2,
            color: 1,
            print: None,
        })),
        Command::Wand,
        Command::Talking(true),
        Command::SteeringPrefs {
            strafe: false,
            auto_return: true,
        },
        Command::BuildGesture(BuildGesture::RotateCw),
        Command::Package(PackageCommand {
            package: String::new(),
            command: "spawn".into(),
            args: vec![PackageArg::Float(1e19), PackageArg::String("x".into())],
        }),
        Command::SetName("Blockhead \u{202e}".into()),
        Command::SetClan(bri_sim::session::Clan {
            prefix: "[\u{e003}CLAN\n".repeat(40),
            suffix: String::new(),
        }),
    ]
}

/// A host and a guest (default trust) in the synthetic world, each with a
/// few bricks and the testing event catalog.
fn session() -> Session {
    let fixture = fixture::synthetic().unwrap();
    let spawn = fixture.spawn_points[0];
    let mut session = fixture.session;
    session
        .set_event_catalog(testing::catalog(), Vec::new())
        .unwrap();
    assert_eq!(session.join("Host".into(), spawn, true).unwrap(), HOST);
    assert_eq!(session.join("Guest".into(), spawn, false).unwrap(), GUEST);
    let mut sequence = 0;
    for (owner, x) in [(HOST, -2.75), (GUEST, 3.25), (HOST, -4.75)] {
        sequence += 1;
        session
            .command(
                owner,
                sequence,
                Command::Plant {
                    definition: fixture::PLATE.into(),
                    position: [x, 0.1, -1.75],
                    quarter_turns: 0,
                    color: 0,
                },
            )
            .unwrap();
    }
    session
}

/// Send `commands` from each owner in turn, stepping between them.
fn run(commands: &[Command]) -> Result<(), TestCaseError> {
    let mut session = session();
    let aim = Some(ActionAim {
        yaw: 0.3,
        pitch: -0.9,
    });
    let mut sequence = 100;
    for command in commands {
        for owner in [GUEST, HOST] {
            sequence += 1;
            let aimed = if matches!(command, Command::Admin(_)) {
                None
            } else {
                aim
            };
            let _ = session.command_with_aim(owner, sequence, command.clone(), aimed);
            session
                .step()
                .map_err(|e| TestCaseError::fail(format!("step failed: {e:#}")))?;
        }
    }
    // Let delayed events, projectiles and respawns play out.
    for _ in 0..40 {
        session
            .step()
            .map_err(|e| TestCaseError::fail(format!("step failed: {e:#}")))?;
    }
    check_replicated(&mut session).map_err(|e| TestCaseError::fail(format!("{e:#}")))?;
    Ok(())
}

#[test]
fn every_command_variant_has_an_example_that_round_trips() {
    let examples = examples();
    let names: std::collections::BTreeSet<_> = examples.iter().map(variant).collect();
    assert_eq!(names.len(), VARIANTS, "every variant has an example");
    for command in &examples {
        let json = serde_json::to_value(command).unwrap();
        let back: Command = serde_json::from_value(json).unwrap();
        assert_eq!(variant(&back), variant(command));
    }
}

#[test]
fn every_undamaged_example_is_handled_cleanly() {
    run(&examples()).unwrap();
}

fn damaged() -> impl Strategy<Value = Vec<Command>> {
    let count = examples().len();
    proptest::collection::vec((0..count, proptest::collection::vec(change(), 1..5)), 1..6).prop_map(
        |picks| {
            let examples = examples();
            picks
                .into_iter()
                .filter_map(|(index, changes)| {
                    let mut value: Value = serde_json::to_value(&examples[index]).unwrap();
                    for change in &changes {
                        apply(&mut value, change);
                    }
                    serde_json::from_value(value).ok()
                })
                .collect()
        },
    )
}

proptest! {
    #![proptest_config(bri_chaos::proptest_config(256, 0xc0d))]

    #[test]
    fn damaged_commands_are_refused_or_handled_cleanly(commands in damaged()) {
        run(&commands)?;
    }
}
