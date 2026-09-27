use bri_content::{
    brick::Brick as Mesh,
    collision::{CollisionBody, Part},
};
use bri_sim::{
    definitions::{Definition, Definitions},
    player::{MoveInput, PlayerTuning},
    session::{
        Command, InspectMode, Reply, Session, ToolAction, ToolCatalog, UNDO_PLANT_LIMIT,
        WrenchProperties,
    },
    simulation::Simulation,
};
use bri_world::{
    Action, Brick, ContentRef, Event, Input, SourceRecord, Target, World, authority::Edit,
};
use glam::Vec3;
use rapier3d::prelude::*;

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
                requires_behavior_adapter: false,
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
    Session::new(Simulation::new(world, definitions, colliders).unwrap())
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
    }
}

#[test]
fn remote_tool_use_requires_selected_inventory_and_switch_or_drop_revokes_inspection() {
    let mut s = session(vec![], false);
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
        s.command(owner, 2, Command::Tool(ToolAction::Hammer))
            .unwrap_err()
            .to_string()
            .contains("not equipped")
    );
    s.command(owner, 3, Command::EquipTool { slot: Some(1) })
        .unwrap();
    let inspect = Command::Tool(ToolAction::Inspect {
        mode: InspectMode::Wrench,
    });
    s.command(owner, 4, inspect.clone()).unwrap();
    s.command(owner, 5, Command::EquipTool { slot: Some(2) })
        .unwrap();
    s.command(owner, 6, Command::EquipTool { slot: Some(1) })
        .unwrap();
    let properties = WrenchProperties {
        name: Some("changed".into()),
        light: None,
        emitter: None,
        emitter_direction: 0,
        item_spawn: Default::default(),
        raycast: true,
        colliding: true,
        visible: true,
    };
    let edit = Command::Tool(ToolAction::SetWrench {
        brick: id,
        properties: properties.clone(),
    });
    assert!(
        s.command(owner, 7, edit.clone())
            .unwrap_err()
            .to_string()
            .contains("Inspect the brick")
    );
    s.command(owner, 8, inspect).unwrap();
    s.command(owner, 9, Command::DropTool { slot: 1 }).unwrap();
    assert!(
        s.command(owner, 10, edit)
            .unwrap_err()
            .to_string()
            .contains("not equipped")
    );
    assert!(
        s.command(
            owner,
            11,
            Command::Edit {
                brick: id,
                edit: Edit::Properties(properties)
            }
        )
        .is_err()
    );
    assert!(s.command(owner, 12, Command::Remove { brick: id }).is_err());
    assert_eq!(s.snapshot().world, before);
    s.command(owner, 13, Command::EquipTool { slot: Some(0) })
        .unwrap();
    s.command(owner, 14, Command::Tool(ToolAction::Hammer))
        .unwrap();
    assert!(!s.simulation().state().bricks.contains_key(&id));
}

#[test]
fn action_aim_is_immediate_does_not_rewind_motion_and_preserves_authority() {
    use bri_sim::session::ActionAim;
    let mut s = session(vec![], false);
    let owner = s
        .join("Builder".into(), Vec3::new(0.5, 0.05, 0.0), false)
        .unwrap();
    let front = plant(&mut s, owner, 1, [0.5, 0.1, -3.25]);
    let back = plant(&mut s, owner, 2, [0.5, 0.1, 3.25]);
    aim(&mut s, owner, 1, [0.5, 0.1, 3.25]);
    let before = s.snapshot();
    let player = &before.players[0];
    let direction = Vec3::new(0.5, 0.1, -3.25) - player.eye(&PlayerTuning::default());
    let captured = ActionAim {
        yaw: 0.0,
        pitch: direction.y.atan2(3.25),
    };
    let inspect = Command::Tool(ToolAction::Inspect {
        mode: InspectMode::Wrench,
    });
    s.equip_tool(owner, Some(1)).unwrap();
    assert!(
        matches!(s.command_with_aim(owner,3,inspect.clone(),Some(captured)).unwrap(),Reply::Inspected{brick_id,..} if brick_id==front)
    );
    assert_eq!(
        s.snapshot().players,
        before.players,
        "Action aim must not rewind the newer body pose"
    );
    assert!(
        matches!(s.command(owner,4,inspect.clone()).unwrap(),Reply::Inspected{brick_id,..} if brick_id==back)
    );
    assert!(
        s.command_with_aim(owner, 4, Command::Tool(ToolAction::Hammer), Some(captured))
            .is_err(),
        "Replay must not execute twice"
    );
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
                5 + index as u64,
                Command::Tool(ToolAction::Hammer),
                Some(invalid)
            )
            .is_err()
        );
        assert_eq!(s.snapshot().world, before.world);
    }
    let other = s
        .join("Other".into(), Vec3::new(3.0, 0.05, 0.0), false)
        .unwrap();
    let other_player = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == other)
        .unwrap();
    let d = Vec3::new(0.5, 0.1, -3.25) - other_player.eye(&PlayerTuning::default());
    let foreign = ActionAim {
        yaw: d.x.atan2(-d.z),
        pitch: d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()),
    };
    s.equip_tool(other, Some(0)).unwrap();
    assert!(
        s.command_with_aim(other, 1, Command::Tool(ToolAction::Hammer), Some(foreign))
            .unwrap_err()
            .to_string()
            .contains("denied")
    );
    assert_eq!(s.snapshot().world, before.world);
    s.equip_tool(owner, Some(0)).unwrap();
    s.command_with_aim(owner, 7, Command::Tool(ToolAction::Hammer), Some(captured))
        .unwrap();
    assert!(!s.simulation().state().bricks.contains_key(&front));
    assert!(s.simulation().state().bricks.contains_key(&back));
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
fn aim(s: &mut Session, owner: u64, seq: u64, target: [f32; 3]) {
    let p = s
        .snapshot()
        .players
        .into_iter()
        .find(|p| p.owner == owner)
        .unwrap();
    let d = Vec3::from(target) - p.eye(&PlayerTuning::default());
    s.movement(
        owner,
        seq,
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
    // dedicated authority tests below exercise remote equip/rejection ordering.
    let slot = match action {
        ToolAction::Hammer => Some(0),
        ToolAction::Inspect {
            mode: InspectMode::Printer,
        }
        | ToolAction::SetPrint { .. } => Some(2),
        ToolAction::Inspect { .. }
        | ToolAction::SetWrench { .. }
        | ToolAction::SetEvents { .. } => Some(1),
        _ => None,
    };
    s.equip_tool(owner, slot)?;
    s.command(owner, seq, Command::Tool(action))
}
fn inspect(s: &mut Session, owner: u64, seq: u64, mode: InspectMode) -> Brick {
    let Reply::Inspected {
        brick_id: _,
        brick,
        mode: actual,
    } = tool(s, owner, seq, ToolAction::Inspect { mode }).unwrap()
    else {
        panic!("expected inspection")
    };
    assert_eq!(actual, mode);
    *brick
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
fn tools_resolve_authoritative_aim_and_permissions_and_validate_effects() {
    let (mut s, owner, id) = setup();
    let guest = s
        .join("Guest".into(), Vec3::new(3.0, 0.05, 0.0), false)
        .unwrap();
    aim(&mut s, guest, 1, [0.5, 0.1, -3.25]);
    let before = s.snapshot().world;
    for (seq, action) in [
        ToolAction::Paint { color: 1 },
        ToolAction::Hammer,
        ToolAction::Inspect {
            mode: InspectMode::Wrench,
        },
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            tool(&mut s, guest, seq as u64 + 1, action)
                .unwrap_err()
                .to_string()
                .contains("denied")
        );
        assert_eq!(s.snapshot().world, before);
    }
    tool(&mut s, owner, 2, ToolAction::Paint { color: 1 }).unwrap();
    tool(&mut s, owner, 3, ToolAction::ColorEffect { effect: 6 }).unwrap();
    tool(&mut s, owner, 4, ToolAction::ShapeEffect { effect: 2 }).unwrap();
    let before = s.snapshot().world;
    assert_eq!(before.bricks[&id].color, 1);
    assert_eq!(before.bricks[&id].color_effect, 6);
    assert_eq!(before.bricks[&id].shape_effect, 2);
    for (seq, action) in [
        ToolAction::Paint { color: 2 },
        ToolAction::ColorEffect { effect: 7 },
        ToolAction::ShapeEffect { effect: 3 },
    ]
    .into_iter()
    .enumerate()
    {
        assert!(tool(&mut s, owner, seq as u64 + 5, action).is_err());
        assert_eq!(s.snapshot().world, before);
    }
    aim(&mut s, owner, 2, [100.0, 2.4, 0.0]);
    let before = s.snapshot().world;
    assert!(tool(&mut s, owner, 8, ToolAction::Hammer).is_err());
    assert_eq!(s.snapshot().world, before);
    assert!(
        serde_json::from_str::<Command>(
            r#"{"kind":"tool","value":{"kind":"paint","value":{"color":0,"position":[0,0,0]}}}"#
        )
        .is_err()
    );
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
    s.command(
        owner,
        10,
        Command::Edit {
            brick: id,
            edit: Edit::Name(Some("changed".into())),
        },
    )
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
    assert!(
        tool(
            &mut s,
            owner,
            12,
            ToolAction::Inspect {
                mode: InspectMode::Printer
            }
        )
        .is_err()
    );
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
    let event = Event {
        enabled: true,
        input: Input::Activate,
        delay_ms: 25,
        target: Target::ThisBrick,
        action: Action::Light(Some(ContentRef::Resolved("unknown".into()))),
    };
    inspect(&mut s, owner, 1, InspectMode::Events);
    let before = s.snapshot().world;
    assert!(
        tool(
            &mut s,
            owner,
            2,
            ToolAction::SetEvents {
                brick: 1,
                events: vec![event.clone()]
            }
        )
        .is_err()
    );
    assert!(
        s.command(
            owner,
            3,
            Command::Edit {
                brick: 1,
                edit: Edit::Events(vec![event.clone()])
            }
        )
        .is_err()
    );
    assert!(
        s.command(
            owner,
            4,
            Command::Edit {
                brick: 1,
                edit: Edit::Action(event.action)
            }
        )
        .is_err()
    );
    assert_eq!(s.snapshot().world, before);
    let event = Event {
        action: Action::Color(1),
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
        assert_eq!(
            tool(&mut s, owner, 1, ToolAction::Hammer).is_ok(),
            succeeds,
            "{position:?} {spawn:?}"
        );
        assert_eq!(s.simulation().state().bricks.is_empty(), succeeds);
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
    let owner = s
        .join("Admin".into(), Vec3::new(0.5, 0.1, 0.0), true)
        .unwrap();
    aim(&mut s, owner, 1, position);
    assert!(tool(&mut s, owner, 1, ToolAction::Hammer).is_err());
    assert!(
        tool(
            &mut s,
            owner,
            2,
            ToolAction::Inspect {
                mode: InspectMode::Wrench
            }
        )
        .is_err()
    );
}

#[test]
fn undo_is_owner_scoped_lifo_skips_removed_bricks_and_survives_authenticated_resume() {
    let (mut s, owner, first) = setup();
    let second = plant(&mut s, owner, 2, [1.5, 0.1, -3.25]);
    let third = plant(&mut s, owner, 3, [2.5, 0.1, -3.25]);
    aim(&mut s, owner, 2, [2.5, 0.1, -3.25]);
    tool(&mut s, owner, 4, ToolAction::Hammer).unwrap();
    assert!(!s.simulation().state().bricks.contains_key(&third));
    let guest = s
        .join("Guest".into(), Vec3::new(5.0, 0.05, 0.0), true)
        .unwrap();
    assert_eq!(
        tool(&mut s, guest, 1, ToolAction::UndoPlant).unwrap(),
        Reply::Undone(None)
    );
    s.disconnect(owner).unwrap();
    s.resume(owner, Vec3::new(50.0, 0.05, 50.0)).unwrap();
    assert_eq!(
        tool(&mut s, owner, 1, ToolAction::UndoPlant).unwrap(),
        Reply::Undone(Some(second))
    );
    assert_eq!(
        tool(&mut s, owner, 2, ToolAction::UndoPlant).unwrap(),
        Reply::Undone(Some(first))
    );
    assert_eq!(
        tool(&mut s, owner, 3, ToolAction::UndoPlant).unwrap(),
        Reply::Undone(None)
    );
    assert!(s.simulation().state().bricks.is_empty());
}

#[test]
fn planting_undo_retains_only_the_stock_512_most_recent_entries() {
    let mut s = session(vec![], false);
    let owner = s
        .join("Builder".into(), Vec3::new(-3.0, 0.05, 0.0), false)
        .unwrap();
    let mut seq = 0;
    let mut first = 0;
    for i in 0..=UNDO_PLANT_LIMIT {
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
    for i in 0..UNDO_PLANT_LIMIT {
        if i % 50 == 0 {
            for _ in 0..120 {
                s.step().unwrap();
            }
        }
        seq += 1;
        assert_eq!(
            tool(&mut s, owner, seq, ToolAction::UndoPlant).unwrap(),
            Reply::Undone(Some((UNDO_PLANT_LIMIT + 1 - i) as u64))
        );
    }
    assert_eq!(
        tool(&mut s, owner, seq + 1, ToolAction::UndoPlant).unwrap(),
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

#[test]
fn nested_events_return_to_wrench_without_overwriting_concurrent_properties() {
    for concurrent_change in [false, true] {
        let (mut s, owner, id) = setup();
        inspect(&mut s, owner, 2, InspectMode::Wrench);
        if concurrent_change {
            s.command(
                owner,
                3,
                Command::Edit {
                    brick: id,
                    edit: Edit::Name(Some("other editor".into())),
                },
            )
            .unwrap();
        }
        inspect(&mut s, owner, 4, InspectMode::Events);
        tool(
            &mut s,
            owner,
            5,
            ToolAction::SetEvents {
                brick: id,
                events: vec![Event {
                    enabled: true,
                    input: Input::Activate,
                    delay_ms: 0,
                    target: Target::ThisBrick,
                    action: Action::Color(1),
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
