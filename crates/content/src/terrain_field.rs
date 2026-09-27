//! Native terrain placement semantics shared by rendering, physics and queries.
//!
//! A `TerrainInstance` is the converted, versioned description of one authored
//! terrain placement (spacing, origin, repetition, empty squares, detail and
//! bump parameters). A `TerrainField` binds it to the native elevations and
//! answers exact height/ray/mesh questions. Neither type reads legacy formats.
//!
//! Coordinates: native world is Y-up. Terrain cell column `x` advances along
//! +X and cell row `y` advances along -Z (legacy +Y). Cell `(x, y)` spans the
//! vertices `(x..=x+1, y..=y+1)`. Elevations repeat with the block period;
//! empty squares apply only to the primary block `[0, side)²`, matching the
//! legacy renderer and collision ("holes only in the primary terrain block").
use crate::{Terrain, terrain_mesh};
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const TERRAIN_INSTANCE_SCHEMA: u32 = 1;

/// Why the repetition flag has its value; retained so fidelity reviews can
/// distinguish authored values from the unverified legacy default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatSource {
    /// The mission explicitly authored `RepeatTerrain`.
    Authored,
    /// The field is absent; the classic engine default (always repeat) is used.
    /// Blockland's own default for this later field is not verified.
    LegacyDefaultUnverified,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TerrainTexture {
    /// Package-local native image filename (content hash name).
    pub file: String,
    /// Original virtual path, provenance only.
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TerrainBump {
    /// Emboss texture; absent means the authored parameters are inert.
    pub texture: Option<TerrainTexture>,
    /// Legacy `bumpScale`: bump texture repeats `32 / scale` times per
    /// four-square texture tile. Values <= 0 were clamped to 0.0001.
    pub scale: f32,
    /// Legacy `bumpOffset`: sun-direction texture offset of the inverted pass.
    pub offset: f32,
    /// Legacy `zeroBumpScale`: fade distance shift (smaller is farther).
    pub zero_scale: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TerrainInstance {
    pub schema_version: u32,
    /// Scene node index of the placement.
    pub node: usize,
    /// Native terrain asset ID.
    pub terrain: String,
    /// Horizontal cell spacing in native world units.
    pub square_size: f32,
    /// Native world position of vertex (0, 0) of the primary block.
    pub origin: [f32; 3],
    pub repeat: bool,
    pub repeat_source: RepeatSource,
    /// Empty square runs `[first, count]` over row-major primary-block cells
    /// (`first = x + y * side`). Converted from the legacy mission field.
    #[serde(default)]
    pub empty_runs: Vec<[u32; 2]>,
    #[serde(default)]
    pub detail: Option<TerrainTexture>,
    pub bump: TerrainBump,
    /// Conversion diagnostics that affect fidelity.
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

impl TerrainInstance {
    pub fn validate(&self, side: u32) -> Result<()> {
        ensure!(
            self.schema_version == TERRAIN_INSTANCE_SCHEMA,
            "Unknown terrain instance schema {}",
            self.schema_version
        );
        ensure!(!self.terrain.is_empty(), "Terrain instance lacks asset");
        ensure!(
            self.square_size.is_finite() && (0.001..=4096.0).contains(&self.square_size),
            "Invalid terrain square size"
        );
        ensure!(
            self.origin.iter().all(|v| v.is_finite() && v.abs() < 1.0e7),
            "Invalid terrain origin"
        );
        let cells = u64::from(side) * u64::from(side);
        ensure!(self.empty_runs.len() <= 65_536, "Too many empty runs");
        for [first, count] in &self.empty_runs {
            ensure!(
                *count > 0 && u64::from(*first) + u64::from(*count) <= cells,
                "Empty square run outside terrain block"
            );
        }
        ensure!(
            self.bump.scale.is_finite()
                && self.bump.scale > 0.0
                && self.bump.offset.is_finite()
                && (0..=31).contains(&self.bump.zero_scale),
            "Invalid terrain bump parameters"
        );
        for texture in self.detail.iter().chain(self.bump.texture.iter()) {
            ensure!(
                !texture.file.is_empty()
                    && !texture.file.contains(['/', '\\', ':'])
                    && texture.file != "."
                    && texture.file != "..",
                "Invalid terrain texture filename"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainRayHit {
    pub distance: f32,
    pub point: Vec3,
    pub normal: Vec3,
    pub cell: [i32; 2],
}

/// Exact native terrain semantics for one placement. Share it with `Arc`.
#[derive(Debug)]
pub struct TerrainField {
    pub id: String,
    pub node: usize,
    pub terrain: Terrain,
    pub spacing: f32,
    pub origin: Vec3,
    pub repeat: bool,
    empty: Vec<u64>,
    empty_count: usize,
    min_height: f32,
    max_height: f32,
}

fn inside_triangle_xz(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let cross = |o: [f32; 2], u: [f32; 2], v: [f32; 2]| {
        (u[0] - o[0]) * (v[1] - o[1]) - (u[1] - o[1]) * (v[0] - o[0])
    };
    let d1 = cross(a, b, p);
    let d2 = cross(b, c, p);
    let d3 = cross(c, a, p);
    let neg = d1 < -1e-7 || d2 < -1e-7 || d3 < -1e-7;
    let pos = d1 > 1e-7 || d2 > 1e-7 || d3 > 1e-7;
    !(neg && pos)
}

impl TerrainField {
    pub fn new(terrain: Terrain, instance: &TerrainInstance) -> Result<Self> {
        terrain.validate()?;
        instance.validate(terrain.side)?;
        ensure!(
            terrain.id == instance.terrain,
            "Terrain instance refers to another asset"
        );
        let side = terrain.side as usize;
        let mut empty = vec![0u64; (side * side).div_ceil(64)];
        for [first, count] in &instance.empty_runs {
            for index in *first..*first + *count {
                empty[index as usize / 64] |= 1 << (index % 64);
            }
        }
        let empty_count = empty.iter().map(|w| w.count_ones() as usize).sum();
        let (min_height, max_height) = terrain
            .elevations
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), h| {
                (lo.min(*h), hi.max(*h))
            });
        Ok(Self {
            id: instance.terrain.clone(),
            node: instance.node,
            terrain,
            spacing: instance.square_size,
            origin: Vec3::from(instance.origin),
            repeat: instance.repeat,
            empty,
            empty_count,
            min_height: min_height + instance.origin[1],
            max_height: max_height + instance.origin[1],
        })
    }
    /// Placement origin from a translation-only scene transform (converter use).
    pub fn origin_from_transform(transform: &[f32; 16]) -> Result<[f32; 3]> {
        let matrix = Mat4::from_cols_array(transform);
        ensure!(matrix.is_finite(), "Non-finite terrain placement");
        let (scale, rotation, translation) = matrix.to_scale_rotation_translation();
        ensure!(
            (scale - Vec3::ONE).abs().max_element() < 1e-5
                && rotation.angle_between(glam::Quat::IDENTITY) < 1e-5,
            "Terrain placements are translation-only in the legacy engine"
        );
        Ok(translation.to_array())
    }
    pub fn side(&self) -> i32 {
        self.terrain.side as i32
    }
    /// World-space vertical extent of all solid terrain.
    pub fn height_range(&self) -> (f32, f32) {
        (self.min_height, self.max_height)
    }
    pub fn empty_square_count(&self) -> usize {
        self.empty_count
    }
    /// Periodic elevation sample at a vertex, relative to the terrain origin.
    pub fn sample(&self, x: i32, y: i32) -> f32 {
        let side = self.side();
        self.terrain.elevations[(y.rem_euclid(side) * side + x.rem_euclid(side)) as usize]
    }
    pub fn in_primary_block(&self, x: i32, y: i32) -> bool {
        let side = self.side();
        (0..side).contains(&x) && (0..side).contains(&y)
    }
    /// True if the authored empty flag removes this square. Only the primary
    /// block carries holes; repeated blocks are solid everywhere.
    pub fn is_empty_square(&self, x: i32, y: i32) -> bool {
        if self.empty_count == 0 || !self.in_primary_block(x, y) {
            return false;
        }
        let index = (y * self.side() + x) as usize;
        self.empty[index / 64] & (1 << (index % 64)) != 0
    }
    /// Whether square `(x, y)` has renderable/collidable triangles.
    pub fn square_exists(&self, x: i32, y: i32) -> bool {
        (self.repeat || self.in_primary_block(x, y)) && !self.is_empty_square(x, y)
    }
    /// Continuous grid coordinates of a world position.
    pub fn grid(&self, world_x: f32, world_z: f32) -> [f32; 2] {
        [
            (world_x - self.origin.x) / self.spacing,
            -(world_z - self.origin.z) / self.spacing,
        ]
    }
    pub fn vertex(&self, x: i32, y: i32) -> Vec3 {
        self.origin
            + Vec3::new(
                x as f32 * self.spacing,
                self.sample(x, y),
                -(y as f32) * self.spacing,
            )
    }
    /// Exact triangle-interpolated height, or `None` over holes/outside.
    pub fn height(&self, world_x: f32, world_z: f32) -> Option<f32> {
        let [gx, gy] = self.grid(world_x, world_z);
        if !(gx.is_finite() && gy.is_finite() && gx.abs() < 1.0e7 && gy.abs() < 1.0e7) {
            return None;
        }
        if !self.square_exists(gx.floor() as i32, gy.floor() as i32) {
            return None;
        }
        Some(
            self.origin.y
                + terrain_mesh::height(
                    &self.terrain,
                    self.spacing,
                    world_x - self.origin.x,
                    world_z - self.origin.z,
                ),
        )
    }
    /// The two triangles of a square in world space, upward wound
    /// (counter-clockwise seen from above), using the checkerboard split.
    pub fn square_triangles(&self, x: i32, y: i32) -> [[Vec3; 3]; 2] {
        let a = self.vertex(x, y);
        let b = self.vertex(x + 1, y);
        let c = self.vertex(x, y + 1);
        let d = self.vertex(x + 1, y + 1);
        if (x ^ y) & 1 == 0 {
            [[a, b, d], [a, d, c]]
        } else {
            [[a, b, c], [b, d, c]]
        }
    }
    pub fn normal(&self, world_x: f32, world_z: f32) -> Option<Vec3> {
        let [gx, gy] = self.grid(world_x, world_z);
        let (x, y) = (gx.floor() as i32, gy.floor() as i32);
        if !self.square_exists(x, y) {
            return None;
        }
        let p = [world_x, world_z];
        for [a, b, c] in self.square_triangles(x, y) {
            if inside_triangle_xz(p, [a.x, a.z], [b.x, b.z], [c.x, c.z]) {
                return Some((b - a).cross(c - a).normalize());
            }
        }
        None
    }
    /// Exact ray cast against solid squares (either side of a triangle).
    /// Uses a 2D cell walk bounded by the terrain's vertical extent.
    pub fn cast_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<TerrainRayHit> {
        let length = direction.length();
        if !(origin.is_finite() && direction.is_finite() && max_distance.is_finite())
            || length < 1e-12
            || max_distance <= 0.0
        {
            return None;
        }
        let dir = direction / length;
        // Clip to the solid vertical slab.
        let (lo, hi) = self.height_range();
        let (mut t0, mut t1) = (0.0f32, max_distance);
        if dir.y.abs() < 1e-12 {
            if origin.y < lo - 1e-3 || origin.y > hi + 1e-3 {
                return None;
            }
        } else {
            let a = (lo - 1e-3 - origin.y) / dir.y;
            let b = (hi + 1e-3 - origin.y) / dir.y;
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
            if t0 > t1 {
                return None;
            }
        }
        let start = origin + dir * t0;
        let [gx, gy] = self.grid(start.x, start.z);
        if !(gx.abs() < 1.0e7 && gy.abs() < 1.0e7) {
            return None;
        }
        // Grid-space direction per unit ray distance.
        let dgx = dir.x / self.spacing;
        let dgy = -dir.z / self.spacing;
        let mut cell = [gx.floor() as i32, gy.floor() as i32];
        let step = [
            if dgx >= 0.0 { 1 } else { -1 },
            if dgy >= 0.0 { 1 } else { -1 },
        ];
        let next_boundary = |g: f32, c: i32, s: i32| {
            if s > 0 {
                (c + 1) as f32 - g
            } else {
                g - c as f32
            }
        };
        let mut t_max = [
            if dgx.abs() < 1e-12 {
                f32::INFINITY
            } else {
                t0 + next_boundary(gx, cell[0], step[0]) / dgx.abs()
            },
            if dgy.abs() < 1e-12 {
                f32::INFINITY
            } else {
                t0 + next_boundary(gy, cell[1], step[1]) / dgy.abs()
            },
        ];
        let t_delta = [
            if dgx.abs() < 1e-12 {
                f32::INFINITY
            } else {
                1.0 / dgx.abs()
            },
            if dgy.abs() < 1e-12 {
                f32::INFINITY
            } else {
                1.0 / dgy.abs()
            },
        ];
        let limit = ((t1 - t0) * (dgx.abs() + dgy.abs())) as usize + 4;
        let mut t_enter = t0;
        for _ in 0..limit.min(4_000_000) {
            if t_enter > t1 {
                break;
            }
            if self.square_exists(cell[0], cell[1]) {
                let mut best: Option<(f32, Vec3)> = None;
                for [a, b, c] in self.square_triangles(cell[0], cell[1]) {
                    // Möller–Trumbore, two-sided.
                    let e1 = b - a;
                    let e2 = c - a;
                    let p = dir.cross(e2);
                    let det = e1.dot(p);
                    if det.abs() < 1e-12 {
                        continue;
                    }
                    let inv = 1.0 / det;
                    let s = origin - a;
                    let u = s.dot(p) * inv;
                    if !(-1e-6..=1.0 + 1e-6).contains(&u) {
                        continue;
                    }
                    let q = s.cross(e1);
                    let v = dir.dot(q) * inv;
                    if v < -1e-6 || u + v > 1.0 + 1e-6 {
                        continue;
                    }
                    let t = e2.dot(q) * inv;
                    if t >= 0.0 && t <= max_distance && best.is_none_or(|(bt, _)| t < bt) {
                        best = Some((t, e1.cross(e2).normalize()));
                    }
                }
                if let Some((t, normal)) = best {
                    return Some(TerrainRayHit {
                        distance: t,
                        point: origin + dir * t,
                        normal,
                        cell,
                    });
                }
            }
            if t_max[0] < t_max[1] {
                t_enter = t_max[0];
                t_max[0] += t_delta[0];
                cell[0] += step[0];
            } else {
                t_enter = t_max[1];
                t_max[1] += t_delta[1];
                cell[1] += step[1];
            }
        }
        None
    }
    /// World-space mesh of solid squares in `[x, y, width, height]` cells.
    /// Vertices are shared; unused vertices of empty squares are omitted.
    pub fn mesh(&self, region: [i32; 4]) -> Result<terrain_mesh::Mesh> {
        let [x0, y0, w, h] = region;
        ensure!(
            w > 0
                && h > 0
                && w <= 1024
                && h <= 1024
                && x0.unsigned_abs() < 1_000_000
                && y0.unsigned_abs() < 1_000_000,
            "Invalid terrain region"
        );
        let mut mesh = terrain_mesh::Mesh {
            positions: vec![],
            normals: vec![],
            grid_uv: vec![],
            triangles: vec![],
        };
        let mut index = vec![u32::MAX; ((w + 1) * (h + 1)) as usize];
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                if !self.square_exists(x, y) {
                    continue;
                }
                let mut vid = |cx: i32, cy: i32| -> u32 {
                    let slot = ((cy - y0) * (w + 1) + (cx - x0)) as usize;
                    if index[slot] == u32::MAX {
                        index[slot] = mesh.positions.len() as u32;
                        mesh.positions.push(self.vertex(cx, cy).to_array());
                        let dx = (self.sample(cx + 1, cy) - self.sample(cx - 1, cy))
                            / (2.0 * self.spacing);
                        let dy = (self.sample(cx, cy + 1) - self.sample(cx, cy - 1))
                            / (2.0 * self.spacing);
                        mesh.normals
                            .push(Vec3::new(-dx, 1.0, dy).normalize().to_array());
                        let side = self.terrain.side as f32;
                        mesh.grid_uv.push([cx as f32 / side, cy as f32 / side]);
                    }
                    index[slot]
                };
                let a = vid(x, y);
                let b = vid(x + 1, y);
                let c = vid(x, y + 1);
                let d = vid(x + 1, y + 1);
                if (x ^ y) & 1 == 0 {
                    mesh.triangles.extend([[a, b, d], [a, d, c]]);
                } else {
                    mesh.triangles.extend([[a, b, c], [b, d, c]]);
                }
            }
        }
        Ok(mesh)
    }
}

/// Legacy `TypeBool` interpretation (`true` or a nonzero number).
pub fn legacy_bool(value: &str) -> bool {
    let value = value.trim();
    value.eq_ignore_ascii_case("true") || value.parse::<f64>().is_ok_and(|v| v != 0.0)
}

/// Bind a map's converted terrain placements to their native terrain assets.
/// Every terrain scene node needs exactly one instance, listed in node order.
pub fn map_fields(
    scene: &crate::scene::Scene,
    instances: Vec<TerrainInstance>,
    mut load: impl FnMut(&str) -> Result<Terrain>,
) -> Result<Vec<Arc<TerrainField>>> {
    let expected: Vec<_> = scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.kind, crate::scene::Kind::Terrain))
        .map(|(i, _)| i)
        .collect();
    ensure!(
        instances.iter().map(|i| i.node).collect::<Vec<_>>() == expected,
        "Native terrain instances disagree with the map's terrain placements"
    );
    instances
        .iter()
        .map(|instance| {
            let node = &scene.nodes[instance.node];
            ensure!(
                node.asset.as_deref() == Some(instance.terrain.as_str()),
                "Terrain instance refers to another placement asset"
            );
            Ok(Arc::new(TerrainField::new(
                load(&instance.terrain)?,
                instance,
            )?))
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::TerrainLayer;

    pub fn fixture(side: u32, repeat: bool, empty_runs: Vec<[u32; 2]>) -> TerrainField {
        let count = (side * side) as usize;
        let elevations = (0..count)
            .map(|i| {
                let (x, y) = ((i % side as usize) as f32, (i / side as usize) as f32);
                10.0 + (x * 0.7).sin() * 3.0 + (y * 0.45).cos() * 2.0 + (i % 7) as f32 * 0.25
            })
            .collect();
        let terrain = Terrain {
            schema_version: 1,
            id: "fixture".into(),
            side,
            elevations,
            primary_layers: vec![0; count],
            layers: vec![TerrainLayer {
                slot: 0,
                material: "test".into(),
                weights: vec![255; count],
            }],
        };
        let instance = TerrainInstance {
            schema_version: TERRAIN_INSTANCE_SCHEMA,
            node: 0,
            terrain: "fixture".into(),
            square_size: 2.0,
            origin: [-(side as f32), 5.0, side as f32],
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
        TerrainField::new(terrain, &instance).unwrap()
    }

    #[test]
    fn holes_only_in_primary_block_and_repeat_policy() {
        // Row 2, columns 3..6 of a 16 block.
        let f = fixture(16, true, vec![[2 * 16 + 3, 3]]);
        assert_eq!(f.empty_square_count(), 3);
        assert!(f.is_empty_square(3, 2) && f.is_empty_square(5, 2));
        assert!(!f.is_empty_square(6, 2) && !f.is_empty_square(2, 2));
        // Repeated block copies are solid.
        assert!(f.square_exists(3 + 16, 2) && f.square_exists(3 - 16, 2 - 32));
        let center = f.vertex(3, 2) + Vec3::new(0.5 * f.spacing, 0.0, -0.5 * f.spacing);
        assert!(f.height(center.x, center.z).is_none());
        let repeated = center + Vec3::new(16.0 * f.spacing, 0.0, 0.0);
        assert!(f.height(repeated.x, repeated.z).is_some());
        let once = fixture(16, false, vec![]);
        assert!(once.square_exists(0, 0) && once.square_exists(15, 15));
        assert!(!once.square_exists(16, 0) && !once.square_exists(-1, 3));
        // The closing row/column uses wrapped samples even without repetition.
        assert_eq!(once.vertex(16, 3).y, once.vertex(0, 3).y);
        let bad = TerrainInstance {
            empty_runs: vec![[250, 7]],
            ..instance_like()
        };
        assert!(bad.validate(16).is_err());
    }

    fn instance_like() -> TerrainInstance {
        TerrainInstance {
            schema_version: TERRAIN_INSTANCE_SCHEMA,
            node: 0,
            terrain: "fixture".into(),
            square_size: 2.0,
            origin: [0.0; 3],
            repeat: true,
            repeat_source: RepeatSource::Authored,
            empty_runs: vec![],
            detail: None,
            bump: TerrainBump {
                texture: None,
                scale: 1.0,
                offset: 0.01,
                zero_scale: 8,
            },
            diagnostics: vec![],
        }
    }

    #[test]
    fn heights_rays_and_normals_agree_across_negative_and_periodic_cells() {
        let f = fixture(16, true, vec![[5 * 16 + 5, 2]]);
        let mut checked = 0;
        for i in 0..400 {
            let gx = -40.0 + (i as f32 * 0.731) % 90.0;
            let gy = -37.0 + (i as f32 * 1.379) % 85.0;
            let x = f.origin.x + gx * f.spacing;
            let z = f.origin.z - gy * f.spacing;
            let Some(h) = f.height(x, z) else {
                assert!(f.is_empty_square(gx.floor() as i32, gy.floor() as i32));
                continue;
            };
            // Periodicity: same height one block over in either direction.
            let period = 16.0 * f.spacing;
            if !f.in_primary_block(gx.floor() as i32 + 16, gy.floor() as i32) {
                let other = f.height(x + period, z).unwrap();
                assert!((other - h).abs() < 1e-3, "{other} vs {h}");
            }
            let hit = f
                .cast_ray(Vec3::new(x, 100.0, z), Vec3::NEG_Y, 1000.0)
                .expect("vertical ray hits solid terrain");
            assert!((hit.point.y - h).abs() < 2e-3, "{} vs {h}", hit.point.y);
            assert!(hit.normal.y > 0.0);
            let n = f.normal(x, z).unwrap();
            assert!(n.dot(hit.normal) > 0.999);
            // Oblique rays agree with the height query at their hit point.
            let origin = Vec3::new(x - 30.0, h + 40.0, z + 20.0);
            let hit = f
                .cast_ray(origin, Vec3::new(x, h, z) - origin, 1000.0)
                .unwrap();
            let expected = f.height(hit.point.x, hit.point.z).unwrap();
            assert!((hit.point.y - expected).abs() < 5e-3);
            checked += 1;
        }
        assert!(checked > 380);
        // Rays fall through holes.
        let hole = f.vertex(5, 5) + Vec3::new(0.5 * f.spacing, 0.0, -0.5 * f.spacing);
        let hit = f.cast_ray(Vec3::new(hole.x, 100.0, hole.z), Vec3::NEG_Y, 1000.0);
        assert!(hit.is_none());
    }

    #[test]
    fn mesh_is_upward_wound_on_the_surface_and_omits_holes() {
        let f = fixture(16, true, vec![[16 + 1, 1]]);
        let mesh = f.mesh([-3, -2, 8, 6]).unwrap();
        assert_eq!(mesh.triangles.len(), 8 * 6 * 2 - 2);
        for tri in &mesh.triangles {
            let [a, b, c] = tri.map(|i| Vec3::from(mesh.positions[i as usize]));
            assert!((b - a).cross(c - a).y > 0.0);
            let centroid = (a + b + c) / 3.0;
            let h = f.height(centroid.x, centroid.z).unwrap();
            assert!((h - centroid.y).abs() < 1e-3);
        }
        // Periodic copies share the closing row/column samples exactly.
        let copy = f.mesh([13, -2, 8, 6]).unwrap();
        assert_eq!(copy.triangles.len(), 8 * 6 * 2);
        assert!(
            copy.positions
                .iter()
                .any(|p| (Vec3::from(*p) - f.vertex(16, 0)).length() < 1e-4)
        );
        assert_eq!(f.vertex(16, 0).y, f.vertex(0, 0).y);
    }

    #[test]
    fn legacy_bool_values() {
        assert!(
            legacy_bool("1") && legacy_bool("true") && legacy_bool(" TRUE ") && legacy_bool("2.5")
        );
        assert!(!legacy_bool("0") && !legacy_bool("false") && !legacy_bool(""));
        assert!(
            TerrainField::origin_from_transform(&Mat4::from_rotation_y(0.3).to_cols_array())
                .is_err()
        );
    }

    #[test]
    fn map_fields_follow_terrain_nodes() {
        let node = |kind, asset: Option<&str>| crate::scene::Node {
            name: "n".into(),
            parent: None,
            kind,
            transform: Mat4::IDENTITY.to_cols_array(),
            asset: asset.map(Into::into),
            properties: Default::default(),
        };
        let fixture = fixture(16, true, vec![]);
        let scene = crate::scene::Scene {
            schema_version: 1,
            id: "map".into(),
            name: "map".into(),
            nodes: vec![
                node(crate::scene::Kind::Spawn, None),
                node(crate::scene::Kind::Terrain, Some("fixture")),
            ],
            pending_scripts: vec![],
        };
        let load = |_: &str| Ok(fixture.terrain.clone());
        let instance = TerrainInstance {
            node: 1,
            ..instance_like()
        };
        assert_eq!(
            map_fields(&scene, vec![instance.clone()], load)
                .unwrap()
                .len(),
            1
        );
        assert!(map_fields(&scene, vec![], load).is_err());
        let wrong = TerrainInstance {
            node: 0,
            ..instance
        };
        assert!(map_fields(&scene, vec![wrong], load).is_err());
    }
}
