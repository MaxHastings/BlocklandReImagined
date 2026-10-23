//! A small, bounded pixel test of upload, shaders, depth, drawing and readback.
//! This is a device smoke check, not proof that a game's world renders correctly.
use anyhow::{Context, Result, ensure};

const SIZE: u32 = 4;
const ROW: u32 = 256;
const COLORS: [[u8; 4]; 4] = [
    [64, 128, 192, 255],
    [192, 64, 128, 255],
    [128, 192, 64, 255],
    [32, 96, 160, 255],
];

pub fn check(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<()> {
    check_surface_view(device, queue, wgpu::TextureFormat::Rgba8Unorm)
}

/// Exercise the unorm presentation view used even when storage is sRGB.
/// Midtone pixels deliberately detect accidental double encoding.
pub fn check_surface_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    storage: wgpu::TextureFormat,
) -> Result<()> {
    let view = storage.remove_srgb_suffix();
    ensure!(
        matches!(
            view,
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
        ),
        "GPU pixel probe requires an RGBA8/BGRA8 attachment"
    );
    let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let result = draw(device, queue, storage, view);
    // Drain every scope even when drawing, mapping or pixel validation failed.
    let errors = [
        pollster::block_on(internal.pop()),
        pollster::block_on(validation.pop()),
        pollster::block_on(memory.pop()),
    ];
    if let Some(error) = errors.into_iter().flatten().next() {
        anyhow::bail!("GPU pixel probe: {error}");
    }
    result
}

fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    storage: wgpu::TextureFormat,
    view_format: wgpu::TextureFormat,
) -> Result<()> {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("GPU pixel probe"),
        source: wgpu::ShaderSource::Wgsl(
            "@group(0) @binding(0) var picture: texture_2d<f32>;
             @vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
                 let uv=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));
                 return vec4<f32>(uv*2.0-1.0,0.5,1.0);
             }
             @fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32> {
                 return textureLoad(picture,vec2<i32>(p.xy)%vec2<i32>(2),0);
             }"
            .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("GPU pixel probe"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(view_format.into())],
        }),
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Greater),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let texture = |label, size, format: wgpu::TextureFormat, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[format.remove_srgb_suffix()],
        })
    };
    let picture = texture(
        "probe source",
        2,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    queue.write_texture(
        picture.as_image_copy(),
        bytemuck::cast_slice(&COLORS),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(2),
        },
        picture.size(),
    );
    let source_view = picture.create_view(&Default::default());
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("probe source"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&source_view),
        }],
    });
    let color = texture(
        "probe result",
        SIZE,
        storage,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let depth = texture(
        "probe depth",
        SIZE,
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let color_view = color.create_view(&wgpu::TextureViewDescriptor {
        format: Some(view_format),
        ..Default::default()
    });
    let depth_view = depth.create_view(&Default::default());
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("probe readback"),
        size: u64::from(ROW * SIZE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GPU pixel probe"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        color.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW),
                rows_per_image: Some(SIZE),
            },
        },
        color.size(),
    );
    let submission = queue.submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .context("GPU pixel probe did not finish")?;
    receive
        .recv_timeout(std::time::Duration::from_secs(1))
        .context("GPU pixel probe readback callback timed out")??;
    let result = {
        let mapped = readback.slice(..).get_mapped_range()?;
        let mut rgba = mapped.to_vec();
        if view_format == wgpu::TextureFormat::Bgra8Unorm {
            for pixel in rgba.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        validate(&rgba)
    };
    readback.unmap();
    result
}

fn validate(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() == (ROW * SIZE) as usize,
        "GPU pixel probe readback size differs"
    );
    for y in 0..SIZE {
        for x in 0..SIZE {
            let at = (y * ROW + x * 4) as usize;
            let expected = COLORS[((y % 2) * 2 + x % 2) as usize];
            ensure!(
                bytes[at..at + 4] == expected,
                "GPU pixel probe differs at {x},{y}: {:?}, expected {expected:?}",
                &bytes[at..at + 4]
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_clear_only_black_result_cannot_pass() {
        assert!(super::validate(&[0; (super::ROW * super::SIZE) as usize]).is_err());
    }
}
