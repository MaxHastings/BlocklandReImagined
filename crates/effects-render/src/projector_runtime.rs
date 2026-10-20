//! Projectors at run time: a game's attach, detach and move of a projector become edits of the renderer's decal layer
//! ([`bri_render::decals::DecalEdit`]). A projector is a picture thrown along a direction
//! onto the map's surfaces inside a volume (a cone, or a box when its field of view is zero); this module is the whole of its
//! authoring surface and knows nothing of any game's own names, units or axes. A game that has its own (the UE2 import in
//! `bri-ue2-import`) converts into [`ProjectorSpec`] and [`Placement`] before it gets here.
//!
//! Lengths are the renderer's own and the frame is the renderer's (y up).
use bri_content::decals::{Blend, Decal, Receivers};
use bri_emitters::projector::{ProjectorProps, calc_matrix};
use bri_emitters::rotation::Rotator;
use bri_render::decals::DecalEdit;
use glam::{Mat3, Mat4, Vec3, Vec4};
use std::collections::HashMap;

/// A projector's key in the decal layer.
pub type ProjectorId = u64;

/// Where a projector is and which way it throws its picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub position: [f32; 3],
    /// The way it projects; need not be normalised.
    pub direction: [f32; 3],
    /// Turn of the picture about that direction, in degrees.
    pub roll_degrees: f32,
}

/// The surfaces a projector draws on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectorReceivers {
    /// The map's walls, floors and static meshes.
    pub map_surfaces: bool,
    pub terrain: bool,
    /// Faces turned away from the projector.
    pub backfaces: bool,
    /// Surfaces nearly parallel to the throw.
    pub grazing_surfaces: bool,
}

impl Default for ProjectorReceivers {
    fn default() -> Self {
        Self {
            map_surfaces: true,
            terrain: true,
            backfaces: false,
            grazing_surfaces: true,
        }
    }
}

/// How a projected picture combines with what is under it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjectorBlend {
    /// Multiplies, doubled: grey leaves the surface as it is (a blob shadow, a stain).
    #[default]
    Modulate2x,
    /// Laid over by the picture's alpha.
    Alpha,
    /// Added (a light pattern).
    Add,
}

/// What a projector draws and where it reaches.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectorSpec {
    /// The picture, as the game names it.
    pub picture: String,
    /// The cone's full opening in degrees; 0 is a box of the picture's width and height.
    pub fov_degrees: f32,
    /// How far along the throw it reaches.
    pub reach: f32,
    /// The footprint's width at the projector; its height keeps the picture's shape.
    pub width: f32,
    /// A turn of the picture about the throw, degrees a second (0 holds it still).
    pub spin_degrees_per_second: f32,
    pub receivers: ProjectorReceivers,
    pub blend: ProjectorBlend,
}

impl Default for ProjectorSpec {
    fn default() -> Self {
        Self {
            picture: String::new(),
            fov_degrees: 0.0,
            reach: 0.0,
            width: 1.0,
            spin_degrees_per_second: 0.0,
            receivers: ProjectorReceivers::default(),
            blend: ProjectorBlend::default(),
        }
    }
}

/// The volume maths works in whole-number steps of 4 cm (25 to the renderer's unit): reach is rounded to one, and the matrix's depth
/// runs in them.
const WORKING: f32 = 25.0;
/// A rotation is held in 1/65536 of a turn.
const TURN: f32 = 65536.0;

/// The renderer's frame to the volume maths' (`x' = -z`, `y' = x`, `z' = y`), and back.
fn axes() -> Mat3 {
    Mat3::from_cols(
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    )
}

/// What differs between the baker and the live path in the decal they make.
#[derive(Clone, Copy, Debug)]
pub struct MakeOptions {
    /// Turn the picture over time (`spin_degrees_per_second`): a baked decal cannot be moved by the game.
    pub bake_spin: bool,
    /// Choose receivers for a frustum wide enough for any turn of a spinning projector.
    pub widen_for_turn: bool,
}

fn rotator(placement: &Placement) -> Rotator {
    let d = axes().transpose() * Vec3::from(placement.direction);
    let flat = d.x.hypot(d.y);
    let units = |degrees: f32| (degrees / 360.0 * TURN).round() as i32;
    Rotator::new(
        units(d.z.atan2(flat).to_degrees()),
        units(d.y.atan2(d.x).to_degrees()),
        units(placement.roll_degrees),
    )
}

/// The decal a projector with `spec` at `placement` makes for a picture of `size` texels. None without a picture size.
pub fn make_decal(
    spec: &ProjectorSpec,
    placement: &Placement,
    size: (u32, u32),
    picture: &str,
    options: MakeOptions,
) -> Option<Decal> {
    let spinning = spec.spin_degrees_per_second != 0.0;
    let to_working = axes().transpose();
    let props = ProjectorProps {
        location: to_working * Vec3::from(placement.position) * WORKING,
        rotation: rotator(placement),
        draw_scale: spec.width * WORKING / size.0.max(1) as f32,
        texture_size: size,
        fov: spec.fov_degrees.round() as i32,
        max_trace_distance: (spec.reach * WORKING).round() as i32,
        rotating: spinning,
        gradient_height: None,
    };
    let volume = calc_matrix(&props)?;
    let (planes, corners) = if spinning && options.widen_for_turn {
        let wide = ProjectorProps {
            draw_scale: props.draw_scale * std::f32::consts::SQRT_2,
            ..props.clone()
        };
        calc_matrix(&wide).map_or((volume.planes, volume.vertices), |v| (v.planes, v.vertices))
    } else {
        (volume.planes, volume.vertices)
    };
    let to_native = axes();
    let back = Mat4::from_mat3(to_native.transpose() * WORKING);
    let plane = |pl: Vec4| {
        let n = to_native * Vec3::new(pl.x, pl.y, pl.z);
        [n.x, n.y, n.z, pl.w / WORKING]
    };
    let blend = match spec.blend {
        ProjectorBlend::Modulate2x => Blend::Modulate2x,
        ProjectorBlend::Alpha => Blend::Alpha,
        ProjectorBlend::Add => Blend::Add,
    };
    // A surface is left out when its angle to the throw is steeper than the gate: grazing ones are kept only when allowed.
    let facing_limit = if spec.receivers.grazing_surfaces {
        -0.0001
    } else {
        -std::f32::consts::FRAC_1_SQRT_2
    };
    let native = corners.map(|c| to_native * c / WORKING);
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
        spin_degrees_per_second: if options.bake_spin {
            spec.spin_degrees_per_second
        } else {
            0.0
        },
        surfaces: Receivers {
            enabled: spec.receivers.map_surfaces,
            facing_limit,
            fade_by_facing: false,
        },
        ground: Receivers {
            enabled: spec.receivers.terrain,
            facing_limit: 2.0,
            fade_by_facing: !spec.receivers.backfaces,
        },
    })
}

struct Entry {
    spec: ProjectorSpec,
    placement: Placement,
    /// The edit that added it went out.
    live: bool,
}

/// The attached projectors, as a queue of decal edits for the renderer.
#[derive(Default)]
pub struct ProjectorRuntime {
    /// Pictures by the name a game uses: texel size and the name the decal layer knows the picture by.
    pictures: HashMap<String, (u32, u32, String)>,
    entries: HashMap<ProjectorId, Entry>,
    edits: Vec<DecalEdit>,
    /// Things worth logging, once each.
    pub notes: Vec<String>,
}

impl ProjectorRuntime {
    pub fn new() -> Self {
        Self::default()
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
            .filter(|(_, e)| !e.live && e.spec.picture.eq_ignore_ascii_case(script_name))
            .map(|(id, _)| *id)
            .collect();
        for id in waiting {
            self.add(id);
        }
    }

    fn options() -> MakeOptions {
        MakeOptions {
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
            .get(&entry.spec.picture.to_ascii_lowercase())
            .cloned()
        else {
            return;
        };
        if let Some(decal) = make_decal(
            &entry.spec,
            &entry.placement,
            (w, h),
            &name,
            Self::options(),
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
    pub fn attach(&mut self, id: ProjectorId, spec: ProjectorSpec, placement: Placement) {
        if spec.picture.is_empty() {
            self.notes.push(format!("projector {id}: no picture named"));
            return;
        }
        self.entries.insert(
            id,
            Entry {
                spec,
                placement,
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
    pub fn set_placement(&mut self, id: ProjectorId, placement: Placement) {
        let Some(entry) = self.entries.get_mut(&id) else {
            return;
        };
        if entry.placement == placement {
            return;
        }
        entry.placement = placement;
        if !entry.live {
            return;
        }
        let Some((w, h, name)) = self
            .pictures
            .get(&entry.spec.picture.to_ascii_lowercase())
            .cloned()
        else {
            return;
        };
        if let Some(decal) = make_decal(
            &entry.spec,
            &placement,
            (w, h),
            &name,
            MakeOptions {
                widen_for_turn: false,
                ..Self::options()
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
