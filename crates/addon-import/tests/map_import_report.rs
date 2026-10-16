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
