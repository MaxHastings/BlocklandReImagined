//! Baked interior light for vertex-lit objects (`bri_render::light_volume`):
//! a lamp that exists only in lightmaps lights players and bricks near it.
use anyhow::{Context, Result};
use bri_render::{
    light_volume::{LightVolume, RAYS_PER_CELL},
    scene::{
        Camera, GpuScene, Material, MeshBatch, SceneData, SceneImage, SceneRenderer, SceneVertex,
        create_depth,
    },
    scene_loader::load_map_bundle,
};
use glam::Vec3;
use std::path::PathBuf;

fn gray(value: u8) -> SceneImage {
    SceneImage {
        label: format!("lightmap {value}"),
        width: 1,
        height: 1,
        rgba: vec![value, value, value, 255],
        srgb: false,
    }
}

/// A closed room from -3 to 3 on every axis, faces pointing in, lit by its
/// lightmaps only: the floor (y = -3) is `floor`, the other five `walls`.
fn room(floor: u8, walls: u8) -> SceneData {
    let mut scene = SceneData {
        images: vec![SceneImage::white(), gray(floor), gray(walls)],
        ..Default::default()
    };
    scene.materials.push(Material::surface("floor", 0, 1));
    scene.materials.push(Material::surface("walls", 0, 2));
    for axis in 0..3 {
        for side in [-1.0f32, 1.0] {
            let inward = -side;
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let corner = |a: f32, b: f32| {
                let mut p = [0.0; 3];
                p[axis] = 3.0 * side;
                p[u] = 3.0 * a;
                p[v] = 3.0 * b;
                p
            };
            let mut normal = [0.0; 3];
            normal[axis] = inward;
            let base = scene.vertices.len() as u32;
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                scene.vertices.push(SceneVertex {
                    position: corner(a, b),
                    normal,
                    uv: [0.0; 2],
                    lightmap_uv: [0.5; 2],
                    color: [1.0; 4],
                    fx: [0.0; 4],
                });
            }
            let start = scene.indices.len() as u32;
            scene.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
            scene.batches.push(MeshBatch {
                indices: start..start + 6,
                material: usize::from(!(axis == 1 && side < 0.0)),
                center: [0.0; 3],
            });
        }
    }
    scene
}

/// Adds a cube of `half` size at `centre` whose faces point in or out.
fn add_cube(scene: &mut SceneData, centre: Vec3, half: f32, inward: bool, material: usize) {
    for axis in 0..3 {
        for side in [-1.0f32, 1.0] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let mut normal = Vec3::ZERO;
            normal[axis] = if inward { -side } else { side };
            let base = scene.vertices.len() as u32;
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut p = Vec3::ZERO;
                p[axis] = side;
                p[u] = a;
                p[v] = b;
                scene.vertices.push(SceneVertex {
                    position: (centre + p * half).to_array(),
                    normal: normal.to_array(),
                    uv: [0.0; 2],
                    lightmap_uv: [0.5; 2],
                    color: [1.0; 4],
                    fx: [0.0; 4],
                });
            }
            let start = scene.indices.len() as u32;
            scene.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
            scene.batches.push(MeshBatch {
                indices: start..start + 6,
                material,
                center: centre.to_array(),
            });
        }
    }
}

/// A dark room 20 units across with a small brightly lit block off centre:
/// light that changes sharply near the block and slowly across open air.
fn lamp_room() -> SceneData {
    let mut scene = SceneData {
        images: vec![SceneImage::white(), gray(12), gray(250)],
        ..Default::default()
    };
    scene.materials.push(Material::surface("room", 0, 1));
    scene.materials.push(Material::surface("lamp", 0, 2));
    add_cube(&mut scene, Vec3::ZERO, 10.0, true, 0);
    add_cube(&mut scene, Vec3::new(3.0, -2.0, 1.0), 1.0, false, 1);
    scene
}

#[test]
fn interpolated_cells_match_casting_from_every_cell() {
    let scene = lamp_room();
    let fast = LightVolume::bake(&scene, 0.5, 100_000).unwrap();
    let every = bri_render::light_volume::Baker::new(&scene)
        .unwrap()
        .bake_every_cell(0.5, 100_000);
    assert_eq!((fast.dims, fast.origin), (every.dims, every.origin));
    // The work saved is real, and a work count, not a clock, measures it.
    assert!(fast.rays * 2 < every.rays, "{} {}", fast.rays, every.rays);
    let mut worst = 0;
    let mut total = 0u64;
    for (a, b) in fast.texels.iter().zip(&every.texels) {
        for c in 0..4 {
            let d = a[c].abs_diff(b[c]);
            worst = worst.max(d);
            total += u64::from(d);
        }
    }
    let mean = total as f64 / (fast.texels.len() * 4) as f64;
    // Interpolated blocks agree within the bake's tolerance (12/255) plus
    // sampling noise between 24 fixed directions; most cells match exactly.
    assert!(worst <= 24 && mean < 1.0, "worst {worst} mean {mean}");
}

#[test]
fn stored_volumes_round_trip_and_keys_follow_the_content() {
    let scene = lamp_room();
    let baker = bri_render::light_volume::Baker::new(&scene).unwrap();
    let key = baker.key(0.5, 100_000);
    assert_eq!(
        key,
        bri_render::light_volume::Baker::new(&scene)
            .unwrap()
            .key(0.5, 100_000)
    );
    assert_ne!(key, baker.key(1.0, 100_000));
    let mut relit = lamp_room();
    relit.images[2].rgba[0] = 200;
    assert_ne!(
        key,
        bri_render::light_volume::Baker::new(&relit)
            .unwrap()
            .key(0.5, 100_000)
    );
    let volume = baker.bake(0.5, 100_000);
    let bytes = volume.to_bytes();
    let stored = LightVolume::from_bytes(&bytes).unwrap();
    assert_eq!(
        (stored.origin, stored.cell, stored.dims),
        (volume.origin, volume.cell, volume.dims)
    );
    assert_eq!((stored.texels, stored.rays), (volume.texels, 0));
    // Truncated, padded or foreign files are refused, so a bake replaces them.
    assert!(LightVolume::from_bytes(&bytes[..bytes.len() - 1]).is_none());
    assert!(LightVolume::from_bytes(&[bytes.as_slice(), &[0]].concat()).is_none());
    assert!(LightVolume::from_bytes(b"not a light volume").is_none());
}

fn luminance(light: [f32; 3]) -> f32 {
    light.iter().sum::<f32>() / 3.0
}

#[test]
fn a_lit_floor_lights_what_stands_on_it() {
    // A floor lamp's pool under an unlit ceiling: the classic engine lit a
    // shape with the lightmap of the surface under it.
    let volume = LightVolume::bake(&room(230, 10), 0.5, 100_000).unwrap();
    for position in [[0.0, -2.0, 0.0], [1.5, 0.0, -1.0], [-2.0, 2.0, 2.0]] {
        let lit = luminance(volume.light(position, [0.0, 0.0, 1.0]));
        assert!(lit > 0.6, "{position:?}: {lit}");
    }
}

#[test]
fn a_lit_shade_lights_what_stands_inside_it() {
    // Inside the Bedroom lamp shade the bars underfoot are unlit, but every
    // other direction sees lit paper.
    let volume = LightVolume::bake(&room(5, 240), 0.5, 100_000).unwrap();
    let lit = luminance(volume.light([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
    assert!(lit > 0.6, "{lit}");
    // Facing up and over the shoulder is brighter than facing down.
    let top = luminance(volume.light([0.0, 0.0, 0.0], [-0.57735, 0.57735, 0.57735]));
    let bottom = luminance(volume.light([0.0, 0.0, 0.0], [0.0, -1.0, 0.0]));
    assert!(top > bottom * 1.3, "{top} {bottom}");
}

#[test]
fn a_dark_room_stays_dark_and_walls_do_not_leak() {
    let volume = LightVolume::bake(&room(0, 0), 0.5, 100_000).unwrap();
    assert!(luminance(volume.light([0.0, 0.0, 0.0], [0.0, 1.0, 0.0])) < 0.01);
    // Cells inside a wall hold no light at all, not black light.
    let lit = LightVolume::bake(&room(255, 255), 1.0, 100_000).unwrap();
    let solid = lit.texels.iter().filter(|t| t[3] == 0).count();
    let open = lit.texels.iter().filter(|t| t[3] == 255).count();
    assert!(solid > 0 && open > 0, "{solid} {open}");
    assert!(lit.texels.iter().all(|t| t[3] == 0 || t[3] == 255));
    // Nothing outside the volume is lit by it.
    assert_eq!(lit.light([50.0, 0.0, 0.0], [0.0, 1.0, 0.0]), [0.0; 3]);
}

#[test]
fn a_scene_without_lightmaps_bakes_nothing_and_the_cell_budget_holds() {
    let mut scene = room(200, 200);
    for material in &mut scene.materials {
        *material = Material::vertex_lit(material.name.clone(), 0);
    }
    assert!(LightVolume::bake(&scene, 0.5, 100_000).is_none());
    let volume = LightVolume::bake(&room(200, 200), 0.01, 5_000).unwrap();
    assert!(volume.texels.len() <= 5_000, "{:?}", volume.dims);
    assert_eq!(
        volume.texels.len(),
        volume.dims.iter().map(|d| *d as usize).product::<usize>()
    );
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}
impl Gpu {
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
            Ok(Self { device, queue })
        })
    }
    /// The centre pixel of a 64x64 frame.
    fn centre(
        &self,
        renderer: &mut SceneRenderer,
        scene: &GpuScene,
        camera: &Camera,
    ) -> Result<[u8; 3]> {
        let size = 64;
        let extent = wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        };
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = create_depth(&self.device, size, size);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256 * u64::from(size),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        renderer.update_camera(&self.queue, camera);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        renderer.render(
            &mut encoder,
            &target.create_view(&Default::default()),
            &depth.create_view(&Default::default()),
            &[scene],
            Some(wgpu::Color::BLACK),
        );
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(size),
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
        let i = 32 * 256 + 32 * 4;
        Ok([mapped[i], mapped[i + 1], mapped[i + 2]])
    }
}

/// A white vertex-lit triangle facing the identity camera at z = 0.5.
fn player_stand_in() -> SceneData {
    SceneData {
        vertices: [[-0.9, -0.8, 0.5], [0.9, -0.8, 0.5], [0.0, 0.9, 0.5]]
            .into_iter()
            .map(|position| SceneVertex {
                position,
                normal: [0.0, 0.0, 1.0],
                uv: [0.0; 2],
                lightmap_uv: [0.0; 2],
                color: [1.0; 4],
                fx: [0.0; 4],
            })
            .collect(),
        indices: vec![0, 1, 2],
        materials: vec![Material::vertex_lit("stand-in", 0)],
        batches: vec![MeshBatch {
            indices: 0..3,
            material: 0,
            center: [0.0, 0.0, 0.5],
        }],
        ..Default::default()
    }
}

#[test]
fn vertex_lit_meshes_take_baked_light_on_a_black_sun_map() -> Result<()> {
    let gpu = Gpu::new()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mesh = renderer.upload(&gpu.device, &gpu.queue, &player_stand_in())?;
    // BedroomDark: black sun and ambient.
    let dark = Camera {
        sun_color: [0.0; 4],
        ambient: [0.0; 4],
        ..Default::default()
    };
    assert_eq!(gpu.centre(&mut renderer, &mesh, &dark)?, [0, 0, 0]);
    let volume = LightVolume::bake(&room(230, 230), 0.5, 100_000).unwrap();
    renderer.set_light_volume(&gpu.device, &gpu.queue, Some(&volume))?;
    let lit = gpu.centre(&mut renderer, &mesh, &dark)?;
    assert!(lit.iter().all(|c| *c > 150), "{lit:?}");
    // The shader matches the CPU mirror (display-encoded).
    let expected = volume.light([0.0, 0.0, 0.5], [0.0, 0.0, 1.0])[0];
    // Scene lighting is display-space: a white pigment shows the light.
    assert!(
        (f32::from(lit[0]) - expected * 255.0).abs() <= 3.0,
        "{lit:?} {expected}"
    );
    // A dark room keeps its black silhouettes.
    let unlit = LightVolume::bake(&room(0, 0), 0.5, 100_000).unwrap();
    renderer.set_light_volume(&gpu.device, &gpu.queue, Some(&unlit))?;
    assert_eq!(gpu.centre(&mut renderer, &mesh, &dark)?, [0, 0, 0]);
    // A bright sun is never dimmed by a dimmer volume: maps with daylight
    // keep their look.
    let daylight = Camera {
        sun_color: [0.0; 4],
        ambient: [0.6, 0.6, 0.6, 0.0],
        ..Default::default()
    };
    renderer.set_light_volume(&gpu.device, &gpu.queue, None)?;
    let before = gpu.centre(&mut renderer, &mesh, &daylight)?;
    let dim = LightVolume::bake(&room(60, 60), 0.5, 100_000).unwrap();
    renderer.set_light_volume(&gpu.device, &gpu.queue, Some(&dim))?;
    assert_eq!(gpu.centre(&mut renderer, &mesh, &daylight)?, before);
    Ok(())
}

fn content() -> PathBuf {
    std::env::var_os("BRI_CONTENT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        PathBuf::from,
    )
}

/// Centre of a scene node's static shape geometry.
fn node_centre(map: &bri_render::scene_loader::MapScene, node: u32) -> Option<Vec3> {
    let range = map.shape_indices.get(&node)?;
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for &i in &map.scene.indices[range.start as usize..range.end as usize] {
        let p = Vec3::from(map.scene.vertices[i as usize].position);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    Some((lo + hi) * 0.5)
}

fn scene_nodes(bundle: &std::path::Path, map_id: &str) -> Result<Vec<serde_json::Value>> {
    let index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("bundle.json"))?)?;
    let file = index["maps"]
        .as_array()
        .context("maps")?
        .iter()
        .find(|m| m["id"] == map_id)
        .context("map")?["file"]
        .as_str()
        .context("file")?
        .to_owned();
    let scene: serde_json::Value = serde_json::from_slice(&std::fs::read(bundle.join(file))?)?;
    Ok(scene["nodes"].as_array().context("nodes")?.clone())
}

fn shape_centres(
    bundle: &std::path::Path,
    map: &bri_render::scene_loader::MapScene,
    map_id: &str,
    datablock: &str,
) -> Result<Vec<Vec3>> {
    Ok(scene_nodes(bundle, map_id)?
        .iter()
        .enumerate()
        .filter(|(_, n)| n["properties"]["datablock"] == datablock)
        .filter_map(|(i, _)| node_centre(map, i as u32))
        .collect())
}

/// The client's bake settings (`LightVolumeState` in the client app).
const MIN_CELL: f32 = 2.0;
const MAX_CELLS: usize = 1_000_000;

#[test]
#[ignore = "requires locally converted map-bundle-017; bakes every stock map"]
fn stock_map_lamps_light_their_surroundings() -> Result<()> {
    let bundle = content().join("map-bundle-017");
    let index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("bundle.json"))?)?;
    for record in index["maps"].as_array().context("maps")? {
        let id = record["id"].as_str().context("id")?;
        let map = load_map_bundle(&bundle, id)?;
        let started = std::time::Instant::now();
        let volume = LightVolume::bake(&map.scene, MIN_CELL, MAX_CELLS);
        let Some(volume) = volume else {
            eprintln!("{id}: no lightmapped surfaces");
            continue;
        };
        eprintln!(
            "{id}: {:?} cells of {:.2} in {:?}; sun {:?} ambient {:?}",
            volume.dims,
            volume.cell,
            started.elapsed(),
            map.scene.sun_color,
            map.scene.ambient
        );
        // Bounded by work, not time: under two thirds of casting from every
        // cell (the gate's debug build took 42 s when every cell cast).
        let every_cell = volume.texels.len() as u64 * RAYS_PER_CELL;
        eprintln!("  rays {} of {every_cell}", volume.rays);
        assert!(volume.rays * 3 < every_cell * 2);
        // A player (feet to head, about 2.7 units) around each light source.
        // Around the Bedroom bulb (a player on the shade's bars stands just
        // above it) and under the Kitchen ceiling lights.
        let around = [
            Vec3::new(0.0, 1.5, 0.0),
            Vec3::new(0.0, -3.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, -2.0),
        ];
        let under = [Vec3::new(0.0, -4.0, 0.0), Vec3::new(0.0, -8.0, 0.0)];
        for (datablock, offsets, least) in [
            ("lightBulbA", &around[..], 0.45),
            ("fluorescentLight", &under[..], 0.25),
        ] {
            for centre in shape_centres(&bundle, &map, id, datablock)? {
                let mut darkest = f32::INFINITY;
                for &offset in offsets {
                    let light = volume.light((centre + offset).to_array(), [0.0, 0.0, 1.0]);
                    eprintln!("  {datablock} {:?} {offset}: {light:?}", centre.to_array());
                    darkest = darkest.min(luminance(light));
                }
                assert!(darkest > least, "{id} {datablock} at {centre}: {darkest}");
            }
        }
    }
    Ok(())
}

/// A white vertex-lit box, `size` across, centred at `centre`.
fn stand_in_box(centre: Vec3, size: Vec3) -> SceneData {
    let mut scene = SceneData::default();
    for axis in 0..3 {
        for side in [-1.0f32, 1.0] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let mut normal = Vec3::ZERO;
            normal[axis] = side;
            let base = scene.vertices.len() as u32;
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut p = Vec3::ZERO;
                p[axis] = side;
                p[u] = a;
                p[v] = b;
                scene.vertices.push(SceneVertex {
                    position: (centre + p * size * 0.5).to_array(),
                    normal: normal.to_array(),
                    uv: [0.0; 2],
                    lightmap_uv: [0.0; 2],
                    color: [1.0; 4],
                    fx: [0.0; 4],
                });
            }
            let order = if side > 0.0 {
                [0, 1, 2, 0, 2, 3]
            } else {
                [0, 2, 1, 0, 3, 2]
            };
            scene.indices.extend(order.map(|i| base + i));
        }
    }
    scene.materials.push(Material::vertex_lit("stand-in", 0));
    scene.materials[0].double_sided = true;
    scene.batches.push(MeshBatch {
        indices: 0..scene.indices.len() as u32,
        material: 0,
        center: centre.to_array(),
    });
    scene
}

/// Offscreen before/after of a player-sized box on the BedroomDark lamp
/// shade's bars, written to BRI_EVIDENCE when set. Never opens a window.
#[test]
#[ignore = "requires locally converted map-bundle-017 and an offscreen GPU"]
fn bedroom_dark_lamp_render() -> Result<()> {
    let bundle = content().join("map-bundle-017");
    let id = "v20/add-ons/map_bedroomdark/bedroomdark.mis";
    let map = load_map_bundle(&bundle, id)?;
    let bulb = shape_centres(&bundle, &map, id, "lightBulbA")?[0];
    let gpu = Gpu::new()?;
    let mut renderer = SceneRenderer::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let world = renderer.upload(&gpu.device, &gpu.queue, &map.scene)?;
    let player = bulb + Vec3::new(0.0, 3.0, 0.0);
    let body = renderer.upload(
        &gpu.device,
        &gpu.queue,
        &stand_in_box(player, Vec3::new(1.2, 2.7, 0.8)),
    )?;
    let (width, height) = (512u32, 384u32);
    let eye = player + Vec3::new(2.5, 1.0, 3.0);
    let mut camera = Camera::perspective(
        eye.to_array(),
        player.to_array(),
        width as f32 / height as f32,
        1.1,
        0.1,
        2000.0,
    );
    camera.apply_environment(&map.scene);
    let shoot = |renderer: &mut SceneRenderer| -> Result<Vec<u8>> {
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = create_depth(&gpu.device, width, height);
        let row = width * 4;
        let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        renderer.update_camera(&gpu.queue, &camera);
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        renderer.render(
            &mut encoder,
            &target.create_view(&Default::default()),
            &depth.create_view(&Default::default()),
            &[&world, &body],
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
        let pixels = readback.slice(..).get_mapped_range()?.to_vec();
        Ok(pixels)
    };
    let before = shoot(&mut renderer)?;
    let volume = LightVolume::bake(&map.scene, MIN_CELL, MAX_CELLS).context("volume")?;
    renderer.set_light_volume(&gpu.device, &gpu.queue, Some(&volume))?;
    let after = shoot(&mut renderer)?;
    let centre = ((height / 2 * width + width / 2) * 4) as usize;
    eprintln!(
        "centre before {:?} after {:?}",
        &before[centre..centre + 3],
        &after[centre..centre + 3]
    );
    if let Some(dir) = std::env::var_os("BRI_EVIDENCE") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir)?;
        for (name, pixels) in [("before", &before), ("after", &after)] {
            image::save_buffer(
                dir.join(format!("bedroomdark-lamp-{name}.png")),
                pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )?;
        }
    }
    assert!(after[centre] > before[centre] + 60);
    Ok(())
}
