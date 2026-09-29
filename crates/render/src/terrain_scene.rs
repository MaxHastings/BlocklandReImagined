//! Camera-following native terrain.
//!
//! Each terrain block is cut into square tiles. Every distinct tile mesh is
//! built once from the shared `TerrainField` and uploaded once; the renderer
//! then draws it as instances at each periodic copy near the camera. Empty
//! squares exist only in the primary block, so a tile with holes has a holed
//! variant (primary copy) and a solid variant (repeated copies).
use crate::scene::{
    GpuInstances, GpuScene, MeshBatch, SceneData, SceneRenderer, SceneTransform, SceneVertex,
};
use anyhow::{Context, Result, ensure};
use bri_content::terrain_field::TerrainField;
use glam::{Mat4, Vec2, Vec3};
use std::sync::Arc;

/// Tile edge in cells; divides the 256-cell Torque terrain block.
const TILE_CELLS: i32 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Variant {
    /// Tile position inside the block, in tiles.
    tile: [i32; 2],
    /// Block the mesh was built at; instances translate by whole periods.
    base_block: [i32; 2],
    holed: bool,
}

/// CPU terrain for one placement: one material, one batch per tile variant.
pub struct TerrainScene {
    pub field: Arc<TerrainField>,
    pub data: SceneData,
    variants: Vec<Variant>,
    tile_cells: i32,
}

impl TerrainScene {
    /// `data` carries the terrain images and its single material (index 0);
    /// this fills vertices, indices and one batch per tile variant.
    pub fn build(field: Arc<TerrainField>, mut data: SceneData) -> Result<Self> {
        ensure!(
            data.materials.len() == 1 && data.vertices.is_empty() && data.batches.is_empty(),
            "Terrain scene expects one material and no geometry"
        );
        let side = field.side();
        let tile_cells = gcd(TILE_CELLS, side);
        let tiles = side / tile_cells;
        let mut variants = Vec::new();
        for ty in 0..tiles {
            for tx in 0..tiles {
                let holed = (ty * tile_cells..(ty + 1) * tile_cells).any(|y| {
                    (tx * tile_cells..(tx + 1) * tile_cells).any(|x| field.is_empty_square(x, y))
                });
                if holed {
                    variants.push(Variant {
                        tile: [tx, ty],
                        base_block: [0, 0],
                        holed: true,
                    });
                }
                // Repeated copies are solid everywhere; build them one period
                // away, where no empty square applies.
                if !holed || field.repeat {
                    variants.push(Variant {
                        tile: [tx, ty],
                        base_block: if holed { [1, 0] } else { [0, 0] },
                        holed: false,
                    });
                }
            }
        }
        let texture_repeats = side as f32 / 8.0;
        for variant in &variants {
            let region = [
                variant.base_block[0] * side + variant.tile[0] * tile_cells,
                variant.base_block[1] * side + variant.tile[1] * tile_cells,
                tile_cells,
                tile_cells,
            ];
            let mesh = field.mesh(region)?;
            let base = u32::try_from(data.vertices.len()).context("Too many terrain vertices")?;
            for i in 0..mesh.positions.len() {
                let uv = mesh.grid_uv[i];
                data.vertices.push(SceneVertex {
                    position: mesh.positions[i],
                    normal: mesh.normals[i],
                    uv: [uv[0] * texture_repeats, uv[1] * texture_repeats],
                    lightmap_uv: uv,
                    color: [1.0; 4],
                    fx: [0.; 4],
                });
            }
            let start = data.indices.len() as u32;
            data.indices
                .extend(mesh.triangles.iter().flatten().map(|i| base + i));
            let center = mesh
                .positions
                .iter()
                .fold(Vec3::ZERO, |sum, p| sum + Vec3::from(*p))
                / mesh.positions.len().max(1) as f32;
            data.batches.push(MeshBatch {
                indices: start..data.indices.len() as u32,
                material: 0,
                center: center.to_array(),
            });
        }
        data.validate()?;
        Ok(Self {
            field,
            data,
            variants,
            tile_cells,
        })
    }
    fn period(&self) -> f32 {
        self.field.side() as f32 * self.field.spacing
    }
    fn tile_size(&self) -> f32 {
        self.tile_cells as f32 * self.field.spacing
    }
    /// Instances per variant needed for any camera within `radius`.
    fn capacity(&self, radius: f32) -> usize {
        let copies = (2.0 * (radius + self.tile_size()) / self.period()).ceil() as usize + 1;
        copies * copies
    }
    /// Translations of every tile copy whose footprint lies within `radius`
    /// of `eye` (horizontally), grouped by variant index.
    pub fn visible(&self, eye: Vec3, radius: f32) -> Vec<Vec<Mat4>> {
        let mut out = vec![Vec::new(); self.variants.len()];
        if !(eye.is_finite() && radius.is_finite() && radius > 0.0) {
            return out;
        }
        let field = &self.field;
        let tiles = field.side() / self.tile_cells;
        let t = self.tile_cells;
        let [gx0, gy0] = field.grid(eye.x - radius, eye.z + radius);
        let [gx1, gy1] = field.grid(eye.x + radius, eye.z - radius);
        if !(gx0.abs() < 1.0e7 && gy0.abs() < 1.0e7 && gx1.abs() < 1.0e7 && gy1.abs() < 1.0e7) {
            return out;
        }
        let (tx0, tx1) = (
            (gx0.floor() as i32).div_euclid(t),
            (gx1.floor() as i32).div_euclid(t),
        );
        let (ty0, ty1) = (
            (gy0.floor() as i32).div_euclid(t),
            (gy1.floor() as i32).div_euclid(t),
        );
        let size = self.tile_size();
        let eye2 = Vec2::new(eye.x, eye.z);
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let block = [tx.div_euclid(tiles), ty.div_euclid(tiles)];
                let primary = block == [0, 0];
                if !field.repeat && !primary {
                    continue;
                }
                // Horizontal distance from the eye to the tile's footprint.
                let min = Vec2::new(
                    field.origin.x + tx as f32 * size,
                    field.origin.z - (ty + 1) as f32 * size,
                );
                let nearest = eye2.clamp(min, min + Vec2::splat(size));
                if nearest.distance(eye2) > radius {
                    continue;
                }
                let tile = [tx.rem_euclid(tiles), ty.rem_euclid(tiles)];
                let Some(index) = self
                    .variants
                    .iter()
                    .position(|v| v.tile == tile && (v.holed == primary || !self.has_holed(tile)))
                else {
                    continue;
                };
                let base = self.variants[index].base_block;
                let period = self.period();
                out[index].push(Mat4::from_translation(Vec3::new(
                    (block[0] - base[0]) as f32 * period,
                    0.0,
                    -((block[1] - base[1]) as f32) * period,
                )));
            }
        }
        out
    }
    fn has_holed(&self, tile: [i32; 2]) -> bool {
        self.variants.iter().any(|v| v.tile == tile && v.holed)
    }
}

/// Classic `worldToScreenScale` (pixels per unit at unit distance) for the
/// reference 1024-pixel-wide, 90-degree view. The original fade distances
/// scaled with resolution; a fixed reference keeps them resolution-independent.
const REFERENCE_WORLD_TO_SCREEN: f32 = 512.0;

/// Terrain material uniforms reproducing the classic detail and emboss-bump
/// passes (see `terrain_passes` in scene.wgsl). `detail_size` is the detail
/// image size in pixels when bound; `sun_direction` is the native light
/// direction (pointing away from the sun).
pub fn parameters(
    field: &TerrainField,
    sun_direction: [f32; 3],
    detail_size: Option<[u32; 2]>,
    bump_bound: bool,
) -> [[f32; 4]; 4] {
    // Classic distances use the integer square size.
    let square = field.spacing.round().max(1.0) as i32;
    let zero = |shift: i32| {
        (square as f32 * REFERENCE_WORLD_TO_SCREEN) / (1 << shift) as f32 - (square >> 1) as f32
    };
    let detail = detail_size.map_or([0.0; 2], |[w, h]| [62.0 / w as f32, 62.0 / h as f32]);
    // Emboss offset: dot of the block's s/t tangents with the vector toward
    // the sun, in Torque's Z-up object space. The t tangent samples heights
    // at object points (0,255) and (255,0) exactly like the original.
    let [nx, ny, nz] = sun_direction;
    let sun = Vec3::new(-nx, nz, -ny);
    let height = |x: f32, y: f32| {
        field
            .height(field.origin.x + x, field.origin.z - y)
            .unwrap_or(field.origin.y)
    };
    let t_tangent =
        Vec3::new(0.0, height(255.0, 0.0) - height(0.0, 255.0), square as f32).normalize_or_zero();
    let offset = [
        Vec3::X.dot(sun) * field.bump.offset,
        t_tangent.dot(sun) * field.bump.offset,
    ];
    let flags = u8::from(detail_size.is_some()) + 2 * u8::from(bump_bound);
    [
        [zero(6), zero(field.bump.zero_scale), detail[0], detail[1]],
        [
            32.0 / field.bump.scale / 4.0,
            offset[0],
            offset[1],
            f32::from(flags),
        ],
        [field.spacing, field.origin.x, field.origin.z, 0.0],
        [0.0; 4],
    ]
}

/// GPU terrain: one shared upload plus per-variant instance lists.
pub struct GpuTerrain {
    scene: Arc<TerrainScene>,
    tiles: Vec<(GpuScene, GpuInstances)>,
    radius: f32,
}
impl GpuTerrain {
    /// `max_radius` bounds the draw distance any later `update` may request.
    pub fn upload(
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: Arc<TerrainScene>,
        max_radius: f32,
    ) -> Result<Self> {
        ensure!(
            max_radius.is_finite() && max_radius > 0.0,
            "Invalid terrain draw distance"
        );
        let shared = renderer.upload(device, queue, &scene.data)?;
        let capacity = scene.capacity(max_radius);
        let tiles = (0..scene.variants.len())
            .map(|batch| {
                Ok((
                    shared.batch_view(batch)?,
                    GpuInstances::new(device, capacity)?,
                ))
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            scene,
            tiles,
            radius: max_radius,
        })
    }
    /// Select the tile copies within `radius` (clamped to the uploaded
    /// maximum) of the camera. Call once per submission, before drawing.
    pub fn update(&mut self, queue: &wgpu::Queue, eye: Vec3, radius: f32) -> Result<()> {
        let visible = self.scene.visible(eye, radius.min(self.radius));
        for ((_, instances), transforms) in self.tiles.iter_mut().zip(visible) {
            let transforms: Vec<_> = transforms
                .into_iter()
                .map(|transform| SceneTransform {
                    transform,
                    ..Default::default()
                })
                .collect();
            instances.update(queue, &transforms)?;
        }
        Ok(())
    }
    pub fn draws(&self) -> impl Iterator<Item = (&GpuScene, &GpuInstances)> {
        self.tiles
            .iter()
            .map(|(scene, instances)| (scene, instances))
    }
    pub fn omissions(&self) -> &[String] {
        &self.scene.data.omissions
    }
}

fn gcd(a: i32, b: i32) -> i32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{AlphaMode, Material, MaterialKind};
    use bri_content::{Terrain, TerrainLayer, terrain_field::*};

    fn scene(repeat: bool, empty_runs: Vec<[u32; 2]>) -> TerrainScene {
        let side = 128u32;
        let count = (side * side) as usize;
        let terrain = Terrain {
            schema_version: 1,
            id: "t".into(),
            side,
            elevations: (0..count).map(|i| (i % 13) as f32 * 0.5).collect(),
            primary_layers: vec![0; count],
            layers: vec![TerrainLayer {
                slot: 0,
                material: "m".into(),
                weights: vec![255; count],
            }],
        };
        let instance = TerrainInstance {
            schema_version: TERRAIN_INSTANCE_SCHEMA,
            node: 0,
            terrain: "t".into(),
            square_size: 8.0,
            origin: [-512.0, 0.0, 512.0],
            repeat,
            repeat_source: RepeatSource::Authored,
            empty_runs,
            detail: None,
            bump: TerrainBump {
                texture: None,
                scale: 1.0,
                offset: 0.01,
                zero_scale: 8,
            },
            diagnostics: vec![],
        };
        let field = Arc::new(TerrainField::new(terrain, &instance).unwrap());
        let data = SceneData {
            materials: vec![Material {
                name: "t".into(),
                images: [0; 13],
                kind: MaterialKind::Terrain,
                alpha: AlphaMode::Opaque,
                double_sided: false,
                clamp_nearest: false,
                temp_brick_flash: false,
                ignore_texture_alpha: false,
                parameters: Some([[0.0; 4]; 4]),
            }],
            ..Default::default()
        };
        TerrainScene::build(field, data).unwrap()
    }

    #[test]
    fn repeated_copies_cover_the_camera_and_holes_stay_primary() {
        // One hole in tile (0, 0) of the primary block.
        let s = scene(true, vec![[3 * 128 + 5, 1]]);
        assert_eq!(s.variants.len(), 4 + 1);
        let period = s.period();
        let size = s.tile_size();
        // A camera far away in a repeated block sees solid copies only.
        let eye = Vec3::new(
            s.field.origin.x + 7.5 * period,
            20.0,
            s.field.origin.z - 3.5 * period,
        );
        let visible = s.visible(eye, 300.0);
        let holed = s.variants.iter().position(|v| v.holed).unwrap();
        assert!(visible[holed].is_empty());
        let total: usize = visible.iter().map(Vec::len).sum();
        assert!(total >= 4, "{total}");
        for (variant, transforms) in s.variants.iter().zip(&visible) {
            for t in transforms {
                // Every copy lies on the terrain: sample its tile center.
                let local = Vec3::new(
                    s.field.origin.x
                        + (variant.base_block[0] * 128 / 64 + variant.tile[0]) as f32 * size
                        + size * 0.5,
                    0.0,
                    s.field.origin.z
                        - ((variant.base_block[1] * 128 / 64 + variant.tile[1]) as f32 * size
                            + size * 0.5),
                );
                let world = t.transform_point3(local);
                assert!(
                    Vec2::new(world.x, world.z).distance(Vec2::new(eye.x, eye.z)) < 300.0 + size
                );
            }
        }
        // The camera over the primary block draws the holed variant there.
        let eye = Vec3::new(s.field.origin.x + 40.0, 20.0, s.field.origin.z - 40.0);
        let visible = s.visible(eye, 100.0);
        assert_eq!(visible[holed], vec![Mat4::IDENTITY]);
        let solid_first = s
            .variants
            .iter()
            .position(|v| v.tile == [0, 0] && !v.holed)
            .unwrap();
        assert!(visible[solid_first].is_empty());
        // Capacity covers the densest selection.
        assert!(visible.iter().all(|v| v.len() <= s.capacity(100.0)));
    }

    #[test]
    fn non_repeating_terrain_draws_only_the_primary_block() {
        let s = scene(false, vec![]);
        let far = Vec3::new(s.field.origin.x - 2000.0, 0.0, s.field.origin.z);
        assert!(s.visible(far, 500.0).iter().all(Vec::is_empty));
        let inside = Vec3::new(s.field.origin.x + 1.0, 0.0, s.field.origin.z - 1.0);
        let visible = s.visible(inside, 2000.0);
        assert_eq!(visible.iter().map(Vec::len).sum::<usize>(), 4);
        assert!(visible.iter().flatten().all(|t| *t == Mat4::IDENTITY));
    }

    #[test]
    fn classic_detail_and_bump_uniforms() {
        let s = scene(true, vec![]);
        // Straight-down sun: no emboss shift along the flat s tangent.
        let p = parameters(&s.field, [0.0, -1.0, 0.0], Some([256, 128]), true);
        // Square size 8 at the reference scale: 8*512/64 - 4 and 8*512/256 - 4.
        assert_eq!(p[0][0], 60.0);
        assert_eq!(p[0][1], 12.0);
        assert_eq!([p[0][2], p[0][3]], [62.0 / 256.0, 62.0 / 128.0]);
        assert_eq!(p[1][0], 8.0);
        assert!(p[1][1].abs() < 1e-6 && p[1][2].abs() > 0.0);
        assert_eq!(p[1][3], 3.0);
        assert_eq!(p[2], [8.0, -512.0, 512.0, 0.0]);
        let none = parameters(&s.field, [0.3, -1.0, 0.4], None, false);
        assert_eq!(none[1][3], 0.0);
        // A low sun along +X shifts the emboss sample along s.
        let low = parameters(&s.field, [-1.0, -0.1, 0.0], None, true);
        assert!(low[1][1] > 0.0);
    }
}
