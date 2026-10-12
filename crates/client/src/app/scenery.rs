//! The Environment window's Sky and Ground over the map's own: what its
//! pickers offer (the map bundle's skies and terrain textures, read once on
//! a worker, with their pictures), and the map scene and terrain redrawn
//! when the host's settings pick another. Nothing here touches collision or
//! the host: a sky or ground this game lacks leaves the map's own.
use super::*;
use bri_render::{
    environment_choices::Choices as Offered, scene::SceneImage, terrain_scene::TerrainScene,
};
use bri_ui::models::environment::{Choice, Choices};

/// UI textures of the pickers' pictures: skies first, then grounds.
const PICTURE_BASE: u64 = 0x454E_0000;

type Loaded = (Arc<Offered>, Vec<(u64, SceneImage)>);

#[derive(Default)]
pub(super) struct Scenery {
    loading: Option<mpsc::Receiver<Result<Loaded>>>,
    /// The bundle could not be read: not asked again this run.
    failed: bool,
    offered: Option<Arc<Offered>>,
    /// The pickers' pictures, and whether the UI renderer has them.
    pictures: Vec<(u64, SceneImage)>,
    uploaded: bool,
    /// What the map scene and terrain draw over the map's own (None: its
    /// own), since they were loaded.
    drawn_sky: Option<String>,
    drawn_ground: Option<String>,
    /// The map's own terrain, while a ground covers `cpu_terrain`.
    own_terrain: Vec<Arc<TerrainScene>>,
    /// The map and whether it has terrain, as the pickers last showed.
    shown: Option<(String, bool)>,
}
impl Scenery {
    /// A map scene was loaded (a new map, or its lighting): it draws its
    /// own sky and ground.
    pub(super) fn reset(&mut self, terrain: &[Arc<TerrainScene>]) {
        self.drawn_sky = None;
        self.drawn_ground = None;
        self.own_terrain = terrain.to_vec();
    }
    /// The UI renderer went with the device: its pictures go up again.
    pub(super) fn gpu_stopped(&mut self) {
        self.uploaded = false;
    }
}

impl App {
    /// Once a frame before drawing: read what the pickers offer, upload
    /// their pictures, tell the Environment window, and redraw the sky or
    /// ground the host picked.
    pub(super) fn update_scenery(&mut self, frame: &mut RenderContext<'_>) {
        let Some(map) = self.scene.cpu_scene.as_ref().map(|s| s.id.clone()) else {
            return;
        };
        let scenery = &mut self.scene.scenery;
        if scenery.offered.is_none() && scenery.loading.is_none() && !scenery.failed {
            let (tx, rx) = mpsc::channel();
            let root = self.content.paths.map_bundle.clone();
            self.runtime.spawn_blocking(move || {
                let _ = tx.send(load(&root));
            });
            scenery.loading = Some(rx);
        }
        if let Some(rx) = &scenery.loading {
            match rx.try_recv() {
                Ok(Ok((offered, pictures))) => {
                    scenery.offered = Some(offered);
                    scenery.pictures = pictures;
                    scenery.uploaded = false;
                    scenery.shown = None;
                    scenery.loading = None;
                }
                Ok(Err(error)) => {
                    bri_console::warn(format!(
                        "The Environment window's skies and grounds are unavailable: {error:#}"
                    ));
                    scenery.failed = true;
                    scenery.loading = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    scenery.failed = true;
                    scenery.loading = None;
                }
            }
        }
        if !scenery.uploaded {
            scenery.uploaded = true;
            for (id, image) in &scenery.pictures {
                crate::save_picture::upload_as(
                    frame,
                    *id,
                    &crate::save_picture::Picture {
                        width: image.width,
                        height: image.height,
                        rgba: image.rgba.clone(),
                    },
                );
            }
        }
        let Some(offered) = scenery.offered.clone() else {
            return;
        };
        let has_ground = !scenery.own_terrain.is_empty();
        if scenery.shown.as_ref() != Some(&(map.clone(), has_ground)) {
            scenery.shown = Some((map.clone(), has_ground));
            self.ui
                .core
                .environment
                .set_choices(choices(&offered, &map, has_ground));
        }
        let (sky, ground) = self
            .net
            .attempt
            .as_ref()
            .and_then(|a| a.view.as_ref())
            .map(|v| (v.environment.sky.clone(), v.environment.ground.clone()))
            .unwrap_or_default();
        if sky != self.scene.scenery.drawn_sky {
            self.scene.scenery.drawn_sky = sky.clone();
            if let Some(scene) = self.scene.cpu_scene.as_mut() {
                if let Err(error) = offered.set_sky(scene, &map, sky.as_deref()) {
                    bri_console::warn(format!("Drawing the picked sky: {error:#}"));
                }
                crate::frame_trace::note("the sky changed");
                // The map scene holds the sky; reflections and the
                // environment probe saw the old one.
                self.gpu.gpu_scene = None;
                self.lighting.reflections = None;
                self.lighting.environment_probe = None;
            }
        }
        if ground != self.scene.scenery.drawn_ground {
            self.scene.scenery.drawn_ground = ground.clone();
            let own = &self.scene.scenery.own_terrain;
            self.scene.cpu_terrain = own
                .iter()
                .map(|terrain| {
                    let Some(id) = ground.as_deref() else {
                        return terrain.clone();
                    };
                    match offered.grounded(terrain, id) {
                        Ok(Some(covered)) => Arc::new(covered),
                        Ok(None) => terrain.clone(),
                        Err(error) => {
                            bri_console::warn(format!("Drawing the picked ground: {error:#}"));
                            terrain.clone()
                        }
                    }
                })
                .collect();
            crate::frame_trace::note("the ground changed");
            self.gpu.gpu_terrain.clear();
        }
    }
}

/// The bundle's skies and grounds, with their pictures as UI textures.
fn load(root: &Path) -> Result<Loaded> {
    let offered = Offered::load(root)?;
    let mut pictures = Vec::new();
    for (i, sky) in offered.skies.iter().enumerate() {
        match offered.sky_thumbnail(sky) {
            Ok(image) => pictures.push((PICTURE_BASE + i as u64, image)),
            Err(error) => bri_console::warn(format!("Sky picture {}: {error:#}", sky.id)),
        }
    }
    for (i, ground) in offered.grounds.iter().enumerate() {
        let id = PICTURE_BASE + (offered.skies.len() + i) as u64;
        match offered.ground_thumbnail(ground) {
            Ok(image) => pictures.push((id, image)),
            Err(error) => bri_console::warn(format!("Ground picture {}: {error:#}", ground.id)),
        }
    }
    Ok((Arc::new(offered), pictures))
}

/// What the Environment window's pickers show on `map`.
fn choices(offered: &Offered, map: &str, has_ground: bool) -> Choices {
    let skies = offered.skies.len();
    Choices {
        skies: offered
            .skies
            .iter()
            .enumerate()
            .map(|(i, s)| Choice {
                id: s.id.clone(),
                label: s.label.clone(),
                icon: IconRef::External(PICTURE_BASE + i as u64),
            })
            .collect(),
        grounds: offered
            .grounds
            .iter()
            .enumerate()
            .map(|(i, g)| Choice {
                id: g.id.clone(),
                label: g.label.clone(),
                icon: IconRef::External(PICTURE_BASE + (skies + i) as u64),
            })
            .collect(),
        map_sky: offered.map_sky(map),
        has_ground,
    }
}
