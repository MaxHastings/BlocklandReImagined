//! Trusted-host native clock/weapon composition, not a package completion bridge.
use anyhow::{Context, Result};
use bri_content::shape::Shape;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{
        AnimationAsset, AnimationAssets, AnimationImageAsset, AnimationOutcome, AnimationRecord,
        AnimationStart, AnimationTarget, Command, Notice, PackageCommand, PlaybackId, Session,
    },
    simulation::Simulation,
};
use bri_weapons::{AnimationBinding, Pack};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::sync::Arc;

const ITEM: &str = "patch:weapon/rifle";
const IMAGE: &str = "patch:image/rifle";
const MODEL: &str = "patch:body/rifle";
const SCRIPT: &str = r#"
fn cmd_seed7(p) { set_rounds(p, "patch:weapon/rifle", 7); }
fn cmd_seed12(p) { set_rounds(p, "patch:weapon/rifle", 12); }
fn cmd_probe(p) { set_player(p, "observed", player(p).magazine.rounds); }
"#;

struct Fixture {
    _root: tempfile::TempDir,
    session: Session,
    owner: u64,
    other: u64,
    first: usize,
    second: usize,
    binding: AnimationBinding,
    command: u64,
}

impl Fixture {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir()?;
        let rules = root.path().join("rules");
        std::fs::create_dir(&rules)?;
        std::fs::write(rules.join("package.json"), json!({
            "schema_version":1,"id":"patchrules","version":"1.0.0","api":1,
            "name":"Invented patch setup","license":"CC0-1.0","capabilities":["player"],
            "provides":[{"kind":"behaviour","id":"patchrules:behaviour/main","file":"behaviour.json"},
                {"kind":"script","id":"patchrules:script/main","file":"main.rhai"}]
        }).to_string())?;
        std::fs::write(
            rules.join("behaviour.json"),
            json!({
                "schema_version":1,"script":"main.rhai",
            "commands":[{"name":"seed7"},{"name":"seed12"},{"name":"probe"}],
                "state":{"player":{"observed":{"default":0,"visible":"server","persist":false}}}
            })
            .to_string(),
        )?;
        std::fs::write(rules.join("main.rhai"), SCRIPT)?;
        let catalog = Catalog::load(
            root.path(),
            &PackageSet {
                schema_version: 1,
                packages: vec![PackageEntry {
                    id: "patchrules".into(),
                    version: "1.0.0".into(),
                    side: Side::Server,
                    dir: "rules".into(),
                    role: None,
                }],
            },
            true,
        )
        .map_err(|error| anyhow::anyhow!("{error:#?}"))?;
        let mut session = Session::new(Simulation::new(
            World::new(
                "Patch clocks".into(),
                "test:patch-map".into(),
                vec![[1.; 4]],
            ),
            Definitions::default(),
            vec![ColliderBuilder::cuboid(100., 0.5, 100.).translation(Vector::new(0., -0.5, 0.))],
        )?);
        session.install_packages(Arc::new(catalog), None)?;
        session.set_weapon_pack(Pack::from_json(&serde_json::to_vec(&json!({
            "schema_version":5,"id":"patch",
            "items":{ITEM:{"ui_name":"Rifle","image":IMAGE}},
            "images":{IMAGE:{"model":MODEL,
                "magazine":{"size":31,"ammo":"rifle","reload_ticks":1,"reserve":0},
                "states":[{"name":"Activate","sequence":"probe","ticks":1,"timeout":1},
                    {"name":"Ready"},{"name":"Busy"}]}},"projectiles":{}
        }))?)?)?;
        let clip = |name: &str, duration: f64, looping: bool| {
            json!({
                "name":name,"frames":2,"duration":duration,"looping":looping,
                "additive":false,"priority":0,
                "nodes":[{"node":"root","translations":[[0.,0.,0.],[1.,0.,0.]],
                    "rotations":[],"scales":[],"scale_rotations":[]}],
                "objects":[],"ground_translations":[],"ground_rotations":[],"triggers":[]
            })
        };
        let shape: Shape = serde_json::from_value(json!({
            "schema_version":1,"id":MODEL,
            "nodes":[{"name":"root","parent":null,"translation":[0.,0.,0.],"rotation":[0.,0.,0.,1.]}],
            "objects":[],"details":[],"meshes":[],"materials":[],
            "animations":[clip("probe",0.015625,false),clip("reload",0.25,false),
                clip("zero",0.,false),clip("loop",0.25,true)]
        }))?;
        shape.validate()?;
        let bytes = serde_json::to_vec(&shape)?;
        session.set_animation_assets(AnimationAssets {
            assets: vec![AnimationAsset {
                resource: MODEL.into(),
                digest: bri_package::environment::hash_bytes(&bytes),
                rig: false,
                library_clips: Vec::new(),
                bytes,
            }],
            images: [(
                IMAGE.into(),
                AnimationImageAsset {
                    model: MODEL.into(),
                    resource: MODEL.into(),
                },
            )]
            .into(),
        })?;
        let owner = session.join("Patch owner".into(), Vec3::new(0., 0.05, 0.), true)?;
        let other = session.join("Other owner".into(), Vec3::new(5., 0.05, 0.), false)?;
        let first = session.give_item(owner, ITEM)?;
        let second = session.give_item(owner, ITEM)?;
        session.equip_tool(owner, Some(second))?;
        let mut fixture = Self {
            _root: root,
            session,
            owner,
            other,
            first,
            second,
            binding: AnimationBinding::default(),
            command: 0,
        };
        fixture.cmd("seed7")?;
        fixture.session.equip_tool(owner, None)?;
        fixture.session.equip_tool(owner, Some(first))?;
        fixture.steps(1)?;
        fixture.binding = fixture
            .session
            .take_animation_records()
            .into_iter()
            .find(|record| {
                record.outcome == AnimationOutcome::Started
                    && record.playback.request.target
                        == AnimationTarget::Image {
                            hand: 0,
                            emote: false,
                        }
            })
            .context("Ordinary mounted native producer did not provide actual binding")?
            .playback
            .request
            .binding;
        assert_eq!(fixture.binding.item.as_ref().unwrap().slot, first);
        fixture.cmd("seed12")?;
        fixture.steps(30)?;
        assert!(fixture.session.animation_view().active.is_empty());
        fixture.session.take_animation_records();
        fixture.session.take_private_notices();
        Ok(fixture)
    }

    fn cmd(&mut self, name: &str) -> Result<()> {
        self.command += 1;
        self.session.command(
            self.owner,
            self.command,
            Command::Package(PackageCommand {
                package: "patchrules".into(),
                command: name.into(),
                args: vec![],
            }),
        )?;
        Ok(())
    }

    fn rounds(&mut self) -> Result<u64> {
        self.cmd("probe")?;
        self.session
            .package_value("patchrules", self.owner, "observed")
            .and_then(|value| value.as_u64())
            .context("Missing actual native ammo readback")
    }

    fn state(&self) -> String {
        self.session.weapon_view().images[&self.owner]
            .iter()
            .find(|image| image.hand == 0)
            .unwrap()
            .state
            .clone()
    }

    fn steps(&mut self, count: u64) -> Result<()> {
        for _ in 0..count {
            self.session.movement(
                self.owner,
                self.session.simulation().state().tick + 100,
                MoveInput::default(),
            )?;
            self.session.step()?;
        }
        Ok(())
    }

    fn request(&self, nonce: u64) -> AnimationStart {
        AnimationStart {
            namespace: "patch-action".into(),
            nonce,
            target: AnimationTarget::Weapon(self.first),
            sequence: "reload".into(),
            binding: self.binding.clone(),
            rate: 1.,
            tween: 0.,
        }
    }

    fn fill(&mut self) -> Result<Vec<PlaybackId>> {
        let mut ids = vec![];
        for nonce in 1..=256 {
            let mut request = self.request(nonce);
            request.namespace = "patch-fill".into();
            request.sequence = "zero".into();
            let prepared = self.session.prepare_animation_start(self.owner, request)?;
            ids.push(self.session.commit_animation_start(prepared)?);
        }
        Ok(ids)
    }

    fn attempt(
        &mut self,
        request: AnimationStart,
        expected: Option<u32>,
        rounds: Option<u32>,
        state: Option<&str>,
    ) -> Result<PlaybackId> {
        self.session
            .start_weapon_animation_with_patch(self.owner, request, expected, rounds, state)
    }
}

fn verify_fill(records: &[AnimationRecord], ids: &[PlaybackId]) {
    assert_eq!(records.len(), 512);
    for (pair, id) in records.chunks_exact(2).zip(ids) {
        assert_eq!(&pair[0].playback.id, id);
        assert_eq!(&pair[1].playback.id, id);
        assert_eq!(pair[0].outcome, AnimationOutcome::Started);
        assert_eq!(pair[1].outcome, AnimationOutcome::Natural);
    }
}

#[test]
fn real_clock_refusal_aborts_patch_then_same_nonce_admits_exactly_once() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let ids = fixture.fill()?;
    let before = fixture.session.animation_view();
    let request = fixture.request(1);
    let error = fixture
        .attempt(request.clone(), Some(12), Some(31), Some("Busy"))
        .unwrap_err();
    assert!(error.to_string().contains("budget"), "{error:#}");
    assert_eq!(
        fixture.rounds()?,
        12,
        "Clock refusal must leave prepared magazine unchanged"
    );
    assert_eq!(fixture.state(), "Ready");
    assert_eq!(fixture.session.animation_view(), before);
    verify_fill(&fixture.session.take_animation_records(), &ids);
    fixture.steps(1)?;
    assert!(
        !fixture
            .session
            .take_private_notices()
            .iter()
            .any(|(_, notice)| matches!(notice, Notice::Ammo(_) | Notice::InventoryAmmo(_)))
    );
    let tick = fixture.session.simulation().state().tick;
    let id = fixture.attempt(request, Some(12), Some(31), Some("Busy"))?;
    assert_eq!(id.serial, ids.last().unwrap().serial + 1);
    assert_eq!(fixture.rounds()?, 31);
    assert_eq!(fixture.state(), "Busy");
    let records = fixture.session.take_animation_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, AnimationOutcome::Started);
    assert_eq!(records[0].playback.id, id);
    assert_eq!(records[0].tick, tick);
    assert_eq!(records[0].playback.clip.resource, MODEL);
    fixture.steps(1)?;
    let notices = fixture.session.take_private_notices();
    let slots = notices
        .iter()
        .find_map(|(owner, notice)| match notice {
            Notice::InventoryAmmo(slots) if *owner == fixture.owner => Some(slots),
            _ => None,
        })
        .context("Missing actual private inventory ammo projection")?;
    assert_eq!(slots[fixture.first].as_ref().unwrap().rounds, Some(31));
    assert_eq!(slots[fixture.second].as_ref().unwrap().rounds, Some(7));
    assert!(
        !notices.iter().any(|(owner, notice)| *owner == fixture.other
            && matches!(notice, Notice::Ammo(_) | Notice::InventoryAmmo(_)))
    );
    fixture.steps(30)?;
    let records = fixture.session.take_animation_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, AnimationOutcome::Natural);
    assert_eq!(records[0].playback.id, id);
    assert_eq!(fixture.rounds()?, 31);
    assert_eq!(fixture.state(), "Busy");
    Ok(())
}

#[test]
fn invalid_requests_and_patch_refusals_leave_no_clock_or_weapon_writes() -> Result<()> {
    let mut fixture = Fixture::new()?;
    for case in 0..16 {
        let mut request = fixture.request(1);
        let (mut expected, mut rounds, mut state) = (Some(12), Some(31), Some("Busy"));
        match case {
            0 => expected = Some(11),
            1 => rounds = Some(32),
            2 => state = Some("missing"),
            3 => request.target = AnimationTarget::Pawn(15),
            4 => request.target = AnimationTarget::Weapon(fixture.second),
            5 => request.binding.image.as_mut().unwrap().hand = 3,
            6 => request.binding.image.as_mut().unwrap().emote = true,
            7 => request.rate = 2.,
            8 => request.tween = 0.05,
            9 => request.sequence = "loop".into(),
            10 => request.sequence = "missing".into(),
            11 => request.binding.owner.as_mut().unwrap().life += 1,
            12 => request.binding.item.as_mut().unwrap().generation += 1,
            13 => request.binding.image.as_mut().unwrap().generation += 1,
            14 => request.rate = f64::NAN,
            15 => request.nonce = 0,
            _ => unreachable!(),
        }
        let before = fixture.session.animation_view();
        assert!(
            fixture.attempt(request, expected, rounds, state).is_err(),
            "case {case}"
        );
        assert_eq!(fixture.session.animation_view(), before, "case {case}");
        assert!(fixture.session.take_animation_records().is_empty());
        assert_eq!(fixture.rounds()?, 12, "case {case}");
        assert_eq!(fixture.state(), "Ready");
    }
    fixture.steps(1)?;
    assert!(
        !fixture
            .session
            .take_private_notices()
            .iter()
            .any(|(_, notice)| matches!(notice, Notice::Ammo(_) | Notice::InventoryAmmo(_)))
    );
    let request = fixture.request(1);
    fixture.attempt(request, Some(12), Some(31), Some("Busy"))?;
    assert_eq!(fixture.rounds()?, 31);
    Ok(())
}

#[test]
fn occupied_target_and_duplicate_after_natural_cannot_apply_another_patch() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let request = fixture.request(1);
    let id = fixture.attempt(request.clone(), Some(12), Some(31), Some("Busy"))?;
    fixture.session.take_animation_records();
    let before = fixture.session.animation_view();
    let replacement = fixture.request(2);
    let error = fixture
        .attempt(replacement, Some(31), Some(0), Some("Ready"))
        .unwrap_err();
    assert!(error.to_string().contains("occupied"));
    assert_eq!(fixture.session.animation_view(), before);
    assert!(fixture.session.take_animation_records().is_empty());
    fixture.steps(30)?;
    let records = fixture.session.take_animation_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].outcome, AnimationOutcome::Natural);
    assert_eq!(records[0].playback.id, id);
    let before = fixture.session.animation_view();
    // Valid writes, but an existing host receipt must not reapply them.
    let error = fixture
        .attempt(request, Some(31), Some(0), Some("Ready"))
        .unwrap_err();
    assert!(error.to_string().contains("Duplicate"), "{error:#}");
    assert_eq!(fixture.session.animation_view(), before);
    assert!(fixture.session.take_animation_records().is_empty());
    assert_eq!(fixture.rounds()?, 31);
    assert_eq!(fixture.state(), "Busy");
    let retry = fixture.request(2);
    let next = fixture.attempt(retry, Some(31), None, None)?;
    assert_eq!(next.serial, id.serial + 1);
    assert_eq!(fixture.rounds()?, 31);
    Ok(())
}

#[test]
fn admitted_weapon_clock_survives_holster_until_its_native_natural() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let request = fixture.request(1);
    let id = fixture.attempt(request, Some(12), Some(31), Some("Busy"))?;
    fixture.session.take_animation_records();
    fixture.session.equip_tool(fixture.owner, None)?;
    assert_eq!(fixture.session.animation_view().active[0].id, id);
    assert!(fixture.session.take_animation_records().is_empty());
    fixture.steps(30)?;
    let records = fixture.session.take_animation_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].playback.id, id);
    assert_eq!(records[0].outcome, AnimationOutcome::Natural);
    Ok(())
}

#[test]
fn actual_selection_redraw_and_death_reject_old_admission_binding() -> Result<()> {
    for case in 0..3 {
        let mut fixture = Fixture::new()?;
        let request = fixture.request(1);
        match case {
            0 => fixture
                .session
                .equip_tool(fixture.owner, Some(fixture.second))?,
            1 => {
                fixture.session.equip_tool(fixture.owner, None)?;
                fixture
                    .session
                    .equip_tool(fixture.owner, Some(fixture.first))?;
                fixture.steps(30)?;
            }
            2 => {
                fixture.command += 1;
                fixture
                    .session
                    .command(fixture.owner, fixture.command, Command::Suicide)?;
                assert!(!fixture.session.is_alive(fixture.owner));
            }
            _ => unreachable!(),
        }
        fixture.session.take_animation_records();
        fixture.session.take_private_notices();
        let before = fixture.session.animation_view();
        let images = fixture.session.weapon_view().images;
        assert!(
            fixture
                .attempt(request, Some(12), Some(31), Some("Busy"))
                .is_err()
        );
        assert_eq!(fixture.session.animation_view(), before);
        assert_eq!(fixture.session.weapon_view().images, images);
        assert!(fixture.session.take_animation_records().is_empty());
        if case != 2 {
            assert_eq!(fixture.rounds()?, if case == 0 { 7 } else { 12 });
        }
    }
    Ok(())
}

#[test]
fn zero_native_duration_orders_started_natural_and_commits_patch_once() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let mut request = fixture.request(1);
    request.sequence = "zero".into();
    let tick = fixture.session.simulation().state().tick;
    let id = fixture.attempt(request, Some(12), Some(31), Some("Busy"))?;
    let records = fixture.session.take_animation_records();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].outcome, AnimationOutcome::Started);
    assert_eq!(records[1].outcome, AnimationOutcome::Natural);
    for record in records {
        assert_eq!(record.playback.id, id);
        assert_eq!(record.tick, tick);
    }
    assert!(fixture.session.animation_view().active.is_empty());
    assert_eq!(fixture.rounds()?, 31);
    assert_eq!(fixture.state(), "Busy");
    fixture.steps(1)?;
    assert_eq!(
        fixture
            .session
            .take_private_notices()
            .iter()
            .filter(|(owner, notice)| *owner == fixture.owner && matches!(notice, Notice::Ammo(_)))
            .count(),
        1
    );
    Ok(())
}

#[test]
fn event_capacity_is_reserved_before_clock_mutation_and_refusal_consumes_no_nonce() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let mut last = None;
    for nonce in 1..=8192 {
        let mut request = fixture.request(nonce);
        request.sequence = "zero".into();
        let id = fixture.attempt(request, Some(12), Some(12), None)?;
        let records = fixture.session.take_animation_records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].playback.id, id);
        assert_eq!(records[1].outcome, AnimationOutcome::Natural);
        last = Some(id);
    }
    let before = fixture.session.animation_view();
    let request = fixture.request(8193);
    let error = fixture
        .attempt(request.clone(), Some(12), Some(31), Some("Busy"))
        .unwrap_err();
    assert!(error.to_string().contains("event budget"), "{error:#}");
    assert_eq!(fixture.session.animation_view(), before);
    assert!(fixture.session.take_animation_records().is_empty());
    assert_eq!(fixture.rounds()?, 12);
    assert_eq!(fixture.state(), "Ready");
    fixture.steps(1)?;
    fixture.session.take_private_notices();
    let id = fixture.attempt(request, Some(12), Some(31), Some("Busy"))?;
    assert_eq!(id.serial, last.unwrap().serial + 1);
    assert_eq!(fixture.rounds()?, 31);
    Ok(())
}
