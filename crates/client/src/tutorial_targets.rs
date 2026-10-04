//! The Tutorial's target practice targets (`launchTarget`): v20's target
//! models from the tutorial pack, drawn where the server's launches put
//! them. A target's motion follows from its launch, so it is placed at the
//! presented server tick every frame and glides between updates.
use anyhow::{Context, Result, ensure};
use bri_content::tutorial::{PackIndex, TARGET_SHAPES, TARGET_SKINS};
use bri_render::scene::{
    GpuInstances, GpuScene, SceneData, SceneImage, SceneRenderer, SceneTransform,
};
use bri_sim::tutorial::TargetView;
use glam::Mat4;
use std::{collections::BTreeMap, path::Path};

const MAX_TEXTURE_BYTES: u64 = 16 << 20;

struct Model {
    data: SceneData,
    gpu: Option<GpuScene>,
    instances: Option<GpuInstances>,
    transforms: Vec<SceneTransform>,
}

/// One model per target shape and skin.
#[derive(Default)]
pub struct TutorialTargets {
    models: BTreeMap<(&'static str, &'static str), Model>,
}

impl TutorialTargets {
    /// Load the target models of a tutorial pack.
    pub fn load(dir: &Path) -> Result<Self> {
        let root = dir.canonicalize()?;
        let index = bri_sim::tutorial::load_index(&root)?;
        let mut images: BTreeMap<String, SceneImage> = BTreeMap::new();
        let mut image = |name: &str| -> Result<SceneImage> {
            if let Some(image) = images.get(name) {
                return Ok(image.clone());
            }
            let loaded = texture(&root, &index, name)?;
            images.insert(name.to_string(), loaded.clone());
            Ok(loaded)
        };
        let mut models = BTreeMap::new();
        for id in TARGET_SHAPES {
            let shape = bri_sim::tutorial::load_target_shape(&root, &index, id)?;
            let pose = bri_content::animation::sample(&shape, None, 0.0)?;
            let skinnable = shape
                .materials
                .iter()
                .any(|m| m.name.to_ascii_lowercase().starts_with("base."));
            let skins: &[&'static str] = if skinnable { &TARGET_SKINS } else { &["base"] };
            for skin in skins {
                // `setSkinName`: `base.` materials take the skin's prefix.
                let bound = shape
                    .materials
                    .iter()
                    .map(|m| {
                        let name = m.name.to_ascii_lowercase();
                        match name.strip_prefix("base.") {
                            Some(rest) => image(&format!("{skin}.{rest}")),
                            None => image(&name),
                        }
                    })
                    .collect::<Result<Vec<_>>>()?;
                let refs: Vec<&SceneImage> = bound.iter().collect();
                let data = crate::items::native_shape_scene(
                    id,
                    &shape,
                    &refs,
                    [1.0; 4],
                    false,
                    Mat4::IDENTITY,
                    &pose,
                )?;
                models.insert(
                    (id, *skin),
                    Model {
                        data,
                        gpu: None,
                        instances: None,
                        transforms: Vec::new(),
                    },
                );
            }
        }
        Ok(Self { models })
    }

    /// Place this frame's targets at the presented server `tick`.
    pub fn update(&mut self, targets: &[TargetView], tick: f64) {
        for model in self.models.values_mut() {
            model.transforms.clear();
        }
        for target in targets {
            if target.gone(tick) {
                continue;
            }
            let skin = if target.marked { target.skin() } else { "base" };
            if let Some(model) = self.models.get_mut(&(target.shape(), skin)) {
                model.transforms.push(SceneTransform {
                    transform: target.transform(tick),
                    tint: [1.0; 4],
                });
            }
        }
    }

    pub fn count(&self) -> usize {
        self.models.values().map(|m| m.transforms.len()).sum()
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

/// A target texture by the name its material gives, checked against the
/// SHA-256 its file is named by.
fn texture(root: &Path, index: &PackIndex, name: &str) -> Result<SceneImage> {
    let file = index
        .textures
        .get(name)
        .with_context(|| format!("Tutorial pack lacks target texture {name}"))?;
    let (sha, _) = file.split_once('.').context("Unnamed target texture")?;
    let bytes = crate::items::checked_read(root, file, sha, MAX_TEXTURE_BYTES)?;
    let image = image::load_from_memory(&bytes)
        .with_context(|| format!("Decoding target texture {name}"))?
        .to_rgba8();
    ensure!(
        image.width() > 0 && image.height() > 0,
        "Empty target texture {name}"
    );
    Ok(SceneImage {
        label: format!("tutorial/{name}"),
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
        srgb: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::tutorial::TargetLaunch;

    /// A tutorial pack's targets: made up (`crate::testing::tutorial`), or
    /// the generated v20 pack.
    struct Pack {
        dir: std::path::PathBuf,
        _scratch: Option<crate::testing::ScratchDir>,
    }
    impl Pack {
        fn synthetic() -> Result<Self> {
            let scratch = crate::testing::ScratchDir::new("tutorial")?;
            crate::testing::tutorial::write_targets(scratch.path())?;
            Ok(Self {
                dir: scratch.path().to_path_buf(),
                _scratch: Some(scratch),
            })
        }
        fn content() -> Result<Self> {
            Ok(Self {
                dir: bri_package::testing::pack_dir(
                    &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
                    "tutorial",
                ),
                _scratch: None,
            })
        }
    }
    crate::testing::synthetic_and_content!(Pack: every_target_look_has_a_model_and_moves_along_its_lane);
    fn every_target_look_has_a_model_and_moves_along_its_lane(fx: &Pack) -> Result<()> {
        let mut targets = TutorialTargets::load(&fx.dir)?;
        // Plain and hit targets, and the marked one in each of its skins.
        assert_eq!(targets.models.len(), 2 + 2 * TARGET_SKINS.len());
        let launch = |kind: &str| TargetLaunch {
            at_ms: 0,
            row: 1,
            speed: 1,
            kind: kind.into(),
        };
        let mut shot = TargetView::launch(1, &launch(""), 0);
        shot.hit = true;
        let views = [
            TargetView::launch(0, &launch("m4"), 0),
            shot,
            TargetView::launch(2, &launch(""), 0),
        ];
        targets.update(&views, 120.0);
        assert_eq!(targets.count(), 3);
        let marked = &targets.models[&(bri_content::tutorial::TARGET_M_SHAPE, "m3")];
        let x = marked.transforms[0].transform.w_axis.x;
        assert!(
            (x - (bri_sim::tutorial::TARGET_START_X + 2.0)).abs() < 1e-4,
            "{x}"
        );
        assert_eq!(
            targets.models[&(bri_content::tutorial::TARGET_HIT_SHAPE, "base")]
                .transforms
                .len(),
            1
        );
        // Past the end of the range it is gone.
        targets.update(&views, 120.0 * 60.0);
        assert_eq!(targets.count(), 0);
        Ok(())
    }
}
