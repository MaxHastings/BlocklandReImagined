//! Cascaded sun shadow maps. v20 drew per-shape projected shadows from
//! players, vehicles and items (quality from `setShadowResolution`), never
//! from bricks, and baked the map's own shadows into lightmaps. Here those
//! casters render into stabilized cascades instead; bricks are optional.
//!
//! Map geometry never casts. v20 lit bricks and players by the sun even
//! inside the Bedroom and Kitchen interiors, so map shadows on bricks would
//! darken nearly every indoor build (checked with Cottage and Town renders).
//! Lightmapped surfaces (which cannot separate their baked sun from other
//! light) darken by a bounded fixed share, as v20's projected shadows did.
//!
//! Surfaces that do not cast (bricks unless Brick Shadows is on, interiors,
//! terrain) still stop a shadow: they render into a second, occluder depth
//! map, and a caster's shadow is dropped wherever an occluder lies between
//! the caster and the receiving surface. A player on a brick tower shades
//! the tower top, not the floor beneath it.
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3, Vec4};

pub const MAX_CASCADES: usize = 4;
pub const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Casters this far beyond a cascade toward the sun still cast into it.
const CASTER_REACH: f32 = 400.0;
/// Caster uniform stride; dynamic offsets must be 256-byte aligned.
const CASTER_STRIDE: u64 = 256;

/// Sun shadow quality: how many cascades, their square resolution, and how
/// far from the eye shadows reach before fading out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSettings {
    pub cascades: u32,
    pub resolution: u32,
    pub distance: f32,
}
impl ShadowSettings {
    pub const BEST: Self = Self {
        cascades: 4,
        resolution: 2048,
        distance: 320.0,
    };
    pub const HIGH: Self = Self {
        cascades: 3,
        resolution: 2048,
        distance: 240.0,
    };
    pub const MEDIUM: Self = Self {
        cascades: 3,
        resolution: 1024,
        distance: 160.0,
    };
    pub const LOW: Self = Self {
        cascades: 2,
        resolution: 1024,
        distance: 100.0,
    };
    pub fn validate(&self, device: &wgpu::Device) -> Result<()> {
        ensure!(
            (1..=MAX_CASCADES as u32).contains(&self.cascades)
                && (256..=device.limits().max_texture_dimension_2d).contains(&self.resolution)
                && self.cascades * 2 <= device.limits().max_texture_array_layers
                && self.distance.is_finite()
                && (10.0..=2000.0).contains(&self.distance),
            "Invalid shadow settings {self:?}"
        );
        Ok(())
    }
}

/// Receiver uniform: cascade matrices, far split distances, world size of
/// one texel per cascade, view forward + cascade count, and map resolution.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ShadowUniform {
    matrices: [[f32; 16]; MAX_CASCADES],
    splits: [f32; 4],
    texels: [f32; 4],
    forward_count: [f32; 4],
    params: [f32; 4],
    /// Shadow-map depth per world unit along the sun, per cascade.
    depth_scale: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Cascade {
    pub view_projection: Mat4,
    pub far: f32,
    pub texel: f32,
    pub depth_scale: f32,
}

/// Split the view from its near plane to `settings.distance` and fit each
/// slice with a texel-snapped sphere in light space, so cascades do not
/// shimmer as the camera turns or moves.
pub(crate) fn cascades(
    view_projection: Mat4,
    eye: Vec3,
    sun_direction: Vec3,
    settings: &ShadowSettings,
) -> Option<(Vec<Cascade>, Vec3)> {
    let sun = sun_direction.normalize_or_zero();
    let inverse = view_projection.inverse();
    if sun == Vec3::ZERO || !inverse.is_finite() {
        return None;
    }
    let corner = |x: f32, y: f32, z: f32| inverse.project_point3(Vec3::new(x, y, z));
    let far: Vec<Vec3> = [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)]
        .iter()
        .map(|&(x, y)| corner(x, y, 1.0))
        .collect();
    let near_center = corner(0.0, 0.0, 0.0);
    let far_center = corner(0.0, 0.0, 1.0);
    let forward = (far_center - near_center).normalize_or_zero();
    let far_depth = (far_center - eye).dot(forward);
    let near_depth = (near_center - eye).dot(forward).max(0.01);
    if forward == Vec3::ZERO
        || far_depth.partial_cmp(&near_depth) != Some(std::cmp::Ordering::Greater)
    {
        return None;
    }
    let distance = settings.distance.min(far_depth);
    let count = settings.cascades as usize;
    // Practical split scheme: mostly logarithmic, partly uniform.
    let splits: Vec<f32> = (1..=count)
        .map(|i| {
            let t = i as f32 / count as f32;
            let log = near_depth * (distance / near_depth).powf(t);
            let uniform = near_depth + (distance - near_depth) * t;
            0.75 * log + 0.25 * uniform
        })
        .collect();
    let up = if sun.dot(Vec3::Y).abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let rotation = glam::camera::rh::view::look_to_mat4(Vec3::ZERO, sun, up);
    let point_at = |ray: Vec3, depth: f32| eye + (ray - eye) * (depth / far_depth);
    let mut start = near_depth;
    let mut out = Vec::with_capacity(count);
    for split in splits {
        let slice: Vec<Vec3> = far
            .iter()
            .flat_map(|&ray| [point_at(ray, start), point_at(ray, split)])
            .collect();
        let center = slice.iter().copied().sum::<Vec3>() / slice.len() as f32;
        let radius = slice
            .iter()
            .map(|p| p.distance(center))
            .fold(0.0f32, f32::max);
        // Quantize the radius so the texel size (and snapping grid) is stable.
        let radius = (radius * 4.0).ceil() / 4.0;
        let texel = radius * 2.0 / settings.resolution as f32;
        let light = rotation.transform_point3(center);
        let (x, y) = (
            (light.x / texel).round() * texel,
            (light.y / texel).round() * texel,
        );
        // Right-handed view looks down -Z: depth along the sun is -z.
        let depth = -light.z;
        let projection = glam::camera::rh::proj::directx::orthographic(
            x - radius,
            x + radius,
            y - radius,
            y + radius,
            depth - radius - CASTER_REACH,
            depth + radius,
        );
        out.push(Cascade {
            view_projection: projection * rotation,
            far: split,
            texel,
            depth_scale: 1.0 / (2.0 * radius + CASTER_REACH),
        });
        start = split;
    }
    Some((out, forward))
}

/// Shadow map textures, uniforms and caster pipelines. Disabled shadows keep
/// a 1x1 map and a zero cascade count so receivers need no variant.
pub(crate) struct ShadowMaps {
    pub settings: Option<ShadowSettings>,
    pub array_view: wgpu::TextureView,
    pub layer_views: Vec<wgpu::TextureView>,
    pub comparison: wgpu::Sampler,
    /// Nearest sampling for gathering caster and occluder depths.
    pub point: wgpu::Sampler,
    pub receiver: wgpu::Buffer,
    caster: wgpu::Buffer,
    pub caster_group: wgpu::BindGroup,
    /// Opaque (depth only) and alpha-masked caster pipelines.
    pub pipelines: [wgpu::RenderPipeline; 2],
    pub cascades: Vec<Cascade>,
}
impl ShadowMaps {
    pub fn new(
        device: &wgpu::Device,
        settings: Option<ShadowSettings>,
        material_layout: &wgpu::BindGroupLayout,
        vertex_layouts: &[Option<wgpu::VertexBufferLayout<'_>>],
    ) -> Self {
        // Per cascade: caster depth, then (after all cascades) occluder depth.
        let (size, layers) = settings.map_or((1, 2), |s| (s.resolution, s.cascades * 2));
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sun shadow maps"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SHADOW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let array_view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let layer_views = (0..layers)
            .map(|layer| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let comparison = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sun shadow comparison"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let point = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sun shadow depth gather"),
            ..Default::default()
        });
        use wgpu::util::DeviceExt;
        let receiver = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sun shadow receiver uniform"),
            contents: bytemuck::bytes_of(&ShadowUniform::zeroed_disabled()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let caster = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sun shadow caster matrices"),
            size: CASTER_STRIDE * MAX_CASCADES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let caster_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sun shadow caster"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let mask_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let caster_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sun shadow caster"),
            layout: &caster_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &caster,
                        offset: 0,
                        size: wgpu::BufferSize::new(64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&mask_sampler),
                },
            ],
        });
        // Opaque casters need no material, so whole chunks draw without
        // rebinding; masked casters sample their material's alpha.
        let opaque_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sun shadow casters"),
            bind_group_layouts: &[Some(&caster_layout)],
            immediate_size: 0,
        });
        let masked_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sun shadow masked casters"),
            bind_group_layouts: &[Some(&caster_layout), Some(material_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sun shadow casters"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shadow.wgsl").into()),
        });
        let pipeline = |masked: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if masked {
                    "sun shadow masked casters"
                } else {
                    "sun shadow casters"
                }),
                layout: Some(if masked {
                    &masked_layout
                } else {
                    &opaque_layout
                }),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: vertex_layouts,
                },
                // Single-sided map geometry must still block the sun.
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: SHADOW_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState {
                        constant: 2,
                        slope_scale: 2.0,
                        clamp: 0.0,
                    },
                }),
                multisample: Default::default(),
                fragment: masked.then(|| wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_masked"),
                    compilation_options: Default::default(),
                    targets: &[],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            settings,
            array_view,
            layer_views,
            comparison,
            point,
            receiver,
            caster,
            caster_group,
            pipelines: [pipeline(false), pipeline(true)],
            cascades: Vec::new(),
        }
    }
    /// Fit cascades to this frame's camera and upload caster/receiver data.
    pub fn update(&mut self, queue: &wgpu::Queue, view_projection: Mat4, eye: Vec3, sun: Vec3) {
        let fitted = self
            .settings
            .and_then(|settings| cascades(view_projection, eye, sun, &settings));
        let mut uniform = ShadowUniform::zeroed_disabled();
        self.cascades.clear();
        if let (Some(settings), Some((cascades, forward))) = (self.settings, fitted) {
            for (i, cascade) in cascades.iter().enumerate() {
                uniform.matrices[i] = cascade.view_projection.to_cols_array();
                uniform.splits[i] = cascade.far;
                uniform.texels[i] = cascade.texel;
                uniform.depth_scale[i] = cascade.depth_scale;
                queue.write_buffer(
                    &self.caster,
                    i as u64 * CASTER_STRIDE,
                    bytemuck::bytes_of(&cascade.view_projection.to_cols_array()),
                );
            }
            uniform.forward_count = forward.extend(cascades.len() as f32).to_array();
            uniform.params = [
                settings.cascades as f32,
                settings.resolution as f32,
                0.0,
                0.0,
            ];
            self.cascades = cascades;
        }
        queue.write_buffer(&self.receiver, 0, bytemuck::bytes_of(&uniform));
    }
    pub fn caster_offset(cascade: usize) -> u32 {
        (cascade as u64 * CASTER_STRIDE) as u32
    }
}
impl ShadowUniform {
    fn zeroed_disabled() -> Self {
        Self {
            matrices: [Mat4::IDENTITY.to_cols_array(); MAX_CASCADES],
            splits: [0.0; 4],
            texels: [0.0; 4],
            forward_count: Vec4::ZERO.to_array(),
            params: [0.0, 1.0, 0.0, 0.0],
            depth_scale: [0.0; 4],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera(eye: Vec3, target: Vec3) -> Mat4 {
        glam::camera::rh::proj::directx::perspective(1.5, 16.0 / 9.0, 0.05, 4000.0)
            * glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y)
    }

    #[test]
    fn cascades_cover_the_view_and_every_slice_corner() {
        let eye = Vec3::new(10.0, 5.0, -20.0);
        let view = camera(eye, eye + Vec3::new(1.0, -0.2, -1.0));
        let sun = Vec3::new(0.3, -1.0, 0.4);
        let (cascades, forward) = cascades(view, eye, sun, &ShadowSettings::BEST).unwrap();
        assert_eq!(cascades.len(), 4);
        assert!((cascades[3].far - 320.0).abs() < 0.01);
        assert!(cascades.windows(2).all(|w| w[0].far < w[1].far));
        assert!(cascades.windows(2).all(|w| w[0].texel < w[1].texel));
        // A point on the view axis at each cascade's far split projects inside it.
        for cascade in &cascades {
            let point = eye + forward * (cascade.far * 0.99);
            let clip = cascade.view_projection.project_point3(point);
            assert!(clip.x.abs() < 1.0 && clip.y.abs() < 1.0, "{clip:?}");
            assert!((0.0..1.0).contains(&clip.z), "{clip:?}");
            // A caster 100 units toward the sun still lands in the map.
            let caster = cascade
                .view_projection
                .project_point3(point - sun.normalize() * 100.0);
            assert!((0.0..1.0).contains(&caster.z), "{caster:?}");
        }
    }

    #[test]
    fn cascades_snap_to_texels_as_the_eye_moves() {
        let sun = Vec3::new(0.3, -1.0, 0.4);
        let settings = ShadowSettings::LOW;
        let base = Vec3::new(3.0, 2.0, 1.0);
        let fit = |eye: Vec3| {
            cascades(camera(eye, eye + Vec3::NEG_Z), eye, sun, &settings)
                .unwrap()
                .0
        };
        let (a, b) = (fit(base), fit(base + Vec3::new(0.013, 0.0, 0.007)));
        for (a, b) in a.iter().zip(&b) {
            assert_eq!(a.texel, b.texel);
            // The same world point lands on the same sub-texel phase.
            let pa = a.view_projection.project_point3(Vec3::ZERO);
            let pb = b.view_projection.project_point3(Vec3::ZERO);
            let texels = (pa - pb).truncate() * settings.resolution as f32 / 2.0;
            assert!((texels - texels.round()).length() < 0.01, "{texels:?}");
        }
        assert!(cascades(Mat4::ZERO, base, sun, &settings).is_none());
        assert!(cascades(camera(base, Vec3::ZERO), base, Vec3::ZERO, &settings).is_none());
    }
}
