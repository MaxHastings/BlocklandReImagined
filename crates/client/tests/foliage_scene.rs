//! Native map plus the actual client foliage adapter, offscreen only.
use anyhow::{Result, ensure};
use bri_client::{
    building::Building,
    foliage::{ClientFoliage, PreparedFoliage},
    platform::RenderContext,
};
use bri_render::{
    scene::{Camera, SceneRenderer, create_depth},
    scene_loader::load_map_bundle,
    terrain_scene::GpuTerrain,
};
use bri_ui::gpu::{Headless, UiRenderer};
use glam::{Mat4, Vec3};
use std::{collections::BTreeMap, path::Path, time::Duration};

#[test]
#[ignore = "native Bedroom, foliage pack and offscreen GPU; no window or input"]
fn original_grass_composes_over_actual_bedroom_terrain() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = workspace.join("content");
    let artifact = workspace.join("artifacts/native-client-foliage");
    std::fs::create_dir_all(&artifact)?;
    let map = "v20/add-ons/map_bedroom/bedroom.mis";
    let native = bri_sim::map::NativeMap::load(&root.join("map-bundle-017"), map)?;
    let mut building = Building::new(
        bri_sim::definitions::Definitions {
            entries: BTreeMap::new(),
        },
        native.colliders,
    )?;
    building.attach_terrain(native.terrain);
    let prepared = PreparedFoliage::load(
        &root.join("foliage-pack-003"),
        map,
        &building,
        &native.waters,
    )?;
    let target_point = prepared.fields[0].plants()[0].position + Vec3::Y;
    let eye = target_point + Vec3::new(0., 3., 10.);
    let placement = prepared.placement.clone();
    let load_ms = prepared.elapsed_ms;
    let mut foliage = ClientFoliage::load(&root.join("foliage-pack-003"))?;
    foliage.set_map(prepared);
    let map_scene = load_map_bundle(&root.join("map-bundle-017"), map)?;
    let scene = map_scene.scene;
    let gpu = Headless::new()?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let size = (768, 512);
    let extent = wgpu::Extent3d {
        width: size.0,
        height: size.1,
        depth_or_array_layers: 1,
    };
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("native Bedroom foliage compositor"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let depth = create_depth(&gpu.device, size.0, size.1).create_view(&Default::default());
    let mut renderer = SceneRenderer::new(&gpu.device, format);
    let uploaded = renderer.upload(&gpu.device, &gpu.queue, &scene)?;
    let mut terrain = map_scene
        .terrain
        .into_iter()
        .map(|t| GpuTerrain::upload(&renderer, &gpu.device, &gpu.queue, t.into(), 4000.))
        .collect::<Result<Vec<_>>>()?;
    for t in &mut terrain {
        t.update(&gpu.device, &gpu.queue, &[eye], 4000.)?;
    }
    let terrain_draws: Vec<_> = terrain.iter().flat_map(GpuTerrain::draws).collect();
    let mut camera = Camera::perspective(
        eye.to_array(),
        target_point.to_array(),
        1.5,
        65_f32.to_radians(),
        0.05,
        4000.,
    );
    camera.apply_environment(&scene);
    renderer.update_camera(&gpu.queue, &camera);
    let forward = (target_point - eye).normalize();
    let mut ui = UiRenderer::new(&gpu.device, &gpu.queue);
    let mut frames = vec![];
    for show in [false, true] {
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        renderer.render_with_instances(
            &mut encoder,
            &view,
            &depth,
            &[&uploaded],
            &terrain_draws,
            Some(wgpu::Color::BLACK),
        );
        if show {
            foliage.prepare(
                &RenderContext {
                    device: &gpu.device,
                    queue: &gpu.queue,
                    encoder: &mut encoder,
                    target: &view,
                    format,
                    size,
                    ui_renderer: &mut ui,
                },
                &bri_foliage::Camera {
                    position: eye,
                    right: forward.cross(Vec3::Y).normalize(),
                    view_projection: Mat4::from_cols_array(&camera.view_projection),
                    visible_distance: camera.atmosphere[1].max(1.),
                },
                camera.atmosphere[0],
                camera.atmosphere[1].max(camera.atmosphere[0] + 0.001),
            )?;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("client foliage over original map"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            foliage.render(&mut pass);
        }
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("foliage evidence readback"),
            size: u64::from(size.0 * size.1 * 4),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size.0 * 4),
                    rows_per_image: Some(size.1),
                },
            },
            extent,
        );
        gpu.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        gpu.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })?;
        rx.recv_timeout(Duration::from_secs(5))??;
        let pixels = buffer.slice(..).get_mapped_range()?.to_vec();
        buffer.unmap();
        frames.push(pixels);
    }
    let changed = frames[0]
        .chunks_exact(4)
        .zip(frames[1].chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    ensure!(
        changed > 100,
        "No substantial foliage in native map compositor: {changed}"
    );
    image::save_buffer(
        artifact.join("bedroom-grass.png"),
        &frames[1],
        size.0,
        size.1,
        image::ColorType::Rgba8,
    )?;
    std::fs::write(
        artifact.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "placement":placement,"load_ms":load_ms,"render":foliage.stats,"changed_pixels":changed,
            "camera":eye.to_array(),"target":target_point.to_array(),"adapter":gpu.adapter_info.name,
            "map":map,"visible_window":false,"desktop_input":false,"subjective_acceptance":false,
        }))?,
    )?;
    Ok(())
}
