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
    data.indices.extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
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

/// The viewer at z = 4 looks at a mirror in the plane z = 0. An orange card
/// at z = 2 turns its face to the mirror (the viewer sees only its culled
/// back); a two-sided green card hides behind the mirror.
fn room() -> SceneData {
    let mut data = SceneData::default();
    let (x, y) = (0.5, 0.2);
    quad(
        &mut data,
        [
            [x + 0.3, y - 0.3, 2.0],
            [x - 0.3, y - 0.3, 2.0],
            [x - 0.3, y + 0.3, 2.0],
            [x + 0.3, y + 0.3, 2.0],
        ],
        [0.8, 0.4, 0.2, 1.0],
        false,
    );
    quad(
        &mut data,
        [
            [-0.8, -0.3, -1.0],
            [-0.2, -0.3, -1.0],
            [-0.2, 0.3, -1.0],
            [-0.8, 0.3, -1.0],
        ],
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
        looks: bri_render::reflection::Looks::Reflect,
        fallback: bri_render::reflection::SILVER,
        recess: 0.0,
    }
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}
/// The device every test here draws on, one test at a time. The CI machine
/// has no GPU and draws on a software adapter whose every frame keeps all
/// its cores busy: tests drawing side by side, each on its own device,
/// starved one another until their frames missed the wait below, and which
/// tests failed changed from run to run. Taking turns on one device gives
/// each frame the whole machine, as one frame of the game has.
static GPU: std::sync::Mutex<Option<Gpu>> = std::sync::Mutex::new(None);
/// One test's turn on the shared device; the next test waits for it.
struct Turn(std::sync::MutexGuard<'static, Option<Gpu>>);
impl std::ops::Deref for Turn {
    type Target = Gpu;
    fn deref(&self) -> &Gpu {
        self.0
            .as_ref()
            .expect("the device is made before a turn starts")
    }
}
impl Gpu {
    /// Wait for this test's turn on the shared device, making it first.
    fn turn() -> Result<Turn> {
        // A test that failed on its turn leaves the device as good as ever.
        let mut gpu = GPU
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if gpu.is_none() {
            *gpu = Some(Self::new()?);
        }
        Ok(Turn(gpu))
    }
    fn new() -> Result<Self> {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            );
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
        self.frame_of(samples, settings, &room(), &[mirror()], 1)
    }
    fn frame_of(
        &self,
        samples: u32,
        settings: ReflectionSettings,
        data: &SceneData,
        mirrors: &[Mirror],
        frames: usize,
    ) -> Result<(Vec<u8>, RenderStats)> {
        self.frame_from([0.0, 0.0, 4.0], samples, settings, data, mirrors, frames)
    }
    /// `frames` frames of `data` and `mirrors` from `eye`, looking at the
    /// origin; the last one's pixels, and the stats of them all.
    fn frame_from(
        &self,
        eye: [f32; 3],
        samples: u32,
        settings: ReflectionSettings,
        data: &SceneData,
        mirrors: &[Mirror],
        frames: usize,
    ) -> Result<(Vec<u8>, RenderStats)> {
        let camera = Camera::perspective(eye, [0.0; 3], 1.0, 1.0, 0.05, 100.0);
        self.frame_with(&camera, samples, settings, data, mirrors, frames)
    }
    /// [`Self::frame_from`] through any camera.
    fn frame_with(
        &self,
        camera: &Camera,
        samples: u32,
        settings: ReflectionSettings,
        data: &SceneData,
        mirrors: &[Mirror],
        frames: usize,
    ) -> Result<(Vec<u8>, RenderStats)> {
        self.frame_with_model_visibility(camera, samples, settings, data, mirrors, frames, true)
    }
    #[allow(clippy::too_many_arguments)]
    fn frame_with_model_visibility(
        &self,
        camera: &Camera,
        samples: u32,
        settings: ReflectionSettings,
        data: &SceneData,
        mirrors: &[Mirror],
        frames: usize,
        visible: bool,
    ) -> Result<(Vec<u8>, RenderStats)> {
        let device = &self.device;
        let mut renderer = SceneRenderer::with_samples(device, FORMAT, samples);
        let scene = renderer.upload(device, &self.queue, data)?;
        let mut instance = GpuInstances::new(device, 1)?;
        instance.update(&self.queue, &[SceneTransform::default()])?;
        let models = [(&scene, &instance)];
        let camera = *camera;
        renderer.update_camera(&self.queue, &camera);
        let mut reflections = Reflections::new(device, FORMAT, samples, settings);
        reflections.prepare(
            device,
            &self.queue,
            &mut renderer,
            &camera,
            (SIZE, SIZE),
            mirrors,
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
        let depth =
            create_depth_samples(device, SIZE, SIZE, samples).create_view(&Default::default());
        let clear = wgpu::Color {
            r: 0.0,
            g: 0.0,
            b: 0.3,
            a: 1.0,
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        // The same frame again: a later frame's echoes show an earlier one's,
        // and it keeps the earlier pictures (`Shows::Last`).
        for frame in 0..frames {
            if frame > 0 {
                reflections.prepare(
                    device,
                    &self.queue,
                    &mut renderer,
                    &camera,
                    (SIZE, SIZE),
                    mirrors,
                )?;
            }
            reflections.render_views(
                &renderer,
                &mut encoder,
                &[],
                &|view| {
                    assert!(view > 0);
                    if visible { &models } else { &[] }
                },
                clear,
                &|_, _| {},
            );
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
                    after_all: None,
                },
                &[&scene],
                &[],
            );
        }
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
        let others = (0..3)
            .filter(|c| *c != channel)
            .map(|c| p[c])
            .max()
            .unwrap();
        if p[channel] > 150 && others < 110 {
            out[usize::from(i as u32 % SIZE >= SIZE / 2)] += 1;
        }
    }
    out
}

#[test]
fn each_virtual_view_can_omit_a_first_person_body_without_changing_other_views() -> Result<()> {
    let gpu = Gpu::turn()?;
    let camera = Camera::perspective([0.0, 0.0, 4.0], [0.0; 3], 1.0, 1.0, 0.05, 100.0);
    for samples in [1, 4] {
        let (shown, _) = gpu.frame_with_model_visibility(
            &camera,
            samples,
            ReflectionSettings::MEDIUM,
            &room(),
            &[mirror()],
            1,
            true,
        )?;
        let (hidden, _) = gpu.frame_with_model_visibility(
            &camera,
            samples,
            ReflectionSettings::MEDIUM,
            &room(),
            &[mirror()],
            1,
            false,
        )?;
        assert!(
            halves(&shown, 0)[1] > 50,
            "visible model must actually appear"
        );
        assert_eq!(
            halves(&hidden, 0),
            [0, 0],
            "hidden model must leave the virtual view"
        );
        assert_eq!(halves(&hidden, 1), [0, 0]);
    }
    Ok(())
}

#[test]
fn a_mirror_shows_what_faces_it_on_the_same_side_and_hides_what_is_behind() -> Result<()> {
    let gpu = Gpu::turn()?;
    for samples in [1, 4] {
        let (pixels, stats) = gpu.frame(samples, ReflectionSettings::MEDIUM)?;
        assert_eq!(stats.reflection_passes, 1);
        // The orange card's face, only in the mirror, on the side it stands.
        let [left, right] = halves(&pixels, 0);
        assert!(left == 0 && right > 50, "orange {left} {right}, {samples}x");
        // The green card behind the mirror: hidden, and not reflected.
        assert_eq!(halves(&pixels, 1), [0, 0], "{samples}x");
    }
    Ok(())
}

#[test]
fn with_reflections_off_a_mirror_is_plain_silver() -> Result<()> {
    let gpu = Gpu::turn()?;
    let (pixels, stats) = gpu.frame(1, ReflectionSettings::OFF)?;
    assert_eq!(stats.reflection_passes, 0);
    assert_eq!(halves(&pixels, 0), [0, 0]);
    assert_eq!(halves(&pixels, 1), [0, 0]);
    // The centre is grey silver, not the blue clear colour.
    let centre = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
    let [r, g, b] = [pixels[centre], pixels[centre + 1], pixels[centre + 2]];
    assert!(
        r > 100 && r.abs_diff(b) < 30 && g.abs_diff(b) < 30,
        "{r} {g} {b}"
    );
    Ok(())
}

#[test]
fn a_live_mirror_is_as_sharp_and_true_as_the_room() -> Result<()> {
    // Each pixel of the reflection is the card's paint or the clear colour
    // exactly: no upscaling blur between them and no shift in colour.
    let gpu = Gpu::turn()?;
    for settings in [ReflectionSettings::MEDIUM, ReflectionSettings::HIGH] {
        let (pixels, _) = gpu.frame(1, settings)?;
        let card = [0xcc, 0x66, 0x33];
        let clear = [0x00, 0x00, 0x95];
        let near = |p: &[u8], c: [u8; 3]| (0..3).all(|i| p[i].abs_diff(c[i]) <= 1);
        let mut shown = 0;
        for p in pixels.chunks_exact(4) {
            assert!(
                near(p, card) || near(p, clear),
                "{:?} in {settings:?}",
                &p[..3]
            );
            shown += usize::from(near(p, card));
        }
        assert!(shown > 50, "{shown} card pixels in {settings:?}");
    }
    Ok(())
}

#[test]
fn facing_mirrors_show_what_only_the_one_behind_the_viewer_sees() -> Result<()> {
    // Behind the viewer a second mirror faces the first; between them an
    // orange card turns its face to the one behind, so only a reflection of
    // a reflection shows it.
    let mut data = SceneData::default();
    let (x, y) = (0.4, 0.1);
    quad(
        &mut data,
        [
            [x - 0.3, y - 0.3, 5.0],
            [x + 0.3, y - 0.3, 5.0],
            [x + 0.3, y + 0.3, 5.0],
            [x - 0.3, y + 0.3, 5.0],
        ],
        [0.8, 0.4, 0.2, 1.0],
        false,
    );
    let mut behind = mirror();
    behind.corners = [0, 3, 2, 1].map(|i| mirror().corners[i] + Vec3::new(0.0, 0.0, 6.0));
    let gpu = Gpu::turn()?;
    let (pixels, stats) =
        gpu.frame_of(1, ReflectionSettings::MEDIUM, &data, &[mirror(), behind], 1)?;
    assert_eq!(stats.reflection_passes, 2);
    let [left, right] = halves(&pixels, 0);
    // Twice mirrored, the card is on the side it stands.
    assert!(left == 0 && right > 10, "orange {left} {right}");
    // One pass: the mirror behind shows silver in the first.
    let (pixels, stats) =
        gpu.frame_of(1, ReflectionSettings::LOW, &data, &[mirror(), behind], 1)?;
    assert_eq!(stats.reflection_passes, 1);
    assert_eq!(halves(&pixels, 0), [0, 0]);
    Ok(())
}

#[test]
fn beyond_the_passes_facing_mirrors_repeat_what_the_nearer_mirror_showed() -> Result<()> {
    // A card between close facing mirrors turns its face to the one in
    // front of the viewer: seen there once, then (a mirror past the two
    // passes) again, small, deep in the tunnel, from the front mirror's
    // last picture.
    let mut data = SceneData::default();
    let (x, y, r) = (0.6, 0.0, 0.3);
    quad(
        &mut data,
        [
            [x + r, y - r, 1.0],
            [x - r, y - r, 1.0],
            [x - r, y + r, 1.0],
            [x + r, y + r, 1.0],
        ],
        [0.8, 0.4, 0.2, 1.0],
        false,
    );
    let mut behind = mirror();
    behind.corners = [0, 3, 2, 1].map(|i| mirror().corners[i] + Vec3::new(0.0, 0.0, 2.0));
    let gpu = Gpu::turn()?;
    let orange = |frames| -> Result<usize> {
        let (pixels, stats) = gpu.frame_from(
            [0.0, 0.0, 1.5],
            1,
            ReflectionSettings::MEDIUM,
            &data,
            &[mirror(), behind],
            frames,
        )?;
        assert_eq!(stats.reflection_passes, 2 * frames as u32);
        let [left, right] = halves(&pixels, 0);
        assert_eq!(left, 0, "every image of the card is on its own side");
        Ok(right)
    };
    let (once, echoed) = (orange(1)?, orange(2)?);
    assert!(once > 50, "{once}");
    assert!(echoed > once + 4, "{once} then {echoed}");
    Ok(())
}

/// Two-sided cards: one facing z and one facing x, centred at `c` with
/// half sizes `h`.
fn cross(data: &mut SceneData, c: [f32; 3], h: [f32; 3], color: [f32; 4]) {
    let [x, y, z] = c;
    quad(
        data,
        [
            [x - h[0], y - h[1], z],
            [x + h[0], y - h[1], z],
            [x + h[0], y + h[1], z],
            [x - h[0], y + h[1], z],
        ],
        color,
        true,
    );
    quad(
        data,
        [
            [x, y - h[1], z + h[2]],
            [x, y - h[1], z - h[2]],
            [x, y + h[1], z - h[2]],
            [x, y + h[1], z + h[2]],
        ],
        color,
        true,
    );
}
/// A 6 by 6 room walled with four mirrors, a checked floor and three
/// coloured posts.
fn mirror_room() -> (SceneData, Vec<Mirror>) {
    let mut data = SceneData::default();
    let s = 3.0;
    let n = 6;
    for i in 0..n {
        for j in 0..n {
            let (x0, z0) = (
                -s + 2.0 * s * i as f32 / n as f32,
                -s + 2.0 * s * j as f32 / n as f32,
            );
            let d = 2.0 * s / n as f32;
            let c = if (i + j) % 2 == 0 {
                [0.85, 0.85, 0.8, 1.0]
            } else {
                [0.2, 0.3, 0.6, 1.0]
            };
            quad(
                &mut data,
                [
                    [x0, -1.0, z0 + d],
                    [x0 + d, -1.0, z0 + d],
                    [x0 + d, -1.0, z0],
                    [x0, -1.0, z0],
                ],
                c,
                true,
            );
        }
    }
    cross(
        &mut data,
        [1.5, -0.4, -1.5],
        [0.3, 0.6, 0.3],
        [0.9, 0.1, 0.1, 1.0],
    );
    cross(
        &mut data,
        [-1.8, -0.5, 1.2],
        [0.25, 0.5, 0.25],
        [0.1, 0.8, 0.2, 1.0],
    );
    cross(
        &mut data,
        [-1.0, 0.0, -2.2],
        [0.2, 1.0, 0.2],
        [0.95, 0.75, 0.1, 1.0],
    );
    let m = |corners: [[f32; 3]; 4]| Mirror {
        corners: corners.map(Vec3::from),
        ..mirror()
    };
    let (b, t) = (-1.0, 2.0);
    let mirrors = vec![
        m([[-s, b, -s], [s, b, -s], [s, t, -s], [-s, t, -s]]),
        m([[s, b, s], [-s, b, s], [-s, t, s], [s, t, s]]),
        m([[-s, b, s], [-s, b, -s], [-s, t, -s], [-s, t, s]]),
        m([[s, b, -s], [s, b, s], [s, t, s], [s, t, -s]]),
    ];
    (data, mirrors)
}
#[test]
fn in_a_mirror_room_a_side_wall_past_the_passes_never_shows_another_view() -> Result<()> {
    // Looking into a corner of a square mirror room, each wall shows the
    // other a bounce deeper than the passes reach. Its echo (the picture
    // drawn for the player's own view of that wall) shows another part of
    // the room there, torn into black wedges and streaks; it fades to
    // silver instead. Several frames, as echoes show the frame before.
    let gpu = Gpu::turn()?;
    let (data, mirrors) = mirror_room();
    let camera = Camera::perspective([0.3, 0.3, 0.6], [-3.0, 0.1, -3.0], 1.0, 1.0, 0.05, 100.0);
    let (pixels, stats) =
        gpu.frame_with(&camera, 1, ReflectionSettings::MEDIUM, &data, &mirrors, 3)?;
    assert_eq!(stats.reflection_passes, 2 * 3);
    let black = pixels
        .chunks_exact(4)
        .filter(|p| p[..3].iter().all(|c| *c < 25))
        .count();
    assert_eq!(black, 0, "black pixels from echoes of the wrong view");
    Ok(())
}

/// Two windows linked as portals are: going in through `a` (the plane
/// z = 0, front +z) comes out of `b` (the plane z = 0 at x = 10, front -z)
/// moving on the same way. Past `b` stand a red card and a grey wall; a
/// green card stands right behind `a`, where nothing should show it.
fn portals() -> (SceneData, [Mirror; 2], glam::Affine3A) {
    let mut data = SceneData::default();
    quad(
        &mut data,
        [
            [9.5, -0.5, -3.0],
            [10.5, -0.5, -3.0],
            [10.5, 0.5, -3.0],
            [9.5, 0.5, -3.0],
        ],
        [0.9, 0.1, 0.1, 1.0],
        false,
    );
    quad(
        &mut data,
        [
            [4.0, -6.0, -8.0],
            [16.0, -6.0, -8.0],
            [16.0, 6.0, -8.0],
            [4.0, 6.0, -8.0],
        ],
        [0.5, 0.5, 0.5, 1.0],
        false,
    );
    quad(
        &mut data,
        [
            [-0.5, -0.5, -1.0],
            [0.5, -0.5, -1.0],
            [0.5, 0.5, -1.0],
            [-0.5, 0.5, -1.0],
        ],
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    let carry = glam::Affine3A::from_translation(Vec3::new(10.0, 0.0, 0.0));
    let window = |corners: [Vec3; 4], carry: glam::Affine3A| Mirror {
        corners,
        looks: bri_render::reflection::Looks::Through(glam::Mat4::from(carry.inverse())),
        fallback: [0.35, 0.42, 0.55],
        ..mirror()
    };
    let a = mirror().corners;
    // Seen from its front (-z), counterclockwise.
    let b = [a[1], a[0], a[3], a[2]].map(|c| carry.transform_point3(c));
    (data, [window(a, carry), window(b, carry.inverse())], carry)
}

#[test]
fn a_portal_the_eye_is_about_to_go_through_shows_what_the_far_side_will() -> Result<()> {
    // Valve's Portal rule: the frame before going through and the frame
    // after are the same picture. The eye closes in on `a` as a walk's eye
    // does, the window recessed as the client draws it within reach, and
    // each frame matches the one drawn straight from where its view comes
    // from past `b`, which is where the eye carries on from once through.
    let gpu = Gpu::new()?;
    let (data, windows, carry) = portals();
    let camera = |eye: Vec3| {
        Camera::perspective(
            eye.to_array(),
            (eye - Vec3::Z).to_array(),
            1.0,
            1.0,
            0.05,
            100.0,
        )
    };
    for distance in [0.6, 0.2, 0.06, 0.02, 0.004] {
        let eye = Vec3::new(0.1, 0.05, distance);
        let mut near = windows;
        if distance < 0.25 {
            near[0].recess = 0.2;
        }
        let (before, _) =
            gpu.frame_with(&camera(eye), 1, ReflectionSettings::MEDIUM, &data, &near, 1)?;
        // Where the eye's view comes from: past `b`, where it carries on
        // to once through.
        let out = carry.transform_point3(eye);
        let (after, _) = gpu.frame_with(
            &camera(out),
            1,
            ReflectionSettings::MEDIUM,
            &data,
            &windows,
            1,
        )?;
        let differ = before
            .chunks_exact(4)
            .zip(after.chunks_exact(4))
            .filter(|(a, b)| (0..3).any(|i| a[i].abs_diff(b[i]) > 24))
            .count();
        let share = differ as f32 / (SIZE * SIZE) as f32;
        assert!(share < 0.02, "{:.1}% differ at {distance}", share * 100.0);
        // The card past `b` shows; the one behind `a` never does.
        assert!(
            halves(&before, 0).iter().sum::<usize>() > 200,
            "at {distance}"
        );
        assert_eq!(halves(&before, 1), [0, 0], "at {distance}");
    }
    Ok(())
}
