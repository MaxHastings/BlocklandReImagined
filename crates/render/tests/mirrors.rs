//! Planar mirrors drawn end to end on an offscreen adapter: what a mirror
//! shows, what it hides, and its silver when reflections are off.
use anyhow::Result;
use bri_render::{
    reflection::{Mirror, ReflectionSettings, Reflections},
    scene::*,
};
use glam::Vec3;

const SIZE: u32 = 128;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// An unlit quad of one colour; `corners` counterclockwise from its front.
fn quad(data: &mut SceneData, corners: [[f32; 3]; 4], color: [f32; 4], double_sided: bool) {
    let first = data.vertices.len() as u32;
    data.vertices.extend(corners.map(|position| SceneVertex {
        position,
        normal: [0.0, 0.0, 1.0],
        uv: [0.0; 2],
        lightmap_uv: [0.0; 2],
        color,
        fx: [0.0; 4],
    }));
    let start = data.indices.len() as u32;
    data.indices
        .extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
    let mut material = Material::surface("quad", 0, 0);
    material.kind = MaterialKind::Unlit;
    material.double_sided = double_sided;
    data.materials.push(material);
    data.batches.push(MeshBatch {
        indices: start..data.indices.len() as u32,
        material: data.materials.len() - 1,
        center: corners[0],
    });
}

/// The viewer at z = 4 looks at a mirror in the plane z = 0. A red card
/// at z = 2 turns its face to the mirror (the viewer sees only its culled
/// back); a two-sided green card hides behind the mirror.
fn room() -> SceneData {
    let mut data = SceneData::default();
    let (x, y) = (0.5, 0.2);
    quad(
        &mut data,
        [[x + 0.3, y - 0.3, 2.0], [x - 0.3, y - 0.3, 2.0], [x - 0.3, y + 0.3, 2.0], [x + 0.3, y + 0.3, 2.0]],
        [1.0, 0.0, 0.0, 1.0],
        false,
    );
    quad(
        &mut data,
        [[-0.8, -0.3, -1.0], [-0.2, -0.3, -1.0], [-0.2, 0.3, -1.0], [-0.8, 0.3, -1.0]],
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    data
}
fn mirror() -> Mirror {
    Mirror {
        corners: [
            Vec3::new(-1.5, -1.5, 0.0),
            Vec3::new(1.5, -1.5, 0.0),
            Vec3::new(1.5, 1.5, 0.0),
            Vec3::new(-1.5, 1.5, 0.0),
        ],
        tint: [1.0; 3],
        strength: 1.0,
    }
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}
impl Gpu {
    fn new() -> Result<Self> {
        pollster::block_on(async {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await?;
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await?;
            Ok(Self { device, queue })
        })
    }
    /// One frame of `room` behind `mirror` at `samples` per pixel.
    fn frame(&self, samples: u32, settings: ReflectionSettings) -> Result<(Vec<u8>, RenderStats)> {
        let device = &self.device;
        let mut renderer = SceneRenderer::with_samples(device, FORMAT, samples);
        let scene = renderer.upload(device, &self.queue, &room())?;
        let camera = Camera::perspective([0.0, 0.0, 4.0], [0.0; 3], 1.0, 1.0, 0.05, 100.0);
        renderer.update_camera(&self.queue, &camera);
        let mut reflections = Reflections::new(device, FORMAT, samples, settings);
        reflections.prepare(
            device,
            &self.queue,
            &mut renderer,
            &camera,
            (SIZE, SIZE),
            &[mirror()],
        )?;
        let texture = |samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("mirror test"),
                size: wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage,
                view_formats: &[],
            })
        };
        let target = texture(
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let multisampled = (samples > 1)
            .then(|| texture(samples, wgpu::TextureUsages::RENDER_ATTACHMENT))
            .map(|t| t.create_view(&Default::default()));
        let view = target.create_view(&Default::default());
        let depth = create_depth_samples(device, SIZE, SIZE, samples).create_view(&Default::default());
        let clear = wgpu::Color {
            r: 0.0,
            g: 0.0,
            b: 0.3,
            a: 1.0,
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        reflections.render(&renderer, &mut encoder, &[&scene], &[], clear);
        let surfaces = |pass: &mut wgpu::RenderPass<'_>| reflections.draw_surfaces(pass, 0);
        renderer.render_world(
            &mut encoder,
            WorldPass {
                view: 0,
                color: multisampled.as_ref().unwrap_or(&view),
                resolve: multisampled.as_ref().map(|_| &view),
                depth: &depth,
                viewport: None,
                clear: Some(clear),
                after_opaque: Some(&surfaces),
            },
            &[&scene],
            &[],
        );
        let row = (SIZE * 4).div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mirror test readback"),
            size: u64::from(row * SIZE),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(SIZE),
                },
            },
            target.size(),
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })?;
        rx.recv_timeout(std::time::Duration::from_secs(30))??;
        let mapped = readback.slice(..).get_mapped_range()?;
        let pixels = mapped
            .chunks_exact(row as usize)
            .flat_map(|r| r[..SIZE as usize * 4].iter().copied())
            .collect();
        Ok((pixels, renderer.stats()))
    }
}

/// Pixels in each half of the screen that are mostly `channel`.
fn halves(pixels: &[u8], channel: usize) -> [usize; 2] {
    let mut out = [0; 2];
    for (i, p) in pixels.chunks_exact(4).enumerate() {
        let others = (0..3).filter(|c| *c != channel).map(|c| p[c]).max().unwrap();
        if p[channel] > 150 && others < 60 {
            out[usize::from(i as u32 % SIZE >= SIZE / 2)] += 1;
        }
    }
    out
}

#[test]
fn a_mirror_shows_what_faces_it_on_the_same_side_and_hides_what_is_behind() -> Result<()> {
    let gpu = Gpu::new()?;
    for samples in [1, 4] {
        let (pixels, stats) = gpu.frame(samples, ReflectionSettings::MEDIUM)?;
        assert_eq!(stats.reflection_passes, 1);
        // The red card's face, only in the mirror, on the side it stands.
        let [left, right] = halves(&pixels, 0);
        assert!(left == 0 && right > 50, "red {left} {right}, {samples}x");
        // The green card behind the mirror: hidden, and not reflected.
        assert_eq!(halves(&pixels, 1), [0, 0], "{samples}x");
    }
    Ok(())
}

#[test]
fn with_reflections_off_a_mirror_is_plain_silver() -> Result<()> {
    let gpu = Gpu::new()?;
    let (pixels, stats) = gpu.frame(1, ReflectionSettings::OFF)?;
    assert_eq!(stats.reflection_passes, 0);
    assert_eq!(halves(&pixels, 0), [0, 0]);
    assert_eq!(halves(&pixels, 1), [0, 0]);
    // The centre is grey silver, not the blue clear colour.
    let centre = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
    let [r, g, b] = [pixels[centre], pixels[centre + 1], pixels[centre + 2]];
    assert!(r > 100 && r.abs_diff(b) < 30 && g.abs_diff(b) < 30, "{r} {g} {b}");
    Ok(())
}
