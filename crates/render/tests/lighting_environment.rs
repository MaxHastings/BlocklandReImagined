//! Map lightmaps and water follow the live day/night environment.
//! Split by behavior so serial software-GPU verification fits the per-binary watchdog.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_content::environment::{Environment, Fog, Image};
use bri_render::{environment_scene, scene::*, shadow::ShadowSettings};
use common::*;
use glam::{Mat4, Vec3};

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
        sun_toward: (-sun).to_array(),
        direct_light: [0.6; 3],
        ambient_light: [0.3; 3],
        shadow_color: None,
        fog_start: 0.0,
        fog_end: 0.0,
        fog_color: [0.0; 3],
        sky_tint: [1.0; 3],
        flare: ([0.0; 4], 1.0),
        vignette: None,
        enhanced_sky: false,
        sky_color: [1.0; 3],
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

// "Sky: Enhanced": the procedural atmosphere follows the real sun, Original
// keeps the map's own sky textures, and far geometry fogs toward whichever sky
// is drawn.

const SIZE: (u32, u32) = (96, 96);

/// A sky of one flat colour a face, with fog from `fog_start` to 1000.
fn sky(fog_start: f32) -> SceneData {
    sky_with(fog_start, false)
}

/// `bottom`: the sky goes on below the horizon (Skylands' bottom face).
fn sky_with(fog_start: f32, bottom: bool) -> SceneData {
    let mut out = SceneData::default();
    let mut faces = Vec::new();
    for i in 0..6 {
        out.images.push(SceneImage {
            label: format!("face-{i}"),
            width: 1,
            height: 1,
            rgba: vec![200, 60, 200, 255],
            srgb: true,
        });
        faces.push(Image {
            file: format!("face-{i}.png"),
            source: format!("face-{i}"),
            sha256: "0".repeat(64),
            width: 1,
            height: 1,
        });
    }
    let env = Environment {
        schema_version: 2,
        source_materials: "fixture".into(),
        source_sha256: "0".repeat(64),
        faces,
        reflection: None,
        clouds: vec![],
        textures: true,
        bottom,
        horizon_band: false,
        solid_color: [0.3; 3],
        fog: Fog {
            start: fog_start,
            end: 1000.0,
            color: [0.5, 0.6, 0.5],
        },
        warnings: vec![],
    };
    let faces: Vec<usize> = (1..=6).collect();
    environment_scene::append(&mut out, &env, &faces, &[]).unwrap();
    out
}

fn toward(azimuth: f32, elevation: f32) -> Vec3 {
    let (a, e) = (azimuth.to_radians(), elevation.to_radians());
    Vec3::new(a.sin() * e.cos(), e.sin(), a.cos() * e.cos())
}

struct Shot {
    pixels: Vec<u8>,
}
impl Shot {
    fn at(&self, x: u32, y: u32) -> [i32; 3] {
        let i = ((y * SIZE.0 + x) * 4) as usize;
        [0, 1, 2].map(|c| i32::from(self.pixels[i + c]))
    }
}

/// The scene drawn looking along +Z, pitched up, with the sun at `sun`
/// (a direction toward it) under `enhanced`, plus `extra` geometry.
fn shoot(enhanced: bool, sun: Vec3, scene: &SceneData, extra: Option<&SceneData>) -> Result<Shot> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let target = color_target(&device, format, SIZE.0, SIZE.1);
    let sky = renderer.upload(&device, &queue, scene)?;
    let uploaded = extra
        .map(|e| renderer.upload(&device, &queue, e))
        .transpose()?;
    let mut camera = Camera::perspective([0., 2., 0.], [0., 2. + 0.35, 1.], 1.0, 1.3, 0.1, 3000.0);
    camera.apply_environment(scene);
    camera.sun_direction = (-sun).extend(0.).to_array();
    camera.sky_sun = sun.extend(0.).to_array();
    camera.set_enhanced_sky(enhanced);
    renderer.update_camera(&queue, &camera);
    let mut scenes = vec![&sky];
    scenes.extend(uploaded.as_ref());
    let pixels = render(&device, &queue, &mut renderer, &target, &scenes, &[], &[])?;
    if let Ok(dir) = std::env::var("BRI_SKY_DUMP") {
        let name = format!(
            "{dir}/{}-{:.0}{}.ppm",
            if enhanced { "enhanced" } else { "original" },
            sun.y.asin().to_degrees(),
            if extra.is_some() { "-wall" } else { "" }
        );
        let mut ppm = format!("P6\n{} {}\n255\n", SIZE.0, SIZE.1).into_bytes();
        ppm.extend(pixels.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]));
        let _ = std::fs::write(name, ppm);
    }
    Ok(Shot { pixels })
}

#[test]
fn original_keeps_the_map_sky_whatever_the_sun_does() -> Result<()> {
    let scene = sky(500.0);
    let noon = shoot(false, toward(30., 70.), &scene, None)?;
    let night = shoot(false, toward(30., -40.), &scene, None)?;
    assert_eq!(noon.pixels, night.pixels);
    // The authored faces are magenta, not a sky blue.
    let [r, g, b] = noon.at(48, 4);
    assert!(r > g + 30 && b > g + 30, "{r} {g} {b}");
    Ok(())
}

#[test]
fn enhanced_sky_runs_from_blue_noon_through_sunset_to_night() -> Result<()> {
    let scene = sky(500.0);
    let noon = shoot(true, toward(30., 70.), &scene, None)?;
    let [r, g, b] = noon.at(48, 3);
    assert!(b > g && g > r && b > 120, "zenith {r} {g} {b}");
    // The horizon is paler than the zenith above it.
    let low = noon.at(48, 62);
    assert!(low[0] > r, "horizon {low:?} zenith {r}");
    let sunset = shoot(true, toward(30., 1.5), &scene, None)?;
    let [r, _, b] = sunset.at(48, 60);
    assert!(r > b + 30, "sunset horizon red {r} blue {b}");
    let night = shoot(true, toward(30., -40.), &scene, None)?;
    for y in [3, 30, 60] {
        let p = night.at(48, y);
        assert!(p.iter().all(|c| *c < 70) && p[2] >= p[0], "night {p:?}");
    }
    // The sky changes with the sun, unlike Original's.
    assert_ne!(noon.pixels, sunset.pixels);
    Ok(())
}

/// A wall of white bricks, far and tall enough to fill the middle of the
/// frame, fogs toward the sky drawn beside it.
fn wall(distance: f32) -> SceneData {
    cuboid(
        Vec3::new(-400., -50., distance),
        Vec3::new(400., 60., distance + 4.),
    )
}

#[test]
fn far_geometry_fogs_toward_the_enhanced_sky() -> Result<()> {
    let scene = sky(100.0);
    let far = wall(900.0);
    for sun in [toward(30., 70.), toward(30., 1.5), toward(30., -40.)] {
        let shot = shoot(true, sun, &scene, Some(&far))?;
        // Where the wall meets the sky beside its top edge: the wall's
        // pixel is within a few levels of the sky's, no silhouette.
        let rows: Vec<[i32; 3]> = (0..SIZE.1).map(|y| shot.at(48, y)).collect();
        let jump = rows
            .windows(2)
            .map(|w| (0..3).map(|c| (w[0][c] - w[1][c]).abs()).max().unwrap())
            .max()
            .unwrap();
        assert!(jump < 28, "a {jump}-level edge in {rows:?}");
    }
    Ok(())
}

/// With `BRI_SKY_DUMP=<dir>`: the day, from midday to night, as PPM frames
/// for a human look (Original beside Enhanced at each sun height).
#[test]
fn the_day_for_a_human_look() -> Result<()> {
    if std::env::var("BRI_SKY_DUMP").is_err() {
        return Ok(());
    }
    let scene = sky(300.0);
    for elevation in [60., 20., 6., 0.5, -6., -30.] {
        for enhanced in [false, true] {
            shoot(enhanced, toward(30., elevation), &scene, None)?;
        }
    }
    Ok(())
}

/// A map whose sky goes on below the horizon (Skylands) shows the generated
/// sky there too: looking down is the sky mirrored, not the fog backdrop.
#[test]
fn a_sky_below_the_horizon_is_generated_too() -> Result<()> {
    let scene = sky_with(500.0, true);
    let sun = toward(30., 40.);
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let target = color_target(&device, format, SIZE.0, SIZE.1);
    let gpu_scene = renderer.upload(&device, &queue, &scene)?;
    let mut pixels = |pitch: f32| -> Result<Shot> {
        let mut camera =
            Camera::perspective([0., 2., 0.], [0., 2. + pitch, 1.], 1.0, 0.4, 0.1, 3000.0);
        camera.apply_environment(&scene);
        camera.sky_sun = sun.extend(1.).to_array();
        renderer.update_camera(&queue, &camera);
        let pixels = render(
            &device,
            &queue,
            &mut renderer,
            &target,
            &[&gpu_scene],
            &[],
            &[],
        )?;
        Ok(Shot { pixels })
    };
    let up = pixels(1.0)?.at(48, 48);
    let down = pixels(-1.0)?.at(48, 48);
    // A sky blue, not the authored magenta, and (away from the sun's glow)
    // a little dimmer than the sky above.
    assert!(down[2] > down[0] && down[1] > down[0], "{down:?}");
    assert!(
        down.iter().sum::<i32>() <= up.iter().sum::<i32>(),
        "{up:?} {down:?}"
    );
    Ok(())
}

/// How the occlusion test frame is drawn: the world alone, split around a
/// pass that does nothing, or split around ambient occlusion (the game's
/// order, so it never shades the glass drawn after it).
#[derive(Clone, Copy)]
enum Occlusion {
    Off,
    SplitOnly,
    BeforeBlended,
}

/// A floor meeting a wall ahead of the camera, with a half-transparent red
/// pane (drawn after the occlusion) over the left half of the crease.
fn occlusion_frame(order: Occlusion) -> Result<Vec<u8>> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, None);
    let target = color_target(&device, format, SIZE.0, SIZE.1);
    let floor = cuboid(Vec3::new(-20., -1., -2.), Vec3::new(20., 0., 30.));
    let wall = cuboid(Vec3::new(-20., 0., 8.), Vec3::new(20., 6., 9.));
    let mut pane = cuboid(Vec3::new(-20., 0., 6.), Vec3::new(0., 3., 6.1));
    pane.materials[0].alpha = AlphaMode::Blend;
    for v in &mut pane.vertices {
        v.color = [1., 0., 0., 0.5];
    }
    let scenes = [floor, wall, pane]
        .iter()
        .map(|s| renderer.upload(&device, &queue, s))
        .collect::<Result<Vec<_>>>()?;
    let scenes: Vec<&GpuScene> = scenes.iter().collect();
    let mut camera = Camera::perspective([0., 2., 0.], [0., 0.5, 8.], 1.0, 1.0, 0.1, 200.0);
    camera.ambient = [0.7, 0.7, 0.7, 0.0];
    renderer.update_camera(&queue, &camera);
    let occlusion = bri_render::ambient_occlusion::AmbientOcclusion::new(&device, format, 1);
    let view = target.create_view(&Default::default());
    let depth = create_depth(&device, SIZE.0, SIZE.1).create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    let pass = || WorldPass {
        view: 0,
        color: &view,
        resolve: None,
        depth: &depth,
        viewport: None,
        clear: Some(wgpu::Color::BLACK),
        after_opaque: None,
        after_all: None,
    };
    let eye = [camera.eye[0], camera.eye[1], camera.eye[2]];
    let mut occlude = |encoder: &mut wgpu::CommandEncoder| {
        occlusion.render(
            &device,
            &queue,
            encoder,
            &view,
            &depth,
            SIZE,
            camera.view_projection,
            eye,
            (camera.atmosphere, camera.fog_color[3]),
        )
    };
    match order {
        Occlusion::Off => renderer.render_world(&mut encoder, pass(), &scenes, &[]),
        Occlusion::SplitOnly => {
            renderer.render_world_split(&mut encoder, pass(), &scenes, &[], &mut |_| {})
        }
        Occlusion::BeforeBlended => {
            renderer.render_world_split(&mut encoder, pass(), &scenes, &[], &mut occlude)
        }
    }
    read_back(&device, &queue, encoder, &target)
}

fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    target: &wgpu::Texture,
) -> Result<Vec<u8>> {
    let (width, height) = (target.width(), target.height());
    let row = (width * 4).div_ceil(256) * 256;
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

#[test]
fn ambient_occlusion_darkens_creases_and_off_draws_as_before() -> Result<()> {
    let off = occlusion_frame(Occlusion::Off)?;
    // Splitting the world pass alone changes nothing: with the occlusion
    // off the picture is the one the game drew before it existed.
    assert_eq!(off, occlusion_frame(Occlusion::SplitOnly)?);
    let on = occlusion_frame(Occlusion::BeforeBlended)?;
    // The crease where the floor meets the wall darkens; nothing brightens.
    let right = SIZE.0 / 2 + 4..SIZE.0;
    let darkest = (0..SIZE.1)
        .flat_map(|y| right.clone().map(move |x| ((y * SIZE.0 + x) * 4) as usize))
        .map(|i| f32::from(on[i]) / f32::from(off[i]).max(1.0))
        .fold(1.0f32, f32::min);
    assert!(darkest < 0.95, "the crease darkens: {darkest}");
    assert!(on.iter().zip(&off).all(|(on, off)| on <= off));
    Ok(())
}
