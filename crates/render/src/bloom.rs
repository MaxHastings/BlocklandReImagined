//! Bloom: the brightest parts of the world (glowing lamps, a muzzle flash,
//! the sun off chrome) bleed a soft halo onto what is around them. The world
//! draws into a texture; its pixels past a brightness threshold are copied
//! at a quarter of its size, blurred both ways, and added back over the
//! world as it is copied out. The frame is display encoded, so the
//! threshold is on display brightness: only what is drawn near white glows.

/// Display brightness (0 to 1) past which a pixel glows.
pub const THRESHOLD: f32 = 0.8;
/// How strongly the halo adds back over the world: a small lamp spreads
/// thin, so its halo is lifted well above its share of light.
pub const INTENSITY: f32 = 2.5;

pub struct Bloom {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    bright: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    /// Per pass: blur direction in texels, threshold and intensity.
    uniforms: [wgpu::Buffer; 4],
    targets: Option<Targets>,
}

/// The world's texture and the two quarter-size halo textures, with the
/// bind groups reading each, at one world size.
struct Targets {
    size: (u32, u32),
    world: wgpu::Texture,
    world_view: wgpu::TextureView,
    _halo: [wgpu::Texture; 2],
    halo_views: [wgpu::TextureView; 2],
    /// Bright pass (reads the world), blur across (halo 0), blur down
    /// (halo 1), composite (world and halo 0).
    binds: [wgpu::BindGroup; 4],
}

impl Bloom {
    /// Writes into a target of `format`.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom"),
            entries: &[
                texture(0),
                texture(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bloom"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("bloom"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bloom"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let uniforms = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bloom"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        Self {
            format,
            bright: pipeline("fs_bright"),
            blur: pipeline("fs_blur"),
            composite: pipeline("fs_composite"),
            layout,
            sampler,
            uniforms,
            targets: None,
        }
    }
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
    /// The texture the world draws into, `size` pixels; made again only
    /// when the size changes.
    pub fn target(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: (u32, u32),
    ) -> &wgpu::TextureView {
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            self.targets = Some(self.make_targets(device, queue, size));
        }
        &self.targets.as_ref().expect("made above").world_view
    }
    fn make_targets(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: (u32, u32),
    ) -> Targets {
        let texture = |label, (width, height): (u32, u32)| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let small = halo_size(size);
        let world = texture("bloom world", size);
        let halo = [texture("bloom halo", small), texture("bloom halo", small)];
        let world_view = world.create_view(&Default::default());
        let halo_views = [
            halo[0].create_view(&Default::default()),
            halo[1].create_view(&Default::default()),
        ];
        let texel = [1.0 / small.0 as f32, 1.0 / small.1 as f32];
        let values: [[f32; 4]; 4] = [
            [0.0, 0.0, THRESHOLD, INTENSITY],
            [texel[0], 0.0, THRESHOLD, INTENSITY],
            [0.0, texel[1], THRESHOLD, INTENSITY],
            [0.0, 0.0, THRESHOLD, INTENSITY],
        ];
        for (buffer, value) in self.uniforms.iter().zip(values) {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&value));
        }
        let bind = |i: usize, a: &wgpu::TextureView, b: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(a),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(b),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.uniforms[i].as_entire_binding(),
                    },
                ],
            })
        };
        let binds = [
            bind(0, &world_view, &world_view),
            bind(1, &halo_views[0], &halo_views[0]),
            bind(2, &halo_views[1], &halo_views[1]),
            bind(3, &world_view, &halo_views[0]),
        ];
        Targets {
            size,
            world,
            world_view,
            _halo: halo,
            halo_views,
            binds,
        }
    }
    /// The texture the world draws into, once `target` has made it.
    pub fn world_texture(&self) -> Option<&wgpu::Texture> {
        self.targets.as_ref().map(|t| &t.world)
    }
    /// The world's texture with its halo over all of `output`.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        let Some(t) = &self.targets else {
            return;
        };
        let passes = [
            (&self.bright, &t.halo_views[0], &t.binds[0]),
            (&self.blur, &t.halo_views[1], &t.binds[1]),
            (&self.blur, &t.halo_views[0], &t.binds[2]),
            (&self.composite, output, &t.binds[3]),
        ];
        for (pipeline, target, bind) in passes {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom"),
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
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

/// The halo's size for a world of `size`: a quarter each way, at least one
/// pixel.
pub fn halo_size(size: (u32, u32)) -> (u32, u32) {
    ((size.0 / 4).max(1), (size.1 / 4).max(1))
}

const SHADER: &str = r"
@group(0) @binding(0) var first:texture_2d<f32>;
@group(0) @binding(1) var second:texture_2d<f32>;
@group(0) @binding(2) var linear:sampler;
// xy: blur step in texels; z: threshold; w: intensity.
@group(0) @binding(3) var<uniform> pass_values:vec4<f32>;
struct Out { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) i:u32)->Out {
    let uv=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));
    return Out(vec4<f32>(uv.x*2.0-1.0,1.0-uv.y*2.0,0.0,1.0),uv);
}
// What of a pixel glows: its brightness past the threshold, with a soft
// knee so the halo does not switch on at an edge.
fn glow(c:vec3<f32>)->vec3<f32> {
    let brightness=max(c.r,max(c.g,c.b));
    let t=pass_values.z;
    let over=clamp((brightness-t)/max(1.0-t,0.0001),0.0,1.0);
    return c*over*over;
}
@fragment fn fs_bright(in:Out)->@location(0) vec4<f32> {
    // Four bilinear taps, each between four of the world's pixels,
    // average the sixteen under the quarter-size texel.
    let d=1.0/vec2<f32>(textureDimensions(first));
    var sum=vec3<f32>(0.0);
    for(var i=0;i<4;i+=1) {
        let o=vec2<f32>(f32(i%2)*2.0-1.0,f32(i/2)*2.0-1.0)*d;
        sum+=glow(textureSample(first,linear,in.uv+o).rgb);
    }
    return vec4<f32>(sum*0.25,1.0);
}
// Nine-tap Gaussian along the pass's direction, in five linear fetches.
@fragment fn fs_blur(in:Out)->@location(0) vec4<f32> {
    let step=pass_values.xy;
    var sum=textureSample(first,linear,in.uv).rgb*0.2270270270;
    let offsets=array<f32,2>(1.3846153846,3.2307692308);
    let weights=array<f32,2>(0.3162162162,0.0702702703);
    for(var i=0;i<2;i+=1) {
        sum+=textureSample(first,linear,in.uv+step*offsets[i]).rgb*weights[i];
        sum+=textureSample(first,linear,in.uv-step*offsets[i]).rgb*weights[i];
    }
    return vec4<f32>(sum,1.0);
}
@fragment fn fs_composite(in:Out)->@location(0) vec4<f32> {
    let world=textureSample(first,linear,in.uv);
    let halo=textureSample(second,linear,in.uv).rgb;
    return vec4<f32>(min(world.rgb+halo*pass_values.w,vec3<f32>(1.0)),world.a);
}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_halo_is_a_quarter_size_and_never_empty() {
        assert_eq!(halo_size((1920, 1080)), (480, 270));
        assert_eq!(halo_size((3, 2)), (1, 1));
    }

    #[test]
    fn the_shader_validates() {
        use wgpu::naga;
        let module = naga::front::wgsl::parse_str(SHADER).expect("parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validates");
    }
}
