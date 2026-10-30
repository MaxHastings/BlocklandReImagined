//! Map reports describe retained gaps; no original assets or runtime substitutes.
use bri_addon_import::{Options, import, report::Report};
use std::path::Path;

fn terrain(material: &str) -> Vec<u8> {
    let mut bytes = vec![3];
    for _ in 0..65536 {
        bytes.extend(320u16.to_le_bytes());
    }
    bytes.extend(vec![0; 65536]);
    bytes.push(material.len() as u8);
    bytes.extend(material.as_bytes());
    bytes.extend([0; 7]);
    bytes.extend(vec![255; 65536]);
    bytes.extend(0u32.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    bytes
}
fn fixture(base: &Path, missing: bool, objects: &str, header: &str, effects: bool) -> Report {
    let input = base.join("Map_Report");
    std::fs::create_dir(&input).unwrap();
    std::fs::write(
        input.join("terrain.ter"),
        terrain("Add-Ons/Map_Report/ground"),
    )
    .unwrap();
    if !missing {
        image::RgbImage::from_pixel(4, 4, image::Rgb([80, 140, 30]))
            .save(input.join("ground.png"))
            .unwrap();
    }
    std::fs::write(input.join("report.mis"), format!(
        "{header}//--- OBJECT WRITE BEGIN ---\nnew SimGroup(MissionGroup) {{\nnew ScriptObject(MissionInfo) {{ name = \"Report fixture\"; }};\nnew Sun() {{ azimuth = \"180\"; elevation = \"60\"; color = \"1 1 1 1\"; ambient = \"0.4 0.4 0.4 1\"; }};\nnew TerrainBlock(Terrain) {{ terrainFile = \"./terrain.ter\"; squareSize = \"8\"; }};\nnew SpawnSphere() {{ position = \"0 0 30\"; }};\n{objects}\n}};\n//--- OBJECT WRITE END ---\n"
    )).unwrap();
    if effects {
        std::fs::write(input.join("effects.cs"), r#"
datablock ParticleData(ReportParticle) { textureName = "base/data/particles/cloud"; lifetimeMS = 1000; };
datablock ParticleEmitterData(ReportEmitter) { ejectionPeriodMS = 20; particles = ReportParticle; };
"#).unwrap();
    }
    import(&Options {
        input,
        out: base.join("out"),
        ..Default::default()
    })
    .unwrap()
}

#[test]
fn retained_map_item_missing_layer_and_source_section_are_reported_as_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let report = fixture(
        dir.path(),
        true,
        "new Item(Sign) { position = \"1 2 30\"; dataBlock = \"ReportSign\"; static = \"1\"; rotate = \"1\"; canPickup = \"1\"; };",
        "// Header includes a declaration and an unhandled call, not three functions.\ndatablock ItemData(ReportSign) { shapeFile = \"./sign.dts\"; };\nsetupReportSign();\n",
        false,
    );
    assert_eq!(report.summary.verdict, "converted_with_gaps");
    assert_eq!(report.summary.unsupported, 2);
    assert_eq!(report.summary.ambiguous, 1);
    assert_eq!(
        report.summary.needs_behaviour, 0,
        "No invented function count"
    );
    assert_eq!(
        report.summary.datablocks_unsupported, 0,
        "No invented datablock census"
    );
    assert!(
        report
            .unsupported
            .iter()
            .any(|x| x.what == "retained_map_object"
                && x.detail.contains("item")
                && x.detail.contains("ReportSign"))
    );
    assert!(
        report
            .unsupported
            .iter()
            .any(|x| x.what == "terrain_texture_stand_in" && x.detail.contains("ground"))
    );
    assert!(
        report
            .ambiguous
            .iter()
            .any(|x| x.what == "retained_mission_source" && x.detail.contains("1..3"))
    );
    let markdown = std::fs::read_to_string(dir.path().join("out/IMPORT-REPORT.md")).unwrap();
    assert!(markdown.contains("converted with gaps"));
    assert!(!markdown.contains("every script function is covered"));
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("out/import-report.json")).unwrap())
            .unwrap();
    assert_eq!(written["summary"]["unsupported"], 2);
    assert_eq!(written["summary"]["ambiguous"], 1);
}

#[test]
fn fully_supported_map_and_comment_only_header_keep_clean_verdict() {
    let dir = tempfile::tempdir().unwrap();
    let report = fixture(
        dir.path(),
        false,
        "",
        "// Metadata comment, not executable source.\n",
        false,
    );
    assert_eq!(report.summary.verdict, "converted");
    assert!(report.unsupported.is_empty());
    assert!(report.ambiguous.is_empty());
    assert!(report.needs_behaviour.is_empty());
    assert!(
        report
            .markdown()
            .contains("Nothing: every script function is covered by data.")
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("out/IMPORT-REPORT.md")).unwrap(),
        report.markdown()
    );
}

#[test]
fn successfully_adapted_map_emitter_is_not_an_unsupported_object() {
    let dir = tempfile::tempdir().unwrap();
    let report = fixture(
        dir.path(),
        false,
        "new ParticleEmitterNode(Emitter) { emitter = \"ReportEmitter\"; position = \"0 0 35\"; };",
        "",
        true,
    );
    assert_eq!(report.summary.verdict, "converted");
    assert!(report.unsupported.is_empty());
    assert!(report.ambiguous.is_empty());
    assert!(dir.path().join("out/map/map_effects.json").is_file());
}

#[test]
fn automatic_map_import_reuses_the_installed_vanilla_sky_without_an_original_install() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let original = root.join("reference");
    let sky = original.join("base/data/skies/Shared");
    let content = root.join("content");
    let native = content.join("native-maps");
    let input = content.join("Add-Ons/Map_Installed");
    std::fs::create_dir_all(&sky).unwrap();
    std::fs::create_dir_all(&native).unwrap();
    std::fs::create_dir_all(&input).unwrap();
    for i in 0..6 {
        image::RgbImage::from_pixel(2, 2, image::Rgb([i * 20, 40, 80]))
            .save(sky.join(format!("{i}.png")))
            .unwrap();
    }
    std::fs::write(sky.join("resource.dml"), "0\n1\n2\n3\n4\n5\n").unwrap();
    let scene = bri_convert::mission::read(
        "new Sky() { materialList = \"base/data/skies/Shared/resource.dml\"; };",
        "Add-Ons/Map_Stock/map.mis",
    )
    .unwrap();
    let sky =
        bri_convert::environment::convert(&original, "Add-Ons/Map_Stock/map.mis", &scene, &native)
            .unwrap()
            .unwrap();
    std::fs::write(
        native.join("bundle.json"),
        serde_json::to_vec(&serde_json::json!({"environments":{"stock":sky}})).unwrap(),
    )
    .unwrap();
    std::fs::write(content.join("packages.json"), r#"{"schema_version":1,"packages":[{"id":"v20-map-bundle","version":"1.0.0","side":"shared","dir":"native-maps","role":"map_bundle"}]}"#).unwrap();
    std::fs::write(
        input.join("terrain.ter"),
        terrain("Add-Ons/Map_Installed/ground"),
    )
    .unwrap();
    image::RgbImage::from_pixel(2, 2, image::Rgb([80, 140, 30]))
        .save(input.join("ground.png"))
        .unwrap();
    std::fs::write(input.join("installed.mis"), "new SimGroup(MissionGroup) { new Sky() { materialList = \"~/data/skies/Shared/resource.dml\"; fogDistance = \"17\"; visibleDistance = \"330\"; useSkyTextures = \"0\"; }; new TerrainBlock(Terrain) { terrainFile = \"./terrain.ter\"; }; new SpawnSphere() { position = \"0 0 30\"; }; };").unwrap();
    // The game installation, not an available source install, supplies it.
    std::fs::remove_dir_all(&original).unwrap();
    let out = content.join("addons/staged-map");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_bri-import-addon"))
        .arg(&input)
        .arg(&out)
        .arg("--installed")
        .arg(&content)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("map/bundle.json")).unwrap()).unwrap();
    let imported = &bundle["environments"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(
        imported["source_materials"],
        "base/data/skies/Shared/resource.dml"
    );
    assert_eq!(imported["source_sha256"], sky.source_sha256);
    assert_eq!(imported["fog"]["start"], 17.0);
    assert_eq!(imported["fog"]["end"], 330.0);
    assert_eq!(imported["textures"], false);
    for image in &sky.faces {
        assert_eq!(
            std::fs::read(native.join(&image.file)).unwrap(),
            std::fs::read(out.join("map").join(&image.file)).unwrap()
        );
    }
    assert_eq!(
        std::fs::read_dir(&input).unwrap().count(),
        3,
        "source Add-On was modified"
    );
}
