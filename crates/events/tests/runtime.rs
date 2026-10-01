use bri_events::*;
use std::collections::BTreeMap;
fn id(n: u64) -> Id {
    Id {
        index: n,
        generation: 1,
    }
}
fn fixture() -> Catalog {
    let input = |name: &str, targets: Vec<(&str, &str)>| InputDef {
        id: format!("in/{name}"),
        class_name: "fxDTSBrick".into(),
        name: name.into(),
        targets: targets
            .into_iter()
            .map(|(a, b)| (a.into(), b.into()))
            .collect(),
        source: "fixture".into(),
        source_line: 1,
    };
    let output = |name: &str, params| OutputDef {
        id: format!("out/{name}"),
        class_name: "fxDTSBrick".into(),
        name: name.into(),
        params,
        append_client: true,
        source: "fixture".into(),
        source_line: 1,
        package: None,
    };
    Catalog {
        schema_version: 1,
        inputs: vec![
            input(
                "onActivate",
                vec![("Self", "fxDTSBrick"), ("Client", "GameConnection")],
            ),
            input("onRelay", vec![("Self", "fxDTSBrick")]),
            input(
                "onPrintCountOverFlow",
                vec![("Self", "fxDTSBrick"), ("Client", "GameConnection")],
            ),
            input(
                "onPrintCountUnderFlow",
                vec![("Self", "fxDTSBrick"), ("Client", "GameConnection")],
            ),
            input("onToolBreak", vec![("Self", "fxDTSBrick")]),
        ],
        outputs: vec![
            output("setColor", vec![Param::PaintColor { default: 0 }]),
            output("fireRelay", vec![]),
            output("fireRelayNorth", vec![]),
            output("cancelEvents", vec![]),
            output(
                "setEventEnabled",
                vec![Param::IntList { width: 157 }, Param::Bool],
            ),
            output("toggleEventEnabled", vec![Param::IntList { width: 176 }]),
            output(
                "incrementPrintCount",
                vec![Param::Int {
                    min: 1,
                    max: 9,
                    default: 1,
                }],
            ),
            output(
                "decrementPrintCount",
                vec![Param::Int {
                    min: 1,
                    max: 9,
                    default: 1,
                }],
            ),
            output(
                "setPrintCount",
                vec![Param::Int {
                    min: 0,
                    max: 9,
                    default: 0,
                }],
            ),
            output(
                "disappear",
                vec![Param::Int {
                    min: -1,
                    max: 300,
                    default: 5,
                }],
            ),
        ],
        targets: vec![],
        sources: vec![],
        scope: serde_json::Value::Null,
    }
}
fn world(limits: Limits) -> EventWorld {
    EventWorld::new(
        fixture(),
        Bindings {
            palette_len: 64,
            ..Default::default()
        },
        limits,
    )
    .unwrap()
}
fn row(input: &str, output: &str, params: Vec<Value>) -> Row {
    Row {
        preserved: None,
        enabled: true,
        input: input.into(),
        delay_ms: 0,
        target: Target::Slot(Slot::SelfBrick),
        output: output.into(),
        params,
    }
}
fn color(input: &str, value: u8) -> Row {
    row(input, "setColor", vec![Value::Color(value)])
}
fn brick(n: u64, rows: Vec<Row>) -> BrickProgram {
    BrickProgram {
        id: id(n),
        owner_scope: 1,
        name: None,
        rows,
        print_count: 0,
        implicit_cancel_relays: false,
    }
}
#[derive(Default)]
struct FakeHost {
    calls: Vec<Dispatch>,
    dead: Vec<Entity>,
    neighbors: Vec<Id>,
    blocked: Option<u64>,
    /// Time each applied row takes.
    slow: Option<std::time::Duration>,
}
impl Host for FakeHost {
    fn alive(&self, e: Entity) -> bool {
        !self.dead.contains(&e)
    }
    fn permitted(&self, _: &Trigger, _: Entity, _: &str) -> bool {
        true
    }
    fn relay_neighbors(&mut self, _: Id, _: Direction, _: usize) -> Result<Vec<Id>, String> {
        Ok(self.neighbors.clone())
    }
    fn apply(&mut self, d: &Dispatch) -> Apply {
        if self.blocked == Some(d.origin) {
            return Apply::Deferred("fixture temporarily blocked".into());
        }
        if let Some(slow) = self.slow {
            std::thread::sleep(slow);
        }
        self.calls.push(d.clone());
        Apply::Applied
    }
}
#[test]
fn four_thousand_rows_execute_ordered_same_phase_and_invalid_edit_is_atomic() {
    let mut w = world(Limits::default());
    let rows = (0..4096)
        .map(|i| color("onActivate", (i % 64) as u8))
        .collect();
    w.install_brick(brick(1, rows)).unwrap();
    let mut host = FakeHost::default();
    assert_eq!(
        w.trigger(Trigger::new(id(1), "onActivate", 1)).unwrap(),
        4096
    );
    let r = w.advance(0, &mut host).unwrap();
    assert_eq!(r.applied, 4096);
    assert_eq!(r.pending, 0);
    for (i, d) in host.calls.iter().enumerate() {
        assert_eq!(d.row, i as u16);
        assert_eq!(d.now_us, 0);
        assert_eq!(d.intent, Intent::Brick(BrickOp::Color((i % 64) as u8)));
    }
    let mut bad = w.program(id(1)).unwrap().clone();
    bad.rows.push(color("onActivate", 1));
    assert!(w.install_brick(bad).is_err());
    assert_eq!(w.program(id(1)).unwrap().rows.len(), 4096);
}
#[test]
fn relay_branches_preserve_authored_breadth_first_order_without_33ms() {
    let mut w = world(Limits::default());
    w.install_brick(brick(
        1,
        vec![
            row("onActivate", "fireRelay", vec![]),
            color("onActivate", 1),
            color("onRelay", 2),
            row("onRelay", "fireRelayNorth", vec![]),
        ],
    ))
    .unwrap();
    w.install_brick(brick(2, vec![color("onRelay", 3)]))
        .unwrap();
    let mut h = FakeHost {
        neighbors: vec![id(2), id(2), id(1)],
        ..Default::default()
    };
    w.trigger(Trigger::new(id(1), "onActivate", 1)).unwrap();
    let r = w.advance(0, &mut h).unwrap();
    assert_eq!(r.pending, 0);
    assert_eq!(
        h.calls.iter().map(|d| d.intent.clone()).collect::<Vec<_>>(),
        vec![
            Intent::Brick(BrickOp::Color(1)),
            Intent::Brick(BrickOp::Color(2)),
            Intent::Brick(BrickOp::Color(3))
        ]
    );
    assert!(h.calls.iter().all(|d| d.now_us == 0));
}
#[test]
fn cancellation_prepass_preserves_new_rows_and_toolbreak_exception() {
    let mut w = world(Limits::default());
    let mut delayed = color("onRelay", 1);
    delayed.delay_ms = 100;
    let mut exception = color("onToolBreak", 2);
    exception.delay_ms = 100;
    let mut new = color("onActivate", 3);
    new.delay_ms = 100;
    w.install_brick(brick(
        1,
        vec![
            delayed,
            exception,
            row("onActivate", "cancelEvents", vec![]),
            new,
        ],
    ))
    .unwrap();
    w.trigger(Trigger::new(id(1), "onRelay", 1)).unwrap();
    w.trigger(Trigger::new(id(1), "onToolBreak", 1)).unwrap();
    w.trigger(Trigger::new(id(1), "onActivate", 1)).unwrap();
    assert_eq!(w.pending(), 2);
    let mut h = FakeHost::default();
    assert_eq!(w.advance(99999, &mut h).unwrap().applied, 0);
    assert_eq!(w.advance(100000, &mut h).unwrap().applied, 2);
    assert_eq!(h.calls[0].intent, Intent::Brick(BrickOp::Color(2)));
    assert_eq!(h.calls[1].intent, Intent::Brick(BrickOp::Color(3)));
}
#[test]
fn enabling_mutates_future_inputs_not_already_captured_output_rows() {
    let mut w = world(Limits::default());
    w.install_brick(brick(
        1,
        vec![
            row(
                "onActivate",
                "setEventEnabled",
                vec![
                    Value::Rows(RowSelection::Indices(vec![1])),
                    Value::Bool(false),
                ],
            ),
            color("onActivate", 4),
        ],
    ))
    .unwrap();
    let mut h = FakeHost::default();
    w.trigger(Trigger::new(id(1), "onActivate", 1)).unwrap();
    w.advance(0, &mut h).unwrap();
    assert_eq!(h.calls.len(), 1);
    assert!(!w.program(id(1)).unwrap().rows[1].enabled);
    w.trigger(Trigger::new(id(1), "onActivate", 1)).unwrap();
    w.advance(0, &mut h).unwrap();
    assert_eq!(h.calls.len(), 1);
}
#[test]
fn eight_independent_origins_remain_fair_during_reentrant_zero_delay_loop() {
    let limits = Limits {
        steps_per_phase: 80,
        steps_per_origin: 20,
        loop_warning_depth: 2,
        ..Default::default()
    };
    let mut w = world(limits);
    for n in 1..=8 {
        w.install_brick(brick(
            n,
            vec![
                color("onRelay", n as u8),
                row("onRelay", "fireRelay", vec![]),
            ],
        ))
        .unwrap();
        w.trigger(Trigger::new(id(n), "onRelay", n)).unwrap();
    }
    let mut h = FakeHost::default();
    let r = w.advance(0, &mut h).unwrap();
    assert_eq!(r.steps, 80);
    assert!(r.origins.values().all(|o| o.steps == 10 && o.loops > 0));
    assert_eq!(r.origins.len(), 8);
    assert!(r.due_pending > 0);
    assert!(w.cancel_origin(1) > 0);
    let before = h.calls.iter().filter(|d| d.origin == 1).count();
    let r = w.advance(0, &mut h).unwrap();
    assert_eq!(before, h.calls.iter().filter(|d| d.origin == 1).count());
    assert!(!r.origins.contains_key(&1));
}
#[test]
fn named_targets_are_indexed_scoped_sorted_and_capture_generation() {
    let mut w = world(Limits::default());
    let mut named = color("onActivate", 7);
    named.target = Target::Named("Door".into());
    named.delay_ms = 10;
    w.install_brick(brick(1, vec![named])).unwrap();
    for n in 2..=4 {
        let mut b = brick(n, vec![]);
        b.name = Some("door".into());
        if n == 4 {
            b.owner_scope = 2;
        }
        w.install_brick(b).unwrap();
    }
    assert_eq!(w.trigger(Trigger::new(id(1), "onActivate", 1)).unwrap(), 2);
    let mut h = FakeHost {
        dead: vec![Entity::brick(id(2))],
        ..Default::default()
    };
    let r = w.advance(10000, &mut h).unwrap();
    assert_eq!(r.stale, 1);
    assert_eq!(h.calls.len(), 1);
    assert_eq!(h.calls[0].target.id, id(3));
}
#[test]
fn overload_admission_is_atomic_and_internal_branch_can_resume() {
    let mut w = world(Limits {
        pending: 4,
        ..Default::default()
    });
    let mut relay = row("onActivate", "fireRelay", vec![]);
    relay.target = Target::Named("branch".into());
    w.install_brick(brick(
        1,
        vec![relay, color("onActivate", 1), color("onActivate", 2)],
    ))
    .unwrap();
    let mut b = brick(
        2,
        vec![
            color("onRelay", 3),
            color("onRelay", 4),
            color("onRelay", 5),
        ],
    );
    b.name = Some("branch".into());
    w.install_brick(b).unwrap();
    w.trigger(Trigger::new(id(1), "onActivate", 1)).unwrap();
    assert!(w.trigger(Trigger::new(id(1), "onActivate", 1)).is_err());
    assert_eq!(w.pending(), 3);
    let mut h = FakeHost::default();
    let r = w.advance(0, &mut h).unwrap();
    assert_eq!(r.admission_backpressure, 1);
    assert_eq!(h.calls.len(), 2);
    assert_eq!(w.pending(), 1);
    w.advance(0, &mut h).unwrap();
    assert_eq!(h.calls.len(), 5);
    assert_eq!(w.pending(), 0);
}
#[test]
fn host_deferred_work_preserves_row_order_but_other_origins_progress() {
    let mut w = world(Limits::default());
    for n in 1..=2 {
        w.install_brick(brick(
            n,
            vec![color("onActivate", 1), color("onActivate", 2)],
        ))
        .unwrap();
        w.trigger(Trigger::new(id(n), "onActivate", n)).unwrap();
    }
    let mut h = FakeHost {
        blocked: Some(1),
        ..Default::default()
    };
    let r = w.advance(0, &mut h).unwrap();
    assert_eq!(r.applied, 2);
    assert_eq!(r.pending, 2);
    assert!(h.calls.iter().all(|d| d.origin == 2));
    h.blocked = None;
    w.advance(50, &mut h).unwrap();
    assert_eq!(h.calls[2].row, 0);
    assert_eq!(h.calls[3].row, 1);
}
#[test]
fn print_overflow_updates_digit_then_fires_client_attributed_chain() {
    let mut w = world(Limits::default());
    let mut b = brick(
        1,
        vec![
            row("onRelay", "incrementPrintCount", vec![Value::Int(3)]),
            color("onPrintCountOverFlow", 9),
        ],
    );
    b.print_count = 8;
    w.install_brick(b).unwrap();
    let mut trigger = Trigger::new(id(1), "onRelay", 1);
    trigger.client = Some(Entity {
        class: Class::Client,
        id: id(99),
    });
    w.trigger(trigger).unwrap();
    let mut h = FakeHost::default();
    w.advance(0, &mut h).unwrap();
    assert_eq!(w.program(id(1)).unwrap().print_count, 1);
    assert_eq!(h.calls[0].intent, Intent::Brick(BrickOp::PrintDigit(1)));
    assert_eq!(h.calls[1].intent, Intent::Brick(BrickOp::Color(9)));
    assert_eq!(
        h.calls[1].client,
        Some(Entity {
            class: Class::Client,
            id: id(99)
        })
    );
}
#[test]
fn checkpoint_resumes_delays_and_rejects_action_tampering() {
    let mut w = world(Limits::default());
    let mut delayed = color("onActivate", 4);
    delayed.delay_ms = 20;
    w.install_brick(brick(1, vec![delayed])).unwrap();
    w.trigger(Trigger::new(id(1), "onActivate", 1)).unwrap();
    w.advance(5000, &mut FakeHost::default()).unwrap();
    let save = w.save().unwrap();
    let mut restored = EventWorld::restore(
        fixture(),
        Bindings {
            palette_len: 64,
            ..Default::default()
        },
        &save,
    )
    .unwrap();
    let mut h = FakeHost::default();
    assert!(restored.advance(4999, &mut h).is_err());
    assert_eq!(restored.advance(19999, &mut h).unwrap().applied, 0);
    assert_eq!(restored.advance(20000, &mut h).unwrap().applied, 1);
    let mut corrupt: serde_json::Value = serde_json::from_slice(&save).unwrap();
    corrupt["jobs"][0]["action"]["Intent"]["Brick"]["Color"] = serde_json::json!(63);
    assert!(
        EventWorld::restore(
            fixture(),
            Bindings {
                palette_len: 64,
                ..Default::default()
            },
            &serde_json::to_vec(&corrupt).unwrap()
        )
        .is_err()
    );
}
#[test]
fn disappear_timer_replacement_survives_save_and_is_not_cancel_events() {
    let mut w = world(Limits::default());
    w.install_brick(brick(
        1,
        vec![row("onActivate", "disappear", vec![Value::Int(1)])],
    ))
    .unwrap();
    let mut h = FakeHost::default();
    w.trigger(Trigger::new(id(1), "onActivate", 1)).unwrap();
    w.advance(0, &mut h).unwrap();
    assert_eq!(w.cancel_source(id(1), CancelMode::AuthoredDelayed), 0);
    let mut w = EventWorld::restore(
        fixture(),
        Bindings {
            palette_len: 64,
            ..Default::default()
        },
        &w.save().unwrap(),
    )
    .unwrap();
    w.advance(1_000_000, &mut h).unwrap();
    assert_eq!(h.calls.len(), 2);
    assert_eq!(
        h.calls[1].intent,
        Intent::Brick(BrickOp::Presence {
            rendering: true,
            colliding: true,
            ray_casting: true,
            revive_fake_dead: false
        })
    );
}
#[test]
fn source_math_keeps_health_projectile_and_relay_semantics() {
    use bri_events::semantics::*;
    use glam::Vec3;
    assert_eq!(add_health(100., 100., 50), HealthChange::Unchanged);
    assert_eq!(add_health(100., 20., 50), HealthChange::SetDamage(0.));
    assert_eq!(set_health(100., 20., 0), HealthChange::Damage(100.));
    assert_eq!(
        bounce(Vec3::new(0., -300., 0.), Vec3::Y, 1.),
        Vec3::Y * 200.
    );
    assert_eq!(redirect(Vec3::Y * 50., Vec3::X * 2., true), Vec3::X * 50.);
    assert_eq!(
        radius_impulse(Vec3::ZERO, Vec3::X * 5., 10., 100., 20.),
        Vec3::new(75., 15., 0.)
    );
    let (c, s) = relay_box(Vec3::ZERO, Vec3::new(2., 1., 4.), Direction::North);
    assert_eq!(c.z, 0.);
    assert!((s.z - 0.1).abs() < 1e-6);
    assert!(!may_recover_vehicle(&[(true, false)]));
    assert_eq!(
        item_spawn_position(
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ONE * 2.,
            Vec3::ZERO
        ),
        Vec3::ZERO
    );
    assert_eq!(
        item_spawn_position(
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ONE * 2.,
            Vec3::Y
        ),
        Vec3::Y * 1.1
    );
    assert!(!sound_allowed(true, true));
}
/// The converted vanilla catalog (`content/events-pack-002`).
fn content_catalog() -> Catalog {
    Catalog::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/events-pack-002/catalog.json"),
    )
    .unwrap()
}
/// Emits a synthetic `#[test]` running `$body` on the made-up
/// `bri_events::testing::catalog_extended()` and an ignored one running the same body
/// on the converted vanilla catalog.
macro_rules! on_both_catalogs {
    ($synthetic:ident, $content:ident, $body:ident) => {
        #[test]
        fn $synthetic() {
            $body(bri_events::testing::catalog_extended());
        }
        #[test]
        #[ignore = "requires generated v20 content"]
        fn $content() {
            $body(content_catalog());
        }
    };
}
/// The first input that can target `class`, with the slot naming it.
fn input_for(catalog: &Catalog, class: Class) -> (&InputDef, Slot) {
    let input = catalog
        .inputs
        .iter()
        .find(|i| {
            i.targets
                .iter()
                .any(|(_, c)| Class::parse(c) == Some(class))
        })
        .unwrap();
    let slot = Slot::parse(
        &input
            .targets
            .iter()
            .find(|(_, c)| Class::parse(c) == Some(class))
            .unwrap()
            .0,
    )
    .unwrap();
    (input, slot)
}
/// A default-parameter row wiring `output` to the first input reaching its
/// class.
fn default_row(catalog: &Catalog, output: &OutputDef) -> (Row, Class, Slot) {
    let class = Class::parse(&output.class_name).unwrap();
    let (input, slot) = input_for(catalog, class);
    let row = Row {
        preserved: None,
        enabled: true,
        input: input.name.clone(),
        delay_ms: 0,
        target: Target::Slot(slot),
        output: output.name.clone(),
        params: output.params.iter().map(Param::default_value).collect(),
    };
    (row, class, slot)
}
fn every_output_compiles_with_default_params(catalog: Catalog) {
    assert!(!catalog.outputs.is_empty());
    for output in &catalog.outputs {
        let (row, _, _) = default_row(&catalog, output);
        catalog
            .validate_row(
                &row,
                &Bindings {
                    palette_len: 64,
                    datablocks: BTreeMap::new(),
                },
            )
            .unwrap();
    }
}
on_both_catalogs!(
    every_output_compiles_with_default_params_synthetic,
    every_output_compiles_with_default_params_content,
    every_output_compiles_with_default_params
);
#[test]
#[ignore = "requires generated v20 content"]
fn actual_catalog_has_all_65_outputs_and_16_inputs() {
    let catalog = content_catalog();
    assert_eq!(catalog.inputs.len(), 16);
    assert_eq!(catalog.outputs.len(), 65);
}

#[test]
fn opaque_rows_keep_indices_and_disabled_source_future_does_not_renumber() {
    let mut w = world(Limits::default());
    let preserved = Row {
        preserved: Some(PreservedRow {
            original: "+-EVENT unknown community row".into(),
            diagnostic: "unregistered".into(),
        }),
        enabled: true,
        input: String::new(),
        delay_ms: 0,
        target: Target::Slot(Slot::SelfBrick),
        output: String::new(),
        params: vec![],
    };
    w.install_brick(brick(1, vec![preserved, color("onActivate", 2)]))
        .unwrap();
    w.trigger(Trigger::new(id(1), "onActivate", 1)).unwrap();
    let mut h = FakeHost::default();
    w.advance(0, &mut h).unwrap();
    assert_eq!(h.calls[0].row, 1);
    let restored = EventWorld::restore(
        fixture(),
        Bindings {
            palette_len: 64,
            ..Default::default()
        },
        &w.save().unwrap(),
    )
    .unwrap();
    assert_eq!(
        restored.program(id(1)).unwrap().rows[0]
            .preserved
            .as_ref()
            .unwrap()
            .original,
        "+-EVENT unknown community row"
    );
}
#[test]
fn editor_rows_row_lists_and_the_world_clock_convert_at_the_boundary() {
    use bri_events::convert::*;
    let ui = serde_json::json!({"enabled":true,"delay_ms":0,"input":"onActivate","target":"Player","named_target":null,"output":"SetVelocity","params":[{"Vector":[0.,0.,10.]}]});
    assert_eq!(
        ui_event(&ui).unwrap().params,
        vec![Value::Vector(glam::Vec3::Y * 10.)]
    );
    assert_eq!(
        row_selection("0 101 4095").unwrap(),
        RowSelection::Indices(vec![0, 101, 4095])
    );
    assert!(row_selection("4096").is_err());
    assert_eq!(world_tick_to_us(120).unwrap(), 1_000_000);
}
#[test]
fn state_byte_limit_rejects_atomically_and_cancellation_reclaims_capacity() {
    let mut w = world(Limits {
        state_bytes: 4096,
        ..Default::default()
    });
    let mut delayed = color("onActivate", 1);
    delayed.delay_ms = 100;
    w.install_brick(brick(1, vec![delayed])).unwrap();
    let mut admitted = 0;
    while w.trigger(Trigger::new(id(1), "onActivate", 1)).is_ok() {
        admitted += 1;
        assert!(admitted < 20);
    }
    assert!(admitted > 0);
    assert_eq!(w.pending(), admitted);
    let bytes = w.save().unwrap();
    let mut restored = EventWorld::restore(
        fixture(),
        Bindings {
            palette_len: 64,
            ..Default::default()
        },
        &bytes,
    )
    .unwrap();
    assert_eq!(restored.pending(), admitted);
    assert!(
        restored
            .trigger(Trigger::new(id(1), "onActivate", 1))
            .is_err()
    );
    assert_eq!(w.cancel_origin(1), admitted);
    assert!(w.trigger(Trigger::new(id(1), "onActivate", 1)).is_ok());
}
#[test]
fn delayed_relay_cycles_do_not_emit_zero_delay_loop_warnings() {
    let mut w = world(Limits {
        loop_warning_depth: 1,
        ..Default::default()
    });
    let mut relay = row("onRelay", "fireRelay", vec![]);
    relay.delay_ms = 1;
    w.install_brick(brick(1, vec![relay])).unwrap();
    w.trigger(Trigger::new(id(1), "onRelay", 1)).unwrap();
    let mut h = FakeHost::default();
    for tick in 1..20 {
        let r = w.advance(tick * 1000, &mut h).unwrap();
        assert!(r.origins.values().all(|o| o.loops == 0));
    }
}
#[test]
fn cancel_prepass_frees_origin_admission_and_saved_context_is_validated() {
    let mut w = world(Limits {
        origins: 1,
        ..Default::default()
    });
    let mut delayed = color("onRelay", 1);
    delayed.delay_ms = 10;
    w.install_brick(brick(
        1,
        vec![
            delayed,
            row("onActivate", "cancelEvents", vec![]),
            color("onActivate", 2),
        ],
    ))
    .unwrap();
    w.trigger(Trigger::new(id(1), "onRelay", 1)).unwrap();
    assert!(w.trigger(Trigger::new(id(1), "onActivate", 2)).is_ok());
    let mut corrupt: serde_json::Value = serde_json::from_slice(&w.save().unwrap()).unwrap();
    corrupt["jobs"][0]["context"]["client"] =
        serde_json::json!({"class":"Player","id":{"index":5,"generation":1}});
    assert!(
        EventWorld::restore(
            fixture(),
            Bindings {
                palette_len: 64,
                ..Default::default()
            },
            &serde_json::to_vec(&corrupt).unwrap()
        )
        .is_err()
    );
}

fn every_output_reaches_its_dispatch_or_internal_route(catalog: Catalog) {
    assert!(!catalog.outputs.is_empty());
    for output in &catalog.outputs {
        let (row, class, slot) = default_row(&catalog, output);
        let input = row.input.clone();
        let mut world = EventWorld::new(
            catalog.clone(),
            Bindings {
                palette_len: 64,
                ..Default::default()
            },
            Limits::default(),
        )
        .unwrap();
        world.install_brick(brick(1, vec![row])).unwrap();
        let mut trigger = Trigger::new(id(1), &input, 1);
        if slot != Slot::SelfBrick {
            trigger.targets.insert(slot, Entity { class, id: id(2) });
        }
        let queued = world.trigger(trigger).unwrap();
        let mut host = FakeHost::default();
        let report = world.advance(0, &mut host).unwrap();
        assert_eq!(report.rejected, 0, "{}", output.name);
        assert_eq!(report.admission_backpressure, 0, "{}", output.name);
        assert!(report.applied > 0 || queued == 0, "{}", output.name);
        world.cancel_origin(1);
        assert_eq!(world.pending(), 0);
    }
}
on_both_catalogs!(
    every_output_reaches_its_dispatch_synthetic,
    every_vanilla_output_reaches_its_native_dispatch_or_internal_route,
    every_output_reaches_its_dispatch_or_internal_route
);
#[test]
fn expansion_budget_preserves_finite_branches_across_same_time_phases() {
    let mut w = world(Limits {
        expansions_per_phase: 2,
        expansions_per_origin: 2,
        ..Default::default()
    });
    for n in 1..=2 {
        w.install_brick(brick(
            n,
            vec![
                row("onActivate", "fireRelay", vec![]),
                color("onRelay", n as u8),
                color("onRelay", n as u8),
            ],
        ))
        .unwrap();
        w.trigger(Trigger::new(id(n), "onActivate", n)).unwrap();
    }
    let mut h = FakeHost::default();
    let r = w.advance(0, &mut h).unwrap();
    assert_eq!(r.expanded, 2);
    assert_eq!(r.admission_backpressure, 1);
    assert_eq!(h.calls.len(), 2);
    w.advance(0, &mut h).unwrap();
    assert_eq!(h.calls.len(), 4);
    assert_eq!(w.pending(), 0);
}
/// v20 stamps every input with its own millisecond. Two clicks in one host
/// tick keep their order through everything they schedule: the first
/// relay's zero-delay rows (which switch the relay brick off) run before
/// the second click's relay reaches it, as Demo Pong's paddle chain needs.
#[test]
fn activations_in_one_tick_keep_their_order_through_relays() {
    let mut w = world(Limits::default());
    let mut relay = row("onActivate", "fireRelay", vec![]);
    relay.target = Target::Named("step".into());
    relay.delay_ms = 33;
    w.install_brick(brick(1, vec![relay])).unwrap();
    let mut step = brick(
        2,
        vec![
            color("onRelay", 5),
            row(
                "onRelay",
                "setEventEnabled",
                vec![
                    Value::Rows(RowSelection::Indices(vec![0, 1])),
                    Value::Bool(false),
                ],
            ),
        ],
    );
    step.name = Some("step".into());
    w.install_brick(step).unwrap();
    // A third origin's work keeps the old per-origin turn order busy.
    w.install_brick(brick(3, vec![color("onRelay", 9)]))
        .unwrap();
    let mut h = FakeHost::default();
    w.trigger(Trigger::new(id(1), "onActivate", 4)).unwrap();
    w.trigger(Trigger::new(id(1), "onActivate", 2)).unwrap();
    w.set_clock(33_000).unwrap();
    w.trigger(Trigger::new(id(3), "onRelay", 1)).unwrap();
    w.advance(33_000, &mut h).unwrap();
    let colors: Vec<_> = h
        .calls
        .iter()
        .filter(|d| d.intent == Intent::Brick(BrickOp::Color(5)))
        .collect();
    assert_eq!(
        colors.len(),
        1,
        "the second relay finds the step switched off"
    );
    assert_eq!(colors[0].origin, 4);
    assert_eq!(w.pending(), 0);
}
/// A button that glows, reverts after 100 ms and cancels itself after 100 ms
/// (Demo Pong's B `+`). A second click inside the 100 ms has its glow and
/// pending rows killed by the first click's late cancel, but the first
/// click's revert still runs after the second glow: the button never stays
/// lit. Rows are scheduled in one queue, earliest due first, whatever
/// origin they came from.
#[test]
fn late_cancel_and_revert_run_in_time_order_across_activations() {
    let mut w = world(Limits::default());
    let mut revert = color("onActivate", 0);
    revert.delay_ms = 100;
    let mut cancel = row("onActivate", "cancelEvents", vec![]);
    cancel.delay_ms = 100;
    w.install_brick(brick(1, vec![color("onActivate", 3), revert, cancel]))
        .unwrap();
    let mut h = FakeHost::default();
    w.trigger(Trigger::new(id(1), "onActivate", 7)).unwrap();
    w.advance(50_000, &mut h).unwrap();
    w.trigger(Trigger::new(id(1), "onActivate", 2)).unwrap();
    let r = w.advance(100_000, &mut h).unwrap();
    assert_eq!(r.cancelled, 2, "the second click's revert and cancel");
    assert_eq!(w.pending(), 0);
    let colors: Vec<_> = h
        .calls
        .iter()
        .map(|d| (d.origin, d.intent.clone()))
        .collect();
    assert_eq!(
        colors,
        vec![
            (7, Intent::Brick(BrickOp::Color(3))),
            (2, Intent::Brick(BrickOp::Color(3))),
            (7, Intent::Brick(BrickOp::Color(0))),
        ]
    );
    // A click after the late cancel came due is untouched by it.
    w.trigger(Trigger::new(id(1), "onActivate", 7)).unwrap();
    w.advance(200_000, &mut h).unwrap();
    assert_eq!(
        h.calls.last().unwrap().intent,
        Intent::Brick(BrickOp::Color(0))
    );
    assert_eq!(w.pending(), 0);
}
#[test]
fn one_owners_zero_delay_loop_stops_at_its_share_and_others_still_run() {
    let mut w = world(Limits {
        cost_per_scope: 50,
        ..Default::default()
    });
    // Owner 1: a zero-delay relay loop. Owner 2: one plain row.
    w.install_brick(brick(
        1,
        vec![color("onRelay", 1), row("onRelay", "fireRelay", vec![])],
    ))
    .unwrap();
    w.install_brick(BrickProgram {
        owner_scope: 2,
        ..brick(2, vec![color("onActivate", 2)])
    })
    .unwrap();
    w.trigger(Trigger::new(id(1), "onRelay", 1)).unwrap();
    w.trigger(Trigger::new(id(2), "onActivate", 2)).unwrap();
    let mut h = FakeHost::default();
    let r = w.advance(0, &mut h).unwrap();
    // Each colour row costs 1; each relay 1 plus the jobs it expands into.
    let (steps, cost) = (r.scopes[&1].steps, r.scopes[&1].cost);
    assert!(
        steps < 50 && (50..53).contains(&cost),
        "{steps} rows, cost {cost}"
    );
    assert!(r.scopes[&1].budget_limited);
    assert_eq!(r.scopes[&2].steps, 1);
    assert_eq!(r.scopes[&2].cost, 1);
    assert!(!r.scopes[&2].budget_limited);
    assert!(h.calls.iter().any(|d| d.source == id(2)));
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.starts_with("owner 1: event budget")),
        "{:?}",
        r.diagnostics
    );
    // The loop carries on where it stopped on the next phase.
    let before = h.calls.len();
    let r = w.advance(1000, &mut h).unwrap();
    assert_eq!(r.scopes[&1].steps, steps);
    assert!(h.calls.len() > before);
}
#[test]
fn the_budget_runs_the_same_rows_on_a_slow_machine() {
    // The same queue on a fast host and on one whose every row takes 1 ms:
    // the budgets count rows, never time them, so each phase runs exactly
    // the same rows, in order.
    let run = |slow: Option<std::time::Duration>| {
        let mut w = world(Limits {
            cost_per_scope: 10,
            cost_per_phase: 40,
            ..Default::default()
        });
        let rows = (0..64).map(|i| color("onActivate", i as u8)).collect();
        w.install_brick(brick(1, rows)).unwrap();
        w.trigger(Trigger::new(id(1), "onActivate", 1)).unwrap();
        let mut h = FakeHost {
            slow,
            ..Default::default()
        };
        let mut phases = Vec::new();
        for phase in 0..7 {
            let before = h.calls.len();
            let r = w.advance(phase * 1000, &mut h).unwrap();
            assert_eq!(r.cost, r.steps);
            phases.push(
                h.calls[before..]
                    .iter()
                    .map(|d| d.row)
                    .collect::<Vec<u16>>(),
            );
        }
        assert_eq!(w.pending(), 0);
        phases
    };
    let fast = run(None);
    assert_eq!(fast, run(Some(std::time::Duration::from_millis(1))));
    // One owner: its share of 10 ends each phase; the rest wait in order.
    assert!(fast[..6].iter().all(|p| p.len() == 10), "{fast:?}");
    assert_eq!(fast.concat(), (0..64).collect::<Vec<u16>>());
}
