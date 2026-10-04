//! Hardening: the authoritative session must reject or ignore requests a
//! normal client never sends, and leave shared state unchanged afterwards.
//!
//! Every test drives a synthetic session (no original game assets) through
//! the same `Session::command*` / `Session::movement` entry points the QUIC
//! host dispatches to. Each test body lists its exact request sequence so a
//! failure can be replayed request by request. The findings these tests
//! first confirmed are recorded in docs/stress-lab/weakness-ledger.md.
use bri_admin::{
    Action, BanDuration, BanId, ConnectionId, PasswordSlot, Principal, Request, Role, Secret,
    ServerSettings,
};
use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_minigames::Settings;
use bri_sim::{
    definitions::{Definition, Definitions},
    player::MoveInput,
    session::{
        ActionAim, AdminData, BuildGesture, Command, InspectMode, MAX_TRUST_LIST, MiniGameRequest,
        Notice, Reply, Session, ToolAction, TrustEntry, TrustLevel, WrenchProperties,
    },
    simulation::Simulation,
};
use bri_world::{Brick, BrickId, EventRow, EventTarget, EventValue, OwnerId, World};
use glam::Vec3;
use rapier3d::prelude::*;
use std::collections::BTreeMap;

// ---------------------------------------------------------------- fixtures

fn definitions() -> Definitions {
    let mesh = Mesh {
        schema_version: 1,
        id: "plate".into(),
        footprint_studs: [2, 1],
        height_plates: 1,
        attachment_rows: vec!["bb".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![],
    };
    let collision = CollisionBody {
        id: "plate".into(),
        parts: vec![Part::Box {
            center: [0.0; 3],
            size: [1.0, 0.2, 0.5],
        }],
    };
    let shape = bri_physics::content::collider(&collision)
        .unwrap()
        .build()
        .shared_shape()
        .clone();
    Definitions {
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
                bot: None,
            },
        )]
        .into(),
    }
}

/// A synthetic session: one plate definition, a flat ground, a two-colour
/// palette, the testing event catalog and a spawn point.
fn plain() -> Session {
    plain_with(World::new(
        "Hardening".into(),
        "hardening".into(),
        vec![[1.0; 4], [0.0; 4]],
    ))
}

/// `plain` around a world that already has bricks.
fn plain_with(world: World) -> Session {
    let mut s = Session::new(
        Simulation::new(
            world,
            definitions(),
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    s.set_event_catalog(bri_events::testing::catalog(), Vec::new())
        .unwrap();
    s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)]).unwrap();
    s
}

fn tooled() -> Session {
    let mut s = plain();
    s.set_weapon_pack(bri_weapons::testing::pack()).unwrap();
    s
}

const HAMMER: usize = 0;
const WRENCH: usize = 1;
/// A's spawn, B's spawn, the brick B can reach from its spawn, and a second
/// brick only A reaches (a positive control for the swing geometry).
const A_SPAWN: Vec3 = Vec3::new(0.0, 0.05, 0.0);
const B_SPAWN: Vec3 = Vec3::new(4.0, 0.05, 0.0);
const SHARED_BRICK: [f32; 3] = [2.5, 0.1, -2.25];
const A_ONLY_BRICK: [f32; 3] = [-1.5, 0.1, -2.25];

/// A session plus the per-connection counters a real client keeps. Every
/// request goes through `command_with_aim`, exactly as the host dispatches
/// it, and the world advances two ticks per request so the 60-per-second
/// action limiter never masks the check under test.
struct Game {
    s: Session,
    seq: BTreeMap<OwnerId, u64>,
    moves: BTreeMap<OwnerId, u64>,
    log: Vec<(OwnerId, Notice)>,
}
impl Game {
    fn new(s: Session) -> Self {
        Self {
            s,
            seq: BTreeMap::new(),
            moves: BTreeMap::new(),
            log: Vec::new(),
        }
    }
    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.s.step().unwrap();
        }
        self.log.extend(self.s.take_private_notices());
    }
    /// Start a fresh 120-tick action/chat window.
    fn fresh_window(&mut self) {
        self.steps(121);
    }
    fn cmd(&mut self, owner: OwnerId, command: Command) -> anyhow::Result<Reply> {
        self.cmd_aim(owner, command, None)
    }
    fn cmd_aim(
        &mut self,
        owner: OwnerId,
        command: Command,
        aim: Option<ActionAim>,
    ) -> anyhow::Result<Reply> {
        let n = self.seq.entry(owner).or_insert(0);
        *n += 1;
        let n = *n;
        let reply = self.s.command_with_aim(owner, n, command, aim);
        self.log.extend(self.s.take_private_notices());
        self.steps(2);
        reply
    }
    fn body(&self, owner: OwnerId) -> bri_sim::player::PlayerState {
        self.s
            .snapshot()
            .players
            .into_iter()
            .find(|p| p.owner == owner)
            .unwrap()
    }
    fn next_move(&mut self, owner: OwnerId) -> u64 {
        let n = self.moves.entry(owner).or_insert(0);
        *n += 1;
        *n
    }
    /// Turn the body to face `target` (a queued movement input) and return
    /// the matching action aim.
    fn look_at(&mut self, owner: OwnerId, target: Vec3) -> ActionAim {
        let feet = Vec3::from(self.body(owner).feet);
        let d = target - (feet + Vec3::Y * 2.4);
        let aim = ActionAim {
            yaw: d.x.atan2(-d.z),
            pitch: d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()),
        };
        let sequence = self.next_move(owner);
        self.s
            .movement(
                owner,
                sequence,
                MoveInput {
                    yaw: aim.yaw,
                    pitch: aim.pitch,
                    ..Default::default()
                },
            )
            .unwrap();
        self.steps(1);
        aim
    }
    /// Requests: EquipTool{slot} (or Wand), a look input, WeaponTrigger{down}
    /// with aim at `target`, 30 ticks holding the look, WeaponTrigger{up}.
    /// Returns the dialog the host opened for `owner`, if any.
    fn swing(
        &mut self,
        owner: OwnerId,
        slot: Option<usize>,
        target: [f32; 3],
    ) -> Option<(BrickId, Box<Brick>, InspectMode)> {
        match slot {
            Some(slot) => self
                .cmd(owner, Command::EquipTool { slot: Some(slot) })
                .unwrap(),
            None => self.cmd(owner, Command::Wand).unwrap(),
        };
        let target = Vec3::from(target);
        let aim = self.look_at(owner, target);
        let start = self.log.len();
        self.cmd_aim(owner, Command::WeaponTrigger { down: true }, Some(aim))
            .unwrap();
        for _ in 0..30 {
            self.look_at(owner, target);
        }
        self.cmd(owner, Command::WeaponTrigger { down: false })
            .unwrap();
        self.log[start..].iter().rev().find_map(|(to, n)| match n {
            Notice::Inspected {
                brick_id,
                brick,
                mode,
            } if *to == owner => Some((*brick_id, brick.clone(), *mode)),
            _ => None,
        })
    }
    fn plant(&mut self, owner: OwnerId, position: [f32; 3]) -> BrickId {
        let Reply::Planted(id) = self
            .cmd(
                owner,
                Command::Plant {
                    definition: "plate".into(),
                    position,
                    quarter_turns: 0,
                    color: 0,
                },
            )
            .unwrap()
        else {
            panic!("expected a planted brick")
        };
        id
    }
    fn bricks(&self) -> BTreeMap<BrickId, Brick> {
        self.s
            .simulation()
            .state()
            .bricks
            .clone()
            .into_iter()
            .collect()
    }
    fn center_prints(&self, owner: OwnerId) -> Vec<String> {
        self.log
            .iter()
            .filter_map(|(to, n)| match n {
                Notice::Center { text, .. } if *to == owner => Some(text.clone()),
                _ => None,
            })
            .collect()
    }
    /// The trust level `viewer` last saw for `other` in its player list.
    fn trust_seen(&self, viewer: OwnerId, other: OwnerId) -> Option<TrustLevel> {
        self.log.iter().rev().find_map(|(to, n)| match n {
            Notice::PlayerTrust(rows) if *to == viewer => rows.get(&other).map(|r| r.level),
            _ => None,
        })
    }
    fn connection(&self, viewer: OwnerId, name: &str) -> ConnectionId {
        ConnectionId(
            self.s
                .admin_state(viewer)
                .unwrap()
                .players
                .iter()
                .find(|p| p.name == name)
                .unwrap()
                .connection,
        )
    }
}

fn admin(action: Action) -> Command {
    Command::Admin(Request::new(action))
}

fn color_row(color: u8) -> EventRow {
    EventRow {
        conditions: vec![],
        preserved: None,
        enabled: true,
        input: "onActivate".into(),
        delay_ms: 0,
        target: EventTarget::Slot(bri_events::Slot::SelfBrick),
        output: "setColor".into(),
        params: vec![EventValue::Color(color)],
    }
}

fn properties(name: Option<&str>, visible: bool) -> WrenchProperties {
    WrenchProperties {
        rule_region: None,
        name: name.map(Into::into),
        light: None,
        emitter: None,
        emitter_direction: 0,
        item_spawn: Default::default(),
        sound: None,
        vehicle: None,
        recolor_vehicle: false,
        raycast: true,
        colliding: true,
        visible,
        vehicle_team: None,
    }
}

fn verified(s: &mut Session, name: &str, spawn: Vec3, key: u8) -> OwnerId {
    s.join_verified(name.into(), spawn, false, Some(Principal([key; 32])))
        .unwrap()
}

// ------------------------------------------------ ownership and trust checks

/// Requests: A Plant(SHARED_BRICK), A Plant(A_ONLY_BRICK); B EquipTool{0},
/// B WeaponTrigger at SHARED_BRICK (hammer); B Wand, B WeaponTrigger at
/// SHARED_BRICK (wand); control: A EquipTool{0}, A WeaponTrigger at
/// A_ONLY_BRICK.
#[test]
fn untrusted_hammer_and_wand_cannot_destroy_another_players_brick() {
    let mut g = Game::new(tooled());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    let b = g.s.join("Bob".into(), B_SPAWN, false).unwrap();
    g.steps(60);
    let shared = g.plant(a, SHARED_BRICK);
    let control = g.plant(a, A_ONLY_BRICK);
    let before = g.bricks();

    g.swing(b, Some(HAMMER), SHARED_BRICK);
    assert_eq!(g.bricks(), before, "untrusted hammer changed the world");
    g.swing(b, None, SHARED_BRICK);
    assert_eq!(g.bricks(), before, "untrusted wand changed the world");
    let refusals = g.center_prints(b);
    assert!(
        refusals
            .iter()
            .filter(|t| t.contains("does not trust you enough"))
            .count()
            >= 2,
        "both swings must reach the trust gate: {refusals:?}"
    );
    // Positive control: the same swing geometry does break the owner's brick.
    g.swing(a, Some(HAMMER), A_ONLY_BRICK);
    assert!(!g.bricks().contains_key(&control));
    assert!(g.bricks().contains_key(&shared));
}

/// Requests: A Plant(SHARED_BRICK); B EquipTool{1} + WeaponTrigger at it
/// (wrench); then B sends Tool(SetWrench / SetEvents / RespawnVehicle /
/// Inspect{Events} / SetPrint) naming A's brick; B EquipTool{0} then
/// Tool(SetEvents) without the wrench; B Tool(UndoBrick).
#[test]
fn wrench_dialog_commands_need_a_trusted_hit_first() {
    let mut g = Game::new(tooled());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    let b = g.s.join("Bob".into(), B_SPAWN, false).unwrap();
    g.steps(60);
    let id = g.plant(a, SHARED_BRICK);
    let before = g.bricks();

    assert!(
        g.swing(b, Some(WRENCH), SHARED_BRICK).is_none(),
        "untrusted wrench opened a dialog"
    );
    for action in [
        ToolAction::SetWrench {
            brick: id,
            properties: properties(Some("stolen"), false),
        },
        ToolAction::SetEvents {
            brick: id,
            events: vec![color_row(1)],
        },
        ToolAction::RespawnVehicle { brick: id },
        ToolAction::Inspect {
            mode: InspectMode::Events,
        },
        ToolAction::SetPrint {
            brick: id,
            print: Some("v20.print.letters.a".into()),
        },
    ] {
        assert!(
            g.cmd(b, Command::Tool(action.clone())).is_err(),
            "{action:?} was accepted"
        );
    }
    g.cmd(b, Command::EquipTool { slot: Some(HAMMER) }).unwrap();
    assert!(
        g.cmd(
            b,
            Command::Tool(ToolAction::SetEvents {
                brick: id,
                events: vec![color_row(1)],
            })
        )
        .is_err()
    );
    // Undo only pops the sender's own stack.
    assert_eq!(
        g.cmd(b, Command::Tool(ToolAction::UndoBrick)).unwrap(),
        Reply::Undone(None)
    );
    assert_eq!(g.bricks(), before);
}

/// Requests: A TrustInvite{B,2}; B AcceptTrust{A}; B wrench hit on A's brick
/// (dialog opens); A DemoteTrust{B,0}; B Tool(SetWrench) and B
/// Tool(Inspect{Events}) + Tool(SetEvents) on the still-open dialog.
#[test]
fn trust_revoked_after_a_wrench_hit_blocks_the_open_dialog() {
    let mut g = Game::new(tooled());
    let a = verified(&mut g.s, "Ann", A_SPAWN, 1);
    let b = verified(&mut g.s, "Bob", B_SPAWN, 2);
    g.steps(60);
    let id = g.plant(a, SHARED_BRICK);
    g.cmd(
        a,
        Command::TrustInvite {
            target: b,
            level: 2,
        },
    )
    .unwrap();
    g.cmd(b, Command::AcceptTrust { from: a }).unwrap();
    assert_eq!(g.trust_seen(b, a), Some(TrustLevel::Full));
    let (hit, _, mode) = g
        .swing(b, Some(WRENCH), SHARED_BRICK)
        .expect("trusted wrench hit");
    assert_eq!((hit, mode), (id, InspectMode::Wrench));

    g.cmd(
        a,
        Command::DemoteTrust {
            target: b,
            level: 0,
        },
    )
    .unwrap();
    assert_eq!(g.trust_seen(b, a), Some(TrustLevel::None));
    let before = g.bricks();
    assert!(
        g.cmd(
            b,
            Command::Tool(ToolAction::SetWrench {
                brick: id,
                properties: properties(Some("after revoke"), true),
            })
        )
        .is_err()
    );
    assert!(
        g.cmd(
            b,
            Command::Tool(ToolAction::Inspect {
                mode: InspectMode::Events
            })
        )
        .is_err()
    );
    assert!(
        g.cmd(
            b,
            Command::Tool(ToolAction::SetEvents {
                brick: id,
                events: vec![color_row(1)],
            })
        )
        .is_err()
    );
    assert_eq!(g.bricks(), before);
}

/// Requests: A TrustInvite{B,1}; B AcceptTrust{A}; B wrench hit;
/// B Tool(SetWrench visible=false); B Tool(Inspect{Events});
/// B Tool(SetEvents); B Tool(SetWrench name only) (allowed control);
/// B hammer swing at the brick.
#[test]
fn build_trust_cannot_make_full_trust_edits() {
    let mut g = Game::new(tooled());
    let a = verified(&mut g.s, "Ann", A_SPAWN, 1);
    let b = verified(&mut g.s, "Bob", B_SPAWN, 2);
    g.steps(60);
    let id = g.plant(a, SHARED_BRICK);
    g.cmd(
        a,
        Command::TrustInvite {
            target: b,
            level: 1,
        },
    )
    .unwrap();
    g.cmd(b, Command::AcceptTrust { from: a }).unwrap();
    assert_eq!(g.trust_seen(b, a), Some(TrustLevel::Build));
    g.swing(b, Some(WRENCH), SHARED_BRICK)
        .expect("build trust may wrench");
    let before = g.bricks();

    assert!(
        g.cmd(
            b,
            Command::Tool(ToolAction::SetWrench {
                brick: id,
                properties: properties(None, false),
            })
        )
        .is_err(),
        "build trust hid a brick"
    );
    g.cmd(
        b,
        Command::Tool(ToolAction::Inspect {
            mode: InspectMode::Events,
        }),
    )
    .unwrap();
    assert!(
        g.cmd(
            b,
            Command::Tool(ToolAction::SetEvents {
                brick: id,
                events: vec![color_row(1)],
            })
        )
        .is_err(),
        "build trust edited events"
    );
    assert_eq!(g.bricks(), before);
    // Positive control: build trust may rename.
    g.cmd(
        b,
        Command::Tool(ToolAction::SetWrench {
            brick: id,
            properties: properties(Some("door"), true),
        }),
    )
    .unwrap();
    assert_eq!(g.bricks()[&id].name.as_deref(), Some("door"));
    let renamed = g.bricks();
    g.swing(b, Some(HAMMER), SHARED_BRICK);
    assert_eq!(g.bricks(), renamed, "build trust hammered a brick");
}

/// Requests: A wrench hit on its own brick, then Tool(SetWrench) with an
/// unknown light, emitter, music, vehicle and item, a control-character
/// name and an over-long name; then a valid rename (control).
#[test]
fn wrench_asset_assignments_must_come_from_the_host_catalog() {
    let mut g = Game::new(tooled());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    g.steps(60);
    let id = g.plant(a, A_ONLY_BRICK);
    g.swing(a, Some(WRENCH), A_ONLY_BRICK)
        .expect("own wrench hit");
    let before = g.bricks();
    let base = properties(None, true);
    let hostile = [
        WrenchProperties {
            light: Some("../../../etc/passwd".into()),
            ..base.clone()
        },
        WrenchProperties {
            emitter: Some("C:\\Windows\\system32".into()),
            ..base.clone()
        },
        WrenchProperties {
            sound: Some("v20.music.nonexistent".into()),
            ..base.clone()
        },
        WrenchProperties {
            vehicle: Some("v20.vehicle.jeepvehicle".into()),
            ..base.clone()
        },
        WrenchProperties {
            item_spawn: bri_world::ItemSpawn {
                item: Some(bri_world::ContentRef::Resolved("v20.weapon.nuke".into())),
                ..Default::default()
            },
            ..base.clone()
        },
        WrenchProperties {
            item_spawn: bri_world::ItemSpawn {
                respawn_ms: 0,
                ..Default::default()
            },
            ..base.clone()
        },
        WrenchProperties {
            emitter_direction: 200,
            ..base.clone()
        },
        WrenchProperties {
            name: Some("line\nbreak".into()),
            ..base.clone()
        },
        WrenchProperties {
            name: Some("n".repeat(4096)),
            ..base.clone()
        },
        WrenchProperties {
            name: Some(String::new()),
            ..base.clone()
        },
    ];
    for properties in hostile {
        assert!(
            g.cmd(
                a,
                Command::Tool(ToolAction::SetWrench {
                    brick: id,
                    properties: properties.clone(),
                })
            )
            .is_err(),
            "{properties:?} was accepted"
        );
    }
    assert_eq!(g.bricks(), before);
    // Wrong brick id with an otherwise valid dialog.
    assert!(
        g.cmd(
            a,
            Command::Tool(ToolAction::SetWrench {
                brick: id + 1000,
                properties: properties(Some("x"), true),
            })
        )
        .is_err()
    );
    g.cmd(
        a,
        Command::Tool(ToolAction::SetWrench {
            brick: id,
            properties: properties(Some("ok"), true),
        }),
    )
    .unwrap();
}

/// Requests: A wrench hit, Tool(Inspect{Events}), then Tool(SetEvents) with
/// MAX_EVENTS_PER_BRICK + 1 rows, a colour outside the palette, an unknown
/// output, a delay over five minutes and five parameters.
#[test]
fn event_rows_are_bounded_and_catalog_checked() {
    let mut g = Game::new(tooled());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    g.steps(60);
    let id = g.plant(a, A_ONLY_BRICK);
    g.swing(a, Some(WRENCH), A_ONLY_BRICK)
        .expect("own wrench hit");
    g.cmd(
        a,
        Command::Tool(ToolAction::Inspect {
            mode: InspectMode::Events,
        }),
    )
    .unwrap();
    let before = g.bricks();
    let hostile: Vec<Vec<EventRow>> = vec![
        vec![color_row(1); bri_world::MAX_EVENTS_PER_BRICK + 1],
        vec![color_row(200)],
        vec![EventRow {
            output: "fireRelayToEveryServerOnTheInternet".into(),
            ..color_row(1)
        }],
        vec![EventRow {
            delay_ms: 300_001,
            ..color_row(1)
        }],
        vec![EventRow {
            params: vec![EventValue::Color(1); 5],
            ..color_row(1)
        }],
        vec![EventRow {
            input: "x".repeat(10_000),
            ..color_row(1)
        }],
    ];
    for events in hostile {
        let len = events.len();
        assert!(
            g.cmd(
                a,
                Command::Tool(ToolAction::SetEvents { brick: id, events })
            )
            .is_err(),
            "{len} hostile rows accepted"
        );
    }
    assert_eq!(g.bricks(), before);
}

/// v20's `serverCmdAddEvent` raises `fireRelay` rows under 33 ms to 33 ms,
/// so a player's relay loop runs at most 30 hops a second and cannot flood
/// the host. Administrators may still relay with no delay.
#[test]
fn relay_rows_keep_v20s_33_ms_floor_except_for_administrators() {
    for administrator in [false, true] {
        let mut g = Game::new(tooled());
        let a = g.s.join("Ann".into(), A_SPAWN, administrator).unwrap();
        g.steps(60);
        let id = g.plant(a, A_ONLY_BRICK);
        g.swing(a, Some(WRENCH), A_ONLY_BRICK)
            .expect("own wrench hit");
        g.cmd(
            a,
            Command::Tool(ToolAction::Inspect {
                mode: InspectMode::Events,
            }),
        )
        .unwrap();
        let relay = |input: &str, delay_ms| EventRow {
            input: input.into(),
            delay_ms,
            output: "fireRelay".into(),
            params: vec![],
            ..color_row(1)
        };
        let events = vec![
            relay("onActivate", 0),
            relay("onRelay", 10),
            relay("onRelay", 500),
            color_row(1),
        ];
        g.cmd(
            a,
            Command::Tool(ToolAction::SetEvents { brick: id, events }),
        )
        .unwrap();
        let delays: Vec<u32> = g.bricks()[&id].events.iter().map(|r| r.delay_ms).collect();
        let expected = if administrator {
            [0, 10, 500, 0]
        } else {
            [33, 33, 500, 0]
        };
        assert_eq!(delays, expected, "administrator {administrator}");
    }
}

/// Requests: A Plant; B Avatar(default appearance) on a host without an
/// avatar catalog; JSON commands carrying forged owner/target/administrator
/// fields.
#[test]
fn avatar_and_forged_identity_fields_are_rejected() {
    let mut g = Game::new(plain());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    let b = g.s.join("Bob".into(), B_SPAWN, false).unwrap();
    let names = g.s.names();
    let avatars = g.s.avatars();
    assert!(
        g.cmd(
            b,
            Command::Avatar(bri_content::avatar::Appearance {
                parts: [("hat".to_string(), "../../hat".repeat(1000))].into(),
                colors: [("hat".to_string(), [f32::NAN; 4])].into(),
                face: "../../face".repeat(1000),
                decal: String::new(),
            })
        )
        .is_err(),
        "avatar accepted without a catalog"
    );
    assert_eq!(g.s.avatars(), avatars);
    assert_eq!(g.s.names(), names);
    // There is no rename/avatar command naming another player: identity
    // fields do not deserialize at all.
    for json in [
        r#"{"kind":"chat","value":"hi","owner":1}"#,
        r#"{"kind":"avatar","value":{},"target":1}"#,
        r#"{"kind":"plant","value":{"definition":"plate","position":[0.5,0.1,-3.25],"quarter_turns":0,"color":0,"owner":1}}"#,
        r#"{"kind":"tool","value":{"kind":"set_events","value":{"brick":1,"events":[],"owner":1}}}"#,
        r#"{"kind":"trust_invite","value":{"target":1,"level":2,"from":2}}"#,
        r#"{"kind":"mini_game","value":{"kind":"kick","value":{"target":1,"actor":2}}}"#,
        r#"{"kind":"rename","value":{"target":1,"name":"Mallory"}}"#,
    ] {
        assert!(
            serde_json::from_str::<Command>(json).is_err(),
            "forged command deserialized: {json}"
        );
    }
    let _ = a;
}

/// Requests (A and B verified, C anonymous): A TrustInvite{A,2};
/// A TrustInvite{9999,2}; A TrustInvite{B,0|3|255}; A TrustInvite{C,2};
/// C TrustInvite{A,2}; B AcceptTrust{A} with no invite; B AcceptTrust{9999};
/// B RejectTrust{A}; B IgnoreTrust{A}; A DemoteTrust{9999,0};
/// A DemoteTrust{B,7}; A UnIgnore{9999}; then B hammers A's brick.
#[test]
fn trust_commands_naming_self_missing_or_invalid_levels_change_nothing() {
    let mut g = Game::new(tooled());
    let a = verified(&mut g.s, "Ann", A_SPAWN, 1);
    let b = verified(&mut g.s, "Bob", B_SPAWN, 2);
    let c =
        g.s.join("Anon".into(), Vec3::new(-4.0, 0.05, 0.0), false)
            .unwrap();
    g.steps(60);
    let id = g.plant(a, SHARED_BRICK);
    let invites_before = g
        .log
        .iter()
        .filter(|(_, n)| matches!(n, Notice::TrustInvite { .. }))
        .count();
    for level in [2, 0, 3, 255] {
        g.cmd(a, Command::TrustInvite { target: a, level }).unwrap();
    }
    g.cmd(
        a,
        Command::TrustInvite {
            target: 9999,
            level: 2,
        },
    )
    .unwrap();
    for level in [0, 3, 255] {
        g.cmd(a, Command::TrustInvite { target: b, level }).unwrap();
    }
    // Unverified players have no identity to trust or be trusted with.
    g.cmd(
        a,
        Command::TrustInvite {
            target: c,
            level: 2,
        },
    )
    .unwrap();
    assert!(
        g.cmd(
            c,
            Command::TrustInvite {
                target: a,
                level: 2
            }
        )
        .is_err()
    );
    let invites_after = g
        .log
        .iter()
        .filter(|(_, n)| matches!(n, Notice::TrustInvite { .. }))
        .count();
    assert_eq!(
        invites_before, invites_after,
        "an invalid invite was delivered"
    );

    g.cmd(b, Command::AcceptTrust { from: a }).unwrap();
    g.cmd(b, Command::AcceptTrust { from: 9999 }).unwrap();
    g.cmd(b, Command::AcceptTrust { from: b }).unwrap();
    g.cmd(b, Command::RejectTrust { from: a }).unwrap();
    g.cmd(b, Command::IgnoreTrust { from: a }).unwrap();
    assert!(
        g.cmd(
            a,
            Command::DemoteTrust {
                target: 9999,
                level: 0
            }
        )
        .is_err()
    );
    g.cmd(
        a,
        Command::DemoteTrust {
            target: b,
            level: 7,
        },
    )
    .unwrap();
    g.cmd(a, Command::UnIgnore { target: 9999 }).unwrap();
    assert_ne!(g.trust_seen(b, a), Some(TrustLevel::Full));
    assert_ne!(g.trust_seen(b, a), Some(TrustLevel::Build));
    // Behavioural proof: B still cannot break A's brick.
    let before = g.bricks();
    g.swing(b, Some(HAMMER), SHARED_BRICK);
    assert!(g.bricks().contains_key(&id));
    assert_eq!(g.bricks(), before);
}

/// Requests: B TrustList(MAX_TRUST_LIST + 1 entries); B TrustList naming A
/// at FULL, itself, and bogus levels; then B hammers A's brick; control:
/// A TrustList naming B at BUILD makes the trust mutual.
#[test]
fn uploaded_trust_lists_are_bounded_and_one_sided_claims_grant_nothing() {
    let mut g = Game::new(tooled());
    let a = verified(&mut g.s, "Ann", A_SPAWN, 1);
    let b = verified(&mut g.s, "Bob", B_SPAWN, 2);
    g.steps(60);
    let id = g.plant(a, SHARED_BRICK);
    let oversized = (0..=MAX_TRUST_LIST)
        .map(|i| TrustEntry {
            principal: {
                let mut p = [7; 32];
                p[..8].copy_from_slice(&(i as u64).to_le_bytes());
                p
            },
            level: 2,
        })
        .collect();
    assert!(g.cmd(b, Command::TrustList(oversized)).is_err());
    g.cmd(
        b,
        Command::TrustList(vec![
            TrustEntry {
                principal: [1; 32],
                level: 2,
            },
            TrustEntry {
                principal: [2; 32],
                level: 2,
            },
            TrustEntry {
                principal: [1; 32],
                level: 3,
            },
            TrustEntry {
                principal: [1; 32],
                level: 255,
            },
            TrustEntry {
                principal: [0; 32],
                level: 2,
            },
        ]),
    )
    .unwrap();
    assert_ne!(g.trust_seen(b, a), Some(TrustLevel::Full));
    let before = g.bricks();
    g.swing(b, Some(HAMMER), SHARED_BRICK);
    assert_eq!(g.bricks(), before, "a one-sided trust claim broke a brick");
    assert!(g.bricks().contains_key(&id));
    // Control: consent from both sides produces mutual trust at the
    // uploader's level.
    g.cmd(
        a,
        Command::TrustList(vec![TrustEntry {
            principal: [2; 32],
            level: 1,
        }]),
    )
    .unwrap();
    assert_eq!(g.trust_seen(b, a), Some(TrustLevel::Build));
}

// ------------------------------------------------------------- role checks

/// Every administration action a non-administrator sends, in this order:
/// Kick, Ban, Unban, RequestBanList, RequestBrickGroups, RequestMaps, Spy,
/// Fetch, Find, ReturnToPreviousPosition, DropPlayerAtCamera,
/// DropCameraAtPlayer, RealBrickCount, ResetVehicles, CancelAllEvents,
/// DestructoWand, ChangeMap, ClearAllBricks, ClearBrickGroup,
/// HighlightBrickGroup, ClearVehicles, ClearBots, Warp, TimeScale,
/// SetAdminPassword, HostSetRole, HostSetAutoRole, HostSetPassword x3,
/// HostConfigure; then Admin with an aim.
#[test]
fn every_administration_action_is_denied_to_a_player() {
    let mut s = plain();
    s.set_map_list(vec![bri_sim::session::MapListing {
        id: "other".into(),
        name: "Other".into(),
    }])
    .unwrap();
    let mut g = Game::new(s);
    let host = g.s.join("Host".into(), A_SPAWN, true).unwrap();
    let guest = verified(&mut g.s, "Guest", B_SPAWN, 2);
    g.steps(60);
    let brick = g.plant(host, A_ONLY_BRICK);
    let host_conn = g.connection(guest, "Host");
    let guest_conn = g.connection(guest, "Guest");
    let bricks = g.bricks();
    let durable = serde_json::to_string(g.s.admin_durable_state()).unwrap();
    let host_body = g.body(host);
    let actions = vec![
        Action::Kick { target: host_conn },
        Action::Ban {
            target: host_conn,
            duration: BanDuration::Forever,
            reason: "x".into(),
        },
        Action::Unban { ban: BanId(1) },
        Action::RequestBanList,
        Action::RequestBrickGroups,
        Action::RequestMaps,
        Action::Spy { target: host_conn },
        Action::Fetch { target: host_conn },
        Action::Find { target: host_conn },
        Action::ReturnToPreviousPosition,
        Action::DropPlayerAtCamera,
        Action::DropCameraAtPlayer,
        Action::RealBrickCount,
        Action::ResetVehicles,
        Action::CancelAllEvents,
        Action::DestructoWand,
        Action::ChangeMap {
            map: "other".into(),
        },
        Action::ClearAllBricks,
        Action::ClearBrickGroup { group: host },
        Action::HighlightBrickGroup { group: host },
        Action::ClearVehicles,
        Action::ClearBots,
        Action::Warp,
        Action::TimeScale { scale: 0.2 },
        Action::SetAdminPassword {
            password: Secret::new("owned".into()).unwrap(),
        },
        Action::HostSetRole {
            target: guest_conn,
            role: Role::SuperAdmin,
        },
        Action::HostSetAutoRole {
            principal: Principal([2; 32]),
            role: Role::SuperAdmin,
        },
        Action::HostSetPassword {
            slot: PasswordSlot::Join,
            password: Secret::new("x".into()).unwrap(),
        },
        Action::HostSetPassword {
            slot: PasswordSlot::Admin,
            password: Secret::new("x".into()).unwrap(),
        },
        Action::HostSetPassword {
            slot: PasswordSlot::SuperAdmin,
            password: Secret::new("x".into()).unwrap(),
        },
        Action::HostConfigure {
            settings: ServerSettings::default(),
        },
    ];
    for action in actions {
        let label = format!("{action:?}");
        let reply = g.cmd(guest, admin(action));
        assert!(reply.is_err(), "{label} accepted: {reply:?}");
        if g.seq[&guest].is_multiple_of(50) {
            g.fresh_window();
        }
    }
    assert!(
        g.cmd_aim(
            guest,
            admin(Action::RequestMaps),
            Some(ActionAim {
                yaw: 0.0,
                pitch: 0.0
            })
        )
        .is_err()
    );
    assert_eq!(g.bricks(), bricks);
    assert!(g.bricks().contains_key(&brick));
    assert_eq!(g.s.time_scale(), 1.0);
    assert_eq!(g.s.take_map_change(), None);
    assert!(g.s.take_admin_disconnects().is_empty());
    assert_eq!(
        serde_json::to_string(g.s.admin_durable_state()).unwrap(),
        durable
    );
    assert_eq!(g.s.admin_state(guest).unwrap().role, Role::Player);
    assert!(!g.s.is_administrator(guest));
    assert_eq!(
        g.s.control(guest),
        Some(bri_sim::session::ControlObject::Player)
    );
    assert_eq!(g.s.names().len(), 2);
    let moved = Vec3::from(g.body(host).feet).distance(Vec3::from(host_body.feet));
    assert!(moved < 0.1, "host was moved {moved}");
}

/// Requests: guest Login(admin password) -> Admin; then HostSetRole{self,
/// SuperAdmin}, HostSetAutoRole, HostSetPassword{Admin}, HostConfigure,
/// SetAdminPassword, Kick{host}, Ban{host}; control: RealBrickCount.
#[test]
fn password_administrators_cannot_use_host_or_super_admin_powers() {
    let mut g = Game::new(plain());
    g.s.set_admin_passwords(
        Secret::new("admin-pass".into()).unwrap(),
        Secret::new("super-pass".into()).unwrap(),
    )
    .unwrap();
    let host = g.s.join("Host".into(), A_SPAWN, true).unwrap();
    let guest = verified(&mut g.s, "Guest", B_SPAWN, 2);
    let host_conn = g.connection(guest, "Host");
    let guest_conn = g.connection(guest, "Guest");
    g.cmd(
        guest,
        admin(Action::Login {
            password: Secret::new("admin-pass".into()).unwrap(),
        }),
    )
    .unwrap();
    assert_eq!(g.s.admin_state(guest).unwrap().role, Role::Admin);
    for action in [
        Action::HostSetRole {
            target: guest_conn,
            role: Role::SuperAdmin,
        },
        Action::HostSetAutoRole {
            principal: Principal([2; 32]),
            role: Role::SuperAdmin,
        },
        Action::HostSetPassword {
            slot: PasswordSlot::Admin,
            password: Secret::new("mine".into()).unwrap(),
        },
        Action::HostConfigure {
            settings: ServerSettings::default(),
        },
        Action::SetAdminPassword {
            password: Secret::new("mine".into()).unwrap(),
        },
        Action::Kick { target: host_conn },
        Action::Ban {
            target: host_conn,
            duration: BanDuration::Forever,
            reason: "coup".into(),
        },
    ] {
        let label = format!("{action:?}");
        assert!(g.cmd(guest, admin(action)).is_err(), "{label} accepted");
    }
    assert_eq!(g.s.admin_state(guest).unwrap().role, Role::Admin);
    assert!(g.s.take_admin_disconnects().is_empty());
    assert_eq!(g.s.names().len(), 2);
    let _ = host;
    // Control: ordinary admin commands work for the password admin.
    g.cmd(guest, admin(Action::RealBrickCount)).unwrap();
}

fn login_game() -> (Game, OwnerId) {
    let mut g = Game::new(plain());
    g.s.set_admin_passwords(
        Secret::new("admin-pass".into()).unwrap(),
        Secret::new("super-pass".into()).unwrap(),
    )
    .unwrap();
    let guest = verified(&mut g.s, "Guest", B_SPAWN, 2);
    (g, guest)
}
fn login(password: &str) -> Command {
    admin(Action::Login {
        password: Secret::new(password.into()).unwrap(),
    })
}
fn login_attempts(reply: &anyhow::Result<Reply>) -> Option<u8> {
    match reply {
        Ok(Reply::Admin(reply)) => match reply.data {
            AdminData::LoginRejected { attempts } => Some(attempts),
            _ => None,
        },
        _ => None,
    }
}

/// Requests: guest Login("wrong") x4; guest Login("admin-pass");
/// guest Admin(RealBrickCount).
#[test]
fn a_locked_connection_cannot_log_in_even_with_the_right_password() {
    let (mut g, guest) = login_game();
    for _ in 0..4 {
        let _ = g.cmd(guest, login("wrong"));
    }
    assert!(g.cmd(guest, login("admin-pass")).is_err());
    assert!(g.cmd(guest, admin(Action::RealBrickCount)).is_err());
    assert!(!g.s.is_administrator(guest));
}

/// Requests: guest Login("wrong") x3 (attempts 1..=3 reported), then a
/// fourth Login("wrong"), which must report attempt 4 and hand the host the
/// connection to close.
#[test]

fn fourth_failed_login_is_reported_and_disconnects() {
    let (mut g, guest) = login_game();
    for expected in 1..=3 {
        assert_eq!(
            login_attempts(&g.cmd(guest, login("wrong"))),
            Some(expected)
        );
    }
    let fourth = g.cmd(guest, login("wrong"));
    let disconnects = g.s.take_admin_disconnects();
    assert_eq!(
        disconnects,
        vec![guest],
        "the lockout disconnect never reached the host (reply was {fourth:?})"
    );
    // The sender's own reply cannot carry a snapshot once it is locked out;
    // the disconnect is what must reach the host.
    assert!(
        login_attempts(&fourth) == Some(4) || fourth.is_err(),
        "{fourth:?}"
    );
}

/// Requests: guest Login("wrong") x4 (locks the connection); the host
/// closes it (Session::disconnect) and the player resumes the same verified
/// identity (Session::resume_verified); guest Login("wrong") once more.
#[test]

fn failed_admin_logins_stay_counted_across_reconnects() {
    let (mut g, guest) = login_game();
    for _ in 0..4 {
        let _ = g.cmd(guest, login("wrong"));
    }
    g.s.disconnect(guest).unwrap();
    g.s.resume_verified(guest, B_SPAWN, false, Some(Principal([2; 32])))
        .unwrap();
    g.seq.insert(guest, 0);
    let fifth = g.cmd(guest, login("wrong"));
    assert!(
        fifth.is_err() || login_attempts(&fifth).is_some_and(|n| n > 4),
        "a reconnect restored fresh password guesses: {fifth:?}"
    );
}

/// Requests: guest LoadBuild; guest DropPlayerAt{eye}; guest
/// Admin(ChangeMap); guest Admin(TimeScale); guest ControlPlayer (harmless).
#[test]
fn players_cannot_load_builds_teleport_change_maps_or_time() {
    let mut s = plain();
    s.set_map_list(vec![bri_sim::session::MapListing {
        id: "other".into(),
        name: "Other".into(),
    }])
    .unwrap();
    let mut g = Game::new(s);
    let host = g.s.join("Host".into(), A_SPAWN, true).unwrap();
    let guest = g.s.join("Guest".into(), B_SPAWN, false).unwrap();
    g.steps(60);
    g.plant(host, A_ONLY_BRICK);
    let mut other = World::new("Other".into(), "o".into(), vec![[1.0; 4], [0.0; 4]]);
    other.bricks.insert(
        1,
        Brick::new(
            bri_world::ContentRef::Resolved("plate".into()),
            [10.5, 0.1, 10.25],
            0,
        ),
    );
    other.next_brick_id = 2;
    let build = bri_world::build::SavedBuild::capture(&other, false, false).unwrap();
    let bricks = g.bricks();
    let feet = g.body(guest).feet;
    assert!(
        g.cmd(
            guest,
            Command::LoadBuild {
                build: Box::new(build),
                ownership: false,
            }
        )
        .is_err()
    );
    assert!(!g.s.build_loading());
    assert!(
        g.cmd(
            guest,
            Command::DropPlayerAtCamera(Some(bri_sim::session::CameraView {
                eye: [50.0, 50.0, 50.0],
                yaw: 0.0,
                pitch: 0.0,
            }))
        )
        .is_err()
    );
    assert!(
        g.cmd(
            guest,
            admin(Action::ChangeMap {
                map: "other".into()
            })
        )
        .is_err()
    );
    assert!(
        g.cmd(guest, admin(Action::TimeScale { scale: 2.0 }))
            .is_err()
    );
    g.cmd(guest, Command::ControlPlayer).unwrap();
    g.steps(30);
    assert_eq!(g.bricks(), bricks);
    assert_eq!(g.s.take_map_change(), None);
    assert_eq!(g.s.time_scale(), 1.0);
    let moved = Vec3::from(g.body(guest).feet).distance(Vec3::from(feet));
    assert!(moved < 0.5, "guest teleported {moved}");
}

/// Requests (host): Admin(ChangeMap) to an unlisted map and to path-like
/// ids; Admin(TimeScale) NaN / inf / 1e9 / -5; DropPlayerAt with NaN, inf
/// and huge eyes; Admin(Spy self); Admin(Fetch unknown connection).
#[test]
fn administrator_requests_are_still_validated() {
    let mut s = plain();
    s.set_map_list(vec![bri_sim::session::MapListing {
        id: "other".into(),
        name: "Other".into(),
    }])
    .unwrap();
    let mut g = Game::new(s);
    let host = g.s.join("Host".into(), A_SPAWN, true).unwrap();
    g.steps(60);
    for map in [
        "unlisted",
        "../other",
        "/etc/passwd",
        "other/..",
        "C:\\x",
        "",
        "o\u{0}",
    ] {
        assert!(
            g.cmd(host, admin(Action::ChangeMap { map: map.into() }))
                .is_err(),
            "{map:?}"
        );
    }
    assert_eq!(g.s.take_map_change(), None);
    for scale in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(g.cmd(host, admin(Action::TimeScale { scale })).is_err());
    }
    assert_eq!(g.s.time_scale(), 1.0);
    g.cmd(host, admin(Action::TimeScale { scale: 1e9 }))
        .unwrap();
    assert_eq!(g.s.time_scale(), 2.0);
    g.cmd(host, admin(Action::TimeScale { scale: -5.0 }))
        .unwrap();
    assert_eq!(g.s.time_scale(), 0.2);
    let feet = g.body(host).feet;
    for eye in [
        [f32::NAN, 1.0, 1.0],
        [f32::INFINITY, 1.0, 1.0],
        [1e7, 1.0, 1.0],
        [0.0, -1e9, 0.0],
    ] {
        assert!(
            g.cmd(
                host,
                Command::DropPlayerAtCamera(Some(bri_sim::session::CameraView {
                    eye,
                    yaw: 0.0,
                    pitch: 0.0,
                }))
            )
            .is_err(),
            "{eye:?}"
        );
    }
    assert!(
        g.cmd(
            host,
            Command::DropPlayerAtCamera(Some(bri_sim::session::CameraView {
                eye: [1.0, 3.0, 1.0],
                yaw: f32::NAN,
                pitch: 0.0,
            }))
        )
        .is_err()
    );
    assert!(g.body(host).feet.iter().all(|v| v.is_finite()));
    assert_eq!(g.body(host).feet, feet);
    let me = g.connection(host, "Host");
    assert!(g.cmd(host, admin(Action::Spy { target: me })).is_err());
    assert!(
        g.cmd(
            host,
            admin(Action::Fetch {
                target: ConnectionId(9999)
            })
        )
        .is_err()
    );
    assert!(
        g.cmd(host, admin(Action::ClearBrickGroup { group: 424242 }))
            .is_err()
    );
}

/// Requests: A MiniGame(Create); B MiniGame(Join); B sends Configure, Kick
/// {A}, Reset, RespawnAll, End, Invite{C}; C Join{unknown}; C Accept with
/// no invite; C Create with hostile settings (long title, zero respawn,
/// unknown player type, unknown loadout item, colour 10, colour taken).
#[test]
fn minigame_management_requires_ownership_and_valid_settings() {
    let mut g = Game::new(plain());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    let b = g.s.join("Bob".into(), B_SPAWN, false).unwrap();
    let c =
        g.s.join("Cat".into(), Vec3::new(-4.0, 0.05, 0.0), false)
            .unwrap();
    g.steps(60);
    g.cmd(
        a,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings::default(),
        }),
    )
    .unwrap();
    let game = g.s.minigame_views()[0].id;
    g.cmd(b, Command::MiniGame(MiniGameRequest::Join { game }))
        .unwrap();
    let views = g.s.minigame_views();
    assert_eq!(views[0].members.len(), 2);
    for request in [
        MiniGameRequest::Configure {
            settings: Settings {
                title: "Hijacked".into(),
                ..Settings::default()
            },
        },
        MiniGameRequest::Kick { target: a },
        MiniGameRequest::Reset,
        MiniGameRequest::RespawnAll,
        MiniGameRequest::End,
        MiniGameRequest::Invite { target: c },
    ] {
        let label = format!("{request:?}");
        assert!(
            g.cmd(b, Command::MiniGame(request)).is_err(),
            "{label} accepted"
        );
    }
    assert!(
        g.cmd(c, Command::MiniGame(MiniGameRequest::Join { game: 9999 }))
            .is_err()
    );
    assert!(
        g.cmd(c, Command::MiniGame(MiniGameRequest::Accept { game }))
            .is_err()
    );
    assert!(
        g.cmd(
            c,
            Command::MiniGame(MiniGameRequest::Kick { target: 424242 })
        )
        .is_err()
    );
    let hostile = [
        (
            1,
            Settings {
                title: "t".repeat(4096),
                ..Settings::default()
            },
        ),
        (
            1,
            Settings {
                title: "bell\u{7}".into(),
                ..Settings::default()
            },
        ),
        (
            1,
            Settings {
                respawn_ms: 0,
                ..Settings::default()
            },
        ),
        (
            1,
            Settings {
                brick_respawn_ms: u32::MAX,
                ..Settings::default()
            },
        ),
        (
            1,
            Settings {
                player_type: "v20.player.godmode".into(),
                ..Settings::default()
            },
        ),
        (
            1,
            Settings {
                loadout: [Some("v20.weapon.nuke".into()), None, None, None, None],
                ..Settings::default()
            },
        ),
        (10, Settings::default()),
        (255, Settings::default()),
        (0, Settings::default()),
    ];
    for (color, settings) in hostile {
        g.fresh_window();
        assert!(
            g.cmd(
                c,
                Command::MiniGame(MiniGameRequest::Create { color, settings })
            )
            .is_err()
        );
    }
    assert_eq!(g.s.minigame_views(), views);
}

/// Requests: A MiniGame(Create{enable_building: false}); A Plant.
#[test]

fn minigame_with_building_disabled_refuses_plants() {
    let mut g = Game::new(plain());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    g.steps(60);
    g.cmd(
        a,
        Command::MiniGame(MiniGameRequest::Create {
            color: 0,
            settings: Settings {
                enable_building: false,
                ..Settings::default()
            },
        }),
    )
    .unwrap();
    g.steps(60);
    let before = g.bricks();
    let planted = g.cmd(
        a,
        Command::Plant {
            definition: "plate".into(),
            position: [0.5, 0.1, -3.25],
            quarter_turns: 0,
            color: 0,
        },
    );
    assert!(
        planted.is_err(),
        "planted with building disabled: {planted:?}"
    );
    assert_eq!(g.bricks(), before);
}

// ----------------------------------------------- inventory and combat state

/// Requests: EquipTool{5}, EquipTool{usize::MAX}, EquipTool{3} (empty),
/// DropTool{5}, DropTool{usize::MAX}, DropTool{3} (empty), WeaponTrigger
/// {down} with nothing held, UseSprayCan{2|255}, UseFxCan{9|255}, Emote
/// (unknown / huge), SwitchSeat(1) on foot, SwitchSeat(0|2|-128),
/// Respawn while alive, Activate with a NaN aim.
#[test]
fn inventory_seat_and_respawn_requests_are_bounded() {
    let mut g = Game::new(tooled());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    g.steps(60);
    let inventories = g.s.tool_inventories();
    let rejected = [
        Command::EquipTool { slot: Some(5) },
        Command::EquipTool {
            slot: Some(usize::MAX),
        },
        Command::EquipTool { slot: Some(3) },
        Command::DropTool { slot: 5 },
        Command::DropTool { slot: usize::MAX },
        Command::DropTool { slot: 3 },
        Command::UseSprayCan { color: 2 },
        Command::UseSprayCan { color: 255 },
        Command::UseFxCan { fx: 9 },
        Command::UseFxCan { fx: 255 },
        Command::Emote("dance".into()),
        Command::Emote("alarm".repeat(100_000)),
        Command::SwitchSeat(0),
        Command::SwitchSeat(2),
        Command::SwitchSeat(i8::MIN),
        Command::Respawn,
    ];
    for command in rejected {
        let label = format!("{:.80}", format!("{command:?}"));
        assert!(g.cmd(a, command).is_err(), "{label} accepted");
    }
    // Fire with empty hands is held, like v20's move trigger, and changes
    // nothing until a tool comes out.
    assert!(g.cmd(a, Command::WeaponTrigger { down: true }).is_ok());
    assert!(g.cmd(a, Command::WeaponTrigger { down: false }).is_ok());
    // `serverCmdNextSeat` on foot does nothing and says nothing.
    assert!(g.cmd(a, Command::SwitchSeat(1)).is_ok());
    for aim in [
        ActionAim {
            yaw: f32::NAN,
            pitch: 0.0,
        },
        ActionAim {
            yaw: 0.0,
            pitch: f32::INFINITY,
        },
        ActionAim {
            yaw: 4.0,
            pitch: 0.0,
        },
    ] {
        assert!(g.cmd_aim(a, Command::Activate, Some(aim)).is_err());
    }
    assert_eq!(g.s.tool_inventories(), inventories);
    assert!(g.s.vitals()[&a].alive);
}

/// Requests: Suicide; then while dead: Suicide, WeaponTrigger{down},
/// EquipTool{0}, Activate, BuildGesture(ShiftUp), ToggleLight, Respawn
/// (before the respawn delay), and Emote("bsd"), which v20 ignores quietly
/// (the brick selector sends it whenever it opens).
#[test]
fn dead_players_cannot_act() {
    let mut g = Game::new(tooled());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    g.steps(60);
    g.cmd(a, Command::Suicide).unwrap();
    assert!(!g.s.vitals()[&a].alive);
    for command in [
        Command::Suicide,
        Command::WeaponTrigger { down: true },
        Command::EquipTool { slot: Some(0) },
        Command::Activate,
        Command::BuildGesture(BuildGesture::ShiftUp),
        Command::ToggleLight,
    ] {
        let label = format!("{command:?}");
        assert!(g.cmd(a, command).is_err(), "{label} accepted while dead");
    }
    assert!(g.cmd(a, Command::Emote("bsd".into())).is_ok());
    let respawn = g.s.vitals()[&a].respawn_tick;
    if g.s.simulation().state().tick < respawn {
        assert!(g.cmd(a, Command::Respawn).is_err());
    }
    assert!(!g.s.vitals()[&a].alive);
}

/// Requests: Suicide; Plant while dead.
#[test]

fn dead_players_cannot_plant() {
    let mut g = Game::new(plain());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    g.steps(60);
    g.cmd(a, Command::Suicide).unwrap();
    let before = g.bricks();
    let planted = g.cmd(
        a,
        Command::Plant {
            definition: "plate".into(),
            position: [0.5, 0.1, -3.25],
            quarter_turns: 0,
            color: 0,
        },
    );
    assert!(planted.is_err(), "corpse planted: {planted:?}");
    assert_eq!(g.bricks(), before);
}

// --------------------------------------------------- session credentials

/// Requests: join with "", "   ", 49 bytes, "\n", "a\u{7}b", "\u{1b}[31m";
/// join with exactly 48 bytes (control); resume an unknown owner; resume a
/// connected owner; resume a verified owner under another key and
/// anonymously; a command and a movement from a departed owner and from an
/// owner that never existed.
#[test]
fn join_and_resume_credentials_are_checked() {
    let mut s = plain();
    let n = s.names().len();
    // Names are cleaned, not refused: blank ones become "Blockhead",
    // control characters go and long ones are cut on a character boundary.
    for (name, taken) in [
        (String::new(), "Blockhead".to_string()),
        ("\n".into(), "Blockhead".into()),
        ("a\u{7}b".into(), "ab".into()),
        ("\u{1b}[31mred".into(), "[31mred".into()),
        ("é".repeat(25), "é".repeat(23)),
    ] {
        let owner = s.join(name.clone(), A_SPAWN, false).unwrap();
        assert_eq!(s.names()[&owner], taken, "{name:?}");
        s.disconnect(owner).unwrap();
    }
    assert_eq!(s.names().len(), n);
    let long = s.join("x".repeat(48), A_SPAWN, false).unwrap();
    let keyed = verified(&mut s, "Keyed", B_SPAWN, 5);
    assert!(s.resume(424242, A_SPAWN).is_err());
    assert!(
        s.resume_verified(keyed, A_SPAWN, false, Some(Principal([5; 32])))
            .is_err()
    );
    s.disconnect(keyed).unwrap();
    assert!(
        s.resume_verified(keyed, B_SPAWN, false, Some(Principal([6; 32])))
            .is_err()
    );
    assert!(s.resume_verified(keyed, B_SPAWN, false, None).is_err());
    assert!(s.command(keyed, 1, Command::Chat("ghost".into())).is_err());
    assert!(s.movement(keyed, 1, MoveInput::default()).is_err());
    assert!(
        s.command(424242, 1, Command::Chat("nobody".into()))
            .is_err()
    );
    assert!(s.movement(424242, 1, MoveInput::default()).is_err());
    assert!(s.chat().is_empty());
    assert!(!s.is_administrator(long));
    // Control: the right key resumes.
    s.resume_verified(keyed, B_SPAWN, false, Some(Principal([5; 32])))
        .unwrap();
}

/// Requests: Chat seq 5; Chat seq 5 (replay); Chat seq 4 (rewind); Chat
/// seq u64::MAX; Chat seq 6 afterwards; 61 Activates in one window.
#[test]
fn command_sequences_cannot_replay_and_actions_are_rate_limited() {
    let mut s = plain();
    let a = s.join("Ann".into(), A_SPAWN, false).unwrap();
    let b = s.join("Bob".into(), B_SPAWN, false).unwrap();
    s.command(a, 5, Command::Chat("one".into())).unwrap();
    assert!(s.command(a, 5, Command::Chat("replay".into())).is_err());
    assert!(s.command(a, 4, Command::Chat("rewind".into())).is_err());
    s.command(a, u64::MAX, Command::Chat("last".into()))
        .unwrap();
    assert!(s.command(a, 6, Command::Chat("after max".into())).is_err());
    assert_eq!(s.chat().len(), 2);
    // A saturated sequence locks out only its own connection.
    s.command(b, 1, Command::Talking(true)).unwrap();
    let mut accepted = 0;
    for seq in 2..=80 {
        if s.command(b, seq, Command::Talking(false)).is_ok() {
            accepted += 1;
        }
    }
    assert_eq!(accepted, 59, "per-window action budget");
}

// --------------------------------------------------------- movement input

fn hostile_inputs() -> Vec<MoveInput> {
    let base = MoveInput::default();
    vec![
        MoveInput {
            forward: f32::NAN,
            ..base
        },
        MoveInput {
            right: f32::NAN,
            ..base
        },
        MoveInput {
            yaw: f32::NAN,
            ..base
        },
        MoveInput {
            pitch: f32::NAN,
            ..base
        },
        MoveInput {
            head_yaw: f32::NAN,
            ..base
        },
        MoveInput {
            forward: f32::INFINITY,
            ..base
        },
        MoveInput {
            right: f32::NEG_INFINITY,
            ..base
        },
        MoveInput {
            yaw: f32::INFINITY,
            ..base
        },
        MoveInput {
            forward: 1.0001,
            ..base
        },
        MoveInput {
            forward: 1e30,
            ..base
        },
        MoveInput {
            right: -2.0,
            ..base
        },
        MoveInput { yaw: 3.2, ..base },
        MoveInput { pitch: 1.6, ..base },
        MoveInput {
            pitch: -1e9,
            ..base
        },
        MoveInput {
            head_yaw: 100.0,
            ..base
        },
        MoveInput {
            forward: f32::MAX,
            right: f32::MIN,
            ..base
        },
    ]
}

/// Requests: movement(seq 1..=16) with each hostile input; 120 idle ticks.
#[test]
fn non_finite_and_out_of_range_movement_is_rejected_and_never_queued() {
    let mut g = Game::new(plain());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    g.steps(60);
    let before = g.body(a);
    for (i, input) in hostile_inputs().into_iter().enumerate() {
        assert!(
            g.s.movement(a, i as u64 + 1, input).is_err(),
            "{input:?} accepted"
        );
    }
    g.steps(120);
    let after = g.body(a);
    assert!(
        after
            .feet
            .iter()
            .chain(&after.velocity)
            .all(|v| v.is_finite())
    );
    assert!(Vec3::from(after.feet).distance(Vec3::from(before.feet)) < 0.05);
    assert_eq!(
        g.s.motion_states()
            .iter()
            .find(|(p, _)| p.owner == a)
            .unwrap()
            .1,
        0,
        "a hostile input was acknowledged"
    );
    for aim in [f32::NAN, f32::INFINITY] {
        assert!(
            ActionAim {
                yaw: aim,
                pitch: 0.0
            }
            .validate()
            .is_err()
        );
    }
}

/// Requests: movement seq 10 (forward); seq 5 and seq 10 again (rewinds);
/// seq u64::MAX (forward); seq 11 after it; 240 ticks.
#[test]
fn rewound_or_saturated_movement_sequences_cannot_replay_or_overflow() {
    let mut g = Game::new(plain());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    g.steps(60);
    let forward = MoveInput {
        forward: 1.0,
        ..Default::default()
    };
    g.s.movement(a, 10, forward).unwrap();
    g.steps(1);
    let acked = |g: &Game| {
        g.s.motion_states()
            .iter()
            .find(|(p, _)| p.owner == a)
            .unwrap()
            .1
    };
    assert_eq!(acked(&g), 10);
    g.s.movement(a, 5, forward).unwrap();
    g.s.movement(a, 10, forward).unwrap();
    g.steps(1);
    assert_eq!(acked(&g), 10, "a rewound input was simulated");
    g.s.movement(a, u64::MAX, forward).unwrap();
    g.s.movement(a, 11, forward).unwrap();
    g.steps(240);
    assert_eq!(acked(&g), u64::MAX);
    let p = g.body(a);
    assert!(p.feet.iter().chain(&p.velocity).all(|v| v.is_finite()));
    // Only two forward inputs ever ran, plus idle ticks.
    assert!(Vec3::from(p.feet).distance(A_SPAWN) < 1.0, "{p:?}");
}

/// Requests per tick for 240 ticks: an honest player sends one forward
/// input; an attacker sends 20 forward inputs (a flood far above the
/// budget). Then an attacker that withholds input for 40 ticks and bursts.
#[test]
fn input_floods_cannot_outrun_the_motor() {
    let mut g = Game::new(plain());
    let honest =
        g.s.join("Honest".into(), Vec3::new(-10.0, 0.05, 0.0), false)
            .unwrap();
    let flood =
        g.s.join("Flood".into(), Vec3::new(10.0, 0.05, 0.0), false)
            .unwrap();
    g.steps(60);
    let start = (g.body(honest).feet, g.body(flood).feet);
    let forward = MoveInput {
        forward: 1.0,
        ..Default::default()
    };
    let (mut hs, mut fs) = (0_u64, 0_u64);
    let ticks = 240;
    for _ in 0..ticks {
        hs += 1;
        g.s.movement(honest, hs, forward).unwrap();
        for _ in 0..20 {
            fs += 1;
            let _ = g.s.movement(flood, fs, forward);
        }
        g.steps(1);
    }
    let honest_distance = Vec3::from(g.body(honest).feet).distance(Vec3::from(start.0));
    let flood_distance = Vec3::from(g.body(flood).feet).distance(Vec3::from(start.1));
    assert!(
        honest_distance > 5.0,
        "baseline did not move: {honest_distance}"
    );
    // At most one extra input-burst worth of motor ticks.
    let bound = honest_distance * (ticks as f32 + 48.0) / ticks as f32 * 1.05;
    assert!(
        flood_distance <= bound,
        "flooded inputs moved {flood_distance} vs honest {honest_distance}"
    );
    let p = g.body(flood);
    assert!(p.feet.iter().chain(&p.velocity).all(|v| v.is_finite()));
}

/// Requests: 240 ticks of forward=1 for one player and forward=1, right=1
/// for another.
#[test]
fn diagonal_input_does_not_outrun_straight_input() {
    let mut g = Game::new(plain());
    let straight =
        g.s.join("Straight".into(), Vec3::new(-10.0, 0.05, 0.0), false)
            .unwrap();
    let diagonal =
        g.s.join("Diagonal".into(), Vec3::new(10.0, 0.05, 0.0), false)
            .unwrap();
    g.steps(60);
    let start = (g.body(straight).feet, g.body(diagonal).feet);
    for seq in 1..=240 {
        g.s.movement(
            straight,
            seq,
            MoveInput {
                forward: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
        g.s.movement(
            diagonal,
            seq,
            MoveInput {
                forward: 1.0,
                right: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
        g.steps(1);
    }
    let s = Vec3::from(g.body(straight).feet).distance(Vec3::from(start.0));
    let d = Vec3::from(g.body(diagonal).feet).distance(Vec3::from(start.1));
    assert!(d <= s * 1.05, "diagonal {d} vs straight {s}");
}

// ------------------------------------------------ size and shape bounds

/// Requests (each Plant within reach of the builder): NaN / inf / 1e30 /
/// off-grid positions; colour 2 and 255 (palette of two); quarter turns 4
/// and 255; definitions "", "../../../etc/passwd", "C:\\Windows\\win.ini",
/// "plate\0", "PLATE", 100 KiB of "a", "\u{202e}etalp"; a position 60 units
/// away. Control: a valid plate.
#[test]
fn plant_rejects_non_finite_huge_unknown_and_path_like_requests() {
    let mut g = Game::new(plain());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    g.steps(60);
    let good = [0.5, 0.1, -3.25];
    let plant =
        |definition: &str, position: [f32; 3], quarter_turns: u8, color: u8| Command::Plant {
            definition: definition.into(),
            position,
            quarter_turns,
            color,
        };
    let hostile = vec![
        plant("plate", [f32::NAN, 0.1, -3.25], 0, 0),
        plant("plate", [0.5, f32::INFINITY, -3.25], 0, 0),
        plant("plate", [0.5, 0.1, f32::NEG_INFINITY], 0, 0),
        plant("plate", [1e30, 0.1, -3.25], 0, 0),
        plant("plate", [-1e30, -1e30, -1e30], 0, 0),
        plant("plate", [f32::MAX, f32::MAX, f32::MAX], 0, 0),
        plant("plate", [0.5, 0.1, -3.3], 0, 0),
        plant("plate", [60.5, 0.1, -3.25], 0, 0),
        plant("plate", good, 0, 2),
        plant("plate", good, 0, 255),
        plant("plate", good, 4, 0),
        plant("plate", good, 255, 0),
        plant("", good, 0, 0),
        plant("../../../etc/passwd", good, 0, 0),
        plant("C:\\Windows\\win.ini", good, 0, 0),
        plant("plate\0", good, 0, 0),
        plant("PLATE", good, 0, 0),
        plant(&"a".repeat(100 * 1024), good, 0, 0),
        plant("\u{202e}etalp", good, 0, 0),
    ];
    let before = g.bricks();
    let revision = g.s.simulation().state().revision;
    for command in hostile {
        let label = format!("{:.100}", format!("{command:?}"));
        assert!(g.cmd(a, command).is_err(), "{label} accepted");
    }
    assert_eq!(g.bricks(), before);
    assert_eq!(g.s.simulation().state().revision, revision);
    g.plant(a, good);
}

/// Requests (a fresh 120-tick window before each): Chat of 257 bytes, 64
/// KiB, 1 MiB, "", "   ", "a\nb", "\u{7}", "\u{0}"; TeamChat of 257 bytes
/// and outside any minigame. Control: a 256-byte chat.
#[test]
fn chat_is_bounded_and_rejects_control_characters() {
    let mut g = Game::new(plain());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    for text in [
        "x".repeat(257),
        "x".repeat(64 * 1024),
        "x".repeat(1024 * 1024),
        String::new(),
        "   ".into(),
        "a\nb".into(),
        "\u{7}".into(),
        "\u{0}".into(),
        "é".repeat(129),
    ] {
        g.fresh_window();
        assert!(
            g.cmd(a, Command::Chat(text.clone())).is_err(),
            "chat of {} bytes accepted",
            text.len()
        );
        g.fresh_window();
        assert!(g.cmd(a, Command::TeamChat(text)).is_err());
    }
    assert!(g.s.chat().is_empty());
    g.fresh_window();
    g.cmd(a, Command::Chat("x".repeat(256))).unwrap();
    assert_eq!(g.s.chat().len(), 1);
}

/// Requests: guest Tool(SetEvents) with a stale sequence carrying
/// MAX_EVENTS_PER_BRICK valid rows and one invalid row at the end.
#[test]

fn stale_or_rate_limited_requests_are_refused_before_per_row_work() {
    let mut s = plain();
    let a = s.join("Ann".into(), A_SPAWN, false).unwrap();
    s.command(a, 10, Command::Talking(true)).unwrap();
    let mut rows = vec![color_row(1); bri_world::MAX_EVENTS_PER_BRICK * 8];
    rows.push(color_row(200));
    let error = s
        .command(
            a,
            10,
            Command::Tool(ToolAction::SetEvents {
                brick: 1,
                events: rows,
            }),
        )
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("Stale/replayed"),
        "per-row validation ran first: {error:#}"
    );
}

/// Requests: host LoadBuild (control, succeeds); a new window; guest
/// SaveBuild x4; host LoadBuild; a new window; host SaveBuild x2.
#[test]

fn player_saves_cannot_starve_the_administrator_build_budget() {
    let mut g = Game::new(plain());
    let host = g.s.join("Host".into(), A_SPAWN, true).unwrap();
    let guest = g.s.join("Guest".into(), B_SPAWN, false).unwrap();
    g.steps(60);
    let empty = World::new("Empty".into(), "e".into(), vec![[1.0; 4], [0.0; 4]]);
    let build = bri_world::build::SavedBuild::capture(&empty, false, false).unwrap();
    let load = |build: &bri_world::build::SavedBuild| Command::LoadBuild {
        build: Box::new(build.clone()),
        ownership: false,
    };
    let control = g.cmd(host, load(&build));
    assert!(
        !format!("{control:?}").contains("rate exceeded"),
        "{control:?}"
    );
    g.fresh_window();
    let save = Command::SaveBuild {
        events: true,
        ownership: true,
    };
    g.cmd(guest, save.clone()).unwrap();
    for _ in 0..3 {
        let refused = g.cmd(guest, save.clone());
        assert!(
            format!("{refused:?}").contains("save rate exceeded"),
            "one save per player per window: {refused:?}"
        );
    }
    let blocked = g.cmd(host, load(&build));
    assert!(
        !format!("{blocked:?}").contains("rate exceeded"),
        "a player's saves blocked the administrator: {blocked:?}"
    ); // The host saving twice in a window (save, load, save over it) is its
    // own work, not a player hogging the slots.
    g.fresh_window();
    for _ in 0..2 {
        let saved = g.cmd(host, save.clone());
        assert!(!format!("{saved:?}").contains("rate exceeded"), "{saved:?}");
    }
}

/// First impressions 15: everyone starts as "Blockhead", so a second
/// player with a connected player's name gets a number to tell them apart.
#[test]
fn players_sharing_a_name_are_numbered() {
    let mut s = plain();
    let a = s
        .join("Blockhead".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let b = s
        .join("blockhead".into(), Vec3::new(3.0, 0.05, 0.0), false)
        .unwrap();
    let c = s
        .join("Blockhead".into(), Vec3::new(6.0, 0.05, 0.0), false)
        .unwrap();
    let long = "x".repeat(23);
    let d = s
        .join(long.clone(), Vec3::new(9.0, 0.05, 0.0), false)
        .unwrap();
    let e = s
        .join(long.clone(), Vec3::new(12.0, 0.05, 0.0), false)
        .unwrap();
    let names = s.names();
    assert_eq!(names[&a], "Blockhead");
    assert_eq!(names[&b], "blockhead 2");
    assert_eq!(names[&c], "Blockhead 3");
    assert_eq!(names[&d], long);
    assert_eq!(names[&e], format!("{} 2", &long[..21]));
}

/// First impressions 14: the host's Server Settings apply. The brick limit
/// holds for everyone, the plant rate for non-administrators (v20
/// `MsgPlantError_Limit` for both), and long chat is cut to the limit.
#[test]
fn host_server_settings_limit_bricks_plant_rate_and_chat() {
    let mut s = plain();
    let host = s
        .join("Host".into(), Vec3::new(0.0, 0.05, 6.0), true)
        .unwrap();
    let guest = s
        .join("Guest".into(), Vec3::new(4.0, 0.05, 6.0), false)
        .unwrap();
    let mut seq = BTreeMap::<OwnerId, u64>::new();
    let mut send = |s: &mut Session, owner: OwnerId, command: Command| {
        let n = seq.entry(owner).or_default();
        *n += 1;
        s.command(owner, *n, command)
    };
    let settings = ServerSettings {
        brick_limit: 4,
        bricks_per_second: 2,
        max_chat_length: 5,
        ..ServerSettings::default()
    };
    // Only the host may change them.
    assert!(
        send(
            &mut s,
            guest,
            admin(Action::HostConfigure {
                settings: settings.clone()
            })
        )
        .is_err()
    );
    let Ok(Reply::Admin(reply)) = send(
        &mut s,
        host,
        admin(Action::HostConfigure {
            settings: settings.clone(),
        }),
    ) else {
        panic!("the host configures")
    };
    assert_eq!(reply.snapshot.options, Some(settings));
    let plant = |x: f32| Command::Plant {
        definition: "plate".into(),
        position: [x + 0.5, 0.1, -3.25],
        quarter_turns: 0,
        color: 0,
    };
    let limit = |r: anyhow::Result<Reply>| {
        r.unwrap_err()
            .downcast_ref::<bri_sim::simulation::PlantFailure>()
            .copied()
            == Some(bri_sim::simulation::PlantFailure::Limit)
    };
    assert!(send(&mut s, guest, plant(0.0)).is_ok());
    assert!(send(&mut s, guest, plant(2.0)).is_ok());
    assert!(
        limit(send(&mut s, guest, plant(4.0))),
        "third plant in a second"
    );
    for _ in 0..121 {
        s.step().unwrap();
    }
    assert!(send(&mut s, guest, plant(4.0)).is_ok());
    assert!(send(&mut s, host, plant(6.0)).is_ok());
    assert!(
        limit(send(&mut s, host, plant(8.0))),
        "the server's brick limit"
    );
    assert!(send(&mut s, guest, Command::Chat("hello there".into())).is_ok());
    assert_eq!(s.chat().last().unwrap().text, "hello");
}

/// `$Pref::Server::BrickPublicDomainTimeout`: once a builder has been gone
/// that many minutes, anyone may break their bricks.
#[test]
fn abandoned_bricks_turn_public_after_the_hosts_timeout() {
    let mut g = Game::new(tooled());
    g.s.set_server_settings(bri_admin::ServerSettings {
        public_domain_timeout_minutes: 1,
        ..Default::default()
    })
    .unwrap();
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    let b = g.s.join("Bob".into(), B_SPAWN, false).unwrap();
    g.steps(60);
    let shared = g.plant(a, SHARED_BRICK);
    g.s.disconnect(a).unwrap();
    g.swing(b, Some(HAMMER), SHARED_BRICK);
    assert!(
        g.bricks().contains_key(&shared),
        "still Ann's a moment later"
    );
    // Trust is looked at on the minute, so by the second one it is public.
    g.steps(2 * 60 * 120);
    g.swing(b, Some(HAMMER), SHARED_BRICK);
    assert!(!g.bricks().contains_key(&shared), "public after a minute");
}

#[test]
fn the_etard_filter_holds_back_chat_and_says_why() {
    let mut g = Game::new(tooled());
    let a = g.s.join("Ann".into(), A_SPAWN, false).unwrap();
    g.s.take_private_notices();
    g.s.command(a, 1, Command::Chat("r u there".into()))
        .unwrap();
    assert!(g.s.take_private_notices().iter().any(
        |(o, n)| *o == a && matches!(n, Notice::Chat(t) if t.contains("Please use full words"))
    ));
    g.s.set_server_settings(bri_admin::ServerSettings {
        chat_filter: false,
        ..Default::default()
    })
    .unwrap();
    // (A different line: the same one again would be "Do not repeat
    // yourself.")
    g.s.command(a, 2, Command::Chat("r u here".into())).unwrap();
    assert!(g.s.take_private_notices().is_empty());
}

/// A loaded world's event programs are installed by `prepare_events`, the
/// whole-world scan a host runs before serving, so the first tick (which
/// also answers joins) installs nothing. Counted, not timed: on the old code
/// the first tick installed every brick and `prepare_events` did not exist.
#[test]
fn prepared_events_leave_the_first_tick_nothing_to_install() {
    let mut world = World::new(
        "Hardening".into(),
        "hardening".into(),
        vec![[1.0; 4], [0.0; 4]],
    );
    for id in 1..=8 {
        let mut brick = Brick::new(
            bri_world::ContentRef::Resolved("plate".into()),
            [2.0 * id as f32 + 0.5, 0.1, 10.25],
            0,
        );
        brick.events = vec![color_row(1)];
        world.bricks.insert(id, brick);
    }
    world.next_brick_id = 9;
    let mut prepared = plain_with(world.clone());
    assert_eq!(prepared.prepare_events(), 8, "every brick is scanned once");
    assert_eq!(prepared.prepare_events(), 0, "and not again");
    prepared.step().unwrap();
    assert_eq!(prepared.last_event_work().installed, 0);
    // Unprepared, the first tick owes the scan; later ticks do not.
    let mut lazy = plain_with(world);
    lazy.step().unwrap();
    assert_eq!(lazy.last_event_work().installed, 8);
    lazy.step().unwrap();
    assert_eq!(lazy.last_event_work().installed, 0);
}
