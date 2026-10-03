//! User colorsets remain authored data rather than requiring executable code.
use bri_addon_import::{Options, import};

#[test]
fn colorsets_preserve_divisions_and_bytes_without_scripts() {
    let temp = std::env::temp_dir().join(format!("bri-import-colorset-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let input = temp.join("Colorset_Creator");
    std::fs::create_dir(&input).unwrap();
    let bytes = b"255 0 0 255\r\nDIV: Red\r\n0 0 255 128\r\nDIV: Glass\r\n";
    std::fs::write(input.join("COLORSET.TXT"), bytes).unwrap();
    std::fs::write(
        input.join("description.txt"),
        "Title: Creator Colors\nAuthor: Test",
    )
    .unwrap();
    let out = temp.join("native");
    let report = import(&Options {
        input,
        out: out.clone(),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(std::fs::read(out.join("colorSet.txt")).unwrap(), bytes);
    assert!(
        report
            .assets
            .iter()
            .any(|a| a.source.ends_with("COLORSET.TXT") && a.status == "copied")
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("package.json")).unwrap()).unwrap();
    assert_eq!(manifest["capabilities"], serde_json::json!([]));
    assert_eq!(manifest["name"], "Creator Colors");
    std::fs::remove_dir_all(temp).unwrap();
}
