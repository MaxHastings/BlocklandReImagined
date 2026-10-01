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
