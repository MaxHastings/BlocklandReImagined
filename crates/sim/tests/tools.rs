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

fn session(bricks: Vec<Brick>, wall: bool) -> Session {
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
    let mut world = World::new(
        "Tools".into(),
        "test".into(),
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
    s.set_weapon_pack(weapon_pack()).unwrap();
    s.set_event_catalog(bri_events::testing::catalog(), Vec::new())
        .unwrap();
    s
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

#[test]
fn tools_swing_only_when_held_and_switching_or_dropping_revokes_the_dialog() {
    let mut s = session(vec![], false);
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
    assert!(
        s.command(owner, 2, Command::WeaponTrigger { down: true })
            .unwrap_err()
            .to_string()
            .contains("No weapon image")
    );
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

#[test]
fn swings_use_the_current_aim_and_respect_brick_trust() {
    use bri_sim::session::ActionAim;
    let mut s = session(vec![], false);
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

#[test]
fn swinging_at_nothing_or_the_ground_is_not_an_error_and_plays_v20_effects() {
    use bri_sim::presentation::CueKind;
    let mut s = session(vec![], false);
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.0), false)
        .unwrap();
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

#[test]
fn spray_cans_mount_in_hand_and_paint_by_projectile() {
    let (mut s, owner, id) = setup();
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
    let d = Vec3::from(target) - p.eye(&PlayerTuning::default());
    let sequence = move_sequence(s);
    s.movement(
        owner,
        sequence,
        MoveInput {
            yaw: d.x.atan2(-d.z),
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
fn setup() -> (Session, u64, u64) {
    let mut s = session(vec![], false);
    s.set_tool_catalog(catalog()).unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(0.0, 0.05, 0.0), false)
        .unwrap();
    let id = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    aim(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    (s, owner, id)
}

#[test]
fn wrench_changes_are_atomic_and_nonraycasting_bricks_remain_editable() {
    let (mut s, owner, id) = setup();
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

#[test]
fn wrench_item_catalog_ranges_and_clear_are_authoritative_and_atomic() {
    let (mut s, owner, id) = setup();
    inspect(&mut s, owner, 2, InspectMode::Wrench);
    let before = s.snapshot().world;
    for (offset, item_spawn) in [
        bri_world::ItemSpawn {
            item: Some(ContentRef::Resolved("v20.weapon.forged".into())),
            ..Default::default()
        },
        bri_world::ItemSpawn {
            item: Some(ContentRef::Unresolved {
                namespace: "item_ui".into(),
                name: "Gun".into(),
            }),
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
    let json = serde_json::to_value(properties()).unwrap();
    let mut legacy = json.as_object().unwrap().clone();
    legacy.remove("item_spawn");
    let old: WrenchProperties = serde_json::from_value(legacy.into()).unwrap();
    assert_eq!(old.item_spawn, bri_world::ItemSpawn::default());
}

#[test]
fn native_item_allowlist_installation_is_atomic_and_bounded() {
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

#[test]
fn printing_uses_catalog_aspect_letters_default_and_inspection_identity() {
    let (mut s, owner, id) = setup();
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

#[test]
fn event_binding_checks_cannot_be_bypassed_and_opaque_source_is_preserved() {
    let mut brick = Brick::new(ContentRef::Resolved("plate".into()), [0.5, 0.1, -3.25], 7);
    brick.source_records.push(SourceRecord {
        line: 12,
        text: "+-EVENT\tunsupported output".into(),
        diagnostic: Some("native adapter required".into()),
    });
    let source = brick.source_records.clone();
    let mut s = session(vec![brick], false);
    s.set_tool_catalog(catalog()).unwrap();
    let owner = s
        .join("Admin".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    aim(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    let event = EventRow {
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
        tool(
            &mut s,
            owner,
            3,
            ToolAction::SetEvents {
                brick: 1,
                events: vec![event.clone()]
            }
        )
        .is_err()
    );
    assert!(
        s.edit_brick(owner, 1, Edit::Events(vec![event.clone()]))
            .is_err()
    );
    assert_eq!(s.snapshot().world, before);
    let event = EventRow {
        output: "setColor".into(),
        params: vec![EventValue::Color(1)],
        ..event
    };
    tool(
        &mut s,
        owner,
        5,
        ToolAction::SetEvents {
            brick: 1,
            events: vec![event.clone()],
        },
    )
    .unwrap();
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

#[test]
fn hammer_ranges_and_map_occlusion_are_authoritative() {
    for (position, spawn, succeeds) in [
        ([0.5, 2.5, -4.75], Vec3::new(0.5, 0.1, 0.0), true),
        ([0.5, 2.5, -5.75], Vec3::new(0.5, 0.1, 0.0), false),
        ([0.5, 0.1, -0.25], Vec3::new(0.5, 3.1, -0.25), true),
        ([0.5, 0.1, -0.25], Vec3::new(0.5, 3.4, -0.25), false),
    ] {
        let mut brick = Brick::new(ContentRef::Resolved("plate".into()), position, 0);
        brick.raycast = false;
        let mut s = session(vec![brick], false);
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
    let mut s = session(
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

#[test]
fn undo_is_owner_scoped_lifo_spends_removed_bricks_and_survives_authenticated_resume() {
    let (mut s, owner, first) = setup();
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

#[test]
fn undo_retains_only_the_511_entries_of_a_512_slot_queue() {
    let mut s = session(vec![], false);
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

/// `serverCmdUndoBrick` walks one mixed stack: prints, shape FX, colour FX,
/// spray paint, then the plant, which breaks like a hammered brick.
#[test]
fn undo_reverts_paint_and_print_then_breaks_the_plant() {
    use bri_sim::presentation::CueKind;
    let (mut s, owner, id) = setup();
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

#[test]
fn nested_events_return_to_wrench_without_overwriting_concurrent_properties() {
    for concurrent_change in [false, true] {
        let (mut s, owner, id) = setup();
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
    let (mut s, owner, id) = setup();
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

#[test]
fn spray_paint_temporarily_recolours_the_body_band_it_hits() {
    let mut s = session(vec![], false);
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

#[test]
#[ignore = "requires the converted native event catalog"]
fn hammering_a_brick_fires_its_on_tool_break_events() {
    let mut s = session(vec![], false);
    let catalog = bri_events::Catalog::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/events-pack-002/catalog.json"),
    )
    .unwrap();
    s.set_event_catalog(catalog, Vec::new()).unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.), false)
        .unwrap();
    let id = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    aim(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    s.edit_brick(
        owner,
        id,
        Edit::Events(vec![EventRow {
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

#[test]
#[ignore = "requires the converted native event catalog"]
fn player_datablock_and_scale_events_reshape_the_player() {
    let mut s = session(vec![], false);
    let catalog = bri_events::Catalog::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/events-pack-002/catalog.json"),
    )
    .unwrap();
    s.set_event_catalog(catalog, Vec::new()).unwrap();
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.), false)
        .unwrap();
    let id = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    let row = |output: &str, params| EventRow {
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
        player.datablock,
        bri_sim::player_types::PlayerType::Quake,
        "{:?}",
        s.take_event_diagnostics()
    );
    assert_eq!(player.scale, 1.5);
}
