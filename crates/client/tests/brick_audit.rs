//! Brick visual audit scene: fixed camera and sun, one brick per family.
//! Writes our render plus a layout the independent v20 reference renderer
//! (`tools/brick_reference.py`) reads. Offscreen only, no window or input.
//! Each scene is drawn from made-up bricks and materials
//! (`support::brick_fixture`) and again, ignored, from the generated v20
//! catalog, which is the layout the v20 reference renderer compares.
#[macro_use]
mod support;

use anyhow::{Context, Result, ensure};
use bri_client::world_scene::build_world_scene_materials;
use bri_content::brick::Catalog;
use bri_net::protocol::PublicWorld;
use bri_render::scene::{Camera, SceneData, SceneRenderer, create_depth};
use bri_sim::definitions::Definitions;
use bri_ui::gpu::Headless;
use bri_world::{Brick, ContentRef};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use support::{
    brick_fixture::{BrickFixture, block},
    files::repo_root,
    gpu,
};

const CONTENT_SIZE: [u32; 2] = [1280, 800];
// Synthetic audits exercise the same layouts and materials at the same aspect
// ratio, without the native reference comparison's image-detail requirement.
// Keep their fragment/readback work bounded on the software GPU used by CI.
const SYNTHETIC_SIZE: [u32; 2] = [640, 400];

fn render(
    gpu: &Headless,
    scene: &SceneData,
    camera: &Camera,
    [width, height]: [u32; 2],
    path: &Path,
) -> Result<()> {
    let size = wgpu::Extent3d {
        width,
        height,
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
    let depth = create_depth(&gpu.device, width, height);
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
    let row = width * 4;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("brick audit readback"),
        size: u64::from(row) * u64::from(height),
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
                rows_per_image: Some(height),
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
    image::save_buffer(path, &pixels, width, height, image::ColorType::Rgba8)?;
    Ok(())
}

#[derive(Clone)]
struct Entry {
    id: String,
    x: f32,
    z: f32,
    color: u8,
    print: Option<String>,
    color_fx: u8,
    shape_fx: u8,
    quarter_turns: u8,
}
fn plain(id: &str, x: f32, z: f32, color: u8) -> Entry {
    Entry {
        id: id.into(),
        x,
        z,
        color,
        print: None,
        color_fx: 0,
        shape_fx: 0,
        quarter_turns: 0,
    }
}
fn turned(id: &str, x: f32, z: f32, color: u8, quarter_turns: u8) -> Entry {
    Entry {
        quarter_turns,
        ..plain(id, x, z, color)
    }
}
fn printed(id: &str, x: f32, z: f32, print: &str) -> Entry {
    Entry {
        print: Some(print.into()),
        ..plain(id, x, z, 2)
    }
}
fn fx(id: &str, x: f32, z: f32, color: u8, color_fx: u8, shape_fx: u8) -> Entry {
    Entry {
        color_fx,
        shape_fx,
        ..plain(id, x, z, color)
    }
}

/// Every colour FX (top row: pearl, chrome, glow, blink; middle: swirl,
/// rainbow, none) and both shape FX on one brick, frozen at 0.37 s.
fn fx_layout(id: &str) -> Vec<Entry> {
    vec![
        fx(id, -3.0, -2.0, 0, 1, 0),
        fx(id, -0.5, -2.0, 3, 2, 0),
        fx(id, 2.0, -2.0, 1, 3, 0),
        fx(id, 4.5, -2.0, 0, 4, 0),
        fx(id, -3.0, 1.0, 3, 5, 0),
        fx(id, -0.5, 1.0, 2, 6, 0),
        fx(id, 2.0, 1.0, 1, 0, 0),
        fx(id, 4.5, 1.0, 2, 0, 1),
        fx(id, -0.5, 4.0, 3, 0, 2),
        fx(id, 2.0, 4.0, 0, 1, 2),
    ]
}

/// Stud tops seen from straight above, the way a builder looks down a well:
/// 1x1, 1x2 and 2x2 bricks packed together at every quarter turn.
fn tops_layout(one: &str, one_by_two: &str, two: &str) -> Vec<Entry> {
    vec![
        turned(one, -1.25, -1.25, 1, 0),
        turned(one, -0.75, -1.25, 1, 1),
        turned(one, -1.25, -0.75, 1, 2),
        turned(one, -0.75, -0.75, 1, 3),
        turned(two, 0.0, -1.0, 0, 1),
        turned(two, 1.0, -1.0, 0, 2),
        turned(two, 2.0, -1.0, 0, 3),
        turned(two, -1.0, 0.0, 2, 3),
        turned(two, 0.0, 0.0, 2, 0),
        turned(one_by_two, 0.75, 0.0, 2, 0),
        turned(one_by_two, 1.25, 0.0, 2, 2),
        turned(two, 2.0, 0.0, 2, 2),
        turned(two, -1.0, 1.0, 3, 1),
        turned(two, 0.0, 1.0, 3, 2),
        turned(two, 1.0, 1.0, 3, 0),
        turned(two, 2.0, 1.0, 3, 3),
    ]
}

/// Brick materials, meshes and the three audit layouts.
struct Audit {
    bricks: BrickFixture,
    /// Each brick's source mesh name, for the reference renderer.
    blb: BTreeMap<String, String>,
    families: Vec<Entry>,
    fx: Vec<Entry>,
    tops: Vec<Entry>,
    /// The materials folder the layout names, and where scenes are written.
    materials_dir: PathBuf,
    out: PathBuf,
}

impl Audit {
    fn content() -> Result<Self> {
        let root = repo_root();
        let content = root.join("content");
        let packages = bri_package::packages::PackageSet::base();
        let catalog: Catalog = serde_json::from_slice(&std::fs::read(
            packages
                .role_dir(&content, "brick_catalog")?
                .join("stock-catalog.json"),
        )?)?;
        let definitions = Definitions::load(
            &packages.role_dir(&content, "brick_catalog")?,
            &packages.role_dir(&content, "geometry")?,
        )?;
        let mut bricks = BrickFixture::content()?;
        bricks.meshes = definitions
            .entries
            .into_iter()
            .map(|(id, def)| (id, def.mesh))
            .collect();
        let v20 = |s: &str| format!("v20/brick/{s}data");
        let families = vec![
            plain(&v20("brick2x4"), -3.0, -2.0, 0),
            plain(&v20("brick4x4f"), -1.0, -2.0, 1),
            plain(&v20("brick1x4f"), 1.0, -2.25, 2),
            plain(&v20("brick2x2ramp"), 3.0, -2.0, 3),
            plain(&v20("brick2x2x5ramp"), 5.0, -2.0, 0),
            plain(&v20("brick2x2rampcorner"), -3.0, 0.5, 1),
            plain(&v20("brick3x3rampcorner"), -0.75, 0.25, 2),
            plain(&v20("brick2x2cresthighcorner"), 1.5, 0.5, 3),
            plain(&v20("brick1x2rampup"), 3.25, 0.5, 0),
            plain(&v20("brick2x2round"), 5.0, 0.5, 1),
            plain(&v20("brick2x2x2cone"), -3.0, 3.0, 2),
            plain(&v20("brick1x1round"), -1.25, 3.25, 3),
            printed(&v20("brick1x1print"), 0.25, 3.25, "Letters/A"),
            printed(&v20("brick2x2fprint"), 2.0, 3.0, "Letters/B"),
            printed(&v20("brick1x4x4print"), 4.0, 3.25, "Letters/C"),
            plain(&v20("brick2x4"), 6.0, 3.0, 4),
        ];
        Ok(Self {
            bricks,
            blb: catalog
                .bricks
                .iter()
                .map(|b| (b.id.clone(), b.mesh_id.clone()))
                .collect(),
            families,
            fx: fx_layout(&v20("brick2x4")),
            tops: tops_layout(&v20("brick1x1"), &v20("brick1x2"), &v20("brick2x2")),
            materials_dir: packages.role_dir(&content, "brick_materials")?,
            out: root.join("artifacts/brick-audit"),
        })
    }

    /// Made-up blocks of several footprints and heights, the fixture's
    /// printed tile and literal-coloured block, on the fixture's materials.
    fn synthetic() -> Result<Self> {
        let mut bricks = BrickFixture::synthetic()?;
        let look = |face| {
            (
                match face {
                    bri_content::brick::Face::Top => bri_content::brick::Surface::Top,
                    bri_content::brick::Face::Bottom => bri_content::brick::Surface::BottomLoop,
                    _ => bri_content::brick::Surface::Side,
                },
                None,
            )
        };
        let id = |s: &str| format!("fixture/brick/{s}");
        for (name, studs, plates) in [
            ("1x1", [1, 1], 3),
            ("1x2", [1, 2], 3),
            ("2x2", [2, 2], 3),
            ("2x4", [2, 4], 3),
            ("4x4f", [4, 4], 1),
            ("1x4f", [1, 4], 1),
            ("2x2x5", [2, 2], 15),
        ] {
            let mesh = block(&id(name), studs, plates, look);
            bricks.meshes.insert(mesh.id.clone(), mesh);
        }
        let print = bricks.print.clone();
        let families = vec![
            plain(&id("2x4"), -3.0, -2.0, 0),
            plain(&id("4x4f"), -1.0, -2.0, 1),
            plain(&id("1x4f"), 1.0, -2.25, 2),
            plain(&id("2x2x5"), 3.0, -2.0, 3),
            plain(&id("2x2"), 5.0, -2.0, 0),
            plain(&bricks.literal, -3.0, 0.5, 1),
            plain(&id("1x1"), -1.25, 3.25, 3),
            printed(&bricks.printable, 2.0, 3.0, &print),
            plain(&id("2x4"), 6.0, 3.0, 4),
        ];
        Ok(Self {
            blb: bricks
                .meshes
                .keys()
                .map(|id| (id.clone(), format!("{id}.blb")))
                .collect(),
            families,
            fx: fx_layout(&id("2x4")),
            tops: tops_layout(&id("1x1"), &id("1x2"), &id("2x2")),
            materials_dir: PathBuf::from("synthetic"),
            out: PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("brick-audit-synthetic"),
            bricks,
        })
    }
}

synthetic_and_content!(
    Audit: brick_family_audit_scene,
    brick_fx_audit_scene,
    brick_top_audit_scene,
);

const EYE: [f32; 3] = [1.5, 7.5, 10.5];
const TARGET: [f32; 3] = [1.5, 0.0, 0.5];

fn audit_scene(
    a: &Audit,
    name: &str,
    layout: &[Entry],
    time: f32,
    eye: [f32; 3],
    target: [f32; 3],
) -> Result<()> {
    let materials = &a.bricks.materials;
    let meshes = &a.bricks.meshes;
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
        let blb = a
            .blb
            .get(&entry.id)
            .with_context(|| format!("Missing catalog brick {}", entry.id))?;
        let mesh = meshes
            .get(&entry.id)
            .with_context(|| format!("Missing brick mesh {}", entry.id))?;
        let height = mesh.height_plates as f32 * 0.2;
        let mut brick = Brick::new(
            ContentRef::Resolved(entry.id.clone()),
            [entry.x, height * 0.5, entry.z],
            1,
        );
        brick.color = entry.color;
        brick.quarter_turns = entry.quarter_turns;
        brick.color_effect = entry.color_fx;
        brick.shape_effect = entry.shape_fx;
        let print_path = entry.print.as_deref().map(|alias| {
            let p = materials.bundle.resolve(alias).expect("print alias");
            brick.print = Some(ContentRef::Resolved(p.id.clone()));
            p.diffuse.path.clone()
        });
        records.push(serde_json::json!({
            "id": entry.id, "blb": blb, "position": brick.position,
            "quarter_turns": entry.quarter_turns, "paint": palette[entry.color as usize],
            "print": print_path, "color_fx": entry.color_fx, "shape_fx": entry.shape_fx,
            "depth_studs": mesh.footprint_studs[1],
        }));
        world.bricks.insert(index as u64 + 1, brick);
    }
    let scene = build_world_scene_materials(&world, meshes, 4_000_000, Some(materials))?;
    ensure!(!scene.indices.is_empty(), "Empty audit scene");
    let out = a.out.join(name);
    std::fs::create_dir_all(&out)?;
    let size = if a.bricks.content {
        CONTENT_SIZE
    } else {
        SYNTHETIC_SIZE
    };
    let [width, height] = size;
    let fov = 45_f32;
    let mut camera = Camera::perspective(
        eye,
        target,
        width as f32 / height as f32,
        fov.to_radians(),
        0.1,
        200.0,
    );
    camera.atmosphere[2] = time;
    let gpu = gpu::turn()?;
    render(&gpu, &scene, &camera, size, &out.join("ours.png"))?;
    std::fs::write(
        out.join("layout.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "width": width, "height": height, "eye": eye, "target": target,
            "fov_y_degrees": fov, "near": 0.1, "far": 200.0, "time_seconds": time,
            "sun_direction": &camera.sun_direction[..3], "sun_color": &camera.sun_color[..3],
            "ambient": &camera.ambient[..3], "background": [0.3, 0.3, 0.3],
            "materials": a.materials_dir,
            "bricks": records,
        }))?,
    )?;
    Ok(())
}

fn brick_family_audit_scene(a: &Audit) -> Result<()> {
    audit_scene(a, "families", &a.families, 0.0, EYE, TARGET)
}

fn brick_fx_audit_scene(a: &Audit) -> Result<()> {
    audit_scene(a, "fx", &a.fx, 0.37, EYE, TARGET)
}

fn brick_top_audit_scene(a: &Audit) -> Result<()> {
    audit_scene(a, "tops", &a.tops, 0.0, [0.5, 6.0, 1.0], [0.5, 0.0, 0.0])
}
