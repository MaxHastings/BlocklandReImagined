//! Original casing DTS uploaded and rendered through the shared scene/instance path.
use anyhow::{Context, Result, ensure};
use bri_client::weapon_debris::{WeaponDebris, WeaponDebrisAssets};
use bri_render::scene::{Camera, GpuInstances, SceneRenderer, SceneTransform, create_depth};
use bri_ui::gpu::Headless;
use glam::{Mat4, Vec3};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
async fn pixels(
    gpu: &Headless,
    renderer: &mut SceneRenderer,
    scene: &bri_render::scene::GpuScene,
    instances: &GpuInstances,
    camera: &Camera,
) -> Result<Vec<u8>> {
    let extent = wgpu::Extent3d {
        width: 256,
        height: 256,
        depth_or_array_layers: 1,
    };
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("weapon shell offscreen"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = create_depth(&gpu.device, 256, 256);
    renderer.update_camera(&gpu.queue, camera);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.render_with_instances(
        &mut encoder,
        &target.create_view(&Default::default()),
        &depth.create_view(&Default::default()),
        &[],
        &[(scene, instances)],
        Some(wgpu::Color::BLACK),
    );
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("weapon shell readback"),
        size: 256 * 256 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1024),
                rows_per_image: Some(256),
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
        timeout: Some(std::time::Duration::from_secs(30)),
    })?;
    rx.recv_timeout(std::time::Duration::from_secs(30))??;
    Ok(buffer.slice(..).get_mapped_range()?.to_vec())
}

#[test]
#[ignore = "requires ignored source-converted weapon debris pack and an offscreen GPU adapter"]
fn source_shell_uses_persistent_shared_scene_and_instanced_gpu_draw() -> Result<()> {
    pollster::block_on(async {
        let assets = WeaponDebrisAssets::load(&root().join("content/weapon-debris-pack-001"))?;
        let mut debris = WeaponDebris::new(assets, Default::default())?;
        let cue = bri_sim::presentation::Cue {
            id: 1,
            tick: 1,
            kind: bri_sim::presentation::CueKind::WeaponShell {
                actor: 9,
                image: "v20.image.gunimage".into(),
                hand: 0,
            },
            position: [0.; 3],
        };
        debris.cues(&[cue], |_, _, _| Some(Mat4::IDENTITY), |_| Vec3::ZERO)?;
        let gpu = Headless::new().context("offscreen weapon casing adapter")?;
        let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
        let scene = renderer.upload(&gpu.device, &gpu.queue, &debris.assets().shell_scene)?;
        let transforms: Vec<_> = debris
            .instances()
            .map(|i| SceneTransform {
                transform: i.transform,
                tint: i.tint,
            })
            .collect();
        let mut instances = GpuInstances::new(&gpu.device, 1)?;
        instances.update(&gpu.queue, &transforms)?;
        let camera = Camera::perspective(
            [0., 0.04, 0.4],
            [0., 0., 0.],
            1.,
            45f32.to_radians(),
            0.001,
            5.,
        );
        let image = pixels(&gpu, &mut renderer, &scene, &instances, &camera).await?;
        let visible = image
            .chunks_exact(4)
            .filter(|p| p[..3].iter().any(|c| *c > 8))
            .count();
        ensure!(
            visible > 12,
            "original gunShell DTS produced only {visible} foreground pixels"
        );
        let out = root().join("artifacts/native-weapon-debris");
        std::fs::create_dir_all(&out)?;
        image::save_buffer(
            out.join("gun-shell-offscreen.png"),
            &image,
            256,
            256,
            image::ColorType::Rgba8,
        )?;
        Ok(())
    })
}
