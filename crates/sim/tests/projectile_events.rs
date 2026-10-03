//! Real projectiles through the existing event scheduler; no direct dispatch.
use bri_events::{
    rules::{Compare, Condition, Datum, Property, Subject},
    *,
};
use bri_sim::{
    definitions::{Definitions, Special},
    player::MoveInput,
    session::{Command, Notice, Session},
    simulation::Simulation,
};
use bri_world::{Brick, ContentRef, World};
use glam::Vec3;
use rapier3d::prelude::*;

fn row(delay_ms: u32, output: &str, params: Vec<Value>) -> Row {
    Row {
        enabled: true,
        input: "onProjectileHit".into(),
        delay_ms,
        target: Target::Slot(Slot::Projectile),
        output: output.into(),
        params,
        conditions: vec![],
        preserved: None,
    }
}
fn bounce(delay: u32, factor: f32) -> Row {
    row(delay, "Bounce", vec![Value::Float(factor)])
}
fn redirect(delay: u32, vector: Vec3, normalized: bool) -> Row {
    row(
        delay,
        "Redirect",
        vec![Value::Vector(vector), Value::Bool(normalized)],
    )
}
fn catalog() -> Catalog {
    let mut c = bri_events::testing::catalog_extended();
    let mut input = c.inputs[0].clone();
    input.name = "onProjectileHit".into();
    input.id = "in/onProjectileHit".into();
    input
        .targets
        .push(("Projectile".into(), "Projectile".into()));
    c.inputs.push(input);
    let template = c.outputs[0].clone();
    for (name, params) in [
        ("Delete", vec![]),
        ("Explode", vec![]),
        (
            "Bounce",
            vec![Param::Float {
                min: -2.0,
                max: 2.0,
                step: 0.1,
                default: 1.0,
            }],
        ),
        (
            "Redirect",
            vec![Param::Vector { max_length: 200.0 }, Param::Bool],
        ),
    ] {
        c.outputs.push(OutputDef {
            id: format!("out/Projectile/{name}"),
            class_name: "Projectile".into(),
            name: name.into(),
            params,
            ..template.clone()
        });
    }
    c
}
fn session(rows: Vec<Row>, lifetime: u32) -> (Session, u64) {
    let defs = Definitions {
        entries: [(
            "wall".into(),
            bri_sim::testing::definition("wall", [16, 1], 20, Special::None, false),
        )]
        .into(),
    };
    let mut world = World::new(
        "Projectile events".into(),
        "test/map".into(),
        vec![[1.0; 4], [0.0; 4]],
    );
    let mut wall = Brick::new(ContentRef::Resolved("wall".into()), [0.0, 2.0, -4.25], 1);
    wall.events = rows;
    world.bricks.insert(1, wall);
    world.next_brick_id = 2;
    let floor = ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0));
    let mut s = Session::new(Simulation::new(world, defs, vec![floor]).unwrap());
    let mut pack = bri_weapons::testing::pack();
    let p = pack
        .projectiles
        .get_mut(bri_weapons::testing::GUN_PROJECTILE)
        .unwrap();
    p.speed = 12.0;
    p.collide_players = false;
    p.brick.direct = false;
    p.brick.force = 0.0;
    p.lifetime_ticks = lifetime;
    p.fade_ticks = lifetime;
    let explosion = p.explosion.effect.clone();
    assert!(!explosion.is_empty());
    s.set_weapon_pack(pack).unwrap();
    s.set_event_catalog(catalog(), Vec::new()).unwrap();
    let owner = s
        .join("Author".into(), Vec3::new(0.0, 0.05, 0.0), true)
        .unwrap();
    let slot = s.give_item(owner, bri_weapons::testing::GUN_ITEM).unwrap();
    s.equip_tool(owner, Some(slot)).unwrap();
    s.explain_rules(owner, 1).unwrap();
    step(&mut s, owner, 20);
    assert!(s.take_event_diagnostics().is_empty());
    (s, owner)
}
fn step(s: &mut Session, owner: u64, n: u64) {
    for _ in 0..n {
        s.movement(owner, s.simulation().state().tick + 1, MoveInput::default())
            .unwrap();
        s.step().unwrap();
    }
}
fn shot(s: &Session, id: u64) -> Option<bri_weapons::Projectile> {
    s.weapon_view().projectiles.into_iter().find(|p| p.id == id)
}
fn fire(s: &mut Session, owner: u64, sequence: u64) -> u64 {
    let before: Vec<_> = s
        .weapon_view()
        .projectiles
        .into_iter()
        .map(|p| p.id)
        .collect();
    s.command(owner, sequence, Command::WeaponTrigger { down: true })
        .unwrap();
    for _ in 0..30 {
        step(s, owner, 1);
        if let Some(p) = s
            .weapon_view()
            .projectiles
            .into_iter()
            .find(|p| !before.contains(&p.id))
        {
            s.release_trigger(owner).unwrap();
            return p.id;
        }
    }
    panic!("ordinary gun must launch");
}
fn contact(s: &mut Session, owner: u64, id: u64) {
    for _ in 0..120 {
        step(s, owner, 1);
        if shot(s, id).is_some_and(|p| p.velocity.z > 0.0) {
            return;
        }
    }
    panic!(
        "actual contact must bounce: {:?}",
        s.take_event_diagnostics()
    );
}
fn trace(s: &mut Session, owner: u64) -> Vec<String> {
    s.take_private_notices();
    s.explain_rules(owner, 1).unwrap();
    s.take_private_notices()
        .into_iter()
        .filter_map(|(_, n)| {
            if let Notice::Chat(t) = n {
                Some(t)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn immediate_response_still_uses_the_first_row_once() {
    let (mut s, owner) = session(vec![bounce(0, 1.0), redirect(0, Vec3::X, false)], 1200);
    let id = fire(&mut s, owner, 1);
    contact(&mut s, owner, id);
    let p = shot(&s, id).unwrap();
    assert!(p.velocity.z > 11.9 && p.velocity.x.abs() < 0.01);
    assert_eq!(s.pending_events(), 0);
}
#[test]
fn delayed_delete_waits_then_retires_the_original() {
    let mut delete = row(100, "Delete", vec![]);
    delete.conditions.push(Condition {
        subject: Subject::Target,
        property: Property::Exists,
        key: String::new(),
        compare: Compare::Equal,
        value: Datum::Bool(true),
    });
    let (mut s, owner) = session(vec![bounce(0, 1.0), delete], 1200);
    let id = fire(&mut s, owner, 1);
    contact(&mut s, owner, id);
    assert!(s.pending_events() > 0);
    step(&mut s, owner, 11);
    assert!(shot(&s, id).is_some());
    step(&mut s, owner, 1);
    assert!(shot(&s, id).is_none());
    assert_eq!(s.pending_events(), 0);
}
#[test]
fn delayed_redirects_use_current_speed_and_authored_same_tick_order() {
    let (mut s, owner) = session(
        vec![
            bounce(0, 0.5),
            redirect(100, Vec3::Y * 4.0, false),
            redirect(100, Vec3::X, true),
        ],
        1200,
    );
    let id = fire(&mut s, owner, 1);
    contact(&mut s, owner, id);
    assert!((shot(&s, id).unwrap().velocity.z - 6.0).abs() < 0.01);
    step(&mut s, owner, 12);
    let p = shot(&s, id).unwrap();
    assert!(p.velocity.distance(Vec3::X * 4.0) < 0.01);
    assert_eq!(s.pending_events(), 0);
}
#[test]
fn delayed_bounce_uses_current_velocity_and_the_original_contact_normal() {
    let (mut s, owner) = session(vec![bounce(0, 0.5), bounce(100, 0.5)], 1200);
    let id = fire(&mut s, owner, 1);
    contact(&mut s, owner, id);
    step(&mut s, owner, 12);
    assert!((shot(&s, id).unwrap().velocity.z + 3.0).abs() < 0.01);
}
#[test]
fn guards_observe_due_time_state_instead_of_the_hit_snapshot() {
    for expected in [0, 1] {
        let mut color = row(50, "setColor", vec![Value::Color(1)]);
        color.target = Target::Slot(Slot::SelfBrick);
        let mut delete = row(100, "Delete", vec![]);
        delete.conditions.push(Condition {
            subject: Subject::SelfBrick,
            property: Property::Color,
            key: String::new(),
            compare: Compare::Equal,
            value: Datum::Number(expected),
        });
        let (mut s, owner) = session(vec![bounce(0, 1.0), color, delete], 1200);
        let id = fire(&mut s, owner, 1);
        contact(&mut s, owner, id);
        step(&mut s, owner, 12);
        assert_eq!(shot(&s, id).is_none(), expected == 1);
        assert!(
            trace(&mut s, owner)
                .iter()
                .any(|t| t.contains(if expected == 1 { "pass" } else { "skipped" }))
        );
    }
}
#[test]
fn natural_impact_retirement_is_explained_and_never_retargets_a_new_shot() {
    let (mut s, owner) = session(vec![redirect(100, Vec3::X, false)], 1200);
    let first = fire(&mut s, owner, 1);
    for _ in 0..120 {
        if shot(&s, first).is_none() {
            break;
        }
        step(&mut s, owner, 1);
    }
    assert!(shot(&s, first).is_none());
    assert!(s.pending_events() > 0);
    let second = fire(&mut s, owner, 2);
    assert_ne!(first, second);
    step(&mut s, owner, 12);
    let p = shot(&s, second).unwrap();
    assert!(p.velocity.z < 0.0 && p.velocity.x.abs() < 0.01);
    assert!(
        trace(&mut s, owner)
            .iter()
            .any(|t| t.contains(&format!("original projectile {first} no longer exists")))
    );
}
#[test]
fn natural_lifetime_expiry_does_not_keep_a_projectile_alive_for_the_delay() {
    let (mut s, owner) = session(vec![bounce(0, 1.0), row(700, "Delete", vec![])], 60);
    let id = fire(&mut s, owner, 1);
    contact(&mut s, owner, id);
    step(&mut s, owner, 59);
    assert!(shot(&s, id).is_some());
    step(&mut s, owner, 1);
    assert!(shot(&s, id).is_none());
    assert!(s.pending_events() > 0);
    step(&mut s, owner, 25);
    assert_eq!(s.pending_events(), 0);
    assert!(
        trace(&mut s, owner)
            .iter()
            .any(|t| t.contains("original projectile"))
    );
}
#[test]
fn delayed_explode_uses_the_existing_explosion_phase() {
    let (mut s, owner) = session(vec![bounce(0, 1.0), row(100, "Explode", vec![])], 1200);
    let id = fire(&mut s, owner, 1);
    contact(&mut s, owner, id);
    s.take_cues();
    step(&mut s, owner, 12);
    assert!(shot(&s, id).is_none());
    assert!(!s.take_cues().iter().any(|c|matches!(&c.kind,bri_sim::presentation::CueKind::WeaponEffect { definition,.. } if definition=="testGunExplosion")));
    step(&mut s, owner, 1);
    assert!(s.take_cues().iter().any(|c|matches!(&c.kind,bri_sim::presentation::CueKind::WeaponEffect { definition,.. } if definition=="testGunExplosion")));
}

#[test]
fn immediate_guards_stay_rejected_but_delayed_guards_are_admitted() {
    let mut r = row(0, "Delete", vec![]);
    r.conditions.push(Condition {
        subject: Subject::Target,
        property: Property::Exists,
        key: String::new(),
        compare: Compare::Equal,
        value: Datum::Bool(true),
    });
    let bindings = Bindings {
        palette_len: 2,
        ..Default::default()
    };
    assert!(catalog().validate_row(&r, &bindings).is_err());
    r.delay_ms = 1;
    assert!(catalog().validate_row(&r, &bindings).is_ok());
}
