//! Hardware-free-window render test: exact blend/depth behavior, not a catalog-only smoke test.
use bri_content::effects::Library;
use bri_fx_runtime::{gpu::EffectsRenderer, pack::TextureImage, *};
use glam::{Mat4, Vec3, Vec4};
use std::collections::BTreeMap;

#[test]
fn billboard_blends_and_depth_match_the_host_pass() {
    pollster::block_on(async {
        let pack = EffectsPack::from_parts(
            Library {
                schema_version: 1,
                lights: Vec::new(),
                particles: Vec::new(),
                emitters: Vec::new(),
                textures: BTreeMap::from([
                    ("pixel".into(), "pixel.png".into()),
                    ("grey".into(), "grey.png".into()),
                ]),
            },
            Manifest {
                schema_version: 1,
                library_sha256: String::new(),
                textures: BTreeMap::new(),
                emitter_alpha: BTreeMap::new(),
                bindings: Vec::new(),
                composites: Vec::new(),
                unresolved: Vec::new(),
            },
            vec![
                TextureImage {
                    id: "pixel".into(),
                    width: 1,
                    height: 1,
                    rgba: vec![255; 4],
                },
                // Mid grey as authored: the game's non-sRGB swapchain must
                // show it as 128, like the world pass shows the same texel.
                TextureImage {
                    id: "grey".into(),
                    width: 1,
                    height: 1,
                    rgba: vec![128, 128, 128, 255],
                },
            ],
        )
        .unwrap();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance.request_adapter(&Default::default()).await.unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let mut renderer = EffectsRenderer::new(
            &device,
            &queue,
            &pack,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Depth32Float,
            1,
            16,
        )
        .unwrap();
        let size = wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        };
        let texture = |format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = texture(
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let depth = texture(
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let cv = color.create_view(&Default::default());
        let dv = depth.create_view(&Default::default());
        let camera = Camera {
            view_projection: Mat4::IDENTITY,
            position: Vec3::Z * 2.,
            right: Vec3::X,
            up: Vec3::Y,
        };
        let grey = Vec4::new(1., 1., 1., 1.);
        let red = Vec4::new(1., 0., 0., 0.5);
        for (texture, tint, blend, depth_test, expected) in [
            (0, red, BlendMode::Alpha, true, [0, 0, 255]),
            (0, red, BlendMode::Alpha, false, [128, 0, 128]),
            (0, red, BlendMode::Additive, false, [128, 0, 255]),
            (0, red, BlendMode::AdditiveColor, false, [255, 0, 255]),
            (1, grey, BlendMode::Alpha, false, [128, 128, 128]),
        ] {
            let frame = FrameEffects {
                particles: vec![ParticleInstance {
                    position: Vec3::Z * 0.5,
                    size: 2.,
                    color: tint,
                    spin: 0.,
                    axis: Vec3::ZERO,
                    texture,
                    blend,
                    depth_test,
                }],
                lights: Vec::new(),
            };
            renderer.prepare(&queue, &camera, &frame).unwrap();
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 1024,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &cv,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.,
                                g: 0.,
                                b: 1.,
                                a: 1.,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &dv,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0.75),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                renderer.render(&mut pass);
            }
            encoder.copy_texture_to_buffer(
                color.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(4),
                    },
                },
                size,
            );
            queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                tx.send(r).unwrap();
            });
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(std::time::Duration::from_secs(20)),
                })
                .unwrap();
            rx.recv().unwrap().unwrap();
            let pixels = readback.slice(..).get_mapped_range().unwrap();
            for channel in 0..3 {
                assert!(
                    (i32::from(pixels[256 + 4 + channel]) - expected[channel]).abs() <= 1,
                    "{blend:?} depth={depth_test}: {:?}",
                    &pixels[260..264]
                );
            }
        }
        // Compile the host integration shader's storage-buffer layout as well.
        let _ = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene effects light integration contract"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../examples/scene_lights.wgsl").into()),
        });
        assert_eq!(std::mem::size_of::<GpuLight>(), 32);
    });
}
