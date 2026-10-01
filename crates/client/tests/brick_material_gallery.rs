//! Every print on a printable brick, and the bricks whose authored colours
//! are literal RGB, drawn offscreen (no window or desktop input) from
//! made-up materials and bricks (`support::brick_fixture`) and again,
//! ignored, from the generated v20 packs.
#[macro_use]
mod support;

use anyhow::{Context, Result, ensure};
use bri_client::world_scene::build_world_scene_materials;
use bri_content::brick::Catalog;
use bri_net::protocol::PublicWorld;
use bri_render::scene::{Camera, SceneData, SceneRenderer, create_depth};
use bri_ui::gpu::Headless;
use bri_world::{Brick, ContentRef};
use std::path::{Path, PathBuf};
use support::{brick_fixture::BrickFixture, files::repo_root, gpu};

synthetic_and_content!(BrickFixture: surfaces_all_prints_and_sentinels);

#[test]
#[ignore = "requires generated v20 content"]
fn the_stock_materials_carry_77_prints() -> Result<()> {
    let f = BrickFixture::content()?;
    ensure!(
        f.materials.bundle.prints.len() == 77,
        "Re-audit changed print scope"
    );
    Ok(())
}

/// A printable brick for each print aspect: the stock catalog's selectable
/// brick of that aspect, or the fixture's one printable brick.
fn printable_for(f: &BrickFixture, catalog: Option<&Catalog>, aspect: &str) -> Result<String> {
    let Some(catalog) = catalog else {
        return Ok(f.printable.clone());
    };
    let aspect = if aspect == "Letters" { "2x2f" } else { aspect };
    Ok(catalog
        .bricks
        .iter()
        .find(|b| b.selectable() && b.print_aspect_ratio.as_deref() == Some(aspect))
        .with_context(|| format!("No printable brick for {aspect}"))?
        .id
        .clone())
}

/// Draw `scene` framed whole and require at least `min_pixels` covered.
fn render(gpu: &Headless, scene: &SceneData, path: &Path, min_pixels: usize) -> Result<()> {
    let size = wgpu::Extent3d {
        width: 1024,
        height: 1024,
        depth_or_array_layers: 1,
    };
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("original brick material gallery"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = create_depth(&gpu.device, size.width, size.height);
    let mut renderer = SceneRenderer::new(&gpu.device, format);
    let uploaded = renderer.upload(&gpu.device, &gpu.queue, scene)?;
    let mut min = glam::Vec3::splat(f32::INFINITY);
    let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
    for vertex in &scene.vertices {
        let p = glam::Vec3::from(vertex.position);
        min = min.min(p);
        max = max.max(p);
    }
    let center = (min + max) * 0.5;
    let span = (max - min).max_element().max(1.0);
    let eye = center + glam::Vec3::new(0.55, 1.6, 1.1) * span;
    let camera = Camera::perspective(
        eye.to_array(),
        center.to_array(),
        1.0,
        55_f32.to_radians(),
        0.1,
        500.0,
    );
    renderer.update_camera(&gpu.queue, &camera);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.render(
        &mut encoder,
        &target.create_view(&Default::default()),
        &depth.create_view(&Default::default()),
        &[&uploaded],
        Some(wgpu::Color {
            r: 0.08,
            g: 0.08,
            b: 0.1,
            a: 1.0,
        }),
    );
    let row = size.width * 4;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("brick gallery readback"),
        size: u64::from(row) * u64::from(size.height),
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
                rows_per_image: Some(size.height),
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
    let background = &pixels[..4];
    let occupied = pixels.chunks_exact(4).filter(|p| *p != background).count();
    // Sparse individual bricks cover a small fraction of this diagnostic grid.
    ensure!(
        occupied > min_pixels,
        "Empty brick gallery {}: {occupied} pixels",
        path.display()
    );
    image::save_buffer(
        path,
        &pixels,
        size.width,
        size.height,
        image::ColorType::Rgba8,
    )?;
    Ok(())
}

fn surfaces_all_prints_and_sentinels(f: &BrickFixture) -> Result<()> {
    let materials = &f.materials;
    let meshes = &f.meshes;
    let catalog: Option<Catalog> = if f.content {
        Some(serde_json::from_slice(&std::fs::read(
            repo_root().join("content/stock-catalog-004/stock-catalog.json"),
        )?)?)
    } else {
        None
    };
    let mut world = PublicWorld {
        name: "Original prints".into(),
        map_id: "gallery".into(),
        palette: vec![
            [0.8, 0.15, 0.1, 1.0],
            [0.15, 0.4, 0.8, 1.0],
            [0.8, 0.8, 0.8, 1.0],
        ],
        bricks: Default::default(),
    };
    let mut records = vec![];
    for (index, print) in materials.bundle.prints.iter().enumerate() {
        let printable = printable_for(f, catalog.as_ref(), &print.aspect)?;
        let mut brick = Brick::new(
            ContentRef::Resolved(printable.clone()),
            [(index % 11) as f32 * 2.8, 0.0, (index / 11) as f32 * 3.6],
            1,
        );
        brick.print = Some(ContentRef::Resolved(print.id.clone()));
        brick.color = (index % 3) as u8;
        world.bricks.insert(index as u64 + 1, brick);
        records
            .push(serde_json::json!({"print":print.id,"brick":printable,"position_index":index}));
    }
    let scene = build_world_scene_materials(&world, meshes, 4_000_000, Some(materials))?;
    ensure!(!materials.bundle.prints.is_empty(), "No prints");
    for print in &materials.bundle.prints {
        let index = scene
            .materials
            .iter()
            .position(|m| m.name == format!("native-overlay/{}", print.diffuse.path))
            .context("Print missing its original material")?;
        ensure!(
            scene.batches.iter().any(|b| b.material == index),
            "Print material is never drawn: {}",
            print.id
        );
    }
    let out = if f.content {
        repo_root().join("artifacts/brick-materials-gallery")
    } else {
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("brick-materials-gallery-synthetic")
    };
    std::fs::create_dir_all(&out)?;
    let gpu = gpu::turn()?;
    // The stock grid fills far more of the frame than the fixture's few
    // bricks.
    let min_pixels = if f.content { 10_000 } else { 1_000 };
    render(&gpu, &scene, &out.join("all-prints.png"), min_pixels)?;
    let print_triangles = scene.indices.len() / 3;
    let print_omissions = scene.omissions;
    world.bricks.clear();
    // Bricks whose authored colours are literal RGB, in three paints; the
    // stock pumpkin base also carries its face.
    let specials: Vec<String> = if f.content {
        [
            "v20/brick/bricktreasurechestdata",
            "v20/brick/brickgravestonedata",
            "v20/brick/brickpumpkinbasedata",
        ]
        .map(String::from)
        .into()
    } else {
        vec![f.literal.clone()]
    };
    for (index, id) in specials.iter().enumerate() {
        ensure!(meshes.contains_key(id), "Missing special native brick {id}");
        for color in 0..3 {
            let mut brick = Brick::new(
                ContentRef::Resolved(id.clone()),
                [7.0 + index as f32 * 7.0, 0.0, 5.0 + color as f32 * 7.0],
                1,
            );
            brick.color = color;
            if id == "v20/brick/brickpumpkinbasedata" {
                let mut face = brick.clone();
                face.definition = ContentRef::Resolved("v20/brick/brickpumpkinfacedata".into());
                world.bricks.insert(100 + u64::from(color), face);
            }
            world
                .bricks
                .insert((index * 3 + usize::from(color) + 1) as u64, brick);
        }
    }
    let special = build_world_scene_materials(&world, meshes, 4_000_000, Some(materials))?;
    ensure!(
        special.vertices.iter().all(|v| v.color[3] >= 0.0),
        "Legacy sentinel leaked into alpha"
    );
    ensure!(
        special
            .omissions
            .iter()
            .any(|s| s.contains("out-of-range literal input conversion remains unverified")),
        "Unverified literal RGB interpretation must remain visible"
    );
    render(
        &gpu,
        &special,
        &out.join("paint-offset-special-bricks.png"),
        min_pixels,
    )?;
    std::fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "adapter":gpu.adapter_info.name,"visible_window":false,"desktop_input":false,
            "prints":records,"print_triangles":print_triangles,"print_omissions":print_omissions,
            "special_triangles":special.indices.len()/3,"special_omissions":special.omissions,
            "fidelity_complete":false
        }))?,
    )?;
    Ok(())
}
