//! Rust physics adapter. Gameplay identities stay independent of physics handles.

use rapier3d::prelude::*;
pub mod content;
pub mod terrain;

pub const FIXED_DT: f32 = 1.0 / 120.0;

/// A fixed-rate world; the caller owns elapsed-time accumulation and tick order.
pub fn new_world() -> PhysicsWorld {
    let mut world = PhysicsWorld::new();
    world.integration_parameters.dt = FIXED_DT;
    world
}

/// Time a [`detect_collisions`] refresh advances the world: far too short
/// for anything to move, long enough for Rapier's sleep bookkeeping, which
/// breaks on a zero-length step.
const REFRESH_DT: f32 = 1e-6;

/// Refresh contacts and queries between physics steps (after inserting,
/// moving or removing colliders or bodies), so queries see the change now.
///
/// This runs Rapier's full pipeline for a microsecond, not its
/// collision-only pipeline (`PhysicsWorld::detect_collisions`), which does
/// not keep islands in step with contacts: a body inserted since the last
/// step lost its island registration, and contacts starting or stopping in
/// that pass were linked into the wrong island or not at all. Rapier's
/// consistency check then panicked on the next step ("touching pair not
/// linked in the persistent islands": a vehicle spawned where a build was
/// then loaded), and release builds kept going with islands that no longer
/// matched their contacts. Kinematic bodies keep their pending targets for
/// the real step instead of reaching them here. Call this, never
/// `PhysicsWorld::detect_collisions`.
pub fn detect_collisions(world: &mut PhysicsWorld) {
    let targets: Vec<_> = world
        .bodies
        .iter()
        .filter(|(_, body)| body.is_kinematic())
        .map(|(handle, body)| (handle, *body.next_position()))
        .collect();
    for (handle, _) in &targets {
        let body = &mut world.bodies[*handle];
        let here = *body.position();
        body.set_next_kinematic_position(here);
    }
    // The step also integrates dynamic bodies: gravity, damping, gyroscopic
    // terms and the contact solver's penetration recovery nudge their poses
    // and velocities. A refresh must not simulate, or a checkpoint restore
    // would no longer continue the run it saved, so they are put back.
    let dynamic: Vec<_> = world
        .bodies
        .iter()
        .filter(|(_, body)| body.is_dynamic())
        .map(|(handle, body)| (handle, *body.position(), body.linvel(), body.angvel()))
        .collect();
    let dt = world.integration_parameters.dt;
    world.integration_parameters.dt = REFRESH_DT;
    world.step();
    world.integration_parameters.dt = dt;
    for (handle, target) in targets {
        world.bodies[handle].set_next_kinematic_position(target);
    }
    for (handle, position, linvel, angvel) in dynamic {
        let body = &mut world.bodies[handle];
        body.set_position(position, false);
        body.set_linvel(linvel, false);
        body.set_angvel(angvel, false);
    }
}

pub mod preflight {
    use super::*;
    use anyhow::{Result, ensure};
    use rapier3d::control::{
        DynamicRayCastVehicleController, KinematicCharacterController, WheelTuning,
    };

    fn floor(world: &mut PhysicsWorld) {
        world.insert(
            RigidBodyBuilder::fixed().translation(Vector::new(0.0, -0.5, 0.0)),
            ColliderBuilder::cuboid(100.0, 0.5, 100.0),
        );
    }

    pub fn drop_and_rest() -> Result<[f32; 3]> {
        let mut world = new_world();
        floor(&mut world);
        let (body, _) = world.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(0.0, 5.0, 0.0))
                .ccd_enabled(true),
            ColliderBuilder::cuboid(0.5, 0.5, 0.5).restitution(0.0),
        );
        for _ in 0..600 {
            world.step();
        }
        let position = world.bodies[body].translation();
        ensure!(
            (position.y - 0.5).abs() < 0.02,
            "Box failed to settle: {position:?}"
        );
        ensure!(
            world.bodies[body].linvel().length() < 0.01,
            "Box did not come to rest"
        );
        Ok(position.to_array())
    }

    pub fn character_wall() -> Result<[f32; 3]> {
        let mut world = new_world();
        floor(&mut world);
        world.insert(
            RigidBodyBuilder::fixed().translation(Vector::new(2.0, 2.0, 0.0)),
            ColliderBuilder::cuboid(0.1, 2.0, 5.0),
        );
        detect_collisions(&mut world);
        let controller = KinematicCharacterController::default();
        let shape = Capsule::new_y(0.5, 0.3);
        let mut position = Pose::translation(0.0, 0.85, 0.0);
        let mut collision_count = 0;
        for _ in 0..120 {
            let motion = controller.move_shape(
                FIXED_DT,
                &world.query_pipeline(),
                &shape,
                &position,
                Vector::new(4.0 * FIXED_DT, -0.1, 0.0),
                |_| collision_count += 1,
            );
            position.translation += motion.translation;
        }
        ensure!(
            position.translation.x > 1.4 && position.translation.x < 1.61,
            "Character did not stop against wall: {:?}",
            position.translation
        );
        ensure!(
            position.translation.y > 0.79 && position.translation.y < 0.9,
            "Character fell through floor: {:?}",
            position.translation
        );
        ensure!(collision_count > 0, "No character collision callbacks");
        Ok(position.translation.to_array())
    }

    pub fn vehicle_suspension_and_drive() -> Result<[f32; 3]> {
        let mut world = new_world();
        floor(&mut world);
        let (chassis, _) = world.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(0.0, 1.0, 0.0))
                .ccd_enabled(true),
            ColliderBuilder::cuboid(0.8, 0.25, 1.3).mass(600.0),
        );
        let mut vehicle = DynamicRayCastVehicleController::new(chassis);
        vehicle.index_forward_axis = 2;
        for x in [-0.8, 0.8] {
            for z in [-0.95, 0.95] {
                vehicle.add_wheel(
                    Vector::new(x, 0.0, z),
                    -Vector::Y,
                    -Vector::X,
                    0.45,
                    0.3,
                    &WheelTuning::default(),
                );
            }
        }
        detect_collisions(&mut world);
        for tick in 0..600 {
            for wheel in vehicle.wheels_mut() {
                wheel.engine_force = if tick > 120 { 800.0 } else { 0.0 };
            }
            let queries = world.broad_phase.as_query_pipeline_mut(
                world.narrow_phase.query_dispatcher(),
                &mut world.bodies,
                &mut world.colliders,
                QueryFilter::default().exclude_rigid_body(chassis),
            );
            vehicle.update_vehicle(FIXED_DT, queries);
            world.step();
        }
        let position = world.bodies[chassis].translation();
        ensure!(
            position.is_finite() && position.y > 0.3 && position.y < 1.2,
            "Vehicle suspension failed: {position:?}"
        );
        ensure!(
            position.z.abs() > 2.0,
            "Vehicle did not drive: {position:?}"
        );
        Ok(position.to_array())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn collision_and_repeatability() {
            assert_eq!(drop_and_rest().unwrap(), drop_and_rest().unwrap());
        }
        #[test]
        fn character_stops_at_wall() {
            character_wall().unwrap();
        }
        #[test]
        fn vehicle_drives_on_suspension() {
            vehicle_suspension_and_drive().unwrap();
        }
    }
}
