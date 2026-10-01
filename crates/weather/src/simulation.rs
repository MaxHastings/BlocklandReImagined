use crate::{Definition, Placement, WeatherPack};
use anyhow::{Result, ensure};
use glam::Vec3;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct CameraState {
    pub position: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub velocity: Vec3,
}
impl Default for CameraState {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: Vec3::NEG_Z,
            right: Vec3::X,
            up: Vec3::Y,
            velocity: Vec3::ZERO,
        }
    }
}
impl CameraState {
    fn validate(self) -> Result<()> {
        ensure!(
            self.position.is_finite()
                && self.velocity.is_finite()
                && self.forward.is_normalized()
                && self.right.is_normalized()
                && self.up.is_normalized(),
            "Invalid weather camera"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
pub struct WeatherEnvironment {
    /// Native Y-up advection velocity in units/second, chosen explicitly by the host.
    /// Placement.reference_wind_velocity contains the documented map conversion.
    pub wind_velocity: Vec3,
}
impl Default for WeatherEnvironment {
    fn default() -> Self {
        Self {
            wind_velocity: Vec3::ZERO,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct WeatherLimits {
    pub drops: usize,
    pub splashes: usize,
    pub queries_per_advance: usize,
    pub steps_per_advance: usize,
}
impl Default for WeatherLimits {
    fn default() -> Self {
        Self {
            drops: 16384,
            splashes: 8192,
            queries_per_advance: 8192,
            steps_per_advance: 8,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherSurface {
    Solid,
    Water,
}
#[derive(Clone, Copy, Debug)]
pub struct CollisionRay {
    pub start: Vec3,
    pub end: Vec3,
}
#[derive(Clone, Copy, Debug)]
pub struct WeatherHit {
    pub position: Vec3,
    pub normal: Vec3,
    pub surface: WeatherSurface,
}
#[derive(Default, Clone, Copy, Debug, serde::Serialize)]
pub struct WeatherDiagnostics {
    pub steps: u64,
    pub collision_queries: u64,
    pub pending_queries: usize,
    pub invalid_hits: u64,
    pub impacts: u64,
    pub water_impacts: u64,
    pub splashes_created: u64,
    pub splash_capacity_drops: u64,
    pub drop_capacity_clipped: usize,
    pub wraps: u64,
    pub teleports: u64,
    pub invalidations: u64,
    pub skipped_cosmetic_seconds: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct WeatherInstance {
    pub position: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub uv: [f32; 4],
    pub texture: u32,
    pub color: [f32; 4],
    pub splash: bool,
}
#[derive(Default)]
pub struct WeatherFrame {
    pub instances: Vec<WeatherInstance>,
    pub drops: usize,
    pub splashes: usize,
}
#[derive(Clone, Copy)]
struct Rng(u64);
impl Rng {
    fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        ((z ^ (z >> 31)) >> 40) as f32 / 16777216.
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.unit()
    }
}
#[derive(Clone, Copy)]
enum Cutoff {
    Pending,
    Clear,
    Hit(WeatherHit),
}
struct Drop {
    position: Vec3,
    speed: f32,
    mass: f32,
    phase: f32,
    atlas: u32,
    initialized: bool,
    cutoff: Cutoff,
    impacted: bool,
    rng: Rng,
}
struct System {
    placement: usize,
    definition: usize,
    drops: Vec<Drop>,
}
struct Splash {
    position: Vec3,
    definition: usize,
    created: f64,
    atlas: u32,
}
pub struct WeatherWorld {
    pack: Arc<WeatherPack>,
    limits: WeatherLimits,
    seed: u64,
    map_id: Option<String>,
    systems: Vec<System>,
    splashes: Vec<Splash>,
    density: f32,
    environment: WeatherEnvironment,
    camera: CameraState,
    last_camera: Option<CameraState>,
    revision: Option<u64>,
    time: f64,
    remainder: f64,
    diagnostics: WeatherDiagnostics,
}
fn stable_hash(s: &str) -> u64 {
    s.as_bytes().iter().fold(0xcbf29ce484222325, |v, b| {
        (v ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
fn center(p: &Placement, c: &CameraState) -> Vec3 {
    if p.follow_camera {
        c.position + c.forward * Vec3::new(p.width, p.height, p.width) * 0.25
    } else {
        Vec3::from_array(p.position)
    }
}
fn extent(p: &Placement) -> Vec3 {
    Vec3::new(p.width, p.height, p.width)
}
fn velocity(d: &Drop, p: &Placement, environment: WeatherEnvironment, tick: f32) -> Vec3 {
    let wind = if p.use_wind {
        environment.wind_velocity
    } else {
        Vec3::ZERO
    };
    wind / d.mass - Vec3::Y * (d.speed / tick)
}
fn initialize(d: &mut Drop, p: &Placement, def: &Definition, center: Vec3) {
    d.position = center
        + Vec3::new(
            d.rng.range(-0.5, 0.5) * p.width,
            d.rng.range(-0.5, 0.5) * p.height,
            d.rng.range(-0.5, 0.5) * p.width,
        );
    d.speed = d.rng.range(p.speed_per_tick[0], p.speed_per_tick[1]);
    d.mass = d.rng.range(p.mass[0], p.mass[1]);
    d.phase = d.rng.unit() * std::f32::consts::TAU;
    d.atlas = (d.rng.unit() * (def.drops_per_side * def.drops_per_side) as f32) as u32;
    d.initialized = true;
    d.cutoff = Cutoff::Pending;
    d.impacted = false;
}
fn atlas_uv(side: u32, index: u32) -> [f32; 4] {
    let x = (index % side) as f32 / side as f32;
    let y = (index / side) as f32 / side as f32;
    [x, y, x + 1. / side as f32, y + 1. / side as f32]
}
impl WeatherWorld {
    pub fn new(pack: Arc<WeatherPack>, limits: WeatherLimits, seed: u64) -> Result<Self> {
        ensure!(
            limits.drops > 0
                && limits.drops <= 65536
                && limits.splashes <= 65536
                && limits.queries_per_advance > 0
                && limits.queries_per_advance <= 131072
                && limits.steps_per_advance > 0
                && limits.steps_per_advance <= 128,
            "Invalid weather limits"
        );
        Ok(Self {
            pack,
            limits,
            seed,
            map_id: None,
            systems: Vec::new(),
            splashes: Vec::new(),
            density: 1.,
            environment: WeatherEnvironment::default(),
            camera: CameraState::default(),
            last_camera: None,
            revision: None,
            time: 0.,
            remainder: 0.,
            diagnostics: WeatherDiagnostics::default(),
        })
    }
    pub fn pack(&self) -> &Arc<WeatherPack> {
        &self.pack
    }
    pub fn diagnostics(&self) -> WeatherDiagnostics {
        self.diagnostics
    }
    pub fn drop_count(&self) -> usize {
        self.systems.iter().map(|s| s.drops.len()).sum()
    }
    pub fn splash_count(&self) -> usize {
        self.splashes.len()
    }
    pub fn map_id(&self) -> Option<&str> {
        self.map_id.as_deref()
    }
    /// Dry maps legitimately select zero systems. Call only with the host's verified map ID.
    pub fn set_map(&mut self, id: &str) -> Result<usize> {
        ensure!(!id.is_empty() && id.len() <= 1024, "Invalid weather map ID");
        self.clear();
        self.map_id = Some(id.into());
        for (i, p) in self
            .pack
            .manifest
            .placements
            .iter()
            .enumerate()
            .filter(|(_, p)| p.map_id == id)
        {
            self.systems.push(System {
                placement: i,
                definition: self.pack.definition_index[&p.definition],
                drops: Vec::new(),
            });
        }
        self.resize();
        Ok(self.systems.len())
    }
    pub fn clear(&mut self) {
        self.map_id = None;
        self.systems.clear();
        self.splashes.clear();
        self.last_camera = None;
        self.revision = None;
        self.time = 0.;
        self.remainder = 0.;
        self.diagnostics = WeatherDiagnostics::default();
    }
    pub fn set_density(&mut self, density: f32) -> Result<()> {
        ensure!(
            density.is_finite() && (0.0..=1.).contains(&density),
            "Invalid weather density"
        );
        self.density = density;
        self.resize();
        if density == 0. {
            self.splashes.clear();
        }
        Ok(())
    }
    pub fn set_environment(&mut self, environment: WeatherEnvironment) -> Result<()> {
        ensure!(
            environment.wind_velocity.is_finite()
                && environment.wind_velocity.length_squared() <= 1e8,
            "Invalid weather wind"
        );
        if environment.wind_velocity != self.environment.wind_velocity {
            self.invalidate_collision();
            self.environment = environment;
        }
        Ok(())
    }
    pub fn invalidate_collision(&mut self) {
        for s in &mut self.systems {
            for d in &mut s.drops {
                d.cutoff = Cutoff::Pending;
            }
        }
        self.splashes.clear();
        self.diagnostics.invalidations += 1;
    }
    fn resize(&mut self) {
        let mut budget = self.limits.drops;
        self.diagnostics.drop_capacity_clipped = 0;
        for s in &mut self.systems {
            let p = &self.pack.manifest.placements[s.placement];
            let desired = (p.drops as f32 * self.density).round() as usize;
            let count = desired.min(budget);
            budget -= count;
            self.diagnostics.drop_capacity_clipped += desired - count;
            s.drops.truncate(count);
            while s.drops.len() < count {
                s.drops.push(Drop {
                    position: Vec3::ZERO,
                    speed: 0.,
                    mass: 1.,
                    phase: 0.,
                    atlas: 0,
                    initialized: false,
                    cutoff: Cutoff::Pending,
                    impacted: false,
                    rng: Rng(self.seed
                        ^ stable_hash(&p.id)
                        ^ ((s.drops.len() as u64 + 1).wrapping_mul(0xa0761d6478bd642f))),
                });
            }
        }
    }
    /// `world_revision` must change when collision roofs/bricks/water surfaces change.
    /// Callback returns the closest hit along the supplied world-space segment.
    pub fn advance(
        &mut self,
        dt: f64,
        camera: CameraState,
        world_revision: u64,
        query: &mut impl FnMut(CollisionRay) -> Option<WeatherHit>,
    ) -> Result<()> {
        ensure!(
            dt.is_finite() && (0.0..=86400.).contains(&dt),
            "Invalid weather elapsed time"
        );
        camera.validate()?;
        let teleport = self.last_camera.is_some_and(|old| {
            self.systems.iter().any(|s| {
                let p = &self.pack.manifest.placements[s.placement];
                p.follow_camera
                    && old.position.distance(camera.position) > p.width.min(p.height) * 0.5
            })
        });
        if self.revision != Some(world_revision) {
            self.invalidate_collision();
            self.revision = Some(world_revision);
        }
        if teleport {
            for s in &mut self.systems {
                for d in &mut s.drops {
                    d.initialized = false;
                }
            }
            self.splashes.clear();
            self.diagnostics.teleports += 1;
        }
        self.camera = camera;
        self.last_camera = Some(camera);
        let tick = (f64::from(self.pack.manifest.legacy_tick_seconds) * 1e6).round() / 1e6;
        self.remainder += dt;
        let ticks = ((self.remainder + 1e-10) / tick).floor() as usize;
        self.remainder = (self.remainder - ticks as f64 * tick).max(0.);
        let steps = ticks.min(self.limits.steps_per_advance);
        if ticks > steps {
            let skipped = (ticks - steps) as f64 * tick;
            self.time += skipped;
            self.diagnostics.skipped_cosmetic_seconds += skipped;
            self.splashes.clear();
            for s in &mut self.systems {
                for d in &mut s.drops {
                    d.initialized = false;
                }
            }
        }
        let mut queries = self.limits.queries_per_advance;
        // Also initialize/validate immediately at dt=0, so map activation need not fake a tick.
        self.update_drops(0., &mut queries, query);
        for _ in 0..steps {
            self.time += tick;
            self.diagnostics.steps += 1;
            self.update_drops(tick as f32, &mut queries, query);
        }
        self.splashes.retain(|s| {
            self.time + self.remainder - s.created
                < f64::from(self.pack.manifest.definitions[s.definition].splash_seconds)
        });
        self.diagnostics.pending_queries = self
            .systems
            .iter()
            .flat_map(|s| &s.drops)
            .filter(|d| matches!(d.cutoff, Cutoff::Pending))
            .count();
        Ok(())
    }
    fn update_drops(
        &mut self,
        dt: f32,
        queries: &mut usize,
        query: &mut impl FnMut(CollisionRay) -> Option<WeatherHit>,
    ) {
        let tick = self.pack.manifest.legacy_tick_seconds;
        for s in &mut self.systems {
            let p = &self.pack.manifest.placements[s.placement];
            let def = &self.pack.manifest.definitions[s.definition];
            let center = center(p, &self.camera);
            let size = extent(p);
            let min = center - size * 0.5;
            let max = center + size * 0.5;
            for d in &mut s.drops {
                if !d.initialized {
                    initialize(d, p, def, center);
                }
                let vel = velocity(d, p, self.environment, tick);
                let old = d.position;
                d.position += vel * dt;
                d.phase = (d.phase + p.turbulence_radians_per_tick * dt / tick)
                    .rem_euclid(std::f32::consts::TAU);
                let wrapped = d.position.cmplt(min).any() || d.position.cmpgt(max).any();
                if wrapped {
                    let below = d.position.y < min.y;
                    let position = Vec3::new(
                        min.x + (d.position.x - min.x).rem_euclid(size.x),
                        min.y + (d.position.y - min.y).rem_euclid(size.y),
                        min.z + (d.position.z - min.z).rem_euclid(size.z),
                    );
                    if below {
                        initialize(d, p, def, center);
                        d.position.y = position.y;
                    } else {
                        d.position = position;
                        d.cutoff = Cutoff::Pending;
                        d.impacted = false;
                    }
                    self.diagnostics.wraps += 1;
                }
                if !p.collision {
                    d.cutoff = Cutoff::Clear;
                    d.impacted = false;
                }
                if matches!(d.cutoff, Cutoff::Pending) && *queries > 0 {
                    let direction =
                        velocity(d, p, self.environment, tick).normalize_or(Vec3::NEG_Y);
                    let ray = CollisionRay {
                        start: d.position - direction * 500.,
                        end: d.position + direction * 100.,
                    };
                    let hit = query(ray);
                    *queries -= 1;
                    self.diagnostics.collision_queries += 1;
                    if let Some(hit) = hit {
                        let delta = ray.end - ray.start;
                        let t = (hit.position - ray.start).dot(delta) / delta.length_squared();
                        let on_ray = ray.start + delta * t;
                        if !hit.position.is_finite()
                            || !hit.normal.is_normalized()
                            || !(-0.0001..=1.0001).contains(&t)
                            || on_ray.distance_squared(hit.position) > 0.01
                        {
                            self.diagnostics.invalid_hits += 1;
                            continue;
                        }
                        d.impacted = (hit.position - d.position).dot(direction) < 0.;
                        d.cutoff = Cutoff::Hit(hit);
                    } else {
                        d.cutoff = Cutoff::Clear;
                        d.impacted = false;
                    }
                }
                if !wrapped
                    && !d.impacted
                    && let Cutoff::Hit(hit) = d.cutoff
                {
                    let direction = vel.normalize_or(Vec3::NEG_Y);
                    if (hit.position - old).dot(direction) >= 0.
                        && (hit.position - d.position).dot(direction) <= 0.
                    {
                        d.impacted = true;
                        self.diagnostics.impacts += 1;
                        if hit.surface == WeatherSurface::Water {
                            self.diagnostics.water_impacts += 1;
                        }
                        if def.splash_texture.is_some() && def.splash_seconds > 0. {
                            if self.splashes.len() < self.limits.splashes {
                                self.splashes.push(Splash {
                                    position: hit.position,
                                    definition: s.definition,
                                    created: self.time,
                                    atlas: (d.rng.unit()
                                        * (def.splashes_per_side * def.splashes_per_side) as f32)
                                        as u32,
                                });
                                self.diagnostics.splashes_created += 1;
                            } else {
                                self.diagnostics.splash_capacity_drops += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    /// The player's camera the drops fall around.
    pub fn camera(&self) -> CameraState {
        self.camera
    }
    pub fn snapshot(&self) -> WeatherFrame {
        self.snapshot_from(&self.camera)
    }
    /// This frame's drops as `camera` sees them: facing it and sorted far to
    /// near from it. The drops themselves fall around the player's camera
    /// (the last `CameraState` given); another view (a mirror's) passes its own.
    pub fn snapshot_from(&self, camera: &CameraState) -> WeatherFrame {
        let mut frame = WeatherFrame::default();
        let tick = self.pack.manifest.legacy_tick_seconds;
        for s in &self.systems {
            let p = &self.pack.manifest.placements[s.placement];
            let def = &self.pack.manifest.definitions[s.definition];
            for d in &s.drops {
                if !d.initialized || d.impacted || matches!(d.cutoff, Cutoff::Pending) {
                    continue;
                }
                let vel = velocity(d, p, self.environment, tick);
                let base = d.position + vel * self.remainder as f32;
                if let Cutoff::Hit(hit) = d.cutoff
                    && (hit.position - base).dot(vel) < 0.
                {
                    continue;
                }
                let phase = d.phase + p.turbulence_radians_per_tick * self.remainder as f32 / tick;
                let position = base
                    + if p.use_turbulence {
                        Vec3::new(phase.sin(), 0., -phase.cos()) * (p.turbulence_amplitude / d.mass)
                    } else {
                        Vec3::ZERO
                    };
                let to_camera = camera.position - position;
                let distance = to_camera.length();
                let view = to_camera.normalize_or(Vec3::Z);
                let (right, up) = if def.true_billboards {
                    (camera.right, camera.up)
                } else {
                    let mut v = vel * tick;
                    if p.rotate_with_camera_velocity {
                        v -= camera.velocity / distance.max(2.) * 0.3;
                    }
                    let v = v.normalize_or(Vec3::NEG_Y);
                    let right = (-v).cross(view).normalize_or(camera.right);
                    let up = (view.cross(right) * 0.5 - v * 0.5).normalize_or(camera.up);
                    (right, up)
                };
                let atlas = if def.drop_animation_seconds > 0. {
                    (((self.time + self.remainder) / f64::from(def.drop_animation_seconds)).fract()
                        * (def.drops_per_side * def.drops_per_side) as f64)
                        as u32
                } else {
                    d.atlas
                };
                frame.instances.push(WeatherInstance {
                    position,
                    right: right * def.drop_radius,
                    up: up * def.drop_radius,
                    uv: atlas_uv(def.drops_per_side, atlas),
                    texture: self.pack.texture_index[&def.drop_texture] as u32,
                    color: [1.; 4],
                    splash: false,
                });
                frame.drops += 1;
            }
        }
        for s in &self.splashes {
            let d = &self.pack.manifest.definitions[s.definition];
            let age = ((self.time + self.remainder - s.created) / f64::from(d.splash_seconds))
                .clamp(0., 1.);
            if age >= 1. {
                continue;
            }
            let count = d.splashes_per_side * d.splashes_per_side;
            let atlas = if d.animate_splashes {
                (age * count as f64) as u32
            } else {
                s.atlas
            };
            frame.instances.push(WeatherInstance {
                position: s.position,
                right: camera.right * d.splash_radius,
                up: camera.up * d.splash_radius,
                uv: atlas_uv(d.splashes_per_side, atlas),
                texture: self.pack.texture_index[d.splash_texture.as_ref().unwrap()] as u32,
                color: [1.; 4],
                splash: true,
            });
            frame.splashes += 1;
        }
        frame.instances.sort_by(|a, b| {
            camera
                .position
                .distance_squared(b.position)
                .total_cmp(&camera.position.distance_squared(a.position))
        });
        frame
    }
}
