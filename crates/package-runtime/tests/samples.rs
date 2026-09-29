//! The sample Add-Ons in packages/samples load on the server and on the
//! client, compile, and their HUD shows only state the rule makes public.
use bri_package::packages::{PackageEntry, PackageSet, Side};
use bri_package_runtime::{Catalog, content::Binding, script::Runtime};
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples")
}
fn set() -> PackageSet {
    PackageSet {
        schema_version: 1,
        packages: [
            ("sample-survival-points", Side::Server),
            ("sample-points-hud", Side::Client),
            ("sample-bubble-blaster", Side::Shared),
        ]
        .into_iter()
        .map(|(id, side)| PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side,
            dir: id.into(),
            role: None,
        })
        .collect(),
    }
}

#[test]
fn samples_load_on_server_and_client() {
    let server = Catalog::load(&root(), &set(), true).unwrap_or_else(|e| panic!("{e:#?}"));
    Runtime::compile(&server).unwrap_or_else(|e| panic!("{e:#?}"));
    assert!(
        server.packages["sample-survival-points"]
            .behaviour
            .is_some()
    );
    // A client never receives the rule's script, only the HUD and weapon.
    let client = Catalog::load(&root(), &set(), false).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(
        client.packages.keys().collect::<Vec<_>>(),
        ["sample-bubble-blaster", "sample-points-hud"]
    );
    assert_eq!(client.huds().count(), 1);
}

#[test]
fn sample_hud_binds_only_public_keys_and_real_commands() {
    let server = Catalog::load(&root(), &set(), true).unwrap();
    let client = Catalog::load(&root(), &set(), false).unwrap();
    let (_, panel) = client.huds().next().unwrap();
    let rule = |package: &str| {
        server.packages[package]
            .behaviour
            .as_ref()
            .unwrap_or_else(|| panic!("{package} has no behaviour"))
    };
    for row in &panel.rows {
        let b = Binding::parse(&row.bind).unwrap();
        let keys = &rule(&b.package).state;
        let player = b.scope != bri_package_runtime::content::Scope::Global;
        let key = if player { &keys.player } else { &keys.global }
            .get(&b.key)
            .unwrap_or_else(|| panic!("{} is not declared", row.bind));
        assert!(
            key.visible != bri_package_runtime::content::Visible::Server,
            "{} stays on the server, so the HUD would stay blank",
            row.bind
        );
    }
    for k in &panel.keys {
        let command = rule(&k.package)
            .commands
            .iter()
            .find(|c| c.name == k.command)
            .unwrap_or_else(|| panic!("no command {}", k.command));
        assert!(command.args.is_empty() && !command.admin);
    }
}

fn with(extra: &[(&str, Side)]) -> PackageSet {
    let mut set = set();
    for &(id, side) in extra {
        set.packages.push(PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side,
            dir: id.into(),
            role: None,
        });
    }
    set
}

#[test]
fn an_addon_with_client_code_loads_beside_the_others() {
    // The spinning cube's `client` section belongs to the client sandbox;
    // the runtime accepts it and keeps every other Add-On running.
    let set = with(&[("spinning-cube", Side::Client)]);
    let client = Catalog::load(&root(), &set, false).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(client.packages.len(), 3);
    assert!(client.packages["spinning-cube"].manifest.client.is_some());
    assert_eq!(client.huds().count(), 1);
}

#[test]
fn one_broken_addon_is_left_out_and_the_rest_still_run() {
    let dir = tempfile::tempdir().unwrap();
    for id in [
        "sample-survival-points",
        "sample-points-hud",
        "sample-bubble-blaster",
    ] {
        copy_dir(&root().join(id), &dir.path().join(id));
    }
    std::fs::create_dir(dir.path().join("broken")).unwrap();
    std::fs::write(
        dir.path().join("broken/package.json"),
        r#"{ "schema_version": 1, "id": "broken", "misspelt": true }"#,
    )
    .unwrap();
    let set = with(&[("broken", Side::Shared)]);
    assert!(
        Catalog::load(dir.path(), &set, true).is_err(),
        "strict load"
    );
    let (catalog, problems) = Catalog::load_skipping(dir.path(), &set, true);
    assert!(!problems.is_empty());
    assert!(
        problems
            .iter()
            .all(|p| p.location.as_deref().unwrap_or("").starts_with("broken/")),
        "{problems:#?}"
    );
    assert_eq!(
        catalog.packages.keys().collect::<Vec<_>>(),
        [
            "sample-bubble-blaster",
            "sample-points-hud",
            "sample-survival-points"
        ]
    );
    Runtime::compile(&catalog).unwrap_or_else(|e| panic!("{e:#?}"));
}

#[test]
fn add_ons_that_clash_are_left_out_one_by_one_and_never_empty_the_set() {
    // A second HUD taking the Leaderboard's key for another command, and an
    // Add-On needing one that is not there: each is left out by name, and
    // the Add-Ons that load fine keep running, from a content root or from
    // a joiner's downloads.
    let dir = tempfile::tempdir().unwrap();
    for id in [
        "sample-survival-points",
        "sample-points-hud",
        "sample-bubble-blaster",
    ] {
        copy_dir(&root().join(id), &dir.path().join(id));
    }
    let clash = dir.path().join("zz-clash-hud");
    std::fs::create_dir(&clash).unwrap();
    std::fs::write(
        clash.join("package.json"),
        std::fs::read_to_string(root().join("sample-points-hud/package.json"))
            .unwrap()
            .replace("sample-points-hud", "zz-clash-hud"),
    )
    .unwrap();
    std::fs::write(
        clash.join("points.json"),
        std::fs::read_to_string(root().join("sample-points-hud/points.json"))
            .unwrap()
            .replace("\"command\": \"top\"", "\"command\": \"reset\""),
    )
    .unwrap();
    let needy = dir.path().join("needy");
    std::fs::create_dir(&needy).unwrap();
    std::fs::write(
        needy.join("package.json"),
        r#"{ "schema_version": 1, "id": "needy", "version": "1.0.0", "api": 1,
             "name": "Needy", "description": "Needs an Add-On nobody has.",
             "authors": ["test"], "license": "CC0-1.0",
             "provenance": { "source": "original" },
             "dependencies": { "nowhere": "^1.0.0" }, "provides": [] }"#,
    )
    .unwrap();
    let set = with(&[("zz-clash-hud", Side::Client), ("needy", Side::Shared)]);
    assert!(Catalog::load(dir.path(), &set, true).is_err(), "strict load");
    let kept = [
        "sample-bubble-blaster",
        "sample-points-hud",
        "sample-survival-points",
    ];
    let check = |catalog: &Catalog, problems: &[bri_package::diag::Diagnostic]| {
        assert_eq!(catalog.packages.keys().collect::<Vec<_>>(), kept, "{problems:#?}");
        let named = |id: &str, code: &str| {
            problems.iter().any(|p| {
                p.code == code && p.location.as_deref().unwrap_or("").starts_with(&format!("{id}/"))
            })
        };
        assert!(named("zz-clash-hud", "set.hud.key.conflict"), "{problems:#?}");
        assert!(named("needy", "set.dependency.missing"), "{problems:#?}");
    };
    let (catalog, problems) = Catalog::load_skipping(dir.path(), &set, true);
    check(&catalog, &problems);
    let dirs: Vec<_> = set
        .packages
        .iter()
        .map(|e| (dir.path().join(&e.dir), e.clone()))
        .collect();
    let (catalog, problems) = Catalog::load_dirs_skipping(&dirs, true);
    check(&catalog, &problems);
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
