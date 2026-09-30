//! Unified lighting (`$pref::Video::Lighting` 1 and 2): a live shadow takes
//! away only the sun a lightmap texel actually had, so baked shade is never
//! darkened twice, while Classic keeps v20's fixed darkening.
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
        images: vec![SceneImage::white(), image("mission", mission), image("parts", parts)],
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
        let pixels = render(&device, &queue, &mut renderer, &target, &[&map, &slab], &[&slab], &[])?;
        Ok(points.map(|p| at(&pixels, p)))
    };
    let [lit_under, lit_open, dark_under, dark_open] = run(1.0)?;
    // Unified: baked shade stays as baked; baked sun under the slab goes
    // down to the static light, the same as the baked shade.
    assert!((dark_under - dark_open).abs() <= 2, "{dark_under} {dark_open}");
    assert!(lit_open > dark_open + 60, "{lit_open} {dark_open}");
    assert!((lit_under - dark_open).abs() <= 3, "{lit_under} {dark_open}");
    // Unified with highlights draws the same diffuse light here (a matte
    // floor seen from above away from the sun's reflection).
    let shine = run(2.0)?;
    assert!(shine[2] >= dark_under && shine[3] >= dark_open);
    // Classic keeps v20's fixed share: it darkens the baked shade again.
    let [_, _, classic_under, classic_open] = run(0.0)?;
    assert_eq!(classic_open, dark_open);
    assert!(classic_under < classic_open - 10, "{classic_under} {classic_open}");
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
    let mut run = |settings: ShadowSettings, mode: f32| -> Result<[i32; 2]> {
        let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(settings));
        renderer.set_map_lighting(&device, &queue, Some(&lamp_lighting(lamp)))?;
        let floor = renderer.upload(&device, &queue, &floor)?;
        let slab = renderer.upload(&device, &queue, &slab)?;
        camera.ambient[3] = mode;
        renderer.update_camera(&queue, &camera);
        let pixels = render(&device, &queue, &mut renderer, &target, &[&floor, &slab], &[&slab], &[])?;
        Ok([at(&pixels, shade), at(&pixels, open)])
    };
    // Unified at Best: the slab shades the floor the lamp lights.
    let [shaded, open_floor] = run(ShadowSettings::BEST, 1.0)?;
    assert!(open_floor > 60, "{open_floor}");
    assert!(shaded < open_floor - 40, "{shaded} {open_floor}");
    // Medium casts from one lamp too.
    let [medium, _] = run(ShadowSettings::MEDIUM, 1.0)?;
    assert!(medium < open_floor - 40, "{medium} {open_floor}");
    // Low quality has no lamp shadows; the floor there, nearer the lamp, is
    // lit at least as brightly as the open floor.
    let [low, low_open] = run(ShadowSettings::LOW, 1.0)?;
    assert!(low >= low_open, "{low} {low_open}");
    // Classic never draws map lights or lamp shadows.
    let [classic, classic_open] = run(ShadowSettings::BEST, 0.0)?;
    assert!((classic - classic_open).abs() <= 2, "{classic} {classic_open}");
    Ok(())
}
