//! A reflection probe: the world drawn into a small cube around one point,
//! as engines light shiny objects since the 2000s, so bare metal
//! (`MaterialKind::Metal`) reflects the bricks, players and sky around it
//! instead of a painted guess.
//!
//! One probe serves every metal surface. The game places it at the metal
//! object nearest the player each frame; surfaces far from it fade to a sky
//! made of the map's own colours, as they do when no probe is live
//! (reflections off, or no metal near). Its six faces are drawn a few per
//! frame ([`FACES_PER_FRAME`]), at [`PROBE_SIZE`] pixels square and no
//! farther than the probe's reach, so it costs a small, bounded pass.
//!
//! The faces are then folded into one octahedral map ([`MAP_SIZE`] square,
//! with mips box filtered for rough metal's blurred reflections). A plain 2D
//! texture binds in a metal material's spare texture slot, so the probe
//! adds no texture binding to the world shader, which already uses the 16
//! every GPU is guaranteed to have.
use crate::scene::{Camera, DEPTH_FORMAT, GpuInstances, GpuScene, SceneRenderer, WorldPass};
use glam::Vec3;

/// Face size in pixels.
pub const PROBE_SIZE: u32 = 128;
/// The octahedral map's size, and its mips: 256 down to 1.
pub const MAP_SIZE: u32 = 256;
pub const MAP_MIPS: u32 = 9;
/// Faces redrawn per frame while the probe stays put.
pub const FACES_PER_FRAME: usize = 2;
/// A probe moved this far since a face was drawn redraws every face.
const JUMP: f32 = 6.0;
/// The first renderer view the probe's faces use: after the player's and
/// the most mirror planes any setting draws.
pub const FIRST_VIEW: usize = 1 + crate::reflection::ReflectionSettings::MAX_PLANES;

/// Each cube face's camera, forward then up. The faces are drawn by
/// right-handed cameras, so the cube stores the world with z mirrored and
/// is sampled at (x, y, -z) (`FOLD`): slot 4 (+z) holds the view toward -z
/// and slot 5 the view toward +z.
const FACES: [(Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::Y),
    (Vec3::NEG_X, Vec3::Y),
    (Vec3::Y, Vec3::Z),
    (Vec3::NEG_Y, Vec3::NEG_Z),
    (Vec3::NEG_Z, Vec3::Y),
    (Vec3::Z, Vec3::Y),
];

/// `Probe` in scene.wgsl: the centre and reach, then whether it is live,
/// its top mip and the distances over which surfaces fade from it to the
/// sky.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ProbeUniform {
    centre_reach: [f32; 4],
    state: [f32; 4],
}

/// The octahedral map every metal surface samples (bound in its material's
/// slot 2), with its sampler and `Probe` uniform in the camera group.
pub(crate) struct ProbeBinding {
    /// Tells a probe built for another renderer's map (`EnvironmentProbe::matches`).
    pub(crate) id: u64,
    pub(crate) texture: wgpu::Texture,
    pub(crate) map: wgpu::TextureView,
    pub(crate) sampler: wgpu::Sampler,
    pub(crate) uniform: wgpu::Buffer,
}
impl ProbeBinding {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("environment probe"),
            size: wgpu::Extent3d {
                width: MAP_SIZE,
                height: MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: MAP_MIPS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let map = texture.create_view(&Default::default());
        // Octahedral edges fold onto themselves, so clamping is the closest
        // plain address mode.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("environment probe"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("environment probe"),
            size: std::mem::size_of::<ProbeUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            texture,
            map,
            sampler,
            uniform,
        }
    }
}

/// Where the probe's faces are drawn and gathered before they are folded
/// into the renderer's map, and the passes that fold and filter it.
pub struct EnvironmentProbe {
    format: wgpu::TextureFormat,
    samples: u32,
    /// The renderer's map this probe writes (`ProbeBinding::id`).
    map_id: u64,
    color: Option<wgpu::TextureView>,
    resolved: wgpu::Texture,
    depth: wgpu::TextureView,
    cube: wgpu::Texture,
    fold: wgpu::RenderPipeline,
    fold_group: wgpu::BindGroup,
    downsample: wgpu::RenderPipeline,
    /// The map's top level, then per mip from 1: the level above as a
    /// source, this level as a target.
    top: wgpu::TextureView,
    mips: Vec<(wgpu::BindGroup, wgpu::TextureView)>,
    centre: Option<Vec3>,
    reach: f32,
    /// Where each face was last drawn from.
    drawn: [Option<Vec3>; 6],
    next: usize,
    /// Faces to draw this frame.
    faces: Vec<usize>,
}

/// A triangle over the whole target, with texture coordinates.
const FULLSCREEN: &str = r#"
struct Out { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) i:u32)->Out {
    let p=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));
    var out:Out;
    out.position=vec4<f32>(p*2.0-1.0,0.0,1.0);
    out.uv=vec2<f32>(p.x,1.0-p.y);
    return out;
}
"#;

const DOWNSAMPLE: &str = r#"
@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var bilinear:sampler;
// Four bilinear taps a source texel apart: a 4x4 tent, smoother than 2x2.
@fragment fn fs_main(in:Out)->@location(0) vec4<f32> {
    let texel=1.0/vec2<f32>(textureDimensions(source));
    var sum=vec4<f32>(0.0);
    for(var i=0;i<4;i+=1) {
        let o=vec2<f32>(f32(i&1)-0.5,f32(i>>1u)-0.5)*texel*2.0;
        sum+=textureSampleLevel(source,bilinear,in.uv+o,0.0);
    }
    return sum*0.25;
}
"#;

/// The cube's faces folded into the octahedral map: each texel's direction
/// (`probe_direction` in scene.wgsl, which inverts it), read from the cube.
const FOLD: &str = r#"
@group(0) @binding(0) var faces:texture_cube<f32>;
@group(0) @binding(1) var bilinear:sampler;
@fragment fn fs_main(in:Out)->@location(0) vec4<f32> {
    let p=in.uv*2.0-1.0;
    var d=vec3<f32>(p.x,1.0-abs(p.x)-abs(p.y),p.y);
    let t=max(-d.y,0.0);
    d.x+=select(t,-t,d.x>=0.0);
    d.z+=select(t,-t,d.z>=0.0);
    d=normalize(d);
    return vec4<f32>(textureSampleLevel(faces,bilinear,vec3<f32>(d.x,d.y,-d.z),0.0).rgb,1.0);
}
"#;

impl EnvironmentProbe {
    pub fn new(
        device: &wgpu::Device,
        renderer: &SceneRenderer,
        format: wgpu::TextureFormat,
        samples: u32,
    ) -> Self {
        let size = wgpu::Extent3d {
            width: PROBE_SIZE,
            height: PROBE_SIZE,
            depth_or_array_layers: 1,
        };
        let texture = |label, samples, format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let color = (samples > 1).then(|| {
            texture("environment probe samples", samples, format, attachment)
                .create_view(&Default::default())
        });
        let resolved = texture(
            "environment probe face",
            1,
            format,
            attachment | wgpu::TextureUsages::COPY_SRC,
        );
        let depth = texture("environment probe depth", samples, DEPTH_FORMAT, attachment)
            .create_view(&Default::default());
        let cube = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("environment probe faces"),
            size: wgpu::Extent3d {
                depth_or_array_layers: 6,
                ..size
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("environment probe filter"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let pass = |label, fragment: &str, dimension| {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: dimension,
                            multisampled: false,
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
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(format!("{FULLSCREEN}{fragment}").into()),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
            (layout, pipeline)
        };
        let group = |layout: &wgpu::BindGroupLayout, view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("environment probe pass"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            })
        };
        let (fold_layout, fold) = pass(
            "environment probe fold",
            FOLD,
            wgpu::TextureViewDimension::Cube,
        );
        let fold_group = group(
            &fold_layout,
            &cube.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            }),
        );
        let (mip_layout, downsample) = pass(
            "environment probe mips",
            DOWNSAMPLE,
            wgpu::TextureViewDimension::D2,
        );
        let binding = renderer.probe();
        let level = |mip: u32| {
            binding.texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("environment probe level"),
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let mips = (1..MAP_MIPS)
            .map(|mip| (group(&mip_layout, &level(mip - 1)), level(mip)))
            .collect();
        Self {
            format,
            samples,
            map_id: binding.id,
            color,
            resolved,
            depth,
            cube,
            fold,
            fold_group,
            downsample,
            top: level(0),
            mips,
            centre: None,
            reach: 0.0,
            drawn: [None; 6],
            next: 0,
            faces: Vec::new(),
        }
    }
    /// Whether this probe draws for `renderer`'s map at `format`; otherwise
    /// make a new one.
    pub fn matches(&self, renderer: &SceneRenderer, format: wgpu::TextureFormat) -> bool {
        self.format == format
            && self.samples == renderer.samples()
            && self.map_id == renderer.probe().id
    }
    /// Where the probe sits this frame, if anywhere.
    pub fn centre(&self) -> Option<Vec3> {
        self.centre
    }
    /// Faces drawn this frame (0..6), after `prepare`.
    pub fn faces(&self) -> &[usize] {
        &self.faces
    }
    /// Place the probe at `centre`, drawing the world out to `reach`
    /// (None: metal reflects only the sky), and plan the faces to draw
    /// this frame; `camera` is the player's, whose light and fog the faces
    /// share.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut SceneRenderer,
        camera: &Camera,
        centre: Option<Vec3>,
        reach: f32,
    ) {
        self.faces.clear();
        let centre = centre.filter(|c| c.is_finite() && reach.is_finite() && reach > 1.0);
        if centre.is_none() || reach != self.reach {
            self.drawn = [None; 6];
        }
        self.centre = centre;
        self.reach = reach;
        let Some(centre) = centre else {
            renderer.set_probe(queue, ProbeUniform::default());
            return;
        };
        let jumped = self
            .drawn
            .iter()
            .any(|at| at.is_none_or(|at| at.distance(centre) > JUMP));
        if jumped {
            self.faces.extend(0..6);
        } else {
            for _ in 0..FACES_PER_FRAME {
                self.faces.push(self.next);
                self.next = (self.next + 1) % 6;
            }
        }
        renderer.set_view_count(device, FIRST_VIEW + 6);
        for &face in &self.faces {
            let (forward, up) = FACES[face];
            let view = Camera::oriented(
                centre.to_array(),
                forward.to_array(),
                up.to_array(),
                1.0,
                std::f32::consts::FRAC_PI_2,
                0.05,
                reach,
            );
            renderer.update_view(
                queue,
                FIRST_VIEW + face,
                &Camera {
                    view_projection: view.view_projection,
                    eye: view.eye,
                    ..*camera
                },
            );
            self.drawn[face] = Some(centre);
        }
        // Surfaces at the probe see it sharp; by half its reach they see
        // mostly the sky.
        renderer.set_probe(
            queue,
            ProbeUniform {
                centre_reach: centre.extend(reach).to_array(),
                state: [1.0, (MAP_MIPS - 1) as f32, 4.0, (reach * 0.5).max(8.0)],
            },
        );
    }
    /// Draw this frame's faces and fold them into the map: `scenes` and
    /// `instances` are what the probe may show.
    pub fn render(
        &self,
        renderer: &SceneRenderer,
        encoder: &mut wgpu::CommandEncoder,
        scenes: &[&GpuScene],
        instances: &[(&GpuScene, &GpuInstances)],
        clear: wgpu::Color,
    ) {
        if self.faces.is_empty() {
            return;
        }
        let resolved = self.resolved.create_view(&Default::default());
        for &face in &self.faces {
            renderer.render_world(
                encoder,
                WorldPass {
                    view: FIRST_VIEW + face,
                    color: self.color.as_ref().unwrap_or(&resolved),
                    resolve: self.color.as_ref().map(|_| &resolved),
                    depth: &self.depth,
                    viewport: None,
                    clear: Some(clear),
                    after_opaque: None,
                    after_all: None,
                },
                scenes,
                instances,
            );
            encoder.copy_texture_to_texture(
                self.resolved.as_image_copy(),
                wgpu::TexelCopyTextureInfo {
                    texture: &self.cube,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: face as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: PROBE_SIZE,
                    height: PROBE_SIZE,
                    depth_or_array_layers: 1,
                },
            );
        }
        let passes = std::iter::once((&self.fold, &self.fold_group, &self.top)).chain(
            self.mips
                .iter()
                .map(|(group, target)| (&self.downsample, group, target)),
        );
        for (pipeline, group, target) in passes {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("environment probe map"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
