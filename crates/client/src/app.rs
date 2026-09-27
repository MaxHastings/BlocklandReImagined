//! UI → native application → asynchronous authoritative server integration.
use crate::{
    content::{ClientContent, LOADABLE_MAPS},
    controls::Controls,
    network::{self, Connected, Worker},
    platform::{PlatformApp, PlatformCommand, RenderContext},
    settings,
};
use anyhow::{Context, Result, ensure};
use bri_net::{
    client::Client,
    content_identity,
    server::{self, ServerOptions},
};
use bri_render::{
    scene::{Camera, GpuScene, SceneData, SceneRenderer, create_depth},
    scene_loader::load_map_bundle,
};
use bri_sim::{
    definitions::Definitions,
    player::PlayerTuning,
    session::{Command, InspectMode, Reply, Session, ToolAction},
};
use bri_ui::{
    api::*,
    binds::Platform,
    screens::ScreenId,
    ui::{Ui, UiConfig},
};
use glam::Vec3;
use std::{
    collections::{BTreeMap, VecDeque},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::Duration,
};

type Meshes = BTreeMap<String, bri_content::brick::Brick>;
/// World camera far plane; also the farthest terrain tiles are ever drawn.
const FAR_PLANE: f32 = 4000.0;
struct Prepared {
    foliage: crate::foliage::PreparedFoliage,
    map_id: String,
    waters: Vec<bri_content::water::Water>,
    scene: SceneData,
    terrain: Vec<Arc<bri_render::terrain_scene::TerrainScene>>,
    meshes: Arc<Meshes>,
    materials: Arc<crate::materials::BrickMaterials>,
    building: crate::building::Building,
    mirror: bri_sim::prediction::CollisionMirror,
}
type WorldRender = (
    Arc<bri_net::protocol::PublicWorld>,
    std::result::Result<SceneData, String>,
);
struct WorldJob {
    receiver: mpsc::Receiver<WorldRender>,
    abort: tokio::task::AbortHandle,
}
impl Drop for WorldJob {
    fn drop(&mut self) {
        self.abort.abort();
    }
}
struct Attempt {
    id: RequestId,
    worker: Worker,
    scene: mpsc::Receiver<Prepared>,
    name: String,
    max_players: u32,
    local: bool,
    single: bool,
    ready: bool,
    entered: bool,
    view: Option<network::View>,
    last_chat: u64,
}
struct PendingAction {
    action: UiAction,
    command: Option<Command>,
    dialog_epoch: u64,
    inspection: Option<InspectMode>,
    dialog_request: bool,
}
/// Request ID for unsolicited state reports; their replies are not awaited.
const REPORT_REQUEST: RequestId = RequestId::MAX;

pub struct App {
    /// Movement the server's map rules currently allow (the Tutorial's lessons).
    abilities: bri_sim::session::Abilities,
    /// Last brick inventory state reported to the server.
    brick_hand: Option<bri_sim::session::BrickHand>,
    pub(crate) item_assets: Arc<crate::items::ItemAssets>,
    item_ui: crate::item_ui::ItemUi,
    world_items: crate::world_items::WorldItems,
    foliage: crate::foliage::ClientFoliage,
    weather: crate::weather::ClientWeather,
    weather_renderer: Option<bri_weather::gpu::WeatherRenderer>,
    audio: crate::audio::ClientAudio,
    pub ui: Ui,
    pub content: ClientContent,
    pub controls: Controls,
    state_dir: PathBuf,
    runtime: tokio::runtime::Runtime,
    attempt: Option<Attempt>,
    cpu_scene: Option<SceneData>,
    cpu_terrain: Vec<Arc<bri_render::terrain_scene::TerrainScene>>,
    renderer: Option<SceneRenderer>,
    effects: crate::effects::WorldEffects,
    weapon_effects: crate::weapon_effects::WeaponEffects,
    actor_effects: crate::actor_effects::ActorEffects,
    explosion_shapes: crate::explosion_shapes::ExplosionShapes,
    weapon_cues: VecDeque<(bri_sim::presentation::Cue, f32)>,
    weapon_cue_drops: u64,
    weapon_light_deferred: usize,
    weapon_effect_session: Option<RequestId>,
    weapon_animation_cues: VecDeque<(bri_sim::presentation::Cue, f32, f64)>,
    weapon_animation_drops: u64,
    weapon_animation_cursor: u64,
    effects_renderer: Option<bri_fx_runtime::gpu::EffectsRenderer>,
    gpu_scene: Option<GpuScene>,
    gpu_terrain: Vec<bri_render::terrain_scene::GpuTerrain>,
    depth: Option<(wgpu::Texture, (u32, u32))>,
    meshes: Option<Arc<Meshes>>,
    cpu_world: Option<SceneData>,
    gpu_world: Option<GpuScene>,
    world_source: Option<Arc<bri_net::protocol::PublicWorld>>,
    world_job: Option<WorldJob>,
    load_limit: Arc<tokio::sync::Semaphore>,
    materials: Option<Arc<crate::materials::BrickMaterials>>,
    building: Option<crate::building::Building>,
    pending_actions: BTreeMap<RequestId, PendingAction>,
    tool_ui: crate::tool_ui::ToolUi,
    dialog_epoch: u64,
    query_source: Option<Arc<bri_net::protocol::PublicWorld>>,
    ghost_gpu: Option<GpuScene>,
    ghost_uploaded: u64,
    avatar_assets: Arc<crate::avatar::AvatarAssets>,
    avatars: BTreeMap<bri_world::OwnerId, crate::avatar::AvatarMesh>,
    avatar_actions: BTreeMap<u64, crate::avatar::ActionAnimation>,
    avatar_action_images: BTreeMap<u64, String>,
    animation_time: f64,
    avatar_preview: Option<crate::avatar::Preview>,
    preview_request: Option<(bri_content::avatar::Appearance, [f32; 3], f32)>,
    preview_dirty: bool,
    motion: crate::motion::Motion,
    vehicle_assets: crate::vehicles::VehicleAssets,
    vehicles: crate::vehicles::ClientVehicles,
    music_world: Option<Arc<bri_net::protocol::PublicWorld>>,
    net_graph: Option<(std::time::Instant, u32)>,
    /// LAN listings from the last discovery query: address -> certificate.
    lan_hosts: BTreeMap<String, Vec<u8>>,
    lan_query: Option<mpsc::Receiver<Vec<(SocketAddr, bri_net::discovery::Beacon)>>>,
    macro_recording: Option<Vec<UiAction>>,
    build_macro: Vec<UiAction>,
    macro_playback: VecDeque<UiAction>,
    combat: CombatPresentation,
    saves: crate::saves::Store,
    file_jobs: crate::saves::Jobs,
}
impl App {
    fn queue_weapon_cue(&mut self, cue: bri_sim::presentation::Cue) {
        if matches!(
            cue.kind,
            bri_sim::presentation::CueKind::WeaponAnimation { .. }
        ) && cue.id > self.weapon_animation_cursor
        {
            self.weapon_animation_cursor = cue.id;
            if self.weapon_animation_cues.len() < bri_sim::presentation::MAX_CUES {
                self.weapon_animation_cues
                    .push_back((cue.clone(), 0., self.animation_time));
            } else {
                self.weapon_animation_drops = self.weapon_animation_drops.saturating_add(1);
            }
        }
        if let bri_sim::presentation::CueKind::Emote { actor, name } = &cue.kind
            && name == "sit"
        {
            self.combat.sitting.insert(*actor);
        }
        self.audio.cue(&cue);
        self.actor_effects.cue(&cue);
        self.explosion_shapes.cue(&cue);
        if matches!(
            cue.kind,
            bri_sim::presentation::CueKind::WeaponEffect { .. }
                | bri_sim::presentation::CueKind::WeaponShell { .. }
                | bri_sim::presentation::CueKind::WeaponAnimation { .. }
        ) {
            if self.weapon_cues.len() < bri_sim::presentation::MAX_CUES {
                self.weapon_cues.push_back((cue, 0.));
            } else {
                self.weapon_cue_drops = self.weapon_cue_drops.saturating_add(1);
            }
        }
    }
    /// Head images, jets and vehicle fire follow this frame's presented bodies.
    #[allow(clippy::too_many_arguments)]
    fn update_actor_effects(
        actor_effects: &mut crate::actor_effects::ActorEffects,
        assets: &crate::avatar::AvatarAssets,
        avatars: &BTreeMap<bri_world::OwnerId, crate::avatar::AvatarMesh>,
        vehicles: &crate::vehicles::ClientVehicles,
        vehicle_assets: &crate::vehicles::VehicleAssets,
        view: &network::View,
        presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
        elapsed: f32,
    ) -> Result<()> {
        let body = |id: u64| {
            vehicles
                .frame(id)
                .map(|f| glam::Mat4::from_rotation_translation(f.rotation, f.position))
        };
        let jets: Vec<_> = presented
            .iter()
            .filter(|(owner, player)| {
                player.jetting
                    && view
                        .vitals
                        .get(owner)
                        .is_some_and(|v| v.alive && v.mounted.is_none())
            })
            .filter_map(|(owner, player)| {
                let avatar = avatars.get(owner)?;
                let feet = [
                    avatar.world_node(assets, "RFoot")?,
                    avatar.world_node(assets, "LFoot")?,
                ];
                Some((*owner, feet, Vec3::from(player.velocity)))
            })
            .collect();
        let burning: Vec<_> = view
            .vehicles
            .values()
            .filter(|info| info.destroyed)
            .filter_map(|info| Some((info.id, body(info.id)?)))
            .collect();
        let pose = |anchor| match anchor {
            crate::actor_effects::Anchor::Actor { actor, mount } => avatars
                .get(&actor)?
                .world_node(assets, &format!("Mount{mount}")),
            crate::actor_effects::Anchor::Vehicle { vehicle } => body(vehicle),
            crate::actor_effects::Anchor::Muzzle { vehicle } => {
                let info = view.vehicles.get(&vehicle)?;
                let weapon = vehicle_assets.definition(&info.definition)?.weapon.as_ref()?;
                let frame = vehicles.frame(vehicle)?;
                Some(crate::actor_effects::muzzle(
                    frame.position,
                    frame.rotation,
                    frame.turret_aim,
                    weapon,
                ))
            }
        };
        actor_effects.advance(elapsed, pose, &jets, &burning)
    }
    fn reset_weapon_effect_session(&mut self, session: RequestId, checkpoint_cursor: u64) {
        if self.weapon_effect_session == Some(session) {
            return;
        }
        self.weapon_effects.reset(checkpoint_cursor);
        self.actor_effects.reset(checkpoint_cursor);
        self.explosion_shapes.reset(checkpoint_cursor);
        self.weapon_cues
            .retain(|(cue, _)| cue.id > checkpoint_cursor);
        self.weapon_animation_cues
            .retain(|(cue, _, _)| cue.id > checkpoint_cursor);
        self.weapon_animation_cursor = checkpoint_cursor;
        self.weapon_effect_session = Some(session);
    }
    #[cfg(test)]
    fn update_weapon_effects(
        &mut self,
        view: &bri_sim::session::WeaponView,
        elapsed: f32,
    ) -> Result<()> {
        Self::update_weapon_effect_parts(
            &mut self.weapon_effects,
            &mut self.weapon_cues,
            &self.world_items,
            view,
            elapsed,
        )
    }
    fn update_weapon_effect_parts(
        weapon_effects: &mut crate::weapon_effects::WeaponEffects,
        weapon_cues: &mut VecDeque<(bri_sim::presentation::Cue, f32)>,
        world_items: &crate::world_items::WorldItems,
        view: &bri_sim::session::WeaponView,
        elapsed: f32,
    ) -> Result<()> {
        weapon_effects.sync(view)?;
        let elapsed = elapsed.min(0.25);
        for (_, age) in weapon_cues.iter_mut() {
            *age += elapsed;
        }
        let mut ready = Vec::new();
        while let Some((cue, age)) = weapon_cues.front() {
            let needs_pose = matches!(
                &cue.kind,
                bri_sim::presentation::CueKind::WeaponEffect {
                    image,
                    node,
                    seconds,
                    ..
                } if image.is_some() || !node.is_empty() || *seconds > 0.
            );
            if needs_pose && world_items.effect_pose(cue).is_none() && *age < 0.5 {
                break;
            }
            ready.push(weapon_cues.pop_front().unwrap().0);
        }
        weapon_effects.cues(&ready, |cue| world_items.effect_pose(cue))?;
        // queue_weapon_cue already transfers these same reliable IDs into App's
        // bounded avatar queue (including its own observable overflow policy).
        // Do not retain a second copy indefinitely in the effects adapter.
        drop(weapon_effects.take_avatar_animation_requests());
        weapon_effects.advance(elapsed, Vec3::ZERO, |cue| world_items.effect_pose(cue))?;
        Ok(())
    }
    fn update_avatar_animation_inputs(
        avatar_actions: &mut BTreeMap<u64, crate::avatar::ActionAnimation>,
        avatar_action_images: &mut BTreeMap<u64, String>,
        weapon_animation_cues: &mut VecDeque<(bri_sim::presentation::Cue, f32, f64)>,
        weapon_animation_drops: &mut u64,
        view: &network::View,
        elapsed: f32,
    ) {
        avatar_actions.retain(|owner, _| view.poses.contains_key(owner));
        avatar_action_images.retain(|owner, _| view.poses.contains_key(owner));
        let identity = |owner: &u64| -> Option<String> {
            let mut parts = Vec::new();
            if let Some(images) = view.weapons.images.get(owner) {
                let mut images: Vec<_> = images.iter().collect();
                images.sort_by_key(|image| image.hand);
                parts.extend(
                    images
                        .iter()
                        .map(|image| format!("{}:{}", image.hand, image.image)),
                );
            }
            if let Some(item) = view
                .tools
                .get(owner)
                .and_then(|inventory| inventory.selected.and_then(|i| inventory.slots.get(i)))
                .and_then(Option::as_deref)
                .filter(|id| bri_weapons::CORE_TOOLS.contains(id))
            {
                parts.push(format!("tool:{item}"));
            }
            (!parts.is_empty()).then(|| parts.join("|"))
        };
        for owner in view.poses.keys() {
            let current = identity(owner);
            if avatar_action_images
                .get(owner)
                .is_some_and(|old| current.as_ref() != Some(old))
            {
                avatar_actions.remove(owner);
                avatar_action_images.remove(owner);
            }
            if current.is_none() {
                avatar_actions.remove(owner);
                avatar_action_images.remove(owner);
            }
        }
        for (_, age, _) in weapon_animation_cues.iter_mut() {
            *age += elapsed.min(0.25);
        }
        while let Some((cue, age, started_at)) = weapon_animation_cues.pop_front() {
            let bri_sim::presentation::CueKind::WeaponAnimation {
                actor,
                thread,
                sequence,
                image_hand,
            } = &cue.kind
            else {
                continue;
            };
            if *thread != 2 || sequence.eq_ignore_ascii_case("root") {
                if *thread == 2 {
                    avatar_actions.remove(actor);
                    avatar_action_images.remove(actor);
                }
                continue;
            }
            let current = identity(actor);
            let hand_matches = image_hand.is_none_or(|hand| {
                view.weapons
                    .images
                    .get(actor)
                    .is_some_and(|images| images.iter().any(|image| image.hand == hand))
                    || (hand == 0
                        && view.tools.get(actor).is_some_and(|inventory| {
                            inventory
                                .selected
                                .and_then(|i| inventory.slots.get(i))
                                .and_then(Option::as_deref)
                                .is_some_and(|id| bri_weapons::CORE_TOOLS.contains(&id))
                        }))
            });
            if current.is_none() || !hand_matches {
                if age >= 0.5 {
                    *weapon_animation_drops = weapon_animation_drops.saturating_add(1);
                    continue;
                }
                weapon_animation_cues.push_front((cue, age, started_at));
                break;
            }
            let action = crate::avatar::ActionAnimation {
                sequence: sequence.clone(),
                started_at,
            };
            avatar_actions.insert(*actor, action);
            avatar_action_images.insert(*actor, current.unwrap());
        }
    }
    pub fn item_assets(&self) -> &Arc<crate::items::ItemAssets> {
        &self.item_assets
    }
    pub fn world_item_stats(&self) -> &crate::world_items::WorldItemDiagnostics {
        &self.world_items.diagnostics
    }
    pub fn foliage_stats(&self) -> &bri_foliage::RenderStats {
        &self.foliage.stats
    }
    pub fn foliage_placement(&self) -> (&[bri_foliage::PlacementStats], f64) {
        (
            &self.foliage.prepared.placement,
            self.foliage.prepared.elapsed_ms,
        )
    }
    pub fn weather_counts(&self) -> (usize, usize) {
        (
            self.weather.world.drop_count(),
            self.weather.world.splash_count(),
        )
    }
    pub fn weather_diagnostics(&self) -> bri_weather::WeatherDiagnostics {
        self.weather.world.diagnostics()
    }
    pub fn audio_stats(&self) -> bri_audio::AudioStats {
        self.audio.stats()
    }
    pub fn audio_requests(&self) -> &BTreeMap<String, u64> {
        &self.audio.requested
    }
    pub fn audio_warnings(&self) -> &std::collections::BTreeSet<String> {
        &self.audio.warnings
    }
    pub fn effect_counts(&self) -> (usize, usize, usize, usize) {
        (
            self.effects.attachment_count(),
            self.effects.world.source_count(),
            self.effects.world.particle_count(),
            self.effects.deferred,
        )
    }
    pub fn weapon_effect_diagnostics(&self) -> &crate::weapon_effects::Diagnostics {
        &self.weapon_effects.diagnostics
    }
    pub fn weapon_effect_backlog(&self) -> (usize, u64, usize) {
        (
            self.weapon_cues.len(),
            self.weapon_cue_drops,
            self.weapon_light_deferred,
        )
    }
    pub fn avatar_scene(&self, owner: bri_world::OwnerId) -> Option<&SceneData> {
        self.avatars.get(&owner).map(|avatar| &avatar.data)
    }
    pub fn building(&self) -> Option<&crate::building::Building> {
        self.building.as_ref()
    }
    pub fn pending_requests(&self) -> usize {
        self.pending_actions.len() + self.file_jobs.len()
    }
    /// True when the CPU render snapshot has caught up with the latest replica.
    pub fn world_render_ready(&self) -> bool {
        self.network_view().is_some_and(|view| {
            self.world_source
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, &view.world))
        })
    }
    /// Presented (predicted and interpolated) local state and camera eye.
    pub fn local_motion(&self) -> Option<(bri_sim::player::PlayerState, Option<Vec3>)> {
        let view = self.network_view()?;
        let state = self.motion.presented().get(&view.owner)?.clone();
        Some((state, self.motion.local_eye()))
    }
    pub fn network_view(&self) -> Option<&network::View> {
        self.attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| a.view.as_ref())
    }
    pub fn load(content_root: &Path, state_dir: &Path, size: (u32, u32)) -> Result<Self> {
        Self::load_with_audio(content_root, state_dir, size, bri_audio::OutputKind::Null)
    }
    pub fn load_with_audio(
        content_root: &Path,
        state_dir: &Path,
        size: (u32, u32),
        output: bri_audio::OutputKind,
    ) -> Result<Self> {
        // Resolve once so state (including identity and host administration)
        // cannot silently switch when the process working directory changes.
        let absolute_state_dir = std::path::absolute(state_dir)?;
        let state_dir = absolute_state_dir.as_path();
        let content = ClientContent::load(content_root)?;
        let foliage = crate::foliage::ClientFoliage::load(&content.paths.foliage)?;
        let effects_pack = bri_fx_runtime::EffectsPack::load(&content.paths.effects_runtime)?;
        let effects = crate::effects::WorldEffects::new(effects_pack.clone(), Default::default())?;
        let weapon_pack = Arc::new(content.weapons.pack.clone());
        let explosion_shapes =
            crate::explosion_shapes::ExplosionShapes::load(&weapon_pack, &content.paths.weapons)?;
        let actor_effects = crate::actor_effects::ActorEffects::new(
            effects_pack.clone(),
            weapon_pack.clone(),
            Default::default(),
        )?;
        let weapon_effects = crate::weapon_effects::WeaponEffects::new(
            effects_pack,
            weapon_pack,
            Default::default(),
        )?;
        let material_path = content.paths.brick_materials.join("brick-materials.json");
        ensure!(
            std::fs::metadata(&material_path)?.len() <= 8 * 1024 * 1024,
            "Oversized material manifest"
        );
        let material_bundle: bri_content::brick_materials::Bundle =
            serde_json::from_slice(&std::fs::read(material_path)?)?;
        material_bundle.validate()?;
        let mut tool_ui = crate::tool_ui::ToolUi::new(
            &content.catalog,
            &content.effects,
            &material_bundle,
            &content.ui_pack,
        )?;
        tool_ui.install_items(content.weapons.item_choices.clone())?;
        tool_ui.install_special(
            content.music.clone(),
            content
                .vehicles
                .definitions
                .iter()
                .filter(|d| {
                    !matches!(
                        d.family,
                        bri_vehicles::Family::Skis
                            | bri_vehicles::Family::Tumble
                            | bri_vehicles::Family::Turret
                    )
                })
                .map(|d| (d.id.clone(), d.name.trim().to_string()))
                .chain(bri_sim::session::Session::bot_choices())
                .collect(),
        )?;
        tool_ui.install_events(
            content.events.clone(),
            content.event_sounds.clone(),
            content
                .weapons
                .pack
                .projectiles
                .iter()
                .map(|(id, p)| (id.clone(), p.name.clone()))
                .collect(),
        );
        let item_assets = Arc::new(crate::items::ItemAssets::load(
            &content.paths.item_presentation,
            &content.paths.weapons,
        )?);
        let item_ui = crate::item_ui::ItemUi::new(
            &item_assets,
            &content.weapons.item_choices,
            &content.ui_pack,
        )?;
        let avatar_assets = Arc::new(crate::avatar::AvatarAssets::load(&content.paths.avatar)?);
        let vehicle_assets = crate::vehicles::VehicleAssets::load(&content.paths.vehicles)?;
        let world_items = crate::world_items::WorldItems::new(
            item_assets.clone(),
            Arc::new(content.weapons.pack.clone()),
            Default::default(),
        )?;
        let mut saved = settings::load(&state_dir.join("settings.json"))?;
        let weather = crate::weather::ClientWeather::load(&content.paths.weather, &mut saved)?;
        let audio = crate::audio::ClientAudio::load(&content.paths.audio, &mut saved, output)?;
        let platform = if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Windows
        };
        let mut ui = Ui::new(
            content.ui_pack.clone(),
            UiConfig {
                size,
                scale: None,
                platform,
            },
            saved,
        );
        ui.apply(UiUpdate::Maps(content.maps.clone()));
        let backgrounds = content
            .ui_pack
            .data
            .images
            .keys()
            .filter(|key| key.to_ascii_lowercase().starts_with("screenshots/"))
            .cloned()
            .map(IconRef::Pack)
            .collect();
        ui.apply(UiUpdate::MainMenuBackgrounds(backgrounds));
        Ok(Self {
            item_assets,
            item_ui,
            world_items,
            foliage,
            weather,
            weather_renderer: None,
            audio,
            ui,
            saves: crate::saves::Store::new(state_dir, &content),
            file_jobs: Default::default(),
            content,
            controls: Controls::default(),
            state_dir: state_dir.into(),
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()?,
            attempt: None,
            abilities: Default::default(),
            brick_hand: None,
            cpu_scene: None,
            cpu_terrain: Vec::new(),
            renderer: None,
            effects,
            weapon_effects,
            actor_effects,
            explosion_shapes,
            weapon_cues: VecDeque::new(),
            weapon_cue_drops: 0,
            weapon_light_deferred: 0,
            weapon_effect_session: None,
            weapon_animation_cues: VecDeque::new(),
            weapon_animation_drops: 0,
            weapon_animation_cursor: 0,
            effects_renderer: None,
            gpu_scene: None,
            gpu_terrain: Vec::new(),
            depth: None,
            meshes: None,
            cpu_world: None,
            gpu_world: None,
            world_source: None,
            world_job: None,
            load_limit: Arc::new(tokio::sync::Semaphore::new(2)),
            materials: None,
            building: None,
            pending_actions: BTreeMap::new(),
            tool_ui,
            dialog_epoch: 0,
            query_source: None,
            ghost_gpu: None,
            ghost_uploaded: u64::MAX,
            avatar_assets,
            avatars: BTreeMap::new(),
            avatar_actions: BTreeMap::new(),
            avatar_action_images: BTreeMap::new(),
            animation_time: 0.0,
            avatar_preview: None,
            preview_request: None,
            preview_dirty: false,
            motion: Default::default(),
            vehicle_assets,
            vehicles: Default::default(),
            music_world: None,
            net_graph: None,
            lan_hosts: BTreeMap::new(),
            lan_query: None,
            macro_recording: None,
            build_macro: Vec::new(),
            macro_playback: VecDeque::new(),
            combat: Default::default(),
        })
    }
    fn answer(&mut self, id: RequestId, result: Result<()>) {
        self.ui.apply(UiUpdate::ActionResult {
            id,
            result: result.map_err(|e| format!("{e:#}")),
        });
    }
    fn handle_admin(
        &mut self,
        id: RequestId,
        action: bri_ui::models::admin::AdminAction,
    ) -> Result<()> {
        let attempt = self
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .context("Not connected")?;
        let snapshot = attempt
            .view
            .as_ref()
            .and_then(|v| v.admin_snapshot.as_ref())
            .context("Waiting for host administration state")?;
        if let Some(command) = crate::admin_ui::command(&action, snapshot)? {
            // Administrative operations do not carry a gameplay aim or a client-selected actor.
            attempt.worker.request(id, command)?;
            self.pending_actions.insert(
                id,
                PendingAction {
                    action: UiAction::Admin(action),
                    command: None,
                    dialog_epoch: self.dialog_epoch,
                    inspection: None,
                    dialog_request: false,
                },
            );
        } else {
            self.ui.apply_session(
                attempt.id,
                UiUpdate::Admin(bri_ui::models::admin::AdminUpdate::State(
                    crate::admin_ui::state(snapshot),
                )),
            );
            self.answer(id, Ok(()));
        }
        Ok(())
    }
    fn disconnect(&mut self) {
        self.abilities = Default::default();
        self.brick_hand = None;
        self.foliage.clear();
        self.weather.clear();
        self.audio.clear();
        self.effects.clear();
        self.weapon_effects.reset(0);
        self.actor_effects.reset(0);
        self.explosion_shapes.reset(0);
        self.weapon_cues.clear();
        self.weapon_animation_cues.clear();
        self.weapon_animation_drops = 0;
        self.weapon_animation_cursor = 0;
        self.weapon_cue_drops = 0;
        self.weapon_light_deferred = 0;
        self.weapon_effect_session = None;
        self.world_items.reset();
        self.attempt.take();
        self.avatars.clear();
        self.avatar_actions.clear();
        self.avatar_action_images.clear();
        self.controls = Controls::default();
        self.cpu_scene = None;
        self.cpu_terrain.clear();
        self.gpu_scene = None;
        self.gpu_terrain.clear();
        self.meshes = None;
        self.cpu_world = None;
        self.gpu_world = None;
        self.world_source = None;
        self.world_job = None;
        self.materials = None;
        self.building = None;
        self.ui
            .apply(UiUpdate::Tools(vec![None; bri_sim::session::TOOL_SLOTS]));
        self.ui.apply(UiUpdate::SetActiveTool(None));
        self.pending_actions.clear();
        self.ui.core.admin = Default::default();
        self.ui.core.minigames = Default::default();
        self.tool_ui.invalidate();
        self.query_source = None;
        self.dialog_epoch = self.dialog_epoch.wrapping_add(1);
        self.ghost_gpu = None;
        self.ghost_uploaded = u64::MAX;
        self.motion.reset();
        self.vehicles.clear();
        self.music_world = None;
        self.controls.free_camera = None;
        self.macro_recording = None;
        self.macro_playback.clear();
        self.combat = Default::default();
    }
    /// The authoritative local player is alive (or not yet known).
    fn local_alive(&self) -> bool {
        self.network_view()
            .and_then(|v| v.vitals.get(&v.owner))
            .is_none_or(|v| v.alive)
    }
    /// Authored `cameraMaxDist` of the vehicle the local player rides.
    fn vehicle_camera(
        assets: &crate::vehicles::VehicleAssets,
        view: &network::View,
    ) -> Option<f32> {
        let (vehicle, _) = view.vitals.get(&view.owner)?.mounted?;
        let info = view.vehicles.get(&vehicle)?;
        assets
            .definition(&info.definition)?
            .authored
            .get("cameramaxdist")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite() && (1.0..=40.0).contains(v))
            .or(Some(8.0))
    }
    fn update_net_graph(&mut self) {
        let Some((since, frames)) = self.net_graph.as_mut() else {
            return;
        };
        *frames += 1;
        let elapsed = since.elapsed().as_secs_f32();
        if elapsed < 0.5 {
            return;
        }
        let fps = *frames as f32 / elapsed;
        *since = std::time::Instant::now();
        *frames = 0;
        let text = match self.network_view() {
            Some(view) => format!(
                "FPS {:.0}   Ping {} ms   Players {}",
                fps,
                view.rtt_ms,
                view.names.len()
            ),
            None => format!("FPS {fps:.0}"),
        };
        self.ui.apply(UiUpdate::NetGraph(Some(text)));
    }
    fn local_mounted(&self) -> bool {
        self.network_view()
            .and_then(|v| v.vitals.get(&v.owner))
            .is_some_and(|v| v.mounted.is_some())
    }
    /// Dead players watch their corpse from the orbit camera.
    fn third_person_view(&self) -> bool {
        self.controls.third_person || self.controls.free_camera.is_some() || !self.local_alive()
    }
    /// Death prompts, damage flash, light sounds, sit state and the
    /// Mini-Games dialog state, all derived from replicated vitals.
    fn update_combat_presentation(&mut self) {
        let Some(a) = self.attempt.as_ref().filter(|a| a.entered) else {
            return;
        };
        let session = a.id;
        let Some(view) = a.view.as_ref() else {
            return;
        };
        let c = &mut self.combat;
        let mut updates = Vec::new();
        for (owner, vitals) in &view.vitals {
            if !vitals.alive {
                c.died_at
                    .entry(*owner)
                    .or_insert_with(std::time::Instant::now);
                c.sitting.remove(owner);
            } else {
                c.died_at.remove(owner);
            }
            let previous = c.lights.insert(*owner, vitals.light);
            if previous.is_some_and(|old| old != vitals.light)
                && let Some(pose) = view.poses.get(owner)
            {
                self.audio.trigger(
                    if vitals.light {
                        "player.light_on"
                    } else {
                        "player.light_off"
                    },
                    bri_audio::Placement::World(pose.player.feet),
                );
            }
            if let Some(pose) = view.poses.get(owner)
                && glam::Vec3::from(pose.player.velocity).length() > 0.5
            {
                c.sitting.remove(owner);
            }
        }
        c.died_at.retain(|owner, _| view.vitals.contains_key(owner));
        c.lights.retain(|owner, _| view.vitals.contains_key(owner));
        if let Some(local) = view.vitals.get(&view.owner) {
            if local.alive {
                if c.alive == Some(false) {
                    updates.push(UiUpdate::ClearPrints);
                }
                if local.health < c.health && c.alive == Some(true) {
                    // Armor::onDamage: flash += delta / maxDamage * 2.
                    updates.push(UiUpdate::DamageFlash(
                        (c.health - local.health) / bri_sim::session::MAX_HEALTH * 2.0,
                    ));
                }
                c.countdown = None;
            } else {
                if c.alive == Some(true) {
                    updates.push(UiUpdate::DamageFlash(0.75));
                }
                // handleYourDeath / respawnCountDownTick.
                let remaining = local.respawn_tick.saturating_sub(view.tick).div_ceil(120);
                if c.countdown != Some(remaining) {
                    c.countdown = Some(remaining);
                    updates.push(UiUpdate::CenterPrint {
                        text: match remaining {
                            0 => "\u{E005}Click to respawn.".into(),
                            1 => "\u{E005}Respawning in 1 second...".into(),
                            n => format!("\u{E005}Respawning in {n} seconds..."),
                        },
                        seconds: if remaining == 0 { 300.0 } else { 2.0 },
                    });
                }
            }
            c.alive = Some(local.alive);
            c.health = local.health;
        }
        let state = crate::minigame_ui::state(
            view.owner,
            &view.minigames,
            &view.vitals,
            &view.names,
            &self.content.weapons.item_choices,
            c.minigame_revision,
        );
        let changed = c.minigame_state.as_ref().is_none_or(|old| {
            MiniGameUiState {
                revision: 0,
                ..old.clone()
            } != MiniGameUiState {
                revision: 0,
                ..state.clone()
            }
        });
        if changed {
            c.minigame_revision += 1;
            let state = MiniGameUiState {
                revision: c.minigame_revision,
                ..state
            };
            c.minigame_state = Some(state.clone());
            updates.push(UiUpdate::MiniGames(state));
        }
        for update in updates {
            self.ui.apply_session(session, update);
        }
    }
    fn player_name(&self) -> String {
        let name = self.ui.settings().avatar.lan_name;
        if name.trim().is_empty() {
            "Blockhead".into()
        } else {
            name
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn host(
        &mut self,
        id: RequestId,
        map: String,
        mode: ServerMode,
        max_players: u32,
        name: String,
        password: String,
        admin: String,
        super_admin: String,
    ) -> Result<()> {
        ensure!(
            password.is_empty(),
            "The server join password is not connected yet. It cannot be silently ignored."
        );
        let admin = bri_admin::Secret::new(admin)?;
        let super_admin = bri_admin::Secret::new(super_admin)?;
        ensure!((1..=64).contains(&max_players), "Invalid player limit");
        ensure!(
            self.content.maps.iter().any(|m| m.id == map),
            "This map has no usable native bundle yet"
        );
        let paths = self.content.paths.clone();
        let weapon_snapshot = self.content.weapons.clone();
        let physics_snapshot = self.content.item_physics.clone();
        let selected: Vec<_> = self
            .content
            .catalog
            .bricks
            .iter()
            .filter(|b| b.selectable())
            .map(|b| (b.id.clone(), b.orientation_fix))
            .collect();
        let avatar_catalog = self.avatar_assets.package.clone();
        let catalog = self.tool_ui.server_catalog();
        let player = self.player_name();
        let local_name = if name.trim().is_empty() {
            "Blockland ReImagined".into()
        } else {
            name
        };
        let single = mode == ServerMode::SinglePlayer;
        let max_players = if single { 1 } else { max_players };
        let listing_name = local_name.clone();
        let listing_map = self
            .content
            .maps
            .iter()
            .find(|m| m.id == map)
            .map_or_else(|| map.clone(), |m| m.name.clone());
        let (scene_tx, scene) = mpsc::sync_channel(1);
        let state_dir = self.state_dir.clone();
        let load_limit = self.load_limit.clone();
        self.disconnect();
        self.ui.apply_session(
            id,
            UiUpdate::Connection(ConnectionState::Loading {
                map: map.clone(),
                preview: self
                    .content
                    .maps
                    .iter()
                    .find(|m| m.id == map)
                    .map_or(IconRef::None, |m| m.preview.clone()),
                phase: LoadPhase::LoadingObjects,
                progress: 0.0,
            }),
        );
        let event_catalog = self.content.events.clone();
        let event_sounds: Vec<String> = self
            .content
            .event_sounds
            .iter()
            .map(|(id, _)| id.clone())
            .collect();
        let worker = Worker::start(self.runtime.handle(), async move {
            let identity_file = state_dir.join("client.identity");
            let native_identity = tokio::task::spawn_blocking(move || {
                bri_identity::ClientIdentity::load_or_create(identity_file)
            })
            .await??;
            let permit = load_limit.acquire_owned().await?;
            let (loaded, visual, identity, catalog, weapon_pack, item_bounds, vehicle_pack) =
                tokio::task::spawn_blocking(move || -> Result<_> {
                    let _permit = permit;
                    let weapons = content_identity::WeaponContent::load(&paths.weapons)?;
                    weapon_snapshot.ensure_same(&weapons)?;
                    let item_physics = content_identity::ItemPhysicsContent::load(
                        &paths.item_presentation,
                        &weapons,
                    )?;
                    physics_snapshot.ensure_same(&item_physics)?;
                    let loaded = paths.load_map(&map, None)?;
                    let visual = load_map_bundle(&paths.map_bundle, &map)?;
                    let identity = content_identity::fingerprint_runtime(
                        &paths.brick_catalog,
                        &paths.geometry,
                        &paths.map_bundle,
                        &paths.brick_materials,
                        &paths.effects,
                        &paths.avatar,
                    )?;
                    let identity =
                        content_identity::with_effects_runtime(&identity, &paths.effects_runtime)?;
                    let identity = content_identity::with_audio(&identity, &paths.audio)?;
                    let identity = content_identity::with_weather(&identity, &paths.weather)?;
                    let identity = content_identity::with_foliage(&identity, &paths.foliage)?;
                    let identity = weapons.extend_identity(&identity);
                    let identity = item_physics.extend_identity(&identity);
                    let identity = content_identity::with_vehicles(&identity, &paths.vehicles)?;
                    let identity = content_identity::with_events(&identity, &paths.events)?;
                    let vehicle_pack =
                        bri_vehicles::Pack::load(paths.vehicles.join("vehicles.json"))?;
                    let meshes = Arc::new(
                        loaded
                            .simulation
                            .definitions
                            .entries
                            .iter()
                            .map(|(id, def)| (id.clone(), def.mesh.clone()))
                            .collect(),
                    );
                    let materials = Arc::new(crate::materials::BrickMaterials::load(
                        &paths.brick_materials,
                    )?);
                    let mut mirror = bri_sim::prediction::CollisionMirror::new(
                        loaded.simulation.definitions.clone(),
                        loaded.query_colliders.clone(),
                        loaded.simulation.waters.clone(),
                    );
                    mirror.attach_terrain(loaded.terrain.clone())?;
                    let mut building = crate::building::Building::new(
                        loaded.simulation.definitions.clone(),
                        loaded.query_colliders.clone(),
                    )?;
                    building.attach_terrain(loaded.terrain.clone());
                    building.set_catalog(selected)?;
                    if let Some(print) = &catalog.default_print {
                        building.set_default_prints(
                            catalog
                                .brick_print_aspects
                                .keys()
                                .map(|id| (id.clone(), print.clone()))
                                .collect(),
                        )?;
                    }
                    let waters = loaded.simulation.waters.clone();
                    let foliage = crate::foliage::PreparedFoliage::load(
                        &paths.foliage,
                        &map,
                        &building,
                        &waters,
                    )?;
                    Ok((
                        loaded,
                        Prepared {
                            foliage,
                            map_id: map.clone(),
                            waters,
                            scene: visual.scene,
                            terrain: visual.terrain.into_iter().map(Arc::new).collect(),
                            meshes,
                            materials,
                            building,
                            mirror,
                        },
                        identity,
                        catalog,
                        weapons.pack,
                        item_physics.bounds,
                        vehicle_pack,
                    ))
                })
                .await??;
            scene_tx.send(visual).context("Loading cancelled")?;
            let bind = if single {
                "127.0.0.1:0"
            } else {
                "0.0.0.0:28000"
            }
            .parse()?;
            let mut session = Session::new(loaded.simulation);
            session.set_admin_passwords(admin, super_admin)?;
            session.set_tool_catalog(catalog)?;
            session.set_weapon_pack(weapon_pack)?;
            session.set_item_bounds(item_bounds)?;
            session.set_avatar_catalog(avatar_catalog)?;
            session.set_vehicle_pack(vehicle_pack)?;
            session.set_event_catalog(event_catalog, event_sounds)?;
            session.set_spawn_points(loaded.spawn_points.clone())?;
            if let Some(tutorial) = loaded.tutorial {
                session.set_tutorial(tutorial)?;
            }
            let mut host = server::start_with_admin_store_and_limit(
                session,
                ServerOptions {
                    bind,
                    content_id: identity.clone(),
                    spawn_points: loaded.spawn_points,
                    // LAN hosts keep one identity so joiners' saved trust stays valid.
                    certificate: if single {
                        None
                    } else {
                        Some(server::HostCertificate::load_or_create(&state_dir)?)
                    },
                },
                max_players as usize,
                state_dir.join("administration.json"),
            )?;
            let address = SocketAddr::from(([127, 0, 0, 1], host.address.port()));
            if !single {
                // LAN players find this host (and its certificate) by broadcast.
                host.advertise(listing_name, listing_map, max_players, identity.clone())
                    .await?;
            }
            let client = Client::connect_with_identity(
                address,
                &host.certificate,
                player,
                identity,
                None,
                Some(host.host_token.clone()),
                &native_identity,
            )
            .await?;
            Ok(Connected {
                client,
                host: Some(host),
            })
        });
        self.attempt = Some(Attempt {
            id,
            worker,
            scene,
            name: local_name,
            max_players,
            local: true,
            single,
            ready: false,
            entered: false,
            view: None,
            last_chat: 0,
        });
        Ok(())
    }
    fn join(&mut self, id: RequestId, address: String, password: String) -> Result<()> {
        ensure!(
            password.is_empty(),
            "Password authentication is not connected yet"
        );
        let address: SocketAddr = address
            .parse()
            .context("Enter an IP address and port, for example 192.168.1.10:28000")?;
        // Certificates come from LAN discovery, then saved pins, then a direct
        // discovery query to the address (trust on first use, then pinned).
        let pins_file = self.state_dir.join("trusted-hosts.json");
        let known = self
            .lan_hosts
            .get(&address.to_string())
            .cloned()
            .or_else(|| {
                std::fs::metadata(&pins_file)
                    .ok()
                    .filter(|m| m.len() <= 1024 * 1024)
                    .and_then(|_| std::fs::read(&pins_file).ok())
                    .and_then(|bytes| {
                        serde_json::from_slice::<BTreeMap<String, Vec<u8>>>(&bytes).ok()
                    })
                    .and_then(|pins| pins.get(&address.to_string()).cloned())
            });
        let paths = self.content.paths.clone();
        let player = self.player_name();
        let weapon_snapshot = self.content.weapons.clone();
        let physics_snapshot = self.content.item_physics.clone();
        let selected: Vec<_> = self
            .content
            .catalog
            .bricks
            .iter()
            .filter(|b| b.selectable())
            .map(|b| (b.id.clone(), b.orientation_fix))
            .collect();
        let catalog = self.tool_ui.server_catalog();
        let (scene_tx, scene) = mpsc::sync_channel(1);
        let load_limit = self.load_limit.clone();
        let identity_file = self.state_dir.join("client.identity");
        self.disconnect();
        self.ui.apply_session(
            id,
            UiUpdate::Connection(ConnectionState::Connecting {
                text: format!("Connecting to {address}…"),
            }),
        );
        let worker = Worker::start(self.runtime.handle(), async move {
            let certificate = match known {
                Some(certificate) => certificate,
                None => bri_net::discovery::query(
                    &[SocketAddr::new(
                        address.ip(),
                        bri_net::discovery::DISCOVERY_PORT,
                    )],
                    Duration::from_millis(1500),
                )
                .await?
                .into_iter()
                .find(|(a, _)| a.port() == address.port())
                .context("No Blockland ReImagined host answered at that address")?
                .1
                .certificate_der()?,
            };
            ensure!(
                !certificate.is_empty() && certificate.len() <= 16384,
                "Invalid host certificate"
            );
            let native_identity = tokio::task::spawn_blocking(move || {
                bri_identity::ClientIdentity::load_or_create(identity_file)
            })
            .await??;
            let identity_paths = paths.clone();
            let permit = load_limit.clone().acquire_owned().await?;
            let identity = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let weapons = content_identity::WeaponContent::load(&identity_paths.weapons)?;
                weapon_snapshot.ensure_same(&weapons)?;
                let item_physics = content_identity::ItemPhysicsContent::load(
                    &identity_paths.item_presentation,
                    &weapons,
                )?;
                physics_snapshot.ensure_same(&item_physics)?;
                let identity = content_identity::fingerprint_runtime(
                    &identity_paths.brick_catalog,
                    &identity_paths.geometry,
                    &identity_paths.map_bundle,
                    &identity_paths.brick_materials,
                    &identity_paths.effects,
                    &identity_paths.avatar,
                )?;
                let identity = content_identity::with_effects_runtime(
                    &identity,
                    &identity_paths.effects_runtime,
                )?;
                let identity = content_identity::with_audio(&identity, &identity_paths.audio)?;
                let identity = content_identity::with_weather(&identity, &identity_paths.weather)?;
                let identity = content_identity::with_foliage(&identity, &identity_paths.foliage)?;
                content_identity::with_events(
                    &content_identity::with_vehicles(
                        &item_physics.extend_identity(&weapons.extend_identity(&identity)),
                        &identity_paths.vehicles,
                    )?,
                    &identity_paths.events,
                )
            })
            .await??;
            let client = Client::connect_with_identity(
                address,
                &certificate,
                player,
                identity,
                None,
                None,
                &native_identity,
            )
            .await?;
            // Remember the host's certificate for later direct joins.
            let pin = certificate.clone();
            let _ = tokio::task::spawn_blocking(move || -> Result<()> {
                let mut pins: BTreeMap<String, Vec<u8>> = std::fs::read(&pins_file)
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .unwrap_or_default();
                if pins.len() < 1024 {
                    pins.insert(address.to_string(), pin);
                    std::fs::write(&pins_file, serde_json::to_vec_pretty(&pins)?)?;
                }
                Ok(())
            })
            .await;
            let map = client.replica.world.map_id.clone();
            ensure!(
                LOADABLE_MAPS.contains(&map.as_str()),
                "Server map has no supported native render bundle yet"
            );
            let permit = load_limit.acquire_owned().await?;
            let visual = tokio::task::spawn_blocking(move || -> Result<Prepared> {
                let _permit = permit;
                let visual = load_map_bundle(&paths.map_bundle, &map)?;
                let definitions = Definitions::load(&paths.brick_catalog, &paths.geometry)?;
                let meshes = Arc::new(
                    definitions
                        .entries
                        .iter()
                        .map(|(id, def)| (id.clone(), def.mesh.clone()))
                        .collect(),
                );
                let native_map = bri_sim::map::NativeMap::load(&paths.map_bundle, &map)?;
                let materials = Arc::new(crate::materials::BrickMaterials::load(
                    &paths.brick_materials,
                )?);
                let mut mirror = bri_sim::prediction::CollisionMirror::new(
                    definitions.clone(),
                    native_map.colliders.clone(),
                    native_map.waters.clone(),
                );
                mirror.attach_terrain(native_map.terrain.clone())?;
                let mut building =
                    crate::building::Building::new(definitions, native_map.colliders)?;
                building.attach_terrain(native_map.terrain);
                building.set_catalog(selected)?;
                if let Some(print) = &catalog.default_print {
                    building.set_default_prints(
                        catalog
                            .brick_print_aspects
                            .keys()
                            .map(|id| (id.clone(), print.clone()))
                            .collect(),
                    )?;
                }
                let foliage = crate::foliage::PreparedFoliage::load(
                    &paths.foliage,
                    &map,
                    &building,
                    &native_map.waters,
                )?;
                Ok(Prepared {
                    foliage,
                    map_id: map,
                    waters: native_map.waters,
                    scene: visual.scene,
                    terrain: visual.terrain.into_iter().map(Arc::new).collect(),
                    meshes,
                    materials,
                    building,
                    mirror,
                })
            })
            .await??;
            scene_tx.send(visual).context("Loading cancelled")?;
            Ok(Connected { client, host: None })
        });
        self.attempt = Some(Attempt {
            id,
            worker,
            scene,
            name: address.to_string(),
            max_players: 64,
            local: false,
            single: false,
            ready: false,
            entered: false,
            view: None,
            last_chat: 0,
        });
        Ok(())
    }
    fn command(&mut self, id: RequestId, command: Command, action: UiAction) -> Result<()> {
        // Tool dialogs and inventory reconciliation need the command afterward. In particular,
        // do not clone/retain an entire loaded build while awaiting its reply.
        let retained = matches!(
            &command,
            Command::Tool(_)
                | Command::EquipTool { .. }
                | Command::DropTool { .. }
                | Command::WeaponTrigger { .. }
        )
        .then(|| command.clone());
        let inspection = match &command {
            Command::Tool(ToolAction::Inspect { mode }) => Some(*mode),
            _ => None,
        };
        let dialog_request = inspection.is_some()
            || matches!(
                action,
                UiAction::SetPrint { .. }
                    | UiAction::SendWrench { .. }
                    | UiAction::SendEvents { .. }
            );
        self.attempt
            .as_ref()
            .filter(|a| a.entered)
            .context("Not connected")?
            .worker
            .request_with_aim(
                id,
                command,
                Some(bri_sim::session::ActionAim {
                    yaw: self.controls.yaw,
                    pitch: self.controls.pitch,
                }),
            )?;
        self.pending_actions.insert(
            id,
            PendingAction {
                action,
                command: retained,
                dialog_epoch: self.dialog_epoch,
                inspection,
                dialog_request,
            },
        );
        Ok(())
    }

    fn invalidate_tool_dialogs(&mut self) {
        self.dialog_epoch = self.dialog_epoch.wrapping_add(1);
        self.tool_ui.invalidate();
    }

    fn handle_building(&mut self, id: RequestId, action: &UiAction) -> Result<bool> {
        if self.attempt.as_ref().is_none_or(|a| !a.entered) {
            return Ok(false);
        }
        if self
            .attempt
            .as_ref()
            .is_some_and(|a| self.ui.session_request() != Some(a.id))
        {
            return Ok(true); // action queued before a newer disconnect/rehost
        }
        if matches!(
            action,
            UiAction::UseTool { .. }
                | UiAction::UseBrickSlot { .. }
                | UiAction::InstantUseBrick { .. }
                | UiAction::BuyBricks { .. }
                | UiAction::UnUseTool
                | UiAction::UseSprayCan { .. }
                | UiAction::UseFxCan { .. }
                | UiAction::CancelWrench { .. }
                | UiAction::ClosePrintSelector
        ) {
            self.invalidate_tool_dialogs();
        }
        if let Some(command) = self.tool_ui.action_command(action)? {
            self.command(id, command, action.clone())?;
            return Ok(true);
        }
        if matches!(
            action,
            UiAction::CancelWrench { .. } | UiAction::ClosePrintSelector
        ) {
            self.answer(id, Ok(()));
            return Ok(true);
        }
        let view = self.network_view().context("No active network view")?;
        let mut player = self
            .motion
            .presented()
            .get(&view.owner)
            .or_else(|| view.poses.get(&view.owner).map(|pose| &pose.player))
            .context("No local player pose")?
            .clone();
        // The latest local body aim drives ghost input. Free-look only changes
        // the camera; server tool targeting still uses its authoritative pose.
        player.yaw = self.controls.yaw;
        player.pitch = self.controls.pitch;
        let ghost_before = self.building.as_ref().and_then(|b| b.ghost().cloned());
        let response = self
            .building
            .as_mut()
            .context("Building controller not ready")?
            .ui_action(action, &player)?;
        let Some(response) = response else {
            return Ok(false);
        };
        if let Some(ghost) = self.building.as_ref().and_then(|b| b.ghost()) {
            let cue = if ghost_before
                .as_ref()
                .is_none_or(|b| b.definition != ghost.definition)
            {
                Some("brick.change")
            } else if ghost_before
                .as_ref()
                .is_some_and(|b| b.quarter_turns != ghost.quarter_turns)
            {
                Some("brick.rotate")
            } else if ghost_before
                .as_ref()
                .is_some_and(|b| b.position != ghost.position)
            {
                Some("brick.move")
            } else {
                None
            };
            if let Some(cue) = cue {
                self.audio
                    .trigger(cue, bri_audio::Placement::World(ghost.position));
            }
        }
        let session = self.attempt.as_ref().unwrap().id;
        for update in response.updates {
            self.ui.apply_session(session, update);
        }
        ensure!(
            response.commands.len() <= 1,
            "One UI request cannot own multiple server commands"
        );
        if let Some(command) = response.commands.into_iter().next() {
            if matches!(command, Command::Tool(ToolAction::Inspect { .. })) {
                self.invalidate_tool_dialogs();
            }
            if let Err(error) = self.building.as_mut().unwrap().command_sent(id, &command) {
                for update in self
                    .building
                    .as_mut()
                    .unwrap()
                    .command_finished(id, &command, false)
                {
                    self.ui.apply_session(session, update);
                }
                return Err(error);
            }
            if let Err(error) = self.command(id, command.clone(), action.clone()) {
                let updates = self
                    .building
                    .as_mut()
                    .unwrap()
                    .command_finished(id, &command, false);
                for update in updates {
                    self.ui.apply_session(session, update);
                }
                // Closing the session clears a host-held trigger if its release
                // cannot enter the bounded transport queue.
                if matches!(command, Command::WeaponTrigger { down: false }) {
                    self.disconnect();
                }
                return Err(error);
            }
        } else {
            self.answer(id, Ok(()));
        }
        Ok(true)
    }

    fn accept_reply(
        &mut self,
        attempt: &Attempt,
        request: RequestId,
        result: std::result::Result<Reply, bri_sim::session::Rejection>,
    ) {
        let Some(pending) = self.pending_actions.remove(&request) else {
            return;
        };
        // Placement failures use the original HUD plant-error icon and sound,
        // never a modal dialog.
        let plant_failure = match &result {
            Err(rejection) => rejection.plant,
            Ok(_) => None,
        };
        let result = result.map_err(|rejection| rejection.message);
        if pending.dialog_request && pending.dialog_epoch != self.dialog_epoch {
            return;
        }
        if let UiAction::Admin(action) = &pending.action {
            let accepted = (|| {
                let reply = result.map_err(anyhow::Error::msg)?;
                let Reply::Admin(reply) = reply else {
                    anyhow::bail!("Server returned an unexpected administration reply");
                };
                if matches!(
                    reply.data,
                    bri_sim::session::AdminData::BrickGroups(_)
                        | bri_sim::session::AdminData::BanList { .. }
                ) {
                    ensure!(
                        reply.snapshot.revision >= self.ui.core.admin.revision,
                        "Administration state changed; refresh the list"
                    );
                }
                for update in crate::admin_ui::reply_updates(request, action, &reply)? {
                    if let UiUpdate::Admin(bri_ui::models::admin::AdminUpdate::State(snapshot)) =
                        &update
                        && snapshot.revision < self.ui.core.admin.revision
                    {
                        continue;
                    }
                    self.ui.apply_session(attempt.id, update);
                }
                Ok(())
            })();
            self.answer(request, accepted);
            return;
        }
        if let Some(command) = &pending.command
            && let Some(building) = &mut self.building
        {
            for update in building.command_finished(request, command, result.is_ok()) {
                self.ui.apply_session(attempt.id, update);
            }
        }
        if matches!(&pending.action, UiAction::SaveBricks { .. }) {
            let queued = match result {
                Ok(Reply::Saved(build)) => self.file_jobs.enqueue(crate::saves::Request {
                    id: request,
                    session: Some(attempt.id),
                    action: pending.action,
                    build: Some(build),
                }),
                Ok(_) => Err(anyhow::anyhow!("Server returned an unexpected save reply")),
                Err(error) => Err(anyhow::anyhow!(error)),
            };
            if let Err(error) = queued {
                self.answer(request, Err(error));
            }
            return;
        }
        let result = result.and_then(|reply| {
            if let Some(mode) = pending.inspection {
                // Initial inspection may not interrupt a newer modal/typing
                // interaction. Events are opened only for their still-open wrench.
                let expected = if let UiAction::RequestEvents { brick } = pending.action {
                    Some(brick)
                } else {
                    None
                };
                let can_open = if expected.is_some() {
                    self.ui
                        .stack()
                        .iter()
                        .any(|s| matches!(s, ScreenId::Wrench(_) | ScreenId::WrenchEvents))
                } else {
                    self.ui.stack() == [ScreenId::Play]
                };
                if !can_open {
                    return Ok(());
                }
                let view = attempt
                    .view
                    .as_ref()
                    .ok_or_else(|| "Inspection has no active world".to_string())?;
                let updates = self
                    .tool_ui
                    .accept_inspection(&reply, mode, expected, &view.world, &view.names, view.owner)
                    .map_err(|e| format!("{e:#}"))?;
                for update in updates {
                    self.ui.apply_session(attempt.id, update);
                }
            } else if matches!(
                pending.action,
                UiAction::SetPrint { .. }
                    | UiAction::SendWrench { .. }
                    | UiAction::SendEvents { .. }
            ) {
                self.tool_ui
                    .command_accepted(
                        pending
                            .command
                            .as_ref()
                            .ok_or("Missing accepted tool command")?,
                    )
                    .map_err(|e| {
                        format!("Server accepted the edit, but dialog refresh failed: {e:#}")
                    })?;
            }
            Ok(())
        });
        let result = match plant_failure {
            Some(failure) => {
                use bri_sim::simulation::PlantFailure as F;
                let icon = match failure {
                    F::Overlap => PlantError::Overlap,
                    F::Float => PlantError::Float,
                    F::Buried => PlantError::Buried,
                    F::Stuck => PlantError::Stuck,
                    F::TooFar => PlantError::TooFar,
                    F::Forbidden => PlantError::Forbidden,
                    F::Limit => PlantError::Limit,
                };
                self.ui
                    .apply_session(attempt.id, UiUpdate::PlantError(icon));
                Ok(())
            }
            None => result,
        };
        self.ui.apply_session(
            attempt.id,
            UiUpdate::ActionResult {
                id: request,
                result,
            },
        );
    }
    fn poll_files(&mut self) {
        let Some((request, result)) = self.file_jobs.poll(&self.saves, &self.runtime) else {
            return;
        };
        let result = match result {
            Ok(crate::saves::Outcome::Listed(entries)) => {
                let maps = entries
                    .iter()
                    .map(|e| e.info.map.clone())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();
                self.ui.apply(UiUpdate::SaveFiles {
                    maps,
                    files: entries.into_iter().map(|e| e.info).collect(),
                });
                Ok(())
            }
            Ok(crate::saves::Outcome::Loaded(build)) => {
                if self.attempt.as_ref().filter(|a| a.entered).map(|a| a.id) != request.session
                    || self.ui.session_request() != request.session
                {
                    Err(anyhow::anyhow!(
                        "Connection changed while reading the build; load canceled"
                    ))
                } else if let UiAction::LoadBricks { ownership, .. } = &request.action {
                    match self.command(
                        request.id,
                        Command::LoadBuild {
                            build,
                            ownership: *ownership,
                        },
                        request.action,
                    ) {
                        Ok(()) => return, // Complete only after authoritative acceptance.
                        Err(error) => Err(error),
                    }
                } else {
                    Err(anyhow::anyhow!("Unexpected loaded build"))
                }
            }
            Err(error) => Err(anyhow::anyhow!(error)),
        };
        self.answer(request.id, result);
    }
    fn poll_network(&mut self) -> Result<()> {
        let Some(mut a) = self.attempt.take() else {
            return Ok(());
        };
        if self.ui.session_request() != Some(a.id) {
            self.disconnect();
            return Ok(());
        }
        if a.worker.view.has_changed().unwrap_or(false) {
            a.view = a.worker.view.borrow_and_update().clone();
        }
        let mut failed = None;
        while let Ok(event) = a.worker.events.try_recv() {
            match event {
                network::Event::Presentation { cues, dropped } => {
                    self.audio.server_dropped = dropped;
                    for cue in cues {
                        self.queue_weapon_cue(cue);
                    }
                }
                network::Event::Ready => a.ready = true,
                network::Event::Notice(notice) => {
                    let update = match notice {
                        bri_sim::session::Notice::Chat(text) => UiUpdate::Chat {
                            text: server_markup(&text),
                        },
                        bri_sim::session::Notice::Center { text, seconds } => {
                            UiUpdate::CenterPrint {
                                text: print_markup(&self.ui.core.binds, &text),
                                seconds,
                            }
                        }
                        bri_sim::session::Notice::Bottom { text, seconds } => {
                            UiUpdate::BottomPrint {
                                text: print_markup(&self.ui.core.binds, &text),
                                seconds,
                                hide_bar: false,
                            }
                        }
                        bri_sim::session::Notice::Abilities(abilities) => {
                            self.abilities = abilities;
                            continue;
                        }
                        bri_sim::session::Notice::Invite {
                            game,
                            owner_name,
                            title,
                        } => UiUpdate::MiniGameInvite(MiniGameInvitation {
                            game: MiniGameId(game),
                            title: plain_chat(&title),
                            owner: MiniGamePlayerId(0),
                            owner_name: plain_chat(&owner_name),
                            owner_display_id: String::new(),
                        }),
                    };
                    self.ui.apply_session(a.id, update);
                }
                network::Event::Reply { request, result } => {
                    self.accept_reply(&a, request, result);
                }
                network::Event::Failed(reason) => {
                    failed = Some(reason);
                    break;
                }
            }
        }
        if failed.is_none() && a.worker.events.is_closed() {
            failed = Some("Connection worker stopped".into());
        }
        if let Some(reason) = failed {
            self.ui.apply_session(
                a.id,
                UiUpdate::Connection(ConnectionState::Failed { reason }),
            );
            self.disconnect();
            return Ok(());
        }
        if let Ok(prepared) = a.scene.try_recv() {
            self.foliage.set_map(prepared.foliage);
            self.weather.set_map(&prepared.map_id, prepared.waters)?;
            self.cpu_scene = Some(prepared.scene);
            self.cpu_terrain = prepared.terrain;
            self.meshes = Some(prepared.meshes);
            self.materials = Some(prepared.materials);
            self.building = Some(prepared.building);
            self.motion.install(prepared.mirror);
            self.building
                .as_mut()
                .unwrap()
                .set_tool_catalog(self.item_ui.catalog())?;
            self.gpu_scene = None;
            self.gpu_terrain.clear();
        }
        if a.worker.view.has_changed().unwrap_or(false) {
            a.view = a.worker.view.borrow_and_update().clone();
        }
        if let (Some(building), Some(view)) = (&mut self.building, &a.view)
            && let Some(inventory) = view.tools.get(&view.owner)
        {
            match building.sync_tools(inventory) {
                Ok(updates) => {
                    for update in updates {
                        self.ui.apply_session(a.id, update);
                    }
                }
                Err(error) => {
                    self.ui.apply_session(
                        a.id,
                        UiUpdate::Connection(ConnectionState::Failed {
                            reason: format!("Native tool inventory: {error:#}"),
                        }),
                    );
                    self.disconnect();
                    return Ok(());
                }
            }
        }
        if a.entered
            && let Some(building) = &self.building
        {
            let hand = bri_sim::session::BrickHand {
                stocked: building.inventory().iter().any(Option::is_some),
                equipped: matches!(building.equipment(), crate::building::Equipment::Brick(_)),
                ghost: building.ghost().is_some(),
            };
            // A full request queue leaves the report pending for the next frame.
            if self.brick_hand != Some(hand)
                && a.worker
                    .request(REPORT_REQUEST, Command::BrickHand(hand))
                    .is_ok()
            {
                self.brick_hand = Some(hand);
            }
        }
        if let (Some(building), Some(view)) = (&mut self.building, &a.view)
            && self
                .query_source
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, &view.world))
        {
            if let Err(error) = building.sync_world(&view.world) {
                self.ui.apply_session(
                    a.id,
                    UiUpdate::Connection(ConnectionState::Failed {
                        reason: format!("Native query mirror: {error:#}"),
                    }),
                );
                self.disconnect();
                return Ok(());
            }
            if a.entered
                && self
                    .query_source
                    .as_ref()
                    .is_none_or(|old| old.palette != view.world.palette)
            {
                let colors = view
                    .world
                    .palette
                    .chunks(9)
                    .enumerate()
                    .map(|(i, c)| PaintDivision {
                        name: format!("World {}", i + 1),
                        colors: c.to_vec(),
                    })
                    .collect();
                self.ui.apply_session(a.id, UiUpdate::Colorset(colors));
            }
            self.query_source = Some(view.world.clone());
            self.ghost_uploaded = u64::MAX;
        }
        if let Some(job) = &self.world_job
            && let Ok((source, result)) = job.receiver.try_recv()
        {
            self.world_job = None;
            match result {
                Ok(scene)
                    if a.view
                        .as_ref()
                        .is_some_and(|view| Arc::ptr_eq(&source, &view.world)) =>
                {
                    self.cpu_world = Some(scene);
                    self.world_source = Some(source);
                    self.gpu_world = None;
                }
                Ok(_) => {} // superseded while the background render build ran
                Err(reason) => {
                    self.ui.apply_session(
                        a.id,
                        UiUpdate::Connection(ConnectionState::Failed { reason }),
                    );
                    self.disconnect();
                    return Ok(());
                }
            }
        }
        if self.world_job.is_none()
            && let (Some(meshes), Some(materials), Some(view)) =
                (&self.meshes, &self.materials, &a.view)
            && self
                .world_source
                .as_ref()
                .is_none_or(|previous| !Arc::ptr_eq(previous, &view.world))
        {
            let meshes = meshes.clone();
            let materials = materials.clone();
            let world = view.world.clone();
            let (send, receive) = mpsc::sync_channel(1);
            let load_limit = self.load_limit.clone();
            let task = self.runtime.spawn(async move {
                let Ok(permit) = load_limit.acquire_owned().await else {
                    return;
                };
                let source = world.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    crate::world_scene::build_world_scene_materials(
                        &world,
                        &meshes,
                        4_000_000,
                        Some(&materials),
                    )
                    .map_err(|e| format!("{e:#}"))
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
                let _ = send.send((source, result));
            });
            self.world_job = Some(WorldJob {
                receiver: receive,
                abort: task.abort_handle(),
            });
        }
        if a.ready
            && !a.entered
            && self.world_source.is_some()
            && let Some(scene) = &self.cpu_scene
        {
            self.ui.apply_session(
                a.id,
                UiUpdate::ActionResult {
                    id: a.id,
                    result: Ok(()),
                },
            );
            self.ui.apply_session(
                a.id,
                UiUpdate::Connection(ConnectionState::InGame {
                    server_name: a.name.clone(),
                    max_players: a.max_players,
                    local: a.local,
                    single_player: a.single,
                    admin: a.view.as_ref().is_some_and(|v| v.administrator),
                }),
            );
            self.ui
                .apply_session(a.id, UiUpdate::Bricks(self.content.bricks.clone()));
            let palette = &a
                .view
                .as_ref()
                .context("Ready connection has no world")?
                .world
                .palette;
            let default_colors: Vec<_> = self
                .content
                .paint
                .iter()
                .flat_map(|d| d.colors.iter().copied())
                .collect();
            let colors = if palette == &default_colors {
                self.content.paint.clone()
            } else {
                palette
                    .chunks(9)
                    .enumerate()
                    .map(|(i, c)| PaintDivision {
                        name: format!("World {}", i + 1),
                        colors: c.to_vec(),
                    })
                    .collect()
            };
            self.ui.apply_session(a.id, UiUpdate::Colorset(colors));
            self.ui
                .apply_session(a.id, UiUpdate::Datablocks(self.content.datablocks.clone()));
            self.ui.apply_session(a.id, UiUpdate::BuildingAllowed(true));
            for update in self.tool_ui.catalog_updates() {
                self.ui.apply_session(a.id, update);
            }
            for update in self
                .building
                .as_ref()
                .context("Ready connection has no building controller")?
                .initial_updates()
            {
                self.ui.apply_session(a.id, update);
            }
            self.ui.apply_session(
                a.id,
                UiUpdate::SaveContext {
                    map: scene.name.clone(),
                    preview: self
                        .content
                        .maps
                        .iter()
                        .find(|m| m.id == scene.id)
                        .map(|m| m.preview.clone())
                        .unwrap_or(IconRef::None),
                },
            );
            a.entered = true;
            if let Some(view) = &a.view {
                self.reset_weapon_effect_session(a.id, view.checkpoint_cue_cursor);
            }
            self.ui.apply_session(a.id, UiUpdate::FirstSpawn);
            self.ui
                .core
                .request(UiAction::SetAvatar(self.ui.settings().avatar));
        }
        if let Some(view) = &a.view {
            if let Some(snapshot) = &view.admin_snapshot
                && (self.ui.core.admin.snapshot.is_none()
                    || snapshot.revision > self.ui.core.admin.revision)
            {
                self.ui.apply_session(
                    a.id,
                    UiUpdate::Admin(bri_ui::models::admin::AdminUpdate::State(
                        crate::admin_ui::state(snapshot),
                    )),
                );
            }
            for line in &view.chat {
                if line.id > a.last_chat {
                    // Owner 0 lines are server-authored (death messages) and
                    // may carry vanilla color escapes and death icons.
                    let text = if line.owner == 0 {
                        server_markup(&line.text)
                    } else {
                        format!("{}: {}", plain_chat(&line.name), plain_chat(&line.text))
                    };
                    self.ui.apply_session(a.id, UiUpdate::Chat { text });
                    a.last_chat = line.id;
                }
            }
            self.ui.apply_session(
                a.id,
                UiUpdate::Players {
                    rows: view
                        .names
                        .iter()
                        .map(|(&owner, name)| PlayerRow {
                            id: owner,
                            name: plain_chat(name),
                            score: view.vitals.get(&owner).map_or(0, |v| {
                                v.score.clamp(i32::MIN as i64, i32::MAX as i64) as i32
                            }),
                            admin: owner == view.owner && view.administrator,
                            super_admin: false,
                            bl_id: None,
                            trust: if owner == view.owner {
                                "You".into()
                            } else {
                                "None".into()
                            },
                        })
                        .collect(),
                    server_name: a.name.clone(),
                    max_players: a.max_players,
                },
            );
        }
        if let Some(view) = &a.view
            && let Err(error) = self.motion.observe(view)
        {
            self.ui.apply_session(
                a.id,
                UiUpdate::Connection(ConnectionState::Failed {
                    reason: format!("Movement prediction: {error:#}"),
                }),
            );
            self.disconnect();
            return Ok(());
        }
        self.attempt = Some(a);
        Ok(())
    }
}
/// Chat strings are plain user content, never UI markup/color instructions.
impl Drop for App {
    fn drop(&mut self) {
        self.disconnect();
    }
}
fn plain_chat(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() && !(0xE000..0xE010).contains(&(*c as u32)))
        .map(|c| match c {
            '<' => '‹',
            '>' => '›',
            _ => c,
        })
        .collect()
}
/// Center and bottom prints are server markup on several lines. `<key:cmd>`
/// names the player's own binding for a command, as the Tutorial's
/// `bindNameFix` does.
fn print_markup(binds: &bri_ui::binds::BindMap, text: &str) -> String {
    let mut resolved = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<key:") {
        resolved.push_str(&rest[..start]);
        let after = &rest[start + 5..];
        match after.find('>') {
            Some(end) if end <= 64 => {
                resolved.push_str(&key_name(binds, &after[..end]));
                rest = &after[end + 1..];
            }
            _ => {
                resolved.push_str("<key:");
                rest = after;
            }
        }
    }
    resolved.push_str(rest);
    resolved
        .split('\n')
        .map(server_markup)
        .collect::<Vec<_>>()
        .join("\n")
}
/// `bindNameFix`: mouse buttons and a few keys get readable names, single
/// letters are upper case.
fn key_name(binds: &bri_ui::binds::BindMap, command: &str) -> String {
    use bri_ui::api::BindInput;
    use bri_ui::input::MouseButton;
    match binds.binding_of(command) {
        Some(BindInput::Mouse(MouseButton::Left)) => "left mouse button".into(),
        Some(BindInput::Mouse(MouseButton::Right)) => "right mouse button".into(),
        Some(BindInput::Mouse(MouseButton::Middle)) => "middle mouse button".into(),
        Some(BindInput::Key(chord)) => match chord.key.torque_name().as_str() {
            "space" => "spacebar".into(),
            "lshift" => "left shift".into(),
            "rshift" => "right shift".into(),
            "lalt" => "left alt".into(),
            "ralt" => "right alt".into(),
            key if key.chars().count() == 1 => key.to_ascii_uppercase(),
            key => key.into(),
        },
        Some(_) => binds.display(command),
        None => "(unbound)".into(),
    }
}
/// Server-authored text keeps vanilla color escapes and `<bitmap:...>` icons
/// (base UI and add-on death icons), but no other markup or control characters.
fn server_markup(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<bitmap:") {
        out.push_str(&plain_chat(&rest[..start]));
        let after = &rest[start..];
        match after.find('>') {
            Some(end)
                if after[8..end]
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/_-".contains(&b))
                    && ["base/client/ui/", "add-ons/"]
                        .iter()
                        .any(|p| after[8..end].to_ascii_lowercase().starts_with(p)) =>
            {
                out.push_str(&after[..=end].to_ascii_lowercase());
                rest = &after[end + 1..];
            }
            _ => {
                out.push_str(&plain_chat(&after[..8]));
                rest = &after[8..];
            }
        }
    }
    out.push_str(
        &rest
            .chars()
            .filter(|c| !c.is_control())
            .map(|c| match c {
                '<' => '‹',
                '>' => '›',
                _ => c,
            })
            .collect::<String>(),
    );
    out
}

/// Client-side death, respawn and status presentation derived from vitals.
#[derive(Default)]
struct CombatPresentation {
    alive: Option<bool>,
    health: f32,
    countdown: Option<u64>,
    died_at: std::collections::BTreeMap<bri_world::OwnerId, std::time::Instant>,
    lights: std::collections::BTreeMap<bri_world::OwnerId, bool>,
    sitting: std::collections::BTreeSet<bri_world::OwnerId>,
    minigame_revision: u64,
    minigame_state: Option<MiniGameUiState>,
}
impl CombatPresentation {
    /// Corpses disappear after `$CorpseTimeoutValue` (5 s).
    fn hidden_bodies(
        &self,
        vitals: &std::collections::BTreeMap<bri_world::OwnerId, bri_sim::session::Vitals>,
    ) -> std::collections::BTreeSet<bri_world::OwnerId> {
        self.died_at
            .iter()
            .filter(|(owner, at)| {
                vitals.get(owner).is_some_and(|v| !v.alive)
                    && at.elapsed() >= Duration::from_secs(5)
            })
            .map(|(owner, _)| *owner)
            .collect()
    }
}

/// Ghost/plant/brick-selection actions recorded by build macros.
fn macro_action(action: &UiAction) -> bool {
    matches!(
        action,
        UiAction::UseBrickSlot { .. }
            | UiAction::InstantUseBrick { .. }
            | UiAction::Game(
                GameAction::ShiftBrick { .. }
                    | GameAction::SuperShiftBrick { .. }
                    | GameAction::RotateBrick { .. }
                    | GameAction::PlantBrick
            )
    )
}

fn building_action(action: &UiAction) -> bool {
    matches!(
        action,
        UiAction::BuyBricks { .. }
            | UiAction::InstantUseBrick { .. }
            | UiAction::UseBrickSlot { .. }
            | UiAction::UseTool { .. }
            | UiAction::UnUseTool
            | UiAction::UseSprayCan { .. }
            | UiAction::UseFxCan { .. }
            | UiAction::SetPrint { .. }
            | UiAction::ClosePrintSelector
            | UiAction::SendWrench { .. }
            | UiAction::RequestEvents { .. }
            | UiAction::SendEvents { .. }
            | UiAction::CancelWrench { .. }
            | UiAction::RespawnVehicle { .. }
            | UiAction::Game(
                GameAction::Held {
                    control: HeldControl::Fire,
                    ..
                } | GameAction::ShiftBrick { .. }
                    | GameAction::SuperShiftBrick { .. }
                    | GameAction::RotateBrick { .. }
                    | GameAction::PlantBrick
                    | GameAction::CancelBrick
                    | GameAction::UndoBrick
                    | GameAction::DropTool
            )
    )
}

fn translucent_ghost(scene: &mut SceneData) {
    // Authored literal quad colors may ignore paint alpha. Apply ghost opacity
    // after building geometry so every surface remains visibly unplanted.
    for vertex in &mut scene.vertices {
        vertex.color[3] *= 0.45;
    }
    for material in &mut scene.materials {
        material.alpha = bri_render::scene::AlphaMode::Blend;
    }
}

fn combine_effect_frames(
    mut world: bri_fx_runtime::FrameEffects,
    others: [bri_fx_runtime::FrameEffects; 2],
    eye: Vec3,
) -> (bri_fx_runtime::FrameEffects, usize) {
    for other in others {
        world.particles.extend(other.particles);
        world.lights.extend(other.lights);
    }
    world.particles.sort_by(|a, b| {
        eye.distance_squared(b.position)
            .total_cmp(&eye.distance_squared(a.position))
    });
    world.lights.sort_by(|a, b| {
        eye.distance_squared(a.position)
            .total_cmp(&eye.distance_squared(b.position))
    });
    let deferred = world
        .lights
        .len()
        .saturating_sub(bri_render::scene::MAX_POINT_LIGHTS);
    world.lights.truncate(bri_render::scene::MAX_POINT_LIGHTS);
    (world, deferred)
}
impl PlatformApp for App {
    fn ui(&self) -> &Ui {
        &self.ui
    }
    fn ui_mut(&mut self) -> &mut Ui {
        &mut self.ui
    }
    fn tick(&mut self, elapsed: Duration) -> Result<()> {
        let mut listener = bri_audio::Listener::default();
        self.animation_time += elapsed.as_secs_f64().min(0.25);
        self.poll_network()?;
        self.poll_files();
        let alive = self.local_alive();
        let observing = self.controls.fly(elapsed.as_secs_f32());
        if let Some(a) = self.attempt.as_ref().filter(|a| a.entered) {
            let input = if alive && !observing {
                self.abilities.apply(self.controls.movement())
            } else {
                // Corpses ignore controls; keep aim so the server agrees.
                bri_sim::player::MoveInput {
                    yaw: self.controls.yaw,
                    pitch: self.controls.pitch,
                    ..Default::default()
                }
            };
            if let Some((newest, inputs)) = self.motion.advance(
                elapsed.as_secs_f32(),
                input,
                bri_net::protocol::MOVEMENT_REDUNDANCY,
            )? {
                a.worker.movement(newest, inputs)?;
            }
            if let Some(view) = &a.view {
                let mounted = view.vitals.get(&view.owner).and_then(|v| v.mounted);
                self.motion.set_mounted(mounted.is_some());
                self.motion
                    .present(view, self.controls.yaw, self.controls.pitch);
                let driven = mounted.filter(|(_, seat)| *seat == 0).map(|(id, _)| id);
                self.vehicles.update(
                    &view.vehicles,
                    &view.vehicle_poses,
                    self.motion.server_tick(),
                    driven,
                );
                // Riders sit exactly on their rendered vehicle's seat.
                for (owner, vitals) in &view.vitals {
                    let Some((vehicle, seat)) = vitals.mounted else {
                        continue;
                    };
                    if let Some(info) = view.vehicles.get(&vehicle)
                        && let Some((feet, yaw)) =
                            self.vehicles
                                .seat(&self.vehicle_assets, info, usize::from(seat))
                    {
                        let velocity = self
                            .vehicles
                            .frame(vehicle)
                            .map_or(Vec3::ZERO, |f| f.velocity);
                        self.motion.override_presented(
                            *owner,
                            feet,
                            yaw,
                            velocity,
                            *owner == view.owner,
                        );
                    }
                }
                self.vehicles.prepare(
                    &mut self.vehicle_assets,
                    &view.vehicles,
                    &view.world.palette,
                );
                if self
                    .music_world
                    .as_ref()
                    .is_none_or(|old| !Arc::ptr_eq(old, &view.world))
                {
                    self.audio.sync_music(&view.world.bricks);
                    self.music_world = Some(view.world.clone());
                }
            }
        }
        self.update_combat_presentation();
        self.update_net_graph();
        if let Some(receiver) = &self.lan_query
            && let Ok(found) = receiver.try_recv()
        {
            self.lan_query = None;
            self.lan_hosts.clear();
            let mut servers = Vec::new();
            for (address, beacon) in found {
                if let Ok(certificate) = beacon.certificate_der() {
                    self.lan_hosts.insert(address.to_string(), certificate);
                    servers.push(ServerInfo {
                        address: address.to_string(),
                        name: plain_chat(&beacon.name),
                        password: false,
                        dedicated: false,
                        ping_ms: None,
                        players: beacon.players,
                        max_players: beacon.max_players,
                        bricks: 0,
                        map: plain_chat(&beacon.map),
                    });
                }
            }
            self.ui.apply(UiUpdate::LanServers {
                servers,
                querying: false,
            });
        }
        // Build macro playback: one recorded building action per frame so the
        // server's action budget is never exceeded.
        if let Some(action) = self.macro_playback.pop_front() {
            self.ui.core.request(action);
        }
        let third_person = self.third_person_view();
        let weapon_checkpoint = self.attempt.as_ref().filter(|a| a.entered).and_then(|a| {
            a.view
                .as_ref()
                .map(|view| (a.id, view.checkpoint_cue_cursor))
        });
        if let Some((session, cursor)) = weapon_checkpoint {
            self.reset_weapon_effect_session(session, cursor);
        }
        if let Some(view) = self
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| a.view.as_ref())
            && let Some(local) = self.motion.presented().get(&view.owner)
            && let Some(meshes) = &self.meshes
            && let Some(building) = &self.building
        {
            let presented = self.motion.presented();
            // Sample every body, including the hidden first-person body, once.
            // Visible geometry and attached items consume these same original nodes.
            Self::update_avatar_animation_inputs(
                &mut self.avatar_actions,
                &mut self.avatar_action_images,
                &mut self.weapon_animation_cues,
                &mut self.weapon_animation_drops,
                view,
                elapsed.as_secs_f32(),
            );
            self.avatars
                .retain(|owner, _| view.poses.contains_key(owner));
            self.avatar_actions
                .retain(|owner, _| view.poses.contains_key(owner));
            for (owner, player) in presented {
                let appearance = view
                    .avatars
                    .get(owner)
                    .unwrap_or(&self.avatar_assets.package.defaults);
                if self
                    .avatars
                    .get(owner)
                    .is_none_or(|mesh| &mesh.appearance != appearance)
                {
                    self.avatars
                        .insert(*owner, self.avatar_assets.mesh(appearance.clone())?);
                }
                let selected_core_tool = view
                    .tools
                    .get(owner)
                    .and_then(|inventory| inventory.selected.and_then(|i| inventory.slots.get(i)))
                    .and_then(Option::as_deref)
                    .filter(|id| bri_weapons::CORE_TOOLS.contains(id));
                let mut ready_hands = Vec::new();
                if let Some(images) = view.weapons.images.get(owner) {
                    for mounted in images {
                        if let Some(image) = self.content.weapons.pack.images.get(&mounted.image) {
                            ready_hands.push((mounted.hand, image.arm_ready));
                        }
                    }
                }
                if let Some(item) = selected_core_tool {
                    // Offline source audit: Hammer, Wrench, and Printer image
                    // datablocks declare armReady=true. Wand remains unaudited.
                    ready_hands.push((0, bri_weapons::CORE_TOOLS[..3].contains(&item)));
                }
                let dead = view.vitals.get(owner).is_some_and(|v| !v.alive);
                let input = crate::avatar::AvatarAnimationInput {
                    held_tool_pose: if dead {
                        crate::avatar::HeldToolPose::None
                    } else {
                        crate::avatar::HeldToolPose::from_mounted_images(ready_hands)
                    },
                    action: self.avatar_actions.get(owner).cloned().filter(|_| !dead),
                    dead,
                    sitting: !dead
                        && (self.combat.sitting.contains(owner)
                            || view
                                .vitals
                                .get(owner)
                                .and_then(|v| v.mounted)
                                .and_then(|(vehicle, seat)| {
                                    let info = view.vehicles.get(&vehicle)?;
                                    let d = self.vehicle_assets.definition(&info.definition)?;
                                    Some(d.seats.get(usize::from(seat))?.pose == "sit")
                                })
                                .unwrap_or(false)),
                };
                self.avatars.get_mut(owner).unwrap().pose_with_animation(
                    &self.avatar_assets,
                    player,
                    self.animation_time,
                    &input,
                )?;
            }
            self.effects.sync(view.world.clone(), meshes)?;
            self.foliage.advance(elapsed);
            // Match the actual view for flare occlusion, including third-person camera collision.
            let (yaw, pitch) = self.controls.view_angles();
            let forward = Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                -yaw.cos() * pitch.cos(),
            );
            let eye = self
                .motion
                .local_eye()
                .unwrap_or_else(|| local.eye(&PlayerTuning::default()));
            let eye = if let Some(camera) = self.controls.free_camera {
                camera
            } else if third_person {
                building.camera_position(
                    eye,
                    forward,
                    Self::vehicle_camera(&self.vehicle_assets, view).unwrap_or(8.),
                )?
            } else {
                eye
            };
            listener = bri_audio::Listener {
                position: eye.to_array(),
                forward: forward.to_array(),
                up: Vec3::Y.to_array(),
            };
            let (local_view_yaw, local_view_pitch) = self.controls.view_angles();
            self.world_items.sync(
                &view.weapons,
                &view.tools,
                crate::world_items::WorldItemFrame {
                    tick: view.tick,
                    seconds: self.animation_time,
                    eye,
                    local_owner: Some(view.owner),
                    first_person: !third_person,
                },
                |owner| {
                    let avatar = self.avatars.get(&owner)?;
                    let player = presented.get(&owner)?;
                    let (yaw, pitch) = if owner == view.owner {
                        (local_view_yaw, local_view_pitch)
                    } else {
                        (player.yaw, player.pitch)
                    };
                    Some(crate::world_items::MountPose {
                        eye: avatar.eye_transform(&self.avatar_assets, yaw, pitch)?,
                        mounts: (0..8)
                            .filter_map(|n| {
                                avatar
                                    .world_node(&self.avatar_assets, &format!("Mount{n}"))
                                    .map(|transform| (n, transform))
                            })
                            .collect(),
                        velocity: Vec3::from_array(player.velocity),
                    })
                },
            )?;
            Self::update_weapon_effect_parts(
                &mut self.weapon_effects,
                &mut self.weapon_cues,
                &self.world_items,
                &view.weapons,
                elapsed.as_secs_f32(),
            )?;
            Self::update_actor_effects(
                &mut self.actor_effects,
                &self.avatar_assets,
                &self.avatars,
                &self.vehicles,
                &self.vehicle_assets,
                view,
                presented,
                elapsed.as_secs_f32(),
            )?;
            self.explosion_shapes.advance(elapsed.as_secs_f32());
            self.audio
                .sync_projectiles(&view.weapons.projectiles, &self.content.weapons.pack);
            // The avatar/image shell and sequence playback APIs are still a host
            // boundary. Retain requests in the adapter and expose its queue-drop
            // diagnostics; do not claim these have been rendered or played.
            self.effects
                .advance(elapsed.as_secs_f32(), eye, Vec3::ZERO, |id, from, to| {
                    building.effect_visible(id, from, to)
                })?;
            let right = forward.cross(Vec3::Y).normalize();
            self.weather.advance(
                elapsed.as_secs_f32(),
                bri_weather::CameraState {
                    position: eye,
                    forward,
                    right,
                    up: right.cross(forward).normalize(),
                    velocity: Vec3::from_array(local.velocity),
                },
                building,
            )?;
        }
        self.audio.tick(elapsed.as_secs_f32(), listener);
        Ok(())
    }
    fn pump(&mut self) -> Result<Vec<PlatformCommand>> {
        for sound in self.ui.drain_sounds() {
            self.audio
                .profile(sound.profile, bri_audio::Placement::Listener);
        }
        let mut platform = Vec::new();
        for (id, action) in self.ui.drain_actions() {
            // Dead players click to respawn; other fire/tool input is ignored.
            if !self.local_alive()
                && matches!(
                    action,
                    UiAction::Game(GameAction::Held {
                        control: HeldControl::Fire,
                        ..
                    })
                )
            {
                if matches!(action, UiAction::Game(GameAction::Held { down: true, .. })) {
                    let ready = self.network_view().is_some_and(|view| {
                        view.vitals
                            .get(&view.owner)
                            .is_some_and(|v| view.tick >= v.respawn_tick)
                    });
                    if ready {
                        if let Err(error) = self.command(id, Command::Respawn, action.clone()) {
                            self.answer(id, Err(error));
                        }
                        continue;
                    }
                }
                self.answer(id, Ok(()));
                continue;
            }
            if self.local_mounted()
                && let UiAction::Game(GameAction::Held {
                    control: HeldControl::Fire,
                    down,
                }) = action
            {
                // Seated fire drives the vehicle weapon (tank, cannon).
                if let Err(error) =
                    self.command(id, Command::WeaponTrigger { down }, action.clone())
                {
                    self.answer(id, Err(error));
                }
                continue;
            }
            if crate::minigame_ui::is_minigame_action(&action) {
                let result = crate::minigame_ui::command(&action).and_then(|command| {
                    match command {
                        Some(command) => self.command(id, command, action.clone()).map(|()| true),
                        None => {
                            self.combat.minigame_state = None; // force a refresh
                            Ok(false)
                        }
                    }
                });
                match result {
                    Ok(true) => {}
                    Ok(false) => self.answer(id, Ok(())),
                    Err(error) => self.answer(id, Err(error)),
                }
                continue;
            }
            if matches!(action, UiAction::OpenAdmin | UiAction::Admin(_)) {
                let admin_action = match action {
                    UiAction::Admin(action) => action,
                    _ => bri_ui::models::admin::AdminAction::Refresh,
                };
                if let Err(error) = self.handle_admin(id, admin_action) {
                    self.answer(id, Err(error));
                }
                continue;
            }
            if let Some(recording) = &mut self.macro_recording
                && macro_action(&action)
                && recording.len() < 4096
            {
                recording.push(action.clone());
            }
            if building_action(&action) {
                match self.handle_building(id, &action) {
                    Ok(true) => {}
                    Ok(false) => self.answer(id, Err(anyhow::anyhow!("Not connected"))),
                    Err(error) => self.answer(id, Err(error)),
                }
                continue;
            }
            let result = match action {
                UiAction::RequestSaveList { .. } | UiAction::LoadBricks { .. } => {
                    let result = (|| {
                        if matches!(action, UiAction::LoadBricks { .. }) {
                            ensure!(
                                self.network_view().is_some_and(|v| v.administrator),
                                "Loading requires host or administrator permission"
                            );
                        }
                        self.file_jobs.enqueue(crate::saves::Request {
                            id,
                            session: self.attempt.as_ref().map(|a| a.id),
                            action,
                            build: None,
                        })
                    })();
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::SaveBricks {
                    ref name,
                    ref description,
                    events,
                    ownership,
                    ..
                } => {
                    let result = (|| {
                        ensure!(
                            crate::saves::valid_name(name),
                            "Invalid native save filename"
                        );
                        ensure!(
                            description.len() <= 64 * 1024,
                            "Save description is too long"
                        );
                        self.command(id, Command::SaveBuild { events, ownership }, action.clone())
                    })();
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::Quit => {
                    self.disconnect();
                    platform.push(PlatformCommand::Quit);
                    continue;
                }
                UiAction::ApplyDisplay {
                    resolution,
                    fullscreen,
                    vsync,
                } => {
                    platform.push(PlatformCommand::ApplyDisplay {
                        request: id,
                        resolution,
                        fullscreen,
                        vsync,
                    });
                    continue;
                }
                UiAction::SaveSettings(value) => {
                    settings::save(&self.state_dir.join("settings.json"), &value).and_then(|()| {
                        self.audio.apply_settings(&value);
                        self.weather.apply_settings(&value)
                    })
                }
                UiAction::SetVolume { channel, value } => self.audio.set_volume(&channel, value),
                UiAction::HostGame {
                    map,
                    mode,
                    max_players,
                    server_name,
                    password,
                    admin_password,
                    super_admin_password,
                } => {
                    if self.ui.session_request() != Some(id) {
                        continue;
                    }
                    let result = self.host(
                        id,
                        map,
                        mode,
                        max_players,
                        server_name,
                        password,
                        admin_password,
                        super_admin_password,
                    );
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::JoinServer { address, password } => {
                    if self.ui.session_request() != Some(id) {
                        continue;
                    }
                    let result = self.join(id, address, password);
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::CancelConnect | UiAction::Disconnect => {
                    if self.attempt.as_ref().is_none_or(|a| a.id <= id) {
                        self.disconnect();
                    }
                    // Core may already contain a newer attempt queued in this
                    // same UI drain. An older cancel must not clear its token.
                    if self.ui.session_request().is_none_or(|token| token <= id) {
                        self.ui.apply(UiUpdate::Connection(ConnectionState::Idle));
                    }
                    Ok(())
                }
                UiAction::Game(GameAction::ToggleFullscreen) => {
                    platform.push(PlatformCommand::ToggleFullscreen);
                    continue;
                }
                UiAction::Game(GameAction::ToggleNetGraph) => {
                    self.net_graph = match self.net_graph {
                        Some(_) => {
                            self.ui.apply(UiUpdate::NetGraph(None));
                            None
                        }
                        None => Some((std::time::Instant::now(), 0)),
                    };
                    Ok(())
                }
                UiAction::Game(GameAction::ToggleBuildMacroRecording) => {
                    let text = match self.macro_recording.take() {
                        Some(recorded) => {
                            let count = recorded.len();
                            self.build_macro = recorded;
                            format!("Build macro saved ({count} actions)")
                        }
                        None => {
                            self.macro_recording = Some(Vec::new());
                            "Recording build macro...".into()
                        }
                    };
                    self.ui.apply(UiUpdate::BottomPrint {
                        text,
                        seconds: 3.0,
                        hide_bar: false,
                    });
                    Ok(())
                }
                UiAction::Game(GameAction::PlayBackBuildMacro) => {
                    if self.macro_recording.is_none() {
                        self.macro_playback.extend(self.build_macro.iter().cloned());
                    }
                    Ok(())
                }
                UiAction::Game(GameAction::Screenshot { kind }) => {
                    let stamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis());
                    platform.push(PlatformCommand::Screenshot {
                        path: self
                            .state_dir
                            .join("screenshots")
                            .join(format!("Blockland_{stamp}.png")),
                        hud: kind == ScreenshotKind::Normal,
                    });
                    Ok(())
                }
                UiAction::Game(GameAction::DropCameraAtPlayer) => {
                    // `serverCmdDropCameraAtPlayer`: administrators only.
                    match self.network_view() {
                        Some(view) if view.administrator => {
                            if self.controls.free_camera.is_some() {
                                self.controls.free_camera = None;
                            } else {
                                self.controls.free_camera = self.motion.local_eye().or_else(|| {
                                    view.poses
                                        .get(&view.owner)
                                        .map(|p| p.player.eye(&PlayerTuning::default()))
                                });
                            }
                            Ok(())
                        }
                        Some(_) => Err(anyhow::anyhow!(
                            "Only administrators can use the free camera"
                        )),
                        None => Err(anyhow::anyhow!("Not connected")),
                    }
                }
                UiAction::Game(GameAction::DropPlayerAtCamera) => {
                    match self.controls.free_camera.take() {
                        Some(eye) => {
                            let result = self.command(
                                id,
                                Command::DropPlayerAt {
                                    eye: eye.to_array(),
                                    yaw: self.controls.yaw,
                                },
                                action.clone(),
                            );
                            if result.is_ok() {
                                continue;
                            }
                            result
                        }
                        None => Ok(()),
                    }
                }
                UiAction::Game(GameAction::NextSeat | GameAction::PrevSeat) => {
                    let step = if matches!(action, UiAction::Game(GameAction::NextSeat)) {
                        1
                    } else {
                        -1
                    };
                    let result = self.command(id, Command::SwitchSeat(step), action.clone());
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::Game(GameAction::Suicide) => {
                    let result = self.command(id, Command::Suicide, action.clone());
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::Game(GameAction::UseLight) => {
                    let result = self.command(id, Command::ToggleLight, action.clone());
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::Game(GameAction::Emote { ref name }) => {
                    let name = name.to_ascii_lowercase();
                    let result = self.command(id, Command::Emote(name), action.clone());
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::Game(action) => {
                    if self.controls.action(&action) {
                        continue;
                    } else {
                        Err(anyhow::anyhow!(
                            "This gameplay action is not connected to the client yet"
                        ))
                    }
                }
                UiAction::Chat {
                    channel: ChatChannel::Say,
                    text,
                } => {
                    let result = self.command(
                        id,
                        Command::Chat(text.clone()),
                        UiAction::Chat {
                            channel: ChatChannel::Say,
                            text,
                        },
                    );
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::Chat {
                    channel: ChatChannel::Team,
                    text,
                } => {
                    let result = self.command(
                        id,
                        Command::TeamChat(text.clone()),
                        UiAction::Chat {
                            channel: ChatChannel::Team,
                            text,
                        },
                    );
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::ChatCommand { ref name, .. } => {
                    // Vanilla slash commands that map to existing requests.
                    let command = match name.to_ascii_lowercase().as_str() {
                        "suicide" | "kill" => Some(Command::Suicide),
                        "light" => Some(Command::ToggleLight),
                        "clearcheckpoint" => Some(Command::ClearCheckpoint),
                        "treasurestatus" => Some(Command::TreasureStatus),
                        "sit" | "love" | "hate" | "alarm" | "confusion" => {
                            Some(Command::Emote(name.to_ascii_lowercase()))
                        }
                        _ => None,
                    };
                    match command {
                        Some(command) => {
                            let result = self.command(id, command, action.clone());
                            if result.is_ok() {
                                continue;
                            }
                            result
                        }
                        None => Err(anyhow::anyhow!("Unknown command: /{name}")),
                    }
                }
                UiAction::StartTutorial => {
                    if self.ui.session_request() != Some(id) {
                        continue;
                    }
                    let result = self.host(
                        id,
                        "v20/add-ons/map_tutorial/tutorial.mis".into(),
                        ServerMode::SinglePlayer,
                        1,
                        "Tutorial".into(),
                        String::new(),
                        String::new(),
                        String::new(),
                    );
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::StartTyping
                | UiAction::StopTyping
                | UiAction::ClosePrintSelector
                | UiAction::CancelWrench { .. } => Ok(()),
                UiAction::SetAvatar(ref prefs) => {
                    let connected = self.network_view().is_some();
                    let result = self.avatar_assets.from_prefs(prefs).and_then(|appearance| {
                        if connected {
                            self.command(id, Command::Avatar(appearance), action.clone())
                        } else {
                            Ok(())
                        }
                    });
                    if connected && result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::PreviewAvatar {
                    avatar,
                    camera_rotation,
                    orbit_distance,
                } => self
                    .avatar_assets
                    .from_prefs(&avatar)
                    .and_then(|appearance| {
                        ensure!(
                            camera_rotation.iter().all(|v| v.is_finite())
                                && orbit_distance.is_finite()
                                && (1.0..=20.0).contains(&orbit_distance),
                            "Invalid avatar preview camera"
                        );
                        self.preview_request = Some((appearance, camera_rotation, orbit_distance));
                        self.preview_dirty = true;
                        Ok(())
                    }),
                UiAction::QueryLan => {
                    let (send, receive) = mpsc::sync_channel(1);
                    self.runtime.spawn(async move {
                        let found = bri_net::discovery::query(
                            &[bri_net::discovery::broadcast()],
                            Duration::from_millis(1200),
                        )
                        .await
                        .unwrap_or_default();
                        let _ = send.send(found);
                    });
                    self.lan_query = Some(receive);
                    self.ui.apply(UiUpdate::LanServers {
                        servers: vec![],
                        querying: true,
                    });
                    Ok(())
                }
                _ => Err(anyhow::anyhow!(
                    "This feature is not connected to native gameplay yet. It remains required before the alpha handoff."
                )),
            };
            self.answer(id, result);
        }
        Ok(platform)
    }
    fn gpu_ready(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Result<()> {
        self.item_ui.gpu_stopped();
        self.world_items.clear_gpu();
        crate::vehicles::ClientVehicles::gpu_stopped(&mut self.vehicle_assets);
        self.explosion_shapes.gpu_stopped();
        for avatar in self.avatars.values_mut() {
            avatar.gpu = None;
        }
        self.avatar_preview = Some(crate::avatar::Preview::new(device));
        self.preview_dirty = self.preview_request.is_some();
        self.renderer = Some(SceneRenderer::new(device, format));
        self.foliage.gpu_stopped();
        let weather_limits = bri_weather::WeatherLimits::default();
        self.weather_renderer = Some(bri_weather::gpu::WeatherRenderer::new(
            device,
            queue,
            self.weather.world.pack(),
            format,
            bri_render::scene::DEPTH_FORMAT,
            1,
            weather_limits.drops + weather_limits.splashes,
        )?);
        let limits = bri_fx_runtime::EffectsLimits::default();
        self.effects_renderer = Some(bri_fx_runtime::gpu::EffectsRenderer::new(
            device,
            queue,
            self.weapon_effects.world().pack(),
            format,
            bri_render::scene::DEPTH_FORMAT,
            1,
            limits.particles.saturating_mul(2) + limits.lights.saturating_mul(2),
        )?);
        self.gpu_scene = None;
        self.gpu_terrain.clear();
        self.gpu_world = None;
        self.ghost_gpu = None;
        self.ghost_uploaded = u64::MAX;
        self.depth = None;
        Ok(())
    }
    fn gpu_stopped(&mut self) {
        self.item_ui.gpu_stopped();
        self.world_items.clear_gpu();
        crate::vehicles::ClientVehicles::gpu_stopped(&mut self.vehicle_assets);
        self.explosion_shapes.gpu_stopped();
        for avatar in self.avatars.values_mut() {
            avatar.gpu = None;
        }
        self.avatar_preview = None;
        self.renderer = None;
        self.foliage.gpu_stopped();
        self.weather_renderer = None;
        self.effects_renderer = None;
        self.gpu_scene = None;
        self.gpu_terrain.clear();
        self.gpu_world = None;
        self.ghost_gpu = None;
        self.ghost_uploaded = u64::MAX;
        self.depth = None;
    }
    fn render_scene(&mut self, frame: &mut RenderContext<'_>) -> Result<bool> {
        self.item_ui.register_icons(frame);
        if self.preview_dirty
            && let Some((appearance, rotation, distance)) = &self.preview_request
        {
            self.avatar_preview
                .as_mut()
                .context("Avatar preview GPU not initialized")?
                .render(&self.avatar_assets, appearance, *rotation, *distance, frame)?;
            self.ui.apply(UiUpdate::AvatarPreview(IconRef::External(
                crate::avatar::Preview::ID,
            )));
            self.preview_dirty = false;
        }

        let Some(a) = self.attempt.as_ref().filter(|a| a.entered) else {
            return Ok(false);
        };
        let Some(scene) = &self.cpu_scene else {
            return Ok(false);
        };
        let Some(view) = &a.view else {
            return Ok(false);
        };
        let Some(local) = self.motion.presented().get(&view.owner) else {
            return Ok(false);
        };
        let third_person = self.controls.third_person
            || self.controls.free_camera.is_some()
            || view.vitals.get(&view.owner).is_some_and(|v| !v.alive);
        let hidden = self.combat.hidden_bodies(&view.vitals);
        let lights_on: Vec<Vec3> = view
            .vitals
            .iter()
            .filter(|(_, v)| v.light && v.alive)
            .filter_map(|(owner, _)| self.motion.presented().get(owner))
            .map(|p| Vec3::from(p.feet) + Vec3::Y * 2.6 + p.forward() * 0.5)
            .collect();
        let renderer = self
            .renderer
            .as_mut()
            .context("Scene GPU not initialized")?;
        if self.gpu_scene.is_none() {
            self.gpu_scene = Some(renderer.upload(frame.device, frame.queue, scene)?);
            self.gpu_terrain = self
                .cpu_terrain
                .iter()
                .map(|terrain| {
                    bri_render::terrain_scene::GpuTerrain::upload(
                        renderer,
                        frame.device,
                        frame.queue,
                        terrain.clone(),
                        FAR_PLANE,
                    )
                })
                .collect::<Result<_>>()?;
        }
        if self.gpu_world.is_none()
            && let Some(world) = &self.cpu_world
            && !world.indices.is_empty()
        {
            self.gpu_world = Some(renderer.upload(frame.device, frame.queue, world)?);
        }
        if let Some(building) = &self.building
            && self.ghost_uploaded != building.ghost_generation()
        {
            self.ghost_gpu = None;
            if let Some(ghost) = building.ghost() {
                let palette = view.world.palette.clone();
                let world = bri_net::protocol::PublicWorld {
                    name: "Local unplanted ghost".into(),
                    map_id: view.world.map_id.clone(),
                    palette,
                    bricks: [(0, ghost.clone())].into(),
                };
                let mut data = crate::world_scene::build_world_scene_materials(
                    &world,
                    self.meshes.as_ref().context("Ghost mesh catalog missing")?,
                    100_000,
                    Some(
                        self.materials
                            .as_ref()
                            .context("Ghost material catalog missing")?,
                    ),
                )?;
                translucent_ghost(&mut data);
                if !data.indices.is_empty() {
                    self.ghost_gpu = Some(renderer.upload(frame.device, frame.queue, &data)?);
                }
            }
            self.ghost_uploaded = building.ghost_generation();
        }
        if self
            .depth
            .as_ref()
            .is_none_or(|(_, size)| *size != frame.size)
        {
            self.depth = Some((
                create_depth(frame.device, frame.size.0, frame.size.1),
                frame.size,
            ));
        }
        for (owner, avatar) in &mut self.avatars {
            if (*owner != view.owner || third_person) && !hidden.contains(owner) {
                avatar.upload(renderer, frame.device, frame.queue)?;
            }
        }
        self.world_items
            .upload(renderer, frame.device, frame.queue)?;
        crate::vehicles::ClientVehicles::upload(
            &mut self.vehicle_assets,
            renderer,
            frame.device,
            frame.queue,
        )?;
        self.explosion_shapes
            .upload(renderer, frame.device, frame.queue)?;
        let (yaw, pitch) = self.controls.view_angles();
        let pitch = pitch.clamp(-1.56, 1.56);
        let forward = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        );
        let eye = self
            .motion
            .local_eye()
            .unwrap_or_else(|| local.eye(&PlayerTuning::default()));
        let camera_distance = Self::vehicle_camera(&self.vehicle_assets, view).unwrap_or(8.0);
        let eye = if let Some(camera) = self.controls.free_camera {
            camera
        } else if third_person {
            self.building
                .as_ref()
                .context("Camera collision mirror missing")?
                .camera_position(eye, forward, camera_distance)?
        } else {
            eye
        };
        // Explosion `CameraShake`: 10 degrees of view rotation per unit of offset.
        let shake = self.actor_effects.camera_shake(eye) * 10f32.to_radians();
        let forward = if shake == Vec3::ZERO {
            forward
        } else {
            let yaw = yaw + shake.z.clamp(-0.3, 0.3);
            let pitch = (pitch + shake.x.clamp(-0.3, 0.3)).clamp(-1.56, 1.56);
            Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos())
        };
        let mut camera = Camera::perspective(
            eye.to_array(),
            (eye + forward).to_array(),
            frame.size.0 as f32 / frame.size.1 as f32,
            self.controls.fov(90.0).to_radians(),
            0.05,
            FAR_PLANE,
        );
        camera.apply_environment(scene);
        camera.atmosphere[2] = (self.animation_time % 86400.0) as f32;
        renderer.update_camera(frame.queue, &camera);
        let right = forward.cross(Vec3::Y).normalize();
        let effects_camera = bri_fx_runtime::Camera {
            view_projection: glam::Mat4::from_cols_array(&camera.view_projection),
            position: eye,
            right,
            up: right.cross(forward).normalize(),
        };
        let world_frame = self.effects.world.snapshot(&effects_camera);
        let weapon_frame = self.weapon_effects.world().snapshot(&effects_camera);
        let actor_frame = self.actor_effects.world().snapshot(&effects_camera);
        let (effects_frame, deferred_lights) =
            combine_effect_frames(world_frame, [weapon_frame, actor_frame], eye);
        let (fog_start, fog_end) = if camera.atmosphere[3] > 0. {
            (camera.atmosphere[0], camera.atmosphere[1])
        } else {
            (FAR_PLANE, FAR_PLANE + 1.)
        };
        for terrain in &mut self.gpu_terrain {
            terrain.update(frame.queue, eye, fog_end.max(1.))?;
        }
        self.foliage.prepare(
            frame,
            &bri_foliage::Camera {
                position: eye,
                right,
                view_projection: effects_camera.view_projection,
                visible_distance: fog_end.max(1.),
            },
            fog_start,
            fog_end.max(fog_start + 0.001),
        )?;
        self.weapon_light_deferred = deferred_lights;
        // `serverCmdLight` player lights first: they are always near the
        // camera and must not be displaced by distant effect lights.
        let mut lights: Vec<_> = lights_on
            .iter()
            .map(|position| bri_render::scene::PointLight {
                position_radius: position.extend(12.0).to_array(),
                color: [1.0, 1.0, 1.0, 0.0],
            })
            .collect();
        lights.extend(
            effects_frame
                .lights
                .iter()
                .map(|light| bri_render::scene::PointLight {
                    position_radius: light.position.extend(light.radius).to_array(),
                    color: light.color.extend(0.).to_array(),
                }),
        );
        lights.truncate(bri_render::scene::MAX_POINT_LIGHTS);
        renderer.update_lights(frame.queue, &lights)?;
        let effects_renderer = self
            .effects_renderer
            .as_mut()
            .context("Effects GPU not initialized")?;
        effects_renderer.prepare(frame.queue, &effects_camera, &effects_frame)?;
        let weather_renderer = self
            .weather_renderer
            .as_mut()
            .context("Weather GPU not initialized")?;
        weather_renderer.prepare(
            frame.queue,
            effects_camera.view_projection,
            &self.weather.world.snapshot(),
        )?;
        let depth = self
            .depth
            .as_ref()
            .unwrap()
            .0
            .create_view(&Default::default());
        let [r, g, b, a] = scene.clear_color.map(f64::from);
        let mut scenes = vec![self.gpu_scene.as_ref().unwrap()];
        if let Some(world) = &self.gpu_world {
            scenes.push(world);
        }
        if let Some(ghost) = &self.ghost_gpu {
            scenes.push(ghost);
        }
        for (owner, avatar) in &self.avatars {
            if (*owner != view.owner || third_person)
                && !hidden.contains(owner)
                && let Some(gpu) = &avatar.gpu
            {
                scenes.push(gpu);
            }
        }
        let mut item_draws = self.world_items.draws();
        item_draws.extend(crate::vehicles::ClientVehicles::draws(&self.vehicle_assets));
        item_draws.extend(self.gpu_terrain.iter().flat_map(|t| t.draws()));
        item_draws.extend(self.explosion_shapes.draws());
        renderer.render_with_instances(
            frame.encoder,
            frame.target,
            &depth,
            &scenes,
            &item_draws,
            Some(wgpu::Color { r, g, b, a }),
        );
        let mut pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("replicated world particles and flares"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: frame.target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        self.foliage.render(&mut pass);
        effects_renderer.render(&mut pass);
        weather_renderer.render(&mut pass);
        Ok(true)
    }
}
#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires generated native content; no window, GPU or audio device"]
    fn app_weapon_effect_path_consumes_cues_once_and_syncs_projectile_trails() -> anyhow::Result<()>
    {
        use super::*;
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let state = workspace
            .join("target")
            .join(format!("weapon-fx-app-{stamp}"));
        let mut app = App::load(&workspace.join("content"), &state, (320, 240))?;
        let trail = app
            .content
            .weapons
            .pack
            .projectiles
            .values()
            .find(|projectile| !projectile.trail.is_empty())
            .context("Native weapon pack has no projectile trails")?;
        let mut view = bri_sim::session::WeaponView::default();
        view.projectiles.push(bri_weapons::Projectile {
            id: 1,
            definition: trail.id.clone(),
            source: bri_weapons::ActorId(1),
            position: Vec3::new(0., 1., 0.),
            velocity: Vec3::NEG_Z,
            scale: 1.,
            age: 0,
            bounced: false,
            stuck: false,
            origin: Vec3::ZERO,
            was_thrown: false,
        });
        let emitter = app
            .weapon_effects
            .world()
            .pack()
            .library
            .emitters
            .iter()
            .find(|emitter| emitter.lifetime > 0.)
            .context("Native effects pack has no finite emitter")?
            .id
            .clone();
        let cue = bri_sim::presentation::Cue {
            id: 1,
            tick: 1,
            position: [0., 1., 0.],
            kind: bri_sim::presentation::CueKind::WeaponEffect {
                source: bri_weapons::TargetId::Map(0),
                definition: emitter,
                node: String::new(),
                seconds: 0.,
                image: None,
                hand: None,
                direction: None,
                scale: 1.,
            },
        };
        app.queue_weapon_cue(cue.clone());
        app.update_weapon_effects(&view, 0.1)?;
        assert_eq!(app.weapon_effects.cue_cursor(), 1);
        assert_eq!(app.weapon_effect_diagnostics().accepted_cues, 1);
        assert!(app.weapon_effects.world().particle_count() > 0);
        let accepted = app.weapon_effect_diagnostics().accepted_cues;
        app.queue_weapon_cue(cue.clone());
        app.update_weapon_effects(&view, 0.)?;
        assert_eq!(app.weapon_effect_diagnostics().accepted_cues, accepted);
        assert_eq!(app.weapon_effect_diagnostics().duplicate_cues, 1);
        assert_eq!(app.weapon_effects.attachment_count(), 1);

        let attached = bri_sim::presentation::Cue {
            id: 2,
            tick: 2,
            position: [0., 1., 0.],
            kind: bri_sim::presentation::CueKind::WeaponEffect {
                source: bri_weapons::TargetId::Actor(bri_weapons::ActorId(1)),
                definition: app
                    .weapon_effects
                    .world()
                    .pack()
                    .library
                    .emitters
                    .iter()
                    .find(|emitter| emitter.lifetime > 0.)
                    .unwrap()
                    .id
                    .clone(),
                node: "muzzlePoint".into(),
                seconds: 0.2,
                image: Some("v20.image.missing-pose-test".into()),
                hand: Some(0),
                direction: None,
                scale: 1.,
            },
        };
        app.queue_weapon_cue(attached.clone());
        app.update_weapon_effects(&view, 0.1)?;
        assert_eq!(
            app.weapon_effect_backlog().0,
            1,
            "cue waits for a sampled mount"
        );
        assert_eq!(app.weapon_effects.cue_cursor(), 1);
        app.update_weapon_effects(&view, 0.25)?;
        app.update_weapon_effects(&view, 0.25)?;
        assert_eq!(app.weapon_effect_backlog().0, 0);
        assert_eq!(app.weapon_effects.cue_cursor(), 2);
        assert_eq!(app.weapon_effect_diagnostics().missing_poses, 1);
        app.queue_weapon_cue(attached);
        app.update_weapon_effects(&view, 0.)?;
        assert_eq!(app.weapon_effect_diagnostics().missing_poses, 1);
        assert_eq!(app.weapon_effect_backlog().0, 1);
        app.update_weapon_effects(&view, 0.25)?;
        app.update_weapon_effects(&view, 0.25)?;
        assert_eq!(app.weapon_effect_diagnostics().duplicate_cues, 2);

        app.reset_weapon_effect_session(999, 2);
        app.queue_weapon_cue(cue);
        app.update_weapon_effects(&view, 0.)?;
        assert_eq!(app.weapon_effects.cue_cursor(), 2);
        assert_eq!(app.weapon_effect_diagnostics().duplicate_cues, 1);
        assert_eq!(app.weapon_effects.attachment_count(), 1);
        app.disconnect();
        assert_eq!(app.weapon_effects.cue_cursor(), 0);
        assert_eq!(app.weapon_effects.world().source_count(), 0);
        Ok(())
    }

    #[test]
    #[ignore = "requires generated native content and loopback QUIC; no window, GPU or audio device"]
    fn native_weapon_catalog_startup_and_headless_host() -> anyhow::Result<()> {
        use super::*;
        use std::time::Instant;
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let state = workspace
            .join("target")
            .join(format!("weapon-startup-{stamp}"));
        let mut app = App::load(&workspace.join("content"), &state, (960, 720))?;
        assert!(
            app.content
                .paths
                .effects_runtime
                .ends_with("effects-runtime-pack-002")
        );
        assert_eq!(app.tool_ui.server_catalog().items.len(), 21);
        assert_eq!(app.content.datablocks["ItemData"].len(), 21);
        assert_eq!(app.content.weapons.pack.items.len(), 17);
        assert_eq!(app.content.item_physics.bounds.len(), 21);
        app.ui.core.request(UiAction::HostGame {
            map: "v20/add-ons/map_bedroom/bedroom.mis".into(),
            mode: ServerMode::SinglePlayer,
            max_players: 1,
            server_name: "Weapon catalog test".into(),
            password: String::new(),
            admin_password: "headless-admin-fixture".into(),
            super_admin_password: "headless-super-fixture".into(),
        });
        let start = Instant::now();
        let mut previous = start;
        loop {
            let now = Instant::now();
            app.tick(now.duration_since(previous))?;
            app.ui
                .update(now.duration_since(previous).as_millis() as u64);
            previous = now;
            ensure!(app.pump()?.is_empty(), "Unexpected native window command");
            if let ConnectionState::Failed { reason } = &app.ui.core.conn {
                anyhow::bail!("Headless startup failed: {reason}");
            }
            if let Some(view) = app.network_view()
                && let Some(inventory) = view.tools.get(&view.owner)
            {
                for (slot, expected) in bri_weapons::CORE_TOOLS[..3].iter().enumerate() {
                    assert_eq!(inventory.slots[slot].as_deref(), Some(*expected));
                }
                break;
            }
            ensure!(
                start.elapsed() < Duration::from_secs(90),
                "Headless host timed out"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        fn until(app: &mut App, ready: impl Fn(&App) -> bool) -> anyhow::Result<()> {
            let start = Instant::now();
            let mut previous = start;
            loop {
                let now = Instant::now();
                app.tick(now.duration_since(previous))?;
                app.ui
                    .update(now.duration_since(previous).as_millis() as u64);
                previous = now;
                ensure!(app.pump()?.is_empty(), "Unexpected native window command");
                if ready(app) {
                    return Ok(());
                }
                ensure!(
                    start.elapsed() < Duration::from_secs(10),
                    "Inventory action timed out: {:?}",
                    app.ui.core.conn
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        until(&mut app, |a| a.ui.core.admin.snapshot.is_some())?;
        assert_eq!(
            app.ui.core.admin.snapshot.as_ref().unwrap().role,
            bri_ui::models::admin::AdminRole::SuperAdmin
        );
        assert!(
            app.ui
                .core
                .admin
                .snapshot
                .as_ref()
                .unwrap()
                .players
                .iter()
                .any(|player| player.owner && player.persistent_identity)
        );
        let stored_identity =
            bri_identity::ClientIdentity::load_or_create(state.join("client.identity"))?;
        let original_public_key = *stored_identity.public_key();
        app.ui.core.request(UiAction::OpenAdmin);
        until(&mut app, |a| {
            !a.ui.core.admin.busy() && a.ui.stack().contains(&ScreenId::Admin)
        })?;
        app.ui
            .core
            .admin_request(bri_ui::models::admin::AdminAction::RequestBrickGroups)
            .context("Host could not request original brick management list")?;
        until(&mut app, |a| {
            !a.ui.core.admin.busy() && a.pending_requests() == 0
        })?;
        assert!(app.ui.core.admin.groups.is_empty());
        app.ui
            .core
            .admin_request(bri_ui::models::admin::AdminAction::RequestBans)
            .context("Host could not request persistent ban list")?;
        until(&mut app, |a| {
            !a.ui.core.admin.busy() && a.pending_requests() == 0
        })?;
        assert!(app.ui.core.admin.bans.is_empty());
        assert!(
            app.ui.core.admin.status.is_empty(),
            "Ban query was not accepted: {}",
            app.ui.core.admin.status
        );
        app.ui
            .core
            .admin_request(bri_ui::models::admin::AdminAction::SetPassword {
                slot: bri_ui::models::admin::AdminPasswordSlot::Admin,
                password: bri_ui::models::admin::AdminSecret("changed-admin-fixture".into()),
            })
            .context("Host could not change administrator password")?;
        until(&mut app, |a| {
            !a.ui.core.admin.busy() && a.pending_requests() == 0
        })?;
        assert!(app.ui.core.admin.status.contains("accepted"));
        assert!(
            !app.ui
                .core
                .admin
                .available(bri_ui::models::admin::AdminFeature::Ban)
        );
        assert_eq!(app.ui.core.hud.tools.len(), 5);
        assert_eq!(
            app.ui.core.hud.tools[2].as_ref().unwrap().id,
            bri_weapons::CORE_TOOLS[2]
        );
        assert!(matches!(
            app.ui.core.hud.tools[2].as_ref().unwrap().icon,
            IconRef::External(_)
        ));
        app.ui.core.request(UiAction::UseTool { slot: 2 });
        until(&mut app, |a| {
            a.network_view()
                .is_some_and(|v| v.tools[&v.owner].selected == Some(2))
                && a.pending_requests() == 0
        })?;
        assert_eq!(app.ui.core.hud.tool_name, "Printer");
        app.ui.core.request(UiAction::UseTool { slot: 1 });
        until(&mut app, |a| {
            a.network_view()
                .is_some_and(|v| v.tools[&v.owner].selected == Some(1))
                && a.pending_requests() == 0
        })?;
        assert_eq!(app.ui.core.hud.tool_name, "Wrench");
        let owner = app.network_view().unwrap().owner;
        assert!(app.world_items.instances().any(|(identity, _)| {
            identity == crate::world_items::ItemIdentity::Mounted(owner, 0)
        }));
        assert_eq!(app.world_item_stats().missing_poses, 0);
        assert_eq!(app.world_item_stats().missing_bindings, 0);
        // A state-machine transition on the SAME mounted image must not cancel
        // the authored avatar thread; changing the actual equipment must.
        let mut animation_view = app.network_view().unwrap().clone();
        animation_view.tools.get_mut(&owner).unwrap().selected = None;
        animation_view.weapons.images.insert(
            owner,
            vec![bri_sim::session::MountedImage {
                image: "v20.image.gunimage".into(),
                state: "Fire".into(),
                hand: 0,
            }],
        );
        let mut animations = BTreeMap::new();
        let mut identities = BTreeMap::new();
        let mut cues = VecDeque::from([(
            bri_sim::presentation::Cue {
                id: 1,
                tick: animation_view.tick,
                position: [0.; 3],
                kind: bri_sim::presentation::CueKind::WeaponAnimation {
                    actor: owner,
                    thread: 2,
                    sequence: "armattack".into(),
                    image_hand: None,
                },
            },
            0.,
            10.,
        )]);
        let mut discarded = 0;
        App::update_avatar_animation_inputs(
            &mut animations,
            &mut identities,
            &mut cues,
            &mut discarded,
            &animation_view,
            0.01,
        );
        assert_eq!(animations[&owner].started_at, 10.);
        animation_view.weapons.images.get_mut(&owner).unwrap()[0].state = "Smoke".into();
        App::update_avatar_animation_inputs(
            &mut animations,
            &mut identities,
            &mut cues,
            &mut discarded,
            &animation_view,
            0.01,
        );
        assert_eq!(animations[&owner].started_at, 10.);
        animation_view.weapons.images.get_mut(&owner).unwrap()[0].image =
            "v20.image.bowimage".into();
        App::update_avatar_animation_inputs(
            &mut animations,
            &mut identities,
            &mut cues,
            &mut discarded,
            &animation_view,
            0.01,
        );
        assert!(animations.is_empty());
        app.ui.core.request(UiAction::Game(GameAction::DropTool));
        until(&mut app, |a| {
            a.network_view()
                .is_some_and(|v| v.tools[&v.owner].slots[1].is_none())
                && a.pending_requests() == 0
        })?;
        assert!(app.ui.core.hud.tools[1].is_none());
        assert!(
            app.network_view()
                .unwrap()
                .weapons
                .drops
                .iter()
                .any(|d| d.item == bri_weapons::CORE_TOOLS[1])
        );
        let dropped = app.network_view().unwrap().weapons.drops.last().unwrap().id;
        assert!(
            app.world_items.instances().any(|(identity, _)| {
                identity == crate::world_items::ItemIdentity::Drop(dropped)
            })
        );
        assert!(!app.world_items.instances().any(|(identity, _)| {
            identity == crate::world_items::ItemIdentity::Mounted(owner, 0)
        }));
        assert_eq!(app.world_item_stats().missing_poses, 0);
        assert_eq!(app.world_item_stats().missing_bindings, 0);
        app.disconnect();
        assert!(app.network_view().is_none());
        assert!(app.ui.core.admin.snapshot.is_none());
        assert_eq!(
            *bri_identity::ClientIdentity::load_or_create(state.join("client.identity"))?
                .public_key(),
            original_public_key,
        );
        assert_eq!(app.world_item_stats().cached_models, 0);
        assert_eq!(app.world_item_stats().geometry_slots, 0);
        assert!(app.world_items.instances().next().is_none());
        assert!(app.ui.core.hud.tools.iter().all(Option::is_none));
        assert!(!app.ui.core.hud.tool_active);
        assert!(app.building.is_none());
        Ok(())
    }
    #[test]
    fn ghost_opacity_includes_literal_authored_faces_without_changing_color() {
        let mut scene = bri_render::scene::SceneData::default();
        scene
            .materials
            .push(bri_render::scene::Material::vertex_lit("literal", 0));
        for alpha in [1.0, 0.5] {
            scene.vertices.push(bri_render::scene::SceneVertex {
                position: [0.0; 3],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0; 2],
                lightmap_uv: [0.0; 2],
                color: [0.8, 0.6, 0.2, alpha],
            });
        }
        super::translucent_ghost(&mut scene);
        assert_eq!(scene.vertices[0].color, [0.8, 0.6, 0.2, 0.45]);
        assert_eq!(scene.vertices[1].color[3], 0.225);
        assert_eq!(
            scene.materials[0].alpha,
            bri_render::scene::AlphaMode::Blend
        );
    }
    #[test]
    fn world_and_weapon_effects_share_depth_order_and_nearest_light_budget() {
        use glam::Vec3;
        let particle = |z, texture| bri_fx_runtime::ParticleInstance {
            position: Vec3::new(0., 0., z),
            size: 1.,
            color: Vec3::ONE.extend(1.),
            spin: 0.,
            axis: Vec3::ZERO,
            texture,
            blend: bri_fx_runtime::BlendMode::Alpha,
            depth_test: true,
        };
        let light = |id, x| bri_fx_runtime::LightSnapshot {
            handle: bri_fx_runtime::EffectHandle(id),
            position: Vec3::new(x, 0., 0.),
            color: Vec3::ONE,
            radius: 1.,
        };
        let world = bri_fx_runtime::FrameEffects {
            particles: vec![particle(-3., 2)],
            lights: (0..bri_render::scene::MAX_POINT_LIGHTS)
                .map(|i| light(i as u64, 1000. + i as f32))
                .collect(),
        };
        let weapon = bri_fx_runtime::FrameEffects {
            particles: vec![particle(-1., 7)],
            lights: vec![light(9000, 1.)],
        };
        let actor = bri_fx_runtime::FrameEffects {
            particles: vec![],
            lights: vec![],
        };
        let (combined, deferred) =
            super::combine_effect_frames(world, [weapon, actor], Vec3::ZERO);
        assert_eq!(combined.particles[0].texture, 2);
        assert_eq!(combined.particles[1].texture, 7);
        assert_eq!(combined.lights.len(), bri_render::scene::MAX_POINT_LIGHTS);
        assert_eq!(combined.lights[0].handle.0, 9000);
        assert_eq!(deferred, 1);
    }
    #[test]
    fn remote_chat_cannot_inject_color_stack_or_markup() {
        assert_eq!(
            super::plain_chat("<color:ff0000>A\u{e003}B\u{e00b}C\u{e00c}\n"),
            "‹color:ff0000›ABC"
        );
    }
}
