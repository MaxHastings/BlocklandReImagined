//! Actual Session clock controls, first characterized before the evaluated view.
use anyhow::Result;
use bri_content::shape::Shape;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{
        AnimationAsset, AnimationAssets, AnimationOutcome, AnimationStart, AnimationTarget,
        PlaybackId, Session,
    },
    simulation::Simulation,
};
use bri_weapons::AnimationBinding;
use bri_world::{TICKS_PER_SECOND, World};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;

fn fixture() -> Result<(Session, u64)> {
    let clip = |name, duration, looping| {
        json!({
            "name":name,"frames":2,"duration":duration,"looping":looping,
            "additive":false,"priority":0,
            "nodes":[{"node":"root","translations":[[0.,0.,0.],[1.,0.,0.]],
                "rotations":[],"scales":[],"scale_rotations":[]}],
            "objects":[],"ground_translations":[],"ground_rotations":[],"triggers":[]
        })
    };
    // Invented native data under the existing default body resource; no originals.
    let shape: Shape = serde_json::from_value(json!({
        "schema_version":1,"id":"v20.shape.m",
        "nodes":[{"name":"root","parent":null,
            "translation":[0.,0.,0.],"rotation":[0.,0.,0.,1.]}],
        "objects":[],"details":[],"meshes":[],"materials":[],
        "animations":[clip("finite",0.25,false),clip("looped",0.25,true),clip("zero",0.,false)]
    }))?;
    shape.validate()?;
    let bytes = serde_json::to_vec(&shape)?;
    let mut session = Session::new(Simulation::new(
        World::new(
            "Evaluated phases".into(),
            "test:phases".into(),
            vec![[1.; 4]],
        ),
        Definitions::default(),
        vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
    )?);
    session.set_animation_assets(AnimationAssets {
        assets: vec![AnimationAsset {
            resource: "v20.shape.m".into(),
            digest: bri_package::environment::hash_bytes(&bytes),
            rig: false,
            bytes,
        }],
        images: Default::default(),
    })?;
    let owner = session.join("Phase owner".into(), Vec3::new(0., 0.05, 0.), true)?;
    Ok((session, owner))
}

fn request(session: &Session, owner: u64, nonce: u64, sequence: &str) -> AnimationStart {
    AnimationStart {
        namespace: "evaluated-phase-control".into(),
        nonce,
        target: AnimationTarget::Pawn(2),
        sequence: sequence.into(),
        binding: AnimationBinding {
            owner: session.animation_owner(owner),
            ..Default::default()
        },
        rate: 1.0,
        tween: 0.0,
    }
}

fn start(session: &mut Session, owner: u64, request: AnimationStart) -> Result<PlaybackId> {
    let prepared = session.prepare_animation_start(owner, request)?;
    session.commit_animation_start(prepared)
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

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
}

#[test]
fn baseline_anchor_duplicate_pause_rate_and_natural_removal() -> Result<()> {
    let (mut session, owner) = fixture()?;
    let value = request(&session, owner, 1, "finite");
    let id = start(&mut session, owner, value.clone())?;
    let initial = session.animation_view();
    steps(&mut session, owner, 10)?;
    let later = session.animation_view();
    assert_eq!(later.active, initial.active);
    assert_eq!(later.revision, initial.revision);
    assert_eq!(later.tick, initial.tick + 10);
    assert_eq!(start(&mut session, owner, value)?, id);
    assert_eq!(session.animation_view(), later);
    assert_eq!(session.take_animation_records().len(), 1);
    session.set_animation_rate(&id, 0.)?;
    let paused = session.animation_view();
    close(paused.active[0].phase, 10. / TICKS_PER_SECOND as f64);
    assert_eq!(paused.active[0].anchor_tick, later.tick);
    steps(&mut session, owner, 20)?;
    assert_eq!(session.animation_view().active, paused.active);
    session.set_animation_rate(&id, 2.)?;
    steps(&mut session, owner, 10)?;
    assert!(session.animation_view().active.is_empty());
    let records = session.take_animation_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].playback.id, id);
    assert_eq!(records[0].outcome, AnimationOutcome::Natural);
    close(records[0].phase, 0.25);
    assert!(session.set_animation_rate(&id, 1.).is_err());
    Ok(())
}

#[test]
fn baseline_loop_stays_active_and_cancel_records_unwrapped_phase() -> Result<()> {
    let (mut session, owner) = fixture()?;
    let value = request(&session, owner, 1, "looped");
    let id = start(&mut session, owner, value)?;
    steps(&mut session, owner, 60)?;
    let view = session.animation_view();
    assert_eq!(view.active[0].id, id);
    assert_eq!(view.active[0].phase, 0.);
    assert_eq!(
        session.take_animation_records()[0].outcome,
        AnimationOutcome::Started
    );
    session.cancel_animation(&id)?;
    let records = session.take_animation_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, AnimationOutcome::Cancelled);
    close(records[0].phase, 60. / TICKS_PER_SECOND as f64);
    Ok(())
}

#[test]
fn baseline_zero_replacement_is_atomic_and_duplicate_does_not_restart() -> Result<()> {
    let (mut session, owner) = fixture()?;
    let value = request(&session, owner, 1, "finite");
    let old = start(&mut session, owner, value)?;
    session.take_animation_records();
    let zero = request(&session, owner, 2, "zero");
    let id = start(&mut session, owner, zero.clone())?;
    let records = session.take_animation_records();
    assert_eq!(
        records.iter().map(|r| r.outcome).collect::<Vec<_>>(),
        [
            AnimationOutcome::Interrupted,
            AnimationOutcome::Started,
            AnimationOutcome::Natural,
        ]
    );
    assert_eq!(records[0].playback.id, old);
    assert_eq!(records[1].playback.id, id);
    assert_eq!(records[2].playback.id, id);
    assert!(
        records
            .iter()
            .all(|r| r.tick == records[0].tick && r.phase == 0.)
    );
    assert!(session.animation_view().active.is_empty());
    assert_eq!(start(&mut session, owner, zero)?, id);
    assert!(session.animation_view().active.is_empty());
    assert!(session.take_animation_records().is_empty());
    Ok(())
}

#[test]
fn baseline_disconnect_resume_invalidates_actual_life_and_playback() -> Result<()> {
    let (mut session, owner) = fixture()?;
    let value = request(&session, owner, 1, "finite");
    let id = start(&mut session, owner, value.clone())?;
    session.take_animation_records();
    session.disconnect(owner)?;
    assert!(session.animation_view().active.is_empty());
    let records = session.take_animation_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, AnimationOutcome::Cancelled);
    assert_eq!(records[0].playback.id, id);
    session.resume(owner, Vec3::new(0., 0.05, 0.))?;
    assert!(session.animation_owner(owner).unwrap().life > id.epochs.life);
    assert!(session.prepare_animation_start(owner, value).is_err());
    let value = request(&session, owner, 1, "finite");
    let new = start(&mut session, owner, value)?;
    assert_ne!(new, id);
    Ok(())
}

#[test]
fn evaluated_tick_advances_without_revision_or_anchor_mutation() -> Result<()> {
    let (mut session, owner) = fixture()?;
    let value = request(&session, owner, 1, "finite");
    let id = start(&mut session, owner, value.clone())?;
    let initial = session.evaluated_animation_view();
    let anchors = session.animation_view();
    assert_eq!(initial.tick, anchors.tick);
    assert_eq!(initial.revision, anchors.revision);
    assert_eq!(initial.active[0].playback, anchors.active[0]);
    assert_eq!(initial.active[0].evaluated_phase, 0.);
    steps(&mut session, owner, 7)?;
    let later = session.evaluated_animation_view();
    assert_eq!(later.tick, initial.tick + 7);
    assert_eq!(later.revision, initial.revision);
    assert_eq!(later.active[0].playback, initial.active[0].playback);
    close(
        later.active[0].evaluated_phase,
        7. / TICKS_PER_SECOND as f64,
    );
    assert_eq!(session.animation_view().active, anchors.active);
    assert_eq!(start(&mut session, owner, value)?, id);
    assert_eq!(session.evaluated_animation_view(), later);
    assert_eq!(session.evaluated_animation_view(), later);
    let records = session.take_animation_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, AnimationOutcome::Started);
    assert_eq!(records[0].playback, anchors.active[0]);
    assert_eq!(records[0].tick, anchors.tick);
    assert_eq!(records[0].phase, 0.);
    assert!(records[0].reason.is_empty());
    assert_eq!(session.evaluated_animation_view(), later);
    assert!(session.take_animation_records().is_empty());
    Ok(())
}

#[test]
fn evaluated_pause_and_rate_change_preserve_continuous_phase() -> Result<()> {
    let (mut session, owner) = fixture()?;
    let value = request(&session, owner, 1, "looped");
    let id = start(&mut session, owner, value)?;
    steps(&mut session, owner, 10)?;
    let moving = session.evaluated_animation_view();
    session.set_animation_rate(&id, 0.)?;
    let paused = session.evaluated_animation_view();
    assert!(paused.revision > moving.revision);
    assert_eq!(
        paused.active[0].evaluated_phase,
        moving.active[0].evaluated_phase
    );
    assert_eq!(
        paused.active[0].playback.phase,
        moving.active[0].evaluated_phase
    );
    assert_eq!(paused.active[0].playback.anchor_tick, moving.tick);
    steps(&mut session, owner, 20)?;
    let held = session.evaluated_animation_view();
    assert_eq!(held.revision, paused.revision);
    assert_eq!(held.active, paused.active);
    session.set_animation_rate(&id, 2.)?;
    let resumed = session.evaluated_animation_view();
    assert_eq!(
        resumed.active[0].evaluated_phase,
        held.active[0].evaluated_phase
    );
    steps(&mut session, owner, 30)?;
    let later = session.evaluated_animation_view();
    assert_eq!(later.revision, resumed.revision);
    assert_eq!(later.active[0].playback, resumed.active[0].playback);
    close(
        later.active[0].evaluated_phase,
        70. / TICKS_PER_SECOND as f64,
    );
    assert!(later.active[0].evaluated_phase > later.active[0].playback.clip.duration);
    session.cancel_animation(&id)?;
    assert!(session.evaluated_animation_view().active.is_empty());
    let records = session.take_animation_records();
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].outcome, AnimationOutcome::Cancelled);
    assert_eq!(records[1].phase, later.active[0].evaluated_phase);
    Ok(())
}

#[test]
fn evaluated_natural_and_zero_leave_no_held_entry_or_extra_records() -> Result<()> {
    let (mut session, owner) = fixture()?;
    let value = request(&session, owner, 1, "finite");
    let id = start(&mut session, owner, value.clone())?;
    steps(&mut session, owner, 29)?;
    let before_end = session.evaluated_animation_view();
    assert_eq!(before_end.active[0].playback.id, id);
    close(
        before_end.active[0].evaluated_phase,
        29. / TICKS_PER_SECOND as f64,
    );
    steps(&mut session, owner, 1)?;
    let complete = session.evaluated_animation_view();
    assert!(complete.active.is_empty());
    assert!(complete.revision > before_end.revision);
    assert_eq!(session.evaluated_animation_view(), complete);
    assert_eq!(start(&mut session, owner, value)?, id);
    assert_eq!(session.evaluated_animation_view(), complete);
    let records = session.take_animation_records();
    assert_eq!(
        records.iter().map(|r| r.outcome).collect::<Vec<_>>(),
        [AnimationOutcome::Started, AnimationOutcome::Natural,]
    );
    assert_eq!(records[1].playback.id, id);
    assert_eq!(records[1].phase, 0.25);
    assert_eq!(records[1].tick, complete.tick);
    let value = request(&session, owner, 2, "zero");
    let zero = start(&mut session, owner, value.clone())?;
    let empty = session.evaluated_animation_view();
    assert!(empty.active.is_empty());
    assert_eq!(start(&mut session, owner, value)?, zero);
    assert_eq!(session.evaluated_animation_view(), empty);
    let records = session.take_animation_records();
    assert_eq!(
        records.iter().map(|r| r.outcome).collect::<Vec<_>>(),
        [AnimationOutcome::Started, AnimationOutcome::Natural,]
    );
    assert!(
        records
            .iter()
            .all(|r| r.playback.id == zero && r.phase == 0.)
    );
    assert!(session.evaluated_animation_view().active.is_empty());
    assert!(session.take_animation_records().is_empty());
    Ok(())
}

#[test]
fn evaluated_queries_preserve_prepared_admission_and_same_tick_replacement() -> Result<()> {
    let (mut session, owner) = fixture()?;
    let value = request(&session, owner, 1, "finite");
    let old = start(&mut session, owner, value)?;
    steps(&mut session, owner, 6)?;
    let value = request(&session, owner, 2, "looped");
    let prepared = session.prepare_animation_start(owner, value)?;
    let before = session.evaluated_animation_view();
    assert_eq!(session.evaluated_animation_view(), before);
    let mut bad = request(&session, owner, 3, "missing");
    assert!(session.prepare_animation_start(owner, bad.clone()).is_err());
    bad.sequence = "finite".into();
    bad.binding.owner.as_mut().unwrap().body += 1;
    assert!(session.prepare_animation_start(owner, bad).is_err());
    assert_eq!(session.evaluated_animation_view(), before);
    let new = session.commit_animation_start(prepared)?;
    let after = session.evaluated_animation_view();
    assert_eq!(after.tick, before.tick);
    assert!(after.revision > before.revision);
    assert_eq!(after.active.len(), 1);
    assert_eq!(after.active[0].playback.id, new);
    assert_ne!(new, old);
    assert_eq!(after.active[0].evaluated_phase, 0.);
    assert_eq!(after.active[0].playback, session.animation_view().active[0]);
    let records = session.take_animation_records();
    assert_eq!(
        records.iter().map(|r| r.outcome).collect::<Vec<_>>(),
        [
            AnimationOutcome::Started,
            AnimationOutcome::Interrupted,
            AnimationOutcome::Started,
        ]
    );
    assert_eq!(records[1].playback.id, old);
    assert_eq!(records[1].phase, before.active[0].evaluated_phase);
    assert_eq!(records[2].playback.id, new);
    Ok(())
}

#[test]
fn evaluated_lifecycle_removes_old_identity_and_snapshot_is_owned() -> Result<()> {
    let (mut session, owner) = fixture()?;
    let value = request(&session, owner, 1, "finite");
    let old = start(&mut session, owner, value.clone())?;
    steps(&mut session, owner, 4)?;
    let before = session.evaluated_animation_view();
    session.disconnect(owner)?;
    let disconnected = session.evaluated_animation_view();
    assert!(disconnected.active.is_empty());
    assert_eq!(disconnected.tick, before.tick);
    assert!(disconnected.revision > before.revision);
    session.resume(owner, Vec3::new(0., 0.05, 0.))?;
    assert!(session.evaluated_animation_view().active.is_empty());
    assert!(session.prepare_animation_start(owner, value).is_err());
    let value = request(&session, owner, 1, "looped");
    let new = start(&mut session, owner, value)?;
    let resumed = session.evaluated_animation_view();
    assert_eq!(resumed.active[0].playback.id, new);
    assert!(new.epochs.life > old.epochs.life);
    assert_eq!(before.active[0].playback.id, old);
    close(
        before.active[0].evaluated_phase,
        4. / TICKS_PER_SECOND as f64,
    );
    let records = session.take_animation_records();
    assert_eq!(
        records.iter().map(|r| r.outcome).collect::<Vec<_>>(),
        [
            AnimationOutcome::Started,
            AnimationOutcome::Cancelled,
            AnimationOutcome::Started,
        ]
    );
    assert_eq!(records[1].playback.id, old);
    assert_eq!(records[1].phase, before.active[0].evaluated_phase);
    assert_eq!(session.evaluated_animation_view(), resumed);
    Ok(())
}
