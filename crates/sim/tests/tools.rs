use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    player::{MoveInput, PlayerTuning},
    session::{
        Command, InspectMode, Reply, Session, ToolAction, ToolCatalog, UNDO_QUEUE_SIZE,
        WrenchProperties,
    },
    simulation::Simulation,
};
use bri_world::{
    Brick, ContentRef, EventRow, EventTarget, EventValue, SourceRecord, World, authority::Edit,
};
use glam::Vec3;
use rapier3d::prelude::*;
mod common;
use common::*;

fn session(f: &Fixture, bricks: Vec<Brick>, wall: bool) -> Session {
    session_on(f, bricks, wall, "test")
}
fn session_on(f: &Fixture, bricks: Vec<Brick>, wall: bool, map_id: &str) -> Session {
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
    let definitions = Definitions {
        entries: [
            (
                "plate".into(),
                Definition {
                    mesh: mesh.clone(),
                    collision: collision.clone(),
                    shape: shape.clone(),
                    indestructible: false,
                    special: Default::default(),
                    reflection: None,
                    link: None,
                    glass: [0.0; 4],
                    bot: None,
                },
            ),
            (
                "sturdy_plate".into(),
                Definition {
                    mesh,
                    collision,
                    shape,
                    indestructible: true,
                    special: Default::default(),
                    reflection: None,
                    link: None,
                    glass: [0.0; 4],
                    bot: None,
                },
            ),
        ]
        .into(),
    };
    let mut world = World::new(
        "Tools".into(),
        map_id.into(),
        vec![[1.0; 4], [0.2, 0.3, 0.4, 0.5]],
    );
    for brick in bricks {
        world.bricks.insert(world.next_brick_id, brick);
        world.next_brick_id += 1;
    }
    let mut colliders =
        vec![ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0))];
    if wall {
        colliders.push(
            ColliderBuilder::cuboid(10.0, 10.0, 0.1).translation(Vector::new(0.0, 0.0, -2.0)),
        );
    }
    let mut s = Session::new(Simulation::new(world, definitions, colliders).unwrap());
    s.set_weapon_pack(f.weapons.clone()).unwrap();
    s.set_event_catalog(bri_events::testing::catalog(), Vec::new())
        .unwrap();
    s
}
fn core_tool_bounds() -> std::collections::BTreeMap<String, bri_weapons::ItemBounds> {
    bri_weapons::CORE_TOOLS
        .into_iter()
        .map(|id| {
            (
                id.into(),
                bri_weapons::ItemBounds {
                    min: [-0.1; 3],
                    max: [0.1; 3],
                },
            )
        })
        .collect()
}
fn catalog() -> ToolCatalog {
    ToolCatalog {
        lights: ["light/red".into()].into(),
        emitters: ["emitter/smoke".into()].into(),
        items: ["v20.weapon.gunitem".into(), "v20.weapon.hammeritem".into()].into(),
        prints: [
            ("print/face".into(), "2x2".into()),
            ("print/wide".into(), "2x1".into()),
            ("print/A".into(), "Letters".into()),
        ]
        .into(),
        brick_print_aspects: [("plate".into(), "2x2".into())].into(),
        default_print: Some("print/A".into()),
        ..Default::default()
    }
}

on_both! {
fn tools_swing_only_when_held_and_switching_or_dropping_revokes_the_dialog(f: &Fixture) {
    let mut s = session(f, vec![], false);
    s.set_tool_catalog(catalog()).unwrap();
    s.set_item_bounds(
        bri_weapons::CORE_TOOLS
            .into_iter()
            .map(|id| {
                (
                    id.into(),
                    bri_weapons::ItemBounds {
                        min: [-0.1; 3],
                        max: [0.1; 3],
                    },
                )
            })
            .collect(),
    )
    .unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.), false)
        .unwrap();
    let id = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    aim(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    let before = s.snapshot().world;
    // v20's move trigger: a press with empty hands is held, and does nothing.
    s.command(owner, 2, Command::WeaponTrigger { down: true })
        .unwrap();
    s.step().unwrap();
    s.release_trigger(owner).unwrap();
    // Equipping mounts the real v20 image for every player to see.
    s.command(owner, 3, Command::EquipTool { slot: Some(1) })
        .unwrap();
    assert_eq!(
        s.weapon_view().images[&owner][0].image,
        "v20.image.wrenchimage"
    );
    swing(&mut s, owner, 4, 1).unwrap();
    assert_eq!(opened(&mut s, owner).unwrap().0, id);
    s.command(owner, 5, Command::EquipTool { slot: Some(2) })
        .unwrap();
    s.command(owner, 6, Command::EquipTool { slot: Some(1) })
        .unwrap();
    let properties = WrenchProperties {
        name: Some("changed".into()),
        raycast: true,
        colliding: true,
        visible: true,
        ..Default::default()
    };
    let edit = Command::Tool(ToolAction::SetWrench {
        brick: id,
        properties: properties.clone(),
    });
    assert!(
        s.command(owner, 7, edit.clone())
            .unwrap_err()
            .to_string()
            .contains("Hit a brick")
    );
    swing(&mut s, owner, 8, 1).unwrap();
    assert!(opened(&mut s, owner).is_some());
    s.command(owner, 9, Command::DropTool { slot: 1 }).unwrap();
    assert!(
        s.command(owner, 10, edit)
            .unwrap_err()
            .to_string()
            .contains("not equipped")
    );
    assert_eq!(s.snapshot().world.bricks, before.bricks);
    swing(&mut s, owner, 11, 0).unwrap();
    assert!(!s.simulation().state().bricks.contains_key(&id));
}
}

on_both! {
fn swings_use_the_current_aim_and_respect_brick_trust(f: &Fixture) {
    use bri_sim::session::ActionAim;
    let mut s = session(f, vec![], false);
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.0), false)
        .unwrap();
    let front = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    let back = plant(&mut s, owner, 2, [0.5, 0.1, 3.25]);
    aim(&mut s, owner, 1, [0.5, 0.1, 3.25]);
    let before = s.snapshot();
    s.equip_tool(owner, Some(0)).unwrap();
    for (index, invalid) in [
        ActionAim {
            yaw: f32::NAN,
            pitch: 0.0,
        },
        ActionAim {
            yaw: 0.0,
            pitch: 2.0,
        },
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            s.command_with_aim(
                owner,
                3 + index as u64,
                Command::WeaponTrigger { down: true },
                Some(invalid)
            )
            .is_err()
        );
    }
    assert!(
        s.command(owner, 2, Command::WeaponTrigger { down: true })
            .is_err(),
        "Replay must not execute twice"
    );
    assert_eq!(s.snapshot().world.bricks, before.world.bricks);
    let other = s
        .join("Other".into(), Vec3::new(3.0, 0.05, 0.0), false)
        .unwrap();
    aim(&mut s, other, 1, [0.5, 0.1, -3.25]);
    swing(&mut s, other, 1, 0).unwrap();
    assert!(
        center_prints(&mut s, other)
            .iter()
            .any(|text| text == "Builder does not trust you enough to do that.")
    );
    assert_eq!(s.snapshot().world.bricks, before.world.bricks);
    // The swing lands where the body aims when it fires.
    swing(&mut s, owner, 5, 0).unwrap();
    assert!(s.simulation().state().bricks.contains_key(&front));
    assert!(!s.simulation().state().bricks.contains_key(&back));
    aim(&mut s, owner, 2, [0.5, 0.1, -3.25]);
    swing(&mut s, owner, 6, 0).unwrap();
    assert!(!s.simulation().state().bricks.contains_key(&front));
}
}

on_both! {
/// A click carries its own aim (the turn and the click leave the client
/// together, as in v20's move that carries the trigger). The wrench swings
/// two ticks after the click (wrenchImage's PreFire), before the movement
/// that turned the body has arrived: the swing still lands where the click
/// aimed. Found by the screen harness: a guest who turned and clicked at
/// once wrenched thin air along their old facing.
fn a_click_lands_its_delayed_swing_where_it_aimed(f: &Fixture) {
    use bri_sim::session::ActionAim;
    let mut s = session(f, vec![], false);
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.0), false)
        .unwrap();
    let front = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    // The body faces away from the brick, and keeps doing so on the host.
    aim(&mut s, owner, 1, [0.5, 0.1, 3.25]);
    s.equip_tool(owner, Some(1)).unwrap();
    hold_still(&mut s, owner);
    s.step().unwrap();
    let p = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap();
    let facing = bri_sim::player::PlayerState { yaw: 0.0, ..p };
    let d = Vec3::new(0.5, 0.1, -3.25) - facing.eye(&PlayerTuning::default());
    let click = ActionAim {
        yaw: 0.0,
        pitch: d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()),
    };
    // A quick click: pressed and released before the swing.
    s.command_with_aim(owner, 2, Command::WeaponTrigger { down: true }, Some(click))
        .unwrap();
    s.command_with_aim(owner, 3, Command::WeaponTrigger { down: false }, Some(click))
        .unwrap();
    for _ in 0..8 {
        hold_still(&mut s, owner);
        s.step().unwrap();
    }
    let (brick, _, mode) = opened(&mut s, owner).expect("the swing hit nothing");
    assert_eq!((brick, mode), (front, InspectMode::Wrench));
    // Without an aim of its own, a click swings where the body faces.
    for _ in 0..60 {
        hold_still(&mut s, owner);
        s.step().unwrap();
    }
    s.command(owner, 4, Command::WeaponTrigger { down: true })
        .unwrap();
    s.command(owner, 5, Command::WeaponTrigger { down: false })
        .unwrap();
    for _ in 0..8 {
        hold_still(&mut s, owner);
        s.step().unwrap();
    }
    assert!(opened(&mut s, owner).is_none(), "the aimless click used an old click's aim");
}
}

on_both! {
fn swinging_at_nothing_or_the_ground_is_not_an_error_and_plays_v20_effects(f: &Fixture) {
    use bri_sim::presentation::CueKind;
    let mut s = session(f, vec![], false);
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.0), false)
        .unwrap();
    // Joining plays v20's spawn projectile, as a respawn does; let it burst
    // before listening for swing effects.
    for _ in 0..1200 {
        if s.snapshot().weapons.projectiles.is_empty() {
            break;
        }
        s.step().unwrap();
    }
    s.step().unwrap();
    s.take_cues();
    // Midair: the swing animates but nothing is hit.
    aim(&mut s, owner, 1, [0.5, 30.0, -3.25]);
    swing(&mut s, owner, 1, 1).unwrap();
    swing(&mut s, owner, 2, 0).unwrap();
    let cues = s.take_cues();
    assert!(cues.iter().any(|c| matches!(&c.kind,
        CueKind::WeaponAnimation { sequence, thread: 2, .. } if sequence == "wrench")));
    assert!(cues.iter().any(|c| matches!(&c.kind,
        CueKind::WeaponAnimation { sequence, thread: 2, .. } if sequence == "armattack")));
    assert!(
        !cues
            .iter()
            .any(|c| matches!(&c.kind, CueKind::WeaponEffect { .. }))
    );
    // The ground: the hammer's spark explosion and hit sound; the wrench's
    // explosion and miss sound.
    aim(&mut s, owner, 2, [0.5, 0.0, -2.0]);
    swing(&mut s, owner, 3, 0).unwrap();
    swing(&mut s, owner, 4, 1).unwrap();
    let cues = s.take_cues();
    for (effect, sound) in [
        ("hammerExplosion", "hammerHitSound"),
        ("wrenchExplosion", "wrenchMissSound"),
    ] {
        assert!(cues.iter().any(|c| matches!(&c.kind,
            CueKind::WeaponEffect { definition, .. } if definition == effect)));
        assert!(cues.iter().any(|c| matches!(&c.kind,
            CueKind::WeaponSound { profile } if profile == sound)));
    }
    assert!(opened(&mut s, owner).is_none());
}
}

on_both! {
fn spray_cans_mount_in_hand_and_paint_by_projectile(f: &Fixture) {
    let (mut s, owner, id) = setup(f);
    let guest = s
        .join("Guest".into(), Vec3::new(3.0, 0.05, 0.0), false)
        .unwrap();
    aim(&mut s, guest, 1, [0.5, 0.1, -3.25]);
    assert!(
        s.command(owner, 2, Command::UseSprayCan { color: 9 })
            .is_err()
    );
    s.command(owner, 3, Command::UseSprayCan { color: 1 })
        .unwrap();
    let held = &s.weapon_view().images[&owner][0];
    assert_eq!(held.image, "v20.image.bluespraycanimage");
    assert_eq!(held.paint, Some(1));
    assert_eq!(s.tool_inventories()[&owner].selected, None);
    hold_still(&mut s, owner);
    s.take_cues();
    s.command(owner, 4, Command::WeaponTrigger { down: true })
        .unwrap();
    for _ in 0..40 {
        s.step().unwrap();
    }
    assert_eq!(s.simulation().state().bricks[&id].color, 1);
    // `setSprayCanColor`'s colour copies: the mist and splash name the paint.
    let cues = s.take_cues();
    for effect in ["color1PaintEmitter", "color1PaintExplosion"] {
        assert!(
            cues.iter().any(|c| matches!(&c.kind,
                bri_sim::presentation::CueKind::WeaponEffect { definition, .. }
                    if definition == effect)),
            "{effect}"
        );
    }
    s.command(owner, 5, Command::WeaponTrigger { down: false })
        .unwrap();
    // Someone else's brick is refused with a centre print.
    s.command(guest, 2, Command::UseSprayCan { color: 0 })
        .unwrap();
    hold_still(&mut s, guest);
    s.command(guest, 3, Command::WeaponTrigger { down: true })
        .unwrap();
    for _ in 0..40 {
        s.step().unwrap();
    }
    assert_eq!(s.simulation().state().bricks[&id].color, 1);
    assert!(!center_prints(&mut s, guest).is_empty());
    // FX cans: rainbow colour effect, jello shape effect.
    for (seq, fx, check) in [(6, 6u8, 6u8), (8, 8, 1)] {
        s.command(owner, seq, Command::UseFxCan { fx }).unwrap();
        hold_still(&mut s, owner);
        s.command(owner, seq + 1, Command::WeaponTrigger { down: true })
            .unwrap();
        for _ in 0..40 {
            s.step().unwrap();
        }
        let brick = &s.simulation().state().bricks[&id];
        if fx < 7 {
            assert_eq!(brick.color_effect, check);
        } else {
            assert_eq!(brick.shape_effect, check);
        }
    }
    assert!(s.command(owner, 10, Command::UseFxCan { fx: 9 }).is_err());
    // Putting tools away drops the can.
    s.command(owner, 11, Command::EquipTool { slot: None })
        .unwrap();
    assert!(!s.weapon_view().images.contains_key(&owner));
}
}

on_both! {
fn scrolling_the_spray_can_while_holding_fire_keeps_spraying(f: &Fixture) {
    // v20: hold the mouse and scroll colours; each new colour can mounts
    // under the held trigger and sprays at once.
    let (mut s, owner, id) = setup(f);
    let spray = |s: &mut Session| {
        hold_still(s, owner);
        for _ in 0..40 {
            s.step().unwrap();
        }
        s.simulation().state().bricks[&id].color
    };
    s.command(owner, 2, Command::UseSprayCan { color: 1 })
        .unwrap();
    s.command(owner, 3, Command::WeaponTrigger { down: true })
        .unwrap();
    assert_eq!(spray(&mut s), 1);
    s.command(owner, 4, Command::UseSprayCan { color: 0 })
        .unwrap();
    assert_eq!(spray(&mut s), 0);
    // Out to the wrench and back to a can, still holding: it sprays again.
    s.command(owner, 5, Command::EquipTool { slot: Some(1) })
        .unwrap();
    s.command(owner, 6, Command::UseSprayCan { color: 1 })
        .unwrap();
    assert_eq!(spray(&mut s), 1);
    // The release still arrives and stops it.
    s.command(owner, 7, Command::WeaponTrigger { down: false })
        .unwrap();
    spray(&mut s);
    s.command(owner, 8, Command::UseSprayCan { color: 0 })
        .unwrap();
    assert_eq!(spray(&mut s), 1, "released");
}
}

on_both! {
fn switching_paint_columns_while_holding_fire_keeps_spraying(f: &Fixture) {
    // E (`shiftPaintColumn`) moves to the next column: another colour, or
    // the FX column's can (`useFXCan`), then back round. Held, each sprays.
    let (mut s, owner, id) = setup(f);
    let spray = |s: &mut Session| {
        hold_still(s, owner);
        for _ in 0..40 {
            s.step().unwrap();
        }
        s.simulation().state().bricks[&id].clone()
    };
    s.command(owner, 2, Command::UseSprayCan { color: 1 })
        .unwrap();
    s.command(owner, 3, Command::WeaponTrigger { down: true })
        .unwrap();
    assert_eq!(spray(&mut s).color, 1);
    s.command(owner, 4, Command::UseFxCan { fx: 6 }).unwrap();
    assert_eq!(spray(&mut s).color_effect, 6);
    s.command(owner, 5, Command::UseSprayCan { color: 0 })
        .unwrap();
    assert_eq!(spray(&mut s).color, 0);
}
}

on_both! {
fn a_press_whose_release_was_lost_is_a_fresh_click(f: &Fixture) {
    // A dialog can take the mouse-up, so the host can see two presses with
    // no release between them. A mouse cannot do that: the second press is
    // a new click, so the wrench (which waits for a release) swings again.
    let (mut s, owner, id) = setup(f);
    s.equip_tool(owner, Some(1)).unwrap();
    hold_still(&mut s, owner);
    s.command(owner, 2, Command::WeaponTrigger { down: true })
        .unwrap();
    for _ in 0..8 {
        s.step().unwrap();
    }
    assert_eq!(opened(&mut s, owner).unwrap().0, id);
    for _ in 0..3 {
        hold_still(&mut s, owner);
        for _ in 0..30 {
            s.step().unwrap();
        }
    }
    assert!(opened(&mut s, owner).is_none(), "held: one swing only");
    hold_still(&mut s, owner);
    s.command(owner, 3, Command::WeaponTrigger { down: true })
        .unwrap();
    for _ in 0..10 {
        s.step().unwrap();
    }
    assert_eq!(opened(&mut s, owner).unwrap().0, id);
    // Still "held", switch to the printer: it prints at once. A click while
    // that print is mid-Fire (which waits out its timeout) must still print
    // again once the printer can take a press (the Gate's app_flow case).
    s.equip_tool(owner, Some(2)).unwrap();
    hold_still(&mut s, owner);
    s.step().unwrap();
    s.step().unwrap();
    assert_eq!(opened(&mut s, owner).unwrap().2, InspectMode::Printer);
    s.command(owner, 4, Command::WeaponTrigger { down: true })
        .unwrap();
    for _ in 0..50 {
        s.step().unwrap();
    }
    assert_eq!(opened(&mut s, owner).unwrap().2, InspectMode::Printer);
}
}

on_both! {
fn a_click_that_beats_the_movement_after_a_stall_still_swings(f: &Fixture) {
    // A stalled client stops renewing its input lease. When it recovers,
    // its reliable trigger commands can reach the host before the movement
    // datagrams do; the click still swings once movement resumes.
    let (mut s, owner, id) = setup(f);
    s.equip_tool(owner, Some(1)).unwrap();
    hold_still(&mut s, owner);
    for _ in 0..90 {
        s.step().unwrap();
    }
    s.command(owner, 2, Command::WeaponTrigger { down: true })
        .unwrap();
    s.command(owner, 3, Command::WeaponTrigger { down: false })
        .unwrap();
    s.step().unwrap();
    for _ in 0..10 {
        hold_still(&mut s, owner);
        s.step().unwrap();
    }
    assert_eq!(opened(&mut s, owner).map(|o| o.0), Some(id));
}
}

fn plant(s: &mut Session, owner: u64, seq: u64, position: [f32; 3]) -> u64 {
    let Reply::Planted(id) = s
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
        panic!("expected plant")
    };
    id
}
fn aim(s: &mut Session, owner: u64, _seq: u64, target: [f32; 3]) {
    let p = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap();
    // The Eye node sits ahead of the body along its yaw, so face the target
    // from the feet first and pitch from the eye that yaw puts in place;
    // aiming from the current eye flips a target under the player behind it.
    let flat = Vec3::from(target) - Vec3::from(p.feet);
    let yaw = if flat.x.abs() + flat.z.abs() > 1e-4 {
        flat.x.atan2(-flat.z)
    } else {
        p.yaw
    };
    let facing = bri_sim::player::PlayerState { yaw, ..p.clone() };
    let d = Vec3::from(target) - facing.eye(&PlayerTuning::default().scaled(p.scale));
    let sequence = move_sequence(s);
    s.movement(
        owner,
        sequence,
        MoveInput {
            yaw,
            pitch: d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()),
            ..Default::default()
        },
    )
    .unwrap();
    s.step().unwrap();
}
fn tool(s: &mut Session, owner: u64, seq: u64, action: ToolAction) -> anyhow::Result<Reply> {
    // Fixture setup equips the tool under test through the trusted host API;
    // dedicated authority tests above exercise remote equip/rejection ordering.
    let slot = match action {
        ToolAction::SetPrint { .. } => Some(2),
        ToolAction::Inspect { .. }
        | ToolAction::SetWrench { .. }
        | ToolAction::SetEvents { .. } => Some(1),
        _ => None,
    };
    s.equip_tool(owner, slot)?;
    s.command(owner, seq, Command::Tool(action))
}
/// Swing the wrench or printer (or open events over the wrench dialog).
fn inspect(s: &mut Session, owner: u64, seq: u64, mode: InspectMode) -> Brick {
    if mode == InspectMode::Events {
        let Reply::Inspected {
            brick,
            mode: actual,
            ..
        } = tool(s, owner, seq, ToolAction::Inspect { mode }).unwrap()
        else {
            panic!("expected inspection")
        };
        assert_eq!(actual, mode);
        return *brick;
    }
    let slot = if mode == InspectMode::Printer { 2 } else { 1 };
    swing(s, owner, seq, slot).unwrap();
    let (_, brick, actual) = opened(s, owner).expect("expected inspection");
    assert_eq!(actual, mode);
    brick
}
fn properties() -> WrenchProperties {
    WrenchProperties {
        name: Some("lamp".into()),
        light: Some("light/red".into()),
        emitter: Some("emitter/smoke".into()),
        emitter_direction: 4,
        item_spawn: bri_world::ItemSpawn::default(),
        raycast: false,
        colliding: false,
        visible: false,
        ..Default::default()
    }
}
fn setup(f: &Fixture) -> (Session, u64, u64) {
    let mut s = session(f, vec![], false);
    s.set_tool_catalog(catalog()).unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let id = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    aim(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    (s, owner, id)
}

on_both! {
fn wrench_changes_are_atomic_and_nonraycasting_bricks_remain_editable(f: &Fixture) {
    let (mut s, owner, id) = setup(f);
    let original = inspect(&mut s, owner, 2, InspectMode::Wrench);
    let before = s.snapshot().world;
    let mut invalid = properties();
    invalid.emitter_direction = 6;
    assert!(
        tool(
            &mut s,
            owner,
            3,
            ToolAction::SetWrench {
                brick: id,
                properties: invalid
            }
        )
        .is_err()
    );
    assert_eq!(s.snapshot().world, before);
    let mut unknown = properties();
    unknown.light = Some("light/forged".into());
    assert!(
        tool(
            &mut s,
            owner,
            4,
            ToolAction::SetWrench {
                brick: id,
                properties: unknown
            }
        )
        .is_err()
    );
    assert_eq!(s.snapshot().world, before);
    tool(
        &mut s,
        owner,
        5,
        ToolAction::SetWrench {
            brick: id,
            properties: properties(),
        },
    )
    .unwrap();
    let changed = &s.simulation().state().bricks[&id];
    assert_eq!(changed.definition, original.definition);
    assert_eq!(changed.owner, owner);
    assert_eq!(
        changed.light.as_ref().unwrap().asset,
        ContentRef::Resolved("light/red".into())
    );
    assert_eq!(changed.emitter.as_ref().unwrap().direction, 4);
    assert!(!changed.raycast && !changed.colliding && !changed.visible);
    assert_eq!(s.simulation().state().revision, before.revision + 1);
    assert!(
        tool(
            &mut s,
            owner,
            6,
            ToolAction::SetWrench {
                brick: id,
                properties: properties()
            }
        )
        .is_err()
    );
    inspect(&mut s, owner, 7, InspectMode::Wrench);
    let mut restore = properties();
    restore.raycast = true;
    restore.colliding = true;
    restore.visible = true;
    tool(
        &mut s,
        owner,
        8,
        ToolAction::SetWrench {
            brick: id,
            properties: restore,
        },
    )
    .unwrap();
    assert!(s.simulation().state().bricks[&id].raycast);
    // Another edit between reading and applying cannot be overwritten silently.
    inspect(&mut s, owner, 9, InspectMode::Wrench);
    s.edit_brick(owner, id, Edit::Name(Some("changed".into())))
        .unwrap();
    let before = s.snapshot().world;
    assert!(
        tool(
            &mut s,
            owner,
            11,
            ToolAction::SetWrench {
                brick: id,
                properties: properties()
            }
        )
        .unwrap_err()
        .to_string()
        .contains("changed since inspection")
    );
    assert_eq!(s.snapshot().world, before);
}
}

on_both! {
fn wrench_item_catalog_ranges_and_clear_are_authoritative_and_atomic(f: &Fixture) {
    let (mut s, owner, id) = setup(f);
    inspect(&mut s, owner, 2, InspectMode::Wrench);
    let before = s.snapshot().world;
    for (offset, item_spawn) in [
        bri_world::ItemSpawn {
            item: Some(ContentRef::Resolved("v20.weapon.forged".into())),
            ..Default::default()
        },
        bri_world::ItemSpawn {
            item: Some(ContentRef::unresolved("item_ui", "Gun")),
            ..Default::default()
        },
        bri_world::ItemSpawn {
            position: 6,
            ..Default::default()
        },
        bri_world::ItemSpawn {
            direction: 1,
            ..Default::default()
        },
        bri_world::ItemSpawn {
            respawn_ms: 0,
            ..Default::default()
        },
        bri_world::ItemSpawn {
            respawn_ms: 300001,
            ..Default::default()
        },
    ]
    .into_iter()
    .enumerate()
    {
        let mut p = properties();
        p.item_spawn = item_spawn;
        assert!(
            tool(
                &mut s,
                owner,
                offset as u64 + 3,
                ToolAction::SetWrench {
                    brick: id,
                    properties: p
                }
            )
            .is_err()
        );
        assert_eq!(s.snapshot().world, before);
    }
    let mut p = properties();
    p.item_spawn = bri_world::ItemSpawn {
        item: Some(ContentRef::Resolved("v20.weapon.gunitem".into())),
        position: 5,
        direction: 3,
        respawn_ms: 12000,
    };
    tool(
        &mut s,
        owner,
        9,
        ToolAction::SetWrench {
            brick: id,
            properties: p.clone(),
        },
    )
    .unwrap();
    assert_eq!(s.simulation().state().bricks[&id].item_spawn, p.item_spawn);
    inspect(&mut s, owner, 10, InspectMode::Wrench);
    p.item_spawn.item = None;
    tool(
        &mut s,
        owner,
        11,
        ToolAction::SetWrench {
            brick: id,
            properties: p.clone(),
        },
    )
    .unwrap();
    assert_eq!(s.simulation().state().bricks[&id].item_spawn, p.item_spawn);
}
}

on_both! {
/// `Item::Respawn` fades a picked-up brick item out until its respawn time,
/// but the wrench's Send always runs `fxDTSBrick::setItem`, which replaces
/// the faded Item with a fresh one.
fn a_wrench_send_replaces_a_faded_item_with_a_fresh_one(f: &Fixture) {
    let mut s = session(f, vec![], false);
    s.set_tool_catalog(catalog()).unwrap();
    s.set_item_bounds(core_tool_bounds()).unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let id = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    aim(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    inspect(&mut s, owner, 2, InspectMode::Wrench);
    let mut p = properties();
    p.item_spawn = bri_world::ItemSpawn {
        item: Some(ContentRef::Resolved(bri_weapons::CORE_TOOLS[0].into())),
        position: 2,
        direction: 2,
        respawn_ms: 5000,
    };
    let set = |s: &mut Session, seq| {
        tool(
            s,
            owner,
            seq,
            ToolAction::SetWrench {
                brick: id,
                properties: p.clone(),
            },
        )
        .unwrap();
    };
    set(&mut s, 3);
    s.step().unwrap();
    let at = Vec3::from(s.weapon_view().static_items[0].position);
    let taker = s
        .join("Taker".into(), Vec3::new(at.x, 0.05, at.z - 0.2), false)
        .unwrap();
    let hammers = |s: &Session| {
        s.tool_inventories()[&taker]
            .slots
            .iter()
            .filter(|t| t.as_deref() == Some(bri_weapons::CORE_TOOLS[0]))
            .count()
    };
    let before = hammers(&s);
    s.step().unwrap();
    assert_eq!(hammers(&s), before + 1);
    let tick = s.simulation().state().tick;
    let faded = s.weapon_view().static_items[0].available_at;
    assert!(faded > tick, "the pickup fades the item out");
    // Swinging the wrench changes nothing; its Send restocks the brick.
    inspect(&mut s, owner, 4, InspectMode::Wrench);
    assert_eq!(s.weapon_view().static_items[0].available_at, faded);
    set(&mut s, 5);
    assert!(s.weapon_view().static_items[0].available_at <= s.simulation().state().tick);
}
}

on_both! {
fn native_item_allowlist_installation_is_atomic_and_bounded(f: &Fixture) {
    let mut c = catalog();
    let before = c.clone();
    assert!(c.install_items(["a".into(), "a".into()]).is_err());
    assert_eq!(c, before);
    assert!(c.install_items(["".into()]).is_err());
    assert_eq!(c, before);
    assert!(
        c.install_items((0..1025).map(|i| format!("item/{i}")))
            .is_err()
    );
    assert_eq!(c, before);
    c.install_items(["v20.weapon.printgun".into()]).unwrap();
    assert_eq!(c.items, ["v20.weapon.printgun".into()].into());
}
}

on_both! {
fn printing_uses_catalog_aspect_letters_default_and_inspection_identity(f: &Fixture) {
    let (mut s, owner, id) = setup(f);
    assert_eq!(
        s.simulation().state().bricks[&id].print,
        Some(ContentRef::Resolved("print/A".into()))
    );
    assert!(
        tool(
            &mut s,
            owner,
            2,
            ToolAction::SetPrint {
                brick: id,
                print: Some("print/face".into())
            }
        )
        .is_err()
    );
    inspect(&mut s, owner, 3, InspectMode::Printer);
    let before = s.snapshot().world;
    for (seq, print) in ["print/wide", "print/missing"].into_iter().enumerate() {
        assert!(
            tool(
                &mut s,
                owner,
                seq as u64 + 4,
                ToolAction::SetPrint {
                    brick: id,
                    print: Some(print.into())
                }
            )
            .is_err()
        );
        assert_eq!(s.snapshot().world, before);
    }
    tool(
        &mut s,
        owner,
        6,
        ToolAction::SetPrint {
            brick: id,
            print: Some("print/face".into()),
        },
    )
    .unwrap();
    inspect(&mut s, owner, 7, InspectMode::Printer);
    tool(
        &mut s,
        owner,
        8,
        ToolAction::SetPrint {
            brick: id,
            print: Some("print/A".into()),
        },
    )
    .unwrap();
    inspect(&mut s, owner, 9, InspectMode::Printer);
    assert!(
        tool(
            &mut s,
            owner,
            10,
            ToolAction::SetPrint {
                brick: id + 1,
                print: None
            }
        )
        .is_err()
    );
    let mut invalid = catalog();
    invalid.default_print = Some("print/missing".into());
    assert!(s.set_tool_catalog(invalid).is_err());
    // Failed catalog replacement retains the prior catalog and inspection.
    tool(
        &mut s,
        owner,
        11,
        ToolAction::SetPrint {
            brick: id,
            print: None,
        },
    )
    .unwrap();
    s.set_tool_catalog(ToolCatalog::default()).unwrap();
    swing(&mut s, owner, 12, 2).unwrap();
    assert!(opened(&mut s, owner).is_none());
}
}

on_both! {
fn next_brick_of_the_aspect_takes_the_players_last_print_like_v20(f: &Fixture) {
    let (mut s, owner, id) = setup(f);
    inspect(&mut s, owner, 2, InspectMode::Printer);
    tool(
        &mut s,
        owner,
        3,
        ToolAction::SetPrint {
            brick: id,
            print: Some("print/face".into()),
        },
    )
    .unwrap();
    let next = plant(&mut s, owner, 4, [1.5, 0.1, -3.25]);
    assert_eq!(
        s.simulation().state().bricks[&next].print,
        Some(ContentRef::Resolved("print/face".into()))
    );
    // Other players keep v20's Letters/A default until they print.
    let other = s
        .join("Other".into(), Vec3::new(-3.0, 0.05, 0.0), false)
        .unwrap();
    let theirs = plant(&mut s, other, 1, [2.5, 0.1, -3.25]);
    assert_eq!(
        s.simulation().state().bricks[&theirs].print,
        Some(ContentRef::Resolved("print/A".into()))
    );
}
}

on_both! {
fn event_binding_checks_cannot_be_bypassed_and_opaque_source_is_preserved(f: &Fixture) {
    let mut brick = Brick::new(ContentRef::Resolved("plate".into()), [0.5, 0.1, -3.25], 7);
    brick.source_records.push(SourceRecord {
        line: 12,
        text: "+-EVENT\tunsupported output".into(),
        diagnostic: Some("native adapter required".into()),
    });
    let source = brick.source_records.clone();
    let mut s = session(f, vec![brick], false);
    s.set_tool_catalog(catalog()).unwrap();
    let owner = s
        .join("Admin".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    aim(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    let event = EventRow {
        conditions: vec![],
            preserved: None,
        enabled: true,
        input: "onActivate".into(),
        delay_ms: 25,
        target: EventTarget::Slot(bri_events::Slot::SelfBrick),
        output: "setLight".into(),
        params: vec![EventValue::Datablock(Some("unknown".into()))],
    };
    inspect(&mut s, owner, 1, InspectMode::Wrench);
    inspect(&mut s, owner, 2, InspectMode::Events);
    let before = s.snapshot().world;
    assert!(
        s.edit_brick(owner, 1, Edit::Events(vec![event.clone()]))
            .is_err()
    );
    assert_eq!(s.snapshot().world, before);
    let bad = event.clone();
    let event = EventRow {
        output: "setColor".into(),
        params: vec![EventValue::Color(1)],
        ..event
    };
    // `serverCmdAddEvent` takes each line alone: the unreadable line is
    // left out, with word why, and the good one still stands.
    s.take_private_notices();
    tool(
        &mut s,
        owner,
        5,
        ToolAction::SetEvents {
            brick: 1,
            events: vec![bad, event.clone()],
        },
    )
    .unwrap();
    assert!(
        s.take_private_notices().iter().any(|(o, n)| *o == owner
            && matches!(n, bri_sim::session::Notice::Chat(text)
                if text.starts_with("Event line 1 was left out"))),
    );
    assert_eq!(s.simulation().state().bricks[&1].source_records, source);
    assert_eq!(s.simulation().state().bricks[&1].events, vec![event]);
    s.command(owner, 6, Command::Activate).unwrap();
    for _ in 0..4 {
        s.step().unwrap();
    }
    assert_eq!(s.simulation().state().bricks[&1].color, 1);
    assert_eq!(s.simulation().state().bricks[&1].source_records, source);
    assert!(
        serde_json::from_str::<ToolAction>(
            r#"{"kind":"set_events","value":{"brick":1,"events":[],"source_records":[]}}"#
        )
        .is_err()
    );
    // v20 has no raw brick-edit command; clients reach events only by wrench.
    assert!(
        serde_json::from_str::<Command>(
            r#"{"kind":"edit","value":{"brick":1,"edit":{"events":[]}}}"#
        )
        .is_err()
    );
}
}

/// An Add-On that reviews the wrench rows builders send
/// (`on_event_row`): no painting by event, and no relays.
fn row_checker() -> (Root, std::sync::Arc<bri_package_runtime::Catalog>) {
    let root = Root(std::env::temp_dir().join(format!("bri-event-rows-{}", std::process::id())));
    let dir = root.0.join("probe");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("package.json"),
        r#"{ "schema_version": 1, "id": "probe", "version": "1.0.0", "api": 1,
             "name": "probe", "license": "CC0-1.0", "capabilities": [],
             "provides": [
               { "kind": "behaviour", "id": "probe:behaviour/main", "file": "behaviour.json" },
               { "kind": "script", "id": "probe:script/main", "file": "main.rhai" } ] }"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("behaviour.json"),
        r#"{ "schema_version": 1, "script": "main.rhai", "on_event_row": true }"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("main.rhai"),
        r#"
fn on_event_row(p, brick, row) {
    if row.output == "setColor" {
        return `No painting: row ${row.index}, ${row.input} ${row.target} ${row.class}`;
    }
    row.output != "fireRelay"
}
"#,
    )
    .unwrap();
    let set = bri_package::packages::PackageSet {
        schema_version: 1,
        packages: vec![bri_package::packages::PackageEntry {
            id: "probe".into(),
            version: "1.0.0".into(),
            side: bri_package::packages::Side::Server,
            dir: "probe".into(),
            role: None,
        }],
    };
    let catalog = bri_package_runtime::Catalog::load(&root.0, &set, true).unwrap();
    (root, std::sync::Arc::new(catalog))
}
struct Root(std::path::PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Requests: a wrench swing, Events, then SetEvents with three rows. The
/// Add-On keeps one and refuses two, and the builder hears why (v20
/// Slayer's `serverCmdAddEvent` with Restrict Output Events on).
#[test]
fn an_add_on_may_refuse_rows_a_builder_sends_and_says_why() {
    let f = Fixture::synthetic();
    let (_root, add_ons) = row_checker();
    let mut s = session(&f, vec![], false);
    s.set_tool_catalog(catalog()).unwrap();
    s.install_packages(add_ons, None).unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let id = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    aim(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    inspect(&mut s, owner, 2, InspectMode::Wrench);
    inspect(&mut s, owner, 3, InspectMode::Events);
    let row = |output: &str, params| EventRow {
        conditions: vec![],
        preserved: None,
        enabled: true,
        input: "onActivate".into(),
        delay_ms: 0,
        target: EventTarget::Slot(bri_events::Slot::SelfBrick),
        output: output.into(),
        params,
    };
    let kept = row("setColliding", vec![EventValue::Bool(false)]);
    s.take_private_notices();
    tool(
        &mut s,
        owner,
        4,
        ToolAction::SetEvents {
            brick: id,
            events: vec![
                row("setColor", vec![EventValue::Color(1)]),
                kept.clone(),
                row("fireRelay", vec![]),
            ],
        },
    )
    .unwrap();
    assert_eq!(s.simulation().state().bricks[&id].events, vec![kept]);
    let told: Vec<String> = s
        .take_private_notices()
        .into_iter()
        .filter_map(|(to, n)| match n {
            bri_sim::session::Notice::Chat(text) if to == owner => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(
        told,
        [
            "No painting: row 0, onActivate Self fxDTSBrick",
            "You may not use the fireRelay event."
        ]
    );
}

on_both! {
fn hammer_ranges_and_map_occlusion_are_authoritative(f: &Fixture) {
    // `hammerImage::onFire` casts from `getEyePoint()` (the m.dts Eye node,
    // 2.156 above the feet) 5 units, or 5.5 looking steeply down. Straight
    // down onto a plate (top 0.2) reaches from feet up to about 3.54. The
    // node is 0.141 ahead of the body, so the player stands that far back to
    // look straight down on the plate.
    for (position, spawn, succeeds) in [
        ([0.5, 2.5, -4.75], Vec3::new(0.5, 0.1, 0.0), true),
        ([0.5, 2.5, -5.75], Vec3::new(0.5, 0.1, 0.0), false),
        ([0.5, 0.1, -0.25], Vec3::new(0.5, 3.3, -0.109), true),
        ([0.5, 0.1, -0.25], Vec3::new(0.5, 3.8, -0.109), false),
    ] {
        let mut brick = Brick::new(ContentRef::Resolved("plate".into()), position, 0);
        brick.raycast = false;
        let mut s = session(f, vec![brick], false);
        let owner = s.join("Admin".into(), spawn, true).unwrap();
        aim(&mut s, owner, 1, position);
        swing(&mut s, owner, 1, 0).unwrap();
        assert_eq!(
            s.simulation().state().bricks.is_empty(),
            succeeds,
            "{position:?} {spawn:?}"
        );
    }
    let position = [0.5, 2.5, -4.25];
    let mut s = session(f,
        vec![Brick::new(
            ContentRef::Resolved("plate".into()),
            position,
            0,
        )],
        true,
    );
    s.set_tool_catalog(catalog()).unwrap();
    let owner = s
        .join("Admin".into(), Vec3::new(0.5, 0.1, 0.0), true)
        .unwrap();
    aim(&mut s, owner, 1, position);
    swing(&mut s, owner, 1, 0).unwrap();
    assert_eq!(s.simulation().state().bricks.len(), 1);
    swing(&mut s, owner, 2, 1).unwrap();
    assert!(opened(&mut s, owner).is_none());
}
}

on_both! {
fn undo_is_owner_scoped_lifo_spends_removed_bricks_and_survives_authenticated_resume(f: &Fixture) {
    let (mut s, owner, first) = setup(f);
    let second = plant(&mut s, owner, 2, [1.5, 0.1, -3.25]);
    let third = plant(&mut s, owner, 3, [2.5, 0.1, -3.25]);
    aim(&mut s, owner, 2, [2.5, 0.1, -3.25]);
    swing(&mut s, owner, 4, 0).unwrap();
    assert!(!s.simulation().state().bricks.contains_key(&third));
    let guest = s
        .join("Guest".into(), Vec3::new(5.0, 0.05, 0.0), true)
        .unwrap();
    assert_eq!(
        tool(&mut s, guest, 1, ToolAction::UndoBrick).unwrap(),
        Reply::Undone(None)
    );
    s.disconnect(owner).unwrap();
    s.resume(owner, Vec3::new(50.0, 0.05, 50.0)).unwrap();
    // v20 pops one entry per press: the hammered brick's entry is spent.
    for (seq, undone) in [(1, None), (2, Some(second)), (3, Some(first)), (4, None)] {
        assert_eq!(
            tool(&mut s, owner, seq, ToolAction::UndoBrick).unwrap(),
            Reply::Undone(undone)
        );
    }
    assert!(s.simulation().state().bricks.is_empty());
}
}

on_both! {
fn undo_retains_only_the_511_entries_of_a_512_slot_queue(f: &Fixture) {
    let mut s = session(f, vec![], false);
    // Fifty plants a second: above v20's default plant rate.
    s.set_server_settings(bri_admin::ServerSettings {
        bricks_per_second: 1000,
        ..Default::default()
    })
    .unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(-3.0, 0.05, 0.0), false)
        .unwrap();
    let mut seq = 0;
    let mut first = 0;
    for i in 0..UNDO_QUEUE_SIZE {
        if i % 50 == 0 {
            for _ in 0..120 {
                s.step().unwrap();
            }
        }
        seq += 1;
        let id = plant(
            &mut s,
            owner,
            seq,
            [(i % 24) as f32 + 0.5, 0.1, -2.25 - (i / 24) as f32 * 0.5],
        );
        if i == 0 {
            first = id;
        }
    }
    for i in 0..UNDO_QUEUE_SIZE - 1 {
        if i % 50 == 0 {
            for _ in 0..120 {
                s.step().unwrap();
            }
        }
        seq += 1;
        assert_eq!(
            tool(&mut s, owner, seq, ToolAction::UndoBrick).unwrap(),
            Reply::Undone(Some((UNDO_QUEUE_SIZE - i) as u64))
        );
    }
    assert_eq!(
        tool(&mut s, owner, seq + 1, ToolAction::UndoBrick).unwrap(),
        Reply::Undone(None)
    );
    assert_eq!(
        s.simulation()
            .state()
            .bricks
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![first]
    );
}
}

on_both! {
/// `serverCmdUndoBrick` walks one mixed stack: prints, shape FX, colour FX,
/// spray paint, then the plant, which breaks like a hammered brick.
fn undo_reverts_paint_and_print_then_breaks_the_plant(f: &Fixture) {
    use bri_sim::presentation::CueKind;
    let (mut s, owner, id) = setup(f);
    let spray = |s: &mut Session, seq: u64, command: Command| {
        s.command(owner, seq, command).unwrap();
        hold_still(s, owner);
        s.command(owner, seq + 1, Command::WeaponTrigger { down: true })
            .unwrap();
        for _ in 0..40 {
            s.step().unwrap();
        }
        s.command(owner, seq + 2, Command::WeaponTrigger { down: false })
            .unwrap();
    };
    spray(&mut s, 2, Command::UseSprayCan { color: 1 });
    spray(&mut s, 5, Command::UseFxCan { fx: 6 });
    spray(&mut s, 8, Command::UseFxCan { fx: 8 });
    inspect(&mut s, owner, 11, InspectMode::Printer);
    tool(
        &mut s,
        owner,
        12,
        ToolAction::SetPrint {
            brick: id,
            print: Some("print/face".into()),
        },
    )
    .unwrap();
    let brick = |s: &Session| s.simulation().state().bricks.get(&id).cloned();
    let edited = brick(&s).unwrap();
    assert_eq!(
        (edited.color, edited.color_effect, edited.shape_effect),
        (1, 6, 1)
    );
    s.take_cues();
    let undo = |s: &mut Session, seq: u64| {
        assert_eq!(
            tool(s, owner, seq, ToolAction::UndoBrick).unwrap(),
            Reply::Undone(Some(id))
        );
        brick(s)
    };
    let b = undo(&mut s, 13).unwrap();
    assert_eq!(b.print, Some(ContentRef::Resolved("print/A".into())));
    assert_eq!(undo(&mut s, 14).unwrap().shape_effect, 0);
    assert_eq!(undo(&mut s, 15).unwrap().color_effect, 0);
    let b = undo(&mut s, 16).unwrap();
    assert_eq!((b.color, b.color_effect, b.shape_effect), (0, 0, 0));
    assert!(undo(&mut s, 17).is_none());
    let cues = s.take_cues();
    assert_eq!(
        cues.iter()
            .filter(|c| matches!(&c.kind,
                CueKind::WeaponAnimation { sequence, thread: 3, .. } if sequence == "undo"))
            .count(),
        5
    );
    // The same `BrickKill` as the hammer: break sound and debris pop.
    assert!(cues.iter().any(|c| matches!(&c.kind,
        CueKind::BrickKill { brick, force, .. } if *brick == id && *force > 0.)));
    assert_eq!(
        tool(&mut s, owner, 18, ToolAction::UndoBrick).unwrap(),
        Reply::Undone(None)
    );
}
}

on_both! {
fn nested_events_return_to_wrench_without_overwriting_concurrent_properties(f: &Fixture) {
    for concurrent_change in [false, true] {
        let (mut s, owner, id) = setup(f);
        inspect(&mut s, owner, 2, InspectMode::Wrench);
        if concurrent_change {
            s.edit_brick(owner, id, Edit::Name(Some("other editor".into())))
                .unwrap();
        }
        inspect(&mut s, owner, 4, InspectMode::Events);
        tool(
            &mut s,
            owner,
            5,
            ToolAction::SetEvents {
                brick: id,
                events: vec![EventRow {
                    conditions: vec![],
            preserved: None,
                    enabled: true,
                    input: "onActivate".into(),
                    delay_ms: 0,
                    target: EventTarget::Slot(bri_events::Slot::SelfBrick),
                    output: "setColor".into(),
                    params: vec![EventValue::Color(1)],
                }],
            },
        )
        .unwrap();
        let before = s.snapshot().world;
        let result = tool(
            &mut s,
            owner,
            6,
            ToolAction::SetWrench {
                brick: id,
                properties: properties(),
            },
        );
        assert_eq!(result.is_err(), concurrent_change);
        if concurrent_change {
            assert_eq!(s.snapshot().world, before);
        } else {
            assert_eq!(s.simulation().state().bricks[&id].events.len(), 1);
        }
    }
    // Cancelling Events emits no network edit and still leaves Wrench usable.
    let (mut s, owner, id) = setup(f);
    inspect(&mut s, owner, 2, InspectMode::Wrench);
    inspect(&mut s, owner, 3, InspectMode::Events);
    tool(
        &mut s,
        owner,
        4,
        ToolAction::SetWrench {
            brick: id,
            properties: properties(),
        },
    )
    .unwrap();
}
}

on_both! {
fn spray_paint_temporarily_recolours_the_body_band_it_hits(f: &Fixture) {
    let mut s = session(f, vec![], false);
    s.set_tool_catalog(catalog()).unwrap();
    s.set_avatar_catalog(
        serde_json::from_value(serde_json::json!({
            "schema_version": 1, "id": "test", "rig": "rig.json", "rig_sha256": "",
            "parts": {"hat": ["none"], "accent": ["none"], "pack": ["none"],
                "secondpack": ["none"], "chest": ["chest"], "hip": ["pants"],
                "rarm": ["rarm"], "larm": ["larm"], "rhand": ["rhand"],
                "lhand": ["lhand"], "rleg": ["rshoe"], "lleg": ["lshoe"]},
            "accents_allowed": {}, "faces": ["smiley"],
            "decals": ["AAA-None", "Alyx"], "surfaces": {}, "textures": {
                "smiley": {"file": "smiley.png", "sha256": "", "source": "", "width": 1, "height": 1},
                "Alyx": {"file": "alyx.png", "sha256": "", "source": "", "width": 1, "height": 1},
                "AAA-None": {"file": "none.png", "sha256": "", "source": "", "width": 1, "height": 1}},
            "defaults": {"parts": {}, "colors": {"head": [1.0, 0.88, 0.61, 1.0],
                "torso": [0.9, 0.9, 0.9, 1.0], "hat": [1.0, 1.0, 0.0, 1.0],
                "accent": [0.0, 0.2, 0.64, 0.7], "pack": [0.0, 0.4, 0.8, 1.0],
                "secondpack": [0.0, 1.0, 0.0, 1.0], "hip": [0.0, 0.0, 1.0, 1.0],
                "rarm": [0.9, 0.0, 0.0, 1.0], "larm": [0.9, 0.0, 0.0, 1.0],
                "rhand": [1.0, 0.88, 0.61, 1.0], "lhand": [1.0, 0.88, 0.61, 1.0],
                "rleg": [0.0, 0.0, 1.0, 1.0], "lleg": [0.0, 0.0, 1.0, 1.0]}, "face": "smiley", "decal": "Alyx"}
        }))
        .unwrap(),
    )
    .unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let guest = s
        .join("Guest".into(), Vec3::new(0.0, 0.05, -3.0), false)
        .unwrap();
    let own = s.avatars()[&guest].clone();
    s.command(owner, 1, Command::UseSprayCan { color: 1 })
        .unwrap();
    aim(&mut s, owner, 2, [0.0, 1.45, -3.0]);
    hold_still(&mut s, guest);
    s.command(owner, 3, Command::WeaponTrigger { down: true })
        .unwrap();
    for _ in 0..12 {
        s.step().unwrap();
    }
    s.command(owner, 4, Command::WeaponTrigger { down: false })
        .unwrap();
    for _ in 0..12 {
        s.step().unwrap();
    }
    // Chest band: the paint colour at full alpha, no decal; legs unchanged.
    let painted = &s.avatars()[&guest];
    assert_eq!(painted.colors["torso"], [0.2, 0.3, 0.4, 1.0]);
    assert_eq!(painted.colors["larm"], [0.2, 0.3, 0.4, 1.0]);
    assert_eq!(painted.colors["lleg"], own.colors["lleg"]);
    assert_eq!(painted.decal, "AAA-None");
    assert_eq!(s.avatars()[&owner], own);
    s.take_cues();
    for _ in 0..200 {
        s.step().unwrap();
    }
    assert_ne!(
        s.avatars()[&guest],
        own,
        "held for 2000 ms after the last hit"
    );
    for _ in 0..60 {
        s.step().unwrap();
    }
    // `ClearTempColor`: the paint's splash at the player, own colours back.
    assert_eq!(s.avatars()[&guest], own);
    assert!(s.take_cues().iter().any(|c| matches!(&c.kind,
        bri_sim::presentation::CueKind::WeaponEffect { definition, scale, .. }
            if definition == "color1PaintExplosion" && *scale == 2.0)));
}
}

on_both! {
fn hammering_a_brick_fires_its_on_tool_break_events(f: &Fixture) {
    let mut s = session(f, vec![], false);
    s.set_event_catalog(f.events(), Vec::new()).unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.), false)
        .unwrap();
    let id = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    aim(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    s.edit_brick(
        owner,
        id,
        Edit::Events(vec![EventRow {
            conditions: vec![],
            preserved: None,
            enabled: true,
            input: "onToolBreak".into(),
            delay_ms: 0,
            target: EventTarget::Slot(bri_events::Slot::Client),
            output: "CenterPrint".into(),
            params: vec![EventValue::Text("Broken".into()), EventValue::Int(2)],
        }]),
    )
    .unwrap();
    swing(&mut s, owner, 2, 0).unwrap();
    assert!(!s.simulation().state().bricks.contains_key(&id));
    assert_eq!(center_prints(&mut s, owner), ["Broken"]);
}
}

on_both! {
fn player_datablock_and_scale_events_reshape_the_player(f: &Fixture) {
    let mut s = session(f, vec![], false);
    s.set_event_catalog(f.events(), Vec::new()).unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.), false)
        .unwrap();
    let id = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    let row = |output: &str, params| EventRow {
        conditions: vec![],
            preserved: None,
        enabled: true,
        input: "onPlayerTouch".into(),
        delay_ms: 0,
        target: EventTarget::Slot(bri_events::Slot::Player),
        output: output.into(),
        params,
    };
    s.edit_brick(
        owner,
        id,
        Edit::Events(vec![
            row(
                "ChangeDatablock",
                vec![EventValue::Datablock(Some("PlayerQuakeArmor".into()))],
            ),
            row("setPlayerScale", vec![EventValue::Float(1.5)]),
        ]),
    )
    .unwrap();
    s.fire_brick_input(id, "onPlayerTouch", Some(owner));
    s.step().unwrap();
    let player = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap();
    assert_eq!(
        player.archetype,
        bri_sim::player_types::PlayerType::Quake.archetype(),
        "{:?}",
        s.take_event_diagnostics()
    );
    assert_eq!(player.scale, 1.5);
}
}

on_both! {
fn tutorial_layout_swaps_keep_their_item_spawns_between_publishes(f: &Fixture) {
    use bri_sim::tutorial::{MAP_ID, TutorialMap, Zone, ZoneKind};
    let mut s = session_on(f, vec![], false, MAP_ID);
    s.set_tool_catalog(catalog()).unwrap();
    s.set_item_bounds(core_tool_bounds()).unwrap();
    // Part 1 carries the break room's hammer on a brick (`+-ITEM Hammer`).
    let mut part1 = World::new("Tutorial_Part1".into(), MAP_ID.into(), vec![[1.0; 4]]);
    let mut pad = Brick::new(ContentRef::Resolved("plate".into()), [4.5, 0.1, -4.25], 0);
    pad.item_spawn.item = Some(ContentRef::Resolved(bri_weapons::CORE_TOOLS[0].into()));
    part1.bricks.insert(1, pad);
    part1.next_brick_id = 2;
    // Spawning in the look zone installs part 1, on a tutorial tick.
    let look = Zone {
        kind: ZoneKind::Look,
        goal: "Look".into(),
        bind: String::new(),
        task: String::new(),
        min: Vec3::new(-2.0, -1.0, -2.0),
        max: Vec3::new(2.0, 3.0, 2.0),
    };
    s.set_tutorial(TutorialMap {
        zones: vec![look],
        look_target: Vec3::new(0.0, 0.0, 10.0),
        part1,
        part2: World::new("Tutorial_Part2".into(), MAP_ID.into(), vec![[1.0; 4]]),
        targets: vec![],
        targets_end_ms: 0,
        target_collision: Default::default(),
    })
    .unwrap();
    s.join("Pupil".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    // The network server publishes, and so clears the dirty bricks, after
    // every sixth tick: the same ticks the tutorial's rules run on.
    for _ in 0..60 {
        s.step().unwrap();
        if s.simulation().state().tick.is_multiple_of(6) {
            s.take_dirty();
        }
    }
    let items: Vec<String> = s
        .weapon_view()
        .static_items
        .into_iter()
        .map(|i| i.item)
        .collect();
    assert_eq!(items, [bri_weapons::CORE_TOOLS[0]]);
}
}

on_both! {
fn admin_destructo_wand_breaks_bricks_from_afar(f: &Fixture) {
    use bri_admin::{Action, Request};
    let mut s = session(f, vec![], false);
    s.set_tool_catalog(catalog()).unwrap();
    let admin = s
        .join("Admin".into(), Vec3::new(0.5, 0.05, 0.0), true)
        .unwrap();
    let id = plant(&mut s, admin, 1, [0.5, 0.1, -3.25]);
    aim(&mut s, admin, 1, [0.5, 0.1, -3.25]);
    s.command(
        admin,
        2,
        Command::Admin(Request::new(Action::DestructoWand)),
    )
    .unwrap();
    assert_eq!(
        s.weapon_view().images[&admin][0].image,
        "v20.image.adminwandimage"
    );
    hold_still(&mut s, admin);
    s.command(admin, 3, Command::WeaponTrigger { down: true })
        .unwrap();
    for _ in 0..40 {
        s.step().unwrap();
    }
    assert!(!s.simulation().state().bricks.contains_key(&id));
}
}

fn bricks(s: &Session) -> Vec<u64> {
    s.simulation().state().bricks.keys().copied().collect()
}

on_both! {
fn hammer_only_breaks_bricks_that_hold_nothing_up(f: &Fixture) {
    let mut s = session(f, vec![], false);
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.0), false)
        .unwrap();
    let low = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    let high = plant(&mut s, owner, 2, [0.5, 0.3, -3.25]);
    assert!(s.simulation().will_cause_chain_kill(low).unwrap());
    assert!(!s.simulation().will_cause_chain_kill(high).unwrap());
    // Swinging at the bottom of the stack does nothing, silently.
    aim(&mut s, owner, 3, [0.5, 0.1, -3.01]);
    swing(&mut s, owner, 4, 0).unwrap();
    assert_eq!(bricks(&s), vec![low, high]);
    assert!(center_prints(&mut s, owner).is_empty());
    // Top first, then the one underneath.
    aim(&mut s, owner, 5, [0.5, 0.3, -3.01]);
    swing(&mut s, owner, 6, 0).unwrap();
    assert_eq!(bricks(&s), vec![low]);
    aim(&mut s, owner, 7, [0.5, 0.1, -3.01]);
    swing(&mut s, owner, 8, 0).unwrap();
    assert!(bricks(&s).is_empty());
}
}

on_both! {
fn hammer_breaks_a_brick_whose_load_is_still_held_up_elsewhere(f: &Fixture) {
    let mut s = session(f, vec![], false);
    let owner = s
        .join("Builder".into(), Vec3::new(1.0, 0.05, 0.0), false)
        .unwrap();
    let left = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    let right = plant(&mut s, owner, 2, [1.5, 0.1, -3.25]);
    let bridge = plant(&mut s, owner, 3, [1.0, 0.3, -3.25]);
    assert!(!s.simulation().will_cause_chain_kill(left).unwrap());
    aim(&mut s, owner, 4, [0.25, 0.1, -3.01]);
    swing(&mut s, owner, 5, 0).unwrap();
    assert_eq!(bricks(&s), vec![right, bridge]);
    // Now the right post alone carries the bridge.
    aim(&mut s, owner, 6, [1.75, 0.1, -3.01]);
    swing(&mut s, owner, 7, 0).unwrap();
    assert_eq!(bricks(&s), vec![right, bridge]);
}
}

on_both! {
fn wand_breaks_anywhere_and_the_stranded_bricks_above_die_with_it(f: &Fixture) {
    let mut s = session(f, vec![], false);
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.0), false)
        .unwrap();
    plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    let middle = plant(&mut s, owner, 2, [0.5, 0.3, -3.25]);
    let top = plant(&mut s, owner, 3, [0.5, 0.5, -3.25]);
    assert_eq!(s.simulation().stranded_by(middle).unwrap(), vec![top]);
    let low = bricks(&s)[0];
    assert_eq!(s.simulation().stranded_by(low).unwrap(), vec![middle, top]);
    s.use_wand(owner).unwrap();
    aim(&mut s, owner, 4, [0.5, 0.1, -3.01]);
    hold_still(&mut s, owner);
    s.command(owner, 5, Command::WeaponTrigger { down: true })
        .unwrap();
    for _ in 0..40 {
        s.step().unwrap();
    }
    assert!(bricks(&s).is_empty());
}
}

on_both! {
fn undoing_a_plant_that_holds_up_untrusting_bricks_is_refused(f: &Fixture) {
    let mut s = session(f, vec![], false);
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.0), false)
        .unwrap();
    // An administrator may build on anyone's bricks.
    let guest = s
        .join("Guest".into(), Vec3::new(3.0, 0.05, 1.0), true)
        .unwrap();
    let low = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    let high = plant(&mut s, guest, 1, [0.5, 0.3, -3.25]);
    center_prints(&mut s, owner);
    tool(&mut s, owner, 2, ToolAction::UndoBrick).unwrap();
    assert_eq!(bricks(&s), vec![low, high]);
    assert_eq!(
        center_prints(&mut s, owner),
        vec!["Guest does not trust you enough to do that.".to_string()]
    );
}
}

on_both! {
/// `hammerImage::onHitObject` lets a swing through without trust when the
/// brick stands in the swinger's own stack (`stackBL_ID`): you may clear
/// what others built on your bricks, though not their own stacks.
fn the_hammer_breaks_others_bricks_built_on_your_stack(f: &Fixture) {
    let mut s = session(f, vec![], false);
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.0), false)
        .unwrap();
    // An administrator may build on anyone's bricks.
    let guest = s
        .join("Guest".into(), Vec3::new(3.0, 0.05, 1.0), true)
        .unwrap();
    let low = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    let high = plant(&mut s, guest, 1, [0.5, 0.3, -3.25]);
    let theirs = plant(&mut s, guest, 2, [2.5, 0.1, -3.25]);
    assert_eq!(s.simulation().stack_owner(high), Some(owner));
    center_prints(&mut s, owner);
    aim(&mut s, owner, 3, [0.5, 0.3, -3.01]);
    swing(&mut s, owner, 4, 0).unwrap();
    assert_eq!(bricks(&s), vec![low, theirs]);
    assert!(center_prints(&mut s, owner).is_empty());
    // The guest's own stack still needs their trust.
    aim(&mut s, owner, 5, [2.5, 0.1, -3.01]);
    swing(&mut s, owner, 6, 0).unwrap();
    assert_eq!(bricks(&s), vec![low, theirs]);
    assert_eq!(
        center_prints(&mut s, owner),
        vec!["Guest does not trust you enough to do that.".to_string()]
    );
}
}

on_both! {
/// v20's `indestructable` (spawn points, vehicle spawns) only keeps
/// explosions off a brick: a builder who is not an administrator hammers or
/// undoes their own like any other (playtest a20).
fn builders_hammer_and_undo_their_own_indestructible_bricks(f: &Fixture) {
    let mut s = session(f, vec![], false);
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.0), false)
        .unwrap();
    let plant_sturdy = |s: &mut Session, seq| {
        let Reply::Planted(id) = s
            .command(
                owner,
                seq,
                Command::Plant {
                    definition: "sturdy_plate".into(),
                    position: [0.5, 0.1, -3.25],
                    quarter_turns: 0,
                    color: 0,
                },
            )
            .unwrap()
        else {
            panic!("expected plant")
        };
        id
    };
    plant_sturdy(&mut s, 1);
    aim(&mut s, owner, 2, [0.5, 0.1, -3.01]);
    swing(&mut s, owner, 3, 0).unwrap();
    assert_eq!(bricks(&s), Vec::<u64>::new());
    let again = plant_sturdy(&mut s, 4);
    assert_eq!(
        tool(&mut s, owner, 5, ToolAction::UndoBrick).unwrap(),
        Reply::Undone(Some(again))
    );
    assert_eq!(bricks(&s), Vec::<u64>::new());
}
}

on_both! {
fn random_brick_color_paints_each_plant_from_v20s_six(f: &Fixture) {
    let mut s = session(f, vec![], false);
    s.set_server_settings(bri_admin::ServerSettings {
        random_brick_color: true,
        bricks_per_second: 1000,
        ..Default::default()
    })
    .unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(-3.0, 0.05, 0.0), false)
        .unwrap();
    // Each plant gives the temp brick its next colour, shown on the ghost
    // and taken by the next brick; the first takes the builder's paint.
    let mut next = None;
    let mut colors = std::collections::BTreeSet::new();
    for i in 0..24 {
        let id = plant(&mut s, owner, i + 1, [i as f32 + 0.5, 0.1, -2.25]);
        let color = s.simulation().state().bricks[&id].color;
        assert_eq!(color, next.unwrap_or(0), "brick {i}");
        next = s
            .take_private_notices()
            .into_iter()
            .find_map(|(to, n)| match n {
                bri_sim::session::Notice::TempBrickColor(c) if to == owner => Some(c),
                _ => None,
            });
        colors.insert(next.expect("a next colour"));
    }
    assert!(colors.is_subset(&[0, 1, 3, 4, 5, 7].into()), "{colors:?}");
    assert!(colors.len() > 1, "{colors:?}");
}
}

on_both! {
fn an_input_past_its_owners_schedule_quota_runs_nothing_and_says_why(f: &Fixture) {
    let brick = Brick::new(ContentRef::Resolved("plate".into()), [0.5, 0.1, -3.25], 7);
    let mut s = session(f, vec![brick], false);
    s.set_tool_catalog(catalog()).unwrap();
    let mut settings = bri_admin::ServerSettings::default();
    settings.per_player.schedules = 10;
    s.set_server_settings(settings).unwrap();
    let owner = s
        .join("Admin".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    aim(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    let row = |color| EventRow {
        conditions: vec![],
            preserved: None,
        enabled: true,
        input: "onActivate".into(),
        delay_ms: 1000,
        target: EventTarget::Slot(bri_events::Slot::SelfBrick),
        output: "setColor".into(),
        params: vec![EventValue::Color(color)],
    };
    let too_many = |s: &mut Session| {
        s.take_private_notices().iter().any(|(_, n)| {
            matches!(n, bri_sim::session::Notice::Center { text, .. }
                if text.ends_with("Too many events at once!\n(onActivate)"))
        })
    };
    // Eleven rows never fit a quota of ten.
    s.edit_brick(owner, 1, Edit::Events((0..11).map(|_| row(1)).collect()))
        .unwrap();
    s.command(owner, 2, Command::Activate).unwrap();
    for _ in 0..130 {
        s.step().unwrap();
    }
    assert_eq!(s.simulation().state().bricks[&1].color, 0);
    assert!(too_many(&mut s));
    // Ten fit once; a second click while they wait does not.
    s.edit_brick(owner, 1, Edit::Events((0..10).map(|_| row(1)).collect()))
        .unwrap();
    s.command(owner, 3, Command::Activate).unwrap();
    s.step().unwrap();
    assert!(!too_many(&mut s));
    s.command(owner, 4, Command::Activate).unwrap();
    s.step().unwrap();
    assert!(too_many(&mut s));
    for _ in 0..130 {
        s.step().unwrap();
    }
    assert_eq!(s.simulation().state().bricks[&1].color, 1);
}
}

on_both! {
fn a_full_environment_quota_leaves_a_new_light_and_emitter_off(f: &Fixture) {
    let (mut s, owner, id) = setup(f);
    // v20 clamps the quota to at least 20: ten other bricks fill it.
    let mut settings = bri_admin::ServerSettings::default();
    settings.per_player.environment = 0;
    s.set_server_settings(settings).unwrap();
    for i in 0..10u64 {
        for _ in 0..120 {
            s.step().unwrap();
        }
        let other = plant(&mut s, owner, 10 + i, [-4.5 + i as f32, 0.1, -5.25]);
        s.edit_brick(owner, other, Edit::Properties(properties()))
            .unwrap();
    }
    aim(&mut s, owner, 30, [0.5, 0.1, -3.25]);
    inspect(&mut s, owner, 31, InspectMode::Wrench);
    tool(
        &mut s,
        owner,
        32,
        ToolAction::SetWrench {
            brick: id,
            properties: properties(),
        },
    )
    .unwrap();
    let brick = &s.simulation().state().bricks[&id];
    assert_eq!(
        brick.name.as_deref(),
        Some("lamp"),
        "the rest still applies"
    );
    assert!(brick.light.is_none());
    assert!(brick.emitter.as_ref().is_none_or(|e| e.asset.is_none()));
    // On a LAN server the larger LAN quota has room.
    s.set_lan_host(true);
    inspect(&mut s, owner, 33, InspectMode::Wrench);
    tool(
        &mut s,
        owner,
        34,
        ToolAction::SetWrench {
            brick: id,
            properties: properties(),
        },
    )
    .unwrap();
    assert!(s.simulation().state().bricks[&id].light.is_some());
}
}

/// The sounds and effects of one brick-breaking hit, in order: v20's hammer
/// plays `hammerHitSound`, the Destructo Wand its explosion's `wandHitSound`,
/// and each `killBrick` one brick death (the client's break sound).
fn hit_cues(s: &mut Session) -> Vec<String> {
    use bri_sim::presentation::CueKind;
    s.take_cues()
        .into_iter()
        .filter_map(|c| match c.kind {
            CueKind::WeaponSound { profile } => Some(format!("sound {profile}")),
            CueKind::WeaponEffect {
                definition,
                image: None,
                ..
            } => Some(format!("explosion {definition}")),
            CueKind::BrickKill { brick, .. } => Some(format!("kill {brick}")),
            _ => None,
        })
        .collect()
}

on_both! {
fn destructo_wand_breaks_a_brick_like_the_hammer_with_its_own_hit_sound(f: &Fixture) {
    use bri_admin::{Action, Request};
    let mut s = session(f, vec![], false);
    s.set_tool_catalog(catalog()).unwrap();
    let admin = s
        .join("Admin".into(), Vec3::new(0.5, 0.05, 0.0), true)
        .unwrap();
    // Let the spawn burst finish first.
    while !s.snapshot().weapons.projectiles.is_empty() {
        s.step().unwrap();
    }
    let first = plant(&mut s, admin, 1, [0.5, 0.1, -3.25]);
    aim(&mut s, admin, 2, [0.5, 0.1, -3.01]);
    s.take_cues();
    swing(&mut s, admin, 3, 0).unwrap();
    assert_eq!(
        hit_cues(&mut s),
        [
            "explosion hammerExplosion".to_string(),
            "sound hammerHitSound".into(),
            format!("kill {first}"),
        ]
    );
    let second = plant(&mut s, admin, 4, [0.5, 0.1, -3.25]);
    aim(&mut s, admin, 5, [0.5, 0.1, -3.25]);
    s.command(
        admin,
        6,
        Command::Admin(Request::new(Action::DestructoWand)),
    )
    .unwrap();
    hold_still(&mut s, admin);
    s.take_cues();
    s.command(admin, 7, Command::WeaponTrigger { down: true })
        .unwrap();
    for _ in 0..20 {
        s.step().unwrap();
    }
    assert_eq!(
        hit_cues(&mut s),
        [
            "explosion AdminWandExplosion".to_string(),
            "sound wandHitSound".into(),
            format!("kill {second}"),
        ]
    );
}
}

on_both! {
fn a_joining_player_learns_the_music_the_host_offers(f: &Fixture) {
    let mut s = session(f, vec![], false);
    let mut first = catalog();
    first.sounds.insert("music/first".into());
    s.set_tool_catalog(first).unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let offered = |s: &mut Session| {
        s.take_private_notices().into_iter().filter_map(|(to, n)| {
            match n {
                bri_sim::session::Notice::MusicTracks(tracks) if to == owner => Some(tracks),
                _ => None,
            }
        }).collect::<Vec<_>>()
    };
    assert_eq!(offered(&mut s), vec![["music/first".into()].into()]);
    s.disconnect(owner).unwrap();
    s.take_private_notices();
    let mut second = catalog();
    second.sounds.insert("music/second".into());
    s.set_tool_catalog(second.clone()).unwrap();
    assert!(offered(&mut s).is_empty());
    s.resume(owner, Vec3::new(0.0, 0.05, 0.0)).unwrap();
    assert_eq!(offered(&mut s), vec![["music/second".into()].into()]);
    s.set_tool_catalog(catalog()).unwrap();
    assert_eq!(offered(&mut s), vec![Default::default()]);
    s.set_tool_catalog(catalog()).unwrap();
    assert!(offered(&mut s).is_empty(), "unchanged catalogs do not resend");
    let mut next = session_on(f, vec![], false, "next-map");
    next.set_tool_catalog(second).unwrap();
    next.adopt(s, owner).unwrap();
    assert_eq!(offered(&mut next), vec![["music/second".into()].into()]);
}
}

on_both! {
/// `Player::ActivateStuff` reaches bricks within `$Game::BrickActivateRange`
/// (5) times the player's scale, along a 10-unit ray: a player made bigger
/// clicks a button a normal one cannot reach.
fn a_click_reaches_bricks_five_units_times_the_players_scale(f: &Fixture) {
    let mut s = session(f, vec![], false);
    s.set_event_catalog(f.events(), Vec::new()).unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.), false)
        .unwrap();
    let button = plant(&mut s, owner, 1, [0.5, 0.1, -6.25]);
    let grow = plant(&mut s, owner, 2, [4.5, 0.1, 0.25]);
    let row = |input: &str, target, output: &str, params| EventRow {
        conditions: vec![],
        preserved: None,
        enabled: true,
        input: input.into(),
        delay_ms: 0,
        target,
        output: output.into(),
        params,
    };
    s.edit_brick(
        owner,
        button,
        Edit::Events(vec![row(
            "onActivate",
            EventTarget::Slot(bri_events::Slot::SelfBrick),
            "setColor",
            vec![EventValue::Color(1)],
        )]),
    )
    .unwrap();
    s.edit_brick(
        owner,
        grow,
        Edit::Events(vec![row(
            "onPlayerTouch",
            EventTarget::Slot(bri_events::Slot::Player),
            "setPlayerScale",
            vec![EventValue::Float(2.0)],
        )]),
    )
    .unwrap();
    let activate = |s: &mut Session, seq| {
        aim(s, owner, seq, [0.5, 0.1, -6.25]);
        let reply = s.command(owner, seq, Command::Activate).unwrap();
        for _ in 0..4 {
            s.step().unwrap();
        }
        reply
    };
    // About 6.4 units from a standing eye: past 5, short of the ray's 10.
    assert_eq!(activate(&mut s, 3), Reply::Activated(None));
    assert_eq!(s.simulation().state().bricks[&button].color, 0);
    s.fire_brick_input(grow, "onPlayerTouch", Some(owner));
    s.step().unwrap();
    let scale = s.snapshot().players.into_iter().find(|p| p.owner == owner).unwrap().scale;
    assert_eq!(scale, 2.0, "{:?}", s.take_event_diagnostics());
    assert_eq!(activate(&mut s, 4), Reply::Activated(Some(button)));
    assert_eq!(s.simulation().state().bricks[&button].color, 1);
}
}

on_both! {
/// The wrench, events and printer dialogs guard only what they show: a
/// brick its own events recolour while the dialog is open (a flashing relay
/// loop) still takes the edit.
fn a_brick_recoloured_while_its_dialog_is_open_still_takes_the_edit(f: &Fixture) {
    let (mut s, owner, id) = setup(f);
    inspect(&mut s, owner, 2, InspectMode::Wrench);
    s.edit_brick(owner, id, Edit::Color(1)).unwrap();
    tool(
        &mut s,
        owner,
        3,
        ToolAction::SetWrench {
            brick: id,
            properties: properties(),
        },
    )
    .unwrap();
    assert_eq!(s.simulation().state().bricks[&id].name.as_deref(), Some("lamp"));
    inspect(&mut s, owner, 4, InspectMode::Wrench);
    inspect(&mut s, owner, 5, InspectMode::Events);
    s.edit_brick(owner, id, Edit::Color(0)).unwrap();
    tool(
        &mut s,
        owner,
        6,
        ToolAction::SetEvents {
            brick: id,
            events: vec![],
        },
    )
    .unwrap();
    assert_eq!(s.simulation().state().bricks[&id].color, 0);
}
}
