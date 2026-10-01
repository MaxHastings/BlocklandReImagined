//! Path cameras (Torque's `PathCamera` with a `PathCameraData`): a camera
//! that flies through a list of knots, each with its own speed, along a
//! spline or straight lines, turning from one knot's view to the next. A
//! rule hands a player one (`follow_path`); the path and the tick it
//! started replicate in [`super::Vitals`], so each client flies it on its
//! own clock and the server knows which knot was reached when.
use super::CameraView;
use anyhow::{Result, ensure};
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Most knots one path holds (`PathCameraData.maxNodes`; Torque's path
/// cameras only work with up to 20).
pub const MAX_KNOTS: usize = 20;
/// Slowest and fastest knot speed, units per second.
pub const SPEEDS: std::ops::RangeInclusive<f32> = 0.1..=1000.0;
const TICKS_PER_SECOND: f64 = 120.0;
/// Steps a spline segment's length is measured in.
const LENGTH_STEPS: usize = 16;

/// How a knot shapes the path through it (`pushBack`'s type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnotKind {
    /// The spline passes smoothly through it, turning to its view.
    #[default]
    Normal,
    /// A sharp corner: the spline does not curve through it.
    Kink,
    /// Only its position counts: the view turns on toward the next knot
    /// that has one (`Position Only`).
    PositionOnly,
}

/// One knot (`PathCamera::pushBack(transform, speed, type, path)`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knot {
    pub view: CameraView,
    /// Units per second flying from here to the next knot.
    pub speed: f32,
    #[serde(default)]
    pub kind: KnotKind,
    /// Straight to the next knot (`Linear`) rather than along the spline.
    #[serde(default)]
    pub linear: bool,
    /// The camera cuts to this knot instead of flying to it (Slayer's
    /// `/setJump`).
    #[serde(default)]
    pub jump: bool,
}

/// A path a player's camera flies, from its first knot at `start_tick`.
/// It stops at the last knot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraPath {
    pub knots: Vec<Knot>,
    pub start_tick: u64,
}

impl CameraPath {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=MAX_KNOTS).contains(&self.knots.len()),
            "A camera path has 1 to {MAX_KNOTS} knots"
        );
        for knot in &self.knots {
            knot.view.validate()?;
            ensure!(
                SPEEDS.contains(&knot.speed),
                "A knot's speed is 0.1 to 1000 units a second"
            );
        }
        Ok(())
    }

    /// Seconds from each knot to the next, at the average of their speeds
    /// over the path's length between them; zero to a jump.
    pub fn legs(&self) -> Vec<f64> {
        (0..self.knots.len().saturating_sub(1))
            .map(|i| {
                let next = &self.knots[i + 1];
                if next.jump {
                    return 0.0;
                }
                let speed = f64::from((self.knots[i].speed + next.speed) * 0.5);
                f64::from(self.length(i)) / speed
            })
            .collect()
    }

    /// Ticks from the first knot to the last.
    pub fn duration_ticks(&self) -> u64 {
        (self.legs().iter().sum::<f64>() * TICKS_PER_SECOND).ceil() as u64
    }

    /// The last knot reached by `tick` (the first from the start).
    pub fn reached(&self, tick: u64) -> usize {
        let mut at = (tick.saturating_sub(self.start_tick)) as f64 / TICKS_PER_SECOND;
        let mut knot = 0;
        for leg in self.legs() {
            if at < leg {
                break;
            }
            at -= leg;
            knot += 1;
        }
        knot
    }

    /// Where the camera is and looks at `tick` (fractional for a client's
    /// smooth clock).
    pub fn sample(&self, tick: f64) -> CameraView {
        let mut at = (tick - self.start_tick as f64).max(0.0) / TICKS_PER_SECOND;
        let legs = self.legs();
        for (i, leg) in legs.iter().enumerate() {
            if at < *leg {
                return self.between(i, (at / leg) as f32);
            }
            at -= leg;
        }
        self.view_at(self.knots.len() - 1)
    }

    fn position(&self, i: usize) -> Vec3 {
        self.knots[i].view.eye()
    }

    /// Position on the leg from knot `i` to `i + 1`, `u` from 0 to 1: a
    /// Catmull-Rom spline through the knots around it, straight beside a
    /// kink or on a linear leg.
    fn point(&self, i: usize, u: f32) -> Vec3 {
        let (p1, p2) = (self.position(i), self.position(i + 1));
        if self.knots[i].linear {
            return p1.lerp(p2, u);
        }
        let p0 = if i == 0 || self.knots[i].kind == KnotKind::Kink {
            p1
        } else {
            self.position(i - 1)
        };
        let p3 = if i + 2 >= self.knots.len() || self.knots[i + 1].kind == KnotKind::Kink {
            p2
        } else {
            self.position(i + 2)
        };
        let (u2, u3) = (u * u, u * u * u);
        0.5 * (2.0 * p1
            + (p2 - p0) * u
            + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2
            + (3.0 * p1 - p0 - 3.0 * p2 + p3) * u3)
    }

    fn length(&self, i: usize) -> f32 {
        if self.knots[i].linear {
            return self.position(i).distance(self.position(i + 1)).max(0.01);
        }
        let mut length = 0.0;
        let mut last = self.point(i, 0.0);
        for step in 1..=LENGTH_STEPS {
            let p = self.point(i, step as f32 / LENGTH_STEPS as f32);
            length += last.distance(p);
            last = p;
        }
        length.max(0.01)
    }

    /// The view a knot turns the camera to: its own, or for a
    /// position-only knot the next one's that has a view (the last
    /// before it at the end of the path).
    fn view_at(&self, i: usize) -> CameraView {
        let pick = |k: &Knot| k.kind != KnotKind::PositionOnly;
        let view = self.knots[i..]
            .iter()
            .find(|k| pick(k))
            .or_else(|| self.knots[..i].iter().rev().find(|k| pick(k)))
            .unwrap_or(&self.knots[i])
            .view;
        CameraView {
            eye: self.knots[i].view.eye,
            ..view
        }
    }

    fn between(&self, i: usize, u: f32) -> CameraView {
        let (a, b) = (self.view_at(i), self.view_at(i + 1));
        let turn = (b.yaw - a.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        CameraView {
            eye: self.point(i, u).to_array(),
            yaw: a.yaw + turn * u,
            pitch: a.pitch + (b.pitch - a.pitch) * u,
        }
    }
}

impl Knot {
    pub(super) fn from_op(k: &bri_package_runtime::ops::PathKnot) -> Self {
        use bri_package_runtime::ops::KnotKind as Op;
        Self {
            view: CameraView {
                eye: k.at,
                yaw: k.yaw,
                pitch: k.pitch,
            },
            speed: k.speed,
            kind: match k.kind {
                Op::Normal => KnotKind::Normal,
                Op::Kink => KnotKind::Kink,
                Op::PositionOnly => KnotKind::PositionOnly,
            },
            linear: k.linear,
            jump: k.jump,
        }
    }
}

/// A path a rule gave a player: whose rule, and the last knot it heard of.
#[derive(Debug, Clone)]
pub(super) struct Following {
    pub package: String,
    pub path: CameraPath,
    pub heard: Option<usize>,
}

impl super::Session {
    /// Where `owner`'s control object is and looks
    /// (`getControlObject().getTransform()`): the path camera's place now,
    /// the free camera's last report, or the body's eye.
    pub(super) fn control_view(&self, p: &super::Peer) -> CameraView {
        let tick = self.simulation.state().tick;
        match (p.control, &p.path, p.camera) {
            (super::ControlObject::Path, Some(f), _) => f.path.sample(tick as f64),
            (super::ControlObject::Camera, _, Some(camera)) => camera,
            _ => {
                let state = p.player.state();
                CameraView {
                    eye: p.player.eye().to_array(),
                    yaw: state.yaw,
                    pitch: state.pitch,
                }
            }
        }
    }

    /// A rule's `follow_path`: fly `owner`'s camera along `knots` from this
    /// tick, or with `None` hand control back. Admin cameras and driven
    /// entities are left alone, as `watch` leaves them.
    pub(super) fn follow_path(
        &mut self,
        package: &str,
        owner: bri_world::OwnerId,
        knots: Option<Vec<Knot>>,
    ) -> Result<()> {
        let tick = self.simulation.state().tick;
        let peer = self
            .peers
            .get_mut(&owner)
            .ok_or_else(|| anyhow::anyhow!("No such player"))?;
        let Some(knots) = knots else {
            peer.path = None;
            if peer.control == super::ControlObject::Path {
                self.return_to_body(owner)?;
            }
            return Ok(());
        };
        ensure!(
            matches!(
                peer.control,
                super::ControlObject::Player
                    | super::ControlObject::Corpse
                    | super::ControlObject::Spy(_)
                    | super::ControlObject::Path
            ),
            "That player is controlling something else"
        );
        let path = CameraPath {
            knots,
            start_tick: tick,
        };
        path.validate()?;
        peer.path = Some(Following {
            package: package.to_owned(),
            path,
            heard: None,
        });
        peer.control = super::ControlObject::Path;
        Ok(())
    }

    /// Each knot the path cameras reached since the last tick, with the
    /// package whose rules gave the path, for their `on_path_node`. A path
    /// whose camera lost control (a death, an admin's camera) is dropped.
    pub(super) fn knots_reached(&mut self) -> Vec<(String, bri_world::OwnerId, usize)> {
        let tick = self.simulation.state().tick;
        let mut reached = Vec::new();
        for (owner, peer) in &mut self.peers {
            if peer.control != super::ControlObject::Path {
                peer.path = None;
                continue;
            }
            let Some(following) = &mut peer.path else {
                continue;
            };
            let now = following.path.reached(tick);
            let from = following.heard.map_or(0, |k| k + 1);
            for knot in from..=now {
                reached.push((following.package.clone(), *owner, knot));
            }
            following.heard = Some(now);
        }
        reached
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn knot(x: f32, yaw: f32, speed: f32) -> Knot {
        Knot {
            view: CameraView {
                eye: [x, 2.0, 0.0],
                yaw,
                pitch: 0.0,
            },
            speed,
            kind: KnotKind::Normal,
            linear: true,
            jump: false,
        }
    }

    #[test]
    fn a_linear_path_flies_each_leg_at_its_knots_speed() {
        let path = CameraPath {
            knots: vec![knot(0.0, 0.0, 10.0), knot(10.0, 1.0, 10.0), knot(30.0, 1.0, 30.0)],
            start_tick: 100,
        };
        path.validate().unwrap();
        // 10 units at 10/s, then 20 units at 20/s: a second each.
        assert_eq!(path.legs(), vec![1.0, 1.0]);
        assert_eq!(path.duration_ticks(), 240);
        assert_eq!(path.reached(100), 0);
        assert_eq!(path.reached(219), 0);
        assert_eq!(path.reached(220), 1);
        assert_eq!(path.reached(1000), 2);
        let half = path.sample(160.0);
        assert!((half.eye[0] - 5.0).abs() < 1e-4 && (half.yaw - 0.5).abs() < 1e-4);
        assert_eq!(path.sample(5000.0).eye, [30.0, 2.0, 0.0]);
        assert_eq!(path.sample(0.0).eye, [0.0, 2.0, 0.0], "waits at the start");
    }

    #[test]
    fn a_spline_passes_through_its_knots_and_a_jump_cuts() {
        let mut knots = vec![knot(0.0, 0.0, 5.0), knot(5.0, 0.0, 5.0), knot(5.0, 0.0, 5.0)];
        for k in &mut knots {
            k.linear = false;
        }
        knots[2].view.eye = [10.0, 2.0, 5.0];
        let path = CameraPath { knots: knots.clone(), start_tick: 0 };
        let legs = path.legs();
        let at_knot = path.sample(legs[0] * 120.0);
        assert!(Vec3::from(at_knot.eye).distance(Vec3::new(5.0, 2.0, 0.0)) < 1e-3);
        // The curve is longer than the chord.
        assert!(legs[1] * 5.0 > f64::from(Vec3::new(5.0, 0.0, 5.0).length()));

        knots[2].jump = true;
        let path = CameraPath { knots, start_tick: 0 };
        assert_eq!(path.legs()[1], 0.0);
        assert_eq!(path.reached((legs[0] * 120.0).ceil() as u64), 2);
    }

    #[test]
    fn a_position_only_knot_turns_toward_the_next_view() {
        let mut knots = vec![knot(0.0, 0.0, 10.0), knot(10.0, 3.0, 10.0), knot(20.0, 1.0, 10.0)];
        knots[1].kind = KnotKind::PositionOnly;
        let path = CameraPath { knots, start_tick: 0 };
        // Its own yaw (3.0) is ignored: halfway to it is halfway to 1.0.
        assert!((path.sample(60.0).yaw - 0.5).abs() < 1e-4);
    }
}
