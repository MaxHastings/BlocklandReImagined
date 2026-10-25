//! What the sun's lens flare needs from the client each frame: whether the sun
//! is in view and whether the map's drawn geometry stands in front of it. The
//! map's own collision includes walls nobody sees (a level's invisible
//! ceiling), so the line to the sun is cast against what is drawn instead:
//! the scene's opaque triangles, built once per map.
use bri_console::Clamp;
use bri_render::scene::{AlphaMode, MaterialKind, SceneData};
use glam::{Vec2, Vec3};
use rapier3d::parry::{
    query::{Ray, RayCast},
    shape::TriMesh,
};
use std::sync::Arc;

/// Longest line to the sun a frame checks, units.
const REACH: f32 = 4000.0;
/// Seconds the flare takes to fade in or out as the sun is found or lost.
const FADE: f32 = 0.2;

#[derive(Default)]
pub struct SunFlare {
    /// The drawn geometry of the scene whose flare this is, with that
    /// scene's flare to tell when the map changed.
    occluders: Option<(Arc<bri_render::scene::LensFlareScene>, Option<TriMesh>)>,
    level: f32,
    clock: Option<std::time::Instant>,
}

/// The triangles that can hide the sun: every opaque or cut-out surface the
/// scene draws, not its sky, water, glass or glow.
fn occluder(scene: &SceneData) -> Option<TriMesh> {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for batch in &scene.batches {
        let material = &scene.materials[batch.material];
        let hides = !matches!(
            material.kind,
            MaterialKind::Sky
                | MaterialKind::Cloud
                | MaterialKind::Water
                | MaterialKind::Unlit
                | MaterialKind::UnlitOverlay
        ) && matches!(material.alpha, AlphaMode::Opaque | AlphaMode::Mask(_));
        if !hides {
            continue;
        }
        let base = vertices.len() as u32;
        let mut used = std::collections::BTreeMap::new();
        for triangle in
            scene.indices[batch.indices.start as usize..batch.indices.end as usize].chunks_exact(3)
        {
            let mut tri = [0u32; 3];
            for (slot, &index) in triangle.iter().enumerate() {
                tri[slot] = *used.entry(index).or_insert_with(|| {
                    vertices.push(Vec3::from(scene.vertices[index as usize].position));
                    base + used_len(&vertices, base)
                });
            }
            indices.push(tri);
        }
    }
    TriMesh::new(vertices, indices).ok()
}

/// How many of the batch's vertices are in `vertices` so far, less one.
fn used_len<T>(vertices: &[T], base: u32) -> u32 {
    vertices.len() as u32 - base - 1
}

impl SunFlare {
    /// How much of `scene`'s flare shows this frame, 0 to 1: nothing without
    /// one, while the sun is behind the view or past the flare's cut-off
    /// angle, or while the drawn map stands in front of it; fading in toward
    /// the view's centre and over a moment as the sun comes out or goes in.
    /// The sun's place on the screen (clip space) comes with it.
    pub fn update(
        &mut self,
        scene: &SceneData,
        eye: Vec3,
        forward: Vec3,
        toward_sun: Vec3,
        view_projection: &glam::Mat4,
        blocked_by_bricks: bool,
    ) -> (Vec2, f32) {
        let now = std::time::Instant::now();
        let dt = self
            .clock
            .replace(now)
            .map_or(0., |before| (now - before).as_secs_f32().min(0.25));
        self.update_after(
            dt,
            scene,
            eye,
            forward,
            toward_sun,
            view_projection,
            blocked_by_bricks,
        )
    }

    /// [`update`](Self::update) with the seconds since the last frame given.
    #[allow(clippy::too_many_arguments)]
    fn update_after(
        &mut self,
        dt: f32,
        scene: &SceneData,
        eye: Vec3,
        forward: Vec3,
        toward_sun: Vec3,
        view_projection: &glam::Mat4,
        blocked_by_bricks: bool,
    ) -> (Vec2, f32) {
        let Some(flare) = &scene.lens_flare else {
            self.occluders = None;
            return (Vec2::ZERO, 0.0);
        };
        if self
            .occluders
            .as_ref()
            .is_none_or(|(known, _)| !Arc::ptr_eq(known, flare))
        {
            self.occluders = Some((flare.clone(), occluder(scene)));
            self.level = 0.0;
        }
        let clip = *view_projection * toward_sun.extend(0.0);
        let cut = flare.cutoff_angle.to_radians().cos();
        let angle = ((toward_sun.dot(forward) - cut) / (1.0 - cut)).clamped(0.0, 1.0);
        let mut target = 0.0;
        if clip.w > 1e-4 && angle > 0.0 && !blocked_by_bricks {
            let ray = Ray::new(eye, toward_sun);
            let blocked = self
                .occluders
                .as_ref()
                .and_then(|(_, mesh)| mesh.as_ref())
                .is_some_and(|mesh| mesh.cast_local_ray(&ray, REACH, false).is_some());
            if !blocked {
                target = angle;
            }
        }
        let step = (dt / FADE).min(1.0);
        self.level += (target - self.level) * step;
        (Vec2::new(clip.x, clip.y) / clip.w.max(1e-4), self.level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_render::scene::{LensFlareScene, Material, MeshBatch, SceneVertex};

    fn scene(wall: bool) -> SceneData {
        let mut data = SceneData {
            lens_flare: Some(Arc::new(LensFlareScene {
                cutoff_angle: 20.0,
                elements: Vec::new(),
            })),
            ..Default::default()
        };
        if wall {
            // A wall of triangles across +Z, 10 units ahead.
            let vertex = |x: f32, y: f32| SceneVertex {
                position: [x, y, 10.0],
                normal: [0.0, 0.0, -1.0],
                uv: [0.0; 2],
                lightmap_uv: [0.0; 2],
                color: [1.0; 4],
                fx: [0.0; 4],
            };
            data.vertices = vec![vertex(-50., -50.), vertex(50., -50.), vertex(0., 50.)];
            data.indices = vec![0, 1, 2];
            data.materials = vec![Material::vertex_lit("wall", 0)];
            data.batches = vec![MeshBatch {
                zone: None,
                indices: 0..3,
                material: 0,
                center: [0.0; 3],
            }];
        }
        data
    }

    fn look(flare: &mut SceneFlareProbe, wall: bool, sun_degrees: f32, bricks: bool) -> f32 {
        let toward = Vec3::new(
            0.0,
            sun_degrees.to_radians().sin(),
            sun_degrees.to_radians().cos(),
        );
        let view = bri_render::scene::perspective(1.2, 1.7, 0.1, 100.0)
            * glam::camera::rh::view::look_at_mat4(Vec3::ZERO, Vec3::Z, Vec3::Y);
        let data = flare.scene(wall);
        let mut level = 0.0;
        for _ in 0..8 {
            level = flare
                .state
                .update_after(0.1, &data, Vec3::ZERO, Vec3::Z, toward, &view, bricks)
                .1;
        }
        level
    }

    /// The same scene across frames, as the client keeps it.
    struct SceneFlareProbe {
        state: SunFlare,
        scenes: [Option<SceneData>; 2],
    }
    impl SceneFlareProbe {
        fn new() -> Self {
            Self {
                state: SunFlare::default(),
                scenes: [Some(scene(false)), Some(scene(true))],
            }
        }
        fn scene(&mut self, wall: bool) -> SceneData {
            self.scenes[usize::from(wall)].clone().unwrap()
        }
    }

    #[test]
    fn the_flare_shows_with_the_sun_in_view_and_fades_with_the_angle() {
        let mut probe = SceneFlareProbe::new();
        let centre = look(&mut probe, false, 0.0, false);
        assert!(centre > 0.99, "{centre}");
        let mut probe = SceneFlareProbe::new();
        let off = look(&mut probe, false, 10.0, false);
        assert!(off > 0.3 && off < 0.9, "{off}");
        let mut probe = SceneFlareProbe::new();
        assert_eq!(look(&mut probe, false, 25.0, false), 0.0);
    }

    #[test]
    fn the_drawn_map_or_bricks_in_front_of_the_sun_hide_it() {
        let mut probe = SceneFlareProbe::new();
        assert!(look(&mut probe, true, 0.0, false) < 0.01);
        let mut probe = SceneFlareProbe::new();
        assert!(look(&mut probe, false, 0.0, true) < 0.01);
    }

    #[test]
    fn a_map_without_a_flare_shows_nothing() {
        let mut state = SunFlare::default();
        let mut data = scene(false);
        data.lens_flare = None;
        let view = glam::Mat4::IDENTITY;
        assert_eq!(
            state
                .update_after(1.0, &data, Vec3::ZERO, Vec3::Z, Vec3::Z, &view, false)
                .1,
            0.0
        );
    }
}
