//! Camera-relative sky geometry. Orientation follows the classic sky face order;
//! authored Sky object translation/rotation does not rotate the backdrop.
use crate::scene::*;
use anyhow::{Result, ensure};
use bri_content::environment::Environment;
use glam::Vec3;

fn material(out: &mut SceneData, name: String, image: usize, cloud: bool, blend: bool) -> usize {
    let mut material = Material::surface(name, image, 0);
    material.kind = if cloud {
        MaterialKind::Cloud
    } else {
        MaterialKind::Sky
    };
    material.double_sided = true;
    material.alpha = if blend {
        AlphaMode::Blend
    } else {
        AlphaMode::Opaque
    };
    let id = out.materials.len();
    out.materials.push(material);
    id
}
fn quad(
    out: &mut SceneData,
    material: usize,
    points: [[f32; 3]; 4],
    uv: [[f32; 2]; 4],
    colors: [[f32; 4]; 4],
    velocity: [f32; 2],
) {
    let base = out.vertices.len() as u32;
    let start = out.indices.len() as u32;
    for i in 0..4 {
        out.vertices.push(SceneVertex {
            position: points[i],
            normal: [velocity[0], velocity[1], 0.0],
            uv: uv[i],
            lightmap_uv: [0.0; 2],
            color: colors[i],
            fx: [0.; 4],
        });
    }
    out.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    out.batches.push(MeshBatch {
        indices: start..start + 6,
        material,
        center: [0.0; 3],
    });
}
pub fn append(
    out: &mut SceneData,
    env: &Environment,
    faces: &[usize],
    clouds: &[usize],
) -> Result<()> {
    env.validate()?;
    ensure!(
        faces.len() == env.faces.len()
            && clouds.len() == env.clouds.len()
            && faces.iter().chain(clouds).all(|i| *i < out.images.len()),
        "Missing sky image binding"
    );
    out.fog = env.fog;
    out.clear_color = [env.fog.color[0], env.fog.color[1], env.fog.color[2], 1.0];
    out.omissions.extend(env.warnings.clone());
    let radius = env.fog.end * 0.95;
    let half = radius / 3.0_f32.sqrt();
    let top = [
        [-half, half, half],
        [half, half, half],
        [half, half, -half],
        [-half, half, -half],
    ];
    // Draw the fog backdrop through the shader's output transfer as well. This
    // covers below-horizon rays identically on sRGB and UNORM attachments.
    let fog_material = material(out, "sky/fog-backdrop".into(), 0, false, false);
    out.materials[fog_material].parameters = Some(FOG_BACKDROP);
    let c = env.fog.color;
    for side in 0..6 {
        let points = if side < 4 {
            let a = top[side];
            let b = top[(side + 1) % 4];
            [a, b, [b[0], -half, b[2]], [a[0], -half, a[2]]]
        } else if side == 4 {
            top
        } else {
            top.map(|p| [p[0], -half, p[2]])
        };
        quad(
            out,
            fog_material,
            points,
            [[0.0; 2]; 4],
            [[c[0], c[1], c[2], 1.0]; 4],
            [0.0; 2],
        );
    }
    for (side, &image) in faces
        .iter()
        .enumerate()
        .take(if env.bottom { 6 } else { 5 })
    {
        let color = if env.textures {
            [1.0; 4]
        } else {
            [
                env.solid_color[0],
                env.solid_color[1],
                env.solid_color[2],
                1.0,
            ]
        };
        let index = material(
            out,
            format!("sky/face-{side}"),
            if env.textures { image } else { 0 },
            false,
            false,
        );
        let mut points = if side < 4 {
            let a = top[side];
            let b = top[(side + 1) % 4];
            [a, b, [b[0], -half, b[2]], [a[0], -half, a[2]]]
        } else if side == 4 {
            [top[3], top[2], top[1], top[0]]
        } else {
            [
                [top[0][0], -half, top[0][2]],
                [top[1][0], -half, top[1][2]],
                [top[2][0], -half, top[2][2]],
                [top[3][0], -half, top[3][2]],
            ]
        };
        let mut uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        if side < 4 && env.horizon_band {
            points[2][1] = 0.0;
            points[3][1] = 0.0;
            uv[2][1] = 0.5;
            uv[3][1] = 0.5;
        }
        let dimensions = [
            out.images[image].width as f32,
            out.images[image].height as f32,
        ];
        for coord in &mut uv {
            for axis in 0..2 {
                coord[axis] = (coord[axis] * (dimensions[axis] - 1.0) + 0.5) / dimensions[axis];
            }
        }
        quad(out, index, points, uv, [color; 4], [0.0; 2]);
    }
    for (layer, &image) in env.clouds.iter().zip(clouds) {
        let index = material(
            out,
            format!("sky/cloud/{}", layer.image.source),
            image,
            true,
            true,
        );
        let mut points = [Vec3::ZERO; 25];
        for y in 0..5 {
            for x in 0..5 {
                let h = if x == 0 || x == 4 || y == 0 || y == 4 {
                    0.05
                } else if x == 2 && y == 2 {
                    layer.center_height
                } else {
                    layer.center_height - 0.05
                };
                points[y * 5 + x] = Vec3::new(
                    -radius + x as f32 * radius * 0.5,
                    h * radius,
                    -radius + y as f32 * radius * 0.5,
                );
            }
        }
        for (corner, a, b, c) in [
            (0, 5, 1, 6),
            (4, 9, 3, 8),
            (20, 21, 15, 16),
            (24, 23, 19, 18),
        ] {
            points[corner] = (points[a] + points[b]) - points[c];
        }
        let mut colors = [[1.0; 4]; 25];
        for (i, p) in points.iter().enumerate() {
            let a = 1.3 - glam::Vec2::new(p.x, p.z).length() / radius;
            colors[i][3] = if a < 0.4 {
                0.0
            } else if a > 0.8 {
                1.0
            } else {
                a
            };
        }
        for y in 0..4 {
            for x in 0..4 {
                let ids = [
                    y * 5 + x,
                    (y + 1) * 5 + x,
                    (y + 1) * 5 + x + 1,
                    y * 5 + x + 1,
                ];
                quad(
                    out,
                    index,
                    ids.map(|i| points[i].to_array()),
                    ids.map(|i| [(i % 5) as f32, (i / 5) as f32]),
                    ids.map(|i| colors[i]),
                    layer.velocity,
                );
            }
        }
    }
    Ok(())
}
