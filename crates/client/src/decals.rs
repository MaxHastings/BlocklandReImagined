//! Marks projectiles leave on bricks (`ProjectileDef::decal`): a soft disc
//! flat on the face that was hit, darkest in the middle, fading over its
//! last second. Each comes from one host cue; it goes when it fades, when
//! its brick does, or when newer marks need its place.
use anyhow::Result;
use bri_console::Clamp;
use bri_render::scene::{
    AlphaMode, GpuInstances, GpuScene, Material, MaterialKind, MeshBatch, SceneData, SceneRenderer,
    SceneTransform, SceneVertex,
};
use glam::{Mat4, Quat, Vec3};

/// Marks drawn at once; the oldest give way.
pub const MAX_LIVE: usize = 512;
/// How far off the face a mark is drawn, so it never fights the brick.
const LIFT: f32 = 0.006;
/// The last seconds over which a mark fades out.
const FADE_SECONDS: f32 = 1.0;

struct Mark {
    /// None: on the map, which stays.
    brick: Option<u64>,
    transform: Mat4,
    color: [f32; 4],
    seconds: f32,
    age: f32,
}

#[derive(Default)]
pub struct Decals {
    data: Option<SceneData>,
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    live: Vec<Mark>,
    transforms: Vec<SceneTransform>,
}

impl Decals {
    /// Mark `brick` (None: the map) at `at` on the face turned to
    /// `normal`, `size` across.
    pub fn add(
        &mut self,
        brick: Option<u64>,
        at: Vec3,
        normal: Vec3,
        decal: &bri_weapons::Decal,
        scale: f32,
    ) {
        let size = decal.size * scale;
        let usable = at.is_finite()
            && normal.is_finite()
            && normal.length_squared() >= 0.5
            && size.is_finite()
            && size > 0.0
            && decal.valid();
        if !usable {
            return;
        }
        let normal = normal.normalize();
        if self.live.len() == MAX_LIVE {
            self.live.remove(0);
        }
        self.live.push(Mark {
            brick,
            transform: Mat4::from_scale_rotation_translation(
                Vec3::splat(size),
                Quat::from_rotation_arc(Vec3::Z, normal),
                at + normal * LIFT,
            ),
            color: decal.color,
            seconds: decal.seconds,
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
    /// Age the marks, drop those whose brick is gone (`exists` says which
    /// remain), and rebuild this frame's instances.
    pub fn advance(&mut self, dt: f32, exists: impl Fn(u64) -> bool) {
        let dt = if dt.is_finite() {
            dt.clamped(0.0, 0.25)
        } else {
            0.0
        };
        self.transforms.clear();
        let transforms = &mut self.transforms;
        self.live.retain_mut(|mark| {
            mark.age += dt;
            if mark.age >= mark.seconds || mark.brick.is_some_and(|b| !exists(b)) {
                return false;
            }
            let fade = ((mark.seconds - mark.age) / FADE_SECONDS).min(1.0);
            let [r, g, b, a] = mark.color;
            transforms.push(SceneTransform {
                transform: mark.transform,
                tint: [r, g, b, a * fade],
            });
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
            let data = self.data.get_or_insert_with(unit_mark);
            self.gpu = Some(renderer.upload(device, queue, data)?);
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

/// A disc 1 across facing +Z: solid to 0.3 from the middle, fading to
/// nothing at the rim, drawn unlit and blended so the instance tint is its
/// colour.
fn unit_mark() -> SceneData {
    const SIDES: u32 = 16;
    let vertex = |x: f32, y: f32, alpha: f32| SceneVertex {
        position: [x, y, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [0.5, 0.5],
        lightmap_uv: [0.0; 2],
        color: [1.0, 1.0, 1.0, alpha],
        fx: [0.0; 4],
    };
    let mut vertices = vec![vertex(0.0, 0.0, 1.0)];
    for (radius, alpha) in [(0.3, 0.85), (0.5, 0.0)] {
        for i in 0..SIDES {
            let a = i as f32 / SIDES as f32 * std::f32::consts::TAU;
            vertices.push(vertex(a.cos() * radius, a.sin() * radius, alpha));
        }
    }
    let mut indices = Vec::new();
    for i in 0..SIDES {
        let j = (i + 1) % SIDES;
        let (inner, inner_next) = (1 + i, 1 + j);
        let (outer, outer_next) = (1 + SIDES + i, 1 + SIDES + j);
        indices.extend([0, inner, inner_next]);
        indices.extend([inner, outer, outer_next, inner, outer_next, inner_next]);
    }
    let mut material = Material::surface("decal", 0, 0);
    material.kind = MaterialKind::Unlit;
    material.alpha = AlphaMode::Blend;
    SceneData {
        id: "decal".into(),
        name: "decal".into(),
        batches: vec![MeshBatch {
            indices: 0..indices.len() as u32,
            material: 0,
            center: [0.0; 3],
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

    fn hole() -> bri_weapons::Decal {
        bri_weapons::Decal {
            size: 0.2,
            color: [0.0, 0.0, 0.0, 0.9],
            seconds: 3.0,
        }
    }

    #[test]
    fn a_mark_lies_flat_on_the_face_it_hit_at_its_size() {
        let mut decals = Decals::default();
        let at = Vec3::new(1.0, 2.0, 3.0);
        decals.add(Some(7), at, Vec3::X * 2.0, &hole(), 1.5);
        decals.advance(0.0, |_| true);
        let m = decals.transforms[0].transform;
        let centre = m.transform_point3(Vec3::ZERO);
        assert!((centre - at).dot(Vec3::X) > 0.0, "just off the face");
        assert!(centre.distance(at) < 0.01);
        assert!(
            m.transform_vector3(Vec3::Z)
                .normalize()
                .abs_diff_eq(Vec3::X, 1e-5)
        );
        assert!((m.transform_vector3(Vec3::Y).length() - 0.3).abs() < 1e-5);
        let data = unit_mark();
        for v in &data.vertices {
            assert_eq!(v.position[2], 0.0, "flat");
            assert!(Vec3::from(v.position).length() <= 0.5 + 1e-5);
        }
        assert!(
            data.indices
                .iter()
                .all(|&i| (i as usize) < data.vertices.len())
        );
    }

    #[test]
    fn marks_fade_at_the_end_and_go_with_their_brick() {
        let mut decals = Decals::default();
        decals.add(Some(1), Vec3::ZERO, Vec3::Y, &hole(), 1.0);
        decals.add(Some(2), Vec3::ZERO, Vec3::Y, &hole(), 1.0);
        decals.add(Some(3), Vec3::ZERO, Vec3::ZERO, &hole(), 1.0);
        decals.add(Some(3), Vec3::NAN, Vec3::Y, &hole(), 1.0);
        assert_eq!(decals.live_count(), 2, "broken marks are dropped");
        // Frames of at most a quarter second.
        for _ in 0..4 {
            decals.advance(0.25, |_| true);
        }
        assert_eq!(
            decals.transforms[0].tint[3], 0.9,
            "full until its last second"
        );
        decals.advance(0.25, |brick| brick != 2);
        assert_eq!(decals.live_count(), 1, "a mark goes with its brick");
        for _ in 0..5 {
            decals.advance(0.25, |_| true);
        }
        assert!(decals.transforms[0].tint[3] < 0.9, "fading");
        for _ in 0..3 {
            decals.advance(0.25, |_| true);
        }
        assert_eq!(decals.live_count(), 0);
        // A mark on the map stays whatever bricks go.
        decals.add(None, Vec3::ZERO, Vec3::Y, &hole(), 1.0);
        decals.advance(0.25, |_| false);
        assert_eq!(decals.live_count(), 1);
    }

    #[test]
    fn the_oldest_marks_give_way() {
        let mut decals = Decals::default();
        for brick in 0..MAX_LIVE as u64 + 10 {
            decals.add(Some(brick + 1), Vec3::ZERO, Vec3::Y, &hole(), 1.0);
        }
        assert_eq!(decals.live_count(), MAX_LIVE);
        assert_eq!(decals.live[0].brick, Some(11));
    }
}
