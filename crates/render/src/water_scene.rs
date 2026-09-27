//! Native water meshes and depth masks; wave/texture motion stays on the GPU.
use crate::scene::*;
use anyhow::Result;
use bri_content::water::Water;

pub fn append(
    out: &mut SceneData,
    water: &Water,
    textures: [usize; 3],
    terrain_height: impl Fn(f32, f32) -> Option<f32>,
) -> Result<()> {
    water.validate()?;
    let width = water.max[0] - water.min[0];
    let depth = water.max[2] - water.min[2];
    let side = 256u32;
    let mut rgba = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            let wx = water.min[0] + (x as f32 + 0.5) / side as f32 * width;
            let wz = water.max[2] - (y as f32 + 0.5) / side as f32 * depth;
            let opacity = if let Some(height) = terrain_height(wx, wz) {
                if height > water.max[1] + water.wave_amplitude * 0.5 {
                    [0.0; 2]
                } else {
                    water.depth_opacity(water.max[1] - height)
                }
            } else if water.depth_mask {
                [water.depth_alpha[1].clamp(0.0, 1.0), 0.0]
            } else {
                [water.opacity.clamp(0.0, 1.0), 0.0]
            };
            rgba.extend([
                (opacity[0] * 255.).round() as u8,
                (opacity[1] * 255.).round() as u8,
                0,
                255,
            ]);
        }
    }
    let mask = out.images.len();
    out.images.push(SceneImage {
        label: format!("{}/depth", water.id),
        width: side,
        height: side,
        rgba,
        srgb: false,
    });
    let mut material = Material::surface(&water.id, textures[0], mask);
    material.kind = MaterialKind::Water;
    material.alpha = AlphaMode::Blend;
    material.double_sided = true;
    material.images[1] = textures[1];
    material.images[2] = textures[2];
    material.parameters = Some([
        [
            water.flow[0],
            water.flow[1],
            water.wave_amplitude,
            water.opacity.clamp(0., 1.),
        ],
        [
            water.distortion[0],
            water.distortion[1],
            water.distortion[2],
            u8::from(water.depth_mask) as f32,
        ],
        [
            water.tiles[0],
            water.tiles[1],
            if water.reflection.is_some() {
                water.reflection_intensity.clamp(0., 1.)
            } else {
                0.
            },
            water.parallax,
        ],
    ]);
    let material_index = out.materials.len();
    out.materials.push(material);
    let columns = (width / 16.).ceil().clamp(1., 128.) as u32;
    let rows = (depth / 16.).ceil().clamp(1., 128.) as u32;
    let copies = if water.repeat_period.is_some() {
        -1..=1
    } else {
        0..=0
    };
    for repeat_z in copies.clone() {
        for repeat_x in copies.clone() {
            let ox = repeat_x as f32 * water.repeat_period.unwrap_or(0.);
            let oz = repeat_z as f32 * water.repeat_period.unwrap_or(0.);
            let base = out.vertices.len() as u32;
            for y in 0..=rows {
                for x in 0..=columns {
                    let uv = [x as f32 / columns as f32, y as f32 / rows as f32];
                    out.vertices.push(SceneVertex {
                        position: [
                            water.min[0] + width * uv[0] + ox,
                            water.max[1],
                            water.max[2] - depth * uv[1] + oz,
                        ],
                        normal: [0., 1., 0.],
                        uv,
                        lightmap_uv: uv,
                        color: [1.; 4],
                    });
                }
            }
            // Small batches let transparency ordering distinguish overlapping water
            // and ground layers; no single huge center for an entire repeated ocean.
            for y in 0..rows {
                let start = out.indices.len() as u32;
                for x in 0..columns {
                    let a = base + y * (columns + 1) + x;
                    let b = a + 1;
                    let c = a + columns + 1;
                    let d = c + 1;
                    out.indices.extend([a, b, d, a, d, c]);
                }
                out.batches.push(MeshBatch {
                    indices: start..out.indices.len() as u32,
                    material: material_index,
                    center: [
                        water.min[0] + width * 0.5 + ox,
                        water.max[1],
                        water.max[2] - depth * (y as f32 + 0.5) / rows as f32 + oz,
                    ],
                });
            }
        }
    }
    out.omissions
        .extend(water.warnings.iter().map(|w| format!("{}: {w}", water.id)));
    Ok(())
}
