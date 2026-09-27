use crate::EffectsPack;
use anyhow::{Context, Result, ensure};
use glam::{Mat4, Quat, Vec3, Vec4};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectHandle(pub u64);
#[derive(Clone, Copy, Debug)]
pub struct SourceTransform {
    pub position: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
}
impl Default for SourceTransform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            velocity: Vec3::ZERO,
        }
    }
}
impl SourceTransform {
    fn validate(self) -> Result<()> {
        ensure!(
            self.position.is_finite()
                && self.velocity.is_finite()
                && self.rotation.is_finite()
                && (self.rotation.length_squared() - 1.).abs() < 0.001,
            "Invalid effects transform"
        );
        Ok(())
    }
    fn interpolate(self, end: Self, t: f32) -> Self {
        Self {
            position: self.position.lerp(end.position, t),
            rotation: self.rotation.slerp(end.rotation, t),
            velocity: self.velocity.lerp(end.velocity, t),
        }
    }
}
#[derive(Clone, Debug)]
pub struct SourceOptions {
    /// Cosmetic emission clock multiplier. Attached projectile/player sources use 1.
    pub time_scale: f32,
    /// Local rectangular placement volume; zero gives a point source.
    pub half_extents: Vec3,
    /// Per-source local wind added to the world wind vector.
    pub wind: Vec3,
    /// Runtime emitter override keys, used only by authored useEmitterColors/Sizes.
    pub colors: Option<[[f32; 4]; 4]>,
    pub sizes: Option<[f32; 4]>,
    /// Script-derived datablock copy (`color<N>Paint*Particle`): replaces the
    /// authored RGB on every key, keeping the authored alpha keys.
    pub recolor: Option<Recolor>,
    /// False pauses new emission; existing particles drain normally.
    pub emitting: bool,
    pub visible: bool,
    /// Suppress third-person-only flares on the local first-person owner.
    pub first_person_owner: bool,
    /// Host-computed occlusion fraction; fades over authored flare fade time.
    pub flare_visibility: f32,
}
impl Default for SourceOptions {
    fn default() -> Self {
        Self {
            time_scale: 1.,
            half_extents: Vec3::ZERO,
            wind: Vec3::ZERO,
            colors: None,
            sizes: None,
            recolor: None,
            emitting: true,
            visible: true,
            first_person_owner: false,
            flare_visibility: 1.,
        }
    }
}
impl SourceOptions {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.time_scale.is_finite()
                && self.time_scale > 0.
                && self.time_scale <= 1000.
                && self.half_extents.is_finite()
                && self.half_extents.min_element() >= 0.
                && self.wind.is_finite()
                && self.flare_visibility.is_finite()
                && (0.0..=1.0).contains(&self.flare_visibility),
            "Invalid source options"
        );
        ensure!(
            self.colors
                .is_none_or(|c| c.iter().flatten().all(|v| v.is_finite() && *v >= 0.))
                && self
                    .sizes
                    .is_none_or(|s| s.iter().all(|v| v.is_finite() && *v >= 0.))
                && self
                    .recolor
                    .is_none_or(|r| r.rgb.iter().all(|v| v.is_finite() && *v >= 0.)),
            "Invalid source override keys"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Recolor {
    pub rgb: [f32; 3],
    /// Overrides the particle's authored `useInvAlpha` when set.
    pub blend: Option<BlendMode>,
}
#[derive(Clone, Copy, Debug)]
pub struct EffectsLimits {
    pub sources: usize,
    pub particles: usize,
    pub lights: usize,
    pub emissions_per_advance: usize,
}
impl Default for EffectsLimits {
    fn default() -> Self {
        Self {
            sources: 4096,
            particles: 65536,
            lights: 256,
            emissions_per_advance: 32768,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct Diagnostics {
    pub emitted: u64,
    pub particle_capacity_drops: u64,
    pub emission_budget_skips: u64,
    pub source_capacity_rejections: u64,
    pub peak_particles: usize,
    pub advances: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopMode {
    Drain,
    Immediate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Alpha,
    Additive,
    AdditiveColor,
}
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub view_projection: Mat4,
    pub position: Vec3,
    pub right: Vec3,
    pub up: Vec3,
}
#[derive(Clone, Copy, Debug)]
pub struct ParticleInstance {
    pub position: Vec3,
    pub size: f32,
    pub color: Vec4,
    pub spin: f32,
    /// Zero means camera-facing. Nonzero means a camera-facing ribbon along this axis.
    pub axis: Vec3,
    pub texture: u32,
    pub blend: BlendMode,
    pub depth_test: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct LightSnapshot {
    pub handle: EffectHandle,
    pub position: Vec3,
    pub color: Vec3,
    pub radius: f32,
}
/// Storage-buffer layout: 32 bytes, aligned vec4 fields. color is brightness-scaled.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    pub position_radius: [f32; 4],
    pub color: [f32; 4],
}
impl From<LightSnapshot> for GpuLight {
    fn from(l: LightSnapshot) -> Self {
        Self {
            position_radius: l.position.extend(l.radius).to_array(),
            color: l.color.extend(0.).to_array(),
        }
    }
}
#[derive(Default)]
pub struct FrameEffects {
    pub particles: Vec<ParticleInstance>,
    pub lights: Vec<LightSnapshot>,
}

#[derive(Clone)]
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
    fn variance(&mut self, value: f32, variance: f32) -> f32 {
        self.range(value - variance, value + variance)
    }
}
struct Source {
    definition: usize,
    light: bool,
    previous: SourceTransform,
    transform: SourceTransform,
    options: SourceOptions,
    rng: Rng,
    age: f64,
    next: f64,
    lifetime: f64,
    flare: f32,
}
struct Particle {
    owner: EffectHandle,
    definition: usize,
    position: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
    direction: Vec3,
    age: f32,
    lifetime: f32,
    spin: f32,
    wind: Vec3,
    orient: bool,
    orient_velocity: bool,
    blend: BlendMode,
    colors: Option<[[f32; 4]; 4]>,
    sizes: Option<[f32; 4]>,
    rgb: Option<[f32; 3]>,
    visible: bool,
}
pub struct EffectsWorld {
    pack: Arc<EffectsPack>,
    limits: EffectsLimits,
    sources: BTreeMap<EffectHandle, Source>,
    particles: Vec<Particle>,
    next_handle: u64,
    seed: u64,
    diagnostics: Diagnostics,
}
impl EffectsWorld {
    pub fn new(pack: Arc<EffectsPack>, limits: EffectsLimits, seed: u64) -> Result<Self> {
        ensure!(
            limits.sources > 0
                && limits.sources <= 65536
                && limits.particles > 0
                && limits.particles <= 1_000_000
                && limits.lights > 0
                && limits.lights <= 4096
                && limits.emissions_per_advance > 0
                && limits.emissions_per_advance <= 1_000_000,
            "Invalid effects limits"
        );
        Ok(Self {
            pack,
            limits,
            sources: BTreeMap::new(),
            particles: Vec::new(),
            next_handle: 1,
            seed,
            diagnostics: Diagnostics::default(),
        })
    }
    pub fn pack(&self) -> &Arc<EffectsPack> {
        &self.pack
    }
    pub fn diagnostics(&self) -> Diagnostics {
        self.diagnostics
    }
    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }
    pub fn is_active(&self, h: EffectHandle) -> bool {
        self.sources.contains_key(&h)
    }
    pub fn start_emitter(
        &mut self,
        id: &str,
        transform: SourceTransform,
        options: SourceOptions,
    ) -> Result<EffectHandle> {
        let i = *self
            .pack
            .emitter_index
            .get(id)
            .with_context(|| format!("Unknown native emitter {id}"))?;
        self.start(i, false, transform, options)
    }
    pub fn start_light(
        &mut self,
        id: &str,
        transform: SourceTransform,
        options: SourceOptions,
    ) -> Result<EffectHandle> {
        let i = *self
            .pack
            .light_index
            .get(id)
            .with_context(|| format!("Unknown native light {id}"))?;
        ensure!(
            self.sources.values().filter(|s| s.light).count() < self.limits.lights,
            "Light budget exhausted"
        );
        self.start(i, true, transform, options)
    }
    fn start(
        &mut self,
        definition: usize,
        light: bool,
        transform: SourceTransform,
        options: SourceOptions,
    ) -> Result<EffectHandle> {
        transform.validate()?;
        options.validate()?;
        if self.sources.len() >= self.limits.sources {
            self.diagnostics.source_capacity_rejections += 1;
            anyhow::bail!("Effects source budget exhausted");
        }
        let handle = EffectHandle(self.next_handle);
        self.next_handle = self
            .next_handle
            .checked_add(1)
            .context("Effect handle space exhausted")?;
        let mut rng = Rng(self.seed ^ handle.0.wrapping_mul(0xa0761d6478bd642f));
        let (next, lifetime) = if light {
            (0., f64::INFINITY)
        } else {
            let e = &self.pack.library.emitters[definition];
            (
                f64::from(rng.variance(e.period, e.period_variance)),
                if e.lifetime == 0. {
                    f64::INFINITY
                } else {
                    f64::from(rng.variance(e.lifetime, e.lifetime_variance).max(0.))
                },
            )
        };
        self.sources.insert(
            handle,
            Source {
                definition,
                light,
                previous: transform,
                transform,
                flare: options.flare_visibility,
                options,
                rng,
                age: 0.,
                next,
                lifetime,
            },
        );
        Ok(handle)
    }
    pub fn update_source(
        &mut self,
        handle: EffectHandle,
        transform: SourceTransform,
    ) -> Result<()> {
        transform.validate()?;
        self.sources
            .get_mut(&handle)
            .context("Stale effect handle")?
            .transform = transform;
        Ok(())
    }
    pub fn update_options(&mut self, handle: EffectHandle, options: SourceOptions) -> Result<()> {
        options.validate()?;
        self.sources
            .get_mut(&handle)
            .context("Stale effect handle")?
            .options = options;
        Ok(())
    }
    /// Cap a source's remaining emission/light lifetime without extending its
    /// authored lifetime. Existing particles drain at their original lifetimes.
    /// The cap is evaluated inside advance, including advances longer than it.
    pub fn set_remaining_lifetime(&mut self, handle: EffectHandle, seconds: f32) -> Result<()> {
        ensure!(
            seconds.is_finite() && (0.0..=3600.0).contains(&seconds),
            "Invalid remaining effect lifetime"
        );
        let source = self
            .sources
            .get_mut(&handle)
            .context("Stale effect handle")?;
        source.lifetime = source.lifetime.min(source.age + f64::from(seconds));
        Ok(())
    }
    pub fn stop(&mut self, handle: EffectHandle, mode: StopMode) -> bool {
        let existed = self.sources.remove(&handle).is_some();
        if mode == StopMode::Immediate {
            self.particles.retain(|p| p.owner != handle);
        }
        existed
    }
    pub fn teardown(&mut self) {
        self.sources.clear();
        self.particles.clear();
    }
    /// Play a source-backed explosion group. Host owns damage, audio, camera shake and debris.
    pub fn play_composite(
        &mut self,
        id: &str,
        transform: SourceTransform,
        options: SourceOptions,
    ) -> Result<Vec<EffectHandle>> {
        let c = self
            .pack
            .manifest
            .composites
            .iter()
            .find(|c| c.id == id)
            .with_context(|| format!("Unknown native composite {id}"))?
            .clone();
        let required =
            c.emitters.len() + usize::from(c.light.is_some()) + usize::from(c.burst.is_some());
        ensure!(
            self.sources.len() + required <= self.limits.sources,
            "Composite source budget exhausted"
        );
        if c.light.is_some() {
            ensure!(
                self.sources.values().filter(|s| s.light).count() < self.limits.lights,
                "Composite light budget exhausted"
            );
        }
        if let Some((_, count, _)) = &c.burst {
            ensure!(
                *count as usize <= self.limits.emissions_per_advance,
                "Composite burst exceeds budget"
            );
        }
        let mut handles = Vec::new();
        for id in &c.emitters {
            let h = self.start_emitter(id, transform, options.clone())?;
            let s = self.sources.get_mut(&h).unwrap();
            s.lifetime = s.lifetime.min(f64::from(c.lifetime));
            handles.push(h);
        }
        if let Some(id) = &c.light {
            let h = self.start_light(id, transform, options.clone())?;
            self.sources.get_mut(&h).unwrap().lifetime = f64::from(c.lifetime);
            handles.push(h);
        }
        if let Some((id, count, radius)) = &c.burst {
            let mut options = options;
            options.half_extents = Vec3::splat(*radius);
            handles.push(self.burst(id, transform, options, *count as usize)?);
        }
        Ok(handles)
    }
    /// Instantaneous one-shot emission. Returns a handle that can immediately remove its tail.
    pub fn burst(
        &mut self,
        id: &str,
        transform: SourceTransform,
        options: SourceOptions,
        count: usize,
    ) -> Result<EffectHandle> {
        ensure!(
            count <= self.limits.emissions_per_advance,
            "Burst exceeds emission budget"
        );
        let handle = self.start_emitter(id, transform, options)?;
        let mut source = self.sources.remove(&handle).unwrap();
        for _ in 0..count {
            self.emit(handle, &mut source, transform, 0., Vec3::ZERO);
        }
        Ok(handle)
    }
    /// Host supplies elapsed cosmetic time and a native Y-up world wind vector.
    /// Dead particles expire at full elapsed time. Work on overdue emissions is bounded.
    pub fn advance(&mut self, dt: f32, wind: Vec3) -> Result<()> {
        ensure!(
            dt.is_finite() && (0.0..=86400.0).contains(&dt) && wind.is_finite(),
            "Invalid effects timestep/wind"
        );
        if dt == 0. {
            return Ok(());
        }
        self.diagnostics.advances += 1;
        for p in &mut self.particles {
            if let Some(source) = self.sources.get(&p.owner) {
                p.wind = source.options.wind;
                p.visible = source.options.visible;
                let e = &self.pack.library.emitters[source.definition];
                if e.use_emitter_colors {
                    p.colors = source.options.colors;
                }
                if e.use_emitter_sizes {
                    p.sizes = source.options.sizes;
                }
            }
            Self::integrate(&self.pack, p, dt, wind);
        }
        self.particles
            .retain(|p| p.age < p.lifetime && p.position.is_finite());
        let mut budget = self.limits.emissions_per_advance;
        let handles: Vec<_> = self.sources.keys().copied().collect();
        for handle in handles {
            let mut source = self.sources.remove(&handle).unwrap();
            let end = source.age + f64::from(dt * source.options.time_scale);
            if source.light {
                let fade = self.pack.library.lights[source.definition]
                    .flare
                    .as_ref()
                    .map_or(0., |f| f.fade_seconds);
                let step = if fade > 0. { dt / fade } else { 1. };
                source.flare += (source.options.flare_visibility - source.flare).clamp(-step, step);
            } else if source.options.emitting {
                while source.next <= end && source.next <= source.lifetime && budget > 0 {
                    let t = ((source.next - source.age) / (end - source.age)).clamp(0., 1.) as f32;
                    let transform = source.previous.interpolate(source.transform, t);
                    let pre_age = if self.pack.library.emitters[source.definition].override_advance
                    {
                        0.
                    } else {
                        dt * (1. - t)
                    };
                    self.emit(handle, &mut source, transform, pre_age, wind);
                    let e = &self.pack.library.emitters[source.definition];
                    source.next += f64::from(source.rng.variance(e.period, e.period_variance));
                    budget -= 1;
                }
                if source.next <= end && source.next <= source.lifetime {
                    let e = &self.pack.library.emitters[source.definition];
                    let skipped = (((end.min(source.lifetime) - source.next) / f64::from(e.period))
                        .floor()
                        .max(0.) as u64)
                        .saturating_add(1);
                    self.diagnostics.emission_budget_skips = self
                        .diagnostics
                        .emission_budget_skips
                        .saturating_add(skipped);
                    source.next = end + f64::from(e.period);
                }
            } else {
                source.next = end + f64::from(self.pack.library.emitters[source.definition].period);
            }
            source.age = end;
            source.previous = source.transform;
            if source.age < source.lifetime {
                self.sources.insert(handle, source);
            }
        }
        Ok(())
    }
    fn emit(
        &mut self,
        handle: EffectHandle,
        s: &mut Source,
        t: SourceTransform,
        pre_age: f32,
        wind: Vec3,
    ) {
        let e = &self.pack.library.emitters[s.definition];
        let definition = self.pack.particle_index
            [&e.particles[(s.rng.unit() * e.particles.len() as f32) as usize]];
        let p = &self.pack.library.particles[definition];
        let theta = s
            .rng
            .range(e.theta_degrees[0], e.theta_degrees[1])
            .to_radians();
        let phi = (e.phi_rate_degrees * s.next as f32 + s.rng.unit() * e.phi_variance_degrees)
            .to_radians();
        let direction = t.rotation
            * Vec3::new(
                theta.sin() * phi.cos(),
                theta.cos(),
                theta.sin() * phi.sin(),
            );
        let placement = Vec3::new(
            s.rng.range(-1., 1.),
            s.rng.range(-1., 1.),
            s.rng.range(-1., 1.),
        ) * s.options.half_extents;
        let velocity_axis = if e.use_placement_velocity && placement.length_squared() > 1e-12 {
            t.rotation * placement.normalize()
        } else {
            direction
        };
        let velocity = velocity_axis * s.rng.variance(e.speed, e.speed_variance)
            + t.velocity * p.inherited_velocity;
        let mut particle = Particle {
            owner: handle,
            definition,
            position: t.position
                + t.rotation * placement
                + direction * s.rng.variance(e.offset, e.offset_variance),
            velocity,
            acceleration: velocity * p.acceleration,
            direction,
            age: 0.,
            lifetime: s.rng.variance(p.lifetime, p.lifetime_variance),
            spin: (p.spin_degrees + s.rng.range(p.random_spin[0], p.random_spin[1])).to_radians(),
            wind: s.options.wind,
            orient: e.orient,
            orient_velocity: e.orient_on_velocity,
            blend: if let Some(blend) = s.options.recolor.and_then(|r| r.blend) {
                blend
            } else if self
                .pack
                .manifest
                .emitter_alpha
                .get(&e.id)
                .copied()
                .unwrap_or(p.alpha_blend)
            {
                BlendMode::Alpha
            } else {
                BlendMode::Additive
            },
            colors: if e.use_emitter_colors {
                s.options.colors
            } else {
                None
            },
            sizes: if e.use_emitter_sizes {
                s.options.sizes
            } else {
                None
            },
            rgb: s.options.recolor.map(|r| r.rgb),
            visible: s.options.visible,
        };
        Self::integrate(&self.pack, &mut particle, pre_age, wind);
        if particle.age >= particle.lifetime {
            return;
        }
        if self.particles.len() >= self.limits.particles {
            self.diagnostics.particle_capacity_drops += 1;
            return;
        }
        self.particles.push(particle);
        self.diagnostics.emitted += 1;
        self.diagnostics.peak_particles = self.diagnostics.peak_particles.max(self.particles.len());
    }
    fn integrate(pack: &EffectsPack, p: &mut Particle, dt: f32, wind: Vec3) {
        p.age += dt;
        if p.age >= p.lifetime {
            return;
        }
        let def = &pack.library.particles[p.definition];
        let acceleration =
            p.acceleration - (wind + p.wind) * def.wind - Vec3::Y * (9.81 * def.gravity);
        // Closed-form constant-force linear drag, stable for stalls and arbitrary frame partitioning.
        if def.drag > 1e-5 {
            let decay = (-def.drag * dt).exp();
            let terminal = acceleration / def.drag;
            p.position += terminal * dt + (p.velocity - terminal) * ((1. - decay) / def.drag);
            p.velocity = terminal + (p.velocity - terminal) * decay;
        } else {
            p.position += p.velocity * dt + acceleration * (0.5 * dt * dt);
            p.velocity += acceleration * dt;
        }
    }
    pub fn snapshot(&self, camera: &Camera) -> FrameEffects {
        let mut frame = FrameEffects {
            particles: Vec::with_capacity(self.particles.len()),
            lights: Vec::new(),
        };
        for p in &self.particles {
            if !p.visible {
                continue;
            }
            let def = &self.pack.library.particles[p.definition];
            let age = p.age / p.lifetime;
            let (mut color, mut size) = def.sample(age);
            let index = def
                .keys
                .windows(2)
                .position(|k| age <= k[1].time)
                .unwrap_or(def.keys.len() - 2);
            let a = &def.keys[index];
            let b = &def.keys[index + 1];
            let weight = if b.time > a.time {
                (age - a.time) / (b.time - a.time)
            } else {
                0.
            };
            if let Some(colors) = p.colors {
                color = std::array::from_fn(|c| {
                    colors[index][c] + (colors[index + 1][c] - colors[index][c]) * weight
                });
            }
            if let Some(rgb) = p.rgb {
                color[..3].copy_from_slice(&rgb);
            }
            if let Some(sizes) = p.sizes {
                size = sizes[index] + (sizes[index + 1] - sizes[index]) * weight;
            }
            let axis = if p.orient {
                if p.orient_velocity {
                    p.velocity.normalize_or_zero()
                } else {
                    p.direction
                }
            } else {
                Vec3::ZERO
            };
            if p.orient && axis == Vec3::ZERO {
                continue;
            }
            frame.particles.push(ParticleInstance {
                position: p.position,
                size,
                color: Vec4::from_array(color),
                spin: p.spin * p.age,
                axis,
                texture: self.pack.texture_index[&def.texture] as u32,
                blend: p.blend,
                depth_test: true,
            });
        }
        for (handle, s) in &self.sources {
            if !s.light || !s.options.visible {
                continue;
            }
            let def = &self.pack.library.lights[s.definition];
            let (color, radius) = def.sample(s.age as f32);
            let color = Vec3::from_array(color);
            if radius <= 0. || color.max_element() <= 0. {
                continue;
            }
            frame.lights.push(LightSnapshot {
                handle: *handle,
                position: s.transform.position,
                color,
                radius,
            });
            if let Some(f) = &def.flare {
                if f.third_person && s.options.first_person_owner {
                    continue;
                }
                let distance = camera.position.distance(s.transform.position);
                let weight = ((distance - f.near_distance) / (f.far_distance - f.near_distance))
                    .clamp(0., 1.);
                let size = 2.
                    * s.flare
                    * f.constant_size
                        .unwrap_or(f.near_size + (f.far_size - f.near_size) * weight)
                    * if f.link_size {
                        color.dot(Vec3::new(0.212671, 0.715160, 0.072169))
                    } else {
                        1.
                    };
                frame.particles.push(ParticleInstance {
                    position: s.transform.position,
                    size,
                    color: if f.link_color {
                        color
                    } else {
                        Vec3::from_array(f.color)
                    }
                    .extend(1.),
                    spin: 0.,
                    axis: Vec3::ZERO,
                    texture: self.pack.texture_index[&f.texture] as u32,
                    blend: match f.blend_mode {
                        1 => BlendMode::Alpha,
                        2 => BlendMode::AdditiveColor,
                        _ => BlendMode::Additive,
                    },
                    depth_test: false,
                });
            }
        }
        // Keep texture runs in this order; regrouping alpha sprites by texture breaks compositing.
        frame.particles.sort_by(|a, b| {
            camera
                .position
                .distance_squared(b.position)
                .total_cmp(&camera.position.distance_squared(a.position))
        });
        frame
    }
}

/// Original numeric wrench direction, converted from Torque (x,y,z) to native (x,z,-y).
pub fn brick_direction(direction: u8) -> Result<Quat> {
    let axis = match direction {
        0 => Vec3::Y,
        1 => Vec3::NEG_Y,
        2 => Vec3::Z,
        3 => Vec3::NEG_X,
        4 => Vec3::NEG_Z,
        5 => Vec3::X,
        _ => anyhow::bail!("Invalid brick emitter direction"),
    };
    Ok(Quat::from_rotation_arc(Vec3::Y, axis))
}
/// Applies the source script's small-brick node choice and world-box thresholds.
/// Placement within the scaled volume is native uniform sampling (v20 engine detail pending).
pub struct BrickAttachment {
    pub center: Vec3,
    pub world_size: Vec3,
    /// Authored Torque brick dimensions (X studs, Y studs, Z plates).
    pub stud_size: [u32; 3],
    pub direction: u8,
    pub paint: [f32; 4],
    pub fake_dead: bool,
}
pub fn brick_source(
    pack: &EffectsPack,
    id: &str,
    brick: &BrickAttachment,
) -> Result<(SourceTransform, SourceOptions)> {
    let BrickAttachment {
        center,
        world_size,
        stud_size,
        direction,
        paint,
        fake_dead,
    } = *brick;
    let e = &pack.library.emitters[*pack
        .emitter_index
        .get(id)
        .context("Unknown brick emitter")?];
    ensure!(
        world_size.is_finite() && world_size.min_element() >= 0.,
        "Invalid brick bounds"
    );
    let rotation = brick_direction(direction)?;
    let world_size = if stud_size == [1, 1, 1] {
        Vec3::ZERO
    } else {
        Vec3::new(
            if world_size.x < 0.55 {
                0.
            } else {
                world_size.x
            },
            if world_size.y < 0.66 {
                0.
            } else {
                world_size.y
            },
            if world_size.z < 0.55 {
                0.
            } else {
                world_size.z
            },
        )
    };
    let local_size = (rotation.inverse() * world_size).abs();
    Ok((
        SourceTransform {
            position: center,
            rotation,
            ..Default::default()
        },
        SourceOptions {
            half_extents: local_size * 0.5,
            time_scale: if stud_size[0] <= 1 && stud_size[1] <= 1 && stud_size[2] <= 3 {
                e.point_node_time_scale
            } else {
                e.node_time_scale
            },
            colors: Some([paint; 4]),
            emitting: !fake_dead,
            ..Default::default()
        },
    ))
}
