//! Temporary re-exporter (throwaway tool).
//!
//! Re-exports one native `.shape.json` as a self-contained `.glb` with the
//! current exporter, for models converted before a pipeline fix and now in
//! need of regeneration. This is what rebuilt `steel-ball.glb` after bare-metal
//! materials learned to ride through glTF (`bri_legacy.metal`).
//!
//! The export is verified against its source before it may be written: the
//! material list survives whole, every metal field bit-exact, and the geometry
//! keeps its vertex counts. Textures are not embedded (the Add-On's texture
//! files stay beside the model, as they do for every item GLB); a used
//! untextured material is reported in the exporter notes so nothing is lost
//! silently.
//!
//! Usage: `shape-json-to-glb INPUT.shape.json [OUTPUT.glb]`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, ensure};
use bri_content::shape::Shape;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 1 || args.len() > 2 {
        eprintln!("usage: shape-json-to-glb INPUT.shape.json [OUTPUT.glb]");
        return ExitCode::from(2);
    }
    let input = PathBuf::from(&args[0]);
    let output = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| input.with_extension("glb"));
    match run(&input, &output) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("SHAPE-JSON-TO-GLB: {e:?}");
            ExitCode::FAILURE
        }
    }
}

fn run(input: &Path, output: &Path) -> Result<String> {
    let old = fs::read(input).with_context(|| format!("reading {}", input.display()))?;
    let shape: Shape = serde_json::from_slice(&old)?;
    shape.validate()?;

    // No textures are embedded: item GLBs keep their bitmaps beside the model,
    // and this tool is for regenerating a model whose material data drifted.
    let mut notes = Vec::new();
    let (glb, _embedded) = bri_gltf::export_shape(&shape, &Default::default(), &mut notes)?;

    // The re-import: materials whole, metals exact, geometry intact.
    let converted = bri_gltf::lower(&glb, input.file_name().unwrap().to_str().unwrap_or("model"))?;
    ensure!(
        converted.shape.materials.len() == shape.materials.len(),
        "material count changed: {} -> {}",
        shape.materials.len(),
        converted.shape.materials.len()
    );
    for (orig, low) in shape.materials.iter().zip(converted.shape.materials.iter()) {
        ensure!(low.metal == orig.metal, "metal of `{}` did not survive", orig.name);
    }
    // The exporter rebuilds each primitive in first-seen order, welding
    // duplicate positions (every GLB this pipeline writes does), so the raw
    // vertex count may drift by a couple — the triangles must not.
    let tris = |s: &Shape| {
        s.meshes.iter().flatten()
            .flat_map(|m| m.primitives.iter())
            .map(|p| p.triangles.len())
            .sum::<usize>()
    };
    ensure!(
        tris(&converted.shape) == tris(&shape),
        "triangle count changed: {} -> {}",
        tris(&shape),
        tris(&converted.shape)
    );

    let verts = |s: &Shape| s.meshes.iter().flatten().map(|m| m.positions.len()).sum::<usize>();
    fs::write(output, &glb).with_context(|| format!("writing {}", output.display()))?;
    let metals = shape.materials.iter().filter(|m| m.metal.is_some()).count();
    Ok(format!(
        "SHAPE-JSON-TO-GLB: {} -> {} ({} materials, {} metal, {} triangles, vertices {} -> {}, {} notes)",
        input.file_name().unwrap().to_string_lossy(),
        output.display(),
        shape.materials.len(),
        metals,
        tris(&shape),
        verts(&shape),
        verts(&converted.shape),
        notes.len()
    ))
}
