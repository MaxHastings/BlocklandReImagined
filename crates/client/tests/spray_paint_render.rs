//! The spray can's nozzle mist and paint splash take the selected palette
//! colour (`setSprayCanColor`'s `color<N>Paint*` copies of the blue can).
use anyhow::{Result, ensure};
use bri_client::weapon_effects::WeaponEffects;
use bri_fx_runtime::{gpu::EffectsRenderer, *};
use bri_sim::presentation::Cue;
use bri_ui::gpu::Headless;
use bri_weapons::Pack;
use glam::Vec3;
use std::{path::Path, sync::Arc};

/// v20 default colorset entries: opaque red, green, yellow and white, and
/// the translucent blue and black.
const PALETTE: [[f32; 4]; 6] = [
    [0.9, 0., 0., 1.],
    [0., 0.5, 0.25, 1.],
    [0.9, 0.9, 0., 1.],
    [1., 1., 1., 1.],
    [0., 0.2, 0.64, 0.7],
    [0.1, 0.1, 0.1, 0.7],
];

fn cue(id: u64, definition: &str, seconds: f32) -> Cue {
    serde_json::from_value(serde_json::json!({"id":id,"tick":1,"position":[0.,0.,0.],"kind":{"WeaponEffect":{
        "source":{"Actor":1},"definition":definition,"node":if seconds > 0. {"muzzleNode"} else {""},"seconds":seconds,
        "image":null,"hand":null,"direction":null,"scale":1.}}})).unwrap()
}
fn pose(_: &Cue) -> Option<SourceTransform> {
    Some(SourceTransform::default())
}
fn camera() -> Camera {
    let eye = Vec3::new(0., 1.8, 18.);
    let view = glam::camera::rh::view::look_at_mat4(eye, Vec3::new(0., 1.8, 0.), Vec3::Y);
    let inverse = view.inverse();
    Camera {
        view_projection: glam::camera::rh::proj::directx::orthographic(
            -2.5, 2.5, -2.5, 2.5, 0.1, 200.,
        ) * view,
        position: eye,
        right: inverse.x_axis.truncate(),
        up: inverse.y_axis.truncate(),
    }
}
fn effects(root: &Path) -> Result<WeaponEffects> {
    let pack = EffectsPack::load(root.join("content/effects-runtime-pack-004"))?;
    let weapons = Arc::new(Pack::from_json(&std::fs::read(
        root.join("content/weapons-pack-008/weapons.json"),
    )?)?);
    let mut fx = WeaponEffects::new(pack, weapons, EffectsLimits::default())?;
    fx.set_palette(&PALETTE);
    Ok(fx)
}

#[test]
#[ignore = "requires original converted packs and an offscreen GPU adapter"]
fn spray_mist_and_splash_use_the_palette_colour_offscreen() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut frames = Vec::new();
    let mut pack = None;
    // Rows: nozzle mist (`bluePaintEmitter`), then the paint splash.
    for effect in ["PaintEmitter", "PaintExplosion"] {
        for (index, color) in PALETTE.iter().enumerate() {
            let mut fx = effects(&root)?;
            let (definition, seconds) = if effect == "PaintEmitter" {
                (format!("color{index}{effect}"), 0.3)
            } else {
                (format!("color{index}{effect}"), 0.)
            };
            fx.cues(&[cue(1, &definition, seconds)], pose)?;
            for _ in 0..12 {
                fx.advance(if seconds > 0. { 0.02 } else { 0.01 }, Vec3::ZERO, pose)?;
            }
            ensure!(
                fx.diagnostics.missing_bindings == 0 && fx.diagnostics.accepted_cues == 1,
                "{definition}: {:?}",
                fx.diagnostics.messages
            );
            let frame = fx.world().snapshot(&camera());
            ensure!(!frame.particles.is_empty(), "{definition} emitted nothing");
            let opaque = color[3] > 0.99;
            let rgb = if !opaque && color[..3].iter().all(|c| *c < 8. / 255.) {
                [8. / 255.; 3]
            } else {
                [color[0], color[1], color[2]]
            };
            for p in &frame.particles {
                ensure!(
                    p.color.truncate().abs_diff_eq(Vec3::from(rgb), 1e-6),
                    "{definition} particle {:?} is not palette {rgb:?}",
                    p.color
                );
                if effect == "PaintExplosion" {
                    ensure!(
                        (p.blend == BlendMode::Alpha) == opaque,
                        "{definition}: translucent paint is additive (useInvAlpha 0)"
                    );
                }
            }
            frames.push(frame);
            pack.get_or_insert_with(|| fx.world().pack().clone());
        }
    }
    // The blue can itself, unpainted, for comparison.
    let mut fx = effects(&root)?;
    fx.cues(&[cue(1, "bluePaintEmitter", 0.3)], pose)?;
    for _ in 0..12 {
        fx.advance(0.02, Vec3::ZERO, pose)?;
    }
    let navy = fx.world().snapshot(&camera());
    ensure!(
        navy.particles.iter().all(|p| p
            .color
            .truncate()
            .abs_diff_eq(Vec3::new(0., 0.317, 0.745), 1e-3)),
        "Authored blue can mist changed"
    );

    let gpu = Headless::new()?;
    let pack = pack.unwrap();
    let mut renderer = EffectsRenderer::new(
        &gpu.device,
        &gpu.queue,
        &pack,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Depth32Float,
        1,
        70000,
    )?;
    let (cols, tile) = (PALETTE.len() as u32, 200u32);
    let (width, height) = (cols * tile, 2 * tile);
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = |format, usage| {
        gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("spray paint gallery"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let color = texture(
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let depth = texture(
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let (cv, dv) = (
        color.create_view(&Default::default()),
        depth.create_view(&Default::default()),
    );
    for (i, frame) in frames.iter().enumerate() {
        renderer.prepare(&gpu.queue, &camera(), frame)?;
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("spray paint gallery"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &cv,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: if i == 0 {
                            wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.45,
                                g: 0.45,
                                b: 0.47,
                                a: 1.,
                            })
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &dv,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let (x, y) = ((i as u32 % cols) * tile, (i as u32 / cols) * tile);
            pass.set_viewport(x as f32, y as f32, tile as f32, tile as f32, 0., 1.);
            pass.set_scissor_rect(x, y, tile, tile);
            renderer.render(&mut pass);
        }
        gpu.queue.submit([encoder.finish()]);
    }
    let stride = (width * 4).div_ceil(256) * 256;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("spray paint readback"),
        size: u64::from(stride) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        color.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
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
    let mapped = readback.slice(..).get_mapped_range()?;
    let mut pixels = Vec::new();
    for row in mapped.chunks(stride as usize) {
        pixels.extend_from_slice(&row[..width as usize * 4]);
    }
    let out = root.join("artifacts/spray-paint");
    std::fs::create_dir_all(&out)?;
    image::save_buffer(
        out.join("spray-paint-colors.png"),
        &pixels,
        width,
        height,
        image::ColorType::Rgba8,
    )?;
    // The opaque red mist tile must contain clearly red pixels.
    let red = (0..tile * tile)
        .filter(|i| {
            let p = &pixels[(((i / tile) * width + i % tile) * 4) as usize..][..3];
            p[0] > p[1].saturating_add(40) && p[0] > p[2].saturating_add(40)
        })
        .count();
    ensure!(red > 50, "Red spray tile has {red} red pixels");
    Ok(())
}
