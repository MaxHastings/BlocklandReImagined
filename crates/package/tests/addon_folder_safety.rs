//! The classic Add-Ons folder sync only ever owns what the player's own
//! conversions made: a dropped copy of an Add-On the game ships (a bundled
//! original at `addons/<id>`) is never adopted, replaced or removed with it.
use bri_package::classic::{self, Record, State, Step};
use bri_package::library::{DROP_DIR, Library};
use serde_json::json;
use std::path::{Path, PathBuf};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bri-folder-safety-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(DROP_DIR)).unwrap();
    dir
}

fn package(root: &Path, dir: &str, info: serde_json::Value) {
    std::fs::create_dir_all(root.join(dir)).unwrap();
    std::fs::write(root.join(dir).join("package.json"), info.to_string()).unwrap();
}

/// The bundled Shark Bot, on as a release lists it, with its port's host
/// rules beside it when `rules`. `tools/addon_bundle.py` marks a bundled
/// original's manifest `provenance.bundled`; its `source` is the importer's,
/// as for any conversion.
fn bundled_shark(root: &Path, rules: bool) {
    let companions: &[&str] = if rules { &["bot_shark-rules"] } else { &[] };
    package(
        root,
        "addons/bot_shark",
        json!({ "schema_version": 1, "id": "bot_shark", "version": "1.0.0", "api": 1,
            "name": "Shark Bot", "companions": companions,
            "provides": [{ "kind": "bots", "id": "bot_shark:bots/shark", "file": "b.json" }],
            "provenance": {
                "source": "Blockland Add-On Bot_Shark (zip), sha256 aa",
                "bundled": "The authors' original Add-On, bundled with Blockland ReImagined with credit to them."
            } }),
    );
    let mut list = vec![
        json!({ "id": "bot_shark", "version": "1.0.0", "side": "shared", "dir": "addons/bot_shark" }),
    ];
    if rules {
        package(
            root,
            "addons/bot_shark-rules",
            json!({ "schema_version": 1, "id": "bot_shark-rules", "version": "1.0.0", "api": 1,
                "dependencies": { "bot_shark": "^1.0" },
                "provides": [{ "kind": "behaviour", "id": "bot_shark-rules:behaviour/b", "file": "b.json" }],
                "provenance": { "source": "Port bot_shark of Blockland Add-On Bot_Shark" } }),
        );
        list.push(
            json!({ "id": "bot_shark-rules", "version": "1.0.0", "side": "server",
            "dir": "addons/bot_shark-rules" }),
        );
    }
    std::fs::write(
        root.join("packages.json"),
        json!({ "schema_version": 1, "packages": list }).to_string(),
    )
    .unwrap();
}

/// What the game's sync (`bri-client`'s `add_ons::sync`) does with each
/// step, short of running the importer: Imports are returned instead.
fn run(root: &Path, steps: Vec<Step>) -> Vec<String> {
    let mut state = State::load(root);
    let mut imports = vec![];
    for step in steps {
        match step {
            Step::Adopt { name, id, stamp } => {
                let dir = Library::scan(root)
                    .ok()
                    .and_then(|l| Some(l.get(&id)?.package.dir.clone()));
                state.set(Record {
                    name,
                    stamp,
                    id: Some(id),
                    dir,
                    error: None,
                    included: None,
                    importer: Some("test importer".into()),
                });
            }
            Step::Included { name, id, stamp } => {
                imports.push(format!("{name} is already included as {id}"));
                state.set(Record {
                    name,
                    stamp,
                    id: None,
                    dir: None,
                    error: None,
                    included: Some(id),
                    importer: Some("test importer".into()),
                });
            }
            Step::Remove { name, id, .. } => {
                if !id.is_empty()
                    && let Err(error) = Library::scan(root).and_then(|mut l| l.uninstall(&id))
                {
                    // As the game does: warn, keep the record, try again.
                    eprintln!("Removing {name}'s conversion: {error:#}");
                    continue;
                }
                state.forget(&name);
            }
            Step::Import { name, replaces, .. } => {
                imports.push(format!("{name} replacing {replaces:?}"));
            }
        }
        state.save(root).unwrap();
    }
    imports
}

fn sync(root: &Path) -> Vec<String> {
    let steps = classic::plan(
        &Library::scan(root).unwrap(),
        &State::load(root),
        "test importer",
    );
    run(root, steps)
}

fn still_shipped(root: &Path, rules: bool) {
    assert!(root.join("addons/bot_shark/package.json").is_file());
    let library = Library::scan(root).unwrap();
    assert!(library.get("bot_shark").is_some_and(|e| e.enabled));
    if rules {
        assert!(root.join("addons/bot_shark-rules/package.json").is_file());
        assert!(library.get("bot_shark-rules").is_some_and(|e| e.enabled));
    }
}

#[test]
fn removing_a_dropped_copy_of_a_bundled_add_on_keeps_the_bundled_one() {
    for rules in [false, true] {
        let root = temp(&format!("remove-{rules}"));
        bundled_shark(&root, rules);
        let zip = classic::folder(&root).join("Bot_Shark.zip");
        std::fs::write(&zip, b"PK the player's copy").unwrap();
        assert_eq!(
            sync(&root),
            ["Bot_Shark is already included as bot_shark"],
            "a copy of a bundled Add-On is never converted"
        );
        let library = Library::scan(&root).unwrap();
        let legacy = &library.legacy[0];
        assert_eq!(legacy.imported_as, None);
        assert_eq!(legacy.included.as_deref(), Some("bot_shark"));
        // Said once.
        assert!(sync(&root).is_empty());
        std::fs::remove_file(&zip).unwrap();
        sync(&root);
        still_shipped(&root, rules);
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[test]
fn a_changed_copy_of_a_bundled_add_on_never_replaces_it() {
    let root = temp("change");
    bundled_shark(&root, true);
    let zip = classic::folder(&root).join("Bot_Shark.zip");
    std::fs::write(&zip, b"PK one").unwrap();
    sync(&root);
    std::fs::write(&zip, b"PK two, a longer copy").unwrap();
    // A changed copy is still the included one: nothing to convert or say.
    assert_eq!(sync(&root), Vec::<String>::new());
    still_shipped(&root, true);
    let _ = std::fs::remove_dir_all(&root);
}

/// A record an earlier build wrote, adopting the bundled package as the
/// dropped zip's conversion, is dropped without touching the package.
#[test]
fn an_earlier_builds_record_of_a_bundled_add_on_never_removes_it() {
    for rules in [false, true] {
        let root = temp(&format!("record-{rules}"));
        bundled_shark(&root, rules);
        let zip = classic::folder(&root).join("Bot_Shark.zip");
        std::fs::write(&zip, b"PK").unwrap();
        let mut state = State::load(&root);
        state.set(Record {
            name: "Bot_Shark".into(),
            stamp: classic::stamp(&zip).unwrap(),
            id: Some("bot_shark".into()),
            dir: Some("addons/bot_shark".into()),
            error: None,
            included: None,
            importer: Some("test importer".into()),
        });
        state.save(&root).unwrap();
        std::fs::remove_file(&zip).unwrap();
        sync(&root);
        still_shipped(&root, rules);
        assert!(State::load(&root).get("Bot_Shark").is_none());
        // Nor does removing it by hand.
        let mut library = Library::scan(&root).unwrap();
        assert!(library.uninstall("bot_shark").is_err());
        if rules {
            assert!(library.uninstall("bot_shark-rules").is_err());
        }
        still_shipped(&root, rules);
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// The player's own conversion with its port's host rules, both on, goes
/// with its zip: the rules are part of it, so they go off with it rather
/// than refusing to turn off by themselves.
#[test]
fn removing_a_conversion_with_host_rules_that_are_on_removes_both() {
    let root = temp("own-rules");
    package(
        &root,
        "addons/weapon_gun",
        json!({ "schema_version": 1, "id": "weapon_gun", "version": "1.0.0", "api": 1,
            "companions": ["weapon_gun-rules"],
            "provides": [{ "kind": "weapons", "id": "weapon_gun:weapons/w", "file": "w.json" }],
            "provenance": { "source": "Blockland Add-On Weapon_Gun (zip), sha256 00" } }),
    );
    package(
        &root,
        "addons/weapon_gun-rules",
        json!({ "schema_version": 1, "id": "weapon_gun-rules", "version": "1.0.0", "api": 1,
            "dependencies": { "weapon_gun": "^1.0" },
            "provides": [{ "kind": "behaviour", "id": "weapon_gun-rules:behaviour/b", "file": "b.json" }] }),
    );
    let mut library = Library::scan(&root).unwrap();
    let plan = library.plan("weapon_gun", true);
    library.apply(&plan).unwrap();
    assert!(library.get("weapon_gun-rules").is_some_and(|e| e.enabled));
    let zip = classic::folder(&root).join("Weapon_Gun.zip");
    std::fs::write(&zip, b"PK").unwrap();
    sync(&root);
    assert!(State::load(&root).get("Weapon_Gun").is_some());
    std::fs::remove_file(&zip).unwrap();
    sync(&root);
    assert!(!root.join("addons/weapon_gun").exists());
    assert!(!root.join("addons/weapon_gun-rules").exists());
    let library = Library::scan(&root).unwrap();
    assert!(library.get("weapon_gun").is_none() && library.get("weapon_gun-rules").is_none());
    assert!(State::load(&root).get("Weapon_Gun").is_none());
    let _ = std::fs::remove_dir_all(&root);
}

/// The player's conversion of Bot_Hole (with host rules) and Bot_Zombie,
/// which needs it, both on.
fn hole_and_zombie(root: &Path) {
    hole(root, "addons/bot_hole", "1.0.0");
    package(
        root,
        "addons/bot_zombie",
        json!({ "schema_version": 1, "id": "bot_zombie", "version": "1.0.0", "api": 1,
            "dependencies": { "bot_hole": "^1.0" },
            "provides": [{ "kind": "bots", "id": "bot_zombie:bots/z", "file": "z.json" }],
            "provenance": { "source": "Blockland Add-On Bot_Zombie (zip), sha256 22" } }),
    );
    let mut library = Library::scan(root).unwrap();
    let plan = library.plan("bot_zombie", true);
    library.apply(&plan).unwrap();
    for id in ["bot_hole", "bot_hole-rules", "bot_zombie"] {
        assert!(library.get(id).is_some_and(|e| e.enabled), "{id} is on");
    }
}

/// Bot_Hole as the importer writes it into `dir` (its host rules beside).
fn hole(root: &Path, dir: &str, version: &str) {
    package(
        root,
        dir,
        json!({ "schema_version": 1, "id": "bot_hole", "version": version, "api": 1,
            "companions": ["bot_hole-rules"],
            "provides": [{ "kind": "bots", "id": "bot_hole:bots/h", "file": "h.json" }],
            "provenance": { "source": format!("Blockland Add-On Bot_Hole (zip), sha256 {version}") } }),
    );
    package(
        root,
        &format!("{dir}-rules"),
        json!({ "schema_version": 1, "id": "bot_hole-rules", "version": version, "api": 1,
            "dependencies": { "bot_hole": "^1.0" },
            "provides": [{ "kind": "behaviour", "id": "bot_hole-rules:behaviour/b", "file": "b.json" }] }),
    );
}

#[test]
fn a_new_copy_of_an_add_on_others_need_keeps_them_on() {
    let root = temp("replace");
    hole_and_zombie(&root);
    let mut library = Library::scan(&root).unwrap();
    let staged = library.staging_dir("Bot_Hole").unwrap();
    hole(&root, &staged, "1.1.0");
    let (id, dir) = library
        .install_staged("Bot_Hole", &staged, Some("bot_hole"))
        .unwrap();
    assert_eq!((id.as_str(), dir.as_str()), ("bot_hole", "addons/bot_hole"));
    for id in ["bot_hole", "bot_hole-rules", "bot_zombie"] {
        let entry = library.get(id).unwrap();
        assert!(entry.enabled, "{id} is still on");
        assert!(!entry.has_errors(), "{id}: {:?}", entry.problems);
    }
    // The lists name the new copy, as the loaders check.
    let listed = bri_package::packages::PackageSet::load_root(&root).unwrap();
    let hole = listed.packages.iter().find(|p| p.id == "bot_hole").unwrap();
    assert_eq!(hole.version, "1.1.0");
    assert!(!root.join(&staged).exists() && !root.join(format!("{staged}-rules")).exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_new_copy_that_cannot_go_in_leaves_the_old_one() {
    let root = temp("replace-fails");
    hole_and_zombie(&root);
    let before = std::fs::read(root.join("addons/bot_hole/package.json")).unwrap();
    let mut library = Library::scan(&root).unwrap();
    // The importer wrote nothing usable.
    let staged = library.staging_dir("Bot_Hole").unwrap();
    std::fs::create_dir_all(root.join(&staged)).unwrap();
    std::fs::write(root.join(&staged).join("package.json"), b"{ half").unwrap();
    assert!(
        library
            .install_staged("Bot_Hole", &staged, Some("bot_hole"))
            .is_err()
    );
    assert_eq!(
        std::fs::read(root.join("addons/bot_hole/package.json")).unwrap(),
        before
    );
    assert!(root.join("addons/bot_hole-rules/package.json").is_file());
    for id in ["bot_hole", "bot_hole-rules", "bot_zombie"] {
        assert!(
            library.get(id).is_some_and(|e| e.enabled),
            "{id} is still on"
        );
    }
    assert!(!root.join(&staged).exists());
    let _ = std::fs::remove_dir_all(&root);
}
