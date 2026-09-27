//! Offscreen native-model pose evidence, with no source-format dependency.
use anyhow::{Context, Result, ensure};
use bri_content::{
    animation::{sample, triangles},
    shape::{ClipSet, Shape},
};
use bri_render::PreviewVertex;
use glam::Vec3;
use std::{
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
};

fn load<T: serde::de::DeserializeOwned>(
    root: &Path,
    records: &[serde_json::Value],
    source: &str,
) -> Result<T> {
    let record = records
        .iter()
        .find(|r| {
            r["virtual_path"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(source))
        })
        .with_context(|| format!("Missing asset {source}"))?;
    let output = record["output"]
        .as_str()
        .with_context(|| format!("Asset failed conversion: {source}: {}", record["error"]))?;
    ensure!(!output.contains(['/', '\\', ':']), "Invalid content path");
    Ok(serde_json::from_slice(&std::fs::read(root.join(output))?)?)
}
fn main() -> Result<()> {
    pollster::block_on(run())
}
async fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 2,
        "Usage: model_preview <conversion-dir> <output-dir>"
    );
    let root = PathBuf::from(&args[0]);
    let output = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&output)?;
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json"))?)?;
    let records = manifest["records"].as_array().context("Missing records")?;
    let player: Shape = load(&root, records, "base/data/shapes/player/m.dts")?;
    player.validate()?;
    let run: ClipSet = load(&root, records, "base/data/shapes/player/m_run.dsq")?;
    let crouch: ClipSet = load(&root, records, "base/data/shapes/player/m_crouch.dsq")?;
    let jeep: Shape = load(&root, records, "Add-Ons/Vehicle_Jeep/jeep.dts")?;
    jeep.validate()?;
    let tire: Shape = load(&root, records, "Add-Ons/Vehicle_Jeep/jeeptire.dts")?;
    tire.validate()?;
    let cases = [
        (&player, None, 0.0, "player bind"),
        (&player, Some(&run.animations[0]), 0.0, "player run A"),
        (
            &player,
            Some(&run.animations[0]),
            run.animations[0].duration * 0.25,
            "player run B",
        ),
        (
            &player,
            Some(&crouch.animations[0]),
            crouch.animations[0].duration * 0.5,
            "player crouch",
        ),
        (&jeep, None, 0.0, "Jeep body"),
        (&tire, None, 0.0, "Jeep tire"),
    ];
    let visible = [
        "headskin", "chest", "pants", "rarm", "larm", "rhand", "lhand", "rshoe", "lshoe",
    ];
    let mut animation_checks = Vec::new();
    for record in records.iter().filter(|r| {
        r["virtual_path"]
            .as_str()
            .is_some_and(|p| p.starts_with("base/data/shapes/player/") && p.ends_with(".dsq"))
    }) {
        let source = record["virtual_path"].as_str().unwrap();
        let clips: ClipSet = load(&root, records, source)?;
        for clip in &clips.animations {
            let unbound: Vec<_> = clip
                .nodes
                .iter()
                .filter(|t| {
                    !player
                        .nodes
                        .iter()
                        .any(|n| n.name.eq_ignore_ascii_case(&t.node))
                })
                .map(|t| t.node.clone())
                .collect();
            // These four old files are present on disk but absent from the stock
            // mDts constructor. Do not invent aliases for their obsolete bones.
            let unused = [
                "m_armready.dsq",
                "m_boot.dsq",
                "m_jump.dsq",
                "m_visorup.dsq",
            ]
            .iter()
            .any(|name| source.ends_with(name));
            ensure!(
                unused || unbound.is_empty(),
                "Stock-used animation has unbound nodes: {source}: {unbound:?}"
            );
            for fraction in [0.0, 0.125, 0.5, 0.875, 1.0, 1.125] {
                let pose = sample(&player, Some(clip), clip.duration * fraction)?;
                ensure!(
                    pose.nodes.iter().all(|m| m.is_finite()),
                    "Non-finite player pose in {source}"
                );
            }
            animation_checks.push(serde_json::json!({"source":source,"name":clip.name,"frames":clip.frames,"samples":6,"unbound_nodes":unbound,"used_by_stock_constructor":!unused}));
        }
    }
    let camera = glam::camera::rh::proj::directx::perspective(40_f32.to_radians(), 1.0, 0.1, 30.0)
        * glam::camera::rh::view::look_at_mat4(Vec3::new(3.0, 2.0, -5.0), Vec3::ZERO, Vec3::Y);
    let mut vertices = Vec::new();
    let mut report = Vec::new();
    let mut poses = Vec::new();
    for (cell, (shape, clip, time, label)) in cases.into_iter().enumerate() {
        let pose = sample(shape, clip, time)?;
        let detail = shape
            .details
            .iter()
            .position(|d| !d.collision)
            .context("No visible model detail")?;
        let geometry = triangles(shape, &pose, detail, |name| {
            cell >= 4 || visible.contains(&name.to_lowercase().as_str())
        })?;
        ensure!(!geometry.is_empty(), "No geometry for {label}");
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for v in geometry.iter().flat_map(|t| t.vertices.iter()) {
            min = min.min(v.position);
            max = max.max(v.position);
        }
        let center = (min + max) * 0.5;
        let scale = 2.8 / (max - min).max_element();
        for triangle in &geometry {
            let name = shape.objects[triangle.object].name.to_lowercase();
            let color = if cell >= 4 {
                [0.4, 0.6, 0.3, 1.0]
            } else if name.contains("head") || name.contains("hand") {
                [0.85, 0.65, 0.35, 1.0]
            } else if name.contains("chest") || name.contains("arm") {
                [0.15, 0.4, 0.8, 1.0]
            } else {
                [0.25, 0.2, 0.18, 1.0]
            };
            for vertex in &triangle.vertices {
                let mut clip = camera * ((vertex.position - center) * scale).extend(1.0);
                clip.x = clip.x / 3.0 + ((cell % 3) as f32 * 2.0 / 3.0 - 2.0 / 3.0) * clip.w;
                clip.y = clip.y / 2.0 + (0.5 - (cell / 3) as f32) * clip.w;
                vertices.push(PreviewVertex {
                    position: clip.to_array(),
                    normal: vertex.normal.to_array(),
                    color,
                });
            }
        }
        poses.push(
            geometry
                .iter()
                .flat_map(|t| t.vertices.iter().map(|v| v.position))
                .collect::<Vec<_>>(),
        );
        report.push(serde_json::json!({"cell":cell,"label":label,"time":time,"triangles":geometry.len(),"bounds":[min.to_array(),max.to_array()]}));
    }
    let motion = poses[1]
        .iter()
        .zip(&poses[2])
        .map(|(a, b)| a.distance(*b))
        .fold(0.0_f32, f32::max);
    ensure!(
        motion > 0.01,
        "Run animation failed to move native geometry"
    );
    let preview = bri_render::render_geometry(&vertices, 1152, 768).await?;
    let mut occupancy = [0_u32; 6];
    let background = &preview.pixels[..4];
    for y in 0..768 {
        for x in 0..1152 {
            let at = (y * 1152 + x) * 4;
            if &preview.pixels[at..at + 4] != background {
                occupancy[(y / 384) * 3 + x / 384] += 1;
            }
        }
    }
    ensure!(
        occupancy.iter().all(|n| *n > 500),
        "Empty pose preview: {occupancy:?}"
    );
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(output.join("native-models.png"))?),
        1152,
        768,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&preview.pixels)?;
    let report = serde_json::json!({"status":"passed","objects":report,"player_animation_checks":animation_checks,"adapter":preview.adapter,"max_run_vertex_motion":motion,"cell_pixels":occupancy,"visible_window_created":false,
        "scope":"native skeleton, posed geometry and animation sample check; debug colors, no stock textures/material fidelity or assembled vehicle wheels"});
    std::fs::write(
        output.join("native-models.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("Native models rendered; run animation maximum vertex displacement {motion:.4}");
    Ok(())
}
