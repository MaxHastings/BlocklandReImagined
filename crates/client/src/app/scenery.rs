//! The Environment window's Sky over the map's own: what its picker offers
//! (the map bundle's skies, read once on a worker, with their pictures), and
//! the map scene redrawn when the host's settings pick another. Nothing here
//! touches collision or the host: a sky this game lacks leaves the map's own.
use super::*;
use bri_render::{environment_choices::Choices as Offered, scene::SceneImage};
use bri_ui::models::environment::{Choice, Choices};

/// UI textures of the picker's pictures, one per sky.
const PICTURE_BASE: u64 = 0x454E_0000;

type Loaded = (Arc<Offered>, Vec<(u64, SceneImage)>);

#[derive(Default)]
pub(super) struct Scenery {
    loading: Option<mpsc::Receiver<Result<Loaded>>>,
    /// The bundle could not be read: not asked again this run.
    failed: bool,
    offered: Option<Arc<Offered>>,
    /// The picker's pictures, and whether the UI renderer has them.
    pictures: Vec<(u64, SceneImage)>,
    uploaded: bool,
    /// The sky the map scene draws over the map's own (None: its own),
    /// since it was loaded.
    drawn_sky: Option<String>,
    /// The map the picker last showed.
    shown: Option<String>,
}
impl Scenery {
    /// A map scene was loaded (a new map, or its lighting): it draws its
    /// own sky.
    pub(super) fn reset(&mut self) {
        self.drawn_sky = None;
    }
    /// The UI renderer went with the device: its pictures go up again.
    pub(super) fn gpu_stopped(&mut self) {
        self.uploaded = false;
    }
}

impl App {
    /// Once a frame before drawing: read what the picker offers, upload its
    /// pictures, tell the Environment window, and redraw the sky the host
    /// picked.
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
                        "The Environment window's skies are unavailable: {error:#}"
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
        if scenery.shown.as_ref() != Some(&map) {
            scenery.shown = Some(map.clone());
            self.ui
                .core
                .environment
                .set_choices(choices(&offered, &map));
        }
        let sky = self
            .net
            .attempt
            .as_ref()
            .and_then(|a| a.view.as_ref())
            .and_then(|v| v.environment.sky.clone());
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
    }
}

/// The bundle's skies, with their pictures as UI textures.
fn load(root: &Path) -> Result<Loaded> {
    let offered = Offered::load(root)?;
    let mut pictures = Vec::new();
    for (i, sky) in offered.skies.iter().enumerate() {
        match offered.sky_thumbnail(sky) {
            Ok(image) => pictures.push((PICTURE_BASE + i as u64, image)),
            Err(error) => bri_console::warn(format!("Sky picture {}: {error:#}", sky.id)),
        }
    }
    Ok((Arc::new(offered), pictures))
}

/// What the Environment window's picker shows on `map`.
fn choices(offered: &Offered, map: &str) -> Choices {
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
        map_sky: offered.map_sky(map),
    }
}
