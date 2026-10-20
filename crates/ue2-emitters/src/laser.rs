//! `ALaserProjector::TickSpecial` (AGP.dll 0x1b7f0; see `specs/coronas-lens-flare-laser.md`,
//! Part 3): the laser's dot is a projector re-attached every tick at the end of a 5000 unit
//! trace, sized by its distance from the local viewer, plus the eye-dazzle bookkeeping.
use crate::lens_flare::rotator_vector;
use crate::rotation::{Coords, Rotator, vector_rotation};
use glam::Vec3;

pub const LASER_RANGE: f32 = 5000.0;
const DOT_SCALE: f32 = 0.0175;
const EYE_COS: f32 = 0.594_712_4;
const EYE_DISTANCE_SQ: f32 = 0.641_588_1;

#[derive(Clone, Copy, Debug)]
pub enum LaserBase {
    /// 0 and 1: mounted on a weapon mod or attachment; the beam follows the projector's own
    /// Y axis (negated for a mod).
    WeaponMod {
        location: Vec3,
        rotation: Rotator,
    },
    WeaponAttachment {
        location: Vec3,
        rotation: Rotator,
    },
    /// 2: the owning pawn's eye and its controller's aim.
    Pawn {
        location: Vec3,
        walk_bob: Vec3,
        eye_height: f32,
        controller_rotation: Rotator,
    },
}

/// What a human-controlled pawn the beam struck reports.
#[derive(Clone, Copy, Debug)]
pub struct HitPawn {
    pub id: u32,
    /// The direction the victim's view rotation points.
    pub view_direction: Vec3,
    /// The world position of the left eye bone.
    pub left_eye: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct LaserHit {
    pub location: Vec3,
    pub pawn: Option<HitPawn>,
}

pub trait LaserWorld {
    /// The trace of flags 0x22087: only actors blocking both bullet and corona traces are
    /// hit, per bone on skeletal meshes. `source` is the laser itself.
    fn trace(&mut self, start: Vec3, end: Vec3) -> Option<LaserHit>;
}

#[derive(Clone, Copy, Debug)]
pub struct LaserInput {
    pub is_on: bool,
    pub has_base: bool,
    pub base: LaserBase,
    /// The local viewer's pawn location, `None` when absent or dead.
    pub viewer: Option<Vec3>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DotPlacement {
    pub location: Vec3,
    pub draw_scale: f32,
    pub rotation: Rotator,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EyeExposure {
    pub pawn: u32,
    pub intensity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaserOutput {
    /// The dot's projector is detached at the start of every tick; it is placed again only
    /// when this is set.
    pub placement: Option<DotPlacement>,
    pub exposure: Option<EyeExposure>,
}

/// One tick. The projector is always detached first, so `placement: None` means hidden.
pub fn tick(input: &LaserInput, world: &mut dyn LaserWorld) -> LaserOutput {
    let none = LaserOutput {
        placement: None,
        exposure: None,
    };
    if !input.is_on || !input.has_base {
        return none;
    }
    let (start, dir, aim) = match input.base {
        LaserBase::WeaponMod { location, rotation } => {
            let d = -Coords::UNIT.mul_rotator(rotation).y;
            (location, d, vector_rotation(d))
        }
        LaserBase::WeaponAttachment { location, rotation } => {
            let d = Coords::UNIT.mul_rotator(rotation).y;
            (location, d, vector_rotation(d))
        }
        LaserBase::Pawn {
            location,
            walk_bob,
            eye_height,
            controller_rotation,
        } => (
            location + Vec3::new(walk_bob.x, walk_bob.y, eye_height + walk_bob.z),
            rotator_vector(controller_rotation),
            controller_rotation,
        ),
    };
    let end = start + dir * LASER_RANGE;
    let Some(hit) = world.trace(start, end) else {
        return none;
    };
    let Some(viewer) = input.viewer else {
        return none;
    };
    let d = (hit.location - viewer).length();
    let scale = if d <= 66.0 {
        1.0
    } else {
        1.0 + (d - 66.0) / 133.0
    };
    let placement = Some(DotPlacement {
        location: hit.location - dir * 4.0,
        draw_scale: scale * DOT_SCALE,
        rotation: aim,
    });
    // Eye test.
    let mut exposure = None;
    if let Some(p) = hit.pawn {
        let c = dir.dot(p.view_direction);
        if c < EYE_COS {
            let dsq = (p.left_eye - hit.location).length_squared();
            if dsq <= EYE_DISTANCE_SQ {
                exposure = Some(EyeExposure {
                    pawn: p.id,
                    intensity: ((EYE_COS - c) / (EYE_COS + 1.0)) * (1.0 - dsq / EYE_DISTANCE_SQ),
                });
            }
        }
    }
    LaserOutput {
        placement,
        exposure,
    }
}

/// One entry of a human controller's `LaserDots` array.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaserDotEntry {
    pub laser: u32,
    pub intensity: f32,
    pub time_started: f32,
}

/// Fold an exposure into the controller's list: a new entry, a rise at once, a decay of
/// 0.5 per second toward the measured value.
pub fn record_exposure(list: &mut Vec<LaserDotEntry>, laser: u32, intensity: f32, now: f32) {
    let Some(e) = list.iter_mut().find(|e| e.laser == laser) else {
        list.push(LaserDotEntry {
            laser,
            intensity,
            time_started: now,
        });
        return;
    };
    let old = e.intensity;
    if old < intensity {
        e.intensity = intensity;
    } else {
        let step = 0.5 * (now - e.time_started);
        e.intensity = if old - intensity > step {
            old - step
        } else {
            intensity
        };
    }
    e.time_started = now;
}
