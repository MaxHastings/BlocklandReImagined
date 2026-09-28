//! The check step of the agent workflow: a copy of the Stress Lab with five
//! planted mistakes reports each one once, with its code and file, instead
//! of stopping at the first or cascading into its dependents.
use bri_package_runtime::{Catalog, script::Runtime};
use std::{fs, path::Path};

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}
fn edit(path: &Path, change: impl FnOnce(&mut serde_json::Value)) {
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    change(&mut value);
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

#[test]
fn check_reports_every_planted_mistake_once() {
    let dir = tempfile::tempdir().unwrap();
    copy(&bri_stresslab::packages_root(), dir.path());
    let root = dir.path();
    edit(&root.join("stresslab-world/package.json"), |v| {
        v["provides"][2]["id"] = "v20:world/strata".into()
    });
    edit(&root.join("stresslab-economy/package.json"), |v| {
        v["capabilities"] = serde_json::json!(["chat", "teleport"])
    });
    let script = root.join("stresslab-economy/economy.rhai");
    let source = fs::read_to_string(&script)
        .unwrap()
        .replace("fn cmd_sell_all(player) {", "fn cmd_sell_all(player {");
    fs::write(&script, source).unwrap();
    edit(&root.join("stresslab-creeper/creeper.json"), |v| {
        v["model"] = "stresslab-creeper-model:model/creepr".into()
    });
    edit(&root.join("stresslab-hud/miner.json"), |v| {
        v["keys"][2]["command"] = "summon".into()
    });

    let (catalog, mut problems) = Catalog::inspect(root, &bri_stresslab::fixture_set(), true);
    if let Err(more) = Runtime::compile(&catalog) {
        problems.extend(more);
    }
    let found: Vec<(&str, &str)> = problems
        .iter()
        .map(|d| (d.code.as_str(), d.location.as_deref().unwrap_or("")))
        .collect();
    for (code, file) in [
        ("manifest.provide.namespace", "stresslab-world/package.json"),
        ("manifest.capability", "stresslab-economy/package.json"),
        ("script.syntax", "stresslab-economy/economy.rhai"),
        ("set.model.unknown", "stresslab-creeper/package.json"),
        ("set.hud.key", "stresslab-hud/package.json"),
    ] {
        assert_eq!(
            found
                .iter()
                .filter(|(c, l)| *c == code && l.starts_with(file))
                .count(),
            1,
            "{code} at {file}: {found:#?}"
        );
    }
    // Dependents of a broken package say so once, not once per reference.
    assert_eq!(
        found
            .iter()
            .filter(|(c, _)| *c == "set.hud.binding")
            .count(),
        0,
        "{found:#?}"
    );
    assert!(
        found
            .iter()
            .any(|(c, l)| *c == "set.dependency.broken" && l.starts_with("stresslab-hud"))
    );
    assert!(
        Catalog::load(root, &bri_stresslab::fixture_set(), true).is_err(),
        "a catalog with problems never runs"
    );
}
