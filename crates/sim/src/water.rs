//! What v20 does to a player in liquid beyond the motor: the splash and exit
//! sound rules (`Player::updateSplash` and the ghost's `inLiquid` check), the
//! froth and bubble timing (`Player::updateFroth`), liquid colours and the
//! camera's screen tint (`GameRenderFilters`). From the read-only disassembly
//! of blocklandv20.exe (see docs/audits/water.md).
use crate::player::{PlayerState, PlayerTuning};
use bri_content::water::Water;
use glam::Vec3;

/// A map `WaterBlock`'s `waterColor`. Every stock v20 mission leaves the
/// default (0.2, 0.6, 0.6, 0.3).
pub const MAP_WATER_COLOR: [f32; 4] = [0.2, 0.6, 0.6, 0.3];
/// `GameRenderFilters` clamps the tint's alpha to 0.9.
pub const MAX_TINT_ALPHA: f32 = 0.9;
/// `splashVelocity`: slower players never splash.
pub const SPLASH_SPEED: f32 = 4.0;
/// `exitSplashSoundVelocity`.
pub const EXIT_SPEED: f32 = 5.0;
/// `splashVelEpsilon`: froth stops below this speed.
pub const FROTH_MIN_SPEED: f32 = 0.6;
/// `splashFreqMod`: froth emitters run `speed * 300` ms of emission per second.
pub const FROTH_FREQUENCY: f32 = 300.0;
/// `bubbleEmitTime`: bubbles follow a splash for 0.1 s.
pub const BUBBLE_SECONDS: f32 = 0.1;

/// A water brick's `PhysicalZone` takes the paint colour with its alpha
/// scaled by 0.75 (`brick8xWaterData::onColorChange`).
pub fn brick_water_color(paint: [f32; 4]) -> [f32; 4] {
    [paint[0], paint[1], paint[2], paint[3] * 0.75]
}

/// Liquid-tinted emitters draw with normal alpha blending once the colour's
/// alpha doubled passes 0.95, and additively otherwise (Blockland's emitter
/// colour override, `0x5a4834`).
pub fn tint_blends_alpha(color: [f32; 4]) -> bool {
    f64::from(color[3]) * 2.0 > 0.95
}

/// The player's height for liquid coverage, from its archetype's tuning.
pub fn body_height(state: &PlayerState, tuning: &PlayerTuning) -> f32 {
    if state.crouched {
        tuning.crouch_height
    } else {
        tuning.stand_height
    }
}

/// The liquid covering most of the player's body, with that fraction.
pub fn deepest(waters: &[Water], feet: [f32; 3], height: f32) -> Option<(usize, f32)> {
    // The one submersion query players, splashes and vehicles share.
    let (water, coverage) = bri_content::water::submersion(waters, feet, height)?;
    let index = waters.iter().position(|w| std::ptr::eq(w, water))?;
    Some((index, coverage))
}

/// Where a swimmer `height` tall in `water` heads to reach `point`: over
/// the water as near it as the water goes, deep enough to stay under and
/// above the bottom.
pub fn swim_point(water: &Water, point: Vec3, height: f32) -> Vec3 {
    let mut to = point;
    if water.footprint(to.x, to.z).is_none() && water.repeat_period.is_none() {
        to.x = to.x.clamp(water.min[0] + 0.5, water.max[0] - 0.5);
        to.z = to.z.clamp(water.min[2] + 0.5, water.max[2] - 0.5);
    }
    let low = water.min[1] + 0.1;
    to.y = to.y.clamp(low, (water.max[1] - height - 0.1).max(low));
    to
}

/// A point inside a liquid volume: the camera test of `GameRenderFilters`.
pub fn contains(water: &Water, point: Vec3) -> bool {
    point.is_finite()
        && water.footprint(point.x, point.z).is_some()
        && point.y > water.min[1]
        && point.y < water.max[1]
}

/// The fullscreen tint `GameRenderFilters` draws for a camera inside a liquid
/// of this colour, alpha clamped to [0, 0.9].
pub fn screen_tint(color: [f32; 4]) -> [f32; 4] {
    [
        color[0],
        color[1],
        color[2],
        color[3].clamp(0.0, MAX_TINT_ALPHA),
    ]
}

/// A liquid and its `waterColor`.
#[derive(Clone, Debug)]
pub struct TintedWater {
    pub water: Water,
    pub color: [f32; 4],
    /// A water brick's `PhysicalZone` rather than a map `WaterBlock`.
    pub brick: bool,
}

/// `GameRenderFilters`' tints for a camera at `eye`: the first water brick
/// zone holding it, then the map water it is under. Both draw when both apply.
pub fn screen_tints(waters: &[TintedWater], eye: Vec3) -> Vec<[f32; 4]> {
    let zone = waters.iter().find(|w| w.brick && contains(&w.water, eye));
    let map = waters.iter().find(|w| !w.brick && contains(&w.water, eye));
    zone.into_iter()
        .chain(map)
        .map(|w| screen_tint(w.color))
        .collect()
}

/// A splash or exit, decided per player the way v20's player does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crossing {
    /// `createSplash`: the splash, its bubbles and an `impactWater*` sound.
    Splash,
    /// The ghost's `exitingWater` sound.
    Exit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SplashState {
    /// Set whenever coverage drops to 0.01 or less; a splash spends it.
    armed: bool,
    /// The ghost's `inLiquid`: set at full coverage, cleared below 0.8.
    submerged: bool,
}
impl SplashState {
    /// One update with this frame's coverage, speed and whether the player
    /// moved. v20 splashes on any entry into partial coverage faster than
    /// `splashVelocity` (its `splashAngle` test is gone), then re-arms only
    /// after leaving the water. The exit sound needs full submersion first.
    pub fn step(&mut self, coverage: f32, speed: f32, moved: bool) -> Option<Crossing> {
        let mut crossing = None;
        if self.armed && speed >= SPLASH_SPEED && moved && (0.01..=0.99).contains(&coverage) {
            self.armed = false;
            crossing = Some(Crossing::Splash);
        }
        if coverage <= 0.01 {
            self.armed = true;
        }
        if !self.submerged && coverage >= 1.0 {
            self.submerged = true;
        } else if self.submerged && coverage < 0.8 {
            self.submerged = false;
            if speed >= EXIT_SPEED {
                crossing = crossing.or(Some(Crossing::Exit));
            }
        }
        crossing
    }
}

/// Where froth emits for a partly submerged player: the surface above its
/// feet, `pos.z + boxHeight * coverage`. None when fully in or out.
pub fn froth_point(feet: Vec3, height: f32, coverage: f32) -> Option<Vec3> {
    (0.01..=0.99)
        .contains(&coverage)
        .then(|| feet + Vec3::Y * height * coverage)
}

/// Froth emission clock rate: `speed * splashFreqMod * dt` milliseconds of
/// emitter time each frame, zero below `splashVelEpsilon`.
pub fn froth_rate(speed: f32) -> f32 {
    if speed.is_finite() && speed >= FROTH_MIN_SPEED {
        speed * FROTH_FREQUENCY / 1000.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_needs_arming_speed_and_partial_coverage() {
        let mut s = SplashState::default();
        // Spawning in water never splashes: nothing armed it.
        assert_eq!(s.step(0.5, 10.0, true), None);
        assert_eq!(s.step(0.0, 10.0, true), None);
        assert_eq!(s.step(0.5, 3.9, true), None);
        assert_eq!(s.step(0.5, 4.0, false), None);
        assert_eq!(s.step(0.5, 4.0, true), Some(Crossing::Splash));
        // Spent until the player leaves the water again.
        assert_eq!(s.step(0.02, 20.0, true), None);
        assert_eq!(s.step(0.01, 0.0, true), None);
        // Horizontal wading in counts too; there is no angle test.
        assert_eq!(s.step(0.3, 7.0, true), Some(Crossing::Splash));
        // Plunging straight to full coverage skips the splash.
        s.step(0.0, 0.0, true);
        assert_eq!(s.step(1.0, 30.0, true), None);
    }

    #[test]
    fn exit_sound_needs_full_submersion_then_speed() {
        let mut s = SplashState::default();
        s.step(0.9, 6.0, true);
        assert_eq!(s.step(0.5, 6.0, true), None, "never fully under");
        s.step(1.0, 6.0, true);
        assert_eq!(s.step(0.85, 6.0, true), None, "hysteresis until 0.8");
        assert_eq!(s.step(0.79, 6.0, true), Some(Crossing::Exit));
        s.step(1.0, 6.0, true);
        assert_eq!(s.step(0.5, 4.9, true), None, "too slow, still clears");
        assert_eq!(s.step(0.4, 9.0, true), None);
    }

    #[test]
    fn colours_and_tints_follow_v20() {
        assert_eq!(
            brick_water_color([0.1, 0.2, 0.3, 1.0]),
            [0.1, 0.2, 0.3, 0.75]
        );
        assert!(tint_blends_alpha(brick_water_color([0.0, 0.0, 1.0, 1.0])));
        assert!(!tint_blends_alpha(MAP_WATER_COLOR));
        assert_eq!(screen_tint([1.0, 1.0, 1.0, 2.0])[3], 0.9);
        assert_eq!(froth_rate(0.5), 0.0);
        assert!((froth_rate(10.0) - 3.0).abs() < 1e-6);
        assert_eq!(froth_point(Vec3::ZERO, 2.0, 1.0), None);
        assert_eq!(froth_point(Vec3::ZERO, 2.0, 0.5), Some(Vec3::Y));
    }
}
