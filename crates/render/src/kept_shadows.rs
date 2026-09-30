//! Kept brick layers for the sun's cascades. Bricks hardly ever move, yet
//! with Brick Shadows on every brick chunk in reach of a cascade was drawn
//! into it again each frame, four cascades at Best: in a large build that
//! was most of the frame. As engines cache static shadow casters, each
//! cascade now keeps its static chunks' depth in a layer of its own that
//! covers twice the cascade's width around the eye, on the same texel grid
//! the cascade snaps to. A frame copies the cascade's square out of it
//! (converting depth to the cascade's range) and draws only the moving
//! casters (players, vehicles, items) on top.
//!
//! Turning the camera moves a cascade inside its kept layer, so nothing is
//! drawn again; the layer is drawn again around the eye only when the
//! cascade would leave it (after walking a fair way) or the sun turns, one
//! layer a frame (the others draw directly meanwhile, as before). A brick
//! placed or removed redraws only its part of each kept layer, the same
//! frame; a large change (a build streaming in) redraws the layer.
use crate::scene::GpuScene;
use crate::shadow::{CASTER_REACH, Cascade, SHADOW_FORMAT};
use glam::{Mat4, Vec3};
use std::cell::RefCell;
use std::collections::HashMap;

/// A kept layer's width in cascade widths.
const WIDTH_FACTOR: u32 = 2;
/// Blit uniform stride; dynamic offsets must be 256-byte aligned.
const BLIT_STRIDE: u64 = 256;
/// A changed area larger than this share of a kept layer redraws it whole.
const REGION_SHARE: f32 = 0.25;
/// Texels a changed region is widened by: the casters' depth bias and
/// rasterization reach past a chunk's bounds by at most a texel.
const REGION_MARGIN_TEXELS: i64 = 2;

/// Where a kept layer lies in light space (see `Cascade`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    sun: Vec3,
    texel: f32,
    center_texels: [i64; 2],
    half_texels: i64,
    depth_range: [f32; 2],
}
impl Placement {
    /// A kept layer `size` texels wide around the eye, on `cascade`'s grid.
    fn around(eye: Vec3, sun: Vec3, cascade: &Cascade, size: u32) -> Self {
        let light = crate::shadow::light_rotation(sun).transform_point3(eye);
        let half = i64::from(size / 2);
        let reach = half as f32 * cascade.texel;
        let depth = -light.z;
        Self {
            sun,
            texel: cascade.texel,
            center_texels: [
                (light.x / cascade.texel).round() as i64,
                (light.y / cascade.texel).round() as i64,
            ],
            half_texels: half,
            depth_range: [depth - reach - CASTER_REACH, depth + reach],
        }
    }
    /// Whether `cascade`'s map lies inside this layer, on the same grid.
    fn holds(&self, sun: Vec3, cascade: &Cascade) -> bool {
        self.sun == sun
            && self.texel == cascade.texel
            && (0..2).all(|axis| {
                (cascade.center_texels[axis] - self.center_texels[axis]).abs() + cascade.half_texels
                    <= self.half_texels
            })
            && cascade.depth_range[0] >= self.depth_range[0]
            && cascade.depth_range[1] <= self.depth_range[1]
    }
    fn view_projection(&self) -> Mat4 {
        let t = |texels: i64| texels as f32 * self.texel;
        let [x, y] = self.center_texels;
        let h = self.half_texels;
        glam::camera::rh::proj::directx::orthographic(
            t(x - h),
            t(x + h),
            t(y - h),
            t(y + h),
            self.depth_range[0],
            self.depth_range[1],
        ) * crate::shadow::light_rotation(self.sun)
    }
    /// The blit's uniform for `cascade`: the kept texel under the
    /// cascade's first texel, and kept depth to cascade depth (`z * a + b`).
    fn blit(&self, cascade: &Cascade) -> [f32; 4] {
        // Texel columns grow with x; rows grow downward, against y.
        let x = cascade.center_texels[0]
            - cascade.half_texels
            - (self.center_texels[0] - self.half_texels);
        let y = (self.center_texels[1] + self.half_texels)
            - (cascade.center_texels[1] + cascade.half_texels);
        let kept = self.depth_range[1] - self.depth_range[0];
        let own = cascade.depth_range[1] - cascade.depth_range[0];
        [
            x as f32,
            y as f32,
            kept / own,
            (self.depth_range[0] - cascade.depth_range[0]) / own,
        ]
    }
}

/// What a frame does with a cascade's kept layer.
pub(crate) enum Use<'a> {
    /// Draw the static casters straight into the cascade, as before.
    Direct,
    /// Copy the cascade from its kept layer (after `redraw`, when given)
    /// and draw only the moving casters.
    Kept { redraw: Option<Redraw<'a>> },
}
/// Static casters to draw into a kept layer first: all of them (the layer
/// cleared), or those over a changed region (scissored, cleared there).
pub(crate) struct Redraw<'a> {
    pub matrix: Mat4,
    pub scenes: Vec<&'a GpuScene>,
    pub scissor: Option<[u32; 4]>,
}

pub(crate) struct KeptShadows {
    pub size: u32,
    views: Vec<wgpu::TextureView>,
    uniform: wgpu::Buffer,
    groups: Vec<wgpu::BindGroup>,
    pipeline: wgpu::RenderPipeline,
    state: RefCell<State>,
}
#[derive(Default)]
struct State {
    placements: Vec<Option<Placement>>,
    /// The static casters as last planned: identity and bounds.
    casters: HashMap<u64, (Vec3, Vec3)>,
}

impl KeptShadows {
    /// Layers for `cascades` cascades of `resolution` texels; None when the
    /// device cannot hold them.
    pub fn new(device: &wgpu::Device, cascades: u32, resolution: u32) -> Option<Self> {
        let size = resolution * WIDTH_FACTOR;
        if size > device.limits().max_texture_dimension_2d || cascades == 0 {
            return None;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("kept sun shadow bricks"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: cascades,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SHADOW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let views: Vec<_> = (0..cascades)
            .map(|layer| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("kept sun shadow blit"),
            size: BLIT_STRIDE * u64::from(cascades),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kept sun shadow blit"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
            ],
        });
        let groups = views
            .iter()
            .enumerate()
            .map(|(cascade, view)| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("kept sun shadow blit"),
                    layout: &layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &uniform,
                                offset: cascade as u64 * BLIT_STRIDE,
                                size: wgpu::BufferSize::new(16),
                            }),
                        },
                    ],
                })
            })
            .collect();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kept sun shadow blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kept sun shadow blit"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("kept sun shadow blit"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            multiview_mask: None,
            cache: None,
        });
        Some(Self {
            size,
            views,
            uniform,
            groups,
            pipeline,
            state: RefCell::new(State {
                placements: vec![None; cascades as usize],
                casters: HashMap::new(),
            }),
        })
    }
    pub fn view(&self, cascade: usize) -> &wgpu::TextureView {
        &self.views[cascade]
    }
    /// Copies cascade `cascade` from its kept layer into the bound pass's
    /// attachment; the caller binds its own group 0 again afterwards.
    pub fn blit(&self, pass: &mut wgpu::RenderPass<'_>, cascade: usize) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.groups[cascade], &[]);
        pass.draw(0..3, 0..1);
    }
    /// This frame's use of each cascade's kept layer, given the static
    /// casters (`statics`, chunks with bounds) and the eye. Writes the blit
    /// uniforms and, through `set_matrix`, each redrawn layer's matrix.
    pub fn plan<'a>(
        &self,
        queue: &wgpu::Queue,
        cascades: &[Cascade],
        sun: Vec3,
        eye: Vec3,
        statics: &[&'a GpuScene],
        set_matrix: impl Fn(usize, Mat4),
    ) -> Vec<Use<'a>> {
        let mut state = self.state.borrow_mut();
        let now: HashMap<u64, (Vec3, Vec3)> = statics
            .iter()
            .filter_map(|s| Some((caster_identity(s), s.bounds?)))
            .collect();
        // Bounds of every static caster added, removed or replaced.
        let changed: Vec<(Vec3, Vec3)> = now
            .iter()
            .filter(|(id, _)| !state.casters.contains_key(id))
            .chain(state.casters.iter().filter(|(id, _)| !now.contains_key(id)))
            .map(|(_, bounds)| *bounds)
            .collect();
        state.casters = now;
        state.placements.resize(cascades.len(), None);
        let mut full_redraw_left = true;
        let mut uses = Vec::with_capacity(cascades.len());
        for (index, cascade) in cascades.iter().enumerate() {
            let mut redraw = None;
            let held = state.placements[index].filter(|p| p.holds(sun, cascade));
            let placement = match held {
                Some(placement) => {
                    // Redraw what changed inside the layer, or all of it
                    // when that is most of it.
                    let matrix = placement.view_projection();
                    match changed_region(matrix, &changed, self.size) {
                        None => Some(placement),
                        Some(region) if region_share(region, self.size) <= REGION_SHARE => {
                            redraw = Some(Redraw {
                                matrix,
                                scenes: statics
                                    .iter()
                                    .copied()
                                    .filter(|s| {
                                        s.bounds
                                            .is_some_and(|b| overlaps(matrix, b, region, self.size))
                                    })
                                    .collect(),
                                scissor: Some(region),
                            });
                            Some(placement)
                        }
                        Some(_) => None,
                    }
                }
                None => None,
            };
            let placement = match placement {
                Some(placement) => Some(placement),
                None if full_redraw_left => {
                    // Around the eye, drawn whole: one layer a frame.
                    let fresh = Placement::around(eye, sun, cascade, self.size);
                    fresh.holds(sun, cascade).then(|| {
                        full_redraw_left = false;
                        let matrix = fresh.view_projection();
                        let planes = crate::scene::frustum_planes(matrix);
                        redraw = Some(Redraw {
                            matrix,
                            scenes: statics
                                .iter()
                                .copied()
                                .filter(|s| {
                                    s.bounds
                                        .is_some_and(|b| crate::scene::aabb_visible(&planes, b))
                                })
                                .collect(),
                            scissor: None,
                        });
                        fresh
                    })
                }
                None => None,
            };
            state.placements[index] = placement;
            match placement {
                Some(placement) => {
                    if let Some(redraw) = &redraw {
                        set_matrix(index, redraw.matrix);
                    }
                    queue.write_buffer(
                        &self.uniform,
                        index as u64 * BLIT_STRIDE,
                        bytemuck::cast_slice(&placement.blit(cascade)),
                    );
                    uses.push(Use::Kept { redraw });
                }
                None => uses.push(Use::Direct),
            }
        }
        uses
    }
}

/// A static caster's identity: the scene and where its geometry lies (a
/// chunk rebuilt in place gets new geometry).
fn caster_identity(scene: &GpuScene) -> u64 {
    crate::scene::kept_casters_key(std::iter::once(scene))
}

/// The texels (x, y, width, height) of a kept layer `size` wide that the
/// changed bounds cover under `matrix`, widened a little; None when none do.
fn changed_region(matrix: Mat4, changed: &[(Vec3, Vec3)], size: u32) -> Option<[u32; 4]> {
    let mut union: Option<[i64; 4]> = None;
    for &bounds in changed {
        let Some([x0, y0, x1, y1]) = texel_rect(matrix, bounds, size) else {
            continue;
        };
        union = Some(union.map_or([x0, y0, x1, y1], |u| {
            [u[0].min(x0), u[1].min(y0), u[2].max(x1), u[3].max(y1)]
        }));
    }
    let [x0, y0, x1, y1] = union?;
    let m = REGION_MARGIN_TEXELS;
    let clamp = |v: i64| v.clamp(0, i64::from(size)) as u32;
    let (x0, y0, x1, y1) = (clamp(x0 - m), clamp(y0 - m), clamp(x1 + m), clamp(y1 + m));
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
}
fn region_share([_, _, w, h]: [u32; 4], size: u32) -> f32 {
    (w as f32 * h as f32) / (size as f32 * size as f32)
}
/// Texel rectangle (x0, y0, x1, y1) of world `bounds` under `matrix` in a
/// map `size` wide; None when they miss it (or lie past its depth range).
fn texel_rect(matrix: Mat4, bounds: (Vec3, Vec3), size: u32) -> Option<[i64; 4]> {
    let (lo, hi) = crate::scene::clip_rect(matrix, bounds);
    if hi.x < -1.0 || lo.x > 1.0 || hi.y < -1.0 || lo.y > 1.0 || hi.z < 0.0 || lo.z > 1.0 {
        return None;
    }
    let size = size as f32;
    Some([
        ((lo.x * 0.5 + 0.5) * size).floor() as i64,
        ((0.5 - hi.y * 0.5) * size).floor() as i64,
        ((hi.x * 0.5 + 0.5) * size).ceil() as i64,
        ((0.5 - lo.y * 0.5) * size).ceil() as i64,
    ])
}
fn overlaps(matrix: Mat4, bounds: (Vec3, Vec3), [x, y, w, h]: [u32; 4], size: u32) -> bool {
    texel_rect(matrix, bounds, size).is_some_and(|[x0, y0, x1, y1]| {
        x0 <= i64::from(x + w) && x1 >= i64::from(x) && y0 <= i64::from(y + h) && y1 >= i64::from(y)
    })
}

/// A triangle over the pass; each texel takes its kept texel's depth,
/// converted to the cascade's depth range. Past the cascade's far plane
/// (and where the kept layer is clear) that is 1: nothing casts there.
const BLIT_SHADER: &str = r"
struct Blit { offset:vec2<f32>, remap:vec2<f32> };
@group(0) @binding(0) var kept:texture_depth_2d;
@group(0) @binding(1) var<uniform> blit:Blit;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
    let corner=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));
    return vec4<f32>(corner*2.0-vec2<f32>(1.0),1.0,1.0);
}
@fragment fn fs_main(@builtin(position) p:vec4<f32>)->@builtin(frag_depth) f32 {
    let texel=vec2<i32>(floor(p.xy))+vec2<i32>(blit.offset);
    let z=textureLoad(kept,texel,0);
    return clamp(z*blit.remap.x+blit.remap.y,0.0,1.0);
}
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shadow::{ShadowSettings, cascades_after};

    fn fitted_after(eye: Vec3, look: Vec3, sun: Vec3, previous: &[Cascade]) -> Vec<Cascade> {
        let view = glam::camera::rh::view::look_at_mat4(eye, look, Vec3::Y);
        let projection =
            glam::camera::rh::proj::directx::perspective(1.2, 21.0 / 9.0, 0.05, 1000.0);
        cascades_after(projection * view, eye, sun, &ShadowSettings::BEST, previous)
            .unwrap()
            .0
    }
    fn fitted(eye: Vec3, look: Vec3, sun: Vec3) -> Vec<Cascade> {
        fitted_after(eye, look, sun, &[])
    }

    #[test]
    fn a_kept_layer_holds_its_cascade_as_the_camera_turns() {
        let sun = Vec3::new(0.3, -1.0, 0.2).normalize();
        let eye = Vec3::new(10.0, 40.0, -5.0);
        let size = ShadowSettings::BEST.resolution * WIDTH_FACTOR;
        // As the game fits them: each frame after the last.
        let mut previous = fitted(eye, eye + Vec3::X, sun);
        let kept: Vec<Placement> = previous
            .iter()
            .map(|c| Placement::around(eye, sun, c, size))
            .collect();
        for turn in 0..16 {
            let angle = turn as f32 / 16.0 * std::f32::consts::TAU;
            for pitch in [-1.2f32, -0.6, 0.0, 0.6] {
                let look = Vec3::new(
                    angle.cos() * pitch.cos(),
                    pitch.sin(),
                    angle.sin() * pitch.cos(),
                );
                previous = fitted_after(eye, eye + look, sun, &previous);
                for (cascade, placement) in previous.iter().zip(&kept) {
                    assert!(
                        placement.holds(sun, cascade),
                        "turn {turn} pitch {pitch}: {cascade:?} in {placement:?}"
                    );
                }
            }
        }
        // Walking far enough away leaves the nearest cascade's layer.
        let far = eye + Vec3::new(60.0, 0.0, 0.0);
        let moved = fitted(far, far + Vec3::X, sun);
        assert!(!kept[0].holds(sun, &moved[0]));
        // A turned sun redraws.
        let turned = Vec3::new(0.35, -1.0, 0.2).normalize();
        assert!(!kept[0].holds(turned, &fitted(eye, eye + Vec3::X, turned)[0]));
    }

    #[test]
    fn the_blit_lands_on_the_cascades_own_texels_and_depths() {
        let sun = Vec3::new(0.3, -1.0, 0.2).normalize();
        let eye = Vec3::new(10.0, 40.0, -5.0);
        let size = ShadowSettings::BEST.resolution * WIDTH_FACTOR;
        let cascade = fitted(eye, eye + Vec3::new(1.0, -0.4, 0.3), sun)[2];
        let kept = Placement::around(eye, sun, &cascade, size);
        let [ox, oy, a, b] = kept.blit(&cascade);
        let resolution = ShadowSettings::BEST.resolution as f32;
        // A world point: its texel and depth in the cascade and in the
        // kept layer must agree through the blit's offset and remap.
        for point in [
            eye + Vec3::new(3.0, -20.0, 7.0),
            eye + Vec3::new(-15.0, -35.0, -4.0),
        ] {
            let c = cascade.view_projection.project_point3(point);
            let k = kept.view_projection().project_point3(point);
            let cx = (c.x * 0.5 + 0.5) * resolution;
            let cy = (0.5 - c.y * 0.5) * resolution;
            let kx = (k.x * 0.5 + 0.5) * size as f32;
            let ky = (0.5 - k.y * 0.5) * size as f32;
            assert!(
                (cx + ox - kx).abs() < 0.01 && (cy + oy - ky).abs() < 0.01,
                "{cx},{cy} + {ox},{oy} vs {kx},{ky}"
            );
            assert!(
                (k.z * a + b - c.z).abs() < 1e-5,
                "{} vs {}",
                k.z * a + b,
                c.z
            );
        }
    }
}
