//! Native vehicle models (chassis, wheels, turret) rendered through the
//! client instancing path, offscreen. Each body runs on the made-up
//! vehicle catalog (`support::vehicle_fixture`) and again, ignored, on the
//! generated v20 pack.
#[macro_use]
mod support;

use anyhow::{Context, Result, ensure};
use bri_client::vehicles::{ClientVehicles, VehicleAssets};
use bri_render::scene::{Camera, SceneRenderer, create_depth};
use bri_sim::session::{VehicleInfo, VehiclePose};
use bri_ui::gpu::Headless;
use std::collections::BTreeMap;
use support::{gpu, vehicle_fixture::VehicleFixture};

synthetic_and_content!(
    VehicleFixture: vehicles_render_with_wheels_and_paint,
    riders_tilt_with_a_car_on_a_slope,
    every_explosion_debris_model_is_in_the_vehicle_pack,
);

const SIZE: u32 = 384;

fn render(
    gpu: &Headless,
    renderer: &mut SceneRenderer,
    assets: &bri_client::vehicles::VehicleAssets,
    camera: &Camera,
) -> Result<Vec<u8>> {
    let extent = wgpu::Extent3d {
        width: SIZE,
        height: SIZE,
        depth_or_array_layers: 1,
    };
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vehicle offscreen"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = create_depth(&gpu.device, SIZE, SIZE);
    renderer.update_camera(&gpu.queue, camera);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.render_with_instances(
        &mut encoder,
        &target.create_view(&Default::default()),
        &depth.create_view(&Default::default()),
        &[],
        &ClientVehicles::draws(assets),
        Some(wgpu::Color {
            r: 0.2,
            g: 0.3,
            b: 0.5,
            a: 1.0,
        }),
    );
    let row = SIZE * 4;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vehicle readback"),
        size: u64::from(row * SIZE),
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
                rows_per_image: Some(SIZE),
            },
        },
        extent,
    );
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    support::gpu::wait(&gpu.device, "the vehicle frame")?;
    rx.recv_timeout(std::time::Duration::from_secs(30))??;
    Ok(buffer.slice(..).get_mapped_range()?.to_vec())
}

fn vehicles_render_with_wheels_and_paint(f: &VehicleFixture) -> Result<()> {
    let mut assets = VehicleAssets::load(&f.dir)?;
    let gpu = gpu::turn().context("offscreen vehicle adapter")?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let out = &f.out;
    std::fs::create_dir_all(out)?;
    let mut report = serde_json::Map::new();
    // Horses are drawn by the avatar horse rig, not this vehicle path;
    // horse_riding_render covers them.
    for (definition, distance) in &f.drawn {
        let (definition, distance) = (definition.as_str(), *distance);
        let wheels = assets
            .definition(definition)
            .context("vehicle definition")?
            .wheels
            .len();
        let infos: BTreeMap<u64, VehicleInfo> = [(
            1,
            VehicleInfo {
                id: 1,
                definition: definition.into(),
                color: Some([0.9, 0.1, 0.1, 1.0]),
                occupants: vec![],
                destroyed: false,
                scale: 1.0,
            },
        )]
        .into();
        let poses: BTreeMap<u64, VehiclePose> = [(
            1,
            VehiclePose {
                passage_frame: Default::default(),
                id: 1,
                tick: 1,
                position: [0.0; 3],
                rotation: glam::Quat::from_rotation_y(0.6).to_array(),
                velocity: [0.0; 3],
                steering: 0.3,
                wheel_suspension: vec![0.3; wheels],
                wheel_rotation: vec![0.0; wheels],
                wheel_contact: vec![true; wheels],
                wheel_tire: vec![Default::default(); wheels],
                turret_aim: [0.4, 0.0],
                jetting: false,
                angular_velocity: [0.0; 3],
                mouse_steering: [0.0; 2],
                driver_input: 0,
                driver_steering: (false, false),
                steering_quiet: 0,
                actor: None,
            },
        )]
        .into();
        let mut vehicles = ClientVehicles::default();
        vehicles.update(&infos, &poses, None, None, &Default::default());
        vehicles.prepare(&mut assets, &infos);
        ClientVehicles::upload(&mut assets, &renderer, &gpu.device, &gpu.queue)?;
        let camera = Camera::perspective(
            [distance * 0.8, distance * 0.45, distance * 0.8],
            [0.0, 1.0, 0.0],
            1.0,
            50f32.to_radians(),
            0.05,
            200.0,
        );
        let image = render(&gpu, &mut renderer, &assets, &camera)?;
        let background = [
            (0.2f32.powf(1.0 / 2.2) * 255.0) as i32,
            (0.3f32.powf(1.0 / 2.2) * 255.0) as i32,
            (0.5f32.powf(1.0 / 2.2) * 255.0) as i32,
        ];
        let visible = image
            .chunks_exact(4)
            .filter(|p| (0..3).any(|i| (i32::from(p[i]) - background[i]).abs() > 12))
            .count();
        let name = definition
            .trim_start_matches("v20.vehicle.")
            .replace([':', '/'], "-");
        image::save_buffer(
            out.join(format!("{name}.png")),
            &image,
            SIZE,
            SIZE,
            image::ColorType::Rgba8,
        )?;
        report.insert(name, visible.into());
        ensure!(visible > 2000, "{definition} drew only {visible} pixels");
    }
    std::fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::Value::Object(report))?,
    )?;
    Ok(())
}

fn riders_tilt_with_a_car_on_a_slope(f: &VehicleFixture) -> Result<()> {
    let assets = VehicleAssets::load(&f.dir)?;
    let definition = f.car.as_str();
    let wheels = assets.definition(definition).context("car")?.wheels.len();
    let info = VehicleInfo {
        id: 1,
        definition: definition.into(),
        color: Some([0.9, 0.1, 0.1, 1.0]),
        occupants: vec![],
        destroyed: false,
        scale: 1.0,
    };
    // Nose up a 20 degree incline, heading 0.6 rad.
    let slope = glam::Quat::from_rotation_y(0.6) * glam::Quat::from_rotation_x(20f32.to_radians());
    let infos: BTreeMap<u64, VehicleInfo> = [(1, info.clone())].into();
    let poses: BTreeMap<u64, VehiclePose> = [(
        1,
        VehiclePose {
            passage_frame: Default::default(),
            id: 1,
            tick: 1,
            position: [0.0; 3],
            rotation: slope.to_array(),
            velocity: [0.0; 3],
            steering: 0.0,
            wheel_suspension: vec![0.3; wheels],
            wheel_rotation: vec![0.0; wheels],
            wheel_contact: vec![true; wheels],
            wheel_tire: vec![Default::default(); wheels],
            turret_aim: [0.0, 0.0],
            jetting: false,
            angular_velocity: [0.0; 3],
            mouse_steering: [0.0; 2],
            driver_input: 0,
            driver_steering: (false, false),
            steering_quiet: 0,
            actor: None,
        },
    )]
    .into();
    let mut vehicles = ClientVehicles::default();
    vehicles.update(&infos, &poses, None, None, &Default::default());
    for seat in 0..assets.definition(definition).unwrap().seats.len() {
        let (_, rotation) = vehicles
            .seat_transform(&assets, &info, seat)
            .context("seat")?;
        let up = rotation * glam::Vec3::Y;
        let vehicle_up = slope * glam::Vec3::Y;
        ensure!(
            up.dot(vehicle_up) > 0.999,
            "seat {seat} sits flush: {up} vs {vehicle_up}"
        );
        ensure!(up.y < 0.95, "seat {seat} tilts off vertical: {up}");
        // The facing yaw ignores the tilt: body yaw turns the other way
        // from a quaternion's turn about +Y.
        let (_, yaw) = vehicles.seat(&assets, &info, seat).context("seat")?;
        ensure!(seat != 0 || (yaw + 0.6).abs() < 0.01, "driver yaw {yaw}");
    }
    Ok(())
}

#[test]
#[ignore = "requires generated v20 content"]
fn the_stock_weapons_have_six_debris_explosions() -> Result<()> {
    let f = VehicleFixture::content()?;
    let debris = bri_weapons::debris::explosion_debris(&f.weapons);
    ensure!(
        debris.len() == 6,
        "stock debris explosions: {}",
        debris.len()
    );
    Ok(())
}

fn every_explosion_debris_model_is_in_the_vehicle_pack(f: &VehicleFixture) -> Result<()> {
    let assets = VehicleAssets::load(&f.dir)?;
    let debris = bri_weapons::debris::explosion_debris(&f.weapons);
    ensure!(!debris.is_empty(), "no debris explosions");
    for (explosion, spec) in debris {
        ensure!(
            assets.has_source_model(&spec.model),
            "{explosion} debris model {} is not converted",
            spec.model
        );
    }
    Ok(())
}
