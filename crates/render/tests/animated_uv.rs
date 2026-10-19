//! UV motion stays on the texture, with the actual native material shader.
#[path = "unified_lighting/common.rs"]
mod common;
use anyhow::Result;
use bri_render::scene::*;

#[test]
fn authored_texture_rotation_uses_the_render_clock_and_declared_center() -> Result<()> {
    let (device, queue) = common::gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = SceneRenderer::new(&device, format);
    let target = common::color_target(&device, format, 64, 64);
    let mut material = Material::surface("animated panel", 0, 0);
    material.kind = MaterialKind::Unlit;
    material.clamp_nearest = true;
    material.uv_motion = Some(bri_content::shape::UvMotion {
        center: [0.5; 2],
        rotation_rate_degrees: 360.,
        ..Default::default()
    });
    let data = SceneData {
        images: vec![SceneImage {
            label: "quadrants".into(),
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
            srgb: true,
        }],
        materials: vec![material],
        vertices: [
            [-10., 0., -10.],
            [-10., 0., 10.],
            [10., 0., 10.],
            [10., 0., -10.],
        ]
        .map(|position| SceneVertex {
            position,
            normal: [0., 1., 0.],
            uv: [0.25; 2],
            lightmap_uv: [0.; 2],
            color: [1.; 4],
            fx: [0.; 4],
        })
        .to_vec(),
        indices: vec![0, 1, 2, 0, 2, 3],
        batches: vec![MeshBatch {
            indices: 0..6,
            material: 0,
            center: [0.; 3],
        }],
        ..Default::default()
    };
    let gpu = renderer.upload(&device, &queue, &data)?;
    for modern in [false, true] {
        let mut observed = Vec::new();
        for seconds in [0., 0.25, 1.] {
            let mut camera = Camera::perspective([0., 3., 0.01], [0.; 3], 1., 1., 0.05, 100.);
            camera.atmosphere[2] = seconds;
            if modern {
                camera.shading[0] = 1.;
                camera.ambient[3] = 2.;
            }
            renderer.update_camera(&queue, &camera);
            let pixels =
                common::render(&device, &queue, &mut renderer, &target, &[&gpu], &[], &[])?;
            observed.push(pixels[(32 * 64 + 32) * 4..][..3].to_vec());
        }
        assert!(
            observed[0][0] > 200 && observed[0][1] < 10,
            "{:?}",
            observed
        );
        assert!(
            observed[1][1] > 200 && observed[1][0] < 10,
            "{:?}",
            observed
        );
        assert_eq!(
            observed[0], observed[2],
            "a full rotation restores the panel without changing geometry"
        );
    }
    Ok(())
}

/// Which quadrant of the 2x2 test image a panel shows at the centre pixel: 0 red, 1 green.
fn wave_quadrants(wave: bri_content::shape::UvWave, times: &[f32]) -> Result<Vec<u8>> {
    use bri_content::shape::{UvMotion, UvWave};
    let (device, queue) = common::gpu()?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = SceneRenderer::new(&device, format);
    let target = common::color_target(&device, format, 64, 64);
    let mut material = Material::surface("wave panel", 0, 0);
    material.kind = MaterialKind::Unlit;
    material.clamp_nearest = true;
    material.uv_motion = Some(UvMotion {
        waves: [wave, UvWave::default()],
        ..Default::default()
    });
    let data = SceneData {
        images: vec![SceneImage {
            label: "quadrants".into(),
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
            srgb: true,
        }],
        materials: vec![material],
        vertices: [
            [-10., 0., -10.],
            [-10., 0., 10.],
            [10., 0., 10.],
            [10., 0., -10.],
        ]
        .map(|position| SceneVertex {
            position,
            normal: [0., 1., 0.],
            uv: [0.25; 2],
            lightmap_uv: [0.; 2],
            color: [1.; 4],
            fx: [0.; 4],
        })
        .to_vec(),
        indices: vec![0, 1, 2, 0, 2, 3],
        batches: vec![MeshBatch {
            indices: 0..6,
            material: 0,
            center: [0.; 3],
        }],
        ..Default::default()
    };
    let gpu = renderer.upload(&device, &queue, &data)?;
    let mut out = Vec::new();
    for &seconds in times {
        let mut camera = Camera::perspective([0., 3., 0.01], [0.; 3], 1., 1., 0.05, 100.);
        camera.atmosphere[2] = seconds;
        renderer.update_camera(&queue, &camera);
        let pixels = common::render(&device, &queue, &mut renderer, &target, &[&gpu], &[], &[])?;
        let p = &pixels[(32 * 64 + 32) * 4..][..3];
        assert!(p[0] > 200 && p[1] < 10 || p[1] > 200 && p[0] < 10, "{p:?}");
        out.push(u8::from(p[1] > 200));
    }
    Ok(out)
}

#[test]
fn uv_waves_follow_the_engine_formulas() -> Result<()> {
    use bri_content::shape::{UvWave, UvWaveKind};
    // Texture u starts at 0.25 (red); a pan of 0.5 reaches 0.75 (green) at a quarter cycle.
    let pan = UvWave {
        kind: UvWaveKind::Pan,
        rate: 1.,
        phase: 0.,
        amplitude: 0.5,
    };
    assert_eq!(wave_quadrants(pan, &[0., 0.25, 0.5])?, [0, 1, 0]);
    // Stretch about the origin: u x (1 + 2 sin) = 0.75 at a quarter cycle.
    let stretch = UvWave {
        kind: UvWaveKind::Stretch,
        rate: 1.,
        phase: 0.,
        amplitude: 2.,
    };
    assert_eq!(wave_quadrants(stretch, &[0., 0.25])?, [0, 1]);
    // Stretch-repeat: sin(pi/2 x cycle) reaches 1 only at the cycle's end.
    let repeat = UvWave {
        kind: UvWaveKind::StretchRepeat,
        rate: 1.,
        phase: 0.,
        amplitude: 2.,
    };
    assert_eq!(wave_quadrants(repeat, &[0., 0.99])?, [0, 1]);
    // Jitter: a random offset in [0, 0.5), so both quadrants appear, and it holds
    // the previous cycle's last value before `phase`.
    let jitter = UvWave {
        kind: UvWaveKind::Jitter,
        rate: 1.,
        phase: 0.5,
        amplitude: 0.5,
    };
    let times: Vec<f32> = (0..40).map(|k| 0.5 + 0.01 * k as f32 + 0.0003).collect();
    let seen = wave_quadrants(jitter, &times)?;
    let greens = seen.iter().filter(|&&q| q == 1).count();
    assert!((5..35).contains(&greens), "{seen:?}");
    let held = wave_quadrants(jitter, &[3.1, 3.4])?;
    assert_eq!(held[0], held[1], "held through the first half of the cycle");
    Ok(())
}
