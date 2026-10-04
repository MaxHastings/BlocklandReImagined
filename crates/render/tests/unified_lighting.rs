//! Unified sunlight removes only the baked sun; map geometry filters live sun shadows.
//! Split by behavior so serial software-GPU verification fits the per-binary watchdog.

#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_render::{scene::*, shadow::ShadowSettings};
use common::*;
use glam::{Mat4, Vec3};

#[test]
fn live_shadows_remove_only_baked_sun() -> Result<()> {
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let sun = Vec3::new(0.0, -1.0, 1.0);
    let map = renderer.upload(&device, &queue, &floor(sun, 0.6))?;
    // A slab over a band across both halves.
    let slab = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-12., 3.0, -1.5), Vec3::new(12., 3.3, 1.5)),
    )?;
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = sun.extend(0.).to_array();
    camera.sun_color = [0.6, 0.6, 0.6, 0.];
    camera.ambient = [0.3, 0.3, 0.3, 0.];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    // Where the slab's shadow lands (it leans with the sun) and open floor.
    let shade_z = 3.15;
    let points = [
        Vec3::new(-5.0, 0.0, shade_z),
        Vec3::new(-5.0, 0.0, 7.0),
        Vec3::new(5.0, 0.0, shade_z),
        Vec3::new(5.0, 0.0, 7.0),
    ];
    let mut run = |mode: f32| -> Result<[i32; 4]> {
        camera.ambient[3] = mode;
        renderer.update_camera(&queue, &camera);
        let pixels = render(
            &device,
            &queue,
            &mut renderer,
            &target,
            &[&map, &slab],
            &[&slab],
            &[],
        )?;
        Ok(points.map(|p| at(&pixels, p)))
    };
    let [lit_under, lit_open, dark_under, dark_open] = run(2.0)?;
    // Unified: baked shade stays as baked; baked sun under the slab goes
    // down to the static light, the same as the baked shade.
    assert!(
        (dark_under - dark_open).abs() <= 2,
        "{dark_under} {dark_open}"
    );
    assert!(lit_open > dark_open + 60, "{lit_open} {dark_open}");
    assert!(
        (lit_under - dark_open).abs() <= 3,
        "{lit_under} {dark_open}"
    );
    // Classic keeps v20's fixed share: it darkens the baked shade again.
    let [_, _, classic_under, classic_open] = run(0.0)?;
    assert_eq!(classic_open, dark_open);
    assert!(
        classic_under < classic_open - 10,
        "{classic_under} {classic_open}"
    );
    Ok(())
}

/// A map with a room roof far above a brick floor, open over a square
/// "window" the sun falls straight through. The visibility volume says the
/// sun reaches every cell (as a coarse 4-unit volume near a window edge
/// can): only the map's own surfaces, through the map layer, shade objects.
#[test]
fn map_walls_shade_objects_from_the_sun_with_a_filtered_edge() -> Result<()> {
    use bri_render::map_lighting::{MapLighting, VisibilityVolume};
    let (device, queue) = gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (width, height) = (256u32, 256u32);
    let mut renderer = SceneRenderer::with_settings(&device, format, 1, Some(ShadowSettings::LOW));
    let dims = [8u32, 4, 8];
    let mut texel = [0u8; 8];
    texel[0] = 255;
    renderer.set_map_lighting(
        &device,
        &queue,
        Some(&MapLighting {
            lights: vec![],
            report: Default::default(),
            visibility: VisibilityVolume {
                origin: [-16.0, -4.0, -16.0],
                cell: 4.0,
                dims,
                texels: vec![texel; dims.iter().map(|d| *d as usize).product()],
            },
            residual: bri_render::light_volume::LightVolume {
                origin: [0.0; 3],
                cell: 1.0,
                dims: [1; 3],
                texels: vec![[0; 4]],
                rays: 0,
            },
            residual_all: bri_render::light_volume::LightVolume {
                origin: [0.0; 3],
                cell: 1.0,
                dims: [1; 3],
                texels: vec![[0; 4]],
                rays: 0,
            },
            leaks: vec![],
            dynamic: vec![],
        }),
        false,
    )?;
    let floor = renderer.upload(
        &device,
        &queue,
        &cuboid(Vec3::new(-10.0, -0.3, -10.0), Vec3::new(10.0, 0.0, 10.0)),
    )?;
    // Roof pieces around a 4x4 opening, farther above the floor than live
    // casters reach toward the sun (the map layer reaches past it).
    let wall = |min: Vec3, max: Vec3| {
        let mut data = cuboid(min, max);
        data.images = vec![SceneImage::white()];
        data.materials[0] = Material::surface("roof", 0, 0);
        renderer.upload(&device, &queue, &data)
    };
    let (y0, y1) = (600.0, 601.0);
    let roof = [
        wall(Vec3::new(-40.0, y0, -40.0), Vec3::new(-2.0, y1, 40.0))?,
        wall(Vec3::new(2.0, y0, -40.0), Vec3::new(40.0, y1, 40.0))?,
        wall(Vec3::new(-2.0, y0, -40.0), Vec3::new(2.0, y1, -2.0))?,
        wall(Vec3::new(-2.0, y0, 2.0), Vec3::new(2.0, y1, 40.0))?,
    ];
    let map: Vec<&GpuScene> = roof.iter().collect();
    let mut camera = Camera::perspective([0., 22., 0.1], [0., 0., 0.], 1.0, 1.4, 0.05, 400.0);
    camera.sun_direction = [0.0, -1.0, 0.0, 0.0];
    camera.sun_color = [0.6, 0.6, 0.6, 0.];
    camera.ambient = [0.2, 0.2, 0.2, 2.0];
    let target = color_target(&device, format, width, height);
    let view_projection = Mat4::from_cols_array(&camera.view_projection);
    let at = |pixels: &[u8], point: Vec3| {
        let ndc = view_projection.project_point3(point);
        let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
        let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
        i32::from(pixels[(y * width as usize + x) * 4 + 1])
    };
    // Under the opening, inside its edge, outside its edge, under the roof.
    let points = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.6, 0.0, 0.0),
        Vec3::new(2.4, 0.0, 0.0),
        Vec3::new(6.0, 0.0, 6.0),
    ];
    let mut frames = vec![];
    for _ in 0..2 {
        renderer.update_camera(&queue, &camera);
        let pixels = render_with_map(
            &device,
            &queue,
            &mut renderer,
            &target,
            &[&floor],
            &[],
            &[],
            &map,
        )?;
        frames.push(points.map(|p| at(&pixels, p)));
    }
    assert_eq!(frames[0], frames[1]);
    let [open, inside, outside, roofed] = frames[0];
    // Sunlit (0.2 + 0.6, and the sun's highlight 0.6 x 0.3 with the sun and
    // eye straight above) through the opening, ambient alone under the
    // roof, and the edge turns within under a unit, not a 4-unit volume cell.
    assert!((open - 250).abs() <= 4, "{open}");
    assert!((inside - open).abs() <= 4, "{inside} {open}");
    assert!((roofed - 51).abs() <= 4, "{roofed}");
    assert!((outside - roofed).abs() <= 4, "{outside} {roofed}");
    // Without the map layer the volume's sun stands in: sunlit everywhere
    // (with less of the highlight away from straight below the eye).
    renderer.update_camera(&queue, &camera);
    let pixels = render(&device, &queue, &mut renderer, &target, &[&floor], &[], &[])?;
    let unroofed = at(&pixels, points[3]);
    assert!(unroofed >= 200 && unroofed <= open + 4, "{unroofed} {open}");
    Ok(())
}
