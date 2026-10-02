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
        .owners
        .insert(7, OwnerRecord::new([0xaa; 32], "Builder".into()));
    world
}
#[test]
fn snapshot_options_preserve_unknown_records_without_replaying_queue() {
    let world = source();
    let full = SavedBuild::capture(&world, true, true).unwrap();
    assert_eq!(full.world.bricks, world.bricks);
    assert_eq!(full.world.owners, world.owners);
    assert_eq!(
        bri_world::build::decode(&bri_world::build::encode(&full).unwrap()).unwrap(),
        full
    );
    let stripped = SavedBuild::capture(&world, false, false).unwrap();
    assert!(stripped.world.owners.is_empty());
    for brick in stripped.world.bricks.values() {
        assert_eq!(brick.owner, 0);
        assert!(brick.events.is_empty());
        assert_eq!(brick.source_records.len(), 1);
        assert_eq!(brick.source_records[0].text, "+-UNKNOWN retained");
        assert!(brick.print.is_some());
    }
    let imported = bri_world::build::decode(&serde_json::to_vec(&world).unwrap()).unwrap();
    assert_eq!(imported.world, world);
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
    let saved = SavedBuild::capture(&source(), true, true).unwrap();
    let plan = || LoadPlan::prepare(&target, saved.clone(), 1, true, 3).unwrap();
    assert_eq!(plan().next_owner, 5);
    // Owner 7 has a principal new to the target: it is claimed with the
    // fresh number. Owner 9 has none and stays unclaimed.
    assert_eq!(plan().owners().keys().copied().collect::<Vec<_>>(), vec![3]);
    let mut authority = Authority::new(target.clone()).unwrap();
    assert!(
        authority
            .load_build(
                &Actor {
                    owner: 1,
                    administrator: false,
                    ..Default::default()
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
                    administrator: true,
                    ..Default::default()
                },
                plan()
            )
            .unwrap(),
        vec![2, 3]
    );
    let state = authority.state();
    assert_eq!(state.bricks[&2].owner, 3);
    assert_eq!(state.bricks[&3].owner, 4);
    assert_eq!(state.owners[&3].principal, "aa".repeat(32));
    assert!(!state.owners.contains_key(&4));
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
                    administrator: true,
                    ..Default::default()
                },
                stale
            )
            .is_err()
    );
    assert_eq!(authority.state(), &committed);
    // A world where that player already has a number gives it back.
    let mut known = target.clone();
    known
        .owners
        .insert(2, OwnerRecord::new([0xaa; 32], "Renamed".into()));
    let same = LoadPlan::prepare(&known, saved.clone(), 1, true, 3).unwrap();
    assert_eq!(same.bricks()[&2].owner, 2);
    assert_eq!(same.bricks()[&3].owner, 3);
    assert!(same.owners().is_empty());
    assert_eq!(same.next_owner, 4);
    let reassigned = LoadPlan::prepare(&target, saved, 1, false, 3).unwrap();
    assert!(reassigned.bricks().values().all(|b| b.owner == 1));
}
