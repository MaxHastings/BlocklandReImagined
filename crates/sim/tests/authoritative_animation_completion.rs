//! Design diagnostic: actual Session producers, no test-local clock/completion.
//! Strict ignored requirements must be corrected before integrating this branch.
use anyhow::{Context, Result};
use bri_content::{animation::sample, shape::Shape};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    presentation::{Cue, CueKind},
    session::{Command, PackageCommand, Session},
    simulation::Simulation,
};
use bri_weapons::Pack;
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::sync::Arc;

const SCRIPT: &str = r#"
fn cmd_body(p) {
    set_archetype(p, "clockbody:archetype/human");
    set_player(p, "busy", true);
    require(play_thread(p, 2, "body_cycle"));
}
// Proposed host-only callback. Current Behaviour has no hook declaration or
// dispatcher for it; merely writing this function cannot authorize completion.
// The later approved request/hook fixture must opt in explicitly.
fn on_animation_result(p, result) {
    if result.outcome == "natural" {
        set_player(p, "results", get_player(p, "results") + 1);
        set_player(p, "busy", false);
    }
}
"#;

fn native_shape() -> Shape {
    let clip = |name| {
        json!({
            "name":name,"frames":2,"duration":0.25,"looping":false,
            "additive":false,"priority":0,
            "nodes":[{"node":"root","translations":[[0.,0.,0.],[1.,0.,0.]],
                "rotations":[],"scales":[],"scale_rotations":[]}],
            "objects":[],"ground_translations":[],"ground_rotations":[],"triggers":[]
        })
    };
    serde_json::from_value(json!({
        "schema_version":1,"id":"clockbody:body/native",
        "nodes":[{"name":"root","parent":null,
            "translation":[0.,0.,0.],"rotation":[0.,0.,0.,1.]}],
        "objects":[],"details":[],"meshes":[],"materials":[],
        "animations":[clip("body_cycle"),clip("item_cycle")]
    }))
    .unwrap()
}

fn fixture() -> Result<(tempfile::TempDir, Session, u64, usize, usize)> {
    let root = tempfile::tempdir()?;
    let body = root.path().join("clockbody");
    let rules = root.path().join("clockrules");
    std::fs::create_dir_all(&body)?;
    std::fs::create_dir_all(&rules)?;
    let shape = native_shape();
    shape.validate()?;
    std::fs::write(body.join("shape.json"), serde_json::to_vec(&shape)?)?;
    std::fs::write(
        body.join("human.json"),
        json!({
            "schema_version":1,"name":"Invented clock body","model":"clockbody:body/native"
        })
        .to_string(),
    )?;
    std::fs::write(
        body.join("package.json"),
        json!({
            "schema_version":1,"id":"clockbody","version":"1.0.0","api":1,
            "name":"Invented body","license":"CC0-1.0","provides":[
                {"kind":"body","id":"clockbody:body/native","file":"shape.json"},
                {"kind":"archetype","id":"clockbody:archetype/human","file":"human.json"}]
        })
        .to_string(),
    )?;
    std::fs::write(rules.join("package.json"), json!({
        "schema_version":1,"id":"clockrules","version":"1.0.0","api":1,
        "name":"Invented rules","license":"CC0-1.0",
        "dependencies":{"clockbody":"^1.0.0"},"capabilities":["player","effects"],
        "provides":[{"kind":"behaviour","id":"clockrules:behaviour/main","file":"behaviour.json"},
            {"kind":"script","id":"clockrules:script/main","file":"main.rhai"}]
    }).to_string())?;
    std::fs::write(
        rules.join("behaviour.json"),
        json!({
            "schema_version":1,"script":"main.rhai","commands":[{"name":"body"}],
            "state":{"player":{
                "busy":{"default":false,"visible":"server","persist":false},
                "results":{"default":0,"visible":"server","persist":false}}}
        })
        .to_string(),
    )?;
    std::fs::write(rules.join("main.rhai"), SCRIPT)?;
    let catalog = Catalog::load(
        root.path(),
        &PackageSet {
            schema_version: 1,
            packages: vec![
                PackageEntry {
                    id: "clockbody".into(),
                    version: "1.0.0".into(),
                    side: Side::Shared,
                    dir: "clockbody".into(),
                    role: None,
                },
                PackageEntry {
                    id: "clockrules".into(),
                    version: "1.0.0".into(),
                    side: Side::Server,
                    dir: "clockrules".into(),
                    role: None,
                },
            ],
        },
        true,
    )
    .map_err(|e| anyhow::anyhow!("{e:#?}"))?;
    let mut session = Session::new(Simulation::new(
        World::new(
            "Invented clocks".into(),
            "test:clock-map".into(),
            vec![[1.; 4]],
        ),
        Definitions::default(),
        vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
    )?);
    session.install_packages(Arc::new(catalog), None)?;
    session.set_weapon_pack(Pack::from_json(serde_json::to_vec(&json!({
        "schema_version":5,"id":"invented-clock",
        "items":{
            "clockbody:weapon/first":{"ui_name":"First","image":"clockbody:image/shared"},
            "clockbody:weapon/second":{"ui_name":"Second","image":"clockbody:image/shared"}},
        "images":{"clockbody:image/shared":{
            "model":"clockbody:body/native","projectile":"clockbody:projectile/shot",
            "states":[{"name":"Activate","ticks":1,"timeout":1},{"name":"Ready","down":2},
                {"name":"Fire","ticks":1,"script":"onFire","sequence":"item_cycle","arm":"body_cycle","timeout":3},
                {"name":"Wait","up":1}]}},
        "projectiles":{"clockbody:projectile/shot":{"speed":10.,"lifetime_ticks":5}}
    }))?.as_slice())?)?;
    let owner = session.join("Clock owner".into(), Vec3::new(0., 0.05, 0.), true)?;
    let first = session.give_item(owner, "clockbody:weapon/first")?;
    let second = session.give_item(owner, "clockbody:weapon/second")?;
    session.equip_tool(owner, Some(first))?;
    steps(&mut session, owner, 4)?;
    session.take_cues();
    Ok((root, session, owner, first, second))
}

fn steps(session: &mut Session, owner: u64, count: u64) -> Result<()> {
    for _ in 0..count {
        session.movement(
            owner,
            session.simulation().state().tick + 100,
            MoveInput::default(),
        )?;
        session.step()?;
    }
    Ok(())
}

fn body_start(session: &mut Session, owner: u64) -> Result<Cue> {
    session.command(
        owner,
        1,
        Command::Package(PackageCommand {
            package: "clockrules".into(),
            command: "body".into(),
            args: vec![],
        }),
    )?;
    let snapshot = session.snapshot();
    let player = snapshot
        .players
        .iter()
        .find(|p| p.owner == owner)
        .context("Missing actual body")?;
    assert_eq!(
        session.archetypes().resolve(player.archetype).look.model,
        "clockbody:body/native"
    );
    session
        .take_cues()
        .into_iter()
        .find(|cue| {
            matches!(&cue.kind,
        CueKind::WeaponAnimation { actor,sequence,image_hand:None,.. }
            if *actor==owner && sequence=="body_cycle")
        })
        .context("Actual Session play_thread did not emit its body animation")
}

fn image_start(session: &mut Session, owner: u64, sequence: u64) -> Result<Cue> {
    session.command(owner, sequence, Command::WeaponTrigger { down: true })?;
    steps(session, owner, 2)?;
    session
        .take_cues()
        .into_iter()
        .find(|cue| {
            matches!(&cue.kind,
        CueKind::WeaponAnimation { actor,sequence,image_hand:Some(0),.. }
            if *actor==owner && sequence=="item_cycle")
        })
        .context("Actual weapon state-entry producer did not emit its image animation")
}

#[test]
fn actual_body_producer_runs_but_authored_endpoint_does_not_finish_package_busy() -> Result<()> {
    let (_root, mut session, owner, _, _) = fixture()?;
    let cue = body_start(&mut session, owner)?;
    cue.validate()?;
    let shape = native_shape();
    let clip = &shape.animations[0];
    assert_eq!(
        sample(&shape, Some(clip), clip.duration)?.nodes,
        sample(&shape, Some(clip), 5.)?.nodes
    );
    let life = session.spawn_tick(owner).context("Missing actual life")?;
    steps(&mut session, owner, 600)?;
    assert_eq!(session.spawn_tick(owner), Some(life));
    assert_eq!(
        session.package_value("clockrules", owner, "busy"),
        Some(json!(true))
    );
    assert_eq!(
        session.package_value("clockrules", owner, "results"),
        Some(json!(0))
    );
    assert!(session.is_alive(owner));
    assert!(session.take_package_problems().is_empty());
    Ok(())
}

#[test]
fn actual_same_image_from_distinct_slots_has_no_incarnation_in_animation_payload() -> Result<()> {
    let (_root, mut session, owner, first, second) = fixture()?;
    let a = image_start(&mut session, owner, 1)?;
    session.command(owner, 2, Command::WeaponTrigger { down: false })?;
    steps(&mut session, owner, 3)?;
    session.equip_tool(owner, Some(second))?;
    let b = image_start(&mut session, owner, 3)?;
    assert_ne!(first, second);
    assert_eq!(a.kind, b.kind);
    assert_ne!(
        a.id, b.id,
        "Cue order exists, but is not an admitted playback token"
    );
    assert_eq!(session.tool_inventories()[&owner].selected, Some(second));
    Ok(())
}

#[test]
#[ignore = "design requirement: actual body producer lacks admitted playback identity"]
fn actual_body_start_retains_validated_clip_and_life_identity() -> Result<()> {
    let (_root, mut session, owner, _, _) = fixture()?;
    let cue = body_start(&mut session, owner)?;
    let value = serde_json::to_value(&cue.kind)?;
    let playback = &value["WeaponAnimation"]["playback"];
    assert!(
        playback.is_object(),
        "Actual body cue has no admitted playback correlation: {value}"
    );
    assert_eq!(playback["owner"], owner);
    assert_eq!(playback["spawn_tick"], session.spawn_tick(owner).unwrap());
    assert!(playback["serial"].as_u64().is_some_and(|n| n > 0));
    Ok(())
}

#[test]
#[ignore = "design requirement: actual image producer lacks equipped item incarnation"]
fn actual_image_start_retains_item_instance_separate_from_body_and_hand() -> Result<()> {
    let (_root, mut session, owner, first, _) = fixture()?;
    let cue = image_start(&mut session, owner, 1)?;
    let value = serde_json::to_value(&cue.kind)?;
    let playback = &value["WeaponAnimation"]["playback"];
    assert!(
        playback.is_object(),
        "Actual image cue has no admitted playback correlation: {value}"
    );
    assert_eq!(playback["owner"], owner);
    assert_eq!(playback["item"]["slot"], first);
    assert!(
        playback["item"]["generation"]
            .as_u64()
            .is_some_and(|n| n > 0)
    );
    assert!(playback["mount_generation"].as_u64().is_some_and(|n| n > 0));
    Ok(())
}
