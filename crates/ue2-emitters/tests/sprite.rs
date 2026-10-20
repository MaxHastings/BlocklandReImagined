//! The sprite vertex fill against the rules read from the original (spec step by step).
use bri_ue2_emitters::particle::{Particle, flags};
use bri_ue2_emitters::props::*;
use bri_ue2_emitters::sprite::{Camera, Owner, SpriteBatch};
use glam::Vec3;

fn camera() -> Camera {
    // Screen right is world +Y, screen up is world +Z, so forward (right x up) is +X.
    Camera {
        position: Vec3::new(-10.0, 0.0, 0.0),
        up: Vec3::Z * 1000.0,
        right: Vec3::Y * 1000.0,
    }
}
fn particle(size: f32) -> Particle {
    Particle {
        flags: flags::ALIVE,
        size: Vec3::splat(size),
        subdivision: -1,
        max_lifetime: 4.0,
        color: Color::WHITE,
        ..Default::default()
    }
}
struct Setup {
    base: ParticleEmitterProps,
    sprite: SpriteExtra,
}
fn props(mode: DirectionUsage) -> Setup {
    let mut base = ParticleEmitterProps::default();
    let mut sprite = SpriteExtra {
        use_direction_as: mode,
        projection_normal: Vec3::Z,
        ..Default::default()
    };
    sprite.initialize(&mut base);
    Setup { base, sprite }
}
fn fill(
    setup: &Setup,
    particles: &[Particle],
    camera: &Camera,
    owner: Option<&Owner>,
) -> SpriteBatch {
    bri_ue2_emitters::sprite::fill(&setup.base, &setup.sprite, particles, camera, owner)
}
fn near(a: Vec3, b: Vec3) {
    assert!((a - b).length() < 1e-3, "{a:?} != {b:?}");
}

#[test]
fn a_plain_sprite_is_a_camera_facing_quad_in_the_original_corner_order() {
    let batch = fill(
        &props(DirectionUsage::None),
        &[particle(2.0)],
        &camera(),
        None,
    );
    let v = &batch.vertices;
    assert_eq!(v.len(), 4);
    // L + Up - Right, L + Up + Right, L - Up + Right, L - Up - Right.
    near(v[0].position, Vec3::new(0.0, -2.0, 2.0));
    near(v[1].position, Vec3::new(0.0, 2.0, 2.0));
    near(v[2].position, Vec3::new(0.0, 2.0, -2.0));
    near(v[3].position, Vec3::new(0.0, -2.0, -2.0));
    assert_eq!(
        [v[0].uv, v[1].uv, v[2].uv, v[3].uv],
        [[0., 0.], [1., 0.], [1., 1.], [0., 1.]]
    );
    assert_eq!(batch.indices(), [0, 1, 2, 0, 2, 3]);
    assert_eq!(v[0].color, 0xffff_ffff);
}

#[test]
fn dead_particles_are_skipped_and_nothing_else_is_tested() {
    let mut dead = particle(1.0);
    dead.flags = 0;
    let mut expired = particle(1.0);
    expired.time = 99.0; // past MaxLifetime but still flagged alive: drawn
    let batch = fill(
        &props(DirectionUsage::None),
        &[dead, expired],
        &camera(),
        None,
    );
    assert_eq!(batch.vertices.len(), 4);
}

#[test]
fn an_explicit_subdivision_picks_a_cell_column_major() {
    let mut e = props(DirectionUsage::None);
    e.base.texture_u_subdivisions = 2;
    e.base.texture_v_subdivisions = 2;
    let mut p = particle(1.0);
    p.subdivision = 3; // q = 3 / 2 = 1, r = 3 % 2 = 1
    let v = fill(&e, &[p.clone()], &camera(), None).vertices;
    assert_eq!(v[0].uv, [0.5, 0.5]);
    assert_eq!(v[2].uv, [1.0, 1.0]);
    p.subdivision = 1; // q = 0, r = 1
    let v = fill(&e, &[p], &camera(), None).vertices;
    assert_eq!(v[0].uv, [0.0, 0.5]);
}

#[test]
fn blending_between_subdivisions_spends_the_vertex_alpha_on_the_fraction() {
    let mut e = props(DirectionUsage::None);
    e.base.texture_u_subdivisions = 2;
    e.base.texture_v_subdivisions = 2;
    e.base.blend_between_subdivisions = true;
    let mut p = particle(1.0);
    p.time = 2.4; // ratio 0.6 of 4 s: position 4 x 0.6 = 2.4, frame 2, fraction 0.4
    let batch = fill(&e, &[p], &camera(), None);
    let v = &batch.vertices;
    assert_eq!(v[0].color >> 24, 102); // int(0.4 * 255)
    assert_eq!(v[0].uv, [0.5, 0.0]); // frame 2: column 1, row 0
    assert_eq!(v[0].uv2, [0.5, 0.5]); // frame 3: column 1, row 1
    assert!(batch.draw.blend_subdivisions);
}

#[test]
fn spin_turns_both_half_axes_about_the_camera_forward() {
    let mut e = props(DirectionUsage::None);
    e.base.spin_particles = true;
    let mut p = particle(2.0);
    p.start_spin = Vec3::new(16384.0, 0.0, 0.0); // a quarter turn
    let v = fill(&e, &[p], &camera(), None).vertices;
    // Up (0,0,2) goes to (0,-2,0), Right (0,2,0) to (0,0,2): V0 = L + A - B.
    near(v[0].position, Vec3::new(0.0, -2.0, -2.0));
    near(v[1].position, Vec3::new(0.0, -2.0, 2.0));
}

#[test]
fn a_forward_sprite_lies_along_its_motion() {
    let e = props(DirectionUsage::Forward);
    let mut p = particle(1.0);
    p.size = Vec3::new(3.0, 5.0, 0.0);
    p.old_location = Vec3::new(-1.0, 0.0, 0.0); // moved +X by 1
    let v = fill(&e, &[p], &camera(), None).vertices;
    // N1 = +X, Q = +Y (|N1.x| > 0.57), A' = X x Y = +Z, A = 5 Z, B = 3 x normalize(X x Z) = 3 (-Y).
    near(v[0].position, Vec3::new(0.0, 3.0, 5.0));
    near(v[1].position, Vec3::new(0.0, -3.0, 5.0));
}

#[test]
fn a_still_velocity_sprite_collapses() {
    let e = props(DirectionUsage::Up);
    let v = fill(&e, &[particle(1.0)], &camera(), None).vertices;
    for vertex in &v {
        near(vertex.position, Vec3::ZERO);
    }
}

#[test]
fn draw_state_follows_the_emitter() {
    let mut e = props(DirectionUsage::None);
    e.base.draw_style = DrawStyle::Translucent;
    e.base.z_write = true;
    let batch = fill(&e, &[particle(1.0)], &camera(), None);
    assert_eq!(batch.draw.state, DrawStyle::Translucent.draw_state());
    assert!(batch.draw.z_write && batch.draw.two_sided);
}

#[test]
fn final_blend_table_matches_the_switch() {
    use bri_ue2_emitters::draw::{BlendFactor::*, final_blend_state};
    assert_eq!(final_blend_state(4).blend, Some((One, InvSrcColor)));
    assert_eq!(final_blend_state(6).blend, Some((SrcAlpha, One)));
    assert_eq!(final_blend_state(4).blend, final_blend_state(9).blend);
    assert_eq!(final_blend_state(1).fog_color, Some([127.0 / 255.0; 3]));
    assert_eq!(final_blend_state(10).blend, Some((One, One)));
    assert_eq!(final_blend_state(0).blend, None);
}
