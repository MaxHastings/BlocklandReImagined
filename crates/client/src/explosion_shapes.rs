//! `explosionShape` models (the rocket's expanding sphere) drawn at their
//! explosions for the shape's ambient sequence: node scale keys and object
//! visibility become each instance's transform and tint alpha.
use anyhow::{Context, Result, ensure};
use bri_content::shape::Shape;
use bri_render::scene::{
    GpuInstances, GpuScene, SceneData, SceneImage, SceneRenderer, SceneTransform,
};
use bri_sim::presentation::{Cue, CueKind};
use glam::{Mat4, Vec3};
use std::{collections::BTreeMap, path::Path};

/// Concurrent explosions drawn per shape.
const MAX_LIVE: usize = 64;

struct Model {
    data: SceneData,
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    transforms: Vec<SceneTransform>,
    /// Ambient node scale and object visibility keys, evenly spaced.
    scales: Vec<Vec3>,
    visibility: Vec<f32>,
    duration: f32,
    base_scale: Vec3,
}

pub struct ExplosionShapes {
    /// Keyed by lower-case explosion name.
    models: BTreeMap<String, Model>,
    live: Vec<(String, Vec3, f32)>,
    cursor: u64,
}

impl ExplosionShapes {
    pub fn load(pack: &bri_weapons::Pack, root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let mut models = BTreeMap::new();
        for (key, explosion) in pack.explosions.iter().filter(|(_, e)| !e.shape.is_empty()) {
            // A shape no package converted, such as a path naming a folder
            // the game does not have (HE Grenade's `Weapon_Rocket Launcher`,
            // with a space), means no shape: the explosion keeps its
            // particles, lights, sounds and damage. Torque found no file
            // there either, so the original drew none.
            let Some((resource, file)) = pack
                .resources
                .iter()
                .find(|r| {
                    r.path.eq_ignore_ascii_case(&explosion.shape) && r.native_file.is_some()
                })
                .and_then(|r| Some((r, r.native_file.as_deref()?)))
            else {
                bri_console::warn(format!(
                    "Explosion {}: shape {} is not provided by the game or any Add-On, so it shows without one",
                    explosion.name, explosion.shape
                ));
                continue;
            };
            // An Add-On's explosion shape that does not load is a cosmetic
            // fault (`crate::cosmetic`): the explosion keeps its particles,
            // lights and sounds and loses only the shape.
            let owner = resource
                .package
                .as_ref()
                .map(|dir| {
                    bri_package::library::add_on_label(
                        &bri_weapons::resource_root(&root, resource),
                        dir,
                    )
                })
                .or_else(|| key.split_once(':').map(|(package, _)| package.to_string()));
            let model = (|| -> Result<Model> {
            let shape: Shape = serde_json::from_slice(&crate::materials::read_resource(
                &bri_weapons::resource_root(&root, resource),
                file,
                32 << 20,
            )?)?;
            shape.validate()?;
            let folder = explosion.shape.rsplit_once('/').map_or("", |(dir, _)| dir);
            let mut images = Vec::new();
            for material in &shape.materials {
                let path = format!("{folder}/{}.png", material.name);
                let texture = pack
                    .resources
                    .iter()
                    .find(|r| r.path.eq_ignore_ascii_case(&path))
                    .with_context(|| format!("Missing explosion texture {path}"))?;
                let bytes = crate::items::checked_read(
                    &bri_weapons::resource_root(&root, texture),
                    texture.native_file.as_deref().context("Unstored texture")?,
                    &texture.sha256,
                    16 << 20,
                )?;
                let image = image::load_from_memory(&bytes)?.to_rgba8();
                images.push(SceneImage {
                    label: texture.path.clone(),
                    width: image.width(),
                    height: image.height(),
                    rgba: image.into_raw(),
                    srgb: false,
                });
            }
            let refs: Vec<&SceneImage> = images.iter().collect();
            let pose = bri_content::animation::sample(&shape, None, 0.0)?;
            let data = crate::items::native_shape_scene(
                &explosion.shape,
                &shape,
                &refs,
                [1.0; 4],
                false,
                Mat4::IDENTITY,
                &pose,
            )?;
            let ambient = shape
                .animations
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case("ambient"));
            let scales = ambient
                .and_then(|a| a.nodes.iter().find(|n| !n.scales.is_empty()))
                .map_or_else(Vec::new, |n| {
                    n.scales.iter().map(|s| Vec3::from(*s)).collect()
                });
            let visibility = ambient
                .and_then(|a| a.objects.first())
                .map_or_else(Vec::new, |o| o.visibility.clone());
            // The explosion lasts the shape's sequence at `playSpeed`, at least `lifetimeMS`.
            let duration = ambient
                .map_or(0.0, |a| a.duration / explosion.play_speed.max(0.01))
                .max(explosion.seconds);
            ensure!(
                duration > 0.0 && duration <= 60.0,
                "Invalid explosion duration"
            );
            Ok(Model {
                    data,
                    gpu: None,
                    instances: None,
                    transforms: Vec::new(),
                    scales,
                    visibility,
                    duration,
                    base_scale: Vec3::from(explosion.scale),
                })
            })();
            match (model, &owner) {
                (Ok(model), _) => {
                    models.insert(key.clone(), model);
                }
                (Err(error), Some(dir)) => {
                    crate::cosmetic::add_on_fault(dir, &explosion.shape, format!("{error:#}"));
                }
                (Err(error), None) => return Err(error),
            }
        }
        Ok(Self {
            models,
            live: Vec::new(),
            cursor: 0,
        })
    }
    pub fn live_count(&self) -> usize {
        self.live.len()
    }
    pub fn reset(&mut self, checkpoint_cursor: u64) {
        self.live.clear();
        self.cursor = checkpoint_cursor;
    }
    pub fn cue(&mut self, cue: &Cue) {
        if cue.id <= self.cursor {
            return;
        }
        self.cursor = cue.id;
        if let CueKind::WeaponEffect { definition, .. } = &cue.kind {
            let key = definition.to_ascii_lowercase();
            if self.models.contains_key(&key)
                && self.live.iter().filter(|(k, _, _)| *k == key).count() < MAX_LIVE
            {
                self.live.push((key, Vec3::from(cue.position), 0.0));
            }
        }
    }
    /// Age the explosions and rebuild this frame's instances.
    pub fn advance(&mut self, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        for model in self.models.values_mut() {
            model.transforms.clear();
        }
        let models = &mut self.models;
        self.live.retain_mut(|(key, position, age)| {
            *age += dt;
            let Some(model) = models.get_mut(key.as_str()) else {
                return false;
            };
            if *age >= model.duration {
                return false;
            }
            let t = *age / model.duration;
            let scale = sample(&model.scales, t).unwrap_or(Vec3::ONE) * model.base_scale;
            let alpha = sample1(&model.visibility, t).unwrap_or(1.0).clamp(0.0, 1.0);
            model.transforms.push(SceneTransform {
                transform: Mat4::from_scale_rotation_translation(
                    scale,
                    glam::Quat::IDENTITY,
                    *position,
                ),
                tint: [1.0, 1.0, 1.0, alpha],
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
        for model in self.models.values_mut() {
            if model.transforms.is_empty() {
                if let Some(instances) = &mut model.instances {
                    instances.update(queue, &[])?;
                }
                continue;
            }
            if model.gpu.is_none() {
                model.gpu = Some(renderer.upload(device, queue, &model.data)?);
            }
            if model
                .instances
                .as_ref()
                .is_none_or(|i| i.capacity() < model.transforms.len())
            {
                model.instances = Some(GpuInstances::new(
                    device,
                    model.transforms.len().next_power_of_two().max(4),
                )?);
            }
            model
                .instances
                .as_mut()
                .unwrap()
                .update(queue, &model.transforms)?;
        }
        Ok(())
    }
    pub fn draws(&self) -> Vec<(&GpuScene, &GpuInstances)> {
        self.models
            .values()
            .filter(|m| !m.transforms.is_empty())
            .filter_map(|m| Some((m.gpu.as_ref()?, m.instances.as_ref()?)))
            .collect()
    }
    pub fn gpu_stopped(&mut self) {
        for model in self.models.values_mut() {
            model.gpu = None;
            model.instances = None;
        }
    }
}

/// Linear interpolation over evenly spaced keys at fraction `t`.
fn sample(keys: &[Vec3], t: f32) -> Option<Vec3> {
    let (a, b, f) = span(keys.len(), t)?;
    Some(keys[a].lerp(keys[b], f))
}
fn sample1(keys: &[f32], t: f32) -> Option<f32> {
    let (a, b, f) = span(keys.len(), t)?;
    Some(keys[a] + (keys[b] - keys[a]) * f)
}
fn span(len: usize, t: f32) -> Option<(usize, usize, f32)> {
    if len == 0 {
        return None;
    }
    let x = t.clamp(0.0, 1.0) * (len - 1) as f32;
    let a = x.floor() as usize;
    let b = (a + 1).min(len - 1);
    Some((a, b, x - a as f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_interpolate_evenly_and_clamp() {
        let keys = [Vec3::ONE, Vec3::splat(3.0), Vec3::splat(5.0)];
        assert_eq!(sample(&keys, 0.25), Some(Vec3::splat(2.0)));
        assert_eq!(sample(&keys, 2.0), Some(Vec3::splat(5.0)));
        assert_eq!(sample1(&[1.0, 0.0], 0.5), Some(0.5));
        assert_eq!(sample1(&[], 0.5), None);
    }
    #[test]
    fn an_explosion_whose_shape_no_package_converted_loads_without_one() -> Result<()> {
        // HE Grenade names `Weapon_Rocket Launcher` (a space) where the game
        // has `Weapon_Rocket_Launcher`; another explosion names a shape whose
        // conversion failed. Neither stops the load or the explosion.
        let explosion = |name: &str, shape: &str| bri_weapons::ExplosionInfo {
            name: name.into(),
            sound: String::new(),
            shake: None,
            shape: shape.into(),
            seconds: 0.5,
            play_speed: 1.0,
            face_viewer: true,
            scale: [1.0; 3],
            sizes: Vec::new(),
        };
        let pack = bri_weapons::Pack {
            effects: Default::default(),
            schema_version: bri_weapons::SCHEMA,
            id: "test".into(),
            items: Default::default(),
            images: Default::default(),
            projectiles: Default::default(),
            damage_types: Default::default(),
            explosions: [
                (
                    "hegrenadeexplosion".into(),
                    explosion(
                        "hegrenadeExplosion",
                        "Add-Ons/Weapon_Rocket Launcher/explosionSphere1.dts",
                    ),
                ),
                (
                    "brokenexplosion".into(),
                    explosion("brokenExplosion", "Add-Ons/Weapon_Broken/shape.dts"),
                ),
            ]
            .into(),
            sounds: Default::default(),
            definitions: Vec::new(),
            resources: vec![bri_weapons::Resource {
                path: "Add-Ons/Weapon_Broken/shape.dts".into(),
                sha256: "0".repeat(64),
                native_file: None,
                diagnostics: vec!["DTS conversion: unsupported".into()],
                package: None,
            }],
            diagnostics: Vec::new(),
        };
        let root = tempfile::tempdir()?;
        let shapes = ExplosionShapes::load(&pack, root.path())?;
        assert!(shapes.models.is_empty());
        Ok(())
    }
    #[test]
    #[ignore = "requires generated native weapons-pack-009; CPU only"]
    fn rocket_explosion_sphere_expands_and_fades() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/weapons-pack-009");
        let pack = bri_weapons::Pack::from_json(&std::fs::read(root.join("weapons.json"))?)?;
        let mut shapes = ExplosionShapes::load(&pack, &root)?;
        shapes.cue(&Cue {
            id: 1,
            tick: 1,
            kind: CueKind::WeaponEffect {
                source: bri_weapons::TargetId::Map(0),
                definition: "rocketExplosion".into(),
                node: String::new(),
                seconds: 0.0,
                image: None,
                hand: None,
                direction: None,
                scale: 1.0,
            },
            position: [1.0, 2.0, 3.0],
        });
        shapes.advance(0.05);
        let model = &shapes.models["rocketexplosion"];
        let first = model.transforms[0];
        shapes.advance(0.1);
        let model = &shapes.models["rocketexplosion"];
        let later = &model.transforms[0];
        assert!(
            later.transform.x_axis.x > first.transform.x_axis.x,
            "expands"
        );
        assert!(later.tint[3] < first.tint[3], "fades");
        shapes.advance(1.0);
        assert_eq!(shapes.live_count(), 0);
        Ok(())
    }
}
