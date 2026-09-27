//! Bounded geometry verification: native JSON only, no Torque reader or window.
use anyhow::{Context, Result, ensure};
use bri_content::brick::Brick;
use glam::Vec3;
use serde::Deserialize;
use std::{fs::File, io::BufWriter, path::PathBuf};

use bri_render::PreviewVertex as GpuVertex;

#[derive(Deserialize)]
struct Manifest {
    records: Vec<Record>,
}
#[derive(Deserialize)]
struct Record {
    virtual_path: String,
    output: Option<String>,
}

fn main() -> Result<()> {
    pollster::block_on(run())
}

async fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 2,
        "Usage: brick_preview <conversion-directory> <output-directory>"
    );
    let content = PathBuf::from(&args[0]);
    let output = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&output)?;
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(content.join("manifest.json"))?)?;
    let choices = [
        "bricks/2x4.blb",
        "ramps/2x2x5ramp.blb",
        "special/4x1x5window.blb",
        "special/4x1x2Fence.blb",
        "special/pineTree.blb",
        "rounds/2x2round.blb",
        "bricks/1x1Print.blb",
        "special/2x2x5girder.blb",
    ];
    let mut vertices = Vec::new();
    let mut objects = Vec::new();
    let camera = glam::camera::rh::proj::directx::perspective(40_f32.to_radians(), 1.0, 0.1, 20.0)
        * glam::camera::rh::view::look_at_mat4(Vec3::new(3.0, 2.2, -4.0), Vec3::ZERO, Vec3::Y);
    for (cell, choice) in choices.iter().enumerate() {
        let name = format!("base/data/bricks/{choice}");
        let record = manifest
            .records
            .iter()
            .find(|r| r.virtual_path.eq_ignore_ascii_case(&name))
            .with_context(|| format!("Missing {name}"))?;
        let filename = record
            .output
            .as_ref()
            .with_context(|| format!("Failed conversion for {name}"))?;
        ensure!(
            std::path::Path::new(filename).components().count() == 1
                && !filename.contains(['/', '\\', ':']),
            "Unsafe native asset path"
        );
        let brick: Brick = serde_json::from_slice(&std::fs::read(content.join(filename))?)?;
        brick.validate()?;
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in brick.quads.iter().flat_map(|q| q.vertices.iter()) {
            min = min.min(Vec3::from(vertex.position));
            max = max.max(Vec3::from(vertex.position));
        }
        let center = (min + max) * 0.5;
        let scale = 2.5 / (max - min).max_element();
        ensure!(scale.is_finite(), "Empty bounds");
        let tint = [
            [0.9, 0.22, 0.13, 1.0],
            [0.85, 0.65, 0.12, 1.0],
            [0.75, 0.85, 0.95, 1.0],
            [0.8, 0.8, 0.82, 1.0],
            [0.2, 0.7, 0.25, 1.0],
            [0.2, 0.5, 0.9, 1.0],
            [0.8, 0.4, 0.2, 1.0],
            [0.65, 0.65, 0.7, 1.0],
        ][cell];
        for quad in &brick.quads {
            for index in [0, 1, 2, 0, 2, 3] {
                let vertex = quad.vertices[index];
                let mut clip =
                    camera * ((Vec3::from(vertex.position) - center) * scale).extend(1.0);
                clip.x = clip.x / 4.0 + ((cell % 4) as f32 * 0.5 - 0.75) * clip.w;
                clip.y = clip.y / 2.0 + (0.5 - (cell / 4) as f32) * clip.w;
                let color = quad.colors.map_or(tint, |colors| colors[index]);
                vertices.push(GpuVertex {
                    position: clip.to_array(),
                    normal: vertex.normal,
                    color,
                });
            }
        }
        objects.push(serde_json::json!({"cell":cell,"source":name,"quads":brick.quads.len(),"collision_boxes":brick.collision_boxes.len(),"external_collision_pending":brick.needs_external_collision}));
    }
    const WIDTH: u32 = 1024;
    const HEIGHT: u32 = 512;
    let preview = bri_render::render_geometry(&vertices, WIDTH, HEIGHT).await?;
    let pixels = preview.pixels;
    let mut occupancy = vec![0; 8];
    let background = &pixels[..4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let at = ((y * WIDTH + x) * 4) as usize;
            if &pixels[at..at + 4] != background {
                occupancy[((y / 256) * 4 + x / 256) as usize] += 1;
            }
        }
    }
    ensure!(
        occupancy.iter().all(|n| *n > 300),
        "Empty/near-empty mesh cell: {occupancy:?}"
    );
    let mut png = png::Encoder::new(
        BufWriter::new(File::create(output.join("native-bricks.png"))?),
        WIDTH,
        HEIGHT,
    );
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    png.write_header()?.write_image_data(&pixels)?;
    let report = serde_json::json!({"status":"passed","adapter":preview.adapter,"vertices":vertices.len(),"cell_pixels":occupancy,"objects":objects,
        "scope":"geometry, winding, transformed normals, authored colors and native-only loading; stock textures/prints and sorted transparency not implemented in this preview","visible_window_created":false});
    std::fs::write(
        output.join("native-bricks.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "Native brick render passed for 8 assets: {}",
        output.join("native-bricks.png").display()
    );
    Ok(())
}
