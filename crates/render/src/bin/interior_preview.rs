//! Native-only, textured offscreen map inspection. No original game access.
use anyhow::{Context, Result, ensure};
use bri_content::{
    interior::Interior,
    scene::{Kind, Scene},
};
use bri_render::textured::{Draw, TextureImage, TextureVertex};
use glam::{Mat4, Vec3};
use std::{collections::BTreeMap, path::PathBuf};
fn decode(bytes: &[u8], srgb: bool) -> Result<TextureImage> {
    let image = image::load_from_memory(bytes)?.to_rgba8();
    let (width, height) = image.dimensions();
    ensure!(
        width <= 4096 && height <= 4096,
        "Image dimensions exceed preview limit"
    );
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
        "Usage: interior_preview <native-map-bundle> <output-dir>"
    );
    let root = PathBuf::from(&args[0]);
    let out = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&out)?;
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("bundle.json"))?)?;
    let mut images = vec![TextureImage {
        rgba: vec![255; 4],
        width: 1,
        height: 1,
        srgb: false,
    }];
    let mut texture_indices = BTreeMap::new();
    let mut vertices = Vec::new();
    let mut draws = Vec::new();
    let mut report = Vec::new();
    for (cell, map) in bundle["maps"]
        .as_array()
        .context("Missing maps")?
        .iter()
        .take(2)
        .enumerate()
    {
        let scene: Scene = serde_json::from_slice(&std::fs::read(
            root.join(map["file"].as_str().context("Missing scene file")?),
        )?)?;
        let spawn = scene
            .nodes
            .iter()
            .find(|n| matches!(n.kind, Kind::Spawn))
            .context("Missing spawn")?;
        let spawn = Mat4::from_cols_array(&spawn.transform).transform_point3(Vec3::ZERO);
        let eye = if cell == 0 {
            spawn + Vec3::new(10.0, 45.0, -15.0)
        } else {
            spawn + Vec3::new(25.0, 55.0, -10.0)
        };
        let target = if cell == 0 {
            Vec3::new(0.0, 335.0, 165.0)
        } else {
            Vec3::new(-420.0, 175.0, 80.0)
        };
        let camera =
            glam::camera::rh::proj::directx::perspective(85_f32.to_radians(), 1.0, 0.05, 2000.0)
                * glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y);
        let mut face_count = 0;
        for node in scene
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, Kind::Interior))
        {
            let id = node.asset.as_ref().context("Interior missing asset")?;
            let file = bundle["assets"][id]
                .as_str()
                .context("Interior missing native file")?;
            let interior: Interior = serde_json::from_slice(&std::fs::read(root.join(file))?)?;
            interior.validate()?;
            let detail = &interior.details[0];
            let transform = Mat4::from_cols_array(&node.transform);
            let mut lightmaps = Vec::new();
            let node_index = scene
                .nodes
                .iter()
                .position(|n| std::ptr::eq(n, node))
                .unwrap();
            let baked = bundle["lighting"][&scene.id]["interiors"]
                .as_array()
                .context("Missing native baked interior lighting")?;
            for (slot, lm) in detail.lightmaps.iter().enumerate() {
                lightmaps.push(images.len());
                let replacement = baked.iter().find(|b| {
                    b["node"].as_u64() == Some(node_index as u64)
                        && b["detail"].as_u64() == Some(0)
                        && b["slot"].as_u64() == Some(slot as u64)
                });
                images.push(if let Some(record) = replacement {
                    decode(
                        &std::fs::read(
                            root.join(record["file"].as_str().context("Missing baked filename")?),
                        )?,
                        false,
                    )?
                } else {
                    decode(&lm.png, false)?
                });
            }
            let mut material_images = BTreeMap::new();
            for binding in bundle["bindings"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|b| b["asset"].as_str() == Some(id) && b["detail"].as_u64() == Some(0))
            {
                let file = binding["texture"]
                    .as_str()
                    .context("Missing original texture")?;
                let index = if let Some(&index) = texture_indices.get(file) {
                    index
                } else {
                    let index = images.len();
                    images.push(decode(&std::fs::read(root.join(file))?, true)?);
                    texture_indices.insert(file.to_owned(), index);
                    index
                };
                material_images.insert(binding["material"].as_u64().unwrap() as usize, index);
            }
            // Group by texture pair to keep this inspection representative of batching.
            let mut groups: BTreeMap<(usize, usize), Vec<TextureVertex>> = BTreeMap::new();
            for surface in &detail.surfaces {
                if surface.triangles.is_empty() {
                    continue;
                }
                let diffuse = *material_images
                    .get(&surface.material)
                    .context("Unresolved surface material")?;
                let lightmap = surface.lightmap.map_or(0, |i| lightmaps[i]);
                let group = groups.entry((diffuse, lightmap)).or_default();
                for triangle in &surface.triangles {
                    for index in triangle {
                        let v = &surface.vertices[*index as usize];
                        let mut position = camera * transform * Vec3::from(v.position).extend(1.0);
                        position.x = position.x * 0.5 + (cell as f32 - 0.5) * position.w;
                        group.push(TextureVertex {
                            position: position.to_array(),
                            uv: v.uv,
                            lightmap_uv: v.lightmap_uv,
                        });
                    }
                }
                face_count += surface.triangles.len();
            }
            for ((diffuse, lightmap), group) in groups {
                draws.push(Draw {
                    start: vertices.len() as u32,
                    count: group.len() as u32,
                    diffuse,
                    lightmap,
                    scissor: [cell as u32 * 768, 0, 768, 768],
                    terrain_images: None,
                });
                vertices.extend(group);
            }
        }
        report.push(serde_json::json!({"map":scene.name,"camera":eye.to_array(),"target":target.to_array(),"triangles":face_count}));
    }
    let started = std::time::Instant::now();
    let result =
        bri_render::textured::render_textured(&vertices, &images, &draws, 1536, 768).await?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(std::fs::File::create(out.join("interiors.png"))?),
        1536,
        768,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&result.pixels)?;
    std::fs::write(
        out.join("interiors.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"maps":report,"textures":images.len(),"draws":draws.len(),"adapter":result.adapter,"elapsed_ms_including_upload_readback":started.elapsed().as_millis(),"scope":"native interior geometry, original diffuse textures and offline-composed mission lighting; no terrain, sky, decorations, dynamic lights or gameplay"}),
        )?,
    )?;
    println!(
        "Textured interior preview: {} triangles, {} texture images, {} draws",
        vertices.len() / 3,
        images.len(),
        draws.len()
    );
    Ok(())
}
