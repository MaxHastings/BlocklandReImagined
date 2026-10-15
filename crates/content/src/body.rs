//! What a body offers the things that attach to it and animate it, so any
//! independently authored body holds any item, sits in any seat and plays
//! its own clips. A body names its sockets (hands, eye, seat) and maps
//! semantic animation roles to its own clip names; whatever it leaves out
//! defaults to the Blockhead's names, so the Blockhead needs neither.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    Jump,
    Fall,
    Crouch,
    CrouchRun,
    CrouchBack,
    CrouchSide,
    AimRight,
    AimLeft,
    AimBoth,
    Sit,
    Death,
    Swim,
    Look,
    HeadSide,
    HeadUp,
    SeatAim,
}
impl Role {
    pub const ALL: [Role; 20] = [
        Role::Idle,
        Role::Run,
        Role::Back,
        Role::Side,
        Role::Jump,
        Role::Fall,
        Role::Crouch,
        Role::CrouchRun,
        Role::CrouchBack,
        Role::CrouchSide,
        Role::AimRight,
        Role::AimLeft,
        Role::AimBoth,
        Role::Sit,
        Role::Death,
        Role::Swim,
        Role::Look,
        Role::HeadSide,
        Role::HeadUp,
        Role::SeatAim,
    ];
    /// The role's name in an archetype's `animations`.
    pub fn name(self) -> &'static str {
        match self {
            Role::Idle => "idle",
            Role::Run => "run",
            Role::Back => "back",
            Role::Side => "side",
            Role::Jump => "jump",
            Role::Fall => "fall",
            Role::Crouch => "crouch",
            Role::CrouchRun => "crouch_run",
            Role::CrouchBack => "crouch_back",
            Role::CrouchSide => "crouch_side",
            Role::AimRight => "aim_right",
            Role::AimLeft => "aim_left",
            Role::AimBoth => "aim_both",
            Role::Sit => "sit",
            Role::Death => "death",
            Role::Swim => "swim",
            Role::Look => "look",
            Role::HeadSide => "head_side",
            Role::HeadUp => "head_up",
            Role::SeatAim => "seat_aim",
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
            Role::Jump => "jump",
            Role::Fall => "fall",
            Role::Crouch => "crouch",
            Role::CrouchRun => "crouchrun",
            Role::CrouchBack => "crouchback",
            Role::CrouchSide => "crouchside",
            Role::AimRight => "armreadyright",
            Role::AimLeft => "armreadyleft",
            Role::AimBoth => "armreadyboth",
            Role::Sit => "sit",
            Role::Death => "death1",
            Role::Swim => "swim",
            Role::Look => "look",
            Role::HeadSide => "headside",
            Role::HeadUp => "headup",
            Role::SeatAim => "seataim",
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
            Role::Jump => &[(Role::Idle, false)],
            Role::Fall => &[(Role::Jump, false), (Role::Idle, false)],
            Role::Crouch => &[(Role::Idle, false)],
            Role::CrouchRun => &[(Role::Crouch, false), (Role::Run, false)],
            Role::CrouchBack => &[(Role::Crouch, false), (Role::Back, false)],
            Role::CrouchSide => &[(Role::Crouch, false), (Role::Side, false)],
            Role::AimRight => &[],
            Role::AimLeft => &[],
            Role::AimBoth => &[(Role::AimRight, false)],
            Role::Sit => &[(Role::Idle, false)],
            Role::Death => &[(Role::Idle, false)],
            Role::Swim => &[],
            Role::Look => &[],
            Role::HeadSide => &[],
            Role::HeadUp => &[],
            Role::SeatAim => &[],
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
            _ => None,
        }
    }
    /// What the body loses when the role has no clip and nothing stands in.
    pub fn cost(self) -> &'static str {
        match self {
            Role::Idle => "it stands in its bind pose",
            Role::AimRight | Role::AimLeft => "its arm does not raise a held item",
            Role::Look => "it does not look up and down",
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

/// An archetype's `ground_speeds`: locomotion role name to the speed, in
/// units per second, at which the body's clip for it plays at 1x.
pub type GroundSpeeds = BTreeMap<String, f32>;

pub fn validate_ground_speeds(speeds: &GroundSpeeds) -> Result<()> {
    for (role, speed) in speeds {
        ensure!(
            Role::from_name(role).is_some_and(|r| r.ground_axis().is_some()),
            "ground speed role `{role}` must be one of run, back, side, crouch_run, crouch_back or crouch_side"
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
            missing.push((role, stand_in.as_ref().map(|(r, _, _)| *r)));
            if let Some((_, clip, reversed)) = stand_in {
                clips.insert(role, (clip, reversed));
            }
        }
        Self { clips, missing }
    }
    /// The clip (lower case) playing `role`, and whether it plays reversed.
    pub fn clip(&self, role: Role) -> Option<(&str, bool)> {
        self.clips.get(&role).map(|(c, r)| (c.as_str(), *r))
    }
    /// One line per role the body has no clip of its own for.
    pub fn report(&self) -> Vec<String> {
        self.missing
            .iter()
            .map(|(role, stand_in)| match stand_in {
                Some(s) => format!(
                    "no `{}` animation; it plays its `{}` instead",
                    role.name(),
                    s.name()
                ),
                None => format!("no `{}` animation; {}", role.name(), role.cost()),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(report.len(), Role::ALL.len() - 2);
    }

    #[test]
    fn a_body_with_no_clips_resolves_nothing_and_says_so() {
        let roles = RoleClips::resolve(&Animations::new(), |_| false);
        assert!(Role::ALL.into_iter().all(|r| roles.clip(r).is_none()));
        assert_eq!(roles.report().len(), Role::ALL.len());
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
}
