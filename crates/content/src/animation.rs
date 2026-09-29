//! Native animation sampling and posed geometry, independent of source formats.
use crate::shape::{Animation, Shape};
use anyhow::{Context, Result, ensure};
use glam::{Mat4, Quat, Vec3};

pub struct Pose {
    pub nodes: Vec<Mat4>,
    pub visibility: Vec<f32>,
    pub frames: Vec<usize>,
    pub material_frames: Vec<usize>,
}
#[derive(Clone)]
pub struct PosedVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: [f32; 2],
}
pub struct PosedTriangle {
    pub object: usize,
    pub material: Option<usize>,
    pub vertices: [PosedVertex; 3],
}

fn frame_pair(animation: &Animation, time: f32) -> (usize, usize, f32) {
    if animation.frames <= 1 || animation.duration <= 0.0 {
        return (0, 0, 0.0);
    }
    let phase = if animation.looping {
        time.rem_euclid(animation.duration) / animation.duration
    } else {
        (time / animation.duration).clamp(0.0, 1.0)
    };
    let f = phase
        * if animation.looping {
            animation.frames as f32
        } else {
            (animation.frames - 1) as f32
        };
    let a = (f.floor() as usize).min(animation.frames - 1);
    let b = if animation.looping {
        (a + 1) % animation.frames
    } else {
        (a + 1).min(animation.frames - 1)
    };
    (a, b, f - f.floor())
}
fn vector(data: &[[f32; 3]], a: usize, b: usize, t: f32, default: Vec3) -> Vec3 {
    if data.is_empty() {
        default
    } else {
        Vec3::from(data[a]).lerp(Vec3::from(data[b]), t)
    }
}
fn quaternion(data: &[[f32; 4]], a: usize, b: usize, t: f32, default: Quat) -> Quat {
    if data.is_empty() {
        default
    } else {
        Quat::from_array(data[a])
            .slerp(Quat::from_array(data[b]), t)
            .normalize()
    }
}
/// Ordered pose layers: absolute channels first, then additive local transforms.
/// Absolute clips only affect authored channels; missing channels preserve the
/// preceding pose. A weight of one replaces those channels, zero has no effect.
/// Clip priority and gameplay state choose the order in the caller.
#[derive(Clone, Copy)]
pub struct Layer<'a> {
    pub animation: &'a Animation,
    pub time: f32,
    pub weight: f32,
}

pub fn sample(shape: &Shape, animation: Option<&Animation>, time: f32) -> Result<Pose> {
    ensure!(time.is_finite(), "Invalid animation time");
    let layers: Vec<_> = animation
        .into_iter()
        .map(|animation| Layer {
            animation,
            time,
            weight: 1.0,
        })
        .collect();
    sample_layers(shape, &layers)
}

pub fn sample_layers(shape: &Shape, layers: &[Layer<'_>]) -> Result<Pose> {
    sample_layers_with_transition(shape, layers, layers.len(), None).map(|(pose, _)| pose)
}

/// Absolute local node channels at one point in the layer stack. A frozen
/// copy is the source pose of a Torque `transitionToSequence`.
#[derive(Clone, Debug, PartialEq)]
pub struct Channels {
    translations: Vec<Vec3>,
    rotations: Vec<Quat>,
    scales: Vec<Vec3>,
    axes: Vec<Quat>,
}
impl Channels {
    fn blend_toward(&mut self, from: &Channels, weight: f32) -> Result<()> {
        ensure!(
            from.rotations.len() == self.rotations.len()
                && weight.is_finite()
                && (0.0..=1.0).contains(&weight),
            "Invalid animation transition"
        );
        for i in 0..self.rotations.len() {
            self.translations[i] = self.translations[i].lerp(from.translations[i], weight);
            self.rotations[i] = self.rotations[i]
                .slerp(from.rotations[i], weight)
                .normalize();
            self.scales[i] = self.scales[i].lerp(from.scales[i], weight);
            self.axes[i] = self.axes[i].slerp(from.axes[i], weight).normalize();
        }
        Ok(())
    }
}

/// `sample_layers`, blending the absolute channels reached before layer `at`
/// toward a frozen `from` pose with the given weight, as a transitioning
/// thread does. Returns the channels at `at` (after that blend) so the caller
/// can freeze them when the next transition starts.
pub fn sample_layers_with_transition(
    shape: &Shape,
    layers: &[Layer<'_>],
    at: usize,
    from: Option<(&Channels, f32)>,
) -> Result<(Pose, Channels)> {
    ensure!(layers.len() <= 32, "Too many animation layers");
    ensure!(at <= layers.len(), "Invalid animation transition layer");
    let mut translations: Vec<_> = shape
        .nodes
        .iter()
        .map(|n| Vec3::from(n.translation))
        .collect();
    let mut rotations: Vec<_> = shape
        .nodes
        .iter()
        .map(|n| Quat::from_array(n.rotation))
        .collect();
    let mut scales = vec![Vec3::ONE; shape.nodes.len()];
    let mut axes = vec![Quat::IDENTITY; shape.nodes.len()];
    let mut additive = vec![Mat4::IDENTITY; shape.nodes.len()];
    let mut pose = Pose {
        nodes: vec![],
        visibility: shape.objects.iter().map(|o| o.visibility).collect(),
        frames: shape.objects.iter().map(|o| o.frame as usize).collect(),
        material_frames: shape
            .objects
            .iter()
            .map(|o| o.material_frame as usize)
            .collect(),
    };
    let mut additive_started = false;
    let mut snapshot = None;
    for index in 0..=layers.len() {
        if index == at {
            let mut channels = Channels {
                translations,
                rotations,
                scales,
                axes,
            };
            if let Some((from, weight)) = from {
                channels.blend_toward(from, weight)?;
            }
            (translations, rotations, scales, axes) = (
                channels.translations.clone(),
                channels.rotations.clone(),
                channels.scales.clone(),
                channels.axes.clone(),
            );
            snapshot = Some(channels);
        }
        let Some(layer) = layers.get(index) else {
            break;
        };
        ensure!(
            layer.time.is_finite()
                && layer.weight.is_finite()
                && (0.0..=1.0).contains(&layer.weight),
            "Invalid animation layer time/weight"
        );
        let animation = layer.animation;
        animation.validate()?;
        if layer.weight == 0.0 {
            continue;
        }
        ensure!(
            animation.additive || !additive_started,
            "Absolute animation must precede additive layers"
        );
        additive_started |= animation.additive;
        let (a, b, t) = frame_pair(animation, layer.time);
        let w = layer.weight;
        for track in &animation.nodes {
            let Some(node) = shape
                .nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(&track.node))
            else {
                continue;
            };
            if animation.additive {
                let translation = vector(&track.translations, a, b, t, Vec3::ZERO) * w;
                let rotation = Quat::IDENTITY
                    .slerp(quaternion(&track.rotations, a, b, t, Quat::IDENTITY), w)
                    .normalize();
                let scale = Vec3::ONE.lerp(vector(&track.scales, a, b, t, Vec3::ONE), w);
                let axis = quaternion(&track.scale_rotations, a, b, t, Quat::IDENTITY);
                let scaling = Mat4::from_quat(axis)
                    * Mat4::from_scale(scale)
                    * Mat4::from_quat(axis.conjugate());
                additive[node] *= Mat4::from_rotation_translation(rotation, translation) * scaling;
            } else {
                if !track.translations.is_empty() {
                    translations[node] = translations[node]
                        .lerp(vector(&track.translations, a, b, t, Vec3::ZERO), w);
                }
                if !track.rotations.is_empty() {
                    rotations[node] = rotations[node]
                        .slerp(quaternion(&track.rotations, a, b, t, Quat::IDENTITY), w)
                        .normalize();
                }
                if !track.scales.is_empty() {
                    scales[node] = scales[node].lerp(vector(&track.scales, a, b, t, Vec3::ONE), w);
                }
                if !track.scale_rotations.is_empty() {
                    axes[node] = axes[node]
                        .slerp(
                            quaternion(&track.scale_rotations, a, b, t, Quat::IDENTITY),
                            w,
                        )
                        .normalize();
                }
            }
        }
        for track in &animation.objects {
            ensure!(
                track.object < pose.visibility.len(),
                "Animation refers to absent object"
            );
            if !track.visibility.is_empty() {
                let visibility = track.visibility[a] * (1.0 - t) + track.visibility[b] * t;
                pose.visibility[track.object] += (visibility - pose.visibility[track.object]) * w;
            }
            // Mesh/UV frame indices are discrete. Choose the incoming frame only
            // once its layer has at least half influence; never interpolate indices.
            if w >= 0.5 {
                if !track.frames.is_empty() {
                    pose.frames[track.object] = track.frames[a] as usize;
                }
                if !track.material_frames.is_empty() {
                    pose.material_frames[track.object] = track.material_frames[a] as usize;
                }
            }
        }
    }
    let local: Vec<_> = (0..shape.nodes.len())
        .map(|i| {
            Mat4::from_rotation_translation(rotations[i], translations[i])
                * Mat4::from_quat(axes[i])
                * Mat4::from_scale(scales[i])
                * Mat4::from_quat(axes[i].conjugate())
                * additive[i]
        })
        .collect();
    let mut cache = vec![None; local.len()];
    let mut visiting = vec![false; local.len()];
    fn world(
        i: usize,
        shape: &Shape,
        local: &[Mat4],
        cache: &mut [Option<Mat4>],
        visiting: &mut [bool],
        depth: usize,
    ) -> Result<Mat4> {
        ensure!(
            i < local.len() && !visiting[i] && depth <= 256,
            "Invalid/cyclic/excessively deep node hierarchy"
        );
        if let Some(m) = cache[i] {
            return Ok(m);
        }
        visiting[i] = true;
        let m = if let Some(p) = shape.nodes[i].parent {
            world(p, shape, local, cache, visiting, depth + 1)? * local[i]
        } else {
            local[i]
        };
        visiting[i] = false;
        cache[i] = Some(m);
        Ok(m)
    }
    for i in 0..local.len() {
        pose.nodes
            .push(world(i, shape, &local, &mut cache, &mut visiting, 0)?);
    }
    Ok((
        pose,
        snapshot.context("Missing animation transition snapshot")?,
    ))
}

/// `visible` selects avatar parts or other named-object customization.
pub fn triangles(
    shape: &Shape,
    pose: &Pose,
    detail: usize,
    visible: impl Fn(&str) -> bool,
) -> Result<Vec<PosedTriangle>> {
    let detail = shape.details.get(detail).context("Missing detail")?;
    let mut out = Vec::new();
    for i in detail.object_start..detail.object_start + detail.object_count {
        let object = &shape.objects[i];
        if pose.visibility[i] <= 0.0 || !visible(&object.name) {
            continue;
        }
        let Some(&mesh_index) = object.meshes.get(detail.mesh_offset) else {
            continue;
        };
        let Some(mesh) = &shape.meshes[mesh_index] else {
            continue;
        };
        let transform = object.node.map_or(Mat4::IDENTITY, |n| pose.nodes[n]);
        let normal_transform = transform.inverse().transpose();
        let offset = pose.frames[i]
            .checked_mul(mesh.frame_vertices)
            .context("Mesh frame overflow")?;
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        if let Some(skin) = &mesh.skin {
            positions.resize(mesh.frame_vertices, Vec3::ZERO);
            normals.resize(mesh.frame_vertices, Vec3::ZERO);
            let matrices: Vec<_> = skin
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| pose.nodes[*n] * Mat4::from_cols_array(&skin.inverse_bind[i]))
                .collect();
            for influence in &skin.influences {
                let m = matrices[influence.bone];
                let p = Vec3::from(mesh.positions[influence.vertex]);
                let n = Vec3::from(mesh.normals[influence.vertex]);
                positions[influence.vertex] += m.transform_point3(p) * influence.weight;
                normals[influence.vertex] +=
                    m.inverse().transpose().transform_vector3(n) * influence.weight;
            }
        } else {
            for j in 0..mesh.frame_vertices {
                positions.push(
                    transform.transform_point3(Vec3::from(
                        *mesh
                            .positions
                            .get(offset + j)
                            .context("Vertex frame out of range")?,
                    )),
                );
                normals
                    .push(normal_transform.transform_vector3(Vec3::from(mesh.normals[offset + j])));
            }
        }
        let uv_offset = pose.material_frames[i] * mesh.frame_vertices;
        for primitive in &mesh.primitives {
            for triangle in &primitive.triangles {
                let vertices = triangle.map(|v| {
                    let v = v as usize;
                    PosedVertex {
                        position: positions[v],
                        normal: normals[v].normalize_or_zero(),
                        uv: mesh.uv.get(uv_offset + v).copied().unwrap_or([0.0; 2]),
                    }
                });
                out.push(PosedTriangle {
                    object: i,
                    material: primitive.material,
                    vertices,
                });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::*;
    fn fixture() -> Shape {
        Shape {
            schema_version: 1,
            id: "test".into(),
            nodes: vec![
                Node {
                    name: "child".into(),
                    parent: Some(1),
                    translation: [1.0, 0.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                },
                Node {
                    name: "root".into(),
                    parent: None,
                    translation: [0.0, 2.0, 0.0],
                    rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array(),
                },
            ],
            objects: vec![],
            details: vec![],
            meshes: vec![],
            materials: vec![],
            animations: vec![],
        }
    }
    #[test]
    fn hierarchy_composes_parent_rotation_and_local_translation() {
        let pose = sample(&fixture(), None, 0.0).unwrap();
        assert!(
            pose.nodes[0]
                .transform_point3(Vec3::ZERO)
                .distance(Vec3::new(0.0, 3.0, 0.0))
                < 1e-5
        );
    }
    #[test]
    fn cycles_are_rejected() {
        let mut s = fixture();
        s.nodes[1].parent = Some(0);
        assert!(sample(&s, None, 0.0).is_err());
    }
    #[test]
    fn looping_interpolation_and_additive_local_motion() {
        let animation = Animation {
            name: "walk".into(),
            frames: 2,
            duration: 1.0,
            looping: true,
            additive: true,
            priority: 0,
            nodes: vec![NodeTrack {
                node: "root".into(),
                rotations: vec![],
                translations: vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
                scales: vec![],
                scale_rotations: vec![],
            }],
            objects: vec![],
            ground_translations: vec![],
            ground_rotations: vec![],
            triggers: vec![],
        };
        let a = sample(&fixture(), Some(&animation), 0.25).unwrap();
        let b = sample(&fixture(), Some(&animation), 1.25).unwrap();
        assert!(
            a.nodes[1]
                .transform_point3(Vec3::ZERO)
                .distance(Vec3::new(0.0, 3.0, 0.0))
                < 1e-5
        );
        assert!(a.nodes[0].abs_diff_eq(b.nodes[0], 1e-6));
    }

    fn clip(
        node: &str,
        translation: Option<[f32; 3]>,
        rotation: Option<Quat>,
        additive: bool,
    ) -> Animation {
        Animation {
            name: "test".into(),
            frames: 1,
            duration: 1.0,
            looping: false,
            additive,
            priority: 0,
            nodes: vec![NodeTrack {
                node: node.into(),
                translations: translation.into_iter().collect(),
                rotations: rotation.into_iter().map(|q| q.to_array()).collect(),
                scales: vec![],
                scale_rotations: vec![],
            }],
            objects: vec![],
            ground_translations: vec![],
            ground_rotations: vec![],
            triggers: vec![],
        }
    }

    #[test]
    fn partial_absolute_channels_preserve_motion_and_weighted_additive_is_local() {
        let shape = fixture();
        let movement = clip("root", Some([4.0, 2.0, 0.0]), None, false);
        let holding = clip("root", None, Some(Quat::IDENTITY), false);
        let recoil = clip("root", Some([2.0, 0.0, 0.0]), None, true);
        let layers = [
            Layer {
                animation: &movement,
                time: 0.0,
                weight: 0.5,
            },
            Layer {
                animation: &holding,
                time: 0.0,
                weight: 0.5,
            },
            Layer {
                animation: &recoil,
                time: 0.0,
                weight: 0.5,
            },
        ];
        let pose = sample_layers(&shape, &layers).unwrap();
        // Blend root translation to (2,2,0), rotate halfway from 90 to 0
        // degrees, then translate one unit along the resulting local X axis.
        let angle = std::f32::consts::FRAC_PI_4;
        let expected =
            Mat4::from_rotation_translation(Quat::from_rotation_z(angle), Vec3::new(2.0, 2.0, 0.0))
                * Mat4::from_translation(Vec3::X);
        assert!(pose.nodes[1].abs_diff_eq(expected, 1e-5));
        assert!(pose.nodes[0].abs_diff_eq(expected * Mat4::from_translation(Vec3::X), 1e-5));
        let empty = sample_layers(&shape, &[]).unwrap();
        let zero = sample_layers(
            &shape,
            &[Layer {
                weight: 0.0,
                ..layers[0]
            }],
        )
        .unwrap();
        assert!(zero.nodes[0].abs_diff_eq(empty.nodes[0], 1e-6));
    }

    #[test]
    fn object_channels_blend_visibility_but_select_discrete_frames_and_validate_order() {
        let mut shape = fixture();
        shape.objects.push(Object {
            name: "part".into(),
            node: None,
            meshes: vec![],
            visibility: 1.0,
            frame: 0,
            material_frame: 0,
        });
        let mut absolute = clip("root", None, None, false);
        absolute.objects.push(ObjectTrack {
            object: 0,
            visibility: vec![0.0],
            frames: vec![2],
            material_frames: vec![3],
        });
        let layer = Layer {
            animation: &absolute,
            time: 0.0,
            weight: 0.25,
        };
        let pose = sample_layers(&shape, &[layer]).unwrap();
        assert_eq!(pose.visibility, [0.75]);
        assert_eq!(pose.frames, [0]);
        let pose = sample_layers(
            &shape,
            &[Layer {
                weight: 0.5,
                ..layer
            }],
        )
        .unwrap();
        assert_eq!(pose.frames, [2]);
        assert_eq!(pose.material_frames, [3]);
        let additive = clip("root", None, None, true);
        assert!(
            sample_layers(
                &shape,
                &[
                    Layer {
                        animation: &additive,
                        ..layer
                    },
                    layer
                ]
            )
            .is_err()
        );
        for weight in [-0.1, 1.1, f32::NAN] {
            assert!(sample_layers(&shape, &[Layer { weight, ..layer }]).is_err());
        }
        assert!(
            sample_layers(
                &shape,
                &[Layer {
                    time: f32::INFINITY,
                    ..layer
                }]
            )
            .is_err()
        );
    }
}
