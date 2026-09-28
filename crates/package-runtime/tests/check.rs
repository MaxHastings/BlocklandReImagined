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
    assert_eq!(
        ids,
        ["stresslab-hud", "stresslab-economy", "stresslab-world"]
    );
    assert_eq!(report.add_ons[0].side, Side::Client);
    assert_eq!(report.add_ons[1].side, Side::Server);
    assert_eq!(
        report.add_ons[1].capabilities[1].meaning,
        "send chat messages"
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
