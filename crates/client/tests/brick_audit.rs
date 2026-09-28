//! Brick visual audit scene: fixed camera and sun, one brick per family.
//! Writes our render plus a layout the independent v20 reference renderer
//! (`tools/brick_reference.py`) reads. Offscreen only, no window or input.
use anyhow::{Context, Result, ensure};
use bri_client::{materials::BrickMaterials, world_scene::build_world_scene_materials};
use bri_content::brick::Catalog;
use bri_net::protocol::PublicWorld;
use bri_render::scene::{Camera, SceneData, SceneRenderer, create_depth};
use bri_sim::definitions::Definitions;
use bri_ui::gpu::Headless;
use bri_world::{Brick, ContentRef};
use std::{collections::BTreeMap, path::Path};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;

fn render(gpu: &Headless, scene: &SceneData, camera: &Camera, path: &Path) -> Result<()> {
    let size = wgpu::Extent3d {
        width: WIDTH,
        height: HEIGHT,
        depth_or_array_layers: 1,
    };
    // The game presents to a non-sRGB surface (platform.rs), so blending
    // happens on display values exactly as in v20.
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("brick audit"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = create_depth(&gpu.device, WIDTH, HEIGHT);
    let mut renderer = SceneRenderer::new(&gpu.device, format);
    let uploaded = renderer.upload(&gpu.device, &gpu.queue, scene)?;
    renderer.update_camera(&gpu.queue, camera);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.render(
        &mut encoder,
        &target.create_view(&Default::default()),
        &depth.create_view(&Default::default()),
        &[&uploaded],
        Some(wgpu::Color {
            r: 0.3,
            g: 0.3,
            b: 0.3,
            a: 1.0,
        }),
    );
    let row = WIDTH * 4;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("brick audit readback"),
        size: u64::from(row) * u64::from(HEIGHT),
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
                rows_per_image: Some(HEIGHT),
            },
        },
        size,
    );
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(std::time::Duration::from_secs(30)),
    })?;
    rx.recv_timeout(std::time::Duration::from_secs(30))??;
    let pixels = readback.slice(..).get_mapped_range()?;
    image::save_buffer(path, &pixels, WIDTH, HEIGHT, image::ColorType::Rgba8)?;
    Ok(())
}

struct Entry {
    id: &'static str,
    x: f32,
    z: f32,
    color: u8,
    print: Option<&'static str>,
    color_fx: u8,
    shape_fx: u8,
    quarter_turns: u8,
}
const fn plain(id: &'static str, x: f32, z: f32, color: u8) -> Entry {
    Entry {
        id,
        x,
        z,
        color,
        print: None,
        color_fx: 0,
        shape_fx: 0,
        quarter_turns: 0,
    }
}
const fn turned(id: &'static str, x: f32, z: f32, color: u8, quarter_turns: u8) -> Entry {
    Entry {
        quarter_turns,
        ..plain(id, x, z, color)
    }
}
const fn printed(id: &'static str, x: f32, z: f32, print: &'static str) -> Entry {
    Entry {
        print: Some(print),
        ..plain(id, x, z, 2)
    }
}
const fn fx(x: f32, z: f32, color: u8, color_fx: u8, shape_fx: u8) -> Entry {
    Entry {
        color_fx,
        shape_fx,
        ..plain("v20/brick/brick2x4data", x, z, color)
    }
}
const FAMILIES: &[Entry] = &[
    plain("v20/brick/brick2x4data", -3.0, -2.0, 0),
    plain("v20/brick/brick4x4fdata", -1.0, -2.0, 1),
    plain("v20/brick/brick1x4fdata", 1.0, -2.25, 2),
    plain("v20/brick/brick2x2rampdata", 3.0, -2.0, 3),
    plain("v20/brick/brick2x2x5rampdata", 5.0, -2.0, 0),
    plain("v20/brick/brick2x2rampcornerdata", -3.0, 0.5, 1),
    plain("v20/brick/brick3x3rampcornerdata", -0.75, 0.25, 2),
    plain("v20/brick/brick2x2cresthighcornerdata", 1.5, 0.5, 3),
    plain("v20/brick/brick1x2rampupdata", 3.25, 0.5, 0),
    plain("v20/brick/brick2x2rounddata", 5.0, 0.5, 1),
    plain("v20/brick/brick2x2x2conedata", -3.0, 3.0, 2),
    plain("v20/brick/brick1x1rounddata", -1.25, 3.25, 3),
    printed("v20/brick/brick1x1printdata", 0.25, 3.25, "Letters/A"),
    printed("v20/brick/brick2x2fprintdata", 2.0, 3.0, "Letters/B"),
    printed("v20/brick/brick1x4x4printdata", 4.0, 3.25, "Letters/C"),
    plain("v20/brick/brick2x4data", 6.0, 3.0, 4),
];
/// Every colour FX (top row: pearl, chrome, glow, blink; middle: swirl,
/// rainbow, none) and both shape FX, frozen at 0.37 s.
const FX: &[Entry] = &[
    fx(-3.0, -2.0, 0, 1, 0),
    fx(-0.5, -2.0, 3, 2, 0),
    fx(2.0, -2.0, 1, 3, 0),
    fx(4.5, -2.0, 0, 4, 0),
    fx(-3.0, 1.0, 3, 5, 0),
    fx(-0.5, 1.0, 2, 6, 0),
    fx(2.0, 1.0, 1, 0, 0),
    fx(4.5, 1.0, 2, 0, 1),
    fx(-0.5, 4.0, 3, 0, 2),
    fx(2.0, 4.0, 0, 1, 2),
];

const EYE: [f32; 3] = [1.5, 7.5, 10.5];
const TARGET: [f32; 3] = [1.5, 0.0, 0.5];

fn audit_scene(
    name: &str,
    layout: &[Entry],
    time: f32,
    eye: [f32; 3],
    target: [f32; 3],
) -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let content = root.join("content");
    let packages = bri_package::packages::PackageSet::base();
    let materials = BrickMaterials::load(&packages.role_dir(&content, "brick_materials")?)?;
    let catalog: Catalog = serde_json::from_slice(&std::fs::read(
        packages
            .role_dir(&content, "brick_catalog")?
            .join("stock-catalog.json"),
    )?)?;
    let definitions = Definitions::load(
        &packages.role_dir(&content, "brick_catalog")?,
        &packages.role_dir(&content, "geometry")?,
    )?;
    let meshes: BTreeMap<String, bri_content::brick::Brick> = definitions
        .entries
        .into_iter()
        .map(|(id, def)| (id, def.mesh))
        .collect();
    // Palette entries mirror default v20 colours; the last is translucent.
    let palette = vec![
        [0.9, 0.0, 0.0, 1.0],
        [0.0, 0.5, 0.25, 1.0],
        [1.0, 1.0, 1.0, 1.0],
        [0.2, 0.0, 0.8, 1.0],
        [0.9, 0.9, 0.0, 0.5],
    ];
    let mut world = PublicWorld {
        name: "Brick audit".into(),
        map_id: "audit".into(),
        palette: palette.clone(),
        bricks: Default::default(),
    };
    let mut records = vec![];
    for (index, entry) in layout.iter().enumerate() {
        let catalog_entry = catalog
            .bricks
            .iter()
            .find(|b| b.id == entry.id)
            .with_context(|| format!("Missing catalog brick {}", entry.id))?;
        let mesh = &meshes[entry.id];
        let height = mesh.height_plates as f32 * 0.2;
        let mut brick = Brick::new(
            ContentRef::Resolved(entry.id.into()),
            [entry.x, height * 0.5, entry.z],
            1,
        );
        brick.color = entry.color;
        brick.quarter_turns = entry.quarter_turns;
        brick.color_effect = entry.color_fx;
        brick.shape_effect = entry.shape_fx;
        let print_path = entry.print.map(|alias| {
            let p = materials.bundle.resolve(alias).expect("print alias");
            brick.print = Some(ContentRef::Resolved(p.id.clone()));
            p.diffuse.path.clone()
        });
        records.push(serde_json::json!({
            "id": entry.id, "blb": catalog_entry.mesh_id, "position": brick.position,
            "quarter_turns": entry.quarter_turns, "paint": palette[entry.color as usize],
            "print": print_path, "color_fx": entry.color_fx, "shape_fx": entry.shape_fx,
            "depth_studs": mesh.footprint_studs[1],
        }));
        world.bricks.insert(index as u64 + 1, brick);
    }
    let scene = build_world_scene_materials(&world, &meshes, 4_000_000, Some(&materials))?;
    ensure!(!scene.indices.is_empty(), "Empty audit scene");
    let out = root.join("artifacts/brick-audit").join(name);
    std::fs::create_dir_all(&out)?;
    let fov = 45_f32;
    let mut camera = Camera::perspective(
        eye,
        target,
        WIDTH as f32 / HEIGHT as f32,
        fov.to_radians(),
        0.1,
        200.0,
    );
    camera.atmosphere[2] = time;
    let gpu = Headless::new()?;
    render(&gpu, &scene, &camera, &out.join("ours.png"))?;
    std::fs::write(
        out.join("layout.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "width": WIDTH, "height": HEIGHT, "eye": eye, "target": target,
            "fov_y_degrees": fov, "near": 0.1, "far": 200.0, "time_seconds": time,
            "sun_direction": &camera.sun_direction[..3], "sun_color": &camera.sun_color[..3],
            "ambient": &camera.ambient[..3], "background": [0.3, 0.3, 0.3],
            "materials": packages.role_dir(&content, "brick_materials")?,
            "bricks": records,
        }))?,
    )?;
    Ok(())
}

#[test]
#[ignore = "requires local converted original assets; offscreen only"]
fn brick_family_audit_scene() -> Result<()> {
    audit_scene("families", FAMILIES, 0.0, EYE, TARGET)
}

#[test]
#[ignore = "requires local converted original assets; offscreen only"]
fn brick_fx_audit_scene() -> Result<()> {
    audit_scene("fx", FX, 0.37, EYE, TARGET)
}

/// Stud tops seen from straight above, the way a builder looks down a well:
/// 1x1, 1x2 and 2x2 bricks packed together at every quarter turn.
const TOPS: &[Entry] = &[
    turned("v20/brick/brick1x1data", -1.25, -1.25, 1, 0),
    turned("v20/brick/brick1x1data", -0.75, -1.25, 1, 1),
    turned("v20/brick/brick1x1data", -1.25, -0.75, 1, 2),
    turned("v20/brick/brick1x1data", -0.75, -0.75, 1, 3),
    turned("v20/brick/brick2x2data", 0.0, -1.0, 0, 1),
    turned("v20/brick/brick2x2data", 1.0, -1.0, 0, 2),
    turned("v20/brick/brick2x2data", 2.0, -1.0, 0, 3),
    turned("v20/brick/brick2x2data", -1.0, 0.0, 2, 3),
    turned("v20/brick/brick2x2data", 0.0, 0.0, 2, 0),
    turned("v20/brick/brick1x2data", 0.75, 0.0, 2, 0),
    turned("v20/brick/brick1x2data", 1.25, 0.0, 2, 2),
    turned("v20/brick/brick2x2data", 2.0, 0.0, 2, 2),
    turned("v20/brick/brick2x2data", -1.0, 1.0, 3, 1),
    turned("v20/brick/brick2x2data", 0.0, 1.0, 3, 2),
    turned("v20/brick/brick2x2data", 1.0, 1.0, 3, 0),
    turned("v20/brick/brick2x2data", 2.0, 1.0, 3, 3),
];

#[test]
#[ignore = "requires local converted original assets; offscreen only"]
fn brick_top_audit_scene() -> Result<()> {
    audit_scene("tops", TOPS, 0.0, [0.5, 6.0, 1.0], [0.5, 0.0, 0.0])
}
