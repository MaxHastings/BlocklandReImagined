//! Temporary tool (throwaway): writes the shared placeholder-cube `.glb` the
//! importer gives to items whose model glTF cannot hold (no visible mesh) and
//! prints its sha256 — so migrated content can carry byte-identical stand-ins
//! without re-importing.
//!
//! Usage: `export-placeholder OUT.glb`
//!
//! TEMPORARY TOOL: delete this file and its `[[bin]]` entry once no package
//! needs a fresh placeholder cube.

use std::collections::BTreeMap;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        eprintln!("usage: export-placeholder OUT.glb");
        return ExitCode::from(2);
    }
    match run(&args[0]) {
        Ok(hash) => {
            println!("{hash}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("export-placeholder: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(out: &str) -> anyhow::Result<String> {
    use bri_addon_import::{gltf, source::hash};

    let shape = bri_addon_import::placeholder();
    // One material whose bitmap is embedded in the file (as the importer's),
    // so it needs no texture beside it.
    let mut white_png = vec![];
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]))
        .write_to(&mut std::io::Cursor::new(&mut white_png), image::ImageFormat::Png)
        .expect("in-memory PNG");
    let mut notes = vec![];
    let (glb, _) = gltf::export_shape(
        &shape,
        &BTreeMap::from([(0usize, Some(("placeholder:white".to_owned(), white_png)))]),
        &mut notes,
    )?;
    for note in &notes {
        eprintln!("note: {note}");
    }
    std::fs::write(out, &glb)?;
    Ok(hash(&glb))
}
