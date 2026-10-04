//! Actual avatar-editor portrait textures, offscreen only.
#[macro_use]
mod support;
use anyhow::{Result, ensure};
use bri_client::{avatar::Preview, platform::RenderContext};
use bri_ui::{
    draw::{DrawList, Filter},
    geom::Rect,
    gpu::UiRenderer,
    pack::{Pack, TexKey},
};
use support::{avatar_fixture::AvatarFixture, gpu};

synthetic_and_content!(AvatarFixture: the_editor_portrait_plays_and_loops_the_authored_run);

fn the_editor_portrait_plays_and_loops_the_authored_run(f: &AvatarFixture) -> Result<()> {
    let gpu = gpu::turn()?;
    let mut ui = UiRenderer::new(&gpu.device, &gpu.queue);
    let pack = Pack::from_parts(Default::default(), Default::default());
    let mut preview = Preview::new(&gpu.device);
    let size = Preview::SIZE;
    let mut draw = DrawList::new(Rect::new(0, 0, size.0 as i32, size.1 as i32));
    draw.image(
        TexKey::External(Preview::ID),
        [0.0, 0.0, 1.0, 1.0],
        [0.0, 0.0, size.0 as f32, size.1 as f32],
        [255; 4],
        Filter::Nearest,
    );
    let appearance = f.assets.package.defaults.clone();
    let run = f.assets.rig.sequence("run").unwrap();
    ensure!(
        run.looping && run.duration > 0.0,
        "the authored run must loop"
    );
    let period = run.duration as f64 / 0.85;
    let mut captures = Vec::new();
    for (label, time) in [("start", 0.0), ("stride", period * 0.25), ("loop", period)] {
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen portrait registration"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        preview.render(
            &f.assets,
            &appearance,
            [0.3, 0.6, 2.52],
            4.34,
            time,
            &mut RenderContext {
                device: &gpu.device,
                queue: &gpu.queue,
                encoder: &mut encoder,
                target: &view,
                format: wgpu::TextureFormat::Rgba8Unorm,
                size: (1, 1),
                ui_renderer: &mut ui,
            },
        )?;
        gpu.queue.submit([encoder.finish()]);
        let rgba = gpu.render_rgba(&mut ui, &pack, &draw, size, 1.0, [0.0; 4])?;
        ensure!(
            rgba.chunks_exact(4).filter(|p| p[3] > 128).count() > 100,
            "the actual preview texture must contain the authored avatar"
        );
        image::save_buffer(
            f.out("avatar-editor-preview")?.join(format!("{label}.png")),
            &rgba,
            size.0,
            size.1,
            image::ColorType::Rgba8,
        )?;
        captures.push(rgba);
    }
    // Pixel evidence, including the legs in the lower half, rather than an
    // animation-name or joint-presence assertion.
    let lower = (size.0 * size.1 / 2 * 4) as usize;
    let changed = captures[0][lower..]
        .chunks_exact(4)
        .zip(captures[1][lower..].chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    ensure!(
        changed > 10,
        "the portrait's walking legs did not move: {changed} pixels"
    );
    let loop_error = captures[0]
        .iter()
        .zip(&captures[2])
        .map(|(a, b)| a.abs_diff(*b) as f64)
        .sum::<f64>()
        / captures[0].len() as f64;
    ensure!(
        loop_error < 0.05,
        "the authored portrait loop did not return: mean error={loop_error}"
    );
    println!("actual portrait: changed lower pixels={changed}, loop mean error={loop_error}");
    Ok(())
}
