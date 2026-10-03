//! Unified lighting (`$pref::Video::Lighting` 2): a live shadow takes
//! away only the sun a lightmap texel actually had, so baked shade is never
//! darkened twice, while Classic keeps v20's fixed darkening.
use anyhow::Result;
use bri_render::{map_lighting::TexelFix, scene::*, shadow::ShadowSettings};
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
    render_with_map(
        device,
        queue,
        renderer,
        target,
        receivers,
        casters,
        occluders,
        &[],
    )
}

/// `render`, with `map` shading objects from the sun (the map layer).
#[allow(clippy::too_many_arguments)]
fn render_with_map(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut SceneRenderer,
    target: &wgpu::Texture,
    receivers: &[&GpuScene],
    casters: &[&GpuScene],
    occluders: &[&GpuScene],
    map: &[&GpuScene],
) -> Result<Vec<u8>> {
    let (width, height) = (target.width(), target.height());
    let view = target.create_view(&Default::default());
    let depth = create_depth(device, width, height).create_view(&Default::default());
    let row = (width * 4).div_ceil(256) * 256;
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.render_shadows_with_map(
        &mut encoder,
        ShadowCasters {
            scenes: casters,
            instances: &[],
        },
        ShadowCasters {
            scenes: occluders,
            instances: &[],
        },
        map,
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

/// A 20x20 lightmapped floor at y = 0: static light 0.3 everywhere, the
/// sun baked in on the half with x < 0 and baked away (in the map's own
/// shadow) on the other.
fn floor(sun: Vec3, sun_color: f32) -> SceneData {
    const SIZE: u32 = 16;
    let facing = Vec3::Y.dot(-sun.normalize());
    let mut mission = vec![];
    let mut parts = vec![];
    for _y in 0..SIZE {
        for x in 0..SIZE {
            let lit = x < SIZE / 2;
            let fixed = 0.3f32;
            let m = (fixed + if lit { sun_color * facing } else { 0.0 }).min(1.0);
            let m = (m * 255.0 + 0.5) as u8;
            mission.extend([m, m, m, 255]);
            parts.extend([77, 77, 77, if lit { 255 } else { 0 }]);
        }
    }
    let image = |label: &str, rgba| SceneImage {
        label: label.into(),
        width: SIZE,
        height: SIZE,
        rgba,
        srgb: false,
    };
    let mut data = SceneData {
        images: vec![
            SceneImage::white(),
            image("mission", mission),
            image("parts", parts),
        ],
        ..Default::default()
    };
    let mut material = Material::surface("floor", 0, 1);
    material.images[9] = 2;
    material.parameters = Some(DECOMPOSED_LIGHTMAP);
    data.materials.push(material);
    for (x, z) in [(-10.0, -10.0), (-10.0, 10.0), (10.0, 10.0), (10.0, -10.0)] {
        data.vertices.push(SceneVertex {
            position: [x, 0.0, z],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0; 2],
            lightmap_uv: [(x + 10.0) / 20.0, (z + 10.0) / 20.0],
            color: [1.0; 4],
            fx: [0.0; 4],
        });
    }
    data.indices.extend([0, 1, 2, 0, 2, 3]);
    data.batches.push(MeshBatch {
        indices: 0..6,
        material: 0,
        center: [0.0; 3],
    });
    data
}

#[test]
fn live_shadows_remove_only_baked_sun() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let sun = Vec3::new(0.0, -1.0, 1.0);
    let map = renderer.upload(&device, &queue, &floor(sun, 0.6))?;
    // A slab over a band across both halves.
    let slab = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-12., 3.0, -1.5), Vec3::new(12., 3.3, 1.5)),
    )?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.).to_array();
    camera.sun_color = [0.6, 0.6, 0.6, 0.];
    camera.ambient = [0.3, 0.3, 0.3, 0.];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    // Where the slab's shadow lands (it leans with the sun) and open floor.
    let shade_z = 3.15;
    let points = [
        Vec3::new(-5.0, 0.0, shade_z),
        Vec3::new(-5.0, 0.0, 7.0),
        Vec3::new(5.0, 0.0, shade_z),
        Vec3::new(5.0, 0.0, 7.0),
    ];
    let mut run = |mode: f32| -> Result<[i32; 4]> {
        camera.ambient[3] = mode;
        renderer.update_camera(&queue, &camera);
        let pixels = render(
            &device,
            &queue,
            &mut renderer,
            &target,
            &[&map, &slab],
            &[&slab],
            &[],
        )?;
        Ok(points.map(|p| at(&pixels, p)))
    };
    let [lit_under, lit_open, dark_under, dark_open] = run(2.0)?;
    // Unified: baked shade stays as baked; baked sun under the slab goes
    // down to the static light, the same as the baked shade.
    assert!(
        (dark_under - dark_open).abs() <= 2,
        "{dark_under} {dark_open}"
    );
    assert!(lit_open > dark_open + 60, "{lit_open} {dark_open}");
    assert!(
        (lit_under - dark_open).abs() <= 3,
        "{lit_under} {dark_open}"
    );
    // Classic keeps v20's fixed share: it darkens the baked shade again.
    let [_, _, classic_under, classic_open] = run(0.0)?;
    assert_eq!(classic_open, dark_open);
    assert!(
        classic_under < classic_open - 10,
        "{classic_under} {classic_open}"
    );
    Ok(())
}

/// One map light over a brick floor, no sun: what the light lights and the
/// visibility volume that lets it through everywhere.
fn lamp_lighting(at: Vec3) -> bri_render::map_lighting::MapLighting {
    use bri_render::map_lighting::{MapLight, MapLighting, VisibilityVolume};
    let dims = [16u32, 8, 16];
    let mut texel = [0u8; 8];
    texel[1] = 255;
    MapLighting {
        lights: vec![MapLight {
            position: at.to_array(),
            color: [0.8; 3],
            inner: 0.0,
            outer: 40.0,
            channel: Some(0),
        }],
        report: Default::default(),
        visibility: VisibilityVolume {
            origin: [-32.0, -4.0, -32.0],
            cell: 4.0,
            dims,
            texels: vec![texel; dims.iter().map(|d| *d as usize).product()],
        },
        residual: bri_render::light_volume::LightVolume {
            origin: [0.0; 3],
            cell: 1.0,
            dims: [1; 3],
            texels: vec![[0; 4]],
            rays: 0,
        },
        residual_all: bri_render::light_volume::LightVolume {
            origin: [0.0; 3],
            cell: 1.0,
            dims: [1; 3],
            texels: vec![[0; 4]],
            rays: 0,
        },
        leaks: vec![],
        dynamic: vec![],
    }
}

#[test]
fn map_lamps_cast_live_shadows_in_unified_modes_by_shadow_quality() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let lamp = Vec3::new(0.0, 12.0, 0.0);
    let floor = cuboid(Vec3::new(-10.0, -0.3, -10.0), Vec3::new(10.0, 0.0, 10.0));
    // A slab under the lamp, over the middle of the floor.
    let slab = cuboid(Vec3::new(-2.0, 4.0, -2.0), Vec3::new(2.0, 4.3, 2.0));
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = [0.0, -1.0, 0.3, 0.0];
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.1, 0.1, 0.1, 0.];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    // The camera sees the floor from above past the slab's edge; the
    // overhead lamp throws the slab's shadow wider, to |x| < 2 * 12 / 8 = 3.
    let shade = Vec3::new(2.6, 0.0, 0.0);
    let open = Vec3::new(6.0, 0.0, -1.0);
    // The slab as a moving caster (drawn into the lamp every frame), or as
    // a brick chunk (kept lamp faces), over three frames.
    let mut run_as = |settings: ShadowSettings, mode: f32, chunk: bool| -> Result<[i32; 2]> {
        let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
        renderer.set_map_lighting(&device, &queue, Some(&lamp_lighting(lamp)), false)?;
        let floor = renderer.upload(&device, &queue, &floor)?;
        let slab = if chunk {
            let palette = renderer.upload(&device, &queue, &slab)?;
            renderer.upload_chunk(&device, &queue, &slab, &palette)?
        } else {
            renderer.upload(&device, &queue, &slab)?
        };
        camera.ambient[3] = mode;
        let mut frames = vec![];
        for _ in 0..3 {
            renderer.update_camera(&queue, &camera);
            let pixels = render(
                &device,
                &queue,
                &mut renderer,
                &target,
                &[&floor, &slab],
                &[&slab],
                &[],
            )?;
            frames.push([at(&pixels, shade), at(&pixels, open)]);
        }
        assert!(frames.windows(2).all(|w| w[0] == w[1]), "{frames:?}");
        Ok(frames[0])
    };
    let mut run = |settings: ShadowSettings, mode: f32| -> Result<[i32; 2]> {
        let moving = run_as(settings, mode, false)?;
        let kept = run_as(settings, mode, true)?;
        assert!(
            (moving[0] - kept[0]).abs() <= 12 && moving[1] == kept[1],
            "{moving:?} {kept:?}"
        );
        Ok(kept)
    };
    // Unified at Best: the slab shades the floor the lamp lights.
    let [shaded, open_floor] = run(ShadowSettings::BEST, 2.0)?;
    assert!(open_floor > 60, "{open_floor}");
    assert!(shaded < open_floor - 40, "{shaded} {open_floor}");
    // Medium casts from one lamp too.
    let [medium, _] = run(ShadowSettings::MEDIUM, 2.0)?;
    assert!(medium < open_floor - 40, "{medium} {open_floor}");
    // Low quality has no lamp shadows; the floor there, nearer the lamp, is
    // lit at least as brightly as the open floor.
    let [low, low_open] = run(ShadowSettings::LOW, 2.0)?;
    assert!(low >= low_open, "{low} {low_open}");
    // Classic never draws map lights or lamp shadows.
    let [classic, classic_open] = run(ShadowSettings::BEST, 0.0)?;
    assert!(
        (classic - classic_open).abs() <= 2,
        "{classic} {classic_open}"
    );
    Ok(())
}

/// A map with a room roof far above a brick floor, open over a square
/// "window" the sun falls straight through. The visibility volume says the
/// sun reaches every cell (as a coarse 4-unit volume near a window edge
/// can): only the map's own surfaces, through the map layer, shade objects.
#[test]
fn map_walls_shade_objects_from_the_sun_with_a_filtered_edge() -> Result<()> {
    use bri_render::map_lighting::{MapLighting, VisibilityVolume};
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let dims = [8u32, 4, 8];
    let mut texel = [0u8; 8];
    texel[0] = 255;
    renderer.set_map_lighting(
        &device,
        &queue,
        Some(&MapLighting {
            lights: vec![],
            report: Default::default(),
            visibility: VisibilityVolume {
                origin: [-16.0, -4.0, -16.0],
                cell: 4.0,
                dims,
                texels: vec![texel; dims.iter().map(|d| *d as usize).product()],
            },
            residual: bri_render::light_volume::LightVolume {
                origin: [0.0; 3],
                cell: 1.0,
                dims: [1; 3],
                texels: vec![[0; 4]],
                rays: 0,
            },
            residual_all: bri_render::light_volume::LightVolume {
                origin: [0.0; 3],
                cell: 1.0,
                dims: [1; 3],
                texels: vec![[0; 4]],
                rays: 0,
            },
            leaks: vec![],
            dynamic: vec![],
        }),
        false,
    )?;
    let floor = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-10.0, -0.3, -10.0), Vec3::new(10.0, 0.0, 10.0)),
    )?;
    // Roof pieces around a 4x4 opening, farther above the floor than live
    // casters reach toward the sun (the map layer reaches past it).
    let wall = |min: Vec3, max: Vec3| {
        let mut data = cuboid(min, max);
        data.images = vec![SceneImage::white()];
        data.materials[0] = Material::surface("roof", 0, 0);
        renderer.upload(&device, &queue, &data)
    };
    let (y0, y1) = (600.0, 601.0);
    let roof = [
        wall(Vec3::new(-40.0, y0, -40.0), Vec3::new(-2.0, y1, 40.0))?,
        wall(Vec3::new(2.0, y0, -40.0), Vec3::new(40.0, y1, 40.0))?,
        wall(Vec3::new(-2.0, y0, -40.0), Vec3::new(2.0, y1, -2.0))?,
        wall(Vec3::new(-2.0, y0, 2.0), Vec3::new(2.0, y1, 40.0))?,
    ];
    let map: Vec<&GpuScene> = roof.iter().collect();
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = [0.0, -1.0, 0.0, 0.0];
    camera.sun_color = [0.6, 0.6, 0.6, 0.];
    camera.ambient = [0.2, 0.2, 0.2, 2.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    // Under the opening, inside its edge, outside its edge, under the roof.
    let points = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.6, 0.0, 0.0),
        Vec3::new(2.4, 0.0, 0.0),
        Vec3::new(6.0, 0.0, 6.0),
    ];
    let mut frames = vec![];
    for _ in 0..2 {
        renderer.update_camera(&queue, &camera);
        let pixels = render_with_map(
            &device,
            &queue,
            &mut renderer,
            &target,
            &[&floor],
            &[],
            &[],
            &map,
        )?;
        frames.push(points.map(|p| at(&pixels, p)));
    }
    assert_eq!(frames[0], frames[1]);
    let [open, inside, outside, roofed] = frames[0];
    // Sunlit (0.2 + 0.6, and the sun's highlight 0.6 x 0.3 with the sun and
    // eye straight above) through the opening, ambient alone under the
    // roof, and the edge turns within under a unit, not a 4-unit volume cell.
    assert!((open - 250).abs() <= 4, "{open}");
    assert!((inside - open).abs() <= 4, "{inside} {open}");
    assert!((roofed - 51).abs() <= 4, "{roofed}");
    assert!((outside - roofed).abs() <= 4, "{outside} {roofed}");
    // Without the map layer the volume's sun stands in: sunlit everywhere
    // (with less of the highlight away from straight below the eye).
    renderer.update_camera(&queue, &camera);
    let pixels = render(&device, &queue, &mut renderer, &target, &[&floor], &[], &[])?;
    let unroofed = at(&pixels, points[3]);
    assert!(unroofed >= 200 && unroofed <= open + 4, "{unroofed} {open}");
    Ok(())
}

/// A lamp the fit made brighter than a lightmap texel holds (0.55 fitted
/// against 0.3 baked): a live shadow from it takes only the lamp's share of
/// the fitted light plus ambient, never the texel's whole light.
#[test]
fn lamp_shadows_on_the_map_take_only_the_lamps_share() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::BEST));
    renderer.set_map_lighting(
        &device,
        &queue,
        Some(&lamp_lighting(Vec3::new(0.0, 12.0, 0.0))),
        false,
    )?;
    // Static light 0.3 everywhere and no sun colour: the map draws 0.3.
    let sun = Vec3::new(0.0, -1.0, 0.3);
    let map = renderer.upload(&device, &queue, &floor(sun, 0.0))?;
    let slab = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-2.0, 4.0, -2.0), Vec3::new(2.0, 4.3, 2.0)),
    )?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.1, 0.1, 0.1, 2.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    let shade = Vec3::new(2.6, 0.0, 0.0);
    let open = Vec3::new(6.0, 0.0, -1.0);
    renderer.update_camera(&queue, &camera);
    let pixels = render(
        &device,
        &queue,
        &mut renderer,
        &target,
        &[&map],
        &[&slab],
        &[],
    )?;
    let (shaded, lit) = (at(&pixels, shade), at(&pixels, open));
    assert!((lit - 77).abs() <= 2, "{lit}");
    // The lamp gives 0.554 of 0.654 fitted there: 0.3 * (1 - 0.554 / 0.654)
    // = 0.046 stays (12), where taking the lamp's whole claim left black.
    assert!((shaded - 12).abs() <= 4, "{shaded}");
    Ok(())
}

/// A player-sized instanced model (the way players, items and vehicles
/// draw) standing on a lightmapped floor beside a low map light, at High
/// shadows: its lamp shadow darkens the floor on the far side.
#[test]
fn an_instanced_model_casts_a_lamp_shadow_on_the_map() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::HIGH));
    // The light 6 above the floor, 12 from the model, as the Bedroom desk
    // lamp's lower light stands to a player on the dresser.
    renderer.set_map_lighting(
        &device,
        &queue,
        Some(&lamp_lighting(Vec3::new(-12.0, 6.0, 0.0))),
        false,
    )?;
    let sun = Vec3::new(0.0, -1.0, 0.3);
    let map = renderer.upload(&device, &queue, &floor(sun, 0.0))?;
    let body = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-0.5, 0.0, -0.3), Vec3::new(0.5, 2.5, 0.3)),
    )?;
    let mut instances = GpuInstances::new(&device, 1)?;
    instances.update(&queue, &[SceneTransform::default()])?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.1, 0.1, 0.1, 2.0];
    let target = color_target(&device, format, width, height);
    let view = target.create_view(&Default::default());
    let depth = create_depth(&device, width, height).create_view(&Default::default());
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let row = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    renderer.update_camera(&queue, &camera);
    let mut encoder = device.create_command_encoder(&Default::default());
    let models = [(&body, &instances)];
    renderer.render_shadows(
        &mut encoder,
        ShadowCasters {
            scenes: &[],
            instances: &models,
        },
        ShadowCasters {
            scenes: &[],
            instances: &[],
        },
    );
    renderer.render_with_instances(
        &mut encoder,
        &view,
        &depth,
        &[&map],
        &[],
        Some(wgpu::Color::BLACK),
    );
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
    let pixels = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("{e:?}"))?
        .to_vec();
    let at = |point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[y * row as usize + x * 4 + 1])
    };
    // Past the model, away from the light, and the same distance off to the side.
    let (shaded, open) = (at(Vec3::new(2.5, 0.0, 0.0)), at(Vec3::new(-2.0, 0.0, 6.0)));
    assert!(open > 60, "{open}");
    assert!(shaded < open - 20, "{shaded} {open}");
    Ok(())
}

/// Beside furniture the visibility volume's few-unit cells can sit inside
/// the map and hide a lamp from the surfaces next to it, or show it through
/// a wall. A lamp with a shadow slot reads the map's own walls from its map
/// faces instead: under a slab its shadow shows where the volume hid the
/// lamp, and behind a wall, where the volume showed it, a slab takes away
/// no light the lamp never gave.
#[test]
fn shadowed_lamps_reach_past_the_map_walls_not_the_coarse_volume() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::BEST));
    // The volume hides the lamp everywhere left of x = 4 and shows it past.
    let mut lighting = lamp_lighting(Vec3::new(0.0, 12.0, 0.0));
    let dx = lighting.visibility.dims[0];
    for (i, texel) in lighting.visibility.texels.iter_mut().enumerate() {
        texel[1] = if i as u32 % dx >= 9 { 255 } else { 0 };
    }
    renderer.set_map_lighting(&device, &queue, Some(&lighting), false)?;
    let sun = Vec3::new(0.0, -1.0, 0.3);
    let floor = renderer.upload(&device, &queue, &floor(sun, 0.0))?;
    // A map wall at x = 4 the lamp cannot see past.
    let mut wall = cuboid(Vec3::new(4.0, 0.0, -10.0), Vec3::new(4.2, 6.0, 10.0));
    wall.images = vec![SceneImage::white()];
    wall.materials[0] = Material::surface("wall", 0, 0);
    let wall = renderer.upload(&device, &queue, &wall)?;
    let slab = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-2.0, 4.0, -2.0), Vec3::new(2.0, 4.3, 2.0)),
    )?;
    let behind = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(5.0, 2.0, -3.0), Vec3::new(8.0, 2.3, 1.0)),
    )?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.1, 0.1, 0.1, 2.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    let points = [
        Vec3::new(2.6, 0.0, 0.0),
        Vec3::new(-6.0, 0.0, -1.0),
        Vec3::new(6.5, 0.0, -1.0),
    ];
    let mut frames = vec![];
    for _ in 0..2 {
        renderer.update_camera(&queue, &camera);
        let pixels = render_with_map(
            &device,
            &queue,
            &mut renderer,
            &target,
            &[&floor],
            &[&slab, &behind],
            &[],
            &[&floor, &wall],
        )?;
        frames.push(points.map(|p| at(&pixels, p)));
    }
    assert_eq!(frames[0], frames[1]);
    let [shaded, open, walled] = frames[0];
    // Unshadowed the floor shows as baked, its own map face never shading it.
    assert!((open - 77).abs() <= 2, "{open}");
    // The slab's full shadow, as where the volume shows the lamp.
    assert!((shaded - 12).abs() <= 4, "{shaded}");
    // Behind the wall the slab over it removes nothing.
    assert!((walled - 77).abs() <= 2, "{walled}");
    Ok(())
}

/// Lightmap leak cleanup reaches an uploaded map: patched images draw at
/// once, in every lighting mode.
#[test]
fn patched_lightmaps_draw_without_a_new_upload() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (64u32, 64u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, None);
    let sun = Vec3::new(0.0, -1.0, 0.3);
    let mut data = floor(sun, 0.0);
    let map = renderer.upload(&device, &queue, &data)?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.1, 0.1, 0.1, 0.0];
    let target = color_target(&device, format, width, height);
    let centre =
        |pixels: &[u8]| i32::from(pixels[((height / 2 * width + width / 2) * 4 + 1) as usize]);
    for mode in [0.0, 2.0] {
        camera.ambient[3] = mode;
        renderer.update_camera(&queue, &camera);
        let before = centre(&render(
            &device,
            &queue,
            &mut renderer,
            &target,
            &[&map],
            &[],
            &[],
        )?);
        assert!((before - 77).abs() <= 2, "{before}");
    }
    // Every texel of the drawn lightmap and its static light down to 30.
    let fixes: Vec<TexelFix> = (0..16 * 16)
        .flat_map(|index| {
            [
                TexelFix {
                    image: 1,
                    index,
                    rgba: [30, 30, 30, 255],
                },
                TexelFix {
                    image: 2,
                    index,
                    rgba: [30, 30, 30, 0],
                },
            ]
        })
        .collect();
    let changed = TexelFix::apply(&fixes, &mut data.images);
    assert_eq!(changed, vec![1, 2]);
    map.patch_images(&queue, &data.images, &changed)?;
    for mode in [0.0, 2.0] {
        camera.ambient[3] = mode;
        renderer.update_camera(&queue, &camera);
        let after = centre(&render(
            &device,
            &queue,
            &mut renderer,
            &target,
            &[&map],
            &[],
            &[],
        )?);
        assert!((after - 30).abs() <= 2, "mode {mode}: {after}");
    }
    Ok(())
}

/// A brick placed under a lamp, or taken away, shows in (or leaves) the
/// lamp's shadow the very frame it changes, not when its kept face's turn
/// to refresh comes round (pharzedia's lagging shadows on the Bedroom
/// dresser).
#[test]
fn placed_and_removed_bricks_change_lamp_shadows_the_same_frame() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let lamp = Vec3::new(0.0, 12.0, 0.0);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::BEST));
    renderer.set_map_lighting(&device, &queue, Some(&lamp_lighting(lamp)), false)?;
    let floor = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-10.0, -0.3, -10.0), Vec3::new(10.0, 0.0, 10.0)),
    )?;
    let slab_data = cuboid(Vec3::new(-2.0, 4.0, -2.0), Vec3::new(2.0, 4.3, 2.0));
    let palette = renderer.upload(&device, &queue, &slab_data)?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = [0.0, -1.0, 0.3, 0.0];
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.1, 0.1, 0.1, 2.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    let shade = Vec3::new(2.6, 0.0, 0.0);
    let open = Vec3::new(6.0, 0.0, -1.0);
    // Settle with the floor alone, then place and remove the slab (a new
    // chunk each time it is placed, as a rebuilt chunk is) frame by frame:
    // six faces refresh one a frame in turn, so waiting for a turn would
    // miss most of these frames.
    for _ in 0..8 {
        renderer.update_camera(&queue, &camera);
        render(&device, &queue, &mut renderer, &target, &[&floor], &[], &[])?;
    }
    for frame in 0..8 {
        let placed = frame % 2 == 0;
        let slab = renderer.upload_chunk(&device, &queue, &slab_data, &palette)?;
        let scenes: Vec<&GpuScene> = if placed {
            vec![&floor, &slab]
        } else {
            vec![&floor]
        };
        let casters: Vec<&GpuScene> = if placed { vec![&slab] } else { vec![] };
        renderer.update_camera(&queue, &camera);
        let pixels = render(
            &device,
            &queue,
            &mut renderer,
            &target,
            &scenes,
            &casters,
            &[],
        )?;
        let (shaded, lit) = (at(&pixels, shade), at(&pixels, open));
        if placed {
            assert!(
                shaded < lit - 40,
                "frame {frame}: placed slab casts no shadow yet ({shaded} vs {lit})"
            );
        } else {
            assert!(
                shaded >= lit,
                "frame {frame}: removed slab still shades ({shaded} vs {lit})"
            );
        }
    }
    Ok(())
}

/// A map light switched off (a broken bulb) or recoloured (an Add-On) at
/// run time: it leaves objects and its share of the map's baked light, and
/// the rest of the baked light stays. Both with a shadowed lamp (Best) and
/// without lamp shadows (Low), where only the tint path runs.
#[test]
fn switched_off_and_recoloured_map_lights_leave_the_map_and_objects() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let sun = Vec3::new(0.0, -1.0, 0.3);
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.1, 0.1, 0.1, 2.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3, channel: usize| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + channel])
    };
    let open = Vec3::new(6.0, 0.0, -1.0);
    let block_top = Vec3::new(-5.0, 0.5, 0.0);
    for settings in [ShadowSettings::BEST, ShadowSettings::LOW] {
        let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
        renderer.set_map_lighting(
            &device,
            &queue,
            Some(&lamp_lighting(Vec3::new(0.0, 12.0, 0.0))),
            false,
        )?;
        let map = renderer.upload(&device, &queue, &floor(sun, 0.0))?;
        let block = renderer.upload(
            &device,
            &queue,
            &cuboid(Vec3::new(-6.0, 0.0, -1.0), Vec3::new(-4.0, 0.5, 1.0)),
        )?;
        let mut frame = |tint: Vec3| -> Result<Vec<u8>> {
            renderer.set_map_light_tints(&queue, &[tint]);
            renderer.update_camera(&queue, &camera);
            render(
                &device,
                &queue,
                &mut renderer,
                &target,
                &[&map, &block],
                &[&block],
                &[],
            )
        };
        let on = frame(Vec3::ONE)?;
        let off = frame(Vec3::ZERO)?;
        let red = frame(Vec3::new(1.0, 0.0, 0.0))?;
        let back = frame(Vec3::ONE)?;
        // The map: 0.3 baked, of which the lamp gives 0.531 of 0.631
        // fitted there, so 0.3 * 0.1 / 0.631 = 0.048 (12) stays when it is off.
        let (map_on, map_off) = (at(&on, open, 1), at(&off, open, 1));
        assert!((map_on - 77).abs() <= 2, "{settings:?}: {map_on}");
        assert!((map_off - 12).abs() <= 4, "{settings:?}: {map_off}");
        // Red keeps the red share and drops the rest.
        assert!(
            (at(&red, open, 0) - 77).abs() <= 2 && (at(&red, open, 1) - 12).abs() <= 4,
            "{settings:?}"
        );
        // The block loses the lamp's light and keeps the ambient.
        let (block_on, block_off) = (at(&on, block_top, 1), at(&off, block_top, 1));
        assert!(
            block_off < block_on - 40,
            "{settings:?}: {block_off} vs {block_on}"
        );
        assert!(
            at(&red, block_top, 0) > at(&red, block_top, 1) + 40,
            "{settings:?}"
        );
        // Switched back, it draws as before.
        assert_eq!(at(&back, open, 1), map_on, "{settings:?}");
        assert_eq!(at(&back, block_top, 1), block_on, "{settings:?}");
    }
    Ok(())
}

/// `floor` equipped for the Dynamic mode as the client does it: its
/// leftover lightmap `left` and, per light in `lights`, the share of it each
/// texel receives (`seen(light, column)`, one row like the next).
fn dynamic_floor(
    sun: Vec3,
    left: impl Fn(u32) -> [u8; 4],
    lights: &[u8],
    seen: impl Fn(usize, u32) -> f32,
) -> SceneData {
    use bri_render::map_lighting::DynamicSheet;
    let mut data = floor(sun, 0.0);
    let texels =
        |f: &dyn Fn(u32) -> [u8; 4]| -> Vec<u8> { (0..16 * 16).flat_map(|i| f(i % 16)).collect() };
    let visibility = (0..lights.len().div_ceil(4))
        .map(|k| {
            texels(&|x| {
                std::array::from_fn(|c| {
                    if 4 * k + c < lights.len() {
                        bri_render::map_lighting::share_byte(seen(4 * k + c, x))
                    } else {
                        0
                    }
                })
            })
        })
        .collect();
    let sheet = DynamicSheet {
        parts_image: 2,
        width: 16,
        height: 16,
        left: texels(&left),
        lights: lights.to_vec(),
        visibility,
    };
    assert!(DynamicSheet::equip(&[sheet], &mut data));
    data
}

/// Modern Dynamic ignores contradictory legacy shares and residuals. Current
/// geometry hides a point light from map surfaces and objects alike; removing
/// that geometry invalidates its cached cube. Live casters use bounded slots.
#[test]
fn dynamic_lighting_lights_map_surfaces_live_from_every_light() -> Result<()> {
    use bri_render::map_lighting::MapLight;
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let sun = Vec3::new(0.0, -1.0, 0.3);
    // 0.05 left over everywhere, no baked sun; the light reaches the
    // texels in front of the wall (x = 4, texel column 11) and not behind.
    let floor_data = dynamic_floor(
        sun,
        |_| [13, 13, 13, 0],
        &[0],
        |_, x| if x <= 10 { 1.0 } else { 0.0 },
    );
    let mut wall = cuboid(Vec3::new(4.0, 0.0, -10.0), Vec3::new(4.2, 6.0, 10.0));
    wall.images = vec![SceneImage::white()];
    wall.materials[0] = Material::surface("wall", 0, 0);
    let mut lighting = lamp_lighting(Vec3::new(0.0, 12.0, 0.0));
    lighting.lights = vec![MapLight {
        position: [0.0, 12.0, 0.0],
        color: [0.8; 3],
        inner: 0.0,
        outer: 40.0,
        channel: None,
    }];
    for texel in &mut lighting.visibility.texels {
        *texel = [0; 8];
    }
    lighting.visibility.dims = [0; 3];
    lighting.visibility.texels.clear();
    lighting.visibility.origin = [f32::NAN; 3];
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
    camera.sun_color = [0.0; 4];
    camera.ambient = [0.0, 0.0, 0.0, 3.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    // The floor in front of the wall, behind it and under the slab; the
    // blocks' tops in front and behind.
    let points = [
        Vec3::new(-6.0, 0.0, -1.0),
        Vec3::new(6.5, 0.0, -1.0),
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(-6.0, 0.5, 5.0),
        Vec3::new(6.5, 0.5, 5.0),
    ];
    let expected = |p: Vec3| {
        let distance = p.distance(Vec3::new(0.0, 12.0, 0.0));
        let lit = 0.8 * (40.0 - distance) / 40.0 * 12.0 / distance;
        (lit * 255.0).round() as i32
    };
    for lamps in [0, ShadowSettings::BEST.lamps] {
        let settings = ShadowSettings {
            lamps,
            light_cubes: true,
            ..ShadowSettings::BEST
        };
        let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
        renderer.set_map_lighting(&device, &queue, Some(&lighting), true)?;
        let floor = renderer.upload(&device, &queue, &floor_data)?;
        let mut wall = renderer.upload(&device, &queue, &wall)?;
        let slab = renderer.upload(
            &device,
            &queue,
            &cuboid(Vec3::new(-2.0, 4.0, -2.0), Vec3::new(2.0, 4.3, 2.0)),
        )?;
        let front_block = renderer.upload(
            &device,
            &queue,
            &cuboid(Vec3::new(-7.0, 0.0, 4.0), Vec3::new(-5.0, 0.5, 6.0)),
        )?;
        let behind_block = renderer.upload(
            &device,
            &queue,
            &cuboid(Vec3::new(5.5, 0.0, 4.0), Vec3::new(7.5, 0.5, 6.0)),
        )?;
        let frame = |renderer: &mut SceneRenderer, wall: &GpuScene| -> Result<[i32; 5]> {
            renderer.update_camera(&queue, &camera);
            let pixels = render_with_map(
                &device,
                &queue,
                renderer,
                &target,
                &[&floor, &front_block, &behind_block],
                &[&slab],
                &[],
                &[&floor, wall],
            )?;
            Ok(points.map(|p| at(&pixels, p)))
        };
        let [front, behind, under, lit_block, hidden_block] = frame(&mut renderer, &wall)?;
        assert_eq!(
            frame(&mut renderer, &wall)?,
            [front, behind, under, lit_block, hidden_block],
            "lamps {lamps}"
        );
        assert!(
            front >= expected(points[0]) - 3 && front <= expected(points[0]) + 35,
            "lamps {lamps}: front {front}"
        );
        assert!(behind <= 2, "lamps {lamps}: behind {behind}");
        if lamps == 0 {
            // No slot, no brick shadow: the texels see only the map.
            assert!(under >= expected(points[2]) - 3, "under {under}");
        } else {
            assert!(under <= 3, "under the slab {under}");
        }
        assert!(lit_block > 100, "lamps {lamps}: block in front {lit_block}");
        assert!(
            hidden_block < 10,
            "lamps {lamps}: block behind the wall {hidden_block}"
        );
        renderer.set_map_light_tints(&queue, &[Vec3::ZERO]);
        let off = frame(&mut renderer, &wall)?;
        assert!(
            off[..3].iter().all(|v| *v <= 2),
            "lamps {lamps}: switched off {off:?}"
        );
        assert!(off[3] < 10, "lamps {lamps}: block switched off {}", off[3]);
        renderer.set_map_light_tints(&queue, &[Vec3::ONE]);
        wall.hide_indices(std::slice::from_ref(&(0..36)));
        let removed = frame(&mut renderer, &wall)?;
        assert!(
            removed[1] > 80 && removed[4] > 80,
            "geometry removal relights floor and object: {removed:?}"
        );
    }
    Ok(())
}

/// Poisoned legacy sun masks cannot shadow Dynamic. Current geometry does,
/// on both originally bright and originally dark texels. Night is live too.
#[test]
fn dynamic_sun_ignores_baked_masks_and_uses_current_geometry() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let settings = ShadowSettings {
        light_cubes: true,
        ..ShadowSettings::LOW
    };
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
    let mut lighting = lamp_lighting(Vec3::new(0.0, 12.0, 0.0));
    lighting.lights.clear();
    for texel in &mut lighting.visibility.texels {
        *texel = [255, 0, 0, 0, 0, 0, 0, 0];
    }
    renderer.set_map_lighting(&device, &queue, Some(&lighting), true)?;
    let sun = Vec3::new(0.0, -1.0, 0.0);
    let floor_data = dynamic_floor(
        sun,
        |x| [51, 51, 51, if x < 8 { 255 } else { 0 }],
        &[],
        |_, _| 0.0,
    );
    let floor = renderer.upload(&device, &queue, &floor_data)?;
    let slab = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-8.0, 4.0, -6.0), Vec3::new(-4.0, 4.3, -2.0)),
    )?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.0).to_array();
    camera.sun_color = [0.6, 0.6, 0.6, 0.];
    camera.ambient = [0.2, 0.2, 0.2, 3.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    // The baked sunlit half, the baked shaded half, and under the slab on
    // the sunlit half.
    let points = [
        Vec3::new(-6.0, 0.0, 6.0),
        Vec3::new(6.0, 0.0, 6.0),
        Vec3::new(-6.0, 0.0, -4.0),
    ];
    renderer.update_camera(&queue, &camera);
    let pixels = render_with_map(
        &device,
        &queue,
        &mut renderer,
        &target,
        &[&floor],
        &[&slab],
        &[],
        &[&floor],
    )?;
    let [sunlit, shaded, under] = points.map(|p| at(&pixels, p));
    // Live ambient 0.2 plus the current sun's 0.6 on both halves; ambient
    // alone under the current brick shadow.
    assert!(sunlit >= 200, "{sunlit}");
    assert!(
        (shaded - sunlit).abs() <= 3,
        "legacy dark half is live: {shaded}/{sunlit}"
    );
    assert!((under - 51).abs() <= 4, "{under}");
    let mut poison = floor_data.clone();
    for image in poison.images.iter_mut().skip(1) {
        for texel in image.rgba.chunks_exact_mut(4) {
            texel.copy_from_slice(&[255, 0, 255, 255]);
        }
    }
    let poisoned = renderer.upload(&device, &queue, &poison)?;
    renderer.update_camera(&queue, &camera);
    camera.baked_sun_direction = [1.0, 0.0, 0.0, 1.0];
    camera.baked_sun_color = [50.0; 4];
    camera.baked_ambient = [50.0; 4];
    renderer.update_camera(&queue, &camera);
    let after = render_with_map(
        &device,
        &queue,
        &mut renderer,
        &target,
        &[&poisoned],
        &[&slab],
        &[],
        &[&poisoned],
    )?;
    assert_eq!(
        pixels, after,
        "all baked illumination/visibility bytes are irrelevant"
    );
    camera.sun_color = [0.; 4];
    renderer.update_camera(&queue, &camera);
    let night = render_with_map(
        &device,
        &queue,
        &mut renderer,
        &target,
        &[&poisoned],
        &[&slab],
        &[],
        &[&poisoned],
    )?;
    for p in points {
        assert!((at(&night, p) - 51).abs() <= 3);
    }
    Ok(())
}

/// A map light switched off (a broken bulb) leaves the same light in the
/// Unified mode, from the bake's per-texel shares: here a
/// light without a visibility channel (which the Unified modes could not
/// switch before), giving 64 levels of the floor's 77. Off, the floor keeps
/// its 13 in Unified; on, it shows as baked.
#[test]
fn unified_switchable_light_preserves_its_original_per_texel_output() -> Result<()> {
    use bri_render::map_lighting::MapLight;
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (128u32, 128u32);
    let sun = Vec3::new(0.0, -1.0, 0.3);
    let floor_data = dynamic_floor(sun, |_| [13, 13, 13, 0], &[0], |_, _| 1.0);
    let mut lighting = lamp_lighting(Vec3::new(0.0, 12.0, 0.0));
    lighting.lights = vec![MapLight {
        position: [0.0, 12.0, 0.0],
        color: [64.0 / 255.0; 3],
        inner: 1000.0,
        outer: 2000.0,
        channel: None,
    }];
    let target = color_target(&device, format, width, height);
    {
        let mode = 2.0;
        let mut renderer =
            SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::BEST));
        renderer.set_map_lighting(&device, &queue, Some(&lighting), false)?;
        let floor = renderer.upload(&device, &queue, &floor_data)?;
        let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
        camera.sun_direction = sun.extend(0.0).to_array();
        camera.sun_color = [0.0; 4];
        camera.ambient = [0.0, 0.0, 0.0, mode];
        let mut frame = |tint: Vec3| -> Result<i32> {
            renderer.set_map_light_tints(&queue, &[tint]);
            renderer.update_camera(&queue, &camera);
            let pixels = render(&device, &queue, &mut renderer, &target, &[&floor], &[], &[])?;
            Ok(i32::from(
                pixels[((height / 2) * width + width / 2) as usize * 4 + 1],
            ))
        };
        let on = frame(Vec3::ONE)?;
        let off = frame(Vec3::ZERO)?;
        assert!((on - 77).abs() <= 2, "mode {mode}: on {on}");
        assert!((off - 13).abs() <= 2, "mode {mode}: off {off}");
    }
    Ok(())
}

/// The admin Environment (bri_content::atmosphere) relights the map's
/// lightmaps from what they were baked with: an unchanged environment draws
/// exactly as before, night takes the baked sun away, more ambient light
/// raises everything, and a moved sun lights the texels the bake left open.
#[test]
fn a_changed_environment_relights_the_maps_lightmaps() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (128u32, 128u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let sun = Vec3::new(0.0, -1.0, 1.0);
    let map = renderer.upload(&device, &queue, &floor(sun, 0.6))?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.).to_array();
    camera.sun_color = [0.6, 0.6, 0.6, 1.];
    camera.ambient = [0.3, 0.3, 0.3, 2.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    let points = [Vec3::new(-5.0, 0.0, 5.0), Vec3::new(5.0, 0.0, 5.0)];
    let baked = camera;
    let authored = bri_content::atmosphere::Live {
        sun_direction: sun.to_array(),
        direct_light: [0.6; 3],
        ambient_light: [0.3; 3],
        shadow_color: None,
        fog_start: 0.0,
        fog_end: 0.0,
        fog_color: [0.0; 3],
        sky_tint: [1.0; 3],
        flare: ([0.0; 4], 1.0),
        vignette: None,
    };
    let mut run = |live: Option<&bri_content::atmosphere::Live>| -> Result<[i32; 2]> {
        let mut camera = baked;
        if let Some(live) = live {
            camera.apply_atmosphere(live);
        }
        renderer.update_camera(&queue, &camera);
        let pixels = render(&device, &queue, &mut renderer, &target, &[&map], &[], &[])?;
        Ok(points.map(|p| at(&pixels, p)))
    };
    let [lit, shade] = run(None)?;
    assert!(lit > shade + 60, "{lit} {shade}");
    assert_eq!(run(Some(&authored))?, [lit, shade]);
    // No sun: the baked sun goes, the static light stays.
    let night = bri_content::atmosphere::Live {
        direct_light: [0.0; 3],
        ..authored
    };
    let [night_lit, night_shade] = run(Some(&night))?;
    assert!((night_lit - shade).abs() <= 3, "{night_lit} {shade}");
    assert!((night_shade - shade).abs() <= 3, "{night_shade} {shade}");
    // More ambient light raises both halves by that much.
    let bright = bri_content::atmosphere::Live {
        ambient_light: [0.5; 3],
        ..authored
    };
    let [bright_lit, bright_shade] = run(Some(&bright))?;
    assert!(
        (bright_shade - shade - 51).abs() <= 3,
        "{bright_shade} {shade}"
    );
    assert!(bright_lit >= lit + 40, "{bright_lit} {lit}");
    // The sun overhead: the half the bake lit takes it at full strength;
    // the half in the map's own shadow stays shaded.
    let noon = bri_content::atmosphere::Live {
        sun_direction: [0.0, -1.0, 0.0],
        ..authored
    };
    let [noon_lit, noon_shade] = run(Some(&noon))?;
    assert!((noon_lit - 230).abs() <= 4, "{noon_lit}");
    assert!((noon_shade - shade).abs() <= 3, "{noon_shade} {shade}");
    Ok(())
}

/// The water surface, shore and authored reflection darken together, in
/// plain and depth-mapped paths and at both near and far viewing distances.
#[test]
fn water_follows_the_live_day_night_environment() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = color_target(&device, format, 128, 128);
    for depth_mapped in [false, true] {
        let mut scene = SceneData::default();
        let mut water = bri_content::water::Water::volume([-30.0, -2.0, -30.0], [30.0, 0.0, 30.0]);
        water.depth_mask = depth_mapped;
        water.reflection = Some(water.surface.clone());
        water.reflection_intensity = 0.5;
        water.wave_amplitude = 0.0;
        bri_render::water_scene::append(
            &mut scene,
            &water,
            [0; 3],
            ([0.5; 4], 8.0),
            false,
            |_, _| None,
        )?;
        let mut renderer = SceneRenderer::with_settings(&device, format, 1, None);
        let mesh = renderer.upload(&device, &queue, &scene)?;
        for mode in [0.0, 3.0] {
            for distance in [12.0, 100.0] {
                let mut camera =
                    Camera::perspective([0.0, distance, 0.1], [0.0; 3], 1.0, 1.2, 0.05, 400.0);
                camera.apply_environment(&scene);
                camera.ambient[3] = mode;
                let day = camera;
                renderer.update_camera(&queue, &day);
                let pixels = render(&device, &queue, &mut renderer, &target, &[&mesh], &[], &[])?;
                if mode == 3.0 {
                    let mut poison = day;
                    poison.baked_sun_direction = [1.0, 0.0, 0.0, 1.0];
                    poison.baked_sun_color = [50.0; 4];
                    poison.baked_ambient = [50.0; 4];
                    renderer.update_camera(&queue, &poison);
                    let after =
                        render(&device, &queue, &mut renderer, &target, &[&mesh], &[], &[])?;
                    assert_eq!(
                        pixels, after,
                        "modern water ignores baked environment uniforms"
                    );
                }
                let middle = (64 * 128 + 64) * 4;
                let daytime = pixels[middle..middle + 3]
                    .iter()
                    .map(|c| u32::from(*c))
                    .sum::<u32>();
                camera.baked_sun_direction = day.sun_direction;
                camera.baked_sun_direction[3] = 1.0;
                camera.baked_sun_color = day.sun_color;
                camera.baked_ambient = day.ambient;
                camera.sun_color = [0.01, 0.01, 0.02, 0.0];
                camera.ambient = [0.02, 0.025, 0.04, mode];
                renderer.update_camera(&queue, &camera);
                let pixels = render(&device, &queue, &mut renderer, &target, &[&mesh], &[], &[])?;
                let nighttime = pixels[middle..middle + 3]
                    .iter()
                    .map(|c| u32::from(*c))
                    .sum::<u32>();
                assert!(
                    daytime > 100 && nighttime * 3 < daytime,
                    "water mode={mode}, depth={depth_mapped}, distance={distance}: day={daytime}, night={nighttime}"
                );
            }
        }
    }
    Ok(())
}

/// Real converted content, source recovery only: no legacy bake, cleanup or
/// residual preparation. Captures are generated under target, never committed.
#[test]
#[ignore = "requires generated v20 content; bounded offscreen inspection"]
fn modern_dynamic_real_maps_without_legacy_preparation() -> Result<()> {
    use bri_render::terrain_scene::GpuTerrain;
    use std::{path::PathBuf, time::Instant};
    let content = std::env::var_os("BRI_CONTENT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    );
    let bundle = std::env::var_os("BRI_MODERN_BUNDLE").map_or_else(
        || bri_package::testing::pack_dir(&content, "map_bundle"),
        PathBuf::from,
    );
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("modern-lighting");
    std::fs::create_dir_all(&out)?;
    let (device, queue) = gpu()?;
    let (width, height) = (960, 540);
    let target = color_target(&device, wgpu::TextureFormat::Rgba8Unorm, width, height);
    let view = target.create_view(&Default::default());
    let depth = create_depth(&device, width, height).create_view(&Default::default());
    let bundle_data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("bundle.json"))?)?;
    let parameters = bri_render::lighting_parameters::Parameters::read(&bundle)?;
    for record in bundle_data["maps"].as_array().unwrap() {
        let id = record["id"].as_str().unwrap();
        let name = id.rsplit('/').next().unwrap().trim_end_matches(".mis");
        let started = Instant::now();
        let map = bri_render::scene_loader::load_map_bundle_dynamic(&bundle, id)?;
        let recovery_ms = started.elapsed().as_secs_f64() * 1000.;
        assert!(map.scene.lightmap_bases.is_empty());
        let lights = parameters.lights(id)?;
        assert_eq!(map.modern_lights.as_deref(), Some(lights.as_slice()));
        let settings = ShadowSettings {
            light_cubes: true,
            ..ShadowSettings::BEST
        };
        let mut renderer = SceneRenderer::with_settings(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            1,
            Some(settings),
        );
        renderer.set_dynamic_lights(&device, &queue, &lights)?;
        let scene = renderer.upload(&device, &queue, &map.scene)?;
        let spawn = Vec3::from(map.scene.spawn);
        let eye = spawn + Vec3::new(0., 3., 0.);
        let mut camera = Camera::perspective(
            eye.to_array(),
            (eye + Vec3::new(30., -3., 30.)).to_array(),
            width as f32 / height as f32,
            1.1,
            0.05,
            4000.,
        );
        camera.apply_environment(&map.scene);
        camera.ambient[3] = 3.;
        let mut terrain = map
            .terrain
            .into_iter()
            .map(|t| GpuTerrain::upload(&renderer, &device, &queue, t.into(), 4000.))
            .collect::<Result<Vec<_>>>()?;
        for t in &mut terrain {
            t.update(&device, &queue, &[eye], 4000.)?;
        }
        let terrain_draws: Vec<_> = terrain.iter().flat_map(GpuTerrain::draws).collect();
        let mut frame_ms = Vec::new();
        for frame in 0..22 {
            let start = Instant::now();
            renderer.update_camera(&queue, &camera);
            let mut encoder = device.create_command_encoder(&Default::default());
            renderer.render_shadows_with_geometry(
                &mut encoder,
                ShadowCasters {
                    scenes: &[],
                    instances: &[],
                },
                ShadowCasters {
                    scenes: &[],
                    instances: &[],
                },
                ShadowCasters {
                    scenes: &[&scene],
                    instances: &terrain_draws,
                },
            );
            renderer.render_with_instances(
                &mut encoder,
                &view,
                &depth,
                &[&scene],
                &terrain_draws,
                Some(wgpu::Color {
                    r: 1.,
                    g: 0.,
                    b: 1.,
                    a: 1.,
                }),
            );
            queue.submit([encoder.finish()]);
            device.poll(wgpu::PollType::wait_indefinitely())?;
            if frame >= 6 {
                frame_ms.push(start.elapsed().as_secs_f64() * 1000.);
            }
        }
        // Read the final frame without an extra render or legacy preparation.
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
        device.poll(wgpu::PollType::wait_indefinitely())?;
        let mapped = buffer.slice(..).get_mapped_range()?;
        let pixels: Vec<u8> = mapped
            .chunks_exact(row as usize)
            .flat_map(|r| r[..width as usize * 4].iter().copied())
            .collect();
        assert!(
            pixels.chunks_exact(4).any(|p| p[..3] != [255, 0, 255]),
            "{name} did not draw over the diagnostic clear colour"
        );
        image::save_buffer(
            out.join(format!("{name}-dynamic.png")),
            &pixels,
            width,
            height,
            image::ColorType::Rgba8,
        )?;
        frame_ms.sort_by(f64::total_cmp);
        eprintln!(
            "{name}: {} prepared lights loaded with geometry in {recovery_ms:.1} ms; 16 warm submitted/waited frames at 960x540 p50 {:.2} ms max {:.2} ms; {out:?}",
            lights.len(),
            frame_ms[8],
            frame_ms[15]
        );
    }
    Ok(())
}

#[test]
fn modern_loader_requires_neither_baked_images_nor_legacy_preparation() -> Result<()> {
    use bri_render::{
        lighting_parameters::{FILE, Parameters, fingerprint},
        scene_loader,
    };
    use std::{collections::BTreeMap, path::PathBuf};
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("modern-loader-{}", std::process::id()));
    let maps = bri_render::testing::rooms();
    bri_render::testing::write_bundle(&dir, &maps)?;
    let mut bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("bundle.json"))?)?;
    for lighting in bundle["lighting"].as_object_mut().unwrap().values_mut() {
        lighting["interiors"] =
            serde_json::json!([{"node":0,"detail":0,"slot":0,"file":"missing-illumination.png"}]);
    }
    std::fs::write(dir.join("bundle.json"), serde_json::to_vec(&bundle)?)?;
    for filename in bundle["assets"]
        .as_object()
        .unwrap()
        .values()
        .filter_map(|v| v.as_str())
    {
        if filename.ends_with(".interior.json") {
            let path = dir.join(filename);
            let mut interior: bri_content::interior::Interior =
                serde_json::from_slice(&std::fs::read(&path)?)?;
            for detail in &mut interior.details {
                for lightmap in &mut detail.lightmaps {
                    lightmap.png = vec![0, 1, 2, 3]; // deliberately not an image
                }
            }
            std::fs::write(path, serde_json::to_vec(&interior)?)?;
        }
    }
    let prepared = Parameters {
        schema_version: 1,
        bundle_sha256: fingerprint(&dir)?,
        maps: maps
            .iter()
            .map(|m| (m.id.clone(), Vec::new()))
            .collect::<BTreeMap<_, _>>(),
    };
    std::fs::write(dir.join(FILE), serde_json::to_vec(&prepared)?)?;
    // Declared illumination paths are absent; embedded bases remain source
    // corrupt asset bytes but the modern loader never decodes them.
    for map in &maps {
        let modern = scene_loader::load_map_bundle_dynamic(&dir, &map.id)?;
        assert!(modern.scene.lightmap_bases.is_empty());
        assert_eq!(modern.modern_lights, Some(Vec::new()));
        assert!(
            scene_loader::load_map_bundle(&dir, &map.id).is_err(),
            "compatibility loader continues to require its authored illumination"
        );
    }
    // Metadata drift is detected without ever opening those missing images.
    bundle["scope"] = "changed source inputs".into();
    std::fs::write(dir.join("bundle.json"), serde_json::to_vec(&bundle)?)?;
    assert!(Parameters::read(&dir).is_err());
    let fallback = scene_loader::load_map_bundle_dynamic(&dir, &maps[0].id)?;
    assert!(
        fallback
            .scene
            .omissions
            .iter()
            .any(|s| s.contains("live sun/ambient only"))
    );
    assert_eq!(fallback.modern_lights, Some(Vec::new()));
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn modern_shadows_off_stays_live_and_mode_switches_do_not_leak_residuals() -> Result<()> {
    let (device, queue) = gpu()?;
    let target = color_target(&device, wgpu::TextureFormat::Rgba8Unorm, 128, 128);
    let mut renderer =
        SceneRenderer::with_settings(&device, wgpu::TextureFormat::Rgba8Unorm, 1, None);
    let sun = Vec3::NEG_Y;
    let data = floor(sun, 0.);
    let scene = renderer.upload(&device, &queue, &data)?;
    let mut lighting = lamp_lighting(Vec3::new(0., 12., 0.));
    lighting.lights[0].color = [0.25; 3];
    lighting.lights[0].inner = 20.;
    lighting.lights[0].outer = 40.;
    let mut camera = Camera::perspective([0., 22., 0.1], [0.; 3], 1., 1.4, 0.05, 400.);
    camera.sun_color = [0.; 4];
    camera.ambient = [0.1, 0.1, 0.1, 2.];
    let mut frame = |renderer: &mut SceneRenderer, mode: f32| -> Result<i32> {
        camera.ambient[3] = mode;
        renderer.update_camera(&queue, &camera);
        let pixels = render(&device, &queue, renderer, &target, &[&scene], &[], &[])?;
        Ok(i32::from(pixels[((64 * 128 + 64) * 4) as usize + 1]))
    };
    for _ in 0..2 {
        renderer.set_light_volume(
            &device,
            &queue,
            Some(&bri_render::light_volume::LightVolume {
                origin: [-100.; 3],
                cell: 200.,
                dims: [1; 3],
                texels: vec![[255; 4]],
                rays: 0,
            }),
        )?;
        renderer.set_map_lighting(&device, &queue, Some(&lighting), false)?;
        assert!(
            (frame(&mut renderer, 2.)? - 77).abs() <= 2,
            "Unified keeps its legacy surface"
        );
        renderer.set_dynamic_lights(&device, &queue, &lighting.lights)?;
        let live = frame(&mut renderer, 3.)?;
        assert!(live > 80 && live < 130, "live unshadowed lamp {live}");
        renderer.set_map_light_tints(&queue, &[Vec3::ZERO]);
        assert!(
            (frame(&mut renderer, 3.)? - 26).abs() <= 2,
            "Dynamic retains only explicit ambient, neither map residual nor baked illumination"
        );
        assert!(
            (frame(&mut renderer, 0.)? - 77).abs() <= 2,
            "Classic surface stays legacy"
        );
    }
    Ok(())
}
