//! Sampled normal directions must follow UV orientation without acquiring UV
//! scale. All geometry, normal texels and reference normals here are synthetic.
#[path = "unified_lighting/common.rs"]
mod common;

use anyhow::Result;
use bri_content::shape::Surface;
use bri_render::scene::*;
use glam::{Quat, Vec2, Vec3};

const SIZE: u32 = 48;
const NORMALS: [[u8; 4]; 3] = [
    [190, 128, 240, 255],
    [128, 190, 240, 255],
    [179, 153, 240, 255],
];

fn direction(texel: [u8; 4], tangent: Vec3, down: Vec3, normal: Vec3) -> Vec3 {
    let sample = Vec3::new(texel[0] as f32, texel[1] as f32, texel[2] as f32) / 127.5 - Vec3::ONE;
    // Existing engine convention: red follows +u, green follows decreasing v.
    (tangent * sample.x - down * sample.y + normal * sample.z).normalize()
}

fn quad(texel: [u8; 4], uv: [Vec2; 2], turn: Quat, reference: Option<Vec3>) -> SceneData {
    let surface = Surface {
        normal: Some(1),
        normal_scale: if reference.is_some() { 0. } else { 1. },
        metallic: 0.,
        roughness: 1.,
        metallic_roughness: None,
        occlusion: None,
        occlusion_strength: 1.,
        emissive_map: None,
        emissive: [0.; 3],
        detail: None,
        detail_scale: 1.,
        reflection: None,
        lightmap: None,
        lightmap_strength: 1.0,
        lightmap_exact: false,
    };
    let mut scene = SceneData {
        images: vec![
            SceneImage {
                label: "white".into(),
                width: 1,
                height: 1,
                rgba: vec![255; 4],
                srgb: true,
            },
            SceneImage {
                label: "constant normal".into(),
                width: 1,
                height: 1,
                rgba: texel.to_vec(),
                srgb: false,
            },
        ],
        ..Default::default()
    };
    let maps = SurfaceMap::ALL.map(|map| {
        if map == SurfaceMap::Normal {
            1
        } else {
            scene.images.push(map.stand_in());
            scene.images.len() - 1
        }
    });
    scene
        .materials
        .push(Material::physical("normal", 0, maps, &surface));
    let normal = reference.unwrap_or(turn * Vec3::Z);
    for xy in [
        Vec2::new(-1., -1.),
        Vec2::new(1., -1.),
        Vec2::new(1., 1.),
        Vec2::new(-1., 1.),
    ] {
        scene.vertices.push(SceneVertex {
            position: (turn * xy.extend(0.)).to_array(),
            normal: normal.to_array(),
            uv: [xy.dot(uv[0]), xy.dot(uv[1])],
            lightmap_uv: [0.; 2],
            color: [1.; 4],
            fx: [0.; 4],
        });
    }
    scene.indices = vec![0, 1, 2, 0, 2, 3];
    scene.batches.push(MeshBatch {
        zone: None,
        material: 0,
        indices: 0..6,
        center: [0.; 3],
    });
    scene
}

struct Fixture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: SceneRenderer,
    target: wgpu::Texture,
}

impl Fixture {
    fn new() -> Result<Self> {
        let (device, queue) = common::gpu()?;
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let renderer = SceneRenderer::new(&device, format);
        let target = common::color_target(&device, format, SIZE, SIZE);
        Ok(Self {
            device,
            queue,
            renderer,
            target,
        })
    }

    fn pixels(&mut self, scene: &SceneData, mode: f32, turn: Quat) -> Result<Vec<u8>> {
        let mut camera = Camera::perspective(
            (turn * Vec3::new(0., 0., 4.)).to_array(),
            [0.; 3],
            1.,
            0.8,
            0.01,
            100.,
        );
        camera.sun_direction = (turn * Vec3::new(0.35, 0.7, -0.6).normalize())
            .extend(0.)
            .to_array();
        camera.sun_color = [0.8, 0.8, 0.8, 1.];
        camera.ambient = [0., 0., 0., mode];
        // Mode 0 uses scene_original; mode 3 with shading uses scene.wgsl.
        camera.shading = [if mode == 0. { 0. } else { 1. }, 0., 0., 0.];
        self.renderer.update_camera(&self.queue, &camera);
        let gpu = self.renderer.upload(&self.device, &self.queue, scene)?;
        common::render(
            &self.device,
            &self.queue,
            &mut self.renderer,
            &self.target,
            &[&gpu],
            &[],
            &[],
        )
    }
}

fn matches_reference(actual: &[u8], reference: &[u8], label: &str) {
    let mut worst = 0;
    for y in SIZE / 2 - 2..SIZE / 2 + 2 {
        for x in SIZE / 2 - 2..SIZE / 2 + 2 {
            let at = ((y * SIZE + x) * 4) as usize;
            for channel in 0..3 {
                worst = worst.max(actual[at + channel].abs_diff(reference[at + channel]));
            }
        }
    }
    assert!(
        worst <= 2,
        "{label}: interior RGB differs from explicit normal by {worst} bytes"
    );
    assert!(
        reference[((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize] > 10,
        "reference must be visibly lit"
    );
}

#[test]
fn positive_uv_scales_keep_constant_normal_directions_in_both_renderers() -> Result<()> {
    let mut fixture = Fixture::new()?;
    for mode in [0., 3.] {
        for texel in NORMALS {
            let normal = direction(texel, Vec3::X, Vec3::NEG_Y, Vec3::Z);
            let reference = fixture.pixels(
                &quad(texel, [Vec2::X, Vec2::NEG_Y], Quat::IDENTITY, Some(normal)),
                mode,
                Quat::IDENTITY,
            )?;
            for scale in [
                Vec2::ONE,
                Vec2::new(2., 1.),
                Vec2::new(1., 2.),
                Vec2::new(0.5, 3.),
                Vec2::splat(2.),
            ] {
                let actual = fixture.pixels(
                    &quad(
                        texel,
                        [Vec2::X * scale.x, Vec2::NEG_Y * scale.y],
                        Quat::IDENTITY,
                        None,
                    ),
                    mode,
                    Quat::IDENTITY,
                )?;
                matches_reference(
                    &actual,
                    &reference,
                    &format!("mode {mode}, texel {texel:?}, scale {scale:?}"),
                );
            }
        }
    }
    Ok(())
}

#[test]
fn mirrored_rotated_and_skewed_uvs_follow_their_handed_frame() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let diagonal = NORMALS[2];
    // UV gradients, then independent unit +u / increasing-v directions.
    let cases = [
        ([Vec2::NEG_X, Vec2::NEG_Y], Vec2::NEG_X, Vec2::NEG_Y),
        ([Vec2::X, Vec2::Y], Vec2::X, Vec2::Y),
        ([Vec2::NEG_X, Vec2::Y], Vec2::NEG_X, Vec2::Y),
        ([Vec2::Y, Vec2::X], Vec2::Y, Vec2::X),
        (
            [Vec2::new(0.6, 0.8), Vec2::new(0.8, -0.6)],
            Vec2::new(0.6, 0.8),
            Vec2::new(0.8, -0.6),
        ),
        (
            [Vec2::X, Vec2::new(0.4, -1.)],
            Vec2::new(1., 0.4).normalize(),
            Vec2::new(0.4, -1.).normalize(),
        ),
    ];
    for mode in [0., 3.] {
        for turn in [
            Quat::IDENTITY,
            Quat::from_rotation_y(0.45) * Quat::from_rotation_z(0.3),
        ] {
            for (uv, tangent, down) in cases {
                let normal = direction(
                    diagonal,
                    turn * tangent.extend(0.),
                    turn * down.extend(0.),
                    turn * Vec3::Z,
                );
                let reference =
                    fixture.pixels(&quad(diagonal, uv, turn, Some(normal)), mode, turn)?;
                let actual = fixture.pixels(&quad(diagonal, uv, turn, None), mode, turn)?;
                matches_reference(
                    &actual,
                    &reference,
                    &format!("mode {mode}, uv {uv:?}, turn {turn:?}"),
                );
            }
        }
    }
    Ok(())
}

#[test]
fn degenerate_uvs_and_zero_strength_keep_the_geometric_normal() -> Result<()> {
    let mut fixture = Fixture::new()?;
    for mode in [0., 3.] {
        let texel = NORMALS[2];
        let reference = fixture.pixels(
            &quad(texel, [Vec2::X, Vec2::NEG_Y], Quat::IDENTITY, Some(Vec3::Z)),
            mode,
            Quat::IDENTITY,
        )?;
        for uv in [
            [Vec2::ZERO; 2],
            [Vec2::X, Vec2::X],
            [Vec2::X, Vec2::new(1., 1e-8)],
        ] {
            let actual =
                fixture.pixels(&quad(texel, uv, Quat::IDENTITY, None), mode, Quat::IDENTITY)?;
            matches_reference(
                &actual,
                &reference,
                &format!("degenerate mode {mode}, uv {uv:?}"),
            );
        }
        let mut zero = quad(texel, [Vec2::X * 2., Vec2::NEG_Y], Quat::IDENTITY, None);
        zero.materials[0].parameters.as_mut().unwrap()[0][2] = 0.;
        matches_reference(
            &fixture.pixels(&zero, mode, Quat::IDENTITY)?,
            &reference,
            "zero strength",
        );
        let mut flat = quad(texel, [Vec2::X * 2., Vec2::NEG_Y], Quat::IDENTITY, None);
        flat.images[1] = SurfaceMap::Normal.stand_in();
        matches_reference(
            &fixture.pixels(&flat, mode, Quat::IDENTITY)?,
            &reference,
            "default flat map",
        );
    }
    Ok(())
}
