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
    scene::{Camera, GpuScene, SceneData, SceneRenderer, create_depth_samples},
    scene_loader::load_map_bundle,
};
use bri_sim::{
    definitions::Definitions,
    session::{Command, InspectMode, Reply, Session, ToolAction},
};
use bri_ui::{
    api::*,
    binds::Platform,
    screens::ScreenId,
    ui::{Ui, UiConfig},
};
use bri_vehicles::schema::SeatRole;
use glam::Vec3;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
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
    palette: Arc<crate::world_chunks::BrickPalette>,
    building: crate::building::Building,
    mirror: bri_sim::prediction::CollisionMirror,
}
/// A background chunk update: the replica revision it reached, and the
/// chunk state handed back with the rebuilt chunks.
type WorldRender = (
    Arc<bri_net::protocol::PublicWorld>,
    u64,
    Arc<network::WorldLog>,
    std::result::Result<
        (
            crate::world_chunks::ChunkedWorld,
            crate::world_chunks::ChunkChanges,
        ),
        String,
    >,
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
    /// Players typing in the chat box, in the order they started.
    talking: Vec<bri_world::OwnerId>,
    /// Internet hosts: router port-forwarding outcomes for the host player.
    router: Option<mpsc::Receiver<String>>,
    /// How this player trusts each other player (`secureClientCmd_ClientTrust`).
    trust: BTreeMap<bri_world::OwnerId, bri_sim::session::PlayerTrust>,
    /// Loading the map the host changed to failed.
    map_failure: Option<mpsc::Receiver<String>>,
    /// The loading screen covers a map change until the new map renders.
    reloading: bool,
}
struct PendingAction {
    action: UiAction,
    command: Option<Command>,
    dialog_epoch: u64,
    inspection: Option<InspectMode>,
    dialog_request: bool,
}
/// Everything a client needs to show and predict on `map` (joins and map changes).
fn prepare_map(
    paths: &crate::content::ContentPaths,
    map: &str,
    selected: Vec<(String, u8)>,
    catalog: &bri_sim::session::ToolCatalog,
) -> Result<Prepared> {
    let map = map.to_owned();
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
    let palette = Arc::new(crate::world_chunks::BrickPalette::new(&materials)?);
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
        palette,
        building,
        mirror,
    })
}
/// Everything a host installs in a map's session; kept to build the next
/// map's session when an administrator changes maps.
struct HostSetup {
    lan: bool,
    catalog: bri_sim::session::ToolCatalog,
    weapon_pack: bri_weapons::Pack,
    item_bounds: BTreeMap<String, bri_weapons::ItemBounds>,
    avatar_catalog: bri_content::avatar::Package,
    vehicle_pack: bri_vehicles::Pack,
    event_catalog: bri_events::Catalog,
    event_sounds: Vec<String>,
    maps: Vec<bri_sim::session::MapListing>,
}
impl HostSetup {
    fn session(&self, loaded: crate::content::LoadedMap) -> Result<Session> {
        let mut session = Session::new(loaded.simulation);
        session.set_lan_host(self.lan);
        session.set_tool_catalog(self.catalog.clone())?;
        session.set_weapon_pack(self.weapon_pack.clone())?;
        session.set_item_bounds(self.item_bounds.clone())?;
        session.set_avatar_catalog(self.avatar_catalog.clone())?;
        session.set_vehicle_pack(self.vehicle_pack.clone())?;
        session.set_event_catalog(self.event_catalog.clone(), self.event_sounds.clone())?;
        session.set_spawn_points(loaded.spawn_points)?;
        session.set_map_list(self.maps.clone())?;
        if let Some(tutorial) = loaded.tutorial {
            session.set_tutorial(tutorial)?;
        }
        Ok(session)
    }
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
    /// Ejected gun casings (`stateEjectShell`) and their GPU model.
    weapon_shells: crate::weapon_debris::WeaponDebris,
    shell_gpu: Option<(GpuScene, bri_render::scene::GpuInstances)>,
    weapon_cues: VecDeque<(bri_sim::presentation::Cue, f32)>,
    weapon_cue_drops: u64,
    /// Killed-brick debris (v20 brick explosions) and its GPU models.
    brick_debris: crate::brick_debris::BrickDebris,
    debris_models: crate::brick_debris::DebrisModels,
    brick_kills: Vec<bri_sim::presentation::Cue>,
    /// Non-rendering bricks, drawn only while a building tool is out, and
    /// whether the uploaded scene is the shown one (None: stale).
    hidden_gpu: Option<GpuScene>,
    hidden_uploaded: Option<bool>,
    weapon_light_deferred: usize,
    weapon_effect_session: Option<RequestId>,
    weapon_animation_cues: VecDeque<(bri_sim::presentation::Cue, f32, f64)>,
    weapon_animation_drops: u64,
    weapon_animation_cursor: u64,
    effects_renderer: Option<bri_fx_runtime::gpu::EffectsRenderer>,
    gpu_scene: Option<GpuScene>,
    gpu_terrain: Vec<bri_render::terrain_scene::GpuTerrain>,
    /// World-pass depth and, with MSAA, the multisampled color attachment
    /// that the last world pass resolves into the frame target.
    depth: Option<(wgpu::Texture, Option<wgpu::Texture>, (u32, u32))>,
    meshes: Option<Arc<Meshes>>,
    /// Replicated bricks as independently rebuilt chunks sharing one
    /// uploaded material palette. A running job owns `chunked`.
    palette: Option<Arc<crate::world_chunks::BrickPalette>>,
    gpu_palette: Option<GpuScene>,
    chunked: crate::world_chunks::ChunkedWorld,
    cpu_chunks: HashMap<crate::world_chunks::ChunkKey, SceneData>,
    gpu_chunks: HashMap<crate::world_chunks::ChunkKey, GpuScene>,
    chunk_uploads: BTreeSet<crate::world_chunks::ChunkKey>,
    world_source: Option<Arc<bri_net::protocol::PublicWorld>>,
    world_revision: u64,
    world_log: Option<Arc<network::WorldLog>>,
    world_job: Option<WorldJob>,
    graphics: crate::graphics::Graphics,
    load_limit: Arc<tokio::sync::Semaphore>,
    /// Rebuild GPU renderers before the next frame (the map changed).
    gpu_restart: bool,
    /// Map of the installed scene.
    scene_map: Option<String>,
    materials: Option<Arc<crate::materials::BrickMaterials>>,
    building: Option<crate::building::Building>,
    pending_actions: BTreeMap<RequestId, PendingAction>,
    tool_ui: crate::tool_ui::ToolUi,
    dialog_epoch: u64,
    /// `dialog_epoch` when the latest trigger click was sent. A wrench or
    /// printer hit notice opens its dialog only if no tool switch, cancel or
    /// close has happened since, so a cancelled click never reopens late.
    trigger_epoch: Option<u64>,
    query_source: Option<Arc<bri_net::protocol::PublicWorld>>,
    /// The replica log and revision `query_source` came from.
    query_log: Option<(Arc<network::WorldLog>, u64)>,
    ghost_gpu: Option<GpuScene>,
    ghost_uploaded: u64,
    avatar_assets: Arc<crate::avatar::AvatarAssets>,
    avatars: BTreeMap<bri_world::OwnerId, crate::avatar::AvatarMesh>,
    /// Horses spawned at vehicle bricks: `HorseArmor` bots, animated like
    /// horse players.
    mount_meshes: BTreeMap<u64, crate::avatar::AvatarMesh>,
    avatar_actions: BTreeMap<u64, crate::avatar::ActionAnimation>,
    /// Thread-3 builder and chat animations by player.
    avatar_gestures: BTreeMap<u64, crate::avatar::ActionAnimation>,
    avatar_action_images: BTreeMap<u64, String>,
    animation_time: f64,
    avatar_preview: Option<crate::avatar::Preview>,
    preview_request: Option<(bri_content::avatar::Appearance, [f32; 3], f32)>,
    preview_dirty: bool,
    motion: crate::motion::Motion,
    vehicle_assets: crate::vehicles::VehicleAssets,
    vehicles: crate::vehicles::ClientVehicles,
    /// Heading of the vehicle the local player rides, last frame.
    mount_heading: Option<f32>,
    /// `mCameraOffset`: how far the chase camera trails the vehicle.
    chase_lag: Vec3,
    /// This frame's seat rotation for every mounted player.
    rider_rotations: BTreeMap<bri_world::OwnerId, glam::Quat>,
    /// The tumble vehicle the local player last started riding.
    tumble: Option<u64>,
    music_world: Option<Arc<bri_net::protocol::PublicWorld>>,
    net_graph: Option<(std::time::Instant, u32)>,
    frame_stats: crate::console::FrameStats,
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
        if matches!(cue.kind, bri_sim::presentation::CueKind::BrickKill { .. })
            && self.brick_kills.len() < bri_sim::presentation::MAX_CUES
        {
            self.brick_kills.push(cue.clone());
        }
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
        flare_visible: impl Fn(Vec3) -> Result<bool>,
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
                let definition = vehicle_assets.definition(&info.definition)?;
                let frame = vehicles.frame(vehicle)?;
                crate::actor_effects::muzzle(
                    frame.position,
                    frame.rotation,
                    frame.turret_aim,
                    definition,
                )
            }
        };
        // `serverCmdLight` attaches `PlayerLight` to the player; v20's
        // `fxLight` follows the player's mount point 1 (the left hand, via
        // `getRenderMountTransform(1)`), so light and corona move with the arm.
        let mut lights = Vec::new();
        for (owner, _) in view.vitals.iter().filter(|(_, v)| v.light && v.alive) {
            let hand = avatars
                .get(owner)
                .and_then(|a| a.world_node(assets, "Mount1"))
                .map(|m| m.w_axis.truncate());
            let Some(position) = hand.or_else(|| {
                presented
                    .get(owner)
                    .map(|p| Vec3::from(p.feet) + Vec3::Y * 1.5)
            }) else {
                continue;
            };
            lights.push(crate::actor_effects::PlayerLight {
                actor: *owner,
                position,
                flare_visible: flare_visible(position)?,
            });
        }
        actor_effects.advance(elapsed, pose, &jets, &burning, &lights)
    }
    fn reset_weapon_effect_session(&mut self, session: RequestId, checkpoint_cursor: u64) {
        if self.weapon_effect_session == Some(session) {
            return;
        }
        self.weapon_effects.reset(checkpoint_cursor);
        self.actor_effects.reset(checkpoint_cursor);
        self.explosion_shapes.reset(checkpoint_cursor);
        self.weapon_shells.reset(checkpoint_cursor);
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
        avatar_gestures: &mut BTreeMap<u64, crate::avatar::ActionAnimation>,
        avatar_action_images: &mut BTreeMap<u64, String>,
        weapon_animation_cues: &mut VecDeque<(bri_sim::presentation::Cue, f32, f64)>,
        weapon_animation_drops: &mut u64,
        view: &network::View,
        elapsed: f32,
    ) {
        avatar_actions.retain(|owner, _| view.poses.contains_key(owner));
        avatar_gestures.retain(|owner, _| view.poses.contains_key(owner));
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
            // Thread 3 is not tied to a mounted image: it is replaced by the
            // next builder or chat animation, or stopped by `root`.
            if *thread == 3 {
                if sequence.eq_ignore_ascii_case("root") {
                    avatar_gestures.remove(actor);
                } else {
                    avatar_gestures.insert(
                        *actor,
                        crate::avatar::ActionAnimation {
                            sequence: sequence.clone(),
                            started_at,
                        },
                    );
                }
                continue;
            }
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
    /// Gun casings currently tumbling or resting.
    pub fn weapon_shell_count(&self) -> usize {
        self.weapon_shells.active_count()
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
    pub fn frame_stats(&self) -> &crate::console::FrameStats {
        &self.frame_stats
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
    /// Map whose scene is installed and drawn.
    pub fn scene_map(&self) -> Option<&str> {
        self.scene_map.as_deref()
    }
    /// The local player as presented this frame (prediction included).
    pub fn presented_local(&self) -> Option<&bri_sim::player::PlayerState> {
        let owner = self.network_view()?.owner;
        self.motion.presented().get(&owner)
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
        let weapon_shells = crate::weapon_debris::WeaponDebris::new(
            crate::weapon_debris::WeaponDebrisAssets::load(&content.paths.weapon_debris)?,
            Default::default(),
        )?;
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
        let mut avatar_assets = crate::avatar::AvatarAssets::load(&content.paths.avatar)?;
        avatar_assets.load_horse(&content.paths.vehicles)?;
        let avatar_assets = Arc::new(avatar_assets);
        let vehicle_assets = crate::vehicles::VehicleAssets::load(&content.paths.vehicles)?;
        let world_items = crate::world_items::WorldItems::new(
            item_assets.clone(),
            Arc::new(content.weapons.pack.clone()),
            Default::default(),
        )?;
        let mut saved = settings::load(&state_dir.join("settings.json"))?;
        let weather = crate::weather::ClientWeather::load(&content.paths.weather, &mut saved)?;
        let graphics = crate::graphics::Graphics::from_settings(&saved);
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
        ui.set_console_commands(crate::console::commands());
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
            weapon_shells,
            shell_gpu: None,
            weapon_cues: VecDeque::new(),
            weapon_cue_drops: 0,
            brick_debris: Default::default(),
            debris_models: Default::default(),
            brick_kills: Vec::new(),
            hidden_gpu: None,
            hidden_uploaded: None,
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
            palette: None,
            gpu_palette: None,
            chunked: Default::default(),
            cpu_chunks: HashMap::new(),
            gpu_chunks: HashMap::new(),
            chunk_uploads: BTreeSet::new(),
            world_source: None,
            world_revision: 0,
            world_log: None,
            world_job: None,
            graphics,
            load_limit: Arc::new(tokio::sync::Semaphore::new(2)),
            gpu_restart: false,
            scene_map: None,
            materials: None,
            building: None,
            pending_actions: BTreeMap::new(),
            tool_ui,
            dialog_epoch: 0,
            trigger_epoch: None,
            query_source: None,
            query_log: None,
            ghost_gpu: None,
            ghost_uploaded: u64::MAX,
            avatar_assets,
            avatars: BTreeMap::new(),
            mount_meshes: BTreeMap::new(),
            avatar_actions: BTreeMap::new(),
            avatar_gestures: BTreeMap::new(),
            avatar_action_images: BTreeMap::new(),
            animation_time: 0.0,
            avatar_preview: None,
            preview_request: None,
            preview_dirty: false,
            motion: Default::default(),
            vehicle_assets,
            vehicles: Default::default(),
            mount_heading: None,
            chase_lag: Vec3::ZERO,
            rider_rotations: BTreeMap::new(),
            tumble: None,
            music_world: None,
            net_graph: None,
            frame_stats: Default::default(),
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
        self.ui.core.name_tags.clear();
        self.scene_map = None;
        self.abilities = Default::default();
        self.brick_hand = None;
        self.foliage.clear();
        self.weather.clear();
        self.audio.clear();
        self.effects.clear();
        self.weapon_effects.reset(0);
        self.actor_effects.reset(0);
        self.explosion_shapes.reset(0);
        self.weapon_shells.clear();
        self.weapon_cues.clear();
        self.weapon_animation_cues.clear();
        self.weapon_animation_drops = 0;
        self.weapon_animation_cursor = 0;
        self.weapon_cue_drops = 0;
        self.brick_debris.clear();
        self.debris_models.clear();
        self.brick_kills.clear();
        self.hidden_gpu = None;
        self.hidden_uploaded = None;
        self.weapon_light_deferred = 0;
        self.weapon_effect_session = None;
        self.world_items.reset();
        self.attempt.take();
        self.avatars.clear();
        self.mount_meshes.clear();
        self.avatar_actions.clear();
        self.avatar_gestures.clear();
        self.avatar_action_images.clear();
        self.controls = Controls::default();
        self.cpu_scene = None;
        self.cpu_terrain.clear();
        self.gpu_scene = None;
        self.gpu_terrain.clear();
        self.meshes = None;
        self.palette = None;
        self.gpu_palette = None;
        self.chunked = Default::default();
        self.cpu_chunks.clear();
        self.gpu_chunks.clear();
        self.chunk_uploads.clear();
        self.world_source = None;
        self.world_revision = 0;
        self.world_log = None;
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
        self.query_log = None;
        self.dialog_epoch = self.dialog_epoch.wrapping_add(1);
        self.ghost_gpu = None;
        self.ghost_uploaded = u64::MAX;
        self.motion.reset();
        self.vehicles.clear();
        self.music_world = None;
        self.controls.clear_observer();
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
    /// The chase camera while riding (`Vehicle::getCameraTransform`):
    /// distance, pivot above the vehicle and downward view tilt.
    fn chase_camera(
        assets: &crate::vehicles::VehicleAssets,
        vehicles: &crate::vehicles::ClientVehicles,
        lag: Vec3,
        view: &network::View,
        local: &bri_sim::player::PlayerState,
    ) -> Option<(f32, Vec3, f32)> {
        // A `HorseArmor` player uses its datablock's camera fields
        // (cameraMaxDist, cameraVerticalOffset above the feet, cameraTilt).
        if local.datablock == bri_sim::player_types::PlayerType::Horse
            && view.vitals.get(&view.owner).is_none_or(|v| v.mounted.is_none())
        {
            let camera = &assets.definition("v20.vehicle.horsearmor")?.camera;
            return Some((
                camera.max_dist.clamp(1.0, 40.0),
                Vec3::from(local.feet) + Vec3::Y * camera.offset + lag,
                camera.tilt,
            ));
        }
        let (vehicle, _) = view.vitals.get(&view.owner)?.mounted?;
        let info = view.vehicles.get(&vehicle)?;
        let camera = &assets.definition(&info.definition)?.camera;
        let frame = vehicles.frame(vehicle)?;
        Some((
            camera.max_dist.clamp(1.0, 40.0),
            frame.position + Vec3::Y * camera.offset + lag,
            camera.tilt,
        ))
    }
    /// Pose each spawned horse with the horse rig from its interpolated
    /// frame: body in the brick's colour, dead ones in `death1`.
    fn pose_mounts(
        mount_meshes: &mut BTreeMap<u64, crate::avatar::AvatarMesh>,
        avatar_assets: &crate::avatar::AvatarAssets,
        vehicle_assets: &crate::vehicles::VehicleAssets,
        vehicles: &crate::vehicles::ClientVehicles,
        animation_time: f64,
        view: &network::View,
    ) -> Result<()> {
        let horses: Vec<_> = view
            .vehicles
            .values()
            .filter(|info| {
                vehicle_assets
                    .definition(&info.definition)
                    .is_some_and(|d| d.family == bri_vehicles::Family::Horse)
            })
            .cloned()
            .collect();
        mount_meshes
            .retain(|id, _| horses.iter().any(|h| h.id == *id));
        for info in horses {
            let Some(frame) = vehicles.frame(info.id).cloned() else {
                continue;
            };
            let mut appearance = avatar_assets.package.defaults.clone();
            let color = info
                .color
                .and_then(|c| view.world.palette.get(usize::from(c)))
                .map_or([1.0; 4], |c| [c[0], c[1], c[2], 1.0]);
            appearance.colors.insert("chest".into(), color);
            if mount_meshes
                .get(&info.id)
                .is_none_or(|m| m.appearance != appearance)
            {
                let mesh = avatar_assets.horse_mesh(appearance)?;
                mount_meshes.insert(info.id, mesh);
            }
            let forward = frame.rotation * Vec3::NEG_Z;
            let state = bri_sim::player::PlayerState {
                owner: 1,
                feet: frame.position.to_array(),
                velocity: frame.velocity.to_array(),
                yaw: forward.x.atan2(-forward.z),
                pitch: 0.0,
                head_yaw: 0.0,
                grounded: frame.velocity.y.abs() < 0.5,
                crouched: false,
                jetting: false,
                jump: Default::default(),
                datablock: bri_sim::player_types::PlayerType::Horse,
                scale: 1.0,
                energy: 0.0,
            };
            let input = crate::avatar::AvatarAnimationInput {
                dead: info.destroyed,
                ..Default::default()
            };
            mount_meshes.get_mut(&info.id).unwrap().pose_with_animation(
                avatar_assets,
                &state,
                animation_time,
                &input,
            )?;
        }
        Ok(())
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
    /// Seated where v20's `armor::onTrigger` fires the mount's gun instead
    /// of tools: the Tank turret and the pirate cannon.
    fn local_weapon_seat(&self) -> bool {
        self.network_view()
            .and_then(|v| {
                let (vehicle, seat) = v.vitals.get(&v.owner)?.mounted?;
                let info = v.vehicles.get(&vehicle)?;
                let d = self.vehicle_assets.definition(&info.definition)?;
                Some(d.seats.get(usize::from(seat))?.weapon)
            })
            .unwrap_or(false)
    }
    /// Steer whatever the server says this client controls. A granted free
    /// camera starts at the player's smoothed eye (`dropCameraAtPlayer`).
    fn follow_control(&mut self) {
        let Some(view) = self.network_view() else {
            return;
        };
        let control = view
            .vitals
            .get(&view.owner)
            .map_or_else(Default::default, |v| v.control);
        let eye = self.motion.local_eye().or_else(|| {
            view.poses
                .get(&view.owner)
                .map(|p| p.player.eye(&p.player.tuning()))
        });
        self.controls.follow(control, view.owner, eye);
    }
    /// Dead players watch their corpse from the orbit camera.
    fn third_person_view(&self) -> bool {
        self.controls.third_person || self.controls.observer().is_some() || !self.local_alive()
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
        // `showEnergyBar` datablocks show the predicted jet energy.
        let energy = self
            .motion
            .presented()
            .get(&view.owner)
            .filter(|p| p.datablock.shows_energy())
            .map(|p| p.energy / p.tuning().max_energy);
        let shown = energy.map(|e| (e.clamp(0.0, 1.0) * 100.0).round() as u8);
        if shown != c.energy {
            c.energy = shown;
            updates.push(UiUpdate::Energy(energy));
        }
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
                    let max = view
                        .poses
                        .get(&view.owner)
                        .map_or(bri_sim::session::MAX_HEALTH, |p| {
                            p.player.datablock.max_health()
                        });
                    updates.push(UiUpdate::DamageFlash((c.health - local.health) / max * 2.0));
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
        let paths_for_maps = paths.clone();
        // Admin Change Map choices (the Tutorial has its own entry point).
        let map_list: Vec<_> = self
            .content
            .maps
            .iter()
            .filter(|m| !m.id.contains("map_tutorial"))
            .map(|m| bri_sim::session::MapListing {
                id: m.id.clone(),
                name: m.name.clone(),
            })
            .collect();
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
        let internet = mode == ServerMode::Internet;
        let max_players = if single { 1 } else { max_players };
        let listing_name = local_name.clone();
        let listing_map = self
            .content
            .maps
            .iter()
            .find(|m| m.id == map)
            .map_or_else(|| map.clone(), |m| m.name.clone());
        let (scene_tx, scene) = mpsc::sync_channel(1);
        let (router_tx, router) = mpsc::channel();
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
                    let palette = Arc::new(crate::world_chunks::BrickPalette::new(&materials)?);
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
                            palette,
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
            let setup = HostSetup {
                // v20 `$Server::LAN`: single-player and LAN hosts keep the looser
                // brick-damage rule; internet hosts use miniGameCanDamage.
                lan: !internet,
                catalog,
                weapon_pack,
                item_bounds,
                avatar_catalog,
                vehicle_pack,
                event_catalog,
                event_sounds,
                maps: map_list,
            };
            let spawn_points = loaded.spawn_points.clone();
            let mut session = setup.session(loaded)?;
            session.set_admin_passwords(admin, super_admin)?;
            let map_loader: server::MapLoader = {
                let paths = paths_for_maps.clone();
                Arc::new(move |map: &str| setup.session(paths.load_map(map, None)?))
            };
            let mut host = server::start_with_admin_store_and_limit(
                session,
                ServerOptions {
                    bind,
                    content_id: identity.clone(),
                    spawn_points,
                    // LAN hosts keep one identity so joiners' saved trust stays valid.
                    certificate: if single {
                        None
                    } else {
                        Some(server::HostCertificate::load_or_create(&state_dir)?)
                    },
                    map_loader: Some(map_loader),
                },
                max_players as usize,
                state_dir.join("administration.json"),
            )?;
            let address = SocketAddr::from(([127, 0, 0, 1], host.address.port()));
            if !single {
                // LAN players find this host (and its certificate) by broadcast;
                // Connect to IP asks the same responder directly, so internet
                // hosts answer it too.
                host.advertise(listing_name, listing_map, max_players, identity.clone())
                    .await?;
            }
            if internet {
                host.open_router_ports(router_tx);
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
            talking: Vec::new(),
            router: internet.then_some(router),
            trust: BTreeMap::new(),
            map_failure: None,
            reloading: false,
        });
        Ok(())
    }
    fn join(&mut self, id: RequestId, address: String, password: String) -> Result<()> {
        ensure!(
            password.is_empty(),
            "Password authentication is not connected yet"
        );
        let address = parse_join_address(&address)?;
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
                    bri_files::replace(&pins_file, &serde_json::to_vec_pretty(&pins)?)?;
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
                prepare_map(&paths, &map, selected, &catalog)
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
            talking: Vec::new(),
            router: None,
            trust: BTreeMap::new(),
            map_failure: None,
            reloading: false,
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
        // Administration requests never carry a gameplay aim.
        let aim = (!matches!(command, Command::Admin(_))).then_some(bri_sim::session::ActionAim {
            yaw: self.controls.yaw,
            pitch: self.controls.pitch,
        });
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
            .request_with_aim(id, command, aim)?;
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
            if matches!(command, Command::WeaponTrigger { down: true }) {
                self.trigger_epoch = Some(self.dialog_epoch);
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
                let last_print = self
                    .tool_ui
                    .command_accepted(
                        pending
                            .command
                            .as_ref()
                            .ok_or("Missing accepted tool command")?,
                    )
                    .map_err(|e| {
                        format!("Server accepted the edit, but dialog refresh failed: {e:#}")
                    })?;
                if let Some(last) = last_print
                    && let Some(building) = self.building.as_mut()
                {
                    building
                        .remember_print(&last)
                        .map_err(|e| format!("Last print was not remembered: {e:#}"))?;
                }
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
                network::Event::MapChanged(map) => {
                    // Load the new map's scene and prediction world; the old
                    // scene stays until it is ready.
                    let paths = self.content.paths.clone();
                    let selected: Vec<_> = self
                        .content
                        .catalog
                        .bricks
                        .iter()
                        .filter(|b| b.selectable())
                        .map(|b| (b.id.clone(), b.orientation_fix))
                        .collect();
                    let catalog = self.tool_ui.server_catalog();
                    let load_limit = self.load_limit.clone();
                    let (scene_tx, scene) = mpsc::sync_channel(1);
                    a.scene = scene;
                    self.world_items.reset();
                    self.brick_debris.clear();
                    let (failed_tx, failed_rx) = mpsc::sync_channel(1);
                    a.map_failure = Some(failed_rx);
                    // v20 shows the loading GUI while the new mission loads.
                    a.reloading = true;
                    self.ui.apply_session(
                        a.id,
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
                    self.runtime.spawn(async move {
                        let prepared = async {
                            let permit = load_limit.acquire_owned().await?;
                            tokio::task::spawn_blocking(move || {
                                let _permit = permit;
                                prepare_map(&paths, &map, selected, &catalog)
                            })
                            .await?
                        }
                        .await;
                        match prepared {
                            Ok(prepared) => {
                                let _ = scene_tx.send(prepared);
                            }
                            Err(error) => {
                                let _ = failed_tx.send(format!("{error:#}"));
                            }
                        }
                    });
                }
                network::Event::Notice(bri_sim::session::Notice::Inspected {
                    brick_id,
                    brick,
                    mode,
                }) => {
                    // A wrench/printer hit opens its dialog only over plain
                    // play, never over a newer modal or while typing, and
                    // only for a click nothing has cancelled since.
                    if self.ui.stack() != [ScreenId::Play]
                        || self.trigger_epoch != Some(self.dialog_epoch)
                    {
                        continue;
                    }
                    let Some(view) = a.view.as_ref() else {
                        continue;
                    };
                    self.invalidate_tool_dialogs();
                    let reply = Reply::Inspected {
                        brick_id,
                        brick,
                        mode,
                    };
                    match self.tool_ui.accept_inspection(
                        &reply,
                        mode,
                        None,
                        &view.world,
                        &view.names,
                        view.owner,
                    ) {
                        Ok(updates) => {
                            for update in updates {
                                self.ui.apply_session(a.id, update);
                            }
                        }
                        Err(error) => {
                            self.ui.apply_session(
                                a.id,
                                UiUpdate::CenterPrint {
                                    text: format!("{error:#}"),
                                    seconds: 2.0,
                                },
                            );
                        }
                    }
                }
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
                        bri_sim::session::Notice::Sound(profile) => {
                            self.audio.profile(&profile, bri_audio::Placement::Listener);
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
                        bri_sim::session::Notice::MessageBox { title, text } => {
                            UiUpdate::MessageBox {
                                title: plain_chat(&title),
                                text: plain_chat(&text),
                            }
                        }
                        bri_sim::session::Notice::TrustInvite {
                            from,
                            name,
                            principal,
                            level,
                        } => {
                            // `clientCmdTrustInvite` refuses ignored players.
                            if a.trust.get(&from).is_some_and(|t| t.ignoring) {
                                continue;
                            }
                            UiUpdate::TrustInvite(TrustInvitation {
                                from,
                                name: plain_chat(&name),
                                bl_id: crate::trust_list::display_id(&principal).to_string(),
                                level,
                            })
                        }
                        bri_sim::session::Notice::TrustSaved {
                            principal,
                            level,
                            name,
                        } => {
                            let path = self.state_dir.join("trust-list.json");
                            let saved = crate::trust_list::TrustList::load(&path)
                                .update(&principal, level, &plain_chat(&name));
                            if let Err(error) = saved {
                                UiUpdate::Chat {
                                    text: plain_chat(&format!("{error:#}")),
                                }
                            } else {
                                continue;
                            }
                        }
                        bri_sim::session::Notice::PlayerTrust(rows) => {
                            a.trust = rows;
                            continue;
                        }
                        bri_sim::session::Notice::Inspected { .. } => unreachable!(),
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
        if let Some(reason) = a.map_failure.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.ui.apply_session(
                a.id,
                UiUpdate::Connection(ConnectionState::Failed {
                    reason: format!("Could not load the new map: {reason}"),
                }),
            );
            self.disconnect();
            return Ok(());
        }
        if let Ok(prepared) = a.scene.try_recv() {
            if self.cpu_scene.is_some() {
                // A map change: renderers keep per-map sky and terrain state,
                // so rebuild them for the new map like a fresh join.
                self.gpu_stopped();
                self.gpu_restart = true;
            }
            self.scene_map = Some(prepared.map_id.clone());
            self.foliage.set_map(prepared.foliage);
            self.weather.set_map(&prepared.map_id, prepared.waters)?;
            self.cpu_scene = Some(prepared.scene);
            self.cpu_terrain = prepared.terrain;
            self.meshes = Some(prepared.meshes);
            self.materials = Some(prepared.materials);
            self.palette = Some(prepared.palette);
            self.gpu_palette = None;
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
        if let (Some(building), Some(view)) = (&mut self.building, &a.view) {
            building.set_held_image(
                view.weapons
                    .images
                    .get(&view.owner)
                    .is_some_and(|images| images.iter().any(|image| image.hand == 0)),
            );
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
            let known = self
                .query_log
                .as_ref()
                .filter(|(log, _)| self.query_source.is_some() && Arc::ptr_eq(log, &view.world_log))
                .and_then(|(log, revision)| log.between(*revision, view.world_revision));
            if let Err(error) = building.sync_world_changes(&view.world, known.as_ref()) {
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
            self.query_log = Some((view.world_log.clone(), view.world_revision));
            self.ghost_uploaded = u64::MAX;
            self.brick_debris.sync_world(&view.world);
            self.hidden_uploaded = None;
        }
        if let Some(job) = &self.world_job
            && let Ok((source, revision, log, result)) = job.receiver.try_recv()
        {
            self.world_job = None;
            match result {
                // Always applied: chunk state is consistent with `source`, and
                // a newer replica is reached by the next incremental update.
                Ok((chunked, changes)) => {
                    self.chunked = chunked;
                    for (key, scene) in changes {
                        if let Some(scene) = scene {
                            self.cpu_chunks.insert(key, scene);
                            self.chunk_uploads.insert(key);
                        } else {
                            self.cpu_chunks.remove(&key);
                            self.gpu_chunks.remove(&key);
                            self.chunk_uploads.remove(&key);
                        }
                    }
                    self.world_source = Some(source);
                    self.world_revision = revision;
                    self.world_log = Some(log);
                }
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
            && let (Some(meshes), Some(materials), Some(palette), Some(view)) =
                (&self.meshes, &self.materials, &self.palette, &a.view)
            && self
                .world_source
                .as_ref()
                .is_none_or(|previous| !Arc::ptr_eq(previous, &view.world))
        {
            let meshes = meshes.clone();
            let materials = materials.clone();
            let palette = palette.clone();
            let world = view.world.clone();
            let (revision, log) = (view.world_revision, view.world_log.clone());
            // Compare only the bricks the replica reports changed since the
            // applied revision; without that history, compare whole worlds.
            let known = self
                .world_log
                .as_ref()
                .filter(|applied| Arc::ptr_eq(applied, &log))
                .and_then(|log| log.between(self.world_revision, revision));
            let mut chunked = std::mem::take(&mut self.chunked);
            let (send, receive) = mpsc::sync_channel(1);
            let load_limit = self.load_limit.clone();
            let task = self.runtime.spawn(async move {
                let Ok(permit) = load_limit.acquire_owned().await else {
                    return;
                };
                let source = world.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    chunked
                        .update(
                            world,
                            known.as_ref(),
                            &meshes,
                            &palette,
                            Some(&materials),
                            4_000_000,
                        )
                        .map(|changes| (chunked, changes))
                        .map_err(|e| format!("{e:#}"))
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
                let _ = send.send((source, revision, log, result));
            });
            self.world_job = Some(WorldJob {
                receiver: receive,
                abort: task.abort_handle(),
            });
        }
        if a.reloading
            && let Some(view) = &a.view
            && self.scene_map.as_deref() == Some(view.world.map_id.as_str())
            && self
                .world_source
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, &view.world))
        {
            a.reloading = false;
            self.ui.apply_session(
                a.id,
                UiUpdate::Connection(ConnectionState::InGame {
                    server_name: a.name.clone(),
                    max_players: a.max_players,
                    local: a.local,
                    single_player: a.single,
                    admin: view.administrator,
                }),
            );
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
            // `clientCmdTrustListUpload_Start`; the reply needs no handling.
            let list = crate::trust_list::TrustList::load(&self.state_dir.join("trust-list.json"));
            a.worker
                .request(REPORT_REQUEST, Command::TrustList(list.entries()))?;
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
                    // addMessageCallback: the message type's GUI sound.
                    if let Some(tag) = line.tag {
                        use bri_sim::session::MessageTag;
                        let key = match tag {
                            MessageTag::UploadStart => "ui.upload_start",
                            MessageTag::UploadEnd => "ui.upload_end",
                            MessageTag::ProcessComplete => "ui.process_complete",
                            MessageTag::ClearBricks => "ui.brick_clear",
                        };
                        self.audio.trigger(key, bri_audio::Placement::Listener);
                    }
                }
            }
            // MsgStartTalking / MsgStopTalking keep WhoTalkSO's order.
            let talking = &mut a.talking;
            talking.retain(|o| view.vitals.get(o).is_some_and(|v| v.talking));
            for (owner, vitals) in &view.vitals {
                if vitals.talking && !talking.contains(owner) {
                    talking.push(*owner);
                }
            }
            let names = talking
                .iter()
                .filter_map(|o| view.names.get(o))
                .map(|n| plain_chat(n))
                .collect();
            self.ui.apply_session(a.id, UiUpdate::Talking(names));
            if a.entered
                && let Some(router) = &a.router
            {
                while let Ok(text) = router.try_recv() {
                    self.ui.apply_session(a.id, UiUpdate::Chat { text: plain_chat(&text) });
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
                            bl_id: a
                                .trust
                                .get(&owner)
                                .and_then(|t| t.principal.as_ref())
                                .map(crate::trust_list::display_id),
                            trust: crate::trust_list::label(a.trust.get(&owner).map_or(
                                if owner == view.owner {
                                    bri_sim::session::TrustLevel::You
                                } else {
                                    bri_sim::session::TrustLevel::None
                                },
                                |t| t.level,
                            ))
                            .into(),
                            ignoring: a.trust.get(&owner).is_some_and(|t| t.ignoring),
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
/// Eye of the camera in control: the free camera itself, an orbit around the
/// spied player, the chase camera, or the player's own eye.
/// `GuiShapeNameHud::onRender`: every other living player's name above their
/// eye point (`verticalOffset` 0.85), hidden behind terrain and interiors and
/// faded over the last 90% of the visible distance (`distanceFade` 0.1).
#[allow(clippy::too_many_arguments)]
fn name_tags(
    view: &network::View,
    presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
    building: Option<&crate::building::Building>,
    view_projection: glam::Mat4,
    camera: Vec3,
    visible_distance: f32,
    size: (f32, f32),
    scale: f32,
    controlling_body: bool,
) -> Vec<bri_ui::api::NameTag> {
    const VERTICAL_OFFSET: f32 = 0.85;
    const DISTANCE_FADE: f32 = 0.1;
    let fade_distance = visible_distance * DISTANCE_FADE;
    let mut tags = Vec::new();
    for (owner, name) in &view.names {
        if (*owner == view.owner && controlling_body)
            || !view.vitals.get(owner).is_some_and(|v| v.alive)
        {
            continue;
        }
        let Some(state) = presented.get(owner) else {
            continue;
        };
        let target = state.eye(&state.tuning());
        let distance = target.distance(camera);
        if distance <= 0.0 || distance > visible_distance {
            continue;
        }
        if building.is_some_and(|b| b.map_blocks(camera, target)) {
            continue;
        }
        let clip = view_projection * (target + Vec3::Y * VERTICAL_OFFSET).extend(1.0);
        if clip.w <= 0.0 {
            continue;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 {
            continue;
        }
        let opacity = if distance < fade_distance {
            1.0
        } else {
            1.0 - (distance - fade_distance) / (visible_distance - fade_distance)
        };
        tags.push(bri_ui::api::NameTag {
            x: (ndc.x + 1.0) * 0.5 * size.0 / scale,
            y: (1.0 - ndc.y) * 0.5 * size.1 / scale,
            text: plain_chat(name),
            opacity,
        });
    }
    tags
}
fn camera_eye(
    controls: &Controls,
    presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
    building: &crate::building::Building,
    own_eye: Vec3,
    forward: Vec3,
    chase: Option<f32>,
) -> Result<Vec3> {
    use crate::controls::ObserverMode;
    match controls.observer().map(|o| o.mode) {
        Some(ObserverMode::Free(position)) => Ok(position),
        // `setOrbitMode(target, ..., 0, 8, 8)` from `Observer::setMode("Corpse")`.
        Some(ObserverMode::Orbit(_)) => building.camera_position(
            controls.orbit_focus(presented).unwrap_or(own_eye),
            forward,
            8.0,
        ),
        None => match chase {
            Some(distance) => building.camera_position(own_eye, forward, distance),
            None => Ok(own_eye),
        },
    }
}
/// Torque's FOV is horizontal (`GuiTSCtrl::processCameraQuery` takes the
/// frustum width from it and the height from the aspect ratio).
fn vertical_fov(horizontal: f32, aspect: f32) -> f32 {
    if !(aspect.is_finite() && aspect > 0.0) {
        return horizontal;
    }
    2.0 * ((horizontal * 0.5).tan() / aspect).atan()
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
    /// Energy bar fraction last shown, in hundredths.
    energy: Option<u8>,
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
    crate::world_scene::v20_temp_brick(scene);
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
        self.frame_stats.push(elapsed);
        let mut listener = bri_audio::Listener::default();
        // `setTimeScale` slows or speeds the whole game, not the interface.
        let scale = self
            .attempt
            .as_ref()
            .and_then(|a| a.view.as_ref())
            .map_or(1.0, |v| v.time_scale);
        let game_elapsed = elapsed.mul_f32(scale);
        self.animation_time += game_elapsed.as_secs_f64().min(0.25);
        self.poll_network()?;
        self.poll_files();
        let alive = self.local_alive();
        self.follow_control();
        self.controls.fly(elapsed.as_secs_f32());
        let prefs = &self.ui.core.prefs;
        self.controls.set_fov_prefs(
            bri_ui::screens::options::default_fov(prefs),
            prefs.f32_or("$Pref::player::CurrentFOV", 10.0),
        );
        self.controls.set_invert_prefs(
            prefs.bool_or("$pref::Input::MouseInvert", false),
            prefs.bool_or("$Pref::Input::VehicleMouseInvert", true),
        );
        self.controls.advance_zoom(elapsed.as_secs_f32());
        if let Some(a) = self.attempt.as_ref().filter(|a| a.entered) {
            let input = if alive {
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
                game_elapsed.as_secs_f32(),
                input,
                bri_net::protocol::MOVEMENT_REDUNDANCY,
            )? {
                a.worker.movement(newest, inputs)?;
            }
            if let Some(view) = &a.view {
                let mounted = view.vitals.get(&view.owner).and_then(|v| v.mounted);
                self.motion.set_mounted(mounted.is_some());
                let head_yaw = self.controls.movement().head_yaw;
                self.motion
                    .present(view, self.controls.yaw, self.controls.pitch, head_yaw);
                let driven = mounted.filter(|(_, seat)| *seat == 0).map(|(id, _)| id);
                self.vehicles.update(
                    &view.vehicles,
                    &view.vehicle_poses,
                    self.motion.server_tick(),
                    driven,
                );
                if let Some((vehicle, seat)) = mounted
                    && let Some(info) = view.vehicles.get(&vehicle)
                    && let Some(d) = self.vehicle_assets.definition(&info.definition)
                    && d.seats.get(usize::from(seat)).is_some_and(|s| s.weapon)
                {
                    self.vehicles
                        .aim_locally(vehicle, d, self.controls.yaw, self.controls.pitch);
                }
                // The view rides along: it faces the seat, follows a
                // mouse-steered vehicle, turns with the hull for a gunner, and
                // stays put on a mount facing the look.
                let riding = mounted.and_then(|(vehicle, seat)| {
                    let info = view.vehicles.get(&vehicle)?;
                    let d = self.vehicle_assets.definition(&info.definition)?;
                    let frame = self.vehicles.frame(vehicle)?;
                    let seat_yaw = self
                        .vehicles
                        .seat(&self.vehicle_assets, info, usize::from(seat))
                        .map(|(_, yaw)| yaw);
                    let dt = elapsed.as_secs_f32().min(0.1);
                    self.chase_lag -=
                        (self.chase_lag * d.camera.decay + frame.velocity * d.camera.lag) * dt;
                    // skiVehicle::onWreck whites the screen out by the crash
                    // speed: clamp(1 + (speed - 10) / 50 * 7, 1, 7) / 7.
                    if d.family == bri_vehicles::Family::Tumble && self.tumble != Some(vehicle) {
                        self.tumble = Some(vehicle);
                        let seconds =
                            (1.0 + (frame.velocity.length() - 10.0) / 50.0 * 7.0).clamp(1.0, 7.0);
                        self.ui.apply(UiUpdate::Whiteout(seconds / 7.0));
                    }
                    let forward = frame.rotation * Vec3::NEG_Z;
                    Some((
                        d.seat_role(usize::from(seat)),
                        forward.x.atan2(-forward.z),
                        forward.y.clamp(-1.0, 1.0).asin(),
                        seat_yaw,
                    ))
                });
                if !matches!(
                    riding,
                    Some((SeatRole::Passenger | SeatRole::StrafeDriver, ..))
                ) {
                    self.controls.set_seat_yaw(None);
                }
                match riding {
                    Some((SeatRole::MouseDriver, heading, pitch, _)) => {
                        self.controls.set_vehicle_view(Some((heading, pitch)));
                        self.mount_heading = Some(heading);
                    }
                    Some((SeatRole::Actor, ..)) => {
                        self.controls.set_vehicle_view(None);
                        self.mount_heading = None;
                    }
                    None => {
                        self.controls.set_vehicle_view(None);
                        self.mount_heading = None;
                        self.chase_lag = Vec3::ZERO;
                    }
                    // Passengers and the Jeep's driver sit fixed in the seat:
                    // the mouse only tilts their view (`Player::processTick`
                    // takes the mount transform; the driver's yaw goes to the
                    // vehicle, which ignores it when the keys steer).
                    Some((SeatRole::Passenger | SeatRole::StrafeDriver, heading, _, seat_yaw)) => {
                        self.controls.set_vehicle_view(None);
                        self.controls.set_seat_yaw(seat_yaw.or(Some(heading)));
                        self.mount_heading = Some(heading);
                    }
                    Some((SeatRole::Gunner, heading, ..)) => {
                        self.controls.set_vehicle_view(None);
                        if let Some(previous) = self.mount_heading {
                            let turn = (heading - previous + std::f32::consts::PI)
                                .rem_euclid(std::f32::consts::TAU)
                                - std::f32::consts::PI;
                            self.controls.carry_yaw(turn);
                        }
                        self.mount_heading = Some(heading);
                    }
                }
                Self::pose_mounts(
                    &mut self.mount_meshes,
                    &self.avatar_assets,
                    &self.vehicle_assets,
                    &self.vehicles,
                    self.animation_time,
                    view,
                )?;
                // Riders sit exactly on their rendered vehicle's seat, tilted
                // with it (`Player::processTick` takes the mount transform).
                self.rider_rotations.clear();
                for (owner, vitals) in &view.vitals {
                    let Some((vehicle, seat)) = vitals.mounted else {
                        continue;
                    };
                    if let Some(info) = view.vehicles.get(&vehicle)
                        && let Some((mut feet, mut rotation)) = self.vehicles.seat_transform(
                            &self.vehicle_assets,
                            info,
                            usize::from(seat),
                        )
                    {
                        // A horse's rider rides its animated mount node, rising
                        // and falling with the gait like v20's `mountObject`.
                        if let Some(node) = self
                            .mount_meshes
                            .get(&vehicle)
                            .zip(
                                self.vehicle_assets
                                    .definition(&info.definition)
                                    .and_then(|d| d.seats.get(usize::from(seat))),
                            )
                            .and_then(|(mesh, s)| mesh.world_node(&self.avatar_assets, &s.node))
                        {
                            let (_, node_rotation, position) =
                                node.to_scale_rotation_translation();
                            feet = position;
                            rotation = node_rotation;
                        }
                        let forward = rotation * Vec3::NEG_Z;
                        let yaw = forward.x.atan2(-forward.z);
                        self.rider_rotations.insert(*owner, rotation);
                        let velocity = self
                            .vehicles
                            .frame(vehicle)
                            .map_or(Vec3::ZERO, |f| f.velocity);
                        // Every rider faces the seat.
                        self.motion.override_presented(
                            *owner,
                            feet,
                            Some(yaw),
                            rotation * Vec3::Y,
                            velocity,
                            *owner == view.owner,
                        );
                    }
                }
                self.vehicles
                    .prepare(&mut self.vehicle_assets, &view.vehicles, &view.world.palette);
                let presented = self.motion.presented();
                let mut loops = BTreeMap::new();
                for (owner, images) in &view.weapons.images {
                    let Some(player) = presented.get(owner) else {
                        continue;
                    };
                    for mounted in images {
                        let sound = self
                            .content
                            .weapons
                            .pack
                            .images
                            .get(&mounted.image)
                            .and_then(|image| {
                                image.states.iter().find(|s| s.name == mounted.state)
                            })
                            .map(|state| state.sound.as_str())
                            .filter(|sound| !sound.is_empty() && self.audio.is_looping(sound));
                        if let Some(sound) = sound {
                            let eye = Vec3::from(player.feet)
                                + Vec3::Y * player.tuning().stand_eye;
                            loops.insert((*owner, mounted.hand), (sound.to_string(), eye.to_array()));
                        }
                    }
                }
                self.audio.sync_image_loops(&loops);
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
                &mut self.avatar_gestures,
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
                // `HorseArmor` players draw horse.dts.
                let horse = player.datablock == bri_sim::player_types::PlayerType::Horse;
                if self
                    .avatars
                    .get(owner)
                    .is_none_or(|mesh| &mesh.appearance != appearance || mesh.horse != horse)
                {
                    let mut mesh = if horse {
                        self.avatar_assets.horse_mesh(appearance.clone())?
                    } else {
                        self.avatar_assets.mesh(appearance.clone())?
                    };
                    // Outfit changes (spray paint included) keep the running
                    // action thread instead of restarting the clip.
                    if let Some(old) = self.avatars.get(owner).filter(|old| old.horse == horse) {
                        mesh.continue_animation(old);
                    }
                    self.avatars.insert(*owner, mesh);
                }
                let mut ready_hands = Vec::new();
                if let Some(images) = view.weapons.images.get(owner) {
                    for mounted in images {
                        if let Some((right, left)) =
                            bri_weapons::scripted_arm_pose(&mounted.image, &mounted.state)
                        {
                            ready_hands.extend([(0, right), (1, left)]);
                        } else if let Some(image) =
                            self.content.weapons.pack.images.get(&mounted.image)
                        {
                            ready_hands.push((mounted.hand, image.arm_ready));
                        }
                    }
                }
                // SkiItem's color shift tints the skis of anyone riding skis.
                let skiing = view
                    .vitals
                    .get(owner)
                    .and_then(|v| v.mounted)
                    .and_then(|(vehicle, _)| view.vehicles.get(&vehicle))
                    .and_then(|info| self.vehicle_assets.definition(&info.definition))
                    .is_some_and(|d| d.family == bri_vehicles::Family::Skis);
                self.avatars
                    .get_mut(owner)
                    .unwrap()
                    .set_skis(skiing.then_some([0.0, 0.2, 0.64, 1.0]));
                let dead = view.vitals.get(owner).is_some_and(|v| !v.alive);
                // `Armor::onMount` applies the mount's look limits; the Tank's
                // gunner rides TankTurretPlayer, so it takes that datablock's.
                let look_limits = view
                    .vitals
                    .get(owner)
                    .and_then(|v| v.mounted)
                    .and_then(|(vehicle, seat)| {
                        let info = view.vehicles.get(&vehicle)?;
                        let d = self.vehicle_assets.definition(&info.definition)?;
                        if d.seat_role(usize::from(seat)) == SeatRole::Gunner
                            && d.attachment_mount.is_some()
                        {
                            return self
                                .vehicle_assets
                                .definition("v20.vehicle.tankturretplayer")
                                .map(|t| t.look_limits);
                        }
                        Some(d.look_limits)
                    });
                let input = crate::avatar::AvatarAnimationInput {
                    look_limits,
                    mount_rotation: self.rider_rotations.get(owner).copied(),
                    held_tool_pose: if dead {
                        crate::avatar::HeldToolPose::None
                    } else {
                        crate::avatar::HeldToolPose::from_mounted_images(ready_hands)
                    },
                    action: self.avatar_actions.get(owner).cloned().filter(|_| !dead),
                    gesture: self.avatar_gestures.get(owner).cloned().filter(|_| !dead),
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
                    // Riders hold `root` (`Armor::onMount` sets the action
                    // thread to root and mountThread on thread 0); they do
                    // not run, jump or fall with their mount's motion.
                    tick_state: if view.vitals.get(owner).is_some_and(|v| v.mounted.is_some()) {
                        Some(bri_sim::player::PlayerState {
                            velocity: [0.0; 3],
                            grounded: true,
                            jetting: false,
                            crouched: false,
                            ..player.clone()
                        })
                    } else {
                        self.motion.ticked(*owner).cloned()
                    },
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
            let (yaw, pitch) = self.controls.camera_angles();
            let forward = Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                -yaw.cos() * pitch.cos(),
            );
            let eye = self
                .motion
                .local_eye()
                .unwrap_or_else(|| local.eye(&local.tuning()));
            let chase = third_person
            .then(|| {
                Self::chase_camera(
                    &self.vehicle_assets,
                    &self.vehicles,
                    self.chase_lag,
                    view,
                    local,
                )
            })
            .flatten();
            let eye = camera_eye(
                &self.controls,
                presented,
                building,
                chase.map_or(eye, |(_, pivot, _)| pivot),
                forward,
                third_person.then(|| chase.map_or(8.0, |(distance, ..)| distance)),
            )?;
            listener = bri_audio::Listener {
                position: eye.to_array(),
                forward: forward.to_array(),
                up: Vec3::Y.to_array(),
            };
            let (local_view_yaw, local_view_pitch) = self.controls.view_angles();
            self.world_items.set_palette(&view.world.palette);
            self.weapon_effects.set_palette(&view.world.palette);
            self.world_items.sync(
                &view.weapons,
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
                        // Torque mounts an image whose mount point has no
                        // `mountN` node (the dribbled basketball's Mount8) at
                        // the player's own transform.
                        mounts: (0..32)
                            .map(|n| {
                                let node =
                                    avatar.world_node(&self.avatar_assets, &format!("Mount{n}"));
                                (n, node.unwrap_or_else(|| avatar.body_transform()))
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
                // `fxLight::TestLOS` casts from the camera to the flare,
                // ignoring the player carrying it.
                |at| {
                    Ok(eye.distance(at) < bri_fx_runtime::FLARE_MAX_DISTANCE
                        && building.effect_visible(bri_world::BrickId::MAX, eye, at)?)
                },
            )?;
            self.explosion_shapes.advance(elapsed.as_secs_f32());
            let shells: Vec<_> = self
                .weapon_effects
                .take_host_requests()
                .filter_map(|r| match r {
                    crate::weapon_effects::HostRequest::Shell(cue) => Some(cue),
                    _ => None,
                })
                .collect();
            let world_items = &self.world_items;
            let eject = |actor: u64, image: &str, hand: u8| {
                world_items
                    .mounted_node(actor, hand, image, "ejectPoint")
                    .or_else(|_| world_items.mounted_node(actor, hand, image, "muzzlePoint"))
                    .ok()
            };
            self.weapon_shells.cues(&shells, eject, |actor| {
                presented
                    .get(&actor)
                    .map_or(Vec3::ZERO, |p| Vec3::from(p.velocity))
            })?;
            self.weapon_shells
                .advance(elapsed.as_secs_f32(), eject, |from, to| {
                    let delta = to - from;
                    let length = delta.length();
                    if length < 1e-5 {
                        return None;
                    }
                    let hit = building.target(from, delta / length, length).ok()??;
                    Some(crate::weapon_debris::DebrisHit {
                        fraction: (hit.distance / length).clamp(0., 1.),
                        normal: hit.normal.normalize(),
                    })
                })?;
            self.audio
                .sync_projectiles(&view.weapons.projectiles, &self.content.weapons.pack);
            let kills = std::mem::take(&mut self.brick_kills);
            if self.brick_debris.cues(&kills, building)? > 0 {
                // Newly dead bricks are not hidden bricks to reveal.
                self.hidden_uploaded = None;
            }
            self.brick_debris
                .advance(elapsed.as_secs_f32().min(0.25), building)?;
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
            // Clicking out of the spy orbit returns to the body
            // (`Observer::onTrigger` in `Corpse` mode); the free camera
            // ignores triggers. The dead click to respawn above.
            if let Some(observer) = self.controls.observer()
                && let UiAction::Game(GameAction::Held {
                    control: HeldControl::Fire,
                    down,
                }) = action
            {
                if down && matches!(observer.mode, crate::controls::ObserverMode::Orbit(_)) {
                    if let Err(error) = self.command(id, Command::ControlPlayer, action.clone()) {
                        self.answer(id, Err(error));
                    }
                } else {
                    self.answer(id, Ok(()));
                }
                continue;
            }
            if self.local_weapon_seat()
                && let UiAction::Game(GameAction::Held {
                    control: HeldControl::Fire,
                    down,
                }) = action
            {
                // A gunner's fire drives the vehicle weapon (tank, cannon);
                // every other rider uses their tools as on foot.
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
                        self.graphics = crate::graphics::Graphics::from_settings(&value);
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
                    // `serverCmdDropCameraAtPlayer`: the server hands control
                    // to the camera; pressing again re-drops it at the eye.
                    match self.network_view() {
                        Some(view) if view.administrator => {
                            if let Some(eye) = self.motion.local_eye() {
                                self.controls.redrop_camera(eye);
                            }
                            let result = self.command(
                                id,
                                Command::Admin(bri_admin::Request::new(
                                    bri_admin::Action::DropCameraAtPlayer,
                                )),
                                action.clone(),
                            );
                            if result.is_ok() {
                                continue;
                            }
                            result
                        }
                        Some(_) => Err(anyhow::anyhow!(
                            "Only administrators can use the free camera"
                        )),
                        None => Err(anyhow::anyhow!("Not connected")),
                    }
                }
                UiAction::Game(GameAction::DropPlayerAtCamera) => {
                    match self.controls.free_camera() {
                        Some(eye) => {
                            // The body arrives facing the camera's heading.
                            let (yaw, _) = self.controls.camera_angles();
                            self.controls.yaw = yaw;
                            let result = self.command(
                                id,
                                Command::DropPlayerAt {
                                    eye: eye.to_array(),
                                    yaw,
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
                UiAction::ChatCommand { ref name, ref args } => {
                    let snapshot = self
                        .attempt
                        .as_ref()
                        .and_then(|a| a.view.as_ref())
                        .and_then(|v| v.admin_snapshot.as_ref());
                    let admin = match snapshot {
                        Some(snapshot) => crate::admin_ui::chat_command(name, args, snapshot),
                        None => Ok(None),
                    };
                    // Vanilla slash commands that map to existing requests.
                    let command = match admin {
                        Err(error) => {
                            self.answer(id, Err(error));
                            continue;
                        }
                        Ok(Some(command)) => Some(command),
                        Ok(None) => match name.to_ascii_lowercase().as_str() {
                        "suicide" | "kill" => Some(Command::Suicide),
                        "light" => Some(Command::ToggleLight),
                        "clearcheckpoint" => Some(Command::ClearCheckpoint),
                        "treasurestatus" => Some(Command::TreasureStatus),
                        "wand" => Some(Command::Wand),
                        "sit" | "love" | "hate" | "alarm" | "confusion" => {
                            Some(Command::Emote(name.to_ascii_lowercase()))
                        }
                        _ => None,
                        },
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
                UiAction::StartTyping | UiAction::StopTyping => {
                    if self.network_view().is_none() {
                        continue;
                    }
                    let talking = matches!(action, UiAction::StartTyping);
                    let result = self.command(id, Command::Talking(talking), action.clone());
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::ClosePrintSelector
                | UiAction::CancelWrench { .. } => Ok(()),
                UiAction::TrustInvite { target, level } => {
                    self.command(id, Command::TrustInvite { target, level }, action.clone())
                }
                UiAction::TrustDemote { target, level } => {
                    self.command(id, Command::DemoteTrust { target, level }, action.clone())
                }
                UiAction::UnIgnore { target } => {
                    self.command(id, Command::UnIgnore { target }, action.clone())
                }
                UiAction::AnswerTrustInvite { from, answer } => {
                    let command = match answer {
                        TrustAnswer::Accept => Command::AcceptTrust { from },
                        TrustAnswer::Reject => Command::RejectTrust { from },
                        TrustAnswer::Ignore => Command::IgnoreTrust { from },
                    };
                    self.command(id, command, action.clone())
                }
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
                UiAction::Console { ref line } => {
                    let mut out = bri_console::Output::default();
                    let unknown = crate::console::registry().exec(self, line, &mut out);
                    out.flush();
                    match unknown.first() {
                        Some(line) => Err(anyhow::anyhow!("Unknown console command: {line}")),
                        None => Ok(()),
                    }
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
        self.shell_gpu = None;
        for avatar in self.avatars.values_mut() {
            avatar.gpu = None;
        }
        self.avatar_preview = Some(crate::avatar::Preview::new(device));
        self.preview_dirty = self.preview_request.is_some();
        let samples = self.graphics.samples;
        self.renderer = Some(SceneRenderer::with_settings(
            device,
            format,
            samples,
            self.graphics.shadows,
        ));
        self.foliage.gpu_stopped();
        self.foliage.set_samples(samples);
        let weather_limits = bri_weather::WeatherLimits::default();
        self.weather_renderer = Some(bri_weather::gpu::WeatherRenderer::new(
            device,
            queue,
            self.weather.world.pack(),
            format,
            bri_render::scene::DEPTH_FORMAT,
            samples,
            weather_limits.drops + weather_limits.splashes,
        )?);
        let limits = bri_fx_runtime::EffectsLimits::default();
        self.effects_renderer = Some(bri_fx_runtime::gpu::EffectsRenderer::new(
            device,
            queue,
            self.weapon_effects.world().pack(),
            format,
            bri_render::scene::DEPTH_FORMAT,
            samples,
            limits.particles.saturating_mul(2) + limits.lights.saturating_mul(2),
        )?);
        self.gpu_scene = None;
        self.gpu_terrain.clear();
        self.gpu_palette = None;
        self.gpu_chunks.clear();
        self.ghost_gpu = None;
        self.ghost_uploaded = u64::MAX;
        self.debris_models.clear();
        self.hidden_gpu = None;
        self.hidden_uploaded = None;
        self.depth = None;
        Ok(())
    }
    fn gpu_stopped(&mut self) {
        self.item_ui.gpu_stopped();
        self.world_items.clear_gpu();
        crate::vehicles::ClientVehicles::gpu_stopped(&mut self.vehicle_assets);
        self.explosion_shapes.gpu_stopped();
        self.shell_gpu = None;
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
        self.gpu_palette = None;
        self.gpu_chunks.clear();
        self.ghost_gpu = None;
        self.ghost_uploaded = u64::MAX;
        self.debris_models.clear();
        self.hidden_gpu = None;
        self.hidden_uploaded = None;
        self.depth = None;
    }
    fn render_scene(&mut self, frame: &mut RenderContext<'_>) -> Result<bool> {
        // Anti-aliasing and shadow quality rebuild world pipelines and maps;
        // a map change needs renderers built for the new map.
        if std::mem::take(&mut self.gpu_restart)
            || self.renderer.as_ref().is_some_and(|r| {
                r.samples() != self.graphics.samples
                    || r.shadow_settings() != self.graphics.shadows
            })
        {
            self.gpu_ready(frame.device, frame.queue, frame.format)?;
        }
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
            || self.controls.observer().is_some()
            || view.vitals.get(&view.owner).is_some_and(|v| !v.alive);
        let hidden = self.combat.hidden_bodies(&view.vitals);
        let renderer = self
            .renderer
            .as_mut()
            .context("Scene GPU not initialized")?;
        renderer.set_filtering(frame.device, self.graphics.filtering);
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
        if self.gpu_palette.is_none()
            && let Some(palette) = &self.palette
        {
            self.gpu_palette = Some(renderer.upload(frame.device, frame.queue, &palette.scene)?);
            self.chunk_uploads.extend(self.cpu_chunks.keys().copied());
        }
        if let Some(palette) = &self.gpu_palette {
            for key in std::mem::take(&mut self.chunk_uploads) {
                if let Some(chunk) = self.cpu_chunks.get(&key) {
                    self.gpu_chunks
                        .insert(key, renderer.upload_chunk(frame.device, chunk, palette)?);
                }
            }
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
                    bricks: bri_world::Bricks::unit(0, ghost.clone()),
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
        if let Some(building) = &self.building
            && let (Some(meshes), Some(materials)) = (&self.meshes, &self.materials)
        {
            // v20 `showBricks` images (hammer, wrench, printer, wands, bricks)
            // reveal non-rendering bricks as ghosts.
            let show = matches!(
                building.equipment(),
                crate::building::Equipment::Brick(_)
                    | crate::building::Equipment::Hammer
                    | crate::building::Equipment::Wrench
                    | crate::building::Equipment::Printer
                    | crate::building::Equipment::Wand
            );
            if self.hidden_uploaded != Some(show) {
                self.hidden_gpu = None;
                if show {
                    let hidden = bri_net::protocol::PublicWorld {
                        name: "Non-rendering bricks".into(),
                        map_id: view.world.map_id.clone(),
                        palette: view.world.palette.clone(),
                        bricks: view
                            .world
                            .bricks
                            .iter()
                            .filter(|(id, b)| !b.visible && !self.brick_debris.is_dead(**id))
                            .map(|(id, b)| {
                                let mut b = b.clone();
                                b.visible = true;
                                (*id, b)
                            })
                            .collect(),
                    };
                    if !hidden.bricks.is_empty() {
                        let mut data = crate::world_scene::build_world_scene_materials(
                            &hidden,
                            meshes,
                            1_000_000,
                            Some(materials),
                        )?;
                        translucent_ghost(&mut data);
                        if !data.indices.is_empty() {
                            self.hidden_gpu =
                                Some(renderer.upload(frame.device, frame.queue, &data)?);
                        }
                    }
                }
                self.hidden_uploaded = Some(show);
            }
            self.debris_models.upload(
                &self.brick_debris,
                renderer,
                frame.device,
                frame.queue,
                meshes,
                materials,
                &view.world.palette,
            )?;
        }
        if self
            .depth
            .as_ref()
            .is_none_or(|(_, _, size)| *size != frame.size)
        {
            let samples = renderer.samples();
            let color = (samples > 1).then(|| {
                frame.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("multisampled world color"),
                    size: wgpu::Extent3d {
                        width: frame.size.0,
                        height: frame.size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format: frame.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
            });
            self.depth = Some((
                create_depth_samples(frame.device, frame.size.0, frame.size.1, samples),
                color,
                frame.size,
            ));
        }
        // With shadows the first-person body is posed too: it casts a
        // shadow without being drawn.
        let casts = renderer.shadow_settings().is_some();
        for (owner, avatar) in &mut self.avatars {
            if (*owner != view.owner || third_person || casts) && !hidden.contains(owner) {
                avatar.upload(renderer, frame.device, frame.queue)?;
            }
        }
        for mesh in self.mount_meshes.values_mut() {
            mesh.upload(renderer, frame.device, frame.queue)?;
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
        let shells: Vec<_> = self
            .weapon_shells
            .instances()
            .map(|i| bri_render::scene::SceneTransform {
                transform: i.transform,
                tint: i.tint,
            })
            .collect();
        if !shells.is_empty() || self.shell_gpu.is_some() {
            if self.shell_gpu.is_none() {
                let scene = renderer.upload(
                    frame.device,
                    frame.queue,
                    &self.weapon_shells.assets().shell_scene,
                )?;
                self.shell_gpu = Some((
                    scene,
                    bri_render::scene::GpuInstances::new(frame.device, 64)?,
                ));
            }
            let (_, instances) = self.shell_gpu.as_mut().unwrap();
            if instances.capacity() < shells.len() {
                *instances = bri_render::scene::GpuInstances::new(
                    frame.device,
                    shells.len().next_power_of_two(),
                )?;
            }
            instances.update(frame.queue, &shells)?;
        }
        let (yaw, pitch) = self.controls.camera_angles();
        let pitch = pitch.clamp(-1.56, 1.56);
        let forward = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        );
        let eye = self
            .motion
            .local_eye()
            .unwrap_or_else(|| local.eye(&local.tuning()));
        let chase = third_person
            .then(|| {
                Self::chase_camera(
                    &self.vehicle_assets,
                    &self.vehicles,
                    self.chase_lag,
                    view,
                    local,
                )
            })
            .flatten();
        let eye = camera_eye(
            &self.controls,
            self.motion.presented(),
            self.building
                .as_ref()
                .context("Camera collision mirror missing")?,
            chase.map_or(eye, |(_, pivot, _)| pivot),
            forward,
            third_person.then(|| chase.map_or(8.0, |(distance, ..)| distance)),
        )?;
        // `cameraTilt` turns the chase view down without moving the camera.
        let (pitch, forward) = match chase {
            Some((_, _, tilt)) if tilt != 0.0 && self.controls.observer().is_none() => {
                let pitch = (pitch - tilt).clamp(-1.56, 1.56);
                (
                    pitch,
                    Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos()),
                )
            }
            _ => (pitch, forward),
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
        let aspect = frame.size.0 as f32 / frame.size.1 as f32;
        let mut camera = Camera::perspective(
            eye.to_array(),
            (eye + forward).to_array(),
            aspect,
            vertical_fov(self.controls.fov().to_radians(), aspect),
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
        self.ui.core.name_tags = name_tags(
            view,
            self.motion.presented(),
            self.building.as_ref(),
            glam::Mat4::from_cols_array(&camera.view_projection),
            eye,
            fog_end.max(1.),
            (frame.size.0 as f32, frame.size.1 as f32),
            self.ui.scale(),
            self.controls.observer().is_none(),
        );
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
        // Player lights are effect lights too; the nearest to the camera win.
        let lights: Vec<_> = effects_frame
            .lights
            .iter()
            .map(|light| bri_render::scene::PointLight {
                position_radius: light.position.extend(light.radius).to_array(),
                color: light.color.extend(0.).to_array(),
            })
            .collect();
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
        let (depth, multisampled, _) = self.depth.as_ref().unwrap();
        let depth = depth.create_view(&Default::default());
        let multisampled = multisampled
            .as_ref()
            .map(|color| color.create_view(&Default::default()));
        let world_target = multisampled.as_ref().unwrap_or(frame.target);
        let [r, g, b, a] = scene.clear_color.map(f64::from);
        let mut scenes = vec![self.gpu_scene.as_ref().unwrap()];
        scenes.extend(self.gpu_chunks.values());
        if let Some(ghost) = &self.ghost_gpu {
            scenes.push(ghost);
        }
        if let Some(hidden) = &self.hidden_gpu {
            scenes.push(hidden);
        }
        for (owner, avatar) in &self.avatars {
            if (*owner != view.owner || third_person)
                && !hidden.contains(owner)
                && let Some(gpu) = &avatar.gpu
            {
                scenes.push(gpu);
            }
        }
        scenes.extend(self.mount_meshes.values().filter_map(|m| m.gpu.as_ref()));
        let mut item_draws = self.world_items.draws();
        item_draws.extend(crate::vehicles::ClientVehicles::draws(&self.vehicle_assets));
        item_draws.extend(self.gpu_terrain.iter().flat_map(|t| t.draws()));
        item_draws.extend(self.explosion_shapes.draws());
        if let Some((scene, instances)) = &self.shell_gpu
            && self.weapon_shells.active_count() > 0
        {
            item_draws.push((scene, instances));
        }
        item_draws.extend(self.debris_models.draws());
        {
            use bri_render::scene::ShadowCasters;
            // Players, vehicles and items (dropped and held) cast, like v20's
            // projected shape shadows; bricks only with the BrickShadows pref.
            // The map's own shadows are baked. Whatever does not cast still
            // stops shadows passing through it (see bri_render::shadow).
            let chunks: Vec<&GpuScene> = self.gpu_chunks.values().collect();
            let (mut bodies, mut blockers) = if self.graphics.brick_shadows {
                (chunks, Vec::new())
            } else {
                (Vec::new(), chunks)
            };
            blockers.extend(self.gpu_scene.as_ref());
            let terrain: Vec<_> = self.gpu_terrain.iter().flat_map(|t| t.draws()).collect();
            bodies.extend(
                self.avatars
                    .iter()
                    .filter(|(owner, _)| !hidden.contains(owner))
                    .filter_map(|(_, avatar)| avatar.gpu.as_ref()),
            );
            let mut models = self.world_items.draws();
            models.extend(crate::vehicles::ClientVehicles::draws(&self.vehicle_assets));
            if let Some((scene, instances)) = &self.shell_gpu
                && self.weapon_shells.active_count() > 0
            {
                models.push((scene, instances));
            }
            models.extend(self.debris_models.draws());
            renderer.render_shadows(
                frame.encoder,
                ShadowCasters {
                    scenes: &bodies,
                    instances: &models,
                },
                ShadowCasters {
                    scenes: &blockers,
                    instances: &terrain,
                },
            );
        }
        renderer.render_with_instances(
            frame.encoder,
            world_target,
            &depth,
            &scenes,
            &item_draws,
            Some(wgpu::Color { r, g, b, a }),
        );
        let mut pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("replicated world particles and flares"),
                // The last world pass resolves MSAA into the frame for the UI.
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: world_target,
                    depth_slice: None,
                    resolve_target: multisampled.as_ref().map(|_| frame.target),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: if multisampled.is_some() {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
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
/// Connect to IP input: an IPv4/IPv6 address with an optional port. A bare
/// address uses the default game port, as Torque's `connect` did.
fn parse_join_address(text: &str) -> Result<SocketAddr> {
    let text = text.trim();
    text.parse::<SocketAddr>()
        .or_else(|_| {
            text.trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<std::net::IpAddr>()
                .map(|ip| SocketAddr::new(ip, 28000))
        })
        .ok()
        .context("Enter an IP address and port, for example 203.0.113.10:28000")
}
#[cfg(test)]
mod tests {
    #[test]
    fn fov_is_horizontal_like_torque() {
        let aspect = 16.0 / 9.0;
        let fov_y = super::vertical_fov(90f32.to_radians(), aspect);
        // The projected width at fov_y and this aspect spans 90 degrees.
        let across = 2.0 * ((fov_y * 0.5).tan() * aspect).atan();
        assert!((across.to_degrees() - 90.0).abs() < 1e-3, "{across}");
        assert!(fov_y.to_degrees() < 60.0);
    }
    #[test]
    fn join_address_accepts_public_ips_with_or_without_port() {
        use super::parse_join_address;
        assert_eq!(
            parse_join_address(" 203.0.113.10:28001 ").unwrap().to_string(),
            "203.0.113.10:28001"
        );
        assert_eq!(
            parse_join_address("100.64.1.2").unwrap().to_string(),
            "100.64.1.2:28000"
        );
        assert_eq!(
            parse_join_address("[2001:db8::1]").unwrap().to_string(),
            "[2001:db8::1]:28000"
        );
        assert!(parse_join_address("example.com").is_err());
    }
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
            paint: None,
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
                .ends_with("effects-runtime-pack-004")
        );
        assert_eq!(app.tool_ui.server_catalog().items.len(), 21);
        assert_eq!(app.content.datablocks["ItemData"].len(), 21);
        assert_eq!(app.content.weapons.pack.items.len(), 21);
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
                paint: None,
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
            &mut BTreeMap::new(),
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
            &mut BTreeMap::new(),
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
            &mut BTreeMap::new(),
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
    fn ghost_matches_v20_temp_brick_shells() {
        let mut scene = bri_render::scene::SceneData::default();
        scene
            .materials
            .push(bri_render::scene::Material::vertex_lit("literal", 0));
        for x in [0.0, 1.0, 0.0] {
            scene.vertices.push(bri_render::scene::SceneVertex {
                position: [x, 0.0, x - 1.0],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0; 2],
                lightmap_uv: [0.0; 2],
                color: [0.4, 0.6, 0.2, 0.5],
                fx: [0.; 4],
            });
        }
        scene.indices = vec![0, 1, 2];
        scene.batches.push(bri_render::scene::MeshBatch {
            indices: 0..3,
            material: 0,
            center: [0.0; 3],
        });
        super::translucent_ghost(&mut scene);
        // Outside: paint x1.5, pushed 0.02 along the normal, forward winding.
        assert_eq!(scene.vertices[0].position, [0.0, 0.02, -1.0]);
        assert!((scene.vertices[0].color[0] - 0.6).abs() < 1e-6);
        assert!((scene.vertices[0].color[1] - 0.9).abs() < 1e-6);
        // Inside: black copy drawn first with reversed winding.
        assert_eq!(scene.vertices[3].color, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(scene.indices, vec![3, 5, 4, 0, 1, 2]);
        assert_eq!(scene.batches[0].indices, 0..6);
        let material = &scene.materials[0];
        assert_eq!(material.alpha, bri_render::scene::AlphaMode::Blend);
        assert!(material.temp_brick_flash);
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
