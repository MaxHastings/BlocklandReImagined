//! What client code asks of the game's physics and posing, and what comes
//! back: the `physics.local` and `avatar.pose` capabilities.
//!
//! Both are presentation only. Bodies an Add-On creates exist on this
//! player's PC alone: they fall, tumble and collide with the world as this
//! client draws it, and what this client draws (players, vehicles, shots)
//! pushes them, one way only; nothing about them reaches the server or any
//! gameplay. A pose changes how this client draws a player's body, never
//! where the player is or what they can do.
//!
//! The sandbox only records requests ([`PhysicsCommand`], [`PlayerPose`])
//! and reads back what the game reports ([`BodyState`]); the game runs the
//! physics between frames, so a body created in one frame is first
//! reported the next.

/// Floats one `rigid_create` record holds.
pub const BODY_RECORD: usize = 28;
/// Floats one `rigid_joint` record holds.
pub const JOINT_RECORD: usize = 12;
/// Floats `rigid_get` writes.
pub const STATE_RECORD: usize = 16;
/// Floats one `pose` node record holds.
pub const POSE_RECORD: usize = 8;
/// Floats one `skeleton` node record holds.
pub const SKELETON_RECORD: usize = 16;
/// Most nodes one skeleton reports or one pose moves.
pub const MAX_NODES: usize = 256;

/// Largest half extent or radius of a body's shape, in world units.
pub const MAX_SIZE: f32 = 32.0;
/// Smallest one: thinner shapes tunnel through the world.
pub const MIN_SIZE: f32 = 0.01;
/// Fastest a body may be thrown or pushed (units/s), and spun (rad/s).
pub const MAX_SPEED: f32 = 200.0;
pub const MAX_SPIN: f32 = 100.0;
/// Farthest from the world's origin a body may start.
pub const MAX_COORDINATE: f32 = 1.0e6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// Half extents x, y, z.
    Box([f32; 3]),
    Ball(f32),
    /// Standing on the body's y axis: radius, then half the height of its
    /// straight middle.
    Capsule(f32, f32),
}

/// A body to create (`rigid_create`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodySpec {
    pub shape: Shape,
    /// Where the shape's centre sits in the body's own frame.
    pub offset: [f32; 3],
    pub position: [f32; 3],
    /// Unit quaternion, x y z w.
    pub rotation: [f32; 4],
    pub velocity: [f32; 3],
    pub spin: [f32; 3],
    /// Mass per cubic unit.
    pub density: f32,
    pub friction: f32,
    pub bounce: f32,
    /// Bodies of the same nonzero group never touch each other (a
    /// ragdoll's limbs); 0 touches everything.
    pub group: u32,
    pub linear_damping: f32,
    pub angular_damping: f32,
}

/// A ball-and-socket joint between two bodies (`rigid_joint`), placed in
/// the world when it is made. The two bodies stop touching each other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JointSpec {
    /// Where the bodies are pinned together.
    pub anchor: [f32; 3],
    /// Unit axis the second body twists about.
    pub axis: [f32; 3],
    /// How far it may swing away from the axis, and twist about it, from
    /// where it was when joined (radians, 0 to pi).
    pub swing: f32,
    pub twist: f32,
    /// How stiffly the joint resists moving: 0 swings freely.
    pub friction: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PhysicsCommand {
    Create { body: u32, spec: BodySpec },
    Joint { a: u32, b: u32, spec: JointSpec },
    /// Removes the body and every joint it is part of.
    Remove { body: u32 },
    /// Adds this velocity (units/s) to the body.
    Push { body: u32, velocity: [f32; 3] },
}

/// Where a body is, as the game last simulated it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyState {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub velocity: [f32; 3],
    pub spin: [f32; 3],
    /// Asleep: it has come to rest and costs nothing.
    pub resting: bool,
}
impl BodyState {
    /// The `rigid_get` record: position, rotation, velocity, spin, flags
    /// (1 resting), then padding.
    pub fn record(&self) -> [f32; STATE_RECORD] {
        let mut out = [0.0; STATE_RECORD];
        out[..3].copy_from_slice(&self.position);
        out[3..7].copy_from_slice(&self.rotation);
        out[7..10].copy_from_slice(&self.velocity);
        out[10..13].copy_from_slice(&self.spin);
        out[13] = f32::from(u8::from(self.resting));
        out.map(|v| if v.is_finite() { v } else { 0.0 })
    }
}

/// How one player's body is drawn this frame: named nodes placed in the
/// world (index, position, unit rotation). Nodes it leaves out keep their
/// animation relative to their parent.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerPose {
    pub player: u64,
    pub nodes: Vec<(u32, [f32; 3], [f32; 4])>,
}

fn finite(values: &[f32]) -> bool {
    values.iter().all(|v| v.is_finite())
}

fn unit_quaternion(q: [f32; 4]) -> Result<[f32; 4], String> {
    let q = glam::Quat::from_array(q);
    let length = q.length();
    if !length.is_finite() || length < 1e-3 {
        return Err("a rotation that is not a quaternion".into());
    }
    Ok((q / length).to_array())
}

fn clamp_length(v: [f32; 3], max: f32) -> [f32; 3] {
    glam::Vec3::from_array(v).clamp_length_max(max).to_array()
}

/// Read and check a `rigid_create` record.
pub fn body_spec(r: &[f32]) -> Result<BodySpec, String> {
    if r.len() != BODY_RECORD || !finite(r) {
        return Err("a body that is not finite numbers".into());
    }
    let size = |v: f32| (MIN_SIZE..=MAX_SIZE).contains(&v);
    let shape = match r[0] {
        0.0 if size(r[1]) && size(r[2]) && size(r[3]) => Shape::Box([r[1], r[2], r[3]]),
        1.0 if size(r[1]) => Shape::Ball(r[1]),
        2.0 if size(r[1]) && (0.0..=MAX_SIZE).contains(&r[2]) => Shape::Capsule(r[1], r[2]),
        0.0..=2.0 => {
            return Err(format!(
                "a body shape sized outside {MIN_SIZE} to {MAX_SIZE} units"
            ));
        }
        other => return Err(format!("no body shape {other}")),
    };
    let offset = [r[4], r[5], r[6]];
    if offset.iter().any(|v| v.abs() > MAX_SIZE) {
        return Err(format!("a shape more than {MAX_SIZE} units off its body"));
    }
    let position = [r[7], r[8], r[9]];
    if position.iter().any(|v| v.abs() > MAX_COORDINATE) {
        return Err("a body placed outside the world".into());
    }
    let group = r[23];
    if !(0.0..=65_535.0).contains(&group) || group.fract() != 0.0 {
        return Err(format!("no body group {group}"));
    }
    Ok(BodySpec {
        shape,
        offset,
        position,
        rotation: unit_quaternion([r[10], r[11], r[12], r[13]])?,
        velocity: clamp_length([r[14], r[15], r[16]], MAX_SPEED),
        spin: clamp_length([r[17], r[18], r[19]], MAX_SPIN),
        density: r[20].clamp(0.01, 100.0),
        friction: r[21].clamp(0.0, 4.0),
        bounce: r[22].clamp(0.0, 1.0),
        group: group as u32,
        linear_damping: r[24].clamp(0.0, 100.0),
        angular_damping: r[25].clamp(0.0, 100.0),
    })
}

/// Read and check a `rigid_joint` record: anchor, axis, swing, twist,
/// friction, then padding.
pub fn joint_spec(r: &[f32]) -> Result<JointSpec, String> {
    if r.len() != JOINT_RECORD || !finite(r) {
        return Err("a joint that is not finite numbers".into());
    }
    let anchor = [r[0], r[1], r[2]];
    if anchor.iter().any(|v| v.abs() > MAX_COORDINATE) {
        return Err("a joint placed outside the world".into());
    }
    let axis = glam::Vec3::new(r[3], r[4], r[5]).normalize_or(glam::Vec3::Y);
    let pi = std::f32::consts::PI;
    Ok(JointSpec {
        anchor,
        axis: axis.to_array(),
        swing: r[6].clamp(0.0, pi),
        twist: r[7].clamp(0.0, pi),
        friction: r[8].clamp(0.0, 100.0),
    })
}

/// Read and check `count` `pose` records.
pub fn pose_nodes(r: &[f32]) -> Result<Vec<(u32, [f32; 3], [f32; 4])>, String> {
    let mut out = Vec::with_capacity(r.len() / POSE_RECORD);
    for n in r.chunks_exact(POSE_RECORD) {
        if !finite(n) {
            return Err("a pose that is not finite numbers".into());
        }
        if !(0.0..MAX_NODES as f32).contains(&n[0]) || n[0].fract() != 0.0 {
            return Err(format!("no node {}", n[0]));
        }
        let position = [n[1], n[2], n[3]];
        if position.iter().any(|v| v.abs() > MAX_COORDINATE) {
            return Err("a node posed outside the world".into());
        }
        out.push((n[0] as u32, position, unit_quaternion([n[4], n[5], n[6], n[7]])?));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> [f32; BODY_RECORD] {
        let mut r = [0.0; BODY_RECORD];
        r[..4].copy_from_slice(&[0.0, 0.5, 0.25, 0.5]);
        r[13] = 1.0;
        r[20] = 1.0;
        r
    }

    #[test]
    fn body_records_are_checked_and_clamped() {
        let mut r = body();
        r[10..14].copy_from_slice(&[0.0, 2.0, 0.0, 0.0]);
        r[14] = 1.0e5;
        r[23] = 7.0;
        let spec = body_spec(&r).unwrap();
        assert_eq!(spec.shape, Shape::Box([0.5, 0.25, 0.5]));
        assert_eq!(spec.rotation, [0.0, 1.0, 0.0, 0.0], "normalised");
        assert_eq!(spec.velocity[0], MAX_SPEED);
        assert_eq!(spec.group, 7);
        let mut bad = body();
        bad[0] = 3.0;
        assert!(body_spec(&bad).is_err());
        let mut bad = body();
        bad[1] = 100.0;
        assert!(body_spec(&bad).unwrap_err().contains("sized"));
        let mut bad = body();
        bad[13] = 0.0;
        assert!(body_spec(&bad).unwrap_err().contains("quaternion"));
        let mut bad = body();
        bad[8] = f32::NAN;
        assert!(body_spec(&bad).is_err());
        let mut bad = body();
        bad[23] = 1.5;
        assert!(body_spec(&bad).is_err());
    }

    #[test]
    fn joints_and_poses_are_checked() {
        let mut j = [0.0; JOINT_RECORD];
        j[6] = 9.0;
        let spec = joint_spec(&j).unwrap();
        assert_eq!(spec.axis, [0.0, 1.0, 0.0], "a zero axis stands up");
        assert_eq!(spec.swing, std::f32::consts::PI);
        let pose = [3.0, 1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 2.0];
        assert_eq!(
            pose_nodes(&pose).unwrap(),
            [(3, [1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0])]
        );
        assert!(pose_nodes(&[0.5, 0., 0., 0., 0., 0., 0., 1.]).is_err());
        assert!(pose_nodes(&[256.0, 0., 0., 0., 0., 0., 0., 1.]).is_err());
    }

    #[test]
    fn state_records_are_finite() {
        let state = BodyState {
            position: [1.0, f32::NAN, 3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            velocity: [0.0; 3],
            spin: [0.0; 3],
            resting: true,
        };
        let r = state.record();
        assert_eq!(&r[..3], &[1.0, 0.0, 3.0]);
        assert_eq!(r[13], 1.0);
    }
}
