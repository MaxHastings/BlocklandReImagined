//! Movement controllers: an archetype's own movement rules (a wall kick, a
//! triple jump, a dive) layered on the v20 motor. The motor stays the
//! mechanism (collision, sliding, stepping, gravity, water, ropes); a
//! controller is policy. Once each 32 ms tick, before the motor moves the
//! body, the controller reads the tick's input and the body's state, may
//! sweep the body through the world, and returns a new velocity, whether it
//! took the jump button, its own small memory and an animation intent.
//!
//! The same controller runs on the host and on the predicting client, so
//! both must get the same answer from the same state: a controller is a
//! pure function of [`ControllerTick`] and its sweeps. Its memory travels in
//! [`PlayerState::controller`](crate::player::PlayerState::controller), so
//! a correction restores it with the rest of the body and a replay runs the
//! same steps again. The engine implements controllers from Add-On
//! WebAssembly (`bri-controller`); this crate only defines the seam, so the
//! motor never depends on a WebAssembly runtime.
use bri_console::Clamp;
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Bytes a controller keeps between ticks.
pub const MEMORY_BYTES: usize = 16;
/// Most sweeps one controller step may make.
pub const MAX_SWEEPS: u8 = 8;
/// Longest sweep a controller may ask for, units at scale 1.
pub const MAX_SWEEP: f32 = 8.0;
/// A sweep starts this far behind the body, so a surface the body already
/// touches (the motor leaves bodies resting against walls) still reports
/// its normal.
pub const SWEEP_BACK_OFF: f32 = 0.1;
/// Largest controller module an archetype may carry.
pub const MAX_MODULE_BYTES: usize = 64 * 1024;

/// A controller's state on a body: its own bytes, the animation intent it
/// last asked for (0 is none; what other numbers mean is the controller's,
/// named in its Add-On and mapped to clips and sounds by its archetype's
/// [`Look`](crate::archetype::Look)) and the way it turned the body, if it
/// turns the body itself.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ControllerState {
    pub memory: [u8; MEMORY_BYTES],
    pub intent: u8,
    /// The body's own heading in [`HEADING_UNITS`] of a radian (the yaw
    /// convention of [`MoveInput::yaw`](crate::player::MoveInput)), when the
    /// controller faces the body away from the camera (Mario turning on his
    /// own, skidding, kicking off a wall). None: the body faces the look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<i16>,
}
/// Heading steps per radian, so a heading is an exact integer on every side
/// and replicates as one.
pub const HEADING_UNITS: f32 = 10_000.0;
impl ControllerState {
    /// The heading in radians, if the controller sets one.
    pub fn heading(&self) -> Option<f32> {
        self.heading.map(|h| f32::from(h) / HEADING_UNITS)
    }
    /// Store a heading in radians, wrapped to -π..π.
    pub fn set_heading(&mut self, radians: Option<f32>) {
        self.heading = radians.filter(|r| r.is_finite()).map(|r| {
            let wrapped =
                (r + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            (wrapped * HEADING_UNITS)
                .round()
                .clamped(-31_416.0, 31_416.0) as i16
        });
    }
}

/// One tick as a controller sees it. Inputs are read-only by convention;
/// `velocity`, `take_jump` and `state` are what it decides.
#[derive(Debug, Clone, PartialEq)]
pub struct ControllerTick {
    /// Seconds this tick covers (v20's 32 ms).
    pub dt: f32,
    pub scale: f32,
    pub feet: Vec3,
    /// The body's facing on the ground plane, unit length.
    pub facing: Vec3,
    /// Where the movement keys push, on the ground plane: facing times
    /// forward plus right times right (length up to about 1.41).
    pub wish: Vec3,
    pub pitch: f32,
    /// Standing on a run surface at the end of the last tick.
    pub grounded: bool,
    pub crouched: bool,
    pub submerged: bool,
    /// Held at some step since the last tick.
    pub jump: bool,
    pub crouch: bool,
    pub jet: bool,
    /// The primary trigger (`MoveInput::attack`): a move button of the
    /// controller's own (SM64's B), held whether or not an item is out.
    pub attack: bool,
    /// The archetype's gravity, units/s², so a module need not copy it.
    pub gravity: f32,
    /// In: the body's velocity. Out: the velocity the motor moves it with.
    pub velocity: Vec3,
    /// Out: the motor does not jump this tick (the controller used it).
    pub take_jump: bool,
    /// Out: the motor adds no gravity this tick (the controller did its
    /// own, as SM64 moves first and falls after).
    pub skip_gravity: bool,
    /// In and out.
    pub state: ControllerState,
}

/// A sweep's hit: how far along the asked distance (0 to 1) and the
/// surface's outward normal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SweepHit {
    pub fraction: f32,
    pub normal: Vec3,
}

/// The world as a controller may query it: the polygons around the body
/// that the motor itself collides with (Torque's working list, gathered
/// [`MAX_SWEEP`] around the feet), with the body's box. It owns what it
/// holds, so a controller host can keep it for the length of a step.
#[derive(Default)]
pub struct Probe {
    soup: crate::torque::Soup,
    feet: Vec3,
    half_width: f32,
    height: f32,
    reach: f32,
    sweeps: u8,
}
impl Probe {
    pub fn new(
        soup: crate::torque::Soup,
        feet: Vec3,
        half_width: f32,
        height: f32,
        reach: f32,
    ) -> Self {
        Self {
            soup,
            feet,
            half_width,
            height,
            reach,
            sweeps: 0,
        }
    }
    /// Sweep the body's box from its feet by `delta`: `None` when nothing
    /// is in the way. Errors when a step asks for more than [`MAX_SWEEPS`]
    /// sweeps or one longer than its reach.
    pub fn sweep(&mut self, delta: Vec3) -> Result<Option<SweepHit>, String> {
        self.sweep_from(Vec3::ZERO, delta)
    }
    /// [`Probe::sweep`] with the box moved `offset` from the feet first:
    /// is there room above the wall at hand height for a ledge grab, is
    /// there floor a step ahead. The offset and the sweep together stay
    /// within the reach.
    pub fn sweep_from(&mut self, offset: Vec3, delta: Vec3) -> Result<Option<SweepHit>, String> {
        self.sweeps += 1;
        if self.sweeps > MAX_SWEEPS {
            return Err(format!("more than {MAX_SWEEPS} sweeps in one tick"));
        }
        let distance = delta.length();
        let from = offset.length();
        if !distance.is_finite() || !from.is_finite() || from + distance > self.reach {
            return Err(format!(
                "a sweep longer than {} units from the feet",
                self.reach
            ));
        }
        let Some(direction) = delta.try_normalize() else {
            return Ok(None);
        };
        let back = direction * SWEEP_BACK_OFF;
        Ok(crate::torque::sweep_box(
            &self.soup,
            self.feet + offset - back,
            self.half_width,
            self.height,
            delta + back,
        )
        .map(|(t, normal)| SweepHit {
            fraction: ((t * (distance + SWEEP_BACK_OFF) - SWEEP_BACK_OFF) / distance)
                .clamped(0.0, 1.0),
            normal,
        }))
    }
}

/// An archetype's movement rules.
pub trait MoveController: Send + Sync + std::fmt::Debug {
    /// Content id, for reports.
    fn id(&self) -> &str;
    /// Decide one tick. An error leaves the tick as the motor alone would
    /// run it (the same on host and client, since a controller's failures
    /// are as deterministic as its results) and is reported.
    fn step(&self, tick: &mut ControllerTick, probe: &mut Probe) -> Result<(), String>;
}

/// A shared controller, as motor constants carry it. Two handles are equal
/// when they are the same controller.
#[derive(Clone)]
pub struct Controller(pub Arc<dyn MoveController>);
impl Controller {
    pub fn id(&self) -> &str {
        self.0.id()
    }
}
impl std::fmt::Debug for Controller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Controller({})", self.0.id())
    }
}
impl PartialEq for Controller {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// An archetype's controller as the host sends it to clients: the module
/// itself, so every side runs exactly the bytes the host runs.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerSource {
    /// Content id: `<package>:controller/<name>`.
    pub id: String,
    /// WebAssembly binary.
    #[serde(with = "serde_bytes_compat")]
    pub module: Vec<u8>,
}
impl std::fmt::Debug for ControllerSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ControllerSource({}, {} bytes)",
            self.id,
            self.module.len()
        )
    }
}
impl ControllerSource {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.id.is_empty()
                && self.id.len() <= 160
                && !self.id.chars().any(char::is_control)
                && !self.module.is_empty()
                && self.module.len() <= MAX_MODULE_BYTES,
            "Invalid controller {}: an id and a module of at most {MAX_MODULE_BYTES} bytes",
            self.id
        );
        Ok(())
    }
}

/// Bytes as one binary value (MessagePack `bin`), not a list of numbers.
mod serde_bytes_compat {
    use serde::{Deserializer, Serializer};
    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(bytes)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Vec<u8>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("bytes")
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                Ok(v.to_vec())
            }
            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
                Ok(v)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(b) = seq.next_element::<u8>()? {
                    if out.len() >= super::MAX_MODULE_BYTES {
                        return Err(serde::de::Error::custom("controller module too large"));
                    }
                    out.push(b);
                }
                Ok(out)
            }
        }
        d.deserialize_byte_buf(Visitor)
    }
}
