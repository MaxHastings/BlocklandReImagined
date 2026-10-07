//! `bri-addon-check`'s engine: an Add-On folder is checked the way the game
//! loads it, with the Add-Ons it needs found beside it.
use bri_package::packages::Side;
use bri_package_runtime::check::{check, side_for};
use serde_json::json;
use std::path::{Path, PathBuf};

fn stresslab(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/stresslab")
        .join(name)
}
fn codes(report: &bri_package_runtime::check::Report) -> Vec<&str> {
    report.diagnostics.iter().map(|d| d.code.as_str()).collect()
}
fn write(dir: &Path, file: &str, value: serde_json::Value) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(file), value.to_string()).unwrap();
}
fn manifest(id: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut m = json!({
        "schema_version": 1, "id": id, "version": "1.0.0", "api": 1,
        "name": "Test", "license": "CC0-1.0", "provenance": { "source": "original" },
        "provides": [],
    });
    m.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    m
}

#[test]
fn a_hud_add_on_checks_with_the_rules_it_needs() {
    let report = check(&stresslab("stresslab-hud"));
    assert!(report.ok, "{report}");
    let ids: Vec<_> = report.add_ons.iter().map(|a| a.id.as_str()).collect();
    // The HUD needs the economy and the creeper; each needs its own parts.
    assert_eq!(ids[0], "stresslab-hud");
    let mut needed = ids[1..].to_vec();
    needed.sort();
    assert_eq!(
        needed,
        [
            "stresslab-creeper",
            "stresslab-creeper-model",
            "stresslab-economy",
            "stresslab-world"
        ]
    );
    assert_eq!(report.add_ons[0].side, Side::Client);
    let economy = report
        .add_ons
        .iter()
        .find(|a| a.id == "stresslab-economy")
        .unwrap();
    assert_eq!(economy.side, Side::Server);
    assert!(
        economy
            .capabilities
            .iter()
            .any(|c| c.meaning == "send chat messages and put text on players' screens")
    );
    assert!(
        report
            .to_string()
            .ends_with("OK: the game can load this Add-On.")
    );
}

#[test]
fn problems_are_named_with_where_and_how_to_fix() {
    let root = tempfile::tempdir().unwrap();
    // A misspelt field is an error, not silently ignored.
    write(
        &root.path().join("typo"),
        "package.json",
        manifest("typo", json!({ "capabilites": ["chat"] })),
    );
    let report = check(&root.path().join("typo"));
    assert!(!report.ok);
    assert_eq!(codes(&report), ["manifest.json"]);
    assert!(report.diagnostics[0].message.contains("capabilites"));

    // A dependency that is not beside it.
    write(
        &root.path().join("lonely"),
        "package.json",
        manifest("lonely", json!({ "dependencies": { "elsewhere": "^1.0" } })),
    );
    let report = check(&root.path().join("lonely"));
    assert_eq!(codes(&report), ["check.dependency.not_found"]);

    // A HUD bound to a key its rules keep private.
    let rules = root.path().join("rules");
    write(
        &rules,
        "package.json",
        manifest(
            "rules",
            json!({ "provides": [
                { "kind": "behaviour", "id": "rules:behaviour/main", "file": "behaviour.json" },
                { "kind": "script", "id": "rules:script/main", "file": "main.rhai" }
            ] }),
        ),
    );
    write(
        &rules,
        "behaviour.json",
        json!({ "schema_version": 1, "script": "main.rhai",
                "state": { "player": { "secret": { "default": 0 } } } }),
    );
    std::fs::write(rules.join("main.rhai"), "fn helper() { 1 }").unwrap();
    let hud = root.path().join("hud");
    write(
        &hud,
        "package.json",
        manifest(
            "hud",
            json!({ "dependencies": { "rules": "^1.0.0" },
                    "provides": [{ "kind": "hud", "id": "hud:hud/panel", "file": "panel.json" }] }),
        ),
    );
    write(
        &hud,
        "panel.json",
        json!({ "schema_version": 1, "slot": "hud.overlay", "anchor": "top_left",
                "title": "T", "background": [0,0,0,1], "accent": [1,1,1,1], "text": [1,1,1,1],
                "rows": [{ "label": "Secret", "bind": "rules:player/secret" }] }),
    );
    let report = check(&hud);
    assert_eq!(codes(&report), ["set.hud.binding"], "{report}");

    // A script that does not compile names its file and line.
    std::fs::write(rules.join("main.rhai"), "fn helper( { 1 }").unwrap();
    let report = check(&rules);
    assert_eq!(codes(&report), ["script.syntax"], "{report}");
    assert!(
        report.diagnostics[0]
            .location
            .as_deref()
            .unwrap()
            .starts_with("rules/main.rhai:")
    );
}

#[test]
fn sides_follow_what_an_add_on_provides() {
    assert_eq!(side_for(["behaviour", "script"]), Some(Side::Server));
    assert_eq!(side_for(["entity"]), Some(Side::Server));
    assert_eq!(side_for(["hud", "model"]), Some(Side::Client));
    assert_eq!(side_for([]), Some(Side::Shared));
    assert_eq!(side_for(["entity", "model"]), None);
}

#[test]
fn weapons_and_bricks_are_shared_and_the_screen_agrees() {
    assert_eq!(side_for(["weapons"]), Some(Side::Shared));
    assert_eq!(side_for(["bricks"]), Some(Side::Shared));
    assert_eq!(side_for(["archetype"]), Some(Side::Server));
    // One rule: a kind the host alone loads is a server kind on the Add-Ons
    // screen too, and the screen's client kinds are ones clients load.
    use bri_package::library::{CLIENT_KINDS, SERVER_KINDS};
    use bri_package_runtime::content::Kind;
    for name in Kind::NAMES {
        let side = Kind::parse(name).unwrap().side();
        assert_eq!(side == Side::Server, SERVER_KINDS.contains(&name), "{name}");
        if CLIENT_KINDS.contains(&name) {
            assert_eq!(side, Side::Client, "{name}");
        }
    }
}

#[test]
fn the_sample_weapon_checks_and_a_broken_one_is_named() {
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/samples/sample-bubble-blaster");
    let report = check(&sample);
    assert!(report.ok, "{report}");
    assert_eq!(report.add_ons[0].side, Side::Shared);
    // It borrows the stock pistol's art: a warning, never a failure.
    assert_eq!(codes(&report), ["check.weapons.presentation"], "{report}");

    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("broken");
    write(&broken, "package.json", manifest("broken", json!({})));
    let mut weapons: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sample.join("assets/weapons.json")).unwrap())
            .unwrap();
    // An item whose image does not exist.
    weapons["items"]["sample-bubble-blaster:weapon/bubble_blaster"]["image"] = json!("nope");
    write(&broken.join("assets"), "weapons.json", weapons);
    let report = check(&broken);
    assert!(!report.ok);
    assert_eq!(codes(&report), ["check.weapons"], "{report}");
}

#[test]
fn stale_or_missing_item_presentation_warns_without_failing() {
    let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/samples/sample-bubble-blaster/assets/weapons.json");
    let weapons = std::fs::read(sample).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let add_on = dir.path().join("stale");
    write(&add_on, "package.json", manifest("stale", json!({})));
    std::fs::create_dir_all(add_on.join("assets")).unwrap();
    std::fs::write(add_on.join("assets/weapons.json"), &weapons).unwrap();
    write(
        &add_on.join("assets"),
        "presentation.json",
        json!({ "schema_version": 2, "weapons_sha256": "0".repeat(64) }),
    );
    let report = check(&add_on);
    assert!(report.ok, "{report}");
    assert_eq!(codes(&report), ["check.weapons.presentation"], "{report}");
    assert!(
        report.diagnostics[0]
            .message
            .contains("different weapons.json"),
        "{report}"
    );
}

#[test]
fn an_items_own_model_checks_and_a_missing_texture_is_named() {
    // A tool Add-On whose item is a model of its own, its materials each
    // naming a PNG beside it.
    let dir = tempfile::tempdir().unwrap();
    let tool = dir.path().join("tool");
    write(&tool, "package.json", manifest("tool", json!({})));
    let assets = tool.join("assets");
    write(
        &assets,
        "weapons.json",
        json!({ "schema_version": 4, "id": "tool",
            "items": { "tool:weapon/pick": { "ui_name": "Pick", "image": "tool:image/pick",
                "model": "models/pick.shape.json", "icon": "", "can_drop": false } },
            "images": { "tool:image/pick": { "name": "PickImage", "model": "models/pick.shape.json",
                "states": [{ "name": "Ready" }] } } }),
    );
    write(
        &assets.join("models"),
        "pick.shape.json",
        json!({ "schema_version": 1, "materials": [{ "name": "wood" }, { "name": "iron" }] }),
    );
    for png in ["wood.png", "iron.png"] {
        std::fs::write(assets.join("models").join(png), b"png").unwrap();
    }
    let report = check(&tool);
    assert!(report.ok, "{report}");
    // Its own model needs no presentation.json: nothing to warn about.
    assert!(codes(&report).is_empty(), "{report}");

    std::fs::remove_file(assets.join("models/iron.png")).unwrap();
    let report = check(&tool);
    assert!(report.ok, "a look problem never stops the load: {report}");
    assert_eq!(codes(&report), ["check.weapons.model"], "{report}");
    assert!(
        report.diagnostics[0].message.contains("iron.png"),
        "{report}"
    );
}
