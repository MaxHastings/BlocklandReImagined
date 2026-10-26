//! What a body offers the things that attach to it and animate it, so any
//! independently authored body holds any item, sits in any seat and plays
//! its own clips. A body names its sockets (hands, eye, seat) and maps
//! semantic animation roles to its own clip names; whatever it leaves out
//! defaults to the Blockhead's names, so the Blockhead needs neither.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Whether crouching is an independent legacy pose thread or already authored
/// into each of the body's crouched locomotion clips.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrouchComposition {
    #[default]
    Overlay,
    Locomotion,
}
impl CrouchComposition {
    pub fn is_overlay(&self) -> bool {
        *self == Self::Overlay
    }
}

/// How a body's ground locomotion clip is picked from its velocity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocomotionPick {
    /// v20's `Player::pickActionAnimation`: the clip whose direction beats the others.
    #[default]
    Torque,
    /// Army Ops' `APawn::Get4WayDirection`: forward within 35 degrees, backward within 35 degrees, a strafe
    /// clip for everything between; the clip plays at speed over class ground speed (3D, no cap but
    /// `max_time_scale`).
    Ue2FourWay,
}
impl LocomotionPick {
    pub fn is_torque(&self) -> bool {
        *self == Self::Torque
    }
}

/// Where on a body something attaches: a node of its model, an offset, or
/// both. With a node the offset is in that node's frame; without one it is
/// from the feet, facing -Z, at scale 1 (a box body has no nodes).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Socket {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(skip_serializing_if = "is_zero")]
    pub offset: [f32; 3],
}
fn is_zero(v: &[f32; 3]) -> bool {
    *v == [0.0; 3]
}
impl Socket {
    pub fn node(name: &str) -> Self {
        Self {
            node: Some(name.into()),
            offset: [0.0; 3],
        }
    }
    pub fn at(offset: [f32; 3]) -> Self {
        Self { node: None, offset }
    }
    fn validate(&self, name: &str) -> Result<()> {
        ensure!(
            self.node
                .as_ref()
                .is_none_or(|n| !n.is_empty() && n.len() <= 64 && !n.chars().any(char::is_control))
                && self.offset.iter().all(|v| v.is_finite() && v.abs() <= 64.0),
            "socket {name} needs a node name of up to 64 characters and a finite offset"
        );
        Ok(())
    }
}

/// The Blockhead's hand and eye nodes: what a body that names no sockets
/// has. Image mount points 0 and 1 are the right and left hands.
pub const BLOCKHEAD_HANDS: [&str; 2] = ["Mount0", "Mount1"];
pub const BLOCKHEAD_EYE: &str = "Eye";

/// A body's sockets. Each left out is the Blockhead's (`Mount0`, `Mount1`,
/// `Eye`); the Blockhead sits by its `sit` clip and so has no seat socket.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sockets {
    /// Authored hand clips/socket frames already hold items correctly. Preserve
    /// them instead of applying generic arm raising and support fitting.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub preserve_hand_pose: bool,
    /// Where image mount point 0 (a held tool) attaches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_hand: Option<Socket>,
    /// Where image mount point 1 (a second, akimbo tool) attaches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_hand: Option<Socket>,
    /// Where the body sees from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eye: Option<Socket>,
    /// The hip: seated, a box body folds what is below it forward.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seat: Option<Socket>,
}
impl Sockets {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    /// The socket image mount point `mount` attaches to: the hands for 0
    /// and 1, `None` for the others, which keep their `Mount<n>` node.
    pub fn hand(&self, mount: u32) -> Option<Socket> {
        let (own, name) = match mount {
            0 => (&self.right_hand, BLOCKHEAD_HANDS[0]),
            1 => (&self.left_hand, BLOCKHEAD_HANDS[1]),
            _ => return None,
        };
        Some(own.clone().unwrap_or_else(|| Socket::node(name)))
    }
    pub fn eye(&self) -> Socket {
        self.eye
            .clone()
            .unwrap_or_else(|| Socket::node(BLOCKHEAD_EYE))
    }
    pub fn validate(&self) -> Result<()> {
        for (name, socket) in [
            ("right_hand", &self.right_hand),
            ("left_hand", &self.left_hand),
            ("eye", &self.eye),
            ("seat", &self.seat),
        ] {
            if let Some(socket) = socket {
                socket.validate(name)?;
            }
        }
        Ok(())
    }
}

/// A semantic animation role: what the engine wants a body to do, which
/// each body maps to a clip of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Idle,
    Run,
    Back,
    Side,
    /// `side` for strafing right, when the body has its own right clip rather
    /// than playing its left one reversed.
    SideRight,
    Jump,
    Fall,
    Crouch,
    CrouchRun,
    CrouchBack,
    CrouchSide,
    /// `crouch_side` for strafing right, as `side_right` is for `side`.
    CrouchSideRight,
    /// Lying prone, on a body whose clips author the lying pose (a body without
    /// them is laid face down by its posture frame). Idle, then crawling forward,
    /// back and sideways, as the crouch roles are for crouching.
    Prone,
    ProneRun,
    ProneBack,
    ProneSide,
    ProneSideRight,
    AimRight,
    AimLeft,
    AimBoth,
    Sit,
    Death,
    Swim,
    Look,
    /// `look` while the body is crouched; a body without it keeps its `look`.
    LookCrouch,
    /// `look` while the body is prone; a body without it keeps its `look_crouch`.
    LookProne,
    HeadSide,
    HeadUp,
    SeatAim,
    /// Climbing a ladder, up and down: the climb clip plays at the rate the
    /// body gains height (a body holding still on the ladder holds its pose).
    LadderUp,
    LadderDown,
}
impl Role {
    pub const ALL: [Role; 31] = [
        Role::Idle,
        Role::Run,
        Role::Back,
        Role::Side,
        Role::SideRight,
        Role::Jump,
        Role::Fall,
        Role::Crouch,
        Role::CrouchRun,
        Role::CrouchBack,
        Role::CrouchSide,
        Role::CrouchSideRight,
        Role::Prone,
        Role::ProneRun,
        Role::ProneBack,
        Role::ProneSide,
        Role::ProneSideRight,
        Role::AimRight,
        Role::AimLeft,
        Role::AimBoth,
        Role::Sit,
        Role::Death,
        Role::Swim,
        Role::Look,
        Role::LookCrouch,
        Role::LookProne,
        Role::HeadSide,
        Role::HeadUp,
        Role::SeatAim,
        Role::LadderUp,
        Role::LadderDown,
    ];
    /// The role's name in an archetype's `animations`.
    pub fn name(self) -> &'static str {
        match self {
            Role::Idle => "idle",
            Role::Run => "run",
            Role::Back => "back",
            Role::Side => "side",
            Role::SideRight => "side_right",
            Role::Jump => "jump",
            Role::Fall => "fall",
            Role::Crouch => "crouch",
            Role::CrouchRun => "crouch_run",
            Role::CrouchBack => "crouch_back",
            Role::CrouchSide => "crouch_side",
            Role::CrouchSideRight => "crouch_side_right",
            Role::Prone => "prone",
            Role::ProneRun => "prone_run",
            Role::ProneBack => "prone_back",
            Role::ProneSide => "prone_side",
            Role::ProneSideRight => "prone_side_right",
            Role::AimRight => "aim_right",
            Role::AimLeft => "aim_left",
            Role::AimBoth => "aim_both",
            Role::Sit => "sit",
            Role::Death => "death",
            Role::Swim => "swim",
            Role::Look => "look",
            Role::LookCrouch => "look_crouch",
            Role::LookProne => "look_prone",
            Role::HeadSide => "head_side",
            Role::HeadUp => "head_up",
            Role::SeatAim => "seat_aim",
            Role::LadderUp => "ladder_up",
            Role::LadderDown => "ladder_down",
        }
    }
    /// The Blockhead's clip for the role (m.dts sequence names, lower case):
    /// what a body that does not map the role is asked for.
    pub fn blockhead_clip(self) -> &'static str {
        match self {
            Role::Idle => "root",
            Role::Run => "run",
            Role::Back => "back",
            Role::Side => "side",
            Role::SideRight => "sideright",
            Role::Jump => "jump",
            Role::Fall => "fall",
            Role::Crouch => "crouch",
            Role::CrouchRun => "crouchrun",
            Role::CrouchBack => "crouchback",
            Role::CrouchSide => "crouchside",
            Role::CrouchSideRight => "crouchsideright",
            Role::Prone => "prone",
            Role::ProneRun => "pronerun",
            Role::ProneBack => "proneback",
            Role::ProneSide => "proneside",
            Role::ProneSideRight => "pronesideright",
            Role::AimRight => "armreadyright",
            Role::AimLeft => "armreadyleft",
            Role::AimBoth => "armreadyboth",
            Role::Sit => "sit",
            Role::Death => "death1",
            Role::Swim => "swim",
            Role::Look => "look",
            Role::LookCrouch => "lookcrouch",
            Role::LookProne => "lookprone",
            Role::HeadSide => "headside",
            Role::HeadUp => "headup",
            Role::SeatAim => "seataim",
            Role::LadderUp => "ladderup",
            Role::LadderDown => "ladderdown",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|r| r.name().eq_ignore_ascii_case(name))
    }
    /// The role a Blockhead clip name plays, for engine code and scripts
    /// that ask for clips by those names (`playThread(0, sit)`).
    pub fn from_blockhead_clip(clip: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|r| r.blockhead_clip().eq_ignore_ascii_case(clip))
    }
    /// What stands in for the role when the body has no clip for it, best
    /// first, each with whether it plays reversed (walking back on the run
    /// clip). Empty: the layer is skipped, which leaves the pose below it.
    pub fn fallbacks(self) -> &'static [(Role, bool)] {
        match self {
            Role::Idle => &[],
            Role::Run => &[(Role::Idle, false)],
            Role::Back => &[(Role::Run, true), (Role::Idle, false)],
            Role::Side => &[(Role::Run, false), (Role::Idle, false)],
            Role::SideRight => &[(Role::Side, true)],
            Role::Jump => &[(Role::Idle, false)],
            Role::Fall => &[(Role::Jump, false), (Role::Idle, false)],
            Role::Crouch => &[(Role::Idle, false)],
            Role::CrouchRun => &[(Role::Crouch, false), (Role::Run, false)],
            Role::CrouchBack => &[(Role::Crouch, false), (Role::Back, false)],
            Role::CrouchSide => &[(Role::Crouch, false), (Role::Side, false)],
            Role::CrouchSideRight => &[(Role::CrouchSide, true), (Role::SideRight, false)],
            Role::Prone => &[],
            Role::ProneRun => &[(Role::Prone, false)],
            Role::ProneBack => &[(Role::ProneRun, true), (Role::Prone, false)],
            Role::ProneSide => &[(Role::ProneRun, false), (Role::Prone, false)],
            Role::ProneSideRight => &[(Role::ProneSide, true)],
            Role::AimRight => &[],
            Role::AimLeft => &[],
            Role::AimBoth => &[(Role::AimRight, false)],
            Role::Sit => &[(Role::Idle, false)],
            Role::Death => &[(Role::Idle, false)],
            Role::Swim => &[],
            Role::Look => &[],
            Role::LookCrouch => &[(Role::Look, false)],
            Role::LookProne => &[(Role::LookCrouch, false)],
            Role::HeadSide => &[],
            Role::HeadUp => &[],
            Role::SeatAim => &[],
            Role::LadderUp => &[(Role::Idle, false)],
            Role::LadderDown => &[(Role::LadderUp, true), (Role::Idle, false)],
        }
    }
    /// The way a locomotion role's clip carries the body, facing -Z (v20's
    /// `ActionAnimationList` directions): the clip plays at the speed the
    /// body covers ground that way (`Player::updateAnimation`). None for a
    /// clip that keeps its own time.
    pub fn ground_axis(self) -> Option<[f32; 3]> {
        match self {
            Role::Run | Role::CrouchRun => Some([0.0, 0.0, -1.0]),
            Role::Back | Role::CrouchBack => Some([0.0, 0.0, 1.0]),
            Role::Side | Role::CrouchSide => Some([-1.0, 0.0, 0.0]),
            Role::SideRight | Role::CrouchSideRight => Some([1.0, 0.0, 0.0]),
            Role::ProneRun => Some([0.0, 0.0, -1.0]),
            Role::ProneBack => Some([0.0, 0.0, 1.0]),
            Role::ProneSide => Some([-1.0, 0.0, 0.0]),
            Role::ProneSideRight => Some([1.0, 0.0, 0.0]),
            Role::LadderUp => Some([0.0, 1.0, 0.0]),
            Role::LadderDown => Some([0.0, -1.0, 0.0]),
            _ => None,
        }
    }
    /// Roles a body supplies only to improve on another's: a body without
    /// one is not reported as missing it.
    pub fn optional(self) -> bool {
        matches!(
            self,
            Role::LookCrouch
                | Role::LookProne
                | Role::SideRight
                | Role::CrouchSideRight
                | Role::Prone
                | Role::ProneRun
                | Role::ProneBack
                | Role::ProneSide
                | Role::ProneSideRight
                | Role::LadderUp
                | Role::LadderDown
        )
    }
    /// The role whose ground speed stands in for this one's when the
    /// archetype names none for it.
    pub fn speed_alias(self) -> Option<Role> {
        match self {
            Role::SideRight => Some(Role::Side),
            Role::CrouchSideRight => Some(Role::CrouchSide),
            Role::ProneSideRight => Some(Role::ProneSide),
            _ => None,
        }
    }
    /// What the body loses when the role has no clip and nothing stands in.
    pub fn cost(self) -> &'static str {
        match self {
            Role::Idle => "it stands in its bind pose",
            Role::AimRight | Role::AimLeft => "its arm does not raise a held item",
            Role::Look | Role::LookCrouch | Role::LookProne => "it does not look up and down",
            Role::HeadSide => "its head does not turn in free look",
            Role::HeadUp => "its head does not tilt under a pack",
            Role::SeatAim => "a seated gunner bends only with its `look` clip",
            Role::Swim => "it swims in its locomotion pose",
            _ => "that layer is left out",
        }
    }
}

/// An archetype's `animations`: role name to the body's clip name.
pub type Animations = BTreeMap<String, String>;

pub fn validate_animations(animations: &Animations) -> Result<()> {
    ensure!(animations.len() <= 64, "at most 64 animation roles");
    for (role, clip) in animations {
        ensure!(
            Role::from_name(role).is_some(),
            "unknown animation role `{role}`: use one of {:?}",
            Role::ALL.map(Role::name)
        );
        ensure!(
            !clip.is_empty() && clip.len() <= 64 && !clip.chars().any(char::is_control),
            "animation role `{role}` needs a clip name of up to 64 characters"
        );
    }
    Ok(())
}

/// An archetype's `deaths`: the body's own death clips by the side the
/// killing hit came from, and others to pick from at random. A side with no
/// clips takes one from `any`; with none at all the body plays its `death`
/// role. Every player's game picks the same clip (the host sends the side
/// and a roll).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Deaths {
    /// Hit from in front: the body falls backward.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub front: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub back: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub left: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub right: Vec<String>,
    /// Whatever the side, and for a death with no side (a fall, lava).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub any: Vec<String>,
    /// A body that died crouched plays one of these, whatever the side:
    /// the clip must start from a crouch. A body with `deaths` needs them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub crouched: Vec<String>,
    /// The same for a body that died lying prone, needed by a body that can lie prone.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prone: Vec<String>,
}
/// Most bones one body's [`Ragdoll`] may name.
pub const MAX_RAGDOLL_BONES: usize = 32;
/// Most shapes one ragdoll bone may be built of.
pub const MAX_RAGDOLL_SHAPES: usize = 8;
/// Smallest and largest a ragdoll bone's shape may be, in units: thinner
/// ones tunnel through the world.
pub const RAGDOLL_SIZE: std::ops::RangeInclusive<f32> = 0.01..=32.0;
/// Fastest a ragdoll may move, at most (units/s).
pub const MAX_RAGDOLL_SPEED: f32 = 200.0;
/// A body that goes limp when it dies, drawn on each player's screen only
/// (nothing about it reaches the host or gameplay): one rigid body per bone
/// of its skeleton, jointed to the nearest bone above it that has one, so it
/// tumbles down stairs and over rails. A body without one plays its death
/// clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ragdoll {
    /// The bones that become bodies, root first (it has no joint).
    pub bones: Vec<RagdollBone>,
    /// Friction of every bone against the world and the others.
    pub friction: f32,
    /// How much of its speed a bone keeps rebounding, 0 to 1.
    #[serde(default, skip_serializing_if = "zero_f32")]
    pub bounce: f32,
    /// Fastest a bone may move (units/s, up to [`MAX_RAGDOLL_SPEED`]).
    pub max_speed: f32,
    /// Fastest a bone may spin (radians/s).
    pub max_spin: f32,
    /// Seconds after the body goes limp before it is held where it lies; 0
    /// never.
    #[serde(default, skip_serializing_if = "zero_f32")]
    pub freeze: f32,
    /// What a killing hit gives the body when its damage type names no
    /// [`DeathKick`] of its own.
    pub kick: DeathKick,
    /// How much of the speed the body died at it keeps: along the ground
    /// and up or down.
    pub carry: [f32; 2],
    /// Downward speed (units/s) past which the body counts as already
    /// falling and gets no `up_kick`.
    pub falling: f32,
    /// Turns the hit's offset from the body's centre and its push into
    /// spin (the inverse of how hard the body is to turn).
    pub inertia: f32,
    /// How much further across the ground than up the hit's offset counts
    /// toward that spin.
    pub spin_scale: f32,
    /// What a shot that kills a prone body gives it, of its kick, its spin
    /// and its turning: 0 to 1, with no upward kick at all.
    pub prone_scale: f32,
    /// The spin (radians/s) of a death with no direction to it, in a random
    /// direction.
    pub random_spin: f32,
}
/// What a killing hit gives a [`Ragdoll`], in speeds (units/s) along the
/// way the hit travelled. A damage type may carry its own (the weapons
/// pack's `death_kick`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeathKick {
    /// Given to the body at the point the hit struck: it turns the body as
    /// it moves it.
    pub impulse: f32,
    /// Given to the whole body.
    pub speed: f32,
    /// Added upward when the body is not already falling.
    pub up_kick: f32,
}
impl DeathKick {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.impulse, self.speed, self.up_kick]
                .iter()
                .all(|v| v.is_finite() && (0.0..=MAX_RAGDOLL_SPEED).contains(v)),
            "a death kick's impulse, speed and up_kick are 0 to {MAX_RAGDOLL_SPEED} units/s"
        );
        Ok(())
    }
}
fn zero_f32(v: &f32) -> bool {
    *v == 0.0
}
/// One bone of a [`Ragdoll`]: the skeleton node it moves with and the
/// shapes that stand for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RagdollBone {
    /// The skeleton node (a glTF node name).
    pub node: String,
    /// What it is built of, up to [`MAX_RAGDOLL_SHAPES`].
    pub shapes: Vec<RagdollShape>,
    /// Its weight.
    pub mass: f32,
    /// How quickly it loses speed and spin to the air.
    #[serde(default, skip_serializing_if = "zero_f32")]
    pub linear_damping: f32,
    #[serde(default, skip_serializing_if = "zero_f32")]
    pub angular_damping: f32,
    /// How it hangs from the bone above: none for the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joint: Option<RagdollJoint>,
}
/// One shape of a [`RagdollBone`], in the node's frame; `offset` is where
/// its centre sits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RagdollShape {
    /// Lying along the node's `axis` (`x`, `y` or `z`): the radius, then half
    /// the length of its straight middle.
    Capsule {
        radius: f32,
        half_length: f32,
        axis: char,
        #[serde(default)]
        offset: [f32; 3],
    },
    /// A capsule spanning the bone itself: from its node toward the bone
    /// below it (the nearest listed bone that hangs from it, else its first
    /// child in the rig; a bone with neither goes on the way its parent
    /// ran), centred between them, so the model supplies only the sizes.
    /// `length` is the straight middle's (the capsule is that and a
    /// `radius` cap beyond each end); left out, the bone's own span.
    Limb {
        radius: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        length: Option<f32>,
    },
    /// `rotation` turns it about the node's x, y then z axes (radians).
    Box {
        half: [f32; 3],
        #[serde(default)]
        offset: [f32; 3],
        #[serde(default)]
        rotation: [f32; 3],
    },
    Ball {
        radius: f32,
        #[serde(default)]
        offset: [f32; 3],
    },
}
/// A [`RagdollBone`]'s joint, pinned at the bone's node and limited round
/// the pose the body died in. A ball swings by `swing` (and `swing_second`)
/// and twists by `twist`; a hinge only turns about the node's x axis
/// between `hinge[0]` and `hinge[1]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RagdollJoint {
    /// Radians, 0 to pi.
    pub swing: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swing_second: Option<f32>,
    pub twist: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hinge: Option<[f32; 2]>,
    /// The node's axis (`x`, `y` or `z`) a ball twists and a hinge turns
    /// about; `x` when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<char>,
    /// How much it resists swinging (a hinge: turning); 0 swings freely.
    #[serde(default, skip_serializing_if = "zero_f32")]
    pub damping: f32,
    /// Karma's skeletal joint cone as a soft limit: with this set, `swing`
    /// and `swing_second` are not stops but the cone past which a spring of
    /// this stiffness (torque per radian past it) and a damper of
    /// `cone_damping` (torque per radian per second) pull the swing back,
    /// as in AA2 (`CONE_STIFFNESS`, `CONE_DAMPING`). Nothing acts inside the
    /// cone. A ball joint only. Left out, the swing stops at its limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cone_stiffness: Option<f32>,
    /// The soft cone's damper; see `cone_stiffness`.
    #[serde(default, skip_serializing_if = "zero_f32")]
    pub cone_damping: f32,
    /// How much it resists twisting; `damping` when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub twist_damping: Option<f32>,
}
impl RagdollShape {
    fn valid(&self) -> bool {
        let size = |v: f32| RAGDOLL_SIZE.contains(&v);
        let near = |o: &[f32; 3]| o.iter().all(|v| v.is_finite() && v.abs() <= 32.0);
        match self {
            Self::Capsule {
                radius,
                half_length,
                axis,
                offset,
            } => {
                size(*radius)
                    && size(*half_length)
                    && matches!(axis, 'x' | 'y' | 'z')
                    && near(offset)
            }
            Self::Limb { radius, length } => size(*radius) && length.is_none_or(size),
            Self::Box {
                half,
                offset,
                rotation,
            } => {
                half.iter().all(|h| size(*h))
                    && near(offset)
                    && rotation
                        .iter()
                        .all(|a| a.is_finite() && a.abs() <= std::f32::consts::TAU)
            }
            Self::Ball { radius, offset } => size(*radius) && near(offset),
        }
    }
}
impl Ragdoll {
    pub fn validate(&self) -> Result<()> {
        let finite = |v: f32, low: f32, high: f32| v.is_finite() && (low..=high).contains(&v);
        ensure!(
            (1..=MAX_RAGDOLL_BONES).contains(&self.bones.len()),
            "a ragdoll names 1 to {MAX_RAGDOLL_BONES} bones"
        );
        ensure!(
            finite(self.friction, 0.0, 4.0)
                && finite(self.bounce, 0.0, 1.0)
                && finite(self.max_speed, 0.1, MAX_RAGDOLL_SPEED)
                && finite(self.max_spin, 0.1, 1000.0)
                && finite(self.freeze, 0.0, 600.0)
                && self.carry.iter().all(|c| finite(*c, 0.0, 1.0))
                && finite(self.falling, 0.0, MAX_RAGDOLL_SPEED)
                && finite(self.inertia, 0.0, 1000.0)
                && finite(self.spin_scale, 0.0, 1000.0)
                && finite(self.prone_scale, 0.0, 1.0)
                && finite(self.random_spin, 0.0, 1000.0),
            "ragdoll friction (0 to 4), bounce (0 to 1), max_speed (0.1 to {MAX_RAGDOLL_SPEED}), max_spin (0.1 to 1000), freeze (0 to 600 s), carry (0 to 1), inertia, spin_scale or random_spin (0 to 1000) or prone_scale (0 to 1) is out of range"
        );
        self.kick.validate()?;
        for (index, bone) in self.bones.iter().enumerate() {
            ensure!(
                !bone.node.is_empty()
                    && bone.node.len() <= 64
                    && !bone.node.chars().any(char::is_control),
                "ragdoll bone {index} needs a node name of up to 64 characters"
            );
            ensure!(
                self.bones[..index].iter().all(|b| b.node != bone.node),
                "ragdoll bone {} is named twice",
                bone.node
            );
            ensure!(
                bone.joint.is_none() == (index == 0),
                "ragdoll bone {}: the first bone is the root and has no joint; every other has one",
                bone.node
            );
            ensure!(
                finite(bone.mass, 0.001, 1000.0)
                    && finite(bone.linear_damping, 0.0, 100.0)
                    && finite(bone.angular_damping, 0.0, 100.0),
                "ragdoll bone {}: mass (0.001 to 1000) or damping (0 to 100) is out of range",
                bone.node
            );
            ensure!(
                (1..=MAX_RAGDOLL_SHAPES).contains(&bone.shapes.len())
                    && bone.shapes.iter().all(RagdollShape::valid),
                "ragdoll bone {}: 1 to {MAX_RAGDOLL_SHAPES} shapes of {} to {} units, a capsule's axis x, y or z, offsets within 32 units, a box turned within a full turn",
                bone.node,
                RAGDOLL_SIZE.start(),
                RAGDOLL_SIZE.end()
            );
            if let Some(joint) = &bone.joint {
                let pi = std::f32::consts::PI;
                ensure!(
                    finite(joint.swing, 0.0, pi)
                        && joint.swing_second.is_none_or(|v| finite(v, 0.0, pi))
                        && finite(joint.twist, 0.0, pi)
                        && joint
                            .hinge
                            .is_none_or(|[low, high]| finite(low, -pi, pi) && finite(high, low, pi))
                        && finite(joint.damping, 0.0, 1000.0)
                        && joint.twist_damping.is_none_or(|v| finite(v, 0.0, 1000.0))
                        && joint.cone_stiffness.is_none_or(|v| finite(v, 0.0, 1.0e6))
                        && finite(joint.cone_damping, 0.0, 1.0e6)
                        && (joint.cone_stiffness.is_none() || joint.hinge.is_none())
                        && joint.axis.is_none_or(|a| matches!(a, 'x' | 'y' | 'z')),
                    "ragdoll bone {}: joint angles are 0 to pi radians (a hinge from -pi, rising), damping 0 to 1000 and axis x, y or z",
                    bone.node
                );
            }
        }
        Ok(())
    }
}
/// `$CorpseTimeoutValue`: seconds a v20 corpse lies.
pub const CORPSE_SECONDS: f32 = 5.0;
/// Longest a body may lie (an archetype's `corpse_seconds`).
pub const MAX_CORPSE_SECONDS: f32 = 600.0;
/// Most clips one side of [`Deaths`] lists.
pub const MAX_DEATH_CLIPS: usize = 8;
/// The posture a body died in, for [`Deaths::pick`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathPosture {
    Standing,
    Crouched,
    Prone,
}
/// A side of the body, for [`Deaths::pick`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Front,
    Back,
    Left,
    Right,
}
impl Deaths {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
    /// Every clip any list names.
    pub fn clips(&self) -> impl Iterator<Item = &String> {
        [
            &self.front,
            &self.back,
            &self.left,
            &self.right,
            &self.any,
            &self.crouched,
            &self.prone,
        ]
        .into_iter()
        .flatten()
    }
    pub fn validate(&self) -> Result<()> {
        for (side, clips) in [
            ("front", &self.front),
            ("back", &self.back),
            ("left", &self.left),
            ("right", &self.right),
            ("any", &self.any),
            ("crouched", &self.crouched),
            ("prone", &self.prone),
        ] {
            ensure!(
                clips.len() <= MAX_DEATH_CLIPS,
                "deaths.{side} lists at most {MAX_DEATH_CLIPS} clips"
            );
            for clip in clips {
                ensure!(
                    !clip.is_empty() && clip.len() <= 64 && !clip.chars().any(char::is_control),
                    "deaths.{side} needs clip names of up to 64 characters"
                );
            }
        }
        Ok(())
    }
    /// The clip for a death from `side` with `roll`, among those the body
    /// has (`has` takes a lower-case clip name), lower case; `None` plays
    /// the `death` role.
    pub fn pick(
        &self,
        posture: DeathPosture,
        side: Option<Side>,
        roll: u8,
        has: impl Fn(&str) -> bool,
    ) -> Option<String> {
        let owned = |clips: &Vec<String>| -> Vec<String> {
            clips
                .iter()
                .map(|c| c.to_ascii_lowercase())
                .filter(|c| has(c))
                .collect()
        };
        // A crouched or prone body plays only its own posture's clips: a standing clip on it is a wrong pose, so
        // with none it plays the `death` role and the body's report says so.
        let mut clips = match (posture, side) {
            (DeathPosture::Prone, _) => owned(&self.prone),
            (DeathPosture::Crouched, _) => owned(&self.crouched),
            (DeathPosture::Standing, side) => {
                let sided = match side {
                    Some(Side::Front) => &self.front,
                    Some(Side::Back) => &self.back,
                    Some(Side::Left) => &self.left,
                    Some(Side::Right) => &self.right,
                    None => &self.any,
                };
                let mut clips = owned(sided);
                if clips.is_empty() {
                    clips = owned(&self.any);
                }
                clips
            }
        };
        (!clips.is_empty()).then(|| clips.swap_remove(usize::from(roll) % clips.len()))
    }
}

/// An archetype's `ground_speeds`: locomotion role name to the speed, in
/// units per second, at which the body's clip for it plays at 1x.
pub type GroundSpeeds = BTreeMap<String, f32>;

pub fn validate_ground_speeds(speeds: &GroundSpeeds) -> Result<()> {
    for (role, speed) in speeds {
        ensure!(
            Role::from_name(role).is_some_and(|r| r.ground_axis().is_some()),
            "ground speed role `{role}` must be a locomotion role (run, back, side, side_right, crouch_run, crouch_back, crouch_side, crouch_side_right, prone_run, prone_back, prone_side or prone_side_right)"
        );
        ensure!(
            speed.is_finite() && (0.01..=1000.0).contains(speed),
            "ground speed for `{role}` must be 0.01 to 1000 units per second"
        );
    }
    Ok(())
}

/// A body's clip for each role, after its own names, the Blockhead's
/// defaults and the fallbacks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoleClips {
    clips: BTreeMap<Role, (String, bool)>,
    /// Roles with no clip of their own: what stands in (`None`: nothing,
    /// the layer is skipped).
    pub missing: Vec<(Role, Option<Role>)>,
    /// Roles the body has a clip of its own for (not a stand-in).
    own: std::collections::BTreeSet<Role>,
    /// What the archetype's `animations` names that cannot be honoured: a key
    /// that is no role, or a clip the body does not have (also for the
    /// optional roles, which are otherwise never reported).
    pub unhonoured: Vec<String>,
}
impl RoleClips {
    /// Resolve `animations` against the clips the body has (`has` takes a
    /// lower-case clip name).
    pub fn resolve(animations: &Animations, has: impl Fn(&str) -> bool) -> Self {
        let own: BTreeMap<Role, String> = Role::ALL
            .into_iter()
            .filter_map(|role| {
                let clip = animations
                    .iter()
                    .find(|(r, _)| r.eq_ignore_ascii_case(role.name()))
                    .map_or(role.blockhead_clip(), |(_, c)| c.as_str())
                    .to_ascii_lowercase();
                has(&clip).then_some((role, clip))
            })
            .collect();
        let mut clips = BTreeMap::new();
        let mut missing = Vec::new();
        for role in Role::ALL {
            if let Some(clip) = own.get(&role) {
                clips.insert(role, (clip.clone(), false));
                continue;
            }
            // The first stand-in the body has, through its own stand-ins.
            let mut stand_in = None;
            let mut queue: Vec<(Role, bool)> = role.fallbacks().to_vec();
            let mut seen = vec![role];
            while !queue.is_empty() {
                let (next, reversed) = queue.remove(0);
                if seen.contains(&next) {
                    continue;
                }
                seen.push(next);
                if let Some(clip) = own.get(&next) {
                    stand_in = Some((next, clip.clone(), reversed));
                    break;
                }
                queue.extend(
                    next.fallbacks()
                        .iter()
                        .map(|(r, rev)| (*r, *rev != reversed)),
                );
            }
            // An optional role is polish over another, never a gap to report.
            if !role.optional() {
                missing.push((role, stand_in.as_ref().map(|(r, _, _)| *r)));
            }
            if let Some((_, clip, reversed)) = stand_in {
                clips.insert(role, (clip, reversed));
            }
        }
        let own = own.keys().copied().collect();
        let mut unhonoured = Vec::new();
        for (key, clip) in animations {
            match Role::from_name(key) {
                None => unhonoured.push(format!(
                    "`animations` names `{key}`, which is no animation role, so its clip `{clip}` never plays"
                )),
                Some(role) if !has(&clip.to_ascii_lowercase()) => unhonoured.push(format!(
                    "`animations` maps `{}` to the clip `{clip}`, which the body does not have",
                    role.name()
                )),
                Some(_) => {}
            }
        }
        Self {
            clips,
            missing,
            own,
            unhonoured,
        }
    }
    /// Whether the body names a clip of its own for `role`.
    pub fn has_own(&self, role: Role) -> bool {
        self.own.contains(&role)
    }
    /// The clip (lower case) playing `role`, and whether it plays reversed.
    pub fn clip(&self, role: Role) -> Option<(&str, bool)> {
        self.clips.get(&role).map(|(c, r)| (c.as_str(), *r))
    }
    /// One line per role the body has no clip of its own for.
    pub fn report(&self) -> Vec<String> {
        self.unhonoured
            .iter()
            .cloned()
            .chain(self.missing.iter().map(|(role, stand_in)| match stand_in {
                Some(s) => format!(
                    "no `{}` animation; it plays its `{}` instead",
                    role.name(),
                    s.name()
                ),
                None => format!("no `{}` animation; {}", role.name(), role.cost()),
            }))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ragdoll() -> Ragdoll {
        serde_json::from_value(serde_json::json!({
            "bones": [
                {"node": "pelvis", "mass": 0.2,
                 "shapes": [{"kind": "capsule", "radius": 0.12, "half_length": 0.1, "axis": "y"}]},
                {"node": "shin", "mass": 0.1, "linear_damping": 1.0,
                 "shapes": [{"kind": "box", "half": [0.05, 0.2, 0.05], "offset": [0.0, -0.2, 0.0]}],
                 "joint": {"swing": 0.3, "twist": 0.2, "damping": 2.0}}
            ],
            "friction": 0.8, "max_speed": 40.0, "max_spin": 10.0, "freeze": 10.0,
            "kick": {"impulse": 10.0, "speed": 2.0, "up_kick": 0.1},
            "carry": [0.6, 1.0], "falling": 0.2, "inertia": 4.0, "spin_scale": 13.0, "prone_scale": 0.35,
            "random_spin": 9.0
        }))
        .unwrap()
    }

    #[test]
    fn a_ragdoll_names_a_root_and_jointed_bones_within_limits() {
        assert!(ragdoll().validate().is_ok());
        let mut bad = ragdoll();
        bad.bones[1].joint = None;
        assert!(bad.validate().is_err(), "a second bone with no joint");
        let mut bad = ragdoll();
        bad.bones[0].joint = bad.bones[1].joint.clone();
        assert!(bad.validate().is_err(), "a root with a joint");
        let mut bad = ragdoll();
        bad.bones[1].node = "pelvis".into();
        assert!(bad.validate().is_err(), "a bone named twice");
        let mut bad = ragdoll();
        bad.max_speed = MAX_RAGDOLL_SPEED + 1.0;
        assert!(bad.validate().is_err(), "too fast");
        let mut bad = ragdoll();
        bad.bones[0].shapes = vec![RagdollShape::Ball {
            radius: 0.0,
            offset: [0.0; 3],
        }];
        assert!(bad.validate().is_err(), "a shape too small to hold");
        let mut bad = ragdoll();
        bad.bones[1].joint.as_mut().unwrap().hinge = Some([1.0, 0.5]);
        assert!(bad.validate().is_err(), "a hinge running backward");
        let mut bad = ragdoll();
        bad.bones = Vec::new();
        assert!(bad.validate().is_err(), "no bones");
        let mut ok = ragdoll();
        ok.bones[0].shapes = vec![
            RagdollShape::Limb {
                radius: 0.1,
                length: None,
            },
            RagdollShape::Box {
                half: [0.1; 3],
                offset: [0.0; 3],
                rotation: [0.5, 0.0, 0.0],
            },
        ];
        ok.bones[1].joint.as_mut().unwrap().axis = Some('y');
        ok.bones[1].joint.as_mut().unwrap().twist_damping = Some(3.0);
        assert!(
            ok.validate().is_ok(),
            "a limb, a turned box, a hinge about y"
        );
        let mut bad = ok.clone();
        bad.bones[1].joint.as_mut().unwrap().axis = Some('w');
        assert!(bad.validate().is_err(), "no such axis");
        let mut bad = ok.clone();
        bad.bones[0].shapes = vec![RagdollShape::Limb {
            radius: 0.0,
            length: None,
        }];
        assert!(bad.validate().is_err(), "a limb too thin");
        let typo = serde_json::json!({"bones": [], "friction": 1.0, "max_speed": 1.0, "max_spin": 1.0, "typo": 1});
        assert!(serde_json::from_value::<Ragdoll>(typo).is_err());
    }

    #[test]
    fn a_death_takes_its_sides_clip_else_any_else_the_role() {
        let deaths = Deaths {
            front: vec!["Die_Back".into()],
            any: vec!["die1".into(), "die2".into(), "missing".into()],
            ..Default::default()
        };
        let has = |c: &str| matches!(c, "die_back" | "die1" | "die2");
        assert_eq!(
            deaths
                .pick(DeathPosture::Standing, Some(Side::Front), 7, has)
                .as_deref(),
            Some("die_back")
        );
        assert_eq!(
            deaths
                .pick(DeathPosture::Standing, Some(Side::Left), 0, has)
                .as_deref(),
            Some("die1")
        );
        assert_eq!(
            deaths
                .pick(DeathPosture::Standing, Some(Side::Left), 1, has)
                .as_deref(),
            Some("die2")
        );
        assert_eq!(
            deaths.pick(DeathPosture::Standing, None, 3, has).as_deref(),
            Some("die2")
        );
        assert_eq!(
            Deaths::default().pick(DeathPosture::Standing, None, 3, has),
            None
        );
        // A crouched or prone body plays only its own posture's clips, never a standing one.
        let crouch = Deaths {
            crouched: vec!["crh_kill".into()],
            prone: vec!["prn_kill".into()],
            ..deaths.clone()
        };
        let has_all = |c: &str| matches!(c, "die_back" | "die1" | "die2" | "crh_kill" | "prn_kill");
        assert_eq!(
            crouch
                .pick(DeathPosture::Crouched, Some(Side::Front), 5, has_all)
                .as_deref(),
            Some("crh_kill")
        );
        assert_eq!(
            crouch
                .pick(DeathPosture::Prone, None, 0, has_all)
                .as_deref(),
            Some("prn_kill")
        );
        let standing_only = |c: &str| matches!(c, "die1" | "die2" | "die_back");
        assert_eq!(
            crouch.pick(DeathPosture::Prone, None, 0, standing_only),
            None,
            "no clip for the posture plays the death role, not a standing one"
        );
        assert!(
            Deaths {
                any: vec!["x".into(); 9],
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn the_blockhead_resolves_every_role_to_its_own_clip() {
        let all: Vec<_> = Role::ALL.map(Role::blockhead_clip).to_vec();
        let roles = RoleClips::resolve(&Animations::new(), |c| all.contains(&c));
        assert!(roles.missing.is_empty());
        for role in Role::ALL {
            assert_eq!(roles.clip(role), Some((role.blockhead_clip(), false)));
            assert_eq!(Role::from_blockhead_clip(role.blockhead_clip()), Some(role));
        }
    }

    #[test]
    fn a_body_with_two_clips_falls_back_and_names_what_it_lacks() {
        let animations: Animations = [
            ("idle".into(), "Stand".into()),
            ("run".into(), "Walk".into()),
        ]
        .into();
        let roles = RoleClips::resolve(&animations, |c| matches!(c, "stand" | "walk"));
        assert_eq!(roles.clip(Role::Idle), Some(("stand", false)));
        assert_eq!(roles.clip(Role::Back), Some(("walk", true)), "walks back");
        assert_eq!(roles.clip(Role::Side), Some(("walk", false)));
        assert_eq!(roles.clip(Role::Fall), Some(("stand", false)));
        assert_eq!(roles.clip(Role::CrouchBack), Some(("stand", false)));
        assert_eq!(roles.clip(Role::AimRight), None, "skipped");
        assert_eq!(roles.clip(Role::Look), None);
        let report = roles.report();
        assert!(report.contains(&"no `sit` animation; it plays its `idle` instead".to_string()));
        assert!(
            report.contains(
                &"no `aim_right` animation; its arm does not raise a held item".to_string()
            )
        );
        // Not reported: the two roles it names and every optional role.
        let optional = Role::ALL.into_iter().filter(|r| r.optional()).count();
        assert_eq!(report.len(), Role::ALL.len() - 2 - optional);
        assert_eq!(
            roles.clip(Role::SideRight),
            Some(("walk", true)),
            "reversed"
        );
        assert_eq!(roles.clip(Role::CrouchSideRight), Some(("stand", true)));
        assert!(!roles.has_own(Role::SideRight) && roles.has_own(Role::Run));
    }

    #[test]
    fn a_body_with_no_clips_resolves_nothing_and_says_so() {
        let roles = RoleClips::resolve(&Animations::new(), |_| false);
        assert!(Role::ALL.into_iter().all(|r| roles.clip(r).is_none()));
        let optional = Role::ALL.into_iter().filter(|r| r.optional()).count();
        assert_eq!(roles.report().len(), Role::ALL.len() - optional);
    }

    #[test]
    fn sockets_default_to_the_blockhead_nodes() {
        let none = Sockets::default();
        assert_eq!(none.hand(0), Some(Socket::node("Mount0")));
        assert_eq!(none.hand(1), Some(Socket::node("Mount1")));
        assert_eq!(none.hand(2), None);
        assert_eq!(none.eye(), Socket::node("Eye"));
        let runner: Sockets =
            serde_json::from_str(r#"{"right_hand": {"offset": [0.5, 0.7, -0.2]}}"#).unwrap();
        runner.validate().unwrap();
        assert_eq!(runner.hand(0), Some(Socket::at([0.5, 0.7, -0.2])));
        assert_eq!(runner.hand(1), Some(Socket::node("Mount1")));
        assert!(serde_json::from_str::<Sockets>(r#"{"tail": {}}"#).is_err());
        let bad = Sockets {
            eye: Some(Socket::at([f32::NAN, 0.0, 0.0])),
            ..Sockets::default()
        };
        assert!(bad.validate().is_err());
        assert!(validate_animations(&[("dance".into(), "x".into())].into()).is_err());
    }

    #[test]
    fn a_role_mapped_to_a_clip_the_body_lacks_is_named_even_when_optional() {
        // `look_prone` is optional (a body without one is not reported), but
        // an archetype that names a clip for it and has none says so.
        let animations: Animations = [("look_prone".to_string(), "Look_Prone".to_string())].into();
        let roles = RoleClips::resolve(&animations, |clip| clip == "root");
        let lines = roles.report();
        assert!(
            lines
                .iter()
                .any(|l| l.contains("look_prone") && l.contains("Look_Prone")),
            "{lines:?}"
        );
        // Naming a clip it does have is not a gap.
        let roles = RoleClips::resolve(&animations, |clip| clip == "look_prone");
        assert!(roles.unhonoured.is_empty());
        // Nothing named, nothing to name.
        let roles = RoleClips::resolve(&Animations::new(), |_| false);
        assert!(roles.unhonoured.is_empty());
    }
}
