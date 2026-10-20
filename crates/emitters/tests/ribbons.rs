#![allow(clippy::field_reassign_with_default)]
use bri_emitters::beam::{Beam, BeamProps, EndPointBy};
use bri_emitters::emitter::{Ctx, Emitter, OwnerState, TraceHit, World};
use bri_emitters::props::{ParticleEmitterProps, Range, RangeVector};
use bri_emitters::random::Rand;
use bri_emitters::trail::{Trail, TrailProps};
use glam::Vec3;

struct Nothing;
impl World for Nothing {
    fn line_check(&mut self, _: Vec3, _: Vec3, _: bool) -> Option<TraceHit> {
        None
    }
    fn owner(&self) -> OwnerState {
        OwnerState::default()
    }
}

fn base() -> ParticleEmitterProps {
    let mut p = ParticleEmitterProps::default();
    p.lifetime_range = Range::fixed(4.0);
    p.particles_per_second = 0.0;
    p.initial_particles_per_second = 0.0;
    p.automatic_initial_spawning = false;
    p.respawn_dead_particles = false;
    p.use_collision = false;
    p.start_velocity_range = RangeVector::fixed(Vec3::new(100.0, 0.0, 0.0));
    p.start_size_range = RangeVector::fixed(Vec3::splat(5.0));
    p.acceleration = Vec3::ZERO;
    p
}

#[test]
fn a_trail_commits_a_point_every_threshold_and_draws_a_ribbon() {
    let mut rand = Rand::new(5);
    let mut tp = TrailProps::default();
    tp.distance_threshold = 20.0;
    tp.max_points_per_trail = 50;
    let mut trail = Trail::new(base(), tp, 4, &mut rand);
    let mut ev = Vec::new();
    let mut others: Vec<Option<Emitter>> = vec![None];
    let mut w = Nothing;
    let mut owner = OwnerState::default();
    {
        let mut ctx = Ctx {
            owner,
            others: &mut others,
            world: &mut w,
            rand: &mut rand,
            events: &mut ev,
            me: 0,
        };
        trail.spawn_particle(&mut ctx, 0, 0.0, 4, 0, Vec3::ZERO);
        trail.emitter.active_particles = 1;
    }
    for i in 0..10 {
        owner.time_seconds = 0.1 * i as f32;
        let mut ctx = Ctx {
            owner,
            others: &mut others,
            world: &mut w,
            rand: &mut rand,
            events: &mut ev,
            me: 0,
        };
        trail.update_particles(&mut ctx, 0.1);
    }
    // 1 second at 100 u/s with a 20 u threshold: about 5 committed points plus the head.
    let n = trail.info[0].num_points;
    assert!((5..=8).contains(&n), "{n}");
    let batch = trail.render(Vec3::new(0.0, -500.0, 0.0)).expect("ribbon");
    assert_eq!(batch.vertices.len(), 2 * n);
    assert_eq!(batch.indices.len(), 6 * (n - 1));
    // The ribbon is 2 * size wide across the horizontal side vector.
    let width = (batch.vertices[0].position - batch.vertices[1].position).length();
    assert!((width - 10.0).abs() < 1e-3, "{width}");
}

#[test]
fn a_velocity_beam_runs_from_the_particle_along_its_velocity_over_its_life() {
    let mut rand = Rand::new(6);
    let mut bp = BeamProps::default();
    bp.determine_end_point_by = EndPointBy::Velocity;
    bp.high_frequency_points = 6;
    bp.low_frequency_points = 3;
    let mut beam = Beam::new(base(), bp, 2, &mut rand);
    let mut ev = Vec::new();
    let mut others: Vec<Option<Emitter>> = vec![None];
    let mut w = Nothing;
    let mut ctx = Ctx {
        owner: OwnerState::default(),
        others: &mut others,
        world: &mut w,
        rand: &mut rand,
        events: &mut ev,
        me: 0,
    };
    beam.spawn_particle(&mut ctx, 0, 0.0, 4, 0, Vec3::ZERO);
    beam.emitter.active_particles = 1;
    let first = beam.hf_points[0].location;
    let last = beam.hf_points[5].location;
    assert!(first.length() < 1e-4);
    assert!(
        (last - Vec3::new(400.0, 0.0, 0.0)).length() < 1e-2,
        "{last:?}"
    );
    let batch = beam
        .render(&mut ctx, Vec3::new(200.0, -500.0, 0.0), None)
        .expect("ribbon");
    assert_eq!(batch.vertices.len(), 12);
    assert_eq!(batch.indices.len(), 30);
    // Even spacing along the beam with no noise.
    let step = beam.hf_points[1].location.x - first.x;
    assert!((step - 80.0).abs() < 1e-2, "{step}");
}

#[test]
fn a_trail_started_by_an_update_has_its_first_points() {
    let mut rand = Rand::new(22);
    let mut b = base();
    b.particles_per_second = 10.0;
    b.initial_particles_per_second = 10.0;
    let mut trail = Trail::new(b, TrailProps::default(), 16, &mut rand);
    let mut ev = Vec::new();
    let mut others: Vec<Option<Emitter>> = vec![None];
    let mut w = Nothing;
    let owner = OwnerState::default();
    let mut ctx = Ctx {
        owner,
        others: &mut others,
        world: &mut w,
        rand: &mut rand,
        events: &mut ev,
        me: 0,
    };
    trail.update_particles(&mut ctx, 0.5);
    let n = trail.emitter.active_particles;
    assert!(n >= 4, "{n}");
    for i in 0..n {
        assert!(trail.info[i].num_points >= 2, "particle {i}");
    }
    assert!(trail.render(Vec3::new(0.0, -500.0, 0.0)).is_some());
}
