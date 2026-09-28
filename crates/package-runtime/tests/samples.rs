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
    assert!(server.packages["sample-survival-points"].behaviour.is_some());
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
        let key = if b.player { &keys.player } else { &keys.global }
            .get(&b.key)
            .unwrap_or_else(|| panic!("{} is not declared", row.bind));
        assert!(key.public, "{} is private, so the HUD would stay blank", row.bind);
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
