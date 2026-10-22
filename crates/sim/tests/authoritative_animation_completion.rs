//! Actual Session producer admission and native host clock/lifecycle controls.
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
fn cmd_other(p) { set_archetype(p, "clockbody:archetype/other"); }
fn cmd_births(p) { respawn(p); respawn(p); }
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
    let mut zero = clip("zero_cycle");
    zero["duration"] = json!(0.0);
    let mut looping = clip("loop_cycle");
    looping["looping"] = json!(true);
    let mut item = clip("item_cycle");
    item["duration"] = json!(0.015625);
    serde_json::from_value(json!({
        "schema_version":1,"id":"clockbody:body/native",
        "nodes":[{"name":"root","parent":null,
            "translation":[0.,0.,0.],"rotation":[0.,0.,0.,1.]}],
        "objects":[],"details":[],"meshes":[],"materials":[],
        "animations":[clip("body_cycle"),item,zero,looping]
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
    let mut other = shape.clone();
    other.id = "clockbody:body/other".into();
    std::fs::write(body.join("other-shape.json"), serde_json::to_vec(&other)?)?;
    std::fs::write(
        body.join("other.json"),
        json!({
            "schema_version":1,"name":"Other invented body","model":"clockbody:body/other"
        })
        .to_string(),
    )?;
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
            {"kind":"body","id":"clockbody:body/other","file":"other-shape.json"},
            {"kind":"archetype","id":"clockbody:archetype/other","file":"other.json"},
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
        "schema_version":1,"script":"main.rhai","commands":[{"name":"body"},{"name":"other"},{"name":"births"}],
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
                {"name":"Fire","ticks":1,"script":"onFire","sequence":"item_cycle","arm":"body_cycle","timeout":3,
                    "cues":[{"after_ms":100,"thread":2,"sequence":"body_cycle"}]},
                {"name":"Wait","up":1}]}},
        "projectiles":{"clockbody:projectile/shot":{"speed":10.,"lifetime_ticks":5}}
    }))?.as_slice())?)?;
    let bytes = serde_json::to_vec(&shape)?;
    let other_bytes = serde_json::to_vec(&other)?;
    session.set_animation_assets(bri_sim::session::AnimationAssets {
        assets: vec![
            bri_sim::session::AnimationAsset {
                resource: "clockbody:body/native".into(),
                digest: bri_package::environment::hash_bytes(&bytes),
                rig: false,
                library_clips: Vec::new(),
                bytes,
            },
            bri_sim::session::AnimationAsset {
                resource: "clockbody:body/other".into(),
                digest: bri_package::environment::hash_bytes(&other_bytes),
                rig: false,
                library_clips: Vec::new(),
                bytes: other_bytes,
            },
        ],
        images: [(
            "clockbody:image/shared".into(),
            bri_sim::session::AnimationImageAsset {
                model: "clockbody:body/native".into(),
                resource: "clockbody:body/native".into(),
            },
        )]
        .into(),
    })?;
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
fn actual_body_start_retains_validated_clip_and_life_identity() -> Result<()> {
    use bri_sim::session::{AnimationOutcome, AnimationTarget};
    let (_root, mut session, owner, _, _) = fixture()?;
    let cue = body_start(&mut session, owner)?;
    let view = session.animation_view();
    let playback = view
        .active
        .iter()
        .find(|a| a.id.owner == owner && a.request.target == AnimationTarget::Pawn(2))
        .context("Actual body start has no durable host playback")?;
    assert_eq!(playback.id.epochs, session.animation_owner(owner).unwrap());
    assert_eq!(playback.started_tick, cue.tick);
    assert_eq!(playback.clip.resource, "clockbody:body/native");
    assert_eq!(playback.clip.sequence, "body_cycle");
    assert!(playback.id.serial > 0);
    steps(&mut session, owner, 30)?;
    assert!(session.animation_view().active.is_empty());
    let records = session.take_animation_records();
    assert_eq!(
        records
            .iter()
            .filter(|r| r.outcome == AnimationOutcome::Natural)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn actual_image_start_retains_item_instance_separate_from_body_and_hand() -> Result<()> {
    use bri_sim::session::AnimationTarget;
    let (_root, mut session, owner, first, _) = fixture()?;
    let cue = image_start(&mut session, owner, 1)?;
    let view = session.animation_view();
    let playback = view
        .active
        .iter()
        .find(|a| {
            a.id.owner == owner
                && a.request.target
                    == (AnimationTarget::Image {
                        hand: 0,
                        emote: false,
                    })
        })
        .context("Actual image start has no durable host playback")?;
    assert_eq!(playback.started_tick, cue.tick);
    assert_eq!(playback.request.binding.item.as_ref().unwrap().slot, first);
    assert!(playback.request.binding.item.as_ref().unwrap().generation > 0);
    assert!(playback.request.binding.image.as_ref().unwrap().generation > 0);
    assert_eq!(playback.clip.sequence, "item_cycle");
    Ok(())
}

fn host_fixture() -> Result<(tempfile::TempDir, Session, u64)> {
    let (root, mut session, owner, _, _) = fixture()?;
    body_start(&mut session, owner)?;
    let id = session.animation_view().active[0].id.clone();
    session.cancel_animation(&id)?;
    session.take_animation_records();
    Ok((root, session, owner))
}
fn request(session: &Session, owner: u64, nonce: u64) -> bri_sim::session::AnimationStart {
    bri_sim::session::AnimationStart {
        namespace: "test-clock".into(),
        nonce,
        target: bri_sim::session::AnimationTarget::Pawn(15),
        sequence: "body_cycle".into(),
        binding: bri_weapons::AnimationBinding {
            owner: session.animation_owner(owner),
            ..Default::default()
        },
        rate: 1.0,
        tween: 0.0,
    }
}
fn start(
    session: &mut Session,
    owner: u64,
    value: bri_sim::session::AnimationStart,
) -> Result<bri_sim::session::PlaybackId> {
    let prepared = session.prepare_animation_start(owner, value)?;
    session.commit_animation_start(prepared)
}
fn package_command(session: &mut Session, owner: u64, sequence: u64, name: &str) -> Result<()> {
    session.command(
        owner,
        sequence,
        Command::Package(PackageCommand {
            package: "clockrules".into(),
            command: name.into(),
            args: vec![],
        }),
    )?;
    Ok(())
}

#[test]
fn failed_preparation_and_stale_ticket_do_not_mutate_clock_or_consume_nonce() -> Result<()> {
    let (_root, mut session, owner) = host_fixture()?;
    let good = request(&session, owner, 1);
    let before = session.animation_view();
    let mut bads = vec![];
    for rate in [f64::NAN, -1.0, 1001.0] {
        let mut bad = good.clone();
        bad.rate = rate;
        bads.push(bad);
    }
    let mut bad = good.clone();
    bad.tween = 0.05;
    bads.push(bad);
    let mut bad = good.clone();
    bad.sequence = "absent".into();
    bads.push(bad);
    let mut bad = good.clone();
    bad.binding.owner.as_mut().unwrap().body += 1;
    bads.push(bad);
    for bad in bads {
        assert!(session.prepare_animation_start(owner, bad).is_err());
        assert_eq!(session.animation_view(), before);
        assert!(session.take_animation_records().is_empty());
    }
    let ticket = session.prepare_animation_start(owner, good.clone())?;
    let mut other = good.clone();
    other.namespace = "other-clock".into();
    other.target = bri_sim::session::AnimationTarget::Pawn(14);
    start(&mut session, owner, other)?;
    let before_stale = session.animation_view();
    assert!(session.commit_animation_start(ticket).is_err());
    assert_eq!(session.animation_view(), before_stale);
    let id = start(&mut session, owner, good.clone())?;
    session.take_animation_records();
    assert_eq!(start(&mut session, owner, good.clone())?, id);
    assert!(session.take_animation_records().is_empty());
    let mut reused = good;
    reused.rate = 2.0;
    assert!(session.prepare_animation_start(owner, reused).is_err());
    Ok(())
}

#[test]
fn evicted_nonce_cannot_restart_and_fresh_nonce_can() -> Result<()> {
    let (_root, mut session, owner) = host_fixture()?;
    for nonce in 1..=300 {
        let value = request(&session, owner, nonce);
        let id = start(&mut session, owner, value)?;
        session.cancel_animation(&id)?;
        assert_eq!(session.take_animation_records().len(), 2);
    }
    let before = session.animation_view();
    assert!(
        session
            .prepare_animation_start(owner, request(&session, owner, 1))
            .is_err()
    );
    assert_eq!(session.animation_view(), before);
    let fresh = request(&session, owner, 301);
    start(&mut session, owner, fresh)?;
    Ok(())
}

#[test]
fn pause_resume_and_looping_have_truthful_native_phase() -> Result<()> {
    use bri_sim::session::AnimationOutcome;
    let (_root, mut session, owner) = host_fixture()?;
    let value = request(&session, owner, 1);
    let id = start(&mut session, owner, value)?;
    steps(&mut session, owner, 10)?;
    session.set_animation_rate(&id, 0.0)?;
    let phase = session.animation_view().active[0].phase;
    assert!((phase - 10.0 / 120.0).abs() < 1e-12);
    steps(&mut session, owner, 100)?;
    assert_eq!(session.animation_view().active[0].phase, phase);
    session.set_animation_rate(&id, 2.0)?;
    steps(&mut session, owner, 10)?;
    assert!(session.animation_view().active.is_empty());
    assert_eq!(
        session
            .take_animation_records()
            .iter()
            .filter(|r| r.outcome == AnimationOutcome::Natural)
            .count(),
        1
    );
    let mut value = request(&session, owner, 2);
    value.sequence = "loop_cycle".into();
    let loop_id = start(&mut session, owner, value)?;
    steps(&mut session, owner, 100)?;
    assert_eq!(session.animation_view().active[0].id, loop_id);
    assert!(
        session
            .take_animation_records()
            .iter()
            .all(|r| r.outcome != AnimationOutcome::Natural)
    );
    Ok(())
}

#[test]
fn full_outbox_rejects_new_start_without_wedging_reserved_natural_result() -> Result<()> {
    use bri_sim::session::AnimationOutcome;
    let (_root, mut session, owner) = host_fixture()?;
    let mut admitted = 0;
    for nonce in 1..=1000 {
        let before = session.animation_view();
        let value = request(&session, owner, nonce);
        match session.prepare_animation_start(owner, value) {
            Ok(ticket) => {
                session.commit_animation_start(ticket)?;
                admitted += 1;
            }
            Err(error) => {
                assert!(error.to_string().contains("budget"));
                assert_eq!(session.animation_view(), before);
                break;
            }
        }
    }
    assert!(admitted > 0 && admitted < 1000);
    steps(&mut session, owner, 30)?;
    assert!(session.animation_view().active.is_empty());
    let records = session.take_animation_records();
    assert_eq!(records.len(), admitted * 2);
    assert_eq!(
        records
            .iter()
            .filter(|r| r.outcome == AnimationOutcome::Started)
            .count(),
        admitted
    );
    assert_eq!(
        records
            .iter()
            .filter(|r| r.outcome == AnimationOutcome::Natural)
            .count(),
        1
    );
    let value = request(&session, owner, 1001);
    start(&mut session, owner, value)?;
    Ok(())
}

#[test]
fn same_tick_zero_duration_replacement_reserves_started_and_both_terminal_records() -> Result<()> {
    use bri_sim::session::AnimationOutcome;
    let (_root, mut session, owner) = host_fixture()?;
    let value = request(&session, owner, 1);
    let old = start(&mut session, owner, value)?;
    session.take_animation_records();
    let mut value = request(&session, owner, 2);
    value.sequence = "zero_cycle".into();
    let new = start(&mut session, owner, value)?;
    let records = session.take_animation_records();
    assert_eq!(
        records.iter().map(|r| r.outcome).collect::<Vec<_>>(),
        [
            AnimationOutcome::Interrupted,
            AnimationOutcome::Started,
            AnimationOutcome::Natural
        ]
    );
    assert_eq!(records[0].playback.id, old);
    assert_eq!(records[1].playback.id, new);
    assert_eq!(records[2].playback.id, new);
    assert!(records.iter().all(|r| r.tick == records[0].tick));
    assert!(session.animation_view().active.is_empty());
    assert!(session.take_animation_records().is_empty());
    Ok(())
}

#[test]
fn two_same_tick_body_replacements_and_respawns_never_reuse_life_or_body_epoch() -> Result<()> {
    let (_root, mut session, owner) = host_fixture()?;
    let before = session.animation_owner(owner).unwrap();
    let tick = session.simulation().state().tick;
    let value = request(&session, owner, 1);
    let old = start(&mut session, owner, value.clone())?;
    package_command(&mut session, owner, 2, "other")?;
    let other = session.animation_owner(owner).unwrap();
    package_command(&mut session, owner, 3, "body")?;
    let back = session.animation_owner(owner).unwrap();
    assert_eq!(session.simulation().state().tick, tick);
    assert_eq!(before.life, other.life);
    assert_eq!(before.life, back.life);
    assert!(before.body < other.body && other.body < back.body);
    assert!(session.prepare_animation_start(owner, value).is_err());
    assert!(session.take_animation_records().iter().any(
        |r| r.playback.id == old && r.outcome == bri_sim::session::AnimationOutcome::Cancelled
    ));
    package_command(&mut session, owner, 4, "births")?;
    let after = session.animation_owner(owner).unwrap();
    assert_eq!(session.simulation().state().tick, tick);
    assert!(after.life >= back.body + 2);
    assert_eq!(after.body, after.life);
    assert_eq!(session.spawn_tick(owner), Some(tick));
    Ok(())
}

fn catalog(shape: bri_content::shape::Shape) -> Result<bri_sim::session::AnimationAssets> {
    use bri_sim::session::{AnimationAsset, AnimationAssets, AnimationImageAsset};
    let bytes = serde_json::to_vec(&shape)?;
    let mut other = shape;
    other.id = "clockbody:body/other".into();
    let other_bytes = serde_json::to_vec(&other)?;
    Ok(AnimationAssets {
        assets: vec![
            AnimationAsset {
                resource: "clockbody:body/native".into(),
                digest: bri_package::environment::hash_bytes(&bytes),
                rig: false,
                library_clips: Vec::new(),
                bytes,
            },
            AnimationAsset {
                resource: "clockbody:body/other".into(),
                digest: bri_package::environment::hash_bytes(&other_bytes),
                rig: false,
                library_clips: Vec::new(),
                bytes: other_bytes,
            },
        ],
        images: [(
            "clockbody:image/shared".into(),
            AnimationImageAsset {
                model: "clockbody:body/native".into(),
                resource: "clockbody:body/native".into(),
            },
        )]
        .into(),
    })
}

#[test]
fn catalog_replacement_rejects_invalid_or_active_inconsistent_data_atomically() -> Result<()> {
    let (_root, mut session, owner) = host_fixture()?;
    let value = request(&session, owner, 1);
    let id = start(&mut session, owner, value.clone())?;
    session.take_animation_records();
    let before = session.animation_view();
    let epochs = session.animation_owner(owner).unwrap();
    let assets = catalog(native_shape())?;
    let mut bad = assets.clone();
    bad.assets[0].digest = "0".repeat(64);
    assert!(session.set_animation_assets(bad).is_err());
    let mut bad = assets.clone();
    bad.images.values_mut().next().unwrap().model = "wrong-model".into();
    assert!(session.set_animation_assets(bad).is_err());
    let mut replacement = native_shape();
    replacement.animations[0].duration = 0.5;
    assert!(
        session
            .set_animation_assets(catalog(replacement.clone())?)
            .is_err()
    );
    assert!(session.set_animation_assets(Default::default()).is_err());
    assert_eq!(session.animation_view(), before);
    assert_eq!(session.animation_owner(owner), Some(epochs));
    assert!(session.take_animation_records().is_empty());
    steps(&mut session, owner, 30)?;
    assert!(
        session.take_animation_records().iter().any(
            |r| r.playback.id == id && r.outcome == bri_sim::session::AnimationOutcome::Natural
        )
    );
    session.set_animation_assets(catalog(replacement)?)?;
    assert!(session.animation_owner(owner).unwrap().body > epochs.body);
    assert!(session.prepare_animation_start(owner, value).is_err());
    let value = request(&session, owner, 2);
    start(&mut session, owner, value)?;
    assert_eq!(session.animation_view().active[0].clip.duration, 0.5);
    Ok(())
}

#[test]
fn command_and_weapon_start_ticks_are_distinct_and_replayed_command_does_not_restart() -> Result<()>
{
    let (_root, mut session, owner, _, _) = fixture()?;
    let tick = session.simulation().state().tick;
    body_start(&mut session, owner)?;
    assert_eq!(session.animation_view().active[0].started_tick, tick);
    image_start(&mut session, owner, 2)?;
    let image = session
        .animation_view()
        .active
        .into_iter()
        .find(|a| {
            matches!(
                a.request.target,
                bri_sim::session::AnimationTarget::Image { .. }
            )
        })
        .unwrap();
    assert_eq!(image.started_tick, tick + 1);
    assert_eq!(session.simulation().state().tick, tick + 2);
    assert_eq!(image.anchor_tick, image.started_tick);
    assert_eq!(image.phase, 0.0);
    let before = session.animation_view();
    assert!(
        session
            .command(owner, 2, Command::WeaponTrigger { down: true })
            .is_err()
    );
    assert_eq!(session.animation_view(), before);
    steps(&mut session, owner, 1)?;
    assert!(
        session
            .take_animation_records()
            .iter()
            .any(|r| r.playback.id == image.id
                && r.outcome == bri_sim::session::AnimationOutcome::Natural
                && r.tick == tick + 3)
    );
    Ok(())
}

#[test]
fn selection_change_cancels_bound_image_before_natural_and_deferred_old_origin_cannot_restart()
-> Result<()> {
    let (_root, mut session, owner, _, second) = fixture()?;
    body_start(&mut session, owner)?;
    image_start(&mut session, owner, 2)?;
    let old: Vec<_> = session
        .animation_view()
        .active
        .into_iter()
        .filter(|a| a.request.binding.item.is_some())
        .map(|a| a.id)
        .collect();
    assert!(!old.is_empty());
    session.take_animation_records();
    session.equip_tool(owner, Some(second))?;
    assert!(session.animation_view().active.is_empty());
    let records = session.take_animation_records();
    assert!(old.iter().all(|id| records.iter().any(
        |r| &r.playback.id == id && r.outcome == bri_sim::session::AnimationOutcome::Cancelled
    )));
    steps(&mut session, owner, 13)?;
    assert!(session.animation_view().active.is_empty());
    assert!(session.take_animation_records().is_empty());
    assert!(session.take_cues().iter().any(
        |c| matches!(&c.kind,CueKind::WeaponAnimation {sequence,..} if sequence=="body_cycle")
    ));
    assert!(
        session
            .weapon_adapter_gaps()
            .get("animation.stale_producer")
            .copied()
            .unwrap_or(0)
            > 0
    );
    Ok(())
}

#[test]
fn inventory_weapon_clock_is_distinct_from_pawn_and_actual_image_lifetimes() -> Result<()> {
    use bri_sim::session::{AnimationOutcome, AnimationTarget};
    let (_root, mut session, owner, first, _) = fixture()?;
    body_start(&mut session, owner)?;
    image_start(&mut session, owner, 2)?;
    let binding = session
        .animation_view()
        .active
        .into_iter()
        .find(|a| matches!(a.request.target, AnimationTarget::Image { .. }))
        .unwrap()
        .request
        .binding;
    let mut weapon = request(&session, owner, 1);
    weapon.target = AnimationTarget::Weapon(first);
    weapon.binding = binding.clone();
    let weapon_id = start(&mut session, owner, weapon)?;
    let mut pawn = request(&session, owner, 2);
    pawn.binding = binding;
    let pawn_id = start(&mut session, owner, pawn)?;
    session.take_animation_records();
    session.equip_tool(owner, None)?;
    let view = session.animation_view();
    assert_eq!(view.active.len(), 1);
    assert_eq!(view.active[0].id, weapon_id);
    assert!(session.cancel_animation(&pawn_id).is_err());
    assert_eq!(session.animation_view(), view);
    assert!(
        session
            .take_animation_records()
            .iter()
            .any(|r| r.playback.id == pawn_id && r.outcome == AnimationOutcome::Cancelled)
    );
    steps(&mut session, owner, 30)?;
    assert!(
        session
            .take_animation_records()
            .iter()
            .any(|r| r.playback.id == weapon_id && r.outcome == AnimationOutcome::Natural)
    );
    Ok(())
}

#[test]
fn disconnect_and_same_tick_resume_cancel_old_ids_and_admit_only_new_life() -> Result<()> {
    let (_root, mut session, owner) = host_fixture()?;
    let old_request = request(&session, owner, 1);
    let old = start(&mut session, owner, old_request.clone())?;
    let tick = session.simulation().state().tick;
    session.take_animation_records();
    session.disconnect(owner)?;
    assert!(session.animation_view().active.is_empty());
    assert!(session.take_animation_records().iter().any(
        |r| r.playback.id == old && r.outcome == bri_sim::session::AnimationOutcome::Cancelled
    ));
    session.resume(owner, Vec3::new(0., 0.05, 0.))?;
    assert_eq!(session.simulation().state().tick, tick);
    assert!(session.animation_owner(owner).unwrap().life > old.epochs.life);
    assert!(session.prepare_animation_start(owner, old_request).is_err());
    assert!(session.cancel_animation(&old).is_err());
    body_start(&mut session, owner)?;
    let value = request(&session, owner, 1);
    let fresh = start(&mut session, owner, value)?;
    assert_ne!(fresh.epochs, old.epochs);
    Ok(())
}
