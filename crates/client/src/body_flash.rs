//! Rules' `body_flash`: a see-through colour drawn over a body, just
//! outside it, fading out (a shield taking a hit, a power-up's glow). The
//! body's own posed mesh is drawn a second time, every surface one unlit
//! colour, a little larger than the body.
use crate::avatar::AvatarMesh;
use anyhow::Result;
use bri_console::Clamp;
use bri_render::scene::{
    AlphaMode, GpuInstances, GpuScene, Material, MaterialKind, MeshBatch, SceneData, SceneRenderer,
    SceneTransform, SceneVertex,
};
use glam::{Mat4, Vec3};
use std::collections::BTreeMap;

type OwnerId = u64;
/// How much larger than the body the colour is drawn, so it lies on top.
const INFLATE: f32 = 1.04;
/// Bodies flashing at once; the oldest give way.
const MAX_LIVE: usize = 64;

struct Flash {
    color: [f32; 4],
    left: f32,
    seconds: f32,
    gpu: Option<Overlay>,
}
struct Overlay {
    scene: GpuScene,
    instance: GpuInstances,
    /// The body's mesh this was built from: rebuilt when it changes.
    shape: (usize, usize),
}

#[derive(Default)]
pub struct BodyFlashes {
    live: BTreeMap<OwnerId, Flash>,
}

impl BodyFlashes {
    pub fn add(&mut self, owner: OwnerId, color: [f32; 4], seconds: f32) {
        if !(color.iter().all(|c| c.is_finite()) && seconds.is_finite() && seconds > 0.0) {
            return;
        }
        if !self.live.contains_key(&owner) && self.live.len() >= MAX_LIVE {
            let shortest = self
                .live
                .iter()
                .min_by(|a, b| a.1.left.total_cmp(&b.1.left))
                .map(|(o, _)| *o);
            if let Some(o) = shortest {
                self.live.remove(&o);
            }
        }
        let seconds = seconds.min(60.0);
        let gpu = self.live.remove(&owner).and_then(|f| f.gpu);
        self.live.insert(
            owner,
            Flash {
                color: color.map(|c| c.clamped(0.0, 1.0)),
                left: seconds,
                seconds,
                gpu,
            },
        );
    }
    pub fn clear(&mut self) {
        self.live.clear();
    }
    pub fn gpu_stopped(&mut self) {
        for flash in self.live.values_mut() {
            flash.gpu = None;
        }
    }
    pub fn live_count(&self) -> usize {
        self.live.len()
    }
    /// The colour drawn over `owner` now, its alpha faded.
    pub fn tint(&self, owner: OwnerId) -> Option<[f32; 4]> {
        let f = self.live.get(&owner)?;
        let [r, g, b, a] = f.color;
        Some([r, g, b, a * (f.left / f.seconds).clamped(0.0, 1.0)])
    }
    pub fn advance(&mut self, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamped(0.0, 0.25)
        } else {
            0.0
        };
        for flash in self.live.values_mut() {
            flash.left -= dt;
        }
        self.live.retain(|_, f| f.left > 0.0);
    }
    /// Bring each flashing body's copy up to date with its drawn mesh;
    /// bodies not drawn this frame (`drawn` false) are skipped.
    pub fn upload(
        &mut self,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        avatars: &BTreeMap<OwnerId, AvatarMesh>,
        drawn: impl Fn(OwnerId) -> bool,
    ) -> Result<()> {
        for (owner, flash) in &mut self.live {
            let Some(avatar) = avatars
                .get(owner)
                .filter(|a| a.gpu.is_some() && drawn(*owner))
            else {
                continue;
            };
            let data = &avatar.data;
            if data.vertices.is_empty() || data.indices.is_empty() {
                continue;
            }
            let shape = (data.vertices.len(), data.indices.len());
            let vertices: Vec<SceneVertex> = data
                .vertices
                .iter()
                .map(|v| SceneVertex {
                    color: [1.0; 4],
                    ..*v
                })
                .collect();
            let center = vertices
                .iter()
                .fold(Vec3::ZERO, |sum, v| sum + Vec3::from(v.position))
                / vertices.len() as f32;
            match &mut flash.gpu {
                Some(overlay) if overlay.shape == shape => {
                    overlay
                        .scene
                        .update_vertices(queue, &vertices, &[center.to_array()])?;
                }
                _ => {
                    let mut material = Material::surface("body flash", 0, 0);
                    material.kind = MaterialKind::Unlit;
                    material.alpha = AlphaMode::Blend;
                    let copy = SceneData {
                        id: "body flash".into(),
                        name: "body flash".into(),
                        batches: vec![MeshBatch {
                            zone: None,
                            indices: 0..data.indices.len() as u32,
                            material: 0,
                            center: center.to_array(),
                        }],
                        vertices,
                        indices: data.indices.clone(),
                        materials: vec![material],
                        ..SceneData::default()
                    };
                    flash.gpu = Some(Overlay {
                        scene: renderer.upload(device, queue, &copy)?,
                        instance: GpuInstances::new(device, 1)?,
                        shape,
                    });
                }
            }
            let [r, g, b, a] = flash.color;
            let fade = (flash.left / flash.seconds).clamped(0.0, 1.0);
            // A little larger about the body's middle.
            let transform = avatar.drawn_transform();
            let middle = transform.transform_point3(center);
            let inflate = Mat4::from_translation(middle)
                * Mat4::from_scale(Vec3::splat(INFLATE))
                * Mat4::from_translation(-middle);
            if let Some(overlay) = &mut flash.gpu {
                overlay.instance.update(
                    queue,
                    &[SceneTransform {
                        transform: inflate * transform,
                        tint: [r, g, b, a * fade],
                    }],
                )?;
            }
        }
        Ok(())
    }
    /// This frame's copies, for bodies drawn this frame (`drawn`).
    pub fn draws(
        &self,
        drawn: impl Fn(OwnerId) -> bool,
    ) -> impl Iterator<Item = (&GpuScene, &GpuInstances)> {
        self.live
            .iter()
            .filter(move |(owner, _)| drawn(**owner))
            .filter_map(|(_, f)| f.gpu.as_ref().map(|o| (&o.scene, &o.instance)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flash_fades_over_its_seconds_and_a_new_one_restarts_it() {
        let mut flashes = BodyFlashes::default();
        flashes.add(3, [0.2, 0.6, 1.0, 0.8], 1.0);
        flashes.add(4, [f32::NAN, 0.0, 0.0, 1.0], 1.0);
        assert_eq!(flashes.live_count(), 1, "a broken colour is dropped");
        flashes.advance(0.25);
        flashes.advance(0.25);
        let a = flashes.tint(3).unwrap()[3];
        assert!((a - 0.4).abs() < 1e-5, "{a}");
        flashes.add(3, [0.2, 0.6, 1.0, 0.8], 1.0);
        assert_eq!(flashes.tint(3).unwrap()[3], 0.8, "hit again, full again");
        for _ in 0..4 {
            flashes.advance(0.25);
        }
        assert_eq!(flashes.live_count(), 0);
    }
}
