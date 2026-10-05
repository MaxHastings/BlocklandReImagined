use crate::Definition;
use anyhow::{Result, ensure};
use bri_console::Clamp;
use glam::{Mat4, Vec3};
use serde::Serialize;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceKind {
    Terrain,
    Interior,
    Static,
    Water,
}
#[derive(Clone, Copy, Debug)]
pub struct SurfaceHit {
    pub position: Vec3,
    pub normal: Vec3,
    pub kind: SurfaceKind,
}
#[derive(Clone, Copy, Debug)]
pub struct PlacementRay {
    pub start: Vec3,
    pub end: Vec3,
    pub include_water: bool,
}
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
pub struct PlacementStats {
    pub requested: u32,
    pub placed: u32,
    pub rejected: u32,
    pub queries: u64,
    pub completed: bool,
}
#[derive(Clone, Debug)]
pub struct Plant {
    pub id: u32,
    pub position: Vec3,
    pub width: f32,
    pub height: f32,
    pub flip: bool,
    pub angle: f32,
    pub sway_phase: f32,
    pub sway_rate: f32,
    pub light_phase: f32,
}
struct Lcg(u32);
impl Lcg {
    fn value(&mut self) -> f32 {
        self.0 = ((u64::from(self.0) * 16807) % 2147483647) as u32;
        self.0 as f32 * (1. / 2147483647.)
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.value()
    }
}
pub struct PlacementBuilder {
    definition: Definition,
    rng: Lcg,
    plants: Vec<Plant>,
    stats: PlacementStats,
    attempt: u32,
    pending: Option<PlacementRay>,
}
impl PlacementBuilder {
    pub fn new(definition: Definition) -> Result<Self> {
        definition.validate()?;
        Ok(Self {
            rng: Lcg(definition.seed),
            stats: PlacementStats {
                requested: definition.count,
                completed: definition.count == 0,
                ..Default::default()
            },
            definition,
            plants: vec![],
            attempt: 0,
            pending: None,
        })
    }
    pub fn stats(&self) -> PlacementStats {
        self.stats
    }
    /// A dropped builder cancels cleanly: no world state was ever created.
    pub fn advance(
        &mut self,
        ray_budget: u32,
        mut trace: impl FnMut(PlacementRay) -> Option<SurfaceHit>,
    ) -> Result<PlacementStats> {
        ensure!(
            ray_budget <= 65536,
            "placement call exceeds ray budget limit"
        );
        for _ in 0..ray_budget {
            if self.stats.completed {
                break;
            }
            let ray = if let Some(r) = self.pending.take() {
                r
            } else {
                let x = self
                    .rng
                    .range(self.definition.inner[0], self.definition.outer[0]);
                let z = self
                    .rng
                    .range(self.definition.inner[1], self.definition.outer[1]);
                let angle = self.rng.range(0., std::f32::consts::TAU);
                let (dx, dz) = if self.definition.square {
                    (
                        x * if angle.cos() < 0. { -1. } else { 1. },
                        z * if angle.sin() < 0. { -1. } else { 1. },
                    )
                } else {
                    (x * angle.cos(), z * angle.sin())
                };
                let p = Vec3::from_array(self.definition.origin) + Vec3::new(dx, 0., -dz);
                PlacementRay {
                    start: Vec3::new(p.x, 2000., p.z),
                    end: Vec3::new(p.x, -2000., p.z),
                    include_water: true,
                }
            };
            self.stats.queries += 1;
            let hit = trace(ray);
            if let Some(h) = hit {
                ensure!(
                    h.position.is_finite()
                        && h.normal.is_finite()
                        && (h.normal.length_squared() - 1.).abs() < 0.01
                        && h.position.y >= -2000.
                        && h.position.y <= 2000.
                        && (h.position.x - ray.start.x).abs() < 0.01
                        && (h.position.z - ray.start.z).abs() < 0.01,
                    "invalid host foliage ray result"
                );
                let allowed = match h.kind {
                    SurfaceKind::Terrain => self.definition.allow_terrain,
                    SurfaceKind::Interior => self.definition.allow_interior,
                    SurfaceKind::Static => self.definition.allow_static,
                    SurfaceKind::Water => self.definition.allow_water,
                };
                if allowed
                    && h.kind == SurfaceKind::Water
                    && !self.definition.water_surface
                    && ray.include_water
                {
                    self.pending = Some(PlacementRay {
                        include_water: false,
                        ..ray
                    });
                    continue;
                }
                if allowed && h.normal.y >= self.definition.allowed_slope.to_radians().cos() - 1e-6
                {
                    self.accept(h.position);
                    continue;
                }
            }
            self.attempt += 1;
            if self.attempt >= self.definition.retries {
                self.stats.rejected += 1;
                self.attempt = 0;
            }
            self.stats.completed = self.stats.placed + self.stats.rejected == self.stats.requested;
        }
        Ok(self.stats)
    }
    fn accept(&mut self, position: Vec3) {
        let d = &self.definition;
        let h = if d.fixed_size {
            d.height[1]
        } else {
            self.rng.range(d.height[0], d.height[1])
        };
        let w = if d.fixed_aspect {
            h
        } else if d.fixed_size {
            d.width[1]
        } else {
            self.rng.range(d.width[0], d.width[1])
        };
        let flip = d.flip && self.rng.range(0., 1000.) >= 500.;
        let angle = if d.random_rotation {
            self.rng.range(0., std::f32::consts::TAU)
        } else {
            d.rotation
        };
        self.plants.push(Plant {
            id: self.stats.placed + self.stats.rejected,
            position: position + Vec3::Y * d.offset,
            width: w,
            height: h,
            flip,
            angle,
            sway_phase: 0.,
            sway_rate: 0.,
            light_phase: 0.,
        });
        self.stats.placed += 1;
        self.attempt = 0;
        self.stats.completed = self.stats.placed + self.stats.rejected == self.stats.requested;
    }
    pub fn finish(mut self) -> Result<FoliageField> {
        ensure!(self.stats.completed, "placement incomplete");
        let d = &self.definition;
        if d.light && !d.light_sync {
            for p in &mut self.plants {
                p.light_phase = self.rng.range(0., 719.);
            }
        }
        if d.sway {
            if d.sway_sync {
                let mut rate = 0.;
                for _ in &self.plants {
                    rate = 719. / self.rng.range(d.sway_seconds[0], d.sway_seconds[1]);
                }
                for p in &mut self.plants {
                    p.sway_rate = rate;
                }
            } else {
                for p in &mut self.plants {
                    p.sway_phase = self.rng.range(0., 719.);
                    p.sway_rate = 719. / self.rng.range(d.sway_seconds[0], d.sway_seconds[1]);
                }
            }
        }
        Ok(FoliageField::build(
            self.definition,
            self.plants,
            self.stats,
        ))
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub right: Vec3,
    pub view_projection: Mat4,
    pub visible_distance: f32,
}
impl Camera {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.position.is_finite()
                && self.right.is_finite()
                && (self.right.length_squared() - 1.).abs() < 0.001
                && self.view_projection.is_finite()
                && self.visible_distance.is_finite()
                && self.visible_distance > 0.,
            "invalid foliage camera"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct CullStats {
    pub source_instances: usize,
    pub source_cells: usize,
    pub cells_tested: usize,
    pub cells_visible: usize,
    pub instances_tested: usize,
    pub instances_visible: usize,
}
#[derive(Clone)]
struct Cell {
    min: Vec3,
    max: Vec3,
    indices: Vec<u32>,
}
#[derive(Clone)]
pub struct FoliageField {
    definition: Definition,
    plants: Vec<Plant>,
    cells: BTreeMap<(i32, i32), Cell>,
    pub placement: PlacementStats,
    max_radius: f32,
}
impl FoliageField {
    fn build(definition: Definition, plants: Vec<Plant>, placement: PlacementStats) -> Self {
        let mut cells: BTreeMap<(i32, i32), Cell> = BTreeMap::new();
        let mut max_radius: f32 = 0.;
        for (i, p) in plants.iter().enumerate() {
            let r = (p.width * 0.5 + definition.sway_magnitude[0] + definition.sway_magnitude[1])
                .max(0.5);
            max_radius = max_radius.max(r);
            let min = p.position - Vec3::splat(r);
            let max = p.position + Vec3::new(r, p.height + r, r);
            let key = (
                (p.position.x / definition.cull_size).floor() as i32,
                (p.position.z / definition.cull_size).floor() as i32,
            );
            let cell = cells.entry(key).or_insert(Cell {
                min,
                max,
                indices: vec![],
            });
            cell.min = cell.min.min(min);
            cell.max = cell.max.max(max);
            cell.indices.push(i as u32);
        }
        Self {
            definition,
            plants,
            cells,
            placement,
            max_radius,
        }
    }
    pub fn definition(&self) -> &Definition {
        &self.definition
    }
    pub fn plants(&self) -> &[Plant] {
        &self.plants
    }
    pub fn visible(&self, camera: &Camera, out: &mut Vec<u32>) -> Result<CullStats> {
        camera.validate()?;
        out.clear();
        let mut stats = CullStats {
            source_instances: self.plants.len(),
            source_cells: self.cells.len(),
            ..Default::default()
        };
        let d = &self.definition;
        if d.hidden {
            return Ok(stats);
        }
        let distance = (d.distance + d.fade_far).min(camera.visible_distance);
        let bounds = distance + self.max_radius;
        let min_x = ((camera.position.x - bounds) / d.cull_size).floor() as i32;
        let max_x = ((camera.position.x + bounds) / d.cull_size).floor() as i32;
        let planes = planes(camera.view_projection);
        for ((_, _), cell) in self.cells.range((min_x, i32::MIN)..=(max_x, i32::MAX)) {
            stats.cells_tested += 1;
            if d.culling
                && (!aabb_visible(cell.min, cell.max, &planes)
                    || (camera.position - cell.min.max(camera.position.min(cell.max)))
                        .length_squared()
                        > distance * distance)
            {
                continue;
            }
            stats.cells_visible += 1;
            for &i in &cell.indices {
                stats.instances_tested += 1;
                let p = &self.plants[i as usize];
                let distance = (p.position - camera.position).length();
                if distance <= camera.visible_distance && fade(d, distance) > 0. {
                    out.push(i);
                }
            }
        }
        stats.instances_visible = out.len();
        Ok(stats)
    }
}
fn planes(m: Mat4) -> [glam::Vec4; 6] {
    let t = m.transpose();
    [
        t.w_axis + t.x_axis,
        t.w_axis - t.x_axis,
        t.w_axis + t.y_axis,
        t.w_axis - t.y_axis,
        t.z_axis,
        t.w_axis - t.z_axis,
    ]
}
fn aabb_visible(min: Vec3, max: Vec3, p: &[glam::Vec4; 6]) -> bool {
    p.iter().all(|p| {
        let n = p.truncate();
        let v = Vec3::new(
            if n.x >= 0. { max.x } else { min.x },
            if n.y >= 0. { max.y } else { min.y },
            if n.z >= 0. { max.z } else { min.z },
        );
        n.dot(v) + p.w >= 0.
    })
}
pub fn fade(d: &Definition, distance: f32) -> f32 {
    if distance < d.closest {
        if d.fade_near == 0. {
            0.
        } else {
            (1. - (d.closest - distance) / d.fade_near).clamped(0., 1.)
        }
    } else if distance > d.distance {
        if d.fade_far == 0. {
            0.
        } else {
            (1. - (distance - d.distance) / d.fade_far).clamped(0., 1.)
        }
    } else {
        1.
    }
}
/// Absolute host time deliberately advances hidden plants too; original culled per-item phases paused.
pub fn motion(d: &Definition, p: &Plant, seconds: f32) -> ([f32; 2], f32) {
    let phase = ((p.sway_phase + p.sway_rate * seconds)
        .rem_euclid(720.)
        .floor())
        * std::f32::consts::TAU
        / 720.;
    let sway = if d.sway {
        [
            d.sway_magnitude[0] * phase.cos(),
            d.sway_magnitude[1] * phase.sin(),
        ]
    } else {
        [0.; 2]
    };
    let lp = ((p.light_phase + 719. / d.light_seconds * seconds)
        .rem_euclid(720.)
        .floor())
        * std::f32::consts::TAU
        / 720.;
    let light = if d.light {
        (d.luminance[0] + d.luminance[1]) * 0.5 + (d.luminance[1] - d.luminance[0]) * 0.5 * lp.cos()
    } else {
        1.
    };
    (sway, light)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn engine_family_lcg_known_sequence() {
        let mut r = Lcg(1);
        r.value();
        assert_eq!(r.0, 16807);
        r.value();
        assert_eq!(r.0, 282475249);
        r.value();
        assert_eq!(r.0, 1622650073);
    }
}
