//! The same v20 content must import to byte-identical packages on every
//! machine: players join only when their shared packages hash alike, and
//! each player imports the base game from their own v20 folder.
use anyhow::{Context, Result, ensure};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const BEDROOM: &str = "//--- OBJECT WRITE BEGIN ---\r\n\
new SimGroup(MissionGroup) {\r\n\
   new ScriptObject(MissionInfo) { name = \"Fixture Bedroom\"; };\r\n\
   new Sun() { azimuth = \"238\"; elevation = \"33\"; color = \"0.6 0.6 0.6 1\"; ambient = \"0.4 0.4 0.4 1\"; };\r\n\
   new SimGroup(Props) { position = \"1 2 3\"; rotation = \"0.3 -0.2 0.9 71.5\"; };\r\n\
};\r\n\
//--- OBJECT WRITE END ---\r\n";
const KITCHEN: &str = "new SimGroup(MissionGroup) {\n\
   new Sun() { azimuth = \"17.25\"; elevation = \"61\"; };\n\
   new SimGroup(Props) { rotation = \"0 0 -1 123.4\"; };\n\
};\n";

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A v20 install at `root`: one map as a loose folder, one inside a zip.
/// `reverse` writes every file in the opposite order.
fn install(root: &Path, reverse: bool) -> Result<()> {
    std::fs::create_dir_all(root.join("base"))?;
    std::fs::create_dir_all(root.join("Add-Ons/Map_Bedroom"))?;
    let loose = || std::fs::write(root.join("Add-Ons/Map_Bedroom/bedroom.mis"), BEDROOM);
    let zipped = || -> Result<()> {
        let mut zip =
            zip::ZipWriter::new(std::fs::File::create(root.join("Add-Ons/Map_Kitchen.zip"))?);
        // Fixed timestamps: the zip is the same file on both "machines".
        let options =
            zip::write::SimpleFileOptions::default().last_modified_time(zip::DateTime::default());
        zip.start_file("kitchen.mis", options)?;
        zip.write_all(KITCHEN.as_bytes())?;
        zip.finish()?;
        Ok(())
    };
    if reverse {
        zipped()?;
        loose()?;
    } else {
        loose()?;
        zipped()?;
    }
    Ok(())
}

fn run(program: &str, args: &[&Path], missions: &[&str]) -> Result<()> {
    let output = Command::new(program)
        .args(args)
        .args(missions)
        .output()
        .with_context(|| format!("running {program}"))?;
    ensure!(
        output.status.success(),
        "{program} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// Convert and bundle the install at `root`, returning the bundle's package hash.
fn import(root: &Path, work: &Path, missions: &[&str]) -> Result<String> {
    let converted = work.join("converted");
    let bundle = work.join("bundle");
    std::fs::create_dir_all(work)?;
    run(env!("CARGO_BIN_EXE_bri-convert"), &[root, &converted], &[])?;
    run(
        env!("CARGO_BIN_EXE_map_bundle"),
        &[root, &converted, &bundle],
        missions,
    )?;
    // The environment's own hash, as a joining player's is compared.
    hash_dir(&bundle)
}

/// `bri_package::environment::hash_dir`, restated so this crate need not
/// depend on the package crate: sorted relative paths, lengths and digests.
fn hash_dir(root: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)?
                .to_str()
                .context("UTF-8 name")?
                .replace('\\', "/");
            files.push((relative, std::fs::read(entry.path())?));
        }
    }
    files.sort();
    let mut total = Sha256::new();
    for (relative, bytes) in files {
        total.update(relative.as_bytes());
        total.update([0]);
        total.update((bytes.len() as u64).to_le_bytes());
        total.update(Sha256::digest(&bytes));
    }
    Ok(format!("{:x}", total.finalize()))
}

#[test]
fn map_bundle_is_identical_across_install_paths_and_orders() -> Result<()> {
    let base = std::env::temp_dir().join(format!(
        "bri-import-determinism-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let fixture = Fixture(base.clone());
    let first = fixture.0.join("a/Blockland v20");
    let second = fixture.0.join("elsewhere/Games (copy)/v20");
    install(&first, false)?;
    install(&second, true)?;
    let bedroom = "Add-Ons/Map_Bedroom/bedroom.mis";
    let kitchen = "Add-Ons/Map_Kitchen/kitchen.mis";
    let one = import(&first, &base.join("out-1"), &[bedroom, kitchen])?;
    let two = import(&second, &base.join("out-2"), &[kitchen, bedroom])?;
    assert_eq!(one, two, "the same content imported differently");

    // The sun directions come from portable sines: these bits are the same
    // on every CPU and C runtime.
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(base.join("out-1/bundle/bundle.json"))?)?;
    let maps: Vec<&str> = bundle["maps"]
        .as_array()
        .context("maps")?
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert_eq!(
        maps,
        [
            "v20/add-ons/map_bedroom/bedroom.mis",
            "v20/add-ons/map_kitchen/kitchen.mis"
        ]
    );
    let sun = bri_convert::scene_lighting::suns(&bri_convert::mission::read(BEDROOM, bedroom)?)?[0];
    assert_eq!(
        sun.source_direction.to_array().map(f32::to_bits),
        [0x3F36_135C, 0x3EE3_8C0A, 0xBF0B_6D77],
        "{:?}",
        sun.source_direction
    );
    Ok(())
}
