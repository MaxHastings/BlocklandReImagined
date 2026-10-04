//! Transfer momentum a swept character collision removed to a dynamic body.
//! The caller supplies actual collision evidence and owns permission and
//! attribution. This is an inelastic finite-mass contact, with no restitution
//! or sustained force added to the character motor's available momentum.
use glam::Vec3;
use rapier3d::prelude::{PhysicsWorld, RigidBodyHandle};

pub(crate) fn transfer(
    world: &mut PhysicsWorld,
    target: RigidBodyHandle,
    point: Vec3,
    outward_normal: Vec3,
    mover_velocity: Vec3,
    removed_speed: f32,
    mover_mass: f32,
) -> f32 {
    if !point.is_finite()
        || !outward_normal.is_finite()
        || !mover_velocity.is_finite()
        || !removed_speed.is_finite()
        || removed_speed <= 0.0
        || !mover_mass.is_finite()
        || mover_mass <= 0.0
    {
        return 0.0;
    }
    let Some(normal) = outward_normal.try_normalize() else {
        return 0.0;
    };
    let Some(body) = world.bodies.get_mut(target) else {
        return 0.0;
    };
    if !body.is_dynamic() {
        return 0.0;
    }
    let direction = -normal;
    let closing = (mover_velocity - body.velocity_at_point(point)).dot(direction);
    if !closing.is_finite() || closing <= 0.0 {
        return 0.0;
    }
    let arm = point - body.center_of_mass();
    let angular = arm.cross(direction);
    let props = body.mass_properties();
    let target_inverse = direction.dot(props.effective_inv_mass * direction)
        + angular.dot(props.effective_world_inv_inertia * angular);
    // A dynamic body with every response axis locked cannot take this hit.
    if !target_inverse.is_finite() || target_inverse <= 0.0 {
        return 0.0;
    }
    let denominator = mover_mass.recip() + target_inverse;
    let available = mover_mass * removed_speed;
    let impulse = (closing / denominator).min(available);
    if !denominator.is_finite() || !available.is_finite() || !impulse.is_finite() || impulse <= 0.0
    {
        return 0.0;
    }
    body.apply_impulse_at_point(direction * impulse, point, true);
    impulse
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier3d::prelude::*;

    fn body(mass: f32) -> (PhysicsWorld, RigidBodyHandle) {
        let mut world = bri_physics::new_world();
        world.gravity = Vec3::ZERO;
        let (handle, _) = world.insert(
            RigidBodyBuilder::dynamic(),
            ColliderBuilder::cuboid(1.0, 1.0, 1.0).mass(mass),
        );
        bri_physics::detect_collisions(&mut world);
        (world, handle)
    }

    fn push(world: &mut PhysicsWorld, h: RigidBodyHandle, point: Vec3, speed: f32) -> f32 {
        transfer(world, h, point, Vec3::NEG_X, Vec3::X * speed, speed, 90.0)
    }

    #[test]
    fn central_collision_has_finite_mass_momentum_and_loses_energy() {
        let (mut world, h) = body(90.0);
        let impulse = push(&mut world, h, Vec3::ZERO, 7.0);
        assert!((impulse - 315.0).abs() < 0.001);
        let target_velocity = world.bodies[h].linvel();
        let hypothetical_mover = Vec3::X * (7.0 - impulse / 90.0);
        assert!((90.0 * (target_velocity.x + hypothetical_mover.x) - 630.0).abs() < 0.001);
        let original_energy = 0.5 * 90.0 * 7.0_f32.powi(2);
        let energy =
            world.bodies[h].kinetic_energy() + 0.5 * 90.0 * hypothetical_mover.length_squared();
        assert!(energy <= original_energy);
        // The actual motor stops, dissipating still more of that energy.
        assert!(world.bodies[h].kinetic_energy() < original_energy);
    }

    #[test]
    fn a_heavy_body_accelerates_less() {
        let (mut light, a) = body(90.0);
        let (mut heavy, b) = body(900.0);
        push(&mut light, a, Vec3::ZERO, 7.0);
        push(&mut heavy, b, Vec3::ZERO, 7.0);
        assert!(heavy.bodies[b].linvel().x < light.bodies[a].linvel().x);
        assert!((heavy.bodies[b].linvel().x - 7.0 * 90.0 / 990.0).abs() < 0.001);
    }

    #[test]
    fn zero_or_receding_contact_transfers_nothing() {
        let (mut world, h) = body(90.0);
        assert_eq!(push(&mut world, h, Vec3::ZERO, 0.0), 0.0);
        world.bodies[h].set_linvel(Vec3::X * 8.0, false);
        assert_eq!(push(&mut world, h, Vec3::ZERO, 7.0), 0.0);
        assert_eq!(world.bodies[h].linvel(), Vec3::X * 8.0);
        world.bodies[h].set_linvel(Vec3::ZERO, false);
        assert_eq!(
            transfer(&mut world, h, Vec3::ZERO, Vec3::X, Vec3::X, 1.0, 90.0),
            0.0
        );
    }

    #[test]
    fn only_momentum_removed_by_the_motor_is_available() {
        let (mut world, h) = body(900.0);
        let impulse = transfer(
            &mut world,
            h,
            Vec3::ZERO,
            Vec3::NEG_X,
            Vec3::X * 7.0,
            0.1,
            90.0,
        );
        assert!((impulse - 9.0).abs() < 0.001);
    }

    #[test]
    fn offcentre_contact_spins_with_less_linear_impulse() {
        let (mut centre, a) = body(90.0);
        let (mut offset, b) = body(90.0);
        let central = push(&mut centre, a, Vec3::ZERO, 7.0);
        let eccentric = push(&mut offset, b, Vec3::Y, 7.0);
        assert!(eccentric < central);
        assert!(offset.bodies[b].angvel().z < 0.0);
        assert!(offset.bodies[b].kinetic_energy() <= 0.5 * 90.0 * 7.0_f32.powi(2));
    }

    #[test]
    fn invalid_evidence_does_not_mutate_the_body() {
        let (mut world, h) = body(90.0);
        for invalid in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            assert_eq!(
                transfer(
                    &mut world,
                    h,
                    Vec3::ZERO,
                    Vec3::NEG_X,
                    Vec3::X,
                    invalid,
                    90.0
                ),
                0.0
            );
            assert_eq!(
                transfer(
                    &mut world,
                    h,
                    Vec3::ZERO,
                    Vec3::NEG_X,
                    Vec3::X,
                    1.0,
                    invalid
                ),
                0.0
            );
        }
        for invalid in [Vec3::splat(f32::NAN), Vec3::splat(f32::INFINITY)] {
            assert_eq!(
                transfer(&mut world, h, invalid, Vec3::NEG_X, Vec3::X, 1.0, 90.0),
                0.0
            );
            assert_eq!(
                transfer(&mut world, h, Vec3::ZERO, invalid, Vec3::X, 1.0, 90.0),
                0.0
            );
            assert_eq!(
                transfer(&mut world, h, Vec3::ZERO, Vec3::NEG_X, invalid, 1.0, 90.0),
                0.0
            );
        }
        assert_eq!(
            transfer(&mut world, h, Vec3::ZERO, Vec3::ZERO, Vec3::X, 1.0, 90.0),
            0.0
        );
        assert_eq!(world.bodies[h].linvel(), Vec3::ZERO);
        assert_eq!(world.bodies[h].angvel(), Vec3::ZERO);
    }

    #[test]
    fn fixed_kinematic_and_locked_bodies_cannot_take_momentum() {
        for builder in [
            RigidBodyBuilder::fixed(),
            RigidBodyBuilder::kinematic_position_based(),
            RigidBodyBuilder::dynamic().locked_axes(LockedAxes::all()),
        ] {
            let mut world = bri_physics::new_world();
            let (h, _) = world.insert(builder, ColliderBuilder::cuboid(1.0, 1.0, 1.0).mass(90.0));
            bri_physics::detect_collisions(&mut world);
            assert_eq!(push(&mut world, h, Vec3::ZERO, 7.0), 0.0);
        }
    }
}
