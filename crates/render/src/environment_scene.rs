//! Camera-relative sky geometry. Orientation follows the classic sky face order;
//! authored Sky object translation/rotation does not rotate the backdrop.
use crate::scene::*;
use anyhow::{Result, ensure};
use bri_console::Clamp;
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
    out.sky_below = env.bottom;
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
    out.sky_bands = sky_bands(out, env, faces);
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

/// The sky faces `append` draws, averaged over each direction band
/// ([`SkyBands`]): far geometry fogs toward the sky behind it rather than
/// a flat fog colour, so a mountain at the edge of the fog meets the sky
/// it fades into. Clouds move and are left out.
fn sky_bands(out: &SceneData, env: &Environment, faces: &[usize]) -> SkyBands {
    const STEPS: usize = 16;
    let mut bands = SkyBands::default();
    for (row, elevations) in bands.iter_mut().enumerate() {
        for (column, band) in elevations.iter_mut().enumerate() {
            let (mut sum, mut covered) = (Vec3::ZERO, 0_usize);
            for a in 0..STEPS {
                for e in 0..STEPS {
                    let around = ((column as f32 + (a as f32 + 0.5) / STEPS as f32)
                        / SKY_AZIMUTHS as f32
                        - 0.5)
                        * std::f32::consts::TAU;
                    let rise = ((row as f32 + (e as f32 + 0.5) / STEPS as f32)
                        / SKY_ELEVATIONS as f32)
                        * 2.0
                        - 1.0;
                    let up = rise.signum() * rise * rise;
                    let flat = (1.0 - up * up).max(0.0).sqrt();
                    let along = Vec3::new(around.sin() * flat, up, around.cos() * flat);
                    if let Some(linear) = sky_face_color(out, env, faces, along) {
                        sum += linear;
                        covered += 1;
                    }
                }
            }
            if covered > 0 {
                let mean = sum / covered as f32;
                let display = mean.to_array().map(display_encode);
                *band = [
                    display[0],
                    display[1],
                    display[2],
                    covered as f32 / (STEPS * STEPS) as f32,
                ];
            }
        }
    }
    bands
}

/// The linear colour of the sky face `append` draws along `along`, or None
/// where only the fog backdrop shows (no face, under a horizon band).
fn sky_face_color(
    out: &SceneData,
    env: &Environment,
    faces: &[usize],
    along: Vec3,
) -> Option<Vec3> {
    let size = along.abs().max_element();
    if size <= 0.0 {
        return None;
    }
    let p = along / size;
    let (side, u, v) = if along.y.abs() >= along.x.abs().max(along.z.abs()) {
        if p.y > 0.0 {
            (4, (p.x + 1.0) * 0.5, (p.z + 1.0) * 0.5)
        } else {
            (5, (p.x + 1.0) * 0.5, (1.0 - p.z) * 0.5)
        }
    } else {
        let v = (1.0 - p.y) * 0.5;
        if along.z.abs() >= along.x.abs() {
            if p.z > 0.0 {
                (0, (p.x + 1.0) * 0.5, v)
            } else {
                (2, (1.0 - p.x) * 0.5, v)
            }
        } else if p.x > 0.0 {
            (1, (1.0 - p.z) * 0.5, v)
        } else {
            (3, (p.z + 1.0) * 0.5, v)
        }
    };
    if (side == 5 && !env.bottom) || (side < 4 && env.horizon_band && p.y < 0.0) {
        return None;
    }
    let &image = faces.get(side)?;
    if !env.textures {
        return Some(Vec3::from(env.solid_color.map(display_decode)));
    }
    let image = &out.images[image];
    let x = (u.clamped(0.0, 1.0) * (image.width - 1) as f32).round() as usize;
    let y = (v.clamped(0.0, 1.0) * (image.height - 1) as f32).round() as usize;
    let at = (y * image.width as usize + x) * 4;
    let texel = image.rgba.get(at..at + 3)?;
    Some(Vec3::new(
        display_decode(f32::from(texel[0]) / 255.0),
        display_decode(f32::from(texel[1]) / 255.0),
        display_decode(f32::from(texel[2]) / 255.0),
    ))
}

/// sRGB decoding and encoding, as the shaders' `linear_color` and
/// `display_color` do.
fn display_decode(display: f32) -> f32 {
    if display <= 0.04045 {
        display / 12.92
    } else {
        ((display + 0.055) / 1.055).powf(2.4)
    }
}
fn display_encode(linear: f32) -> f32 {
    if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.max(0.0).powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::environment::{Fog, Image};

    /// One solid colour a face: +Z, +X, -Z, -X, +Y, -Y.
    fn sky(horizon_band: bool, bottom: bool) -> SceneData {
        let colors = [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 0, 255],
            [255, 0, 255, 255],
            [0, 255, 255, 255],
        ];
        let mut out = SceneData::default();
        let mut faces = Vec::new();
        for (i, color) in colors.iter().enumerate() {
            out.images.push(SceneImage {
                label: format!("face-{i}"),
                width: 1,
                height: 1,
                rgba: color.to_vec(),
                srgb: true,
            });
            faces.push(Image {
                file: format!("face-{i}.png"),
                source: format!("face-{i}"),
                sha256: "0".repeat(64),
                width: 1,
                height: 1,
            });
        }
        let env = Environment {
            schema_version: 2,
            source_materials: "fixture".into(),
            source_sha256: "0".repeat(64),
            faces,
            reflection: None,
            clouds: vec![],
            textures: true,
            bottom,
            horizon_band,
            solid_color: [0.3; 3],
            fog: Fog {
                start: 10.0,
                end: 100.0,
                color: [1.0; 3],
            },
            warnings: vec![],
        };
        append(&mut out, &env, &[1, 2, 3, 4, 5, 6], &[]).unwrap();
        out
    }

    #[test]
    fn sky_bands_hold_the_face_each_way_and_leave_the_backdrop_out() {
        let bands = sky(false, true).sky_bands;
        let near = |band: [f32; 4], color: [f32; 4]| {
            band.iter().zip(color).all(|(a, b)| (a - b).abs() < 1e-3)
        };
        // Just above the horizon, around from -Z (atan2(x, z) = -180°) in
        // 45° columns: -Z, -X, +Z and +X each fill two.
        let row = SKY_ELEVATIONS / 2;
        for (columns, color) in [
            ([7, 0], [0.0, 0.0, 1.0, 1.0]),
            ([1, 2], [1.0, 1.0, 0.0, 1.0]),
            ([3, 4], [1.0, 0.0, 0.0, 1.0]),
            ([5, 6], [0.0, 1.0, 0.0, 1.0]),
        ] {
            for column in columns {
                let band = bands[row][column];
                assert!(near(band, color), "{column}: {band:?}");
            }
        }
        assert!(near(bands[SKY_ELEVATIONS - 1][3], [1.0, 0.0, 1.0, 1.0]));
        assert!(near(bands[0][3], [0.0, 1.0, 1.0, 1.0]));
        // No bottom face: straight down is the fog backdrop; with a horizon
        // band, all below the horizon is.
        let open = sky(false, false).sky_bands;
        assert_eq!(open[0][3][3], 0.0);
        assert!(near(open[row - 1][4], [1.0, 0.0, 0.0, 1.0]));
        let band = sky(true, false).sky_bands;
        assert_eq!(band[row - 1][4][3], 0.0);
        assert!(near(band[row][4], [1.0, 0.0, 0.0, 1.0]));
    }
}
