//! Brick debris: how v20's client draws a dying brick.
//!
//! A blasted brick (`fakeKillBrick`, weapon blasts: v20's "brick
//! explosion") becomes a short-lived Rapier rigid body. It is thrown away
//! from the blast origin, tumbles against the map, terrain, nearby bricks
//! and other debris, then fades out like a ghost. A killed brick (hammer,
//! wands, undo, chain kills: `killBrick`) never collides: it hops up, spins
//! and falls straight through the world, fading after half a second, as
//! `blocklandv20.exe` draws it. Debris is purely cosmetic, like particles:
//! the server already hid or removed the brick, nothing about it is sent
//! over the network, and nothing in gameplay can see it. Each throw is
//! seeded from its cue id, so every client sees the same throw.
//!
//! Players and vehicles as this client draws them (see [`Pusher`]),
//! projectiles and later blasts push the bodies, one way only: pushers are
//! kinematic, so debris can never slow, block or move them.
//!
//! Like v20's `$pref::Physics::MaxBricks`, only a bounded number of bodies
//! are alive at once; the oldest make way for new ones. The player picks the
//! bound with Options' Physics Quality, and when debris work outgrows its
//! share of the frame the client keeps fewer until it recovers.
use crate::building::Building;
use crate::local_physics::{MAX_STEPS, PUSHER_REACH, Pushers, STEP, Shots, Surroundings};
pub use crate::local_physics::{Pusher, Shot};
use crate::world_chunks::BrickPalette;
use anyhow::{Context, Result, ensure};
use bri_net::protocol::PublicWorld;
use bri_render::scene::{GpuInstances, GpuScene, SceneData, SceneRenderer, SceneTransform};
use bri_sim::presentation::{BrickDeath, Cue, CueKind};
use bri_world::{BrickId, ContentRef};
use glam::{Mat4, Quat, Vec3};
use rapier3d::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;

/// The limit a player who never chose gets: v20's default Physics Quality,
/// High (see [`bri_ui::screens::options::debris_limit`]).
pub const DEFAULT_LIMIT: usize = bri_ui::screens::options::PHYSICS_LIMITS[1] as usize;
/// The highest limit `$pref::Physics::MaxBricks` may ask for.
pub const MAX_LIMIT: usize = bri_ui::screens::options::MAX_BRICKS_RANGE.1 as usize;
/// Debris work per frame (cues, pushes and physics) the client aims to stay
/// under: a third of a 60 Hz frame. The client learns what a moving body
/// costs on this PC and keeps no more than fit; above it for two frames
/// running, the oldest bodies go early.
pub const BUDGET: Duration = Duration::from_millis(6);
/// The budget never sheds below this many bodies.
const SHED_FLOOR: usize = 32;
/// Frames with fewer moving bodies than this say little about their cost.
const SAMPLE_FLOOR: usize = 16;
/// How fast the learned cost follows each frame's.
const LEARN_RATE: f64 = 0.2;
/// Seconds a body removed early (over the limit or the budget) takes to
/// fade out. It stops colliding and drifts on, so it costs nothing and
/// never pops out of sight.
const GHOST_SECONDS: f32 = 0.35;
/// Seconds a body stays solid before it starts to fade: long enough to
/// kick it around.
const SOLID_SECONDS: f32 = 10.0;
/// Seconds of fading to fully transparent, after which the body is removed.
const FADE_SECONDS: f32 = 3.0;
/// Converts v20 blast force into launch speed (units/s).
const FORCE_TO_SPEED: f32 = 0.5;
const MAX_SPEED: f32 = 40.0;
/// Cosmetic explosion cues waiting for the next debris frame.
const MAX_BLASTS: usize = 64;
/// Bodies are a hair smaller than the brick so neighbours killed together
/// don't start out interpenetrating.
const BODY_SHRINK: f32 = 0.96;
/// Momentum a projectile gives each body it passes, per unit of speed.
const PROJECTILE_MASS: f32 = 0.2;
/// Mass per cubic unit of debris: a 2x4 brick weighs 6.
const DENSITY: f32 = 5.0;
/// Distinct brick looks kept on the GPU.
const MAX_LOOKS: usize = 64;
/// Most instances one look draws: bodies and ghosts within the limit, plus
/// falling bricks.
const MAX_INSTANCES: usize = 3 * MAX_LIMIT;

// A killed brick, from `blocklandv20.exe`: the fxDTSBrick death update
// (0x5399a8-0x539c0a) throws it; its advance (0x53cfa9-0x53d11c) moves it
// with no collision; the colour update (0x53d29d-0x53d2ca, 0x53d553) fades
// it toward alpha 0 (set at 0x539965).
/// Launch speed: a random direction, mostly up, times 8.
const KILL_SPEED: f32 = 8.0;
/// Falls 16 t^2: gravity 32 units/s^2.
const KILL_GRAVITY: f32 = 32.0;
/// Opaque this long after it dies...
const KILL_SOLID_SECONDS: f32 = 0.5;
/// ...then its alpha closes on 0 at this rate per second.
const KILL_FADE_RATE: f32 = 3.0;
/// Fainter than this, a falling brick is gone.
const KILL_GONE: f32 = 1.0 / 255.0;
/// Most killed bricks falling at once (a huge chain kill). They cost no
/// physics, only drawing.
const MAX_FALLING: usize = MAX_LIMIT;

/// What a dead brick looks like; bodies with the same look share one model.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub definition: String,
    pub color: u8,
    pub color_effect: u8,
    pub shape_effect: u8,
    pub print: Option<ContentRef>,
}
impl Look {
    fn key(&self) -> (&str, u8, u8, u8, Option<String>) {
        (
            &self.definition,
            self.color,
            self.color_effect,
            self.shape_effect,
            self.print.as_ref().map(|p| format!("{p:?}")),
        )
    }
}
impl Eq for Look {}
impl PartialOrd for Look {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Look {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key().cmp(&other.key())
    }
}

struct Body {
    handle: RigidBodyHandle,
    brick: BrickId,
    look: Look,
    age: f32,
}
impl Body {
    /// How opaque the body is at its age.
    fn fade(&self) -> f32 {
        1.0 - ((self.age - SOLID_SECONDS) / FADE_SECONDS).clamp(0.0, 1.0)
    }
}

/// A killed brick as v20 draws it: no physics, just a hop, a spin and a
/// fall straight through everything while it fades. It moves in closed
/// form, so every frame rate draws the same path.
struct Falling {
    brick: BrickId,
    look: Look,
    start: Vec3,
    rotation: Quat,
    velocity: Vec3,
    axis: Vec3,
    /// Radians per second about `axis`.
    spin: f32,
    age: f32,
}
impl Falling {
    fn position(&self) -> Vec3 {
        self.start + self.velocity * self.age - Vec3::Y * (0.5 * KILL_GRAVITY * self.age * self.age)
    }
    fn rotation(&self) -> Quat {
        Quat::from_axis_angle(self.axis, self.spin * self.age) * self.rotation
    }
    fn fade(&self) -> f32 {
        (-KILL_FADE_RATE * (self.age - KILL_SOLID_SECONDS).max(0.0)).exp()
    }
}

/// A body removed early, fading out where it was heading.
struct Ghost {
    look: Look,
    position: Vec3,
    rotation: Quat,
    velocity: Vec3,
    spin: Vec3,
    /// Opacity when it was removed.
    fade: f32,
    left: f32,
}

#[derive(Clone, Copy)]
struct CosmeticBlast {
    tick: u64,
    origin: Vec3,
    normal: Option<Vec3>,
    force: f32,
    radius: f32,
}

#[derive(Clone, Debug, Default)]
pub struct BrickDebrisDiagnostics {
    pub accepted: u64,
    pub duplicates: u64,
    /// Cues for bricks this client has no definition for.
    pub unknown: u64,
    /// Oldest bodies removed early to stay within the limit.
    pub evicted: u64,
    /// Oldest bodies removed early because debris outgrew its budget.
    pub shed: u64,
    /// Blasts that left no debris (Physics Quality Off, or the budget has
    /// no room right now), and kills beyond the falling bricks drawn at once.
    pub skipped: u64,
    pub dropped_steps: u64,
    pub projectile_hits: u64,
}

/// Debris physics work at one moment (see [`BrickDebris::work`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebrisWork {
    pub bodies: usize,
    /// Bodies still moving; sleeping ones cost almost nothing.
    pub awake: usize,
    /// Map bricks and terrain patches made solid around moving bodies.
    pub statics: usize,
    /// Collider pairs in contact, which the solver works through each step.
    pub touching: usize,
}

pub struct BrickDebris {
    world: PhysicsWorld,
    /// Keyed by cue id, so iteration runs oldest first.
    bodies: BTreeMap<u64, Body>,
    surroundings: Surroundings,
    /// Bricks this client saw die that have not come back yet.
    dead: BTreeSet<BrickId>,
    cursor: u64,
    explosion_cursor: u64,
    pending_blasts: Vec<CosmeticBlast>,
    accumulator: f32,
    pushers: Pushers,
    shots: Shots,
    /// The player's limit (Physics Quality or `$pref::Physics::MaxBricks`).
    limit: usize,
    /// The limit the budget allows right now; at most `limit`.
    room: usize,
    /// Frames in a row over budget.
    over: u32,
    /// Learned seconds of debris work per moving body per frame on this PC.
    per_body: Option<f64>,
    /// Bodies were thrown this frame: its cost is the spawn, not the
    /// tumbling, so it teaches nothing.
    threw: bool,
    ghosts: Vec<Ghost>,
    falling: Vec<Falling>,
    pub diagnostics: BrickDebrisDiagnostics,
}

impl Default for BrickDebris {
    fn default() -> Self {
        Self::new()
    }
}

impl BrickDebris {
    pub fn new() -> Self {
        Self {
            world: crate::local_physics::new_world(),
            bodies: BTreeMap::new(),
            surroundings: Surroundings::default(),
            dead: BTreeSet::new(),
            cursor: 0,
            explosion_cursor: 0,
            pending_blasts: Vec::new(),
            accumulator: 0.0,
            pushers: Pushers::default(),
            shots: Shots::default(),
            limit: DEFAULT_LIMIT,
            room: DEFAULT_LIMIT,
            over: 0,
            per_body: None,
            threw: false,
            ghosts: Vec::new(),
            falling: Vec::new(),
            diagnostics: Default::default(),
        }
    }
    /// Forget everything (disconnect, new server). The limit and what
    /// debris costs on this PC stay.
    pub fn clear(&mut self) {
        let (limit, per_body) = (self.limit, self.per_body);
        *self = Self::new();
        self.per_body = per_body;
        self.set_limit(limit);
    }
    /// Keep at most `limit` bodies (clamped to [`MAX_LIMIT`]); extra bodies
    /// go now, oldest first.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit.min(MAX_LIMIT);
        self.over = 0;
        self.fit_room();
        self.diagnostics.evicted += self.evict_to(self.limit);
    }
    /// Room for as many bodies as the budget pays for, within the limit.
    fn fit_room(&mut self) {
        let fit = self.per_body.map_or(usize::MAX, |p| {
            ((BUDGET.as_secs_f64() / p) as usize).max(SHED_FLOOR)
        });
        self.room = fit.min(self.limit);
    }
    pub fn limit(&self) -> usize {
        self.limit
    }
    /// How many bodies the budget allows right now.
    pub fn room(&self) -> usize {
        self.room
    }
    /// What this frame's debris work cost. It teaches the client what a
    /// moving body costs on this PC, so later blasts keep only as many as
    /// [`BUDGET`] pays for. Over budget two frames running, the oldest
    /// bodies beyond that go early.
    pub fn spent(&mut self, cost: Duration) {
        let moving = self
            .bodies
            .values()
            .filter(|b| !self.world.bodies[b.handle].is_sleeping())
            .count();
        let threw = std::mem::take(&mut self.threw);
        if moving >= SAMPLE_FLOOR && !threw {
            let sample = cost.as_secs_f64() / moving as f64;
            self.per_body = Some(
                self.per_body
                    .map_or(sample, |p| p + (sample - p) * LEARN_RATE),
            );
            self.fit_room();
        }
        if cost <= BUDGET {
            self.over = 0;
            return;
        }
        self.over += 1;
        if self.over >= 2 {
            self.over = 0;
            self.diagnostics.shed += self.evict_to(self.room);
        }
    }
    /// Remove the oldest bodies until at most `keep` remain; returns how
    /// many went. Bodies already seen fade out as ghosts; ones killed and
    /// removed before a frame drew them just go.
    fn evict_to(&mut self, keep: usize) -> u64 {
        let mut gone = 0;
        while self.bodies.len() > keep {
            let (_, oldest) = self.bodies.pop_first().expect("bodies over the limit");
            let rb = &self.world.bodies[oldest.handle];
            if oldest.age > 0.0 && self.ghosts.len() < self.limit {
                self.ghosts.push(Ghost {
                    position: Vec3::from_array(rb.translation().to_array()),
                    rotation: Quat::from_array(rb.rotation().to_array()),
                    velocity: Vec3::from_array(rb.linvel().to_array()),
                    spin: Vec3::from_array(rb.angvel().to_array()),
                    fade: oldest.fade(),
                    left: GHOST_SECONDS,
                    look: oldest.look,
                });
            }
            self.world.remove_body_with_colliders(oldest.handle, true);
            gone += 1;
        }
        gone
    }
    /// Bodies fading out after being removed early.
    pub fn ghosts(&self) -> usize {
        self.ghosts.len()
    }
    /// Killed bricks falling through the world. They are not bodies.
    pub fn falling(&self) -> usize {
        self.falling.len()
    }
    pub fn len(&self) -> usize {
        self.bodies.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }
    /// Whether `brick` died (or was fake-killed) and hasn't come back. Such
    /// bricks are not "hidden" bricks the building tools reveal.
    pub fn is_dead(&self, brick: BrickId) -> bool {
        self.dead.contains(&brick)
    }
    /// A real explosion can move existing debris even when no new brick dies.
    /// The native projectile's brick force supplies cosmetic launch strength;
    /// its impulse radius supplies reach. Unrecognized effects supply neither.
    pub fn explosion_cue(&mut self, cue: &Cue, pack: &bri_weapons::Pack) {
        if cue.id <= self.explosion_cursor {
            return;
        }
        self.explosion_cursor = cue.id;
        let CueKind::WeaponEffect {
            definition,
            direction,
            scale,
            node,
            image: None,
            ..
        } = &cue.kind
        else {
            return;
        };
        if !node.is_empty() || cue.validate().is_err() || self.pending_blasts.len() >= MAX_BLASTS {
            return;
        }
        let (force, radius) = pack
            .projectiles
            .values()
            .filter(|p| p.explosion.effect.eq_ignore_ascii_case(definition))
            .fold((0.0_f32, 0.0_f32), |(force, radius), p| {
                (
                    force.max(p.brick.force),
                    radius.max(p.brick.radius.max(p.explosion.impulse_radius)),
                )
            });
        if force <= 0.0 || radius <= 0.0 {
            return;
        }
        self.pending_blasts.push(CosmeticBlast {
            tick: cue.tick,
            origin: Vec3::from(cue.position),
            normal: direction
                .map(Vec3::from)
                .filter(|n| n.length_squared() > 1e-6)
                .map(Vec3::normalize),
            force: (force * scale).min(bri_sim::presentation::MAX_BRICK_FORCE),
            radius: (radius * scale).min(bri_sim::presentation::MAX_BRICK_FORCE),
        });
    }
    /// Turn new `BrickKill` cues into debris and return how many there were:
    /// falling bricks for kills, bodies for blasts. Other cues are ignored.
    pub fn cues<'a>(
        &mut self,
        cues: impl IntoIterator<Item = &'a Cue>,
        building: &Building,
    ) -> Result<usize> {
        let mut spawned = 0;
        let explosions = std::mem::take(&mut self.pending_blasts);
        for blast in &explosions {
            self.blast(blast.origin, blast.force, blast.radius, blast.normal);
        }
        // One blast kills many bricks, one cue each: it shoves the debris
        // already flying once, not once per brick it killed.
        let mut last_blast = None;
        for cue in cues {
            let CueKind::BrickKill {
                brick,
                death,
                definition,
                quarter_turns,
                color,
                color_effect,
                shape_effect,
                print,
                origin,
                force,
                radius,
            } = &cue.kind
            else {
                continue;
            };
            if cue.id <= self.cursor {
                self.diagnostics.duplicates += 1;
                continue;
            }
            cue.validate()?;
            self.cursor = cue.id;
            let ContentRef::Resolved(definition) = definition else {
                unreachable!("validated brick kill definition");
            };
            let Some(half) = building.definition_half_extents(definition) else {
                self.diagnostics.unknown += 1;
                continue;
            };
            self.dead.insert(*brick);
            let look = Look {
                definition: definition.clone(),
                color: *color,
                color_effect: *color_effect,
                shape_effect: *shape_effect,
                print: print.clone(),
            };
            if *death == BrickDeath::Kill {
                if self.falling.len() >= MAX_FALLING {
                    self.diagnostics.skipped += 1;
                    continue;
                }
                self.fall(
                    cue.id,
                    *brick,
                    look,
                    Vec3::from(cue.position),
                    *quarter_turns,
                    half,
                );
                self.diagnostics.accepted += 1;
                spawned += 1;
                continue;
            }
            // A big blast also shoves the debris already flying around it.
            let blast = (*origin, *force, *radius);
            let explosion = explosions
                .iter()
                .find(|b| b.tick == cue.tick && b.origin == Vec3::from(*origin));
            if *radius > 0.5 && last_blast != Some(blast) && explosion.is_none() {
                self.blast(Vec3::from(*origin), *force, *radius, None);
            }
            last_blast = Some(blast);
            // The dead brick must never hold up its own debris.
            self.surroundings.forget_brick(&mut self.world, *brick);
            if self.room == 0 {
                self.diagnostics.skipped += 1;
                continue;
            }
            self.spawn(
                cue.id,
                *brick,
                look,
                Vec3::from(cue.position),
                *quarter_turns,
                half,
                Vec3::from(*origin),
                *force,
                *radius,
                explosion.filter(|_| *radius > 1.0).and_then(|b| b.normal),
            );
            self.diagnostics.accepted += 1;
            spawned += 1;
            self.threw = true;
        }
        Ok(spawned)
    }
    /// v20's `killBrick` throw: straight up plus up to a quarter sideways,
    /// at 8 units/s, spinning about a random axis. Small bricks spin faster:
    /// up to `8 / length` radians per second (integer studs), kept within
    /// 3..=8.
    fn fall(
        &mut self,
        id: u64,
        brick: BrickId,
        look: Look,
        center: Vec3,
        quarter_turns: u8,
        half: Vec3,
    ) {
        let mut rng = Seeded::new(id);
        // v20 (Z up): x and y in [-0.5, 0.5), z in [1, 5).
        let sideways = Vec3::new(rng.unit() - 0.5, 0.0, rng.unit() - 0.5);
        let up = 1.0 + 4.0 * rng.unit();
        let velocity = (sideways + Vec3::Y * up).normalize() * KILL_SPEED;
        let axis =
            Vec3::new(rng.unit() - 0.5, rng.unit() - 0.5, rng.unit() - 0.5).normalize_or(Vec3::Y);
        // `brickSizeY`, which the brick's footprint stores second.
        let studs = ((half.z * 4.0).round() as i32).max(1);
        let spin = rng.unit() * (8 / studs).clamp(3, 8) as f32;
        self.falling.push(Falling {
            brick,
            look,
            start: center,
            rotation: Quat::from_rotation_y(
                -f32::from(quarter_turns) * std::f32::consts::FRAC_PI_2,
            ),
            velocity,
            axis,
            spin,
            age: 0.0,
        });
    }
    #[allow(clippy::too_many_arguments)] // one cue's fields
    fn spawn(
        &mut self,
        id: u64,
        brick: BrickId,
        look: Look,
        center: Vec3,
        quarter_turns: u8,
        half: Vec3,
        origin: Vec3,
        force: f32,
        radius: f32,
        normal: Option<Vec3>,
    ) {
        self.diagnostics.evicted += self.evict_to(self.room - 1);
        let mut rng = Seeded::new(id);
        let offset = center - origin;
        let distance = offset.length();
        let direction = if distance > 1e-4 {
            offset / distance
        } else {
            Vec3::Y
        };
        // Small blasts (direct hits, events, hammer pops) throw at full force;
        // radius blasts weaken towards their edge.
        let falloff = if radius > 0.5 {
            (1.0 - distance / radius).clamp(0.25, 1.0)
        } else {
            1.0
        };
        let falloff = if normal.is_some() {
            falloff.max(0.4)
        } else {
            falloff
        };
        let wobble = Vec3::new(rng.signed(), rng.signed(), rng.signed()) * 0.25;
        // A wall impact's center-to-origin vector points into the wall.
        // Its observed surface normal sends the broken pieces out of that
        // face instead of wedging them against surviving backing bricks.
        let direction =
            (direction + normal.unwrap_or(Vec3::ZERO) * 2.0 + wobble).normalize_or(Vec3::Y);
        let speed = (force * FORCE_TO_SPEED * falloff * (0.85 + 0.3 * rng.unit())).min(MAX_SPEED);
        let mut velocity = direction * speed;
        if normal.is_some() && speed > 0.0 {
            // A modest lift separates pieces from the floor/shelves before
            // friction can put them back to sleep. Authored event throws,
            // which have no explosion surface cue, retain their direction.
            velocity.y = velocity.y.max((speed * 0.45).min(8.0));
        }
        velocity = velocity.clamp_length_max(MAX_SPEED);
        let spin = Vec3::new(rng.signed(), rng.signed(), rng.signed()).normalize_or(Vec3::X)
            * (2.0 + 6.0 * rng.unit());
        let rotation = Rotation::from_scaled_axis(
            Vector::Y * (-f32::from(quarter_turns) * std::f32::consts::FRAC_PI_2),
        );
        let body = RigidBodyBuilder::dynamic()
            .pose(Pose::from_parts(
                Vector::from_array(center.to_array()),
                rotation,
            ))
            .linvel(Vector::from_array(velocity.to_array()))
            .angvel(Vector::from_array(spin.to_array()))
            .linear_damping(0.1)
            .angular_damping(0.4)
            .ccd_enabled(true);
        let h = half * BODY_SHRINK;
        let collider = ColliderBuilder::cuboid(h.x.max(0.01), h.y.max(0.01), h.z.max(0.01))
            .density(DENSITY)
            .friction(0.7)
            .restitution(0.25);
        let (handle, _) = self.world.insert(body, collider);
        self.bodies.insert(
            id,
            Body {
                handle,
                brick,
                look,
                age: 0.0,
            },
        );
    }
    /// Advance debris by `dt` seconds against the current surroundings.
    pub fn advance(&mut self, dt: f32, building: &Building) -> Result<()> {
        ensure!(dt.is_finite() && dt >= 0.0, "Invalid debris frame time");
        for falling in &mut self.falling {
            falling.age += dt;
        }
        self.falling.retain(|f| f.fade() > KILL_GONE);
        for ghost in &mut self.ghosts {
            ghost.position += ghost.velocity * dt;
            ghost.rotation = (Quat::from_scaled_axis(ghost.spin * dt) * ghost.rotation).normalize();
            ghost.left -= dt;
        }
        self.ghosts.retain(|g| g.left > 0.0);
        if self.bodies.is_empty() {
            self.accumulator = 0.0;
            self.surroundings.clear(&mut self.world);
            self.pushers.clear(&mut self.world);
            return Ok(());
        }
        // Bricks changed: the solid surroundings are rebuilt from scratch.
        self.surroundings.sync(&mut self.world, building);
        self.accumulator += dt;
        let mut steps = (self.accumulator / STEP) as u32;
        if steps > MAX_STEPS {
            self.diagnostics.dropped_steps += u64::from(steps - MAX_STEPS);
            self.accumulator = 0.0;
            steps = MAX_STEPS;
        } else {
            self.accumulator -= steps as f32 * STEP;
        }
        if steps == 0 {
            return Ok(());
        }
        self.load_surroundings(building, steps as f32 * STEP)?;
        for step in 1..=steps {
            self.pushers
                .drive(&mut self.world, step as f32 / steps as f32);
            self.world.step();
            self.age(STEP);
        }
        self.pushers.settle();
        Ok(())
    }
    /// Where players and vehicles are this frame. Call before `advance`.
    /// Only those near debris take part; they are kinematic, so debris
    /// moves out of their way and never moves them.
    pub fn push(&mut self, pushers: &[Pusher]) {
        let (world, bodies) = (&mut self.world, &self.bodies);
        let centers: Vec<Vec3> = bodies
            .values()
            .map(|b| Vec3::from_array(world.bodies[b.handle].translation().to_array()))
            .collect();
        self.pushers.update(world, pushers, |p| {
            let reach = PUSHER_REACH + p.half.max_element();
            centers.iter().any(|c| c.distance(p.center) < reach)
        });
    }
    /// Projectiles as drawn this frame. Each pushes every body it passes
    /// through once, and flies on as if the debris were not there.
    pub fn shots(&mut self, shots: &[Shot]) {
        let owners: HashMap<RigidBodyHandle, u64> =
            self.bodies.iter().map(|(id, b)| (b.handle, *id)).collect();
        for strike in self.shots.strike(&self.world, shots, &owners) {
            let rb = &mut self.world.bodies[strike.handle];
            let impulse = strike.direction * PROJECTILE_MASS * strike.speed;
            rb.apply_impulse_at_point(
                Vector::from_array(impulse.to_array()),
                Vector::from_array(strike.point.to_array()),
                true,
            );
            let v = Vec3::from_array(rb.linvel().to_array()).clamp_length_max(MAX_SPEED);
            rb.set_linvel(Vector::from_array(v.to_array()), true);
            self.diagnostics.projectile_hits += 1;
        }
    }
    /// Shove every body within `radius` of `origin` away from it.
    fn blast(&mut self, origin: Vec3, force: f32, radius: f32, normal: Option<Vec3>) {
        if !origin.is_finite()
            || !force.is_finite()
            || force <= 0.0
            || !radius.is_finite()
            || radius <= 0.0
        {
            return;
        }
        for body in self.bodies.values() {
            let rb = &mut self.world.bodies[body.handle];
            let at = Vec3::from_array(rb.translation().to_array());
            let offset = at - origin;
            let distance = offset.length();
            if distance > radius {
                continue;
            }
            let direction = (offset.normalize_or(Vec3::Y) + normal.unwrap_or(Vec3::ZERO) * 2.0)
                .normalize_or(Vec3::Y);
            let falloff = (1.0 - distance / radius).clamp(0.25, 1.0);
            let speed = (force * FORCE_TO_SPEED * falloff).min(MAX_SPEED);
            let mut kick = direction * speed;
            if normal.is_some() {
                kick.y = kick.y.max((speed * 0.45).min(8.0));
            }
            let v = (Vec3::from_array(rb.linvel().to_array()) + kick).clamp_length_max(MAX_SPEED);
            rb.set_linvel(Vector::from_array(v.to_array()), true);
        }
    }
    fn age(&mut self, dt: f32) {
        let mut expired = Vec::new();
        for (id, body) in &mut self.bodies {
            body.age += dt;
            if body.age >= SOLID_SECONDS + FADE_SECONDS {
                expired.push(*id);
            }
        }
        for id in expired {
            if let Some(body) = self.bodies.remove(&id) {
                self.world.remove_body_with_colliders(body.handle, true);
            }
        }
    }
    /// Make bricks and terrain around every awake body solid for the next
    /// `seconds` of motion.
    fn load_surroundings(&mut self, building: &Building, seconds: f32) -> Result<()> {
        let boxes: Vec<_> = self
            .bodies
            .values()
            .map(|body| &self.world.bodies[body.handle])
            .filter(|rb| !rb.is_sleeping())
            .map(|rb| {
                Surroundings::reach(
                    Vec3::from_array(rb.translation().to_array()),
                    Vec3::from_array(rb.linvel().to_array()),
                    0.0,
                    seconds,
                )
            })
            .collect();
        let dead = &self.dead;
        self.surroundings
            .load(&mut self.world, building, &boxes, |id| dead.contains(&id))
    }
    /// Forget deaths the world has since undone (respawned bricks) or made
    /// permanent (removed bricks).
    pub fn sync_world(&mut self, world: &PublicWorld) {
        let alive: BTreeSet<BrickId> = self
            .bodies
            .values()
            .map(|b| b.brick)
            .chain(self.falling.iter().map(|f| f.brick))
            .collect();
        // A cue can arrive before the brick change it announces, so a
        // brick with live debris counts as dead even while still visible.
        self.dead.retain(|id| {
            world
                .bricks
                .get(id)
                .is_some_and(|b| !b.visible || alive.contains(id))
        });
    }
    /// World transform and fade of every body and falling brick, by look.
    pub fn instances(&self) -> impl Iterator<Item = (&Look, SceneTransform)> {
        let bodies = self.bodies.values().map(|body| {
            let rb = &self.world.bodies[body.handle];
            let rotation = Quat::from_array(rb.rotation().to_array());
            let translation = Vec3::from_array(rb.translation().to_array());
            (
                &body.look,
                SceneTransform {
                    transform: Mat4::from_rotation_translation(rotation, translation),
                    tint: [1.0, 1.0, 1.0, body.fade()],
                },
            )
        });
        let ghosts = self.ghosts.iter().map(|g| {
            (
                &g.look,
                SceneTransform {
                    transform: Mat4::from_rotation_translation(g.rotation, g.position),
                    tint: [
                        1.0,
                        1.0,
                        1.0,
                        g.fade * (g.left / GHOST_SECONDS).clamp(0.0, 1.0),
                    ],
                },
            )
        });
        let falling = self.falling.iter().map(|f| {
            (
                &f.look,
                SceneTransform {
                    transform: Mat4::from_rotation_translation(f.rotation(), f.position()),
                    tint: [1.0, 1.0, 1.0, f.fade()],
                },
            )
        });
        bodies.chain(ghosts).chain(falling)
    }
    /// What the physics has to chew on right now, for probes.
    pub fn work(&self) -> DebrisWork {
        DebrisWork {
            bodies: self.bodies.len(),
            awake: self
                .bodies
                .values()
                .filter(|b| !self.world.bodies[b.handle].is_sleeping())
                .count(),
            statics: self.surroundings.len(),
            touching: self
                .world
                .contact_pairs()
                .filter(|p| p.has_any_active_contact())
                .count(),
        }
    }
    /// Body centers, for tests and diagnostics.
    pub fn positions(&self) -> Vec<Vec3> {
        self.bodies
            .values()
            .map(|b| Vec3::from_array(self.world.bodies[b.handle].translation().to_array()))
            .collect()
    }
}

/// GPU models for debris, one shared mesh per look. Looks index the shared
/// brick material palette, so a new look uploads only its geometry: brick
/// textures and bind groups are never uploaded again for debris.
#[derive(Default)]
pub struct DebrisModels {
    models: BTreeMap<Look, Model>,
    frame: u64,
    pub diagnostics: DebrisModelDiagnostics,
}
#[derive(Clone, Debug, Default)]
pub struct DebrisModelDiagnostics {
    /// Look meshes built and uploaded.
    pub looks_built: u64,
    /// Texture images those uploads carried; the palette holds them all.
    pub images_uploaded: u64,
}
struct Model {
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    transforms: Vec<SceneTransform>,
    used: u64,
}
impl DebrisModels {
    pub fn clear(&mut self) {
        self.models.clear();
    }
    /// Build missing looks against `gpu_palette` (the uploaded `palette`)
    /// and upload this frame's transforms.
    #[allow(clippy::too_many_arguments)] // GPU context plus the brick catalogs
    pub fn upload(
        &mut self,
        debris: &BrickDebris,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &BTreeMap<String, bri_content::brick::Brick>,
        palette: &BrickPalette,
        gpu_palette: &GpuScene,
        materials: &crate::materials::BrickMaterials,
        colors: &[[f32; 4]],
    ) -> Result<()> {
        self.frame += 1;
        for model in self.models.values_mut() {
            model.transforms.clear();
        }
        for (look, transform) in debris.instances() {
            if transform.tint[3] <= 0.0 {
                continue;
            }
            if !self.models.contains_key(look) {
                if usize::from(look.color) >= colors.len() {
                    continue;
                }
                if self.models.len() >= MAX_LOOKS {
                    self.evict();
                }
                let data = look_scene(look, meshes, palette, materials, colors)
                    .with_context(|| format!("Debris model for {}", look.definition))?;
                let gpu = data
                    .map(|data| {
                        self.diagnostics.looks_built += 1;
                        self.diagnostics.images_uploaded += data.images.len() as u64;
                        renderer.upload_palette_model(device, &data, gpu_palette)
                    })
                    .transpose()?;
                self.models.insert(
                    look.clone(),
                    Model {
                        gpu,
                        instances: None,
                        transforms: Vec::new(),
                        used: self.frame,
                    },
                );
            }
            let model = self.models.get_mut(look).expect("model built above");
            model.transforms.push(transform);
            model.used = self.frame;
        }
        for model in self.models.values_mut() {
            if model.gpu.is_none() {
                continue;
            }
            // Room for this look's bodies, grown in steps as the limit allows.
            let wanted = model.transforms.len();
            if wanted > 0
                && model
                    .instances
                    .as_ref()
                    .is_none_or(|i| i.capacity() < wanted)
            {
                let capacity = wanted.next_power_of_two().clamp(64, MAX_INSTANCES);
                model.instances = Some(GpuInstances::new(device, capacity)?);
            }
            if let Some(instances) = &mut model.instances {
                instances.update(queue, &model.transforms)?;
            }
        }
        Ok(())
    }
    /// Drop the least recently used look that has no bodies this frame.
    fn evict(&mut self) {
        if let Some(look) = self
            .models
            .iter()
            .filter(|(_, m)| m.used != self.frame)
            .min_by_key(|(_, m)| m.used)
            .map(|(look, _)| look.clone())
        {
            self.models.remove(&look);
        }
    }
    pub fn draws(&self) -> Vec<(&GpuScene, &GpuInstances)> {
        self.models
            .values()
            .filter(|m| !m.transforms.is_empty())
            .filter_map(|m| Some((m.gpu.as_ref()?, m.instances.as_ref()?)))
            .collect()
    }
}

/// One brick at the origin in its own frame, drawn like a planted brick
/// with the planted bricks' materials.
fn look_scene(
    look: &Look,
    meshes: &BTreeMap<String, bri_content::brick::Brick>,
    palette: &BrickPalette,
    materials: &crate::materials::BrickMaterials,
    colors: &[[f32; 4]],
) -> Result<Option<SceneData>> {
    let mut brick =
        bri_world::Brick::new(ContentRef::Resolved(look.definition.clone()), [0.0; 3], 0);
    brick.color = look.color;
    brick.color_effect = look.color_effect;
    brick.shape_effect = look.shape_effect;
    brick.print = look.print.clone();
    let data = crate::world_chunks::build_brick(&brick, colors, meshes, palette, Some(materials))?;
    Ok((!data.indices.is_empty()).then_some(data))
}

/// SplitMix64: the same throw from the same cue on every client.
struct Seeded(u64);
impl Seeded {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x6a09_e667_f3bc_c909)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f32 {
        ((self.next() >> 40) as f32) / (1u32 << 24) as f32
    }
    fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use bri_content::collision::{CollisionBody, Part};
    use bri_sim::definitions::{Definition, Definitions};

    /// A flat floor at y 0 and 1x0.6x1 bricks at `bricks`.
    pub(crate) fn building(bricks: &[(BrickId, [f32; 3])]) -> (Building, PublicWorld) {
        let definition = Definition {
            mesh: bri_content::brick::Brick {
                schema_version: 1,
                id: "brick".into(),
                footprint_studs: [2, 2],
                height_plates: 3,
                attachment_rows: vec!["bb".into(); 6],
                collision_boxes: vec![],
                needs_external_collision: false,
                coverage: None,
                quads: vec![],
            },
            shape: SharedShape::cuboid(0.5, 0.3, 0.5),
            collision: CollisionBody {
                id: "brick".into(),
                parts: vec![Part::Box {
                    center: [0.0; 3],
                    size: [1.0, 0.6, 1.0],
                }],
            },
            indestructible: false,
            special: Default::default(),
            reflection: None,
            link: None,
            glass: [0.0; 4],
            bot: None,
        };
        let floor =
            ColliderBuilder::cuboid(50.0, 0.5, 50.0).translation(Vector::new(0.0, -0.5, 0.0));
        let mut building = Building::new(
            Definitions {
                entries: [("brick".into(), definition)].into(),
            },
            vec![floor],
        )
        .unwrap();
        let world = PublicWorld {
            name: "Debris".into(),
            map_id: "map".into(),
            palette: vec![[1.0; 4]; 2],
            bricks: bricks
                .iter()
                .map(|(id, p)| {
                    (
                        *id,
                        bri_world::Brick::new(ContentRef::Resolved("brick".into()), *p, 1),
                    )
                })
                .collect(),
        };
        building.sync_world(&world).unwrap();
        (building, world)
    }
    pub(crate) fn kill(
        id: u64,
        brick: BrickId,
        at: [f32; 3],
        origin: [f32; 3],
        force: f32,
        radius: f32,
    ) -> Cue {
        Cue {
            id,
            tick: 1,
            position: at,
            kind: CueKind::BrickKill {
                brick,
                death: BrickDeath::Blast,
                definition: ContentRef::Resolved("brick".into()),
                quarter_turns: 1,
                color: 1,
                color_effect: 0,
                shape_effect: 0,
                print: None,
                origin,
                force,
                radius,
            },
        }
    }
    /// A hammer, wand or undo kill: v20 `killBrick`.
    pub(crate) fn tool_kill(id: u64, brick: BrickId, at: [f32; 3]) -> Cue {
        let mut cue = kill(id, brick, at, [at[0], at[1] - 1.0, at[2]], 12.0, 0.0);
        if let CueKind::BrickKill { death, .. } = &mut cue.kind {
            *death = BrickDeath::Kill;
        }
        cue
    }
    fn run(debris: &mut BrickDebris, building: &Building, seconds: f32) {
        for _ in 0..(seconds * 60.0) as usize {
            debris.advance(1.0 / 60.0, building).unwrap();
        }
    }

    #[test]
    fn a_tool_kill_hops_spins_and_falls_through_everything_as_it_fades() {
        // Brick 7 sits on brick 8 on the floor; brick 9 is a wall beside it.
        let (building, world) = building(&[
            (7, [0.0, 0.9, 0.0]),
            (8, [0.0, 0.3, 0.0]),
            (9, [1.0, 0.9, 0.0]),
        ]);
        let mut debris = BrickDebris::new();
        // The cue arrives before the world update that removes the brick.
        debris
            .cues(&[tool_kill(1, 7, [0.0, 0.9, 0.0])], &building)
            .unwrap();
        assert!(debris.is_dead(7));
        // No physics body: nothing to collide with, push or budget.
        assert_eq!((debris.len(), debris.falling()), (0, 1));
        assert!(debris.is_empty());
        debris.sync_world(&world);
        assert!(debris.is_dead(7), "still dying while its brick is drawn");
        let at = |d: &BrickDebris| d.instances().next().map(|(_, t)| t);
        let start = at(&debris).unwrap();
        assert_eq!(start.tint[3], 1.0);
        let mut peak = 0.0f32;
        let mut spun = false;
        for _ in 0..15 {
            debris.advance(1.0 / 60.0, &building).unwrap();
            let t = at(&debris).unwrap();
            peak = peak.max(t.transform.w_axis.y);
            spun |= t.transform.x_axis != start.transform.x_axis;
        }
        // 8 units/s mostly up against 32 units/s^2: a hop of at most one.
        assert!(peak > 0.9 + 0.3 && peak < 0.9 + 1.05, "hopped to {peak}");
        assert!(spun || debris.falling[0].spin < 0.1);
        // Opaque for half a second...
        run(&mut debris, &building, 0.2);
        assert_eq!(at(&debris).unwrap().tint[3], 1.0);
        // ...by when it is falling straight through brick 8 and the floor.
        run(&mut debris, &building, 0.3);
        let t = at(&debris).unwrap();
        assert!(
            t.transform.w_axis.y < -0.5,
            "held up at {}",
            t.transform.w_axis
        );
        assert!(t.tint[3] < 1.0);
        // Faint by 1.5 s, gone by 2.4 s.
        run(&mut debris, &building, 0.75);
        let t = at(&debris).unwrap();
        assert!(t.tint[3] < 0.06 && t.tint[3] > 0.0, "{}", t.tint[3]);
        run(&mut debris, &building, 0.9);
        assert_eq!(debris.falling(), 0);
        assert_eq!(debris.instances().count(), 0);
    }

    #[test]
    fn tool_kills_fall_the_same_on_every_client_and_frame_rate_whatever_the_limit() {
        let (building, _) = building(&[]);
        let cues: Vec<_> = (0..40u64)
            .map(|i| tool_kill(i + 1, i + 1, [i as f32, 0.3, 0.0]))
            .collect();
        let mut a = BrickDebris::new();
        let mut b = BrickDebris::new();
        // Physics Quality Off: v20 draws `killBrick` without physics.
        b.set_limit(0);
        a.cues(&cues, &building).unwrap();
        b.cues(&cues, &building).unwrap();
        for _ in 0..24 {
            a.advance(1.0 / 30.0, &building).unwrap();
        }
        for _ in 0..144 {
            b.advance(1.0 / 180.0, &building).unwrap();
        }
        let poses =
            |d: &BrickDebris| -> Vec<Mat4> { d.instances().map(|(_, t)| t.transform).collect() };
        let (pa, pb) = (poses(&a), poses(&b));
        assert_eq!(pa.len(), 40);
        for (x, y) in pa.iter().zip(&pb) {
            assert!(x.abs_diff_eq(*y, 1e-4), "{x} vs {y}");
        }
        // Each hops its own way, mostly up.
        let mut sideways = BTreeSet::new();
        for f in &a.falling {
            assert!(f.velocity.y > 0.7 * KILL_SPEED, "{}", f.velocity);
            assert!((f.velocity.length() - KILL_SPEED).abs() < 1e-3);
            sideways.insert((f.velocity.x * 100.0) as i32);
        }
        assert!(sideways.len() > 20);
        // Blasts on top keep their bodies and limit.
        a.set_limit(10);
        a.cues(&blast(20, 1000, 8.0), &building).unwrap();
        assert_eq!((a.len(), a.falling()), (10, 40));
    }

    #[test]
    fn a_blast_pop_rises_tumbles_lands_fades_and_is_removed() {
        let (building, _) = building(&[(7, [0.0, 0.3, 0.0])]);
        let mut debris = BrickDebris::new();
        // Cue arrives before the world update that removes the brick.
        debris
            .cues(
                &[kill(1, 7, [0.0, 0.3, 0.0], [0.0, -0.7, 0.0], 12.0, 0.0)],
                &building,
            )
            .unwrap();
        assert!(debris.is_dead(7));
        let mut peak = 0.0f32;
        for _ in 0..30 {
            debris.advance(1.0 / 60.0, &building).unwrap();
            peak = peak.max(debris.positions()[0].y);
        }
        assert!(peak > 0.9, "popped only to {peak}");
        run(&mut debris, &building, 1.0);
        let rest = debris.positions()[0];
        assert!(
            rest.y > 0.05 && rest.y < 0.7,
            "fell through or floated: {rest}"
        );
        let (_, t) = debris.instances().next().unwrap();
        assert_eq!(t.tint[3], 1.0);
        // A rocket's debris is still fully visible after the old five-second
        // total lifetime, so creators can enjoy the settled destruction.
        run(&mut debris, &building, 4.0);
        assert_eq!(debris.instances().next().unwrap().1.tint[3], 1.0);
        run(&mut debris, &building, SOLID_SECONDS - 4.0);
        let (_, t) = debris.instances().next().unwrap();
        assert!(
            t.tint[3] > 0.0 && t.tint[3] < 1.0,
            "not fading: {}",
            t.tint[3]
        );
        run(&mut debris, &building, FADE_SECONDS);
        assert!(debris.is_empty());
    }

    #[test]
    fn debris_lands_on_neighbouring_bricks_but_not_its_own_spot() {
        // Brick 1 sits on brick 2. Killing brick 2 drops nothing onto its own
        // (still colliding) collider; killing brick 1 lands it on brick 2.
        let (building, _) = building(&[(1, [0.0, 0.9, 0.0]), (2, [0.0, 0.3, 0.0])]);
        let mut debris = BrickDebris::new();
        debris
            .cues(
                &[kill(1, 1, [0.0, 0.9, 0.0], [0.0, 0.9, 0.0], 0.0, 0.0)],
                &building,
            )
            .unwrap();
        run(&mut debris, &building, 1.0);
        let y = debris.positions()[0].y;
        assert!((y - 0.9).abs() < 0.15, "should rest on brick 2: {y}");
        let mut debris = BrickDebris::new();
        debris
            .cues(
                &[kill(1, 2, [0.0, 0.3, 0.0], [0.0, 0.3, 0.0], 0.0, 0.0)],
                &building,
            )
            .unwrap();
        run(&mut debris, &building, 1.0);
        let y = debris.positions()[0].y;
        assert!(y < 0.45, "held up by its own dead brick: {y}");
    }

    #[test]
    fn every_client_throws_the_same_way_and_blasts_push_outward() {
        let (building, _) = building(&[]);
        let cues: Vec<_> = (0..6)
            .map(|i| {
                let x = i as f32 - 2.5;
                kill(10 + i, 100 + i, [x, 0.3, -4.0], [0.0, 0.3, -4.0], 30.0, 3.0)
            })
            .collect();
        let mut a = BrickDebris::new();
        let mut b = BrickDebris::new();
        a.cues(&cues, &building).unwrap();
        b.cues(&cues, &building).unwrap();
        // Different frame pacing on each client.
        for _ in 0..30 {
            a.advance(1.0 / 30.0, &building).unwrap();
        }
        for _ in 0..60 {
            b.advance(1.0 / 60.0, &building).unwrap();
        }
        assert_eq!(a.positions(), b.positions());
        for (cue, p) in cues.iter().zip(a.positions()) {
            let x = cue.position[0];
            assert!(p.x * x.signum() > x.abs(), "{x} not thrown outward: {p}");
        }
        // Duplicates (resent checkpoints) are ignored.
        a.cues(&cues, &building).unwrap();
        assert_eq!(a.len(), 6);
        assert_eq!(a.diagnostics.duplicates, 6);
    }

    fn cosmetic_blast_pack() -> bri_weapons::Pack {
        let mut pack = bri_weapons::testing::pack();
        let projectile = pack
            .projectiles
            .get_mut(bri_weapons::testing::ROCKET_PROJECTILE)
            .unwrap();
        // An arbitrary provider effect, not a weapon-name special case.
        projectile.explosion.effect = "fixtureSurfaceBlast".into();
        projectile.explosion.impulse_radius = 6.0;
        projectile.brick.force = 30.0;
        projectile.brick.radius = 3.0;
        pack
    }

    fn cosmetic_blast(id: u64, at: Vec3, normal: Option<Vec3>) -> Cue {
        Cue {
            id,
            tick: 1,
            position: at.to_array(),
            kind: CueKind::WeaponEffect {
                source: bri_weapons::TargetId::Map(0),
                definition: "fixtureSurfaceBlast".into(),
                node: String::new(),
                seconds: 0.0,
                image: None,
                hand: None,
                direction: normal.map(|n| n.to_array()),
                scale: 1.0,
            },
        }
    }

    #[test]
    fn a_surface_blast_ejects_broken_wall_bricks_away_from_surviving_backing() {
        let bricks: Vec<_> = (0..3)
            .flat_map(|i| {
                let y = 0.3 + i as f32 * 0.6;
                [(i + 1, [0.0, y, 0.0]), (i + 11, [0.0, y, 1.0])]
            })
            .collect();
        let (building, _) = building(&bricks);
        let origin = Vec3::new(0.0, 0.9, -0.5);
        let pack = cosmetic_blast_pack();
        let kills: Vec<_> = (0..3)
            .map(|i| {
                kill(
                    i + 2,
                    i + 1,
                    [0.0, 0.3 + i as f32 * 0.6, 0.0],
                    origin.to_array(),
                    30.0,
                    3.0,
                )
            })
            .collect();
        let mut debris = BrickDebris::new();
        debris.explosion_cue(&cosmetic_blast(1, origin, Some(Vec3::NEG_Z)), &pack);
        debris.cues(&kills, &building).unwrap();
        for body in debris.bodies.values() {
            let velocity = debris.world.bodies[body.handle].linvel();
            assert!(velocity.z < -4.0 && velocity.y > 0.0, "{velocity:?}");
        }
        run(&mut debris, &building, 0.5);
        for position in debris.positions() {
            assert!(position.z < -0.75, "still inside the wall: {position}");
        }
        assert_eq!(debris.len(), 3);
        assert!(
            !debris.is_dead(11),
            "surviving backing remains a world brick"
        );
    }

    #[test]
    fn an_explosion_without_new_kills_wakes_and_moves_existing_debris_once() {
        let (building, _) = building(&[]);
        let pack = cosmetic_blast_pack();
        let mut debris = BrickDebris::new();
        debris
            .cues(&[lying(1, [-1.0, 0.3, 0.0])], &building)
            .unwrap();
        run(&mut debris, &building, 0.5);
        let handle = debris.bodies[&1].handle;
        debris.world.bodies[handle].sleep();
        let cue = cosmetic_blast(2, Vec3::new(0.0, 0.3, 0.0), None);
        debris.explosion_cue(&cue, &pack);
        assert_eq!(debris.cues(&[], &building).unwrap(), 0);
        let velocity = debris.world.bodies[handle].linvel();
        assert!(velocity.x < -10.0 && !debris.world.bodies[handle].is_sleeping());
        // No new body, no repeated kick when a checkpoint resends the cue.
        debris.explosion_cue(&cue, &pack);
        debris.cues(&[], &building).unwrap();
        assert_eq!(debris.world.bodies[handle].linvel(), velocity);
        assert_eq!(debris.len(), 1);
        run(&mut debris, &building, 0.25);
        assert!(debris.positions()[0].x < -2.0, "{:?}", debris.positions());
    }

    #[test]
    fn explosion_queue_is_bounded_and_does_not_double_kick_or_change_authored_throws() {
        let (building, _) = building(&[]);
        let pack = cosmetic_blast_pack();
        let origin = Vec3::new(0.0, 0.3, 0.0);
        let cue = cosmetic_blast(2, origin, Some(Vec3::NEG_Z));
        let mut debris = BrickDebris::new();
        debris
            .cues(&[lying(1, [1.0, 0.3, 0.0])], &building)
            .unwrap();
        debris.explosion_cue(&cue, &pack);
        debris.cues(&[], &building).unwrap();
        let handle = debris.bodies[&1].handle;
        let once = debris.world.bodies[handle].linvel();
        let mut with_kills = BrickDebris::new();
        with_kills
            .cues(&[lying(1, [1.0, 0.3, 0.0])], &building)
            .unwrap();
        with_kills.explosion_cue(&cue, &pack);
        with_kills
            .cues(
                &[kill(3, 3, [2.0, 0.3, 0.0], origin.to_array(), 30.0, 3.0)],
                &building,
            )
            .unwrap();
        let handle = with_kills.bodies[&1].handle;
        assert_eq!(with_kills.world.bodies[handle].linvel(), once);

        // Small authored fakeKill/direct throws retain their exact launch,
        // even if an explosion surface cue happens to share their origin.
        for radius in [0.02, 1.0] {
            let kill = kill(3, 3, [0.0, 0.3, 1.0], origin.to_array(), 20.0, radius);
            let mut authored = BrickDebris::new();
            authored
                .cues(std::slice::from_ref(&kill), &building)
                .unwrap();
            let mut accompanied = BrickDebris::new();
            accompanied.explosion_cue(&cue, &pack);
            accompanied.cues(&[kill], &building).unwrap();
            let a = authored.bodies[&3].handle;
            let b = accompanied.bodies[&3].handle;
            assert_eq!(
                authored.world.bodies[a].linvel(),
                accompanied.world.bodies[b].linvel()
            );
        }
        for id in 10..10 + MAX_BLASTS as u64 + 10 {
            debris.explosion_cue(&cosmetic_blast(id, origin, None), &pack);
        }
        assert_eq!(debris.pending_blasts.len(), MAX_BLASTS);
        debris.cues(&[], &building).unwrap();
        let rb = &debris.world.bodies[debris.bodies[&1].handle];
        assert!(Vec3::from_array(rb.linvel().to_array()).length() <= MAX_SPEED + 1e-4);
        let mut unknown = cosmetic_blast(1000, origin, None);
        if let CueKind::WeaponEffect { definition, .. } = &mut unknown.kind {
            *definition = "unknownVisualEffect".into();
        }
        debris.explosion_cue(&unknown, &pack);
        assert!(debris.pending_blasts.is_empty());
    }

    fn player_at(id: u64, feet: Vec3) -> Pusher {
        Pusher {
            id,
            center: feet + Vec3::Y * 1.2,
            rotation: Quat::IDENTITY,
            half: Vec3::new(0.5, 1.2, 0.5),
        }
    }
    /// A brick left lying (no throw) at `at`.
    fn lying(id: u64, at: [f32; 3]) -> Cue {
        kill(id, id, at, at, 0.0, 0.0)
    }
    fn pusher_center(debris: &BrickDebris, id: u64) -> Vec3 {
        Vec3::from_array(
            debris.world.bodies[debris.pushers.handle(id).unwrap()]
                .translation()
                .to_array(),
        )
    }

    #[test]
    fn a_player_walking_into_debris_shoves_it_and_is_never_moved_by_it() {
        let (building, _) = building(&[]);
        let mut debris = BrickDebris::new();
        debris
            .cues(&[lying(1, [0.0, 0.3, -3.0])], &building)
            .unwrap();
        run(&mut debris, &building, 0.5);
        let rest = debris.positions()[0];
        // v20's run speed, 7 units/s, straight through the brick's spot.
        for frame in 1..=60 {
            let feet = Vec3::new(0.0, 0.0, -7.0 * frame as f32 / 60.0);
            debris.push(&[player_at(9, feet)]);
            debris.advance(1.0 / 60.0, &building).unwrap();
            // One way: the player is exactly where the game put it.
            let center = pusher_center(&debris, 9);
            assert!(
                center.distance(feet + Vec3::Y * 1.2) < 1e-4,
                "debris moved the player to {center}"
            );
        }
        let shoved = debris.positions()[0];
        assert!(
            shoved.z < rest.z - 1.0 && shoved.y > 0.0,
            "not shoved ahead: {rest} -> {shoved}"
        );
    }

    #[test]
    fn a_vehicle_ramming_a_pile_scatters_it() {
        let (building, _) = building(&[]);
        let mut debris = BrickDebris::new();
        let mut cues = Vec::new();
        for layer in 0..3 {
            for x in -1..=1 {
                let id = cues.len() as u64 + 1;
                cues.push(lying(id, [x as f32 * 1.05, 0.3 + layer as f32 * 0.6, -6.0]));
            }
        }
        debris.cues(&cues, &building).unwrap();
        run(&mut debris, &building, 1.0);
        let before = debris.positions();
        let turned = Quat::from_rotation_y(0.2);
        for frame in 0..60 {
            let z = -15.0 * frame as f32 / 60.0;
            debris.push(&[Pusher {
                id: 1 << 63 | 4,
                center: Vec3::new(0.0, 0.8, z),
                rotation: turned,
                half: Vec3::new(1.2, 0.6, 2.0),
            }]);
            debris.advance(1.0 / 60.0, &building).unwrap();
        }
        let moved = debris
            .positions()
            .iter()
            .zip(&before)
            .filter(|(a, b)| a.distance(**b) > 1.0)
            .count();
        assert!(moved >= 7, "only {moved} of 9 scattered");
    }

    #[test]
    fn a_resting_pile_stays_put_with_nobody_near() {
        let (building, _) = building(&[]);
        let mut debris = BrickDebris::new();
        let mut cues = Vec::new();
        for layer in 0..4 {
            for x in 0..3 {
                for z in 0..2 {
                    let id = cues.len() as u64 + 1;
                    cues.push(lying(
                        id,
                        [x as f32 * 1.02, 0.3 + layer as f32 * 0.6, z as f32 * 1.02],
                    ));
                }
            }
        }
        debris.cues(&cues, &building).unwrap();
        run(&mut debris, &building, 0.5);
        let settled = debris.positions();
        // A player far away takes no part.
        debris.push(&[player_at(9, Vec3::new(30.0, 0.0, 0.0))]);
        assert!(debris.pushers.is_empty());
        run(&mut debris, &building, 2.0);
        for (now, then) in debris.positions().iter().zip(&settled) {
            assert!(now.distance(*then) < 0.05, "crept from {then} to {now}");
        }
    }

    #[test]
    fn shots_and_blasts_push_debris_and_fly_on() {
        let (building, _) = building(&[]);
        let mut debris = BrickDebris::new();
        debris
            .cues(
                &[lying(1, [0.0, 0.3, -5.0]), lying(2, [4.0, 0.3, 0.0])],
                &building,
            )
            .unwrap();
        run(&mut debris, &building, 0.5);
        let velocity = Vec3::new(0.0, 0.0, -100.0);
        for frame in 0..10 {
            let shot = Shot {
                id: 7,
                position: Vec3::new(0.0, 0.3, -1.0) + velocity * (frame as f32 / 60.0),
                velocity,
            };
            debris.shots(&[shot]);
            debris.advance(1.0 / 60.0, &building).unwrap();
        }
        assert_eq!(debris.diagnostics.projectile_hits, 1, "once per brick");
        run(&mut debris, &building, 0.5);
        assert!(debris.positions()[0].z < -5.2, "{}", debris.positions()[0]);
        // A later blast shoves debris already lying around it.
        debris
            .cues(
                &[kill(3, 3, [3.0, 0.3, 0.0], [3.0, 0.3, 0.0], 30.0, 4.0)],
                &building,
            )
            .unwrap();
        run(&mut debris, &building, 0.5);
        assert!(debris.positions()[1].x > 5.0, "{}", debris.positions()[1]);
    }

    #[test]
    fn mass_kills_stay_bounded_and_cheap() {
        let (building, _) = building(&[]);
        let cues: Vec<_> = (0..500u64)
            .map(|i| {
                let p = [
                    (i % 20) as f32,
                    0.3 + (i / 100) as f32 * 0.6,
                    -(((i / 20) % 5) as f32),
                ];
                kill(i + 1, i + 1, p, [10.0, 0.0, -2.0], 50.0, 5.0)
            })
            .collect();
        let mut debris = BrickDebris::new();
        debris.set_limit(128);
        debris.cues(&cues, &building).unwrap();
        assert_eq!(debris.len(), 128);
        assert_eq!(debris.diagnostics.evicted, 500 - 128);
        run(&mut debris, &building, 1.0);
        // A crowd around the blast: only the nearest few dozen push.
        let crowd: Vec<_> = (0..64)
            .map(|i| {
                player_at(
                    i + 1,
                    Vec3::new((i % 8) as f32 * 2.0, 0.0, -((i / 8) as f32)),
                )
            })
            .collect();
        for _ in 0..60 {
            debris.push(&crowd);
            debris.advance(1.0 / 60.0, &building).unwrap();
            assert!(debris.pushers.len() <= crate::local_physics::MAX_PUSHERS);
        }
        // A long hitch drops debris time instead of spiralling.
        debris.advance(5.0, &building).unwrap();
        assert!(debris.diagnostics.dropped_steps > 0);
        assert!(debris.positions().iter().all(|p| p.is_finite()));
    }

    fn blast(n: u64, first: u64, radius: f32) -> Vec<Cue> {
        (0..n)
            .map(|i| {
                let p = [
                    (i % 10) as f32 * 1.05,
                    0.3 + (i / 100) as f32 * 0.6,
                    -((i / 10 % 10) as f32),
                ];
                kill(first + i, first + i, p, [4.5, 0.0, -4.5], 40.0, radius)
            })
            .collect()
    }

    #[test]
    fn the_player_picks_the_limit_and_off_leaves_no_debris() {
        let (building, _) = building(&[]);
        let mut debris = BrickDebris::new();
        assert_eq!(debris.limit(), DEFAULT_LIMIT);
        debris.cues(&blast(300, 1, 8.0), &building).unwrap();
        assert_eq!(debris.len(), 300);
        // Lowering the limit mid-blast removes the oldest bodies now.
        debris.set_limit(100);
        assert_eq!(debris.len(), 100);
        assert_eq!(debris.diagnostics.evicted, 200);
        // Off: bricks still die (and stay hidden), with nothing thrown.
        debris.set_limit(0);
        assert!(debris.is_empty());
        let kills = blast(50, 1000, 8.0);
        assert_eq!(debris.cues(&kills, &building).unwrap(), 0);
        assert!(debris.is_empty() && debris.is_dead(1000));
        assert_eq!(debris.diagnostics.skipped, 50);
        // The limit survives a disconnect.
        debris.clear();
        assert_eq!(debris.limit(), 0);
        debris.set_limit(usize::MAX);
        assert_eq!(debris.limit(), MAX_LIMIT);
    }

    #[test]
    fn debris_learns_what_this_pc_pays_for_and_sheds_the_oldest_over_budget() {
        let (building, _) = building(&[]);
        let mut debris = BrickDebris::new();
        debris.set_limit(1024);
        debris.cues(&blast(400, 1, 8.0), &building).unwrap();
        // The frame that threw them pays for the throw: it teaches nothing,
        // and one slow frame alone sheds nothing.
        debris.spent(BUDGET * 5);
        assert_eq!((debris.room(), debris.len()), (1024, 400));
        // 400 moving bodies at twice the budget: this PC pays for 200, and
        // a second slow frame running sheds the oldest beyond that.
        debris.spent(BUDGET * 2);
        assert_eq!(debris.room(), 200);
        assert_eq!(debris.len(), 200);
        assert_eq!(debris.diagnostics.shed, 200);
        // The oldest went, fading: the newest cue is still here.
        assert!(debris.bodies.contains_key(&400) && !debris.bodies.contains_key(&1));
        assert!(debris.ghosts() == 0, "never drawn, so nothing to fade");
        // The next blast keeps only what fits from the start.
        debris.cues(&blast(300, 1000, 8.0), &building).unwrap();
        assert_eq!(debris.len(), 200);
        // Never below the floor, however slow.
        for _ in 0..30 {
            debris.spent(BUDGET * 1000);
        }
        assert_eq!(debris.len(), SHED_FLOOR);
        // Cheap frames teach it the PC has room again, up to the limit.
        for _ in 0..60 {
            debris.spent(BUDGET / 1000);
        }
        assert_eq!(debris.room(), 1024);
        // A disconnect keeps what it learned about this PC.
        debris.spent(BUDGET * 1000);
        let room = debris.room();
        debris.clear();
        assert_eq!(debris.room(), room);
        // Few moving bodies teach nothing.
        debris.set_limit(500);
        debris.cues(&blast(10, 5000, 8.0), &building).unwrap();
        debris.spent(BUDGET / 1000);
        debris.spent(BUDGET / 1000);
        assert_eq!(debris.room(), room);
    }

    #[test]
    fn bodies_removed_early_fade_out_instead_of_popping() {
        let (building, _) = building(&[]);
        let mut debris = BrickDebris::new();
        debris.set_limit(100);
        debris.cues(&blast(100, 1, 8.0), &building).unwrap();
        // Killed and pushed out in the same batch: never drawn, just gone.
        debris.cues(&blast(20, 1000, 8.0), &building).unwrap();
        assert_eq!((debris.len(), debris.ghosts()), (100, 0));
        run(&mut debris, &building, 0.1);
        // Seen bodies over the limit fade where they were heading.
        debris.cues(&blast(30, 2000, 8.0), &building).unwrap();
        assert_eq!((debris.len(), debris.ghosts()), (100, 30));
        assert_eq!(debris.instances().count(), 130);
        debris.advance(GHOST_SECONDS / 2.0, &building).unwrap();
        let ghost = debris.instances().last().unwrap().1.tint[3];
        assert!(ghost > 0.3 && ghost < 0.7, "half faded: {ghost}");
        debris.advance(GHOST_SECONDS, &building).unwrap();
        assert_eq!(debris.ghosts(), 0);
    }

    #[test]
    fn one_blast_shoves_older_debris_once_not_once_per_brick() {
        let (building, _) = building(&[]);
        let mut debris = BrickDebris::new();
        debris
            .cues(&[lying(1, [3.0, 0.3, 0.0])], &building)
            .unwrap();
        run(&mut debris, &building, 0.5);
        // 40 bricks killed by one weak blast next to the lying brick.
        let cues: Vec<_> = (0..40)
            .map(|i| {
                kill(
                    10 + i,
                    10 + i,
                    [-(i as f32), 0.3, 5.0],
                    [2.0, 0.3, 0.0],
                    4.0,
                    4.0,
                )
            })
            .collect();
        debris.cues(&cues, &building).unwrap();
        let rb = &debris.world.bodies[debris.bodies[&1].handle];
        let speed = Vec3::from_array(rb.linvel().to_array()).length();
        // 4 force * 0.5 * falloff 0.75: one shove, not forty.
        assert!(speed < 2.0, "shoved {speed} units/s");
        assert!(speed > 1.0, "not shoved: {speed}");
    }

    #[test]
    fn debris_looks_upload_geometry_only_against_the_brick_palette() {
        // Each new look once built its own scene with every brick texture,
        // mipmapped on the CPU and uploaded on the frame the bricks died: a
        // dozen bricks hitched ~50 ms, a full 128 ~300 ms. Looks now index
        // the shared palette the chunks use, so none carries an image.
        let meshes = crate::world_chunks::tests::meshes();
        let materials = crate::materials::BrickMaterials::in_memory();
        let palette = BrickPalette::new(&materials).unwrap();
        let colors = [[0.9, 0.2, 0.1, 1.0], [0.2, 0.4, 0.8, 0.5]];
        let plain = Look {
            definition: "definition/a".into(),
            color: 0,
            color_effect: 0,
            shape_effect: 0,
            print: None,
        };
        let looks = [
            plain.clone(),
            Look {
                color: 1,
                ..plain.clone()
            },
            Look {
                print: Some(ContentRef::Resolved("print/print_letters_default/a".into())),
                ..plain.clone()
            },
            Look {
                color_effect: 1,
                shape_effect: 1,
                ..plain.clone()
            },
        ];
        for look in &looks {
            let data = look_scene(look, &meshes, &palette, &materials, &colors)
                .unwrap()
                .unwrap();
            assert!(data.images.is_empty(), "{look:?} uploads textures");
            assert_eq!(data.materials, palette.scene.materials);
            // The same geometry the standalone scene drew.
            let mut brick =
                bri_world::Brick::new(ContentRef::Resolved(look.definition.clone()), [0.0; 3], 0);
            brick.color = look.color;
            brick.color_effect = look.color_effect;
            brick.shape_effect = look.shape_effect;
            brick.print = look.print.clone();
            let standalone = crate::world_scene::build_world_scene_materials(
                &PublicWorld {
                    name: "Look".into(),
                    map_id: "look".into(),
                    palette: colors.to_vec(),
                    bricks: bri_world::Bricks::unit(0, brick),
                },
                &meshes,
                1000,
                Some(&materials),
            )
            .unwrap();
            assert!(!standalone.images.is_empty());
            assert_eq!(
                format!("{:?}", data.vertices),
                format!("{:?}", standalone.vertices)
            );
            assert_eq!(data.indices, standalone.indices);
        }
    }
}
