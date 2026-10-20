//! A UE2 `Projector` actor's properties, and their conversion to the engine's neutral projector.
use bri_effects_render::projector_runtime::{
    Placement, ProjectorBlend, ProjectorReceivers, ProjectorSpec,
};
use bri_emitters::projector::{FrameBufferBlend, ProjectorBlending, frame_buffer_blend};
use bri_emitters::rotation::{Coords, Rotator};
use glam::{Mat3, Vec3};

/// `PHYS_Rotating`.
const PHYS_ROTATING: i32 = 5;
/// A rotation unit is 1/65536 of a turn.
const UNITS_PER_TURN: f32 = 65536.0;

/// Package axes (x forward, y right, z up, left-handed) to the engine's: `x' = y`, `y' = z`, `z' = -x`.
pub fn axes() -> Mat3 {
    Mat3::from_cols(
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    )
}

/// The properties of a `Projector` that decide what it draws and where.
#[derive(Clone, Debug, PartialEq)]
pub struct Ue2Projector {
    /// `ProjTexture`, as the script names it.
    pub texture: String,
    pub fov: i32,
    pub max_trace_distance: i32,
    pub draw_scale: f32,
    pub physics: i32,
    /// `RotationRate`, pitch, yaw, roll in rotation units a second.
    pub rotation_rate: [f32; 3],
    pub project_bsp: bool,
    pub project_static_mesh: bool,
    pub project_terrain: bool,
    pub project_on_backfaces: bool,
    pub project_on_parallel_bsp: bool,
    pub blending_op: u8,
}

impl Default for Ue2Projector {
    fn default() -> Self {
        Self {
            texture: String::new(),
            fov: 0,
            max_trace_distance: 0,
            draw_scale: 1.0,
            physics: 0,
            rotation_rate: [0.0; 3],
            project_bsp: true,
            project_static_mesh: true,
            project_terrain: true,
            project_on_backfaces: false,
            project_on_parallel_bsp: true,
            blending_op: 1,
        }
    }
}

impl Ue2Projector {
    pub fn rotating(&self) -> bool {
        self.physics == PHYS_ROTATING
    }

    /// The neutral projector for a picture of `size` texels. `unit` is the native length of one package unit.
    pub fn spec(&self, size: (u32, u32), unit: f32) -> ProjectorSpec {
        ProjectorSpec {
            picture: self.texture.clone(),
            fov_degrees: self.fov as f32,
            reach: self.max_trace_distance as f32 * unit,
            width: size.0 as f32 * self.draw_scale * unit,
            spin_degrees_per_second: if self.rotating() {
                self.rotation_rate[2] / UNITS_PER_TURN * 360.0
            } else {
                0.0
            },
            receivers: ProjectorReceivers {
                map_surfaces: self.project_bsp || self.project_static_mesh,
                terrain: self.project_terrain,
                backfaces: self.project_on_backfaces,
                grazing_surfaces: self.project_on_parallel_bsp,
            },
            blend: match frame_buffer_blend(ProjectorBlending::from_byte(self.blending_op)) {
                FrameBufferBlend::Modulate2X => ProjectorBlend::Modulate2x,
                FrameBufferBlend::Alpha => ProjectorBlend::Alpha,
                FrameBufferBlend::Add => ProjectorBlend::Add,
            },
        }
    }
}

/// A UE2 actor's `Location` (package units) and `Rotation` (rotation units) as a neutral placement.
pub fn placement(location: [f32; 3], rotation: [i32; 3], unit: f32) -> Placement {
    let axes_of = Coords::UNIT.mul_rotator(Rotator::new(rotation[0], rotation[1], rotation[2]));
    Placement {
        position: (axes() * Vec3::from(location) * unit).to_array(),
        direction: (axes() * axes_of.x).to_array(),
        roll_degrees: rotation[2] as f32 / UNITS_PER_TURN * 360.0,
    }
}
