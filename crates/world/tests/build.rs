use bri_world::{
    authority::{Actor, Authority},
    build::{LoadPlan, SavedBuild},
    *,
};

fn source() -> World {
    let mut world = World::new(
        "Source".into(),
        "map".into(),
        vec![[0.2, 0.4, 0.6, 1.0], [1.0; 4]],
    );
    let mut brick = Brick::new(ContentRef::Resolved("plate".into()), [0.5, 0.1, -3.25], 7);
    brick.name = Some("trigger".into());
    brick.print = Some(ContentRef::Resolved("Letters/A".into()));
    brick.events.push(EventRow {
        preserved: None,
        enabled: true,
        input: "onActivate".into(),
        delay_ms: 0,
        target: EventTarget::Named("trigger".into()),
        output: "setColor".into(),
        params: vec![EventValue::Color(1)],
    });
    for text in [
        "+-EVENT unsupported preserved",
        "+-OWNER 7",
        "+-UNKNOWN retained",
    ] {
        brick.source_records.push(SourceRecord {
            line: 1,
            text: text.into(),
            diagnostic: Some("retained".into()),
        });
    }
    world.bricks.insert(4, brick.clone());
    brick.owner = 9;
    world.bricks.insert(8, brick);
    world.next_brick_id = 9;
    world
}
#[test]
fn snapshot_options_preserve_unknown_records_without_replaying_queue() {
    let world = source();
    let full = SavedBuild::capture(&world, Some("scope".into()), true, true).unwrap();
    assert_eq!(full.world.bricks, world.bricks);
    assert_eq!(
        bri_world::build::decode(&bri_world::build::encode(&full).unwrap()).unwrap(),
        full
    );
    let stripped = SavedBuild::capture(&world, Some("scope".into()), false, false).unwrap();
    assert!(stripped.ownership_scope.is_none());
    for brick in stripped.world.bricks.values() {
        assert_eq!(brick.owner, 0);
        assert!(brick.events.is_empty());
        assert_eq!(brick.source_records.len(), 1);
        assert_eq!(brick.source_records[0].text, "+-UNKNOWN retained");
        assert!(brick.print.is_some());
    }
    let imported = bri_world::build::decode(&serde_json::to_vec(&world).unwrap()).unwrap();
    assert!(imported.ownership_scope.is_none());
    assert_eq!(world, source(), "Capture changed source");
}
#[test]
fn append_remaps_ids_exact_colors_and_owners_and_rejects_stale_or_unprivileged_commit() {
    let mut target = World::new("Target".into(), "different-map".into(), vec![[1.0; 4]]);
    target.bricks.insert(
        1,
        Brick::new(ContentRef::Resolved("plate".into()), [0.0; 3], 1),
    );
    target.next_brick_id = 2;
    let saved = SavedBuild::capture(&source(), Some("old".into()), true, true).unwrap();
    let plan = || LoadPlan::prepare(&target, saved.clone(), 1, true, Some("new"), 3).unwrap();
    assert_eq!(plan().next_owner, 5);
    let mut authority = Authority::new(target.clone()).unwrap();
    assert!(
        authority
            .load_build(
                &Actor {
                    owner: 1,
                    administrator: false
                },
                plan()
            )
            .is_err()
    );
    assert_eq!(authority.state(), &target);
    let stale = plan();
    assert_eq!(
        authority
            .load_build(
                &Actor {
                    owner: 1,
                    administrator: true
                },
                plan()
            )
            .unwrap(),
        vec![2, 3]
    );
    let state = authority.state();
    assert_eq!(state.bricks[&2].owner, 3);
    assert_eq!(state.bricks[&3].owner, 4);
    assert_eq!(state.bricks[&1], target.bricks[&1]);
    assert_eq!(state.bricks[&2].color, 1);
    assert_eq!(
        state.bricks[&2].events[0].params,
        vec![EventValue::Color(0)]
    );
    assert_eq!(state.palette[1], source().palette[0]);
    let committed = state.clone();
    assert!(
        authority
            .load_build(
                &Actor {
                    owner: 1,
                    administrator: true
                },
                stale
            )
            .is_err()
    );
    assert_eq!(authority.state(), &committed);
    let same = LoadPlan::prepare(&target, saved.clone(), 1, true, Some("old"), 3).unwrap();
    assert_eq!(same.bricks()[&2].owner, 7);
    assert_eq!(same.next_owner, 10);
    let reassigned = LoadPlan::prepare(&target, saved, 1, false, Some("new"), 3).unwrap();
    assert!(reassigned.bricks().values().all(|b| b.owner == 1));
}
