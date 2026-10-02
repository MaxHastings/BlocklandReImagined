use crate::EffectsPack;
use anyhow::{Context, Result, ensure};
use bri_content::passage::Passages;
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
#[derive(Clone, Debug, PartialEq)]
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
    /// `ParticleEmitterNode::setColor` (brick paint): with authored
    /// useEmitterColors, replaces the RGB on every key and keeps the particle's
    /// authored alpha keys, so fog stays a faint fading haze in any colour.
    pub paint: Option<[f32; 3]>,
    /// False pauses new emission; existing particles drain normally.
    pub emitting: bool,
    pub visible: bool,
    /// Suppress third-person-only flares on the local first-person owner.
    pub first_person_owner: bool,
    /// Drawn in every view but the owner's own eye
    /// ([`EffectsWorld::snapshot_in_view`]): a first-person player's own
    /// jets, which mirrors and portals still show.
    pub hidden_from_own_eye: bool,
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
            paint: None,
            emitting: true,
            visible: true,
            first_person_owner: false,
            hidden_from_own_eye: false,
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
                    .is_none_or(|r| r.rgb.iter().all(|v| v.is_finite() && *v >= 0.))
                && self
                    .paint
                    .is_none_or(|c| c.iter().all(|v| v.is_finite() && *v >= 0.)),
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
/// Distance at which v20 stops drawing fxLight flares.
pub const FLARE_MAX_DISTANCE: f32 = 75.;

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
#[derive(Clone, Default)]
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
    hidden_from_own_eye: bool,
}
pub struct EffectsWorld {
    pack: Arc<EffectsPack>,
    limits: EffectsLimits,
    sources: BTreeMap<EffectHandle, Source>,
    particles: Vec<Particle>,
    next_handle: u64,
    seed: u64,
    diagnostics: Diagnostics,
    /// A source's options changed since the last advance, so its particles
    /// must pick up the new wind, visibility and override keys.
    options_changed: bool,
    /// The pack's names resolved once: each particle definition's texture,
    /// each emitter's particle definitions and blend override, and each
    /// light's flare texture.
    particle_texture: Vec<u32>,
    /// Each particle definition's largest size, for culling before sampling.
    particle_reach: Vec<f32>,
    emitter_particles: Vec<Vec<usize>>,
    emitter_alpha: Vec<Option<bool>>,
    flare_texture: Vec<Option<u32>>,
    /// The world's portals: a particle that flies in through one goes on
    /// out of its partner (see [`Self::set_passages`]).
    passages: Passages,
}
impl EffectsWorld {
    /// The world's portals ([`bri_content::passage`]), as the player's game
    /// sees them: every particle that flies in through one comes out of
    /// its partner, turned with it, as bodies and shots do. Drawn only.
    pub fn set_passages(&mut self, passages: &Passages) {
        if &self.passages != passages {
            self.passages = passages.clone();
        }
    }
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
        let texture = |id: &str| {
            pack.texture_index
                .get(id)
                .map(|i| *i as u32)
                .with_context(|| format!("Unknown effects texture {id}"))
        };
        let particle_texture = pack
            .library
            .particles
            .iter()
            .map(|p| texture(&p.texture))
            .collect::<Result<_>>()?;
        let particle_reach = pack
            .library
            .particles
            .iter()
            .map(|p| p.keys.iter().fold(0f32, |m, k| m.max(k.size.abs())))
            .collect();
        let emitter_particles = pack
            .library
            .emitters
            .iter()
            .map(|e| {
                e.particles
                    .iter()
                    .map(|id| {
                        pack.particle_index
                            .get(id)
                            .copied()
                            .with_context(|| format!("Unknown particle {id}"))
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .collect::<Result<_>>()?;
        let emitter_alpha = pack
            .library
            .emitters
            .iter()
            .map(|e| pack.manifest.emitter_alpha.get(&e.id).copied())
            .collect();
        let flare_texture = pack
            .library
            .lights
            .iter()
            .map(|l| l.flare.as_ref().map(|f| texture(&f.texture)).transpose())
            .collect::<Result<_>>()?;
        Ok(Self {
            pack,
            limits,
            sources: BTreeMap::new(),
            particles: Vec::new(),
            next_handle: 1,
            seed,
            diagnostics: Diagnostics::default(),
            options_changed: false,
            particle_texture,
            particle_reach,
            emitter_particles,
            emitter_alpha,
            flare_texture,
            passages: Passages::default(),
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
    /// Move a source somewhere it did not travel to (through a portal):
    /// it emits from there on, with no streak back to where it was.
    pub fn jump_source(&mut self, handle: EffectHandle, transform: SourceTransform) -> Result<()> {
        transform.validate()?;
        let source = self
            .sources
            .get_mut(&handle)
            .context("Stale effect handle")?;
        source.transform = transform;
        source.previous = transform;
        Ok(())
    }
    pub fn update_options(&mut self, handle: EffectHandle, options: SourceOptions) -> Result<()> {
        options.validate()?;
        let source = self
            .sources
            .get_mut(&handle)
            .context("Stale effect handle")?;
        if source.options != options {
            source.options = options;
            self.options_changed = true;
        }
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
        // Particles follow their source's current options; only look the
        // sources up when some source's options changed.
        if std::mem::take(&mut self.options_changed) {
            for p in &mut self.particles {
                if let Some(source) = self.sources.get(&p.owner) {
                    p.wind = source.options.wind;
                    p.visible = source.options.visible;
                    p.hidden_from_own_eye = source.options.hidden_from_own_eye;
                    let e = &self.pack.library.emitters[source.definition];
                    if e.use_emitter_colors {
                        p.colors = source.options.colors;
                        if let Some(paint) = source.options.paint {
                            p.rgb = Some(paint);
                        }
                    }
                    if e.use_emitter_sizes {
                        p.sizes = source.options.sizes;
                    }
                }
            }
        }
        for p in &mut self.particles {
            Self::integrate(&self.pack, &self.passages, p, dt, wind);
        }
        self.particles
            .retain(|p| p.age < p.lifetime && p.position.is_finite());
        let mut budget = self.limits.emissions_per_advance;
        // Sources advance in handle order; the map is set aside meanwhile
        // so each source emits into the world in place.
        let mut sources = std::mem::take(&mut self.sources);
        for (&handle, source) in sources.iter_mut() {
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
                    self.emit(handle, source, transform, pre_age, wind);
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
        }
        sources.retain(|_, source| source.age < source.lifetime);
        self.sources = sources;
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
        let choices = &self.emitter_particles[s.definition];
        let definition = choices[(s.rng.unit() * choices.len() as f32) as usize];
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
            } else if self.emitter_alpha[s.definition].unwrap_or(p.alpha_blend) {
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
            rgb: s
                .options
                .recolor
                .map(|r| r.rgb)
                .or(s.options.paint.filter(|_| e.use_emitter_colors)),
            visible: s.options.visible,
            hidden_from_own_eye: s.options.hidden_from_own_eye,
        };
        Self::integrate(&self.pack, &self.passages, &mut particle, pre_age, wind);
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
    fn integrate(pack: &EffectsPack, passages: &Passages, p: &mut Particle, dt: f32, wind: Vec3) {
        p.age += dt;
        if p.age >= p.lifetime {
            return;
        }
        let before = p.position;
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
        if passages.list.is_empty() {
            return;
        }
        if let (end, Some(carry)) = passages.travel(before, p.position) {
            let (_, turn, _) = carry.to_scale_rotation_translation();
            p.position = end;
            p.velocity = turn * p.velocity;
            p.acceleration = turn * p.acceleration;
            p.direction = turn * p.direction;
        }
    }
    /// This frame's particles, farthest from the camera first, and lights.
    pub fn snapshot(&self, camera: &Camera) -> FrameEffects {
        self.snapshot_culled(camera, None, true)
    }
    /// [`EffectsWorld::snapshot`] without the sprites wholly outside the
    /// camera's view, which would draw nothing: what a renderer needs.
    /// A live particle's sprite this frame and its squared distance from
    /// the camera; None when hidden or outside the view.
    fn particle_instance(
        &self,
        p: &Particle,
        camera: &Camera,
        own_eye: bool,
        sees: &impl Fn(Vec3, f32) -> bool,
    ) -> Option<(f32, ParticleInstance)> {
        if !p.visible || (own_eye && p.hidden_from_own_eye) {
            return None;
        }
        // Out of view at its largest authored size: skip sampling it (the
        // keys' sizes bound the sampled one; emitter sizes may extrapolate
        // past their last key, so those are always sampled).
        if p.sizes.is_none() && !sees(p.position, self.particle_reach[p.definition]) {
            return None;
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
            return None;
        }
        if !sees(p.position, size) {
            return None;
        }
        Some((
            camera.position.distance_squared(p.position),
            ParticleInstance {
                position: p.position,
                size,
                color: Vec4::from_array(color),
                spin: p.spin * p.age,
                axis,
                texture: self.particle_texture[p.definition],
                blend: p.blend,
                depth_test: true,
            },
        ))
    }
    /// What the player's own eye sees: without what its first-person owner
    /// hides (`SourceOptions::hidden_from_own_eye`, third-person flares).
    pub fn snapshot_in_view(&self, camera: &Camera) -> FrameEffects {
        self.snapshot_culled(camera, Some(Frustum::new(camera.view_projection)), true)
    }
    /// [`Self::snapshot_in_view`] for another view of the world (a mirror,
    /// a portal, the environment probe), which sees the player from
    /// outside: their own jets and third-person flares included.
    pub fn snapshot_in_other_view(&self, camera: &Camera) -> FrameEffects {
        self.snapshot_culled(camera, Some(Frustum::new(camera.view_projection)), false)
    }
    fn snapshot_culled(
        &self,
        camera: &Camera,
        frustum: Option<Frustum>,
        own_eye: bool,
    ) -> FrameEffects {
        let sees = |center, size| frustum.as_ref().is_none_or(|f| f.sees(center, size));
        // Each sprite's squared distance, computed once for the sort. A
        // large crowd of particles is sampled on the worker threads, in
        // order, so the result is the same as on one.
        let part = parallel_part(self.particles.len());
        let sample = |particles: &[Particle]| -> Vec<(f32, ParticleInstance)> {
            particles
                .iter()
                .filter_map(|p| self.particle_instance(p, camera, own_eye, &sees))
                .collect()
        };
        let mut drawn: Vec<(f32, ParticleInstance)> = if self.particles.len() > part {
            use rayon::prelude::*;
            let parts: Vec<_> = self.particles.par_chunks(part).map(sample).collect();
            let mut drawn = Vec::with_capacity(parts.iter().map(Vec::len).sum());
            for p in parts {
                drawn.extend(p);
            }
            drawn
        } else {
            sample(&self.particles)
        };
        let mut lights = Vec::new();
        for (handle, s) in &self.sources {
            if !s.light || !s.options.visible {
                continue;
            }
            let def = &self.pack.library.lights[s.definition];
            let (color, radius) = def.sample(s.age as f32);
            // A painted source tints its light (an image light worn in a
            // team's colour).
            let color =
                Vec3::from_array(color) * s.options.paint.map_or(Vec3::ONE, Vec3::from_array);
            if radius <= 0. || color.max_element() <= 0. {
                continue;
            }
            lights.push(LightSnapshot {
                handle: *handle,
                position: s.transform.position,
                color,
                radius,
            });
            if let (Some(f), Some(texture)) = (&def.flare, self.flare_texture[s.definition]) {
                let own = (f.third_person && s.options.first_person_owner)
                    || s.options.hidden_from_own_eye;
                if own_eye && own {
                    continue;
                }
                let distance = camera.position.distance(s.transform.position);
                // v20's `fxLight::renderObject` only shows flares nearer than 75 units.
                if distance >= FLARE_MAX_DISTANCE {
                    continue;
                }
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
                if !sees(s.transform.position, size) {
                    continue;
                }
                drawn.push((
                    camera.position.distance_squared(s.transform.position),
                    ParticleInstance {
                        position: s.transform.position,
                        size,
                        // v20 divides the linked (brightness-scaled) colour by its
                        // largest channel: a Brightness 5 white light flares white.
                        color: if f.link_color {
                            color / color.max_element()
                        } else {
                            Vec3::from_array(f.color)
                        }
                        .extend(1.),
                        spin: 0.,
                        axis: Vec3::ZERO,
                        texture,
                        blend: match f.blend_mode {
                            1 => BlendMode::Alpha,
                            2 => BlendMode::AdditiveColor,
                            _ => BlendMode::Additive,
                        },
                        depth_test: false,
                    },
                ));
            }
        }
        // Keep texture runs in this order; regrouping alpha sprites by texture breaks compositing.
        // Equally distant sprites keep emission order.
        let order = far_first(drawn.iter().map(|(d, _)| *d));
        FrameEffects {
            particles: order.into_iter().map(|i| drawn[i as usize].1).collect(),
            lights,
        }
    }
}

/// Particles below which one thread samples them all.
const PARALLEL_PARTICLES: usize = 4096;

/// How many particles each worker takes: an even share of at most 8.
fn parallel_part(particles: usize) -> usize {
    particles
        .div_ceil(rayon::current_num_threads().clamp(1, 8))
        .max(PARALLEL_PARTICLES)
}

/// Indices of `distances` (squared, never negative) farthest first, equal
/// ones in their original order: the order of a stable descending sort, by
/// a four-pass radix sort of the distances' bits (for non-negative floats
/// the bits order as the values do), linear in the sprite count.
fn far_first(distances: impl ExactSizeIterator<Item = f32>) -> Vec<u32> {
    let keys: Vec<u32> = distances.map(|d| !d.max(0.0).to_bits()).collect();
    let mut order: Vec<u32> = (0..keys.len() as u32).collect();
    let mut scratch = vec![0u32; keys.len()];
    for shift in [0, 8, 16, 24] {
        let mut counts = [0usize; 257];
        for &i in &order {
            counts[((keys[i as usize] >> shift) & 0xff) as usize + 1] += 1;
        }
        if counts[1..].contains(&keys.len()) {
            continue;
        }
        for b in 1..257 {
            counts[b] += counts[b - 1];
        }
        for &i in &order {
            let bucket = ((keys[i as usize] >> shift) & 0xff) as usize;
            scratch[counts[bucket]] = i;
            counts[bucket] += 1;
        }
        std::mem::swap(&mut order, &mut scratch);
    }
    order
}

/// The camera's view volume as six planes, for leaving out sprites whose
/// bounding sphere lies wholly outside it.
struct Frustum([Vec4; 6]);
impl Frustum {
    fn new(view_projection: Mat4) -> Self {
        let m = view_projection.transpose();
        let (x, y, z, w) = (m.x_axis, m.y_axis, m.z_axis, m.w_axis);
        // wgpu clip space: -w <= x, y <= w and 0 <= z <= w.
        Self([w + x, w - x, w + y, w - y, z, w - z].map(|p| {
            let length = p.truncate().length();
            if length > 0. { p / length } else { p }
        }))
    }
    /// Whether a sprite `size` across at `center` can show. A camera
    /// with no usable planes culls nothing.
    fn sees(&self, center: Vec3, size: f32) -> bool {
        // The quad's corners lie size / 2 along two axes: within size * 0.71.
        let radius = size.abs() * std::f32::consts::FRAC_1_SQRT_2;
        self.0
            .iter()
            .all(|p| !p.is_finite() || p.truncate().dot(center) + p.w >= -radius)
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
            paint: Some([paint[0], paint[1], paint[2]]),
            emitting: !fake_dead,
            ..Default::default()
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::far_first;

    mod sampled_on_threads {
        use super::super::*;
        use bri_content::effects::*;
        use std::collections::BTreeMap;

        fn fixture(mut change: impl FnMut(&mut Library)) -> Arc<EffectsPack> {
            let mut library = Library {
                schema_version: 1,
                textures: BTreeMap::from([("original".into(), "texture.png".into())]),
                lights: vec![Light {
                    id: "light".into(),
                    name: "Light".into(),
                    enabled: true,
                    color: [1., 0.5, 0.],
                    brightness: 2.,
                    radius: 5.,
                    color_curves: None,
                    brightness_curve: None,
                    radius_curve: None,
                    flare: Some(Flare {
                        texture: "original".into(),
                        color: [1.; 3],
                        third_person: true,
                        constant_size: Some(1.),
                        near_size: 1.,
                        far_size: 0.5,
                        near_distance: 0.,
                        far_distance: 10.,
                        fade_seconds: 0.5,
                        blend_mode: 0,
                        link_color: true,
                        link_size: true,
                    }),
                }],
                particles: vec![bri_content::effects::Particle {
                    id: "particle".into(),
                    texture: "original".into(),
                    alpha_blend: true,
                    lifetime: 2.,
                    lifetime_variance: 0.,
                    drag: 0.,
                    wind: 0.,
                    gravity: 0.,
                    inherited_velocity: 0.,
                    acceleration: 0.,
                    spin_degrees: 90.,
                    random_spin: [0., 0.],
                    keys: vec![
                        ParticleKey {
                            time: 0.,
                            color: [1., 0., 0., 1.],
                            size: 1.,
                        },
                        ParticleKey {
                            time: 1.,
                            color: [0., 0., 1., 0.],
                            size: 3.,
                        },
                    ],
                }],
                emitters: vec![Emitter {
                    id: "emitter".into(),
                    name: "Emitter".into(),
                    particles: vec!["particle".into()],
                    period: 0.1,
                    period_variance: 0.,
                    speed: 2.,
                    speed_variance: 0.,
                    offset: 0.,
                    offset_variance: 0.,
                    theta_degrees: [0., 0.],
                    phi_rate_degrees: 0.,
                    phi_variance_degrees: 0.,
                    lifetime: 0.,
                    lifetime_variance: 0.,
                    orient: false,
                    orient_on_velocity: true,
                    override_advance: false,
                    use_emitter_colors: false,
                    use_emitter_sizes: false,
                    use_placement_velocity: false,
                    node_time_scale: 1.,
                    point_node_time_scale: 1.,
                }],
            };
            change(&mut library);
            crate::EffectsPack::from_parts(
                library,
                crate::pack::Manifest {
                    schema_version: 1,
                    library_sha256: String::new(),
                    textures: BTreeMap::new(),
                    emitter_alpha: BTreeMap::new(),
                    bindings: Vec::new(),
                    composites: Vec::new(),
                    unresolved: Vec::new(),
                },
                vec![crate::pack::TextureImage {
                    id: "original".into(),
                    width: 1,
                    height: 1,
                    rgba: vec![120, 80, 20, 255],
                }],
            )
            .unwrap()
        }

        #[test]
        fn a_crowd_sampled_on_threads_matches_one_thread() {
            let mut world =
                EffectsWorld::new(fixture(|_| {}), EffectsLimits::default(), 7).unwrap();
            for i in 0..600 {
                let position = Vec3::new((i % 30) as f32, (i / 30) as f32 * 0.5, (i % 7) as f32);
                world
                    .burst(
                        "emitter",
                        SourceTransform {
                            position,
                            ..Default::default()
                        },
                        SourceOptions::default(),
                        20,
                    )
                    .unwrap();
            }
            world.advance(0.3, Vec3::ZERO).unwrap();
            assert!(world.particle_count() > 2 * PARALLEL_PARTICLES);
            let camera = Camera {
                view_projection: glam::camera::rh::proj::directx::perspective(1.2, 1.5, 0.1, 100.),
                position: Vec3::new(10., 5., 40.),
                right: Vec3::X,
                up: Vec3::Y,
            };
            let frustum = Frustum::new(camera.view_projection);
            let sees = |center, size| frustum.sees(center, size);
            let one: Vec<_> = world
                .particles
                .iter()
                .filter_map(|p| world.particle_instance(p, &camera, true, &sees))
                .collect();
            let order = far_first(one.iter().map(|(d, _)| *d));
            let expected: Vec<_> = order.into_iter().map(|i| one[i as usize].1).collect();
            let threaded = world.snapshot_in_view(&camera).particles;
            assert_eq!(threaded.len(), expected.len());
            assert_eq!(format!("{threaded:?}"), format!("{expected:?}"));
        }

        #[test]
        fn own_jets_stay_out_of_the_own_eye_but_show_in_mirrors() {
            let mut world =
                EffectsWorld::new(fixture(|_| {}), EffectsLimits::default(), 7).unwrap();
            let own = SourceOptions {
                hidden_from_own_eye: true,
                ..Default::default()
            };
            for (x, options) in [(-2., own), (2., SourceOptions::default())] {
                let at = SourceTransform {
                    position: Vec3::new(x, 0., -20.),
                    ..Default::default()
                };
                world.burst("emitter", at, options, 10).unwrap();
            }
            world.advance(0.1, Vec3::ZERO).unwrap();
            let camera = Camera {
                view_projection: glam::camera::rh::proj::directx::perspective(1.2, 1.5, 0.1, 100.),
                position: Vec3::ZERO,
                right: Vec3::X,
                up: Vec3::Y,
            };
            let own_eye = world.snapshot_in_view(&camera).particles.len();
            let other = world.snapshot_in_other_view(&camera).particles.len();
            assert_eq!((own_eye, other), (10, 20));
        }

        #[test]
        fn particles_fly_on_out_of_a_portals_partner() {
            use bri_content::passage::{Passage, Passages};
            // Sprayed up in a cone; a 2x2 opening one unit up leads twenty
            // units along x, turned a quarter about z.
            let pack = fixture(|l| {
                l.emitters[0].speed = 10.;
                l.emitters[0].theta_degrees = [0., 70.];
            });
            let carry = glam::Affine3A::from_translation(Vec3::new(20., 1., 0.))
                * glam::Affine3A::from_rotation_z(std::f32::consts::FRAC_PI_2)
                * glam::Affine3A::from_translation(Vec3::new(0., -1., 0.));
            let passages = Passages {
                list: vec![Passage {
                    brick: 1,
                    centre: Vec3::Y,
                    normal: Vec3::NEG_Y,
                    u: Vec3::Z,
                    v: Vec3::X,
                    half: glam::Vec2::new(1., 1.),
                    carry,
                }],
                closed: vec![],
            };
            let spray = |portals: bool| {
                let mut world =
                    EffectsWorld::new(pack.clone(), EffectsLimits::default(), 7).unwrap();
                if portals {
                    world.set_passages(&passages);
                }
                world
                    .burst(
                        "emitter",
                        SourceTransform::default(),
                        SourceOptions::default(),
                        60,
                    )
                    .unwrap();
                for _ in 0..6 {
                    world.advance(0.05, Vec3::ZERO).unwrap();
                }
                world.particles
            };
            let (free, through) = (spray(false), spray(true));
            assert_eq!(free.len(), through.len());
            let mut carried = 0;
            for (f, t) in free.iter().zip(&through) {
                // Straight from the emitter: through the opening or past it.
                let (position, velocity) = match passages.first(Vec3::ZERO, f.position) {
                    Some(_) => {
                        carried += 1;
                        (
                            carry.transform_point3(f.position),
                            carry.transform_vector3(f.velocity),
                        )
                    }
                    None => (f.position, f.velocity),
                };
                assert!(
                    t.position.distance(position) < 1e-3,
                    "{} not {position}",
                    t.position
                );
                assert!(
                    t.velocity.distance(velocity) < 1e-3,
                    "{} not {velocity}",
                    t.velocity
                );
            }
            assert!(
                carried > 0 && carried < free.len(),
                "{carried} of {}",
                free.len()
            );
        }
    }
    #[test]
    fn radix_depth_order_matches_a_stable_descending_sort() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for len in [0, 1, 2, 7, 300, 5000] {
            // Coarse values give many ties; some spread over large ranges.
            let distances: Vec<f32> = (0..len)
                .map(|_| match next() % 3 {
                    0 => (next() % 16) as f32,
                    1 => (next() % 100_000) as f32 * 0.37,
                    _ => f32::from_bits((next() % 0x7f00_0000) as u32),
                })
                .collect();
            let mut expected: Vec<u32> = (0..len as u32).collect();
            expected.sort_by(|a, b| distances[*b as usize].total_cmp(&distances[*a as usize]));
            assert_eq!(far_first(distances.iter().copied()), expected, "{len}");
        }
    }
}
