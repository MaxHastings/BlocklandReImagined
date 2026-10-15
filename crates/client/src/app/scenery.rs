//! The Environment window's Sky, Ground and Water over the map's own: what
//! the pickers offer (Steam's environment pack and the map bundle's skies,
//! read once on a worker, with their pictures), and the map scene redrawn
//! when the host's settings pick another. A ground and a water go only on a
//! flat map's plate; the water is also swum in (the prediction mirror's
//! liquids, splashes, rain hitting it), as the host's is. A name this game
//! lacks leaves the map's own.
use super::*;
use bri_content::{atmosphere::Settings, scene::Plate, water::Water};
use bri_render::{
    environment_choices::Choices as Offered,
    environment_planes::{self, Planes},
    scene::{MaterialKind, MeshBatch, SceneImage},
};
use bri_ui::models::environment::{Choice, Choices};

/// UI textures of the pickers' pictures: skies, then grounds, then waters.
const PICTURE_BASE: u64 = 0x454E_0000;
const GROUND_PICTURES: u64 = PICTURE_BASE + 0x1000;
const WATER_PICTURES: u64 = PICTURE_BASE + 0x2000;

type Loaded = (Arc<Offered>, Vec<(u64, SceneImage)>);

#[derive(Default)]
pub(super) struct Scenery {
    loading: Option<mpsc::Receiver<Result<Loaded>>>,
    /// The content could not be read: not asked again this run.
    failed: bool,
    offered: Option<Arc<Offered>>,
    /// The pickers' pictures, and whether the UI renderer has them.
    pictures: Vec<(u64, SceneImage)>,
    uploaded: bool,
    /// The scenery settings the map scene and liquids show, since the map
    /// was loaded (all unset: as it loaded).
    drawn: Settings,
    /// The map scene's own water batches, kept while a picked water hides
    /// them.
    own_water: Option<Vec<MeshBatch>>,
    /// The plate the loaded map stands on, and its own liquids.
    plate: Option<Plate>,
    own_waters: Vec<Water>,
    /// The map the pickers last showed.
    shown: Option<String>,
}
impl Scenery {
    /// A map scene was loaded (a new map, or its lighting): it draws its
    /// own scenery.
    pub(super) fn reset(&mut self) {
        self.drawn = Settings::default();
        self.own_water = None;
    }
    /// A new map: its plate and its own liquids.
    pub(super) fn set_map(&mut self, plate: Option<Plate>, waters: Vec<Water>) {
        self.reset();
        self.plate = plate;
        self.own_waters = waters;
        self.shown = None;
    }
    /// The UI renderer went with the device: its pictures go up again.
    pub(super) fn gpu_stopped(&mut self) {
        self.uploaded = false;
    }
}

/// The settings that change what the scene draws or what is swum in.
fn scenery_of(s: &Settings) -> Settings {
    Settings {
        sky: s.sky.clone(),
        ground: s.ground.clone(),
        ground_color: s.ground_color,
        water: s.water.clone(),
        water_height: s.water_height,
        water_color: s.water_color,
        underwater_color: s.underwater_color,
        ..Default::default()
    }
}

impl App {
    /// Once a frame before drawing: read what the pickers offer, upload
    /// their pictures, tell the Environment window, and redraw the sky,
    /// ground and water the host picked.
    pub(super) fn update_scenery(&mut self, frame: &mut RenderContext<'_>) {
        let Some(map) = self.scene.cpu_scene.as_ref().map(|s| s.id.clone()) else {
            return;
        };
        let scenery = &mut self.scene.scenery;
        if scenery.offered.is_none() && scenery.loading.is_none() && !scenery.failed {
            let (tx, rx) = mpsc::channel();
            let root = self.content.paths.map_bundle.clone();
            let pack = self.content.paths.environments.clone();
            self.runtime.spawn_blocking(move || {
                let _ = tx.send(load(&root, pack.as_deref()));
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
                        "The Environment window's skies, grounds and waters are unavailable: \
                         {error:#}"
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
            let flat = scenery.plate.is_some();
            self.ui
                .core
                .environment
                .set_choices(choices(&offered, &map, flat));
        }
        let wanted = self
            .net
            .attempt
            .as_ref()
            .and_then(|a| a.view.as_ref())
            .map(|v| scenery_of(&v.environment))
            .unwrap_or_default();
        if wanted == self.scene.scenery.drawn {
            return;
        }
        let drawn = std::mem::replace(&mut self.scene.scenery.drawn, wanted.clone());
        let plate = self.scene.scenery.plate;
        let planes = Planes::of(&offered.pack, &wanted);
        // The water is swum in: the prediction mirror's liquids (and so
        // splashes and the underwater tint), and where rain stops.
        if plate.is_some()
            && (
                drawn.water,
                drawn.water_height,
                drawn.water_color,
                drawn.underwater_color,
            ) != (
                wanted.water.clone(),
                wanted.water_height,
                wanted.water_color,
                wanted.underwater_color,
            )
        {
            let waters =
                offered
                    .pack
                    .waters(plate.as_ref(), &wanted, &self.scene.scenery.own_waters);
            self.motion.set_map_waters(waters.clone());
            self.weather.set_waters(waters);
        }
        if drawn.sky != wanted.sky {
            self.set_sky_rain(&offered, wanted.sky.as_deref());
        }
        let Some(scene) = self.scene.cpu_scene.as_mut() else {
            return;
        };
        // A map Add-On's own sky lives in its own bundle.
        let map_root = self.content.paths.maps.root(&map).ok();
        if let Err(error) = offered.set_sky_for(
            scene,
            &map,
            map_root,
            wanted.sky.as_deref(),
            plate.is_some() && planes.ground_shows(),
        ) {
            bri_console::warn(format!("Drawing the picked sky: {error:#}"));
        }
        if let Some(plate) = &plate {
            let own_water = self.scene.scenery.own_water.get_or_insert_with(|| {
                scene
                    .batches
                    .iter()
                    .filter(|b| scene.materials[b.material].kind == MaterialKind::Water)
                    .cloned()
                    .collect()
            });
            environment_planes::strip(scene);
            let materials = &scene.materials;
            scene
                .batches
                .retain(|b| materials[b.material].kind != MaterialKind::Water);
            if planes.water.is_none() {
                scene.batches.extend(own_water.iter().cloned());
            }
            let picture = |image: &bri_content::environment::Image| match offered.picture(image) {
                Ok(picture) => Some(picture),
                Err(error) => {
                    bri_console::warn(format!("Environment picture: {error:#}"));
                    None
                }
            };
            let ground = planes.ground.and_then(|(g, _)| picture(&g.texture));
            let water = match planes.water {
                Some(Some((w, ..))) => picture(&w.top).zip(picture(&w.bottom)),
                _ => None,
            };
            environment_planes::append(scene, plate, &planes, ground, water);
        }
        crate::frame_trace::note("the scenery changed");
        // The map scene holds the sky and planes; reflections and the
        // environment probe saw the old ones.
        self.gpu.gpu_scene = None;
        self.lighting.reflections = None;
        self.lighting.environment_probe = None;
    }
}

impl App {
    /// The rain or snow a picked Steam sky brings, over the map's own.
    fn set_sky_rain(&mut self, offered: &Offered, sky: Option<&str>) {
        let texture = |image: &bri_content::environment::Image| {
            offered
                .picture(image)
                .map(|picture| bri_weather::WeatherTexture {
                    id: String::new(),
                    width: picture.width,
                    height: picture.height,
                    rgba: picture.rgba,
                })
        };
        let rain = sky
            .and_then(|id| offered.sky(id))
            .and_then(|choice| Some((choice.id.as_str(), choice.precipitation.as_ref()?)))
            .map(|(sky, p)| -> Result<crate::weather::SkyRain<'_>> {
                Ok(crate::weather::SkyRain {
                    sky,
                    precipitation: p,
                    drop: (&p.drop, texture(&p.drop)?),
                    splash: match &p.splash {
                        Some(splash) => Some((splash, texture(splash)?)),
                        None => None,
                    },
                })
            })
            .transpose();
        if let Err(error) = rain.and_then(|rain| self.weather.set_sky(rain)) {
            bri_console::warn(format!("The picked sky's rain or snow: {error:#}"));
        }
    }
}

/// The pickers' choices, with their pictures as UI textures.
fn load(root: &Path, pack: Option<&Path>) -> Result<Loaded> {
    let offered = Offered::load(root, pack)?;
    let mut pictures = Vec::new();
    for (i, sky) in offered.skies.iter().enumerate() {
        match offered.sky_thumbnail(sky) {
            Ok(image) => pictures.push((PICTURE_BASE + i as u64, image)),
            Err(error) => bri_console::warn(format!("Sky picture {}: {error:#}", sky.id)),
        }
    }
    let pack = &offered.pack;
    for (base, list) in [
        (
            GROUND_PICTURES,
            pack.grounds
                .iter()
                .map(|g| (&g.id, &g.thumbnail))
                .collect::<Vec<_>>(),
        ),
        (
            WATER_PICTURES,
            pack.waters.iter().map(|w| (&w.id, &w.thumbnail)).collect(),
        ),
    ] {
        for (i, (id, thumbnail)) in list.into_iter().enumerate() {
            match offered.pack_thumbnail(thumbnail) {
                Ok(image) => pictures.push((base + i as u64, image)),
                Err(error) => bri_console::warn(format!("Picture of {id}: {error:#}")),
            }
        }
    }
    Ok((Arc::new(offered), pictures))
}

/// What the Environment window's pickers show on `map`.
fn choices(offered: &Offered, map: &str, flat: bool) -> Choices {
    let pack = &offered.pack;
    let none = Choice {
        id: bri_content::atmosphere::NO_WATER.into(),
        label: "None".into(),
        icon: Default::default(),
        look: Settings::default(),
    };
    Choices {
        skies: offered
            .skies
            .iter()
            .enumerate()
            .map(|(i, s)| Choice {
                id: s.id.clone(),
                label: s.label.clone(),
                icon: IconRef::External(PICTURE_BASE + i as u64),
                look: s.look.clone(),
            })
            .collect(),
        grounds: pack
            .grounds
            .iter()
            .enumerate()
            .map(|(i, g)| Choice {
                id: g.id.clone(),
                label: g.label.clone(),
                icon: IconRef::External(GROUND_PICTURES + i as u64),
                look: Settings {
                    ground_color: Some(g.color),
                    ..Default::default()
                },
            })
            .collect(),
        waters: if pack.waters.is_empty() {
            Vec::new()
        } else {
            std::iter::once(none)
                .chain(pack.waters.iter().enumerate().map(|(i, w)| Choice {
                    id: w.id.clone(),
                    label: w.label.clone(),
                    icon: IconRef::External(WATER_PICTURES + i as u64),
                    look: Settings {
                        water_height: Some(w.height),
                        water_color: Some(w.color),
                        underwater_color: Some(w.overlay_color),
                        ..Default::default()
                    },
                }))
                .collect()
        },
        map_sky: offered.map_sky(map),
        flat,
    }
}
