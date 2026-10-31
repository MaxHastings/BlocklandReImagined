//! Original-free reference Add-Ons lend identical bytes at distinct authored paths.
use bri_addon_import::{Options, import};
use std::collections::BTreeSet;

#[test]
fn an_installed_native_folder_is_not_a_conflicting_legacy_source() {
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    let mut zip =
        zip::ZipWriter::new(std::fs::File::create(root.path().join("Brick_Lender.zip")).unwrap());
    zip.start_file("shape.blb", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"1 1 3\nBRICK\n").unwrap();
    zip.finish().unwrap();
    let native = root.path().join("brick_lender");
    std::fs::create_dir_all(&native).unwrap();
    std::fs::write(native.join("package.json"), r#"{"schema_version":1,"id":"brick_lender","version":"1.0.0","api":1,"name":"Native","license":"CC0-1.0","provides":[]}"#).unwrap();
    let borrower = root.path().join("Brick_Borrower");
    std::fs::create_dir_all(&borrower).unwrap();
    std::fs::write(
        borrower.join("server.cs"),
        r#"LoadRequiredAddOn("Brick_Lender");
        datablock fxDTSBrickData(BrickBorrowData) { brickFile="Add-Ons/Brick_Lender/shape.blb";
        category="Special"; subCategory="Test"; uiName="Borrowed"; };"#,
    )
    .unwrap();
    let out = root.path().join("output");
    import(&Options {
        input: borrower,
        out: out.clone(),
        ..Default::default()
    })
    .unwrap();
    let dir = out.join("assets/brick-catalog");
    assert_eq!(
        bri_sim::definitions::Definitions::load(&dir, &dir)
            .unwrap()
            .entries
            .len(),
        1
    );
}

#[test]
fn conflicting_sibling_sources_are_rejected_instead_of_choosing_arbitrarily() {
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    let folder = root.path().join("Brick_Lender");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("shape.blb"), b"1 1 3\nBRICK\n").unwrap();
    let mut zip =
        zip::ZipWriter::new(std::fs::File::create(root.path().join("Brick_Lender.zip")).unwrap());
    zip.start_file("shape.blb", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"2 2 3\nBRICK\n").unwrap();
    zip.finish().unwrap();
    let borrower = root.path().join("Brick_Borrower");
    std::fs::create_dir_all(&borrower).unwrap();
    std::fs::write(
        borrower.join("server.cs"),
        r#"LoadRequiredAddOn("Brick_Lender");
        datablock fxDTSBrickData(BrickBorrowData) { brickFile="Add-Ons/Brick_Lender/shape.blb";
        category="Special"; subCategory="Test"; uiName="Borrowed"; };"#,
    )
    .unwrap();
    let error = import(&Options {
        input: borrower.clone(),
        out: root.path().join("output"),
        ..Default::default()
    })
    .unwrap_err();
    assert!(format!("{error:#}").contains("conflicting"), "{error:#}");
    let mut zip =
        zip::ZipWriter::new(std::fs::File::create(root.path().join("Brick_Lender.zip")).unwrap());
    zip.start_file("shape.blb", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"1 1 3\nBRICK\n").unwrap();
    zip.finish().unwrap();
    import(&Options {
        input: borrower,
        out: root.path().join("identical"),
        ..Default::default()
    })
    .expect("identical folder and ZIP are not ambiguous");
}

#[test]
#[ignore = "needs a private Bot_Town import; set BRI_BOT_TOWN_OUTPUT"]
fn supplied_bot_town_catalog_loads_all_eight_native_shapes() {
    let root = std::path::PathBuf::from(
        std::env::var_os("BRI_BOT_TOWN_OUTPUT").expect("BRI_BOT_TOWN_OUTPUT"),
    );
    let dir = root.join("assets/brick-catalog");
    let catalog: bri_content::brick::Catalog =
        serde_json::from_slice(&std::fs::read(dir.join("stock-catalog.json")).unwrap()).unwrap();
    let defs = bri_sim::definitions::Definitions::load(&dir, &dir).unwrap();
    assert_eq!(catalog.bricks.len(), 8);
    assert_eq!(defs.entries.len(), 8);
    for entry in &catalog.bricks {
        let def = &defs.entries[&entry.id];
        let bounds = def.shape.compute_local_aabb();
        assert!(
            (0..3).all(|i| bounds.mins[i].is_finite() && bounds.maxs[i] > bounds.mins[i]),
            "{}",
            entry.id
        );
        assert_eq!(def.mesh.id, entry.mesh_id);
    }
}

#[test]
fn a_declared_dependency_can_lend_its_installed_native_brick_geometry() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let source = root.join("provider-source/Brick_Lender");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("shape.blb"), b"1 1 3\nBRICK\n").unwrap();
    let content = root.join("installed");
    std::fs::create_dir_all(content.join("addons")).unwrap();
    std::fs::write(
        content.join("packages.json"),
        r#"{"schema_version":1,"packages":[]}"#,
    )
    .unwrap();
    import(&Options {
        input: source,
        out: content.join("addons/brick_lender"),
        ..Default::default()
    })
    .unwrap();
    let borrower = root.join("borrower-source/Brick_Borrower");
    std::fs::create_dir_all(&borrower).unwrap();
    std::fs::write(
        borrower.join("server.cs"),
        r#"
        LoadRequiredAddOn("Brick_Lender");
        datablock fxDTSBrickData(BrickBorrowData) {
            brickFile = "Add-ONs/bRiCk_LeNder/SHAPE.blb";
            category = "Special"; subCategory = "Test"; uiName = "Borrowed";
        };
    "#,
    )
    .unwrap();
    let out = root.join("output");
    let report = import(&Options {
        input: borrower.clone(),
        out: out.clone(),
        installed: Some(content.clone()),
        ..Default::default()
    })
    .unwrap();
    let dir = out.join("assets/brick-catalog");
    let defs = bri_sim::definitions::Definitions::load(&dir, &dir)
        .expect("the dependent native shape and collision load");
    assert!(
        defs.entries
            .contains_key("brick_borrower:brick/brickborrowdata")
    );
    assert!(
        report
            .dependencies
            .iter()
            .any(|d| d.addon.eq_ignore_ascii_case("brick_lender") && d.status != "missing")
    );
    let script = std::fs::read_to_string(borrower.join("server.cs")).unwrap();
    std::fs::write(
        borrower.join("server.cs"),
        script.replace("LoadRequiredAddOn(\"Brick_Lender\");", ""),
    )
    .unwrap();
    let undeclared_out = root.join("undeclared");
    import(&Options {
        input: borrower.clone(),
        out: undeclared_out.clone(),
        installed: Some(content.clone()),
        ..Default::default()
    })
    .unwrap();
    assert!(
        !undeclared_out
            .join("assets/brick-catalog/stock-catalog.json")
            .exists(),
        "an undeclared installed package must not lend geometry"
    );
    std::fs::write(borrower.join("server.cs"), script).unwrap();
    let geometry = report
        .assets
        .iter()
        .find(|a| a.kind == "brick_geometry")
        .unwrap();
    assert_eq!(geometry.status, "copied");
    assert!(geometry.notes.iter().any(|n| n.contains("native SHA256")));
    let first_output = geometry.output.clone();
    let provider = content.join("addons/brick_lender");
    let content_index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(provider.join("assets/content.json")).unwrap())
            .unwrap();
    let path = provider.join(
        content_index["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["kind"] == "brick_geometry")
            .unwrap()["file"]
            .as_str()
            .unwrap(),
    );
    let mut mesh: bri_content::brick::Brick =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    mesh.quads[0].vertices[0].position[0] += 0.001;
    mesh.validate().unwrap();
    std::fs::write(&path, serde_json::to_vec(&mesh).unwrap()).unwrap();
    let changed = import(&Options {
        input: borrower.clone(),
        out: root.join("changed"),
        installed: Some(content.clone()),
        ..Default::default()
    })
    .unwrap();
    assert_ne!(
        changed
            .assets
            .iter()
            .find(|a| a.kind == "brick_geometry")
            .unwrap()
            .output,
        first_output,
        "native changes invalidate the borrowed output fingerprint"
    );
    // An installed file claiming a different identity is not a usable source.
    mesh.id = "other:brick_geometry/forged".into();
    std::fs::write(&path, serde_json::to_vec(&mesh).unwrap()).unwrap();
    let invalid_out = root.join("invalid");
    import(&Options {
        input: borrower,
        out: invalid_out.clone(),
        installed: Some(content),
        ..Default::default()
    })
    .unwrap();
    assert!(
        !invalid_out
            .join("assets/brick-catalog/stock-catalog.json")
            .exists(),
        "a mismatched installed mesh must not be borrowed"
    );
}

#[test]
fn borrowed_identical_brick_bytes_keep_each_authored_mesh_binding_loadable() {
    let root = std::env::temp_dir().join(format!("bri-borrowed-alias-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let reference = root.join("reference");
    for (lender, filename) in [("Brick_Quartz", "moon.blb"), ("Brick_Indigo", "cobalt.blb")] {
        let folder = reference.join("Add-Ons").join(lender);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("description.txt"),
            "Title: Invented geometry lender\nAuthor: Test\n",
        )
        .unwrap();
        // Built-in BRICK syntax is a complete authored box, with no originals.
        std::fs::write(folder.join(filename), b"1 1 3\nBRICK\n").unwrap();
    }
    let input = root.join("Brick_Saffron");
    std::fs::create_dir_all(&input).unwrap();
    std::fs::write(
        input.join("description.txt"),
        "Title: Invented two-path borrower\nAuthor: Test\n",
    )
    .unwrap();
    std::fs::write(
        input.join("server.cs"),
        r#"
        datablock fxDTSBrickData(BrickBorrowQuartzData) {
            brickFile = "Add-Ons/Brick_Quartz/moon.blb";
            category = "Special"; subCategory = "Aliases"; uiName = "Quartz Borrow";
        };
        datablock fxDTSBrickData(BrickBorrowIndigoData) {
            brickFile = "Add-Ons/Brick_Indigo/cobalt.blb";
            category = "Special"; subCategory = "Aliases"; uiName = "Indigo Borrow";
        };
    "#,
    )
    .unwrap();
    let out = root.join("native");
    import(&Options {
        input,
        out: out.clone(),
        reference: Some(reference),
        ..Default::default()
    })
    .unwrap();
    let catalog_dir = out.join("assets/brick-catalog");
    let catalog: bri_content::brick::Catalog =
        serde_json::from_slice(&std::fs::read(catalog_dir.join("stock-catalog.json")).unwrap())
            .unwrap();
    assert_eq!(
        catalog.bricks.len(),
        2,
        "both authored borrower datablocks survive"
    );
    let ids: BTreeSet<_> = catalog.bricks.iter().map(|brick| &brick.mesh_id).collect();
    assert_eq!(
        ids.len(),
        2,
        "distinct authored paths retain distinct stable IDs"
    );
    // Load this package by itself: neither lender contributes a native catalog.
    let definitions = bri_sim::definitions::Definitions::load(&catalog_dir, &catalog_dir).unwrap();
    for brick in &catalog.bricks {
        let definition = &definitions.entries[&brick.id];
        assert_eq!(
            definition.mesh.id, brick.mesh_id,
            "{} loaded another path's overwritten mesh",
            brick.id
        );
        assert_eq!(definition.mesh.footprint_studs, [1, 1]);
        assert_eq!(definition.mesh.height_plates, 3);
    }
    std::fs::remove_dir_all(root).unwrap();
}

/// Reads generated native content only; original archives remain outside Git.
#[test]
#[ignore = "needs generated original content; optional BRI_REFRESH_BUNDLE overrides content"]
fn refreshed_creature_bundle_catalogs_load_without_lender_geometry() {
    let bundle = std::env::var_os("BRI_REFRESH_BUNDLE")
        .or_else(|| std::env::var_os("BRI_CONTENT"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"));
    if !bundle.join("addons/bot_shark/package.json").is_file() {
        eprintln!(
            "skipped: generated original content is unavailable at {}",
            bundle.display()
        );
        return;
    }
    for package in ["bot_shark", "bot_zombie"] {
        let catalog_dir = bundle
            .join("addons")
            .join(package)
            .join("assets/brick-catalog");
        let catalog: bri_content::brick::Catalog =
            serde_json::from_slice(&std::fs::read(catalog_dir.join("stock-catalog.json")).unwrap())
                .unwrap();
        assert!(
            !catalog.bricks.is_empty(),
            "{package}: expected authored hole catalog"
        );
        // Load each independently: the lender's catalog is deliberately absent.
        let definitions = bri_sim::definitions::Definitions::load(&catalog_dir, &catalog_dir)
            .unwrap_or_else(|error| {
                panic!("{package}: actual native geometry load failed: {error:#}")
            });
        assert_eq!(definitions.entries.len(), catalog.bricks.len());
        for entry in &catalog.bricks {
            let definition = &definitions.entries[&entry.id];
            assert_eq!(
                definition.mesh.id, entry.mesh_id,
                "{}: stable binding",
                entry.id
            );
            let bounds = definition.shape.compute_local_aabb();
            for axis in 0..3 {
                assert!(
                    bounds.mins[axis].is_finite()
                        && bounds.maxs[axis].is_finite()
                        && bounds.maxs[axis] > bounds.mins[axis],
                    "{}: loaded native collision",
                    entry.id
                );
            }
        }
        eprintln!(
            "{package}: {} original imported brick definitions loaded without lender geometry",
            definitions.entries.len()
        );
    }
}
