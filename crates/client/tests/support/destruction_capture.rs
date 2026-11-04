//! Bounded offscreen captures for the explicit private-save rocket repro.
use anyhow::Result;
use bri_client::{
    brick_debris::{BrickDebris, DebrisModels},
    content::ClientContent,
    materials::BrickMaterials,
    world_chunks::{BrickPalette, ChunkedWorld},
};
use bri_render::scene::{Camera, GpuScene, SceneRenderer, create_depth};
use bri_ui::gpu::Headless;
use glam::Vec3;
use std::{collections::BTreeMap, path::Path, sync::Arc};
pub struct Capture {
    gpu: Headless,
    renderer: SceneRenderer,
    pub models: DebrisModels,
    meshes: BTreeMap<String, bri_content::brick::Brick>,
    materials: Arc<BrickMaterials>,
    palette: BrickPalette,
    gpu_palette: GpuScene,
}
impl Capture {
    pub fn new(
        content: &ClientContent,
        meshes: BTreeMap<String, bri_content::brick::Brick>,
    ) -> Result<Self> {
        let gpu = Headless::new()?;
        let renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
        let materials = content.paths.materials()?;
        let palette = BrickPalette::new(&materials)?;
        let gpu_palette = renderer.upload(&gpu.device, &gpu.queue, &palette.scene)?;
        Ok(Self {
            gpu,
            renderer,
            models: Default::default(),
            meshes,
            materials,
            palette,
            gpu_palette,
        })
    }
    pub fn upload(&mut self, debris: &BrickDebris, colors: &[[f32; 4]]) -> Result<()> {
        self.models.upload(
            debris,
            &self.renderer,
            &self.gpu.device,
            &self.gpu.queue,
            &self.meshes,
            &self.palette,
            &self.gpu_palette,
            &self.materials,
            colors,
        )
    }
    pub fn save(
        &mut self,
        world: &bri_net::protocol::PublicWorld,
        eye: [f32; 3],
        yaw: f32,
        pitch: f32,
        path: &Path,
    ) -> Result<()> {
        let mut chunked = ChunkedWorld::default();
        let scenes = chunked.update(
            Arc::new(world.clone()),
            None,
            &self.meshes,
            &self.palette,
            Some(&self.materials),
            8_000_000,
        )?;
        let chunks: Vec<_> = scenes
            .into_iter()
            .filter_map(|(_, built)| built.map(|b| b.scene))
            .collect();
        self.renderer
            .reserve_chunks(&chunks.iter().collect::<Vec<_>>())?;
        let uploaded = chunks
            .iter()
            .map(|c| {
                self.renderer
                    .upload_chunk(&self.gpu.device, &self.gpu.queue, c, &self.gpu_palette)
            })
            .collect::<Result<Vec<_>>>()?;
        let direction = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        );
        let camera = Camera::perspective(
            eye,
            (Vec3::from(eye) + direction).to_array(),
            1.,
            75f32.to_radians(),
            0.05,
            3000.,
        );
        let pixels = pollster::block_on(pixels(
            &self.gpu,
            &mut self.renderer,
            &uploaded.iter().collect::<Vec<_>>(),
            &self.models.draws(),
            &camera,
        ))?;
        image::save_buffer(path, &pixels, 256, 256, image::ColorType::Rgba8)?;
        Ok(())
    }
}
async fn pixels(
    gpu: &Headless,
    renderer: &mut SceneRenderer,
    scenes: &[&GpuScene],
    instances: &[(&GpuScene, &bri_render::scene::GpuInstances)],
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
        scenes,
        instances,
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
