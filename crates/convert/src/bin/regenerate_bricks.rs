//! Copy a converted geometry pack to a new pack, re-emitting every generated
//! (`BRICK`) mesh with the current converter. SPECIAL meshes, which carry
//! authored quads, and all other assets are copied byte for byte.
use anyhow::{Context, Result, ensure};
use std::path::PathBuf;

#[derive(serde::Deserialize)]
struct Source {
    geometry: String,
    original_text: String,
}

fn main() -> Result<()> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        args.len() == 2,
        "Usage: regenerate_bricks <input-pack> <new-output-pack>"
    );
    let (input, output) = (&args[0], &args[1]);
    ensure!(
        !output.exists(),
        "Refusing existing output {}",
        output.display()
    );
    ensure!(
        input.join("manifest.json").is_file(),
        "Input is not a converted pack"
    );
    std::fs::create_dir(output)?;
    let mut regenerated = 0;
    for entry in std::fs::read_dir(input)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .ok()
            .context("Non-UTF-8 name")?;
        ensure!(entry.file_type()?.is_file(), "Unexpected directory {name}");
        let target = output.join(&name);
        let Some(key) = name.strip_suffix(".brick.json") else {
            std::fs::copy(entry.path(), target)?;
            continue;
        };
        let source: Source =
            serde_json::from_slice(&std::fs::read(input.join(format!("{key}.source.json")))?)?;
        if source.geometry != "BRICK" {
            std::fs::copy(entry.path(), target)?;
            continue;
        }
        let old: bri_content::brick::Brick = serde_json::from_slice(&std::fs::read(entry.path())?)?;
        let (brick, _) = bri_convert::brick::read(source.original_text.as_bytes(), old.id)?;
        std::fs::write(target, serde_json::to_vec(&brick)?)?;
        regenerated += 1;
    }
    println!("Regenerated {regenerated} generated brick meshes");
    Ok(())
}
