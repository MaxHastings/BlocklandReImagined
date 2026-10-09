//! A bounded mixed-scene check: native character/car plus Glow, glass and
//! a particle sprite, through the same opaque/AO/blended/effects ordering.
#[path = "../../render/tests/unified_lighting/common.rs"]
mod geometry;
mod support;
use anyhow::{Context, Result, ensure};
use bri_client::vehicles::{ClientVehicles, VehicleAssets};
use bri_fx_runtime::{
    BlendMode, EffectsPack, FrameEffects, Manifest, ParticleInstance, gpu::EffectsRenderer,
    pack::TextureImage,
};
use bri_render::{ambient_occlusion::AmbientOcclusion, scene::*};
use bri_sim::{
    player::PlayerState,
    session::{VehicleInfo, VehiclePose},
};
use glam::{Mat4, Quat, Vec3, Vec4};
use std::collections::BTreeMap;
use support::{avatar_fixture::AvatarFixture, gpu, vehicle_fixture::VehicleFixture};
const SIZE: (u32, u32) = (640, 480);
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn particle_pack() -> std::sync::Arc<EffectsPack> {
    EffectsPack::from_parts(
        bri_content::effects::Library {
            schema_version: 1,
            lights: vec![],
            particles: vec![],
            emitters: vec![],
            textures: BTreeMap::from([("pixel".into(), "pixel.png".into())]),
        },
        Manifest {
            schema_version: 1,
            library_sha256: String::new(),
            textures: BTreeMap::new(),
            emitter_alpha: BTreeMap::new(),
            bindings: vec![],
            composites: vec![],
            unresolved: vec![],
        },
        vec![TextureImage {
            id: "pixel".into(),
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        }],
    )
    .unwrap()
}
#[test]
fn synthetic_mixed_shading_scene() -> Result<()> {
    mixed_scene(AvatarFixture::synthetic()?, VehicleFixture::synthetic()?)
}
#[test]
#[ignore = "requires generated v20 content"]
fn native_mixed_shading_scene() -> Result<()> {
    mixed_scene(AvatarFixture::content()?, VehicleFixture::content()?)
}
fn mixed_scene(avatar: AvatarFixture, car: VehicleFixture) -> Result<()> {
    let gpu = gpu::turn()?;
    let out = avatar.out("sky-shading-mixed")?;
    let mut assets = VehicleAssets::load(&car.dir)?;
    let wheels = assets.definition(&car.car).context("car")?.wheels.len();
    let infos = BTreeMap::from([(
        1,
        VehicleInfo {
            id: 1,
            definition: car.car.clone(),
            color: Some([0.9, 0.1, 0.1, 1.]),
            occupants: vec![],
            destroyed: false,
            turret_broken: false,
            scale: 1.,
        },
    )]);
    let poses = BTreeMap::from([(
        1,
        VehiclePose {
            passage_frame: Default::default(),
            id: 1,
            tick: 1,
            position: [3., 0., 1.],
            rotation: Quat::from_rotation_y(0.6).to_array(),
            velocity: [0.; 3],
            steering: 0.3,
            wheel_suspension: vec![0.3; wheels],
            wheel_rotation: vec![0.; wheels],
            wheel_contact: vec![true; wheels],
            wheel_tire: vec![Default::default(); wheels],
            turret_aim: [0.; 2],
            jetting: false,
            angular_velocity: [0.; 3],
            mouse_steering: [0.; 2],
            driver_input: 0,
            driver_steering: (false, false),
            steering_quiet: 0,
            actor: None,
        },
    )]);
    let mut vehicles = ClientVehicles::default();
    vehicles.update(&infos, &poses, None, None, &Default::default());
    vehicles.prepare(&mut assets, &infos);
    let player = PlayerState {
        owner: 1,
        feet: [-4., 0., 1.],
        velocity: [0.; 3],
        yaw: 0.,
        pitch: 0.,
        head_yaw: 0.,
        grounded: true,
        crouched: false,
        jetting: false,
        jump: Default::default(),
        archetype: Default::default(),
        scale: 1.,
        energy: 100.,
        speed_scale: 1.,
        tick: Default::default(),
        tether: None,
    };
    let mut mesh = avatar.assets.mesh(avatar.assets.package.defaults.clone())?;
    mesh.pose(&avatar.assets, &player, 0.)?;
    let mut glow = geometry::cuboid(Vec3::new(-0.5, 0., -2.), Vec3::new(0.5, 2., -1.));
    for v in &mut glow.vertices {
        v.fx = BrickFx::new(3, 0)?.encode([0.; 3], 0, 1)?;
        v.color = [0., 1., 0., 1.];
    }
    let mut glass = geometry::cuboid(Vec3::new(1., 0., 4.), Vec3::new(5., 3., 4.1));
    glass.materials[0].kind = MaterialKind::Unlit;
    glass.materials[0].alpha = AlphaMode::Blend;
    for v in &mut glass.vertices {
        v.color = [0.1, 0.8, 1., 0.3];
    }
    let world = [
        geometry::cuboid(Vec3::new(-10., -0.5, -8.), Vec3::new(10., 0., 10.)),
        geometry::cuboid(Vec3::new(-10., 0., -8.), Vec3::new(10., 6., -7.5)),
        geometry::cuboid(Vec3::new(-5., 0., -4.), Vec3::new(-3., 2., -2.)),
        glow,
        glass,
    ];
    for samples in [1, 4] {
        let mut renderer = SceneRenderer::with_settings(&gpu.device, FORMAT, samples, None);
        ClientVehicles::upload(&mut assets, &renderer, &gpu.device, &gpu.queue)?;
        // Re-upload for this renderer's material/buffer pools.
        let character = renderer.upload(&gpu.device, &gpu.queue, &mesh.data)?;
        let uploaded = world
            .iter()
            .map(|s| renderer.upload(&gpu.device, &gpu.queue, s))
            .collect::<Result<Vec<_>>>()?;
        let scenes: Vec<_> = uploaded.iter().chain(std::iter::once(&character)).collect();
        let models = ClientVehicles::draws(&assets);
        let ao = AmbientOcclusion::new(&gpu.device, FORMAT, samples);
        let target = geometry::color_target(&gpu.device, FORMAT, SIZE.0, SIZE.1);
        let view = target.create_view(&Default::default());
        let multisampled = (samples > 1).then(|| {
            gpu.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("mixed scene MSAA"),
                    size: target.size(),
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format: FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        });
        let color = multisampled.as_ref().unwrap_or(&view);
        let depth = create_depth_samples(&gpu.device, SIZE.0, SIZE.1, samples)
            .create_view(&Default::default());
        let mut sprites = EffectsRenderer::new(
            &gpu.device,
            &gpu.queue,
            &particle_pack(),
            FORMAT,
            DEPTH_FORMAT,
            samples,
            16,
        )?;
        let eye = Vec3::new(0., 6., 16.);
        let toward = Vec3::new(0., 1., 0.);
        let right = (toward - eye).normalize().cross(Vec3::Y).normalize();
        let up = right.cross((toward - eye).normalize());
        for mode in [2., 3.] {
            let mut base =
                Camera::perspective(eye.to_array(), toward.to_array(), 4. / 3., 1., 0.05, 100.);
            base.ambient = [0.65, 0.65, 0.65, mode];
            base.sun_direction = [0.4, -1., -0.4, 0.];
            base.sun_color = [0.25, 0.25, 0.25, 0.];
            base.sky_bands = [[[0.3, 0.5, 0.9, 1.]; 8]; 16];
            let fx_camera = bri_fx_runtime::Camera {
                view_projection: Mat4::from_cols_array(&base.view_projection),
                position: eye,
                right,
                up,
            };
            sprites.prepare(
                &gpu.queue,
                &fx_camera,
                &FrameEffects {
                    particles: vec![ParticleInstance {
                        position: Vec3::new(-2., 3., 4.),
                        size: 1.,
                        color: Vec4::new(1., 0., 1., 1.),
                        spin: 0.,
                        axis: Vec3::ZERO,
                        texture: 0,
                        blend: BlendMode::Alpha,
                        depth_test: true,
                    }],
                    lights: vec![],
                },
            )?;
            let mut images = Vec::new();
            for (name, soft, occlude) in [
                ("off", false, false),
                ("soft", true, false),
                ("both", true, true),
            ] {
                let mut camera = base;
                camera.set_sky_ambient(soft);
                renderer.update_camera(&gpu.queue, &camera);
                let mut encoder = gpu.device.create_command_encoder(&Default::default());
                let late = |pass: &mut wgpu::RenderPass<'_>| sprites.render(pass);
                let pass = WorldPass {
                    view: 0,
                    color,
                    resolve: multisampled.as_ref().map(|_| &view),
                    depth: &depth,
                    viewport: None,
                    clear: Some(wgpu::Color {
                        r: 0.15,
                        g: 0.2,
                        b: 0.3,
                        a: 1.,
                    }),
                    after_opaque: None,
                    after_all: Some(&late),
                };
                if occlude {
                    let mut between = |encoder: &mut wgpu::CommandEncoder| {
                        ao.render(
                            &gpu.device,
                            &gpu.queue,
                            encoder,
                            color,
                            &depth,
                            SIZE,
                            camera.view_projection,
                            eye.to_array(),
                            (camera.atmosphere, camera.fog_color[3]),
                            (&renderer, &scenes, &models),
                        )
                    };
                    renderer.render_world_split(&mut encoder, pass, &scenes, &models, &mut between);
                } else {
                    renderer.render_world(&mut encoder, pass, &scenes, &models);
                }
                let row = SIZE.0 * 4;
                let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("mixed scene readback"),
                    size: u64::from(row * SIZE.1),
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
                            rows_per_image: Some(SIZE.1),
                        },
                    },
                    target.size(),
                );
                gpu.queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                    let _ = tx.send(r);
                });
                gpu::wait(&gpu.device, "mixed shading scene")?;
                rx.recv_timeout(std::time::Duration::from_secs(30))??;
                let pixels = buffer.slice(..).get_mapped_range()?.to_vec();
                image::save_buffer(
                    out.join(format!("mode-{}-msaa-{samples}-{name}.png", mode as u32)),
                    &pixels,
                    SIZE.0,
                    SIZE.1,
                    image::ColorType::Rgba8,
                )?;
                images.push(pixels);
            }
            let soft = &images[1];
            let both = &images[2];
            ensure!(
                soft.chunks_exact(4)
                    .zip(both.chunks_exact(4))
                    .filter(|(a, b)| a != b)
                    .count()
                    > 20,
                "AO must shade the mixed scene"
            );
            let magenta = |p: &[u8]| p[0] > 240 && p[1] < 10 && p[2] > 240;
            let sprite_pixels: Vec<_> = images[0]
                .chunks_exact(4)
                .enumerate()
                .filter(|(_, p)| magenta(p))
                .map(|(i, _)| i)
                .collect();
            ensure!(sprite_pixels.len() > 10, "particle must be visible");
            for i in sprite_pixels {
                ensure!(
                    images[0][i * 4..i * 4 + 4] == both[i * 4..i * 4 + 4],
                    "AO/shading changed the particle"
                );
            }
            let p =
                Mat4::from_cols_array(&base.view_projection).project_point3(Vec3::new(0., 1., -1.));
            let x = ((p.x * 0.5 + 0.5) * SIZE.0 as f32) as usize;
            let y = ((0.5 - p.y * 0.5) * SIZE.1 as f32) as usize;
            let i = (y * SIZE.0 as usize + x) * 4;
            ensure!(
                soft[i + 1] > 100 && soft[i + 1] > soft[i],
                "Glow must be visible"
            );
            ensure!(soft[i..i + 4] == both[i..i + 4], "AO changed Glow");
            let mut montage = image::RgbaImage::new(SIZE.0 * 3, SIZE.1);
            for (i, pixels) in images.into_iter().enumerate() {
                let frame = image::RgbaImage::from_raw(SIZE.0, SIZE.1, pixels).unwrap();
                image::imageops::replace(&mut montage, &frame, i as i64 * i64::from(SIZE.0), 0);
            }
            montage.save(out.join(format!("mode-{}-msaa-{samples}-montage.png", mode as u32)))?;
        }
    }
    println!("mixed scene evidence: {}", out.display());
    Ok(())
}
