//! A `Print_<aspect>_<name>` Add-On (Print_ModTer_Default) imports as a
//! print pack the game adds to its prints, under the aspect its name
//! carries. Made-up prints, no original game assets.
use bri_addon_import::{Options, import};
use bri_content::brick_materials::{Bundle, PrintPack};

#[test]
fn a_print_add_on_imports_its_prints_under_its_aspect() {
    let root = std::env::temp_dir().join(format!("bri-print-pack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let input = root.join("Print_Turf_Default");
    let out = root.join("imported");
    std::fs::create_dir_all(input.join("prints")).unwrap();
    std::fs::create_dir_all(input.join("icons")).unwrap();
    std::fs::write(
        input.join("description.txt"),
        "Title: Invented turf prints\nAuthor: Test\n",
    )
    .unwrap();
    std::fs::write(input.join("server.cs"), "// print loading is automatic\n").unwrap();
    for (stem, rgba) in [("Grass", [40, 160, 40, 255]), ("dirt", [120, 80, 40, 255])] {
        image::RgbaImage::from_pixel(4, 4, image::Rgba(rgba))
            .save(input.join(format!("prints/{stem}.png")))
            .unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba(rgba))
            .save(input.join(format!("icons/{stem}.png")))
            .unwrap();
    }
    // v20 lists no print without its icon.
    image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 255, 255]))
        .save(input.join("prints/water.png"))
        .unwrap();
    let report = import(&Options {
        input,
        out: out.clone(),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(report.summary.verdict, "converted");

    let pack: PrintPack =
        serde_json::from_slice(&std::fs::read(out.join("assets/prints.json")).unwrap()).unwrap();
    let names: Vec<_> = pack.prints.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["dirt", "Grass"]);
    let grass = &pack.prints[1];
    assert_eq!(grass.id, "print/print_turf_default/grass");
    assert_eq!(grass.aspect, "Turf");
    assert_eq!(grass.aliases, ["Turf/Grass"]);
    assert_eq!(
        grass.icon.source.archive.as_deref(),
        Some("Add-Ons/Print_Turf_Default.zip")
    );
    assert_eq!(grass.icon.source.path, "icons/Grass.png");
    assert_eq!((grass.diffuse.width, grass.icon.width), (4, 2));
    assert!(out.join("assets").join(&grass.diffuse.path).is_file());
    let manifest = std::fs::read_to_string(out.join("package.json")).unwrap();
    assert!(
        manifest.contains("\"print_turf_default:prints/main\""),
        "{manifest}"
    );

    // The game adds it after its own prints; a save's `Turf/Grass` finds it.
    let base: Bundle = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "surfaces": ["top", "side", "bottom_edge", "bottom_loop", "ramp"].map(|s| (s.to_string(), image(&format!("{s}.png")))).into_iter().collect::<serde_json::Map<_, _>>(),
        "prints": [], "packages": [], "evidence": [],
        "excluded_installed_packages": [], "warnings": []
    }))
    .unwrap();
    let (merged, notes) = base.with_print_packs([("addons/print_turf_default/assets", pack)]);
    assert!(notes.is_empty(), "{notes:?}");
    let found = merged.resolve("turf/grass").unwrap();
    assert!(found.compatible("Turf"));
    assert_eq!(
        found.diffuse.path,
        "addons/print_turf_default/assets/prints/grass.png"
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn image(path: &str) -> serde_json::Value {
    let hash = "a".repeat(64);
    serde_json::json!({ "path": path, "width": 1, "height": 1, "sha256": hash,
        "source": { "path": path, "archive": null, "sha256": hash } })
}
