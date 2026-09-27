//! Full-weight native terrain rendering; no dominant-layer approximation.
use anyhow::{Context, Result, ensure};
use bri_content::{
    Terrain,
    scene::{Kind, Scene},
};
use bri_render::textured::{Draw, TextureImage, TextureVertex};
use glam::{Mat4, Vec3};
use std::path::PathBuf;
fn decode(bytes: &[u8], srgb: bool) -> Result<TextureImage> {
    let image = image::load_from_memory(bytes)?.to_rgba8();
    let (width, height) = image.dimensions();
    Ok(TextureImage {
        rgba: image.into_raw(),
        width,
        height,
        srgb,
    })
}
fn main() -> Result<()> {
    pollster::block_on(run())
}
async fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 2,
        "Usage: terrain_preview <native-map-bundle> <output-dir>"
    );
    let root = PathBuf::from(&args[0]);
    let output = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&output)?;
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("bundle.json"))?)?;
    let map = bundle["maps"]
        .as_array()
        .context("Missing maps")?
        .iter()
        .find(|m| m["id"].as_str().is_some_and(|s| s.ends_with("/slopes.mis")))
        .context("Missing Slopes scene")?;
    let scene: Scene =
        serde_json::from_slice(&std::fs::read(root.join(map["file"].as_str().unwrap()))?)?;
    let (node_index, node) = scene
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| matches!(n.kind, Kind::Terrain))
        .context("Missing terrain placement")?;
    let id = node.asset.as_ref().context("Missing terrain ID")?;
    let terrain: Terrain = serde_json::from_slice(&std::fs::read(
        root.join(bundle["assets"][id].as_str().unwrap()),
    )?)?;
    let spacing: f32 = node
        .properties
        .get("squaresize")
        .context("Missing grid spacing")?
        .parse()?;
    let transform = Mat4::from_cols_array(&node.transform);
    let region = [-64, -64, 384, 384];
    let mesh = bri_content::terrain_mesh::mesh(&terrain, spacing, region)?;
    let mut images = vec![TextureImage {
        rgba: vec![255; 4],
        width: 1,
        height: 1,
        srgb: true,
    }];
    let mut indices = [0_usize; 11];
    for binding in bundle["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["asset"].as_str() == Some(id))
    {
        let slot = binding["terrain_layer"]
            .as_u64()
            .context("Missing layer slot")? as usize;
        ensure!(slot < 8, "Invalid terrain slot");
        indices[slot] = images.len();
        images.push(decode(
            &std::fs::read(root.join(binding["texture"].as_str().context("Missing layer image")?))?,
            true,
        )?);
    }
    let light = bundle["lighting"][&scene.id]["terrain"]
        .as_array()
        .context("Missing native terrain lightmap")?
        .iter()
        .find(|l| l["node"].as_u64() == Some(node_index as u64))
        .context("Unbound terrain lightmap")?;
    indices[8] = images.len();
    images.push(decode(
        &std::fs::read(root.join(light["file"].as_str().unwrap()))?,
        false,
    )?);
    for group in 0..2 {
        let mut rgba = vec![0; terrain.side as usize * terrain.side as usize * 4];
        for layer in terrain
            .layers
            .iter()
            .filter(|l| l.slot as usize / 4 == group)
        {
            for (i, w) in layer.weights.iter().enumerate() {
                rgba[i * 4 + layer.slot as usize % 4] = *w;
            }
        }
        indices[9 + group] = images.len();
        images.push(TextureImage {
            rgba,
            width: terrain.side,
            height: terrain.side,
            srgb: false,
        });
    }
    let spawn = scene
        .nodes
        .iter()
        .find(|n| matches!(n.kind, Kind::Spawn))
        .context("Missing spawn")?;
    let spawn = Mat4::from_cols_array(&spawn.transform).transform_point3(Vec3::ZERO);
    let cameras = [
        (
            spawn + Vec3::new(0.0, 15.0, 0.0),
            Vec3::new(-50.0, 400.0, -250.0),
        ),
        (
            Vec3::new(1250.0, 1500.0, 1200.0),
            Vec3::new(0.0, 220.0, 0.0),
        ),
    ];
    let mut vertices = Vec::new();
    let mut draws = Vec::new();
    for (cell, (eye, target)) in cameras.iter().enumerate() {
        let camera =
            glam::camera::rh::proj::directx::perspective(70_f32.to_radians(), 1.0, 0.1, 7000.0)
                * glam::camera::rh::view::look_at_mat4(*eye, *target, Vec3::Y);
        let start = vertices.len() as u32;
        for triangle in &mesh.triangles {
            for index in triangle {
                let i = *index as usize;
                let mut clip = camera * transform * Vec3::from(mesh.positions[i]).extend(1.0);
                clip.x = clip.x * 0.5 + (cell as f32 - 0.5) * clip.w;
                // Classic TGE: 256 cells / eight cells per source texture.
                let uv = mesh.grid_uv[i];
                vertices.push(TextureVertex {
                    position: clip.to_array(),
                    uv: [uv[0] * 32.0, uv[1] * 32.0],
                    lightmap_uv: uv,
                });
            }
        }
        draws.push(Draw {
            start,
            count: vertices.len() as u32 - start,
            diffuse: 0,
            lightmap: 0,
            scissor: [cell as u32 * 768, 0, 768, 768],
            terrain_images: Some(indices),
        });
    }
    let result =
        bri_render::textured::render_terrain(&vertices, &images, &draws, 1536, 768).await?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(std::fs::File::create(output.join("slopes.png"))?),
        1536,
        768,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&result.pixels)?;
    std::fs::write(
        output.join("slopes.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"adapter":result.adapter,"map":scene.name,"native_origin":transform.transform_point3(Vec3::ZERO).to_array(),"spacing":spacing,"region":region,"triangles_per_view":mesh.triangles.len(),"layers":terrain.layers.len(),"native_cache_lightmap":light["file"],"scope":"periodic native terrain, all authored blend weights and original cached lighting; sky/fog/water/snow/detail bump and production terrain LOD pending"}),
        )?,
    )?;
    println!(
        "Slopes terrain: {} triangles per view, {} blended layers",
        mesh.triangles.len(),
        terrain.layers.len()
    );
    Ok(())
}
