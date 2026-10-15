//! Bloom spreads a bright spot into a soft halo around it and leaves dark,
//! distant pixels and dim ones as they were.
use anyhow::Result;
use bri_render::bloom::Bloom;

#[test]
fn a_bright_spot_glows_onto_its_surroundings_and_nowhere_else() -> Result<()> {
    let (device, queue) = pollster::block_on(async {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        anyhow::Ok(adapter.request_device(&Default::default()).await?)
    })?;
    const SIDE: u32 = 128;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut bloom = Bloom::new(&device, format);
    bloom.target(&device, &queue, (SIDE, SIDE));
    // Black, with a white 4x4 spot at (32, 32) and a dim grey one at (96, 96).
    let mut pixels = vec![0u8; (SIDE * SIDE * 4) as usize];
    for (centre, value) in [(32u32, 255u8), (96, 120)] {
        for y in centre - 2..centre + 2 {
            for x in centre - 2..centre + 2 {
                let i = ((y * SIDE + x) * 4) as usize;
                pixels[i..i + 4].copy_from_slice(&[value, value, value, 255]);
            }
        }
    }
    queue.write_texture(
        bloom.world_texture().unwrap().as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(SIDE * 4),
            rows_per_image: Some(SIDE),
        },
        wgpu::Extent3d {
            width: SIDE,
            height: SIDE,
            depth_or_array_layers: 1,
        },
    );
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bloom output"),
        size: wgpu::Extent3d {
            width: SIDE,
            height: SIDE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bloom readback"),
        size: u64::from(SIDE * SIDE * 4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    bloom.draw(&mut encoder, &output.create_view(&Default::default()));
    encoder.copy_texture_to_buffer(
        output.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIDE * 4),
                rows_per_image: Some(SIDE),
            },
        },
        wgpu::Extent3d {
            width: SIDE,
            height: SIDE,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    let out = readback.slice(..).get_mapped_range()?.to_vec();
    let red = |x: u32, y: u32| out[((y * SIDE + x) * 4) as usize];
    assert_eq!(red(31, 31), 255, "the spot itself stays white");
    assert!(red(38, 32) > 10, "a halo beside the spot: {}", red(38, 32));
    assert!(red(32, 26) > 10, "and above it: {}", red(32, 26));
    assert_eq!(red(64, 10), 0, "far away stays black");
    assert_eq!(red(95, 95), 120, "a dim spot does not glow");
    assert_eq!(red(102, 96), 0, "nor around it");
    Ok(())
}
