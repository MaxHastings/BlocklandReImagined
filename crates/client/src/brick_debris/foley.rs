//! Small recorded responses to real local events. No simulation or wire state.
use super::*;
use bri_weapons::brick_audio::Policy;
use std::sync::{Arc, Mutex};

const REPORTS: usize = 64;
const HISTORY: usize = 4096;
const BURST: f64 = 3.;
const RATE: f64 = 8.;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Kind {
    Landing,
    Snap,
    Impact,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Source {
    Body(u64),
    Section(u64, u64),
}
#[derive(Clone, Copy)]
pub(super) struct Hit {
    pub source: Source,
    pub kind: Kind,
    pub point: Vec3,
    pub speed: f32,
}
pub struct Playback {
    pub profile: String,
    pub point: [f32; 3],
    pub seed: u64,
    pub voice: crate::audio::Voice,
}
#[derive(Default)]
pub(super) struct Foley {
    pub policy: Option<Arc<Policy>>,
    pub ear: Vec3,
    now: f64,
    tokens: f64,
    pending: Vec<Hit>,
    history: BTreeMap<(Source, Kind), (f64, u64)>,
    cells: BTreeMap<[i32; 3], f64>,
    pub dropped: u64,
}
impl Foley {
    pub fn set_policy(&mut self, policy: Option<&Policy>) {
        *self = Self {
            policy: policy.cloned().map(Arc::new),
            tokens: BURST,
            ..Default::default()
        };
    }
    pub fn begin(&mut self, dt: f32) {
        self.now += f64::from(dt);
        self.tokens = (self.tokens + f64::from(dt) * RATE).min(BURST);
        self.pending.clear();
        self.history.retain(|_, (at, _)| self.now < *at + 2.);
        self.cells.retain(|_, at| self.now < *at + 2.);
    }
    pub fn born(&mut self, handle: RigidBodyHandle, grace: f64) {
        if self.policy.is_some() && self.history.len() < HISTORY {
            self.history.insert(
                (Source::Body(body_key(handle)), Kind::Impact),
                (self.now + grace, 0),
            );
        }
    }
    pub fn report(&mut self, hit: Hit) {
        if self.policy.is_some()
            && self.pending.len() < REPORTS
            && hit.point.is_finite()
            && hit.speed.is_finite()
        {
            self.pending.push(hit);
        }
    }
    pub fn take(&mut self) -> Vec<Playback> {
        let Some(policy) = &self.policy else {
            self.pending.clear();
            return Vec::new();
        };
        // Hero landings precede small contacts. Then prefer nearby strong hits.
        self.pending.sort_by(|a, b| {
            a.kind.cmp(&b.kind).then_with(|| {
                let score = |h: &Hit| h.speed / (1. + h.point.distance_squared(self.ear));
                score(b).total_cmp(&score(a))
            })
        });
        let mut out = Vec::new();
        for hit in self.pending.drain(..) {
            let response = match hit.kind {
                Kind::Landing => &policy.landing,
                Kind::Snap => &policy.snap,
                Kind::Impact => &policy.impact,
            };
            let Some(response) = response else { continue };
            if hit.point.distance_squared(self.ear) > response.radius * response.radius
                || (hit.kind == Kind::Impact && hit.speed < policy.min_speed)
            {
                continue;
            }
            let key = (hit.source, hit.kind);
            if self
                .history
                .get(&key)
                .is_some_and(|(next, _)| self.now < *next)
            {
                continue;
            }
            if !self.history.contains_key(&key) && self.history.len() >= HISTORY {
                self.dropped += 1;
                continue;
            }
            let ordinal = self.history.get(&key).map_or(1, |(_, n)| n + 1);
            self.history
                .insert(key, (self.now + f64::from(policy.cooldown), ordinal));
            let cell = hit.point.to_array().map(|x| (x / 2.).floor() as i32);
            if self.tokens < 1.
                || self
                    .cells
                    .get(&cell)
                    .is_some_and(|at| self.now < *at + 0.10)
            {
                self.dropped += 1;
                continue;
            }
            self.tokens -= 1.;
            self.cells.insert(cell, self.now);
            let identity = match hit.source {
                Source::Body(k) => k,
                Source::Section(a, b) => a.rotate_left(17) ^ b,
            };
            let seed = identity
                ^ ordinal.wrapping_mul(0x9e3779b97f4a7c15)
                ^ (hit.kind as u64).rotate_left(41);
            let unit = crate::audio::sound_choice_unit(seed);
            out.push(Playback {
                profile: response.profile.clone(),
                point: hit.point.to_array(),
                seed,
                voice: crate::audio::Voice {
                    volume: (hit.speed / policy.full_speed).sqrt().clamped(0.25, 1.)
                        * (0.9 + 0.1 * unit),
                    pitch: policy.pitch[0] + (policy.pitch[1] - policy.pitch[0]) * unit,
                    radius: response.radius,
                },
            });
        }
        out
    }
    pub fn collector(&self) -> Option<Contacts> {
        let policy = self.policy.as_ref()?;
        let response = policy.impact.as_ref()?;
        Some(Contacts {
            ear: self.ear,
            radius: response.radius,
            min_speed: policy.min_speed,
            hits: Mutex::new(Vec::new()),
        })
    }
}
pub(super) fn body_key(h: RigidBodyHandle) -> u64 {
    let (index, generation) = h.0.into_raw_parts();
    u64::from(generation) << 32 | u64::from(index)
}
pub(super) fn configure(
    world: &mut PhysicsWorld,
    handle: RigidBodyHandle,
    policy: Option<&Policy>,
) {
    let body = &world.bodies[handle];
    let mass = body.mass();
    let colliders = body.colliders().to_vec();
    for h in colliders {
        let c = &mut world.colliders[h];
        let mut events = c.active_events();
        events.set(
            ActiveEvents::CONTACT_FORCE_EVENTS,
            policy.is_some_and(|p| p.impact.is_some()),
        );
        c.set_active_events(events);
        c.set_contact_force_event_threshold(policy.map_or(f32::MAX, |p| mass * p.min_speed / STEP));
    }
}
/// Rapier only calls this for contacts over an authored mass-scaled threshold.
/// Reports are capped across all fixed steps of one frame. No contact-pair scan.
pub(super) struct Contacts {
    ear: Vec3,
    radius: f32,
    min_speed: f32,
    hits: Mutex<Vec<Hit>>,
}
impl Contacts {
    pub fn take(self) -> Vec<Hit> {
        self.hits.into_inner().unwrap_or_default()
    }
}
impl EventHandler for Contacts {
    fn handle_collision_event(
        &self,
        _: &RigidBodySet,
        _: &ColliderSet,
        _: CollisionEvent,
        _: Option<&ContactPair>,
    ) {
    }
    fn handle_soft_body_tear_event(&self, _: &SoftBodySet, _: &SoftBodyTearEvent) {}
    fn handle_contact_force_event(
        &self,
        dt: Real,
        bodies: &RigidBodySet,
        colliders: &ColliderSet,
        pair: &ContactPair,
        force: Real,
    ) {
        let Ok(mut hits) = self.hits.lock() else {
            return;
        };
        if hits.len() >= REPORTS {
            return;
        }
        let dynamic = [pair.collider1, pair.collider2]
            .map(|c| colliders[c].parent().filter(|h| bodies[*h].is_dynamic()));
        let Some(h) = dynamic.iter().flatten().next() else {
            return;
        };
        let inverse_mass: f32 = dynamic
            .iter()
            .flatten()
            .map(|h| 1. / bodies[*h].mass().max(0.001))
            .sum();
        let speed = force * dt * inverse_mass;
        if !speed.is_finite() || speed < self.min_speed {
            return;
        }
        let Some((manifold, contact)) = pair
            .solver_manifolds()
            .iter()
            .take(4)
            .find_map(|m| m.data.solver_contacts.first().map(|c| (m, c)))
        else {
            return;
        };
        // Rapier stores solver anchors in a body's CoM frame; a dominant
        // world-attached side already stores world coordinates.
        let anchor = if manifold.data.relative_dominance <= 0 {
            manifold.data.rigid_body1.map_or(contact.anchor1, |body| {
                bodies[body].rotation() * contact.anchor1 + bodies[body].center_of_mass()
            })
        } else {
            contact.anchor1
        };
        let point = Vec3::from_array(anchor.to_array());
        if point.distance_squared(self.ear) > self.radius * self.radius {
            return;
        }
        hits.push(Hit {
            source: Source::Body(body_key(*h)),
            kind: Kind::Impact,
            point,
            speed,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn policy() -> Policy {
        bri_weapons::Pack::from_json(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/brick-destruction-audio/assets/weapons.json"
        )))
        .unwrap()
        .effects
        .brick_debris_audio
        .unwrap()
    }
    #[test]
    fn reports_have_one_shared_rate_budget_distance_filter_and_no_backlog() {
        let mut f = Foley::default();
        f.set_policy(Some(&policy()));
        let mut total = 0;
        for frame in 0..60 {
            f.begin(1. / 60.);
            for i in 0..1000 {
                f.report(Hit {
                    source: Source::Body((frame * 1000 + i) as u64),
                    kind: Kind::Impact,
                    point: Vec3::new((i % 5) as f32 * 2.1, 0., (i / 5 % 5) as f32 * 2.1),
                    speed: 8.,
                });
            }
            assert!(f.pending.len() <= REPORTS);
            let sounds = f.take();
            assert!(sounds.len() <= 3);
            assert!(
                sounds
                    .iter()
                    .all(|s| (0.94..=1.06).contains(&s.voice.pitch) && s.voice.volume <= 1.)
            );
            total += sounds.len();
            assert!(f.take().is_empty());
        }
        assert!((5..=11).contains(&total), "{total}");
        f.set_policy(Some(&policy()));
        f.begin(0.);
        f.report(Hit {
            source: Source::Body(999),
            kind: Kind::Impact,
            point: Vec3::splat(1000.),
            speed: 8.,
        });
        f.report(Hit {
            source: Source::Body(998),
            kind: Kind::Impact,
            point: Vec3::ZERO,
            speed: 0.2,
        });
        assert!(f.take().is_empty());
        f.report(Hit {
            source: Source::Body(997),
            kind: Kind::Impact,
            point: Vec3::ZERO,
            speed: 8.,
        });
        f.begin(1. / 60.);
        assert!(f.take().is_empty(), "old reports never replay next frame");
        f.report(Hit {
            source: Source::Body(996),
            kind: Kind::Impact,
            point: Vec3::ZERO,
            speed: 8.,
        });
        f.set_policy(None);
        assert!(f.take().is_empty());
    }
    #[test]
    fn real_impacts_use_world_contact_points_and_sleeping_rubble_stays_quiet() {
        for first in [false, true] {
            let mut world = crate::local_physics::new_world();
            let floor =
                || ColliderBuilder::cuboid(30., 0.5, 30.).translation(Vector::new(12., -0.5, 0.));
            if first {
                world.insert_collider(floor(), None);
            }
            let (h, _) = world.insert(
                RigidBodyBuilder::dynamic()
                    .translation(Vector::new(12., 3., 0.))
                    .rotation(Vector::new(0., 0., 0.3)),
                ColliderBuilder::cuboid(0.25, 0.25, 0.25)
                    .density(5.)
                    .restitution(0.3),
            );
            if !first {
                world.insert_collider(floor(), None);
            }
            let mut f = Foley::default();
            f.set_policy(Some(&policy()));
            f.ear = Vec3::new(12., 1., 2.);
            configure(&mut world, h, f.policy.as_deref());
            let mut heard = 0;
            for frame in 0..300 {
                f.begin(STEP);
                let contacts = f.collector().unwrap();
                world.step_with_events(&(), &contacts);
                for hit in contacts.take() {
                    assert!(
                        (hit.point.x - 12.).abs() < 1. && hit.point.y.abs() < 0.6,
                        "bad world contact {:?}",
                        hit.point
                    );
                    f.report(hit);
                }
                let sounds = f.take();
                if frame > 240 {
                    assert!(sounds.is_empty(), "resting gravity is not another impact");
                }
                heard += sounds.len();
            }
            assert!(heard > 0, "a real drop must produce contact audio");
        }
    }
    #[test]
    fn dense_pile_keeps_sound_work_bounded_and_preserves_physics() {
        fn run(enabled: bool) -> (Vec<[f32; 3]>, usize, u128) {
            let mut world = crate::local_physics::new_world();
            world.insert_collider(
                ColliderBuilder::cuboid(40., 0.5, 40.).translation(Vector::new(0., -0.5, 0.)),
                None,
            );
            let mut f = Foley::default();
            if enabled {
                f.set_policy(Some(&policy()));
            }
            f.ear = Vec3::new(0., 1., 0.);
            let mut handles = Vec::new();
            for i in 0..2048 {
                let (h, _) = world.insert(
                    RigidBodyBuilder::dynamic()
                        .translation(Vector::new(
                            (i % 64) as f32 * 0.42 - 13.2,
                            1.0 + (i % 3) as f32 * 0.1,
                            (i / 64) as f32 * 0.42 - 6.6,
                        ))
                        .ccd_enabled(true),
                    ColliderBuilder::cuboid(0.2, 0.2, 0.2)
                        .density(5.)
                        .restitution(0.3),
                );
                configure(&mut world, h, f.policy.as_deref());
                if enabled {
                    let collider = world.bodies[h].colliders()[0];
                    assert!(
                        world.colliders[collider].contact_force_event_threshold() > 0.,
                        "new bodies need a mass-scaled threshold"
                    );
                }
                handles.push(h);
            }
            let start = std::time::Instant::now();
            let mut sounds = 0;
            for _ in 0..180 {
                f.begin(STEP);
                if let Some(contacts) = f.collector() {
                    world.step_with_events(&(), &contacts);
                    let hits = contacts.take();
                    assert!(hits.len() <= REPORTS);
                    for hit in hits {
                        f.report(hit);
                    }
                } else {
                    world.step();
                }
                let starts = f.take();
                assert!(starts.len() <= BURST as usize);
                sounds += starts.len();
            }
            let micros = start.elapsed().as_micros();
            (
                handles
                    .into_iter()
                    .map(|h| world.bodies[h].translation().to_array())
                    .collect(),
                sounds,
                micros,
            )
        }
        let (without, quiet, baseline) = run(false);
        let (with, sounds, enabled) = run(true);
        assert_eq!(quiet, 0);
        assert!(sounds > 0 && sounds <= 27, "{sounds}");
        assert_eq!(without, with, "audio cannot perturb physics");
        println!(
            "brick_audio_dense_pile: 2048 bodies,180 steps; baseline_us={baseline}; enabled_us={enabled}; starts={sounds}"
        );
    }
}
