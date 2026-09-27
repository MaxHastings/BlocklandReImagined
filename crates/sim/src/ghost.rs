//! Body-facing stock brick shifts and parity-aware quarter-turn rotation.
//! Pass the player's body forward vector, never the free-look camera direction.
use bri_content::brick::Brick as Mesh;
use bri_world::Brick;
use glam::Vec3;
fn cardinal(forward: Vec3) -> Vec3 {
    if forward.x.abs() > forward.z.abs() {
        Vec3::X * forward.x.signum()
    } else if forward.z != 0.0 {
        Vec3::Z * forward.z.signum()
    } else {
        -Vec3::Z
    }
}
pub fn shift(
    brick: &mut Brick,
    mesh: &Mesh,
    forward: Vec3,
    away: i32,
    left: i32,
    up: i32,
    super_shift: bool,
) {
    let facing = cardinal(forward);
    let leftward = Vec3::Y.cross(facing);
    let mut delta = facing * away as f32 + leftward * left as f32;
    if super_shift {
        let [w, d] = mesh.footprint_studs.map(|n| n as f32);
        let (x, z) = if brick.quarter_turns.is_multiple_of(2) {
            (w, d)
        } else {
            (d, w)
        };
        delta.x *= x;
        delta.z *= z;
    }
    delta *= 0.5;
    delta.y = up as f32
        * 0.2
        * if super_shift {
            mesh.height_plates as f32
        } else {
            1.0
        };
    brick.position = (Vec3::from(brick.position) + delta).to_array();
}
pub fn rotate(brick: &mut Brick, mesh: &Mesh, forward: Vec3, direction: i32) {
    if direction == 0 {
        return;
    }
    let facing = cardinal(forward);
    if mesh.footprint_studs[0] % 2 != mesh.footprint_studs[1] % 2 {
        let step = if brick.quarter_turns.is_multiple_of(2) {
            0.25
        } else {
            -0.25
        };
        // Temporary parity correction. The audit confirmed the recovered v20
        // branches are complete, including an identity case for +X facing.
        // Reconcile engine setTransform snapping with those script offsets before
        // claiming exact v20 rotation anchoring. Four turns restore this center.
        let delta = (facing + Vec3::Y.cross(facing)) * step;
        brick.position = (Vec3::from(brick.position) + delta).to_array();
    }
    brick.quarter_turns = (i32::from(brick.quarter_turns) + direction.signum()).rem_euclid(4) as u8;
}
