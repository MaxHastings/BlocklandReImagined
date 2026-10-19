//! Rules' `body_shell`: one of the weapons pack's shells (`effects.shells`) over
//! a body, just outside it, at a strength that fades away (a shield taking a
//! hit). The body's own posed mesh is drawn a second time as a plasma material,
//! pushed off along its normals by the shell's stand-off.
use crate::avatar::AvatarMesh;
use anyhow::Result;
use bri_console::Clamp;
use bri_render::scene::{
    AlphaMode, GpuInstances, GpuScene, Material, MaterialKind, MeshBatch, SceneData, SceneImage,
    SceneRenderer, SceneTransform, SceneVertex,
};
use bri_weapons::Shell;
use glam::Vec3;
use std::collections::BTreeMap;

type OwnerId = u64;
/// Bodies shown a shell at once; the one with least left gives way.
const MAX_LIVE: usize = 64;

struct Live {
    shell: Shell,
    images: [SceneImage; 2],
    strength: f32,
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
pub struct BodyShells {
    live: BTreeMap<OwnerId, Live>,
}

/// The strength a shell shows when `share` of its fade is left.
pub fn eased(strength: f32, share: f32, ease: f32) -> f32 {
    strength * (1.0 - (1.0 - share.clamped(0.0, 1.0)).powf(ease))
}

/// The plasma material a shell draws with: its two noise images are the scene's
/// images 0 and 1.
pub fn plasma_material(shell: &Shell) -> Material {
    let mut material = Material::surface("body shell", 0, 0);
    material.kind = MaterialKind::Plasma;
    material.alpha = AlphaMode::Additive;
    material.images = [0; 13];
    material.images[1] = 1;
    let tint = |c: [f32; 4], depth: f32| [c[0] * c[3], c[1] * c[3], c[2] * c[3], depth];
    let layer = |n: &bri_weapons::ShellNoise| [n.repeats, n.scroll[0], n.scroll[1], n.seconds];
    material.parameters = Some([
        tint(shell.facing, shell.noise[0].scroll[2]),
        tint(shell.edge, shell.noise[1].scroll[2]),
        layer(&shell.noise[0]),
        layer(&shell.noise[1]),
    ]);
    material
}

impl BodyShells {
    /// Start (or restart) `shell` over `owner`, with the two noise images it
    /// draws.
    pub fn add(
        &mut self,
        owner: OwnerId,
        shell: &Shell,
        images: [SceneImage; 2],
        strength: f32,
        seconds: f32,
    ) {
        if !(strength.is_finite() && seconds.is_finite() && seconds > 0.0 && shell.valid()) {
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
        // Keep the GPU copy when the same shell restarts.
        let gpu = self
            .live
            .remove(&owner)
            .filter(|l| l.shell == *shell)
            .and_then(|l| l.gpu);
        self.live.insert(
            owner,
            Live {
                shell: shell.clone(),
                images,
                strength: strength.clamped(0.0, 1.0),
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
        for live in self.live.values_mut() {
            live.gpu = None;
        }
    }
    pub fn live_count(&self) -> usize {
        self.live.len()
    }
    /// How strongly `owner`'s shell shows now.
    pub fn showing(&self, owner: OwnerId) -> Option<f32> {
        let l = self.live.get(&owner)?;
        Some(eased(l.strength, l.left / l.seconds, l.shell.ease))
    }
    pub fn advance(&mut self, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamped(0.0, 0.25)
        } else {
            0.0
        };
        for live in self.live.values_mut() {
            live.left -= dt;
        }
        self.live.retain(|_, l| l.left > 0.0);
    }
    /// Bring each shelled body's copy up to date with its drawn mesh; bodies
    /// not drawn this frame (`drawn` false) are skipped.
    pub fn upload(
        &mut self,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        avatars: &BTreeMap<OwnerId, AvatarMesh>,
        drawn: impl Fn(OwnerId) -> bool,
    ) -> Result<()> {
        for (owner, live) in &mut self.live {
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
            let strength = eased(live.strength, live.left / live.seconds, live.shell.ease);
            let stand_off = live.shell.stand_off * strength * strength;
            let vertices: Vec<SceneVertex> = data
                .vertices
                .iter()
                .map(|v| SceneVertex {
                    position: (Vec3::from(v.position) + Vec3::from(v.normal) * stand_off)
                        .to_array(),
                    color: [1.0; 4],
                    ..*v
                })
                .collect();
            let center = vertices
                .iter()
                .fold(Vec3::ZERO, |sum, v| sum + Vec3::from(v.position))
                / vertices.len() as f32;
            let shape = (data.vertices.len(), data.indices.len());
            match &mut live.gpu {
                Some(overlay) if overlay.shape == shape => {
                    overlay
                        .scene
                        .update_vertices(queue, &vertices, &[center.to_array()])?;
                }
                _ => {
                    let material = plasma_material(&live.shell);
                    let copy = SceneData {
                        id: "body shell".into(),
                        name: "body shell".into(),
                        batches: vec![MeshBatch {
                            indices: 0..data.indices.len() as u32,
                            material: 0,
                            center: center.to_array(),
                        }],
                        vertices,
                        indices: data.indices.clone(),
                        materials: vec![material],
                        images: live.images.to_vec(),
                        ..SceneData::default()
                    };
                    live.gpu = Some(Overlay {
                        scene: renderer.upload(device, queue, &copy)?,
                        instance: GpuInstances::new(device, 1)?,
                        shape,
                    });
                }
            }
            if let Some(overlay) = &mut live.gpu {
                overlay.instance.update(
                    queue,
                    &[SceneTransform {
                        transform: avatar.drawn_transform(),
                        tint: [1.0, 1.0, 1.0, strength],
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
            .filter_map(|(_, l)| l.gpu.as_ref().map(|o| (&o.scene, &o.instance)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_weapons::ShellNoise;

    fn shell() -> Shell {
        let noise = ShellNoise {
            texture: "ce-fx/noise.png".into(),
            repeats: 10.0,
            scroll: [2.0, 0.0, 0.0],
            seconds: 2.2,
        };
        Shell {
            id: "pack:shell/shield".into(),
            facing: [1.0, 0.545, 0.243, 1.0],
            edge: [0.067, 0.067, 1.0, 1.0],
            stand_off: 0.12,
            ease: 3.0,
            noise: [noise.clone(), noise],
        }
    }
    fn image() -> SceneImage {
        SceneImage {
            label: "noise".into(),
            width: 1,
            height: 1,
            rgba: vec![255; 4],
            srgb: false,
        }
    }

    #[test]
    fn a_shell_fades_by_its_ease_and_a_new_one_restarts_it() {
        let mut shells = BodyShells::default();
        shells.add(3, &shell(), [image(), image()], 1.0, 1.0);
        assert_eq!(shells.live_count(), 1);
        assert!(
            (shells.showing(3).unwrap() - 1.0).abs() < 1e-6,
            "full strength at the start"
        );
        shells.advance(0.25);
        shells.advance(0.25);
        // Half the fade left at ease 3: 1 - 0.5^3.
        assert!((shells.showing(3).unwrap() - 0.875).abs() < 1e-5);
        shells.add(3, &shell(), [image(), image()], 0.5, 1.0);
        shells.advance(0.25);
        shells.advance(0.25);
        assert!(
            (shells.showing(3).unwrap() - 0.4375).abs() < 1e-5,
            "restarted at its strength"
        );
        for _ in 0..4 {
            shells.advance(0.25);
        }
        assert_eq!(shells.live_count(), 0, "gone when the fade ends");
    }

    #[test]
    fn a_broken_shell_or_strength_is_dropped() {
        let mut shells = BodyShells::default();
        shells.add(1, &shell(), [image(), image()], f32::NAN, 1.0);
        shells.add(2, &shell(), [image(), image()], 1.0, 0.0);
        let mut broken = shell();
        broken.ease = 0.0;
        shells.add(3, &broken, [image(), image()], 1.0, 1.0);
        assert_eq!(shells.live_count(), 0);
    }
}
