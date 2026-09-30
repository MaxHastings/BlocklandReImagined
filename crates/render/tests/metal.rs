//! Bare metal on an offscreen adapter: a mirror-smooth ball reflects the room
//! around it the right way round through the environment probe, and only
//! the sky without one.
use anyhow::Result;
use bri_render::{
    environment_probe::{EnvironmentProbe, PROBE_SIZE},
    reflection::{Mirror, ReflectionSettings, Reflections},
    scene::*,
};
use glam::Vec3;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn vertex(position: Vec3, normal: Vec3, uv: [f32; 2], color: [f32; 4]) -> SceneVertex {
    SceneVertex {
        position: position.to_array(),
        normal: normal.to_array(),
        uv,
        lightmap_uv: [0.0; 2],
        color,
        fx: [0.0; 4],
    }
}

fn batch(data: &mut SceneData, start: u32, material: Material) {
    data.materials.push(material);
    data.batches.push(MeshBatch {
        indices: start..data.indices.len() as u32,
        material: data.materials.len() - 1,
        center: [0.0; 3],
    });
}

/// A quad of one colour; `corners` counterclockwise from its front.
fn quad(data: &mut SceneData, corners: [Vec3; 4], color: [f32; 4], kind: MaterialKind) {
    let first = data.vertices.len() as u32;
    let normal = (corners[1] - corners[0])
        .cross(corners[2] - corners[0])
        .normalize();
    data.vertices
        .extend(corners.map(|p| vertex(p, normal, [0.0; 2], color)));
    let start = data.indices.len() as u32;
    data.indices
        .extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
    let mut material = Material::vertex_lit("quad", 0);
    material.kind = kind;
    batch(data, start, material);
}

/// A box's six faces, facing out, of one colour.
fn cube(data: &mut SceneData, centre: Vec3, half: Vec3, color: [f32; 4]) {
    for axis in 0..3 {
        for sign in [-1.0, 1.0] {
            let n = Vec3::AXES[axis] * sign;
            let (u, v) = (Vec3::AXES[(axis + 1) % 3], Vec3::AXES[(axis + 2) % 3]);
            let (u, v) = if sign > 0.0 { (u, v) } else { (v, u) };
            let c = centre + n * half;
            let corner = |a: f32, b: f32| c + u * half * a + v * half * b;
            quad(
                data,
                [
                    corner(-1.0, -1.0),
                    corner(1.0, -1.0),
                    corner(1.0, 1.0),
                    corner(-1.0, 1.0),
                ],
                color,
                MaterialKind::VertexLit,
            );
        }
    }
}

/// The inside of a box: six walls facing its centre, unlit.
fn room(data: &mut SceneData, half: f32, colors: [[f32; 4]; 6]) {
    // +X, -X, +Y, -Y, +Z, -Z
    for (i, color) in colors.into_iter().enumerate() {
        let axis = i / 2;
        let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
        let n = Vec3::AXES[axis] * sign;
        let (u, v) = (Vec3::AXES[(axis + 1) % 3], Vec3::AXES[(axis + 2) % 3]);
        // Facing the centre: the reverse of a box face's winding.
        let (u, v) = if sign > 0.0 { (v, u) } else { (u, v) };
        let c = n * half;
        let corner = |a: f32, b: f32| c + (u * a + v * b) * half;
        quad(
            data,
            [
                corner(-1.0, -1.0),
                corner(1.0, -1.0),
                corner(1.0, 1.0),
                corner(-1.0, 1.0),
            ],
            color,
            MaterialKind::Unlit,
        );
    }
}

/// A metal ball: `tint` and `detail` are image indices, `parameters` as
/// the client builds them from `bri_content::shape::Metal`.
fn ball(
    data: &mut SceneData,
    centre: Vec3,
    radius: f32,
    tint: usize,
    detail: usize,
    roughness: f32,
    color: [f32; 3],
) {
    let (rings, segments) = (48, 96);
    let first = data.vertices.len() as u32;
    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        let theta = v * std::f32::consts::PI;
        for segment in 0..=segments {
            let u = segment as f32 / segments as f32;
            let phi = u * std::f32::consts::TAU;
            let n = Vec3::new(theta.sin() * phi.cos(), theta.cos(), -theta.sin() * phi.sin());
            data.vertices
                .push(vertex(centre + n * radius, n, [u * 2.0, v], [1.0; 4]));
        }
    }
    let start = data.indices.len() as u32;
    let row = segments + 1;
    for ring in 0..rings {
        for segment in 0..segments {
            let a = first + ring * row + segment;
            let b = a + row;
            data.indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    let mut material = Material::vertex_lit("metal", tint);
    material.kind = MaterialKind::Metal;
    material.images[1] = detail;
    material.parameters = Some([
        [roughness, 3.0, 1.0, 0.0],
        [color[0], color[1], color[2], 0.0],
        [0.0; 4],
        [0.0; 4],
    ]);
    batch(data, start, material);
}

fn image(label: &str, width: u32, height: u32, rgba: Vec<u8>, srgb: bool) -> SceneImage {
    SceneImage {
        label: label.into(),
        width,
        height,
        rgba,
        srgb,
    }
}

/// Flat detail (1), after the scene's own white (0): roughness as given,
/// no grime.
fn plain_images(data: &mut SceneData) {
    data.images
        .push(image("flat", 1, 1, vec![128, 255, 128, 128], false));
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
    /// One `size`² frame of `data` from `camera`, with the probe at `probe`
    /// (drawn out to `reach`) or none.
    fn frame(
        &self,
        data: &SceneData,
        camera: Camera,
        probe: Option<Vec3>,
        reach: f32,
        size: u32,
        samples: u32,
    ) -> Result<Vec<u8>> {
        self.frame_with(data, camera, probe, reach, size, samples, &[])
    }
    /// [`Self::frame`] with `mirrors`, whose surfaces the probe's faces draw
    /// as the player's view does.
    #[allow(clippy::too_many_arguments)]
    fn frame_with(
        &self,
        data: &SceneData,
        mut camera: Camera,
        probe: Option<Vec3>,
        reach: f32,
        size: u32,
        samples: u32,
        mirrors: &[Mirror],
    ) -> Result<Vec<u8>> {
        let device = &self.device;
        let mut renderer = SceneRenderer::with_samples(device, FORMAT, samples);
        let scene = renderer.upload(device, &self.queue, data)?;
        camera.apply_environment(data);
        renderer.update_camera(&self.queue, &camera);
        let mut reflections =
            Reflections::new(device, FORMAT, samples, ReflectionSettings::MEDIUM);
        reflections.prepare(device, &self.queue, &mut renderer, &camera, (size, size), mirrors)?;
        let mut environment = EnvironmentProbe::new(device, &renderer, FORMAT, samples);
        environment.prepare(device, &self.queue, &mut renderer, &camera, probe, reach);
        assert_eq!(environment.faces().len(), if probe.is_some() { 6 } else { 0 });
        for face in environment.face_views() {
            reflections.prepare_view(
                device,
                &self.queue,
                face.view,
                face.view_projection,
                face.eye,
                (PROBE_SIZE, PROBE_SIZE),
            );
        }
        let texture = |samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("metal test"),
                size: wgpu::Extent3d {
                    width: size,
                    height: size,
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
        let depth =
            create_depth_samples(device, size, size, samples).create_view(&Default::default());
        let [r, g, b, a] = data.clear_color.map(f64::from);
        let clear = wgpu::Color { r, g, b, a };
        let mut encoder = device.create_command_encoder(&Default::default());
        reflections.render(&renderer, &mut encoder, &[&scene], &[], clear, &|_, _| {});
        let surfaces = |pass: &mut wgpu::RenderPass<'_>, view: usize| {
            reflections.draw_surfaces(pass, view)
        };
        environment.render(&renderer, &mut encoder, &[&scene], &[], clear, &surfaces, &|_, _| {});
        let own = |pass: &mut wgpu::RenderPass<'_>| reflections.draw_surfaces(pass, 0);
        renderer.render_world(
            &mut encoder,
            WorldPass {
                view: 0,
                color: multisampled.as_ref().unwrap_or(&view),
                resolve: multisampled.as_ref().map(|_| &view),
                depth: &depth,
                viewport: None,
                clear: Some(clear),
                after_opaque: Some(&own),
                after_all: None,
            },
            &[&scene],
            &[],
        );
        let row = (size * 4).div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("metal test readback"),
            size: u64::from(row * size),
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
                    rows_per_image: Some(size),
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
            timeout: Some(std::time::Duration::from_secs(60)),
        })?;
        rx.recv_timeout(std::time::Duration::from_secs(60))??;
        let mapped = readback.slice(..).get_mapped_range()?;
        Ok(mapped
            .chunks_exact(row as usize)
            .flat_map(|r| r[..size as usize * 4].iter().copied())
            .collect())
    }
}

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const GREY: [f32; 4] = [0.25, 0.25, 0.25, 1.0];
const YELLOW: [f32; 4] = [1.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

/// The pixel at (x, y) of a `size`-wide frame.
fn at(pixels: &[u8], size: u32, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * size + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}

/// Which of the room's colours `p` is, if any.
fn named(p: [u8; 3]) -> &'static str {
    let [r, g, b] = p.map(|c| c > 150);
    let dark = p.iter().all(|c| *c < 150);
    match (r, g, b) {
        (true, false, false) => "red",
        (false, true, false) => "green",
        (true, true, true) => "white",
        (true, true, false) => "yellow",
        (false, false, true) => "blue",
        _ if dark => "dark",
        _ => "mixed",
    }
}

#[test]
fn a_smooth_ball_reflects_the_room_the_right_way_round() -> Result<()> {
    // The viewer at +Z looks at a mirror ball in a room: red wall at +X,
    // green at -X, white ceiling, grey floor, yellow wall behind the
    // viewer, blue behind the ball.
    let mut data = SceneData::default();
    plain_images(&mut data);
    room(&mut data, 8.0, [RED, GREEN, WHITE, GREY, YELLOW, BLUE]);
    ball(&mut data, Vec3::ZERO, 1.0, 0, 1, 0.03, [0.97, 0.97, 0.97]);
    data.sun_color = [0.0; 3];
    let gpu = Gpu::new()?;
    let size = 128;
    let camera = Camera::perspective([0.0, 0.0, 3.2], [0.0; 3], 1.0, 0.7, 0.05, 100.0);
    for samples in [1, 4] {
        let pixels = gpu.frame(&data, camera, Some(Vec3::ZERO), 32.0, size, samples)?;
        let c = size / 2;
        // The ball fills about the middle half of the frame.
        let seen = |x: u32, y: u32| named(at(&pixels, size, x, y));
        assert_eq!(seen(c, c), "yellow", "the wall behind the viewer, {samples}x");
        assert_eq!(seen(c + 32, c), "red", "the +X wall on the right, {samples}x");
        assert_eq!(seen(c - 32, c), "green", "the -X wall on the left, {samples}x");
        assert_eq!(seen(c, c - 32), "white", "the ceiling above, {samples}x");
        assert_eq!(seen(c, c + 32), "dark", "the floor below, {samples}x");
    }
    // No probe: only the sky built from the fog and ambient light, never
    // the room.
    let pixels = gpu.frame(&data, camera, None, 32.0, size, 1)?;
    for (x, y) in [(size / 2 + 32, size / 2), (size / 2 - 32, size / 2)] {
        let p = at(&pixels, size, x, y);
        assert!(!matches!(named(p), "red" | "green"), "{p:?}");
    }
    Ok(())
}

/// A small city block around the Steel Kit's ball, under a sunny sky, as a
/// picture: set BRI_METAL_SHOT to a .png path to save it.
#[test]
fn a_steel_ball_among_bricks() -> Result<()> {
    let mut data = SceneData::default();
    let textures = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/showcase/steel-ball-kit/assets/textures/"
    );
    let load = |name: &str, srgb: bool| -> Result<SceneImage> {
        let img = image::open(format!("{textures}{name}"))?.to_rgba8();
        Ok(image(name, img.width(), img.height(), img.into_raw(), srgb))
    };
    data.images.push(load("steel.png", true)?);
    data.images.push(load("steel-detail.png", false)?);
    // Sky: a big box around it all, pale at the horizon, deeper above.
    let sky = [0.55, 0.72, 0.95, 1.0];
    room(&mut data, 200.0, [sky, sky, [0.3, 0.5, 0.9, 1.0], GREY, sky, sky]);
    // A baseplate of grey studs-less plates and a few coloured builds.
    quad(
        &mut data,
        [
            Vec3::new(-40.0, 0.0, 40.0),
            Vec3::new(40.0, 0.0, 40.0),
            Vec3::new(40.0, 0.0, -40.0),
            Vec3::new(-40.0, 0.0, -40.0),
        ],
        [0.35, 0.55, 0.3, 1.0],
        MaterialKind::VertexLit,
    );
    let bricks = [
        (Vec3::new(-6.0, 2.0, -4.0), Vec3::new(2.0, 2.0, 1.0), [0.8, 0.15, 0.1, 1.0]),
        (Vec3::new(5.0, 3.0, -6.0), Vec3::new(1.5, 3.0, 1.5), [0.95, 0.8, 0.1, 1.0]),
        (Vec3::new(7.0, 1.0, 3.0), Vec3::new(1.0, 1.0, 3.0), [0.1, 0.35, 0.85, 1.0]),
        (Vec3::new(-5.0, 0.6, 5.0), Vec3::new(2.0, 0.6, 2.0), [0.95, 0.95, 0.95, 1.0]),
        (Vec3::new(0.0, 4.0, -12.0), Vec3::new(6.0, 4.0, 0.5), [0.6, 0.35, 0.2, 1.0]),
        (Vec3::new(-3.0, 0.2, 0.0), Vec3::new(0.5, 0.2, 0.5), [0.2, 0.7, 0.2, 1.0]),
    ];
    for (centre, half, color) in bricks {
        cube(&mut data, centre, half, color);
    }
    let centre = Vec3::new(0.0, 1.5, 0.0);
    ball(&mut data, centre, 1.5, 1, 2, 0.16, [0.62, 0.63, 0.65]);
    data.sun_direction = [-0.4, -0.8, -0.45];
    data.sun_color = [0.85, 0.82, 0.75];
    data.ambient = [0.45, 0.47, 0.5];
    data.fog.color = [0.7, 0.8, 0.95];
    data.clear_color = [0.55, 0.72, 0.95, 1.0];
    let gpu = Gpu::new()?;
    let size = 512;
    let camera = Camera::perspective([2.5, 3.0, 6.0], centre.to_array(), 1.0, 0.75, 0.05, 400.0);
    let pixels = gpu.frame(&data, camera, Some(centre), 64.0, size, 4)?;
    // The ball's middle shows neither the flat sky fallback nor black.
    let c = at(&pixels, size, size / 2, size / 2);
    assert!(c.iter().any(|v| *v > 20), "{c:?}");
    if let Ok(path) = std::env::var("BRI_METAL_SHOT") {
        image::RgbaImage::from_raw(size, size, pixels)
            .expect("frame size")
            .save(path)?;
    }
    Ok(())
}

#[test]
fn a_mirror_behind_the_viewer_shows_in_the_ball() -> Result<()> {
    // The yellow wall behind the viewer, which the ball's middle shows, is
    // covered by a mirror facing the ball: the ball shows the mirror (its
    // silver, nothing drawn it live from there), not the wall under it.
    let mut data = SceneData::default();
    plain_images(&mut data);
    room(&mut data, 8.0, [RED, GREEN, WHITE, GREY, YELLOW, BLUE]);
    ball(&mut data, Vec3::ZERO, 1.0, 0, 1, 0.03, [0.97, 0.97, 0.97]);
    data.sun_color = [0.0; 3];
    let z = 7.9;
    let mirror = Mirror {
        corners: [
            Vec3::new(3.0, -3.0, z),
            Vec3::new(-3.0, -3.0, z),
            Vec3::new(-3.0, 3.0, z),
            Vec3::new(3.0, 3.0, z),
        ],
        tint: [1.0; 3],
        strength: 1.0,
        looks: bri_render::reflection::Looks::Reflect,
        fallback: bri_render::reflection::SILVER,
        recess: 0.0,
    };
    let gpu = Gpu::new()?;
    let size = 128;
    let camera = Camera::perspective([0.0, 0.0, 3.2], [0.0; 3], 1.0, 0.7, 0.05, 100.0);
    let pixels = gpu.frame_with(&data, camera, Some(Vec3::ZERO), 32.0, size, 1, &[mirror])?;
    let centre = at(&pixels, size, size / 2, size / 2);
    assert_ne!(named(centre), "yellow", "{centre:?}");
    let [r, g, b] = centre.map(i32::from);
    assert!(r > 60 && (r - b).abs() < 40 && (g - b).abs() < 40, "silver: {centre:?}");
    Ok(())
}
