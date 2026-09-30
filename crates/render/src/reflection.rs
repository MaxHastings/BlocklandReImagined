//! Planar mirrors: each frame the world is drawn again from the view a
//! mirror reflects, into a texture its surface then shows, as engines have
//! drawn flat mirrors and polished floors since the 1990s.
//!
//! Coplanar mirrors (a wall of mirror bricks) share one plane and one extra
//! pass. The planes that fill the most of the screen reflect live, up to the
//! player's Reflections setting; the rest, and mirrors seen inside another
//! mirror, show a plain silver. Each pass draws only what lies in front of
//! its mirror (an oblique near plane) inside the part of the screen the
//! mirror covers, so a small mirror costs a small pass.
use crate::scene::{Camera, DEPTH_FORMAT, GpuInstances, GpuScene, SceneRenderer, WorldPass};
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3, Vec4, Vec4Swizzles};
use std::ops::Range;
use wgpu::util::DeviceExt;

/// One flat mirror in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mirror {
    /// Counterclockwise seen from the side it reflects.
    pub corners: [Vec3; 4],
    /// Multiplies the reflected picture.
    pub tint: [f32; 3],
    /// 1 hides what the mirror is drawn on; less blends over it.
    pub strength: f32,
}
impl Mirror {
    /// The plane `n·p + d = 0`, `n` toward the reflecting side, or None for
    /// a degenerate or non-finite mirror.
    pub fn plane(&self) -> Option<Vec4> {
        let [a, b, c, _] = self.corners;
        let normal = (b - a).cross(c - b).try_normalize()?;
        let plane = normal.extend(-normal.dot(a));
        (self.corners.iter().all(|p| p.is_finite())
            && self.tint.iter().all(|t| t.is_finite())
            && self.strength.is_finite()
            && self.strength > 0.0)
            .then_some(plane)
    }
}

/// How many planes reflect live and how finely.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReflectionSettings {
    /// Planes drawn live each frame; 0 shows every mirror as silver.
    pub planes: usize,
    /// Reflection resolution as a share of the screen's. Below 1 the
    /// reflection is upscaled and its textures read a coarser mip, so it
    /// looks soft and greyer than the room; only Low trades that for speed.
    pub scale: f32,
    /// Mirrors further than this from the eye stay silver.
    pub distance: f32,
}
impl ReflectionSettings {
    pub const OFF: Self = Self {
        planes: 0,
        scale: 0.5,
        distance: 0.0,
    };
    pub const LOW: Self = Self {
        planes: 1,
        scale: 0.5,
        distance: 48.0,
    };
    pub const MEDIUM: Self = Self {
        planes: 2,
        scale: 1.0,
        distance: 64.0,
    };
    pub const HIGH: Self = Self {
        planes: 3,
        scale: 1.0,
        distance: 96.0,
    };
    /// The most planes any setting draws.
    pub const MAX_PLANES: usize = 3;
}

/// At most this many mirrors, nearest first, are drawn at all.
pub const MAX_MIRRORS: usize = 4096;

/// A plane chosen to reflect live this frame, seen from the player's view
/// or, a bounce deeper, from another live plane's reflected view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlannedPlane {
    pub plane: Vec4,
    /// The reflected view: mirrored, clipped at the mirror and cropped to
    /// `viewport`, with x flipped so its triangles keep their winding.
    pub view_projection: Mat4,
    /// The eye reflected in the plane (and in each plane it is seen in).
    pub eye: Vec3,
    /// x, y, width, height in target pixels; the same share of the screen.
    pub viewport: [f32; 4],
    /// Left plus right edge of the viewport as a share of the target's
    /// width: a screen position `u` samples the reflection at this less `u`.
    pub mirror_u: f32,
    /// The view this plane is seen in: 0 the player's, 1 + i live plane i's.
    pub parent: usize,
    /// The coplanar group of mirrors it shows (an index into `Plan::groups`).
    pub group: usize,
    /// The reflections from the player's view down to this plane,
    /// composed: it takes the player's eye and camera axes to this view's.
    pub unreflect: Mat4,
}

impl PlannedPlane {
    /// A direction as the mirror shows it: what faces the player's camera
    /// faces the reflected eye once turned by this.
    pub fn reflect_direction(&self, direction: Vec3) -> Vec3 {
        self.unreflect.transform_vector3(direction)
    }
}

/// What one frame draws: the live planes (a parent before the planes seen
/// in it) and the mirrors, in coplanar groups.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub planes: Vec<PlannedPlane>,
    /// Coplanar mirrors (a wall of mirror bricks), by index into the
    /// mirrors given; every drawn mirror is in exactly one.
    pub groups: Vec<Vec<usize>>,
    /// The mirrors drawn at all (the nearest `MAX_MIRRORS`), in order.
    pub drawn: Vec<usize>,
}
/// What a mirror surface shows in one view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shows {
    /// Live plane i's reflection, drawn this frame for this view.
    Live(usize),
    /// Live plane i's last picture, reprojected: a mirror seen deeper than
    /// the passes reach shows what the same mirror showed nearer the
    /// player, a frame late, so facing mirrors repeat into the distance.
    Echo(usize),
    Silver,
}
impl Plan {
    /// For each group, what `view` (0 the player's, 1 + i live plane i's)
    /// shows on it; None for the view's own plane, which lies on its clip
    /// plane.
    pub fn slots(&self, view: usize) -> Vec<Option<Shows>> {
        let mut out = vec![Some(Shows::Silver); self.groups.len()];
        // The nearest plane of a group is the one echoed.
        for (i, plane) in self.planes.iter().enumerate().rev() {
            out[plane.group] = Some(Shows::Echo(i));
        }
        for (i, plane) in self.planes.iter().enumerate() {
            if plane.parent == view {
                out[plane.group] = Some(Shows::Live(i));
            }
        }
        if let Some(own) = view.checked_sub(1).and_then(|i| self.planes.get(i)) {
            out[own.group] = None;
        }
        out
    }
}

/// The screen rectangle (NDC min x, min y, max x, max y) a quad covers,
/// clipped to the near plane and the screen; None when off screen.
fn screen_rect(corners: &[Vec3; 4], view_projection: Mat4) -> Option<[f32; 4]> {
    let clip: Vec<Vec4> = corners
        .iter()
        .map(|p| view_projection * p.extend(1.0))
        .collect();
    // Sutherland-Hodgman against the near plane (z >= 0, 0..1 depth).
    let mut kept = Vec::with_capacity(8);
    for i in 0..clip.len() {
        let (a, b) = (clip[i], clip[(i + 1) % clip.len()]);
        if a.z >= 0.0 {
            kept.push(a);
        }
        if (a.z >= 0.0) != (b.z >= 0.0) {
            let t = a.z / (a.z - b.z);
            kept.push(a + (b - a) * t);
        }
    }
    let mut rect = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    for c in kept {
        if c.w <= 1e-6 {
            continue;
        }
        let (x, y) = (c.x / c.w, c.y / c.w);
        rect = [rect[0].min(x), rect[1].min(y), rect[2].max(x), rect[3].max(y)];
    }
    let rect = [
        rect[0].max(-1.0),
        rect[1].max(-1.0),
        rect[2].min(1.0),
        rect[3].min(1.0),
    ];
    (rect[0] < rect[2] && rect[1] < rect[3]).then_some(rect)
}

/// Reflection through the plane `n·p + d = 0`.
pub fn reflection_matrix(plane: Vec4) -> Mat4 {
    let n = plane.xyz();
    let column = |axis: Vec3| (axis - 2.0 * n.dot(axis) * n).extend(0.0);
    Mat4::from_cols(
        column(Vec3::X),
        column(Vec3::Y),
        column(Vec3::Z),
        (-2.0 * plane.w * n).extend(1.0),
    )
}

/// `matrix` with its near plane moved onto `plane` (kept side positive),
/// by Lengyel's oblique frustum for 0..1 depth: nothing behind the mirror
/// draws, and the far plane still encloses the old frustum.
pub fn oblique(matrix: Mat4, plane: Vec4) -> Mat4 {
    let inverse = matrix.inverse();
    let clip = inverse.transpose() * plane;
    let corner = inverse * Vec4::new(clip.x.signum(), clip.y.signum(), 1.0, 1.0);
    let along = plane.dot(corner);
    if !along.is_finite() || along.abs() < 1e-9 {
        return matrix;
    }
    let row = plane * (matrix.row(3).dot(corner) / along);
    rows([matrix.row(0), matrix.row(1), row, matrix.row(3)])
}

fn rows(r: [Vec4; 4]) -> Mat4 {
    Mat4::from_cols(r[0], r[1], r[2], r[3]).transpose()
}

/// One view planes may be seen in: its clip matrix, the target pixels its
/// clip space fills, its eye, the reflections leading to it and the plane
/// it looks out of.
struct View {
    view_projection: Mat4,
    viewport: [f32; 4],
    eye: Vec3,
    unreflect: Mat4,
    own: Option<usize>,
}
/// A group a view sees: its screen rectangle in target pixels (x0, y0,
/// x1, y1) and the pixels it covers.
fn seen(
    view: &View,
    group: usize,
    plane: Vec4,
    members: &[usize],
    mirrors: &[Mirror],
    settings: &ReflectionSettings,
) -> Option<[f32; 4]> {
    if view.own == Some(group) || plane.xyz().dot(view.eye) + plane.w <= 1e-3 {
        return None;
    }
    // Reach runs along the reflected path: the view's eye is as far behind
    // its mirrors as the light has travelled.
    let near = members
        .iter()
        .flat_map(|&i| mirrors[i].corners)
        .map(|p| p.distance(view.eye))
        .fold(f32::INFINITY, f32::min);
    if near > settings.distance {
        return None;
    }
    let mut rect: Option<[f32; 4]> = None;
    for &i in members {
        if let Some(r) = screen_rect(&mirrors[i].corners, view.view_projection) {
            rect = Some(rect.map_or(r, |a| {
                [a[0].min(r[0]), a[1].min(r[1]), a[2].max(r[2]), a[3].max(r[3])]
            }));
        }
    }
    // Clip space to whole target pixels within the view's viewport.
    let [vx, vy, vw, vh] = view.viewport;
    let rect = rect?;
    let x0 = (vx + (rect[0] + 1.0) * 0.5 * vw).floor().max(vx);
    let x1 = (vx + (rect[2] + 1.0) * 0.5 * vw).ceil().min(vx + vw);
    let y0 = (vy + (1.0 - rect[3]) * 0.5 * vh).floor().max(vy);
    let y1 = (vy + (1.0 - rect[1]) * 0.5 * vh).ceil().min(vy + vh);
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1, y1])
}

/// Choose this frame's live planes. `target` is the reflection textures'
/// size, a `settings.scale` share of the screen. Planes the player sees
/// and planes seen in their reflections (two mirrors facing each other)
/// compete for the setting's passes by the screen they fill, so a bounce
/// deeper costs a pass only when it shows; beyond the passes, mirrors are
/// silver.
pub fn plan(
    mirrors: &[Mirror],
    view_projection: Mat4,
    eye: Vec3,
    settings: &ReflectionSettings,
    target: (u32, u32),
) -> Plan {
    let mut out = Plan::default();
    // The nearest mirrors, each by its nearest corner.
    let mut near: Vec<(f32, usize, Vec4)> = mirrors
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let plane = m.plane()?;
            let distance = m
                .corners
                .iter()
                .map(|p| p.distance(eye))
                .fold(f32::INFINITY, f32::min);
            Some((distance, i, plane))
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    near.truncate(MAX_MIRRORS);
    out.drawn = near.iter().map(|(_, i, _)| *i).collect();
    out.drawn.sort_unstable();
    // Coplanar mirrors, found by their plane rounded (a hundredth of a
    // degree, 5 mm), then kept in mirror order.
    let mut planes: Vec<Vec4> = Vec::new();
    let mut by_plane = std::collections::HashMap::new();
    for &i in &out.drawn {
        let plane = mirrors[i].plane().expect("drawn mirrors have planes");
        let key = (plane.xyz() * 1e4).round().as_ivec3().extend((plane.w * 200.0).round() as i32);
        let group = *by_plane.entry(key).or_insert_with(|| {
            planes.push(plane);
            out.groups.push(Vec::new());
            planes.len() - 1
        });
        out.groups[group].push(i);
    }
    if settings.planes == 0 || target.0 == 0 || target.1 == 0 {
        return out;
    }
    let (width, height) = (target.0 as f32, target.1 as f32);
    let mut views = vec![View {
        view_projection,
        viewport: [0.0, 0.0, width, height],
        eye,
        unreflect: Mat4::IDENTITY,
        own: None,
    }];
    // Candidates: (view, group, pixel rect); the largest goes live next.
    let mut candidates: Vec<(usize, usize, [f32; 4])> = Vec::new();
    let look = |view: usize, views: &[View], candidates: &mut Vec<_>| {
        for (group, members) in out.groups.iter().enumerate() {
            if let Some(rect) = seen(&views[view], group, planes[group], members, mirrors, settings) {
                candidates.push((view, group, rect));
            }
        }
    };
    look(0, &views, &mut candidates);
    while out.planes.len() < settings.planes {
        let area = |r: &[f32; 4]| (r[2] - r[0]) * (r[3] - r[1]);
        let Some(best) = (0..candidates.len()).max_by(|&a, &b| {
            area(&candidates[a].2)
                .total_cmp(&area(&candidates[b].2))
                .then(b.cmp(&a))
        }) else {
            break;
        };
        let (parent, group, [x0, y0, x1, y1]) = candidates.swap_remove(best);
        let plane = planes[group];
        let seen_from = &views[parent];
        // The parent's clip space over the covered pixels: left, right,
        // bottom, top.
        let [vx, vy, vw, vh] = seen_from.viewport;
        let (left, right) = ((x0 - vx) / vw * 2.0 - 1.0, (x1 - vx) / vw * 2.0 - 1.0);
        let (bottom, top) = (1.0 - (y1 - vy) / vh * 2.0, 1.0 - (y0 - vy) / vh * 2.0);
        let reflection = reflection_matrix(plane);
        let mirrored = oblique(seen_from.view_projection * reflection, plane);
        let w = mirrored.row(3);
        let view_projection = rows([
            -(2.0 * mirrored.row(0) - (left + right) * w) / (right - left),
            (2.0 * mirrored.row(1) - (bottom + top) * w) / (top - bottom),
            mirrored.row(2),
            w,
        ]);
        let unreflect = reflection * seen_from.unreflect;
        let reflected_eye = reflection.transform_point3(seen_from.eye);
        out.planes.push(PlannedPlane {
            plane,
            view_projection,
            eye: reflected_eye,
            viewport: [x0, y0, x1 - x0, y1 - y0],
            mirror_u: (x0 + x1) / width,
            parent,
            group,
            unreflect,
        });
        views.push(View {
            view_projection,
            viewport: [x0, y0, x1 - x0, y1 - y0],
            eye: reflected_eye,
            unreflect,
            own: Some(group),
        });
        look(views.len() - 1, &views, &mut candidates);
    }
    out
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MirrorVertex {
    position: [f32; 3],
    tint: [f32; 4],
}
/// One view's camera and fog for mirror surfaces.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniform {
    view_projection: [f32; 16],
    eye: [f32; 4],
    fog_color: [f32; 4],
    atmosphere: [f32; 4],
    /// Target width and height in pixels.
    screen: [f32; 4],
}
/// `mirror_u`, then 1 when the slot shows a live reflection.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SlotUniform {
    /// Echo: the view the plane was seen in, world to clip space.
    reproject: [f32; 16],
    /// mirror_u, then 1 live or 2 echo (0 silver).
    sample: [f32; 4],
    /// Echo: that view's viewport, then the plane's, in target pixels.
    parent: [f32; 4],
    viewport: [f32; 4],
    /// Target width and height in pixels.
    target: [f32; 4],
}
impl SlotUniform {
    fn silver() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

struct Target {
    /// Multisampled colour when the world pass is; it resolves into `picture`.
    color: Option<wgpu::TextureView>,
    picture: wgpu::TextureView,
    depth: wgpu::TextureView,
}
struct Bound {
    buffer: wgpu::Buffer,
    group: wgpu::BindGroup,
}

/// Mirror surfaces and the textures their reflections render into, for
/// one colour format and sample count (the world pass's).
pub struct Reflections {
    settings: ReflectionSettings,
    samples: u32,
    format: wgpu::TextureFormat,
    /// Reflection texture size.
    size: (u32, u32),
    targets: Vec<Target>,
    /// Opaque (strength 1), then blended.
    pipelines: [wgpu::RenderPipeline; 2],
    frame_layout: wgpu::BindGroupLayout,
    slot_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// One per view: the player's, then each live plane's.
    frames: Vec<Bound>,
    /// One per live plane, then the silver one every other mirror shows.
    slots: Vec<Bound>,
    silver: wgpu::TextureView,
    vertices: Option<wgpu::Buffer>,
    /// Vertex ranges per pipeline, by coplanar group.
    ranges: [Vec<Range<u32>>; 2],
    plan: Plan,
    camera: Camera,
}

const SHADER: &str = include_str!("mirror.wgsl");

impl Reflections {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        samples: u32,
        settings: ReflectionSettings,
    ) -> Self {
        let uniform = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirror frame"),
            entries: &[uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let slot_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mirror reflection"),
            entries: &[
                uniform(0, wgpu::ShaderStages::FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mirror surfaces"),
            bind_group_layouts: &[Some(&frame_layout), Some(&slot_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mirror surfaces"),
            source: wgpu::ShaderSource::Wgsl(crate::color::shader_source(SHADER).into()),
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
        let pipeline = |blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mirror surfaces"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<MirrorVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(blend.is_none()),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &crate::color::output_constants(format),
                        ..Default::default()
                    },
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipelines = [pipeline(None), pipeline(Some(wgpu::BlendState::ALPHA_BLENDING))];
        let silver = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("silver mirror"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mirror reflection"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            settings,
            samples,
            format,
            size: (0, 0),
            targets: Vec::new(),
            pipelines,
            frame_layout,
            slot_layout,
            sampler,
            frames: Vec::new(),
            slots: Vec::new(),
            silver,
            vertices: None,
            ranges: Default::default(),
            plan: Plan::default(),
            camera: Camera::default(),
        }
    }
    pub fn settings(&self) -> ReflectionSettings {
        self.settings
    }
    /// A new setting frees or grows the reflection textures on the next
    /// frame that needs them.
    pub fn set_settings(&mut self, settings: ReflectionSettings) {
        if settings != self.settings {
            self.settings = settings;
            self.targets.clear();
            self.slots.clear();
        }
    }
    pub fn matches(&self, format: wgpu::TextureFormat, samples: u32) -> bool {
        self.format == format && self.samples == samples
    }
    /// This frame's plan, after `prepare`.
    pub fn plan(&self) -> &Plan {
        &self.plan
    }
    /// Live planes this frame.
    pub fn live(&self) -> usize {
        self.plan.planes.len()
    }
    /// Plan the frame and upload what it draws: `camera` is the player's
    /// (as last given to `update_camera`) and `screen` the world target's
    /// size. Gives the renderer a view per live plane.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut SceneRenderer,
        camera: &Camera,
        screen: (u32, u32),
        mirrors: &[Mirror],
    ) -> Result<()> {
        ensure!(screen.0 > 0 && screen.1 > 0, "Empty mirror screen");
        let settings = self.settings;
        let size = (
            ((screen.0 as f32 * settings.scale).ceil() as u32).clamp(1, screen.0),
            ((screen.1 as f32 * settings.scale).ceil() as u32).clamp(1, screen.1),
        );
        if size != self.size {
            self.size = size;
            self.targets.clear();
            self.slots.clear();
        }
        let eye = Vec4::from(camera.eye).truncate();
        self.plan = plan(
            mirrors,
            Mat4::from_cols_array(&camera.view_projection),
            eye,
            &settings,
            self.size,
        );
        self.camera = *camera;
        let live = self.plan.planes.len();
        // Textures only once a mirror is live, then kept for the setting.
        while self.targets.len() < live {
            self.targets.push(self.target(device));
            self.slots.clear();
        }
        // Each target shows live, then silver, then each target echoed.
        let kept = self.targets.len();
        if self.slots.len() != 2 * kept + 1 {
            let pictures = || self.targets.iter().map(|t| &t.picture);
            self.slots = pictures()
                .chain(std::iter::once(&self.silver))
                .chain(pictures())
                .map(|view| self.bound_slot(device, view))
                .collect();
        }
        let target = [self.size.0 as f32, self.size.1 as f32, 0.0, 0.0];
        for i in 0..kept {
            let (live, echo) = match self.plan.planes.get(i) {
                Some(plane) => {
                    let (reproject, parent) = match plane.parent.checked_sub(1) {
                        None => (
                            Mat4::from_cols_array(&camera.view_projection),
                            [0.0, 0.0, target[0], target[1]],
                        ),
                        Some(p) => {
                            let seen_from = &self.plan.planes[p];
                            (seen_from.view_projection, seen_from.viewport)
                        }
                    };
                    let uniform = |mode: f32| SlotUniform {
                        reproject: reproject.to_cols_array(),
                        sample: [plane.mirror_u, mode, 0.0, 0.0],
                        parent,
                        viewport: plane.viewport,
                        target,
                    };
                    (uniform(1.0), uniform(2.0))
                }
                // A kept texture no plane uses this frame: silver.
                None => (SlotUniform::silver(), SlotUniform::silver()),
            };
            queue.write_buffer(&self.slots[i].buffer, 0, bytemuck::bytes_of(&live));
            queue.write_buffer(&self.slots[kept + 1 + i].buffer, 0, bytemuck::bytes_of(&echo));
        }
        renderer.set_view_count(device, 1 + live);
        while self.frames.len() < 1 + live {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mirror frame"),
                size: std::mem::size_of::<FrameUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mirror frame"),
                layout: &self.frame_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            self.frames.push(Bound { buffer, group });
        }
        let frame = |view_projection: Mat4, eye: Vec3, size: (u32, u32)| FrameUniform {
            view_projection: view_projection.to_cols_array(),
            eye: eye.extend(1.0).to_array(),
            fog_color: camera.fog_color,
            atmosphere: camera.atmosphere,
            screen: [size.0 as f32, size.1 as f32, 0.0, 0.0],
        };
        queue.write_buffer(
            &self.frames[0].buffer,
            0,
            bytemuck::bytes_of(&frame(
                Mat4::from_cols_array(&camera.view_projection),
                eye,
                screen,
            )),
        );
        for (i, plane) in self.plan.planes.iter().enumerate() {
            queue.write_buffer(
                &self.frames[1 + i].buffer,
                0,
                bytemuck::bytes_of(&frame(plane.view_projection, plane.eye, self.size)),
            );
            renderer.update_view(
                queue,
                1 + i,
                &Camera {
                    view_projection: plane.view_projection.to_cols_array(),
                    eye: plane.eye.extend(1.0).to_array(),
                    ..*camera
                },
            );
        }
        // Surfaces by pipeline, then by coplanar group.
        let mut vertices = Vec::new();
        for (p, ranges) in self.ranges.iter_mut().enumerate() {
            ranges.clear();
            for members in &self.plan.groups {
                let start = vertices.len() as u32;
                for &i in members {
                    let mirror = &mirrors[i];
                    if usize::from(mirror.strength < 1.0) != p {
                        continue;
                    }
                    let tint = [mirror.tint[0], mirror.tint[1], mirror.tint[2], mirror.strength];
                    for corner in [0, 1, 2, 0, 2, 3] {
                        vertices.push(MirrorVertex {
                            position: mirror.corners[corner].to_array(),
                            tint,
                        });
                    }
                }
                ranges.push(start..vertices.len() as u32);
            }
        }
        if !vertices.is_empty() {
            let bytes: &[u8] = bytemuck::cast_slice(&vertices);
            if self
                .vertices
                .as_ref()
                .is_none_or(|b| b.size() < bytes.len() as u64)
            {
                self.vertices = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("mirror surfaces"),
                    contents: &vec![0; bytes.len().next_power_of_two()],
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                }));
            }
            if let Some(buffer) = &self.vertices {
                queue.write_buffer(buffer, 0, bytes);
            }
        }
        Ok(())
    }
    fn target(&self, device: &wgpu::Device) -> Target {
        let texture = |label, samples, format, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: self.size.0,
                        height: self.size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        Target {
            color: (self.samples > 1).then(|| {
                texture("mirror reflection samples", self.samples, self.format, attachment)
            }),
            picture: texture(
                "mirror reflection",
                1,
                self.format,
                attachment | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
            depth: texture("mirror reflection depth", self.samples, DEPTH_FORMAT, attachment),
        }
    }
    fn bound_slot(&self, device: &wgpu::Device, picture: &wgpu::TextureView) -> Bound {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mirror reflection"),
            contents: bytemuck::bytes_of(&SlotUniform::silver()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirror reflection"),
            layout: &self.slot_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(picture),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        Bound { buffer, group }
    }
    /// Draw each live plane's reflection: the world from its reflected view,
    /// the planes seen in it (drawn first) on their mirrors and other
    /// mirrors in silver. `scenes` and `instances` are what the
    /// mirrors may show (the player's own body even in first person);
    /// `after` records last in each plane's pass, given its view (1 + the
    /// plane's index), for what draws outside the scene renderer.
    pub fn render(
        &self,
        renderer: &SceneRenderer,
        encoder: &mut wgpu::CommandEncoder,
        scenes: &[&GpuScene],
        instances: &[(&GpuScene, &GpuInstances)],
        clear: wgpu::Color,
        after: &dyn Fn(&mut wgpu::RenderPass<'_>, usize),
    ) {
        // A plane is planned after the view it is seen in: deepest first.
        for (i, plane) in self.plan.planes.iter().enumerate().rev() {
            let Some(target) = self.targets.get(i) else {
                continue;
            };
            let surfaces = |pass: &mut wgpu::RenderPass<'_>| self.draw_surfaces(pass, 1 + i);
            let late = |pass: &mut wgpu::RenderPass<'_>| after(pass, 1 + i);
            renderer.render_world(
                encoder,
                WorldPass {
                    view: 1 + i,
                    color: target.color.as_ref().unwrap_or(&target.picture),
                    resolve: target.color.as_ref().map(|_| &target.picture),
                    depth: &target.depth,
                    viewport: Some(plane.viewport),
                    clear: Some(clear),
                    after_opaque: Some(&surfaces),
                    after_all: Some(&late),
                },
                scenes,
                instances,
            );
        }
    }
    /// Mirror surfaces as `view` sees them (0 the player's, 1 + i live
    /// plane i's), for `WorldPass::after_opaque`.
    pub fn draw_surfaces(&self, pass: &mut wgpu::RenderPass<'_>, view: usize) {
        let (Some(vertices), Some(frame)) = (&self.vertices, self.frames.get(view)) else {
            return;
        };
        let slots = self.plan.slots(view);
        let kept = self.targets.len();
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_bind_group(0, &frame.group, &[]);
        for (pipeline, ranges) in self.pipelines.iter().zip(&self.ranges) {
            if ranges.iter().all(|r| r.is_empty()) {
                continue;
            }
            pass.set_pipeline(pipeline);
            // Neighbouring groups showing the same slot draw as one run:
            // silver runs broken only by the planes this view sees.
            let mut run: Option<(Shows, Range<u32>)> = None;
            let mut flush = |run: Option<(Shows, Range<u32>)>| {
                if let Some((shows, range)) = run
                    && !range.is_empty()
                    && let Some(bound) = self.slots.get(match shows {
                        Shows::Live(i) => i,
                        Shows::Silver => kept,
                        Shows::Echo(i) => kept + 1 + i,
                    })
                {
                    pass.set_bind_group(1, &bound.group, &[]);
                    pass.draw(range, 0..1);
                }
            };
            for (range, slot) in ranges.iter().zip(&slots) {
                match (&mut run, slot) {
                    (Some((current, r)), Some(slot)) if current == slot && r.end == range.start => {
                        r.end = range.end;
                    }
                    (_, Some(slot)) => flush(run.replace((*slot, range.clone()))),
                    (_, None) => flush(run.take()),
                }
            }
            flush(run);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera(eye: Vec3, target: Vec3) -> Mat4 {
        let view = glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y);
        glam::camera::rh::proj::directx::perspective(1.2, 16.0 / 9.0, 0.05, 1000.0) * view
    }
    /// A 2x2 mirror in the plane z = 0 facing +z, centred at `x`.
    fn wall(x: f32) -> Mirror {
        Mirror {
            corners: [
                Vec3::new(x - 1.0, -1.0, 0.0),
                Vec3::new(x + 1.0, -1.0, 0.0),
                Vec3::new(x + 1.0, 1.0, 0.0),
                Vec3::new(x - 1.0, 1.0, 0.0),
            ],
            tint: [1.0; 3],
            strength: 1.0,
        }
    }

    #[test]
    fn the_reflected_view_sees_the_viewer_where_the_mirror_shows_them() {
        let eye = Vec3::new(0.3, 0.2, 4.0);
        let main = camera(eye, Vec3::ZERO);
        let mirrors = [wall(0.0)];
        let plan = plan(&mirrors, main, eye, &ReflectionSettings::MEDIUM, (960, 540));
        assert_eq!(plan.groups, vec![vec![0]]);
        let plane = plan.planes[0];
        assert_eq!((plane.parent, plane.group), (0, 0));
        assert!(plane.eye.abs_diff_eq(Vec3::new(0.3, 0.2, -4.0), 1e-5));
        // Something in front of the mirror (a head beside the viewer) lands
        // in the reflection where the mirror's surface shows it: the screen
        // point where the eye's ray meets the mirror, flipped across the
        // viewport as the surface shader samples it.
        let head = Vec3::new(-0.2, 0.1, 2.0);
        let image = Vec3::new(head.x, head.y, -head.z);
        let seen = main.project_point3(image);
        let hit = plane.view_projection.project_point3(head);
        let [x, y, w, h] = plane.viewport;
        let texel = Vec3::new((hit.x + 1.0) * 0.5 * w + x, (1.0 - hit.y) * 0.5 * h + y, 0.0);
        let screen_u = (seen.x + 1.0) * 0.5;
        let sampled_u = plane.mirror_u - screen_u;
        assert!((texel.x / 960.0 - sampled_u).abs() < 1e-3, "{texel} {sampled_u}");
        assert!((texel.y / 540.0 - (1.0 - seen.y) * 0.5).abs() < 1e-3);
        // Behind the mirror clips at its near plane; in front stays in.
        assert!(hit.z > 0.0 && hit.z < 1.0);
        let behind = plane.view_projection * Vec4::new(0.0, 0.0, -1.0, 1.0);
        assert!(behind.z < 0.0);
        // Winding is kept, so the reflection draws with the usual culling: a
        // face turned to the mirror is front-facing in it, as one turned to
        // the viewer is on screen.
        let facing = |z: f32| {
            let (a, b) = (Vec3::new(0., 0., 2.), Vec3::new(0.2, 0., 2.));
            [a, b, Vec3::new(0., 0.2 * z, 2.)]
        };
        let area = |m: Mat4, tri: [Vec3; 3]| {
            let [a, b, c] = tri.map(|p| m.project_point3(p));
            (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
        };
        assert!(area(main, facing(1.0)) > 0.0);
        assert!(area(plane.view_projection, facing(-1.0)) > 0.0);
        assert!(area(plane.view_projection, facing(1.0)) < 0.0);
    }

    #[test]
    fn coplanar_mirrors_share_a_plane_and_the_largest_planes_go_live() {
        let eye = Vec3::new(0.0, 0.0, 6.0);
        let main = camera(eye, Vec3::ZERO);
        // Three bricks of one wall, and a small mirror on a nearer plane.
        let mut small = wall(0.0);
        for corner in &mut small.corners {
            *corner = *corner * 0.1 + Vec3::new(2.0, 0.0, 1.0);
        }
        let mirrors = [wall(-2.0), wall(0.0), wall(2.0), small];
        let one = ReflectionSettings {
            planes: 1,
            ..ReflectionSettings::MEDIUM
        };
        let p = plan(&mirrors, main, eye, &one, (960, 540));
        assert_eq!(p.planes.len(), 1);
        assert_eq!(p.groups, vec![vec![0, 1, 2], vec![3]]);
        assert_eq!(p.slots(0), vec![Some(Shows::Live(0)), Some(Shows::Silver)]);
        assert_eq!(p.drawn, vec![0, 1, 2, 3]);
        let p = plan(&mirrors, main, eye, &ReflectionSettings::MEDIUM, (960, 540));
        assert_eq!((p.planes[1].parent, p.planes[1].group), (0, 1));
        // Off, out of reach, behind the viewer or seen from behind: silver.
        let off = plan(&mirrors, main, eye, &ReflectionSettings::OFF, (960, 540));
        assert!(off.planes.is_empty() && off.drawn.len() == 4);
        let far = ReflectionSettings {
            distance: 3.0,
            ..ReflectionSettings::MEDIUM
        };
        assert!(plan(&mirrors, main, eye, &far, (960, 540)).planes.is_empty());
        let behind = Vec3::new(0.0, 0.0, -6.0);
        let back = camera(behind, Vec3::new(0.0, 0.0, -20.0));
        assert!(
            plan(&mirrors, back, behind, &ReflectionSettings::MEDIUM, (960, 540))
                .planes
                .is_empty()
        );
        let facing = camera(behind, Vec3::ZERO);
        assert!(
            plan(&mirrors[..3], facing, behind, &ReflectionSettings::MEDIUM, (960, 540))
                .planes
                .is_empty()
        );
    }

    #[test]
    fn facing_mirrors_show_each_other_a_bounce_deeper_within_the_passes() {
        // The viewer between two facing mirrors looks at one (z = 0); the
        // other (z = 6, facing back) is behind them, seen only in the first.
        let eye = Vec3::new(0.3, 0.2, 4.0);
        let main = camera(eye, Vec3::ZERO);
        let mut back = wall(0.0);
        back.corners = [0, 3, 2, 1].map(|i| wall(0.0).corners[i] * 2.0 + Vec3::new(0.0, 0.0, 6.0));
        let mirrors = [wall(0.0), back];
        let p = plan(&mirrors, main, eye, &ReflectionSettings::MEDIUM, (960, 540));
        assert_eq!(p.planes.len(), 2);
        let (first, second) = (p.planes[0], p.planes[1]);
        assert_eq!((first.parent, first.group, second.parent, second.group), (0, 0, 1, 1));
        // Twice reflected: the eye behind the far mirror, as far as the
        // light travelled.
        assert!(second.eye.abs_diff_eq(Vec3::new(0.3, 0.2, 16.0), 1e-4));
        // The player's view shows the first plane (the back mirror is off
        // screen); the first plane's view shows the second, and the
        // second's shows the first mirror as it last looked, out of passes.
        assert_eq!(p.slots(0), vec![Some(Shows::Live(0)), Some(Shows::Echo(1))]);
        assert_eq!(p.slots(1), vec![None, Some(Shows::Live(1))]);
        assert_eq!(p.slots(2), vec![Some(Shows::Echo(0)), None]);
        // A head between the mirrors, seen by way of the back mirror and
        // then the front one, lands where the two surfaces sample it.
        let head = Vec3::new(-0.2, 0.1, 5.0);
        let image = reflection_matrix(first.plane)
            .transform_point3(reflection_matrix(second.plane).transform_point3(head));
        let seen = main.project_point3(image);
        let texel = |plane: &PlannedPlane, point: Vec3| {
            let hit = plane.view_projection.project_point3(point);
            let [x, y, w, h] = plane.viewport;
            ((hit.x + 1.0) * 0.5 * w + x, (1.0 - hit.y) * 0.5 * h + y)
        };
        let screen_x = (seen.x + 1.0) * 0.5 * 960.0;
        let in_first = first.mirror_u * 960.0 - screen_x;
        let in_second = second.mirror_u * 960.0 - in_first;
        let (x, y) = texel(&second, head);
        assert!((x - in_second).abs() < 0.5, "{x} {in_second}");
        assert!((y - (1.0 - seen.y) * 0.5 * 540.0).abs() < 0.5);
        // Billboards in the second view face its eye.
        assert!(second.reflect_direction(Vec3::Z).abs_diff_eq(Vec3::Z, 1e-5));
        assert!(first.reflect_direction(Vec3::Z).abs_diff_eq(Vec3::NEG_Z, 1e-5));
        // With one pass the back mirror stays silver.
        let low = plan(&mirrors, main, eye, &ReflectionSettings::LOW, (960, 540));
        assert_eq!(low.planes.len(), 1);
        assert_eq!(low.slots(1), vec![None, Some(Shows::Silver)]);
    }

    #[test]
    fn a_mirror_the_eye_stands_beside_covers_only_its_part_of_the_screen() {
        let eye = Vec3::new(0.0, 0.0, 3.0);
        // Looking along the wall's right half at an angle: around a corner.
        let main = camera(eye, Vec3::new(-3.0, 0.0, 0.0));
        let plan = plan(&[wall(-2.0)], main, eye, &ReflectionSettings::MEDIUM, (960, 540));
        let [x, _, w, _] = plan.planes[0].viewport;
        assert!(w < 960.0 && x + w <= 960.0);
        assert!(plan.planes[0].view_projection.is_finite());
    }

    #[test]
    fn bad_mirrors_are_left_out() {
        let mut flat = wall(0.0);
        flat.corners[2] = flat.corners[1];
        flat.corners[3] = flat.corners[0];
        let mut nan = wall(0.0);
        nan.corners[0].x = f32::NAN;
        let clear = Mirror {
            strength: 0.0,
            ..wall(0.0)
        };
        let eye = Vec3::new(0.0, 0.0, 4.0);
        let plan = plan(
            &[flat, nan, clear],
            camera(eye, Vec3::ZERO),
            eye,
            &ReflectionSettings::HIGH,
            (100, 100),
        );
        assert!(plan.drawn.is_empty() && plan.planes.is_empty());
    }
}
