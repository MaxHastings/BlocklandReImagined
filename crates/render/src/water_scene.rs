//! Native water meshes and depth masks; wave/texture motion stays on the GPU.
use crate::scene::*;
use anyhow::Result;
use bri_content::water::Water;

/// `specular` is the block's authored `specularColor` and `specularPower`.
/// `terrain` says whether the map has a terrain at all: v20 builds depth
/// masks only from one (`GenerateDepthTextures` returns at once without it)
/// and a fresh `GBitmap` is filled with 0xFF, so on terrainless maps (the
/// Slate variants) both masks stay opaque white.
pub fn append(
    out: &mut SceneData,
    water: &Water,
    textures: [usize; 3],
    specular: ([f32; 4], f32),
    terrain: bool,
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
            let opacity = if !water.depth_mask {
                [water.opacity.clamp(0.0, 1.0), 0.0]
            } else if !terrain {
                [1.0; 2]
            } else if let Some(height) = terrain_height(wx, wz) {
                if height > water.max[1] + water.wave_amplitude * 0.5 {
                    [0.0; 2]
                } else {
                    water.depth_opacity(water.max[1] - height)
                }
            } else {
                // An empty terrain square writes 0x00FFFFFF: no water there.
                [0.0; 2]
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
        // The specular pass adds colour * pow(half.up, power) at alpha
        // colour.a * pow * depth mask: colour.rgb * colour.a * pow squared.
        [
            specular.0[0] * specular.0[3],
            specular.0[1] * specular.0[3],
            specular.0[2] * specular.0[3],
            specular.1,
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
                        // Texture coordinates run on across repeated copies, so
                        // the rotated second pass has no seam at their edges.
                        uv: [uv[0] + repeat_x as f32, uv[1] - repeat_z as f32],
                        lightmap_uv: uv,
                        color: [1.; 4],
                        fx: [0.; 4],
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

#[cfg(test)]
mod tests {
    use super::append;
    use crate::scene::{SceneData, translucent_order};
    use bri_content::water::Water;
    use glam::Vec3;

    fn sea() -> Water {
        // Slate Sea's ocean: UseDepthMask, MinAlpha 1, MaxAlpha 10, ShoreDepth 0.
        let mut water = Water::volume([0.0, -91.0, -2048.0], [2048.0, 9.0, 0.0]);
        water.depth_mask = true;
        water.depth_alpha = [1.0, 10.0, 0.0, 0.0];
        water.repeat_period = Some(2048.0);
        water.tiles = [50.0, 60.0];
        water
    }

    fn masks(water: &Water, terrain: bool) -> (Vec<u8>, Vec<u8>) {
        let mut scene = SceneData::default();
        let white = ([1.0; 4], 6.0);
        append(&mut scene, water, [0; 3], white, terrain, |_, _| None).unwrap();
        let image = scene.images.last().unwrap();
        (
            image.rgba.chunks(4).map(|p| p[0]).collect(),
            image.rgba.chunks(4).map(|p| p[1]).collect(),
        )
    }

    #[test]
    fn terrainless_depth_masks_stay_white_as_in_v20() {
        // No terrain: v20 never fills the 0xFF bitmap, so the surface and the
        // shore pass are both opaque (Slate Sea, Storm's dirt, Desert's sand).
        let (surface, shore) = masks(&sea(), false);
        assert!(surface.iter().chain(&shore).all(|&a| a == 255));
        // With a terrain, squares it does not cover hold no water at all.
        let (surface, shore) = masks(&sea(), true);
        assert!(surface.iter().chain(&shore).all(|&a| a == 0));
        // Without the depth mask it is the plain two-pass surface opacity.
        let mut plain = sea();
        plain.depth_mask = false;
        plain.opacity = 0.15;
        let (surface, shore) = masks(&plain, false);
        assert!(surface.iter().all(|&a| a == 38) && shore.iter().all(|&a| a == 0));
    }

    #[test]
    fn specular_colour_and_seamless_repeats_reach_the_shader() {
        let mut scene = SceneData::default();
        // Slate Sea's specularColor 0.7 0.6 0.55 0.9 and specularPower 0.7.
        let specular = ([0.7, 0.6, 0.55, 0.9], 0.7);
        append(&mut scene, &sea(), [0; 3], specular, false, |_, _| None).unwrap();
        let parameters = scene.materials[0].parameters.unwrap();
        let expected = [0.63, 0.54, 0.495, 0.7];
        let pairs = parameters[3].iter().zip(expected);
        assert!(
            pairs.clone().all(|(a, b)| (a - b).abs() < 1e-6),
            "{pairs:?}"
        );
        // Texture coordinates continue across the repeated copies: equal
        // positions on either side of a copy's edge share one coordinate.
        let mut seen = std::collections::HashMap::new();
        for v in &scene.vertices {
            let key = (v.position[0].to_bits(), v.position[2].to_bits());
            if let Some(uv) = seen.insert(key, v.uv) {
                assert_eq!(uv, v.uv, "seam at {:?}", v.position);
            }
            let x = (v.position[0] - 0.0) / 2048.0;
            let z = (0.0 - v.position[2]) / 2048.0;
            assert!((v.uv[0] - x).abs() < 1e-4 && (v.uv[1] - z).abs() < 1e-4);
        }
    }

    #[test]
    fn water_sorts_as_planes_so_the_sea_covers_its_sand_floor() {
        // Slate Sea: an opaque sand "water" layer just under the slate and the
        // sea surface 9 above it, each cut into strips. A far sea strip must
        // still draw after a near sand strip when the camera is above both.
        let eye = Vec3::new(0.0, 10.5, 0.0);
        let draws = [
            (Vec3::new(0.0, 9.0, -900.0), Some(9.0)),
            (Vec3::new(0.0, -0.238, -8.0), Some(-0.238)),
            (Vec3::new(0.0, 12.0, -30.0), None), // glass above the sea
            (Vec3::new(0.0, 3.0, -2.0), None),   // glass under the sea
            (Vec3::new(0.0, 9.0, -8.0), Some(9.0)),
            (Vec3::new(0.0, -0.238, -1500.0), Some(-0.238)),
        ];
        let order = translucent_order(eye, &draws);
        let at = |i: usize| order.iter().position(|&o| o == i).unwrap();
        for sand in [1, 5] {
            assert!(at(sand) < at(3), "sand before what lies above it");
            for sea in [0, 4] {
                assert!(at(sand) < at(sea), "{order:?}");
            }
        }
        for sea in [0, 4] {
            assert!(at(3) < at(sea), "submerged glass before the sea");
            assert!(at(sea) < at(2), "the sea before glass above it");
        }
        // Under the sea, looking up, the surface draws after what lies beyond it.
        let under = translucent_order(
            Vec3::new(0.0, 5.0, 0.0),
            &[
                (Vec3::new(0.0, 9.0, -8.0), Some(9.0)),
                (Vec3::new(0.0, 20.0, -40.0), None),
            ],
        );
        assert_eq!(under, [1, 0]);
    }
}
