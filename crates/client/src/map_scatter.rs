//! The map's scatter layers ([`bri_content::scatter`]) drawn as instanced models: one uploaded model per layer and, each time the viewer has moved, the
//! placements within the layer's fade range uploaded as instance transforms (an instance's tint alpha carries its fade).
use anyhow::Result;
use bri_content::scatter::{Order, visibility};
use bri_render::scene::{GpuInstances, GpuScene, SceneRenderer, SceneTransform};
use bri_render::scene_loader::ScatterScene;
use glam::Vec3;

/// Most instances one instance buffer holds (`GpuInstances::new` takes 1 to 16384).
const CHUNK: usize = 16384;
/// How far the viewer moves before the placements are chosen again, in world units.
const MOVE_THRESHOLD: f32 = 0.5;

struct Layer {
    scene: ScatterScene,
    gpu: Option<GpuScene>,
    /// One buffer per `CHUNK` of the placements shown, grown as needed.
    chunks: Vec<GpuInstances>,
    /// How many of `chunks` hold something to draw now.
    shown: usize,
    /// Where the viewer was when the placements shown were chosen.
    chosen_at: Option<Vec3>,
}

/// Every scatter layer of the current map.
#[derive(Default)]
pub struct ClientScatter {
    layers: Vec<Layer>,
    /// The viewer's detail setting; a layer needing more than this is not drawn. Nothing sets it yet, so every layer draws.
    detail: Option<u8>,
}

/// The transforms of the placements to draw from `eye`: those not faded out, each tinted by its own tint and its fade (doubled when the layer
/// modulates 2x), ordered as the layer says. Distance order is by whole units, ties in the placements' own order.
fn choose(scene: &ScatterScene, eye: Vec3) -> Vec<SceneTransform> {
    let (instances, fade) = (&scene.instances, scene.fade);
    let gain = if scene.modulate_2x { 2.0 } else { 1.0 };
    let far2 = fade.map(|[_, far]| far * far);
    let mut out: Vec<(i64, SceneTransform)> = Vec::new();
    for instance in instances.iter() {
        let distance2 = (Vec3::from(instance.position) - eye).length_squared();
        if far2.is_some_and(|f| distance2 > f) {
            continue;
        }
        let shown = visibility(fade, distance2.sqrt());
        if shown <= 0.0 {
            continue;
        }
        let [r, g, b, a] = instance.tint;
        out.push((
            distance2.sqrt() as i64,
            SceneTransform {
                transform: instance.matrix(),
                tint: [
                    f32::from(r) / 255.0 * gain,
                    f32::from(g) / 255.0 * gain,
                    f32::from(b) / 255.0 * gain,
                    f32::from(a) / 255.0 * shown,
                ],
            },
        ));
    }
    match scene.order {
        Order::Unsorted => {}
        Order::NearToFar => out.sort_by_key(|(d, _)| *d),
        Order::FarToNear => out.sort_by_key(|(d, _)| std::cmp::Reverse(*d)),
    }
    out.into_iter().map(|(_, t)| t).collect()
}

impl ClientScatter {
    /// The new map's layers (`MapScene::scatter`).
    pub fn set_map(&mut self, layers: Vec<ScatterScene>) {
        self.layers = layers
            .into_iter()
            .map(|scene| Layer {
                scene,
                gpu: None,
                chunks: Vec::new(),
                shown: 0,
                chosen_at: None,
            })
            .collect();
    }
    /// Swap the models for ones loaded again (a lighting mode change), keeping the placements.
    pub fn replace_scenes(&mut self, layers: Vec<ScatterScene>) {
        self.set_map(layers);
    }
    pub fn count(&self) -> usize {
        self.layers.len()
    }
    /// The viewer's detail setting (None: every layer draws).
    pub fn set_detail(&mut self, detail: Option<u8>) {
        self.detail = detail;
    }
    /// Upload what changed: the models once, the placements shown whenever the viewer is `MOVE_THRESHOLD` away from where they were chosen.
    pub fn upload(
        &mut self,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        eye: Vec3,
    ) -> Result<()> {
        let detail = self.detail;
        for layer in &mut self.layers {
            if detail.is_some_and(|d| layer.scene.detail > d) {
                layer.shown = 0;
                layer.chosen_at = None;
                continue;
            }
            if layer.gpu.is_none() {
                layer.gpu = Some(renderer.upload(device, queue, &layer.scene.scene)?);
                layer.chosen_at = None;
            }
            if layer
                .chosen_at
                .is_some_and(|at| at.distance(eye) < MOVE_THRESHOLD)
            {
                continue;
            }
            let transforms = choose(&layer.scene, eye);
            let needed = transforms.len().div_ceil(CHUNK);
            while layer.chunks.len() < needed {
                layer.chunks.push(GpuInstances::new(device, CHUNK)?);
            }
            for (n, chunk) in layer.chunks.iter_mut().enumerate() {
                let part = transforms.get(n * CHUNK..).unwrap_or(&[]);
                chunk.update(queue, &part[..part.len().min(CHUNK)])?;
            }
            layer.shown = needed;
            layer.chosen_at = Some(eye);
        }
        Ok(())
    }
    pub fn draws(&self) -> Vec<(&GpuScene, &GpuInstances)> {
        let mut out = Vec::new();
        for layer in &self.layers {
            if let Some(gpu) = &layer.gpu {
                out.extend(layer.chunks.iter().take(layer.shown).map(|c| (gpu, c)));
            }
        }
        out
    }
    pub fn gpu_stopped(&mut self) {
        for layer in &mut self.layers {
            layer.gpu = None;
            layer.chunks.clear();
            layer.shown = 0;
            layer.chosen_at = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::scatter::Instance;

    fn at(x: f32, tint: [u8; 4]) -> Instance {
        Instance {
            position: [x, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            tint,
        }
    }

    fn layer(list: Vec<Instance>, fade: Option<[f32; 2]>, order: Order, modulate_2x: bool) -> ScatterScene {
        ScatterScene {
            name: "test".into(),
            scene: Default::default(),
            instances: list.into(),
            fade,
            order,
            modulate_2x,
            detail: 0,
        }
    }

    #[test]
    fn placements_past_the_far_distance_are_left_out_and_those_between_fade() {
        let list = vec![
            at(5.0, [255; 4]),
            at(20.0, [255, 128, 0, 200]),
            at(31.0, [255; 4]),
        ];
        let chosen = choose(&layer(list, Some([10.0, 30.0]), Order::Unsorted, false), Vec3::ZERO);
        assert_eq!(chosen.len(), 2, "the one past 30 is gone");
        assert_eq!(chosen[0].tint, [1.0; 4]);
        assert!((chosen[1].tint[3] - 200.0 / 255.0 * 0.5).abs() < 1e-5, "half faded");
        assert!((chosen[1].tint[1] - 128.0 / 255.0).abs() < 1e-5);
    }

    #[test]
    fn a_layer_without_a_fade_draws_everything() {
        let list = vec![at(1.0e6, [255; 4]), at(-1.0e6, [255; 4])];
        assert_eq!(choose(&layer(list, None, Order::Unsorted, false), Vec3::ZERO).len(), 2);
    }

    #[test]
    fn instances_are_ordered_by_distance_when_the_layer_says_so() {
        let list = vec![at(30.0, [1; 4]), at(10.0, [2; 4]), at(20.0, [3; 4])];
        let tints = |order| {
            choose(&layer(list.clone(), None, order, false), Vec3::ZERO)
                .iter()
                .map(|t| (t.tint[0] * 255.0).round() as u8)
                .collect::<Vec<_>>()
        };
        assert_eq!(tints(Order::Unsorted), [1, 2, 3]);
        assert_eq!(tints(Order::FarToNear), [1, 3, 2]);
        assert_eq!(tints(Order::NearToFar), [2, 3, 1]);
    }

    #[test]
    fn a_layer_that_modulates_2x_doubles_its_tint_but_not_its_alpha() {
        let chosen = choose(&layer(vec![at(1.0, [128, 64, 255, 100])], None, Order::Unsorted, true), Vec3::ZERO);
        let t = chosen[0].tint;
        assert!((t[0] - 128.0 / 255.0 * 2.0).abs() < 1e-5 && (t[2] - 2.0).abs() < 1e-5);
        assert!((t[3] - 100.0 / 255.0).abs() < 1e-5);
    }
}
