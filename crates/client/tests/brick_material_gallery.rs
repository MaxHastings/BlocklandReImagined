//! Original native content, no window or desktop input.
use anyhow::{Context, Result, ensure};
use bri_client::{materials::BrickMaterials, world_scene::build_world_scene_materials};
use bri_content::brick::Catalog;
use bri_net::protocol::PublicWorld;
use bri_render::scene::{Camera, SceneData, SceneRenderer, create_depth};
use bri_sim::definitions::Definitions;
use bri_ui::gpu::Headless;
use bri_world::{Brick, ContentRef};
use std::{collections::BTreeMap, path::Path};

fn render(gpu: &Headless, scene: &SceneData, path: &Path) -> Result<()> {
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
        occupied > 10_000,
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

#[test]
#[ignore = "requires local converted original assets; offscreen only"]
fn original_surfaces_all_prints_and_sentinels() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let content = root.join("content");
    let materials = BrickMaterials::load(&content.join("brick-materials-001"))?;
    let catalog: Catalog = serde_json::from_slice(&std::fs::read(
        content.join("stock-catalog-004/stock-catalog.json"),
    )?)?;
    let definitions = Definitions::load(
        &content.join("stock-catalog-004"),
        &content.join("maps-pass-003"),
    )?;
    let meshes = definitions
        .entries
        .into_iter()
        .map(|(id, def)| (id, def.mesh))
        .collect();
    let mut world = PublicWorld {
        name: "Original prints".into(),
        map_id: "gallery".into(),
        palette: vec![
            [0.8, 0.15, 0.1, 1.0],
            [0.15, 0.4, 0.8, 1.0],
            [0.8, 0.8, 0.8, 1.0],
        ],
        bricks: BTreeMap::new(),
    };
    let mut records = vec![];
    for (index, print) in materials.bundle.prints.iter().enumerate() {
        let aspect = if print.aspect == "Letters" {
            "2x2f"
        } else {
            &print.aspect
        };
        let entry = catalog
            .bricks
            .iter()
            .find(|b| b.selectable() && b.print_aspect_ratio.as_deref() == Some(aspect))
            .with_context(|| format!("No printable brick for {aspect}"))?;
        let mut brick = Brick::new(
            ContentRef::Resolved(entry.id.clone()),
            [(index % 11) as f32 * 2.8, 0.0, (index / 11) as f32 * 3.6],
            1,
        );
        brick.print = Some(ContentRef::Resolved(print.id.clone()));
        brick.color = (index % 3) as u8;
        world.bricks.insert(index as u64 + 1, brick);
        records.push(serde_json::json!({"print":print.id,"brick":entry.id,"position_index":index}));
    }
    let scene = build_world_scene_materials(&world, &meshes, 4_000_000, Some(&materials))?;
    ensure!(
        materials.bundle.prints.len() == 77,
        "Re-audit changed print scope"
    );
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
    let out = root.join("artifacts/brick-materials-gallery");
    std::fs::create_dir_all(&out)?;
    let gpu = Headless::new()?;
    render(&gpu, &scene, &out.join("all-77-prints.png"))?;
    let print_triangles = scene.indices.len() / 3;
    let print_omissions = scene.omissions;
    world.bricks.clear();
    for (index, id) in [
        "v20/brick/bricktreasurechestdata",
        "v20/brick/brickgravestonedata",
        "v20/brick/brickpumpkinbasedata",
    ]
    .iter()
    .enumerate()
    {
        ensure!(
            meshes.contains_key(*id),
            "Missing special native brick {id}"
        );
        for color in 0..3 {
            let mut brick = Brick::new(
                ContentRef::Resolved((*id).into()),
                [7.0 + index as f32 * 7.0, 0.0, 5.0 + color as f32 * 7.0],
                1,
            );
            brick.color = color;
            if index == 2 {
                let mut face = brick.clone();
                face.definition = ContentRef::Resolved("v20/brick/brickpumpkinfacedata".into());
                world.bricks.insert(100 + u64::from(color), face);
            }
            world
                .bricks
                .insert((index * 3 + usize::from(color) + 1) as u64, brick);
        }
    }
    let special = build_world_scene_materials(&world, &meshes, 4_000_000, Some(&materials))?;
    ensure!(
        special.vertices.iter().all(|v| v.color[3] >= 0.0),
        "Legacy sentinel leaked into alpha"
    );
    ensure!(
        special
            .omissions
            .iter()
            .any(|s| s.contains("unverified out-of-range literal")),
        "Unverified pumpkin RGB interpretation must remain visible"
    );
    render(&gpu, &special, &out.join("paint-offset-special-bricks.png"))?;
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
