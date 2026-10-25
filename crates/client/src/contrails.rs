//! Projectile contrails (`bri_weapons::ProjectileDef::contrail`): a ribbon of
//! textured, camera-facing quads through the points a projectile has passed,
//! thinning and fading by each point's age, the texture running along it.
//! Presentation only; each client keeps its own points from the projectiles
//! it is shown. One dynamic mesh per ribbon, its vertices rewritten each frame.
use anyhow::Result;
use bri_render::scene::{
    AlphaMode, GpuScene, Material, MaterialKind, MeshBatch, SceneData, SceneImage, SceneRenderer,
    SceneVertex,
};
use bri_weapons::{Contrail, Projectile};
use glam::Vec3;
use std::collections::{BTreeMap, VecDeque};

/// Most ribbons drawn at once, and most quads one has.
const MAX_TRACKS: usize = 64;
const MAX_QUADS: usize = 512;

struct Track {
    definition: String,
    points: VecDeque<(Vec3, f32)>,
    /// Seconds of the next point's interval already passed.
    carry: f32,
    /// Where the projectile is now; None once it is gone.
    head: Option<Vec3>,
    /// The ribbon's quads this frame (4 vertices each), padded with
    /// collapsed quads to `capacity`.
    vertices: Vec<SceneVertex>,
    quads: usize,
    capacity: usize,
    gpu: Option<GpuScene>,
}

#[derive(Default)]
pub struct Contrails {
    /// Each projectile definition's contrail and its texture.
    definitions: BTreeMap<String, (Contrail, SceneImage)>,
    tracks: BTreeMap<u64, Track>,
}

impl Contrails {
    /// The contrails the pack's projectiles have, with the textures
    /// `texture` finds (a contrail whose texture is missing is left out).
    pub fn new<'t>(
        projectiles: &BTreeMap<String, bri_weapons::ProjectileDef>,
        texture: impl Fn(&str) -> Option<&'t SceneImage>,
    ) -> Self {
        let mut me = Self::default();
        for (id, def) in projectiles {
            let Some(contrail) = &def.contrail else {
                continue;
            };
            let Some(image) = texture(&contrail.texture) else {
                continue;
            };
            me.definitions
                .insert(id.clone(), (contrail.clone(), image.clone()));
        }
        me
    }
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
    pub fn live_tracks(&self) -> usize {
        self.tracks.len()
    }
    pub fn clear(&mut self) {
        self.tracks.clear();
    }
    /// Note where the projectiles in view are; a projectile no longer there
    /// stops adding points and its ribbon runs out.
    pub fn observe(&mut self, projectiles: &[Projectile]) {
        for track in self.tracks.values_mut() {
            track.head = None;
        }
        if self.definitions.is_empty() {
            return;
        }
        for p in projectiles {
            let Some((contrail, _)) = self.definitions.get(&p.definition) else {
                continue;
            };
            if !p.position.is_finite() {
                continue;
            }
            if !self.tracks.contains_key(&p.id) && self.tracks.len() >= MAX_TRACKS {
                continue;
            }
            let last = contrail.keys.last().map_or(0.0, |k| k.age);
            let capacity = ((last * contrail.rate).ceil() as usize + 2).min(MAX_QUADS);
            let track = self.tracks.entry(p.id).or_insert_with(|| Track {
                definition: p.definition.clone(),
                points: VecDeque::new(),
                carry: 0.0,
                head: None,
                vertices: Vec::new(),
                quads: 0,
                capacity,
                gpu: None,
            });
            track.head = Some(p.position);
            if track.points.is_empty() {
                track.points.push_back((p.position, 0.0));
            }
        }
    }
    /// Age the points, add this interval's and rebuild the ribbons, turned
    /// to face `eye`.
    pub fn advance(&mut self, dt: f32, eye: Vec3) {
        let dt = if dt.is_finite() {
            bri_console::Clamp::clamped(dt, 0.0, 0.25)
        } else {
            0.0
        };
        let definitions = &self.definitions;
        self.tracks.retain(|_, track| {
            let Some((contrail, _)) = definitions.get(&track.definition) else {
                return false;
            };
            let last = contrail.keys.last().map_or(0.0, |k| k.age);
            for (_, age) in &mut track.points {
                *age += dt;
            }
            while track.points.front().is_some_and(|(_, age)| *age >= last) {
                track.points.pop_front();
            }
            if let Some(head) = track.head {
                track.carry += dt;
                let every = 1.0 / contrail.rate;
                while track.carry >= every {
                    track.carry -= every;
                    if track.points.len() == MAX_QUADS {
                        track.points.pop_front();
                    }
                    // Newest at the back, aged by what is left of its interval.
                    track.points.push_back((head, track.carry));
                }
            }
            if track.points.is_empty() && track.head.is_none() {
                return false;
            }
            let mut points: Vec<(Vec3, f32)> = track.points.iter().copied().collect();
            if let Some(head) = track.head {
                points.push((head, 0.0));
            }
            build(track, contrail, &points, eye);
            true
        });
    }
    /// Make this frame's ribbons known to the GPU: a new one uploads its
    /// mesh, a live one rewrites its vertices.
    pub fn upload(
        &mut self,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<()> {
        for track in self.tracks.values_mut() {
            if track.vertices.is_empty() {
                continue;
            }
            let Some((contrail, image)) = self.definitions.get(&track.definition) else {
                continue;
            };
            match &mut track.gpu {
                Some(gpu) => gpu.update_vertices(queue, &track.vertices, &[[0.0; 3]])?,
                None => {
                    let data = mesh(image, contrail.additive, track.vertices.clone());
                    track.gpu = Some(renderer.upload(device, queue, &data)?);
                }
            }
        }
        Ok(())
    }
    pub fn scenes(&self) -> impl Iterator<Item = &GpuScene> {
        self.tracks
            .values()
            .filter(|t| t.quads > 0)
            .filter_map(|t| t.gpu.as_ref())
    }
    pub fn gpu_stopped(&mut self) {
        for track in self.tracks.values_mut() {
            track.gpu = None;
        }
    }
    #[cfg(test)]
    fn quad_count(&self) -> usize {
        self.tracks.values().map(|t| t.quads).sum()
    }
}

/// The ribbon through `points` (oldest first), `capacity` quads, the rest
/// collapsed to nothing.
fn build(track: &mut Track, contrail: &Contrail, points: &[(Vec3, f32)], eye: Vec3) {
    let last = contrail.keys.last().map_or(1.0, |k| k.age).max(1e-3);
    let blank = SceneVertex {
        position: [0.0; 3],
        normal: [0.0, 1.0, 0.0],
        uv: [0.0; 2],
        lightmap_uv: [0.0; 2],
        color: [0.0; 4],
        fx: [0.0; 4],
    };
    track.vertices.clear();
    track.vertices.resize(track.capacity * 4, blank);
    track.quads = 0;
    // The points' sides: across the line through them, turned to the eye.
    let sides: Vec<Option<Vec3>> = (0..points.len())
        .map(|i| {
            let before = points[i.saturating_sub(1)].0;
            let after = points[(i + 1).min(points.len() - 1)].0;
            (after - before)
                .try_normalize()
                .and_then(|d| d.cross(eye - points[i].0).try_normalize())
        })
        .collect();
    for (i, pair) in points.windows(2).enumerate() {
        if track.quads == track.capacity {
            break;
        }
        let ((a, age_a), (b, age_b)) = (pair[0], pair[1]);
        let (Some(side_a), Some(side_b)) = (sides[i], sides[i + 1]) else {
            continue;
        };
        let (Some((width_a, color_a)), Some((width_b, color_b))) =
            (contrail.at(age_a), contrail.at(age_b.min(last - 1e-4)))
        else {
            continue;
        };
        let vertex = |at: Vec3, side: Vec3, width: f32, color: [f32; 4], age: f32, v: f32| {
            let across = side * (width / 2.0) * if v > 0.5 { 1.0 } else { -1.0 };
            SceneVertex {
                position: (at + across).to_array(),
                normal: [0.0, 1.0, 0.0],
                uv: [age / last * contrail.repeats, v],
                lightmap_uv: [0.0; 2],
                color,
                fx: [0.0; 4],
            }
        };
        let at = track.quads * 4;
        track.vertices[at] = vertex(a, side_a, width_a, color_a, age_a, 0.0);
        track.vertices[at + 1] = vertex(a, side_a, width_a, color_a, age_a, 1.0);
        track.vertices[at + 2] = vertex(b, side_b, width_b, color_b, age_b, 1.0);
        track.vertices[at + 3] = vertex(b, side_b, width_b, color_b, age_b, 0.0);
        track.quads += 1;
    }
}

/// A ribbon's mesh: `vertices.len() / 4` quads, unlit, with the texture.
fn mesh(image: &SceneImage, additive: bool, vertices: Vec<SceneVertex>) -> SceneData {
    let quads = (vertices.len() / 4) as u32;
    let indices: Vec<u32> = (0..quads)
        .flat_map(|q| [0, 1, 2, 0, 2, 3].map(|i| q * 4 + i))
        .collect();
    let mut material = Material::surface("contrail", 0, 0);
    material.kind = MaterialKind::Unlit;
    material.alpha = if additive {
        AlphaMode::Additive
    } else {
        AlphaMode::Blend
    };
    material.double_sided = true;
    SceneData {
        id: "contrail".into(),
        name: "contrail".into(),
        batches: vec![MeshBatch {
            zone: None,
            indices: 0..indices.len() as u32,
            material: 0,
            center: [0.0; 3],
        }],
        vertices,
        indices,
        images: vec![image.clone()],
        materials: vec![material],
        ..SceneData::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_weapons::ContrailKey;

    fn contrail() -> Contrail {
        Contrail {
            texture: "pkg/streak.png".into(),
            rate: 30.0,
            keys: vec![
                ContrailKey {
                    age: 0.0,
                    width: 0.4,
                    color: [1.0; 4],
                },
                ContrailKey {
                    age: 0.5,
                    width: 0.1,
                    color: [1.0, 1.0, 1.0, 0.0],
                },
            ],
            additive: true,
            repeats: 4.0,
        }
    }

    fn contrails() -> Contrails {
        let def = bri_weapons::ProjectileDef {
            contrail: Some(contrail()),
            ..Default::default()
        };
        let image = SceneImage {
            label: "streak".into(),
            width: 1,
            height: 1,
            rgba: vec![255; 4],
            srgb: true,
        };
        Contrails::new(&[("bolt".to_string(), def)].into(), |_| Some(&image))
    }

    fn projectile(id: u64, at: Vec3) -> Projectile {
        serde_json::from_value(serde_json::json!({
            "id": id, "definition": "bolt", "source": 1,
            "position": at.to_array(), "velocity": [0.0, 0.0, -50.0], "scale": 1.0, "age": 0,
            "bounced": false, "stuck": false, "origin": at.to_array(), "was_thrown": false,
            "paint": null, "heading": null
        }))
        .unwrap()
    }

    fn fly(c: &mut Contrails, steps: usize, eye: Vec3) {
        let mut z = 0.0;
        for _ in 0..steps {
            z -= 50.0 / 60.0;
            c.observe(&[projectile(1, Vec3::new(0.0, 0.0, z))]);
            c.advance(1.0 / 60.0, eye);
        }
    }

    #[test]
    fn a_ribbon_follows_a_flying_projectile_and_runs_out_after_it() {
        let mut c = contrails();
        assert!(!c.is_empty());
        let eye = Vec3::new(0.0, 5.0, 0.0);
        fly(&mut c, 30, eye);
        assert_eq!(c.live_tracks(), 1);
        let flying = c.quad_count();
        assert!(flying >= 8, "{flying}");
        // The projectile is gone: no more points, and the ribbon is gone
        // once its last key's age has passed.
        for _ in 0..20 {
            c.observe(&[]);
            c.advance(1.0 / 60.0, eye);
        }
        assert!(c.quad_count() < flying, "it shortens");
        for _ in 0..60 {
            c.observe(&[]);
            c.advance(1.0 / 60.0, eye);
        }
        assert_eq!((c.live_tracks(), c.quad_count()), (0, 0));
    }

    #[test]
    fn the_ribbon_thins_fades_faces_the_eye_and_the_texture_runs_along_it() {
        let mut c = contrails();
        let eye = Vec3::new(0.0, 5.0, 0.0);
        fly(&mut c, 24, eye);
        let track = c.tracks.values().next().unwrap();
        let quads = track.quads;
        assert!(quads >= 6);
        let v = |q: usize, k: usize| track.vertices[q * 4 + k];
        let width =
            |q: usize| (Vec3::from(v(q, 0).position) - Vec3::from(v(q, 1).position)).length();
        // The oldest quad is first: thinner and fainter than the newest.
        assert!(width(0) < width(quads - 1));
        assert!(v(0, 0).color[3] < v(quads - 1, 0).color[3]);
        // Its width is across the flight (x), square to the line of flight.
        for q in 0..quads {
            let across = (Vec3::from(v(q, 1).position) - Vec3::from(v(q, 0).position)).normalize();
            assert!(across.dot(Vec3::Z).abs() < 1e-3, "{across}");
        }
        // The texture runs along the ribbon: u rises with age, v across.
        assert!(v(0, 0).uv[0] > v(quads - 1, 0).uv[0]);
        assert_eq!((v(0, 0).uv[1], v(0, 1).uv[1]), (0.0, 1.0));
        // Continuity: a quad ends where the next begins.
        for q in 0..quads - 1 {
            assert!((v(q, 3).uv[0] - v(q + 1, 0).uv[0]).abs() < 1e-5);
        }
    }

    #[test]
    fn contrail_limits_and_aging() {
        let c = contrail();
        c.validate().unwrap();
        assert!(c.at(0.0).is_some() && c.at(0.25).is_some());
        assert!(c.at(0.5).is_none() && c.at(-1.0).is_none());
        let (width, color) = c.at(0.25).unwrap();
        assert!((width - 0.25).abs() < 1e-6 && (color[3] - 0.5).abs() < 1e-6);
        for bad in [
            |c: &mut Contrail| c.rate = 0.5,
            |c: &mut Contrail| c.keys.truncate(1),
            |c: &mut Contrail| c.keys[0].age = 0.1,
            |c: &mut Contrail| c.keys[1].age = 20.0,
            |c: &mut Contrail| c.keys[1].width = f32::NAN,
            |c: &mut Contrail| c.texture = "../x.png".into(),
            |c: &mut Contrail| c.repeats = 0.0,
        ] {
            let mut c = contrail();
            bad(&mut c);
            assert!(c.validate().is_err());
        }
    }
}
