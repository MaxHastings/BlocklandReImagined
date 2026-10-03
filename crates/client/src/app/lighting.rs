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
    /// One compatibility request per current map, retained through mode flips.
    compatibility_ticket: Option<CompatibilityTicket>,
    compatibility_source: Option<Arc<SceneData>>,
    compatibility_started: bool,
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
type LightingSource = (bri_render::scene_loader::MapScene, Option<Arc<SceneData>>);
type LightingSourceReceiver =
    std::sync::mpsc::Receiver<std::result::Result<LightingSource, String>>;

/// Dropping a map cancels its work before the next expensive stage. Source-mode
/// changes retain this ticket. A running stage is not interrupted, but there is
/// only one process-wide worker and one pending input, including across maps.
struct CompatibilityTicket(Arc<std::sync::atomic::AtomicBool>);
impl Drop for CompatibilityTicket {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
impl CompatibilityTicket {
    fn cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}
struct CompatibilityWork {
    source: Arc<SceneData>,
    cache: PathBuf,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    tx: std::sync::mpsc::Sender<Baked>,
}
impl CompatibilityWork {
    fn cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn send(&self, baked: Baked) -> bool {
        !self.cancelled() && self.tx.send(baked).is_ok()
    }
}
#[derive(Default)]
struct CompatibilityQueue {
    pending: std::sync::Mutex<Option<CompatibilityWork>>,
    ready: std::sync::Condvar,
}
impl CompatibilityQueue {
    fn enqueue(&self, work: CompatibilityWork) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(previous) = pending.replace(work) {
            previous
                .cancelled
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.ready.notify_one();
    }
    fn next(&self) -> CompatibilityWork {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(work) = pending.take() {
                return work;
            }
            pending = self.ready.wait(pending).unwrap_or_else(|e| e.into_inner());
        }
    }
    fn worker() -> Option<&'static Arc<Self>> {
        static QUEUE: std::sync::OnceLock<Option<Arc<CompatibilityQueue>>> =
            std::sync::OnceLock::new();
        QUEUE
            .get_or_init(|| {
                let queue = Arc::new(Self::default());
                let worker = queue.clone();
                match std::thread::Builder::new()
                    .name("compatibility lighting".into())
                    .spawn(move || {
                        loop {
                            LightVolumeState::bake_compatibility(worker.next());
                        }
                    }) {
                    Ok(_) => Some(queue),
                    Err(error) => {
                        eprintln!("Compatibility lighting worker failed: {error}");
                        None
                    }
                }
            })
            .as_ref()
    }
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
        let mut state = Self::default();
        // start is called by the map preparation worker, not the render thread.
        let compatibility_source = modern_lights.is_none().then(|| Arc::new(scene.clone()));
        state.change_source(compatibility_source, cache, modern_lights);
        state
    }
    /// Keep the compatibility job/results owned by this map while replacing
    /// only its render source. A fresh legacy scene needs its patches reapplied.
    pub(super) fn change_source(
        &mut self,
        compatibility_source: Option<Arc<SceneData>>,
        cache: &Path,
        modern_lights: Option<&[bri_render::map_lighting::MapLight]>,
    ) {
        self.source_modern = modern_lights.is_some();
        self.recovered = modern_lights.unwrap_or_default().to_vec();
        if let Some(source) = compatibility_source {
            self.compatibility_source = Some(source);
        }
        self.source_loading = None;
        self.source_failed = None;
        self.switchable_equipped = false;
        self.uploaded = false;
        self.leaks = if self.source_modern {
            Vec::new()
        } else {
            self.map
                .as_ref()
                .map_or_else(Vec::new, |map| map.leaks.clone())
        };
        if !self.source_modern {
            self.ensure_compatibility(cache);
        }
    }
    /// A newer map can supersede a queued request. Only the accepted legacy
    /// source retries it; Dynamic never creates or resubmits compatibility work.
    pub(super) fn ensure_compatibility(&mut self, cache: &Path) {
        if self.source_modern
            || (self.compatibility_started
                && self
                    .compatibility_ticket
                    .as_ref()
                    .is_none_or(|ticket| !ticket.cancelled()))
        {
            return;
        }
        self.compatibility_started = true;
        let Some(source) = self.compatibility_source.as_ref() else {
            return;
        };
        let Some(queue) = CompatibilityQueue::worker() else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        queue.enqueue(CompatibilityWork {
            source: source.clone(),
            cache: cache.to_owned(),
            cancelled: cancelled.clone(),
            tx,
        });
        self.compatibility_ticket = Some(CompatibilityTicket(cancelled));
        self.baking = Some(std::sync::Mutex::new(rx));
    }
    /// Source-mode changes prepare off-thread; completed work is applied only
    /// if the user's latest selection still wants it. Gameplay is untouched.
    pub(super) fn poll_source(
        &mut self,
        root: &Path,
        id: &str,
        modern: bool,
    ) -> Option<LightingSource> {
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
        let prepare_compatibility_source = !modern && !self.compatibility_started;
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("lighting mode source".into())
            .spawn(move || {
                let result = super::load_visual_map(&root, &id, if modern { 3 } else { 2 })
                    .map(|scene| {
                        // Clone only on this source-loading thread. BVH and
                        // texel construction run later on the bounded worker.
                        let input =
                            prepare_compatibility_source.then(|| Arc::new(scene.scene.clone()));
                        (scene, input)
                    })
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
    fn bake_compatibility(work: CompatibilityWork) {
        if work.cancelled() {
            return;
        }
        let Some(baker) = bri_render::light_volume::Baker::new(work.source.as_ref()) else {
            return;
        };
        if work.cancelled() {
            return;
        }
        let hex = |key: [u8; 32]| key.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let key = baker.key(Self::MIN_CELL, Self::MAX_CELLS);
        let file = work.cache.join(format!("{}.lightvolume", hex(key)));
        let stored = std::fs::read(&file)
            .ok()
            .and_then(|bytes| bri_render::light_volume::LightVolume::from_bytes(&bytes));
        let (volume, bytes) = match stored {
            Some(volume) => (volume, None),
            None => {
                let volume = baker.bake(Self::MIN_CELL, Self::MAX_CELLS);
                if work.cancelled() {
                    return;
                }
                let bytes = volume.to_bytes();
                (volume, Some(bytes))
            }
        };
        // An abandoned map cannot begin map-light fitting after this stage.
        if !work.send(Baked::Volume(volume)) {
            return;
        }
        if let Some(bytes) = bytes {
            store_bake(&work.cache, &file, bytes);
        }
        if work.cancelled() {
            return;
        }
        let Some(map) = bri_render::map_lighting::Bake::new(work.source.as_ref()) else {
            return;
        };
        if work.cancelled() {
            return;
        }
        let key = map.key();
        let file = work.cache.join(format!("{}.maplighting", hex(key)));
        let stored = std::fs::read(&file)
            .ok()
            .and_then(|bytes| bri_render::map_lighting::MapLighting::from_bytes(&bytes, key));
        let (lighting, bytes) = match stored {
            Some(lighting) => (lighting, None),
            None => {
                let lighting = map.bake_compatibility(
                    Self::MIN_CELL,
                    Self::MAX_CELLS,
                    Self::VIS_CELL,
                    Self::VIS_CELLS,
                );
                if work.cancelled() {
                    return;
                }
                let bytes = lighting.to_bytes(key);
                (lighting, Some(bytes))
            }
        };
        if !work.send(Baked::Map(Box::new(lighting))) {
            return;
        }
        if let Some(bytes) = bytes {
            store_bake(&work.cache, &file, bytes);
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

    fn poll_compatibility(&mut self) {
        // Queued supersession is retried by ensure_compatibility; preserve its
        // source until that retry rather than treating it as a completed bake.
        if self.source_modern
            || self
                .compatibility_ticket
                .as_ref()
                .is_some_and(CompatibilityTicket::cancelled)
        {
            return;
        }
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
                    self.uploaded = false;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.baking = None;
                    self.compatibility_ticket = None;
                    self.compatibility_source = None;
                }
            }
        }
    }

    pub(super) fn upload(
        &mut self,
        renderer: &mut SceneRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        requested: u8,
    ) -> Result<()> {
        if requested != 3 && !self.source_modern {
            self.poll_compatibility();
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
        tx.send(Ok((modern_scene(), None))).unwrap();
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
    fn volume() -> bri_render::light_volume::LightVolume {
        bri_render::light_volume::LightVolume {
            origin: [0.0; 3],
            cell: 1.0,
            dims: [1; 3],
            texels: vec![[3, 4, 5, 255]],
            rays: 0,
        }
    }

    #[test]
    fn mode_flips_keep_one_compatibility_ticket_and_queued_result() {
        let (tx, rx) = std::sync::mpsc::channel();
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut state = LightVolumeState {
            baking: Some(std::sync::Mutex::new(rx)),
            compatibility_ticket: Some(CompatibilityTicket(cancelled.clone())),
            compatibility_source: Some(Arc::new(SceneData::default())),
            compatibility_started: true,
            light_shapes: vec![(7, Vec3::ZERO)],
            ..Default::default()
        };
        tx.send(Baked::Volume(volume())).unwrap();
        for _ in 0..100 {
            state.change_source(None, Path::new("unused"), Some(&[]));
            state.uploaded = true;
            state.poll_compatibility();
            assert!(
                state.volume.is_none(),
                "Dynamic must not consume legacy completion"
            );
            assert!(
                state.uploaded,
                "Legacy completion cannot invalidate Dynamic bindings"
            );
            state.change_source(None, Path::new("unused"), None);
            assert!(state.baking.is_some());
            assert!(Arc::ptr_eq(
                &state.compatibility_ticket.as_ref().unwrap().0,
                &cancelled
            ));
            assert!(!cancelled.load(std::sync::atomic::Ordering::Relaxed));
            assert_eq!(state.light_shapes, vec![(7, Vec3::ZERO)]);
        }
        state.poll_compatibility();
        assert_eq!(state.volume, Some(volume()));
        drop(tx);
        state.poll_compatibility();
        assert!(state.baking.is_none() && state.compatibility_source.is_none());
        state.change_source(None, Path::new("unused"), Some(&[]));
        state.change_source(None, Path::new("unused"), None);
        assert!(
            state.baking.is_none(),
            "Completed compatibility results must be reused"
        );
        assert_eq!(state.volume, Some(volume()));
    }

    #[test]
    fn compatibility_queue_has_only_the_latest_pending_input() {
        let queue = CompatibilityQueue::default();
        let mut tickets = Vec::new();
        let mut sources = Vec::new();
        let mut receivers = Vec::new();
        for i in 0..100 {
            let source = Arc::new(SceneData {
                id: format!("queued-{i}"),
                ..Default::default()
            });
            sources.push(Arc::downgrade(&source));
            let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let (tx, rx) = std::sync::mpsc::channel();
            queue.enqueue(CompatibilityWork {
                source,
                cache: PathBuf::new(),
                cancelled: cancelled.clone(),
                tx,
            });
            tickets.push(CompatibilityTicket(cancelled));
            receivers.push(rx);
        }
        assert!(tickets[..99].iter().all(CompatibilityTicket::cancelled));
        assert!(
            sources[..99]
                .iter()
                .all(|source| source.upgrade().is_none())
        );
        assert!(receivers[..99].iter().all(|rx| matches!(
            rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Disconnected)
        )));
        let active = queue.next();
        assert_eq!(active.source.id, "queued-99");
        assert!(!active.cancelled());
        assert!(queue.pending.lock().unwrap().is_none());
        // Dropping the active map ticket prevents delivery and entry to the
        // next expensive stage, without starting any constructor in this test.
        drop(tickets);
        assert!(active.cancelled());
        assert!(!active.send(Baked::Volume(volume())));
        assert!(matches!(
            receivers[99].try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
    }

    #[test]
    fn failed_compatibility_source_keeps_modern_lights_and_shadow_policy() {
        let descriptor = bri_render::map_lighting::MapLight {
            position: [1.0, 2.0, 3.0],
            color: [0.5; 3],
            inner: 0.0,
            outer: 20.0,
            channel: None,
        };
        for requested in [0, 2] {
            let mut state = LightVolumeState::start(
                &SceneData::default(),
                Path::new("unused"),
                Some(std::slice::from_ref(&descriptor)),
            );
            let (tx, rx) = std::sync::mpsc::channel();
            tx.send(Err("missing compatibility illumination".into()))
                .unwrap();
            state.source_loading = Some((false, std::sync::Mutex::new(rx)));
            for _ in 0..20 {
                assert!(
                    state
                        .poll_source(Path::new("unused"), "unused", false)
                        .is_none()
                );
                state.ensure_compatibility(Path::new("unused"));
                let mut settings = bri_ui::api::Settings::default();
                settings
                    .prefs
                    .insert(crate::graphics::LIGHTING.into(), requested.to_string());
                let intent = crate::graphics::Graphics::from_settings(&settings);
                let effective = intent.with_lighting(state.mode(requested));
                assert_eq!(intent.lighting, requested);
                assert_eq!(effective.lighting, 3);
                assert!(effective.shadows.unwrap().light_cubes);
                assert!(effective.brick_shadows || effective.lighting == 3);
                assert_ne!(effective.lighting, 0, "current map geometry still casts");
                assert_eq!(effective.lighting, 3, "current terrain still casts");
                assert_eq!(state.recovered, vec![descriptor]);
                assert!(state.baking.is_none() && state.source_loading.is_none());
            }
        }
    }
    #[test]
    fn returning_to_legacy_reapplies_retained_cleanup_without_rebaking() {
        use bri_render::map_lighting::{
            DynamicSheet, FitReport, MapLighting, TexelFix, VisibilityVolume,
        };
        let fix = TexelFix {
            image: 0,
            index: 0,
            rgba: [11, 22, 33, 255],
        };
        let sheets = vec![DynamicSheet {
            parts_image: 1,
            width: 1,
            height: 1,
            left: vec![13, 13, 13, 0],
            lights: Vec::new(),
            visibility: Vec::new(),
        }];
        let mut state = LightVolumeState {
            compatibility_started: true,
            map: Some(MapLighting {
                lights: Vec::new(),
                report: FitReport::default(),
                visibility: VisibilityVolume {
                    origin: [0.0; 3],
                    cell: 1.0,
                    dims: [1; 3],
                    texels: vec![[0; 8]],
                },
                residual: volume(),
                residual_all: volume(),
                leaks: vec![fix],
                dynamic: sheets.clone(),
            }),
            switchable_sheets: sheets.clone(),
            switchable_equipped: true,
            ..Default::default()
        };
        state.change_source(None, Path::new("unused"), Some(&[]));
        assert!(state.leaks.is_empty());
        state.change_source(None, Path::new("unused"), None);
        let mut images = vec![bri_render::scene::SceneImage::white()];
        bri_render::map_lighting::TexelFix::apply(&state.leaks, &mut images);
        assert_eq!(images[0].rgba, fix.rgba);
        assert_eq!(state.switchable_sheets, sheets);
        assert!(
            !state.switchable_equipped,
            "a fresh legacy source must equip its retained shares"
        );
        assert!(
            state.baking.is_none(),
            "the completed map did not start another job"
        );
    }
}
