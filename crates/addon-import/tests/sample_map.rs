//! `packages/samples/sample-map` is what `bri-import-addon gltf --kind map`
//! makes from its own `blender/sample-map.glb`, so the sample modders copy
//! never drifts from the importer. `BRI_BLESS=1` rewrites it (the `.glb`
//! from `testing::room`, then the Add-On from the `.glb`).
use bri_addon_import::gltf::{self, Kind, Options};
use std::path::{Path, PathBuf};

const LICENSE: &str = "CC0-1.0";

fn sample() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/samples/sample-map")
}

/// Every file under `dir` but the Blender source, by relative path.
fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                if !rel.starts_with("blender/") {
                    out.push((rel, std::fs::read(&path).unwrap()));
                }
            }
        }
    }
    out.sort();
    out
}

/// The sample as the importer makes it from `glb`, with the sample's own
/// authors and licence.
fn import(glb: &Path, out: &Path) {
    let mut opts = Options::new(glb, out, Kind::Map);
    opts.name = Some("Sample Map".into());
    gltf::import(&opts).unwrap();
    std::fs::remove_file(out.join("IMPORT-REPORT.md")).unwrap();
    let manifest = out.join("package.json");
    let mut info: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    info["authors"] = serde_json::json!(["Blockland ReImagined"]);
    info["license"] = LICENSE.into();
    info["description"] =
        "A sample map Add-On to copy: a room with a spawn point and two lights, imported from blender/sample-map.glb (docs/modding/maps.md).".into();
    info["provenance"] = serde_json::json!({ "source": "original" });
    let mut text = serde_json::to_vec_pretty(&info).unwrap();
    text.push(b'\n');
    std::fs::write(manifest, text).unwrap();
}

#[test]
fn the_sample_map_is_what_the_importer_makes() {
    let glb = sample().join("blender/sample-map.glb");
    let made = tempfile::tempdir().unwrap();
    if std::env::var_os("BRI_BLESS").is_some() {
        let _ = std::fs::remove_dir_all(sample());
        std::fs::create_dir_all(glb.parent().unwrap()).unwrap();
        std::fs::write(&glb, gltf::testing::room(false)).unwrap();
        // The importer wants a fresh folder; keep the source beside it.
        import(&glb, &made.path().join("map"));
        for (rel, bytes) in files(&made.path().join("map")) {
            let path = sample().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
    }
    import(&glb, &made.path().join("again"));
    let want = files(&made.path().join("again"));
    let have = files(&sample());
    assert_eq!(
        have.iter().map(|(p, _)| p).collect::<Vec<_>>(),
        want.iter().map(|(p, _)| p).collect::<Vec<_>>(),
        "rerun with BRI_BLESS=1"
    );
    for ((path, a), (_, b)) in have.iter().zip(&want) {
        assert!(
            a == b,
            "{path} differs from a fresh import; rerun with BRI_BLESS=1"
        );
    }
    let check = bri_package_runtime::check::check(&sample());
    assert!(check.ok, "{check}");
}
