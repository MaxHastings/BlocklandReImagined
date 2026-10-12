//! A body imported from glTF (`bri-import-addon gltf`) loads as an Add-On's
//! own body: every object of its own draws with its textures untinted, it
//! holds items at its hand socket and sees from its eye socket, and its
//! clips pose it. No window, GPU or gameplay input.
use anyhow::{Context, Result, ensure};
use bri_addon_import::gltf::{self, Kind};
use bri_client::avatar::AvatarAnimationInput;
use bri_package::packages::PackageSet;
use glam::Vec3;

const MODEL: &str = "figure:asset/figure.glb";

fn player() -> Result<bri_sim::player::PlayerState> {
    Ok(serde_json::from_value(serde_json::json!({
        "owner": 1, "feet": [0.0, 0.0, 0.0], "velocity": [0.0, 0.0, 0.0],
        "yaw": 0.0, "pitch": 0.0, "grounded": true, "crouched": false,
        "jetting": false, "scale": 1.0
    }))?)
}

#[test]
fn an_imported_gltf_body_draws_untinted_holds_and_animates() -> Result<()> {
    let root = bri_client::testing::ScratchDir::new("gltf-body")?;
    let source = root.path().join("figure.glb");
    std::fs::write(&source, gltf::testing::character(true).glb)?;
    let mut opts = gltf::Options::new(&source, root.path().join("addons/figure"), Kind::Body);
    opts.height = Some(1.6);
    let report = gltf::import(&opts)?;
    ensure!(report.model == MODEL, "{}", report.model);

    let mut assets = bri_client::testing::avatar::assets()?;
    let packages: PackageSet = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "packages": [{ "id": "figure", "version": "1.0.0", "side": "shared", "dir": "addons/figure" }]
    }))?;
    assets.load_bodies(root.path(), &packages);
    ensure!(assets.has_body(MODEL), "the imported body loaded");

    let mut appearance = assets.package.defaults.clone();
    appearance
        .colors
        .insert("torso".into(), [0.1, 0.2, 0.3, 1.0]);
    let mut mesh = assets.body_mesh(MODEL, appearance)?;
    let player = player()?;
    mesh.pose(&assets, &player, 0.0)?;
    // Its objects are named `Body` and `Crown`, no Blockhead part: both draw,
    // white (untinted), whatever colours the player picked.
    ensure!(
        mesh.data.vertices.iter().all(|v| v.color == [1.0; 4]),
        "untinted: {:?}",
        mesh.data.vertices.first().map(|v| v.color)
    );
    let drawn = mesh.data.indices.len() / 3;
    ensure!(
        drawn == report.triangles,
        "all {} triangles draw, not {drawn}",
        report.triangles
    );
    let top = mesh
        .data
        .vertices
        .iter()
        .map(|v| v.position[1])
        .fold(f32::MIN, f32::max);
    ensure!(
        (top - 1.6).abs() < 0.01,
        "imported at 1.6 units tall, not {top}"
    );

    // Held items go to its hand socket, level with the body; it sees from
    // its eye socket.
    let hand = mesh.mount_node(&assets, 0).context("Mount0")?;
    let (_, rotation, at) = hand.to_scale_rotation_translation();
    ensure!(
        rotation.dot(glam::Quat::IDENTITY).abs() > 0.999,
        "level grip"
    );
    ensure!(
        at.x > 0.1 && at.y > 0.5 && at.y < 1.2,
        "the right hand (+X facing -Z): {at}"
    );
    let eye = mesh
        .animated_world_node(&assets, "Eye")
        .context("Eye")?
        .w_axis
        .truncate();
    ensure!(
        eye.y > 1.3 && eye.z < 0.0,
        "eye in front of the head: {eye}"
    );

    // Running swings the arm that holds things.
    let mut moving = player.clone();
    moving.velocity = [0.0, 0.0, -8.0];
    let mut swung = Vec::new();
    for time in [0.0, 0.25, 0.5] {
        mesh.pose_with_animation(&assets, &moving, time, &AvatarAnimationInput::default())?;
        swung.push(
            mesh.mount_node(&assets, 0)
                .context("Mount0")?
                .w_axis
                .truncate(),
        );
    }
    let travel = swung
        .windows(2)
        .map(|w| w[0].distance(w[1]))
        .fold(0.0, f32::max);
    ensure!(
        travel > 0.05,
        "the run clip moves the hand: {swung:?} ({})",
        swung[0].distance(swung[2])
    );

    // Seated, it plays its own `sit` clip: the hips drop, and the hand
    // with them, still holding.
    let seated = AvatarAnimationInput {
        sitting: true,
        ..Default::default()
    };
    for time in [1.0, 1.5] {
        mesh.pose_with_animation(&assets, &player, time, &seated)?;
    }
    let sat = mesh
        .mount_node(&assets, 0)
        .context("Mount0 seated")?
        .w_axis
        .truncate();
    ensure!(
        sat.y < at.y - 0.1,
        "seated the hand is lower: {sat} vs {at}"
    );
    Ok(())
}

mod support;

/// Draw a posed body offscreen from in front and from its right, side by
/// side, `height` the body's height.
fn render(mesh: &mut bri_client::avatar::AvatarMesh, height: f32) -> Result<image::RgbaImage> {
    use bri_render::scene::{Camera, SceneRenderer, create_depth};
    const CELL: (u32, u32) = (240, 320);
    let gpu = support::gpu::turn()?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = SceneRenderer::new(&gpu.device, format);
    mesh.upload(&renderer, &gpu.device, &gpu.queue)?;
    let mut sheet = image::RgbaImage::new(CELL.0 * 2, CELL.1);
    for (column, from) in [Vec3::new(0.0, 0.0, -1.0), Vec3::new(1.0, 0.0, 0.0)]
        .into_iter()
        .enumerate()
    {
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gltf body"),
            size: wgpu::Extent3d {
                width: CELL.0,
                height: CELL.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = create_depth(&gpu.device, CELL.0, CELL.1);
        let target = Vec3::new(0.0, height * 0.5, 0.0);
        let eye = target + from * height * 2.6 + Vec3::Y * height * 0.2;
        let mut camera = Camera::perspective(
            eye.to_array(),
            target.to_array(),
            CELL.0 as f32 / CELL.1 as f32,
            35_f32.to_radians(),
            0.05,
            50.0,
        );
        camera.sun_direction = [0.4, 0.8, -0.45, 0.0];
        camera.sun_color = [1.0, 1.0, 1.0, 0.0];
        camera.ambient = [0.5, 0.5, 0.5, 0.0];
        renderer.update_camera(&gpu.queue, &camera);
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        renderer.render(
            &mut encoder,
            &texture.create_view(&Default::default()),
            &depth.create_view(&Default::default()),
            &[mesh.gpu.as_ref().context("uploaded body")?],
            Some(wgpu::Color {
                r: 0.55,
                g: 0.7,
                b: 0.9,
                a: 1.0,
            }),
        );
        let row = (CELL.0 * 4).div_ceil(256) * 256;
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gltf body readback"),
            size: u64::from(row) * u64::from(CELL.1),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(CELL.1),
                },
            },
            wgpu::Extent3d {
                width: CELL.0,
                height: CELL.1,
                depth_or_array_layers: 1,
            },
        );
        gpu.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        support::gpu::wait(&gpu.device, "the frame")?;
        rx.recv_timeout(std::time::Duration::from_secs(5))??;
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
        let mut pixels = Vec::with_capacity((CELL.0 * CELL.1 * 4) as usize);
        for line in mapped.chunks_exact(row as usize) {
            pixels.extend_from_slice(&line[..CELL.0 as usize * 4]);
        }
        drop(mapped);
        buffer.unmap();
        let cell = image::RgbaImage::from_raw(CELL.0, CELL.1, pixels).context("pixels")?;
        image::imageops::overlay(&mut sheet, &cell, i64::from(column as u32 * CELL.0), 0);
    }
    Ok(sheet)
}

fn load(
    glb: &[u8],
    height: f32,
    name: &str,
) -> Result<(
    bri_client::avatar::AvatarAssets,
    String,
    bri_client::testing::ScratchDir,
)> {
    let root = bri_client::testing::ScratchDir::new(name)?;
    let source = root.path().join(format!("{name}.glb"));
    std::fs::write(&source, glb)?;
    let mut opts = gltf::Options::new(
        &source,
        root.path().join(format!("addons/{name}")),
        Kind::Body,
    );
    opts.height = Some(height);
    let report = gltf::import(&opts)?;
    let mut assets = bri_client::testing::avatar::assets()?;
    let packages: PackageSet = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "packages": [{ "id": report.id, "version": "1.0.0", "side": "shared", "dir": format!("addons/{name}") }]
    }))?;
    assets.load_bodies(root.path(), &packages);
    ensure!(
        assets.has_body(&report.model),
        "{} did not load",
        report.model
    );
    Ok((assets, report.model, root))
}

#[test]
fn an_imported_gltf_body_renders_with_its_own_textures() -> Result<()> {
    let (assets, model, _root) = load(&gltf::testing::character(true).glb, 2.25, "figure")?;
    let mut mesh = assets.body_mesh(&model, assets.package.defaults.clone())?;
    mesh.pose(&assets, &player()?, 0.0)?;
    let sheet = render(&mut mesh, 2.25)?;
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("gltf-body");
    std::fs::create_dir_all(&out)?;
    sheet.save(out.join("figure.png"))?;
    // The skin texture's red and green squares show, lit, on the body.
    let count = |test: &dyn Fn([u8; 4]) -> bool| sheet.pixels().filter(|p| test(p.0)).count();
    let red = count(&|[r, g, b, _]| r > 120 && g < 60 && b < 60);
    let green = count(&|[r, g, b, _]| g > 120 && r < 60 && b < 60);
    ensure!(
        red > 200 && green > 200,
        "texture squares drawn: {red} red, {green} green pixels"
    );
    Ok(())
}

/// A real Blender-made model through the whole path, for a look: set
/// BRI_GLTF to a `.glb` (the Khronos CesiumMan, say) and find the picture
/// in the test's target folder.
#[test]
#[ignore = "set BRI_GLTF to a .glb to render it"]
fn a_gltf_model_from_disk_renders() -> Result<()> {
    let path = std::path::PathBuf::from(std::env::var_os("BRI_GLTF").context("set BRI_GLTF")?);
    let (assets, model, _root) = load(&std::fs::read(&path)?, 2.65, "sample")?;
    let mut mesh = assets.body_mesh(&model, assets.package.defaults.clone())?;
    mesh.pose(&assets, &player()?, 0.0)?;
    let sheet = render(&mut mesh, 2.65)?;
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("gltf-body");
    std::fs::create_dir_all(&out)?;
    let file = out.join(format!(
        "{}.png",
        path.file_stem().unwrap_or_default().to_string_lossy()
    ));
    sheet.save(&file)?;
    println!("rendered {}", file.display());
    Ok(())
}
