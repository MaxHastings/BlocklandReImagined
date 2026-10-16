//! Owning-package native admission and state-only observation through ordinary input.
use anyhow::{Context, Result};
use bri_content::shape::Shape;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::Catalog;
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    session::{
        AnimationAsset, AnimationAssets, AnimationImageAsset, AnimationOutcome, AnimationStart,
        AnimationTarget, Command, Notice, PackageCommand, PlaybackId, Session,
    },
    simulation::Simulation,
};
use bri_weapons::{AnimationBinding, Pack};
use bri_world::World;
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::sync::Arc;

#[test]
fn typed_command_has_no_image_authority_and_keeps_speculative_state() -> Result<()> {
    let mut fixture = Fixture::new()?;
    assert!(fixture.cmd("reload").is_err());
    assert_eq!(fixture.rounds()?, 12);
    assert_eq!(fixture.state(), "Ready");
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "busy"),
        Some(json!(false))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "spare"),
        Some(json!(30))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "nonce"),
        Some(json!(0))
    );
    assert!(fixture.session.take_animation_records().is_empty());
    Ok(())
}

#[test]
fn real_clock_refusal_rolls_back_package_exchange_and_does_not_consume_intent() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let ids = fixture.fill()?;
    let before = fixture.session.animation_view();
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    assert_eq!(fixture.rounds()?, 12);
    assert_eq!(fixture.state(), "Ready");
    assert_eq!(fixture.session.animation_view(), before);
    for (key, value) in [
        ("busy", json!(false)),
        ("spare", json!(30)),
        ("nonce", json!(0)),
        ("handle", json!(-1)),
    ] {
        assert_eq!(
            fixture
                .session
                .package_value("patchrules", fixture.owner, key),
            Some(value)
        );
    }
    let records = fixture.session.take_animation_records();
    assert_eq!(records.len(), 512);
    for (pair, id) in records.chunks_exact(2).zip(&ids) {
        assert_eq!(&pair[0].playback.id, id);
        assert_eq!(&pair[1].playback.id, id);
        assert_eq!(pair[0].outcome, AnimationOutcome::Started);
        assert_eq!(pair[1].outcome, AnimationOutcome::Natural);
    }
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    assert_eq!(fixture.rounds()?, 31);
    let records = fixture.session.take_animation_records();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].playback.id.serial,
        ids.last().unwrap().serial + 1
    );
    assert_eq!(records[0].playback.request.nonce, 1);
    fixture.steps(1)?;
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "observations"),
        Some(json!(1))
    );
    Ok(())
}

#[test]
fn provider_dependency_observer_digest_and_actual_model_are_authorized() -> Result<()> {
    for case in 0..5 {
        let mut fixture = Fixture::configured(SCRIPT, case != 0, case != 1, false)?;
        if case >= 2 {
            let mut bytes = std::fs::read(fixture._root.path().join("content/rifle.json"))?;
            if case == 2 {
                bytes.push(b'\n');
            }
            let registration = fixture.session.set_animation_assets(AnimationAssets {
                assets: vec![AnimationAsset {
                    resource: MODEL.into(),
                    digest: bri_package::environment::hash_bytes(&bytes),
                    rig: false,
                    bytes,
                }],
                images: [(
                    IMAGE.into(),
                    AnimationImageAsset {
                        model: if case == 3 {
                            "patch:body/wrong-model".into()
                        } else {
                            MODEL.into()
                        },
                        resource: MODEL.into(),
                    },
                )]
                .into(),
            });
            if case == 3 {
                assert!(registration.is_err());
                assert_eq!(fixture.rounds()?, 12);
                assert_eq!(fixture.state(), "Ready");
                continue;
            }
            registration?;
            if case == 4 {
                // Registered source matches, but the script asks for undeclared foreign content.
                let script = SCRIPT.replace(
                    "\"patch:body/rifle\", \"reload\"",
                    "\"foreign:body/rifle\", \"reload\"",
                );
                std::fs::write(fixture._root.path().join("rules/main.rhai"), script)?;
                fixture
                    .session
                    .reload_rules()
                    .map_err(|errors| anyhow::anyhow!("{errors:#?}"))?;
            }
        }
        let before = fixture.session.animation_view();
        fixture.command += 1;
        fixture
            .session
            .command(fixture.owner, fixture.command, Command::ToggleLight)?;
        assert_eq!(fixture.rounds()?, 12, "case {case}");
        assert_eq!(fixture.state(), "Ready");
        assert_eq!(fixture.session.animation_view(), before, "case {case}");
        assert!(fixture.session.take_animation_records().is_empty());
        assert_eq!(
            fixture
                .session
                .package_value("patchrules", fixture.owner, "busy"),
            Some(json!(false))
        );
        assert_eq!(
            fixture
                .session
                .package_value("patchrules", fixture.owner, "spare"),
            Some(json!(30))
        );
        assert!(
            !fixture.session.package_diagnostics().is_empty(),
            "case {case}"
        );
    }
    Ok(())
}

#[test]
fn observer_first_preserves_host_records_and_custody_is_distinct_from_selection() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    fixture.session.equip_tool(fixture.owner, None)?;
    fixture.steps(30)?;
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "observations"),
        Some(json!(2))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "custody"),
        Some(json!(true))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "selection"),
        Some(json!(false))
    );
    let records = fixture.session.take_animation_records();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].outcome, AnimationOutcome::Started);
    assert_eq!(records[1].outcome, AnimationOutcome::Natural);
    assert_eq!(records[0].playback.id, records[1].playback.id);
    assert!(fixture.session.take_animation_records().is_empty());
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "busy"),
        Some(json!(true))
    );
    Ok(())
}

#[test]
fn observer_errors_world_operations_and_recursive_admission_are_not_retried() -> Result<()> {
    for injection in [
        "throw \"observer failure\";",
        "set_rounds(p, \"patch:weapon/rifle\", 0);",
        "start_weapon_animation(p, \"patch:body/rifle\", \"reload\", 2, 31, 0, \"Ready\");",
    ] {
        let script = SCRIPT.replace("set_player(p, \"token\", info.token);", injection);
        let mut fixture = Fixture::configured(&script, true, true, false)?;
        fixture.command += 1;
        fixture
            .session
            .command(fixture.owner, fixture.command, Command::ToggleLight)?;
        fixture.steps(1)?;
        assert_eq!(
            fixture
                .session
                .package_value("patchrules", fixture.owner, "observations"),
            Some(json!(0))
        );
        assert_eq!(fixture.rounds()?, 31);
        assert_eq!(fixture.state(), "Busy");
        let records = fixture.session.take_animation_records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].outcome, AnimationOutcome::Started);
        let diagnostics = fixture.session.package_diagnostics().len();
        assert!(diagnostics > 0);
        fixture.steps(1)?;
        assert!(fixture.session.take_animation_records().is_empty());
        assert_eq!(fixture.session.package_diagnostics().len(), diagnostics);
        assert_eq!(
            fixture
                .session
                .package_value("patchrules", fixture.owner, "observations"),
            Some(json!(0))
        );
    }
    Ok(())
}

#[test]
fn mixed_world_operations_refuse_whole_call_before_package_or_native_mutation() -> Result<()> {
    let script = SCRIPT.replace(
        "set_player(p, \"handle\", handle);",
        "set_player(p, \"handle\", handle); set_image_state(p, \"Ready\");",
    );
    let mut fixture = Fixture::configured(&script, true, true, false)?;
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    assert_eq!(fixture.rounds()?, 12);
    assert_eq!(fixture.state(), "Ready");
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "spare"),
        Some(json!(30))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "nonce"),
        Some(json!(0))
    );
    assert!(fixture.session.animation_view().active.is_empty());
    assert!(fixture.session.take_animation_records().is_empty());
    assert!(!fixture.session.package_diagnostics().is_empty());
    Ok(())
}

#[test]
fn queued_command_cannot_adopt_a_replacement_mount_with_the_same_content_id() -> Result<()> {
    let mut fixture = Fixture::configured(SCRIPT, true, true, true)?;
    fixture.cmd("activate")?;
    fixture.steps(1)?;
    assert_eq!(fixture.rounds()?, 12);
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "busy"),
        Some(json!(false))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "nonce"),
        Some(json!(0))
    );
    assert!(
        !fixture
            .session
            .animation_view()
            .active
            .iter()
            .any(|a| matches!(a.request.target, AnimationTarget::Weapon(_)))
    );
    assert!(
        !fixture
            .session
            .take_animation_records()
            .iter()
            .any(|r| matches!(r.playback.request.target, AnimationTarget::Weapon(_)))
    );
    assert!(!fixture.session.package_diagnostics().is_empty());
    Ok(())
}

#[test]
fn reloaded_script_does_not_adopt_old_routes_or_cancel_the_existing_clock() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    let id = fixture.session.animation_view().active[0].id.clone();
    std::fs::write(
        fixture._root.path().join("rules/main.rhai"),
        format!("{SCRIPT}\n// changed rule source\n"),
    )?;
    assert_eq!(
        fixture
            .session
            .reload_rules()
            .map_err(|errors| anyhow::anyhow!("{errors:#?}"))?,
        vec!["patchrules"]
    );
    assert_eq!(fixture.session.animation_view().active[0].id, id);
    fixture.steps(30)?;
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "observations"),
        Some(json!(0))
    );
    let records = fixture.session.take_animation_records();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].outcome, AnimationOutcome::Started);
    assert_eq!(records[1].outcome, AnimationOutcome::Natural);
    assert!(records.iter().all(|r| r.playback.id == id));
    assert_eq!(fixture.rounds()?, 31);
    assert_eq!(fixture.state(), "Busy");
    Ok(())
}

#[test]
fn zero_duration_ack_and_terminal_preserve_lossless_ticks_and_host_delivery() -> Result<()> {
    let script = SCRIPT.replace(
        "\"reload\", get_player(p, \"nonce\")",
        "\"zero\", get_player(p, \"nonce\")",
    );
    let mut fixture = Fixture::configured(&script, true, true, false)?;
    let authority = fixture.session.simulation().state().tick;
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    assert!(fixture.session.animation_view().active.is_empty());
    assert_eq!(fixture.rounds()?, 31);
    fixture.steps(1)?;
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "observations"),
        Some(json!(2))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "authority"),
        Some(json!(authority.to_string()))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "delivery"),
        Some(json!((authority + 1).to_string()))
    );
    let records = fixture.session.take_animation_records();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].outcome, AnimationOutcome::Started);
    assert_eq!(records[1].outcome, AnimationOutcome::Natural);
    assert_eq!(records[0].playback.id, records[1].playback.id);
    let token = fixture
        .session
        .package_value("patchrules", fixture.owner, "token")
        .unwrap();
    assert_eq!(
        serde_json::from_str::<PlaybackId>(token.as_str().unwrap())?,
        records[0].playback.id
    );
    let binding = fixture
        .session
        .package_value("patchrules", fixture.owner, "binding")
        .unwrap();
    assert_eq!(
        serde_json::from_str::<AnimationBinding>(binding.as_str().unwrap())?,
        records[0].playback.request.binding
    );
    assert_eq!(fixture.state(), "Busy");
    Ok(())
}

#[test]
fn unread_host_evidence_backpressures_owned_admission_after_package_observation() -> Result<()> {
    let script = SCRIPT
        .replace("if get_player(p, \"busy\") { return; }", "")
        .replace(
            "\"reload\", get_player(p, \"nonce\")",
            "\"zero\", get_player(p, \"nonce\")",
        );
    let mut fixture = Fixture::configured(&script, true, true, false)?;
    for _ in 0..256 {
        fixture.command += 1;
        fixture
            .session
            .command(fixture.owner, fixture.command, Command::ToggleLight)?;
        fixture.steps(6)?;
    }
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "nonce"),
        Some(json!(256))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "observations"),
        Some(json!(512))
    );
    let before = fixture.session.animation_view();
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "nonce"),
        Some(json!(256))
    );
    assert_eq!(fixture.session.animation_view(), before);
    let records = fixture.session.take_animation_records();
    assert_eq!(records.len(), 512);
    for pair in records.chunks_exact(2) {
        assert_eq!(pair[0].outcome, AnimationOutcome::Started);
        assert_eq!(pair[1].outcome, AnimationOutcome::Natural);
        assert_eq!(pair[0].playback.id, pair[1].playback.id);
    }
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "nonce"),
        Some(json!(257))
    );
    let next = fixture.session.take_animation_records();
    assert_eq!(
        next[0].playback.id.serial,
        records.last().unwrap().playback.id.serial + 1
    );
    Ok(())
}

#[test]
fn caller_cannot_redirect_original_image_context_to_another_owner() -> Result<()> {
    let script = SCRIPT.replace("start_weapon_animation(p,", "start_weapon_animation(p+1,");
    let mut fixture = Fixture::configured(&script, true, true, false)?;
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    assert_eq!(fixture.rounds()?, 12);
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "spare"),
        Some(json!(30))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "busy"),
        Some(json!(false))
    );
    assert!(fixture.session.take_animation_records().is_empty());
    assert!(!fixture.session.package_diagnostics().is_empty());
    Ok(())
}

#[test]
fn nested_parts_and_emitted_events_are_refused_before_atomic_publication() -> Result<()> {
    for extra in [
        "call_addon(\"patchobserver\", \"empty\");",
        "emit(\"discarded\", #{ok:true});",
    ] {
        let script = SCRIPT.replace(
            "set_player(p, \"handle\", handle);",
            &format!("set_player(p, \"handle\", handle); {extra}"),
        );
        let mut fixture = Fixture::configured(&script, true, true, false)?;
        fixture.command += 1;
        fixture
            .session
            .command(fixture.owner, fixture.command, Command::ToggleLight)?;
        assert_eq!(fixture.rounds()?, 12);
        assert_eq!(fixture.state(), "Ready");
        assert_eq!(
            fixture
                .session
                .package_value("patchrules", fixture.owner, "spare"),
            Some(json!(30))
        );
        assert!(fixture.session.take_animation_records().is_empty());
        assert!(
            fixture
                .session
                .package_diagnostics()
                .iter()
                .any(|d| d.code == "animation.call_shape"),
            "{extra}"
        );
    }
    Ok(())
}

#[test]
fn owned_entity_variable_writes_cannot_join_a_weapon_action() -> Result<()> {
    let script = format!("{SCRIPT}\nfn think_probe(e) {{}}\n");
    let mut fixture = Fixture::configured(&script, true, true, false)?;
    let entity = fixture
        .session
        .spawn_package_entity("patchrules:entity/probe", Vec3::new(10., 0.05, 0.))?;
    let script = script.replace(
        "set_player(p, \"handle\", handle);",
        &format!("set_player(p, \"handle\", handle); entity_set({entity}, \"touched\", 1);"),
    );
    std::fs::write(fixture._root.path().join("rules/main.rhai"), script)?;
    fixture
        .session
        .reload_rules()
        .map_err(|errors| anyhow::anyhow!("{errors:#?}"))?;
    let before = fixture.session.package_entity_vars();
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    assert_eq!(fixture.session.package_entity_vars(), before);
    assert_eq!(fixture.rounds()?, 12);
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "spare"),
        Some(json!(30))
    );
    assert!(fixture.session.take_animation_records().is_empty());
    assert!(
        fixture
            .session
            .package_diagnostics()
            .iter()
            .any(|d| d.code == "animation.call_shape")
    );
    Ok(())
}

#[test]
fn death_observation_preserves_identity_without_current_life_authority() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    let started = fixture
        .session
        .take_animation_records()
        .pop()
        .context("started")?;
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::Suicide)?;
    fixture.steps(1)?;
    let token = fixture
        .session
        .package_value("patchrules", fixture.owner, "token")
        .unwrap();
    assert_eq!(
        serde_json::from_str::<PlaybackId>(token.as_str().unwrap())?,
        started.playback.id
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "outcome"),
        Some(json!("cancelled"))
    );
    for flag in ["custody", "selection", "current"] {
        assert_eq!(
            fixture
                .session
                .package_value("patchrules", fixture.owner, flag),
            Some(json!(false)),
            "{flag}"
        );
    }
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "busy"),
        Some(json!(true))
    );
    assert_eq!(fixture.session.take_animation_records().len(), 1);
    Ok(())
}

#[test]
fn duplicate_package_nonce_refuses_exchange_and_speculative_state() -> Result<()> {
    let script = SCRIPT
        .replace("    if get_player(p, \"busy\") { return; }", "")
        .replace("get_player(p, \"nonce\"), old", "1, old")
        .replace("\"reload\", 1, old", "\"zero\", 1, old");
    let mut fixture = Fixture::configured(&script, true, true, false)?;
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    fixture.steps(6)?;
    let before = fixture.session.take_animation_records();
    assert_eq!(before.len(), 2);
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "nonce"),
        Some(json!(1))
    );
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    assert_eq!(fixture.rounds()?, 31);
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "nonce"),
        Some(json!(1))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "spare"),
        Some(json!(11))
    );
    assert!(fixture.session.take_animation_records().is_empty());
    assert!(
        fixture
            .session
            .package_diagnostics()
            .iter()
            .any(|d| d.code == "animation.refused")
    );
    Ok(())
}

const ITEM: &str = "patch:weapon/rifle";
const IMAGE: &str = "patch:image/rifle";
const MODEL: &str = "patch:body/rifle";
const SCRIPT: &str = r#"
fn cmd_seed7(p) { set_rounds(p, "patch:weapon/rifle", 7); }
fn cmd_seed12(p) { set_rounds(p, "patch:weapon/rifle", 12); }
fn cmd_probe(p) { set_player(p, "observed", player(p).magazine.rounds); }
fn cmd_arm(p) { set_player(p, "armed", true); }
fn cmd_activate(p) { set_image_state(p, "Activate"); }
fn cmd_replace(p) { if get_player(p, "armed") { mount_image(p, "patch:image/rifle"); } }
fn cmd_reload(p) {
    if !get_player(p, "armed") { return; }
    if get_player(p, "busy") { return; }
    let old = player(p).magazine.rounds;
    set_player(p, "nonce", get_player(p, "nonce") + 1);
    set_player(p, "spare", old - 1);
    set_player(p, "busy", true);
    let handle = start_weapon_animation(p, "patch:body/rifle", "reload", get_player(p, "nonce"), old, 31, "Busy");
    set_player(p, "handle", handle);
}

fn on_weapon_animation(p, info) {
    set_player(p, "observations", get_player(p, "observations") + 1);
    set_player(p, "token", info.token);
    set_player(p, "binding", info.binding);
    set_player(p, "outcome", info.outcome);
    set_player(p, "custody", info.custody_valid);
    set_player(p, "selection", info.current_selection_applicable);
    set_player(p, "current", info.current_action_applicable);
    set_player(p, "authority", info.authority_tick);
    set_player(p, "delivery", info.delivery_tick);
}
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
        Self::configured(SCRIPT, true, true, false)
    }
    fn configured(script: &str, observer: bool, dependency: bool, queued: bool) -> Result<Self> {
        let root = tempfile::tempdir()?;
        let rules = root.path().join("rules");
        std::fs::create_dir(&rules)?;
        std::fs::write(rules.join("package.json"), json!({
            "schema_version":1,"id":"patchrules","version":"1.0.0","api":1,
            "name":"Invented patch setup","license":"CC0-1.0","capabilities":["player"],"dependencies":if dependency {json!({"patch":"^1.0.0","patchobserver":"^1.0.0"})} else {json!({})},
            "provides":[{"kind":"behaviour","id":"patchrules:behaviour/main","file":"behaviour.json"},
                {"kind":"script","id":"patchrules:script/main","file":"main.rhai"}]
        }).to_string())?;
        std::fs::write(
            rules.join("behaviour.json"),
            json!({
                "schema_version":1,"script":"main.rhai",
"commands":[{"name":"seed7"},{"name":"seed12"},{"name":"probe"},{"name":"reload"},{"name":"arm"},{"name":"activate"},{"name":"replace"}],"on_weapon_animation":observer,"events":["discarded"],
                "state":{"player":{
"observed":{"default":0,"visible":"server","persist":false},
"nonce":{"default":0,"visible":"server","persist":false},
"spare":{"default":30,"visible":"server","persist":false},
"armed":{"default":false,"visible":"server","persist":false},
"busy":{"default":false,"visible":"server","persist":false},
"handle":{"default":-1,"visible":"server","persist":false},
"observations":{"default":0,"visible":"server","persist":false},
"token":{"default":"","visible":"server","persist":false},
"binding":{"default":"","visible":"server","persist":false},
"outcome":{"default":"","visible":"server","persist":false},
"custody":{"default":false,"visible":"server","persist":false},
"selection":{"default":false,"visible":"server","persist":false},
"current":{"default":false,"visible":"server","persist":false},
"authority":{"default":"","visible":"server","persist":false},
"delivery":{"default":"","visible":"server","persist":false}}}
            })
            .to_string(),
        )?;
        std::fs::write(rules.join("main.rhai"), script)?;
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
        let content = root.path().join("content");
        std::fs::create_dir(&content)?;
        std::fs::write(content.join("rifle.json"), &bytes)?;
        std::fs::write(
            content.join("package.json"),
            json!({
                "schema_version":1,"id":"patch","version":"1.0.0","api":1,
                "name":"Native rifle fixture","license":"CC0-1.0",
                "provides":[{"kind":"body","id":MODEL,"file":"rifle.json"}]
            })
            .to_string(),
        )?;
        let observer_dir = root.path().join("observer");
        std::fs::create_dir(&observer_dir)?;
        std::fs::write(observer_dir.join("package.json"),json!({"schema_version":1,"id":"patchobserver","version":"1.0.0","api":1,"name":"Observer isolation fixture","license":"CC0-1.0","provides":[{"kind":"behaviour","id":"patchobserver:behaviour/main","file":"behaviour.json"},{"kind":"script","id":"patchobserver:script/main","file":"main.rhai"}]}).to_string())?;
        std::fs::write(observer_dir.join("behaviour.json"),json!({
            "schema_version":1,"script":"main.rhai","exports":[{"name":"empty","args":0}],
            "on_weapon_animation":true,"state":{"player":{"seen":{"default":0,"visible":"server","persist":false}}}
        }).to_string())?;
        std::fs::write(
            observer_dir.join("main.rhai"),
            "fn api_empty(from) { return 0; } fn on_weapon_animation(p, info) { set_player(p, \"seen\", get_player(p, \"seen\")+1); }",
        )?;
        let entity_fixture = script.contains("think_probe");
        if entity_fixture {
            let mut manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(rules.join("package.json"))?)?;
            manifest["provides"].as_array_mut().unwrap().extend([
                json!({"kind":"entity","id":"patchrules:entity/probe","file":"entity.json"}),
            ]);
            std::fs::write(rules.join("package.json"), serde_json::to_vec(&manifest)?)?;
            std::fs::write(
                rules.join("entity.json"),
                json!({"schema_version":1,"name":"Probe","model":"patch:model/probe",
                "think":"think_probe","think_interval":120,"speed":0,"health":1,"max_alive":1})
                .to_string(),
            )?;
            let mut manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(content.join("package.json"))?)?;
            manifest["provides"]
                .as_array_mut()
                .unwrap()
                .push(json!({"kind":"model","id":"patch:model/probe","file":"model.json"}));
            std::fs::write(content.join("package.json"), serde_json::to_vec(&manifest)?)?;
            std::fs::write(content.join("model.json"),json!({"schema_version":1,"boxes":[{"center":[0,0.5,0],"size":[0.2,1,0.2],"color":[1,1,1,1]}]}).to_string())?;
        }
        let catalog = Catalog::load(
            root.path(),
            &PackageSet {
                schema_version: 1,
                packages: vec![
                    PackageEntry {
                        id: "patchobserver".into(),
                        version: "1.0.0".into(),
                        side: Side::Server,
                        dir: "observer".into(),
                        role: None,
                    },
                    PackageEntry {
                        id: "patch".into(),
                        version: "1.0.0".into(),
                        side: Side::Shared,
                        dir: "content".into(),
                        role: None,
                    },
                    PackageEntry {
                        id: "patchrules".into(),
                        version: "1.0.0".into(),
                        side: Side::Server,
                        dir: "rules".into(),
                        role: None,
                    },
                ],
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
            "images":{IMAGE:{"model":MODEL,"commands":{"light":"patchrules:reload", "states":if queued {json!({"onreplace":"patchrules:replace","onqueued":"patchrules:reload"})} else {json!({})}},
                "magazine":{"size":31,"ammo":"rifle","reload_ticks":1,"reserve":0},
                "states":[{"name":"Activate","sequence":"probe","ticks":if queued {0} else {1},"timeout":1,"script":if queued {"onReplace"} else {""}},
                    {"name":"Ready","script":if queued {"onQueued"} else {""}},{"name":"Busy"}]}},"projectiles":{}
        }))?)?)?;
        session.set_animation_assets(AnimationAssets {
            assets: vec![AnimationAsset {
                resource: MODEL.into(),
                digest: bri_package::environment::hash_bytes(&bytes),
                rig: false,
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
        fixture.cmd("arm")?;
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
}

#[test]
fn ordinary_package_reload_admits_once_and_host_drain_cannot_steal_observation() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let request = fixture.request(1);
    fixture.command += 1;
    fixture
        .session
        .command(fixture.owner, fixture.command, Command::ToggleLight)?;
    assert_eq!(fixture.rounds()?, 31);
    assert_eq!(fixture.state(), "Busy");
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "spare"),
        Some(json!(11))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "busy"),
        Some(json!(true))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "handle"),
        Some(json!(0))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "token"),
        Some(json!(""))
    );
    let records = fixture.session.take_animation_records();
    assert_eq!(records.len(), 1);
    let id = records[0].playback.id.clone();
    assert_eq!(records[0].outcome, AnimationOutcome::Started);
    assert_eq!(records[0].playback.request.binding, request.binding);
    fixture.steps(1)?;
    let notices = fixture.session.take_private_notices();
    let slots = notices
        .iter()
        .find_map(|(owner, notice)| match notice {
            Notice::InventoryAmmo(slots) if *owner == fixture.owner => Some(slots),
            _ => None,
        })
        .context("Missing ordinary native inventory notice")?;
    assert_eq!(slots[fixture.first].as_ref().unwrap().rounds, Some(31));
    assert_eq!(slots[fixture.second].as_ref().unwrap().rounds, Some(7));
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "observations"),
        Some(json!(1))
    );
    let token = fixture
        .session
        .package_value("patchrules", fixture.owner, "token")
        .unwrap();
    assert_eq!(
        serde_json::from_str::<PlaybackId>(token.as_str().unwrap())?,
        id
    );
    assert!(fixture.session.take_animation_records().is_empty());
    fixture.steps(30)?;
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "observations"),
        Some(json!(2))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "outcome"),
        Some(json!("native_natural"))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.owner, "busy"),
        Some(json!(true))
    );
    assert_eq!(fixture.rounds()?, 31);
    assert_eq!(fixture.state(), "Busy");
    let records = fixture.session.take_animation_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].playback.id, id);
    assert_eq!(records[0].outcome, AnimationOutcome::Natural);
    assert_eq!(
        fixture
            .session
            .package_value("patchrules", fixture.other, "observations"),
        Some(json!(0))
    );
    assert_eq!(
        fixture
            .session
            .package_value("patchobserver", fixture.owner, "seen"),
        Some(json!(0))
    );
    assert!(fixture.session.take_package_problems().is_empty());
    Ok(())
}
