//! Isolated modern source lighting and compatibility light-volume preparation.
use super::*;

/// Map lighting, reflections and the environment probe.
pub(super) struct Lighting {
    pub(super) light_volume: LightVolumeState,
    /// Mirror surfaces and their reflections, for the world pass's format.
    pub(super) reflections: Option<bri_render::reflection::Reflections>,
    /// The cube metal surfaces reflect, drawn around the nearest one.
    pub(super) environment_probe: Option<bri_render::environment_probe::EnvironmentProbe>,
}

/// Compatibility preparation runs only for a compatibility scene: Classic's
/// light volume and Unified's recovered lights, visibility, residual and
/// switchable per-texel shares. Dynamic loads an offline descriptor sidecar,
/// without starting either bake. A mode switch lazily prepares its own scene.
pub(super) enum Baked {
    Volume(bri_render::light_volume::LightVolume),
    /// The Unified bake, including its legacy switchable-light shares.
    Map(Box<bri_render::map_lighting::MapLighting>),
}
#[derive(Default)]
pub(super) struct LightVolumeState {
    /// Behind a mutex so a prepared map (which carries it) stays `Sync`.
    pub(super) baking: Option<std::sync::Mutex<LightVolumeReceiver>>,
    pub(super) volume: Option<bri_render::light_volume::LightVolume>,
    pub(super) map: Option<bri_render::map_lighting::MapLighting>,
    /// The bake's lightmap leak cleanup, until the map's lightmaps take it.
    pub(super) leaks: Vec<bri_render::map_lighting::TexelFix>,
    /// The bake's per-texel lightmaps (leftover light and each light's
    /// share), for the map's images once a mode needs them.
    pub(super) switchable_sheets: Vec<bri_render::map_lighting::DynamicSheet>,
    /// Prepared source descriptors; no recovery runs in the client.
    pub(super) recovered: Vec<bri_render::map_lighting::MapLight>,
    pub(super) source_modern: bool,
    source_loading: Option<(bool, std::sync::Mutex<LightingSourceReceiver>)>,
    source_failed: Option<bool>,
    /// The map's images hold the per-texel lightmaps (the scene uploaded
    /// again with them).
    pub(super) switchable_equipped: bool,
    pub(super) uploaded: bool,
    /// The lighting mode the bound volumes serve.
    pub(super) bound_mode: u8,
    /// The map's breakable light shapes (scene node, centre): a broken bulb
    /// switches its lights off.
    pub(super) light_shapes: Vec<(u32, Vec3)>,
}
type LightingSourceReceiver =
    std::sync::mpsc::Receiver<std::result::Result<bri_render::scene_loader::MapScene, String>>;
/// Each recovered light's run-time tint: what the Add-On rules give it (1
/// as the map was lit), scaled by the share of its owning light shapes still
/// whole, so it goes dark when all of them break and half when one of two
/// does. Rules cannot light a broken shape again.
pub(super) fn map_light_tints(
    lights: &[bri_render::map_lighting::MapLight],
    light_shapes: &[(u32, Vec3)],
    broken: &BTreeSet<u32>,
    rules: &[bri_sim::session::MapLightRule],
) -> Vec<Vec3> {
    lights
        .iter()
        .zip(bri_render::map_lighting::fixture_owners(
            lights,
            light_shapes,
        ))
        .map(|(light, owners)| {
            let tint = bri_sim::session::MapLightRule::tint_at(rules, Vec3::from(light.position));
            let whole = owners
                .iter()
                .filter(|&&(node, _)| !broken.contains(&node))
                .count();
            if owners.is_empty() {
                tint
            } else {
                tint * (whole as f32 / owners.len() as f32)
            }
        })
        .collect()
}
/// Stores `bytes` as `file`, through a partial file. A lost write only
/// means baking again next time.
pub(super) fn store_bake(cache: &std::path::Path, file: &std::path::Path, bytes: Vec<u8>) {
    let partial = file.with_extension("partial");
    let _ = std::fs::create_dir_all(cache)
        .and_then(|_| std::fs::write(&partial, bytes))
        .and_then(|_| std::fs::rename(&partial, file));
}
impl LightVolumeState {
    /// Cells of at least 2 units, at most a million (4 MB): about 4.7 units
    /// across the whole Bedroom.
    const MIN_CELL: f32 = 2.0;
    const MAX_CELLS: usize = 1_000_000;
    /// Map light visibility: cells of at least 2 units, at most 2 million
    /// (16 MB, two RGBA layers per cell): 3.6 units across Bedroom. Finer
    /// grids cost frame time where many surfaces overlap on screen.
    const VIS_CELL: f32 = 2.0;
    const VIS_CELLS: usize = 2_000_000;
    pub(super) fn start(
        scene: &SceneData,
        cache: &std::path::Path,
        modern_lights: Option<&[bri_render::map_lighting::MapLight]>,
    ) -> Self {
        let source_modern = modern_lights.is_some();
        let mut state = if source_modern {
            Self::default()
        } else {
            Self::start_legacy(scene, cache)
        };
        state.source_modern = source_modern;
        state.recovered = modern_lights.unwrap_or_default().to_vec();
        state
    }
    /// Source-mode changes prepare off-thread; completed work is applied only
    /// if the user's latest selection still wants it. Gameplay is untouched.
    pub(super) fn poll_source(
        &mut self,
        root: &Path,
        id: &str,
        modern: bool,
    ) -> Option<bri_render::scene_loader::MapScene> {
        if let Some((loading, rx)) = &mut self.source_loading {
            let received = rx.get_mut().ok().map(|rx| rx.try_recv());
            match received {
                Some(Ok(Ok(scene))) => {
                    let applies = *loading == modern;
                    self.source_loading = None;
                    if applies {
                        return Some(scene);
                    }
                }
                Some(Ok(Err(error))) => {
                    eprintln!("Lighting mode preparation failed: {error}");
                    self.source_failed = Some(*loading);
                    self.source_loading = None;
                }
                Some(Err(std::sync::mpsc::TryRecvError::Empty)) => return None,
                _ => self.source_loading = None,
            }
        }
        if modern == self.source_modern {
            self.source_failed = None;
            return None;
        }
        if self.source_failed == Some(modern) {
            return None;
        }
        let root = root.to_owned();
        let id = id.to_owned();
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("lighting mode source".into())
            .spawn(move || {
                let result = super::load_visual_map(&root, &id, if modern { 3 } else { 2 })
                    .map_err(|e| format!("{e:#}"));
                let _ = tx.send(result);
            });
        if spawned.is_ok() {
            self.source_loading = Some((modern, std::sync::Mutex::new(rx)));
        } else {
            self.source_failed = Some(modern);
        }
        None
    }
    fn start_legacy(scene: &SceneData, cache: &std::path::Path) -> Self {
        let Some(baker) = bri_render::light_volume::Baker::new(scene) else {
            return Self::default();
        };
        let map = bri_render::map_lighting::Bake::new(scene);
        let (tx, rx) = std::sync::mpsc::channel();
        let cache = cache.to_owned();
        let spawned = std::thread::Builder::new()
            .name("light volume".into())
            .spawn(move || {
                let hex =
                    |key: [u8; 32]| key.iter().map(|b| format!("{b:02x}")).collect::<String>();
                let key = baker.key(Self::MIN_CELL, Self::MAX_CELLS);
                let file = cache.join(format!("{}.lightvolume", hex(key)));
                let stored = std::fs::read(&file)
                    .ok()
                    .and_then(|bytes| bri_render::light_volume::LightVolume::from_bytes(&bytes));
                match stored {
                    Some(volume) => {
                        let _ = tx.send(Baked::Volume(volume));
                    }
                    None => {
                        let volume = baker.bake(Self::MIN_CELL, Self::MAX_CELLS);
                        let bytes = volume.to_bytes();
                        let _ = tx.send(Baked::Volume(volume));
                        store_bake(&cache, &file, bytes);
                    }
                }
                let Some(map) = map else { return };
                let key = map.key();
                let file = cache.join(format!("{}.maplighting", hex(key)));
                let stored = std::fs::read(&file).ok().and_then(|bytes| {
                    bri_render::map_lighting::MapLighting::from_bytes(&bytes, key)
                });
                match stored {
                    Some(lighting) => {
                        let _ = tx.send(Baked::Map(Box::new(lighting)));
                    }
                    None => {
                        // Only compatibility modes ask for this bake. The
                        // residual for all recovered lights is obsolete in
                        // Dynamic, so do not bake that second volume.
                        let lighting = map.bake_compatibility(
                            Self::MIN_CELL,
                            Self::MAX_CELLS,
                            Self::VIS_CELL,
                            Self::VIS_CELLS,
                        );
                        let _ = tx.send(Baked::Map(Box::new(lighting.clone())));
                        store_bake(&cache, &file, lighting.to_bytes(key));
                    }
                }
            });
        Self {
            baking: spawned.ok().map(|_| std::sync::Mutex::new(rx)),
            ..Self::default()
        }
    }
    /// The map's light bulbs and tubes, whose breaking puts their lights out.
    pub(super) fn set_light_shapes(&mut self, breakables: &[bri_sim::map::Breakable]) {
        self.light_shapes = breakables
            .iter()
            .filter(|b| {
                LIGHT_SHAPES
                    .iter()
                    .any(|name| b.datablock.eq_ignore_ascii_case(name))
            })
            .map(|b| (b.node, b.center))
            .collect();
    }
    /// Broken bulbs and Add-On rules onto the bound map lights; uploads
    /// only when a tint changed.
    pub(super) fn tint(
        &self,
        renderer: &mut SceneRenderer,
        queue: &wgpu::Queue,
        broken: &BTreeSet<u32>,
        rules: &[bri_sim::session::MapLightRule],
    ) {
        let lights = if self.bound_mode == 3 {
            Some(self.recovered.as_slice())
        } else {
            self.map.as_ref().map(|m| m.lights.as_slice())
        };
        if let Some(lights) = lights {
            renderer.set_map_light_tints(
                queue,
                &map_light_tints(lights, &self.light_shapes, broken, rules),
            );
        }
    }
    /// The lighting mode frames can draw with now: a Unified mode needs the
    /// map bake when the map has interior lightmaps (their residual light
    /// replaces the classic volume).
    pub(super) fn mode(&self, requested: u8) -> u8 {
        // Dynamic uses current geometry and the live environment immediately;
        // recovered point parameters join when preparation completes.
        if self.source_modern {
            return 3;
        }
        if requested == 3 || requested == 0 || self.map.is_some() || self.baking.is_none() {
            requested
        } else {
            0
        }
    }

    pub(super) fn upload(
        &mut self,
        renderer: &mut SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        requested: u8,
    ) -> Result<()> {
        while let Some(rx) = self.baking.as_mut() {
            let received = match rx.get_mut() {
                Ok(rx) => rx.try_recv(),
                Err(_) => Err(std::sync::mpsc::TryRecvError::Disconnected),
            };
            match received {
                Ok(Baked::Volume(volume)) => {
                    self.volume = Some(volume);
                    self.uploaded = false;
                }
                Ok(Baked::Map(map)) => {
                    self.leaks = map.leaks.clone();
                    self.switchable_sheets = map.dynamic.clone();
                    self.map = Some(*map);
                    if requested != 3 {
                        self.uploaded = false;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.baking = None,
            }
        }
        let mode = self.mode(requested);
        if self.uploaded && self.bound_mode == mode {
            return Ok(());
        }
        if mode == 3 {
            renderer.set_dynamic_lights(device, queue, &self.recovered)?;
        } else {
            let unified = mode > 0;
            let map = self.map.as_ref().filter(|_| unified);
            let volume = match map {
                Some(map) => Some(&map.residual),
                None if unified => None,
                None => self.volume.as_ref(),
            };
            renderer.set_light_volume(device, queue, volume)?;
            renderer.set_map_lighting(device, queue, map, false)?;
        }
        self.uploaded = true;
        self.bound_mode = mode;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modern_scene() -> bri_render::scene_loader::MapScene {
        bri_render::scene_loader::MapScene {
            scene: SceneData::default(),
            terrain: Vec::new(),
            shape_indices: Default::default(),
            modern_lights: Some(Vec::new()),
        }
    }

    #[test]
    fn modern_state_has_no_compatibility_preparation() {
        let descriptor = bri_render::map_lighting::MapLight {
            position: [1.0, 2.0, 3.0],
            color: [0.5; 3],
            inner: 0.0,
            outer: 20.0,
            channel: None,
        };
        let scene = SceneData::default();
        let state = LightVolumeState::start(
            &scene,
            Path::new("unused"),
            Some(std::slice::from_ref(&descriptor)),
        );
        assert!(state.baking.is_none());
        assert!(state.volume.is_none() && state.map.is_none());
        assert!(state.leaks.is_empty() && state.switchable_sheets.is_empty());
        assert_eq!(state.recovered, vec![descriptor]);
        assert_eq!(state.mode(3), 3);
        // Until the compatibility source reload arrives, keep this scene in
        // modern shading; its placeholder illumination cannot enter Classic.
        assert_eq!(state.mode(0), 3);
    }

    #[test]
    fn latest_source_selection_discards_stale_completion() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Ok(modern_scene())).unwrap();
        let mut state = LightVolumeState {
            source_modern: false,
            source_loading: Some((true, std::sync::Mutex::new(rx))),
            ..Default::default()
        };
        assert!(
            state
                .poll_source(Path::new("unused"), "unused", false)
                .is_none()
        );
        assert!(state.source_loading.is_none());
        assert!(!state.source_modern);
    }
}
