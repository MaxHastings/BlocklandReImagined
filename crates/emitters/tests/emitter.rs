#![allow(clippy::field_reassign_with_default)]
use bri_emitters::emitter::{Ctx, Emitter, Event, OwnerState, TraceHit, World};
use bri_emitters::props::{ParticleEmitterProps, Range, RangeVector};
use bri_emitters::random::Rand;
use glam::Vec3;

struct Floor;
impl World for Floor {
    fn line_check(&mut self, s: Vec3, e: Vec3, _w: bool) -> Option<TraceHit> {
        (s.z > 0.0 && e.z <= 0.0).then(|| {
            let t = s.z / (s.z - e.z);
            TraceHit {
                location: s + (e - s) * t,
                normal: Vec3::Z,
                actor: None,
                material: None,
            }
        })
    }
    fn owner(&self) -> OwnerState {
        OwnerState::default()
    }
}

fn props() -> ParticleEmitterProps {
    let mut p = ParticleEmitterProps::default();
    p.lifetime_range = Range::new(2.0, 2.0);
    p.particles_per_second = 10.0;
    p.initial_particles_per_second = 10.0;
    p.start_velocity_range = RangeVector {
        x: Range::new(0.0, 0.0),
        y: Range::new(0.0, 0.0),
        z: Range::new(-100.0, -100.0),
    };
    p.acceleration = Vec3::ZERO;
    p.use_collision = true;
    p.automatic_initial_spawning = false;
    p.respawn_dead_particles = false;
    p.start_location_offset = Vec3::new(0.0, 0.0, 50.0);
    p.damping_factor_range = RangeVector {
        x: Range::new(1.0, 1.0),
        y: Range::new(1.0, 1.0),
        z: Range::new(1.0, 1.0),
    };
    p
}

fn run(e: &mut Emitter, rand: &mut Rand, dt: f32) -> (usize, Vec<Event>) {
    let mut events = Vec::new();
    let mut others: Vec<Option<Emitter>> = vec![None];
    let mut w = Floor;
    let mut ctx = Ctx {
        owner: OwnerState::default(),
        others: &mut others,
        world: &mut w,
        rand,
        events: &mut events,
        me: 0,
    };
    (e.update_particles(&mut ctx, dt), events)
}

#[test]
fn rate_spawns_the_whole_part_and_carries_the_fraction() {
    let mut rand = Rand::new(1);
    let mut e = Emitter::new(props(), 32, &mut rand);
    let (n, _) = run(&mut e, &mut rand, 0.25);
    assert_eq!(n, 2);
    assert!((e.pps_fraction - 0.5).abs() < 1e-5);
}

#[test]
fn particles_fall_and_bounce_off_the_floor() {
    let mut rand = Rand::new(2);
    let mut e = Emitter::new(props(), 32, &mut rand);
    for _ in 0..40 {
        run(&mut e, &mut rand, 0.05);
    }
    let live: Vec<_> = e.particles.iter().filter(|p| p.flags & 1 != 0).collect();
    assert!(!live.is_empty());
    assert!(live.iter().any(|p| p.hit_count > 0 && p.velocity.z > 0.0));
    assert!(live.iter().all(|p| p.location.z > -1.0));
}

#[test]
fn particles_die_at_the_end_of_their_lifetime() {
    let mut rand = Rand::new(3);
    let mut p = props();
    p.particles_per_second = 0.0;
    p.initial_particles_per_second = 0.0;
    p.use_collision = false;
    let mut e = Emitter::new(p, 8, &mut rand);
    let mut ev = Vec::new();
    let mut others: Vec<Option<Emitter>> = vec![None];
    let mut w = Floor;
    let mut ctx = Ctx {
        owner: OwnerState::default(),
        others: &mut others,
        world: &mut w,
        rand: &mut rand,
        events: &mut ev,
        me: 0,
    };
    e.spawn_particle(&mut ctx, 0, 0.0, 4, 0, Vec3::ZERO);
    e.active_particles = 1;
    for _ in 0..50 {
        run(&mut e, &mut rand, 0.1);
    }
    assert_eq!(e.particles[0].flags & 1, 0);
    assert!(e.all_particles_dead);
}
