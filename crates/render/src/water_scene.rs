//! Native water meshes and depth masks; wave/texture motion stays on the GPU.
use crate::scene::*;
use anyhow::{Context, Result, ensure};
use bri_console::Clamp;
use bri_content::terrain_field::TerrainField;
use bri_content::water::Water;
use glam::{Vec2, Vec3};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

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
    append_copy(
        out,
        water,
        textures,
        specular,
        terrain,
        terrain_height,
        [0, 0],
    )
}

fn append_copy(
    out: &mut SceneData,
    water: &Water,
    textures: [usize; 3],
    specular: ([f32; 4], f32),
    terrain: bool,
    terrain_height: impl Fn(f32, f32) -> Option<f32>,
    copy: [i32; 2],
) -> Result<()> {
    water.validate()?;
    let width = water.max[0] - water.min[0];
    let depth = water.max[2] - water.min[2];
    let [repeat_x, repeat_z] = copy;
    let ox = repeat_x as f32 * water.repeat_period.unwrap_or(0.);
    let oz = repeat_z as f32 * water.repeat_period.unwrap_or(0.);
    let side = 256u32;
    let mut rgba = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            let wx = water.min[0] + (x as f32 + 0.5) / side as f32 * width + ox;
            let wz = water.max[2] - (y as f32 + 0.5) / side as f32 * depth + oz;
            let opacity = if !water.depth_mask {
                [water.opacity.clamped(0.0, 1.0), 0.0]
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
            water.opacity.clamped(0., 1.),
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
                water.reflection_intensity.clamped(0., 1.)
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
    let columns = (width / 16.).ceil().clamped(1., 128.) as u32;
    let rows = (depth / 16.).ceil().clamped(1., 128.) as u32;
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
            zone: None,
            indices: start..out.indices.len() as u32,
            material: material_index,
            center: [
                water.min[0] + width * 0.5 + ox,
                water.max[1],
                water.max[2] - depth * (y as f32 + 0.5) / rows as f32 + oz,
            ],
        });
    }
    out.omissions
        .extend(water.warnings.iter().map(|w| format!("{}: {w}", water.id)));
    Ok(())
}

/// Maximum conservative active CPU/GPU cost of repeated waters in one map.
/// Includes duplicate construction buffers, mask mip chains and shared source images.
pub const WATER_BUDGET_BYTES: usize = 256 * 1024 * 1024;

/// Native repeated-water inputs. The original footprint and terrain queries remain
/// independent of the camera; only the renderer chooses which copies to draw.
pub struct WaterScene {
    pub water: Water,
    source: SceneData,
    textures: [usize; 3],
    specular: ([f32; 4], f32),
    fields: Vec<Arc<TerrainField>>,
}
impl WaterScene {
    pub fn new(
        water: Water,
        source: SceneData,
        textures: [usize; 3],
        specular: ([f32; 4], f32),
        fields: Vec<Arc<TerrainField>>,
    ) -> Result<Self> {
        water.validate()?;
        source.validate()?;
        ensure!(
            water.repeat_period.is_some(),
            "Camera-following water needs a repeat period"
        );
        ensure!(
            source.vertices.is_empty() && source.indices.is_empty() && source.materials.is_empty(),
            "Repeated water expects images without static geometry"
        );
        ensure!(
            textures.iter().all(|&i| i < source.images.len()),
            "Invalid repeated water textures"
        );
        Ok(Self {
            water,
            source,
            textures,
            specular,
            fields,
        })
    }
    /// Build one copy, retaining global surface UVs and local depth-mask UVs.
    pub fn copy_scene(&self, copy: [i32; 2]) -> Result<SceneData> {
        let period = self.water.repeat_period.unwrap();
        ensure!(
            copy.iter().all(|&i| i.unsigned_abs() <= 1_048_576),
            "Repeated water copy index exceeds precision limit"
        );
        ensure!(
            [
                self.water.min[0] + copy[0] as f32 * period,
                self.water.max[0] + copy[0] as f32 * period,
                self.water.min[2] + copy[1] as f32 * period,
                self.water.max[2] + copy[1] as f32 * period
            ]
            .iter()
            .all(|p| p.is_finite() && p.abs() <= 10_000_000.),
            "Repeated water copy exceeds coordinate limit"
        );
        let mut out = self.source.clone();
        append_copy(
            &mut out,
            &self.water,
            self.textures,
            self.specular,
            !self.fields.is_empty(),
            |x, z| {
                self.fields
                    .iter()
                    .filter_map(|f| f.height(x, z))
                    .max_by(f32::total_cmp)
            },
            copy,
        )?;
        // The dedicated texture cache keys immutable pixels by label.
        let mask = out.images.last_mut().unwrap();
        mask.label = format!("water-depth/{:x}", Sha256::digest(&mask.rgba));
        Ok(out)
    }
    fn geometry_bytes(&self) -> usize {
        let cols = ((self.water.max[0] - self.water.min[0]) / 16.)
            .ceil()
            .clamped(1., 128.) as usize;
        let rows = ((self.water.max[2] - self.water.min[2]) / 16.)
            .ceil()
            .clamped(1., 128.) as usize;
        (cols + 1) * (rows + 1) * std::mem::size_of::<SceneVertex>() + cols * rows * 6 * 4
    }
    fn copy_cost(&self) -> usize {
        // GPU geometry/mask plus conservative owned batch/material descriptors.
        self.geometry_bytes() + mip_bytes(256, 256) + 16_384
    }
    fn source_cost(&self) -> Result<usize> {
        // Retained CPU originals and the dedicated cache's source GPU textures.
        self.source.images.iter().try_fold(0usize, |total, image| {
            total
                .checked_add(image.rgba.len())
                .and_then(|n| n.checked_add(mip_bytes(image.width, image.height)))
                .context("Water image cost overflow")
        })
    }
    fn construction_cost(&self) -> Result<usize> {
        // Only one copy is built at a time: CPU Vec growth, cloned source
        // images, the new mask, and the largest upload helper CPU mip chain.
        let clones = self.source.images.iter().try_fold(0usize, |sum, image| {
            sum.checked_add(image.rgba.len())
                .context("Water construction cost overflow")
        })?;
        let source_mips = self
            .source
            .images
            .iter()
            .map(|image| mip_bytes(image.width, image.height))
            .max()
            .unwrap_or(0);
        (2 * self.geometry_bytes() + 256 * 256 * 4)
            .checked_add(clones)
            .and_then(|n| n.checked_add(source_mips.max(mip_bytes(256, 256))))
            .context("Water construction cost overflow")
    }
    /// Every footprint intersecting any view's draw-distance disc, once.
    /// Reject unsupported coverage before enumerating or allocating a large grid.
    pub fn visible_from(&self, eyes: &[Vec3], radius: f32) -> Result<BTreeSet<[i32; 2]>> {
        ensure!(
            radius.is_finite() && radius > 0.,
            "Invalid water draw distance"
        );
        let p = f64::from(self.water.repeat_period.unwrap());
        let w = &self.water;
        let limit = WATER_BUDGET_BYTES / self.copy_cost();
        let mut ranges = Vec::new();
        for eye in eyes {
            ensure!(
                eye.is_finite() && eye.abs().max_element() <= 10_000_000.,
                "Invalid repeated water eye"
            );
            let bounds = [
                ((f64::from(eye.x) - f64::from(radius) - f64::from(w.max[0])) / p).ceil(),
                ((f64::from(eye.x) + f64::from(radius) - f64::from(w.min[0])) / p).floor(),
                ((f64::from(eye.z) - f64::from(radius) - f64::from(w.max[2])) / p).ceil(),
                ((f64::from(eye.z) + f64::from(radius) - f64::from(w.min[2])) / p).floor(),
            ];
            ensure!(
                bounds
                    .iter()
                    .all(|b| b.is_finite() && b.abs() <= 1_048_576.),
                "Repeated water coverage exceeds copy precision limit"
            );
            let [x0, x1, z0, z1] = bounds.map(|b| b as i32);
            if x1 < x0 || z1 < z0 {
                continue;
            }
            let count = (i64::from(x1) - i64::from(x0) + 1)
                .checked_mul(i64::from(z1) - i64::from(z0) + 1)
                .context("Repeated water copy count overflow")?;
            ensure!(
                count <= limit as i64,
                "Repeated water view coverage exceeds 256 MiB budget"
            );
            ranges.push((Vec2::new(eye.x, eye.z), [x0, x1, z0, z1]));
        }
        let mut copies = BTreeSet::new();
        for (eye, [x0, x1, z0, z1]) in ranges {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let offset = Vec2::new(x as f32, z as f32) * p as f32;
                    let min = Vec2::new(w.min[0], w.min[2]) + offset;
                    let max = Vec2::new(w.max[0], w.max[2]) + offset;
                    ensure!(
                        min.abs().max_element() <= 10_000_000.
                            && max.abs().max_element() <= 10_000_000.,
                        "Repeated water footprint exceeds coordinate limit"
                    );
                    if eye.clamp(min, max).distance(eye) <= radius {
                        ensure!(
                            copies.contains(&[x, z]) || copies.len() < limit,
                            "Repeated water view union exceeds 256 MiB budget"
                        );
                        copies.insert([x, z]);
                    }
                }
            }
        }
        Ok(copies)
    }
}
fn mip_bytes(mut width: u32, mut height: u32) -> usize {
    let mut bytes = 0;
    loop {
        // Conservative texture storage: padded rows and at least one 4 KiB
        // allocation page per mip, in addition to the separately counted CPU pixels.
        let row = (width as usize * 4).div_ceil(256) * 256;
        bytes += (row * height as usize).div_ceil(4096) * 4096;
        if width == 1 && height == 1 {
            return bytes;
        }
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
}

/// Persistent geometry per visible copy; immutable textures/materials shared by
/// equal depth masks. This cache belongs exclusively to water on one device/map.
pub struct GpuWater {
    scene: Arc<WaterScene>,
    copies: BTreeMap<[i32; 2], (GpuScene, [u8; 32])>,
    palettes: BTreeMap<[u8; 32], GpuScene>,
    cache: TextureCache,
    max_radius: f32,
    mask_builds: usize,
}
impl GpuWater {
    pub fn new(scene: Arc<WaterScene>, max_radius: f32) -> Result<Self> {
        ensure!(
            max_radius.is_finite() && max_radius > 0.,
            "Invalid maximum water draw distance"
        );
        Ok(Self {
            scene,
            copies: BTreeMap::new(),
            palettes: BTreeMap::new(),
            cache: TextureCache::default(),
            max_radius,
            mask_builds: 0,
        })
    }
    /// Check the whole map's conservative resource bound before making any new
    /// geometry, masks or GPU allocations. No coverage is silently dropped.
    pub fn update_all(
        waters: &mut [Self],
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        eyes: &[Vec3],
        radius: f32,
    ) -> Result<()> {
        let mut selections = Vec::new();
        for water in waters.iter() {
            ensure!(
                radius.is_finite() && radius > 0. && radius <= water.max_radius,
                "Water draw distance exceeds uploaded maximum"
            );
            selections.push(water.scene.visible_from(eyes, radius)?);
        }
        let cost = Self::map_cost(waters, selections.iter().map(|c| c.len()))?;
        ensure!(
            cost <= WATER_BUDGET_BYTES,
            "Map repeated water coverage exceeds 256 MiB budget ({cost} bytes)"
        );
        // Every group's outgoing resources leave before ANY new group grows.
        for (water, copies) in waters.iter_mut().zip(&selections) {
            water.copies.retain(|key, _| copies.contains(key));
            water.prune();
        }
        for i in 0..waters.len() {
            // Enforce the bound at each admission as well as for the final union.
            // A future lifetime regression must refuse before uploading, not
            // briefly hold the old and new view unions together.
            let cost = Self::map_cost(
                waters,
                (0..waters.len()).map(|j| {
                    if i == j {
                        selections[j].len()
                    } else {
                        waters[j].copies.len()
                    }
                }),
            )?;
            ensure!(
                cost <= WATER_BUDGET_BYTES,
                "Repeated water admission exceeds map budget ({cost} bytes)"
            );
            waters[i].update(renderer, device, queue, std::mem::take(&mut selections[i]))?;
        }
        Ok(())
    }
    fn map_cost(waters: &[Self], counts: impl Iterator<Item = usize>) -> Result<usize> {
        let mut retained = 0usize;
        let mut peak = 0usize;
        for (water, count) in waters.iter().zip(counts) {
            let source = water.scene.source_cost()?;
            retained = retained
                .checked_add(
                    count
                        .checked_mul(water.scene.copy_cost())
                        .context("Water geometry cost overflow")?,
                )
                .and_then(|n| n.checked_add(source))
                .context("Water resource cost overflow")?;
            if count > 0 {
                peak = peak.max(water.scene.construction_cost()?);
            }
        }
        retained
            .checked_add(peak)
            .context("Water resource cost overflow")
    }
    fn update(
        &mut self,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        visible: BTreeSet<[i32; 2]>,
    ) -> Result<()> {
        for copy in visible {
            if self.copies.contains_key(&copy) {
                continue;
            }
            let mut data = self.scene.copy_scene(copy)?;
            self.mask_builds += 1;
            let hash: [u8; 32] = Sha256::digest(&data.images.last().unwrap().rgba).into();
            if let std::collections::btree_map::Entry::Vacant(entry) = self.palettes.entry(hash) {
                // Palette uploads carry no duplicate copy geometry.
                let (vertices, indices, batches) = (
                    std::mem::take(&mut data.vertices),
                    std::mem::take(&mut data.indices),
                    std::mem::take(&mut data.batches),
                );
                entry.insert(renderer.upload_cached(device, queue, &data, &mut self.cache)?);
                (data.vertices, data.indices, data.batches) = (vertices, indices, batches);
            }
            let gpu = renderer.upload_palette_model(device, &data, &self.palettes[&hash])?;
            self.copies.insert(copy, (gpu, hash));
        }
        self.prune();
        Ok(())
    }
    fn prune(&mut self) {
        let used: BTreeSet<_> = self.copies.values().map(|(_, hash)| *hash).collect();
        self.palettes.retain(|hash, _| used.contains(hash));
        let mut labels: BTreeSet<String> = self
            .scene
            .source
            .images
            .iter()
            .map(|i| i.label.clone())
            .collect();
        // Use the exact hex formatting used by copy_scene.
        for hash in &used {
            labels.insert(format!(
                "water-depth/{}",
                hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
            ));
        }
        self.cache.retain_labels(&labels);
    }
    pub fn draws(&self) -> impl Iterator<Item = &GpuScene> {
        self.copies.values().map(|(scene, _)| scene)
    }
    /// The same conservative bound used by admission, including construction
    /// buffers and unshared masks; shared palettes can use substantially less.
    pub fn resource_bound_bytes(&self) -> Result<usize> {
        Self::map_cost(
            std::slice::from_ref(self),
            std::iter::once(self.copies.len()),
        )
    }
    pub fn copy_count(&self) -> usize {
        self.copies.len()
    }
    pub fn mask_builds(&self) -> usize {
        self.mask_builds
    }
    pub fn texture_count(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{append, append_copy};
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
        for z in -1..=1 {
            for x in -1..=1 {
                append_copy(
                    &mut scene,
                    &sea(),
                    [0; 3],
                    specular,
                    false,
                    |_, _| None,
                    [x, z],
                )
                .unwrap();
            }
        }
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
