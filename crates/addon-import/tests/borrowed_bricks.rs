//! Original-free reference Add-Ons lend identical bytes at distinct authored paths.
use bri_addon_import::{Options, import};
use std::collections::BTreeSet;

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
