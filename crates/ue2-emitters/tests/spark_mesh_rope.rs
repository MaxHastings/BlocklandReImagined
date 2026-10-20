#![allow(clippy::field_reassign_with_default)]
use bri_ue2_emitters::beam::BeamProps;
use bri_ue2_emitters::emitter::{Ctx, Emitter, OwnerState, TraceHit, World};
use bri_ue2_emitters::mesh::{Mesh, MeshProps};
use bri_ue2_emitters::props::{ParticleEmitterProps, Range, RangeVector};
use bri_ue2_emitters::random::Rand;
use bri_ue2_emitters::rope::{Rope, RopeProps};
use bri_ue2_emitters::spark::{Spark, SparkProps};
use glam::Vec3;

struct Open;
impl World for Open {
    fn line_check(&mut self, _: Vec3, _: Vec3, _: bool) -> Option<TraceHit> {
        None
    }
    fn owner(&self) -> OwnerState {
        OwnerState::default()
    }
}

/// A floor at z = 0 the rope can land on.
struct Floor;
impl World for Floor {
    fn line_check(&mut self, s: Vec3, e: Vec3, _: bool) -> Option<TraceHit> {
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

fn base() -> ParticleEmitterProps {
    let mut p = ParticleEmitterProps::default();
    p.lifetime_range = Range::fixed(5.0);
    p.particles_per_second = 0.0;
    p.initial_particles_per_second = 0.0;
    p.automatic_initial_spawning = false;
    p.respawn_dead_particles = false;
    p.use_collision = false;
    p.start_velocity_range = RangeVector::fixed(Vec3::new(10.0, 0.0, 0.0));
    p.start_size_range = RangeVector::fixed(Vec3::splat(2.0));
    p.acceleration = Vec3::new(0.0, 0.0, -4.0);
    p
}

#[test]
fn a_spark_path_has_no_half_on_its_acceleration_term() {
    let mut rand = Rand::new(8);
    let mut sp = SparkProps::default();
    sp.line_segments_range = Range::fixed(3.0);
    sp.time_between_segments_range = Range::fixed(0.5);
    let mut spark = Spark::new(base(), sp, 2, &mut rand);
    let mut ev = Vec::new();
    let mut others: Vec<Option<Emitter>> = vec![None];
    let mut w = Open;
    let mut ctx = Ctx {
        owner: OwnerState::default(),
        others: &mut others,
        world: &mut w,
        rand: &mut rand,
        events: &mut ev,
        me: 0,
    };
    spark.spawn_particle(&mut ctx, 0, 0.0, 4, 0, Vec3::ZERO);
    spark.emitter.active_particles = 1;
    spark.emitter.particles[0].time = 1.0;
    let batch = spark.render(None).expect("lines");
    assert_eq!(batch.vertices.len(), 6);
    // First segment starts at t = 1: x = 10, z = -4 (a * t * t, not half of it).
    let v0 = batch.vertices[0].position;
    assert!((v0 - Vec3::new(10.0, 0.0, -4.0)).length() < 1e-4, "{v0:?}");
    // Its far end is at t = 0.5.
    let v1 = batch.vertices[1].position;
    assert!((v1 - Vec3::new(5.0, 0.0, -1.0)).length() < 1e-4, "{v1:?}");
    assert_eq!(batch.vertices[1].uv, [1.0, 0.0]);
}

#[test]
fn a_mesh_particle_is_placed_by_its_scale_and_location() {
    let mut rand = Rand::new(9);
    let mut mesh = Mesh::new(
        base(),
        MeshProps::default(),
        Some((Vec3::splat(-1.0), Vec3::new(1.0, 1.0, 3.0))),
        2,
        &mut rand,
    );
    assert!((mesh.props.mesh_extent - Vec3::new(1.0, 1.0, 2.0)).length() < 1e-6);
    let mut ev = Vec::new();
    let mut others: Vec<Option<Emitter>> = vec![None];
    let mut w = Open;
    let owner = OwnerState::default();
    let mut ctx = Ctx {
        owner,
        others: &mut others,
        world: &mut w,
        rand: &mut rand,
        events: &mut ev,
        me: 0,
    };
    mesh.spawn_particle(&mut ctx, 0, 0.0, 4, 0, Vec3::new(100.0, 0.0, 0.0));
    mesh.emitter.active_particles = 1;
    let inst = mesh.render(&owner).expect("instances");
    let m = inst.instances[0].local_to_world;
    // The mesh origin lands on the particle's location; the size scales its axes.
    let o = m.transform_point3(Vec3::ZERO);
    assert!((o - mesh.emitter.particles[0].location).length() < 1e-4);
    let x = m.transform_point3(Vec3::X) - o;
    assert!((x.length() - 2.0).abs() < 1e-4);
}

#[test]
fn a_rope_hangs_from_its_owner_at_the_segment_length() {
    let mut rand = Rand::new(10);
    let mut bp = BeamProps::default();
    bp.high_frequency_points = 3;
    bp.low_frequency_points = 2;
    let mut b = base();
    b.use_collision = false;
    b.start_velocity_range = RangeVector::fixed(Vec3::ZERO);
    b.acceleration = Vec3::ZERO;
    let mut rp = RopeProps::default();
    rp.segment_length = 10.0;
    rp.verlet_integration_count = 20;
    let mut rope = Rope::new(b, bp, rp, 6, &mut rand);
    let mut ev = Vec::new();
    let mut others: Vec<Option<Emitter>> = vec![None];
    let mut w = Open;
    let owner = OwnerState {
        location: Vec3::new(0.0, 0.0, 100.0),
        gravity: Some(Vec3::new(0.0, 0.0, -100.0)),
        ..Default::default()
    };
    let mut ctx = Ctx {
        owner,
        others: &mut others,
        world: &mut w,
        rand: &mut rand,
        events: &mut ev,
        me: 0,
    };
    for i in 0..6 {
        rope.spawn_particle(&mut ctx, i, 0.0, 4, Vec3::ZERO);
    }
    rope.beam.emitter.active_particles = 6;
    for _ in 0..200 {
        rope.update_particles(&mut ctx, 0.016);
    }
    let p = &rope.beam.emitter.particles;
    assert!((p[0].location - Vec3::new(0.0, 0.0, 100.0)).length() < 1e-3);
    for i in 1..6 {
        let len = (p[i].location - p[i - 1].location).length();
        assert!((len - 10.0).abs() < 0.5, "segment {i}: {len}");
    }
    // Gravity pulls it straight down from the anchor.
    assert!(p[5].location.z < 60.0, "{:?}", p[5].location);
    // Re-linearised draw points run from the first particle to the last.
    let h = 3;
    assert!((rope.beam.hf_points[0].location - p[0].location).length() < 1e-4);
    assert!((rope.beam.hf_points[5 * h].location - p[5].location).length() < 1e-4);
}

#[test]
fn a_rope_does_not_fall_through_the_floor() {
    let mut rand = Rand::new(11);
    let mut bp = BeamProps::default();
    bp.high_frequency_points = 2;
    bp.low_frequency_points = 2;
    let mut b = base();
    b.use_collision = true;
    b.start_velocity_range = RangeVector::fixed(Vec3::ZERO);
    b.acceleration = Vec3::ZERO;
    let mut rp = RopeProps::default();
    rp.segment_length = 20.0;
    rp.bounciness = 0.2;
    rp.min_vel_threshold = 50.0;
    let mut rope = Rope::new(b, bp, rp, 8, &mut rand);
    let mut ev = Vec::new();
    let mut others: Vec<Option<Emitter>> = vec![None];
    let mut w = Floor;
    let owner = OwnerState {
        location: Vec3::new(0.0, 0.0, 30.0),
        gravity: Some(Vec3::new(0.0, 0.0, -300.0)),
        ..Default::default()
    };
    let mut ctx = Ctx {
        owner,
        others: &mut others,
        world: &mut w,
        rand: &mut rand,
        events: &mut ev,
        me: 0,
    };
    for i in 0..8 {
        rope.spawn_particle(&mut ctx, i, 0.0, 4, Vec3::ZERO);
    }
    rope.beam.emitter.active_particles = 8;
    for _ in 0..300 {
        rope.update_particles(&mut ctx, 0.016);
    }
    assert!(
        rope.beam
            .emitter
            .particles
            .iter()
            .all(|p| p.location.z > -0.5),
        "{:?}",
        rope.beam
            .emitter
            .particles
            .iter()
            .map(|p| p.location.z)
            .collect::<Vec<_>>()
    );
}

#[test]
fn spawns_made_by_an_update_get_their_subclass_data() {
    let mut rand = Rand::new(21);
    let mut b = base();
    b.particles_per_second = 10.0;
    b.initial_particles_per_second = 10.0;
    b.acceleration = Vec3::ZERO;
    let mut spark = Spark::new(b, SparkProps::default(), 16, &mut rand);
    let mut ev = Vec::new();
    let mut others: Vec<Option<Emitter>> = vec![None];
    let mut w = Open;
    let mut ctx = Ctx {
        owner: OwnerState::default(),
        others: &mut others,
        world: &mut w,
        rand: &mut rand,
        events: &mut ev,
        me: 0,
    };
    spark.update_particles(&mut ctx, 0.5);
    let live = spark.emitter.active_particles;
    assert!(live >= 4, "{live}");
    for i in 0..live {
        assert!((spark.data[i].start_velocity.x - 10.0).abs() < 1e-3, "{i}");
        assert!(spark.data[i].time_between_segments > 0.0);
    }
}
