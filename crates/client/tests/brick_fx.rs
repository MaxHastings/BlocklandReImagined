//! Real replicated brick adapter + native brick resources, headless only.
//! The textured scenes run on a synthetic materials pack and bricks
//! (`support::brick_fixture`), and again, ignored, on the generated v20
//! packs.
#[macro_use]
mod support;

use anyhow::{Result, ensure};
use bri_client::world_scene::build_world_scene_materials;
use bri_content::brick::{Brick as Mesh, Face, Quad, Surface, Vertex};
use bri_net::protocol::PublicWorld;
use bri_render::scene::*;
use bri_ui::gpu::Headless;
use bri_world::{Brick, ContentRef};
use std::collections::BTreeMap;
use support::{brick_fixture::BrickFixture, files::evidence_dir, gpu};
fn quad() -> Mesh {
    Mesh {
        schema_version: 1,
        id: "fx-quad".into(),
        footprint_studs: [1, 1],
        height_plates: 1,
        attachment_rows: vec!["b".into()],
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads: vec![Quad {
            face: Face::Omni,
            surface: Surface::Side,
            colors: None,
            vertices: [
                [-0.8, -0.8, 0.],
                [0.8, -0.8, 0.],
                [0.8, 0.8, 0.],
                [-0.8, 0.8, 0.],
            ]
            .map(|position| Vertex {
                position,
                normal: [0., 0., 1.],
                uv: [0.25, 0.75],
            }),
        }],
    }
}
fn world(color: u8, shape: u8) -> PublicWorld {
    let mut b = Brick::new(ContentRef::Resolved("test".into()), [0.; 3], 1);
    b.color_effect = color;
    b.shape_effect = shape;
    PublicWorld {
        name: "FX".into(),
        map_id: "test".into(),
        palette: vec![[0.3, 0.6, 0.2, 0.65]],
        bricks: bri_world::Bricks::unit(1, b),
    }
}
#[test]
fn authoritative_fx_metadata_materials_and_ghost_isolation() -> Result<()> {
    let meshes = BTreeMap::from([("test".into(), quad())]);
    for color in 0..=6 {
        for shape in 0..=2 {
            let scene = build_world_scene_materials(&world(color, shape), &meshes, 2, None)?;
            for vertex in &scene.vertices {
                let decoded = BrickFx::decode(vertex.fx).expect("valid FX record");
                assert_eq!(decoded.fx, BrickFx { color, shape });
                if (color, shape) != (0, 0) {
                    assert_eq!(decoded.depth_studs, 1);
                    assert_eq!(decoded.centre, [0.; 3]);
                }
                assert_eq!(vertex.uv, [0.25, 0.75]);
                assert_eq!(vertex.color, [0.3, 0.6, 0.2, 0.65]);
            }
            assert!(scene.materials.len() <= 2);
        }
    }
    let mut ghost = SceneData::default();
    ghost.materials.push(Material::vertex_lit("ghost", 0));
    ghost.append_brick(
        &quad(),
        glam::Mat4::IDENTITY.to_cols_array(),
        [1.; 4],
        [0; 6],
    )?;
    assert!(ghost.vertices.iter().all(|v| v.fx == [0.; 4]));
    let mut repeated = world(4, 1);
    for id in 2..=128 {
        repeated.bricks.insert(id, repeated.bricks[&1].clone());
    }
    let shared = build_world_scene_materials(&repeated, &meshes, 1000, None)?;
    assert!(
        shared.materials.len() <= 2,
        "FX material count grew per brick"
    );
    assert_eq!(BrickFx::new(0, 1)?.displacement_bounds(), [0.08; 3]);
    assert_eq!(BrickFx::new(0, 2)?.displacement_bounds(), [0., 0.2, 0.]);
    Ok(())
}
#[test]
fn marker_validation_and_literal_offset_alpha_regression() -> Result<()> {
    for color in 0..=6 {
        for shape in 0..=2 {
            let fx = BrickFx::new(color, shape)?;
            let encoded = fx.encode([1.5, -2., 3.25], 3, 64)?;
            let decoded = BrickFx::decode(encoded).unwrap();
            assert_eq!(decoded.fx, fx);
            if fx != BrickFx::default() {
                assert_eq!((decoded.corner, decoded.depth_studs), (3, 64));
                assert_eq!(decoded.centre, [1.5, -2., 3.25]);
            }
        }
    }
    assert!(BrickFx::new(1, 0)?.encode([0.; 3], 4, 1).is_err());
    assert!(BrickFx::new(1, 0)?.encode([0.; 3], 0, 0).is_err());
    for fx in [
        [0., 0., 0., 0.5],
        [0., 0., 0., 129.5],
        [0., 0., 0., 8.],
        [0., 0., 0., 128.],
        [f32::NAN, 0., 0., 130.],
        [0., 0., 0., 24.],
    ] {
        assert_eq!(BrickFx::decode(fx), None, "{fx:?}");
    }
    let meshes = BTreeMap::from([("test".into(), quad())]);
    let mut scene = build_world_scene_materials(&world(1, 0), &meshes, 2, None)?;
    let index = scene.batches[0].material;
    for kind in [
        MaterialKind::Surface,
        MaterialKind::Terrain,
        MaterialKind::Sky,
        MaterialKind::Cloud,
        MaterialKind::Water,
    ] {
        scene.materials[index].kind = kind;
        assert!(
            scene.validate().is_err(),
            "Brick marker accepted on {kind:?}"
        );
    }
    let paint = [0.2, 0.4, 0.8, 0.5];
    assert_eq!(
        resolve_brick_vertex_color(paint, Some([0.2, -0.3, 0.4, -1.]))?.rgba,
        [0.4, 0.099999994, 1., 0.5]
    );
    assert!(resolve_brick_vertex_color(paint, Some([0., 0., 0., -0.5])).is_err());
    let literal = resolve_brick_vertex_color(paint, Some([200., 150., 0., 1.]))?;
    assert_eq!(literal.rgba, [200., 150., 0., 1.]);
    assert!(literal.provisional);
    assert_eq!(
        resolve_brick_vertex_color(paint, Some([0.8, 0.2, 0.1, 0.]))?.rgba,
        [0.8, 0.2, 0.1, 0.]
    );
    assert!(resolve_brick_vertex_color(paint, Some([0., 0., 0., -2.])).is_err());
    assert!(resolve_brick_vertex_color(paint, Some([f32::NAN, 0., 0., 1.])).is_err());
    let mut invalid_mesh = quad();
    invalid_mesh.quads[0].colors = Some([[0., 0., 0., -0.5]; 4]);
    let mut target = SceneData::default();
    target.materials.push(Material::vertex_lit("test", 0));
    assert!(
        target
            .append_brick_with_fx(
                &invalid_mesh,
                glam::Mat4::IDENTITY.to_cols_array(),
                paint,
                [0; 6],
                BrickFx::new(1, 0)?
            )
            .is_err()
    );
    assert!(target.vertices.is_empty() && target.indices.is_empty() && target.batches.is_empty());
    assert_eq!(target.materials.len(), 1);
    Ok(())
}
fn render(gpu: &Headless, scene: &SceneData, time: f32) -> Result<Vec<u8>> {
    render_from(gpu, scene, time, [0.55, 1.6, 1.1])
}
#[test]
fn fx_marker_is_flat_across_perspective_triangles_offscreen() -> Result<()> {
    let meshes = BTreeMap::from([("test".into(), quad())]);
    let mut w = world(3, 0);
    w.palette[0] = [0.25, 0.5, 0.75, 1.];
    let mut scene = build_world_scene_materials(&w, &meshes, 2, None)?;
    // v20 glow lights the brick with ambient + sun/min(sun): exactly paint here.
    scene.ambient = [0.; 3];
    scene.sun_color = [1.; 3];
    let gpu = gpu::turn()?;
    let pixels = render(&gpu, &scene, 0.)?;
    let background = &pixels[..4];
    let mut covered = 0;
    for pixel in pixels.chunks_exact(4).filter(|p| *p != background) {
        covered += 1;
        assert!(
            pixel[..3]
                .iter()
                .zip([64u8, 128, 191])
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "FX metadata lost during perspective interpolation: {pixel:?}"
        );
    }
    assert!(covered > 1000);
    Ok(())
}
fn render_from(
    gpu: &Headless,
    scene: &SceneData,
    time: f32,
    direction: [f32; 3],
) -> Result<Vec<u8>> {
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let uploaded = renderer.upload(&gpu.device, &gpu.queue, scene)?;
    render_uploaded(gpu, scene, &mut renderer, &uploaded, time, direction)
}
fn render_uploaded(
    gpu: &Headless,
    scene: &SceneData,
    renderer: &mut SceneRenderer,
    uploaded: &GpuScene,
    time: f32,
    direction: [f32; 3],
) -> Result<Vec<u8>> {
    let size = wgpu::Extent3d {
        width: 256,
        height: 256,
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

    let mut min = glam::Vec3::splat(f32::INFINITY);
    let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
    for vertex in &scene.vertices {
        let p = glam::Vec3::from(vertex.position);
        min = min.min(p);
        max = max.max(p);
    }
    let center = (min + max) * 0.5;
    let span = (max - min).max_element().max(1.0);
    let eye = center + glam::Vec3::from(direction) * span;
    let mut camera = Camera::perspective(
        eye.to_array(),
        center.to_array(),
        1.0,
        55_f32.to_radians(),
        0.1,
        500.0,
    );
    camera.apply_environment(scene);
    camera.atmosphere[2] = time;
    renderer.update_camera(&gpu.queue, &camera);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    renderer.render(
        &mut encoder,
        &target.create_view(&Default::default()),
        &depth.create_view(&Default::default()),
        &[uploaded],
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
    Ok(pixels.to_vec())
}

synthetic_and_content!(BrickFixture: literal_rgb_candidates_offscreen, brick_fx_prints_phase_and_paint_offscreen);

/// Total RGB of a rendered frame.
fn brightness(pixels: &[u8]) -> u64 {
    pixels
        .chunks_exact(4)
        .flat_map(|p| &p[..3])
        .map(|c| u64::from(*c))
        .sum()
}

fn fixture_world(definition: &str, color: u8, shape: u8) -> PublicWorld {
    let mut w = world(color, shape);
    w.bricks.get_mut(&1).unwrap().definition = ContentRef::Resolved(definition.into());
    w
}

/// Authored literal RGB above 1 (v20's pumpkin face rows): the raw values
/// against two candidate readings, rendered side by side. This is a
/// diagnostic comparison, deliberately not a policy selection.
fn literal_rgb_candidates_offscreen(f: &BrickFixture) -> Result<()> {
    let mut w = fixture_world(&f.literal, 0, 0);
    w.palette[0] = [0.8, 0.3, 0.05, 1.];
    let mut raw = build_world_scene_materials(&w, &f.meshes, 100000, Some(&f.materials))?;
    raw.ambient = [0.08; 3];
    raw.sun_color = [0.; 3];
    let literal = |v: &SceneVertex| v.color[..3].iter().any(|c| *c > 1.);
    let unusual = raw.vertices.iter().filter(|v| literal(v)).count();
    assert!(unusual > 0, "Authored literal rows absent");
    let mut byte_candidate = raw.clone();
    let mut clamp_candidate = raw.clone();
    for vertex in &mut byte_candidate.vertices {
        if literal(vertex) {
            for c in &mut vertex.color[..3] {
                *c /= 255.;
            }
        }
    }
    for vertex in &mut clamp_candidate.vertices {
        if literal(vertex) {
            for c in &mut vertex.color[..3] {
                *c = c.clamp(0., 1.);
            }
        }
    }
    let gpu = gpu::turn()?;
    let images = [
        render_from(&gpu, &raw, 0., [-1.7, 0.3, 0.2])?,
        render_from(&gpu, &byte_candidate, 0., [-1.7, 0.3, 0.2])?,
        render_from(&gpu, &clamp_candidate, 0., [-1.7, 0.3, 0.2])?,
    ];
    drop(gpu);
    let differences: Vec<usize> = images[1..]
        .iter()
        .map(|candidate| {
            candidate
                .chunks_exact(4)
                .zip(images[0].chunks_exact(4))
                .filter(|(a, b)| a != b)
                .count()
        })
        .collect();
    assert!(
        differences.iter().all(|n| *n > 0),
        "Candidate interpretations must visibly differ in this diagnostic"
    );
    // Literal RGB above 1 lights past the clamped reading.
    assert!(
        brightness(&images[0]) > brightness(&images[2]),
        "Raw literal rows no brighter than clamped ones"
    );
    let mut gallery = image::RgbaImage::new(256 * 3, 256);
    for (i, bytes) in images.into_iter().enumerate() {
        let tile = image::RgbaImage::from_raw(256, 256, bytes).unwrap();
        image::imageops::replace(&mut gallery, &tile, i as i64 * 256, 0);
    }
    // Regenerated evidence goes to target/; docs/research keeps the reviewed copy.
    let out = evidence_dir(
        f.content,
        "target/test-artifacts/brick-fx",
        "brick-fx-synthetic",
    );
    std::fs::create_dir_all(&out)?;
    gallery.save(out.join("sentinel-candidates.png"))?;
    std::fs::write(
        out.join("sentinel-comparison.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version":1,"brick":f.literal,"left_to_right":["unchanged raw finite RGBA", "diagnostic RGB divided by255", "diagnostic RGB clamped before lighting"],
            "ambient":0.08,"sun":0,"changed_pixels_against_raw":differences,"anomalous_native_vertices":unusual,
            "selected_interpretation":null,"acceptance_open":true,"runtime_policy":"preserve prior raw values with diagnostic"
        }))?,
    )?;
    Ok(())
}

/// v20's pumpkin face authors its odd rows as literal RGBA 200 150 0 1.
#[test]
#[ignore = "requires generated v20 content"]
fn original_pumpkin_keeps_its_literal_200_150_0_rows() -> Result<()> {
    let f = BrickFixture::content()?;
    let mut w = fixture_world(&f.literal, 0, 0);
    w.palette[0] = [0.8, 0.3, 0.05, 1.];
    let raw = build_world_scene_materials(&w, &f.meshes, 100000, Some(&f.materials))?;
    assert!(raw.vertices.iter().any(|v| v.color == [200., 150., 0., 1.]));
    Ok(())
}

fn brick_fx_prints_phase_and_paint_offscreen(f: &BrickFixture) -> Result<()> {
    ensure!(
        f.meshes.contains_key(&f.printable),
        "Missing printed tile {}",
        f.printable
    );
    let print = f
        .materials
        .bundle
        .resolve(&f.print)
        .unwrap_or_else(|| panic!("print {} does not resolve", f.print))
        .id
        .clone();
    let gpu = gpu::turn()?;
    // One renderer for every scene here: each scene is uploaded once.
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let draw = |renderer: &mut SceneRenderer, scene: &SceneData, time: f32| {
        let uploaded = renderer.upload(&gpu.device, &gpu.queue, scene)?;
        render_uploaded(&gpu, scene, renderer, &uploaded, time, [0.55, 1.6, 1.1])
    };
    let mut images = vec![];
    let mut later_images = vec![];
    let mut records = vec![];
    for color in 0..=6 {
        let mut w = fixture_world(&f.printable, color, 0);
        w.bricks.get_mut(&1).unwrap().print = Some(ContentRef::Resolved(print.clone()));
        w.palette[0] = [0.25, 0.5, 0.75, 1.];
        let scene = build_world_scene_materials(&w, &f.meshes, 100000, Some(&f.materials))?;
        assert!(
            scene
                .vertices
                .iter()
                .all(|v| v.color.iter().all(|x| (0. ..=1.).contains(x)))
        );
        let uploaded = renderer.upload(&gpu.device, &gpu.queue, &scene)?;
        let a = render_uploaded(&gpu, &scene, &mut renderer, &uploaded, 0., [0.55, 1.6, 1.1])?;
        let b = render_uploaded(
            &gpu,
            &scene,
            &mut renderer,
            &uploaded,
            0.8,
            [0.55, 1.6, 1.1],
        )?;
        if color >= 4 {
            assert_ne!(a, b, "animated FX{color} did not change");
        } else {
            assert_eq!(a, b, "static FX changed with time");
        }
        if color > 0 {
            assert_ne!(
                if color >= 4 { &b } else { &a },
                &images[0],
                "FX{color} indistinguishable from none"
            );
        }
        images.push(a);
        later_images.push(b);
        records.push(serde_json::json!({"color_fx":color,"material_count":scene.materials.len(),"triangles":scene.indices.len()/3,"omissions":scene.omissions}));
    }
    // Glow (FX3) lights the brick past its unlit paint.
    assert!(
        brightness(&images[3]) > brightness(&images[0]),
        "Glow did not brighten the brick"
    );
    for shape in 1..=2 {
        let w = fixture_world(&f.printable, 0, shape);
        let scene = build_world_scene_materials(&w, &f.meshes, 100000, Some(&f.materials))?;
        assert_ne!(
            draw(&mut renderer, &scene, 0.)?,
            draw(&mut renderer, &scene, 0.8)?
        );
    }
    let mut w = fixture_world(&f.printable, 0, 0);
    let a = draw(
        &mut renderer,
        &build_world_scene_materials(&w, &f.meshes, 100000, Some(&f.materials))?,
        0.,
    )?;
    w.palette[0] = [0.8, 0.1, 0.1, 0.4];
    let b = draw(
        &mut renderer,
        &build_world_scene_materials(&w, &f.meshes, 100000, Some(&f.materials))?,
        0.,
    )?;
    drop(renderer);
    drop(gpu);
    assert_ne!(a, b);
    // Regenerated evidence goes to target/; docs/research keeps the reviewed copy.
    let out = evidence_dir(
        f.content,
        "target/test-artifacts/brick-fx",
        "brick-fx-synthetic",
    );
    std::fs::create_dir_all(&out)?;
    let mut gallery = image::RgbaImage::new(256 * 7, 512);
    for (i, bytes) in images.iter().enumerate() {
        let tile = image::RgbaImage::from_raw(256, 256, bytes.clone()).unwrap();
        image::imageops::replace(&mut gallery, &tile, i as i64 * 256, 0);
        let later = image::RgbaImage::from_raw(256, 256, later_images[i].clone()).unwrap();
        image::imageops::replace(&mut gallery, &later, i as i64 * 256, 256);
    }
    gallery.save(out.join("offscreen-fx.png"))?;
    std::fs::write(
        out.join("offscreen-report.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"schema_version":1,"brick":f.printable,"records":records,"row_times_seconds":[0,0.8],"single_upload_per_phase_pair":true,"offscreen":true,"not_parity_acceptance":true}),
        )?,
    )?;
    ensure!(images.len() == 7, "missing FX");
    Ok(())
}
