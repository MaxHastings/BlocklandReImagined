//! The map's keyed movers (doors) drawn where the host's states put
//! them: each model is its own small scene about its origin, placed by an
//! instance transform every frame, like the Tutorial's targets.
use anyhow::Result;
use bri_content::mover::MoverState;
use bri_render::scene::{GpuInstances, GpuScene, SceneData, SceneRenderer, SceneTransform};
use bri_sim::map::MapMover;

struct Model {
    mover: MapMover,
    data: SceneData,
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    transform: SceneTransform,
}

/// Every mover of the current map.
#[derive(Default)]
pub struct ClientMovers {
    models: Vec<Model>,
}

impl ClientMovers {
    /// The new map's movers and their scenes (`MapScene::movers`).
    pub fn set_map(&mut self, movers: Vec<MapMover>, scenes: Vec<(u32, SceneData)>) {
        self.models = movers
            .into_iter()
            .filter_map(|mover| {
                let data = scenes
                    .iter()
                    .find(|(node, _)| *node == mover.node)?
                    .1
                    .clone();
                let transform = SceneTransform {
                    transform: mover.placement,
                    tint: [1.0; 4],
                };
                Some(Model {
                    mover,
                    data,
                    gpu: None,
                    instances: None,
                    transform,
                })
            })
            .collect();
    }
    /// Swap the scenes for ones loaded again (a lighting mode change),
    /// keeping the movers.
    pub fn replace_scenes(&mut self, scenes: Vec<(u32, SceneData)>) {
        for model in &mut self.models {
            if let Some((_, data)) = scenes.iter().find(|(node, _)| *node == model.mover.node) {
                model.data = data.clone();
                model.gpu = None;
                model.instances = None;
            }
        }
    }
    pub fn count(&self) -> usize {
        self.models.len()
    }
    /// Place every mover as `states` (map order) have it at the presented
    /// server `tick`.
    pub fn update(&mut self, states: &[MoverState], tick: f64) {
        for (index, model) in self.models.iter_mut().enumerate() {
            let state = states.get(index).copied().unwrap_or_default();
            model.transform.transform = model.mover.transform_at(&state, tick);
        }
    }
    /// Where the mover at scene node `node` is drawn now.
    pub fn transform_of(&self, node: u32) -> Option<glam::Mat4> {
        self.models
            .iter()
            .find(|m| m.mover.node == node)
            .map(|m| m.transform.transform)
    }
    pub fn upload(
        &mut self,
        renderer: &SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<()> {
        for model in &mut self.models {
            if model.gpu.is_none() {
                model.gpu = Some(renderer.upload(device, queue, &model.data)?);
            }
            if model.instances.is_none() {
                model.instances = Some(GpuInstances::new(device, 1)?);
            }
            if let Some(instances) = &mut model.instances {
                instances.update(queue, std::slice::from_ref(&model.transform))?;
            }
        }
        Ok(())
    }
    pub fn draws(&self) -> Vec<(&GpuScene, &GpuInstances)> {
        self.models
            .iter()
            .filter_map(|m| Some((m.gpu.as_ref()?, m.instances.as_ref()?)))
            .collect()
    }
    pub fn gpu_stopped(&mut self) {
        for model in &mut self.models {
            model.gpu = None;
            model.instances = None;
        }
    }
}
