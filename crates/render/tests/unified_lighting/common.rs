//! Shared offscreen fixtures for the lighting integration binaries.
//! Each binary uses a subset of these builders and readback paths.
#![allow(dead_code)]

use anyhow::Result;
use bri_render::scene::*;
use glam::Vec3;

/// Axis-aligned box with outward normals, one white vertex-lit material.
pub(crate) fn cuboid(min: Vec3, max: Vec3) -> SceneData {
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

pub(crate) fn gpu() -> Result<(wgpu::Device, wgpu::Queue)> {
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
pub(crate) fn render(
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
pub(crate) fn render_with_map(
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

pub(crate) fn color_target(
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
pub(crate) fn floor(sun: Vec3, sun_color: f32) -> SceneData {
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

/// One map light over a brick floor, no sun: what the light lights and the
/// visibility volume that lets it through everywhere.
pub(crate) fn lamp_lighting(at: Vec3) -> bri_render::map_lighting::MapLighting {
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

/// `floor` equipped for the Dynamic mode as the client does it: its
/// leftover lightmap `left` and, per light in `lights`, the share of it each
/// texel receives (`seen(light, column)`, one row like the next).
pub(crate) fn dynamic_floor(
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
