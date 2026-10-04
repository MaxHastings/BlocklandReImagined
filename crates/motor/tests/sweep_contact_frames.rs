//! Swept contact evidence must use the contacted collider's world frame,
//! including a body seen through a linked opening before the actor crosses.
use bri_content::passage::{Passage, Passages};
use bri_motor::torque::{self, Box3, Epsilon, Mover, Soup};
use glam::{Affine3A, Quat, Vec2, Vec3};
use rapier3d::prelude::*;

fn openings() -> Passages {
    Passages {
        list: vec![Passage {
            brick: 1,
            centre: Vec3::new(0.0, 1.0, 0.0),
            normal: Vec3::Z,
            u: Vec3::X,
            v: Vec3::Y,
            half: Vec2::splat(3.0),
            carry: Affine3A::from_rotation_translation(
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                Vec3::new(20.0, 0.0, 10.0),
            ),
        }],
        closed: vec![],
    }
}

fn sweep(world: &PhysicsWorld, openings: &Passages, feet: Vec3, velocity: Vec3) -> torque::Moved {
    let query = world.query_pipeline_with_filter(QueryFilter::default().exclude_sensors());
    let region = Box3 {
        min: Vec3::new(-2.0, -1.0, -2.0),
        max: Vec3::new(2.0, 3.0, 2.0),
    };
    let mut soup = Soup::gather(&query, &world.bodies, region, feet, &());
    soup.open_passages(&query, &world.bodies, openings, feet + Vec3::Y, region, &());
    let mut velocity = velocity;
    torque::update_pos(
        &soup,
        &Mover {
            half_width: 0.4,
            height: 2.0,
            run_cos: 0.7,
            jump_cos: 0.7,
            max_step: 0.0,
            step_reach: 0.0,
            elasticity: torque::NORMAL_ELASTICITY,
            back_off: torque::BACK_OFF,
            epsilon: Epsilon::at(1.0),
        },
        feet,
        &mut velocity,
        0.1,
    )
}

#[test]
fn far_contact_uses_the_body_frame_before_the_actor_crosses() {
    let openings = openings();
    let carry = openings.list[0].carry;
    let far_centre = carry.transform_point3(Vec3::new(0.0, 1.0, -0.35));
    let mut world = bri_physics::new_world();
    world.gravity = Vec3::ZERO;
    let (_, collider) = world.insert(
        RigidBodyBuilder::dynamic().translation(far_centre),
        // A quarter turn maps the source box's thin Z axis onto world X.
        ColliderBuilder::cuboid(0.1, 1.0, 0.8).mass(90.0),
    );
    bri_physics::detect_collisions(&mut world);
    let feet = Vec3::new(0.0, 0.0, 0.4);
    let velocity = Vec3::NEG_Z * 7.0;
    let moved = sweep(&world, &openings, feet, velocity);
    assert!(
        moved.feet.z > 0.0,
        "the entrance stopped the actor's center"
    );
    assert!(
        openings
            .travel(feet + Vec3::Y, moved.feet + Vec3::Y)
            .1
            .is_none(),
        "contact precedes the portal crossing"
    );
    let contact = moved
        .contacts
        .iter()
        .find(|c| c.collider == collider)
        .expect("the actual far dynamic body blocked the sweep");
    assert!((contact.normal - carry.transform_vector3(Vec3::Z)).length() < 1e-5);
    assert!((contact.velocity - carry.transform_vector3(velocity)).length() < 1e-5);
    assert!((contact.removed_speed - 7.0).abs() < 1e-5);
    let patch_centre = carry.transform_point3(Vec3::new(0.0, 1.0, -0.25));
    assert!(
        (contact.point - patch_centre).length() < 1e-4,
        "a centered flat contact has no invented lever arm: {:?}, expected {patch_centre:?}",
        contact.point
    );
}

#[test]
fn ordinary_contact_keeps_its_source_frame_with_an_open_portal() {
    let openings = openings();
    let mut world = bri_physics::new_world();
    world.gravity = Vec3::ZERO;
    let (_, collider) = world.insert(
        RigidBodyBuilder::dynamic().translation(Vec3::new(0.0, 1.0, 0.75)),
        ColliderBuilder::cuboid(0.8, 1.0, 0.1).mass(90.0),
    );
    bri_physics::detect_collisions(&mut world);
    let velocity = Vec3::Z * 7.0;
    let moved = sweep(&world, &openings, Vec3::new(0.0, 0.0, 0.1), velocity);
    let contact = moved
        .contacts
        .iter()
        .find(|c| c.collider == collider)
        .expect("the actual source dynamic body blocked the sweep");
    assert!((contact.normal - Vec3::NEG_Z).length() < 1e-5);
    assert!((contact.velocity - velocity).length() < 1e-5);
    assert!((contact.removed_speed - 7.0).abs() < 1e-5);
    let patch_centre = Vec3::new(0.0, 1.0, 0.65);
    assert!(
        (contact.point - patch_centre).length() < 1e-4,
        "a centered flat contact has no invented lever arm: {:?}, expected {patch_centre:?}",
        contact.point
    );
}
