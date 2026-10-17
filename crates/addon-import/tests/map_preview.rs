//! Original image bytes through map import; all fixture art and missions invented.
use bri_addon_import::{Options, import};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{io::Cursor, path::Path};

fn picture(format: image::ImageFormat, color: [u8; 3], width: u32) -> Vec<u8> {
    let mut bytes = vec![];
    image::RgbImage::from_pixel(width, 2, image::Rgb(color))
        .write_to(&mut Cursor::new(&mut bytes), format)
        .unwrap();
    bytes
}
fn mission(name: &str) -> String {
    format!(
        "//--- OBJECT WRITE BEGIN ---\nnew SimGroup(MissionGroup) {{ new ScriptObject(MissionInfo) {{ name = \"{name}\"; previewImage = \"./decoy\"; }}; new SpawnSphere() {{ position = \"0 0 10\"; }}; }};\n//--- OBJECT WRITE END ---\n"
    )
}
fn run(input: &Path, output: &Path) -> Value {
    let report = import(&Options {
        input: input.into(),
        out: output.into(),
        ..Default::default()
    })
    .unwrap();
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(output.join("package.json")).unwrap()).unwrap();
    for provide in manifest["provides"].as_array().unwrap() {
        let bundle = output.join(provide["file"].as_str().unwrap());
        let b: Value = serde_json::from_slice(&std::fs::read(&bundle).unwrap()).unwrap();
        if let Some(file) = b["maps"][0]["preview"]["file"].as_str() {
            let relative = bundle
                .parent()
                .unwrap()
                .join(file)
                .strip_prefix(output)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            assert!(
                report.package.files.contains(&relative),
                "Preview must enter package inventory"
            );
        }
    }
    manifest
}

#[test]
fn distinct_missions_copy_companion_originals_with_png_priority_and_case_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("Map_Preview");
    std::fs::create_dir(&input).unwrap();
    let first = picture(image::ImageFormat::Png, [180, 20, 40], 3);
    let second = picture(image::ImageFormat::Jpeg, [30, 140, 90], 4);
    std::fs::write(input.join("First.mis"), mission("First")).unwrap();
    std::fs::write(input.join("Second.mis"), mission("Second")).unwrap();
    std::fs::write(input.join("FIRST.PNG"), &first).unwrap();
    std::fs::write(
        input.join("First.jpg"),
        picture(image::ImageFormat::Jpeg, [1, 2, 3], 5),
    )
    .unwrap();
    std::fs::write(input.join("sEcOnD.JpG"), &second).unwrap();
    std::fs::write(
        input.join("decoy.png"),
        picture(image::ImageFormat::Png, [7, 8, 9], 6),
    )
    .unwrap();
    let output = dir.path().join("out");
    let manifest = run(&input, &output);
    for (i, bytes, width) in [(0, &first, 3), (1, &second, 4)] {
        let bundle = output.join(manifest["provides"][i]["file"].as_str().unwrap());
        let b: Value = serde_json::from_slice(&std::fs::read(&bundle).unwrap()).unwrap();
        let p = &b["maps"][0]["preview"];
        assert_eq!(p["width"], width, "Missing imported preview: {b:?}");
        assert_eq!(p["height"], 2);
        assert_eq!(p["sha256"], format!("{:x}", Sha256::digest(bytes)));
        assert_eq!(
            std::fs::read(bundle.parent().unwrap().join(p["file"].as_str().unwrap())).unwrap(),
            *bytes
        );
        assert!(!p["file"].as_str().unwrap().contains(['/', '\\', ':']));
    }
}

#[test]
fn absent_corrupt_unsupported_and_oversized_previews_leave_maps_importable() {
    let cases = [
        None,
        Some(b"corrupt PNG".to_vec()),
        Some(b"GIF89a unsupported".to_vec()),
        Some(picture(image::ImageFormat::Jpeg, [9, 8, 7], 3)),
        Some(picture(image::ImageFormat::Png, [4, 5, 6], 8193)),
        Some(vec![0; 4 * 1024 * 1024 + 1]),
    ];
    for preferred in cases {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("Map_Preview");
        std::fs::create_dir(&input).unwrap();
        std::fs::write(
            input.join("First.mis"),
            mission("First").replace("./decoy", "../decoy"),
        )
        .unwrap();
        // Metadata does not authorize escaping to an unrelated authored field.
        std::fs::write(
            dir.path().join("decoy.png"),
            picture(image::ImageFormat::Png, [7, 8, 9], 3),
        )
        .unwrap();
        if let Some(bytes) = preferred {
            std::fs::write(input.join("First.png"), bytes).unwrap();
            std::fs::write(
                input.join("First.jpg"),
                picture(image::ImageFormat::Jpeg, [1, 2, 3], 3),
            )
            .unwrap();
        }
        let output = dir.path().join("out");
        let manifest = run(&input, &output);
        let b: Value = serde_json::from_slice(
            &std::fs::read(output.join(manifest["provides"][0]["file"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        assert_eq!(
            b["maps"][0].get("preview"),
            None,
            "Invalid optional image must not be bound"
        );
        assert_eq!(b["maps"].as_array().unwrap().len(), 1);
    }
}
