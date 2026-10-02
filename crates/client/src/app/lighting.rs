//! Baked map lighting: the light volume and its cache.
use super::*;

/// Map lighting: the baked light volume, reflections and the environment probe.
pub(super) struct Lighting {
    pub(super) light_volume: LightVolumeState,
    /// Mirror surfaces and their reflections, for the world pass's format.
    pub(super) reflections: Option<bri_render::reflection::Reflections>,
    /// The cube metal surfaces reflect, drawn around the nearest one.
    pub(super) environment_probe: Option<bri_render::environment_probe::EnvironmentProbe>,
}

/// The map's baked lighting, started on its own thread as soon as the map's
/// scene is read, so it bakes while the rest of the map loads, and uploaded
/// once per renderer. Two bakes: the classic light volume
/// (`bri_render::light_volume`, for the Classic lighting mode) and the map's
/// recovered lights with their visibility and residual volumes
/// (`bri_render::map_lighting`, for the Unified modes). Each is stored under
/// the client state directory by its content key, so each map bakes once.
/// Until a bake arrives, the modes that need it draw as Classic.
pub(super) enum Baked {
    Volume(bri_render::light_volume::LightVolume),
    /// The map bake, and whether its Dynamic-mode residual volume is in it
    /// (else `ResidualAll` follows).
    Map(Box<bri_render::map_lighting::MapLighting>, bool),
    /// The Dynamic mode's residual volume, when it bakes after the rest.
    ResidualAll(bri_render::light_volume::LightVolume),
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
    pub(super) dynamic: Vec<bri_render::map_lighting::DynamicSheet>,
    /// The Dynamic mode's residual volume is baked (it can follow the rest
    /// of the map bake).
    pub(super) dynamic_ready: bool,
    /// The map's images hold the per-texel lightmaps (the scene uploaded
    /// again with them).
    pub(super) dynamic_equipped: bool,
    pub(super) uploaded: bool,
    /// The lighting mode the bound volumes serve.
    pub(super) bound_mode: u8,
    /// The map's breakable light shapes (scene node, centre): a broken bulb
    /// switches its lights off.
    pub(super) light_shapes: Vec<(u32, Vec3)>,
}
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
        .zip(bri_render::map_lighting::fixture_owners(lights, light_shapes))
        .map(|(light, owners)| {
            let tint = bri_sim::session::MapLightRule::tint_at(rules, Vec3::from(light.position));
            let whole = owners.iter().filter(|&&(node, _)| !broken.contains(&node)).count();
            if owners.is_empty() { tint } else { tint * (whole as f32 / owners.len() as f32) }
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
    pub(super) fn start(scene: &SceneData, cache: &std::path::Path) -> Self {
        let Some(baker) = bri_render::light_volume::Baker::new(scene) else {
            return Self::default();
        };
        let map = bri_render::map_lighting::Bake::new(scene);
        let (tx, rx) = std::sync::mpsc::channel();
        let cache = cache.to_owned();
        let spawned = std::thread::Builder::new()
            .name("light volume".into())
            .spawn(move || {
                let hex = |key: [u8; 32]| key.iter().map(|b| format!("{b:02x}")).collect::<String>();
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
                let stored = std::fs::read(&file)
                    .ok()
                    .and_then(|bytes| bri_render::map_lighting::MapLighting::from_bytes(&bytes, key));
                match stored {
                    Some(lighting) => {
                        let _ = tx.send(Baked::Map(Box::new(lighting), true));
                    }
                    None => {
                        // The other modes start without waiting for the
                        // Dynamic mode's own residual volume.
                        let (mut lighting, rest) =
                            map.bake_staged(Self::MIN_CELL, Self::MAX_CELLS, Self::VIS_CELL, Self::VIS_CELLS);
                        let _ = tx.send(Baked::Map(Box::new(lighting.clone()), rest.is_none()));
                        if let Some(rest) = rest {
                            lighting.residual_all = rest.bake(Self::MIN_CELL, Self::MAX_CELLS);
                            let _ = tx.send(Baked::ResidualAll(lighting.residual_all.clone()));
                        }
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
            .filter(|b| LIGHT_SHAPES.iter().any(|name| b.datablock.eq_ignore_ascii_case(name)))
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
        if let Some(map) = &self.map {
            renderer.set_map_light_tints(queue, &map_light_tints(&map.lights, &self.light_shapes, broken, rules));
        }
    }
    /// The lighting mode frames can draw with now: a Unified mode needs the
    /// map bake when the map has interior lightmaps (their residual light
    /// replaces the classic volume).
    pub(super) fn mode(&self, requested: u8) -> u8 {
        // Without interior lightmaps (an outdoor map) there is nothing to
        // wait for: Unified is the sun, its shadows and ambient. Dynamic
        // draws as Unified with highlights until its own residual volume
        // is baked and the map's images hold its lightmaps.
        if requested == 3 && self.map.is_some() && !(self.dynamic_ready && self.dynamic_equipped) {
            2
        } else if requested == 0 || self.map.is_some() || self.baking.is_none() {
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
                Ok(Baked::Map(map, dynamic_ready)) => {
                    self.leaks = map.leaks.clone();
                    self.dynamic = map.dynamic.clone();
                    self.map = Some(*map);
                    self.dynamic_ready = dynamic_ready;
                    self.uploaded = false;
                }
                Ok(Baked::ResidualAll(volume)) => {
                    if let Some(map) = &mut self.map {
                        map.residual_all = volume;
                        self.dynamic_ready = true;
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
        let unified = mode > 0;
        // Dynamic shades every recovered light live, so objects add the
        // residual without any of them.
        let dynamic = mode == 3;
        let map = self.map.as_ref().filter(|_| unified);
        let volume = match map {
            Some(map) if dynamic => Some(&map.residual_all),
            Some(map) => Some(&map.residual),
            None if unified => None,
            None => self.volume.as_ref(),
        };
        renderer.set_light_volume(device, queue, volume)?;
        renderer.set_map_lighting(device, queue, map, dynamic)?;
        self.uploaded = true;
        self.bound_mode = mode;
        Ok(())
    }
}
