//! Projectors at run time: a game mode's attach, detach and move of a projector become edits of the renderer's
//! decal layer ([`bri_render::decals::DecalEdit`]). A projector's settings come from its actor's properties; its volume from
//! `AProjector::CalcMatrix` ([`bri_ue2_emitters::projector::calc_matrix`]), recomputed whenever the actor moves or turns.
//!
//! The decals are in the renderer's frame: a package position `(x, y, z)` is `(y, z, -x) * unit`; directions take the axes without the
//! unit. The same conversion bakes a map's placed projectors ([`crate::projector_bake`]).
use bri_content::decals::{Blend, Decal, Receivers};
use bri_render::decals::DecalEdit;
use bri_ue2_emitters::projector::{
    FrameBufferBlend, ProjectorBlending, ProjectorProps, calc_matrix, frame_buffer_blend,
};
use bri_ue2_emitters::rotation::Rotator;
use glam::{Mat3, Mat4, Vec3, Vec4};
use std::collections::HashMap;

/// A projector's key in the decal layer.
pub type ProjectorId = u64;

/// Where a projector is, in package units and rotation units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub location: [f32; 3],
    pub rotation: [i32; 3],
}

/// `PHYS_Rotating`.
const PHYS_ROTATING: i32 = 5;
/// A rotation unit is 1/65536 of a turn.
const UNITS_PER_TURN: f32 = 65536.0;

/// Package axes to the renderer's: `x' = y`, `y' = z`, `z' = -x`.
pub fn axes() -> Mat3 {
    Mat3::from_cols(
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    )
}

/// The properties of a `Projector` that decide what it draws and where.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectorSettings {
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

impl Default for ProjectorSettings {
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

impl ProjectorSettings {
    pub fn rotating(&self) -> bool {
        self.physics == PHYS_ROTATING
    }
}

/// What differs between the baker and the live path in the decal they make.
#[derive(Clone, Copy, Debug)]
pub struct MakeOptions {
    /// Native length of one package unit.
    pub unit: f32,
    /// Turn the picture over time (`spin_degrees_per_second`) for a rotating projector: a baked decal cannot be moved by the game.
    pub bake_spin: bool,
    /// Choose receivers for a frustum wide enough for any turn of a rotating projector.
    pub widen_for_turn: bool,
}

/// The decal a projector with `settings` at `location` / `rotation` makes for a picture of `size` texels. None without a picture size.
pub fn make_decal(
    settings: &ProjectorSettings,
    location: [f32; 3],
    rotation: [i32; 3],
    size: (u32, u32),
    picture: &str,
    options: MakeOptions,
) -> Option<Decal> {
    let rotating = settings.rotating();
    let props = ProjectorProps {
        location: Vec3::from(location),
        rotation: Rotator::new(rotation[0], rotation[1], rotation[2]),
        draw_scale: settings.draw_scale,
        texture_size: size,
        fov: settings.fov,
        max_trace_distance: settings.max_trace_distance,
        rotating,
        gradient_height: None,
    };
    let volume = calc_matrix(&props)?;
    let turning = rotating && settings.rotation_rate[2] != 0.0;
    let (planes, corners) = if turning && options.widen_for_turn {
        let wide = ProjectorProps {
            draw_scale: props.draw_scale * std::f32::consts::SQRT_2,
            ..props.clone()
        };
        calc_matrix(&wide).map_or((volume.planes, volume.vertices), |v| (v.planes, v.vertices))
    } else {
        (volume.planes, volume.vertices)
    };
    let to_native = axes();
    let unit = options.unit;
    let back = Mat4::from_mat3(to_native.transpose() * (1.0 / unit));
    let plane = |pl: Vec4| {
        let n = to_native * Vec3::new(pl.x, pl.y, pl.z);
        [n.x, n.y, n.z, pl.w * unit]
    };
    let blend = match frame_buffer_blend(ProjectorBlending::from_byte(settings.blending_op)) {
        FrameBufferBlend::Modulate2X => Blend::Modulate2x,
        FrameBufferBlend::Alpha => Blend::Alpha,
        FrameBufferBlend::Add => Blend::Add,
    };
    // The BSP attach gate: left out when N . F >= -0.0001, and when >= -0.7071 unless parallel surfaces are allowed.
    let facing_limit = if settings.project_on_parallel_bsp {
        -0.0001
    } else {
        -std::f32::consts::FRAC_1_SQRT_2
    };
    let native = corners.map(|c| to_native * c * unit);
    let bounds = [
        native.iter().fold(native[0], |a, b| a.min(*b)).to_array(),
        native.iter().fold(native[0], |a, b| a.max(*b)).to_array(),
    ];
    Some(Decal {
        picture: picture.into(),
        world_to_texture: (volume.matrix * back).to_cols_array(),
        perspective: volume.perspective,
        planes: planes.map(plane),
        forward: (to_native * volume.forward).to_array(),
        bounds: Some(bounds),
        blend,
        // The bitmap's own clamp flags are not known here; the edge is extended.
        wrap: [false, false],
        two_sided: false,
        spin_degrees_per_second: if options.bake_spin && rotating {
            settings.rotation_rate[2] / UNITS_PER_TURN * 360.0
        } else {
            0.0
        },
        surfaces: Receivers {
            enabled: settings.project_bsp || settings.project_static_mesh,
            facing_limit,
            fade_by_facing: false,
        },
        ground: Receivers {
            enabled: settings.project_terrain,
            facing_limit: 2.0,
            fade_by_facing: !settings.project_on_backfaces,
        },
    })
}

struct Entry {
    settings: ProjectorSettings,
    pose: Pose,
    /// The edit that added it went out.
    live: bool,
}

/// The attached projectors, as a queue of decal edits for the renderer.
#[derive(Default)]
pub struct ProjectorRuntime {
    unit: f32,
    /// Pictures by the name a script uses: texel size and the name the decal layer knows the picture by.
    pictures: HashMap<String, (u32, u32, String)>,
    entries: HashMap<ProjectorId, Entry>,
    edits: Vec<DecalEdit>,
    /// Things worth logging, once each.
    pub notes: Vec<String>,
}

impl ProjectorRuntime {
    /// `unit`: the native length of one package unit.
    pub fn new(unit: f32) -> Self {
        Self {
            unit,
            ..Self::default()
        }
    }

    /// A projector picture the renderer holds: `layer_name` is how `DecalLayer::add_picture` was given it. Projectors waiting
    /// for it attach now.
    pub fn register_picture(&mut self, script_name: &str, size: (u32, u32), layer_name: &str) {
        self.pictures.insert(
            script_name.to_ascii_lowercase(),
            (size.0, size.1, layer_name.into()),
        );
        let waiting: Vec<ProjectorId> = self
            .entries
            .iter()
            .filter(|(_, e)| !e.live && e.settings.texture.eq_ignore_ascii_case(script_name))
            .map(|(id, _)| *id)
            .collect();
        for id in waiting {
            self.add(id);
        }
    }

    fn options(&self) -> MakeOptions {
        MakeOptions {
            unit: self.unit,
            bake_spin: false,
            widen_for_turn: true,
        }
    }

    fn add(&mut self, id: ProjectorId) {
        let Some(entry) = self.entries.get_mut(&id) else {
            return;
        };
        let Some((w, h, name)) = self
            .pictures
            .get(&entry.settings.texture.to_ascii_lowercase())
            .cloned()
        else {
            return;
        };
        let options = MakeOptions {
            unit: self.unit,
            bake_spin: false,
            widen_for_turn: true,
        };
        if let Some(decal) = make_decal(
            &entry.settings,
            entry.pose.location,
            entry.pose.rotation,
            (w, h),
            &name,
            options,
        ) {
            entry.live = true;
            self.edits.push(DecalEdit::Add { key: id, decal });
        }
    }

    /// The edits since the last call, in order.
    pub fn drain(&mut self) -> Vec<DecalEdit> {
        std::mem::take(&mut self.edits)
    }

    pub fn attached(&self) -> usize {
        self.entries.len()
    }
}

impl ProjectorRuntime {
    /// A projector starts drawing its picture on the surfaces its volume overlaps.
    pub fn attach(&mut self, id: ProjectorId, settings: ProjectorSettings, pose: Pose) {
        if settings.texture.is_empty() {
            self.notes.push(format!("projector {id}: no picture named"));
            return;
        }
        if settings.project_bsp != settings.project_static_mesh {
            let note = "a projector takes BSP and static meshes differently; the map's walls and meshes are one receiver class, so both are taken or neither".to_string();
            if !self.notes.contains(&note) {
                self.notes.push(note);
            }
        }
        self.entries.insert(
            id,
            Entry {
                settings,
                pose,
                live: false,
            },
        );
        self.add(id);
    }

    /// The projector is gone.
    pub fn detach(&mut self, id: ProjectorId) {
        if let Some(entry) = self.entries.remove(&id)
            && entry.live
        {
            self.edits.push(DecalEdit::Remove { key: id });
        }
    }

    /// The projector moved or turned.
    pub fn set_pose(&mut self, id: ProjectorId, pose: Pose) {
        let options = self.options();
        let Some(entry) = self.entries.get_mut(&id) else {
            return;
        };
        if entry.pose == pose {
            return;
        }
        entry.pose = pose;
        if !entry.live {
            return;
        }
        let Some((w, h, name)) = self
            .pictures
            .get(&entry.settings.texture.to_ascii_lowercase())
            .cloned()
        else {
            return;
        };
        if let Some(decal) = make_decal(
            &entry.settings,
            pose.location,
            pose.rotation,
            (w, h),
            &name,
            MakeOptions {
                widen_for_turn: false,
                ..options
            },
        ) {
            self.edits.push(DecalEdit::Move {
                key: id,
                world_to_texture: decal.world_to_texture,
                forward: decal.forward,
            });
        }
    }
}
