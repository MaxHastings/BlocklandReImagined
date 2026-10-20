//! The base particle simulation: the pool, `SpawnParticles`, `SpawnParticle` and
//! `UpdateParticles` of the original's `UParticleEmitter`, as read in
//! `native-research/specs/particle-emitter-spawn.md` and `-update.md` (step numbers in the
//! comments are theirs). The host supplies the owner's state and a line trace
//! ([`World`]); sounds and damage come back as [`Event`]s.
#![allow(
    clippy::collapsible_if,
    clippy::needless_range_loop,
    clippy::field_reassign_with_default
)]
use crate::particle::{Particle, flags};
use crate::props::{
    Color, CoordinateSystem, DrawStyle, EffectAxis, MeshSpawning, ParticleEmitterProps,
    RotationSource, SkelLocationUpdate, SoundSelection, StartLocationShape, VelocityDirection,
};
use crate::random::Rand;
use crate::rotation::{
    Coords, Rotator, mirror_by_vector, rotate_angle_axis, safe_normal, vector_rotation,
};
use glam::Vec3;

/// The emitter's owner actor, as the simulation reads it.
#[derive(Clone, Copy, Debug, Default)]
pub struct OwnerState {
    pub location: Vec3,
    /// Where it was last tick.
    pub old_location: Vec3,
    pub rotation: Rotator,
    pub absolute_velocity: Vec3,
    /// The owner's random start offset (copied into every emitter each tick).
    pub global_offset: Vec3,
    /// The owner's sound volume scale (Actor +0x3a4).
    pub sound_volume: f32,
    /// The level clock (`Level.TimeSeconds`).
    pub time_seconds: f32,
    /// The owner's PrePivot (mesh emitters).
    pub pre_pivot: Vec3,
    /// The owner's physics volume gravity, when it has one (rope emitters).
    pub gravity: Option<Vec3>,
}

#[derive(Clone, Copy, Debug)]
pub struct TraceHit {
    pub location: Vec3,
    pub normal: Vec3,
    pub actor: Option<u32>,
    pub material: Option<u32>,
}

/// What the simulation asks of the world.
pub trait World {
    /// A line trace from `start` to `end`: the first thing hit. `world_only` limits it to
    /// world geometry (the original's flag 0x04); otherwise it also hits the extra class
    /// of geometry (0x84).
    fn line_check(&mut self, start: Vec3, end: Vec3, world_only: bool) -> Option<TraceHit>;
    /// The local player's sound factor for collision sounds (1 when there is none).
    fn client_sound_factor(&mut self) -> f32 {
        1.0
    }
    /// The owner's current state.
    fn owner(&self) -> OwnerState;
    /// The location of the first actor whose tag is `tag` (a beam's `Actor` end point).
    fn actor_location(&mut self, _tag: u32) -> Option<Vec3> {
        None
    }
}

/// Something the simulation wants done outside itself.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A sound from the emitter's own sound table.
    PlaySound {
        sound: u32,
        location: Vec3,
        volume: f32,
        radius: f32,
        pitch: f32,
        on_spawn: bool,
    },
    /// `bPlayCollisionSoundFromOwner`: ask the owner's script which sound to play.
    OwnerCollisionSound {
        emitter: usize,
        particle: usize,
        material: Option<u32>,
        location: Vec3,
    },
    /// A beam with `TriggerEndpoint` reached the actor tagged `tag`.
    TriggerActor { tag: u32 },
    Damage {
        actor: u32,
        amount: i32,
        location: Vec3,
        momentum: Vec3,
    },
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BBox {
    pub min: Vec3,
    pub max: Vec3,
    pub valid: bool,
}
impl BBox {
    pub fn add(&mut self, p: Vec3) {
        if self.valid {
            self.min = self.min.min(p);
            self.max = self.max.max(p);
        } else {
            self.min = p;
            self.max = p;
            self.valid = true;
        }
    }
    pub fn expand_by(&mut self, w: f32) {
        self.min -= Vec3::splat(w);
        self.max += Vec3::splat(w);
        self.valid = true;
    }
}

#[derive(Clone, Debug)]
pub struct Emitter {
    pub props: ParticleEmitterProps,
    pub particles: Vec<Particle>,
    pub particle_index: usize,
    pub active_particles: usize,
    pub max_active_particles: usize,
    pub pps_fraction: f32,
    pub other_index: i32,
    pub deferred_particles: i32,
    pub kill_pending: bool,
    pub current_spawn_on_trigger: i32,
    /// Bone origin / direction pairs from the skeletal mesh, filled by the host before an update.
    pub mesh_verts_and_normals: Vec<Vec3>,
    pub max_size_scale: f32,
    pub bounding_box: BBox,
    pub real_extent_multiplier: Vec3,
    pub real_mesh_normal: Vec3,
    pub real_velocity_loss_range: crate::props::RangeVector,
    pub current_collision_sound_index: i32,
    pub current_spawning_sound_index: i32,
    pub current_mesh_spawning_index: i32,
    pub renderable_particles: usize,
    pub all_particles_dead: bool,
    pub initialized: bool,
    pub inactive: bool,
    /// Seconds the emitter has gone undrawn (`SecondsBeforeInactive` stops its updates).
    pub inactive_time: f32,
    /// The warm-up ticks have run (cleared by a reset).
    pub warmed_up: bool,
    /// Every particle spawned since a subclass last took the log: its slot, the age it was spawned with
    /// and a copy as it stood at spawn. The beam, trail and spark classes finish their own
    /// `SpawnParticle` overrides from it.
    pub spawn_log: Vec<(usize, f32, crate::particle::Particle)>,
    /// Whether to keep the log (the subclasses that need it turn it on).
    pub log_spawns: bool,
    pub initial_delay: f32,
    pub time_till_reset: f32,
    /// This emitter's copy of the owner's global offset (set each tick).
    pub global_offset: Vec3,
}

/// Everything an emitter call needs besides the emitter: the siblings it can reach
/// by index, the world, the random source, and the event queue.
pub struct Ctx<'a> {
    pub owner: OwnerState,
    /// The owner's other emitters; the entry for the emitter being run is `None`.
    pub others: &'a mut [Option<Emitter>],
    pub world: &'a mut dyn World,
    pub rand: &'a mut Rand,
    pub events: &'a mut Vec<Event>,
    /// This emitter's index in the owner (for events).
    pub me: usize,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn frac(v: f32) -> f32 {
    v - v.floor()
}
fn sign(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}
fn clampf(x: f32, lo: f32, hi: f32) -> f32 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

impl Emitter {
    /// `Initialize(count)`: an empty pool of `count` slots.
    pub fn new(props: ParticleEmitterProps, count: usize, rand: &mut Rand) -> Self {
        let mut e = Self {
            real_extent_multiplier: props.extent_multiplier,
            real_mesh_normal: safe_normal(props.mesh_normal),
            real_velocity_loss_range: props.velocity_loss_range,
            props,
            particles: vec![Particle::default(); count],
            particle_index: 0,
            active_particles: 0,
            max_active_particles: count,
            pps_fraction: 0.0,
            other_index: 0,
            deferred_particles: 0,
            kill_pending: false,
            current_spawn_on_trigger: 0,
            mesh_verts_and_normals: Vec::new(),
            max_size_scale: 1.0,
            bounding_box: BBox::default(),
            current_collision_sound_index: 0,
            current_spawning_sound_index: 0,
            current_mesh_spawning_index: 0,
            renderable_particles: 0,
            all_particles_dead: false,
            initialized: true,
            inactive: false,
            inactive_time: 0.0,
            warmed_up: false,
            spawn_log: Vec::new(),
            log_spawns: false,
            initial_delay: 0.0,
            time_till_reset: 0.0,
            global_offset: Vec3::ZERO,
        };
        e.reset(rand);
        e
    }

    /// `Reset()`: existing particles are not cleared.
    pub fn reset(&mut self, rand: &mut Rand) {
        self.all_particles_dead = false;
        self.warmed_up = false;
        self.active_particles = 0;
        self.particle_index = 0;
        self.other_index = 0;
        self.initial_delay = rand.range(self.props.initial_delay_range);
        self.time_till_reset = rand.range(self.props.auto_reset_time_range);
    }

    fn advance_pool(&mut self) {
        self.active_particles = self.active_particles.max(self.particle_index + 1);
        self.particle_index = (self.particle_index + 1) % self.max_active_particles.max(1);
    }

    /// The coordinate frame `UseRotationFrom` selects for rotating a start location or velocity.
    fn rotation_coords(&self, owner: &OwnerState, pitch_fixup: bool) -> Option<Coords> {
        let e = &self.props;
        match e.use_rotation_from {
            RotationSource::None => None,
            RotationSource::Actor => Some(
                Coords::UNIT
                    .mul_rotator(owner.rotation)
                    .mul_rotator(e.rotation_offset),
            ),
            RotationSource::Offset => Some(Coords::UNIT.mul_rotator(e.rotation_offset)),
            RotationSource::Normal => {
                let mut r = vector_rotation(e.rotation_normal);
                if pitch_fixup && e.effect_axis == EffectAxis::PositiveZ {
                    r.pitch = r.pitch.wrapping_sub(0x4000);
                }
                Some(Coords::UNIT.mul_rotator(r))
            }
        }
    }

    /// `SpawnParticles(frac, rate, dt)`: returns the new fraction.
    pub fn spawn_particles(&mut self, ctx: &mut Ctx<'_>, frac_in: f32, rate: f32, dt: f32) -> f32 {
        if rate <= 0.0 {
            return 0.0;
        }
        let x = rate * dt + frac_in;
        let mut n = x as i32;
        let new_frac = x - n as f32;
        let interval = 1.0 / rate;
        let t0 = interval * frac_in + dt - interval;
        n = n.clamp(0, self.max_active_particles as i32);
        if self.current_spawn_on_trigger != 0 {
            n = n.clamp(0, self.current_spawn_on_trigger);
            self.current_spawn_on_trigger -= n;
        }
        let moved = (ctx.owner.old_location - ctx.owner.location).length() > 1.0;
        for i in 0..n {
            let idx = self.particle_index;
            self.spawn_particle(ctx, idx, t0 - i as f32 * interval, 4, 0, Vec3::ZERO);
            if moved && self.props.coordinate_system == CoordinateSystem::Independent {
                let shift = (ctx.owner.old_location - ctx.owner.location)
                    * (1.0 - (i + 1) as f32 / n as f32);
                self.particles[idx].location += shift;
            }
            self.advance_pool();
        }
        new_frac
    }

    /// `SpawnParticle(index, age, flags, mode, offset)`.
    pub fn spawn_particle(
        &mut self,
        ctx: &mut Ctx<'_>,
        index: usize,
        age: f32,
        flags_arg: i32,
        mode: i32,
        offset: Vec3,
    ) {
        let e = self.props.clone();
        // Step 0.
        if self.max_active_particles == 0
            || self.kill_pending
            || e.lifetime_range.max <= 0.0
            || index >= self.particles.len()
        {
            return;
        }
        let owner = ctx.owner;
        let rand = &mut *ctx.rand;
        let mut p = Particle::default();
        // Step 1 and 2.
        p.location = e.start_location_offset;
        p.velocity = rand.vec_yxz(e.start_velocity_range);
        // Step 3: box.
        if matches!(
            e.start_location_shape,
            StartLocationShape::Box | StartLocationShape::All
        ) {
            p.location += rand.vec_yxz(e.start_location_range);
        }
        // Step 4: sphere.
        if matches!(
            e.start_location_shape,
            StartLocationShape::Sphere | StartLocationShape::All
        ) {
            let dir = loop {
                let a = Vec3::new(rand.signed_unit(), rand.signed_unit(), rand.signed_unit());
                let len = a.length();
                if (0.1..=1.0).contains(&len) {
                    break a / len;
                }
            };
            p.location += dir * rand.range(e.sphere_radius_range);
        }
        // Step 5: polar.
        if matches!(
            e.start_location_shape,
            StartLocationShape::Polar | StartLocationShape::All
        ) {
            let d = rand.vec_yxz(e.start_location_polar_range);
            let k = std::f32::consts::TAU / 65536.0;
            let (a, b) = (d.x * k, d.y * k);
            p.location += d.z * Vec3::new(b.sin() * a.cos(), b.sin() * a.sin(), b.cos());
        }
        p.color_multiplier = Vec3::ONE;
        // Step 7: static mesh spawning.
        if e.mesh_spawning != MeshSpawning::None {
            if let Some(mesh) = e.mesh.as_ref().filter(|m| !m.positions.is_empty()) {
                let count = mesh.positions.len() as i32;
                let raw = if e.mesh_spawning == MeshSpawning::Linear {
                    let v = self.current_mesh_spawning_index;
                    self.current_mesh_spawning_index += 1;
                    v
                } else {
                    (rand.app_srand() * count as f32) as i32
                };
                let mut i = raw % count;
                if i < 0 {
                    i = 0;
                }
                if i >= count {
                    i = count;
                }
                let i = (i as usize).min(mesh.positions.len() - 1);
                let normal = mesh.normals.get(i).copied().unwrap_or(Vec3::ZERO);
                if e.spawn_only_in_direction_of_normal {
                    let t = rand.range(e.mesh_normal_threshold_range);
                    if normal.dot(self.real_mesh_normal) < 1.0 - 2.0 * t {
                        p.flags &= !flags::ALIVE;
                        self.particles[index] = p;
                        return;
                    }
                }
                let s = rand.vec_yxz(e.mesh_scale_range);
                let v = mesh.positions[i];
                p.location += if e.uniform_mesh_scale { s.x * v } else { s * v };
                if e.velocity_from_mesh {
                    let vs = rand.vec_yxz(e.velocity_scale_range);
                    let scale = if e.uniform_velocity_scale {
                        Vec3::splat(vs.x)
                    } else {
                        vs
                    };
                    p.velocity += scale * normal;
                }
                if e.use_color_from_mesh {
                    if let Some(c) = mesh.colors.get(i) {
                        p.color_multiplier = Vec3::new(
                            f32::from(c.r) / 255.0,
                            f32::from(c.g) / 255.0,
                            f32::from(c.b) / 255.0,
                        );
                    }
                }
            }
        }
        // Step 8: skeletal.
        let bone_count = self.mesh_verts_and_normals.len() / 2;
        if e.use_skeletal_location_as != SkelLocationUpdate::None && bone_count != 0 {
            let rv = rand.range(e.relative_bone_index_range);
            let bone = ((rv * bone_count as f32) as i32).clamp(0, bone_count as i32 - 1) as usize;
            p.bone_index = bone as i32;
            p.old_mesh_location = self.mesh_verts_and_normals[2 * bone] * e.skeletal_scale;
            p.location += p.old_mesh_location;
        }
        // Step 9: location from another emitter.
        self.other_index += 1;
        if e.add_location_from_other_emitter >= 0 {
            if let Some(q) = peek_particle(
                self,
                ctx.others,
                e.add_location_from_other_emitter,
                self.other_index,
            ) {
                p.location += q.location - owner.location;
            }
        }
        // Step 10: rotate the location.
        if let Some(c) = self.rotation_coords(&owner, true) {
            p.location = c.transform_vector(p.location);
        }
        // Step 11.
        p.revolution_center = rand.vec_yxz(e.revolution_center_offset_range);
        p.revolutions_per_second = rand.vec_yxz(e.revolutions_per_second_range);
        // Step 12: coordinate system offsets.
        if e.coordinate_system == CoordinateSystem::Independent {
            if mode & 2 == 0 {
                p.location += owner.location;
            }
            p.revolution_center += owner.location;
        }
        if mode & 1 == 0 {
            p.location += self.global_offset;
        }
        p.location += offset;
        p.old_location = p.location;
        p.start_location = p.location;
        // Step 13.
        p.color_multiplier *= rand.vec_yxz(e.color_multiplier_range);
        // Step 14.
        p.max_lifetime = rand.range(e.lifetime_range);
        let t0 = rand.range(e.initial_time_range);
        p.time = t0 + age;
        p.fade_time = p.time;
        p.num_frames_rendered = 0;
        p.hit_count = 0;
        p.flags = flags_arg | flags::ALIVE;
        // Step 15 and 16.
        p.mass = rand.range(e.start_mass_range);
        p.start_size = rand.vec_yxz(e.start_size_range);
        if e.uniform_size {
            p.start_size.y = p.start_size.x;
            p.start_size.z = p.start_size.x;
        }
        p.size = p.start_size;
        // Step 17.
        p.velocity += e.acceleration * age;
        p.velocity_multiplier = Vec3::ONE;
        p.revolutions_multiplier = Vec3::ONE;
        // Step 18: rotate the velocity.
        if let Some(c) = self.rotation_coords(&owner, false) {
            p.velocity = c.transform_vector(p.velocity);
        }
        // Step 19: velocity direction.
        if e.get_velocity_direction_from != VelocityDirection::None {
            let d = if e.coordinate_system == CoordinateSystem::Relative {
                safe_normal(p.location)
            } else {
                safe_normal(owner.location - p.location)
            };
            match e.get_velocity_direction_from {
                VelocityDirection::StartPositionAndOwner => {
                    p.velocity = d * -p.velocity.length();
                }
                VelocityDirection::OwnerAndStartPosition => {
                    p.velocity = d * p.velocity.length();
                }
                VelocityDirection::AddRadial => {
                    p.velocity += d * rand.range(e.start_velocity_radial_range);
                }
                VelocityDirection::None => {}
            }
        }
        // Step 20.
        if e.add_velocity_from_owner && e.coordinate_system != CoordinateSystem::Relative {
            let m = rand.vec_yxz(e.add_velocity_multiplier_range);
            p.velocity += m * owner.absolute_velocity;
        }
        if e.add_velocity_from_other_emitter >= 0 {
            let m = rand.vec_yxz(e.add_velocity_multiplier_range);
            if let Some(q) = peek_particle(
                self,
                ctx.others,
                e.add_velocity_from_other_emitter,
                self.other_index,
            ) {
                p.velocity += m * q.velocity;
            }
        }
        // Step 21.
        p.location += p.velocity * age;
        // Step 22.
        if e.scale_size_x_by_velocity || e.scale_size_y_by_velocity || e.scale_size_z_by_velocity {
            let speed = p.velocity.length().min(e.scale_size_by_velocity_max);
            let m = e.scale_size_by_velocity_multiplier;
            if e.scale_size_x_by_velocity {
                p.size.x = m.x * p.size.x * speed;
            }
            if e.scale_size_y_by_velocity {
                p.size.y = m.y * p.size.y * speed;
            }
            if e.scale_size_z_by_velocity {
                p.size.z = m.z * p.size.z * speed;
            }
        }
        // Step 23.
        if p.mass != 0.0 {
            p.mass = 1.0 / p.mass;
        }
        // Step 24: spin.
        p.start_spin = rand.vec_yxz(e.start_spin_range);
        p.spins_per_second = rand.vec_yxz(e.spins_per_second_range);
        for axis in 0..3 {
            let r = rand.unit();
            if r < e.spin_ccw_or_cw[axis] {
                p.spins_per_second[axis] = -p.spins_per_second[axis];
            }
        }
        p.start_spin *= 65535.0;
        p.spins_per_second *= 65535.0;
        // Step 25.
        p.subdivision = -1;
        if e.use_random_subdivision {
            let r = rand.unit();
            p.subdivision = if e.subdivision_end != 0 {
                e.subdivision_start + (r * (e.subdivision_end - e.subdivision_start) as f32) as i32
            } else {
                (r * e.texture_v_subdivisions as f32 * e.texture_u_subdivisions as f32) as i32
            };
        }
        let expired = p.time > p.max_lifetime && p.max_lifetime != 0.0;
        let location = p.location;
        self.particles[index] = p;
        // Step 26: already expired.
        if expired {
            let (time, life) = (
                self.particles[index].time,
                self.particles[index].max_lifetime,
            );
            self.spawn_particle(ctx, index, time % life, 0, 0, Vec3::ZERO);
        }
        // Step 27: spawn sound.
        let location = if expired {
            self.particles[index].location
        } else {
            location
        };
        self.spawn_sound(ctx, location);
        // Step 28.
        self.all_particles_dead = false;
        if self.log_spawns {
            self.spawn_log
                .push((index, age, self.particles[index].clone()));
        }
    }

    fn spawn_sound(&mut self, ctx: &mut Ctx<'_>, location: Vec3) {
        let e = &self.props;
        if e.spawning_sound == SoundSelection::None || e.sounds.is_empty() {
            return;
        }
        let rand = &mut *ctx.rand;
        // Spawn-side selection: modes 1 and 2 both use the emitter-wide counter.
        let idx0 = match e.spawning_sound {
            SoundSelection::LinearGlobal | SoundSelection::LinearLocal => {
                let v = self.current_spawning_sound_index;
                self.current_spawning_sound_index += 1;
                v
            }
            SoundSelection::Random => (rand.app_srand() * 1000.0) as i32,
            SoundSelection::None => 0,
        };
        let lo = e.spawning_sound_index.min.min(e.spawning_sound_index.max);
        let span = e.spawning_sound_index.max.max(e.spawning_sound_index.min) - lo;
        let d = if span == 0.0 {
            e.sounds.len() as i32
        } else {
            (span + 1.0) as i32
        };
        if d == 0 {
            return;
        }
        let i = (idx0 % d + lo as i32).clamp(0, e.sounds.len() as i32 - 1) as usize;
        let s = e.sounds[i];
        let ps = rand.range(s.probability);
        let q = rand.range(e.spawning_sound_probability);
        let u = rand.app_srand();
        if q * ps >= u {
            let radius = rand.range(s.radius);
            let volume = rand.range(s.volume);
            let pitch = rand.range(s.pitch);
            ctx.events.push(Event::PlaySound {
                sound: s.sound,
                location,
                volume: volume * ctx.owner.sound_volume,
                radius,
                pitch,
                on_spawn: true,
            });
        }
    }

    /// `UpdateParticles(dt)`: returns the live particle count.
    pub fn update_particles(&mut self, ctx: &mut Ctx<'_>, dt: f32) -> usize {
        self.update_particles_with(ctx, dt, &mut |_, _, _, _| {})
    }

    /// `UpdateParticles` with the `UpdateParticle(dt, index)` virtual (step 15 of POST)
    /// supplied by the subclass: it runs once per particle that reaches POST.
    pub fn update_particles_with(
        &mut self,
        ctx: &mut Ctx<'_>,
        dt: f32,
        hook: &mut dyn FnMut(&mut Emitter, &mut Ctx<'_>, usize, f32),
    ) -> usize {
        let e = self.props.clone();
        self.bounding_box = BBox::default();
        let owner = ctx.owner;
        // Step A.
        let n_emitters = ctx.others.len() as i32;
        let clamp_link = |v: i32| if v >= 0 { v.min(n_emitters - 1) } else { v };
        self.props.add_location_from_other_emitter = clamp_link(e.add_location_from_other_emitter);
        self.props.add_velocity_from_other_emitter = clamp_link(e.add_velocity_from_other_emitter);
        self.props.spawn_from_other_emitter = clamp_link(e.spawn_from_other_emitter);
        let e = self.props.clone();
        self.real_velocity_loss_range = if !e.rotate_velocity_loss_range {
            e.velocity_loss_range
        } else {
            let v = e.velocity_loss_range;
            let lo = Vec3::new(v.x.min, v.y.min, v.z.min);
            let hi = Vec3::new(v.x.max, v.y.max, v.z.max);
            match self.rotation_coords(&owner, false) {
                Some(c) => {
                    let (lo, hi) = (c.transform_vector(lo), c.transform_vector(hi));
                    use crate::props::{Range, RangeVector};
                    RangeVector {
                        x: Range::new(lo.x, hi.x),
                        y: Range::new(lo.y, hi.y),
                        z: Range::new(lo.z, hi.z),
                    }
                }
                None => v,
            }
        };
        let skel_count = self.mesh_verts_and_normals.len() / 2;
        // Step B.
        let rate = if self.active_particles < self.max_active_particles {
            if e.automatic_initial_spawning {
                self.max_active_particles as f32
                    / ((e.lifetime_range.min + e.lifetime_range.max) * 0.5)
            } else {
                e.initial_particles_per_second
            }
        } else {
            e.particles_per_second
        } + if self.current_spawn_on_trigger != 0 {
            e.spawn_on_trigger_pps
        } else {
            0.0
        };
        if rate > 0.0 && !self.kill_pending {
            self.pps_fraction = self.spawn_particles(ctx, self.pps_fraction, rate, dt);
        }
        let owner_moved = (owner.old_location - owner.location).length() > 1.0;
        if !self.kill_pending {
            let n = self
                .deferred_particles
                .clamp(0, self.max_active_particles as i32);
            for i in 0..n {
                let idx = self.particle_index;
                self.spawn_particle(ctx, idx, 0.0, 0, 0, Vec3::ZERO);
                if owner_moved && e.coordinate_system == CoordinateSystem::Independent {
                    self.particles[idx].location +=
                        (owner.old_location - owner.location) * (1.0 - (i + 1) as f32 / n as f32);
                }
                self.advance_pool();
            }
            self.deferred_particles = 0;
        }
        // Step C.
        let mut respawn_count = 0;
        let mut i = 0;
        while i < self.active_particles.min(self.max_active_particles) {
            let p = &mut self.particles[i];
            if p.flags & flags::ALIVE != 0 {
                if p.flags & flags::NEW == 0 {
                    p.time += dt;
                    p.fade_time += dt;
                }
                if p.time > p.max_lifetime && e.respawn_dead_particles {
                    respawn_count += 1;
                }
            }
            i += 1;
        }
        let dtc = clampf(dt, 0.001, 0.15);
        let inv_dt = 1.0 / dtc;
        let mut max_speed_scale = 1.0f32;
        let mut respawned = 0;
        let mut dead = 0usize;
        // Step D.
        let mut idx = 0;
        while idx < self.active_particles.min(self.max_active_particles) {
            let at = idx;
            idx += 1;
            if self.particles[at].flags & flags::ALIVE == 0 {
                dead += 1;
                continue;
            }
            let fl = self.particles[at].flags;
            let do_move = !(fl & flags::STOPPED != 0
                && e.coordinate_system != CoordinateSystem::Relative)
                && fl & flags::NEW == 0;
            if fl & flags::NEW != 0 {
                self.particles[at].flags &= !flags::NEW;
            }
            let mut respawned_now = false;
            {
                let p = &self.particles[at];
                if p.max_lifetime != 0.0
                    && p.time > p.max_lifetime
                    && i32::from(p.num_frames_rendered) >= e.min_frames_before_death
                {
                    if !e.respawn_dead_particles {
                        self.particles[at].flags &= !flags::ALIVE;
                        dead += 1;
                        continue;
                    }
                    let t0 = ctx.rand.range(e.initial_time_range);
                    let start = (p.time - p.max_lifetime + t0) % p.max_lifetime;
                    self.spawn_particle(ctx, at, start, 0, 0, Vec3::ZERO);
                    if owner_moved && e.coordinate_system == CoordinateSystem::Independent {
                        let shift = (owner.old_location - owner.location)
                            * (1.0 - (respawned + 1) as f32 / respawn_count.max(1) as f32);
                        self.particles[at].location += shift;
                    }
                    respawned += 1;
                    respawned_now = true;
                }
            }
            if do_move && !respawned_now {
                // Step 15.
                let p = &mut self.particles[at];
                p.velocity += e.acceleration * dt;
                p.old_location = p.location;
                p.location += (p.velocity * p.velocity_multiplier) * dt;
                if skel_count != 0 && e.use_skeletal_location_as == SkelLocationUpdate::Location {
                    let bone = p.bone_index.max(0) as usize;
                    if let Some(&v) = self.mesh_verts_and_normals.get(2 * bone) {
                        let m = v * e.skeletal_scale;
                        p.location += m - p.old_mesh_location;
                        p.old_mesh_location = m;
                    }
                }
                if e.use_revolution {
                    let mut rel = p.location - p.revolution_center;
                    for (axis, ax) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
                        let a = (p.revolutions_multiplier[axis]
                            * p.revolutions_per_second[axis]
                            * dt
                            * 65535.0) as i32;
                        rel = rotate_angle_axis(rel, a, ax);
                    }
                    p.location = p.revolution_center + rel;
                }
            } else if !do_move {
                // fall through to the collision stage's skip
            }
            // Step 17: collision.
            let mut collided = false;
            let mut normal = Vec3::ZERO;
            let mut hit_point = Vec3::ZERO;
            if do_move && e.coordinate_system != CoordinateSystem::Relative {
                let p = self.particles[at].clone();
                if e.use_collision {
                    let d = p.location - p.old_location;
                    let dir = safe_normal(d);
                    let ext = dir * self.real_extent_multiplier * p.size;
                    if let Some(hit) = ctx.world.line_check(
                        p.old_location,
                        p.location + ext,
                        e.b_only_bsp_collision,
                    ) {
                        normal = hit.normal;
                        hit_point = hit.location - ext;
                        collided = true;
                        if e.b_play_collision_sound_from_owner {
                            ctx.events.push(Event::OwnerCollisionSound {
                                emitter: ctx.me,
                                particle: at,
                                material: hit.material,
                                location: hit.location,
                            });
                        }
                        if e.damage_amount > 0 {
                            if let Some(actor) = hit.actor {
                                ctx.events.push(Event::Damage {
                                    actor,
                                    amount: e.damage_amount,
                                    location: hit.location,
                                    momentum: safe_normal(p.location - p.old_location)
                                        * e.damage_momentum,
                                });
                            }
                        }
                        if e.b_fade_on_collide && p.fade_time < e.fade_out_start_time {
                            self.particles[at].fade_time = e.fade_out_start_time;
                        }
                    }
                }
                if e.use_collision_planes && !collided {
                    for plane in &e.collision_planes {
                        let n = Vec3::new(plane[0], plane[1], plane[2]);
                        let s_new = sign(p.location.dot(n) - plane[3]);
                        let s_old = sign(p.old_location.dot(n) - plane[3]);
                        if s_new != s_old {
                            let d = p.location - p.old_location;
                            let denom = d.dot(n);
                            let t = (plane[3] - p.old_location.dot(n)) / denom;
                            hit_point = p.old_location + d * t;
                            normal = n;
                            collided = true;
                            break;
                        }
                    }
                }
            }
            let mut skip_post = false;
            if collided {
                // 17d: collision sound table.
                self.collision_sound(ctx, at, hit_point);
                // 17e: spawn into another emitter.
                if e.spawn_from_other_emitter >= 0 && dt != 0.0 {
                    self.spawn_into_other(ctx, &e, normal, hit_point, dt);
                }
                // 17f: rest on landing.
                let v2 = self.particles[at].velocity.length_squared();
                if e.landed_min_squared_velocity != 0.0
                    && normal.z * e.landed_min_squared_velocity > v2
                {
                    let p = &mut self.particles[at];
                    p.location = hit_point + normal * 0.01;
                    p.flags |= flags::STOPPED;
                    if !e.b_rest_on_land {
                        continue;
                    }
                    let rr = [
                        e.rest_reset_flags[0],
                        e.rest_reset_flags[1],
                        e.rest_reset_flags[2],
                    ];
                    for axis in 0..3 {
                        p.start_spin[axis] = if rr[axis] {
                            e.rest_reset[axis] * 65535.0
                        } else {
                            p.time * p.spins_per_second[axis] + p.start_spin[axis]
                        };
                    }
                    p.spins_per_second = Vec3::ZERO;
                    continue;
                }
                // 17g: max collisions.
                if e.use_max_collisions {
                    let m = ctx.rand.range(e.max_collisions) as i32;
                    if self.particles[at].hit_count + 1 >= m {
                        if !e.respawn_dead_particles {
                            self.particles[at].flags &= !flags::ALIVE;
                            dead += 1;
                            continue;
                        }
                        self.spawn_particle(ctx, at, dt * 0.5, 0, 0, Vec3::ZERO);
                        skip_post = true;
                    }
                }
                // 17h: bounce.
                if !skip_post {
                    let mut n = normal;
                    if e.weird_bounce != 0.0 {
                        let n0 = n;
                        n = safe_normal(n + ctx.rand.random_unit_vector() * e.weird_bounce);
                        if n0.dot(n) < 0.0 {
                            n = -n;
                        }
                    }
                    if e.b_weird_spin_on_collide {
                        let p = &mut self.particles[at];
                        p.start_spin += p.time * p.spins_per_second;
                        let s = ctx.rand.vec_yxz(e.spins_per_second_range) * 65535.0;
                        p.spins_per_second = s;
                        p.start_spin -= p.time * p.spins_per_second;
                    }
                    let damping = ctx.rand.vec_yxz(e.damping_factor_range);
                    let rot_damping = if e.damp_rotation {
                        Some(ctx.rand.vec_yxz(e.rotation_damping_factor_range))
                    } else {
                        None
                    };
                    let p = &mut self.particles[at];
                    p.velocity -= e.acceleration * dt * 0.5;
                    p.velocity = mirror_by_vector(p.velocity, n);
                    p.velocity *= damping;
                    if let Some(r) = rot_damping {
                        p.spins_per_second *= r;
                    }
                    p.location = hit_point + n * 0.01;
                    p.old_location = p.location;
                    p.hit_count += 1;
                }
            }
            // Step 18: POST.
            self.post_stage(ctx, at, dt, inv_dt, &mut max_speed_scale, collided);
            hook(self, ctx, at, dt);
        }
        // Step E.
        let mut a = 1.0f32;
        if e.use_size_scale && !e.use_regular_size_scale {
            for s in &e.size_scale {
                a = a.max(s.relative_size);
            }
        }
        let mut b = 1.0f32;
        if e.scale_size_x_by_velocity {
            b = e.scale_size_by_velocity_multiplier.x.max(0.0);
            if e.scale_size_y_by_velocity {
                b = b.max(e.scale_size_by_velocity_multiplier.y);
            }
            if e.scale_size_z_by_velocity {
                b = b.max(e.scale_size_by_velocity_multiplier.z);
            }
        }
        self.max_size_scale = max_speed_scale * b * a;
        let all_dead = (dead >= self.max_active_particles || dead == self.active_particles)
            && rate == 0.0
            && !e.respawn_dead_particles;
        self.all_particles_dead = all_dead;
        self.active_particles.saturating_sub(dead)
    }

    fn collision_sound(&mut self, ctx: &mut Ctx<'_>, at: usize, hit: Vec3) {
        let e = &self.props;
        if e.collision_sound == SoundSelection::None || e.sounds.is_empty() {
            return;
        }
        let rand = &mut *ctx.rand;
        let idx0 = match e.collision_sound {
            SoundSelection::LinearGlobal => {
                let v = self.current_collision_sound_index;
                self.current_collision_sound_index += 1;
                v
            }
            SoundSelection::LinearLocal => self.particles[at].hit_count,
            SoundSelection::Random => (rand.app_srand() * 1000.0) as i32,
            SoundSelection::None => 0,
        };
        let lo = e.collision_sound_index.min.min(e.collision_sound_index.max);
        let span = e.collision_sound_index.max.max(e.collision_sound_index.min) - lo;
        let d = if span == 0.0 {
            e.sounds.len() as i32
        } else {
            (span + 1.0) as i32
        };
        if d == 0 {
            return;
        }
        // As read: the offset added is the field as written (Min), not the lower of the two.
        let i = (idx0 % d + e.collision_sound_index.min as i32).clamp(0, e.sounds.len() as i32 - 1)
            as usize;
        let s = e.sounds[i];
        let q = rand.range(e.collision_sound_probability);
        let ps = rand.range(s.probability);
        let u = rand.app_srand();
        if q * ps >= u {
            let factor = ctx.world.client_sound_factor();
            let volume = rand.range(s.volume) * ctx.owner.sound_volume * factor;
            let radius = rand.range(s.radius);
            let pitch = rand.range(s.pitch);
            ctx.events.push(Event::PlaySound {
                sound: s.sound,
                location: hit,
                volume,
                radius,
                pitch,
                on_spawn: false,
            });
        }
    }

    fn spawn_into_other(
        &mut self,
        ctx: &mut Ctx<'_>,
        e: &ParticleEmitterProps,
        n: Vec3,
        h: Vec3,
        dt: f32,
    ) {
        let target = e.spawn_from_other_emitter as usize;
        if target >= ctx.others.len() || target == ctx.me {
            return;
        }
        let Some(mut other) = ctx.others[target].take() else {
            return;
        };
        if other.initialized {
            for _ in 0..e.spawn_amount {
                let pos = h + n * 0.01;
                let at = other.particle_index;
                let saved_me = ctx.me;
                ctx.me = target;
                other.spawn_particle(ctx, at, dt, 0, 3, pos);
                ctx.me = saved_me;
                if e.use_spawned_velocity_scale {
                    let vs = ctx.rand.vec_yxz(e.spawned_velocity_scale_range);
                    if let Some(p) = other.particles.get_mut(at) {
                        p.velocity += vs * n;
                    }
                }
                other.advance_pool();
            }
        }
        ctx.others[target] = Some(other);
    }

    fn post_stage(
        &mut self,
        ctx: &mut Ctx<'_>,
        at: usize,
        dt: f32,
        inv_dt: f32,
        max_speed_scale: &mut f32,
        collided: bool,
    ) {
        let e = &self.props;
        let p = &mut self.particles[at];
        let age = p.time.min(p.max_lifetime);
        let f = if p.max_lifetime != 0.0 {
            clampf(age / p.max_lifetime, 0.0, 1.0)
        } else {
            0.0
        };
        let mut col = [1.0f32, 1.0, 1.0, 1.0];
        // 18.2 size.
        let mut s = 1.0f32;
        if e.use_size_scale {
            if e.use_regular_size_scale {
                s = 1.0 / (p.time + 1.0);
            } else {
                let x1 = if e.use_absolute_time_for_size_scale {
                    age
                } else {
                    f
                };
                let x = frac(x1 * (e.size_scale_repeats + 1.0));
                s = 1.0;
                if let Some(i) = e.size_scale.iter().position(|c| c.relative_time >= x) {
                    let cur = e.size_scale[i];
                    let (pt, ps) = if i > 0 {
                        (
                            e.size_scale[i - 1].relative_time,
                            e.size_scale[i - 1].relative_size,
                        )
                    } else {
                        (0.0, 1.0)
                    };
                    let a = if cur.relative_time != 0.0 {
                        (x - pt) / (cur.relative_time - pt)
                    } else {
                        1.0
                    };
                    s = lerp(ps, cur.relative_size, a);
                }
            }
            p.size = p.start_size * s;
        } else {
            p.size = p.start_size * s;
        }
        // 18.3 velocity scale.
        if e.use_velocity_scale && p.max_lifetime != 0.0 {
            let x = frac(f * (e.velocity_scale_repeats + 1.0));
            if let Some(i) = e.velocity_scale.iter().position(|c| c.relative_time >= x) {
                let cur = e.velocity_scale[i];
                let (pt, pv) = if i > 0 {
                    (
                        e.velocity_scale[i - 1].relative_time,
                        e.velocity_scale[i - 1].value,
                    )
                } else {
                    (0.0, Vec3::ONE)
                };
                let a = if cur.relative_time != 0.0 {
                    (x - pt) / (cur.relative_time - pt)
                } else {
                    1.0
                };
                p.velocity_multiplier = pv + (cur.value - pv) * a;
            }
        }
        // 18.4 size by velocity.
        if e.scale_size_x_by_velocity || e.scale_size_y_by_velocity || e.scale_size_z_by_velocity {
            let speed = if e.determine_velocity_by_location_difference {
                (p.location - p.old_location).length() * inv_dt
            } else {
                (p.velocity * p.velocity_multiplier).length()
            };
            *max_speed_scale = max_speed_scale.max(speed);
            let m = e.scale_size_by_velocity_multiplier;
            if e.scale_size_x_by_velocity {
                p.size.x *= speed * m.x;
            }
            if e.scale_size_y_by_velocity {
                p.size.y *= speed * m.y;
            }
            if e.scale_size_z_by_velocity {
                p.size.z *= speed * m.z;
            }
        }
        // 18.5 revolution scale.
        if e.use_revolution_scale && p.max_lifetime != 0.0 {
            let x = frac(f * (e.revolution_scale_repeats + 1.0));
            if let Some(i) = e.revolution_scale.iter().position(|c| c.relative_time >= x) {
                let cur = e.revolution_scale[i];
                let (pt, pv) = if i > 0 {
                    (
                        e.revolution_scale[i - 1].relative_time,
                        e.revolution_scale[i - 1].value,
                    )
                } else {
                    (0.0, Vec3::ONE)
                };
                let a = if cur.relative_time != 0.0 {
                    (x - pt) / (cur.relative_time - pt)
                } else {
                    1.0
                };
                p.revolutions_multiplier = pv + (cur.value - pv) * a;
            }
        }
        // 18.6 colour scale.
        if e.use_color_scale && p.max_lifetime != 0.0 {
            let x = frac(f * (e.color_scale_repeats + 1.0));
            if let Some(i) = e.color_scale.iter().position(|c| c.relative_time >= x) {
                let cur = e.color_scale[i];
                let (pt, pc) = if i > 0 {
                    (
                        e.color_scale[i - 1].relative_time,
                        e.color_scale[i - 1].color.to_unit(),
                    )
                } else {
                    (0.0, [1.0; 4])
                };
                let cc = cur.color.to_unit();
                let a = if cur.relative_time != 0.0 {
                    (x - pt) / (cur.relative_time - pt)
                } else {
                    1.0
                };
                for k in 0..3 {
                    col[k] = lerp(pc[k], cc[k], a);
                }
                col[3] = if e.draw_style == DrawStyle::AlphaBlend {
                    lerp(pc[3], cc[3], a)
                } else {
                    1.0
                };
            }
        }
        // 18.7.
        for k in 0..3 {
            col[k] *= p.color_multiplier[k];
        }
        // 18.8 fade.
        let fo = e.fade_out
            && p.fade_time > e.fade_out_start_time
            && e.fade_out_start_time != p.max_lifetime;
        let fi = e.fade_in && p.fade_time < e.fade_in_end_time && e.fade_in_end_time != 0.0;
        if fo || fi {
            let (g, fac) = if e.fade_out && p.fade_time > e.fade_out_start_time {
                (
                    (p.fade_time - e.fade_out_start_time)
                        / (p.max_lifetime - e.fade_out_start_time),
                    e.fade_out_factor,
                )
            } else {
                (
                    (e.fade_in_end_time - p.fade_time) / e.fade_in_end_time,
                    e.fade_in_factor,
                )
            };
            match e.draw_style {
                DrawStyle::Modulated => col = [0.5, 0.5, 0.5, 1.0 - fac[3] * g],
                DrawStyle::AlphaBlend => col[3] -= fac[3] * g,
                _ => {
                    for k in 0..3 {
                        col[k] -= fac[k] * g;
                    }
                }
            }
        }
        // 18.9 opacity.
        if e.opacity < 1.0 {
            match e.draw_style {
                DrawStyle::Regular => {}
                DrawStyle::AlphaBlend
                | DrawStyle::Modulated
                | DrawStyle::AlphaModulateMightNotFogCorrectly => col[3] *= e.opacity,
                _ => {
                    for c in col.iter_mut().take(3) {
                        *c *= e.opacity;
                    }
                }
            }
        }
        // 18.10 pack: round to nearest even, clamp.
        let byte = |c: f32| clampf((c * 255.0).round_ties_even(), 0.0, 255.0) as u8;
        p.color = Color {
            r: byte(col[0]),
            g: byte(col[1]),
            b: byte(col[2]),
            a: byte(col[3]),
        };
        // 18.11.
        self.bounding_box.add(p.location);
        // 18.12.
        for axis in 0..3 {
            let m = e.max_abs_velocity[axis];
            if m != 0.0 {
                p.velocity[axis] = clampf(p.velocity[axis], -m, m);
            }
        }
        // 18.13 velocity loss.
        let loss = ctx.rand.vec_yxz(self.real_velocity_loss_range);
        let v = p.velocity;
        p.velocity = v - loss * v * dt;
        // 18.14.
        if collided && p.velocity.length_squared() < e.min_squared_velocity {
            p.flags |= flags::STOPPED;
        }
        // 18.15: UpdateParticle is empty in the base class.
    }
}

/// A particle of a sibling emitter, by the original's `OtherIndex % ActiveParticles` rule.
fn peek_particle(
    me: &Emitter,
    others: &[Option<Emitter>],
    link: i32,
    other_index: i32,
) -> Option<Particle> {
    let target = usize::try_from(link).ok()?;
    let other = if let Some(Some(o)) = others.get(target) {
        o
    } else {
        // The slot is the emitter itself (or missing).
        if target < others.len() {
            me
        } else {
            return None;
        }
    };
    if other.active_particles == 0 {
        return None;
    }
    let i = other_index.rem_euclid(other.active_particles as i32) as usize;
    other.particles.get(i).cloned()
}
