//! The map's keyed movers (doors) drawn where the host's states put
//! them: each model is its own small scene about its origin, placed by an
//! instance transform every frame, like the Tutorial's targets.
use crate::audio::PositionalSource;
use anyhow::Result;
use bri_content::mover::MoverState;
use bri_render::scene::{GpuInstances, GpuScene, SceneData, SceneRenderer, SceneTransform};
use bri_sim::map::MapMover;

/// How near a mover's looped sound is at full volume, and how far it
/// carries, in world units: the range of an Add-On weapon's sounds.
const LOOP_RANGE: (f32, f32) = (10.0, 60.0);
/// Source keys for mover loops start here so they cannot meet an actor's.
const LOOP_KEY_BASE: u64 = 1 << 62;

/// A one-shot a mover's change of motion asks for, at the mover's origin.
pub struct MoverSound {
    pub profile: String,
    pub position: [f32; 3],
    pub volume: f32,
}

struct Model {
    mover: MapMover,
    data: SceneData,
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    transform: SceneTransform,
    /// Whether it was moving, and toward which key, at the last update;
    /// None until the first, which only learns and never sounds (a
    /// player joining while a door swings hears nothing of the start).
    last: Option<(bool, u8)>,
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
                    last: None,
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
    /// Returns the one-shot sounds the changes since the last call ask
    /// for.
    pub fn update(&mut self, states: &[MoverState], tick: f64) -> Vec<MoverSound> {
        let mut sounds = Vec::new();
        for (index, model) in self.models.iter_mut().enumerate() {
            let state = states.get(index).copied().unwrap_or_default();
            model.transform.transform = model.mover.transform_at(&state, tick);
            let moving = state.moving && state.alpha_at_f(tick) < 1.0;
            let now = (moving, state.to);
            let Some(before) = model.last.replace(now) else {
                continue;
            };
            let Some(set) = &model.mover.def.sounds else {
                continue;
            };
            let name = match (before, now) {
                // Began, or turned back toward another key mid-swing.
                ((was, to), (true, new_to)) if !was || to != new_to => set.start(new_to),
                ((true, _), (false, rest)) => set.rest(rest),
                _ => None,
            };
            if let Some(profile) = name {
                sounds.push(MoverSound {
                    profile: profile.to_string(),
                    position: model.transform.transform.w_axis.truncate().into(),
                    volume: set.volume,
                });
            }
        }
        sounds
    }
    /// The looped sounds of the movers in motion now.
    pub fn loops(&self) -> Vec<PositionalSource> {
        self.models
            .iter()
            .enumerate()
            .filter_map(|(index, model)| {
                let (true, _) = model.last? else { return None };
                let set = model.mover.def.sounds.as_ref()?;
                Some(PositionalSource {
                    key: LOOP_KEY_BASE + index as u64,
                    sound: set.moving.clone()?,
                    position: model.transform.transform.w_axis.truncate().into(),
                    volume: set.volume,
                    pitch: 1.0,
                    reference_distance: LOOP_RANGE.0,
                    max_distance: LOOP_RANGE.1,
                })
            })
            .collect()
    }
    /// Where the mover called `id` is drawn now.
    pub fn transform_by_id(&self, id: &str) -> Option<glam::Mat4> {
        self.models
            .iter()
            .find(|m| m.mover.def.id == id)
            .map(|m| m.transform.transform)
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

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::mover::{Encroach, Glide, MoverDef, MoverKey, MoverSounds};

    fn door() -> MapMover {
        let sounds = MoverSounds {
            opening: Some("door.open".into()),
            opened: Some("door.opened".into()),
            closing: Some("door.close".into()),
            closed: Some("door.shut".into()),
            moving: Some("door.creak".into()),
            volume: 1.0,
        };
        MapMover {
            node: 1,
            def: MoverDef {
                id: "door".into(),
                keys: vec![MoverKey::default(), MoverKey::default()],
                glide: Glide::Linear,
                encroach: Encroach::Stop,
                spin: None,
                collide: true,
                params: Default::default(),
                sounds: Some(sounds),
            },
            placement: glam::Mat4::IDENTITY,
            colliders: 0..0,
        }
    }
    fn with_door() -> ClientMovers {
        ClientMovers {
            models: vec![Model {
                mover: door(),
                data: Default::default(),
                gpu: None,
                instances: None,
                transform: SceneTransform {
                    transform: glam::Mat4::IDENTITY,
                    tint: [1.0; 4],
                },
                last: None,
            }],
        }
    }
    fn names(sounds: Vec<MoverSound>) -> Vec<String> {
        sounds.into_iter().map(|s| s.profile).collect()
    }

    #[test]
    fn a_door_sounds_as_it_starts_and_as_it_comes_to_rest() {
        let mut movers = with_door();
        let rest = MoverState::at_rest(0);
        let mut opening = MoverState::at_rest(0);
        opening.to = 1;
        opening.alpha = 0.0;
        opening.rate = 0.01;
        opening.moving = true;
        assert!(names(movers.update(&[rest], 0.0)).is_empty());
        assert_eq!(names(movers.update(&[opening], 1.0)), ["door.open"]);
        assert_eq!(movers.loops().len(), 1);
        assert!(names(movers.update(&[opening], 2.0)).is_empty());
        assert_eq!(
            names(movers.update(&[MoverState::at_rest(1)], 200.0)),
            ["door.opened"]
        );
        assert!(movers.loops().is_empty());
    }

    #[test]
    fn joining_while_a_door_swings_sounds_nothing_until_it_changes() {
        let mut movers = with_door();
        let mut swinging = MoverState::at_rest(0);
        swinging.to = 1;
        swinging.alpha = 0.2;
        swinging.rate = 0.01;
        swinging.moving = true;
        assert!(names(movers.update(&[swinging], 0.0)).is_empty());
        assert_eq!(
            names(movers.update(&[MoverState::at_rest(1)], 100.0)),
            ["door.opened"]
        );
    }
}
