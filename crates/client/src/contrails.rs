//! Projectile contrails (`bri_weapons::ProjectileDef::contrail`): a ribbon of
//! textured, camera-facing quads through the points a projectile has passed,
//! thinning and fading by each point's age. Presentation only; each client
//! keeps its own points from the projectiles it is shown.
use anyhow::Result;
use bri_render::scene::{
    AlphaMode, GpuInstances, GpuScene, Material, MaterialKind, MeshBatch, SceneData, SceneImage,
    SceneRenderer, SceneTransform, SceneVertex,
};
use bri_weapons::{Contrail, Projectile};
use glam::{Mat4, Vec3};
use std::collections::{BTreeMap, VecDeque};

/// Most ribbons drawn at once, and most points one keeps.
const MAX_TRACKS: usize = 64;
const MAX_POINTS: usize = 512;

struct Track {
    definition: String,
    points: VecDeque<(Vec3, f32)>,
    /// Seconds of the next point's interval already passed.
    carry: f32,
    /// Where the projectile is now; None once it is gone.
    head: Option<Vec3>,
}

/// The quads of every contrail drawn with one texture.
struct Group {
    data: SceneData,
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    transforms: Vec<SceneTransform>,
}

#[derive(Default)]
pub struct Contrails {
    /// Each projectile definition's contrail and its texture's group.
    definitions: BTreeMap<String, (Contrail, String)>,
    groups: BTreeMap<String, Group>,
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
            me.groups
                .entry(group_key(contrail))
                .or_insert_with(|| Group {
                    data: quad(image, contrail.additive),
                    gpu: None,
                    instances: None,
                    transforms: Vec::new(),
                });
            me.definitions
                .insert(id.clone(), (contrail.clone(), group_key(contrail)));
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
        for g in self.groups.values_mut() {
            g.transforms.clear();
        }
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
            if !self.definitions.contains_key(&p.definition) || !p.position.is_finite() {
                continue;
            }
            if !self.tracks.contains_key(&p.id) && self.tracks.len() >= MAX_TRACKS {
                continue;
            }
            let track = self.tracks.entry(p.id).or_insert_with(|| Track {
                definition: p.definition.clone(),
                points: VecDeque::new(),
                carry: 0.0,
                head: None,
            });
            track.head = Some(p.position);
            if track.points.is_empty() {
                track.points.push_back((p.position, 0.0));
            }
        }
    }
    /// Age the points, add this interval's and rebuild the quads, turned to
    /// face `eye`.
    pub fn advance(&mut self, dt: f32, eye: Vec3) {
        let dt = if dt.is_finite() {
            bri_console::Clamp::clamped(dt, 0.0, 0.25)
        } else {
            0.0
        };
        for g in self.groups.values_mut() {
            g.transforms.clear();
        }
        let definitions = &self.definitions;
        let groups = &mut self.groups;
        self.tracks.retain(|_, track| {
            let Some((contrail, group)) = definitions.get(&track.definition) else {
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
                    if track.points.len() == MAX_POINTS {
                        track.points.pop_front();
                    }
                    // Newest at the back, aged by what is left of its interval.
                    track.points.push_back((head, track.carry));
                }
            }
            if track.points.is_empty() && track.head.is_none() {
                return false;
            }
            let Some(g) = groups.get_mut(group) else {
                return true;
            };
            let mut segments: Vec<(Vec3, f32)> = track.points.iter().copied().collect();
            if let Some(head) = track.head {
                segments.push((head, 0.0));
            }
            for pair in segments.windows(2) {
                let ((a, age_a), (b, age_b)) = (pair[0], pair[1]);
                // Older toward the front: this segment's age is its mid age.
                let age = (age_a + age_b) / 2.0;
                let Some((width, [r, gr, bl, alpha])) = contrail.at(age) else {
                    continue;
                };
                if let Some(transform) = quad_transform(a, b, width, eye) {
                    g.transforms.push(SceneTransform {
                        transform,
                        tint: [r, gr, bl, alpha],
                    });
                }
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
        for g in self.groups.values_mut() {
            if g.transforms.is_empty() {
                if let Some(instances) = &mut g.instances {
                    instances.update(queue, &[])?;
                }
                continue;
            }
            if g.gpu.is_none() {
                g.gpu = Some(renderer.upload(device, queue, &g.data)?);
            }
            if g.instances
                .as_ref()
                .is_none_or(|i| i.capacity() < g.transforms.len())
            {
                g.instances = Some(GpuInstances::new(
                    device,
                    g.transforms.len().next_power_of_two().max(64),
                )?);
            }
            g.instances
                .as_mut()
                .expect("created above")
                .update(queue, &g.transforms)?;
        }
        Ok(())
    }
    pub fn draws(&self) -> impl Iterator<Item = (&GpuScene, &GpuInstances)> {
        self.groups.values().filter_map(|g| {
            if g.transforms.is_empty() {
                return None;
            }
            Some((g.gpu.as_ref()?, g.instances.as_ref()?))
        })
    }
    pub fn gpu_stopped(&mut self) {
        for g in self.groups.values_mut() {
            g.gpu = None;
            g.instances = None;
        }
    }
    #[cfg(test)]
    fn quad_count(&self) -> usize {
        self.groups.values().map(|g| g.transforms.len()).sum()
    }
}

fn group_key(contrail: &Contrail) -> String {
    format!("{}|{}", contrail.texture, contrail.additive)
}

/// The unit quad (1 wide along X, 1 long from z 0 to -1, in the XZ plane)
/// with the texture along its length (u) and across its width (v), unlit.
fn quad(image: &SceneImage, additive: bool) -> SceneData {
    let vertex = |x: f32, z: f32, u: f32, v: f32| SceneVertex {
        position: [x, 0.0, z],
        normal: [0.0, 1.0, 0.0],
        uv: [u, v],
        lightmap_uv: [0.0; 2],
        color: [1.0; 4],
        fx: [0.0; 4],
    };
    let vertices = vec![
        vertex(-0.5, 0.0, 0.0, 0.0),
        vertex(0.5, 0.0, 0.0, 1.0),
        vertex(0.5, -1.0, 1.0, 1.0),
        vertex(-0.5, -1.0, 1.0, 0.0),
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
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

/// The unit quad stretched from `from` to `to`, `width` wide and turned to
/// face `eye` about the line between them.
fn quad_transform(from: Vec3, to: Vec3, width: f32, eye: Vec3) -> Option<Mat4> {
    let span = to - from;
    let length = span.length();
    if !(length.is_finite() && length > 1e-4 && width.is_finite() && width > 0.0) {
        return None;
    }
    let direction = span / length;
    let view = eye - (from + to) / 2.0;
    let right = direction.cross(view).try_normalize()?;
    let normal = right.cross(direction);
    // Local -z runs from `from` to `to`; x is across, y the quad's normal.
    Some(Mat4::from_cols(
        (right * width).extend(0.0),
        normal.extend(0.0),
        (-span).extend(0.0),
        from.extend(1.0),
    ))
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

    #[test]
    fn a_ribbon_follows_a_flying_projectile_and_runs_out_after_it() {
        let mut c = contrails();
        assert!(!c.is_empty());
        let eye = Vec3::new(0.0, 5.0, 0.0);
        let mut z = 0.0;
        for _ in 0..30 {
            z -= 50.0 / 60.0;
            c.observe(&[projectile(1, Vec3::new(0.0, 0.0, z))]);
            c.advance(1.0 / 60.0, eye);
        }
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
    fn older_quads_are_thinner_and_fainter_and_all_face_the_eye() {
        let mut c = contrails();
        let eye = Vec3::new(0.0, 5.0, 0.0);
        let mut z = 0.0;
        for _ in 0..24 {
            z -= 50.0 / 60.0;
            c.observe(&[projectile(1, Vec3::new(0.0, 0.0, z))]);
            c.advance(1.0 / 60.0, eye);
        }
        let group = c.groups.values().next().unwrap();
        let t = &group.transforms;
        let width = |i: usize| t[i].transform.x_axis.truncate().length();
        assert!(width(0) < width(t.len() - 1), "the oldest is the thinnest");
        assert!(t[0].tint[3] < t[t.len() - 1].tint[3], "and the faintest");
        for q in t {
            // Its normal is square to the line along its length and turned
            // toward the eye as far as that allows.
            let normal = q.transform.y_axis.truncate().normalize();
            let along = q.transform.z_axis.truncate().normalize();
            let to_eye = (eye - q.transform.w_axis.truncate()).normalize();
            assert!(normal.dot(along).abs() < 1e-4);
            assert!(normal.dot(to_eye) > 0.0, "{normal} vs {to_eye}");
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
        ] {
            let mut c = contrail();
            bad(&mut c);
            assert!(c.validate().is_err());
        }
    }
}
