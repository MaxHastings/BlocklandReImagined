//! PCSS Sun Shadows: a shadow edge is as sharp as the ordinary filter where its
//! caster touches the surface and widens with the caster's distance above it.
//! Runs both scene shaders (Classic and the modern Unified one).
use anyhow::Result;
use bri_render::{scene::*, shadow::ShadowSettings};
use glam::{Mat4, Vec3};

/// Axis-aligned box with outward normals, one white vertex-lit material.
fn cuboid(min: Vec3, max: Vec3) -> SceneData {
    let mut data = SceneData::default();
    data.materials.push(Material::vertex_lit("white", 0));
    let faces: [(Vec3, [Vec3; 4]); 6] = [
        (
            Vec3::Y,
            [
                Vec3::new(min.x, max.y, min.z),
                Vec3::new(min.x, max.y, max.z),
                Vec3::new(max.x, max.y, max.z),
                Vec3::new(max.x, max.y, min.z),
            ],
        ),
        (
            Vec3::NEG_Y,
            [
                Vec3::new(min.x, min.y, min.z),
                Vec3::new(max.x, min.y, min.z),
                Vec3::new(max.x, min.y, max.z),
                Vec3::new(min.x, min.y, max.z),
            ],
        ),
        (
            Vec3::X,
            [
                Vec3::new(max.x, min.y, min.z),
                Vec3::new(max.x, max.y, min.z),
                Vec3::new(max.x, max.y, max.z),
                Vec3::new(max.x, min.y, max.z),
            ],
        ),
        (
            Vec3::NEG_X,
            [
                Vec3::new(min.x, min.y, min.z),
                Vec3::new(min.x, min.y, max.z),
                Vec3::new(min.x, max.y, max.z),
                Vec3::new(min.x, max.y, min.z),
            ],
        ),
        (
            Vec3::Z,
            [
                Vec3::new(min.x, min.y, max.z),
                Vec3::new(max.x, min.y, max.z),
                Vec3::new(max.x, max.y, max.z),
                Vec3::new(min.x, max.y, max.z),
            ],
        ),
        (
            Vec3::NEG_Z,
            [
                Vec3::new(min.x, min.y, min.z),
                Vec3::new(min.x, max.y, min.z),
                Vec3::new(max.x, max.y, min.z),
                Vec3::new(max.x, min.y, min.z),
            ],
        ),
    ];
    for (normal, corners) in faces {
        let base = data.vertices.len() as u32;
        data.vertices.extend(corners.map(|p| SceneVertex {
            position: p.to_array(),
            normal: normal.to_array(),
            uv: [0.0; 2],
            lightmap_uv: [0.0; 2],
            color: [1.0; 4],
            fx: [0.0; 4],
        }));
        data.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    data.batches.push(MeshBatch {
        zone: None,
        indices: 0..data.indices.len() as u32,
        material: 0,
        center: ((min + max) * 0.5).to_array(),
    });
    data
}

fn gpu() -> Result<(wgpu::Device, wgpu::Queue)> {
    pollster::block_on(async {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await?;
        anyhow::Ok(adapter.request_device(&Default::default()).await?)
    })
}

/// Shadow passes, then `receivers` into `target`, read back as RGBA rows.
fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut SceneRenderer,
    target: &wgpu::Texture,
    receivers: &[&GpuScene],
    casters: &[&GpuScene],
    occluders: &[&GpuScene],
) -> Result<Vec<u8>> {
    let (width, height) = (target.width(), target.height());
    let view = target.create_view(&Default::default());
    let depth = create_depth(device, width, height).create_view(&Default::default());
    let row = (width * 4).div_ceil(256) * 256;
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.render_shadows(
        &mut encoder,
        ShadowCasters {
            scenes: casters,
            instances: &[],
        },
        ShadowCasters {
            scenes: occluders,
            instances: &[],
        },
    );
    renderer.render(
        &mut encoder,
        &view,
        &depth,
        receivers,
        Some(wgpu::Color::BLACK),
    );
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    Ok(mapped
        .chunks_exact(row as usize)
        .flat_map(|line| line[..width as usize * 4].to_vec())
        .collect())
}

fn color_target(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow test target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Width in world units of the dark-to-lit transition across a shadow edge at
/// x = 0, from the floor luminance averaged over several lines along z.
fn edge_width(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pcss: bool,
    mode: f32,
    caster_height: f32,
) -> Result<(f32, i32, i32, f32, f32)> {
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let (width, height) = (512u32, 512u32);
    let settings = ShadowSettings {
        pcss,
        ..ShadowSettings::LOW
    };
    let mut renderer = SceneRenderer::with_settings(device, format, 1, Some(settings));
    let floor = renderer.upload(
        device,
        queue,
        &cuboid(Vec3::new(-10., -0.5, -10.), Vec3::new(10., 0., 10.)),
    )?;
    // Covers x < 0 only, so its shadow edge falls on x = 0 under a straight
    // down sun. It casts but is never drawn.
    let slab = renderer.upload(
        device,
        queue,
        &cuboid(
            Vec3::new(-8., caster_height, -8.),
            Vec3::new(0., caster_height + 0.2, 8.),
        ),
    )?;
    let mut camera = Camera::perspective(
        [0., 6., 1.5],
        [0., 0., 0.],
        width as f32 / height as f32,
        1.0,
        0.05,
        400.0,
    );
    camera.sun_direction = [0., -1., 0., 0.];
    camera.sun_color = [0.7, 0.7, 0.7, 0.];
    camera.ambient = [0.3, 0.3, 0.3, mode];
    // Mode 0 draws with scene_original.wgsl, the others with scene.wgsl.
    camera.shading = [if mode == 0. { 0. } else { 1. }, 0., 0., 0.];
    renderer.update_camera(queue, &camera);
    let target = color_target(device, format, width, height);
    let pixels = render(
        device,
        queue,
        &mut renderer,
        &target,
        &[&floor],
        &[&slab],
        &[&floor],
    )?;
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        f32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    let profile: Vec<(f32, f32)> = (-100..=100)
        .map(|i| {
            let x = i as f32 * 0.01;
            let sum: f32 = (-6..=6)
                .map(|j| at(Vec3::new(x, 0., j as f32 * 0.25)))
                .sum();
            (x, sum / 13.0)
        })
        .collect();
    let dark = profile[..10].iter().map(|p| p.1).sum::<f32>() / 10.0;
    let lit = profile[191..].iter().map(|p| p.1).sum::<f32>() / 10.0;
    let crossing = |fraction: f32| {
        let level = dark + (lit - dark) * fraction;
        profile
            .iter()
            .find(|p| p.1 >= level)
            .map_or(f32::NAN, |p| p.0)
    };
    let width = crossing(0.9) - crossing(0.1);
    let center = crossing(0.5);
    // The biggest step between neighbouring samples, as a share of the whole
    // dark-to-lit range: a smooth edge has none, a hard outline has one.
    let step = profile
        .windows(2)
        .map(|pair| (pair[1].1 - pair[0].1).abs())
        .fold(0.0, f32::max)
        / (lit - dark);
    eprintln!(
        "mode {mode} pcss {pcss} caster {caster_height}: dark {dark:.0} lit {lit:.0} width {width:.2} center {center:.2} step {step:.2}"
    );
    Ok((width, dark as i32, lit as i32, center, step))
}

#[test]
fn sun_shadow_edges_soften_with_distance_from_the_caster() -> Result<()> {
    let (device, queue) = gpu()?;
    for mode in [0., 2.] {
        let (off_near, ..) = edge_width(&device, &queue, false, mode, 0.3)?;
        let (off_far, off_dark, off_lit, ..) = edge_width(&device, &queue, false, mode, 4.0)?;
        let (on_near, ..) = edge_width(&device, &queue, true, mode, 0.3)?;
        let (on_far, on_dark, on_lit, ..) = edge_width(&device, &queue, true, mode, 4.0)?;
        assert!(off_lit - off_dark > 40, "mode {mode}: the shadow shows");
        assert!(
            (on_lit - off_lit).abs() <= 3,
            "mode {mode}: the lit side is untouched"
        );
        assert!(
            (on_dark - off_dark).abs() <= 8,
            "mode {mode}: the umbra stays dark"
        );
        assert!(
            (off_far - off_near).abs() < 0.05,
            "mode {mode}: the ordinary filter is one width ({off_near} vs {off_far})"
        );
        assert!(
            on_near <= off_near + 0.06,
            "mode {mode}: contact stays sharp ({on_near} vs {off_near})"
        );
        assert!(
            on_far > off_far * 2.0 && on_far > on_near * 2.0,
            "mode {mode}: a far caster softens the edge ({on_far}, near {on_near}, off {off_far})"
        );
    }
    Ok(())
}

/// A soft edge spreads to both sides of where the sharp edge would be and
/// fades out smoothly: no hard outline where the filter stops looking for the
/// shadow, and its midpoint stays on the true edge.
#[test]
fn pcss_penumbra_spreads_both_ways_without_a_hard_outline() -> Result<()> {
    let (device, queue) = gpu()?;
    for mode in [0., 2.] {
        // A caster 12 units up widens the filter to its limit.
        let (off_width, _, _, off_center, _) = edge_width(&device, &queue, false, mode, 12.0)?;
        let (on_width, _, _, on_center, on_step) = edge_width(&device, &queue, true, mode, 12.0)?;
        assert!(
            on_width > off_width * 3.0,
            "mode {mode}: the far caster's edge is soft ({on_width} vs {off_width})"
        );
        assert!(
            (on_center - off_center).abs() < 0.03,
            "mode {mode}: the soft edge is centered on the true edge ({on_center} vs {off_center})"
        );
        assert!(
            on_step < 0.07,
            "mode {mode}: no step in the fade, even at its outer edge ({on_step})"
        );
    }
    Ok(())
}

/// `data` turned by `rotation` (positions, normals and batch centers).
fn transformed(mut data: SceneData, rotation: Mat4) -> SceneData {
    for v in &mut data.vertices {
        v.position = rotation.transform_point3(Vec3::from(v.position)).to_array();
        v.normal = rotation
            .transform_vector3(Vec3::from(v.normal))
            .normalize()
            .to_array();
    }
    for b in &mut data.batches {
        b.center = rotation.transform_point3(Vec3::from(b.center)).to_array();
    }
    data
}

/// A ramp (tilted `tilt_degrees` about z) lit straight down under a roof whose
/// edge falls across it, seen from above: the shadow edge's width in pixels
/// along a row of the ramp, and the lowest luminance on its lit side.
fn ramp_edge(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pcss: bool,
    mode: f32,
    tilt_degrees: f32,
) -> Result<(usize, f32, f32)> {
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let (width, height) = (700u32, 700u32);
    let settings = ShadowSettings {
        pcss,
        ..ShadowSettings::LOW
    };
    let mut renderer = SceneRenderer::with_settings(device, format, 1, Some(settings));
    let floor = renderer.upload(
        device,
        queue,
        &cuboid(Vec3::new(-10., -0.5, -10.), Vec3::new(10., 0., 10.)),
    )?;
    let turn = Mat4::from_translation(Vec3::new(0., 1.5, 0.))
        * Mat4::from_rotation_z(tilt_degrees.to_radians());
    let ramp = renderer.upload(
        device,
        queue,
        &transformed(
            cuboid(Vec3::new(-4., -0.1, -4.), Vec3::new(4., 0.1, 4.)),
            turn,
        ),
    )?;
    // Covers x < 0, 4.5 units above the ramp where its edge falls.
    let roof = renderer.upload(
        device,
        queue,
        &cuboid(Vec3::new(-8., 6., -8.), Vec3::new(0., 6.2, 8.)),
    )?;
    let mut camera = Camera::perspective([0., 9., 0.3], [0., 1.5, 0.], 1.0, 0.45, 0.05, 400.0);
    camera.sun_direction = [0., -1., 0., 0.];
    camera.sun_color = [0.7, 0.7, 0.7, 0.];
    camera.ambient = [0.3, 0.3, 0.3, mode];
    camera.shading = [if mode == 0. { 0. } else { 1. }, 0., 0., 0.];
    renderer.update_camera(queue, &camera);
    let target = color_target(device, format, width, height);
    let pixels = render(
        device,
        queue,
        &mut renderer,
        &target,
        &[&floor, &ramp],
        &[&ramp, &roof],
        &[&floor],
    )?;
    // Luminance per column over a band of rows.
    let profile: Vec<f32> = (0..width as usize)
        .map(|x| {
            (330..370)
                .map(|y| f32::from(pixels[(y * width as usize + x) * 4 + 1]))
                .sum::<f32>()
                / 40.0
        })
        .collect();
    let dark = profile[150..250].iter().sum::<f32>() / 100.0;
    let lit = profile[450..550].iter().sum::<f32>() / 100.0;
    let crossing = |fraction: f32| {
        let level = dark + (lit - dark) * fraction;
        (150..550).find(|&x| profile[x] >= level).unwrap_or(0)
    };
    let edge = crossing(0.9) - crossing(0.1);
    // Past the penumbra the lit side must be fully lit.
    let lowest = profile[crossing(0.9) + 20..550]
        .iter()
        .copied()
        .fold(f32::MAX, f32::min);
    eprintln!(
        "mode {mode} pcss {pcss} tilt {tilt_degrees}: edge {edge}px lit {lit:.0} lowest {lowest:.0}"
    );
    Ok((edge, lit, lowest))
}

/// On a slanted receiver the wide filter must follow the receiver's plane: the
/// penumbra is as wide as the roof's distance says, and the lit side beside
/// it is not speckled or darkened by the surface's own depth.
#[test]
fn pcss_penumbra_follows_slanted_receivers() -> Result<()> {
    let (device, queue) = gpu()?;
    // (mode, tilt): Unified at 65 degrees lights the ramp unevenly on its own,
    // which this edge measurement cannot tell from a penumbra.
    for (mode, tilt) in [(0., 50.), (0., 65.), (2., 50.)] {
        let (off_edge, ..) = ramp_edge(&device, &queue, false, mode, tilt)?;
        let (on_edge, on_lit, on_lowest) = ramp_edge(&device, &queue, true, mode, tilt)?;
        assert!(
            on_edge >= 30 && on_edge >= off_edge * 3,
            "mode {mode} tilt {tilt}: roof 4.5 units up gives a wide penumbra ({on_edge}px vs {off_edge}px)"
        );
        assert!(
            on_lowest >= on_lit - 4.0,
            "mode {mode} tilt {tilt}: the lit side stays lit ({on_lowest} vs {on_lit})"
        );
    }
    Ok(())
}
