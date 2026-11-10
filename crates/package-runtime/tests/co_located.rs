//! Author folder -> one ZIP -> transactional installation -> host/client loading.
use bri_package::{
    addon, classic, environment::Environment, library::Library, packages::PackageSet, sync::Listing,
};
use bri_package_runtime::{Catalog, check};

#[test]
fn co_located_rules_keep_their_host_boundary_through_the_real_install_path() -> anyhow::Result<()> {
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/samples/sample-co-located");
    let checked = check::check(&sample);
    assert!(checked.ok, "{checked}");
    assert_eq!(checked.add_ons.len(), 2);
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    let zip = root.join("one-addon.zip");
    let packed = classic::pack_native_zip(&sample, &zip)?;
    assert_eq!(packed.folders.len(), 1);
    std::fs::write(
        root.join("packages.json"),
        br#"{"schema_version":1,"packages":[]}"#,
    )?;
    let mut library = Library::scan(root)?;
    let staged = library.staging_dir("One Add-On")?;
    classic::unpack_native_zip(&zip, &root.join(&staged))?;
    let (id, _) = library.install_staged("One Add-On", &staged, None)?;
    let plan = library.plan(&id, true);
    assert!(plan.allowed(), "{plan:?}");
    library.apply(&plan)?;
    let set = PackageSet::load_root(root)?;
    assert_eq!(set.packages.len(), 2);
    let host = Catalog::load(root, &set, true).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(host.packages["sample-co-located-rules"].behaviour.is_some());
    let client = Catalog::load(root, &set, false).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(client.packages.len(), 1);
    assert_eq!(client.packages[&id].models.len(), 1);
    let environment = Environment::load(root, &set)?;
    let visible = environment.client_packages();
    assert_eq!(visible.len(), 1);
    let entry = set.packages.iter().find(|p| p.id == id).unwrap();
    let listing = Listing::of(&root.join(&entry.dir), &visible[0])?;
    assert!(listing.files.iter().all(|f| !f.path.ends_with(".rhai")));
    let rules_hash = environment
        .packages
        .iter()
        .find(|p| p.id.ends_with("-rules"))
        .unwrap()
        .hash
        .clone();
    let rules = set
        .packages
        .iter()
        .find(|p| p.id.ends_with("-rules"))
        .unwrap();
    std::fs::write(root.join(&rules.dir).join("main.rhai"), "fn on_join(p) {}")?;
    let changed = Environment::load(root, &set)?;
    assert_eq!(
        changed.client_packages(),
        visible,
        "server code must not alter content identity"
    );
    assert_ne!(
        changed
            .packages
            .iter()
            .find(|p| p.id.ends_with("-rules"))
            .unwrap()
            .hash,
        rules_hash
    );
    let off = library.plan(&id, false);
    library.apply(&off)?;
    assert!(PackageSet::load_root(root)?.packages.is_empty());
    // Folder installation follows exactly the same staging representation.
    let stage = root.join("folder-copy");
    addon::stage(&sample, &stage)?;
    assert_eq!(
        std::fs::read(stage.join("marker.json"))?,
        std::fs::read(sample.join("content/marker.json"))?
    );
    Ok(())
}

#[test]
fn authoring_dependencies_cross_wrapper_boundaries_without_relocating_packages()
-> anyhow::Result<()> {
    use serde_json::json;
    let root = tempfile::tempdir()?;
    let root = root.path();
    let write = |path: &str, data: serde_json::Value| -> anyhow::Result<()> {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, serde_json::to_vec(&data)?)?;
        Ok(())
    };
    write(
        "game/addon.json",
        json!({"schema_version":1,"content":"art","server":"policy"}),
    )?;
    write(
        "game/art/package.json",
        json!({"schema_version":1,"id":"story-art","version":"1.0.0","api":1,"name":"Art","license":"CC0-1.0","companions":["story-policy"],"dependencies":{"shared-art":"*"},"provides":[]}),
    )?;
    write(
        "game/policy/package.json",
        json!({"schema_version":1,"id":"story-policy","version":"1.0.0","api":1,"name":"Policy","license":"CC0-1.0","dependencies":{"story-art":"*"},"provides":[{"kind":"behaviour","id":"story-policy:behaviour/main","file":"behaviour.json"},{"kind":"script","id":"story-policy:script/main","file":"main.rhai"}]}),
    )?;
    write(
        "game/policy/behaviour.json",
        json!({"schema_version":1,"script":"main.rhai","on_join":true}),
    )?;
    std::fs::write(root.join("game/policy/main.rhai"), "fn on_join(p) {}")?;
    write(
        "shared/addon.json",
        json!({"schema_version":1,"content":"assets"}),
    )?;
    write(
        "shared/assets/package.json",
        json!({"schema_version":1,"id":"shared-art","version":"1.0.0","api":1,"name":"Shared","license":"CC0-1.0","provides":[]}),
    )?;
    write(
        "consumer/package.json",
        json!({"schema_version":1,"id":"consumer","version":"1.0.0","api":1,"name":"Consumer","license":"CC0-1.0","dependencies":{"story-art":"*"},"provides":[]}),
    )?;
    for folder in ["game", "consumer"] {
        let report = check::check(&root.join(folder));
        assert!(report.ok, "{folder}: {report}");
        assert!(report.add_ons.iter().any(|a| a.id == "shared-art"));
    }
    std::fs::remove_file(root.join("shared/assets/package.json"))?;
    assert!(
        !check::check(&root.join("game")).ok,
        "missing external dependency must still fail"
    );
    Ok(())
}

#[test]
fn co_location_does_not_allow_path_escape_or_client_rules() {
    for json in [
        r#"{"schema_version":1,"content":"../outside"}"#,
        r#"{"schema_version":1,"content":"C:escape"}"#,
        r#"{"schema_version":1,"content":"content","server":"CONTENT"}"#,
        r#"{"schema_version":2,"content":"content"}"#,
    ] {
        assert!(addon::Layout::parse(json.as_bytes()).is_err());
    }
    let layout =
        addon::Layout::parse(br#"{"schema_version":1,"content":"content","server":"server"}"#)
            .unwrap();
    let content =
        br#"{"id":"example","companions":["example-rules"],"provides":[{"kind":"model"}]}"#;
    let wrong =
        br#"{"id":"example-rules","dependencies":{"example":"*"},"provides":[{"kind":"model"}]}"#;
    assert!(layout.validate_manifests(content, Some(wrong)).is_err());
}
