//! How a body's camera feels: its field of view, where the first-person
//! camera sits, how it bobs as the body walks, how a held weapon sways
//! behind the look, and where the third-person camera hangs. A body
//! declares this in its archetype (`camera`); while the body is active its
//! profile owns the on-foot camera, and switching to another body hands the
//! camera to that body's profile (or to the stock camera). Everything left
//! out is the stock Blockhead camera, exactly: a profile of all defaults
//! changes nothing.
//!
//! The math here is pure and deterministic in the times it is given, so the
//! client's camera and tests agree.
use anyhow::{Result, ensure};
use bri_console::Clamp;
use serde::{Deserialize, Serialize};

/// The stock horizontal field of view a profile's `fov` is written against.
pub const STOCK_FOV: f32 = 90.0;

/// A body's camera. See the module docs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CameraProfile {
    /// Horizontal field of view, in degrees, for a player who keeps the
    /// stock 90. A player who set their own FOV gets the same ratio (a 70
    /// body seen at a 110 setting shows 85.6), so the body's feel and the
    /// player's screen both count. Aiming and zoom still own the FOV while
    /// they run, and a rule's `set_fov` still wins over the body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fov: Option<f32>,
    /// Where the first-person camera sits relative to the eye, in body units
    /// at scale 1: right, up and forward along the look's heading. Only the
    /// camera moves; aim and shots keep the body's `eye` socket.
    #[serde(skip_serializing_if = "is_zero")]
    pub eye_offset: [f32; 3],
    /// First-person view bob while walking on the ground.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bob: Option<Bob>,
    /// First-person held weapon sway: the weapon trails the look and settles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sway: Option<Sway>,
    /// The on-foot third-person camera's pivot: over a shoulder and higher
    /// or lower. Its distance is the archetype's `camera_distance`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub third_person: Option<ThirdPerson>,
}
fn is_zero(v: &[f32; 3]) -> bool {
    *v == [0.0; 3]
}

/// View bob: one cycle is two footfalls.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bob {
    /// How far the camera dips at each footfall, units.
    #[serde(default)]
    pub height: f32,
    /// How far it rocks side to side over a cycle, units.
    #[serde(default)]
    pub side: f32,
    /// Ground covered in one cycle (two steps), units.
    #[serde(default = "stride")]
    pub stride: f32,
}
fn stride() -> f32 {
    2.4
}
impl Bob {
    /// (right, up) at `phase` through a cycle and strength `gain`: it dips
    /// at each footfall and rocks once a cycle.
    fn offset(&self, phase: f32, gain: f32) -> (f32, f32) {
        let turn = phase * std::f32::consts::TAU;
        let side = self.side * turn.sin() * gain;
        let dip = -self.height * (0.5 - 0.5 * (2.0 * turn).cos()) * gain;
        (side, dip)
    }
}

/// Weapon sway in first person.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sway {
    /// How much of each turn the weapon trails behind, 0 to 1 (a radian
    /// of turn leaves it `lag` radians behind, up to `max_degrees`).
    #[serde(default)]
    pub lag: f32,
    /// The furthest it trails, degrees.
    #[serde(default = "max_degrees")]
    pub max_degrees: f32,
    /// How fast it settles back, per second (the lag left after one
    /// second is `exp(-settle)`).
    #[serde(default = "settle")]
    pub settle: f32,
    /// How much the weapon bobs against the camera as the body walks, as a
    /// share of the camera's bob (0 keeps it still on screen).
    #[serde(default)]
    pub bob: f32,
    /// The weapon's own bob on screen while walking, whatever the camera
    /// does: it dips `height` at each footfall and swings `side` across a
    /// cycle of `stride` units, in view space. With a faint or no camera
    /// `bob` this moves the gun while the view barely does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walk: Option<Bob>,
    /// The weapon shifting against a fast turn of the look, in view space:
    /// `side` units to the left at a turn to the right of `rate` radians a
    /// second (and the reverse), `up` units down at a pitch up at that rate.
    /// Smaller turns shift it in proportion, and it settles at `settle`.
    /// This moves the gun on screen rather than turning it behind the look
    /// like `lag`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<LookShift>,
}
/// [`Sway::look`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookShift {
    /// Units across at the full turn rate, within 0.5.
    #[serde(default)]
    pub side: f32,
    /// Units up and down at the full pitch rate, within 0.5.
    #[serde(default)]
    pub up: f32,
    /// The turn rate that gives the full shift, radians a second, 0.1 to 20.
    pub rate: f32,
}
fn max_degrees() -> f32 {
    5.0
}
fn settle() -> f32 {
    8.0
}

/// The third-person pivot, at scale 1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThirdPerson {
    /// Right of the body's middle (negative is the left shoulder), units.
    pub side: f32,
    /// Above the stock pivot, units.
    pub height: f32,
}

impl CameraProfile {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn validate(&self) -> Result<()> {
        let finite = |v: f32, max: f32| v.is_finite() && v.abs() <= max;
        ensure!(
            self.fov
                .is_none_or(|f| f.is_finite() && (30.0..=140.0).contains(&f)),
            "camera fov must be 30 to 140 degrees"
        );
        ensure!(
            self.eye_offset.iter().all(|v| finite(*v, 2.0)),
            "camera eye_offset must be finite and within 2 units"
        );
        if let Some(b) = &self.bob {
            ensure!(
                finite(b.height, 0.5)
                    && finite(b.side, 0.5)
                    && b.stride.is_finite()
                    && (0.2..=20.0).contains(&b.stride),
                "camera bob needs height and side within 0.5 units and a stride of 0.2 to 20"
            );
        }
        if let Some(s) = &self.sway {
            ensure!(
                s.lag.is_finite()
                    && (0.0..=1.0).contains(&s.lag)
                    && s.max_degrees.is_finite()
                    && (0.0..=30.0).contains(&s.max_degrees)
                    && s.settle.is_finite()
                    && (0.1..=60.0).contains(&s.settle)
                    && finite(s.bob, 4.0),
                "camera sway needs lag 0 to 1, max_degrees 0 to 30, settle 0.1 to 60 and bob within 4"
            );
            if let Some(l) = &s.look {
                ensure!(
                    finite(l.side, 0.5)
                        && finite(l.up, 0.5)
                        && l.rate.is_finite()
                        && (0.1..=20.0).contains(&l.rate),
                    "camera sway look needs side and up within 0.5 units and a rate of 0.1 to 20 radians a second"
                );
            }
            if let Some(w) = &s.walk {
                ensure!(
                    finite(w.height, 0.5)
                        && finite(w.side, 0.5)
                        && w.stride.is_finite()
                        && (0.2..=20.0).contains(&w.stride),
                    "camera sway walk needs height and side within 0.5 units and a stride of 0.2 to 20"
                );
            }
        }
        if let Some(t) = &self.third_person {
            ensure!(
                finite(t.side, 4.0) && finite(t.height, 4.0),
                "camera third_person side and height must be within 4 units"
            );
        }
        Ok(())
    }
    /// The field of view this body shows for a player whose own normal FOV
    /// is `player_fov`.
    pub fn fov_for(&self, player_fov: f32) -> f32 {
        match self.fov {
            Some(fov) => (player_fov * fov / STOCK_FOV).clamped(5.0, 140.0),
            None => player_fov,
        }
    }
}

/// The running state of a body's first-person camera: bob phase and
/// strength, and the weapon's trail. Default is at rest.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CameraMotion {
    /// Through the current bob cycle, 0 to 1.
    pub phase: f32,
    /// Bob strength, easing toward 1 while walking on the ground and 0
    /// otherwise.
    pub gain: f32,
    /// The weapon's trail behind the look, radians (yaw, pitch).
    pub trail: (f32, f32),
    /// Through the weapon's own bob cycle (`Sway::walk`), 0 to 1.
    pub weapon_phase: f32,
    /// How far the look's turn rate has shifted the weapon (`Sway::look`),
    /// yaw and pitch, each -1 to 1.
    pub look: (f32, f32),
}
/// How fast the bob eases in and out, per second.
const BOB_EASE: f32 = 4.0;
/// Below this ground speed (units a second) the body is standing.
const STILL_SPEED: f32 = 0.5;
impl CameraMotion {
    /// Advance by `seconds` while moving at `ground_speed` (horizontal units
    /// a second), `grounded` or not, the look having turned by `turned`
    /// (yaw, pitch radians) since the last call.
    pub fn advance(
        &mut self,
        profile: &CameraProfile,
        seconds: f32,
        ground_speed: f32,
        grounded: bool,
        turned: (f32, f32),
    ) {
        if !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        let seconds = seconds.min(0.25);
        let walk = profile.sway.as_ref().and_then(|s| s.walk.as_ref());
        if profile.bob.is_some() || walk.is_some() {
            let walking = grounded && ground_speed.is_finite() && ground_speed > STILL_SPEED;
            let target = if walking { 1.0 } else { 0.0 };
            let step = BOB_EASE * seconds;
            self.gain += (target - self.gain).clamped(-step, step);
            if walking {
                let advance =
                    |phase: f32, stride: f32| (phase + ground_speed * seconds / stride).fract();
                if let Some(bob) = &profile.bob {
                    self.phase = advance(self.phase, bob.stride);
                }
                if let Some(walk) = walk {
                    self.weapon_phase = advance(self.weapon_phase, walk.stride);
                }
            }
        } else {
            *self = Self {
                trail: self.trail,
                look: self.look,
                ..Self::default()
            }
        }
        match &profile.sway {
            Some(sway) => {
                let max = sway.max_degrees.to_radians();
                let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
                let decay = (-sway.settle * seconds).exp();
                let trail = |now: f32, turn: f32| {
                    ((now - finite(turn) * sway.lag) * decay).clamped(-max, max)
                };
                self.trail = (trail(self.trail.0, turned.0), trail(self.trail.1, turned.1));
                self.look = match &sway.look {
                    Some(l) => {
                        let ease = 1.0 - decay;
                        let toward = |now: f32, turn: f32| {
                            let target = (finite(turn) / seconds / l.rate).clamped(-1.0, 1.0);
                            let shift = now + (target - now) * ease;
                            if shift.abs() < 1e-4 { 0.0 } else { shift }
                        };
                        (toward(self.look.0, turned.0), toward(self.look.1, turned.1))
                    }
                    None => (0.0, 0.0),
                };
            }
            None => {
                self.trail = (0.0, 0.0);
                self.look = (0.0, 0.0);
            }
        }
    }
    /// The bob's offset this moment: (right, up) in units at scale 1. The
    /// camera dips at each footfall and rocks once a cycle.
    pub fn bob(&self, profile: &CameraProfile) -> (f32, f32) {
        match &profile.bob {
            Some(bob) => bob.offset(self.phase, self.gain),
            None => (0.0, 0.0),
        }
    }
    /// The held weapon's bob against the view this moment: (right, up) in
    /// view-space units at scale 1. The camera's bob times `Sway::bob`,
    /// plus the weapon's own `Sway::walk`.
    pub fn weapon_bob(&self, profile: &CameraProfile) -> (f32, f32) {
        let Some(sway) = &profile.sway else {
            return (0.0, 0.0);
        };
        let (side, dip) = self.bob(profile);
        let (own_side, own_dip) = sway
            .walk
            .map_or((0.0, 0.0), |w| w.offset(self.weapon_phase, self.gain));
        let (look_side, look_up) = sway
            .look
            .map_or((0.0, 0.0), |l| (-self.look.0 * l.side, -self.look.1 * l.up));
        (
            side * sway.bob + own_side + look_side,
            dip * sway.bob + own_dip + look_up,
        )
    }
    /// At rest: no bob and no trail, as the stock camera.
    pub fn at_rest(&self) -> bool {
        self.gain == 0.0 && self.trail == (0.0, 0.0) && self.look == (0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_of_defaults_is_the_stock_camera() {
        let p: CameraProfile = serde_json::from_str("{}").unwrap();
        assert!(p.is_default());
        p.validate().unwrap();
        assert_eq!(p.fov_for(90.0), 90.0);
        assert_eq!(p.fov_for(110.0), 110.0);
        let mut m = CameraMotion::default();
        m.advance(&p, 0.1, 10.0, true, (0.3, 0.1));
        assert!(m.at_rest());
        assert_eq!(m.bob(&p), (0.0, 0.0));
        assert_eq!(serde_json::to_string(&p).unwrap(), "{}");
    }

    #[test]
    fn a_turn_shifts_the_weapon_against_it_and_it_settles() {
        let p = CameraProfile {
            sway: Some(Sway {
                lag: 0.0,
                max_degrees: 5.0,
                settle: 8.0,
                bob: 0.0,
                walk: None,
                look: Some(LookShift {
                    side: 0.04,
                    up: 0.03,
                    rate: 4.0,
                }),
            }),
            ..Default::default()
        };
        p.validate().unwrap();
        let mut m = CameraMotion::default();
        // A turn right at the full rate (4 rad/s) for a second, then none.
        for _ in 0..60 {
            m.advance(&p, 1.0 / 60.0, 0.0, true, (4.0 / 60.0, 0.0));
        }
        let (side, up) = m.weapon_bob(&p);
        assert!((side + 0.04).abs() < 1e-3, "left of centre at the full rate: {side}");
        assert!(up.abs() < 1e-6);
        // Half the rate gives half the shift; turning the other way reverses it.
        let mut half = CameraMotion::default();
        for _ in 0..60 {
            half.advance(&p, 1.0 / 60.0, 0.0, true, (-2.0 / 60.0, 1.0 / 60.0));
        }
        let (side, up) = half.weapon_bob(&p);
        assert!((side - 0.02).abs() < 1e-3, "{side}");
        assert!((up + 0.0075).abs() < 1e-3, "pitch at a quarter of the rate: {up}");
        // Settled: no turn brings it back to the middle.
        for _ in 0..120 {
            m.advance(&p, 1.0 / 60.0, 0.0, true, (0.0, 0.0));
        }
        assert!(m.at_rest(), "{m:?}");
        // A rate past the full one is clamped to it.
        let mut fast = CameraMotion::default();
        for _ in 0..120 {
            fast.advance(&p, 1.0 / 60.0, 0.0, true, (40.0 / 60.0, 0.0));
        }
        assert!((fast.weapon_bob(&p).0 + 0.04).abs() < 1e-3);
        let bad = CameraProfile {
            sway: Some(Sway {
                look: Some(LookShift {
                    side: 0.04,
                    up: 0.0,
                    rate: 0.0,
                }),
                ..p.sway.unwrap()
            }),
            ..Default::default()
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn the_body_fov_keeps_the_players_ratio() {
        let p = CameraProfile {
            fov: Some(70.0),
            ..Default::default()
        };
        assert_eq!(p.fov_for(90.0), 70.0);
        assert!((p.fov_for(110.0) - 85.555_55).abs() < 1e-3);
    }

    #[test]
    fn bob_eases_in_while_walking_and_out_when_standing() {
        let p = CameraProfile {
            bob: Some(Bob {
                height: 0.1,
                side: 0.05,
                stride: 2.0,
            }),
            ..Default::default()
        };
        let mut m = CameraMotion::default();
        for _ in 0..30 {
            m.advance(&p, 1.0 / 60.0, 6.0, true, (0.0, 0.0));
        }
        assert!((m.gain - 1.0).abs() < 1e-6, "{m:?}");
        let mut lowest: f32 = 0.0;
        for _ in 0..40 {
            m.advance(&p, 1.0 / 60.0, 6.0, true, (0.0, 0.0));
            let (side, dip) = m.bob(&p);
            assert!(side.abs() <= 0.05 + 1e-6 && (-0.1 - 1e-6..=1e-6).contains(&dip));
            lowest = lowest.min(dip);
        }
        assert!(lowest < -0.09, "it dips at a footfall: {lowest}");
        for _ in 0..30 {
            m.advance(&p, 1.0 / 60.0, 0.0, true, (0.0, 0.0));
        }
        assert_eq!(m.gain, 0.0);
        assert_eq!(m.bob(&p), (0.0, 0.0));
        // In the air the bob stops too.
        m.advance(&p, 0.1, 6.0, false, (0.0, 0.0));
        assert_eq!(m.gain, 0.0);
    }

    #[test]
    fn the_weapon_trails_a_turn_and_settles() {
        let p = CameraProfile {
            sway: Some(Sway {
                lag: 0.5,
                max_degrees: 4.0,
                settle: 8.0,
                bob: 0.0,
                walk: None,
                look: None,
            }),
            ..Default::default()
        };
        let mut m = CameraMotion::default();
        m.advance(&p, 1.0 / 60.0, 0.0, true, (0.05, 0.0));
        assert!(
            m.trail.0 < 0.0,
            "turning right leaves it behind, left: {m:?}"
        );
        m.advance(&p, 1.0 / 60.0, 0.0, true, (2.0, 0.0));
        assert!(
            (m.trail.0 + 4f32.to_radians()).abs() < 1e-6,
            "capped: {m:?}"
        );
        for _ in 0..240 {
            m.advance(&p, 1.0 / 60.0, 0.0, true, (0.0, 0.0));
        }
        assert!(m.trail.0.abs() < 1e-6, "settled: {m:?}");
    }

    #[test]
    fn the_weapon_bobs_on_its_own_while_the_view_stays_still() {
        let p: CameraProfile = serde_json::from_str(
            r#"{"sway": {"walk": {"height": 0.04, "side": 0.03, "stride": 2.0}}}"#,
        )
        .unwrap();
        p.validate().unwrap();
        let mut m = CameraMotion::default();
        let (mut lowest, mut widest) = (0.0f32, 0.0f32);
        for _ in 0..90 {
            m.advance(&p, 1.0 / 60.0, 6.0, true, (0.0, 0.0));
            assert_eq!(m.bob(&p), (0.0, 0.0), "the view never bobs");
            let (side, dip) = m.weapon_bob(&p);
            lowest = lowest.min(dip);
            widest = widest.max(side.abs());
        }
        assert!(lowest < -0.035 && widest > 0.025, "{lowest} {widest}");
        // Standing, it eases out.
        for _ in 0..30 {
            m.advance(&p, 1.0 / 60.0, 0.0, true, (0.0, 0.0));
        }
        assert_eq!(m.weapon_bob(&p), (0.0, 0.0));
        // With a camera bob too, the shares add.
        let p: CameraProfile = serde_json::from_str(
            r#"{"bob": {"height": 0.1, "stride": 2.0},
                "sway": {"bob": 0.5, "walk": {"height": 0.04, "stride": 2.0}}}"#,
        )
        .unwrap();
        let mut m = CameraMotion::default();
        for _ in 0..47 {
            m.advance(&p, 1.0 / 60.0, 6.0, true, (0.0, 0.0));
        }
        let (_, dip) = m.weapon_bob(&p);
        let (_, view) = m.bob(&p);
        assert!(view < -0.01);
        assert!(
            (dip - (view * 0.5 + view * 0.4)).abs() < 1e-5,
            "{dip} {view}"
        );
    }

    #[test]
    fn bad_profiles_are_refused() {
        for bad in [
            r#"{"fov": 10}"#,
            r#"{"eye_offset": [0, 5, 0]}"#,
            r#"{"bob": {"height": 2}}"#,
            r#"{"bob": {"stride": 0}}"#,
            r#"{"sway": {"lag": 3}}"#,
            r#"{"sway": {"walk": {"height": 3}}}"#,
            r#"{"third_person": {"side": 9}}"#,
        ] {
            let p: CameraProfile = serde_json::from_str(bad).unwrap();
            assert!(p.validate().is_err(), "{bad}");
        }
        assert!(serde_json::from_str::<CameraProfile>(r#"{"zoom": 1}"#).is_err());
    }
}
