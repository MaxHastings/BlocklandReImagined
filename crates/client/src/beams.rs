//! Package beams (`beam` in rules: tracers, lasers, bolts): a thin
//! unlit box stretched between two points, in the beam's colour, fading out
//! over its life. One cue per beam; nothing is sent while it shows.
use anyhow::Result;
use bri_render::scene::{
    AlphaMode, GpuInstances, GpuScene, Material, MaterialKind, MeshBatch, SceneData, SceneRenderer,
    SceneTransform, SceneVertex,
};
use glam::{Mat4, Quat, Vec3};

/// Beams drawn at once; the oldest give way.
const MAX_LIVE: usize = 256;

struct Beam {
    from: Vec3,
    to: Vec3,
    color: [f32; 4],
    width: f32,
    seconds: f32,
    age: f32,
}

pub struct Beams {
    data: SceneData,
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    live: Vec<Beam>,
    transforms: Vec<SceneTransform>,
}

impl Default for Beams {
    fn default() -> Self {
        Self {
            data: unit_beam(),
            gpu: None,
            instances: None,
            live: Vec::new(),
            transforms: Vec::new(),
        }
    }
}

impl Beams {
    /// Show a beam; `from` is where it starts as this client draws it.
    pub fn add(&mut self, from: Vec3, to: Vec3, color: [f32; 4], width: f32, seconds: f32) {
        if !(from.is_finite() && to.is_finite() && width.is_finite() && seconds.is_finite())
            || seconds <= 0.0
            || from.distance_squared(to) < 1e-8
        {
            return;
        }
        if self.live.len() == MAX_LIVE {
            self.live.remove(0);
        }
        self.live.push(Beam {
            from,
            to,
            color: color.map(|c| {
                if c.is_finite() {
                    c.clamp(0.0, 1.0)
                } else {
                    1.0
                }
            }),
            width,
            seconds,
            age: 0.0,
        });
    }
    pub fn live_count(&self) -> usize {
        self.live.len()
    }
    pub fn clear(&mut self) {
        self.live.clear();
        self.transforms.clear();
    }
    /// Age the beams and rebuild this frame's instances. A beam thins and
    /// fades as it ages.
    pub fn advance(&mut self, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        self.transforms.clear();
        let transforms = &mut self.transforms;
        self.live.retain_mut(|beam| {
            beam.age += dt;
            if beam.age >= beam.seconds {
                return false;
            }
            let left = 1.0 - beam.age / beam.seconds;
            if let Some(transform) = beam_transform(beam.from, beam.to, beam.width * left.sqrt()) {
                let [r, g, b, a] = beam.color;
                transforms.push(SceneTransform {
                    transform,
                    tint: [r, g, b, a * left],
                });
            }
            true
        });
    }
    pub fn upload(
        &mut self,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<()> {
        if self.transforms.is_empty() {
            if let Some(instances) = &mut self.instances {
                instances.update(queue, &[])?;
            }
            return Ok(());
        }
        if self.gpu.is_none() {
            self.gpu = Some(renderer.upload(device, queue, &self.data)?);
        }
        if self
            .instances
            .as_ref()
            .is_none_or(|i| i.capacity() < self.transforms.len())
        {
            self.instances = Some(GpuInstances::new(
                device,
                self.transforms.len().next_power_of_two().max(16),
            )?);
        }
        self.instances
            .as_mut()
            .expect("created above")
            .update(queue, &self.transforms)?;
        Ok(())
    }
    pub fn draws(&self) -> Option<(&GpuScene, &GpuInstances)> {
        if self.transforms.is_empty() {
            return None;
        }
        Some((self.gpu.as_ref()?, self.instances.as_ref()?))
    }
    pub fn gpu_stopped(&mut self) {
        self.gpu = None;
        self.instances = None;
    }
}

/// The unit beam (1 wide, 1 long along -Z from the origin) stretched and
/// turned to run from `from` to `to`, `width` wide.
fn beam_transform(from: Vec3, to: Vec3, width: f32) -> Option<Mat4> {
    let span = to - from;
    let length = span.length();
    if !length.is_finite() || length <= 1e-4 || width <= 0.0 || !width.is_finite() {
        return None;
    }
    let turn = Quat::from_rotation_arc(Vec3::NEG_Z, span / length);
    Some(Mat4::from_scale_rotation_translation(
        Vec3::new(width, width, length),
        turn,
        from,
    ))
}

/// A white box from z 0 to -1, 1 wide, drawn unlit and blended so the
/// instance tint is its colour.
fn unit_beam() -> SceneData {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    // Four long sides and two ends.
    let faces: [(Vec3, Vec3, Vec3); 6] = [
        (Vec3::X, Vec3::Y, Vec3::NEG_Z),
        (Vec3::NEG_X, Vec3::Y, Vec3::Z),
        (Vec3::Y, Vec3::NEG_Z, Vec3::X),
        (Vec3::NEG_Y, Vec3::Z, Vec3::X),
        (Vec3::Z, Vec3::Y, Vec3::X),
        (Vec3::NEG_Z, Vec3::Y, Vec3::NEG_X),
    ];
    let centre = Vec3::new(0.0, 0.0, -0.5);
    let half = Vec3::new(0.5, 0.5, 0.5);
    for (normal, up, right) in faces {
        let base = vertices.len() as u32;
        for (u, v) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = centre + (normal + right * u + up * v) * half;
            vertices.push(SceneVertex {
                position: p.to_array(),
                normal: normal.to_array(),
                uv: [0.5, 0.5],
                lightmap_uv: [0.0; 2],
                color: [1.0; 4],
                fx: [0.0; 4],
            });
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let mut material = Material::surface("beam", 0, 0);
    material.kind = MaterialKind::Unlit;
    material.alpha = AlphaMode::Blend;
    material.double_sided = true;
    SceneData {
        id: "beam".into(),
        name: "beam".into(),
        batches: vec![MeshBatch {
            indices: 0..indices.len() as u32,
            material: 0,
            center: centre.to_array(),
        }],
        vertices,
        indices,
        materials: vec![material],
        ..SceneData::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_beam_spans_its_two_points() {
        let from = Vec3::new(1.0, 2.0, 3.0);
        let to = Vec3::new(1.0, 2.0, 13.0);
        let m = beam_transform(from, to, 0.5).unwrap();
        assert!(m.transform_point3(Vec3::ZERO).distance(from) < 1e-4);
        assert!(m.transform_point3(Vec3::NEG_Z).distance(to) < 1e-4);
        assert!((m.transform_vector3(Vec3::X).length() - 0.5).abs() < 1e-4);
        assert!(beam_transform(from, from, 0.5).is_none());
    }

    #[test]
    fn beams_thin_fade_and_expire() {
        let mut beams = Beams::default();
        beams.add(Vec3::ZERO, Vec3::X * 10.0, [1.0, 0.5, 0.0, 1.0], 0.2, 0.5);
        beams.add(Vec3::ZERO, Vec3::ZERO, [1.0; 4], 0.2, 0.5);
        beams.add(Vec3::ZERO, Vec3::X, [1.0; 4], 0.2, f32::NAN);
        assert_eq!(beams.live_count(), 1, "empty and broken beams are dropped");
        beams.advance(0.1);
        let early = beams.transforms[0];
        beams.advance(0.2);
        let late = beams.transforms[0];
        assert!(late.tint[3] < early.tint[3], "fades");
        assert!(
            late.transform.x_axis.length() < early.transform.x_axis.length(),
            "thins"
        );
        assert_eq!(early.tint[..3], [1.0, 0.5, 0.0]);
        beams.advance(0.25);
        assert_eq!(beams.live_count(), 0);
    }

    #[test]
    fn the_unit_beam_is_a_closed_box_along_negative_z() {
        let data = unit_beam();
        assert_eq!(data.vertices.len(), 24);
        assert_eq!(data.indices.len(), 36);
        for v in &data.vertices {
            assert!(v.position[0].abs() <= 0.5 && v.position[1].abs() <= 0.5);
            assert!((-1.0..=0.0).contains(&v.position[2]));
        }
    }
}
