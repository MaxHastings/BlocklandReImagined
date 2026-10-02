use anyhow::Result;
use bri_render::{scene::*, terrain_scene::GpuTerrain};
use glam::{Mat4, Vec3};

fn triangle(color: [f32; 4], z: f32, alpha: AlphaMode) -> SceneData {
    let vertices = [[-0.9, -0.8, z], [0.9, -0.8, z], [0.0, 0.9, z]]
        .into_iter()
        .map(|position| SceneVertex {
            position,
            normal: [0.0, 0.0, 1.0],
            uv: [0.0; 2],
            lightmap_uv: [0.0; 2],
            color,
            fx: [0.; 4],
        })
        .collect();
    let mut data = SceneData {
        vertices,
        indices: vec![0, 1, 2],
        ..Default::default()
    };
    let mut material = Material::surface("test", 0, 0);
    material.alpha = alpha;
    data.materials.push(material);
    data.batches.push(MeshBatch {
        indices: 0..3,
        material: 0,
        center: [0.0, 0.0, z],
    });
    data
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter: String,
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

#[test]
fn point_lights_update_and_clear_without_reuploading_geometry() -> Result<()> {
    let gpu = Gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut data = triangle([1.; 4], 0.5, AlphaMode::Opaque);
    data.materials[0] = Material::vertex_lit("dark fixture", 0);
    let mesh = renderer.upload(&gpu.device, &gpu.queue, &data)?;
    let camera = Camera {
        sun_color: [0.; 4],
        ambient: [0.; 4],
        ..Default::default()
    };
    let dark = gpu.frame(&mut renderer, &[&mesh], &camera, (64, 64))?;
    let center = (32 * 64 + 32) * 4;
    assert_eq!(&dark[center..center + 3], &[0, 0, 0]);
    let mut light = PointLight {
        position_radius: [0., 0., 1.5, 4.],
        color: [1., 0., 0., 0.],
    };
    renderer.update_lights(&gpu.queue, &[light])?;
    let red = gpu.frame(&mut renderer, &[&mesh], &camera, (64, 64))?;
    assert!(red[center] > 100 && red[center + 1] == 0);
    light.position_radius[0] = 10.;
    renderer.update_lights(&gpu.queue, &[light])?;
    assert_eq!(dark, gpu.frame(&mut renderer, &[&mesh], &camera, (64, 64))?);
    light.position_radius[0] = 0.;
    light.color = [0., 1., 0., 0.];
    renderer.update_lights(&gpu.queue, &[light])?;
    let green = gpu.frame(&mut renderer, &[&mesh], &camera, (64, 64))?;
    assert!(green[center + 1] > 100 && green[center] == 0);
    renderer.update_lights(&gpu.queue, &[])?;
    assert_eq!(dark, gpu.frame(&mut renderer, &[&mesh], &camera, (64, 64))?);
    light.color[0] = f32::NAN;
    assert!(renderer.update_lights(&gpu.queue, &[light]).is_err());
    Ok(())
}

/// A vertex-lit square facing +z at depth `z`, corners at +-`half`.
fn square(half: f32, z: f32) -> SceneData {
    let mut data = triangle([1.; 4], z, AlphaMode::Opaque);
    data.materials[0] = Material::vertex_lit("square", 0);
    data.vertices = [[-half, -half], [half, -half], [half, half], [-half, half]]
        .into_iter()
        .map(|[x, y]| SceneVertex {
            position: [x, y, z],
            ..data.vertices[0]
        })
        .collect();
    data.indices = vec![0, 1, 2, 0, 2, 3];
    data.batches[0].indices = 0..6;
    data
}

/// v20 lights bricks, players and items with fixed-function GL: per vertex,
/// colour x N.L / (1 + 0.1 d^2) (docs/audits/bricks.md). A light over the
/// middle of a large face whose corners it does not reach leaves the face
/// dark, as a lamp leaves a v20 baseplate; corners it reaches take exactly
/// GL's attenuation, interpolated across the face.
#[test]
fn point_lights_light_objects_per_vertex_with_v20_attenuation() -> Result<()> {
    let gpu = Gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let camera = Camera {
        sun_color: [0.; 4],
        ambient: [0.; 4],
        ..Default::default()
    };
    let center = (32 * 64 + 32) * 4;
    // Corners 0.5 across and 1 below the light: d^2 = 1.5 at every corner.
    let small = renderer.upload(&gpu.device, &gpu.queue, &square(0.5, 0.5))?;
    renderer.update_lights(
        &gpu.queue,
        &[PointLight {
            position_radius: [0., 0., 1.5, 10.],
            color: [1., 1., 1., 0.],
        }],
    )?;
    let lit = gpu.frame(&mut renderer, &[&small], &camera, (64, 64))?;
    let expected = (1.0 / 1.5_f32.sqrt() / (1.0 + 0.1 * 1.5) * 255.0).round() as u8;
    assert!(
        lit[center].abs_diff(expected) <= 2,
        "{} vs {expected}",
        lit[center]
    );
    // The light 0.3 over the middle of a square whose corners lie beyond
    // its 0.8 reach: a per-pixel light would light the middle brightly.
    let large = renderer.upload(&gpu.device, &gpu.queue, &square(0.9, 0.5))?;
    renderer.update_lights(
        &gpu.queue,
        &[PointLight {
            position_radius: [0., 0., 0.8, 0.8],
            color: [5., 5., 5., 0.],
        }],
    )?;
    let far = gpu.frame(&mut renderer, &[&large], &camera, (64, 64))?;
    assert_eq!(&far[center..center + 3], &[0, 0, 0]);
    Ok(())
}

#[test]
fn a_full_light_budget_lights_each_pixel_with_the_lights_that_reach_it() -> Result<()> {
    let gpu = Gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut data = triangle([1.; 4], 0.5, AlphaMode::Opaque);
    data.materials[0] = Material::vertex_lit("dark fixture", 0);
    let mesh = renderer.upload(&gpu.device, &gpu.queue, &data)?;
    let camera = Camera {
        sun_color: [0.; 4],
        ambient: [0.; 4],
        ..Default::default()
    };
    let near = [
        PointLight {
            position_radius: [0.3, 0., 1.5, 4.],
            color: [0.6, 0., 0., 0.],
        },
        PointLight {
            position_radius: [-0.3, 0.2, 1.2, 3.],
            color: [0., 0.5, 0.3, 0.],
        },
    ];
    renderer.update_lights(&gpu.queue, &near)?;
    let expected = gpu.frame(&mut renderer, &[&mesh], &camera, (64, 64))?;
    // The same two among 254 lights that cannot reach the triangle, spread
    // wide enough to make the grid's cells coarse.
    let mut all: Vec<_> = (0..254)
        .map(|i| PointLight {
            position_radius: [20. + (i % 16) as f32 * 9., (i / 16) as f32 * 7., -30., 6.],
            color: [1., 1., 1., 0.],
        })
        .collect();
    all.insert(100, near[0]);
    all.insert(200, near[1]);
    renderer.update_lights(&gpu.queue, &all)?;
    let lit = gpu.frame(&mut renderer, &[&mesh], &camera, (64, 64))?;
    let center = (32 * 64 + 32) * 4;
    assert!(expected[center] > 50 && expected[center + 1] > 30);
    assert_eq!(expected, lit);
    let stats = renderer.stats();
    assert_eq!(stats.point_lights, 256);
    assert!(stats.lights_per_cell < 64, "{stats:?}");
    Ok(())
}

#[test]
fn water_depth_mask_and_time_motion_use_one_upload() -> Result<()> {
    use bri_content::{environment::Image, water::Water};
    let image = Image {
        file: "fixture.png".into(),
        source: "fixture".into(),
        sha256: "0".repeat(64),
        width: 2,
        height: 1,
    };
    let water = Water {
        schema_version: 1,
        node: 0,
        id: "water-test".into(),
        min: [-4., -2., -4.],
        max: [4., 0., 4.],
        repeat_period: None,
        liquid_type: "water".into(),
        density: 1.,
        viscosity: 40.,
        surface: image.clone(),
        shore: image,
        reflection: None,
        opacity: 0.4,
        wave_amplitude: 0.,
        flow: [0.2, 0.1],
        distortion: [0.1, 0.0, 0.5],
        tiles: [1., 1.],
        depth_mask: true,
        depth_alpha: [0.2, 0.8, 1., 1.],
        reflection_intensity: 0.,
        parallax: 0.5,
        warnings: vec![],
        current: [0.0; 3],
    };
    let mut data = SceneData::default();
    data.images.push(SceneImage {
        label: "stripes".into(),
        width: 2,
        height: 1,
        rgba: vec![255, 50, 20, 255, 20, 50, 255, 255],
        srgb: true,
    });
    bri_render::water_scene::append(
        &mut data,
        &water,
        [1, 1, 0],
        ([1.0; 4], 6.0),
        true,
        |x, _| Some(if x < 0. { 2. } else { -4. }),
    )?;
    let mask = &data.images.last().unwrap().rgba;
    assert_eq!(
        &mask[(128 * 256 + 64) * 4..(128 * 256 + 64) * 4 + 2],
        &[0, 0]
    );
    assert!(mask[(128 * 256 + 192) * 4] > 0);
    let gpu = Gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let scene = renderer.upload(&gpu.device, &gpu.queue, &data)?;
    let mut camera = Camera::perspective(
        [0., 6., 6.],
        [0., 0., 0.],
        1.,
        70f32.to_radians(),
        0.1,
        100.,
    );
    let first = gpu.frame(&mut renderer, &[&scene], &camera, (128, 128))?;
    camera.atmosphere[2] = 1.3;
    let second = gpu.frame(&mut renderer, &[&scene], &camera, (128, 128))?;
    assert_ne!(first, second, "Water texture motion did not advance");
    // Removing geometry must change the image: a successful empty GPU pass is
    // not evidence of visible water.
    let empty = gpu.frame(&mut renderer, &[], &camera, (128, 128))?;
    assert_ne!(first, empty);
    Ok(())
}

fn sky_fixture() -> (SceneData, bri_content::environment::Environment) {
    use bri_content::environment::{Environment, Fog, Image};
    let mut data = SceneData::default();
    let colors = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
        [255, 0, 255, 255],
        [0, 255, 255, 255],
    ];
    let mut faces = vec![];
    for (i, color) in colors.iter().enumerate() {
        data.images.push(SceneImage {
            label: format!("face-{i}"),
            width: 1,
            height: 1,
            rgba: color.to_vec(),
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
        bottom: true,
        horizon_band: false,
        solid_color: [0.3; 3],
        fog: Fog {
            start: 1.0,
            end: 3.0,
            color: [1.0; 3],
        },
        warnings: vec![],
    };
    (data, env)
}
#[test]
fn sky_orientation_translation_depth_and_distance_fog() -> Result<()> {
    let gpu = Gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let (mut data, env) = sky_fixture();
    bri_render::environment_scene::append(&mut data, &env, &[1, 2, 3, 4, 5, 6], &[])?;
    let sky = renderer.upload(&gpu.device, &gpu.queue, &data)?;
    let center = (32 * 64 + 32) * 4;
    for (direction, color) in [
        (Vec3::Z, [255, 0, 0]),
        (Vec3::X, [0, 255, 0]),
        (Vec3::NEG_Z, [0, 0, 255]),
        (Vec3::NEG_X, [255, 255, 0]),
        (Vec3::Y, [255, 0, 255]),
        (Vec3::NEG_Y, [0, 255, 255]),
    ] {
        let camera = Camera::perspective(
            [0.0; 3],
            direction.to_array(),
            1.0,
            60_f32.to_radians(),
            0.05,
            10.0,
        );
        let before = gpu.frame(&mut renderer, &[&sky], &camera, (64, 64))?;
        assert_eq!(
            before[center..center + 3],
            color,
            "Wrong sky face {direction}"
        );
        let eye = Vec3::new(12.0, 7.0, -3.0);
        let translated = Camera::perspective(
            eye.to_array(),
            (eye + direction).to_array(),
            1.0,
            60_f32.to_radians(),
            0.05,
            10.0,
        );
        let after = gpu.frame(&mut renderer, &[&sky], &translated, (64, 64))?;
        assert_eq!(before, after, "Sky has translation parallax");
    }
    let mut geometry = triangle([0.0, 0.0, 0.0, 1.0], -2.0, AlphaMode::Opaque);
    geometry.materials[0].double_sided = true;
    let foreground = renderer.upload(&gpu.device, &gpu.queue, &geometry)?;
    let mut camera = Camera::perspective(
        [0.0; 3],
        [0.0, 0.0, -1.0],
        1.0,
        60_f32.to_radians(),
        0.05,
        10.0,
    );
    let plain = gpu.frame(&mut renderer, &[&foreground, &sky], &camera, (64, 64))?;
    assert_eq!(
        plain[center..center + 3],
        [0, 0, 0],
        "Sky overwrote nearer opaque geometry"
    );
    camera.apply_environment(&data);
    let fog = gpu.frame(&mut renderer, &[&foreground, &sky], &camera, (64, 64))?;
    let haze = (255.0 * env.fog.amount(2.0)).round() as u8;
    for value in &fog[center..center + 3] {
        assert!(
            value.abs_diff(haze) <= 2,
            "Expected {haze} haze halfway through authored range, got {value}"
        );
    }
    assert_eq!(env.fog.amount(0.5), 0.0);
    assert_eq!(env.fog.amount(1.0), 0.0);
    assert!((env.fog.amount(2.0) - (1.0 - (-2.0_f32).exp())).abs() < 1e-5);
    assert_eq!(env.fog.amount(3.0), 1.0);
    assert_eq!(env.fog.amount(10.0), 1.0);
    Ok(())
}

/// Thick or thin, the fog that hides far geometry covers the sky toward the
/// horizon by the same rule (`Fog::sky_amount`), so the horizon is one fog
/// colour instead of fogged silhouettes cut out against a clear sky.
#[test]
fn sky_fades_into_the_world_fog_at_the_horizon() -> Result<()> {
    let gpu = Gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let (mut data, mut env) = sky_fixture();
    env.fog.start = 100.0;
    env.fog.end = 1000.0;
    bri_render::environment_scene::append(&mut data, &env, &[1, 2, 3, 4, 5, 6], &[])?;
    let sky = renderer.upload(&gpu.device, &gpu.queue, &data)?;
    let center = (32 * 64 + 32) * 4;
    // Each centre pixel's own ray sits half a pixel off the view direction.
    let half_pixel = 30_f32.to_radians().tan() / 64.0;
    let rising = Vec3::new(0.0, 0.1, 1.0).normalize();
    let right = rising.cross(Vec3::Y).normalize();
    let ray = (rising + (right - right.cross(rising)) * half_pixel).normalize();
    let fogged = |face: [u8; 3], up: f32| {
        let a = env.fog.sky_amount(up, env.bottom);
        face.map(|c| (f32::from(c) * (1.0 - a) + 255.0 * a).round() as u8)
    };
    for (direction, expected) in [
        (Vec3::Z, [255, 255, 255]),
        (rising, fogged([255, 0, 0], ray.y)),
        (Vec3::Y, fogged([255, 0, 255], 1.0)),
        // A bottom face (Skylands' mirrored floor) hazes like the sky above.
        (Vec3::NEG_Y, fogged([0, 255, 255], -1.0)),
    ] {
        let mut camera = Camera::perspective(
            [0.0; 3],
            direction.to_array(),
            1.0,
            60_f32.to_radians(),
            0.05,
            10.0,
        );
        camera.apply_environment(&data);
        let frame = gpu.frame(&mut renderer, &[&sky], &camera, (64, 64))?;
        for (got, want) in frame[center..center + 3].iter().zip(expected) {
            assert!(
                got.abs_diff(want) <= 3,
                "Sky toward {direction} is {:?}, expected {expected:?}",
                &frame[center..center + 3]
            );
        }
    }
    // A thin fog leaves a horizon haze and a nearly clear sky overhead.
    let haze = env.fog.sky_amount(ray.y, env.bottom);
    assert!(haze > 0.3 && haze < 0.95, "haze {haze}");
    assert!(env.fog.sky_amount(1.0, env.bottom) < 0.1);
    assert_eq!(env.fog.sky_amount(0.0, env.bottom), 1.0);
    // Without a bottom face the fog backdrop fills below the horizon.
    assert_eq!(env.fog.sky_amount(-0.5, false), 1.0);
    // Geometry ends as fogged as the sky behind it, high, level or low.
    for up in [-0.4_f32, 0.0, 0.05, 0.2, 0.6] {
        let d = env.fog.end;
        let offset = [0.0, up * d, (1.0 - up * up).sqrt() * d];
        assert!((env.fog.amount_along(offset, env.bottom) - env.fog.sky_amount(up, env.bottom)).abs() < 1e-3);
    }
    // Thick fog reaches far up the sky.
    env.fog.start = 5.0;
    env.fog.end = 90.0;
    assert!(env.fog.sky_amount(0.5, env.bottom) > 0.99);
    Ok(())
}

#[test]
fn cloud_wind_updates_without_geometry_upload_and_calm_stays_still() -> Result<()> {
    use bri_content::environment::{Cloud, Image};
    let gpu = Gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    for velocity in [[0.125, 0.0], [0.0, 0.0]] {
        let (mut data, mut env) = sky_fixture();
        // Fog far enough out to leave the sky overhead clear.
        env.fog.start = 100.0;
        env.fog.end = 1000.0;
        data.images.push(SceneImage {
            label: "cloud-pattern".into(),
            width: 2,
            height: 2,
            rgba: vec![
                255, 255, 255, 128, 0, 0, 255, 128, 0, 0, 255, 128, 255, 255, 255, 128,
            ],
            srgb: true,
        });
        env.clouds.push(Cloud {
            image: Image {
                file: "cloud.png".into(),
                source: "fixture".into(),
                sha256: "0".repeat(64),
                width: 2,
                height: 2,
            },
            center_height: 0.5,
            velocity,
        });
        bri_render::environment_scene::append(&mut data, &env, &[1, 2, 3, 4, 5, 6], &[7])?;
        let scene = renderer.upload(&gpu.device, &gpu.queue, &data)?;
        let mut camera = Camera::perspective(
            [0.0; 3],
            [0.1, 1.0, 0.0],
            1.0,
            60_f32.to_radians(),
            0.05,
            10.0,
        );
        camera.apply_environment(&data);
        let mut frames = Vec::new();
        for seconds in [0.0, 2.0, 8.0] {
            camera.atmosphere[2] = seconds;
            frames.push(gpu.frame(&mut renderer, &[&scene], &camera, (64, 64))?);
        }
        if velocity[0] > 0.0 {
            assert_ne!(
                frames[0], frames[1],
                "Wind did not animate the cloud texture"
            );
        } else {
            assert_eq!(frames[0], frames[1], "Calm clouds moved");
        }
        assert_eq!(
            frames[0], frames[2],
            "Cloud UV period changed after a full cycle"
        );
    }
    Ok(())
}

/// An avatar rig to pose and draw: the made-up one of
/// `bri_content::testing::avatar`, or the converted original.
struct AvatarRig {
    rig: bri_content::avatar::Rig,
    /// Where the frames and a report are kept for review (the converted
    /// rig only; the made-up one keeps nothing).
    artifacts: Option<std::path::PathBuf>,
}
impl AvatarRig {
    fn synthetic() -> Result<Self> {
        let rig = bri_content::testing::avatar::rig();
        rig.validate()?;
        Ok(Self {
            rig,
            artifacts: None,
        })
    }
    fn content() -> Result<Self> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rig: bri_content::avatar::Rig = serde_json::from_slice(&std::fs::read(
            root.join("content/avatar-rig-001/rig.json"),
        )?)?;
        rig.validate()?;
        Ok(Self {
            rig,
            artifacts: Some(root.join("artifacts/native-avatar")),
        })
    }
}

#[test]
fn avatar_layers_update_one_persistent_gpu_scene() -> Result<()> {
    avatar_layers_update_one_persistent_gpu_scene_on(&AvatarRig::synthetic()?)
}

#[test]
#[ignore = "requires generated v20 content"]
fn original_avatar_layers_update_one_persistent_gpu_scene() -> Result<()> {
    avatar_layers_update_one_persistent_gpu_scene_on(&AvatarRig::content()?)
}

/// The converted rig's own catalog: its sequence count and the aliases
/// that share one clip's tracks.
#[test]
#[ignore = "requires generated v20 content"]
fn original_avatar_rig_sequence_catalog() -> Result<()> {
    let rig = AvatarRig::content()?.rig;
    assert_eq!(rig.sequences.len(), 39);
    assert_eq!(
        rig.sequence("run").unwrap().nodes.len(),
        rig.sequence("WALK").unwrap().nodes.len()
    );
    assert_eq!(
        rig.sequence("jump").unwrap().frames,
        rig.sequence("standjump").unwrap().frames
    );
    Ok(())
}

fn avatar_layers_update_one_persistent_gpu_scene_on(fixture: &AvatarRig) -> Result<()> {
    use bri_content::animation::{Layer, sample, sample_layers, triangles};
    use bri_render::shape_scene::ShapeInstance;
    let rig = &fixture.rig;
    let detail = rig.shape.details.iter().position(|d| !d.collision).unwrap();
    let selected = [
        "headskin", "chest", "pants", "rarm", "larm", "rhand", "lhand", "rshoe", "lshoe",
    ];
    let visible = |name: &str| selected.contains(&name.to_ascii_lowercase().as_str());
    let mut sampled = 0;
    for clip in rig.sequences.values() {
        for phase in [-0.25, 0.0, 0.125, 0.5, 0.875, 1.25] {
            let pose = sample(&rig.shape, Some(clip), clip.duration * phase)?;
            let geometry = triangles(&rig.shape, &pose, detail, visible)?;
            assert!(!geometry.is_empty());
            assert!(
                geometry
                    .iter()
                    .flat_map(|t| &t.vertices)
                    .all(|v| v.position.is_finite() && v.normal.is_finite())
            );
            sampled += 1;
        }
    }
    let run = rig.sequence("run").unwrap();
    let holding = rig.sequence("armReadyRight").unwrap();
    let look = rig.sequence("look").unwrap();
    let layers = [
        Layer {
            animation: run,
            time: run.duration * 0.25,
            weight: 1.0,
        },
        Layer {
            animation: holding,
            time: holding.duration,
            weight: 1.0,
        },
        Layer {
            animation: look,
            time: look.duration * 0.2,
            weight: 1.0,
        },
    ];
    let base = sample_layers(&rig.shape, &layers[..1])?;
    let held = sample_layers(&rig.shape, &layers[..2])?;
    assert!(
        base.nodes
            .iter()
            .zip(&held.nodes)
            .any(|(a, b)| !a.abs_diff_eq(*b, 1e-5))
    );
    // Holding a tool must not replace the running pose on the legs (a
    // skinned part has no node of its own; its bones are the legs').
    let mut legs = 0;
    for node in rig
        .shape
        .objects
        .iter()
        .filter(|o| ["rshoe", "lshoe", "pants"].contains(&o.name.to_ascii_lowercase().as_str()))
        .filter_map(|o| o.node)
    {
        assert!(base.nodes[node].abs_diff_eq(held.nodes[node], 1e-5));
        legs += 1;
    }
    assert!(legs >= 2, "the rig has no leg parts to check");
    let mut data = Vec::new();
    for count in 1..=3 {
        let pose = sample_layers(&rig.shape, &layers[..count])?;
        let mut scene = SceneData {
            name: "Original Blockhead pose diagnostic".into(),
            ..Default::default()
        };
        scene.materials.push(Material::vertex_lit(
            "Diagnostic paint; original face/material binding pending",
            0,
        ));
        let materials = vec![0; rig.shape.materials.len()];
        scene.append_shape(
            ShapeInstance {
                shape: &rig.shape,
                pose: &pose,
                detail,
                transform: Mat4::IDENTITY,
                materials: &materials,
                translucent_materials: None,
                unassigned_material: 0,
            },
            |name| {
                if !visible(name) {
                    return None;
                }
                let name = name.to_ascii_lowercase();
                Some(if name.contains("head") || name.contains("hand") {
                    [0.9, 0.7, 0.35, 1.0]
                } else if name.contains("arm") || name.contains("chest") {
                    [0.2, 0.45, 0.85, 1.0]
                } else {
                    [0.2, 0.2, 0.25, 1.0]
                })
            },
        )?;
        scene.validate()?;
        data.push(scene);
    }
    let gpu = Gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut scene = renderer.upload(&gpu.device, &gpu.queue, &data[0])?;
    let mut low = Vec3::splat(f32::INFINITY);
    let mut high = Vec3::splat(f32::NEG_INFINITY);
    for vertex in data.iter().flat_map(|s| &s.vertices) {
        low = low.min(Vec3::from(vertex.position));
        high = high.max(Vec3::from(vertex.position));
    }
    let center = (low + high) * 0.5;
    let extent = (high - low).max_element();
    let camera = Camera::perspective(
        (center + Vec3::new(1.0, 0.35, -1.6) * extent).to_array(),
        center.to_array(),
        1.0,
        42_f32.to_radians(),
        0.01,
        extent * 10.0,
    );
    if let Some(output) = &fixture.artifacts {
        std::fs::create_dir_all(output)?;
    }
    let mut frames = Vec::new();
    for (i, data) in data.iter().enumerate() {
        assert_eq!(
            data.indices,
            (0..scene.index_count as u32).collect::<Vec<_>>()
        );
        scene.update_vertices(
            &gpu.queue,
            &data.vertices,
            &data.batches.iter().map(|b| b.center).collect::<Vec<_>>(),
        )?;
        let frame = gpu.frame(&mut renderer, &[&scene], &camera, (512, 512))?;
        assert!(
            frame
                .chunks_exact(4)
                .filter(|p| p[0] > 0 || p[1] > 0 || p[2] > 0)
                .count()
                > 1000
        );
        if let Some(output) = &fixture.artifacts {
            image::save_buffer(
                output.join(format!("layers-{i}.png")),
                &frame,
                512,
                512,
                image::ColorType::Rgba8,
            )?;
        }
        frames.push(frame);
    }
    assert_ne!(frames[0], frames[1]);
    assert_ne!(frames[1], frames[2]);
    assert!(scene.update_vertices(&gpu.queue, &[], &[]).is_err());
    let mut invalid = data[2].vertices.clone();
    invalid[0].position[0] = f32::NAN;
    assert!(
        scene
            .update_vertices(
                &gpu.queue,
                &invalid,
                &data[2].batches.iter().map(|b| b.center).collect::<Vec<_>>()
            )
            .is_err()
    );
    let after = gpu.frame(&mut renderer, &[&scene], &camera, (512, 512))?;
    assert_eq!(
        after, frames[2],
        "Rejected dynamic update changed the GPU scene"
    );
    let Some(output) = &fixture.artifacts else {
        return Ok(());
    };
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "sequences":rig.sequences.len(),"single_clip_samples":sampled,"combined_frames":3,"gpu_uploads":1,"vertices":scene.vertex_count,
            "adapter":gpu.adapter,"window_launched":false,"os_input_used":false,
            "lower_body_preserved_by_tool_pose":true,"rejected_update_preserves_frame":true,
            "omissions":["Diagnostic paint only; original textures/face/decal are not bound", "No App/network avatar integration or gameplay animation selection yet"]
        }))?,
    )?;
    Ok(())
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
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    ..Default::default()
                })
                .await?;
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await?;
            Ok(Self {
                device,
                queue,
                adapter: adapter.get_info().name,
            })
        })
    }
    fn frame(
        &self,
        renderer: &mut SceneRenderer,
        scenes: &[&GpuScene],
        camera: &Camera,
        size: (u32, u32),
    ) -> Result<Vec<u8>> {
        self.frame_format(
            renderer,
            scenes,
            camera,
            size,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )
    }
    fn frame_format(
        &self,
        renderer: &mut SceneRenderer,
        scenes: &[&GpuScene],
        camera: &Camera,
        size: (u32, u32),
        format: wgpu::TextureFormat,
    ) -> Result<Vec<u8>> {
        self.frame_instances(renderer, scenes, &[], camera, size, format)
    }
    #[allow(clippy::too_many_arguments)]
    fn frame_instances(
        &self,
        renderer: &mut SceneRenderer,
        scenes: &[&GpuScene],
        instances: &[(&GpuScene, &GpuInstances)],
        camera: &Camera,
        size: (u32, u32),
        format: wgpu::TextureFormat,
    ) -> Result<Vec<u8>> {
        let (width, height) = size;
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene test target"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = create_depth(&self.device, width, height);
        let row = (width * 4).div_ceil(256) * 256;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene test readback"),
            size: u64::from(row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        renderer.update_camera(&self.queue, camera);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        renderer.render_with_instances(
            &mut encoder,
            &target.create_view(&Default::default()),
            &depth.create_view(&Default::default()),
            scenes,
            instances,
            Some(wgpu::Color::BLACK),
        );
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            extent,
        );
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })?;
        rx.recv_timeout(std::time::Duration::from_secs(30))??;
        let mapped = readback.slice(..).get_mapped_range()?;
        Ok(mapped
            .chunks_exact(row as usize)
            .flat_map(|r| r[..width as usize * 4].iter().copied())
            .collect())
    }
}

#[test]
fn shared_instances_match_cpu_geometry_fade_sorting_and_atomic_updates() -> Result<()> {
    let gpu = Gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut data = triangle([1.; 4], 0., AlphaMode::Opaque);
    data.materials[0] = Material::vertex_lit("shared", 0);
    let base = renderer.upload(&gpu.device, &gpu.queue, &data)?;
    let mut instances = GpuInstances::new(&gpu.device, 4)?;
    let transforms = [
        SceneTransform {
            transform: Mat4::from_scale_rotation_translation(
                Vec3::new(0.6, 0.7, 0.8),
                glam::Quat::from_rotation_y(0.25),
                Vec3::new(0.05, 0., 0.2),
            ),
            tint: [1., 0., 0., 0.5],
        },
        SceneTransform {
            transform: Mat4::from_scale_rotation_translation(
                Vec3::new(0.7, 0.6, 0.9),
                glam::Quat::from_rotation_y(-0.2),
                Vec3::new(-0.05, 0., 0.7),
            ),
            tint: [0., 0., 1., 0.5],
        },
    ];
    let bake = |t: SceneTransform| {
        let mut posed = data.clone();
        for vertex in &mut posed.vertices {
            vertex.position = t
                .transform
                .transform_point3(Vec3::from(vertex.position))
                .to_array();
            vertex.normal = t
                .transform
                .inverse()
                .transpose()
                .transform_vector3(Vec3::from(vertex.normal))
                .to_array();
            vertex.color = std::array::from_fn(|i| vertex.color[i] * t.tint[i]);
        }
        for batch in &mut posed.batches {
            batch.center = t
                .transform
                .transform_point3(Vec3::from(batch.center))
                .to_array();
        }
        if t.tint[3] < 1. {
            posed.materials[0].alpha = AlphaMode::Blend;
        }
        posed
    };
    let a = renderer.upload(&gpu.device, &gpu.queue, &bake(transforms[0]))?;
    let b = renderer.upload(&gpu.device, &gpu.queue, &bake(transforms[1]))?;
    let camera = Camera {
        eye: [0., 0., -2., 1.],
        ..Default::default()
    };
    assert!(instances.update(&gpu.queue, &transforms)?);
    assert!(!instances.update(&gpu.queue, &transforms)?);
    let expected = gpu.frame(&mut renderer, &[&a, &b], &camera, (64, 64))?;
    let actual = gpu.frame_instances(
        &mut renderer,
        &[],
        &[(&base, &instances)],
        &camera,
        (64, 64),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )?;
    assert!(
        actual
            .iter()
            .zip(&expected)
            .all(|(a, b)| a.abs_diff(*b) <= 1),
        "Shared transform/fade/sort differs from independently transformed geometry"
    );
    assert!(actual.chunks_exact(4).any(|p| p[0] > 20 && p[2] > 20));
    let mut invalid = transforms;
    invalid[1].transform = Mat4::ZERO;
    assert!(instances.update(&gpu.queue, &invalid).is_err());
    assert!(instances.update(&gpu.queue, &[transforms[0]; 5]).is_err());
    assert_eq!(instances.len(), 2);
    assert_eq!(
        actual,
        gpu.frame_instances(
            &mut renderer,
            &[],
            &[(&base, &instances)],
            &camera,
            (64, 64),
            wgpu::TextureFormat::Rgba8UnormSrgb
        )?
    );
    assert!(instances.update(&gpu.queue, &[])?);
    assert!(instances.is_empty());
    let empty = gpu.frame_instances(
        &mut renderer,
        &[],
        &[(&base, &instances)],
        &camera,
        (64, 64),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )?;
    assert!(empty.chunks_exact(4).all(|p| p[..3] == [0, 0, 0]));
    // Fully opaque instances use a batched instanced draw. The independent
    // geometry oracle covers depth and correct nonuniform normal transformation.
    let opaque = transforms.map(|mut t| {
        t.tint[3] = 1.;
        t
    });
    instances.update(&gpu.queue, &opaque)?;
    let a = renderer.upload(&gpu.device, &gpu.queue, &bake(opaque[0]))?;
    let b = renderer.upload(&gpu.device, &gpu.queue, &bake(opaque[1]))?;
    let expected = gpu.frame(&mut renderer, &[&a, &b], &camera, (64, 64))?;
    let actual = gpu.frame_instances(
        &mut renderer,
        &[],
        &[(&base, &instances)],
        &camera,
        (64, 64),
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )?;
    assert!(
        actual
            .iter()
            .zip(&expected)
            .all(|(a, b)| a.abs_diff(*b) <= 1)
    );
    Ok(())
}

#[test]
fn posed_geometry_shares_bindings_and_rejects_foreign_materials_or_pixels() -> Result<()> {
    let gpu = Gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut data = triangle([0., 1., 0., 1.], 0.3, AlphaMode::Opaque);
    let base = renderer.upload(&gpu.device, &gpu.queue, &data)?;
    for v in &mut data.vertices {
        v.position[0] *= 0.5;
    }
    let shared = renderer.upload_geometry_shared(&gpu.device, &data, &base)?;
    let separate = renderer.upload(&gpu.device, &gpu.queue, &data)?;
    assert_eq!(
        gpu.frame(&mut renderer, &[&shared], &Camera::default(), (64, 64))?,
        gpu.frame(&mut renderer, &[&separate], &Camera::default(), (64, 64))?
    );
    data.materials[0].double_sided = true;
    assert!(
        renderer
            .upload_geometry_shared(&gpu.device, &data, &base)
            .is_err()
    );
    data.materials[0].double_sided = false;
    data.images[0].rgba[0] = 0;
    assert!(
        renderer
            .upload_geometry_shared(&gpu.device, &data, &base)
            .is_err()
    );
    Ok(())
}

#[test]
fn invalid_scene_references_and_vertical_cameras() {
    let mut data = triangle([1.0; 4], 0.5, AlphaMode::Opaque);
    data.validate().unwrap();
    data.indices.push(999);
    assert!(data.validate().is_err());
    data.indices.pop();
    data.materials[0].images[0] = 99;
    assert!(data.validate().is_err());
    for target in [[0.0, 5.0, 0.0], [0.0, -5.0, 0.0], [0.0, 0.0, 0.0]] {
        let camera = Camera::perspective([0.0; 3], target, 1.0, 1.2, 0.1, 100.0);
        assert!(camera.view_projection.iter().all(|v| v.is_finite()));
    }
}

#[test]
fn brick_overlay_coverage_preserves_paint_opacity_and_display_space() -> Result<()> {
    let gpu = Gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let camera = Camera {
        ambient: [1.0, 1.0, 1.0, 0.0],
        sun_color: [0.0; 4],
        ..Default::default()
    };
    for coverage in [0u8, 46, 255] {
        let mut data = triangle([0.8, 0.2, 0.1, 1.0], 0.3, AlphaMode::Opaque);
        data.images.push(SceneImage {
            label: "original-style overlay".into(),
            width: 1,
            height: 1,
            rgba: vec![32, 128, 224, coverage],
            srgb: false,
        });
        data.materials[0] = Material::brick_overlay("coverage", 1);
        let scene = renderer.upload(&gpu.device, &gpu.queue, &data)?;
        let pixels = gpu.frame(&mut renderer, &[&scene], &camera, (64, 64))?;
        let center = &pixels[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4];
        let blend = f32::from(coverage) / 255.0;
        for ((actual, paint), ink) in center[..3]
            .iter()
            .zip([0.8, 0.2, 0.1])
            .zip([32.0, 128.0, 224.0])
        {
            let expected = (paint * 255.0 * (1.0 - blend) + ink * blend).round() as i32;
            assert!(
                (i32::from(*actual) - expected).abs() <= 2,
                "coverage {coverage}: {center:?} expected {expected}"
            );
        }
        assert_eq!(
            center[3], 255,
            "overlay coverage must not make an opaque brick transparent"
        );
    }
    Ok(())
}

#[test]
fn persistent_gpu_camera_depth_alpha_and_resize() -> Result<()> {
    // Depth is reversed (`DEPTH_CLEAR`): nearer is larger.
    let gpu = Gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let red = renderer.upload(
        &gpu.device,
        &gpu.queue,
        &triangle([1.0, 0.0, 0.0, 1.0], 0.7, AlphaMode::Opaque),
    )?;
    let blue = renderer.upload(
        &gpu.device,
        &gpu.queue,
        &triangle([0.0, 0.0, 1.0, 1.0], 0.2, AlphaMode::Opaque),
    )?;
    let green = renderer.upload(
        &gpu.device,
        &gpu.queue,
        &triangle([0.0, 1.0, 0.0, 0.5], 0.9, AlphaMode::Blend),
    )?;
    let first = gpu.frame(
        &mut renderer,
        &[&green, &red, &blue],
        &Camera::default(),
        (64, 64),
    )?;
    let center = &first[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4];
    assert!(
        (180..=195).contains(&center[0]) && (180..=195).contains(&center[1]) && center[2] < 5,
        "Depth/blend color {center:?}"
    );
    let camera = Camera {
        view_projection: Mat4::from_translation(Vec3::new(0.8, 0.0, 0.0)).to_cols_array(),
        ..Default::default()
    };
    let second = gpu.frame(&mut renderer, &[&red], &camera, (64, 64))?;
    assert_ne!(first, second);
    assert_eq!(
        &second[(32 * 64 + 10) * 4..(32 * 64 + 10) * 4 + 3],
        &[0, 0, 0]
    );
    let resized = gpu.frame(&mut renderer, &[&red], &Camera::default(), (93, 71))?;
    assert_eq!(resized.len(), 93 * 71 * 4);
    assert_eq!(red.vertex_count, 3);
    assert_eq!(red.image_count, 1);
    Ok(())
}

#[test]
#[ignore = "requires generated v20 content"]
fn real_native_maps_upload_once_camera_motion() -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = root.join("artifacts/persistent-scene");
    std::fs::create_dir_all(&output)?;
    let gpu = Gpu::turn()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut reports = vec![];
    let bundle_path = root.join("content/map-bundle-017");
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle_path.join("bundle.json"))?)?;
    let maps = bundle["maps"].as_array().unwrap();
    assert_eq!(maps.len(), 14);
    for map in maps {
        let id = map["id"].as_str().unwrap();
        let name = id.rsplit('/').next().unwrap().strip_suffix(".mis").unwrap();
        let started = std::time::Instant::now();
        let map = bri_render::scene_loader::load_map_bundle(&bundle_path, id)?;
        let data = map.scene;
        assert_eq!(
            data.materials
                .iter()
                .filter(|m| m.kind == MaterialKind::Water)
                .count(),
            bundle["waters"][id].as_array().unwrap().len(),
            "Missing native water material"
        );
        let architectural = matches!(
            name,
            "bedroom" | "bedroomdark" | "kitchen" | "kitchendark" | "tutorial"
        );
        assert_eq!(
            map.terrain.len(),
            usize::from(architectural || name == "slopes")
        );
        assert!(
            map.terrain
                .iter()
                .all(|t| t.data.materials[0].kind == MaterialKind::Terrain)
        );
        if architectural {
            assert!(
                data.materials
                    .iter()
                    .any(|m| m.kind == MaterialKind::Surface)
            );
        }
        assert!(data.fog.end > data.fog.start);
        assert!(data.materials.iter().any(|m| m.kind == MaterialKind::Sky));
        assert!(
            !data
                .omissions
                .iter()
                .any(|s| s.contains("StaticModel") || s.contains("DatablockModel")),
            "Authored static model was omitted"
        );
        if matches!(name, "bedroom" | "bedroomdark" | "kitchen" | "kitchendark") {
            assert!(
                data.materials
                    .iter()
                    .any(|m| m.name.contains("sharp_trees")),
                "Trees lack material bindings"
            );
            // Torque: opaque DTS materials ignore texture alpha (frond stems).
            assert!(
                data.materials.iter().any(|m| m.name.contains("sharp_trees")
                    && m.alpha == AlphaMode::Opaque
                    && m.ignore_texture_alpha),
                "Opaque tree materials still alpha-test"
            );
        }
        assert!(
            !data
                .omissions
                .iter()
                .any(|s| s.contains("environment binding missing"))
        );
        let (target, offset) = match name {
            "bedroom" | "bedroomdark" => ([0.0, 335.0, 165.0], [10.0, 45.0, -15.0]),
            "kitchen" | "kitchendark" => ([-420.0, 175.0, 80.0], [25.0, 55.0, -10.0]),
            "slopes" => ([-50.0, 400.0, -250.0], [0.0, 15.0, 0.0]),
            _ => (
                (Vec3::from(data.spawn) + Vec3::new(0.0, 15.0, -100.0)).to_array(),
                [0.0, 4.0, 0.0],
            ),
        };
        // Far outside the old finite patch: repeated terrain must still stream in.
        let far_eye = map.terrain.first().map(|t| {
            let (x, z) = (data.spawn[0] + 6100.0, data.spawn[2] - 6100.0);
            Vec3::new(x, t.field.height(x, z).unwrap_or(0.0) + 4.0, z)
        });
        let scene = renderer.upload(&gpu.device, &gpu.queue, &data)?;
        let mut terrain = map
            .terrain
            .into_iter()
            .map(|t| GpuTerrain::upload(&renderer, &gpu.device, &gpu.queue, t.into(), 7000.0))
            .collect::<Result<Vec<_>>>()?;
        let upload_ms = started.elapsed().as_millis();
        let eye = Vec3::from(data.spawn) + Vec3::from(offset);
        let mut frames = vec![];
        let mut terrain_instances = 0;
        for (i, shift) in [0.0, 12.0].into_iter().enumerate() {
            let mut camera = Camera::perspective(
                (eye + Vec3::X * shift).to_array(),
                target,
                4.0 / 3.0,
                80_f32.to_radians(),
                0.05,
                7000.0,
            );
            camera.apply_environment(&data);
            let radius = if camera.atmosphere[3] > 0. {
                camera.atmosphere[1]
            } else {
                7000.
            };
            for t in &mut terrain {
                t.update(&gpu.device, &gpu.queue, &[eye + Vec3::X * shift], radius)?;
            }
            let draws: Vec<_> = terrain.iter().flat_map(GpuTerrain::draws).collect();
            terrain_instances = draws.iter().map(|(_, i)| i.len()).sum::<usize>();
            let frame = gpu.frame_instances(
                &mut renderer,
                &[&scene],
                &draws,
                &camera,
                (640, 480),
                wgpu::TextureFormat::Rgba8UnormSrgb,
            )?;
            let occupied = frame
                .chunks_exact(4)
                .filter(|p| p[0] != 0 || p[1] != 0 || p[2] != 0)
                .count();
            assert!(
                name == "destruct" || occupied > 640 * 480 / 5,
                "Empty or nearly empty native scene {name}"
            );
            image::save_buffer(
                output.join(format!("{name}-{i}.png")),
                &frame,
                640,
                480,
                image::ColorType::Rgba8,
            )?;
            frames.push(frame);
        }
        if let Some(far) = far_eye {
            let mut camera = Camera::perspective(
                far.to_array(),
                (far + Vec3::new(40.0, -6.0, 0.0)).to_array(),
                4.0 / 3.0,
                80_f32.to_radians(),
                0.05,
                7000.0,
            );
            camera.apply_environment(&data);
            for t in &mut terrain {
                t.update(
                    &gpu.device,
                    &gpu.queue,
                    &[far],
                    camera.atmosphere[1].max(1.),
                )?;
            }
            let draws: Vec<_> = terrain.iter().flat_map(GpuTerrain::draws).collect();
            assert!(draws.iter().any(|(_, i)| !i.is_empty()));
            let frame = gpu.frame_instances(
                &mut renderer,
                &[&scene],
                &draws,
                &camera,
                (640, 480),
                wgpu::TextureFormat::Rgba8UnormSrgb,
            )?;
            image::save_buffer(
                output.join(format!("{name}-far.png")),
                &frame,
                640,
                480,
                image::ColorType::Rgba8,
            )?;
            let bare = gpu.frame(&mut renderer, &[&scene], &camera, (640, 480))?;
            let covered = frame
                .chunks_exact(4)
                .zip(bare.chunks_exact(4))
                .filter(|(a, b)| a != b)
                .count();
            assert!(
                covered > 640 * 480 / 4,
                "Far repeated terrain did not render: {covered} pixels"
            );
        }
        if name == "destruct" {
            assert!(
                frames[0].chunks_exact(4).all(|p| p[..3] == [0, 0, 0]),
                "Destruct's empty authored sky must stay black"
            );
        } else if name != "construct" {
            assert_ne!(frames[0], frames[1], "Camera uniform did not move {name}");
        }
        reports.push(serde_json::json!({"map":data.id,"name":data.name,"vertices":scene.vertex_count,"triangles":scene.index_count/3,"images":scene.image_count,"batches":data.batches.len(),"terrain_tile_instances":terrain_instances,"load_upload_ms":upload_ms,"camera_frames":2,"gpu_scene_uploads":1,"omissions":data.omissions,"terrain_omissions":terrain.iter().flat_map(|t| t.omissions().to_vec()).collect::<Vec<_>>()}));
    }
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"adapter":gpu.adapter,"same_device_all_maps":true,"maps":reports}),
        )?,
    )?;
    Ok(())
}

#[test]
fn display_colors_match_on_srgb_and_unorm_output_attachments() -> Result<()> {
    let gpu = Gpu::turn()?;
    let data = triangle([0.5, 0.25, 0.75, 1.0], 0.5, AlphaMode::Opaque);
    let mut images = Vec::new();
    for format in [
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Rgba8Unorm,
    ] {
        let mut renderer = SceneRenderer::new(&gpu.device, format);
        let scene = renderer.upload(&gpu.device, &gpu.queue, &data)?;
        images.push(gpu.frame_format(
            &mut renderer,
            &[&scene],
            &Camera::default(),
            (64, 64),
            format,
        )?);
    }
    let center = (32 * 64 + 32) * 4;
    for image in &images {
        for (actual, expected) in image[center..center + 3].iter().zip([128u8, 64, 191]) {
            assert!(
                actual.abs_diff(expected) <= 1,
                "Wrong output transfer: {actual} != {expected}"
            );
        }
    }
    assert!(
        images[0]
            .iter()
            .zip(&images[1])
            .all(|(a, b)| a.abs_diff(*b) <= 1)
    );
    Ok(())
}
