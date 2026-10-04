//! A posed model can hide every object: the spear's `fire` sequence keys
//! both spear objects invisible while it is thrown. a16 panicked with
//! "buffer slice can not be empty" when that empty scene was drawn.
use anyhow::Result;
use bri_render::{scene::*, shadow::ShadowSettings};
use glam::Mat4;

#[test]
fn an_empty_posed_scene_draws_nothing_instead_of_panicking() -> Result<()> {
    let (device, queue) = pollster::block_on(async {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await?;
        anyhow::Ok(adapter.request_device(&Default::default()).await?)
    })?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let mut camera = Camera::perspective([4., 3., 4.], [0.; 3], 1.0, 1.0, 0.05, 100.0);
    camera.sun_direction = [0.3, -1.0, 0.0, 0.0];
    renderer.update_camera(&queue, &camera);
    let mut hidden = SceneData::default();
    hidden.materials.push(Material::vertex_lit("hidden", 0));
    let base = renderer.upload(&device, &queue, &hidden)?;
    // World items upload each posed frame with the base model's materials.
    let empty = renderer.upload_geometry_shared(&device, &hidden, &base)?;
    let mut instances = GpuInstances::new(&device, 4)?;
    instances.update(
        &queue,
        &[SceneTransform {
            transform: Mat4::IDENTITY,
            tint: [1.0; 4],
        }],
    )?;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("empty scene target"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let depth = create_depth(&device, 64, 64).create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.render_shadows(
        &mut encoder,
        ShadowCasters {
            scenes: &[&empty],
            instances: &[(&empty, &instances)],
        },
        ShadowCasters {
            scenes: &[&empty],
            instances: &[(&empty, &instances)],
        },
    );
    renderer.render_with_instances(
        &mut encoder,
        &view,
        &depth,
        &[&empty],
        &[(&empty, &instances)],
        Some(wgpu::Color::BLACK),
    );
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    Ok(())
}
