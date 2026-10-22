//! Trusted-host native final sampling through actual Session state.
use anyhow::{Context, Result};
use bri_content::{animation::sample, shape::Shape};
use bri_package::packages::PackageSet;
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{
        AnimationAsset, AnimationAssets, AnimationOutcome, AnimationPoseRetention,
        AnimationPoseState, AnimationStart, AnimationTarget, BodyPoseBinding, BodyPoseBindings,
        Command, PackageArg, PackageCommand, PlaybackId, Session,
    },
    simulation::Simulation,
};
use bri_weapons::{AnimationBinding, Pack};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::sync::Arc;

const MODEL: &str = "posehold:body/native";
const OTHER: &str = "posehold:body/other";
const BODY: &str = "posehold:archetype/human";
const SCRIPT: &str = r#"
fn cmd_body(p) { set_archetype(p, "posehold:archetype/human"); }
fn cmd_other(p) { set_archetype(p, "posehold:archetype/other"); }
fn cmd_root(p) { play_thread(p, 2, "root"); }
fn cmd_birth(p) { respawn(p); }
fn cmd_wear(p, slot, image) {
    if image == "" { mount_image(p, (), slot); } else { mount_image(p, image, slot); }
}
fn cmd_scope(p, image) {
    if image == "" { mount_image(p, ()); } else { mount_image(p, image); }
}
fn cmd_emote(p, image) { emote(p, image); }
fn cmd_tools(p) { set_tools(p, ["posehold:weapon/gun", "posehold:weapon/gun"]); }
fn cmd_empty(p) { set_tools(p, []); }
fn cmd_take(p) { take_item(p, "posehold:weapon/gun"); }
"#;

fn shape() -> Shape {
    let clip = |name, duration, looping| {
        json!({
            "name":name,"frames":2,"duration":duration,"looping":looping,
            "additive":false,"priority":0,
            "nodes":[{"node":"root","translations":[[0.,0.,0.],[1.,0.,0.]],
                "rotations":[],"scales":[],"scale_rotations":[]}],
            "objects":[],"ground_translations":[],"ground_rotations":[],"triggers":[]
        })
    };
    serde_json::from_value(json!({
        "schema_version":1,"id":MODEL,
        "nodes":[{"name":"root","parent":null,
            "translation":[0.,0.,0.],"rotation":[0.,0.,0.,1.]}],
        "objects":[],"details":[],"meshes":[],"materials":[],
        "animations":[clip("finite",0.25,false),clip("looped",0.25,true),clip("zero",0.,false)]
    }))
    .unwrap()
}

struct Game {
    _root: tempfile::TempDir,
    session: Session,
    assets: AnimationAssets,
    bindings: BodyPoseBindings,
    sequence: u64,
}
impl Game {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir()?;
        let dir = root.path().join("posehold");
        let rules = root.path().join("poserules");
        std::fs::create_dir_all(&dir)?;
        std::fs::create_dir_all(&rules)?;
        let native = shape();
        native.validate()?;
        std::fs::write(dir.join("shape.json"), serde_json::to_vec(&native)?)?;
        let mut provides = vec![json!({"kind":"body","id":MODEL,"file":"shape.json"})];
        for (name, model) in [("human", MODEL), ("other", OTHER)] {
            let file = format!("{name}.json");
            std::fs::write(
                dir.join(&file),
                json!({
                    "schema_version":1,"name":name,"model":model,
                    "sockets":{"right_hand":{"node":"root","offset":[0.01,0.,0.]}}
                })
                .to_string(),
            )?;
            provides.push(
                json!({"kind":"archetype","id":format!("posehold:archetype/{name}"),"file":file}),
            );
        }
        std::fs::write(
            dir.join("package.json"),
            json!({
                "schema_version":1,"id":"posehold","version":"1.0.0","api":1,
                "name":"Invented retained pose","license":"CC0-1.0",
                "provides":provides
            })
            .to_string(),
        )?;
        std::fs::write(rules.join("package.json"),json!({
            "schema_version":1,"id":"poserules","version":"1.0.0","api":1,
            "name":"Invented pose controls","license":"CC0-1.0","capabilities":["player","effects"],
            "dependencies":{"posehold":"^1.0.0"},"provides":[
                {"kind":"behaviour","id":"poserules:behaviour/main","file":"behaviour.json"},
                {"kind":"script","id":"poserules:script/main","file":"main.rhai"}]
        }).to_string())?;
        std::fs::write(rules.join("main.rhai"), SCRIPT)?;
        std::fs::write(
            rules.join("behaviour.json"),
            json!({
                "schema_version":1,"script":"main.rhai","commands":[
                    {"name":"body"},{"name":"other"},{"name":"root"},{"name":"birth"},
                    {"name":"wear","args":["int","string"]},{"name":"scope","args":["string"]},
                    {"name":"emote","args":["string"]},{"name":"tools"},{"name":"empty"},{"name":"take"}]
            })
            .to_string(),
        )?;
        let packages: PackageSet = serde_json::from_value(json!({"schema_version":1,"packages":[
            {"id":"posehold","version":"1.0.0","side":"shared","dir":"posehold"},
            {"id":"poserules","version":"1.0.0","side":"server","dir":"poserules"}
        ]}))?;
        let mut session = Session::new(Simulation::new(
            World::new(
                "Retained pose".into(),
                "test:retained-pose".into(),
                vec![[1.; 4]],
            ),
            Definitions::default(),
            vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
        )?);
        session.install_packages(
            Arc::new(
                Catalog::load(root.path(), &packages, true)
                    .map_err(|e| anyhow::anyhow!("{e:#?}"))?,
            ),
            None,
        )?;
        session.set_weapon_pack(Pack::from_json(&serde_json::to_vec(&json!({
            "schema_version":5,"id":"posehold",
            "items":{"posehold:weapon/gun":{"ui_name":"Invented gun","image":"posehold:image/right"},
                "posehold:weapon/blocked":{"ui_name":"Invented waiting gun","image":"posehold:image/blocked"}},
            "images":{
                "posehold:image/right":{"model":MODEL,"left_image":"posehold:image/left",
                    "states":[{"name":"Activate","ticks":1,"timeout":1,"arm":"finite",
                        "cues":[{"after_ms":100,"thread":2,"sequence":"root"}]},{"name":"Ready"}]},
                "posehold:image/left":{"model":MODEL,"states":[{"name":"Ready"}]},
                "posehold:image/blocked":{"model":MODEL,"left_image":"posehold:image/left",
                    "states":[{"name":"Busy","ticks":12,"allow_change":false,"timeout":1},{"name":"Ready"}]},
                "posehold:image/worn":{"model":MODEL,"states":[{"name":"Ready"}]},
                "posehold:image/scope":{"model":MODEL,"states":[{"name":"Ready"}]},
                "posehold:image/emote":{"model":MODEL,"states":[{"name":"Ready"}]}
            },"projectiles":{}
        }))?)?)?;
        session.set_item_bounds(Default::default())?;
        let asset = |resource: &str, shape: &Shape| -> Result<AnimationAsset> {
            let bytes = serde_json::to_vec(shape)?;
            Ok(AnimationAsset {
                resource: resource.into(),
                digest: bri_package::environment::hash_bytes(&bytes),
                rig: false,
                library_clips: Vec::new(),
                bytes,
            })
        };
        let mut other = native.clone();
        other.id = OTHER.into();
        let assets = AnimationAssets {
            assets: vec![asset(MODEL, &native)?, asset(OTHER, &other)?],
            images: Default::default(),
        };
        let bindings = BodyPoseBindings {
            reference_resource: None,
            bodies: [MODEL, OTHER]
                .into_iter()
                .map(|model| BodyPoseBinding {
                    model: model.into(),
                    resource: model.into(),
                    fitted: true,
                })
                .collect(),
        };
        session.set_body_pose_assets(assets.clone(), bindings.clone())?;
        Ok(Self {
            _root: root,
            session,
            assets,
            bindings,
            sequence: 0,
        })
    }
    fn run(&mut self, owner: u64, name: &str, args: Vec<PackageArg>) -> Result<()> {
        self.sequence += 1;
        self.session.command(
            owner,
            self.sequence,
            Command::Package(PackageCommand {
                package: "poserules".into(),
                command: name.into(),
                args,
            }),
        )?;
        let problems = self.session.take_package_problems();
        anyhow::ensure!(
            problems.is_empty(),
            "fixture package failure: {problems:#?}"
        );
        Ok(())
    }
    fn owner(&mut self) -> Result<u64> {
        let x = self.sequence as f32 * 3.;
        let owner = self
            .session
            .join("Pose owner".into(), Vec3::new(x, 0.05, 0.), true)?;
        self.run(owner, "body", vec![])?;
        Ok(owner)
    }
    fn steps(&mut self, owner: u64, count: u64) -> Result<()> {
        for _ in 0..count {
            self.session.movement(
                owner,
                self.session.simulation().state().tick + 100,
                MoveInput::default(),
            )?;
            self.session.step()?;
        }
        Ok(())
    }
    fn request(&self, owner: u64, nonce: u64, sequence: &str) -> AnimationStart {
        AnimationStart {
            namespace: "retained-pose-control".into(),
            nonce,
            target: AnimationTarget::Pawn(2),
            sequence: sequence.into(),
            binding: AnimationBinding {
                owner: self.session.animation_owner(owner),
                ..Default::default()
            },
            rate: 1.,
            tween: 0.,
        }
    }
    fn start(&mut self, owner: u64, request: AnimationStart) -> Result<PlaybackId> {
        let ticket = self.session.prepare_animation_start(owner, request)?;
        self.session.commit_animation_start(ticket)
    }
    fn pose_phase(&self, id: &PlaybackId) -> Option<f64> {
        self.session
            .animation_pose_view()
            .poses
            .into_iter()
            .find(|entry| &entry.playback.id == id)
            .map(|entry| entry.phase)
    }
    fn hold(&mut self, owner: u64, request: AnimationStart) -> Result<PlaybackId> {
        let ticket = self.session.prepare_animation_start_with_pose(
            owner,
            request,
            AnimationPoseRetention::HoldNativeFinal,
        )?;
        self.session.commit_animation_start(ticket)
    }
    fn command(&mut self, owner: u64, command: Command) -> Result<()> {
        self.sequence += 1;
        self.session.command(owner, self.sequence, command)?;
        Ok(())
    }
    fn equipped_binding(&mut self, owner: u64, slot: usize) -> Result<AnimationBinding> {
        self.session.equip_tool(owner, Some(slot))?;
        self.steps(owner, 1)?;
        let active = self
            .session
            .animation_view()
            .active
            .into_iter()
            .find(|a| a.id.owner == owner && a.request.target == AnimationTarget::Pawn(2))
            .context("actual image state did not produce its arm playback")?;
        assert!(
            self.session.animation_pose_view().poses.is_empty(),
            "native producer must remain NoHold"
        );
        self.session.cancel_animation(&active.id)?;
        self.session.take_animation_records();
        Ok(active.request.binding)
    }
}

#[test]
fn baseline_default_natural_removes_active_despite_native_final_sample() -> Result<()> {
    let mut game = Game::new()?;
    let owner = game.owner()?;
    let request = game.request(owner, 1, "finite");
    let id = game.start(owner, request)?;
    game.steps(owner, 30)?;
    assert!(game.session.animation_view().active.is_empty());
    assert!(game.session.evaluated_animation_view().active.is_empty());
    assert!(game.pose_phase(&id).is_none());
    let native = shape();
    let clip = &native.animations[0];
    assert_eq!(
        sample(&native, Some(clip), 0.25)?.nodes,
        sample(&native, Some(clip), 10.)?.nodes
    );
    let records = game.session.take_animation_records();
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].outcome, AnimationOutcome::Natural);
    assert_eq!(records[1].phase, 0.25);
    Ok(())
}

#[test]
fn opted_in_final_pose_survives_natural_for_native_sampling() -> Result<()> {
    let mut game = Game::new()?;
    let owner = game.owner()?;
    let request = game.request(owner, 1, "finite");
    let id = game.hold(owner, request)?;
    game.steps(owner, 30)?;
    // The baseline used the active-only adapter. D2a remains active-only; this
    // pose adapter now explicitly opts in and reads the separate pose view.
    assert_eq!(game.pose_phase(&id), Some(0.25));
    assert!(game.session.animation_view().active.is_empty());
    assert!(game.session.evaluated_animation_view().active.is_empty());
    let held = game.session.animation_pose_view();
    assert_eq!(held.poses[0].state, AnimationPoseState::HeldFinal);
    assert_eq!(held.poses[0].applicability.body.resource, MODEL);
    let records = game.session.take_animation_records();
    assert_eq!(
        records.iter().map(|r| r.outcome).collect::<Vec<_>>(),
        [AnimationOutcome::Started, AnimationOutcome::Natural]
    );
    game.steps(owner, 60)?;
    assert_eq!(game.pose_phase(&id), Some(0.25));
    assert!(game.session.take_animation_records().is_empty());
    assert!(game.session.cancel_animation(&id).is_err());
    assert!(game.session.set_animation_rate(&id, 2.).is_err());
    game.session.clear_animation_pose(&id)?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    assert!(game.session.take_animation_records().is_empty());
    Ok(())
}

#[test]
fn zero_replacement_duplicate_and_id_clear_preserve_terminal_order() -> Result<()> {
    let mut game = Game::new()?;
    let owner = game.owner()?;
    let request = game.request(owner, 1, "finite");
    let old = game.start(owner, request)?;
    game.session.take_animation_records();
    let zero = game.request(owner, 2, "zero");
    let id = game.hold(owner, zero.clone())?;
    let held = game.session.animation_pose_view();
    assert_eq!(held.poses[0].state, AnimationPoseState::HeldFinal);
    assert_eq!(held.poses[0].phase, 0.);
    assert_eq!(game.hold(owner, zero.clone())?, id);
    assert!(
        game.start(owner, zero.clone()).is_err(),
        "a nonce cannot change retention policy"
    );
    assert!(game.session.clear_animation_pose(&old).is_err());
    assert_eq!(game.session.animation_pose_view(), held);
    let records = game.session.take_animation_records();
    assert_eq!(
        records.iter().map(|r| r.outcome).collect::<Vec<_>>(),
        [
            AnimationOutcome::Interrupted,
            AnimationOutcome::Started,
            AnimationOutcome::Natural
        ]
    );
    assert_eq!(records[0].playback.id, old);
    assert!(
        records[1..]
            .iter()
            .all(|r| r.playback.id == id && r.tick == held.tick)
    );
    assert_eq!(game.session.animation_pose_view(), held);
    game.session.clear_animation_pose(&id)?;
    let cleared = game.session.animation_pose_view();
    assert!(cleared.poses.is_empty());
    assert!(cleared.revision > held.revision);
    assert_eq!(game.hold(owner, zero)?, id);
    assert_eq!(game.session.animation_pose_view(), cleared);
    assert!(game.session.take_animation_records().is_empty());
    let request = game.request(owner, 3, "finite");
    let ticket = game.session.prepare_animation_start_with_pose(
        owner,
        request,
        AnimationPoseRetention::HoldNativeFinal,
    )?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    let active = game.session.commit_animation_start(ticket)?;
    game.session.clear_animation_pose(&active)?;
    let records = game.session.take_animation_records();
    assert_eq!(
        records.iter().map(|r| r.outcome).collect::<Vec<_>>(),
        [AnimationOutcome::Started, AnimationOutcome::Cancelled]
    );
    assert!(game.session.animation_pose_view().poses.is_empty());
    Ok(())
}

#[test]
fn loop_pause_root_and_nohold_replacement_keep_distinct_visual_and_clock_laws() -> Result<()> {
    let mut game = Game::new()?;
    let owner = game.owner()?;
    let request = game.request(owner, 1, "looped");
    let id = game.hold(owner, request)?;
    game.steps(owner, 60)?;
    let playing = game.session.animation_pose_view();
    assert_eq!(playing.poses[0].state, AnimationPoseState::Playing);
    assert_eq!(playing.poses[0].phase, 0.5);
    game.session.set_animation_rate(&id, 0.)?;
    game.steps(owner, 30)?;
    assert_eq!(game.pose_phase(&id), Some(0.5));
    game.session.set_animation_rate(&id, 2.)?;
    game.steps(owner, 15)?;
    assert_eq!(game.pose_phase(&id), Some(0.75));
    game.run(owner, "root", vec![])?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    let records = game.session.take_animation_records();
    assert_eq!(
        records.iter().map(|r| r.outcome).collect::<Vec<_>>(),
        [AnimationOutcome::Started, AnimationOutcome::Cancelled]
    );
    let zero = game.request(owner, 2, "zero");
    let old = game.hold(owner, zero)?;
    game.session.take_animation_records();
    game.run(owner, "root", vec![])?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    assert!(game.session.take_animation_records().is_empty());
    let zero = game.request(owner, 3, "zero");
    game.hold(owner, zero)?;
    game.session.take_animation_records();
    let request = game.request(owner, 4, "finite");
    let new = game.start(owner, request)?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    assert!(game.session.clear_animation_pose(&old).is_err());
    assert!(
        game.session.clear_animation_pose(&new).is_err(),
        "NoHold is not a retained pose"
    );
    let records = game.session.take_animation_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, AnimationOutcome::Started);
    Ok(())
}

#[test]
fn unsupported_targets_tween_and_missing_or_mismatched_body_do_not_consume_nonce() -> Result<()> {
    let mut game = Game::new()?;
    let owner = game.owner()?;
    let request = game.request(owner, 1, "finite");
    let before = game.session.animation_view();
    for target in [
        AnimationTarget::Pawn(0),
        AnimationTarget::Pawn(1),
        AnimationTarget::Pawn(3),
        AnimationTarget::Pawn(15),
        AnimationTarget::Pawn(31),
        AnimationTarget::Pawn(32),
        AnimationTarget::Weapon(0),
        AnimationTarget::Image {
            hand: 0,
            emote: false,
        },
        AnimationTarget::Image {
            hand: 3,
            emote: true,
        },
    ] {
        let mut bad = request.clone();
        bad.target = target;
        assert!(
            game.session
                .prepare_animation_start_with_pose(
                    owner,
                    bad,
                    AnimationPoseRetention::HoldNativeFinal
                )
                .err()
                .unwrap()
                .to_string()
                .contains("Unsupported pose retention target")
        );
    }
    let mut tween = request.clone();
    tween.tween = 0.05;
    assert!(game.hold(owner, tween).is_err());
    assert_eq!(game.session.animation_view(), before);
    assert!(game.session.take_animation_records().is_empty());
    let mut plain = request.clone();
    plain.target = AnimationTarget::Pawn(15);
    let default = game.start(owner, plain.clone())?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    let encoded = serde_json::to_value(&plain)?;
    assert_eq!(encoded.as_object().unwrap().len(), 7);
    assert!(encoded.get("retention").is_none());
    assert_eq!(serde_json::from_value::<AnimationStart>(encoded)?, plain);
    game.session.cancel_animation(&default)?;
    game.session.take_animation_records();
    let mut missing = game.bindings.clone();
    missing.bodies.retain(|b| b.model != MODEL);
    game.session
        .set_body_pose_assets(game.assets.clone(), missing)?;
    let next = game.request(owner, 2, "finite");
    assert!(game.hold(owner, next.clone()).is_err());
    let mut mismatch = game.bindings.clone();
    mismatch.bodies[0].resource = OTHER.into();
    game.session
        .set_body_pose_assets(game.assets.clone(), mismatch)?;
    let error = game.hold(owner, next.clone()).unwrap_err();
    assert!(error.to_string().contains("does not belong"), "{error:#}");
    assert!(game.session.animation_view().active.is_empty());
    assert!(game.session.take_animation_records().is_empty());
    game.session
        .set_body_pose_assets(game.assets.clone(), game.bindings.clone())?;
    game.hold(owner, next)?;
    Ok(())
}

#[test]
fn catalog_swaps_compare_semantic_full_receipts_and_reject_atomically() -> Result<()> {
    let mut game = Game::new()?;
    let owner = game.owner()?;
    let request = game.request(owner, 1, "finite");
    let ticket = game.session.prepare_animation_start_with_pose(
        owner,
        request,
        AnimationPoseRetention::HoldNativeFinal,
    )?;
    let mut bad = game.assets.clone();
    bad.assets[0].digest = "0".repeat(64);
    assert!(
        game.session
            .set_body_pose_assets(bad, game.bindings.clone())
            .is_err()
    );
    let id = game.session.commit_animation_start(ticket)?;
    let active = game.session.animation_pose_view();
    let mut bad_context = game.bindings.clone();
    bad_context.bodies[0].fitted = false;
    assert!(
        game.session
            .set_body_pose_assets(game.assets.clone(), bad_context)
            .is_err()
    );
    assert_eq!(game.session.animation_pose_view(), active);
    game.steps(owner, 30)?;
    game.session.take_animation_records();
    let before = game.session.animation_pose_view();
    let body = game.session.archetypes().find(BODY).unwrap();
    let original = game.session.body_pose_asset_info(body)?;
    game.session
        .set_body_pose_assets(game.assets.clone(), game.bindings.clone())?;
    let equivalent = game.session.body_pose_asset_info(body)?;
    assert!(!Arc::ptr_eq(&original, &equivalent));
    assert_eq!(original, equivalent);
    assert_eq!(game.session.animation_pose_view().poses, before.poses);
    let stable = game.session.animation_pose_view();
    let mut changes = vec![];
    let mut fitting = game.bindings.clone();
    fitting.bodies[0].fitted = false;
    changes.push(fitting);
    let mut reference = game.bindings.clone();
    reference.reference_resource = Some(OTHER.into());
    changes.push(reference);
    let mut mismatch = game.bindings.clone();
    mismatch.bodies[0].resource = OTHER.into();
    changes.push(mismatch);
    let mut absent = game.bindings.clone();
    absent.bodies.retain(|b| b.model != MODEL);
    changes.push(absent);
    for bindings in changes {
        assert!(
            game.session
                .set_body_pose_assets(game.assets.clone(), bindings)
                .is_err()
        );
        assert_eq!(game.session.animation_pose_view(), stable);
        assert!(game.session.take_animation_records().is_empty());
    }
    let mut changed = game.assets.clone();
    let mut native = shape();
    native.animations[0].duration = 0.5;
    changed.assets[0].bytes = serde_json::to_vec(&native)?;
    changed.assets[0].digest = bri_package::environment::hash_bytes(&changed.assets[0].bytes);
    assert!(
        game.session
            .set_body_pose_assets(changed.clone(), game.bindings.clone())
            .is_err()
    );
    assert_eq!(game.session.animation_pose_view(), stable);
    game.session.clear_animation_pose(&id)?;
    game.session
        .set_body_pose_assets(changed, game.bindings.clone())?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    Ok(())
}

#[test]
fn actual_tuning_body_birth_and_disconnect_invalidate_held_context() -> Result<()> {
    let mut game = Game::new()?;
    let owner = game.owner()?;
    let request = game.request(owner, 1, "zero");
    let old = game.hold(owner, request)?;
    game.session.take_animation_records();
    game.session
        .tune_archetype(owner, BODY, &[("sockets.right_hand.offset.0".into(), 0.19)])?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    game.steps(owner, 1)?;
    assert!(game.session.take_animation_records().is_empty());
    let request = game.request(owner, 2, "zero");
    game.hold(owner, request)?;
    assert_eq!(
        game.session.animation_pose_view().poses[0]
            .applicability
            .body
            .definition
            .sockets
            .hand(0)
            .unwrap()
            .offset[0],
        0.19
    );
    game.session.take_animation_records();
    game.run(owner, "other", vec![])?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    assert!(game.session.take_animation_records().is_empty());
    assert!(game.session.animation_owner(owner).unwrap().body > old.epochs.body);
    game.run(owner, "body", vec![])?;
    let request = game.request(owner, 3, "zero");
    let prior_life = game.hold(owner, request)?;
    game.session.take_animation_records();
    game.run(owner, "birth", vec![])?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    assert!(game.session.animation_owner(owner).unwrap().life > prior_life.epochs.life);
    assert!(game.session.take_animation_records().is_empty());
    game.run(owner, "body", vec![])?;
    let request = game.request(owner, 1, "zero");
    let before_disconnect = game.hold(owner, request.clone())?;
    game.session.take_animation_records();
    game.session.disconnect(owner)?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    assert!(game.session.take_animation_records().is_empty());
    game.session.resume(owner, Vec3::new(0., 0.05, 0.))?;
    assert!(game.hold(owner, request).is_err());
    assert!(
        game.session
            .clear_animation_pose(&before_disconnect)
            .is_err()
    );
    Ok(())
}

#[test]
fn full_ordinary_set_includes_worn_slots_and_same_id_generations_but_not_emote() -> Result<()> {
    let mut game = Game::new()?;
    let owner = game.owner()?;
    game.run(owner, "tools", vec![])?;
    game.equipped_binding(owner, 0)?;
    for slot in [2, 3] {
        game.run(
            owner,
            "wear",
            vec![
                PackageArg::Int(slot),
                PackageArg::String("posehold:image/worn".into()),
            ],
        )?;
    }
    let request = game.request(owner, 1, "zero");
    let old = game.hold(owner, request)?;
    game.session.take_animation_records();
    let held = game.session.animation_pose_view();
    assert!(
        held.poses[0]
            .applicability
            .images
            .iter()
            .all(Option::is_some)
    );
    for (hand, image) in held.poses[0].applicability.images.iter().enumerate() {
        let image = image.as_ref().unwrap();
        assert_eq!(image.hand, hand as u8);
        assert!(!image.emote);
    }
    for _ in 0..2 {
        game.run(
            owner,
            "emote",
            vec![PackageArg::String("posehold:image/emote".into())],
        )?;
    }
    assert_eq!(game.session.animation_pose_view().poses, held.poses);
    game.run(
        owner,
        "wear",
        vec![
            PackageArg::Int(3),
            PackageArg::String("posehold:image/worn".into()),
        ],
    )?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    assert!(game.session.take_animation_records().is_empty());
    let request = game.request(owner, 2, "zero");
    game.hold(owner, request)?;
    let remounted = game.session.animation_pose_view();
    assert_ne!(
        remounted.poses[0].applicability.images[3],
        held.poses[0].applicability.images[3]
    );
    assert_eq!(
        remounted.poses[0].applicability.images[..3],
        held.poses[0].applicability.images[..3]
    );
    game.run(
        owner,
        "wear",
        vec![PackageArg::Int(2), PackageArg::String(String::new())],
    )?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    let request = game.request(owner, 3, "zero");
    game.hold(owner, request)?;
    game.run(
        owner,
        "scope",
        vec![PackageArg::String("posehold:image/right".into())],
    )?;
    assert!(
        game.session.animation_pose_view().poses.is_empty(),
        "same-ID forced redraw must invalidate"
    );
    assert!(game.session.clear_animation_pose(&old).is_err());
    Ok(())
}

#[test]
fn actual_custody_duplicate_selection_take_and_drop_transfer_revoke_bound_holds() -> Result<()> {
    let mut game = Game::new()?;
    let owner = game.owner()?;
    game.run(owner, "tools", vec![])?;
    let binding = game.equipped_binding(owner, 0)?;
    let mut request = game.request(owner, 1, "zero");
    request.binding = binding.clone();
    game.hold(owner, request.clone())?;
    let images = game.session.animation_pose_view().poses[0]
        .applicability
        .images
        .clone();
    game.session.take_animation_records();
    game.session.equip_tool(owner, Some(1))?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    assert!(game.session.take_animation_records().is_empty());
    let unbound = game.request(owner, 2, "zero");
    let unbound_id = game.hold(owner, unbound)?;
    assert_eq!(
        game.session.animation_pose_view().poses[0]
            .applicability
            .images,
        images,
        "duplicate selection did not remount"
    );
    game.session.clear_animation_pose(&unbound_id)?;
    game.run(owner, "take", vec![])?;
    game.session.equip_tool(owner, Some(0))?;
    game.steps(owner, 1)?;
    let actual = game
        .session
        .animation_view()
        .active
        .into_iter()
        .find(|a| a.id.owner == owner && a.request.target == AnimationTarget::Pawn(2))
        .unwrap();
    let mut drop_request = game.request(owner, 3, "zero");
    drop_request.binding = actual.request.binding.clone();
    game.hold(owner, drop_request.clone())?;
    game.session.take_animation_records();
    game.command(owner, Command::DropTool { slot: 0 })?;
    assert!(game.session.animation_pose_view().poses.is_empty());
    let drop = game.session.weapon_view().drops[0].clone();
    let receiver = game.session.join(
        "Receiver".into(),
        drop.position + Vec3::new(0., -0.1, -0.7),
        true,
    )?;
    game.run(receiver, "body", vec![])?;
    game.run(receiver, "empty", vec![])?;
    game.steps(receiver, 1)?;
    assert!(
        game.session
            .weapon_view()
            .drops
            .iter()
            .all(|d| d.id != drop.id)
    );
    let slot = game.session.tool_inventories()[&receiver]
        .slots
        .iter()
        .position(|i| i.as_deref() == Some("posehold:weapon/gun"))
        .context("actual receiver pickup")?;
    let transferred = game.equipped_binding(receiver, slot)?;
    assert_eq!(
        transferred.item.as_ref().unwrap().item,
        binding.item.as_ref().unwrap().item
    );
    assert_ne!(
        transferred.item.as_ref().unwrap().generation,
        binding.item.as_ref().unwrap().generation
    );
    assert_ne!(transferred.owner, binding.owner);
    assert!(game.hold(owner, drop_request).is_err());
    let mut request = game.request(receiver, 1, "zero");
    request.binding = transferred;
    game.hold(receiver, request)?;
    game.session.take_animation_records();
    game.run(receiver, "tools", vec![])?;
    assert!(
        game.session.animation_pose_view().poses.is_empty(),
        "same-name loadout replacement must revoke custody"
    );
    Ok(())
}

#[test]
fn shared_occupancy_saturates_without_eviction_and_natural_transfer_needs_no_slot() -> Result<()> {
    let mut game = Game::new()?;
    let owners = (0..5).map(|_| game.owner()).collect::<Result<Vec<_>>>()?;
    let request = game.request(owners[0], 1, "finite");
    let finite = game.hold(owners[0], request)?;
    let request = game.request(owners[1], 1, "zero");
    let held = game.hold(owners[1], request)?;
    for &owner in &owners[..4] {
        for channel in 0..32 {
            if channel == 2 && (owner == owners[0] || owner == owners[1]) {
                continue;
            }
            let mut request = game.request(owner, 10 + u64::from(channel), "looped");
            request.target = AnimationTarget::Pawn(channel);
            game.start(owner, request)?;
            game.session.take_animation_records();
        }
    }
    assert_eq!(game.session.animation_view().active.len(), 127);
    let before = game.session.animation_pose_view();
    let request = game.request(owners[4], 1, "zero");
    let error = game.hold(owners[4], request.clone()).unwrap_err();
    assert!(error.to_string().contains("occupancy"));
    assert_eq!(game.session.animation_pose_view(), before);
    assert!(game.session.take_animation_records().is_empty());
    game.steps(owners[0], 30)?;
    assert_eq!(game.pose_phase(&finite), Some(0.25));
    assert_eq!(game.session.animation_view().active.len(), 126);
    assert_eq!(game.session.animation_pose_view().poses.len(), 2);
    assert_eq!(
        game.session
            .take_animation_records()
            .iter()
            .filter(|r| r.outcome == AnimationOutcome::Natural)
            .count(),
        1
    );
    let same = game.request(owners[1], 100, "zero");
    game.hold(owners[1], same)?;
    game.session.clear_animation_pose(&finite)?;
    game.hold(owners[4], request)?;
    assert_eq!(
        game.session.animation_view().active.len()
            + game
                .session
                .animation_pose_view()
                .poses
                .iter()
                .filter(|p| p.state == AnimationPoseState::HeldFinal)
                .count(),
        128
    );
    assert!(
        game.session.clear_animation_pose(&held).is_err(),
        "replacement's pose belongs to its new ID"
    );
    Ok(())
}

#[test]
fn independent_result_pressure_reserves_natural_but_never_held_terminal_capacity() -> Result<()> {
    let mut game = Game::new()?;
    let owner = game.owner()?;
    let second = game.owner()?;
    let request = game.request(owner, 1, "zero");
    let held = game.hold(owner, request)?;
    let request = game.request(second, 1, "finite");
    let finite = game.hold(second, request)?;
    for nonce in 2..=255 {
        let mut request = game.request(owner, nonce, "zero");
        request.target = AnimationTarget::Pawn(3);
        game.start(owner, request)?;
    }
    let before = game.session.animation_pose_view();
    let mut next = game.request(owner, 256, "zero");
    next.target = AnimationTarget::Pawn(3);
    assert!(
        game.start(owner, next.clone())
            .unwrap_err()
            .to_string()
            .contains("result budget")
    );
    assert_eq!(game.session.animation_pose_view(), before);
    game.steps(second, 30)?;
    assert_eq!(game.pose_phase(&finite), Some(0.25));
    game.session.clear_animation_pose(&held)?;
    assert!(
        game.start(owner, next.clone()).is_err(),
        "clearing a hold cannot drain records"
    );
    let records = game.session.take_animation_records();
    assert_eq!(records.len(), 512);
    assert_eq!(
        records
            .iter()
            .filter(|r| r.playback.id == finite && r.outcome == AnimationOutcome::Natural)
            .count(),
        1
    );
    let surviving = game.session.animation_pose_view();
    game.start(owner, next)?;
    assert_eq!(game.session.animation_pose_view().poses, surviving.poses);
    Ok(())
}

#[test]
fn receipt_eviction_does_not_evict_or_resurrect_held_pose() -> Result<()> {
    let mut game = Game::new()?;
    let owner = game.owner()?;
    let request = game.request(owner, 1, "zero");
    let held = game.hold(owner, request.clone())?;
    game.session.take_animation_records();
    for nonce in 2..=260 {
        let mut value = game.request(owner, nonce, "zero");
        value.target = AnimationTarget::Pawn(3);
        game.start(owner, value)?;
        game.session.take_animation_records();
    }
    let before = game.session.animation_pose_view();
    assert_eq!(game.pose_phase(&held), Some(0.));
    assert!(game.hold(owner, request.clone()).is_err());
    assert_eq!(game.session.animation_pose_view(), before);
    game.session.clear_animation_pose(&held)?;
    assert!(game.hold(owner, request).is_err());
    assert!(game.session.animation_pose_view().poses.is_empty());
    assert!(game.session.take_animation_records().is_empty());
    Ok(())
}

#[test]
fn prepared_pose_revalidates_actual_mount_and_body_changes_without_consuming_nonce() -> Result<()> {
    let mut game = Game::new()?;
    let owner = game.owner()?;
    let request = game.request(owner, 1, "finite");
    let prepared = game.session.prepare_animation_start_with_pose(
        owner,
        request.clone(),
        AnimationPoseRetention::HoldNativeFinal,
    )?;
    let before = game.session.animation_view();
    game.run(
        owner,
        "wear",
        vec![
            PackageArg::Int(2),
            PackageArg::String("posehold:image/worn".into()),
        ],
    )?;
    assert_eq!(
        game.session.animation_view(),
        before,
        "mount change alone has no clock mutation"
    );
    assert!(game.session.commit_animation_start(prepared).is_err());
    assert_eq!(game.session.animation_view(), before);
    assert!(game.session.take_animation_records().is_empty());
    let prepared = game.session.prepare_animation_start_with_pose(
        owner,
        request.clone(),
        AnimationPoseRetention::HoldNativeFinal,
    )?;
    game.session
        .tune_archetype(owner, BODY, &[("sockets.right_hand.offset.0".into(), 0.19)])?;
    assert!(game.session.commit_animation_start(prepared).is_err());
    assert!(game.session.animation_view().active.is_empty());
    assert!(game.session.take_animation_records().is_empty());
    let prepared = game.session.prepare_animation_start_with_pose(
        owner,
        request.clone(),
        AnimationPoseRetention::HoldNativeFinal,
    )?;
    let mut other = game.request(owner, 2, "looped");
    other.namespace = "other-clock".into();
    other.target = AnimationTarget::Pawn(1);
    game.start(owner, other)?;
    let stable = game.session.animation_view();
    assert!(game.session.commit_animation_start(prepared).is_err());
    assert_eq!(game.session.animation_view(), stable);
    game.hold(owner, request)?;
    Ok(())
}

#[test]
fn pending_selection_preserves_unbound_hold_until_actual_images_change() -> Result<()> {
    let mut game = Game::new()?;
    let owner = game.owner()?;
    game.run(owner, "empty", vec![])?;
    let blocked = game.session.give_item(owner, "posehold:weapon/blocked")?;
    let next = game.session.give_item(owner, "posehold:weapon/gun")?;
    game.session.equip_tool(owner, Some(blocked))?;
    game.steps(owner, 1)?;
    let request = game.request(owner, 1, "zero");
    let held = game.hold(owner, request)?;
    let before = game.session.animation_pose_view();
    game.session.equip_tool(owner, Some(next))?;
    assert_eq!(game.session.tool_inventories()[&owner].selected, Some(next));
    assert_eq!(game.session.animation_pose_view().poses, before.poses);
    assert_eq!(
        game.session.weapon_view().images[&owner][0].image,
        "posehold:image/blocked"
    );
    game.steps(owner, 12)?;
    assert_eq!(
        game.session.weapon_view().images[&owner][0].image,
        "posehold:image/right"
    );
    assert!(game.pose_phase(&held).is_none());
    Ok(())
}

#[test]
fn actual_death_clears_active_or_held_once_and_rejects_dead_admission() -> Result<()> {
    for sequence in ["finite", "zero"] {
        let mut game = Game::new()?;
        let owner = game.owner()?;
        let request = game.request(owner, 1, sequence);
        let id = game.hold(owner, request.clone())?;
        game.command(owner, Command::Suicide)?;
        assert!(!game.session.is_alive(owner));
        assert!(game.session.animation_pose_view().poses.is_empty());
        assert!(game.hold(owner, request).is_err());
        assert!(game.session.clear_animation_pose(&id).is_err());
        let records = game.session.take_animation_records();
        assert_eq!(
            records.iter().map(|r| r.outcome).collect::<Vec<_>>(),
            [
                AnimationOutcome::Started,
                if sequence == "zero" {
                    AnimationOutcome::Natural
                } else {
                    AnimationOutcome::Cancelled
                }
            ]
        );
        let wait = game.session.vitals()[&owner]
            .respawn_tick
            .saturating_sub(game.session.simulation().state().tick);
        assert!(wait <= 600, "fixture respawn wait must stay bounded");
        for _ in 0..wait {
            game.session.step()?;
        }
        game.command(owner, Command::Respawn)?;
        assert!(game.session.is_alive(owner));
        game.run(owner, "body", vec![])?;
        let request = game.request(owner, 1, "zero");
        let fresh = game.hold(owner, request)?;
        assert!(fresh.epochs.life > id.epochs.life);
    }
    Ok(())
}

#[test]
fn stale_item_or_image_queued_root_cannot_clear_new_hold() -> Result<()> {
    for redraw in [false, true] {
        let mut game = Game::new()?;
        let owner = game.owner()?;
        game.run(owner, "tools", vec![])?;
        let old = game.equipped_binding(owner, 0)?;
        let due = game.session.simulation().state().tick + 12;
        // Activate queued a real root under slot0 custody. Either selection
        // changes without remount, or forced redraw changes only image identity.
        if redraw {
            game.run(
                owner,
                "scope",
                vec![PackageArg::String("posehold:image/right".into())],
            )?;
            let new = game.equipped_binding(owner, 0)?;
            assert_eq!(new.item, old.item);
            assert_ne!(new.image, old.image);
        } else {
            game.session.equip_tool(owner, Some(1))?;
            assert_eq!(old.item.as_ref().unwrap().slot, 0);
        }
        let request = game.request(owner, 1, "zero");
        let id = game.hold(owner, request)?;
        let before = game.session.animation_pose_view();
        game.session.take_animation_records();
        let wait = due - game.session.simulation().state().tick;
        game.steps(owner, wait)?;
        assert_eq!(game.pose_phase(&id), Some(0.));
        assert_eq!(game.session.animation_pose_view().poses, before.poses);
        assert!(game.session.take_animation_records().is_empty());
        // A current ordinary free root still clears this target.
        game.run(owner, "root", vec![])?;
        assert!(game.session.animation_pose_view().poses.is_empty());
        assert!(game.session.take_animation_records().is_empty());
    }
    Ok(())
}
