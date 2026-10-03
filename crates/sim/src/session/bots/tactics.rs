//! Pure weapon suitability and bounded interception over the host's flight law.
//!
//! No item IDs, world access, commands or clocks live here. The adapter supplies
//! the effective launch parameters, an actual muzzle and dated target motion.
//! A solution does not authorize firing: geometry, allies, permissions, current
//! image state and ammo must be revalidated by the ordinary control executor.
use glam::{DVec3, Vec3};

const HZ: f64 = bri_weapons::TICK_HZ as f64;
const DT: f64 = 1.0 / HZ;
pub const MAX_LIFETIME_TICKS: u32 = 36_000;
pub const MAX_CANDIDATES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    NonFinite,
    OutOfBounds,
    TooManyCandidates,
    DuplicateSlot,
}

/// Effective values at launch. Body scale belongs in speed/inherit, not fall:
/// runtime scales launch velocity, while fall_per_tick depends on the projectile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flight {
    pub speed: f32,
    pub fall_per_tick: f32,
    pub inherit: f32,
    pub lifetime_ticks: u32,
}
impl Flight {
    pub fn validate(self) -> Result<(), Invalid> {
        finite(&[self.speed, self.fall_per_tick, self.inherit])?;
        if !(0.0..=10_000.0).contains(&self.speed)
            || self.speed == 0.0
            || !(0.0..=100_000.0).contains(&self.fall_per_tick)
            || !(0.0..=100_000.0).contains(&self.inherit)
            || self.lifetime_ticks == 0
            || self.lifetime_ticks > MAX_LIFETIME_TICKS
        {
            return Err(Invalid::OutOfBounds);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Intercept {
    pub muzzle: Vec3,
    pub target: Vec3,
    pub target_velocity: Vec3,
    /// Effective shooter velocity before the projectile's inheritance factor.
    /// Include resolved recoil here if the shot inherits it.
    pub shooter_velocity: Vec3,
}
impl Intercept {
    fn validate(self) -> Result<(), Invalid> {
        for vector in [
            self.muzzle,
            self.target,
            self.target_velocity,
            self.shooter_velocity,
        ] {
            if !vector.is_finite() {
                return Err(Invalid::NonFinite);
            }
            if vector.abs().max_element() > 1_000_000.0 {
                return Err(Invalid::OutOfBounds);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    pub direction: Vec3,
    pub launch_velocity: Vec3,
    pub time_seconds: f64,
    pub impact: Vec3,
    /// The 1-based free-flight segment containing the impact.
    pub flight_tick: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Solutions {
    /// Earliest feasible intercept, normally the low arc.
    pub low: Option<Aim>,
    /// Latest distinct feasible intercept, normally the high arc.
    /// Moving targets need not give monotonically ordered elevation angles.
    pub high: Option<Aim>,
    pub examined_segments: u32,
    /// False means work remains, not that the target is unreachable.
    pub complete: bool,
}

/// Resumable, allocation-free search with one quadratic per free-flight tick.
/// Keeping this state makes the caller's per-tick planning budget deterministic.
#[derive(Clone, Debug)]
pub struct InterceptSearch {
    flight: Flight,
    input: Intercept,
    next_tick: u32,
    solutions: Solutions,
}
impl InterceptSearch {
    pub fn new(flight: Flight, input: Intercept) -> Result<Self, Invalid> {
        flight.validate()?;
        input.validate()?;
        let inherited = input.shooter_velocity.as_dvec3() * f64::from(flight.inherit);
        if inherited.length() > 10_000.0 {
            return Err(Invalid::OutOfBounds);
        }
        Ok(Self {
            flight,
            input,
            next_tick: 1,
            solutions: Solutions::default(),
        })
    }

    pub fn result(&self) -> Solutions {
        self.solutions
    }

    /// At most `segment_budget` quadratics; zero does no work. Candidate roots
    /// are checked against their own segment, speed and projectile lifetime.
    pub fn advance(&mut self, segment_budget: u32) -> Solutions {
        let count = segment_budget.min(self.flight.lifetime_ticks + 1 - self.next_tick);
        for _ in 0..count {
            let tick = self.next_tick;
            let n = f64::from(tick);
            let gravity = DVec3::NEG_Y * f64::from(self.flight.fall_per_tick) * HZ;
            let relative = self.input.target.as_dvec3() - self.input.muzzle.as_dvec3();
            let inherited = self.input.shooter_velocity.as_dvec3() * f64::from(self.flight.inherit);
            // On segment n, semi-implicit gravity displacement is
            // g * (n*dt*t - dt²*n*(n-1)/2), including partial ticks.
            let c = relative + gravity * (DT * DT * n * (n - 1.0) * 0.5);
            let d = self.input.target_velocity.as_dvec3() - inherited - gravity * (n * DT);
            let speed = f64::from(self.flight.speed);
            let (roots, all_times) = quadratic(
                d.length_squared() - speed * speed,
                2.0 * c.dot(d),
                c.length_squared(),
            );
            if all_times {
                self.consider(tick, n * DT, c, d);
            } else {
                for time in roots.into_iter().flatten() {
                    self.consider(tick, time, c, d);
                }
            }
            self.next_tick += 1;
            self.solutions.examined_segments += 1;
        }
        self.solutions.complete = self.next_tick > self.flight.lifetime_ticks;
        self.solutions
    }

    fn consider(&mut self, tick: u32, time: f64, c: DVec3, d: DVec3) {
        let lo = f64::from(tick - 1) * DT;
        let hi = f64::from(tick) * DT;
        let tolerance = 1e-9 * hi.max(1.0);
        if !time.is_finite() || time <= 1e-8 || time < lo - tolerance || time > hi + tolerance {
            return;
        }
        let time = time.clamp(lo, hi);
        let relative_launch = (c + d * time) / time;
        let speed = f64::from(self.flight.speed);
        if (relative_launch.length() - speed).abs() > speed.max(1.0) * 1e-7 {
            return;
        }
        let direction = (relative_launch / speed).as_vec3().normalize_or_zero();
        let launch_velocity =
            direction * self.flight.speed + self.input.shooter_velocity * self.flight.inherit;
        let impact =
            (self.input.target.as_dvec3() + self.input.target_velocity.as_dvec3() * time).as_vec3();
        if !direction.is_finite()
            || direction.length_squared() < 0.9
            || !launch_velocity.is_finite()
            || launch_velocity.length() > 10_000.0
            || !impact.is_finite()
        {
            return;
        }
        let aim = Aim {
            direction,
            launch_velocity,
            time_seconds: time,
            impact,
            flight_tick: tick,
        };
        if self
            .solutions
            .low
            .is_none_or(|old| time < old.time_seconds - tolerance)
        {
            if let Some(old) = self.solutions.low
                && self
                    .solutions
                    .high
                    .is_none_or(|high| old.time_seconds > high.time_seconds)
            {
                self.solutions.high = Some(old);
            }
            self.solutions.low = Some(aim);
        } else if self
            .solutions
            .low
            .is_some_and(|low| time > low.time_seconds + tolerance)
            && self
                .solutions
                .high
                .is_none_or(|old| time > old.time_seconds + tolerance)
        {
            self.solutions.high = Some(aim);
        }
    }
}

/// Exact algebraic displacement of `runtime::coast`, with linear travel inside
/// the current tick. Collision, portal crossings and redirects require the live
/// trajectory validator and invalidate this uncollided solution.
pub fn flight_position(
    muzzle: Vec3,
    launch: Vec3,
    fall_per_tick: f32,
    seconds: f64,
) -> Result<DVec3, Invalid> {
    if !muzzle.is_finite()
        || !launch.is_finite()
        || !fall_per_tick.is_finite()
        || !seconds.is_finite()
    {
        return Err(Invalid::NonFinite);
    }
    if seconds < 0.0 || seconds > f64::from(MAX_LIFETIME_TICKS) * DT || fall_per_tick < 0.0 {
        return Err(Invalid::OutOfBounds);
    }
    let n = (seconds * HZ).ceil();
    let gravity = DVec3::NEG_Y * f64::from(fall_per_tick) * HZ;
    Ok(muzzle.as_dvec3()
        + launch.as_dvec3() * seconds
        + gravity * (n * DT * seconds - DT * DT * n * (n - 1.0) * 0.5))
}

/// Stable quadratic roots, including a double root and degenerate linear case.
fn quadratic(a: f64, b: f64, c: f64) -> ([Option<f64>; 2], bool) {
    let scale = a.abs().max(b.abs()).max(c.abs());
    if scale == 0.0 {
        return ([None, None], true);
    }
    let (a, b, c) = (a / scale, b / scale, c / scale);
    let epsilon = 32.0 * f64::EPSILON;
    if a.abs() <= epsilon {
        return (
            if b.abs() <= epsilon {
                [None, None]
            } else {
                [Some(-c / b), None]
            },
            false,
        );
    }
    let discriminant = b * b - 4.0 * a * c;
    let slack = epsilon * (b * b + (4.0 * a * c).abs());
    if discriminant < -slack {
        return ([None, None], false);
    }
    let root = discriminant.max(0.0).sqrt();
    if root == 0.0 {
        return ([Some(-b / (2.0 * a)), None], false);
    }
    let q = -0.5 * (b + root.copysign(b));
    let (x, y) = (q / a, c / q);
    (
        if x <= y {
            [Some(x), Some(y)]
        } else {
            [Some(y), Some(x)]
        },
        false,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Direct,
    Splash,
    Melee,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Delivery {
    Ray,
    Projectile(Flight),
    // Explicit non-projectile contact providers are pure tested; no generic
    // fallback manufactures one for an unknown runtime tool.
    #[allow(dead_code)]
    Contact,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Trigger {
    pub hold: bool,
    pub charge_on_release: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capability {
    pub family: Family,
    pub delivery: Delivery,
    pub trigger: Trigger,
    pub reach: f32,
    pub near: f32,
    pub direct_damage: f32,
    pub splash_damage: f32,
    pub splash_radius: f32,
    /// Arm time limits feasible damaging impact, independently of trigger charge.
    pub arm_ticks: u32,
    pub cadence_ticks: u32,
    pub rounds_per_attack: u32,
}
impl Capability {
    pub fn validate(self) -> Result<(), Invalid> {
        finite(&[
            self.reach,
            self.near,
            self.direct_damage,
            self.splash_damage,
            self.splash_radius,
        ])?;
        if self.reach <= 0.0
            || self.reach > 1_000_000.0
            || self.near < 0.0
            || self.near > self.reach
            || self.direct_damage < 0.0
            || self.splash_damage < 0.0
            || self.splash_radius < 0.0
            || self.direct_damage > 100_000.0
            || self.splash_damage > 100_000.0
            || self.splash_radius > 100_000.0
            || self.cadence_ticks == 0
            || self.cadence_ticks > MAX_LIFETIME_TICKS
            || self.rounds_per_attack == 0
        {
            return Err(Invalid::OutOfBounds);
        }
        if let Delivery::Projectile(flight) = self.delivery {
            flight.validate()?;
            if self.arm_ticks > flight.lifetime_ticks {
                return Err(Invalid::OutOfBounds);
            }
        }
        if self.family == Family::Splash && (self.splash_radius == 0.0 || self.splash_damage == 0.0)
        {
            return Err(Invalid::OutOfBounds);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorRequired {
    ScriptedTool,
    MissingAttack,
    GeometryAdjustedMelee,
    StateDependentLaunch,
    MissingProjectile,
    AlternateHitscanImpact,
    SecondaryEffects,
    NonActorAttack,
    InvalidNativeData,
}
/// Conservative extraction for the ordinary native shot path. The caller must
/// resolve the actual active projectile and launch scale. More complex native
/// The positive cadence must come from the validated current image cycle;
/// min_shot_ticks alone is only a lower bound, never its complete fire rate.
/// More complex native state/volley/lob/recoil and scripted tools require an explicit descriptor;
/// no absent projectile becomes an imaginary melee attack.
pub fn native_capability(
    image: &bri_weapons::Image,
    projectile: Option<&bri_weapons::ProjectileDef>,
    scale: f32,
    cadence_ticks: u32,
) -> Result<Capability, DescriptorRequired> {
    if image.command.is_some() || !image.commands.is_empty() || !image.scripts.is_empty() {
        return Err(DescriptorRequired::ScriptedTool);
    }
    if image.melee {
        return Err(DescriptorRequired::GeometryAdjustedMelee);
    }
    if image.cook.is_some() {
        return Err(DescriptorRequired::SecondaryEffects);
    }
    if image.last_shot.is_some()
        || !image.state_shots.is_empty()
        || !image.volleys.is_empty()
        || image.left_image.is_some()
        || image.shot.as_ref().is_some_and(|s| {
            s.lob.is_some()
                || s.recoil != 0.0
                || s.recoil_vertical.is_some()
                || s.projectiles != 1
                || s.spread != 0.0
                || s.moving_spread.is_some()
                || s.moving_projectile.is_some()
                || s.rested.is_some()
                || s.free
                || s.scale != 1.0
        })
    {
        return Err(DescriptorRequired::StateDependentLaunch);
    }
    let ray = image.shot.as_ref().and_then(|s| s.hitscan.as_ref());
    let Some(p) = projectile else {
        return Err(if image.projectile.is_some() {
            DescriptorRequired::MissingProjectile
        } else {
            DescriptorRequired::MissingAttack
        });
    };
    if !p.children.is_empty() || p.aura.is_some() {
        return Err(DescriptorRequired::SecondaryEffects);
    }
    if !p.collide_players {
        return Err(DescriptorRequired::NonActorAttack);
    }
    if !scale.is_finite()
        || !(0.01..=100.0).contains(&scale)
        || p.sport_image.is_some()
        || cadence_ticks == 0
    {
        return Err(DescriptorRequired::InvalidNativeData);
    }
    if ray.is_some_and(|r| !r.explosion.is_empty() || !r.flown.is_empty() || r.ricochet.is_some()) {
        return Err(DescriptorRequired::AlternateHitscanImpact);
    }
    if ray.is_some_and(|r| {
        r.moving_range.is_some()
            || (!r.from_eye
                && (p.inherit != 0.0 || p.speed <= 0.0 || r.converge || r.eye_within.is_some()))
    }) {
        // Non-eye native rays may use inherited launch velocity or converge
        // from the muzzle, rather than the live look direction we validate.
        // Keep their ordinary legacy executor until a faithful descriptor is
        // available; do not authorize a different ray from the one traced.
        return Err(DescriptorRequired::StateDependentLaunch);
    }
    let using = image.bot.unwrap_or_default();
    let delivery = if ray.is_some() {
        Delivery::Ray
    } else {
        Delivery::Projectile(Flight {
            speed: p.speed * scale,
            fall_per_tick: bri_weapons::runtime::fall_per_tick(p),
            inherit: p.inherit * scale,
            lifetime_ticks: p.lifetime_ticks,
        })
    };
    let reach = using.reach.unwrap_or_else(|| {
        ray.map_or(p.speed * scale * p.lifetime_ticks as f32 / HZ as f32, |r| {
            r.range * scale
        })
    });
    let result = Capability {
        family: if p.explosion.radius > 0.0 && p.explosion.damage > 0.0 {
            Family::Splash
        } else {
            Family::Direct
        },
        delivery,
        trigger: Trigger {
            hold: using.fire == bri_weapons::BotFire::Hold,
            charge_on_release: image.charges(),
        },
        reach,
        near: using.near.unwrap_or(0.0),
        direct_damage: ray
            .and_then(|r| r.damage)
            .unwrap_or(p.damage)
            .clamp(0.0, 100.0)
            * if p.fixed_damage { 1.0 } else { scale },
        splash_damage: p.explosion.damage * scale,
        splash_radius: p.explosion.radius.max(p.explosion.impulse_radius) * scale,
        arm_ticks: if ray.is_some() || !p.ballistic || p.explode_player || p.explosion.damage == 0.0
        {
            0
        } else {
            p.arm_ticks
        },
        cadence_ticks: image.min_shot_ticks.max(cadence_ticks),
        rounds_per_attack: image.magazine.as_ref().map_or(1, |m| m.per_shot),
    };
    if result.direct_damage <= 0.0 && result.splash_damage <= 0.0 {
        return Err(DescriptorRequired::MissingAttack);
    }
    result
        .validate()
        .map_err(|_| DescriptorRequired::InvalidNativeData)?;
    Ok(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Geometry {
    #[allow(dead_code)] // Admission remains useful to pure/external providers.
    Unvalidated,
    Clear,
    #[allow(dead_code)] // Live adapter discards blocked paths before scoring.
    Blocked,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Context {
    pub distance: f32,
    pub target_health: f32,
    pub hit_probability: f32,
    /// Live clearances from predicted impact, including actor bounds and motion.
    pub self_clearance: f32,
    pub ally_clearance: Option<f32>,
    pub blast_margin: f32,
    pub geometry: Geometry,
    pub aim: Option<Aim>,
    /// None means unlimited ammo. Some counts currently fireable rounds only.
    pub ready_rounds: Option<u32>,
    /// The adapter's explicit opportunity cost in the same score units.
    pub opportunity_cost: f32,
    pub switch_seconds: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsuited {
    Invalid,
    Range,
    Ammo,
    #[allow(dead_code)] // Admission remains useful to pure/external providers.
    Unvalidated,
    #[allow(dead_code)] // Live adapter discards blocked paths before scoring.
    Blocked,
    NoIntercept,
    Unarmed,
    UnsafeBlast,
    NoDamage,
}

/// Expected capped damage per occupied attack second minus opportunity cost.
/// Permission, aim error and current image readiness remain live executor gates.
pub fn suitability(weapon: Capability, context: Context) -> Result<f32, Unsuited> {
    weapon.validate().map_err(|_| Unsuited::Invalid)?;
    finite(&[
        context.distance,
        context.target_health,
        context.hit_probability,
        context.self_clearance,
        context.blast_margin,
        context.opportunity_cost,
        context.switch_seconds,
    ])
    .map_err(|_| Unsuited::Invalid)?;
    if context
        .ally_clearance
        .is_some_and(|d| !d.is_finite() || d < 0.0)
        || context.distance < 0.0
        || context.target_health <= 0.0
        || !(0.0..=1.0).contains(&context.hit_probability)
        || context.self_clearance < 0.0
        || context.blast_margin < 0.0
        || context.opportunity_cost < 0.0
        || context.switch_seconds < 0.0
    {
        return Err(Unsuited::Invalid);
    }
    if context.distance < weapon.near || context.distance > weapon.reach {
        return Err(Unsuited::Range);
    }
    if context
        .ready_rounds
        .is_some_and(|n| n < weapon.rounds_per_attack)
    {
        return Err(Unsuited::Ammo);
    }
    match context.geometry {
        Geometry::Unvalidated => return Err(Unsuited::Unvalidated),
        Geometry::Blocked => return Err(Unsuited::Blocked),
        Geometry::Clear => {}
    }
    let flight_seconds = if let Delivery::Projectile(flight) = weapon.delivery {
        let aim = context.aim.ok_or(Unsuited::NoIntercept)?;
        if !aim.time_seconds.is_finite()
            || aim.time_seconds <= 0.0
            || aim.time_seconds > f64::from(flight.lifetime_ticks) * DT
            || !aim.direction.is_finite()
            || (aim.direction.length_squared() - 1.0).abs() > 0.001
            || !aim.launch_velocity.is_finite()
            || !aim.impact.is_finite()
        {
            return Err(Unsuited::NoIntercept);
        }
        if aim.time_seconds < f64::from(weapon.arm_ticks) * DT {
            return Err(Unsuited::Unarmed);
        }
        aim.time_seconds as f32
    } else {
        0.0
    };
    if weapon.splash_radius > 0.0 {
        let safe = weapon.splash_radius + context.blast_margin;
        if context.self_clearance <= safe || context.ally_clearance.is_some_and(|d| d <= safe) {
            return Err(Unsuited::UnsafeBlast);
        }
    }
    let damage = (weapon.direct_damage + weapon.splash_damage).min(context.target_health)
        * context.hit_probability;
    if damage <= 0.0 {
        return Err(Unsuited::NoDamage);
    }
    let seconds = weapon.cadence_ticks as f32 / HZ as f32 + flight_seconds + context.switch_seconds;
    let score = damage / seconds - context.opportunity_cost;
    if !score.is_finite() {
        return Err(Unsuited::Invalid);
    }
    Ok(score)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub slot: u8,
    pub capability: Capability,
    pub context: Context,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selection {
    pub slot: u8,
    pub score: f32,
}
/// Ties keep the lowest slot; a valid current choice wins unless a replacement
/// improves its score by more than `switch_margin`. Invalid current gear never
/// holds a safe usable weapon hostage. No time-based pacing lives here.
pub fn select(
    candidates: &[Candidate],
    current: Option<u8>,
    switch_margin: f32,
) -> Result<Option<Selection>, Invalid> {
    finite(&[switch_margin])?;
    if switch_margin < 0.0 {
        return Err(Invalid::OutOfBounds);
    }
    if candidates.len() > MAX_CANDIDATES {
        return Err(Invalid::TooManyCandidates);
    }
    let mut best: Option<Selection> = None;
    let mut held: Option<Selection> = None;
    let mut slots = [false; 256];
    for candidate in candidates {
        let index = usize::from(candidate.slot);
        if std::mem::replace(&mut slots[index], true) {
            return Err(Invalid::DuplicateSlot);
        }
        let Ok(score) = suitability(candidate.capability, candidate.context) else {
            continue;
        };
        if score <= 0.0 {
            continue;
        }
        let choice = Selection {
            slot: candidate.slot,
            score,
        };
        if current == Some(candidate.slot) {
            held = Some(choice);
        }
        if best.is_none_or(|b| score > b.score || (score == b.score && candidate.slot < b.slot)) {
            best = Some(choice);
        }
    }
    if let (Some(best), Some(held)) = (best, held)
        && best.slot != held.slot
        && best.score <= held.score + switch_margin
    {
        return Ok(Some(held));
    }
    Ok(best)
}
fn finite(values: &[f32]) -> Result<(), Invalid> {
    if values.iter().any(|n| !n.is_finite()) {
        Err(Invalid::NonFinite)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flight(speed: f32, gravity: f32, ticks: u32) -> Flight {
        Flight {
            speed,
            fall_per_tick: gravity / HZ as f32,
            inherit: 0.0,
            lifetime_ticks: ticks,
        }
    }
    fn target(position: Vec3) -> Intercept {
        Intercept {
            muzzle: Vec3::ZERO,
            target: position,
            target_velocity: Vec3::ZERO,
            shooter_velocity: Vec3::ZERO,
        }
    }
    fn solve(f: Flight, input: Intercept) -> Solutions {
        InterceptSearch::new(f, input)
            .unwrap()
            .advance(MAX_LIFETIME_TICKS)
    }
    fn check_endpoint(f: Flight, input: Intercept, aim: Aim) {
        let endpoint = flight_position(
            input.muzzle,
            aim.launch_velocity,
            f.fall_per_tick,
            aim.time_seconds,
        )
        .unwrap();
        let expected =
            input.target.as_dvec3() + input.target_velocity.as_dvec3() * aim.time_seconds;
        assert!(
            (endpoint - expected).length() < 0.002,
            "algebraic endpoint {:?} expected {:?}, aim {:?}",
            endpoint,
            expected,
            aim
        );
        // Exercise the actual host/client free-flight integrator, including a
        // partial final swept segment. No alternate continuous gravity model.
        let mut projectile = bri_weapons::Projectile {
            id: 1,
            definition: "unfamiliar:projectile/arbitrary".into(),
            source: bri_weapons::ActorId(1),
            position: input.muzzle,
            velocity: aim.launch_velocity,
            scale: 1.0,
            age: 0,
            bounced: false,
            stuck: false,
            origin: input.muzzle,
            was_thrown: false,
            paint: None,
            heading: None,
            bounces: 0,
            spawned: 0,
        };
        let complete_ticks = (aim.time_seconds * HZ).floor() as u32;
        for _ in 0..complete_ticks {
            bri_weapons::runtime::coast(&mut projectile, f.fall_per_tick);
        }
        let remaining = (aim.time_seconds - f64::from(complete_ticks) * DT) as f32;
        if remaining > 0.0 {
            projectile.velocity.y -= f.fall_per_tick;
            projectile.position += projectile.velocity * remaining;
        }
        assert!(
            (projectile.position.as_dvec3() - expected).length() < 0.02,
            "runtime endpoint {:?} expected {:?}, aim {:?}",
            projectile.position,
            expected,
            aim
        );
    }

    #[test]
    fn stationary_ballistic_target_has_low_and_high_runtime_correct_arcs() {
        let f = flight(25.0, 9.81, 1_200);
        let input = target(Vec3::new(30.0, 0.0, 0.0));
        let result = solve(f, input);
        let low = result.low.unwrap();
        let high = result.high.unwrap();
        assert!(result.complete && result.examined_segments == 1_200);
        assert!(high.direction.y > low.direction.y && high.time_seconds > low.time_seconds);
        check_endpoint(f, input, low);
        check_endpoint(f, input, high);
    }

    #[test]
    fn moving_target_and_inherited_motion_have_both_verified_arcs() {
        let f = Flight {
            inherit: 0.65,
            ..flight(30.0, 9.81, 1_200)
        };
        let input = Intercept {
            muzzle: Vec3::new(2.0, 1.5, -1.0),
            target: Vec3::new(24.0, 4.0, 8.0),
            target_velocity: Vec3::new(0.8, -0.2, -0.4),
            shooter_velocity: Vec3::new(3.0, 0.5, -1.0),
        };
        let result = solve(f, input);
        check_endpoint(f, input, result.low.unwrap());
        check_endpoint(f, input, result.high.unwrap());
    }

    #[test]
    fn zero_gravity_lead_and_fractional_tick_interception_are_exact() {
        let f = Flight {
            inherit: 0.5,
            ..flight(40.0, 0.0, 480)
        };
        let input = Intercept {
            target_velocity: Vec3::new(2.0, 0.0, 1.0),
            shooter_velocity: Vec3::new(1.0, 0.0, 0.0),
            ..target(Vec3::new(21.7, 2.0, 0.0))
        };
        let result = solve(f, input);
        assert!(result.high.is_none());
        let aim = result.low.unwrap();
        assert!((aim.time_seconds * HZ).fract().abs() > 0.001);
        check_endpoint(f, input, aim);
    }

    #[test]
    fn horizontal_gravity_formula_includes_the_first_tick_drop() {
        let fall = 9.81 / 120.0;
        let position = flight_position(Vec3::ZERO, Vec3::X * 10.0, fall, DT).unwrap();
        assert!((position.y + f64::from(fall) * DT).abs() < 1e-12);
        let f = flight(20.0, 9.81, 480);
        // Pick an actual trajectory's fractional segment endpoint, then solve.
        let t = 0.2531;
        let launch = Vec3::new(15.0, 5.0, 12.247449);
        let point = flight_position(Vec3::ZERO, launch, f.fall_per_tick, t)
            .unwrap()
            .as_vec3();
        let input = target(point);
        let solved = solve(f, input).low.unwrap();
        check_endpoint(f, input, solved);
    }

    #[test]
    fn budgeted_search_resumes_to_the_identical_result() {
        let f = flight(25.0, 9.81, 1_200);
        let input = target(Vec3::new(30.0, 0.0, 0.0));
        let full = solve(f, input);
        let mut search = InterceptSearch::new(f, input).unwrap();
        assert_eq!(search.advance(0), Solutions::default());
        for _ in 0..9 {
            assert!(!search.advance(127).complete);
        }
        assert_eq!(search.advance(127), full);
        assert_eq!(search.advance(u32::MAX), full);
    }

    #[test]
    fn lifetime_and_receding_targets_can_be_unreachable() {
        let f = flight(20.0, 0.0, 60);
        let result = solve(f, target(Vec3::X * 100.0));
        assert!(result.complete && result.low.is_none() && result.high.is_none());
        let input = Intercept {
            target_velocity: Vec3::X * 25.0,
            ..target(Vec3::X * 10.0)
        };
        assert!(solve(flight(20.0, 0.0, 1_200), input).low.is_none());
        assert!(
            solve(flight(10.0, 9.81, 1_200), target(Vec3::Y * 80.0))
                .low
                .is_none()
        );
    }

    #[test]
    fn roots_at_tick_boundaries_are_not_duplicated() {
        let f = flight(120.0, 0.0, 240);
        let result = solve(f, target(Vec3::X * 120.0));
        assert!((result.low.unwrap().time_seconds - 1.0).abs() < 1e-10);
        assert!(result.high.is_none());
        check_endpoint(f, target(Vec3::X * 120.0), result.low.unwrap());
    }

    #[test]
    fn degenerate_linear_and_double_roots_are_stable() {
        assert_eq!(quadratic(0.0, 2.0, -4.0), ([Some(2.0), None], false));
        assert_eq!(quadratic(1.0, -2.0, 1.0), ([Some(1.0), None], false));
        assert_eq!(quadratic(0.0, 0.0, 0.0), ([None, None], true));
        assert_eq!(quadratic(1.0, 0.0, 1.0), ([None, None], false));
        let result = solve(
            flight(20.0, 0.0, 240),
            Intercept {
                target_velocity: Vec3::NEG_X * 20.0,
                ..target(Vec3::X * 20.0)
            },
        );
        assert!((result.low.unwrap().time_seconds - 0.5).abs() < 1e-10);
    }

    #[test]
    fn malformed_and_nonfinite_inputs_never_enter_the_search() {
        let f = flight(20.0, 9.81, 240);
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                InterceptSearch::new(Flight { speed: value, ..f }, target(Vec3::X)).unwrap_err(),
                Invalid::NonFinite
            );
            assert_eq!(
                InterceptSearch::new(f, target(Vec3::new(value, 0.0, 0.0))).unwrap_err(),
                Invalid::NonFinite
            );
        }
        for invalid in [
            Flight { speed: 0.0, ..f },
            Flight {
                fall_per_tick: -1.0,
                ..f
            },
            Flight {
                lifetime_ticks: 0,
                ..f
            },
            Flight {
                lifetime_ticks: MAX_LIFETIME_TICKS + 1,
                ..f
            },
        ] {
            assert!(InterceptSearch::new(invalid, target(Vec3::X)).is_err());
        }
        assert!(flight_position(Vec3::ZERO, Vec3::X, 0.0, f64::NAN).is_err());
        assert!(flight_position(Vec3::ZERO, Vec3::X, 0.0, -1.0).is_err());
        assert!(
            solve(flight(20.0, 0.0, 240), target(Vec3::ZERO))
                .low
                .is_none()
        );
    }

    #[test]
    fn seeded_grid_of_motion_and_gravity_cases_hits_runtime_endpoints() {
        let mut checked = 0;
        for gravity in [0.0, 4.905, 9.81] {
            for lateral in [-1.0, 0.0, 1.0] {
                for height in [-3.0, 0.0, 3.0] {
                    let f = Flight {
                        inherit: 0.4,
                        ..flight(32.0, gravity, 900)
                    };
                    let input = Intercept {
                        target_velocity: Vec3::new(0.5, 0.1, lateral),
                        shooter_velocity: Vec3::new(1.0, 0.0, -1.0),
                        ..target(Vec3::new(25.0, height, 7.0))
                    };
                    let result = solve(f, input);
                    for aim in [result.low, result.high].into_iter().flatten() {
                        check_endpoint(f, input, aim);
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked >= 36);
    }

    fn ray() -> Capability {
        Capability {
            family: Family::Direct,
            delivery: Delivery::Ray,
            trigger: Trigger::default(),
            reach: 100.0,
            near: 0.0,
            direct_damage: 20.0,
            splash_damage: 0.0,
            splash_radius: 0.0,
            arm_ticks: 0,
            cadence_ticks: 12,
            rounds_per_attack: 1,
        }
    }
    fn context() -> Context {
        Context {
            distance: 20.0,
            target_health: 100.0,
            hit_probability: 1.0,
            self_clearance: 20.0,
            ally_clearance: None,
            blast_margin: 1.0,
            geometry: Geometry::Clear,
            aim: None,
            ready_rounds: None,
            opportunity_cost: 0.0,
            switch_seconds: 0.0,
        }
    }
    fn candidate(slot: u8, capability: Capability, context: Context) -> Candidate {
        Candidate {
            slot,
            capability,
            context,
        }
    }

    #[test]
    fn range_ammo_geometry_and_unsafe_blast_are_explicit_rejections() {
        assert_eq!(
            suitability(
                ray(),
                Context {
                    distance: 101.0,
                    ..context()
                }
            ),
            Err(Unsuited::Range)
        );
        assert_eq!(
            suitability(
                ray(),
                Context {
                    ready_rounds: Some(0),
                    ..context()
                }
            ),
            Err(Unsuited::Ammo)
        );
        assert_eq!(
            suitability(
                ray(),
                Context {
                    geometry: Geometry::Unvalidated,
                    ..context()
                }
            ),
            Err(Unsuited::Unvalidated)
        );
        assert_eq!(
            suitability(
                ray(),
                Context {
                    geometry: Geometry::Blocked,
                    ..context()
                }
            ),
            Err(Unsuited::Blocked)
        );
        let splash = Capability {
            family: Family::Splash,
            splash_radius: 8.0,
            splash_damage: 80.0,
            ..ray()
        };
        assert_eq!(
            suitability(
                splash,
                Context {
                    self_clearance: 9.0,
                    ..context()
                }
            ),
            Err(Unsuited::UnsafeBlast)
        );
        assert_eq!(
            suitability(
                splash,
                Context {
                    ally_clearance: Some(3.0),
                    ..context()
                }
            ),
            Err(Unsuited::UnsafeBlast)
        );
        assert!(suitability(splash, context()).is_ok());
    }

    #[test]
    fn all_attack_families_change_choice_with_context() {
        let gun = ray();
        let melee = Capability {
            family: Family::Melee,
            delivery: Delivery::Contact,
            reach: 3.0,
            direct_damage: 30.0,
            cadence_ticks: 120,
            ..ray()
        };
        let splash = Capability {
            family: Family::Splash,
            splash_radius: 8.0,
            splash_damage: 80.0,
            ..ray()
        };
        let near = Context {
            distance: 2.0,
            self_clearance: 2.0,
            ..context()
        };
        let scarce = Context {
            opportunity_cost: 180.0,
            ..near
        };
        assert_eq!(
            select(
                &[
                    candidate(0, gun, scarce),
                    candidate(1, melee, near),
                    candidate(2, splash, near)
                ],
                None,
                0.0
            )
            .unwrap()
            .unwrap()
            .slot,
            1
        );
        assert_eq!(
            select(
                &[
                    candidate(0, gun, context()),
                    candidate(1, melee, context()),
                    candidate(2, splash, context())
                ],
                None,
                0.0
            )
            .unwrap()
            .unwrap()
            .slot,
            2
        );
        let ally_close = Context {
            ally_clearance: Some(2.0),
            ..context()
        };
        assert_eq!(
            select(
                &[
                    candidate(0, gun, ally_close),
                    candidate(1, melee, ally_close),
                    candidate(2, splash, ally_close)
                ],
                None,
                0.0
            )
            .unwrap()
            .unwrap()
            .slot,
            0
        );
    }

    #[test]
    fn projectile_selection_requires_a_valid_live_trajectory_and_arming() {
        let f = flight(30.0, 9.81, 900);
        let input = target(Vec3::X * 20.0);
        let aim = solve(f, input).low.unwrap();
        let weapon = Capability {
            delivery: Delivery::Projectile(f),
            ..ray()
        };
        assert_eq!(suitability(weapon, context()), Err(Unsuited::NoIntercept));
        assert!(
            suitability(
                weapon,
                Context {
                    aim: Some(aim),
                    ..context()
                }
            )
            .is_ok()
        );
        assert_eq!(
            suitability(
                Capability {
                    arm_ticks: 300,
                    ..weapon
                },
                Context {
                    aim: Some(aim),
                    ..context()
                }
            ),
            Err(Unsuited::Unarmed)
        );
        assert_eq!(
            suitability(
                weapon,
                Context {
                    aim: Some(Aim {
                        time_seconds: f64::NAN,
                        ..aim
                    }),
                    ..context()
                }
            ),
            Err(Unsuited::NoIntercept)
        );
        assert_eq!(
            suitability(
                weapon,
                Context {
                    geometry: Geometry::Blocked,
                    aim: Some(aim),
                    ..context()
                }
            ),
            Err(Unsuited::Blocked)
        );
    }

    #[test]
    fn hysteresis_and_stable_ties_stop_inventory_flip_flop() {
        let a = candidate(0, ray(), context());
        let b = candidate(
            1,
            Capability {
                direct_damage: 21.0,
                ..ray()
            },
            context(),
        );
        assert_eq!(select(&[a, b], Some(0), 15.0).unwrap().unwrap().slot, 0);
        assert_eq!(select(&[a, b], Some(0), 5.0).unwrap().unwrap().slot, 1);
        let blocked = candidate(
            0,
            ray(),
            Context {
                geometry: Geometry::Blocked,
                ..context()
            },
        );
        assert_eq!(
            select(&[blocked, b], Some(0), 1_000.0)
                .unwrap()
                .unwrap()
                .slot,
            1
        );
        assert_eq!(
            select(
                &[
                    candidate(4, ray(), context()),
                    candidate(2, ray(), context())
                ],
                None,
                0.0
            )
            .unwrap()
            .unwrap()
            .slot,
            2
        );
    }

    #[test]
    fn candidate_budget_duplicates_and_nan_are_rejected() {
        let a = candidate(0, ray(), context());
        assert_eq!(select(&[a, a], None, 0.0), Err(Invalid::DuplicateSlot));
        assert_eq!(
            select(&[a; MAX_CANDIDATES + 1], None, 0.0),
            Err(Invalid::TooManyCandidates)
        );
        assert_eq!(select(&[a], None, f32::NAN), Err(Invalid::NonFinite));
        assert_eq!(
            suitability(
                ray(),
                Context {
                    hit_probability: f32::NAN,
                    ..context()
                }
            ),
            Err(Unsuited::Invalid)
        );
    }

    fn native_projectile() -> bri_weapons::ProjectileDef {
        bri_weapons::ProjectileDef {
            id: "stranger:projectile/not-a-weapon-name".into(),
            name: "An unfamiliar object".into(),
            speed: 30.0,
            inherit: 0.5,
            ballistic: true,
            gravity: 1.0,
            lifetime_ticks: 900,
            damage: 30.0,
            ..Default::default()
        }
    }
    fn native_image() -> bri_weapons::Image {
        bri_weapons::Image {
            id: "stranger:image/arbitrary".into(),
            name: "Not stock".into(),
            projectile: Some("stranger:projectile/not-a-weapon-name".into()),
            ..Default::default()
        }
    }

    #[test]
    fn metadata_uses_actual_flight_scale_and_trigger_without_ids() {
        let mut image = native_image();
        image.bot = Some(bri_weapons::BotUse {
            fire: bri_weapons::BotFire::Hold,
            reach: None,
            near: None,
            manipulation: None,
        });
        image.states = vec![
            bri_weapons::State {
                up: Some(1),
                ..Default::default()
            },
            bri_weapons::State {
                script: "onFire".into(),
                ..Default::default()
            },
        ];
        let p = native_projectile();
        let cap = native_capability(&image, Some(&p), 2.0, 60).unwrap();
        let Delivery::Projectile(f) = cap.delivery else {
            panic!("projectile descriptor");
        };
        assert_eq!(f.speed, 60.0);
        assert_eq!(f.inherit, 1.0);
        assert_eq!(f.fall_per_tick, bri_weapons::runtime::fall_per_tick(&p));
        assert_eq!(cap.direct_damage, 60.0);
        assert!(cap.trigger.hold && cap.trigger.charge_on_release);
        assert_eq!(cap.cadence_ticks, 60);
        image.id = "totally-different:unnamed".into();
        image.name = "Rocket Sword Gun".into();
        assert_eq!(native_capability(&image, Some(&p), 2.0, 60).unwrap(), cap);
        let fixed = bri_weapons::ProjectileDef {
            fixed_damage: true,
            ..p
        };
        assert_eq!(
            native_capability(&image, Some(&fixed), 2.0, 60)
                .unwrap()
                .direct_damage,
            30.0
        );
    }

    #[test]
    fn absent_projectile_and_unknown_scripts_are_never_invented_melee() {
        let image = bri_weapons::Image::default();
        assert_eq!(
            native_capability(&image, None, 1.0, 60),
            Err(DescriptorRequired::MissingAttack)
        );
        let tool = bri_weapons::Image {
            command: Some("alien:grab-and-move".into()),
            bot: Some(bri_weapons::BotUse {
                reach: Some(30.0),
                ..Default::default()
            }),
            ..image
        };
        assert_eq!(
            native_capability(&tool, None, 1.0, 60),
            Err(DescriptorRequired::ScriptedTool)
        );
        let melee = bri_weapons::Image {
            melee: true,
            ..native_image()
        };
        assert_eq!(
            native_capability(&melee, Some(&native_projectile()), 1.0, 60),
            Err(DescriptorRequired::GeometryAdjustedMelee)
        );
        assert_eq!(
            native_capability(&native_image(), Some(&native_projectile()), 1.0, 0),
            Err(DescriptorRequired::InvalidNativeData)
        );
    }

    #[test]
    fn explosion_scale_impulse_clearance_and_actor_arming_follow_native_data() {
        let image = native_image();
        let p = bri_weapons::ProjectileDef {
            arm_ticks: 120,
            explode_player: true,
            explosion: bri_weapons::Explosion {
                damage: 50.0,
                radius: 5.0,
                impulse_radius: 8.0,
                ..Default::default()
            },
            ..native_projectile()
        };
        let cap = native_capability(&image, Some(&p), 2.0, 60).unwrap();
        assert_eq!(cap.family, Family::Splash);
        assert_eq!(cap.splash_damage, 100.0);
        assert_eq!(cap.splash_radius, 16.0);
        assert_eq!(cap.arm_ticks, 0);
        let p = bri_weapons::ProjectileDef {
            explode_player: false,
            ..p
        };
        assert_eq!(
            native_capability(&image, Some(&p), 1.0, 60)
                .unwrap()
                .arm_ticks,
            120
        );
    }

    #[test]
    fn hitscan_and_complex_native_launch_paths_remain_distinct() {
        let mut image: bri_weapons::Image = serde_json::from_str(
            r#"{"projectile":"unknown:ray","shot":{"hitscan":{"range":150,"from_eye":true}}}"#,
        )
        .unwrap();
        let p = native_projectile();
        let cap = native_capability(&image, Some(&p), 1.0, 60).unwrap();
        assert_eq!(cap.delivery, Delivery::Ray);
        assert_eq!(cap.reach, 150.0);
        image.shot.as_mut().unwrap().recoil = 3.0;
        assert_eq!(
            native_capability(&image, Some(&p), 1.0, 60),
            Err(DescriptorRequired::StateDependentLaunch)
        );
        image.shot.as_mut().unwrap().recoil = 0.0;
        image
            .shot
            .as_mut()
            .unwrap()
            .hitscan
            .as_mut()
            .unwrap()
            .explosion = "another:blast".into();
        assert_eq!(
            native_capability(&image, Some(&p), 1.0, 60),
            Err(DescriptorRequired::AlternateHitscanImpact)
        );
    }

    #[test]
    fn a_moving_shooters_inherited_muzzle_ray_requires_a_descriptor() {
        let mut image: bri_weapons::Image = serde_json::from_str(
            r#"{"projectile":"unfamiliar:ray","shot":{"hitscan":{"range":100}}}"#,
        )
        .unwrap();
        let mut p = native_projectile();
        let look = Vec3::NEG_Z;
        let velocity = look * p.speed + Vec3::X * 10.0 * p.inherit;
        assert!(
            velocity.normalize().distance(look) > 0.1,
            "ordinary moving-shooter launch ray differs from the look we validate"
        );
        assert_eq!(
            native_capability(&image, Some(&p), 1.0, 60),
            Err(DescriptorRequired::StateDependentLaunch)
        );
        image
            .shot
            .as_mut()
            .unwrap()
            .hitscan
            .as_mut()
            .unwrap()
            .from_eye = true;
        assert_eq!(
            native_capability(&image, Some(&p), 1.0, 60)
                .unwrap()
                .delivery,
            Delivery::Ray
        );
        p.collide_players = false;
        assert_eq!(
            native_capability(&image, Some(&p), 1.0, 60),
            Err(DescriptorRequired::NonActorAttack)
        );
        p.collide_players = true;
        p.inherit = 0.0;
        let ray = image.shot.as_mut().unwrap().hitscan.as_mut().unwrap();
        ray.from_eye = false;
        assert_eq!(
            native_capability(&image, Some(&p), 1.0, 60)
                .unwrap()
                .delivery,
            Delivery::Ray
        );
        let straight = image.clone();
        for converge in [false, true] {
            let ray = image.shot.as_mut().unwrap().hitscan.as_mut().unwrap();
            ray.converge = converge;
            ray.eye_within = (!converge).then_some(2.0);
            assert_eq!(
                native_capability(&image, Some(&p), 1.0, 60),
                Err(DescriptorRequired::StateDependentLaunch)
            );
        }
        image = straight;
        image
            .shot
            .as_mut()
            .unwrap()
            .hitscan
            .as_mut()
            .unwrap()
            .moving_range = Some(10.0);
        assert_eq!(
            native_capability(&image, Some(&p), 1.0, 60),
            Err(DescriptorRequired::StateDependentLaunch)
        );
    }
    #[test]
    fn collateral_and_non_actor_native_paths_require_explicit_descriptors() {
        let image = native_image();
        let p = native_projectile();
        let mut cooked = image.clone();
        cooked.cook = Some(
            serde_json::from_value(serde_json::json!({"script":"onpin", "fuse_ticks":120}))
                .unwrap(),
        );
        assert_eq!(
            native_capability(&cooked, Some(&p), 1.0, 60),
            Err(DescriptorRequired::SecondaryEffects)
        );
        let mut child = p.clone();
        child.children.push(
            serde_json::from_value(serde_json::json!({"projectile":"foreign:projectile/orbit"}))
                .unwrap(),
        );
        assert_eq!(
            native_capability(&image, Some(&child), 1.0, 60),
            Err(DescriptorRequired::SecondaryEffects)
        );
        let mut aura = p.clone();
        aura.aura = Some(
            serde_json::from_value(serde_json::json!({"radius":4.0,"damage":5.0,"every_ticks":12}))
                .unwrap(),
        );
        assert_eq!(
            native_capability(&image, Some(&aura), 1.0, 60),
            Err(DescriptorRequired::SecondaryEffects)
        );
        let mut non_actor = p.clone();
        non_actor.collide_players = false;
        assert_eq!(
            native_capability(&image, Some(&non_actor), 1.0, 60),
            Err(DescriptorRequired::NonActorAttack)
        );
        let mut cosmetic = p;
        cosmetic.damage = 0.0;
        cosmetic.explosion.damage = 0.0;
        assert_eq!(
            native_capability(&image, Some(&cosmetic), 1.0, 60),
            Err(DescriptorRequired::MissingAttack)
        );
    }
}
