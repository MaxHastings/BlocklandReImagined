//! How a body walks, shared by the host and every client: which locomotion
//! clip its motion picks and the stretch of a clip a frame plays, so the
//! footfalls a clip marks are the same moments wherever they are worked out.
use crate::player::PlayerState;
use glam::Vec3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocomotionAction {
    pub sequence: &'static str,
    pub forward: bool,
}

/// Speeds within this of each other tie. v20 rotates velocity into object
/// space with the transpose of the same z rotation that built it, so a
/// 45 degree move yields bit-identical forward and side components. Our
/// dot products differ by a few ulps instead, which alone must not flip it.
const PICK_TIE: f32 = 1e-4;

/// v20 `Player::pickActionAnimation` (blocklandv20.exe 0x5a2fe0): jetting
/// always holds the root pose; otherwise only a real fall (below -10 vertical
/// speed) uses `fall`.
///
/// On the ground an object-space velocity shorter than 0.4, vertical speed
/// included, is root. Otherwise it walks `actionList` in order (run, back,
/// side) and keeps the first sequence whose direction dotted with velocity
/// beats the running maximum, which starts at 0.1. The comparison is strict,
/// so an exact diagonal keeps `run` or `back`, never `side`. The Blockhead
/// clips carry no ground motion, so their directions are the table defaults:
/// run +Y, back -Y and side -X (left). Only the side clip is reused in
/// reverse, for strafing right. Crouching maps these to the crouch clips.
///
/// Then water (0x5a3308): over 60% covered always holds the root pose, as does
/// any coverage while off the ground or sinking faster than 0.1. (v20 also
/// roots a wading player holding jump; remote players' triggers are not
/// replicated, so that clause is left out.)
pub fn locomotion(player: &PlayerState, coverage: f32) -> LocomotionAction {
    let forward = |sequence| LocomotionAction {
        sequence,
        forward: true,
    };
    if player.jetting {
        return forward("root");
    }
    let crouched = player.crouched && !player.posture.prone;
    let root = if crouched { "crouch" } else { "root" };
    if coverage > 0.6 || (coverage > 0.01 && (!player.grounded || player.velocity[1] < -0.1)) {
        return forward(root);
    }
    if !player.grounded {
        return forward(if player.velocity[1] < -10.0 {
            "fall"
        } else if player.velocity[1] > 0.5 {
            "jump"
        } else {
            "root"
        });
    }
    let facing = Vec3::new(player.yaw.sin(), 0.0, -player.yaw.cos());
    let right = Vec3::new(player.yaw.cos(), 0.0, player.yaw.sin());
    let velocity = Vec3::from(player.velocity);
    let (f, r) = (velocity.dot(facing), velocity.dot(right));
    let [root, run, back, side] = if crouched {
        ["crouch", "crouchrun", "crouchback", "crouchside"]
    } else {
        ["root", "run", "back", "side"]
    };
    let mut action = forward(root);
    if velocity.length() < 0.4 {
        return action;
    }
    let mut best = 0.1;
    for (sequence, d) in [(run, f), (back, -f), (side, -r)] {
        if d > best + PICK_TIE {
            best = d;
            action = forward(sequence);
        } else if sequence == side && -d > best + PICK_TIE {
            best = -d;
            action = LocomotionAction {
                sequence,
                forward: false,
            };
        }
    }
    action
}
/// Army Ops' ground pick, `APawn::Get4WayDirection`, in place of v20's: a ground run, back or side pick
/// (the stance prefix kept) becomes the direction the unit horizontal velocity falls in, relative to the
/// body's facing. Forward and backward each take a cone of 0.82 (cos 34.9 degrees) of the facing axis,
/// so a 45 degree diagonal plays a strafe clip. Right plays the left strafe clip reversed (or its own).
/// Every other pick (root, jump, fall, swim) keeps v20's rule.
pub fn ue2_four_way(action: LocomotionAction, player: &PlayerState) -> LocomotionAction {
    let crouch = match action.sequence {
        "crouchrun" | "crouchback" | "crouchside" => true,
        "run" | "back" | "side" => false,
        _ => return action,
    };
    let facing = [player.yaw.sin(), -player.yaw.cos(), 0.0];
    let right = [player.yaw.cos(), player.yaw.sin(), 0.0];
    let velocity = [player.velocity[0], player.velocity[2], 0.0];
    let (sequence, forward) =
        match bri_ue2_rules::ground::four_way_direction(velocity, facing, right) {
            bri_ue2_rules::ground::BACKWARD => (if crouch { "crouchback" } else { "back" }, true),
            bri_ue2_rules::ground::LEFT => (if crouch { "crouchside" } else { "side" }, true),
            bri_ue2_rules::ground::RIGHT => (if crouch { "crouchside" } else { "side" }, false),
            _ => (if crouch { "crouchrun" } else { "run" }, true),
        };
    LocomotionAction { sequence, forward }
}

/// The stretch of its base clip a body played this frame: `from` to `to` in
/// seconds, not wrapped, so a looping clip that came round runs past
/// `duration`. A clip played backward has `to` below `from`; one that just
/// started (`restart`) also covers its first instant.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipSpan {
    /// The clip's lower-case name.
    pub clip: String,
    pub from: f32,
    pub to: f32,
    pub duration: f32,
    pub looping: bool,
    pub restart: bool,
}
impl ClipSpan {
    /// Whether a moment `at` seconds into the clip went by in this span.
    pub fn passes(&self, at: f32) -> bool {
        let (lo, hi) = (self.from.min(self.to), self.from.max(self.to));
        let forward = self.to >= self.from;
        let hit = |t: f32| {
            if self.restart {
                t >= lo && t <= hi
            } else if forward {
                t > lo && t <= hi
            } else {
                t >= lo && t < hi
            }
        };
        if !(self.looping && self.duration > 0.0) {
            return hit(at);
        }
        // Every lap the span reaches into, a few at most.
        let first = (lo / self.duration).floor() as i64;
        let last = (hi / self.duration).floor() as i64;
        (first..=last.min(first + 3)).any(|lap| hit(at + lap as f32 * self.duration))
    }
}
