//! How a body walks, shared by the host and every client: which locomotion
//! clip its motion picks and the stretch of a clip a frame plays, so the
//! footfalls a clip marks are the same moments wherever they are worked out.
use crate::player::PlayerState;
use bri_console::Clamp;
use bri_content::body::{GroundSpeeds, LocomotionPick, Role, RoleClips};
use glam::{Quat, Vec3};
use std::collections::BTreeMap;

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
    // Holding a ladder: the climb clip of the way it moves, which a body
    // holding still keeps (its clip plays at the rate it gains height).
    if player.posture.climbing {
        return forward(if player.velocity[1] < 0.0 {
            "ladderdown"
        } else {
            "ladderup"
        });
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

/// The ground locomotion action of a body in `player`'s state: v20's pick, or
/// Army Ops' four-way pick, with a jet in flight holding the root pose and a
/// body with its own prone or right-strafe clips playing those.
pub fn ground_action(
    player: &PlayerState,
    water_coverage: f32,
    pick: LocomotionPick,
    roles: &RoleClips,
    prone_authored: bool,
) -> LocomotionAction {
    let mut action = locomotion(player, water_coverage);
    if pick == LocomotionPick::Ue2FourWay {
        action = ue2_four_way(action, player);
    }
    if player.jump.jet_flight && action.sequence == "jump" {
        action.sequence = if player.crouched { "crouch" } else { "root" };
    }
    if prone_authored {
        own_prone_action(action, roles)
    } else {
        own_right_strafe(action, roles)
    }
}

/// How fast a locomotion clip for `role` plays while the body moves at
/// `velocity` (its own frame, facing -Z): v20's `Player::updateAnimation`
/// plays it at the speed the body covers the ground along the clip's way,
/// `|v . dir| / speed`, up to `max_time_scale`. The speed is the archetype's
/// for the role, else the clip's own ground motion (`ground`: its last
/// translation and its duration); a clip with neither, and every other role,
/// plays at 1x.
pub fn time_scale(
    ground_speeds: &GroundSpeeds,
    pick: LocomotionPick,
    max_time_scale: f32,
    ground: Option<(Vec3, f32)>,
    role: Role,
    velocity: Vec3,
) -> f32 {
    let Some(axis) = role.ground_axis() else {
        return 1.0;
    };
    let ground =
        ground.filter(|(t, duration)| t.is_finite() && *duration > 0.0 && t.length() > 1e-4);
    let named = ground_speeds.get(role.name()).or_else(|| {
        role.speed_alias()
            .and_then(|alias| ground_speeds.get(alias.name()))
    });
    let max = if max_time_scale > 0.0 {
        max_time_scale
    } else {
        crate::archetype::MAX_TIME_SCALE
    };
    // `APawn::PlayRunning`: the loop rate is the pawn's whole speed over the class ground speed,
    // whichever way the clip faces (see `bri_ue2_rules::ground`).
    if let (LocomotionPick::Ue2FourWay, Some(&speed)) = (pick, named) {
        let scale = velocity.length() / speed;
        return if scale.is_finite() {
            scale.min(max)
        } else {
            1.0
        };
    }
    let (speed, dir) = match named {
        Some(&speed) => (speed, Vec3::from(axis)),
        None => match ground {
            Some((t, duration)) => (t.length() / duration, t.normalize()),
            None => return 1.0,
        },
    };
    let scale = (velocity.dot(dir) / speed).abs().min(max);
    if scale.is_finite() { scale } else { 1.0 }
}

/// A body lying prone whose clips pose the lying body plays its prone locomotion
/// roles for the stance names `locomotion` picks from the velocity. A right
/// strafe takes its own clip when it has one, else the left clip reversed.
pub fn own_prone_action(action: LocomotionAction, roles: &RoleClips) -> LocomotionAction {
    let stay = |sequence| LocomotionAction {
        sequence,
        forward: true,
    };
    match (action.sequence, action.forward) {
        ("run", _) => stay("pronerun"),
        ("back", _) => stay("proneback"),
        ("side", true) => stay("proneside"),
        ("side", false) if roles.has_own(Role::ProneSideRight) => stay("pronesideright"),
        ("side", false) => LocomotionAction {
            sequence: "proneside",
            forward: false,
        },
        _ => stay("prone"),
    }
}

/// A body with its own right-strafe clip plays it, rather than its left clip
/// reversed (v20's only way to strafe right, which `locomotion` returns).
pub fn own_right_strafe(action: LocomotionAction, roles: &RoleClips) -> LocomotionAction {
    let (sequence, role) = match action.sequence {
        "side" => ("sideright", Role::SideRight),
        "crouchside" => ("crouchsideright", Role::CrouchSideRight),
        _ => return action,
    };
    if action.forward || !roles.has_own(role) {
        return action;
    }
    LocomotionAction {
        sequence,
        forward: true,
    }
}

/// Where the host has each body in its locomotion clip, so the footfalls the
/// clip marks fall on the moments a client draws them: the same pick, the
/// same rate from `ground_speeds`, the same marks. Overrides only a client
/// sees (a scripted thread, an intent clip, a seat, water) are not followed.
#[derive(Default)]
pub struct Gaits {
    bodies: BTreeMap<u64, Pace>,
}
#[derive(Default)]
struct Pace {
    /// Whether it stood on the ground last tick, and the fastest it fell since.
    grounded: bool,
    falling: f32,
    /// The action the body is in.
    mode: String,
    phase: f32,
    forward: bool,
}
impl Gaits {
    /// Advance `owner` by `seconds` and return the stretch of its clip it
    /// played. `clip` gives a clip's duration and whether it loops, by
    /// lower-case name; none when the body lacks the clip.
    pub fn advance(
        &mut self,
        owner: u64,
        seconds: f32,
        player: &PlayerState,
        look: &crate::archetype::Look,
        clip: impl Fn(&str) -> Option<(f32, bool)>,
    ) -> Option<ClipSpan> {
        let roles = RoleClips::resolve(&look.animations, |name| clip(name).is_some());
        let prone_authored = player.posture.prone && roles.has_own(Role::Prone);
        let action = ground_action(player, 0.0, look.locomotion_pick, &roles, prone_authored);
        let role = Role::from_blockhead_clip(action.sequence).unwrap_or(Role::Idle);
        let (name, reversed) = roles.clip(role)?;
        let (duration, looping) = clip(name)?;
        let pace = self.bodies.entry(owner).or_default();
        let restart = pace.mode != action.sequence;
        let before = pace.phase;
        if restart {
            pace.mode = action.sequence.to_owned();
            pace.forward = action.forward != reversed;
            pace.phase = if pace.forward { 0.0 } else { duration };
        } else {
            let local = Quat::from_rotation_y(player.body_yaw()) * Vec3::from(player.velocity)
                / player.scale.max(0.01);
            let rate = time_scale(
                &look.ground_speeds,
                look.locomotion_pick,
                look.max_time_scale,
                None,
                role,
                local,
            );
            pace.phase += if pace.forward {
                seconds * rate
            } else {
                -seconds * rate
            };
        }
        let span = ClipSpan {
            clip: name.to_owned(),
            from: if restart { pace.phase } else { before },
            to: pace.phase,
            duration,
            looping,
            restart,
        };
        pace.phase = if looping && duration > 0.0 {
            pace.phase.rem_euclid(duration)
        } else {
            pace.phase.clamped(0.0, duration)
        };
        Some(span)
    }
    /// How fast `owner` fell if it touched down this tick after falling (units
    /// per second, positive), else none. Call once a tick for each body.
    pub fn landed(&mut self, owner: u64, player: &PlayerState) -> Option<f32> {
        let pace = self.bodies.entry(owner).or_default();
        let fell =
            (player.grounded && !pace.grounded && pace.falling > 0.0).then_some(pace.falling);
        pace.falling = if player.grounded {
            0.0
        } else {
            pace.falling.max(-player.velocity[1])
        };
        pace.grounded = player.grounded;
        fell
    }
    /// Forget the bodies `keep` does not name.
    pub fn retain(&mut self, keep: impl Fn(u64) -> bool) {
        self.bodies.retain(|owner, _| keep(*owner));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archetype::Archetypes;

    fn running(speed: f32) -> PlayerState {
        PlayerState {
            owner: 1,
            feet: [0.0; 3],
            velocity: [0.0, 0.0, -speed],
            yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
            grounded: true,
            crouched: false,
            jetting: false,
            jump: Default::default(),
            archetype: Default::default(),
            scale: 1.0,
            energy: 100.0,
            speed_scale: 1.0,
            walking: false,
            jump_locked: false,
            tick: Default::default(),
            tether: None,
            controller: None,
            pass_group: 0,
            posture: Default::default(),
        }
    }

    #[test]
    fn a_body_on_a_ladder_plays_the_climb_clip_of_the_way_it_moves() {
        let mut on_ladder = running(0.0);
        on_ladder.grounded = false;
        on_ladder.posture.climbing = true;
        let pick = |vy: f32, p: &mut PlayerState| {
            p.velocity = [0.0, vy, 0.0];
            locomotion(p, 0.0).sequence
        };
        assert_eq!(pick(4.0, &mut on_ladder), "ladderup");
        assert_eq!(pick(-4.0, &mut on_ladder), "ladderdown");
        // Holding still keeps the pose, and the clip stands still with it.
        assert_eq!(pick(0.0, &mut on_ladder), "ladderup");
        let speeds = GroundSpeeds::from([("ladder_up".to_string(), 3.0)]);
        let rate = |vy: f32| {
            time_scale(
                &speeds,
                LocomotionPick::Ue2FourWay,
                1.5,
                None,
                Role::LadderUp,
                Vec3::new(0.0, vy, 0.0),
            )
        };
        assert_eq!((rate(0.0), rate(1.5)), (0.0, 0.5));
    }

    #[test]
    fn the_host_walks_the_clip_at_the_rate_a_client_would() {
        let mut look = Archetypes::default()
            .resolve(Default::default())
            .look
            .clone();
        look.locomotion_pick = LocomotionPick::Ue2FourWay;
        look.ground_speeds.insert("run".into(), 6.0);
        let clips = |name: &str| (name == "run" || name == "root").then_some((0.8, true));
        let mut gaits = Gaits::default();
        let me = running(6.0);
        // The first frame puts the body at the start of its clip.
        let first = gaits.advance(1, 0.1, &me, &look, clips).unwrap();
        assert!(first.restart);
        assert_eq!(first.clip, "run");
        assert_eq!((first.from, first.to), (0.0, 0.0));
        // At the class ground speed the clip plays at 1x.
        let next = gaits.advance(1, 0.1, &me, &look, clips).unwrap();
        assert!(!next.restart);
        assert!((next.to - next.from - 0.1).abs() < 1e-5, "{next:?}");
        // Twice as fast plays faster, up to the body's limit (v20's 1.5).
        let quick = running(12.0);
        let span = gaits.advance(1, 0.1, &quick, &look, clips).unwrap();
        assert!((span.to - span.from - 0.15).abs() < 1e-5, "{span:?}");
        // A mark at 0.25 s is passed exactly once as the clip goes by it.
        let mut gaits = Gaits::default();
        gaits.advance(2, 0.0, &me, &look, clips);
        let mut passes = 0;
        for _ in 0..40 {
            let span = gaits.advance(2, 0.02, &me, &look, clips).unwrap();
            passes += usize::from(span.passes(0.25));
        }
        // 0.8 s of clip over 40 frames: one lap.
        assert_eq!(passes, 1);
        // A body that stops falls back to its root clip and restarts.
        let still = running(0.0);
        let span = gaits.advance(2, 0.02, &still, &look, clips).unwrap();
        assert_eq!(span.clip, "root");
        assert!(span.restart);
    }
}
