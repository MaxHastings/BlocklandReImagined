//! A player standing on a brick tower shades the tower top, not the floor
//! beneath it: non-casting surfaces still stop shadows (occluder map).
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
        indices: 0..data.indices.len() as u32,
        material: 0,
        center: ((min + max) * 0.5).to_array(),
    });
    data
}

fn gpu() -> Result<(wgpu::Device, wgpu::Queue)> {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
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

#[test]
fn shadows_land_only_on_the_first_surface_they_reach() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let (width, height) = (256u32, 256u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let floor = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-10., -0.5, -10.), Vec3::new(10., 0., 10.)),
    )?;
    let tower = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-1., 0., -1.), Vec3::new(1., 4., 1.)),
    )?;
    let player = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-0.3, 4.05, -0.3), Vec3::new(0.3, 5., 0.3)),
    )?;
    // A ceiling high over everything (an interior's roof, an overhang):
    // occluders toward the sun from the player must not hide the tower.
    let ceiling = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-20., 12., -20.), Vec3::new(20., 12.5, 20.)),
    )?;
    let sun = Vec3::new(0.3, -1.0, 0.0);
    let mut camera = Camera::perspective(
        [8., 7., 6.],
        [1., 1.5, 0.],
        width as f32 / height as f32,
        1.0,
        0.05,
        400.0,
    );
    camera.sun_direction = sun.extend(0.).to_array();
    camera.sun_color = [0.7, 0.7, 0.7, 0.];
    camera.ambient = [0.3, 0.3, 0.3, 0.];
    renderer.update_camera(&queue, &camera);

    let target = color_target(&device, format, width, height);
    let mut frame = |casters: &[&GpuScene], occluders: &[&GpuScene]| {
        render(
            &device,
            &queue,
            &mut renderer,
            &target,
            &[&floor, &tower, &player],
            casters,
            occluders,
        )
    };
    // Luminance of the pixel showing a world point.
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    // The player's shadow along the sun: on the tower top, and where it
    // would reach the floor if the tower did not stop it.
    let tower_top = Vec3::new(0.15, 4.0, 0.0);
    let floor_below = Vec3::new(1.35, 0.0, 0.0);

    let unshadowed = frame(&[], &[&floor, &tower])?;
    let fixed = frame(&[&player], &[&floor, &tower])?;
    let see_through = frame(&[&player], &[])?;
    let lit_top = at(&unshadowed, tower_top);
    let lit_floor = at(&unshadowed, floor_below);
    assert!(
        at(&fixed, tower_top) < lit_top - 20,
        "player shades the tower top"
    );
    assert!(
        (at(&fixed, floor_below) - lit_floor).abs() <= 3,
        "tower stops the shadow"
    );
    assert!(
        at(&see_through, floor_below) < lit_floor - 20,
        "without occluders the shadow would pass through (test sensitivity)"
    );
    let covered = frame(&[&player], &[&floor, &tower, &ceiling])?;
    assert!(
        at(&covered, tower_top) < lit_top - 20,
        "player under a ceiling still shades the tower top"
    );
    assert!(
        (at(&covered, floor_below) - lit_floor).abs() <= 3,
        "tower stops the shadow under a ceiling"
    );
    // Occluders never cast shadows of their own.
    assert_eq!(unshadowed, frame(&[], &[])?);
    Ok(())
}

/// A thin shadow running away from the camera crosses cascade splits, where
/// texels get coarser and its core lightens. Cascades blend, so that change
/// is gradual down the screen instead of a step along the split line.
#[test]
fn cascade_splits_do_not_cut_a_shadow() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let (width, height) = (256u32, 512u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let floor = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-20., -0.5, -200.), Vec3::new(20., 0., 10.)),
    )?;
    // A thin rail over the floor along the view: its shadow is a strip a
    // few near-cascade texels wide.
    let rail = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-0.05, 1., -150.), Vec3::new(0.05, 1.1, -1.)),
    )?;
    let mut camera = Camera::perspective(
        [0.6, 2.5, 0.],
        [0.6, 0., -12.],
        width as f32 / height as f32,
        1.0,
        0.05,
        400.0,
    );
    camera.sun_direction = [0.6, -1.0, 0.0, 0.0];
    camera.sun_color = [0.7, 0.7, 0.7, 0.];
    camera.ambient = [0.3, 0.3, 0.3, 0.];
    renderer.update_camera(&queue, &camera);
    let target = color_target(&device, format, width, height);
    let pixels = render(
        &device,
        &queue,
        &mut renderer,
        &target,
        &[&floor],
        &[&rail],
        &[],
    )?;
    // The darkest pixel of each screen row is the strip's core.
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let screen_row = |z: f32| {
        let ndc = view_projection.project_point3(Vec3::new(0.6, 0., z));
        ((0.5 - ndc.y * 0.5) * height as f32) as usize
    };
    let core = |y: usize| {
        let line = &pixels[y * width as usize * 4..][..width as usize * 4];
        line.chunks_exact(4)
            .map(|p| i32::from(p[1]))
            .min()
            .unwrap_or(255)
    };
    // From 3 to 60 units ahead: past the near cascade's split.
    let (first, last) = (screen_row(-60.), screen_row(-3.));
    let cores: Vec<i32> = (first..=last).map(core).collect();
    let steps: Vec<i32> = cores.windows(2).map(|w| (w[0] - w[1]).abs()).collect();
    let spread = cores.iter().max().unwrap() - cores.iter().min().unwrap();
    assert!(
        spread > 12,
        "test sensitivity: the core lightens with distance ({spread})"
    );
    let worst = *steps.iter().max().unwrap();
    assert!(
        worst * 3 < spread,
        "the strip's core jumps by {worst} of {spread} between neighbouring rows"
    );
    Ok(())
}

/// Brick chunks stop shadows only where a caster lies over them, so the
/// occluder layer draws only the chunks under some caster: a chunk far from
/// every player is not drawn, and the shadows are the same as drawing all.
#[test]
fn occluders_draw_only_the_chunks_under_a_caster() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let (width, height) = (256u32, 256u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let floor_data = cuboid(Vec3::new(-10., -0.5, -10.), Vec3::new(10., 0., 10.));
    let tower_data = cuboid(Vec3::new(-1., 0., -1.), Vec3::new(1., 4., 1.));
    let far_data = cuboid(Vec3::new(30., 0., 30.), Vec3::new(34., 6., 34.));
    let floor = renderer.upload(&device, &queue, &floor_data)?;
    let tower = renderer.upload(&device, &queue, &tower_data)?;
    let palette = renderer.upload(&device, &queue, &tower_data)?;
    let tower_chunk = renderer.upload_chunk(&device, &queue, &tower_data, &palette)?;
    let far_chunk = renderer.upload_chunk(&device, &queue, &far_data, &palette)?;
    let player = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-0.3, 4.05, -0.3), Vec3::new(0.3, 5., 0.3)),
    )?;
    let mut camera = Camera::perspective(
        [8., 7., 6.],
        [1., 1.5, 0.],
        width as f32 / height as f32,
        1.0,
        0.05,
        400.0,
    );
    camera.sun_direction = [0.3, -1.0, 0.0, 0.0];
    camera.sun_color = [0.7, 0.7, 0.7, 0.];
    camera.ambient = [0.3, 0.3, 0.3, 0.];
    renderer.update_camera(&queue, &camera);
    let target = color_target(&device, format, width, height);
    let mut frame =
        |casters: &[&GpuScene], occluders: &[&GpuScene]| -> Result<(Vec<u8>, RenderStats)> {
            renderer.update_camera(&queue, &camera);
            let pixels = render(
                &device,
                &queue,
                &mut renderer,
                &target,
                &[&floor, &tower],
                casters,
                occluders,
            )?;
            Ok((pixels, renderer.stats()))
        };
    let (unbounded, _) = frame(&[&player], &[&floor, &tower])?;
    let (chunked, near_only) = frame(&[&player], &[&floor, &tower_chunk])?;
    let (with_far, stats) = frame(&[&player], &[&floor, &tower_chunk, &far_chunk])?;
    assert_eq!(
        unbounded, chunked,
        "a chunk stops the shadow as the same box uploaded whole"
    );
    assert_eq!(chunked, with_far, "the far chunk changes nothing");
    assert_eq!(
        stats.shadow_triangles, near_only.shadow_triangles,
        "the chunk far from the player is not drawn into the occluder layer"
    );
    // With no caster at all, no occluder is drawn.
    let (_, empty) = frame(&[], &[&tower_chunk, &far_chunk])?;
    assert_eq!(empty.shadow_triangles, 0);
    Ok(())
}

/// With Brick Shadows on, bricks keep their sun shadow depth per cascade
/// and only moving casters draw each frame. The shadows must match drawing
/// every brick every frame as the camera turns and moves, as a moving caster
/// crosses them, and the same frame a brick chunk is added or removed.
#[test]
fn kept_brick_shadows_match_drawing_every_brick() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let (width, height) = (256u32, 256u32);
    let settings = ShadowSettings {
        lamps: 0,
        ..ShadowSettings::MEDIUM
    };
    let make = |keep: bool| {
        let renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
        renderer.keep_brick_shadows(keep);
        renderer
    };
    let (mut kept, mut direct) = (make(true), make(false));
    let floor_data = cuboid(Vec3::new(-40., -0.5, -40.), Vec3::new(40., 0., 40.));
    // Towers as brick chunks, spread over several cascades.
    let towers: Vec<SceneData> = (0..12)
        .map(|i| {
            let (x, z) = ((i % 4) as f32 * 9.0 - 14.0, (i / 4) as f32 * 11.0 - 12.0);
            cuboid(
                Vec3::new(x, 0., z),
                Vec3::new(x + 1.5, 2.0 + i as f32 * 0.7, z + 1.0),
            )
        })
        .collect();
    let late = cuboid(Vec3::new(3., 0., 3.), Vec3::new(5., 6., 4.));
    struct Uploaded {
        floor: GpuScene,
        towers: Vec<GpuScene>,
        late: GpuScene,
        player: GpuScene,
    }
    let upload = |renderer: &SceneRenderer| -> Result<Uploaded> {
        let palette = renderer.upload(&device, &queue, &towers[0])?;
        Ok(Uploaded {
            floor: renderer.upload(&device, &queue, &floor_data)?,
            towers: towers
                .iter()
                .map(|t| renderer.upload_chunk(&device, &queue, t, &palette))
                .collect::<Result<_>>()?,
            late: renderer.upload_chunk(&device, &queue, &late, &palette)?,
            player: renderer.upload(
                &device,
                &queue,
                &cuboid(Vec3::new(-0.3, 0.0, -0.3), Vec3::new(0.3, 1.8, 0.3)),
            )?,
        })
    };
    let (a, b) = (upload(&kept)?, upload(&direct)?);
    let target = color_target(&device, format, width, height);
    let frame = |renderer: &mut SceneRenderer,
                     scenes: &Uploaded,
                     camera: &Camera,
                     player_at: Vec3,
                     with_late: bool| {
        renderer.update_camera(&queue, camera);
        let mut casters: Vec<&GpuScene> = scenes.towers.iter().collect();
        if with_late {
            casters.push(&scenes.late);
        }
        // The moving caster: a player walking between the towers.
        let mut instances = GpuInstances::new(&device, 1)?;
        instances.update(
            &queue,
            &[SceneTransform {
                transform: Mat4::from_translation(player_at),
                tint: [1.0; 4],
            }],
        )?;
        let (w, h) = (target.width(), target.height());
        let view = target.create_view(&Default::default());
        let depth = create_depth(&device, w, h).create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.render_shadows(
            &mut encoder,
            ShadowCasters {
                scenes: &casters,
                instances: &[(&scenes.player, &instances)],
            },
            ShadowCasters {
                scenes: &[],
                instances: &[],
            },
        );
        let mut receivers: Vec<&GpuScene> = vec![&scenes.floor];
        receivers.extend(casters.iter().copied());
        renderer.render_with_instances(
            &mut encoder,
            &view,
            &depth,
            &receivers,
            &[(&scenes.player, &instances)],
            Some(wgpu::Color::BLACK),
        );
        queue.submit([encoder.finish()]);
        let pixels = read(&device, &queue, &target)?;
        anyhow::Ok((pixels, renderer.stats()))
    };
    let mut kept_frames = 0;
    for step in 0..24 {
        let t = step as f32;
        // Turning, then walking off (past what the nearest cascades keep).
        let eye = Vec3::new(
            18.0 * (t * 0.3).cos() + (t - 12.0).max(0.0) * 3.0,
            14.0,
            18.0 * (t * 0.3).sin(),
        );
        let mut camera = Camera::perspective(eye.to_array(), [0., 1., 0.], 1.0, 1.1, 0.05, 400.0);
        camera.sun_direction = [0.35, -1.0, 0.25, 0.0];
        camera.sun_color = [0.7, 0.7, 0.7, 0.];
        camera.ambient = [0.3, 0.3, 0.3, 0.];
        let player_at = Vec3::new(-6.0 + t * 0.5, 0.0, 2.0);
        // The late chunk arrives at step 8 and leaves at step 16.
        let with_late = (8..16).contains(&step);
        let (kept_pixels, stats) = frame(&mut kept, &a, &camera, player_at, with_late)?;
        let (direct_pixels, direct_stats) = frame(&mut direct, &b, &camera, player_at, with_late)?;
        assert_eq!(direct_stats.kept_cascades, 0);
        kept_frames += stats.kept_cascades;
        let differing = kept_pixels
            .chunks_exact(4)
            .zip(direct_pixels.chunks_exact(4))
            .filter(|(k, d)| k.iter().zip(d.iter()).any(|(x, y)| x.abs_diff(*y) > 2))
            .count();
        assert!(
            differing * 1000 <= kept_pixels.len() / 4,
            "step {step}: {differing} pixels differ from drawing every brick ({stats:?})"
        );
    }
    assert!(
        kept_frames > 24,
        "test sensitivity: most cascades came from kept layers ({kept_frames})"
    );
    Ok(())
}

fn read(device: &wgpu::Device, queue: &wgpu::Queue, target: &wgpu::Texture) -> Result<Vec<u8>> {
    let (width, height) = (target.width(), target.height());
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
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
