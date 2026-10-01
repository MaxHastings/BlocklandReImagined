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
    client::{Client, HostPin},
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
/// PlayerStandardArmor's `cameraMaxDist`, `cameraVerticalOffset` and
/// `cameraTilt`; the stock Player_* add-ons inherit them.
pub(crate) const PLAYER_CAMERA: (f32, f32, f32) = (8.0, 0.75, 0.261);
struct Prepared {
    foliage: crate::foliage::PreparedFoliage,
    map_id: String,
    waters: Vec<bri_content::water::Water>,
    scene: SceneData,
    terrain: Vec<Arc<bri_render::terrain_scene::TerrainScene>>,
    meshes: Arc<Meshes>,
    mirror_shapes: Arc<crate::mirrors::MirrorShapes>,
    materials: Arc<crate::materials::BrickMaterials>,
    palette: Arc<crate::world_chunks::BrickPalette>,
    building: crate::building::Building,
    mirror: bri_sim::prediction::CollisionMirror,
    shape_indices: BTreeMap<u32, std::ops::Range<u32>>,
    light_volume: LightVolumeState,
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
/// A background job's answer: `None` while it runs, `Some(Ok(answer))` when
/// it finished, `Some(Err(..))` when it ended without one (its thread
/// panicked or dropped the sender). Waiting on a job that can never answer
/// would leave its screen or slot stuck for the rest of the session.
fn finished<T>(receiver: &mpsc::Receiver<T>, job: &str) -> Option<Result<T>> {
    match receiver.try_recv() {
        Ok(answer) => Some(Ok(answer)),
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => {
            bri_console::warn(format!("{job} stopped without an answer"));
            Some(Err(anyhow::anyhow!(
                "{job} stopped unexpectedly; see the log"
            )))
        }
    }
}

struct WorldJob {
    receiver: mpsc::Receiver<WorldRender>,
    abort: tokio::task::AbortHandle,
    /// Easing bricks the rebuilt chunks leave out.
    left_out: BTreeSet<u64>,
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
    /// LAN and internet hosts: reachability, invite and firewall news for
    /// the host player.
    router: Option<mpsc::Receiver<HostNotice>>,
    /// How this player trusts each other player (`secureClientCmd_ClientTrust`).
    trust: BTreeMap<bri_world::OwnerId, bri_sim::session::PlayerTrust>,
    /// Loading the map the host changed to failed.
    map_failure: Option<mpsc::Receiver<String>>,
    /// The loading screen covers a map change until the new map renders.
    reloading: bool,
    /// What the host start, join or map change is doing, for the loading
    /// screen; the network client reports its part into the same one.
    progress: bri_progress::Progress,
    progress_seen: u64,
    /// Hosts: the world revision last saved under a name (or entered, or
    /// loaded). `None` takes the next revision seen.
    saved_revision: Option<u64>,
    /// Hosts: a load or map change is still rebuilding the world until this
    /// time; its changes are not the player's unsaved work.
    settling: Option<std::time::Instant>,
    /// Joins: the saved server answered with a different identity, so a
    /// failure asks whether to trust the new one.
    identity_changed: Arc<std::sync::atomic::AtomicBool>,
    /// Joins: the server's Add-Ons bring bricks, weapons or vehicles, so
    /// the game loads this package list and joins again.
    add_ons: Arc<std::sync::Mutex<Option<bri_package::packages::PackageSet>>>,
    /// Joins: the package list the server's game runs here when it differs
    /// from this client's own but brings no content to reload: whose
    /// Add-On code runs.
    joined: Arc<std::sync::Mutex<Option<bri_package::packages::PackageSet>>>,
}
struct PendingAction {
    action: UiAction,
    command: Option<Command>,
    dialog_epoch: u64,
    inspection: Option<InspectMode>,
    dialog_request: bool,
}
/// Everything a client needs to show and predict on `map` (joins and map changes).
/// The packages a view draws with: those loaded for its server when joining
/// it downloaded some, else this client's own.
/// Everything the game derives from its content that depends on which
/// Add-Ons are on: weapons, items, vehicles and the tool menus. Built at
/// startup and again when a game starts with a different Add-On list.
struct ContentParts {
    weapon_effects: crate::weapon_effects::WeaponEffects,
    actor_effects: crate::actor_effects::ActorEffects,
    explosion_shapes: crate::explosion_shapes::ExplosionShapes,
    explosion_debris: crate::explosion_debris::ExplosionDebris,
    tool_ui: crate::tool_ui::ToolUi,
    item_assets: Arc<crate::items::ItemAssets>,
    item_ui: crate::item_ui::ItemUi,
    vehicle_assets: crate::vehicles::VehicleAssets,
    world_items: crate::world_items::WorldItems,
}
/// Item icons drawn from their models, kept between runs.
const ITEM_ICONS: &str = "item-icons";
impl ContentParts {
    /// `icon_cache` keeps item icons drawn from their models
    /// (`ItemAssets::draw_icons`).
    fn build(
        content: &ClientContent,
        effects_pack: Arc<bri_fx_runtime::EffectsPack>,
        icon_cache: &Path,
    ) -> Result<Self> {
        let weapon_pack = Arc::new(content.weapons.pack.clone());
        let explosion_shapes =
            crate::explosion_shapes::ExplosionShapes::load(&weapon_pack, &content.paths.weapons)?;
        let explosion_debris = crate::explosion_debris::ExplosionDebris::new(&weapon_pack);
        // Vehicle trails bring their Add-On's own particles and emitters.
        let (actor_pack, notes) =
            crate::actor_effects::with_vehicle_effects(effects_pack.clone(), &content.vehicles)?;
        for note in notes {
            bri_console::warn(format!("Vehicle effects: {note}"));
        }
        let actor_effects = crate::actor_effects::ActorEffects::new(
            actor_pack,
            weapon_pack.clone(),
            Default::default(),
        )?;
        // Items first: an Add-On's particle textures are among theirs.
        let mut item_assets = crate::items::ItemAssets::load_with(
            &content.paths.item_presentation,
            &content.paths.weapons,
            &content.paths.weapon_extras,
        )?;
        item_assets.draw_icons(Some(icon_cache));
        let item_assets = Arc::new(item_assets);
        let weapon_effects = crate::weapon_effects::WeaponEffects::with_textures(
            effects_pack,
            weapon_pack,
            Default::default(),
            |key| item_assets.texture(key),
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
                .chain(
                    content
                        .paths
                        .bot_kinds()?
                        .into_iter()
                        .map(|k| (k.id, k.name)),
                )
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
        let item_ui = crate::item_ui::ItemUi::new(
            &item_assets,
            &content.weapons.item_choices,
            &content.ui_pack,
        )?;
        let vehicle_assets = crate::vehicles::VehicleAssets::load_with(
            &content.paths.vehicles,
            &content.paths.vehicle_extras,
        )?;
        let world_items = crate::world_items::WorldItems::new(
            item_assets.clone(),
            Arc::new(content.weapons.pack.clone()),
            Default::default(),
        )?;
        Ok(Self {
            weapon_effects,
            actor_effects,
            explosion_shapes,
            explosion_debris,
            tool_ui,
            item_assets,
            item_ui,
            vehicle_assets,
            world_items,
        })
    }
}
fn packages_for<'a>(
    own: &'a Option<Arc<bri_package_runtime::Catalog>>,
    view: &'a crate::network::View,
) -> Option<&'a bri_package_runtime::Catalog> {
    if view.mods.packages.is_empty() {
        own.as_deref()
    } else {
        Some(&view.mods)
    }
}
fn prepare_map(
    paths: &crate::content::ContentPaths,
    map: &str,
    selected: Vec<(String, u8)>,
    catalog: &bri_sim::session::ToolCatalog,
    light_cache: &std::path::Path,
) -> Result<Prepared> {
    let map = map.to_owned();
    let visual = load_map_bundle(&paths.map_bundle, &map)?;
    let mut light_volume = LightVolumeState::start(&visual.scene, light_cache);
    // The same definitions the host's session loads, Add-On bricks included.
    let definitions =
        Definitions::load_with(&paths.brick_catalog, &paths.geometry, &paths.brick_extras)?;
    let meshes = Arc::new(
        definitions
            .entries
            .iter()
            .map(|(id, def)| (id.clone(), def.mesh.clone()))
            .collect(),
    );
    let mirror_shapes = Arc::new(crate::mirrors::shapes(&definitions));
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
    mirror.set_breakables(&native_map.breakables);
    light_volume.set_light_shapes(&native_map.breakables);
    let mut building = crate::building::Building::new(definitions, native_map.colliders)?;
    building.set_breakables(&native_map.breakables);
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
    let foliage =
        crate::foliage::PreparedFoliage::load(&paths.foliage, &map, &building, &native_map.waters)?;
    Ok(Prepared {
        foliage,
        map_id: map,
        waters: native_map.waters,
        scene: visual.scene,
        terrain: visual.terrain.into_iter().map(Arc::new).collect(),
        meshes,
        mirror_shapes,
        materials,
        palette,
        building,
        mirror,
        shape_indices: visual.shape_indices,
        light_volume,
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
    /// The Blockhead's mount points, from its rig.
    body_mounts: Vec<bri_sim::archetype::MountPoint>,
    vehicle_pack: bri_vehicles::Pack,
    bot_kinds: Vec<bri_sim::bot_kind::BotKind>,
    event_catalog: bri_events::Catalog,
    event_sounds: Vec<String>,
    maps: Vec<bri_sim::session::MapListing>,
}
/// The Blockhead's model id (`m.dts`).
const BLOCKHEAD_MODEL: &str = "v20.shape.m";
impl HostSetup {
    fn session(&self, loaded: crate::content::LoadedMap) -> Result<Session> {
        let mut session = Session::new(loaded.simulation);
        session.set_lan_host(self.lan);
        session.set_tool_catalog(self.catalog.clone())?;
        session.set_weapon_pack(self.weapon_pack.clone())?;
        session.set_item_bounds(self.item_bounds.clone())?;
        session.set_avatar_catalog(self.avatar_catalog.clone())?;
        session.set_body_mount_points(BLOCKHEAD_MODEL, self.body_mounts.clone())?;
        session.set_vehicle_pack(self.vehicle_pack.clone(), self.bot_kinds.clone())?;
        session.set_event_catalog(self.event_catalog.clone(), self.event_sounds.clone())?;
        session.set_spawn_points(loaded.spawn_points)?;
        session.set_breakables(loaded.breakables)?;
        session.set_map_list(self.maps.clone())?;
        if let Some(tutorial) = loaded.tutorial {
            session.set_tutorial(tutorial)?;
        }
        Ok(session)
    }
}
/// Request ID for unsolicited state reports; their replies are not awaited.
const REPORT_REQUEST: RequestId = RequestId::MAX;
/// How often a moving ghost brick is reported to the server.
const GHOST_REPORT_INTERVAL: Duration = Duration::from_millis(100);

pub struct App {
    /// Movement the server's map rules currently allow (the Tutorial's lessons).
    abilities: bri_sim::session::Abilities,
    /// Last brick inventory state reported to the server.
    brick_hand: Option<bri_sim::session::BrickHand>,
    /// Last ghost brick reported to the server, and when.
    ghost_report: Option<(Option<bri_sim::session::GhostBrick>, std::time::Instant)>,
    /// Other players' ghost bricks as uploaded, by owner.
    remote_ghosts: BTreeMap<bri_world::OwnerId, (bri_sim::session::GhostBrick, Option<GpuScene>)>,
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
    /// Runs a hosted game's server (its tick, peers and saves) on threads of
    /// its own, so a heavy tick never holds up this client's networking,
    /// file jobs or the host player's own connection.
    host_runtime: tokio::runtime::Runtime,
    attempt: Option<Attempt>,
    cpu_scene: Option<SceneData>,
    /// Steering prefs last sent to this session (`SteeringPrefsEvent`).
    steering_sent: Option<(RequestId, (bool, bool))>,
    /// Whether the UI was last told to hide the crosshair.
    crosshair_hidden: bool,
    /// The held tool's `wheel` command: while its trigger is held, it takes
    /// the mouse wheel (`UiUpdate::ToolWheel`).
    tool_wheel: Option<String>,
    /// The aim takes the mouse wheel (`Controls::aim_takes_wheel`).
    aim_wheel: bool,
    /// The scope overlay shown (`ItemUi::scope_overlay`).
    scope_overlay: Option<(u64, f32)>,
    cpu_terrain: Vec<Arc<bri_render::terrain_scene::TerrainScene>>,
    renderer: Option<crate::gpu_build::Building<SceneRenderer>>,
    effects: crate::effects::WorldEffects,
    weapon_effects: crate::weapon_effects::WeaponEffects,
    actor_effects: crate::actor_effects::ActorEffects,
    explosion_shapes: crate::explosion_shapes::ExplosionShapes,
    /// Add-On beams: tracers, lasers.
    beams: crate::beams::Beams,
    /// The Tutorial's target practice targets.
    tutorial_targets: crate::tutorial_targets::TutorialTargets,
    /// Pieces thrown by explosions with `debris` (vehicle wrecks, tank shells).
    explosion_debris: crate::explosion_debris::ExplosionDebris,
    /// Presentation faults absorbed instead of closing the game.
    pub cosmetic_faults: crate::cosmetic::CosmeticFaults,
    /// Ejected gun casings (`stateEjectShell`) and their GPU model.
    weapon_shells: crate::weapon_debris::WeaponDebris,
    shell_gpu: Option<(GpuScene, bri_render::scene::GpuInstances)>,
    weapon_cues: VecDeque<(bri_sim::presentation::Cue, f32)>,
    weapon_cue_drops: u64,
    /// Killed-brick debris (v20 brick explosions) and its GPU models.
    brick_debris: crate::brick_debris::BrickDebris,
    debris_models: crate::brick_debris::DebrisModels,
    /// Bricks easing to a new paint colour, drawn apart from their chunks.
    brick_fades: crate::brick_fade::BrickFades,
    fade_models: crate::brick_fade::FadeModels,
    /// The easing bricks the applied chunks leave out.
    chunks_left_out: BTreeSet<u64>,
    /// Client-side mod packages (HUD panels, models) from `packages.json`.
    package_catalog: Option<Arc<bri_package_runtime::Catalog>>,
    /// Sandboxed code of enabled Add-Ons, run while a game is entered.
    client_code: crate::client_code::ClientCode,
    /// Add-On items' skins, drawn over every copy of them.
    item_skins: crate::item_skins::ItemSkins,
    /// Every enabled package including server behaviour, for hosting.
    server_packages: Option<Arc<bri_package_runtime::Catalog>>,
    /// The loaded Add-Ons are this player's own choice (packages.json, the
    /// Add-Ons screen, `enable_packages` or `apply_packages`), so hosting
    /// runs them as they are. False after a join loaded another server's.
    packages_from_tools: bool,
    /// The next join keeps the loaded content even when the server's
    /// Add-Ons bring bricks, weapons or vehicles: loading them failed, so
    /// the player joins without them rather than not at all.
    skip_add_on_reload: bool,
    /// Told to the player in chat once the next game is entered.
    join_notices: Vec<String>,
    package_models: crate::packages::PackageModels,
    brick_kills: Vec<bri_sim::presentation::Cue>,
    /// Outlines of non-rendering bricks, drawn only while a building tool is
    /// out, and whether the uploaded lines are the shown ones (None: stale).
    hidden_lines: Option<bri_render::lines::LineRenderer>,
    /// The Environment window's vignette over the world.
    vignette: Option<bri_render::vignette::VignetteRenderer>,
    /// The environment the UI was last told of, for which session.
    environment_sent: Option<(RequestId, bri_ui::models::environment::EnvironmentView)>,
    /// An Add-On's selection box (`Notice::SelectionBox`), and the box it
    /// last uploaded.
    selection_lines: Option<bri_render::lines::LineRenderer>,
    selection_uploaded: Option<Option<([f32; 3], [f32; 3])>>,
    hidden_uploaded: Option<bool>,
    /// `BrickFades::outlined` when the outlines were built: bricks fading
    /// in or out gain or lose theirs as they pass v20's alpha 0.1.
    hidden_fading: Vec<(u64, bool)>,
    weapon_light_deferred: usize,
    weapon_effect_session: Option<RequestId>,
    weapon_animation_cues: VecDeque<(bri_sim::presentation::Cue, f32, f64)>,
    weapon_animation_drops: u64,
    weapon_animation_cursor: u64,
    effects_renderer: Option<bri_fx_runtime::gpu::EffectsRenderer>,
    gpu_scene: Option<GpuScene>,
    light_volume: LightVolumeState,
    /// Map static shapes' index ranges, and the smashed ones `gpu_scene`
    /// no longer draws.
    shape_indices: BTreeMap<u32, std::ops::Range<u32>>,
    gpu_broken: BTreeSet<u32>,
    gpu_terrain: Vec<bri_render::terrain_scene::GpuTerrain>,
    /// World-pass depth and, with MSAA, the multisampled color attachment
    /// that the last world pass resolves into the frame target.
    depth: Option<(wgpu::Texture, Option<wgpu::Texture>, (u32, u32))>,
    meshes: Option<Arc<Meshes>>,
    /// Mirror bricks' definitions, and where the world's mirrors are.
    mirror_shapes: Arc<crate::mirrors::MirrorShapes>,
    mirror_index: crate::mirrors::MirrorIndex,
    /// Mirror surfaces and their reflections, for the world pass's format.
    reflections: Option<bri_render::reflection::Reflections>,
    /// The cube metal surfaces reflect, drawn around the nearest one.
    environment_probe: Option<bri_render::environment_probe::EnvironmentProbe>,
    /// Replicated bricks as independently rebuilt chunks sharing one
    /// uploaded material palette. A running job owns `chunked`.
    palette: Option<Arc<crate::world_chunks::BrickPalette>>,
    gpu_palette: Option<GpuScene>,
    chunked: crate::world_chunks::ChunkedWorld,
    cpu_chunks: HashMap<crate::world_chunks::ChunkKey, SceneData>,
    /// Where each brick of a CPU chunk is in its vertices.
    cpu_chunk_bricks: HashMap<crate::world_chunks::ChunkKey, Arc<crate::world_chunks::ChunkBricks>>,
    gpu_chunks: HashMap<crate::world_chunks::ChunkKey, GpuScene>,
    /// The same for each chunk as uploaded, which may be older.
    gpu_chunk_bricks: HashMap<crate::world_chunks::ChunkKey, Arc<crate::world_chunks::ChunkBricks>>,
    /// Dead bricks (thrown as debris or falling) the drawn chunks may still
    /// hold, with their chunk and whether its upload hides them yet. The
    /// rebuilt chunk without them lands later (100-200 ms on a big build);
    /// until then they are hidden inside the drawn chunk the frame they die.
    chunk_hides: BTreeMap<bri_world::BrickId, (crate::world_chunks::ChunkKey, bool)>,
    /// This frame's liquids, rebuilt only when they or the paint change.
    liquid_cache: Option<LiquidCache>,
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
    /// The ghost built at the origin and the one transform that places it.
    ghost_gpu: Option<(GpuScene, bri_render::scene::GpuInstances)>,
    /// What `ghost_gpu` was built from: moving the ghost only moves it.
    ghost_look: Option<GhostLook>,
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
    avatar_preview: Option<crate::gpu_build::Building<crate::avatar::Preview>>,
    preview_request: Option<(bri_content::avatar::Appearance, [f32; 3], f32)>,
    preview_dirty: bool,
    /// Each listed save's own file, whose picture Load Bricks previews.
    save_sources: HashMap<crate::save_picture::Key, PathBuf>,
    save_previews: crate::save_picture::Previews,
    /// The save picture to take with the next scene drawn.
    save_picture: Option<PathBuf>,
    /// Save pictures being read back and written.
    save_shots: crate::platform::Screenshots,
    motion: crate::motion::Motion,
    /// Projectiles, drops and package entities smoothed between host updates.
    ghosts: crate::ghosts::Ghosts,
    vehicle_assets: crate::vehicles::VehicleAssets,
    vehicles: crate::vehicles::ClientVehicles,
    /// Heading of the vehicle the local player rides, last frame.
    mount_heading: Option<f32>,
    /// The vehicle seat the local player sat in last frame.
    seated_on: Option<(u64, u8)>,
    /// This frame's seat rotation for every mounted player.
    rider_rotations: BTreeMap<bri_world::OwnerId, glam::Quat>,
    /// This frame's first-person eye while the local player rides a vehicle
    /// or another player, from their posed `eye` node.
    rider_eye: Option<Vec3>,
    /// Where the admin, spy or death camera was last drawn from, reported
    /// to the server as the camera's transform.
    observer_eye: Option<Vec3>,
    /// The camera the last rendered frame was drawn from (eye, yaw, pitch).
    rendered_camera: Option<(Vec3, f32, f32)>,
    /// Which driven vehicle is predicted, and one whose prediction failed.
    drive_state: DriveState,
    /// The rendered camera's roll about its forward axis (a rider's
    /// first-person view tilting with the seat), radians.
    rendered_roll: f32,
    /// The controls as the last tick sampled them. The tick poses the body,
    /// the held items and the eye from these; the redraw must draw the camera
    /// from them too. Mouse motion the window loop delivers between the tick
    /// and the redraw would otherwise turn the camera by an amount the body
    /// never saw, a different amount each frame. Torque draws the control
    /// object and its camera from one move per frame (`Player::getRenderEyeTransform`,
    /// 0x5aafa0, places both the first-person camera and the mounted images).
    drawn_controls: Option<Controls>,
    /// The tumble vehicle the local player last started riding.
    tumble: Option<u64>,
    music_world: Option<Arc<bri_net::protocol::PublicWorld>>,
    /// Connection samples for the net graph and the expanded overlay.
    net_sampler: crate::perf::NetSampler,
    /// Whether a joined host has gone quiet, for the lag icon.
    lag_watch: bri_net::lag::LagWatch,
    /// When the performance overlay's slower figures are next refreshed.
    perf_stats_due: std::time::Instant,
    gpu_name: String,
    /// GPU time per world pass, in ms, from the latest timed frame: while
    /// the expanded performance overlay shows, or always once
    /// `time_gpu_passes` asks.
    gpu_passes: Vec<(&'static str, f32)>,
    time_passes: bool,
    frame_stats: crate::console::FrameStats,
    /// Minute-by-minute frame times for the session log (player sessions).
    frame_log: Option<crate::quality::FrameLog>,
    /// The start-up release check's answer, until it is shown.
    update_check: Option<mpsc::Receiver<crate::updates::Newer>>,
    /// Pick a graphics quality from the GPU if the player never has.
    auto_quality: bool,
    /// LAN listings from the last discovery query: address -> certificate.
    lan_hosts: BTreeMap<String, Vec<u8>>,
    /// Automatic rejoins tried since the connection last dropped.
    reconnects: u8,
    lan_query: Option<mpsc::Receiver<JoinList>>,
    /// Add-On import in progress: request, row id and the worker's answer.
    add_on_import: Option<(RequestId, String, mpsc::Receiver<Result<String>>)>,
    /// The Add-On list last asked for, the list that loaded without the
    /// Add-Ons that broke it, and why each was left out.
    left_out_add_ons: Option<(
        bri_package::packages::PackageSet,
        bri_package::packages::PackageSet,
        Vec<String>,
    )>,
    /// The invite for the game this player hosts (`/invite` copies it).
    invite: Option<String>,
    /// The elevated firewall helper's outcome.
    firewall_fix: Option<mpsc::Receiver<Result<(), String>>>,
    /// The frame cap the platform was last given (startup, then saves), so
    /// a save that leaves it alone sends no window command.
    frame_limit: Option<u32>,
    macro_recording: Option<Vec<UiAction>>,
    build_macro: Vec<UiAction>,
    macro_playback: VecDeque<UiAction>,
    combat: CombatPresentation,
    saves: crate::saves::Store,
    file_jobs: crate::saves::Jobs,
    /// v20 `.bls` saves players brought over; converting starts with the
    /// first frame, once startup has settled which Add-Ons are on.
    old_saves: std::sync::Arc<crate::old_saves::OldSaves>,
    old_saves_started: bool,
    /// A save list read because converted saves arrived while a save
    /// dialog was open.
    save_refresh: Option<std::sync::mpsc::Receiver<Result<Vec<crate::saves::Entry>, String>>>,
    /// A read save waiting on `LoadBricksColorGui`'s choice.
    color_load: Option<(crate::saves::Request, Box<bri_world::build::SavedBuild>)>,
    /// Transport tasks still stopping a host and keeping its world; quitting
    /// waits for them.
    closing: Vec<tokio::task::JoinHandle<()>>,
    /// When the window's close button last asked about unsaved changes.
    close_asked: Option<std::time::Instant>,
}
/// News for a hosting player from background checks.
enum HostNotice {
    /// Internet host: router, public address and reachability.
    Reach(bri_net::reach::Report),
    /// LAN host: the invite on the home network.
    Lan { invite: String },
    Firewall {
        status: crate::firewall::Status,
        port: u16,
    },
}

/// What the Join Server list found: LAN games and the saved servers, each
/// probed over its game port.
#[derive(Default)]
struct JoinList {
    lan: Vec<(SocketAddr, bri_net::discovery::Beacon)>,
    saved: Vec<(
        crate::servers::SavedServer,
        Result<bri_net::client::Probe, String>,
    )>,
}

/// Put `text` on the system clipboard.
fn copy_to_clipboard(text: &str) -> Result<()> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text.to_string()))
        .map_err(|error| anyhow::anyhow!("Could not use the clipboard: {error}"))
}

impl App {
    /// Chat lines (and an optional question) for a host notice.
    fn host_notice(&mut self, notice: HostNotice) -> Vec<(String, Option<UiUpdate>)> {
        match notice {
            HostNotice::Reach(report) => {
                bri_console::echo(format!("Hosting check: {report:?}"));
                let mut lines: Vec<_> = report.lines().into_iter().map(|l| (l, None)).collect();
                if let Some(invite) = report.invite.clone()
                    && matches!(
                        report.verdict,
                        bri_net::reach::Verdict::Reachable | bri_net::reach::Verdict::Likely
                    )
                {
                    let copied = copy_to_clipboard(&invite).is_ok();
                    self.invite = Some(invite);
                    lines.push((
                        if copied {
                            "Your invite is on the clipboard; paste it to friends. Type /invite to copy it again.".into()
                        } else {
                            "Type /invite to copy your invite.".into()
                        },
                        None,
                    ));
                } else if let Some(invite) = report.invite.or(report.lan_invite) {
                    // Without a public address, the home network invite is
                    // still something to copy.
                    self.invite = Some(invite);
                    lines.push(("Type /invite to copy an invite.".into(), None));
                }
                lines
            }
            HostNotice::Lan { invite } => {
                self.invite = Some(invite);
                vec![(
                    "Players on your network see this game in Join Server. Type /invite to copy an invite for them.".into(),
                    None,
                )]
            }
            HostNotice::Firewall { status, port } => match status.advice() {
                Some(advice) => vec![(
                    advice.to_string(),
                    Some(UiUpdate::Confirm {
                        title: "Windows Firewall".into(),
                        text: "Windows Firewall would stop friends from joining your game. Let Blockland ReImagined through? Windows will ask for permission once.".into(),
                        action: Box::new(UiAction::AllowFirewall { port }),
                    }),
                )],
                None => Vec::new(),
            },
        }
    }
    /// Enable mod packages from another root than the content root (tools
    /// and tests); replaces the packages loaded at startup. Their worlds
    /// join the Start Game list.
    pub fn enable_packages(
        &mut self,
        root: &std::path::Path,
        set: &bri_package::packages::PackageSet,
    ) -> Result<()> {
        let (client, problems) = crate::packages::load_set(root, set, false);
        ensure!(
            problems.is_empty(),
            "{}",
            problems.join(
                "
"
            )
        );
        let (server, problems) = crate::packages::load_set(root, set, true);
        ensure!(
            problems.is_empty(),
            "{}",
            problems.join(
                "
"
            )
        );
        self.content.maps.retain(|m| !m.id.contains(':'));
        if let Some(catalog) = &server {
            let worlds = crate::packages::world_maps(catalog, &self.content.maps);
            self.content.maps.extend(worlds);
        }
        self.ui.apply(UiUpdate::Maps(self.content.maps.clone()));
        self.ui
            .apply(UiUpdate::GameModes(crate::packages::modes(server.as_ref())));
        self.package_catalog = client;
        self.server_packages = server;
        self.client_code = crate::client_code::ClientCode::load(root, set);
        self.packages_from_tools = true;
        Ok(())
    }
    /// The Add-Ons screen changed which Add-Ons are on: the next game uses
    /// the new list, with no restart.
    fn add_ons_changed(&mut self, mut view: AddOnsView) {
        self.packages_from_tools = false;
        let root = self.content.paths.root.clone();
        let applied = bri_package::packages::PackageSet::load_root(&root)
            .and_then(|set| self.apply_packages(&set));
        if let Err(error) = applied {
            bri_console::warn(format!("Add-On change not applied: {error:#}"));
            view.notice = format!("{} It could not be loaded: {error:#}", view.notice);
        }
        self.ui.apply(UiUpdate::AddOns(view));
    }
    /// Run with the Add-Ons `set` lists, loading again what depends on them:
    /// HUD panels, rules, game modes and worlds, and (when the list differs
    /// from the one loaded) bricks, weapons, items and vehicles. Only between
    /// games; a game in progress keeps what it started with.
    pub fn apply_packages(&mut self, set: &bri_package::packages::PackageSet) -> Result<()> {
        ensure!(
            self.attempt.is_none(),
            "Leave the game before changing Add-Ons"
        );
        let root = self.content.paths.root.clone();
        // An Add-On that broke loading this list before stays left out
        // without trying it again on every host.
        let known = self
            .left_out_add_ons
            .as_ref()
            .is_some_and(|(requested, loaded, _)| {
                requested == set && *loaded == self.content.paths.packages
            });
        if *set != self.content.paths.packages && !known {
            let (content, left_out) = ClientContent::load_leaving_out_broken(&root, set)?;
            self.left_out_add_ons = if left_out.is_empty() {
                None
            } else {
                self.notify_left_out_add_ons(&left_out);
                Some((set.clone(), content.paths.packages.clone(), left_out))
            };
            let effects_pack = bri_fx_runtime::EffectsPack::load(&content.paths.effects_runtime)?;
            let parts = ContentParts::build(&content, effects_pack, &self.state_dir.join(ITEM_ICONS))?;
            self.weapon_effects = parts.weapon_effects;
            self.actor_effects = parts.actor_effects;
            self.explosion_shapes = parts.explosion_shapes;
            self.explosion_debris = parts.explosion_debris;
            self.tool_ui = parts.tool_ui;
            self.item_assets = parts.item_assets;
            self.item_ui = parts.item_ui;
            self.vehicle_assets = parts.vehicle_assets;
            self.world_items = parts.world_items;
            let world_items = &self.world_items;
            for note in self
                .weapon_shells
                .set_casings(&content.weapons.pack, |m| world_items.has_model(m))
            {
                bri_console::warn(format!("Gun casings: {note}"));
            }
            self.ui.core.pack = content.ui_pack.clone();
            self.audio
                .set_pack_sounds(&content.weapons.pack, &content.paths.weapons);
            self.content = content;
        }
        // What actually loaded, less any Add-On left out above.
        let set = &self.content.paths.packages.clone();
        let (client, problems) = crate::packages::load_set(&root, set, false);
        let (server, more) = crate::packages::load_set(&root, set, true);
        for problem in problems.iter().chain(&more) {
            bri_console::warn(format!("Add-On problem: {problem}"));
        }
        self.content.maps.retain(|m| !m.id.contains(':'));
        if let Some(catalog) = &server {
            let worlds = crate::packages::world_maps(catalog, &self.content.maps);
            self.content.maps.extend(worlds);
        }
        self.saves =
            crate::saves::Store::new(&self.state_dir, &self.content, Some(self.old_saves.clone()));
        if self.old_saves_started {
            self.start_old_saves();
        }
        self.ui.apply(UiUpdate::Maps(self.content.maps.clone()));
        self.ui
            .apply(UiUpdate::GameModes(crate::packages::modes(server.as_ref())));
        self.ui
            .apply(UiUpdate::Datablocks(self.content.datablocks.clone()));
        self.package_catalog = client;
        self.server_packages = server;
        self.client_code = crate::client_code::ClientCode::load(&root, set);
        self.packages_from_tools = true;
        Ok(())
    }
    /// Tell the player which Add-Ons were left out and why: the game runs
    /// without them rather than not at all.
    fn notify_left_out_add_ons(&mut self, left_out: &[String]) {
        for line in left_out {
            bri_console::warn(format!("Add-On left out: {line}"));
        }
        self.ui.apply(UiUpdate::MessageBox {
            title: "Add-Ons Left Out".into(),
            text: format!(
                "These Add-Ons could not be loaded, so the game started without them. Turn them off or fix them in Add-Ons.\n\n{}",
                left_out.join("\n")
            ),
        });
    }
    /// Package HUD panels and keys from the latest replicated state.
    fn update_package_hud(&mut self) {
        let view = self
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| a.view.as_ref());
        let Some(view) = view else {
            self.ui.core.package_panels.clear();
            self.ui.core.package_keys.clear();
            return;
        };
        let Some(catalog) = packages_for(&self.package_catalog, view) else {
            self.ui.core.package_panels.clear();
            self.ui.core.package_keys.clear();
            return;
        };
        let binds = &self.ui.core.binds;
        let held = view
            .weapons
            .images
            .get(&view.owner)
            .and_then(|images| images.iter().find(|i| i.hand == 0))
            .map_or("", |i| i.image.as_str());
        let (panels, keys) =
            crate::packages::panels(catalog, &view.package_state, view.owner, held, |letter| {
                binds
                    .command_for_key(
                        bri_ui::input::Key::Letter(letter),
                        bri_ui::input::Modifiers::NONE,
                    )
                    .is_some()
            });
        self.ui.core.package_panels = panels;
        self.ui.core.package_keys = keys;
    }
    /// Bricks whose kill cues wait for this frame's debris.
    fn pending_kills(&self) -> BTreeSet<bri_world::BrickId> {
        self.brick_kills
            .iter()
            .filter_map(|cue| match cue.kind {
                bri_sim::presentation::CueKind::BrickKill { brick, .. } => Some(brick),
                _ => None,
            })
            .collect()
    }
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
        // Sitting is replicated state (`Vitals::sitting`); `/hug` is a pose
        // only the cue starts.
        if let bri_sim::presentation::CueKind::Emote { actor, name } = &cue.kind
            && name == "hug"
        {
            self.combat.hugging.insert(*actor, None);
        }
        self.audio.cue(&cue);
        if let Some(text) = caption(&cue, self.presented_local().map(|p| Vec3::from(p.feet))) {
            self.ui.apply(UiUpdate::Caption(text.into()));
        }
        // The engine explosion operation looks like v20's rocket blast.
        let cue = match &cue.kind {
            bri_sim::presentation::CueKind::Explosion { radius, .. } => {
                bri_sim::presentation::Cue {
                    kind: bri_sim::presentation::CueKind::WeaponEffect {
                        source: bri_weapons::TargetId::Map(0),
                        definition: "rocketexplosion".into(),
                        node: String::new(),
                        seconds: 0.,
                        image: None,
                        hand: None,
                        direction: None,
                        scale: (radius / 4.).clamp(0.5, 3.),
                    },
                    ..cue
                }
            }
            _ => cue,
        };
        // A beam fired with `muzzle` starts where this client draws that
        // player's muzzle.
        if let bri_sim::presentation::CueKind::Beam {
            to,
            color,
            width,
            seconds,
            muzzle,
        } = &cue.kind
        {
            let from = muzzle
                .and_then(|actor| self.world_items.held_muzzle(actor, 0))
                .unwrap_or(Vec3::from(cue.position));
            self.beams
                .add(from, Vec3::from(*to), *color, *width, *seconds);
        }
        self.actor_effects.cue(&cue);
        self.explosion_shapes.cue(&cue);
        self.explosion_debris.cue(&cue);
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
        ground: impl Fn(Vec3, Vec3, f32) -> Option<(f32, Vec3)>,
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
        // The jet exhausts straight down (`ActorEffects::advance`); v20 casts
        // its ground dust along the same axis.
        let dust: Vec<_> = jets
            .iter()
            .flat_map(|(owner, feet, _)| {
                feet.iter().zip(0u8..).filter_map(|(m, i)| {
                    let origin = m.w_axis.truncate();
                    let hit = ground(
                        origin,
                        Vec3::NEG_Y,
                        crate::actor_effects::JET_GROUND_DISTANCE,
                    );
                    crate::actor_effects::jet_dust(*owner, i, origin, Vec3::NEG_Y, hit)
                })
            })
            .collect();
        actor_effects.update_jet_dust(&dust)?;
        // A wreck burns with its own damage emitters, from the replicated
        // destroyed state alone.
        let burning: Vec<_> = view
            .vehicles
            .values()
            .filter(|info| info.destroyed)
            .filter_map(|info| {
                let at = body(info.id)?;
                let d = vehicle_assets.definition(&info.definition)?;
                Some(
                    d.wreck_emitters()
                        .into_iter()
                        .map(move |e| (info.id, e, at)),
                )
            })
            .flatten()
            .collect();
        let pose = |anchor| match anchor {
            crate::actor_effects::Anchor::Actor { actor, mount } => avatars
                .get(&actor)?
                .mount_node(assets, mount as usize),
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
        let swimmers: Vec<_> = presented
            .iter()
            .map(|(owner, player)| crate::actor_effects::Swimmer {
                actor: *owner,
                feet: Vec3::from(player.feet),
                height: bri_sim::water::body_height(
                    player,
                    &view.archetypes.tuning(player.archetype, player.scale),
                ),
                velocity: Vec3::from(player.velocity),
            })
            .collect();
        actor_effects.update_water(elapsed, &swimmers)?;
        let mut sprays = Vec::new();
        let mut trails = Vec::new();
        for (id, info) in &view.vehicles {
            let (Some(d), Some(frame)) = (
                vehicle_assets.definition(&info.definition),
                vehicles.frame(*id),
            ) else {
                continue;
            };
            sprays.extend(crate::actor_effects::tire_sprays(*id, d, frame));
            trails.extend(crate::actor_effects::vehicle_trails(*id, d, frame));
        }
        actor_effects.update_tires(&sprays)?;
        actor_effects.update_trails(&trails)?;
        // Other admins' free cameras; the controller does not see its own
        // (`firstPersonParticles = 0`).
        actor_effects.set_orbs(
            view.orbs
                .iter()
                .filter(|(owner, _)| {
                    **owner != view.owner
                        && view
                            .vitals
                            .get(owner)
                            .is_some_and(|v| v.control == bri_sim::session::ControlObject::Camera)
                })
                .map(|(owner, orb)| (*owner, Vec3::from(orb.eye)))
                .collect(),
        );
        actor_effects.advance(elapsed, pose, &jets, &burning, &lights)
    }
    fn reset_weapon_effect_session(&mut self, session: RequestId, checkpoint_cursor: u64) {
        if self.weapon_effect_session == Some(session) {
            return;
        }
        self.weapon_effects.reset(checkpoint_cursor);
        self.actor_effects.reset(checkpoint_cursor);
        self.explosion_shapes.reset(checkpoint_cursor);
        self.beams.clear();
        self.explosion_debris.reset(checkpoint_cursor);
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
        // The images in a player's hands; empty when they hold nothing.
        let identity = |owner: &u64| -> String {
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
            parts.join("|")
        };
        // An action belongs to the hands it started with: a tool's swing
        // ends when the tool changes or is put away. One a rule started
        // with empty hands (`playThread(2, armReadyBoth)`, `death1`) plays
        // on, as v20's thread 2 does, until a tool is taken out.
        for owner in view.poses.keys() {
            let current = identity(owner);
            if avatar_action_images
                .get(owner)
                .is_some_and(|old| current != *old)
            {
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
            // An image's own animation waits for that image to arrive; a
            // rule's (`image_hand: None`) plays with whatever is in hand.
            let hand_matches = image_hand.is_none_or(|hand| {
                view.weapons
                    .images
                    .get(actor)
                    .is_some_and(|images| images.iter().any(|image| image.hand == hand))
            });
            if !hand_matches {
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
            avatar_action_images.insert(*actor, current);
        }
    }
    pub fn item_assets(&self) -> &Arc<crate::items::ItemAssets> {
        &self.item_assets
    }
    /// Names of the Add-Ons whose client code runs in the game entered.
    pub fn add_on_code_running(&self) -> Vec<&str> {
        self.client_code.running()
    }
    /// Gun casings currently tumbling or resting.
    pub fn weapon_shell_count(&self) -> usize {
        self.weapon_shells.active_count()
    }
    pub fn world_item_stats(&self) -> &crate::world_items::WorldItemDiagnostics {
        &self.world_items.diagnostics
    }
    /// The drawn world items: identity, transform and instance tint.
    pub fn world_item_instances(
        &self,
    ) -> impl Iterator<
        Item = (
            crate::world_items::ItemIdentity,
            &bri_render::scene::SceneTransform,
        ),
    > {
        self.world_items.instances()
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
    /// Draws and binds the last rendered frame recorded.
    pub fn render_stats(&self) -> Option<bri_render::scene::RenderStats> {
        self.renderer.as_ref().and_then(|r| r.finished()).map(|r| r.stats())
    }
    /// Time each world pass on the GPU every frame (as the expanded
    /// performance overlay does), for benchmarks.
    pub fn time_gpu_passes(&mut self, on: bool) {
        self.time_passes = on;
    }
    /// GPU ms per world pass in the latest timed frame, in frame order.
    pub fn gpu_pass_times(&self) -> &[(&'static str, f32)] {
        &self.gpu_passes
    }
    pub fn frame_stats(&self) -> &crate::console::FrameStats {
        &self.frame_stats
    }
    /// First run: choose Low, Medium or High from the GPU and screen, and
    /// save it as the player's graphics options. Their own later choices win.
    fn pick_quality(&mut self, adapter: &wgpu::AdapterInfo) {
        if !crate::quality::first_run(&self.ui.settings().prefs) {
            return;
        }
        let screen = self.ui.core.display_modes.as_ref().map(|m| m.native);
        let quality = crate::quality::pick(adapter.device_type, screen);
        bri_console::echo(format!(
            "First run: {} graphics quality for {} ({:?}) on a {} screen. Options > Graphics changes it.",
            quality.name(),
            adapter.name,
            adapter.device_type,
            screen.map_or("unknown".into(), |(w, h)| format!("{w}x{h}")),
        ));
        self.ui.apply(UiUpdate::SetPrefs(quality.prefs()));
        // The renderer about to be built uses it; saving follows in `pump`.
        self.graphics = crate::graphics::Graphics::from_settings(&self.ui.settings());
    }
    /// The game a player started (not a test or tool): check for a newer
    /// release, pick a graphics quality on the first run, and log frame
    /// times to the session log.
    pub fn player_session(&mut self) {
        let settings = self.ui.settings();
        self.update_check = crate::updates::start(&settings);
        self.auto_quality = true;
        self.frame_log = Some(Default::default());
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
    /// Live weapon effect sources and particles (trails, muzzle and image
    /// state emitters, explosions).
    pub fn weapon_effect_counts(&self) -> (usize, usize) {
        (
            self.weapon_effects.world().source_count(),
            self.weapon_effects.world().particle_count(),
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
    /// A body's object transform (feet and facing), as drawn this frame.
    pub fn avatar_body(&self, owner: bri_world::OwnerId) -> Option<glam::Mat4> {
        Some(self.avatars.get(&owner)?.body_transform())
    }
    /// A body's action sequence and whether it is still blending in.
    pub fn avatar_action(&self, owner: bri_world::OwnerId) -> Option<(&'static str, bool)> {
        Some(self.avatars.get(&owner)?.action())
    }
    /// A body's posed node in the world, as drawn this frame.
    pub fn avatar_node(&self, owner: bri_world::OwnerId, name: &str) -> Option<glam::Mat4> {
        self.avatars
            .get(&owner)?
            .world_node(&self.avatar_assets, name)
    }
    /// The camera the last rendered frame was drawn from: eye, yaw, pitch.
    pub fn rendered_camera(&self) -> Option<(Vec3, f32, f32)> {
        self.rendered_camera
    }
    /// The last drawn view's roll, radians (see [`crate::controls::roll`]).
    pub fn rendered_roll(&self) -> f32 {
        self.rendered_roll
    }
    /// The local player's image in `hand`, placed as drawn this frame.
    pub fn held_image_transform(&self, hand: u8) -> Option<glam::Mat4> {
        let owner = self.network_view()?.owner;
        self.world_items.mounted_transform(owner, hand)
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
        Some((state, self.local_eye()))
    }
    pub fn network_view(&self) -> Option<&network::View> {
        self.attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| a.view.as_ref())
    }
    /// How many cosmetic entities this client simulates and draws, for the
    /// headless performance probes.
    pub fn entity_counts(&self) -> serde_json::Value {
        let world = |w: &bri_fx_runtime::EffectsWorld| serde_json::json!({ "sources": w.source_count(), "particles": w.particle_count() });
        let drawn = self.effects_renderer.as_ref().map(|r| r.stats());
        serde_json::json!({
            "brick_effects": world(&self.effects.world),
            "brick_effects_deferred": self.effects.deferred,
            "weapon_effects": world(self.weapon_effects.world()),
            "actor_effects": world(self.actor_effects.world()),
            "particles_drawn": drawn.map_or(0, |s| s.instances),
            "particle_draw_calls": drawn.map_or(0, |s| s.draw_calls),
            "particle_upload_bytes": drawn.map_or(0, |s| s.uploaded_bytes),
            "avatars": self.avatars.len(),
            "vehicles": self.network_view().map_or(0, |v| v.vehicles.len()),
            "projectiles": self.network_view().map_or(0, |v| v.weapons.projectiles.len()),
            "explosion_debris": self.explosion_debris.models().count(),
            "shells": self.weapon_shells.active_count(),
            "brick_debris": self.brick_debris.len(),
        })
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
        let requested = bri_package::packages::PackageSet::load_root(content_root)?;
        let (mut content, left_out) =
            ClientContent::load_leaving_out_broken(content_root, &requested)?;
        let old_saves = crate::old_saves::OldSaves::new(
            state_dir.join("saves"),
            state_dir.join("converted-saves"),
        );
        let package_catalog = {
            let (catalog, problems) = crate::packages::load(&content.paths.root);
            for problem in problems {
                eprintln!("Package problem: {problem}");
            }
            catalog
        };
        let client_code = crate::client_code::ClientCode::load(
            &content.paths.root,
            &bri_package::packages::PackageSet::load_root(&content.paths.root)
                .unwrap_or_else(|_| bri_package::packages::PackageSet::base()),
        );
        // Worlds that packages provide are hosted like maps.
        let server_packages = {
            let (catalog, problems) = crate::packages::load_server(&content.paths.root);
            for problem in problems {
                eprintln!("Package problem (hosting): {problem}");
            }
            catalog
        };
        if let Some(catalog) = &server_packages {
            let worlds = crate::packages::world_maps(catalog, &content.maps);
            content.maps.extend(worlds);
        }
        let foliage = crate::foliage::ClientFoliage::load(&content.paths.foliage)?;
        let effects_pack = bri_fx_runtime::EffectsPack::load(&content.paths.effects_runtime)?;
        let effects = crate::effects::WorldEffects::new(effects_pack.clone(), Default::default())?;
        let mut weapon_shells = crate::weapon_debris::WeaponDebris::new(
            crate::weapon_debris::WeaponDebrisAssets::load(&content.paths.weapon_debris)?,
            Default::default(),
        )?;
        // Without its models the practice still runs and completes on
        // schedule; only the targets go undrawn.
        let tutorial_targets = crate::tutorial_targets::TutorialTargets::load(&content.paths.tutorial)
            .unwrap_or_else(|error| {
                eprintln!("Tutorial targets will not be drawn: {error:#}");
                Default::default()
            });
        let ContentParts {
            weapon_effects,
            actor_effects,
            explosion_shapes,
            explosion_debris,
            tool_ui,
            item_assets,
            item_ui,
            vehicle_assets,
            world_items,
        } = ContentParts::build(&content, effects_pack, &state_dir.join(ITEM_ICONS))?;
        for note in weapon_shells.set_casings(&content.weapons.pack, |m| world_items.has_model(m)) {
            bri_console::warn(format!("Gun casings: {note}"));
        }
        let mut avatar_assets = crate::avatar::AvatarAssets::load(&content.paths.avatar)?;
        avatar_assets.load_horse(&content.paths.vehicles)?;
        let avatar_assets = Arc::new(avatar_assets);
        let settings::Recovered {
            settings: mut saved,
            notice: settings_notice,
        } = settings::recover(&state_dir.join("settings.json"));
        let weather = crate::weather::ClientWeather::load(&content.paths.weather, &mut saved)?;
        let graphics = crate::graphics::Graphics::from_settings(&saved);
        let mut audio = crate::audio::ClientAudio::load(&content.paths.audio, &mut saved, output)?;
        audio.set_pack_sounds(&content.weapons.pack, &content.paths.weapons);
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
        if let Some(text) = settings_notice {
            ui.apply(UiUpdate::MessageBox {
                title: "Settings Problem".into(),
                text,
            });
        }
        ui.apply(UiUpdate::Maps(content.maps.clone()));
        ui.core.music_tracks = content.music.iter().map(|(_, name)| name.clone()).collect();
        ui.apply(UiUpdate::GameModes(crate::packages::modes(
            server_packages.as_ref(),
        )));
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
        let frame_limit = settings::startup_display(&ui.settings()).max_fps;
        ui.apply(UiUpdate::Version(crate::updates::version()));
        let mut app = Self {
            item_assets,
            item_ui,
            world_items,
            foliage,
            weather,
            weather_renderer: None,
            audio,
            ui,
            saves: crate::saves::Store::new(state_dir, &content, Some(old_saves.clone())),
            old_saves,
            old_saves_started: false,
            save_refresh: None,
            file_jobs: Default::default(),
            color_load: None,
            closing: Vec::new(),
            close_asked: None,
            content,
            controls: Controls::default(),
            state_dir: state_dir.into(),
            runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()?,
            host_runtime: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("bri-host")
                .enable_all()
                .build()?,
            attempt: None,
            abilities: Default::default(),
            brick_hand: None,
            ghost_report: None,
            remote_ghosts: BTreeMap::new(),
            cpu_scene: None,
            steering_sent: None,
            crosshair_hidden: false,
            tool_wheel: None,
            aim_wheel: false,
            scope_overlay: None,
            cpu_terrain: Vec::new(),
            renderer: None,
            effects,
            weapon_effects,
            actor_effects,
            explosion_shapes,
            beams: Default::default(),
            tutorial_targets,
            explosion_debris,
            cosmetic_faults: Default::default(),
            weapon_shells,
            shell_gpu: None,
            weapon_cues: VecDeque::new(),
            weapon_cue_drops: 0,
            brick_debris: Default::default(),
            debris_models: Default::default(),
            brick_fades: Default::default(),
            fade_models: Default::default(),
            chunks_left_out: BTreeSet::new(),
            package_catalog,
            client_code,
            item_skins: Default::default(),
            server_packages,
            packages_from_tools: false,
            skip_add_on_reload: false,
            join_notices: Vec::new(),
            package_models: Default::default(),
            brick_kills: Vec::new(),
            hidden_lines: None,
            vignette: None,
            environment_sent: None,
            selection_lines: None,
            selection_uploaded: None,
            hidden_uploaded: None,
            hidden_fading: Vec::new(),
            weapon_light_deferred: 0,
            weapon_effect_session: None,
            weapon_animation_cues: VecDeque::new(),
            weapon_animation_drops: 0,
            weapon_animation_cursor: 0,
            effects_renderer: None,
            gpu_scene: None,
            light_volume: LightVolumeState::default(),
            shape_indices: BTreeMap::new(),
            gpu_broken: BTreeSet::new(),
            gpu_terrain: Vec::new(),
            depth: None,
            meshes: None,
            mirror_shapes: Default::default(),
            mirror_index: Default::default(),
            reflections: None,
            environment_probe: None,
            palette: None,
            gpu_palette: None,
            chunked: Default::default(),
            cpu_chunks: HashMap::new(),
            cpu_chunk_bricks: HashMap::new(),
            gpu_chunks: HashMap::new(),
            gpu_chunk_bricks: HashMap::new(),
            chunk_hides: BTreeMap::new(),
            liquid_cache: None,
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
            ghost_look: None,
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
            save_sources: HashMap::new(),
            save_previews: Default::default(),
            save_picture: None,
            save_shots: Default::default(),
            motion: Default::default(),
            ghosts: Default::default(),
            vehicle_assets,
            vehicles: Default::default(),
            mount_heading: None,
            seated_on: None,
            rider_rotations: BTreeMap::new(),
            rider_eye: None,
            observer_eye: None,
            rendered_camera: None,
            drive_state: DriveState::default(),
            rendered_roll: 0.0,
            drawn_controls: None,
            tumble: None,
            music_world: None,
            net_sampler: Default::default(),
            lag_watch: Default::default(),
            perf_stats_due: std::time::Instant::now(),
            gpu_name: String::new(),
            gpu_passes: Vec::new(),
            time_passes: false,
            frame_stats: Default::default(),
            frame_log: None,
            update_check: None,
            auto_quality: false,
            lan_hosts: BTreeMap::new(),
            reconnects: 0,
            lan_query: None,
            add_on_import: None,
            left_out_add_ons: None,
            invite: None,
            firewall_fix: None,
            frame_limit,
            macro_recording: None,
            build_macro: Vec::new(),
            macro_playback: VecDeque::new(),
            combat: Default::default(),
        };
        if !left_out.is_empty() {
            // Catalogs, rules and client code follow the list that loaded.
            let loaded = app.content.paths.packages.clone();
            app.left_out_add_ons = Some((requested.clone(), loaded, left_out.clone()));
            if let Err(error) = app.apply_packages(&requested) {
                bri_console::warn(format!("Add-Ons not applied: {error:#}"));
            }
            app.notify_left_out_add_ons(&left_out);
        }
        Ok(app)
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
        self.invite = None;
        self.ui.core.name_tags.clear();
        self.scene_map = None;
        self.abilities = Default::default();
        self.brick_hand = None;
        self.ghost_report = None;
        self.remote_ghosts.clear();
        self.foliage.clear();
        self.weather.clear();
        self.audio.clear();
        self.effects.clear();
        self.weapon_effects.reset(0);
        self.actor_effects.reset(0);
        self.explosion_shapes.reset(0);
        self.beams.clear();
        self.tutorial_targets.update(&[], 0.0);
        self.explosion_debris.reset(0);
        self.weapon_shells.clear();
        self.weapon_cues.clear();
        self.weapon_animation_cues.clear();
        self.weapon_animation_drops = 0;
        self.weapon_animation_cursor = 0;
        self.weapon_cue_drops = 0;
        self.brick_debris.clear();
        self.debris_models.clear();
        self.fade_models.clear();
        self.package_models.clear();
        self.brick_kills.clear();
        if let Some(lines) = &mut self.hidden_lines {
            lines.clear();
        }
        self.hidden_uploaded = None;
        if let Some(lines) = &mut self.selection_lines {
            lines.clear();
        }
        self.selection_uploaded = None;
        self.weapon_light_deferred = 0;
        self.weapon_effect_session = None;
        self.world_items.reset();
        if let Some(mut attempt) = self.attempt.take() {
            self.closing.retain(|task| !task.is_finished());
            self.closing.extend(attempt.worker.finish());
        }
        self.ui.apply(UiUpdate::UnsavedChanges(false));
        self.client_code.stop();
        self.avatars.clear();
        self.mount_meshes.clear();
        self.avatar_actions.clear();
        self.avatar_gestures.clear();
        self.avatar_action_images.clear();
        self.controls = Controls::default();
        self.cpu_scene = None;
        self.light_volume = LightVolumeState::default();
        self.cpu_terrain.clear();
        self.gpu_scene = None;
        self.gpu_terrain.clear();
        self.meshes = None;
        self.mirror_shapes = Default::default();
        self.mirror_index.clear();
        self.palette = None;
        self.gpu_palette = None;
        self.chunked = Default::default();
        self.cpu_chunks.clear();
        self.cpu_chunk_bricks.clear();
        self.gpu_chunks.clear();
        self.gpu_chunk_bricks.clear();
        self.chunk_hides.clear();
        self.chunk_uploads.clear();
        self.brick_fades.clear();
        self.fade_models.clear();
        self.chunks_left_out.clear();
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
        self.ghost_look = None;
        self.ghost_uploaded = u64::MAX;
        self.remote_ghosts.clear();
        self.motion.reset();
        self.ghosts.clear();
        self.vehicles.clear();
        self.music_world = None;
        self.controls.clear_observer();
        self.macro_recording = None;
        self.macro_playback.clear();
        self.combat = Default::default();
        // A save waiting on the colour question belongs to the session
        // that just ended.
        if let Some((request, _)) = self.color_load.take() {
            self.ui.core.pop(ScreenId::LoadBricksColor);
            self.answer(
                request.id,
                Err(anyhow::anyhow!(bri_ui::api::LOAD_CANCELED)),
            );
        }
    }
    /// The authoritative local player is alive (or not yet known).
    fn local_alive(&self) -> bool {
        self.network_view()
            .and_then(|v| v.vitals.get(&v.owner))
            .is_none_or(|v| v.alive)
    }
    /// The chase camera for a gunner seat with no turret player to look
    /// through: distance, pivot above the vehicle and downward view tilt.
    fn chase_camera(
        assets: &crate::vehicles::VehicleAssets,
        vehicles: &crate::vehicles::ClientVehicles,
        view: &network::View,
    ) -> Option<(f32, Vec3, f32)> {
        let (vehicle, _) = view.vitals.get(&view.owner)?.mounted?;
        let info = view.vehicles.get(&vehicle)?;
        let camera = &assets.definition(&info.definition)?.camera;
        let frame = vehicles.frame(vehicle)?;
        Some((
            camera.max_dist.clamp(1.0, 40.0),
            frame.position + Vec3::Y * camera.offset,
            camera.tilt,
        ))
    }
    /// `Player::getCameraTransform` (blocklandv20.exe 0x5ab7d0) on foot, as
    /// a horse or riding as a passenger: the pivot is the middle of the
    /// standing box over `feet` plus `cameraVerticalOffset` (0.75 while
    /// sliding in), the view is pitched down by `cameraTilt`, and the camera
    /// sits `cameraMaxDist` back along that tilted view. Box, offset and
    /// distance scale with the player. A package archetype keeps its own
    /// `camera_distance`.
    fn player_camera(
        assets: &crate::vehicles::VehicleAssets,
        archetypes: &bri_sim::archetype::Archetypes,
        local: &bri_sim::player::PlayerState,
        feet: Vec3,
        pos: f32,
    ) -> (f32, Vec3, f32) {
        let horse = archetypes.resolve(local.archetype).look.is_horse();
        let (max_dist, offset, tilt) = match assets.definition("v20.vehicle.horsearmor") {
            Some(d) if horse => (d.camera.max_dist, d.camera.offset, d.camera.tilt),
            _ => (
                archetypes.resolve(local.archetype).look.camera_distance,
                PLAYER_CAMERA.1,
                PLAYER_CAMERA.2,
            ),
        };
        let scale = local.scale;
        let stand_height = archetypes.tuning(local.archetype, scale).stand_height;
        pivot_camera(stand_height, scale, (max_dist, offset, tilt), feet, pos)
    }
    /// Predict the vehicle this client drives, as Torque runs the moves of
    /// the object a client controls on that client: the host's own vehicle
    /// code against the collision mirror, corrected from each newer pose.
    /// Player-type mounts the rider controls are predicted the same way.
    #[allow(clippy::too_many_arguments)]
    fn predict_driven(
        motion: &mut crate::motion::Motion,
        vehicles: &mut crate::vehicles::ClientVehicles,
        assets: &crate::vehicles::VehicleAssets,
        prefs: &bri_ui::prefs::Prefs,
        faults: &mut crate::cosmetic::CosmeticFaults,
        state: &mut DriveState,
        view: &network::View,
        driven: Option<u64>,
    ) {
        // The host's copy of this driver's steering prefs, which it steers
        // their moves by: predicting with it keeps the two agreeing even
        // before (or without) the host hearing the client's own.
        let wanted = driven.and_then(|id| {
            let info = view.vehicles.get(&id)?;
            let pose = view.vehicle_poses.get(&id)?;
            let d = assets.definition(&info.definition)?;
            let target = drive_target(info, d, pose.driver_steering.0)?;
            (state.refused.as_ref() != Some(&target)).then_some(())?;
            Some((target, info, pose))
        });
        let steering = steering_in_use(wanted.as_ref().map(|(_, _, pose)| *pose), prefs);
        let prefs = (!steering.0, !steering.1);
        // A new vehicle, a respawn under a new id, a changed definition or
        // scale, or leaving the seat: start again or stop.
        let target = wanted.as_ref().map(|(t, ..)| t.clone());
        if target != state.target {
            state.target = target;
            let request = wanted.as_ref().map(|(target, info, pose)| {
                let owner = view.owner;
                (
                    target.id,
                    assets.pack().clone(),
                    bri_sim::prediction::DriveSpawn {
                        spawn: bri_vehicles::Spawn {
                            id: bri_vehicles::VehicleId(target.id),
                            owner: bri_vehicles::OwnerId(owner),
                            definition: info.definition.clone(),
                            transform: Default::default(),
                            spawn_id: None,
                            respawn_ticks: None,
                            scale: info.scale,
                        },
                        seat: 0,
                        occupant: bri_vehicles::Occupant {
                            id: bri_vehicles::OccupantId(owner),
                            owner: bri_vehicles::OwnerId(owner),
                            body: [1.25, 2.65],
                        },
                        prefs,
                    },
                    pose.motion(),
                )
            });
            if faults
                .absorb("vehicle prediction", motion.drive(request))
                .is_none()
            {
                // Show the host's poses for this vehicle instead.
                state.refused = state.target.take();
                let _ = motion.drive(None);
            }
        }
        motion.set_drive_prefs(prefs);
        if let Some((_, _, pose)) = wanted {
            let corrected = motion.observe_vehicle(pose);
            if faults
                .absorb("vehicle prediction", corrected)
                .is_none()
            {
                state.refused = state.target.take();
                let _ = motion.drive(None);
            }
        }
        if driven.is_none() {
            state.refused = None;
        }
        vehicles.set_predicted(motion.driven_frame());
    }
    /// The local first-person eye: the rider's while mounted, else the
    /// smoothed predicted eye.
    fn local_eye(&self) -> Option<Vec3> {
        self.rider_eye.or(self.motion.local_eye())
    }
    /// Players and vehicles as drawn this frame, as boxes that shove
    /// client-only bodies (debris, Add-On bodies). Vehicle ids have the top
    /// bit set.
    fn pushers(
        presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
        view: &network::View,
        vehicles: &crate::vehicles::ClientVehicles,
        vehicle_assets: &crate::vehicles::VehicleAssets,
    ) -> Vec<crate::local_physics::Pusher> {
        let mut pushers: Vec<_> = presented
            .iter()
            .map(|(owner, p)| {
                let t = view.archetypes.tuning(p.archetype, p.scale);
                let height = if p.crouched {
                    t.crouch_height
                } else {
                    t.stand_height
                };
                crate::local_physics::Pusher {
                    id: *owner,
                    center: Vec3::from(p.feet) + Vec3::Y * height * 0.5,
                    rotation: glam::Quat::IDENTITY,
                    half: Vec3::new(t.width * 0.5, height * 0.5, t.width * 0.5),
                }
            })
            .collect();
        for (id, info) in &view.vehicles {
            let (Some(frame), Some(d)) = (
                vehicles.frame(*id),
                vehicle_assets.definition(&info.definition),
            ) else {
                continue;
            };
            let (min, max) = (Vec3::from(d.bounds_min), Vec3::from(d.bounds_max));
            pushers.push(crate::local_physics::Pusher {
                id: id | 1 << 63,
                center: frame.position + frame.rotation * ((min + max) * 0.5),
                rotation: frame.rotation,
                half: (max - min) * 0.5,
            });
        }
        pushers
    }
    /// The local rider's first-person eye, from their posed `eye` node
    /// (`Player::getCameraTransform` at `pos` 0, blocklandv20.exe 0x5ab7d0).
    /// The rider controlling a player-type mount (a `PlayerObjectType`
    /// mount, 0x5ab856) sees from the seat's mount node plus the eye node in
    /// the mount's frame; every other rider, including a vehicle's driver,
    /// from the eye node through their seat (`getRenderEyeTransform`).
    /// `None` on foot.
    fn rider_eye(
        avatars: &BTreeMap<bri_world::OwnerId, crate::avatar::AvatarMesh>,
        avatar_assets: &crate::avatar::AvatarAssets,
        vehicle_assets: &crate::vehicles::VehicleAssets,
        vehicles: &crate::vehicles::ClientVehicles,
        view: &network::View,
        local: &bri_sim::player::PlayerState,
    ) -> Option<Vec3> {
        let vitals = view.vitals.get(&view.owner)?;
        if vitals.mounted.is_none() && vitals.ride.is_none() {
            return None;
        }
        let avatar = avatars.get(&view.owner)?;
        let driving = vitals.mounted.and_then(|(vehicle, seat)| {
            let info = view.vehicles.get(&vehicle)?;
            let d = vehicle_assets.definition(&info.definition)?;
            let seat = usize::from(seat);
            (d.seat_role(seat) == SeatRole::Actor).then_some(())?;
            Some((vehicles.frame(vehicle)?, d.seats.get(seat)?))
        });
        let eye = match driving {
            Some((frame, seat)) => crate::vehicle_camera::driver_eye(
                frame.position,
                frame.rotation,
                Vec3::from(seat.transform.position),
                avatar.model_node(avatar_assets, "Eye")?.w_axis.truncate() * local.scale,
            ),
            None => avatar
                .animated_world_node(avatar_assets, "Eye")?
                .w_axis
                .truncate(),
        };
        eye.is_finite().then_some(eye)
    }
    /// Where the view camera is and how it looks (yaw, pitch, roll): first
    /// person, sliding out to the chase camera, or an observer camera. Only
    /// a rider's first-person view rolls, with its seat.
    #[allow(clippy::too_many_arguments)]
    /// [`Self::view_camera_here`], carried through any opening between the
    /// local body and the camera: a camera whose body's middle is not yet
    /// through (the eye leads it) or has just come out (the chase camera
    /// trails it) looks from the side the body is seen from, so walking
    /// through never cuts.
    #[allow(clippy::too_many_arguments)]
    fn view_camera(
        controls: &Controls,
        presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
        building: &crate::building::Building,
        assets: &crate::vehicles::VehicleAssets,
        vehicles: &crate::vehicles::ClientVehicles,
        view: &network::View,
        local: &bri_sim::player::PlayerState,
        first_person_eye: Vec3,
        passages: &bri_content::passage::Passages,
        drawn_offset: Option<Vec3>,
    ) -> Result<(Vec3, f32, f32, f32)> {
        let (eye, yaw, pitch, roll, boom) = Self::view_camera_here(
            controls,
            presented,
            building,
            assets,
            vehicles,
            view,
            local,
            first_person_eye,
            drawn_offset,
            passages,
        )?;
        // A free camera flies through openings itself (`Controls::fly`); an
        // orbit camera's boom went back through one, which turns its look.
        if controls.observer().is_some() {
            let (yaw, pitch, roll) = match boom {
                Some(carry) => crate::portal_view::carried_look((yaw, pitch, roll), &carry),
                None => (yaw, pitch, roll),
            };
            return Ok((eye, yaw, pitch, roll));
        }
        // A chase camera whose boom went through an opening is already
        // there; otherwise the eye leading the body's middle is carried.
        // The tilt a floor or ceiling opening left eases out after: the eye
        // starts where the carry put it and comes round.
        let middle =
            Vec3::from(local.feet) + Vec3::Y * bri_sim::player::nominal_middle(local.scale);
        let look = (yaw, pitch, roll);
        let eye = if boom.is_none() && controls.camera_pos() == 0.0 {
            middle + controls.portal_tilt() * (eye - middle)
        } else {
            eye
        };
        let (eye, (yaw, pitch, roll)) =
            crate::portal_view::through(eye, look, boom, middle, passages);
        Ok((eye, yaw, pitch, roll))
    }
    /// The view camera where the body is, and the carry of any opening the
    /// chase camera's boom went back through.
    #[allow(clippy::too_many_arguments)]
    fn view_camera_here(
        controls: &Controls,
        presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
        building: &crate::building::Building,
        assets: &crate::vehicles::VehicleAssets,
        vehicles: &crate::vehicles::ClientVehicles,
        view: &network::View,
        local: &bri_sim::player::PlayerState,
        first_person_eye: Vec3,
        drawn_offset: Option<Vec3>,
        passages: &bri_content::passage::Passages,
    ) -> Result<(Vec3, f32, f32, f32, Option<glam::Affine3A>)> {
        let look = |yaw: f32, pitch: f32| {
            Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                -yaw.cos() * pitch.cos(),
            )
        };
        let pos = controls.camera_pos();
        let seated = view.vitals.get(&view.owner).and_then(|v| v.mounted);
        // The rider of a player-type mount looks along the mount as drawn,
        // in first and third person, so the two turn together.
        let mount = seated
            .filter(|_| controls.observer().is_none())
            .and_then(|(vehicle, seat)| {
                let info = view.vehicles.get(&vehicle)?;
                let d = assets.definition(&info.definition)?;
                (d.seat_role(usize::from(seat)) == SeatRole::Actor).then_some(())?;
                Some(vehicles.frame(vehicle)?.rotation)
            });
        let (yaw, pitch) = mount.map_or_else(|| controls.camera_angles(), |m| controls.mount_look(m));
        // `minLookAngle`/`maxLookAngle`: exactly straight down and up.
        let pitch = pitch.clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
        if controls.observer().is_some() || pos == 0.0 {
            let ride = controls
                .ride_view()
                .filter(|_| controls.observer().is_none());
            let (yaw, pitch, roll) = match ride {
                Some(ride) => {
                    let (yaw, pitch) =
                        crate::controls::angles(ride * Vec3::NEG_Z, ride * Vec3::Y);
                    (yaw, pitch, crate::controls::roll(ride))
                }
                // The roll a floor or ceiling opening left, easing out.
                None if controls.observer().is_none() => (yaw, pitch, controls.portal_roll()),
                None => (yaw, pitch, 0.0),
            };
            let (eye, boom) = camera_eye(
                controls,
                presented,
                &view.entities,
                drawn_offset,
                building,
                first_person_eye,
                look(yaw, pitch),
                None,
                passages,
            )?;
            return Ok((eye, yaw, pitch, roll, boom));
        }
        let riding = seated.and_then(|(vehicle, seat)| {
            let info = view.vehicles.get(&vehicle)?;
            let d = assets.definition(&info.definition)?;
            Some((info, d, usize::from(seat), vehicles.frame(vehicle)?))
        });
        // In third person a player with a control object hands the camera to
        // it (`Player::getCameraTransform` 0x5ab80e): a vehicle's driver sees
        // its chase camera, swung round by the head's turn; the Tank gunner
        // and a horse's rider see their player-type mount's own camera.
        // Passengers have no control object and keep their own camera.
        let player_view = match riding {
            Some((_, d, seat, frame))
                if matches!(
                    d.seat_role(seat),
                    SeatRole::StrafeDriver | SeatRole::MouseDriver
                ) =>
            {
                let center = (Vec3::from(d.bounds_min) + Vec3::from(d.bounds_max)) * 0.5;
                // The boom goes back through any portal behind the vehicle,
                // as a player's chase camera's does.
                let mut boom = None;
                let (eye, yaw, pitch) = crate::vehicle_camera::driver_view(
                    frame.position,
                    frame.rotation,
                    center,
                    &d.camera,
                    controls.driver_head_yaw(),
                    pos,
                    |from, to| {
                        let (hit, through) =
                            crate::portal_view::ray(from, to, passages, |from, to| {
                                Ok(building
                                    .solid_segment(from, to)?
                                    .map(|hit| (hit.distance, hit.normal)))
                            })?;
                        boom = Some((from, through));
                        Ok(hit)
                    },
                )?;
                // The eye carried; `view_camera` turns the look with it.
                let (eye, carry) = match boom {
                    Some((from, through)) => {
                        crate::portal_view::along(&through, from.distance(eye), eye)
                    }
                    None => (eye, None),
                };
                return Ok((eye, yaw, pitch, 0.0, carry));
            }
            Some((_, d, seat, frame)) if d.seat_role(seat) == SeatRole::Actor => {
                Some(mount_camera(d, frame.position, pos))
            }
            // The gunner controls the `TankTurretPlayer` on the Tank's mount2.
            Some((_, d, seat, frame)) if d.seat_role(seat) == SeatRole::Gunner => assets
                .attachment_definition(d)
                .zip(d.attachment_mount.as_ref())
                .map(|(turret, mount)| {
                    let feet = frame.position + frame.rotation * Vec3::from(mount.position);
                    mount_camera(turret, feet, pos)
                }),
            Some((info, _, seat, _)) => vehicles
                .seat(assets, info, seat)
                .map(|(feet, _)| Self::player_camera(assets, &view.archetypes, local, feet, pos)),
            _ if seated.is_none() => Some(Self::player_camera(
                assets,
                &view.archetypes,
                local,
                Vec3::from(local.feet),
                pos,
            )),
            _ => None,
        };
        if let Some((distance, pivot, tilt)) = player_view {
            // `getCameraTransform` composes the tilt onto the eye's pitch, so
            // the chase camera keeps swinging over the head past vertical.
            let (yaw, pitch, roll) =
                crate::portal_view::leaned((yaw, pitch, controls.portal_roll()), tilt);
            // Just out of an opening in a floor or ceiling, the pivot comes
            // round from where the carry turned it (`Controls::portal_tilt`).
            let middle = Vec3::from(local.feet)
                + Vec3::Y * bri_sim::player::nominal_middle(local.scale);
            let pivot = middle + controls.portal_tilt() * (pivot - middle);
            let (eye, boom) = camera_eye(
                controls,
                presented,
                &view.entities,
                drawn_offset,
                building,
                pivot,
                look(yaw, pitch),
                Some((middle, distance)),
                passages,
            )?;
            return Ok((eye, yaw, pitch, roll, boom));
        }
        // `cameraTilt` turns the vehicle chase view down without moving the camera.
        let chase = Self::chase_camera(assets, vehicles, view);
        let (eye, boom) = camera_eye(
            controls,
            presented,
            &view.entities,
            drawn_offset,
            building,
            chase.map_or(first_person_eye, |(_, pivot, _)| pivot),
            look(yaw, pitch),
            Some((
                chase.map_or(first_person_eye, |(_, pivot, _)| pivot),
                chase.map_or(
                    view.archetypes
                        .resolve(local.archetype)
                        .look
                        .camera_distance,
                    |(distance, ..)| distance,
                ) * pos,
            )),
            passages,
        )?;
        let pitch = chase.map_or(pitch, |(_, _, tilt)| (pitch - tilt).clamp(-1.56, 1.56));
        Ok((eye, yaw, pitch, 0.0, boom))
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
        mount_meshes.retain(|id, _| horses.iter().any(|h| h.id == *id));
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
                archetype: bri_sim::player_types::PlayerType::Horse.archetype(),
                scale: 1.0,
                energy: 0.0,
                tick: Default::default(),
            };
            let input = crate::avatar::AvatarAnimationInput {
                dead: info.destroyed,
                ..Default::default()
            };
            mount_meshes
                .get_mut(&info.id)
                .unwrap()
                .pose_with_animation(avatar_assets, &state, animation_time, &input)?;
        }
        Ok(())
    }
    /// v20's lag icon (`GameConnection::setLagIcon`): shown while a joined
    /// host has sent nothing for `$Pref::Net::LagThreshold` ms. Never for the
    /// game this process hosts, which v20 skips as a "local" connection.
    fn update_lag(&mut self) {
        let joined = self
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| Some((a.id, a.worker.probes.get()?)))
            .filter(|(_, p)| p.host.is_none());
        let Some((id, probes)) = joined else {
            if self.lag_watch.lagging() {
                self.ui.apply(UiUpdate::Lagging(false));
            }
            self.lag_watch.reset();
            return;
        };
        let default = bri_net::lag::DEFAULT_LAG_THRESHOLD.as_millis() as i64;
        let threshold = self
            .ui
            .core
            .prefs
            .i64_or("$Pref::Net::LagThreshold", default)
            .clamp(1, 60_000);
        self.lag_watch.set_threshold(Duration::from_millis(threshold as u64));
        let received = probes.link.received();
        if let Some(lagging) = self.lag_watch.observe(std::time::Instant::now(), received) {
            self.ui.apply_session(id, UiUpdate::Lagging(lagging));
        }
    }
    /// Feed the net graph and performance overlay while they show; nothing
    /// is sampled while both are hidden.
    fn update_perf(&mut self) {
        let wants_net = self.ui.core.net_graph.is_some() || self.ui.core.perf.wants_net();
        let wants_stats = self.ui.core.perf.visible();
        if !wants_net && !wants_stats {
            self.net_sampler.reset();
            return;
        }
        let now = std::time::Instant::now();
        let probes = self
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| a.worker.probes.get())
            .cloned();
        let ghosts = self
            .network_view()
            .map_or(0, |v| v.poses.len() + v.vehicles.len() + v.entities.len());
        match probes.as_ref().filter(|_| wants_net) {
            Some(p) => {
                if let Some(sample) = self.net_sampler.sample(now, &p.link, ghosts) {
                    self.ui.apply(UiUpdate::NetSample(sample));
                }
            }
            None => self.net_sampler.reset(),
        }
        if !wants_stats || now < self.perf_stats_due {
            return;
        }
        self.perf_stats_due = now + Duration::from_millis(500);
        let memory = crate::perf::process_memory();
        let view = self.network_view();
        let server = probes.as_ref().and_then(|p| p.host.as_ref()).map(|host| {
            let p = host.lock().unwrap_or_else(|e| e.into_inner()).clone();
            bri_ui::models::perf::ServerStats {
                ticks_per_second: p.ticks_per_second,
                tick_ms_mean: p.tick_ms_mean,
                tick_ms_max: p.tick_ms_max,
                script_ms: p.script_ms,
            }
        });
        let stats = bri_ui::models::perf::PerfStats {
            bricks: view.map(|v| v.world.bricks.len()),
            players: view.map(|v| v.names.len()),
            vehicles: view.map(|v| v.vehicles.len()),
            entities: view.map(|v| v.entities.len()),
            memory_bytes: memory.map(|m| m.0),
            private_bytes: memory.map(|m| m.1),
            remote_server: probes.as_ref().is_some_and(|p| p.host.is_none()),
            server,
            gpu: self.gpu_name.clone(),
            gpu_passes: self
                .gpu_passes
                .iter()
                .map(|(pass, ms)| ((*pass).to_string(), *ms))
                .collect(),
        };
        self.ui.apply(UiUpdate::PerfStats(stats));
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
        let eye = self.local_eye().or_else(|| {
            view.poses
                .get(&view.owner)
                .map(|p| view.archetypes.eye(&p.player))
        });
        self.controls.follow(control, view.owner, eye);
    }
    /// The camera in control, as the server's `%client.Camera` transform:
    /// the free camera's position, or where the orbit camera was drawn from.
    fn camera_view(&self) -> Option<bri_sim::session::CameraView> {
        let observer = self.controls.observer()?;
        let eye = match observer.mode {
            crate::controls::ObserverMode::Free(position) => position,
            crate::controls::ObserverMode::Orbit(_) | crate::controls::ObserverMode::Drive(_) => {
                self.observer_eye?
            }
        };
        let view = bri_sim::session::CameraView {
            eye: eye.to_array(),
            yaw: observer.yaw,
            pitch: observer.pitch,
        };
        view.validate().ok().map(|()| view)
    }
    /// The local player's held weapon as their own game shows it: its aim
    /// zoom and whether it hides the crosshair (`Image::zoom`,
    /// `Image::crosshair`). Purely local.
    fn update_held_weapon(&mut self) {
        let view = self
            .attempt
            .as_ref()
            .filter(|a| a.entered)
            .and_then(|a| a.view.as_ref());
        let pack = &self.content.weapons.pack;
        let image = view.and_then(|view| {
            if !view.vitals.get(&view.owner).is_some_and(|v| v.alive) {
                return None;
            }
            let mounted = view.weapons.images.get(&view.owner)?;
            let mounted = mounted.iter().find(|m| m.hand == 0)?;
            pack.images.get(&mounted.image)
        });
        self.controls.set_aim(image.and_then(|i| i.zoom.clone()));
        let hidden = image.is_some_and(|i| !i.crosshair) || self.controls.aim_hides_crosshair();
        if hidden != self.crosshair_hidden {
            self.crosshair_hidden = hidden;
            self.ui.apply(UiUpdate::HideCrosshair(hidden));
        }
        // The trigger goes to the tool only on foot or in a seat that is not
        // a gunner's, and not from a camera. Whether it is held is the UI's
        // to know: it gives the tool the wheel only while it is.
        let wheel = image
            .and_then(|i| i.commands.wheel.clone())
            .filter(|_| self.controls.observer().is_none() && !self.local_weapon_seat());
        claim_wheel(&mut self.ui, &mut self.tool_wheel, wheel);
        // Aiming a scope with steps, the wheel zooms instead (`Zoom::levels`).
        let aim_wheel = self.controls.aim_takes_wheel();
        if aim_wheel != self.aim_wheel {
            self.aim_wheel = aim_wheel;
            self.ui.apply(UiUpdate::AimWheel(aim_wheel));
        }
        // A scope's picture while aiming from the eye (`Zoom::overlay`);
        // the weapon itself is not drawn behind it.
        let overlay = image
            .filter(|_| self.controls.scope_overlay().is_some())
            .and_then(|i| self.item_ui.scope_overlay(&i.id));
        if overlay != self.scope_overlay {
            self.scope_overlay = overlay;
            self.world_items.set_scoped(overlay.is_some());
            self.ui.apply(UiUpdate::ScopeOverlay(overlay));
        }
    }
    /// Dead players watch their corpse from the orbit camera.
    fn third_person_view(&self) -> bool {
        draws_third_person(&self.controls, self.local_alive())
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
        let sun = self.cpu_scene.as_ref().map(|s| s.sun_color);
        let auto_light = self.ui.core.prefs.bool_or("$pref::Input::AutoLight", true);
        let c = &mut self.combat;
        let mut updates = Vec::new();
        let mut light_on_spawn = false;
        // `showEnergyBar` datablocks show the predicted jet energy.
        let energy = self
            .motion
            .presented()
            .get(&view.owner)
            .filter(|p| view.archetypes.resolve(p.archetype).energy_bar)
            .map(|p| p.energy / view.archetypes.tuning(p.archetype, p.scale).max_energy);
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
        }
        c.died_at.retain(|owner, _| view.vitals.contains_key(owner));
        c.lights.retain(|owner, _| view.vitals.contains_key(owner));
        if let Some(local) = view.vitals.get(&view.owner) {
            if local.alive {
                if c.alive == Some(false) {
                    updates.push(UiUpdate::ClearPrints);
                }
                if c.alive != Some(true) {
                    light_on_spawn = auto_light && sun.is_some_and(dark_sun);
                }
                if local.health < c.health && c.alive == Some(true) {
                    // Armor::onDamage: flash += delta / maxDamage * 2.
                    let max = view
                        .poses
                        .get(&view.owner)
                        .map_or(bri_sim::session::MAX_HEALTH, |p| {
                            view.archetypes.resolve(p.player.archetype).max_health
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
            &view.archetypes,
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
        if light_on_spawn {
            self.ui.core.game(GameAction::UseLight);
        }
    }
    /// The name and clan tags a join sends (`onConnectRequest`'s name,
    /// `$Pref::Player::ClanPrefix` and `ClanSuffix`).
    fn join_name(&self) -> bri_net::protocol::JoinName {
        let avatar = &self.ui.settings().avatar;
        bri_net::protocol::JoinName {
            name: player_name(avatar),
            clan: clan(avatar),
        }
    }
    /// Game start: ask for a name once while it is still the stock "Blockhead".
    /// A first run asks after its controls and welcome questions instead
    /// (`Core::first_run_welcome`), so a fresh install asks once.
    pub fn prompt_for_name(&mut self) {
        // The stored settings, not `Ui::settings()`, which always fills in
        // the live binds: a fresh install has none saved until its controls
        // question is answered.
        if self.ui.core.settings.binds.is_some() {
            self.ui.core.name_prompt();
            self.ui.update(0);
        }
    }
    /// Avatar Done while connected also renames the player on the server.
    fn send_name(&mut self, prefs: &AvatarPrefs) {
        let name = player_name(prefs);
        let current = self
            .network_view()
            .and_then(|v| v.names.get(&v.owner).cloned());
        if current.as_deref() != Some(name.as_str())
            && let Some(a) = self.attempt.as_mut().filter(|a| a.entered)
        {
            let _ = a.worker.request(REPORT_REQUEST, Command::SetName(name));
        }
        // The host ignores tags it already has, so Done sends them each time.
        if let Some(a) = self.attempt.as_mut().filter(|a| a.entered) {
            let _ = a
                .worker
                .request(REPORT_REQUEST, Command::SetClan(clan(prefs)));
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn host(
        &mut self,
        id: RequestId,
        map: String,
        mode: ServerMode,
        game_mode: Option<String>,
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
        // v20's Tutorial is a single-player walkthrough (the main menu's
        // Tutorial button): one spawn and a script per player, no guests.
        let mode = if map.contains("map_tutorial") {
            ServerMode::SinglePlayer
        } else {
            mode
        };
        let admin = bri_admin::Secret::new(admin)?;
        let super_admin = bri_admin::Secret::new(super_admin)?;
        ensure!((1..=64).contains(&max_players), "Invalid player limit");
        // A host runs its own Add-On list as it is now; a game joined before
        // may have loaded another server's.
        self.disconnect();
        if !self.packages_from_tools {
            let set = bri_package::packages::PackageSet::load_root(&self.content.paths.root)?;
            self.apply_packages(&set)?;
        }
        // What runs: the chosen game mode's Add-Ons, or (Custom) every
        // enabled Add-On that fits the map. A package world stands on its
        // environment map; the packages then generate the ground.
        let hosted =
            crate::packages::hosted(self.server_packages.as_ref(), &map, game_mode.as_deref())?;
        let map = hosted.map;
        ensure!(
            self.content.maps.iter().any(|m| m.id == map),
            "This map has no usable native bundle yet"
        );
        let paths = self.content.paths.clone();
        let light_cache = self.state_dir.join("light-volumes");
        let paths_for_maps = paths.clone();
        let base_map = hosted.base_map;
        let package_world = hosted.catalog;
        let package_save = package_world.as_ref().map(|_| {
            self.state_dir.join("packages").join(format!(
                "{}.save.json",
                hosted.save_key.replace([':', '/'], "-")
            ))
        });
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
        let selected = self.content.selectable.clone();
        let avatar_catalog = self.avatar_assets.package.clone();
        let body_mounts = bri_sim::session::shape_mount_points(&self.avatar_assets.rig.shape);
        let mut catalog = self.tool_ui.server_catalog();
        // Start Game's Music Files: the loops this game's music bricks offer.
        let prefs = &self.ui.core.prefs;
        let off: std::collections::BTreeSet<&str> = self
            .content
            .music
            .iter()
            .filter(|(_, name)| !bri_ui::screens::music::music_enabled(prefs, name))
            .map(|(id, _)| id.as_str())
            .collect();
        catalog.sounds.retain(|id| !off.contains(id.as_str()));
        let player = self.join_name();
        let local_name = if name.trim().is_empty() {
            "Blockland ReImagined".into()
        } else {
            name
        };
        let single = mode == ServerMode::SinglePlayer;
        let internet = mode == ServerMode::Internet;
        let max_players = if single { 1 } else { max_players };
        // Start Game's Advanced Config: v20's saved `$Pref::Server::*`.
        let server_settings = crate::admin_ui::host_settings(
            &bri_ui::models::admin::options_from_prefs(&self.ui.core.prefs),
            &local_name,
            u16::try_from(max_players).unwrap_or(1),
        );
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
        // v20's `$Pref::Server::Port`, 28000 unless the player changed it.
        let port = u16::try_from(self.ui.core.prefs.i64_or("$Pref::Server::Port", 28000))
            .ok()
            .filter(|p| *p != 0)
            .unwrap_or(bri_net::invite::DEFAULT_PORT);
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
                status: "LOADING".into(),
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
        let progress = bri_progress::Progress::new();
        progress.set_subject(&map);
        let reporting = progress.clone();
        let host_runtime = self.host_runtime.handle().clone();
        let worker = Worker::start(self.runtime.handle(), async move {
            let identity_file = state_dir.join("client.identity");
            let native_identity = tokio::task::spawn_blocking(move || {
                bri_identity::ClientIdentity::load_or_create(identity_file)
            })
            .await??;
            reporting.begin(
                bri_progress::Stage::LoadingMap,
                bri_progress::Unit::Steps,
                None,
            );
            let permit = load_limit.acquire_owned().await?;
            let (
                loaded,
                visual,
                identity,
                catalog,
                weapon_pack,
                item_bounds,
                (vehicle_pack, bot_kinds),
            ) =
                tokio::task::spawn_blocking(move || -> Result<_> {
                    let _permit = permit;
                    let weapons = paths.weapon_content()?;
                    weapon_snapshot.ensure_same(&weapons)?;
                    let item_physics = paths.item_physics(&weapons)?;
                    physics_snapshot.ensure_same(&item_physics)?;
                    let loaded = paths.load_map(&base_map, None)?;
                    let visual = load_map_bundle(&paths.map_bundle, &base_map)?;
                    let mut light_volume = LightVolumeState::start(&visual.scene, &light_cache);
                    light_volume.set_light_shapes(&loaded.breakables);
                    // Every package this host loaded, hashed: what joiners must match.
                    let identity = paths.environment()?;
                    let vehicle_pack = paths.vehicle_pack()?;
                    let bot_kinds = paths.bot_kinds()?;
                    let meshes = Arc::new(
                        loaded
                            .simulation
                            .definitions
                            .entries
                            .iter()
                            .map(|(id, def)| (id.clone(), def.mesh.clone()))
                            .collect(),
                    );
                    let mirror_shapes = Arc::new(crate::mirrors::shapes(
                        &loaded.simulation.definitions,
                    ));
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
                    mirror.set_breakables(&loaded.breakables);
                    let mut building = crate::building::Building::new(
                        loaded.simulation.definitions.clone(),
                        loaded.query_colliders.clone(),
                    )?;
                    building.set_breakables(&loaded.breakables);
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
                        &base_map,
                        &building,
                        &waters,
                    )?;
                    Ok((
                        loaded,
                        Prepared {
                            foliage,
                            map_id: base_map.clone(),
                            waters,
                            scene: visual.scene,
                            terrain: visual.terrain.into_iter().map(Arc::new).collect(),
                            meshes,
                            mirror_shapes,
                            materials,
                            palette,
                            building,
                            mirror,
                            shape_indices: visual.shape_indices,
                            light_volume,
                        },
                        identity,
                        catalog,
                        weapons.pack,
                        item_physics.bounds,
                        (vehicle_pack, bot_kinds),
                    ))
                })
                .await??;
            scene_tx.send(visual).context("Loading cancelled")?;
            reporting.begin(
                bri_progress::Stage::StartingServer,
                bri_progress::Unit::Steps,
                None,
            );
            // Tests set BRI_TEST_HOST_PORT so a hosted test game never takes
            // the port of a real game running on this machine.
            let port = std::env::var("BRI_TEST_HOST_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(port);
            let bind: SocketAddr = if single {
                "127.0.0.1:0".parse()?
            } else {
                SocketAddr::from(([0, 0, 0, 0], port))
            };
            let setup = HostSetup {
                // v20 `$Server::LAN`: single-player and LAN hosts keep the looser
                // brick-damage rule; internet hosts use miniGameCanDamage.
                lan: !internet,
                catalog,
                weapon_pack,
                item_bounds,
                avatar_catalog,
                body_mounts,
                vehicle_pack,
                bot_kinds,
                event_catalog,
                event_sounds,
                maps: map_list,
            };
            let mut spawn_points = loaded.spawn_points.clone();
            let mut session = setup.session(loaded)?;
            session.set_server_settings(server_settings.clone())?;
            if let Some(catalog) = package_world {
                let save = match package_save.as_ref().map(std::fs::read) {
                    Some(Ok(bytes)) => Some(bri_sim::session::PackageSave::decode(&bytes)?),
                    _ => None,
                };
                let world = catalog.world().is_some();
                let generated = session.install_packages(catalog, save)?;
                if world {
                    ensure!(
                        !generated.is_empty(),
                        "The package world generated no ground to stand on"
                    );
                    spawn_points = generated;
                }
                if let Some(dir) = package_save.as_ref().and_then(|p| p.parent()) {
                    std::fs::create_dir_all(dir)?;
                }
            }
            session.set_admin_passwords(admin, super_admin)?;
            let map_loader: server::MapLoader = {
                let paths = paths_for_maps.clone();
                Arc::new(move |map: &str| {
                    // Change Map keeps the host's Server Settings.
                    let mut session = setup.session(paths.load_map(map, None)?)?;
                    session.set_server_settings(server_settings.clone())?;
                    Ok(session)
                })
            };
            // The server's tasks spawn onto the host runtime it is started in.
            let entered = host_runtime.enter();
            let mut host = server::start_with_admin_store_and_limit(
                session,
                ServerOptions {
                    bind,
                    environment: identity.clone(),
                    spawn_points,
                    // LAN hosts keep one identity so joiners' saved trust stays valid.
                    certificate: if single {
                        None
                    } else {
                        Some(server::HostCertificate::load_or_create(&state_dir)?)
                    },
                    map_loader: Some(map_loader),
                    // Joiners download the Add-Ons this host runs.
                    packages: Some(Arc::new(bri_net::packages::PackageShelf::new(
                        &paths_for_maps.root,
                        &paths_for_maps.packages,
                        &identity,
                    )?)),
                },
                max_players as usize,
                state_dir.join("administration.json"),
            )?;
            drop(entered);
            let address = SocketAddr::from(([127, 0, 0, 1], host.address.port()));
            if !single {
                // LAN players find this host (and its certificate) by broadcast;
                // Connect to IP asks the same responder directly, so internet
                // hosts answer it too.
                // Another game on this computer may hold the LAN port; this
                // one is then joined by address only.
                if let Err(error) = host
                    .advertise(listing_name, listing_map, max_players, identity.digest())
                    .await
                {
                    bri_console::warn(format!("Not listed on the LAN: {error:#}"));
                }
            }
            if !single {
                // Windows Firewall can block friends whatever the router does.
                let firewall = router_tx.clone();
                let port = host.address.port();
                std::thread::spawn(move || {
                    let status = crate::firewall::status(port);
                    let _ = firewall.send(HostNotice::Firewall { status, port });
                });
            }
            if internet {
                let reach = router_tx.clone();
                host.open_to_internet(move |report| {
                    let _ = reach.send(HostNotice::Reach(report));
                });
            } else if !single && let Some(ip) = bri_net::reach::local_ip() {
                let invite = bri_net::invite::invite(
                    SocketAddr::new(ip, host.address.port()),
                    &host.certificate,
                );
                let _ = router_tx.send(HostNotice::Lan { invite });
            }
            let client = Client::connect_reporting(
                address,
                &host.certificate,
                player,
                identity.client_packages(),
                None,
                Some(host.host_token.clone()),
                &native_identity,
                reporting,
            )
            .await?;
            Ok(Connected {
                client,
                host: Some(host),
                mods: Default::default(),
                package_save,
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
            router: (!single).then_some(router),
            trust: BTreeMap::new(),
            map_failure: None,
            reloading: false,
            progress,
            progress_seen: 0,
            saved_revision: None,
            settling: None,
            identity_changed: Default::default(),
            add_ons: Default::default(),
            joined: Default::default(),
        });
        Ok(())
    }
    /// Asked when a saved server answers with a different identity.
    fn identity_question(&self, address: &str) -> bri_ui::api::Question {
        let saved = crate::servers::SavedServers::load(&self.state_dir.join("servers.json"));
        let name = saved
            .find(address)
            .map(|s| plain_chat(&s.name))
            .filter(|name| !name.trim().is_empty())
            .map_or_else(|| address.to_string(), |name| format!("{name} ({address})"));
        bri_ui::api::Question {
            title: "Server Identity Changed".into(),
            text: format!(
                "{name} has a different identity than when you last joined.\n\nThis happens \
                 when its host reinstalls the game, but it can also mean someone else is \
                 answering at that address. Only continue if the host told you they \
                 reinstalled."
            ),
            yes: "Continue".into(),
            no: "Cancel".into(),
            on_yes: Box::new(UiAction::TrustNewServerIdentity {
                address: address.to_string(),
            }),
            on_no: None,
        }
    }
    /// Continue after an identity change: drop the saved identity and the
    /// saved invite's key, so the next join trusts what answers and saves it.
    fn forget_server_identity(&self, address: &str) -> Result<()> {
        let key = bri_net::invite::JoinTarget::parse(address)?.address();
        update_small_json(
            &self.state_dir.join("trusted-hosts.json"),
            |pins: &mut BTreeMap<String, Vec<u8>>| {
                pins.remove(&key);
            },
        )?;
        let path = self.state_dir.join("servers.json");
        let mut saved = crate::servers::SavedServers::load(&path);
        for server in &mut saved.servers {
            if server.address.eq_ignore_ascii_case(&key) {
                server.invite = None;
            }
        }
        saved.save(&path)
    }
    fn join(&mut self, id: RequestId, address: String, password: String) -> Result<()> {
        ensure!(
            password.is_empty(),
            "Password authentication is not connected yet"
        );
        let target = bri_net::invite::JoinTarget::parse(&address)?;
        let typed = target.address();
        let reload_add_ons = !std::mem::take(&mut self.skip_add_on_reload);
        // An invite's key, a LAN listing or a saved pin identifies the host;
        // a first join trusts the certificate the host presents and pins it.
        let pins_file = self.state_dir.join("trusted-hosts.json");
        let servers_file = self.state_dir.join("servers.json");
        let lan_hosts = self.lan_hosts.clone();
        let paths = self.content.paths.clone();
        let light_cache = self.state_dir.join("light-volumes");
        let player = self.join_name();
        let weapon_snapshot = self.content.weapons.clone();
        let physics_snapshot = self.content.item_physics.clone();
        let selected = self.content.selectable.clone();
        let catalog = self.tool_ui.server_catalog();
        let (scene_tx, scene) = mpsc::sync_channel(1);
        let load_limit = self.load_limit.clone();
        let identity_file = self.state_dir.join("client.identity");
        // Downloaded Add-Ons live in the game folder (a dot folder the
        // Add-Ons list skips), so their content loads like a local Add-On's.
        let package_cache = self.content.paths.root.join(".downloads");
        let add_ons = Arc::new(std::sync::Mutex::new(None));
        let needs_add_ons = add_ons.clone();
        let joined_list = Arc::new(std::sync::Mutex::new(None));
        let joined_add_ons = joined_list.clone();
        self.disconnect();
        self.ui.apply_session(
            id,
            UiUpdate::Connection(ConnectionState::Connecting {
                text: if self.reconnects > 0 {
                    format!("Connection lost. Reconnecting to {typed}…")
                } else {
                    format!("Connecting to {typed}…")
                },
            }),
        );
        let pin_key = typed.clone();
        let progress = bri_progress::Progress::new();
        let reporting = progress.clone();
        // A saved server's invite carries the key it had when joined.
        let saved_invite = crate::servers::SavedServers::load(&servers_file)
            .servers
            .iter()
            .any(|s| s.invite.as_deref() == Some(address.trim()));
        let identity_changed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let changed = identity_changed.clone();
        let worker = Worker::start(self.runtime.handle(), async move {
            let route = target.resolve().await?;
            let address = route.address;
            let pins = pins_file.clone();
            let saved_key = pin_key.clone();
            let saved = match route.key {
                Some(_) => None,
                None => {
                    tokio::task::spawn_blocking(move || {
                        read_small_json::<BTreeMap<String, Vec<u8>>>(&pins)
                            .and_then(|pins| pins.get(&saved_key).cloned())
                    })
                    .await?
                }
            };
            let (pin, had_pin) =
                crate::servers::join_pin(route.key, saved, lan_hosts.get(&address.to_string()));
            let native_identity = tokio::task::spawn_blocking(move || {
                bri_identity::ClientIdentity::load_or_create(identity_file)
            })
            .await??;
            let identity_paths = paths.clone();
            let (package_root, package_set) = (paths.root.clone(), paths.packages.clone());
            let permit = load_limit.clone().acquire_owned().await?;
            let identity = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let weapons = identity_paths.weapon_content()?;
                weapon_snapshot.ensure_same(&weapons)?;
                let item_physics = identity_paths.item_physics(&weapons)?;
                physics_snapshot.ensure_same(&item_physics)?;
                identity_paths.environment()
            })
            .await??;
            // A server running Add-Ons this client lacks, or has in another
            // version, refuses the join naming them; download them into the
            // package cache, load the server's set and join again. Nothing
            // asks the player (only sandboxed Add-On code does, after).
            let cache = bri_package::sync::Cache::open(&package_cache)?;
            let local = identity.client_packages();
            let mut mods = None;
            let joined = Client::connect_fetching(
                address,
                pin,
                player,
                local.clone(),
                None,
                &native_identity,
                &cache,
                reporting.clone(),
                |fetched, dropped| {
                    let (catalog, packages) = crate::mods::load_fetched(
                        &package_root,
                        &package_set,
                        &local,
                        fetched,
                        dropped,
                    )?;
                    mods = Some(catalog);
                    Ok(packages)
                },
            )
            .await;
            let client = match joined {
                Ok((client, fetched, dropped)) if !fetched.is_empty() || !dropped.is_empty() => {
                    let set =
                        crate::mods::joined_set(&package_root, &package_set, &fetched, &dropped);
                    let set = if reload_add_ons { Some(set?) } else { set.ok() };
                    // Content that does not resolve reloads too: applying it
                    // names the problem and the join goes ahead without it.
                    let reload = reload_add_ons
                        && set.as_ref().is_some_and(|set| {
                            crate::content::ContentPaths::resolve(&package_root, set).map_or(
                                true,
                                |fresh| {
                                    fresh.brick_extras != paths.brick_extras
                                        || fresh.weapon_extras != paths.weapon_extras
                                        || fresh.vehicle_extras != paths.vehicle_extras
                                        || fresh.bot_extras != paths.bot_extras
                                },
                            )
                        });
                    if reload {
                        client.close();
                        if let Ok(mut slot) = needs_add_ons.lock() {
                            *slot = set;
                        }
                        anyhow::bail!("Loading the server's Add-Ons");
                    }
                    // No new content, but the server's Add-On code runs for
                    // this game: the host decides which (`ClientCode`).
                    if let Ok(mut slot) = joined_add_ons.lock() {
                        *slot = set;
                    }
                    client
                }
                Ok((client, _, _)) => client,
                Err(error) => {
                    // A saved server that answers with a new identity may
                    // have reinstalled, or may not be the same host: the
                    // player decides (`TrustNewServerIdentity`). A pasted
                    // invite's key came from the host just now and stands.
                    if (had_pin || saved_invite)
                        && matches!(
                            error.downcast_ref::<bri_net::client::JoinError>(),
                            Some(bri_net::client::JoinError::IdentityChanged(_))
                        )
                    {
                        changed.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    return Err(error);
                }
            };
            let mods = std::sync::Arc::new(mods.unwrap_or_default());
            // Remember the host's certificate and the server for later joins.
            let pin = client.certificate.clone();
            let name = plain_chat(&client.listing.name);
            let _ = tokio::task::spawn_blocking(move || -> Result<()> {
                let shared = bri_net::invite::JoinTarget::Direct {
                    host: target_host(&pin_key),
                    port: address.port(),
                    key: Some(bri_net::invite::host_key(&pin)),
                };
                update_small_json(&pins_file, |pins: &mut BTreeMap<String, Vec<u8>>| {
                    if pins.len() < 1024 || pins.contains_key(&pin_key) {
                        pins.insert(pin_key.clone(), pin);
                    }
                })?;
                let mut saved = crate::servers::SavedServers::load(&servers_file);
                saved.joined(&pin_key, Some(shared.to_string()), &name, unix_now());
                saved.save(&servers_file)
            })
            .await;
            let map = client.replica.world.map_id.clone();
            ensure!(
                LOADABLE_MAPS.contains(&map.as_str()),
                "Server map has no supported native render bundle yet"
            );
            reporting.begin(
                bri_progress::Stage::LoadingMap,
                bri_progress::Unit::Steps,
                None,
            );
            let permit = load_limit.acquire_owned().await?;
            let visual = tokio::task::spawn_blocking(move || -> Result<Prepared> {
                let _permit = permit;
                prepare_map(&paths, &map, selected, &catalog, &light_cache)
            })
            .await??;
            scene_tx.send(visual).context("Loading cancelled")?;
            Ok(Connected {
                client,
                host: None,
                mods,
                package_save: None,
            })
        });
        self.attempt = Some(Attempt {
            id,
            worker,
            scene,
            name: typed.clone(),
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
            progress,
            progress_seen: 0,
            saved_revision: None,
            settling: None,
            identity_changed,
            add_ons,
            joined: joined_list,
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
        let archetypes = view.archetypes.clone();
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
        let copy_before = self.building.as_ref().and_then(|b| b.copy_pose());
        let building = self
            .building
            .as_mut()
            .context("Building controller not ready")?;
        building.set_archetypes(archetypes);
        let response = building.ui_action(action, &player)?;
        let Some(response) = response else {
            return Ok(false);
        };
        if let Some((anchor, turns)) = self.building.as_ref().and_then(|b| b.copy_pose()) {
            let cue = match copy_before {
                Some((_, before)) if before != turns => Some("brick.rotate"),
                Some((before, _)) if before != anchor => Some("brick.move"),
                _ => None,
            };
            if let Some(cue) = cue {
                self.audio.trigger(cue, bri_audio::Placement::World(anchor));
            }
        } else if let Some(ghost) = self.building.as_ref().and_then(|b| b.ghost()) {
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
            // The dialog that asked is gone, so its data is not applied, but
            // the request is still answered: a screen left waiting on it
            // (the print selector's pending print) would otherwise refuse
            // every later request.
            if let Err(reason) = &result {
                bri_console::echo(format!("Closed dialog's request refused: {reason}"));
            }
            self.ui.apply_session(
                attempt.id,
                UiUpdate::ActionResult {
                    id: request,
                    result: Ok(()),
                },
            );
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
                        | bri_sim::session::AdminData::AutoRoles(_)
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
        // A click the server refuses (nothing in hand yet, just after a
        // respawn or Change Map) shows nothing, as in v20; the reason goes
        // to the log only.
        let result = match (&pending.command, result) {
            (Some(Command::WeaponTrigger { .. }), Err(reason)) => {
                bri_console::echo(format!("Trigger ignored: {reason}"));
                Ok(())
            }
            (_, result) => result,
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
                self.show_save_files(entries);
                Ok(())
            }
            Ok(crate::saves::Outcome::Saved(path, entries)) => {
                // What the host has now is saved under a name.
                if let Some(a) = self.attempt.as_mut().filter(|a| a.local) {
                    a.saved_revision = a.view.as_ref().map(|v| v.world_revision);
                }
                // v20's save picture: the next scene drawn, without the interface.
                self.save_picture = crate::save_picture::path_for(&path);
                self.show_save_files(entries);
                Ok(())
            }
            Ok(crate::saves::Outcome::Loaded(build)) => {
                if self.attempt.as_ref().filter(|a| a.entered).map(|a| a.id) != request.session
                    || self.ui.session_request() != request.session
                {
                    Err(anyhow::anyhow!(
                        "Connection changed while reading the build; load canceled"
                    ))
                } else if matches!(request.action, UiAction::LoadBricks { .. }) {
                    // `LoadBricks_ColorCheck`: differing colours ask first.
                    let differs = self
                        .query_source
                        .as_ref()
                        .and_then(|w| crate::saves::color_difference(&w.palette, &build));
                    if let Some(append) = differs {
                        self.ui.apply(UiUpdate::ColorWarning { append });
                        self.color_load = Some((request, build));
                        return;
                    }
                    match self.send_load(request.id, build, request.action) {
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
    /// Draw the scene once more into a texture of its own, without the
    /// interface, and write it as the save picture at `path` (v20's
    /// `screenShot` after `Canvas.setContent(noHudGui)`). Waits for a frame
    /// with a scene to draw.
    fn take_save_picture(&mut self, frame: &mut RenderContext<'_>, path: PathBuf) -> Result<()> {
        let texture = frame.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Save picture frame"),
            size: wgpu::Extent3d {
                width: frame.size.0,
                height: frame.size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: frame.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let drawn = self.render_scene(&mut RenderContext {
            device: frame.device,
            queue: frame.queue,
            encoder: frame.encoder,
            target: &view,
            format: frame.format,
            size: frame.size,
            ui_renderer: frame.ui_renderer,
        })?;
        if !drawn {
            self.save_picture = Some(path);
            return Ok(());
        }
        let capture =
            crate::platform::capture_copy(frame.device, frame.encoder, &texture, frame.format)?;
        self.save_shots.copied(
            crate::platform::Shot {
                path,
                fit: Some(crate::save_picture::FIT),
            },
            capture,
        );
        Ok(())
    }
    fn show_save_files(&mut self, entries: Vec<crate::saves::Entry>) {
        self.save_sources = entries
            .iter()
            .filter_map(|e| {
                let source = e.source.clone()?;
                Some(((e.info.map.clone(), e.info.name.clone()), source))
            })
            .collect();
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
    }
    /// Convert `.bls` saves against the content now loaded.
    fn start_old_saves(&mut self) {
        self.old_saves_started = true;
        match crate::old_saves::Converter::new(&self.content) {
            Ok(converter) => {
                self.old_saves.set_converter(converter);
                self.old_saves.start();
            }
            Err(error) => bri_console::warn(format!("Old saves can't be converted: {error:#}")),
        }
    }
    /// Start converting on the first frame, and put newly converted saves in
    /// an open save dialog as they arrive.
    fn poll_old_saves(&mut self) {
        if !self.old_saves_started {
            self.start_old_saves();
        }
        if let Some(rx) = &self.save_refresh {
            match rx.try_recv() {
                Ok(Ok(entries)) => {
                    self.save_refresh = None;
                    self.show_save_files(entries);
                }
                Ok(Err(error)) => {
                    self.save_refresh = None;
                    bri_console::warn(format!("Could not list saves: {error}"));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.save_refresh = None,
            }
        }
        let open = self.ui.is_open(ScreenId::LoadBricks) || self.ui.is_open(ScreenId::SaveBricks);
        // A closed dialog lists afresh when it opens.
        if self.old_saves.take_changed() && open {
            let store = self.saves.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            self.save_refresh = Some(rx);
            self.runtime.spawn_blocking(move || {
                let _ = tx.send(store.list().map_err(|e| format!("{e:#}")));
            });
        }
    }
    fn send_load(
        &mut self,
        id: RequestId,
        build: Box<bri_world::build::SavedBuild>,
        action: UiAction,
    ) -> Result<()> {
        let UiAction::LoadBricks { ownership, .. } = action else {
            anyhow::bail!("Unexpected loaded build");
        };
        self.command(id, Command::LoadBuild { build, ownership }, action)?;
        // The loaded build arrives in batches; it matches its file.
        if let Some(a) = self.attempt.as_mut() {
            a.settling = Some(std::time::Instant::now() + SETTLE);
        }
        Ok(())
    }
    /// `ColorWarning_Click*`: load the waiting save as chosen, or leave Load
    /// Bricks open.
    fn choose_color_load(&mut self, choice: bri_ui::api::ColorLoad) {
        use bri_ui::api::ColorLoad;
        let Some((request, mut build)) = self.color_load.take() else {
            return;
        };
        let result = if choice == ColorLoad::Cancel {
            Err(anyhow::anyhow!(bri_ui::api::LOAD_CANCELED))
        } else if self.attempt.as_ref().filter(|a| a.entered).map(|a| a.id) != request.session {
            Err(anyhow::anyhow!(
                "Connection changed while reading the build; load canceled"
            ))
        } else {
            if choice == ColorLoad::Match
                && let Some(world) = &self.query_source
            {
                crate::saves::match_colors(&world.palette, &mut build);
            }
            self.send_load(request.id, build, request.action)
        };
        if let Err(error) = result {
            self.answer(request.id, Err(error));
        }
    }
    /// Put the load's progress on screen: the loading screen for a host
    /// start, a join once the host has named its map, and a map change once
    /// the new world starts arriving. Nothing changes once in game.
    fn show_progress(&mut self, a: &mut Attempt) {
        let snapshot = a.progress.snapshot();
        if snapshot.revision == a.progress_seen {
            return;
        }
        a.progress_seen = snapshot.revision;
        let Some(map) = a.progress.subject() else {
            return;
        };
        let showing = match &self.ui.core.conn {
            ConnectionState::Connecting { .. } | ConnectionState::Loading { .. } => true,
            // A map change: the host's new world has started to arrive.
            ConnectionState::InGame { .. } => {
                a.reloading
                    || (snapshot.stage == bri_progress::Stage::ReceivingWorld
                        && self.scene_map.as_deref() != Some(map.as_str()))
            }
            _ => false,
        };
        if !showing
            || (a.entered && !a.reloading && snapshot.stage != bri_progress::Stage::ReceivingWorld)
        {
            return;
        }
        let preview = self
            .content
            .maps
            .iter()
            .find(|m| m.id == map)
            .map_or(IconRef::None, |m| m.preview.clone());
        self.ui.apply_session(
            a.id,
            UiUpdate::Connection(ConnectionState::Loading {
                map,
                preview,
                status: snapshot.status(),
                progress: snapshot.fraction(),
            }),
        );
    }
    /// Paint divisions for a world palette: the content's named divisions
    /// when the world uses the default palette, numbered ones otherwise.
    fn colorset(&self, palette: &[[f32; 4]]) -> Vec<PaintDivision> {
        let default_colors: Vec<_> = self
            .content
            .paint
            .iter()
            .flat_map(|d| d.colors.iter().copied())
            .collect();
        if palette == default_colors.as_slice() {
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
        }
    }

    /// Everything the HUD takes from the map and the building controller.
    /// First entry sends it, and every map change sends it again, since the
    /// change replaces both.
    fn map_setup_updates(&self, scene: &SceneData, palette: &[[f32; 4]]) -> Result<Vec<UiUpdate>> {
        let mut updates = vec![
            UiUpdate::Bricks(self.content.bricks.clone()),
            UiUpdate::Colorset(self.colorset(palette)),
            UiUpdate::Datablocks(self.content.datablocks.clone()),
            UiUpdate::BuildingAllowed(true),
        ];
        updates.extend(self.tool_ui.catalog_updates());
        updates.extend(
            self.building
                .as_ref()
                .context("Ready connection has no building controller")?
                .initial_updates(),
        );
        // The name the save list files this map's saves under (`Store::map_name`),
        // so the dialogs open on the map being played.
        let map = self.content.maps.iter().find(|m| m.id == scene.id);
        updates.push(UiUpdate::SaveContext {
            map: map.map_or_else(|| scene.name.clone(), |m| m.name.clone()),
            preview: map.map(|m| m.preview.clone()).unwrap_or(IconRef::None),
        });
        Ok(updates)
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
            // A join knows only the typed address until the host names
            // itself; hosting keeps the name and size it was started with.
            if !a.local
                && let Some(view) = &a.view
            {
                (a.name, a.max_players) = joined_server(&view.listing, &a.name, a.max_players);
            }
        }
        self.show_progress(&mut a);
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
                    a.saved_revision = None;
                    // Movement limits belong to the old map's Tutorial; the
                    // new map sends its own if it has any.
                    self.abilities = Default::default();
                    a.settling = Some(std::time::Instant::now() + SETTLE);
                    // Load the new map's scene and prediction world; the old
                    // scene stays until it is ready.
                    let paths = self.content.paths.clone();
                    let light_cache = self.state_dir.join("light-volumes");
                    let selected = self.content.selectable.clone();
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
                    a.progress.begin(
                        bri_progress::Stage::LoadingMap,
                        bri_progress::Unit::Steps,
                        None,
                    );
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
                            status: "LOADING".into(),
                            progress: 0.0,
                        }),
                    );
                    self.runtime.spawn(async move {
                        let prepared = async {
                            let permit = load_limit.acquire_owned().await?;
                            tokio::task::spawn_blocking(move || {
                                let _permit = permit;
                                prepare_map(&paths, &map, selected, &catalog, &light_cache)
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
                            text: bri_ui::ml::sanitize(&text),
                        },
                        bri_sim::session::Notice::Center { text, seconds } => {
                            UiUpdate::CenterPrint {
                                text: print_markup(&self.ui.core.binds, &text),
                                seconds,
                            }
                        }
                        bri_sim::session::Notice::Bottom {
                            text,
                            seconds,
                            hide_bar,
                        } => UiUpdate::BottomPrint {
                            text: print_markup(&self.ui.core.binds, &text),
                            seconds,
                            hide_bar,
                        },
                        bri_sim::session::Notice::Abilities(abilities) => {
                            self.abilities = abilities;
                            continue;
                        }
                        bri_sim::session::Notice::MusicTracks(music) => {
                            self.tool_ui.offer_music(&music);
                            continue;
                        }
                        bri_sim::session::Notice::TempBrickColor(color) => {
                            if let Some(building) = self.building.as_mut() {
                                building.set_random_color(color);
                            }
                            continue;
                        }
                        bri_sim::session::Notice::Sound(profile) => {
                            self.audio.profile(&profile, bri_audio::Placement::Listener);
                            continue;
                        }
                        bri_sim::session::Notice::Fov(fov) => {
                            self.controls.set_server_fov(fov);
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
                            let saved = crate::trust_list::TrustList::load(&path).update(
                                &principal,
                                level,
                                &plain_chat(&name),
                            );
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
                        bri_sim::session::Notice::Blueprint(blueprint) => {
                            if let Some(building) = self.building.as_mut()
                                && let Err(error) = building.set_blueprint(blueprint.map(|b| *b))
                            {
                                bri_console::echo(format!("Copied build ignored: {error:#}"));
                            }
                            continue;
                        }
                        bri_sim::session::Notice::MirrorCopy { across_z } => {
                            if let Some(building) = self.building.as_mut() {
                                building.mirror_copy(across_z);
                            }
                            continue;
                        }
                        bri_sim::session::Notice::SelectionBox(outline) => {
                            if let Some(building) = self.building.as_mut()
                                && let Err(error) = building.set_outline(outline.map(|o| *o))
                            {
                                bri_console::echo(format!("Selection box ignored: {error:#}"));
                            }
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
        if let Some(mut reason) = failed {
            // A joined remote game whose network dropped is rejoined
            // automatically a few times; the host gives the player their
            // owner number, and so their bricks, back.
            let id = a.id;
            let rejoin =
                (!a.local && a.entered && reason.contains(bri_net::client::CONNECTION_LOST))
                    .then(|| a.name.clone());
            if let Some(address) = rejoin
                && self.reconnects < MAX_RECONNECTS
            {
                self.reconnects += 1;
                if self.join(id, address, String::new()).is_ok() {
                    return Ok(());
                }
            }
            self.reconnects = 0;
            // The server's Add-Ons bring content: load it and join again
            // (the downloads are cached, so this join fetches nothing).
            let add_ons = a.add_ons.lock().ok().and_then(|mut slot| slot.take());
            if let Some(set) = add_ons {
                self.disconnect();
                let applied = self.apply_packages(&set);
                // The next game this player hosts runs their own list again.
                self.packages_from_tools = false;
                // Add-Ons that do not load here are joined without: the
                // player is told which, in chat, once in the game.
                if let Err(error) = applied {
                    let text = format!(
                        "Some of this server's Add-Ons could not be loaded on this computer, so you joined without them: {error:#}"
                    );
                    bri_console::warn(&text);
                    self.join_notices.push(text);
                    self.skip_add_on_reload = true;
                }
                match self.join(id, a.name.clone(), String::new()) {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        self.join_notices.clear();
                        reason =
                            format!("Could not join again with the server's Add-Ons: {error:#}");
                        bri_console::warn(&reason);
                    }
                }
            }
            if a.identity_changed
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                let question = self.identity_question(&a.name);
                self.ui
                    .apply_session(id, UiUpdate::FailureQuestion(question));
            }
            if let Some(mismatch) = crate::add_ons::mismatch(&self.content.paths.root, &reason) {
                self.ui.apply_session(id, UiUpdate::AddOnMismatch(mismatch));
            }
            self.ui
                .apply_session(id, UiUpdate::Connection(ConnectionState::Failed { reason }));
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
            a.progress.begin(
                bri_progress::Stage::LoadingGraphics,
                bri_progress::Unit::Steps,
                None,
            );
            if self.cpu_scene.is_some() {
                // A map change: renderers keep per-map sky and terrain state,
                // so rebuild them for the new map like a fresh join.
                self.gpu_stopped();
                self.gpu_restart = true;
            }
            self.scene_map = Some(prepared.map_id.clone());
            self.foliage.set_map(prepared.foliage);
            self.weather.set_map(&prepared.map_id, prepared.waters)?;
            self.light_volume = prepared.light_volume;
            self.cpu_scene = Some(prepared.scene);
            self.shape_indices = prepared.shape_indices;
            self.cpu_terrain = prepared.terrain;
            self.meshes = Some(prepared.meshes);
            self.mirror_shapes = prepared.mirror_shapes;
            self.mirror_index.clear();
            self.materials = Some(prepared.materials);
            self.palette = Some(prepared.palette);
            self.gpu_palette = None;
            let old = self.building.replace(prepared.building);
            self.motion.install(prepared.mirror);
            let building = self.building.as_mut().unwrap();
            building.set_tool_catalog(self.item_ui.catalog())?;
            if let Some(old) = &old {
                building.carry_over(old);
            }
            self.gpu_scene = None;
            self.gpu_terrain.clear();
            // A map change replaced what first entry set the HUD up from.
            if a.entered
                && let (Some(scene), Some(view)) = (&self.cpu_scene, &a.view)
            {
                for update in self.map_setup_updates(scene, &view.world.palette)? {
                    self.ui.apply_session(a.id, update);
                }
            }
        }
        if a.worker.view.has_changed().unwrap_or(false) {
            a.view = a.worker.view.borrow_and_update().clone();
        }
        if let (Some(building), Some(view)) = (&mut self.building, &a.view) {
            building.set_held_brick(view.weapons.images.get(&view.owner).is_some_and(|images| {
                images.iter().any(|image| {
                    image.hand == 0
                        && bri_sim::session::BRICK_HAND_IMAGES.contains(&image.image.as_str())
                })
            }));
            building.set_held_image(view.weapons.images.get(&view.owner).is_some_and(|images| {
                images.iter().any(|image| {
                    image.hand == 0
                        && !bri_sim::session::BRICK_HAND_IMAGES.contains(&image.image.as_str())
                })
            }));
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
            // Others see the ghost too (v20 ghosted `tempBrick`). Moves are
            // sent at most ten times a second; putting it away goes at once.
            let ghost = building.ghost().and_then(|ghost| {
                let id = |r: &bri_world::ContentRef| match r {
                    bri_world::ContentRef::Resolved(id) => Some(id.clone()),
                    _ => None,
                };
                Some(bri_sim::session::GhostBrick {
                    definition: id(&ghost.definition)?,
                    position: ghost.position,
                    quarter_turns: ghost.quarter_turns,
                    color: ghost.color,
                    print: ghost.print.as_ref().and_then(id),
                })
            });
            let due = self.ghost_report.as_ref().is_none_or(|(sent, at)| {
                *sent != ghost && (ghost.is_none() || at.elapsed() >= GHOST_REPORT_INTERVAL)
            });
            if due
                && a.worker
                    .request(REPORT_REQUEST, Command::GhostBrick(ghost.clone()))
                    .is_ok()
            {
                self.ghost_report = Some((ghost, std::time::Instant::now()));
            }
        }
        if let (Some(building), Some(view)) = (&mut self.building, &a.view) {
            building.set_broken_shapes(&view.broken_shapes)?;
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
                let colors = self.colorset(&view.world.palette);
                self.ui.apply_session(a.id, UiUpdate::Colorset(colors));
            }
            self.query_source = Some(view.world.clone());
            self.query_log = Some((view.world_log.clone(), view.world_revision));
            self.ghost_uploaded = u64::MAX;
            self.brick_debris.sync_world(&view.world);
            self.hidden_uploaded = None;
        }
        if let Some(view) = &a.view {
            self.mirror_index.follow(
                &view.world,
                &view.world_log,
                view.world_revision,
                &self.mirror_shapes,
            );
            // One set of openings: the windows show where bodies go.
            if let Some(collision) = self.motion.collision() {
                self.mirror_index.link(collision.links(), &self.mirror_shapes);
            }
        }
        if let Some(job) = &mut self.world_job
            && let Ok((source, revision, log, result)) = job.receiver.try_recv()
        {
            let left_out = std::mem::take(&mut job.left_out);
            self.world_job = None;
            match result {
                // Always applied: chunk state is consistent with `source`, and
                // a newer replica is reached by the next incremental update.
                Ok((chunked, changes)) => {
                    self.chunked = chunked;
                    for (key, built) in changes {
                        if let Some(built) = built {
                            self.cpu_chunks.insert(key, built.scene);
                            self.cpu_chunk_bricks.insert(key, Arc::new(built.bricks));
                            self.chunk_uploads.insert(key);
                        } else {
                            self.cpu_chunks.remove(&key);
                            self.cpu_chunk_bricks.remove(&key);
                            self.gpu_chunks.remove(&key);
                            self.gpu_chunk_bricks.remove(&key);
                            self.chunk_uploads.remove(&key);
                        }
                    }
                    self.world_source = Some(source);
                    self.world_revision = revision;
                    self.world_log = Some(log);
                    self.brick_fades.chunks_applied(&left_out);
                    self.chunks_left_out = left_out;
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
            && (self
                .world_source
                .as_ref()
                .is_none_or(|previous| !Arc::ptr_eq(previous, &view.world))
                || self.brick_fades.needs_rebuild(&self.chunks_left_out))
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
            // v20 eases repainted bricks to their new colour (`brick_fade`).
            match (&self.world_source, &known) {
                (Some(drawn), Some(known)) if !known.palette => {
                    self.brick_fades
                        .observe(drawn, &world, known.bricks.iter().copied());
                    // A knocked-out brick does not fade out in place: its
                    // debris replaces it at once. Easing it would draw it
                    // twice and cost a model per brick plus a second
                    // chunk rebuild once the fades settle.
                    let killing = self.pending_kills();
                    for id in &known.bricks {
                        if self.brick_debris.is_dead(*id) || killing.contains(id) {
                            self.brick_fades.settle(*id);
                        }
                    }
                }
                _ => self.brick_fades.settle_all(),
            }
            let left_out = self.brick_fades.left_out();
            let job_left_out = left_out.clone();
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
                        .update_leaving_out(
                            world,
                            known.as_ref(),
                            &left_out,
                            &meshes,
                            &palette,
                            Some(&materials),
                            WORLD_TRIANGLE_BUDGET,
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
                left_out: job_left_out,
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
            let palette = &a
                .view
                .as_ref()
                .context("Ready connection has no world")?
                .world
                .palette;
            for update in self.map_setup_updates(scene, palette)? {
                self.ui.apply_session(a.id, update);
            }
            a.entered = true;
            self.reconnects = 0;
            for text in std::mem::take(&mut self.join_notices) {
                self.ui.apply_session(a.id, UiUpdate::Chat { text });
            }
            // A game this player hosts runs their own Add-Ons' code; someone
            // else's server runs only code the player trusted for its host
            // key (never its address, which another host can take over).
            let server = a
                .view
                .as_ref()
                .map_or_else(String::new, |v| v.host_key.clone());
            // The code of the Add-Ons this game runs: the server's list when
            // joining changed it, else this client's own.
            let set = (!a.local)
                .then(|| a.joined.lock().ok().and_then(|mut slot| slot.take()))
                .flatten()
                .unwrap_or_else(|| self.content.paths.packages.clone());
            if self.client_code.loaded_from() != Some(&set) {
                self.client_code =
                    crate::client_code::ClientCode::load(&self.content.paths.root, &set);
            }
            self.client_code.start(
                if a.local {
                    crate::client_code::Host::Local
                } else {
                    crate::client_code::Host::Remote(&server)
                },
                &self.state_dir,
            );
            // Code the player has not trusted on this server yet: ask before
            // any of it runs. Leave ends the game.
            if !a.local
                && let Some(prompt) =
                    self.client_code
                        .trust_prompt(&server, &plain_chat(&a.name), &self.state_dir)
            {
                self.ui
                    .apply_session(a.id, UiUpdate::Question(trust_question(&prompt)));
            }
            if let Some(view) = &a.view {
                self.reset_weapon_effect_session(a.id, view.checkpoint_cue_cursor);
            }
            // `handleYourSpawn`: no favorites auto-buy in a local Tutorial,
            // whose lessons hand out the bricks.
            let tutorial = a.local
                && a.view.as_ref().is_some_and(|v| {
                    v.world
                        .map_id
                        .eq_ignore_ascii_case(bri_sim::tutorial::MAP_ID)
                });
            if !tutorial {
                self.ui.apply_session(a.id, UiUpdate::FirstSpawn);
            }
            self.ui
                .core
                .request(UiAction::SetAvatar(self.ui.settings().avatar));
            // `clientCmdTrustListUpload_Start`; the reply needs no handling.
            let list = crate::trust_list::TrustList::load(&self.state_dir.join("trust-list.json"));
            a.worker
                .request(REPORT_REQUEST, Command::TrustList(list.entries()))?;
        }
        if let Some(view) = &a.view {
            // The Environment window's view: on every change, and each
            // second while a day/night cycle turns.
            if let Some(scene) = &self.cpu_scene {
                let next = bri_ui::models::environment::EnvironmentView {
                    authored: authored_environment(scene),
                    settings: view.environment.clone(),
                    tick: view.tick,
                };
                let due = self.environment_sent.as_ref().is_none_or(|(session, sent)| {
                    *session != a.id
                        || sent.authored != next.authored
                        || sent.settings != next.settings
                        || next.settings.day_cycle.is_some()
                            && next.tick.abs_diff(sent.tick) >= bri_content::atmosphere::TICKS_PER_SECOND
                });
                if due {
                    self.environment_sent = Some((a.id, next.clone()));
                    self.ui.apply_session(a.id, UiUpdate::Environment(next));
                }
            }
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
                    // may carry ML markup, colour codes and death icons.
                    // Player lines use v20's chat format
                    // `\c7<clan prefix>\c3<name>\c7<clan suffix>\c6: <text>`.
                    let text = if line.owner == 0 {
                        bri_ui::ml::sanitize(&line.text)
                    } else {
                        player_chat(&line.clan, &line.name, &line.text)
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
                while let Ok(notice) = router.try_recv() {
                    for (text, confirm) in self.host_notice(notice) {
                        self.ui.apply_session(
                            a.id,
                            UiUpdate::Chat {
                                text: format!("\u{E006}{}", plain_chat(&text)),
                            },
                        );
                        if let Some(update) = confirm {
                            self.ui.apply_session(a.id, update);
                        }
                    }
                }
            }
            // Everyone's rank, from the host's administration list. Names
            // are unique on a server, so they pair the two lists.
            let rank = |name: &str| {
                view.admin_snapshot
                    .as_ref()
                    .and_then(|s| s.players.iter().find(|p| p.name == name))
                    .map(|p| p.role)
            };
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
                            admin: rank(name).map_or(
                                owner == view.owner && view.administrator,
                                bri_admin::Role::is_admin,
                            ),
                            super_admin: rank(name) == Some(bri_admin::Role::SuperAdmin),
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
        self.track_unsaved(&mut a);
        self.attempt = Some(a);
        Ok(())
    }
    /// Tell the menus whether leaving would drop changes the host has not
    /// saved under a name.
    fn track_unsaved(&mut self, a: &mut Attempt) {
        let now = std::time::Instant::now();
        let unsaved = match a.view.as_ref().map(|v| v.world_revision) {
            Some(revision) if a.local && a.entered => {
                match a.settling {
                    Some(until) if now < until => {
                        // Still rebuilding: follow it, and wait for it to go quiet.
                        if a.saved_revision != Some(revision) {
                            a.settling = Some(now + SETTLE);
                        }
                        a.saved_revision = Some(revision);
                    }
                    Some(_) => a.settling = None,
                    None => {}
                }
                *a.saved_revision.get_or_insert(revision) != revision
            }
            _ => false,
        };
        if unsaved != self.ui.core.unsaved_changes {
            self.ui
                .apply_session(a.id, UiUpdate::UnsavedChanges(unsaved));
        }
    }
}
/// How long a load or map change must stop changing the world before later
/// changes count as unsaved.
const SETTLE: Duration = Duration::from_secs(3);
/// Chat strings are plain user content, never UI markup/color instructions.
impl Drop for App {
    fn drop(&mut self) {
        self.disconnect();
        // Let a hosted game stop and keep its world before the runtime (and
        // every task on it) goes away.
        let closing = std::mem::take(&mut self.closing);
        if !closing.is_empty() && tokio::runtime::Handle::try_current().is_err() {
            self.runtime.block_on(async {
                let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
                for task in closing {
                    let _ = tokio::time::timeout_at(deadline, task).await;
                }
            });
        }
    }
}
/// Whether the view draws as third person: the own body, its third-person
/// images, jets and shadow show and the crosshair hides unless the camera is
/// in the eye (`isFirstPerson`), so sliding in keeps the body until the
/// camera arrives and sliding out shows it at once. Observers and the dead
/// always see their body. The vehicle chase camera slides by the same
/// position, so riders switch at the same point.
fn draws_third_person(controls: &Controls, alive: bool) -> bool {
    !controls.at_eye() || controls.observer().is_some() || !alive
}
/// `Player::getCameraTransform` (blocklandv20.exe 0x5ab7d0) for a body
/// `stand_height` tall standing at `feet`: distance, pivot and downward tilt.
/// The pivot is the middle of the box plus `cameraVerticalOffset` (0.75
/// while sliding in); offset and distance scale with the body.
pub(crate) fn pivot_camera(
    stand_height: f32,
    scale: f32,
    (max_dist, offset, tilt): (f32, f32, f32),
    feet: Vec3,
    pos: f32,
) -> (f32, Vec3, f32) {
    let lift = stand_height * 0.5 + (offset * pos + 0.75 * (1.0 - pos)) * scale;
    (
        (max_dist * scale * pos).clamp(0.0, 40.0),
        feet + Vec3::Y * lift,
        tilt,
    )
}
/// The player camera of a player-type mount standing at `feet`: its
/// `PlayerData` box is the collision hull (the horse's is 2.4 tall, so the
/// pivot sits 1.2 + 2.3 over its feet). The client does not know the
/// mount's scale, so it is drawn at 1.
fn mount_camera(d: &bri_vehicles::schema::Definition, feet: Vec3, pos: f32) -> (f32, Vec3, f32) {
    let (low, high) = d
        .collision_hulls
        .iter()
        .flatten()
        .fold((f32::MAX, f32::MIN), |(low, high), p| {
            (low.min(p[1]), high.max(p[1]))
        });
    let stand_height = if high > low { high - low } else { 0.0 };
    pivot_camera(
        stand_height,
        1.0,
        (d.camera.max_dist, d.camera.offset, d.camera.tilt),
        feet,
        pos,
    )
}
/// Eye of the camera in control: the free camera itself, an orbit around the
/// spied player, the chase camera, or the player's own eye.
/// A name's distance fade in `GuiShapeNameHud::onRender` (blocklandv20.exe
/// 0x5278f0). Blockland replaces the control's `distanceFade` with the
/// shape's name distance (8192 unless `setShapeNameDistance`): names show
/// out to `min(nameDistance, visibleDistance)` and fade from
/// `min(fogDistance, max(0.8 × nameDistance, nameDistance - 5))`. None past
/// the far end.
pub fn name_opacity(distance: f32, fog_distance: f32, visible_distance: f32) -> Option<f32> {
    const NAME_DISTANCE: f32 = 8192.0;
    let far = NAME_DISTANCE.min(visible_distance);
    let fade = fog_distance.min((NAME_DISTANCE * 0.8).max(NAME_DISTANCE - 5.0));
    if distance <= 0.0 || distance > far {
        return None;
    }
    Some(if distance < fade {
        1.0
    } else {
        1.0 - (distance - fade) / (far - fade)
    })
}
/// `GuiShapeNameHud::onRender`: every other living player's name above their
/// eye point (`verticalOffset` 0.85), hidden behind the map and raycasting
/// bricks ([`crate::building::Building::name_visible`]), faded by
/// [`name_opacity`] and drawn in the mini-game colour a member's player is
/// given at spawn (`GameConnection::createPlayer`), white otherwise.
#[allow(clippy::too_many_arguments)]
fn name_tags(
    view: &network::View,
    presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
    building: Option<&crate::building::Building>,
    view_projection: glam::Mat4,
    camera: Vec3,
    (fog_distance, visible_distance): (f32, f32),
    size: (f32, f32),
    scale: f32,
    controlling_body: bool,
) -> Vec<bri_ui::api::NameTag> {
    const VERTICAL_OFFSET: f32 = 0.85;
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
        let target = view.archetypes.eye(state);
        let Some(opacity) = name_opacity(target.distance(camera), fog_distance, visible_distance)
        else {
            continue;
        };
        if building.is_some_and(|b| !b.name_visible(camera, target).unwrap_or(true)) {
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
        let color = view
            .minigames
            .iter()
            .find(|m| m.members.contains(owner))
            .and_then(|m| crate::minigame_ui::color_rgb(m.color))
            .unwrap_or([255; 3]);
        tags.push(bri_ui::api::NameTag {
            x: (ndc.x + 1.0) * 0.5 * size.0 / scale,
            y: (1.0 - ndc.y) * 0.5 * size.1 / scale,
            text: plain_chat(name),
            opacity,
            color,
        });
    }
    tags
}
/// How far Add-On code moved the orbit camera's target from where the game
/// has it ([`crate::avatar::AvatarMesh::drawn_offset`]): the dead watch
/// their ragdoll wherever it slid, not the spot where they died.
fn orbit_drawn_offset(
    controls: &Controls,
    avatars: &BTreeMap<bri_world::OwnerId, crate::avatar::AvatarMesh>,
) -> Option<Vec3> {
    match controls.observer()?.mode {
        crate::controls::ObserverMode::Orbit(target) => avatars.get(&target)?.drawn_offset(),
        _ => None,
    }
}
#[allow(clippy::too_many_arguments)]
fn camera_eye(
    controls: &Controls,
    presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
    entities: &BTreeMap<u64, bri_sim::session::EntityInfo>,
    drawn_offset: Option<Vec3>,
    building: &crate::building::Building,
    own_eye: Vec3,
    forward: Vec3,
    chase: Option<(Vec3, f32)>,
    passages: &bri_content::passage::Passages,
) -> Result<(Vec3, Option<glam::Affine3A>)> {
    use crate::controls::ObserverMode;
    match controls.observer().map(|o| o.mode) {
        Some(ObserverMode::Free(position)) => Ok((position, None)),
        // `setOrbitMode(target, ..., 0, 8, 8)` from `Observer::setMode("Corpse")`.
        // Its boom goes back through a portal behind the focus, as a chase
        // camera's does.
        Some(ObserverMode::Orbit(_) | ObserverMode::Drive(_)) => {
            let focus = controls
                .orbit_focus(presented, building.archetypes(), entities)
                .map(|focus| focus + drawn_offset.unwrap_or(Vec3::ZERO))
                .unwrap_or(own_eye);
            building.camera_boom(focus, focus, forward, 8.0, passages)
        }
        None => match chase {
            // A chase camera's boom from `own_eye`, its pivot, which rides
            // on the body at `from`.
            Some((from, distance)) => {
                building.camera_boom(from, own_eye, forward, distance, passages)
            }
            None => Ok((own_eye, None)),
        },
    }
}
/// The view's forward, right and up for a look turned by `yaw` then pitched
/// by `pitch`, as Torque builds the eye transform (`zmat(yaw) * xmat(pitch)`).
/// Right stays level, so looking straight up or down (v20's look limits are
/// exactly +-90 degrees) or past it (the chase camera adds `cameraTilt`)
/// keeps turning with the yaw instead of snapping to a fixed roll.
fn view_basis(yaw: f32, pitch: f32) -> (Vec3, Vec3, Vec3) {
    let forward = Vec3::new(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    );
    let right = Vec3::new(yaw.cos(), 0.0, yaw.sin());
    (forward, right, right.cross(forward))
}
/// [`view_basis`] turned about the forward axis by `roll`, positive tipping
/// the top of the view left (see [`crate::controls::roll`]).
fn rolled_view_basis(yaw: f32, pitch: f32, roll: f32) -> (Vec3, Vec3, Vec3) {
    let (forward, right, up) = view_basis(yaw, pitch);
    if roll == 0.0 || !roll.is_finite() {
        return (forward, right, up);
    }
    let (sin, cos) = roll.sin_cos();
    (forward, right * cos + up * sin, up * cos - right * sin)
}
/// Torque's FOV is horizontal (`GuiTSCtrl::processCameraQuery` takes the
/// frustum width from it and the height from the aspect ratio).
fn vertical_fov(horizontal: f32, aspect: f32) -> f32 {
    if !(aspect.is_finite() && aspect > 0.0) {
        return horizontal;
    }
    2.0 * ((horizontal * 0.5).tan() / aspect).atan()
}
/// The join's trust question for a server's sandboxed Add-On code, as
/// `bri_client_sandbox::trust` words it.
fn trust_question(prompt: &bri_client_sandbox::TrustPrompt) -> bri_ui::api::Question {
    let rows: Vec<String> = prompt
        .rows
        .iter()
        .map(|row| {
            let changed = if row.changed { " (changed)" } else { "" };
            format!("{}{changed}: {}", plain_chat(&row.name), row.can.join(", "))
        })
        .collect();
    bri_ui::api::Question {
        title: plain_chat(&prompt.title),
        text: format!(
            "{}\n\n{}\n\n{}",
            prompt.body,
            rows.join("\n"),
            prompt.footer
        ),
        yes: prompt.accept.into(),
        no: prompt.decline.into(),
        on_yes: Box::new(UiAction::TrustAddOnCode),
        on_no: Some(Box::new(UiAction::Disconnect)),
    }
}
/// `serverCmdMessageSent`: `'\c7%1\c3%2\c7%3\c6: %4'` with the clan
/// prefix, name and clan suffix, so the tags are grey, the name yellow and
/// the message white.
fn player_chat(clan: &bri_sim::session::Clan, name: &str, text: &str) -> String {
    format!(
        "\u{E007}{}\u{E003}{}\u{E007}{}\u{E006}: {}",
        plain_chat(&clan.prefix),
        plain_chat(name),
        plain_chat(&clan.suffix),
        linked_chat(text, '\u{E006}')
    )
}
/// The name and size a joined server goes by: its listing's, or what the
/// join had (the typed address) when the listing leaves them out.
fn joined_server(
    listing: &bri_net::protocol::Listing,
    name: &str,
    max_players: u32,
) -> (String, u32) {
    let listed = plain_chat(&listing.name);
    (
        if listed.trim().is_empty() {
            name.to_string()
        } else {
            listed
        },
        if (1..=64).contains(&listing.max_players) {
            listing.max_players
        } else {
            max_players
        },
    )
}

/// `serverCmdMessageSent` (mainServer.cs:1136-1166): the first `http://` or
/// `https://` address in a message becomes `<a:url>url</a>` (without the
/// scheme, `<` and `>` removed), then the chat colour resumes. The rest of
/// the text stays literal.
fn linked_chat(text: &str, resume: char) -> String {
    let start = ["http://", "https://"]
        .iter()
        .filter_map(|p| text.find(p).map(|i| (i, p.len())))
        .min();
    let Some((start, scheme)) = start else {
        return plain_chat(text);
    };
    let end = text[start..].find(' ').map_or(text.len(), |e| start + e);
    let url: String = text[start + scheme..end]
        .chars()
        .filter(|c| c.is_ascii_graphic() && !matches!(c, '<' | '>'))
        .take(256)
        .collect();
    if url.is_empty() {
        return plain_chat(text);
    }
    format!(
        "{}<a:{url}>{url}</a>{resume}{}",
        plain_chat(&text[..start]),
        plain_chat(&text[end..])
    )
}
/// Player-typed text is shown literally: no ML tags, colour codes or control
/// characters (v20's server strips ML control characters from chat).
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
/// Center and bottom prints are server ML markup (parsed and bounded by
/// `bri_ui::ml`) on several lines. `<key:cmd>` names the player's own binding
/// for a command, as the Tutorial's `bindNameFix` does.
fn print_markup(binds: &bri_ui::binds::BindMap, text: &str) -> String {
    let text = bri_ui::ml::sanitize(text);
    let mut resolved = String::new();
    let mut rest = text.as_str();
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
/// `$pref::Input::UseStrafeSteering` and `$pref::Input::UseAutoReturnSteering`
/// (both on in stock v20's defaults.cs; off as shipped, the reference
/// install's, which the host assumes too).
fn steering_prefs(prefs: &bri_ui::prefs::Prefs) -> (bool, bool) {
    let (strafe, auto_return) = bri_sim::session::DEFAULT_STEERING;
    (
        prefs.bool_or("$pref::Input::UseStrafeSteering", strafe),
        prefs.bool_or("$pref::Input::UseAutoReturnSteering", auto_return),
    )
}

/// The steering prefs a driver's moves are steered by: the host's copy,
/// echoed in their vehicle's pose, else their own. Predicting with the
/// host's keeps prediction from ever fighting it.
fn steering_in_use(
    pose: Option<&bri_sim::session::VehiclePose>,
    prefs: &bri_ui::prefs::Prefs,
) -> (bool, bool) {
    pose.map_or_else(|| steering_prefs(prefs), |pose| pose.driver_steering)
}

/// `handleYourSpawn`'s `$pref::Input::AutoLight` test: every spawn under a
/// sun whose red, green and blue are all below 0.4 turns the light on.
pub fn dark_sun(color: [f32; 3]) -> bool {
    color.iter().all(|c| *c < 0.4)
}

/// Client-side death, respawn and status presentation derived from vitals.
#[derive(Default)]
struct CombatPresentation {
    alive: Option<bool>,
    health: f32,
    countdown: Option<u64>,
    died_at: std::collections::BTreeMap<bri_world::OwnerId, std::time::Instant>,
    lights: std::collections::BTreeMap<bri_world::OwnerId, bool>,
    /// `/hug` and `/zombie`: `playThread(1, armReadyBoth)` holds until the
    /// arms change again (`Player::updateArm`, `fixArms`, unequip), kept
    /// with the held pose it replaced (`None` until the next frame sees it).
    hugging: std::collections::BTreeMap<bri_world::OwnerId, Option<crate::avatar::HeldToolPose>>,
    minigame_revision: u64,
    minigame_state: Option<MiniGameUiState>,
    /// Energy bar fraction last shown, in hundredths.
    energy: Option<u8>,
}
impl CombatPresentation {
    fn hug_pose(
        &mut self,
        owner: bri_world::OwnerId,
        held: crate::avatar::HeldToolPose,
    ) -> crate::avatar::HeldToolPose {
        match self.hugging.get_mut(&owner) {
            Some(replaced @ None) => *replaced = Some(held),
            Some(Some(replaced)) if *replaced == held => {}
            Some(Some(_)) => {
                self.hugging.remove(&owner);
                return held;
            }
            None => return held,
        }
        crate::avatar::HeldToolPose::Both
    }
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

/// The vehicle this client drives, drawn ahead on its own moves: the one
/// whose steering seat it sits in. A tumble's seat steers nothing, so a
/// tumbling (or Gravity Gun held) player sees their body where everyone
/// else does, smoothly between the host's poses, instead of guessed ahead
/// and pulled back each pose (Max, v0.1.9: dragged about, "on their screen
/// it seems a bit stuttering like teleporting").
fn driven_vehicle(
    mounted: Option<(u64, u8)>,
    steers: impl FnOnce(u64, usize) -> bool,
) -> Option<u64> {
    let (vehicle, seat) = mounted.filter(|(_, seat)| *seat == 0)?;
    steers(vehicle, usize::from(seat)).then_some(vehicle)
}

/// Tell the UI whether the held tool can take the wheel (its image's
/// `wheel` command, "package:command"), once each time that changes.
fn claim_wheel(ui: &mut Ui, current: &mut Option<String>, wheel: Option<String>) {
    if wheel.is_some() != current.is_some() {
        ui.apply(UiUpdate::ToolWheel(wheel.is_some()));
    }
    *current = wheel;
}

/// Whether the trigger is down is the player's, whichever path then takes
/// the click (building, a gunner's seat, the spy camera): a tool that takes
/// the wheel while the trigger is held (the Gravity Gun's reel) reads it
/// from `controls`.
fn note_trigger(controls: &mut Controls, action: &UiAction) {
    if let UiAction::Game(
        held @ GameAction::Held {
            control: HeldControl::Fire,
            ..
        },
    ) = action
    {
        controls.action(held);
    }
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

/// The caption for a sound a player would hear from `listener`, if it is
/// one worth reading: blasts, gunfire, cries, splashes and breaking bricks
/// within earshot. Footsteps, plants and menu sounds get none.
fn caption(cue: &bri_sim::presentation::Cue, listener: Option<Vec3>) -> Option<&'static str> {
    use bri_sim::presentation::CueKind as K;
    const EARSHOT: f32 = 80.0;
    if listener.is_some_and(|l| l.distance(Vec3::from(cue.position)) > EARSHOT) {
        return None;
    }
    Some(match &cue.kind {
        K::Explosion { .. } => "[Explosion]",
        K::WeaponSound { .. } => "[Weapon fire]",
        K::Death { .. } => "[Death cry]",
        K::Pain { cry: true, .. } => "[Cry of pain]",
        K::Water {
            entered: true,
            speed,
            ..
        } if *speed > 4.0 => "[Splash]",
        K::BrickKill { .. } => "[Bricks breaking]",
        K::Teleport { .. } => "[Teleport]",
        K::Emote { name, .. } if name == "alarm" => "[Alarm]",
        _ => return None,
    })
}

/// What the player's controls send. The Tutorial's walking limits (no jet,
/// no jump) belong to the player's body, as v20's `PlayerNoJet` datablock
/// did; a rider's jet still reaches the mount, where it dismounts
/// (`doDismount`), and its jump still jumps the horse.
fn rider_input(
    abilities: bri_sim::session::Abilities,
    input: bri_sim::player::MoveInput,
    mounted: bool,
) -> bri_sim::player::MoveInput {
    if mounted {
        input
    } else {
        abilities.apply(input)
    }
}
/// The vehicle a client predicts: which one, from which definition, at
/// which scale. Any change starts its prediction again.
#[derive(Clone, Debug, PartialEq)]
struct DriveTarget {
    id: u64,
    definition: String,
    scale_bits: u32,
}
#[derive(Default)]
struct DriveState {
    target: Option<DriveTarget>,
    /// A target whose prediction failed: the host's poses are shown until
    /// the player leaves it.
    refused: Option<DriveTarget>,
}
/// What the local player, in `info`'s first seat, predicts: a live vehicle
/// they steer or a player-type mount they control (horse, rowboat, cannon,
/// turret), as v20 predicts the object a client controls. Destroyed
/// vehicles and passengers show the host's poses.
fn drive_target(
    info: &bri_sim::session::VehicleInfo,
    d: &bri_vehicles::Definition,
    strafe_steering: bool,
) -> Option<DriveTarget> {
    let drives = matches!(
        d.seat_role_for(0, strafe_steering),
        SeatRole::StrafeDriver | SeatRole::MouseDriver | SeatRole::Actor
    );
    (drives && !info.destroyed).then(|| DriveTarget {
        id: info.id,
        definition: info.definition.clone(),
        scale_bits: info.scale.to_bits(),
    })
}
/// A ghost the server would refuse, before `v20_temp_brick` brightens it.
const BLOCKED_GHOST: [f32; 4] = [0.6, 0.05, 0.05, 1.0];
/// The ghost is redrawn when it moves or the bricks around it change.
fn ghost_key(building: &crate::building::Building) -> u64 {
    building
        .ghost_generation()
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ building.query_generation()
        // Taking the copy's tool in hand or putting it away.
        ^ u64::from(building.copy_ghost().is_some()) << 63
}

/// The world box v20 outlines around a non-rendering brick: its footprint
/// and height, turned with it, about its position.
fn hidden_brick_box(
    brick: &bri_world::Brick,
    mesh: &bri_content::brick::Brick,
) -> (glam::Vec3, glam::Vec3) {
    let [w, d] = mesh.footprint_studs.map(|v| v as f32 * 0.5);
    let h = mesh.height_plates as f32 * 0.2;
    let size = if brick.quarter_turns.is_multiple_of(2) {
        glam::Vec3::new(w, h, d)
    } else {
        glam::Vec3::new(d, h, w)
    };
    let centre = glam::Vec3::from(brick.position);
    (centre - size * 0.5, centre + size * 0.5)
}

fn translucent_ghost(scene: &mut SceneData, look: &crate::world_scene::TempBrickLook) {
    crate::world_scene::v20_temp_brick(scene, look);
}

/// The map's baked lighting, started on its own thread as soon as the map's
/// scene is read, so it bakes while the rest of the map loads, and uploaded
/// once per renderer. Two bakes: the classic light volume
/// (`bri_render::light_volume`, for the Classic lighting mode) and the map's
/// recovered lights with their visibility and residual volumes
/// (`bri_render::map_lighting`, for the Unified modes). Each is stored under
/// the client state directory by its content key, so each map bakes once.
/// Until a bake arrives, the modes that need it draw as Classic.
enum Baked {
    Volume(bri_render::light_volume::LightVolume),
    /// The map bake, and whether its Dynamic-mode residual volume is in it
    /// (else `ResidualAll` follows).
    Map(Box<bri_render::map_lighting::MapLighting>, bool),
    /// The Dynamic mode's residual volume, when it bakes after the rest.
    ResidualAll(bri_render::light_volume::LightVolume),
}
type LightVolumeReceiver = std::sync::mpsc::Receiver<Baked>;
#[derive(Default)]
struct LightVolumeState {
    /// Behind a mutex so a prepared map (which carries it) stays `Sync`.
    baking: Option<std::sync::Mutex<LightVolumeReceiver>>,
    volume: Option<bri_render::light_volume::LightVolume>,
    map: Option<bri_render::map_lighting::MapLighting>,
    /// The bake's lightmap leak cleanup, until the map's lightmaps take it.
    leaks: Vec<bri_render::map_lighting::TexelFix>,
    /// The bake's Dynamic-mode lightmaps and per-texel light visibility,
    /// for the map's images once Dynamic is chosen.
    dynamic: Vec<bri_render::map_lighting::DynamicSheet>,
    /// The Dynamic mode's residual volume is baked (it can follow the rest
    /// of the map bake).
    dynamic_ready: bool,
    /// The map's images hold the Dynamic lightmaps (the scene uploaded
    /// again with them).
    dynamic_equipped: bool,
    uploaded: bool,
    /// The lighting mode the bound volumes serve.
    bound_mode: u8,
    /// The map's breakable light shapes (scene node, centre): a broken bulb
    /// switches its lights off.
    light_shapes: Vec<(u32, Vec3)>,
}
/// Breakable map shapes that are lights (v20 `Glass` datablocks): the
/// Bedroom lamp's bulb and the Kitchen's fluorescent tubes.
const LIGHT_SHAPES: &[&str] = &["lightBulbA", "fluorescentLight"];
/// A recovered light belongs to the light shapes nearest it, up to this far
/// from their centres. The fit places a fixture's lights where their falloff
/// fits the lightmaps best, not on the bulb: measured on v20's maps, the
/// Bedroom bulb's main light sits 19.9 units from it and the Kitchen tubes'
/// lights 8.8 to 15.9. Window and sun light, fitted farther from any
/// fixture, stays unowned.
const LIGHT_SHAPE_REACH: f32 = 24.0;
/// Shapes up to this many times the nearest one's distance share a light:
/// the Kitchen's paired tubes fit as one light between them.
const LIGHT_SHAPE_SHARE: f32 = 1.5;
/// Each recovered light's run-time tint: what the Add-On rules give it (1
/// as the map was lit), scaled by the share of its owning light shapes still
/// whole, so it goes dark when all of them break and half when one of two
/// does. Rules cannot light a broken shape again.
fn map_light_tints(
    lights: &[bri_render::map_lighting::MapLight],
    light_shapes: &[(u32, Vec3)],
    broken: &BTreeSet<u32>,
    rules: &[bri_sim::session::MapLightRule],
) -> Vec<Vec3> {
    lights
        .iter()
        .map(|light| {
            let at = Vec3::from(light.position);
            let tint = bri_sim::session::MapLightRule::tint_at(rules, at);
            let nearest = light_shapes
                .iter()
                .map(|(_, centre)| centre.distance(at))
                .fold(f32::INFINITY, f32::min);
            let limit = LIGHT_SHAPE_REACH.min(nearest * LIGHT_SHAPE_SHARE);
            let (owners, whole) = light_shapes
                .iter()
                .filter(|(_, centre)| centre.distance(at) <= limit)
                .fold((0u32, 0u32), |(owners, whole), (node, _)| {
                    (owners + 1, whole + u32::from(!broken.contains(node)))
                });
            if owners == 0 { tint } else { tint * (whole as f32 / owners as f32) }
        })
        .collect()
}
/// Stores `bytes` as `file`, through a partial file. A lost write only
/// means baking again next time.
fn store_bake(cache: &std::path::Path, file: &std::path::Path, bytes: Vec<u8>) {
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
    fn start(scene: &SceneData, cache: &std::path::Path) -> Self {
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
    fn set_light_shapes(&mut self, breakables: &[bri_sim::map::Breakable]) {
        self.light_shapes = breakables
            .iter()
            .filter(|b| LIGHT_SHAPES.iter().any(|name| b.datablock.eq_ignore_ascii_case(name)))
            .map(|b| (b.node, b.center))
            .collect();
    }
    /// Broken bulbs and Add-On rules onto the bound map lights; uploads
    /// only when a tint changed.
    fn tint(
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
    fn mode(&self, requested: u8) -> u8 {
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
    fn upload(
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

/// One frame of sprites from the three effect worlds, farthest first. Each
/// world's snapshot is already sorted from `eyes[0]`, so they merge in one
/// pass; equally distant sprites keep world order, as a stable sort of the
/// three lists end to end would. The lights every view shares are the ones
/// nearest any of `eyes` ([`crate::views::eyes`]), so a mirror or portal
/// keeps the lights beside what it shows.
pub(crate) fn combine_effect_frames(
    mut world: bri_fx_runtime::FrameEffects,
    others: [bri_fx_runtime::FrameEffects; 2],
    eyes: &[Vec3],
) -> (bri_fx_runtime::FrameEffects, usize) {
    let eye = eyes.first().copied().unwrap_or_default();
    let [weapon, actor] = others;
    let lists = [
        std::mem::take(&mut world.particles),
        weapon.particles,
        actor.particles,
    ];
    let total = lists.iter().map(Vec::len).sum();
    let mut heads = [0usize; 3];
    let mut merged = Vec::with_capacity(total);
    while merged.len() < total {
        let mut best: Option<(usize, f32)> = None;
        for (i, list) in lists.iter().enumerate() {
            if let Some(p) = list.get(heads[i]) {
                let d = eye.distance_squared(p.position);
                // Strictly farther wins; a tie keeps the earlier list.
                if best.is_none_or(|(_, b)| d.total_cmp(&b).is_gt()) {
                    best = Some((i, d));
                }
            }
        }
        let (i, _) = best.expect("a list with sprites left");
        merged.push(lists[i][heads[i]]);
        heads[i] += 1;
    }
    world.particles = merged;
    world.lights.extend(weapon.lights);
    world.lights.extend(actor.lights);
    let nearest = |at: Vec3| {
        eyes.iter()
            .map(|e| e.distance_squared(at))
            .fold(f32::INFINITY, f32::min)
    };
    world
        .lights
        .sort_by(|a, b| nearest(a.position).total_cmp(&nearest(b.position)));
    let deferred = world
        .lights
        .len()
        .saturating_sub(bri_render::scene::MAX_POINT_LIGHTS);
    world.lights.truncate(bri_render::scene::MAX_POINT_LIGHTS);
    (world, deferred)
}
/// Show a drop folder (saves, Add-Ons) in the file browser, making it first
/// so a player can always find where files go. A folder that cannot be made
/// is this request's failure, never the whole game's.
fn show_drop_folder(folder: &Path) -> Result<()> {
    std::fs::create_dir_all(folder)
        .with_context(|| format!("Could not create {}", folder.display()))?;
    if !bri_crash::open(&folder.to_string_lossy()) {
        bri_console::warn(format!("Could not open {}", folder.display()));
    }
    Ok(())
}

impl PlatformApp for App {
    fn ui(&self) -> &Ui {
        &self.ui
    }
    fn focus_changed(&mut self, focused: bool) {
        self.audio.set_focused(focused);
    }
    fn wants_frame_timing(&self) -> bool {
        self.ui.core.perf.visible()
    }
    fn frame_timed(&mut self, timing: crate::perf::FrameTiming) {
        self.ui.apply(UiUpdate::PerfFrame(timing.sample()));
    }
    fn ui_mut(&mut self) -> &mut Ui {
        &mut self.ui
    }
    fn tick(&mut self, elapsed: Duration) -> Result<()> {
        self.frame_stats.push(elapsed);
        if let Some(line) = self.frame_log.as_mut().and_then(|log| log.frame(elapsed)) {
            // Session log only: players send it, the console stays quiet.
            eprintln!("{line}");
        }
        // Tell the player about a newer release outside a game, not as a
        // dialog over play.
        if !self.ui.core.in_game()
            && let Some(check) = &self.update_check
        {
            match check.try_recv() {
                Ok(newer) => {
                    self.update_check = None;
                    self.ui.apply(UiUpdate::NewerVersion {
                        name: newer.name,
                        url: newer.url,
                    });
                }
                Err(mpsc::TryRecvError::Disconnected) => self.update_check = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let mut listener = bri_audio::Listener::default();
        // `setTimeScale` slows or speeds the whole game, not the interface:
        // every in-world visual advances by `game_elapsed`; only the UI,
        // camera easing and audio mixing use wall time.
        let scale = self
            .attempt
            .as_ref()
            .and_then(|a| a.view.as_ref())
            .map_or(1.0, |v| v.time_scale);
        let game_elapsed = elapsed.mul_f32(scale);
        self.animation_time += game_elapsed.as_secs_f64().min(0.25);
        if let Err(error) = self.poll_network() {
            // A fault while following the session (a map's weather, a tool
            // catalog, a closed connection) ends that session with its real
            // reason, never the whole game.
            bri_console::warn(format!("Session ended by a client error: {error:#}"));
            self.ui.apply(UiUpdate::Connection(ConnectionState::Failed {
                reason: format!("{error:#}"),
            }));
            self.disconnect();
        }
        self.poll_files();
        if let Some((map, name)) = self.save_previews.poll() {
            self.ui.apply(UiUpdate::SavePreview {
                map,
                name,
                preview: IconRef::None,
            });
        }
        self.poll_old_saves();
        self.update_package_hud();
        if let Some(a) = self.attempt.as_ref().filter(|a| a.entered) {
            let skins = self.item_skins.take_messages();
            for text in self.client_code.take_messages().into_iter().chain(skins) {
                self.ui.apply_session(a.id, UiUpdate::Chat { text });
            }
        }
        let alive = self.local_alive();
        self.follow_control();
        self.controls
            .fly(elapsed.as_secs_f32(), &self.motion.passages());
        let prefs = &self.ui.core.prefs;
        self.controls.set_fov_prefs(
            bri_ui::screens::options::default_fov(prefs),
            bri_ui::screens::options::zoom_fov(prefs),
        );
        self.controls.set_invert_prefs(
            prefs.bool_or("$pref::Input::MouseInvert", false),
            prefs.bool_or("$Pref::Input::VehicleMouseInvert", true),
        );
        let steering = steering_prefs(prefs);
        if let Some(a) = self.attempt.as_ref().filter(|a| a.entered)
            && self.steering_sent != Some((a.id, steering))
        {
            self.steering_sent = Some((a.id, steering));
            self.ui.core.request(UiAction::SteeringPrefs {
                strafe: steering.0,
                auto_return: steering.1,
            });
        }
        self.update_held_weapon();
        self.controls.advance_sway(elapsed.as_secs_f32());
        self.controls.advance_zoom(elapsed.as_secs_f32());
        self.controls.ease_roll(elapsed.as_secs_f32());
        self.controls.advance_view(elapsed.as_secs_f32());
        self.controls.advance_head(elapsed.as_secs_f32());
        if let Some(a) = self.attempt.as_ref().filter(|a| a.entered) {
            let mounted = a
                .view
                .as_ref()
                .and_then(|v| v.vitals.get(&v.owner))
                .is_some_and(|v| v.mounted.is_some() || v.ride.is_some());
            let input = if alive {
                rider_input(self.abilities, self.controls.movement(), mounted)
            } else {
                // Corpses ignore controls; keep aim so the server agrees.
                bri_sim::player::MoveInput {
                    yaw: self.controls.yaw,
                    pitch: self.controls.pitch,
                    ..Default::default()
                }
            };
            // A tool in hand with a jet command (v20's `onTrigger` slot 4)
            // takes right click: predict no jet, as the host runs none.
            let tool_jet = a.view.as_ref().is_some_and(|v| {
                v.weapons.images.get(&v.owner).is_some_and(|images| {
                    images.iter().any(|m| {
                        m.hand == 0
                            && self
                                .content
                                .weapons
                                .pack
                                .images
                                .get(&m.image)
                                .is_some_and(|i| i.commands.jet.is_some())
                    })
                })
            });
            self.motion.set_tool_jet(tool_jet);
            if let Some((newest, inputs)) = self.motion.advance(
                game_elapsed.as_secs_f32(),
                input,
                bri_net::protocol::MOVEMENT_REDUNDANCY,
            )? {
                a.worker.movement(newest, inputs, self.camera_view())?;
            }
            // Through an opening: the look turns as the body did.
            if let Some(carry) = self.motion.take_passed() {
                self.controls.carry_look(&carry);
            }
            if let Some((speed, archetype)) = self.motion.take_impact() {
                let min = bri_sim::player_types::PlayerType::from_archetype(archetype)
                    .unwrap_or_default()
                    .min_impact_speed();
                self.actor_effects
                    .ground_impact(speed, min, self.animation_time.to_bits());
            }
            if let Some(view) = &a.view {
                let vitals = view.vitals.get(&view.owner);
                let mounted = vitals.and_then(|v| v.mounted);
                // Driving a package entity parks the avatar like a seat does.
                let driving = vitals.is_some_and(|v| {
                    matches!(v.control, bri_sim::session::ControlObject::Entity(_))
                });
                // A player riding another player shows the host's pose too.
                let ride = vitals.and_then(|v| v.ride);
                self.motion
                    .set_mounted(mounted.is_some() || ride.is_some() || driving);
                self.controls.set_mounted(mounted.is_some() || ride.is_some());
                let head_yaw = self.controls.movement().head_yaw;
                self.motion
                    .present(view, self.controls.yaw, self.controls.body_pitch(), head_yaw);
                let driven = driven_vehicle(mounted, |vehicle, seat| {
                    view.vehicles
                        .get(&vehicle)
                        .and_then(|info| self.vehicle_assets.definition(&info.definition))
                        .and_then(|d| d.seats.get(seat))
                        .is_some_and(|s| s.controls)
                });
                Self::predict_driven(
                    &mut self.motion,
                    &mut self.vehicles,
                    &self.vehicle_assets,
                    &self.ui.core.prefs,
                    &mut self.cosmetic_faults,
                    &mut self.drive_state,
                    view,
                    driven,
                );
                self.vehicles.update(
                    &view.vehicles,
                    &view.vehicle_poses,
                    self.motion.server_tick(),
                    driven,
                    &self.motion.passages(),
                );
                self.tutorial_targets.update(
                    &view.targets,
                    self.motion.server_tick().unwrap_or(view.tick as f64),
                );
                if let Some((vehicle, seat)) = mounted
                    && let Some(info) = view.vehicles.get(&vehicle)
                    && let Some(d) = self.vehicle_assets.definition(&info.definition)
                    && d.seats.get(usize::from(seat)).is_some_and(|s| s.weapon)
                {
                    // A new gunner takes control of the turret looking where
                    // it points (the host keeps it there until they do).
                    if mounted != self.seated_on
                        && !d.is_actor()
                        && d.attachment_mount.is_some()
                        && let Some(pose) = view.vehicle_poses.get(&vehicle)
                    {
                        let (yaw, pitch) = crate::vehicles::turret_look(pose);
                        self.controls.yaw = yaw;
                        self.controls.pitch = pitch;
                        self.mount_heading = None;
                    }
                    self.vehicles
                        .aim_locally(vehicle, d, self.controls.yaw, self.controls.pitch);
                }
                // A new seat starts facing it (`Armor::onMount` resets the
                // transform), even from one passenger seat to another.
                if mounted != self.seated_on {
                    self.seated_on = mounted;
                    self.controls.set_ride(None);
                    // Tell the host the steering prefs again with every seat,
                    // should its copy have been lost (a reconnect).
                    self.steering_sent = None;
                }
                // The view rides along: it faces the seat, follows a
                // mouse-steered vehicle, turns with the hull for a gunner, and
                // stays put on a mount facing the look.
                let riding = mounted.and_then(|(vehicle, seat)| {
                    let info = view.vehicles.get(&vehicle)?;
                    let d = self.vehicle_assets.definition(&info.definition)?;
                    let frame = self.vehicles.frame(vehicle)?;
                    let seat_rotation = self
                        .vehicles
                        .seat_transform(&self.vehicle_assets, info, usize::from(seat))
                        .map(|(_, rotation)| rotation);
                    // skiVehicle::onWreck whites the screen out by the crash
                    // speed: clamp(1 + (speed - 10) / 50 * 7, 1, 7) / 7.
                    if d.family == bri_vehicles::Family::Tumble && self.tumble != Some(vehicle) {
                        self.tumble = Some(vehicle);
                        let seconds =
                            (1.0 + (frame.velocity.length() - 10.0) / 50.0 * 7.0).clamp(1.0, 7.0);
                        self.ui.apply(UiUpdate::Whiteout(seconds / 7.0));
                    }
                    let forward = frame.rotation * Vec3::NEG_Z;
                    // A driver steers as the host steers them (its copy of
                    // their prefs, in the pose), so view and prediction agree.
                    let pose = view.vehicle_poses.get(&vehicle).filter(|_| seat == 0);
                    let (strafe, _) = steering_in_use(pose, &self.ui.core.prefs);
                    let role = d.seat_role_for(usize::from(seat), strafe);
                    // The first-person view rides the seat on a vehicle and
                    // the hull under a gunner's turret; a player-type mount
                    // stays upright like any player.
                    self.controls.set_ride(match role {
                        // Any passenger, a rowboat's too: the mouse turns the
                        // body on the seat and pitches the head.
                        SeatRole::Passenger => seat_rotation.map(|r| {
                            crate::controls::Ride::Seat(r, crate::controls::SeatLook::Passenger)
                        }),
                        // A player-type mount's rider controls a Player:
                        // upright, and its head never springs back.
                        _ if d.is_actor() => None,
                        SeatRole::StrafeDriver => seat_rotation.map(|r| {
                            crate::controls::Ride::Seat(r, crate::controls::SeatLook::StrafeDriver)
                        }),
                        SeatRole::MouseDriver => seat_rotation.map(|r| {
                            crate::controls::Ride::Seat(r, crate::controls::SeatLook::MouseDriver)
                        }),
                        SeatRole::Gunner => Some(crate::controls::Ride::Hull(frame.rotation)),
                        SeatRole::Actor => None,
                    });
                    Some((
                        role,
                        forward.x.atan2(-forward.z),
                        forward.y.clamp(-1.0, 1.0).asin(),
                        seat_rotation.map(|r| {
                            let forward = r * Vec3::NEG_Z;
                            forward.x.atan2(-forward.z)
                        }),
                    ))
                });
                if riding.is_none() {
                    // A passenger on another player (no control object)
                    // turns on its seat like one on a vehicle.
                    let seat = ride.filter(|r| !r.steers).and_then(|r| {
                        let heading = self.motion.presented().get(&r.mount)?.yaw;
                        Some(crate::controls::Ride::Seat(
                            glam::Quat::from_rotation_y(-heading),
                            crate::controls::SeatLook::Passenger,
                        ))
                    });
                    self.controls.set_ride(seat);
                }
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
                // On another player, a passenger faces the seat like one on a
                // vehicle; the first seat of a bot mount turns it instead.
                if let Some(ride) = ride.filter(|r| !r.steers)
                    && let Some(heading) = self.motion.presented().get(&ride.mount).map(|m| m.yaw)
                {
                    self.controls.set_seat_yaw(Some(heading));
                    self.mount_heading = Some(heading);
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
                self.rider_eye = None;
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
                            let (_, node_rotation, position) = node.to_scale_rotation_translation();
                            feet = position;
                            rotation = node_rotation;
                        }
                        let forward = rotation * Vec3::NEG_Z;
                        let mut yaw = forward.x.atan2(-forward.z);
                        // A passenger's body turns on the seat by its own
                        // `mRot.z` (`Player::setPosition` 0x5a6bc0): the
                        // local one by the mouse, others by the host's yaw.
                        // A tumbling body only rolls with its tumble: its
                        // player watches through the corpse camera.
                        let passenger = self
                            .vehicle_assets
                            .definition(&info.definition)
                            .is_some_and(|d| {
                                d.family != bri_vehicles::Family::Tumble
                                    && d.seat_role(usize::from(seat)) == SeatRole::Passenger
                            });
                        let turn = if !passenger {
                            0.0
                        } else if *owner == view.owner {
                            self.controls.passenger_turn()
                        } else {
                            self.motion.presented().get(owner).map_or(0.0, |p| {
                                (p.yaw - yaw + std::f32::consts::PI)
                                    .rem_euclid(std::f32::consts::TAU)
                                    - std::f32::consts::PI
                            })
                        };
                        rotation *= glam::Quat::from_rotation_y(-turn);
                        yaw += turn;
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
                // Players riding players sit on the mount's mount node,
                // rising and falling with its gait (`mountObject`).
                for (owner, vitals) in &view.vitals {
                    let Some(ride) = vitals.ride else {
                        continue;
                    };
                    let Some(mount) = self.motion.presented().get(&ride.mount).cloned() else {
                        continue;
                    };
                    let Some(point) = view
                        .archetypes
                        .resolve(mount.archetype)
                        .mount_points
                        .get(usize::from(ride.seat))
                    else {
                        continue;
                    };
                    let body = glam::Quat::from_rotation_y(-mount.yaw);
                    let (feet, rotation) = match self
                        .avatars
                        .get(&ride.mount)
                        .and_then(|mesh| mesh.model_node(&self.avatar_assets, &point.node))
                    {
                        Some(node) => {
                            let (_, turn, offset) = node.to_scale_rotation_translation();
                            (
                                Vec3::from(mount.feet) + body * offset * mount.scale,
                                body * turn,
                            )
                        }
                        None => (
                            point.seat(Vec3::from(mount.feet), mount.yaw, mount.scale),
                            body,
                        ),
                    };
                    // A passenger's body turns on the seat by its own
                    // `mRot.z`; the rider steering a bot mount faces it.
                    let turn = if ride.steers {
                        0.0
                    } else if *owner == view.owner {
                        self.controls.passenger_turn()
                    } else {
                        self.motion.presented().get(owner).map_or(0.0, |p| {
                            (p.yaw - mount.yaw + std::f32::consts::PI)
                                .rem_euclid(std::f32::consts::TAU)
                                - std::f32::consts::PI
                        })
                    };
                    let rotation = rotation * glam::Quat::from_rotation_y(-turn);
                    self.rider_rotations.insert(*owner, rotation);
                    self.motion.override_presented(
                        *owner,
                        feet,
                        Some(mount.yaw + turn),
                        rotation * Vec3::Y,
                        Vec3::from(mount.velocity),
                        *owner == view.owner,
                    );
                }
                self.vehicles.set_passages(&self.motion.passages());
                self.vehicles.prepare(
                    &mut self.vehicle_assets,
                    &view.vehicles,
                    &view.world.palette,
                );
                // Add-On casings, and debris that is not a vehicle's model,
                // draw as loose item models.
                let mut loose: Vec<_> = self.weapon_shells.model_instances().collect();
                for (model, transform, tint) in self.explosion_debris.models() {
                    if !self
                        .vehicle_assets
                        .push_source_model(model, transform, tint)
                    {
                        loose.push((
                            model.replace('\\', "/").to_ascii_lowercase(),
                            transform,
                            tint,
                        ));
                    }
                }
                self.world_items.set_loose(loose);
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
                            .and_then(|image| image.states.iter().find(|s| s.name == mounted.state))
                            .map(|state| state.sound.as_str())
                            .filter(|sound| !sound.is_empty() && self.audio.is_looping(sound));
                        if let Some(sound) = sound {
                            let eye = Vec3::from(player.feet)
                                + Vec3::Y
                                    * view
                                        .archetypes
                                        .tuning(player.archetype, player.scale)
                                        .stand_eye;
                            loops.insert(
                                (*owner, mounted.hand),
                                (sound.to_string(), eye.to_array()),
                            );
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
        self.update_perf();
        self.update_lag();
        if let Some((request, _, receiver)) = &self.add_on_import
            && let Some(result) = finished(receiver, "Add-On import")
        {
            let result = result.and_then(|imported| imported);
            let request = *request;
            self.add_on_import = None;
            let mut view = crate::add_ons::view(&self.content.paths.root, crate::add_ons::machine());
            match result {
                Ok(notice) => {
                    view.notice = notice;
                    self.ui.apply(UiUpdate::AddOns(view));
                    self.answer(request, Ok(()));
                }
                Err(error) => {
                    self.ui.apply(UiUpdate::AddOns(view));
                    self.answer(request, Err(error));
                }
            }
        }
        if let Some(receiver) = &self.lan_query
            && let Some(found) = finished(receiver, "LAN query")
        {
            self.lan_query = None;
            // A query that died finds nothing rather than spinning forever.
            let found = found.unwrap_or_default();
            self.lan_hosts.clear();
            let mut servers = Vec::new();
            for (address, beacon) in found.lan {
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
                        favorite: false,
                    });
                }
            }
            // Servers joined before or starred, favourites first, with what
            // their game port answered just now.
            let mut saved_rows = Vec::new();
            for (saved, probe) in found.saved {
                let lan = servers
                    .iter_mut()
                    .find(|s| s.address.eq_ignore_ascii_case(&saved.address));
                if let Some(lan) = lan {
                    lan.favorite = saved.favorite;
                    continue;
                }
                let name = if saved.name.is_empty() {
                    saved.address.clone()
                } else {
                    plain_chat(&saved.name)
                };
                let mut row = ServerInfo {
                    address: saved.target().to_string(),
                    name,
                    password: false,
                    dedicated: false,
                    ping_ms: None,
                    players: 0,
                    max_players: 0,
                    bricks: 0,
                    map: String::new(),
                    favorite: saved.favorite,
                };
                match probe {
                    Ok(probe) => {
                        if !probe.listing.name.is_empty() {
                            row.name = plain_chat(&probe.listing.name);
                        }
                        row.map = plain_chat(&probe.listing.map);
                        row.players = probe.listing.players;
                        row.max_players = probe.listing.max_players;
                        row.ping_ms = Some(probe.ping.as_millis().min(9999) as u32);
                    }
                    Err(reason) => row.map = reason,
                }
                saved_rows.push(row);
            }
            // LAN favourites and saved favourites lead the list.
            servers.sort_by_key(|s| !s.favorite);
            saved_rows.sort_by_key(|s| !s.favorite);
            let (favorites, rest): (Vec<_>, Vec<_>) =
                saved_rows.into_iter().partition(|s| s.favorite);
            let mut list = favorites;
            list.extend(servers);
            list.extend(rest);
            self.ui.apply(UiUpdate::LanServers {
                servers: list,
                querying: false,
            });
        }
        if let Some(receiver) = &self.firewall_fix
            && let Some(result) = finished(receiver, "Firewall fix")
        {
            self.firewall_fix = None;
            let result = result.unwrap_or_else(|reason| Err(reason.to_string()));
            let (title, text) = match result {
                Ok(()) => (
                    "Windows Firewall",
                    "Blockland ReImagined can now accept friends through Windows Firewall."
                        .to_string(),
                ),
                Err(reason) => ("Windows Firewall", reason),
            };
            self.ui.apply(UiUpdate::MessageBox {
                title: title.into(),
                text,
            });
        }
        // Build macro playback: one recorded building action per frame so the
        // server's action budget is never exceeded.
        if let Some(action) = self.macro_playback.pop_front() {
            self.ui.core.request(action);
        }
        let third_person = self.third_person_view();
        self.ui.apply(UiUpdate::FirstPerson(!third_person));
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
            // Balls, projectiles, dropped items and package entities move at
            // the frame rate between the host's 20 Hz updates.
            let projectiles = &self.content.weapons.pack.projectiles;
            let passages = self.motion.passages();
            self.ghosts.update(
                game_elapsed.as_secs_f32(),
                view.tick,
                &view.weapons,
                &view.entities,
                |id| {
                    let d = projectiles.get(id)?;
                    let gravity = if d.ballistic { 9.81 * d.gravity } else { 0.0 };
                    Some(crate::ghosts::Flight {
                        acceleration: Vec3::NEG_Y * gravity,
                        lifetime: d.lifetime_ticks,
                        // Balls and grenades bounce; the rest stop until the
                        // host says what the contact did.
                        bounce: (d.ballistic && d.elasticity > 0.0).then_some(
                            crate::ghosts::Bounce {
                                elasticity: d.elasticity,
                                friction: d.friction,
                                rest_speed: d.rest_speed,
                            },
                        ),
                    })
                },
                // Through portals as the host flies them.
                |from, to| {
                    crate::ghosts::Hit::first(&passages, from, to, |from, to| {
                        let length = (to - from).length();
                        let hit = building.solid_segment(from, to).ok()??;
                        Some(crate::ghosts::Hit {
                            position: hit.position,
                            normal: hit.normal,
                            fraction: hit.distance / length,
                            carry: None,
                        })
                    })
                },
            );
            let weapons = self.ghosts.weapons();
            // Rebuilt only when the liquids or the paint change; they were
            // cloned (textures' names and all) several times every frame.
            let (liquids, waters) = match self.motion.collision() {
                Some(mirror) => {
                    let generation = mirror.water_generation();
                    if self.liquid_cache.as_ref().is_none_or(|c| {
                        c.generation != generation || c.palette != view.world.palette
                    }) {
                        let liquids: Arc<[bri_sim::water::TintedWater]> = mirror
                            .tinted_waters(&view.world.bricks, &view.world.palette)
                            .into();
                        self.liquid_cache = Some(LiquidCache {
                            generation,
                            palette: view.world.palette.clone(),
                            waters: liquids.iter().map(|w| w.water.clone()).collect(),
                            liquids,
                        });
                    }
                    let cache = self.liquid_cache.as_ref().expect("filled above");
                    (cache.liquids.clone(), cache.waters.clone())
                }
                None => (Arc::from(Vec::new()), Arc::from(Vec::new())),
            };
            // Sample every body, including the hidden first-person body, once.
            // Visible geometry and attached items consume these same original nodes.
            Self::update_avatar_animation_inputs(
                &mut self.avatar_actions,
                &mut self.avatar_gestures,
                &mut self.avatar_action_images,
                &mut self.weapon_animation_cues,
                &mut self.weapon_animation_drops,
                view,
                game_elapsed.as_secs_f32(),
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
                // `HorseArmor` players, and archetypes that look like it,
                // draw horse.dts.
                let horse = view.archetypes.resolve(player.archetype).look.is_horse();
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
                    // The drawn mesh is built at render time, and only for
                    // bodies in view (`render_scene`).
                    mesh.defer_mesh = true;
                    mesh.instanced = true;
                    // Outfit changes (spray paint included) keep the running
                    // action thread instead of restarting the clip.
                    if let Some(old) = self.avatars.get(owner).filter(|old| old.horse == horse) {
                        mesh.continue_animation(old);
                    }
                    self.avatars.insert(*owner, mesh);
                }
                // Death and respawn as of the drawn pose, not the newest vitals.
                let life = view.vitals.get(owner).map(|vitals| {
                    let (tick, spawned) = if *owner == view.owner {
                        view.poses
                            .get(owner)
                            .map_or((u64::MAX, None), |p| (p.tick, Some(p.spawn_tick)))
                    } else {
                        (self.motion.ticked_at(*owner).unwrap_or(u64::MAX), None)
                    };
                    crate::avatar::drawn_life(vitals, tick, spawned)
                });
                // A respawned body starts fresh: no corpse pose, and none of
                // the old body's action or gesture threads.
                let mesh = self.avatars.get_mut(owner).unwrap();
                if life
                    .and_then(|life| life.body)
                    .is_some_and(|body| mesh.set_body(body))
                {
                    self.avatar_actions.remove(owner);
                    self.avatar_gestures.remove(owner);
                    self.avatar_action_images.remove(owner);
                }
                let mut ready_hands = Vec::new();
                let mut hidden_nodes = Vec::new();
                if let Some(images) = view.weapons.images.get(owner) {
                    for mounted in images {
                        let image = self.content.weapons.pack.images.get(&mounted.image);
                        if let Some(image) = image {
                            hidden_nodes.extend(image.hide_nodes.iter().cloned());
                        }
                        if let Some((right, left)) =
                            bri_weapons::scripted_arm_pose(&mounted.image, &mounted.state)
                        {
                            ready_hands.extend([(0, right), (1, left)]);
                        } else if let Some(image) = image {
                            if image.both_arms {
                                ready_hands.extend([(0, true), (1, true)]);
                            } else {
                                ready_hands.push((mounted.hand, image.arm_ready));
                            }
                        }
                    }
                }
                self.avatars.get_mut(owner).unwrap().set_hidden_nodes(hidden_nodes);
                // `Player::startSkiing` shows the LSki/RSki nodes in the
                // skier's paint colour, carried by the ski vehicle.
                let skis = view
                    .vitals
                    .get(owner)
                    .and_then(|v| v.mounted)
                    .and_then(|(vehicle, _)| view.vehicles.get(&vehicle))
                    .filter(|info| {
                        self.vehicle_assets
                            .definition(&info.definition)
                            .is_some_and(|d| d.family == bri_vehicles::Family::Skis)
                    })
                    .map(|info| {
                        info.color
                            .and_then(|c| view.world.palette.get(usize::from(c)))
                            .or_else(|| view.world.palette.first())
                            .map_or([1.0; 4], |c| [c[0], c[1], c[2], c[3]])
                    });
                self.avatars.get_mut(owner).unwrap().set_skis(skis);
                let dead = life.is_some_and(|life| life.dead);
                self.avatars.get_mut(owner).unwrap().set_dead(dead);
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
                    })
                    // A rule's `setLookLimits` for the body.
                    .or_else(|| view.vitals.get(owner).and_then(|v| v.look_limits));
                let held = crate::avatar::HeldToolPose::from_mounted_images(ready_hands);
                let input = crate::avatar::AvatarAnimationInput {
                    look_limits,
                    mount_rotation: self.rider_rotations.get(owner).copied(),
                    held_tool_pose: if dead {
                        self.combat.hugging.remove(owner);
                        crate::avatar::HeldToolPose::None
                    } else {
                        self.combat.hug_pose(*owner, held)
                    },
                    action: self.avatar_actions.get(owner).cloned().filter(|_| !dead),
                    gesture: self.avatar_gestures.get(owner).cloned().filter(|_| !dead),
                    dead,
                    sitting: !dead
                        && (view.vitals.get(owner).is_some_and(|v| v.sitting)
                            || view
                                .vitals
                                .get(owner)
                                .and_then(|v| v.mounted)
                                .and_then(|(vehicle, seat)| {
                                    let info = view.vehicles.get(&vehicle)?;
                                    let d = self.vehicle_assets.definition(&info.definition)?;
                                    Some(d.seats.get(usize::from(seat))?.pose == "sit")
                                })
                                .unwrap_or(false)
                            // A mount point's `mountThread`.
                            || view
                                .vitals
                                .get(owner)
                                .and_then(|v| v.ride)
                                .and_then(|ride| {
                                    let mount = presented.get(&ride.mount)?;
                                    let kind = view.archetypes.resolve(mount.archetype);
                                    Some(kind.mount_points.get(usize::from(ride.seat))?.pose == "sit")
                                })
                                .unwrap_or(false)),
                    // Riders hold `root` (`Armor::onMount` sets the action
                    // thread to root and mountThread on thread 0); they do
                    // not run, jump or fall with their mount's motion.
                    tick_state: if view
                        .vitals
                        .get(owner)
                        .is_some_and(|v| v.mounted.is_some() || v.ride.is_some())
                    {
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
                    water_coverage: bri_sim::water::deepest(
                        &waters,
                        player.feet,
                        bri_sim::water::body_height(
                            player,
                            &view.archetypes.tuning(player.archetype, player.scale),
                        ),
                    )
                    .map_or(0.0, |(_, c)| c),
                };
                let avatar = self.avatars.get_mut(owner).unwrap();
                let posed =
                    avatar.pose_with_animation(&self.avatar_assets, player, self.animation_time, &input);
                // Add-On code (`avatar.pose`) may draw the body its own way:
                // a ragdoll, a dance. Only the drawing changes.
                if posed.is_ok()
                    && let Some(nodes) = self.client_code.pose(*owner)
                {
                    avatar.override_nodes(&self.avatar_assets, nodes);
                }
                self.cosmetic_faults.absorb("avatar pose", posed);
            }
            self.rider_eye = Self::rider_eye(
                &self.avatars,
                &self.avatar_assets,
                &self.vehicle_assets,
                &self.vehicles,
                view,
                local,
            );
            let synced = self.effects.sync(view.world.clone(), meshes);
            self.cosmetic_faults.absorb("world effects", synced);
            self.foliage.advance(game_elapsed);
            // Match the actual view for flare occlusion, including third-person camera collision.
            let (eye, yaw, pitch, roll) = Self::view_camera(
                &self.controls,
                presented,
                building,
                &self.vehicle_assets,
                &self.vehicles,
                view,
                local,
                self.local_eye()
                    .unwrap_or_else(|| view.archetypes.eye(local)),
                &self.motion.passages(),
                orbit_drawn_offset(&self.controls, &self.avatars),
            )?;
            let (forward, view_right, view_up) = rolled_view_basis(yaw, pitch, roll);
            self.observer_eye = self.controls.observer().map(|_| eye);
            listener = bri_audio::Listener {
                position: eye.to_array(),
                forward: forward.to_array(),
                up: view_up.to_array(),
            };
            // v20 tints the screen with the liquid the camera is in, and
            // colours player splashes and froth with the liquid they touch.
            self.ui
                .apply(UiUpdate::Underwater(bri_sim::water::screen_tints(
                    &liquids, eye,
                )));
            self.actor_effects.set_liquids(liquids, waters);
            let (local_view_yaw, local_view_pitch) = self.controls.view_angles();
            self.world_items.set_palette(&view.world.palette);
            self.world_items.set_render_my_items(
                self.ui
                    .core
                    .prefs
                    .bool_or("$pref::Player::renderMyItems", true),
            );
            self.weapon_effects.set_palette(&view.world.palette);
            // Shots' trails, spray, smoke and sparks fly on through portals.
            let passages = self.motion.passages();
            self.weapon_effects.set_passages(&passages);
            self.effects.world.set_passages(&passages);
            self.actor_effects.set_passages(&passages);
            let items = self.world_items.sync(
                weapons,
                crate::world_items::WorldItemFrame {
                    tick: view.tick,
                    seconds: self.animation_time,
                    eye,
                    local_owner: Some(view.owner),
                    first_person: !third_person,
                    // Mirrors, metal and shadows show the player's own
                    // items as others see them, not at the eye.
                    reflected_self: true,
                },
                |owner| {
                    let avatar = self.avatars.get(&owner)?;
                    let player = presented.get(&owner)?;
                    // Torque draws a first-person image in the eye's frame,
                    // and the eye is the camera: it pitches, rolls and loops
                    // with any seat, so the image stays where it sits on
                    // screen.
                    let eye = if owner == view.owner && !third_person {
                        crate::controls::view_frame(eye, yaw, pitch, roll)
                    } else {
                        let (yaw, pitch) = if owner == view.owner {
                            (local_view_yaw, local_view_pitch)
                        } else {
                            (player.yaw, player.pitch)
                        };
                        avatar.eye_transform(&self.avatar_assets, yaw, pitch)
                    }?;
                    Some(crate::world_items::MountPose {
                        eye,
                        // Torque mounts an image whose mount point has no
                        // `mountN` node (the dribbled basketball's Mount8) at
                        // the player's own transform.
                        mounts: (0..32)
                            .map(|n| {
                                let node = avatar.mount_node(&self.avatar_assets, n as usize);
                                (n, node.unwrap_or_else(|| avatar.body_transform()))
                            })
                            .collect(),
                        actions: (0..32)
                            .filter_map(|n| {
                                Some((n, avatar.mount_action(&self.avatar_assets, n as usize)?))
                            })
                            .collect(),
                        velocity: Vec3::from_array(player.velocity),
                    })
                },
            );
            self.cosmetic_faults.absorb("held and dropped items", items);
            let parts = Self::update_weapon_effect_parts(
                &mut self.weapon_effects,
                &mut self.weapon_cues,
                &self.world_items,
                weapons,
                game_elapsed.as_secs_f32(),
            );
            self.cosmetic_faults.absorb("weapon effects", parts);
            let trails = self
                .actor_effects
                .update_debris_trails(&self.explosion_debris.trails());
            self.cosmetic_faults.absorb("explosion debris", trails);
            // Show Jets in First Person (`$pref::Player::renderMyJets`, off
            // in v20): one's own jets stay out of one's own eye in first
            // person, but mirrors and portals still show them.
            let own_jets_hidden = !third_person
                && !self
                    .ui
                    .core
                    .prefs
                    .bool_or("$pref::Player::renderMyJets", false);
            self.actor_effects
                .set_own_eye(own_jets_hidden.then_some(view.owner));
            let actors = Self::update_actor_effects(
                &mut self.actor_effects,
                &self.avatar_assets,
                &self.avatars,
                &self.vehicles,
                &self.vehicle_assets,
                view,
                presented,
                game_elapsed.as_secs_f32(),
                // `fxLight::TestLOS` casts from the camera to the flare,
                // ignoring the player carrying it.
                |at| {
                    Ok(eye.distance(at) < bri_fx_runtime::FLARE_MAX_DISTANCE
                        && building.effect_visible(bri_world::BrickId::MAX, eye, at)?)
                },
                |from, direction, length| {
                    let hit = building.target(from, direction, length).ok()??;
                    Some((hit.distance, hit.normal))
                },
            );
            self.cosmetic_faults
                .absorb("player and vehicle effects", actors);
            self.explosion_shapes.advance(game_elapsed.as_secs_f32());
            self.beams.advance(game_elapsed.as_secs_f32());
            self.explosion_debris
                .advance(game_elapsed.as_secs_f32(), |from, to| {
                    let delta = to - from;
                    let length = delta.length();
                    if length < 1e-5 {
                        return None;
                    }
                    let hit = building.target(from, delta / length, length).ok()??;
                    Some(crate::weapon_debris::DebrisHit {
                        fraction: (hit.distance / length).clamp(0., 1.),
                        normal: hit.normal.normalize_or(Vec3::Y),
                    })
                });
            let mut shells = Vec::new();
            for request in self.weapon_effects.take_host_requests() {
                match request {
                    crate::weapon_effects::HostRequest::Shell(cue) => shells.push(cue),
                    crate::weapon_effects::HostRequest::Animation(cue) => {
                        if let bri_sim::presentation::CueKind::WeaponAnimation {
                            actor,
                            thread: 0,
                            sequence,
                            image_hand: Some(hand),
                        } = &cue.kind
                        {
                            self.world_items
                                .restart_image_sequence(*actor, *hand, sequence);
                        }
                    }
                }
            }
            let world_items = &self.world_items;
            let eject = |actor: u64, image: &str, hand: u8| {
                world_items
                    .mounted_node(actor, hand, image, "ejectPoint")
                    .or_else(|_| world_items.mounted_node(actor, hand, image, "muzzlePoint"))
                    .ok()
            };
            let queued = self.weapon_shells.cues(&shells, eject, |actor| {
                presented
                    .get(&actor)
                    .map_or(Vec3::ZERO, |p| Vec3::from(p.velocity))
            });
            self.cosmetic_faults.absorb("gun casings", queued);
            let moved =
                self.weapon_shells
                    .advance(game_elapsed.as_secs_f32(), eject, |from, to| {
                        let delta = to - from;
                        let length = delta.length();
                        if length < 1e-5 {
                            return None;
                        }
                        let hit = building.target(from, delta / length, length).ok()??;
                        Some(crate::weapon_debris::DebrisHit {
                            fraction: (hit.distance / length).clamp(0., 1.),
                            normal: hit.normal.normalize_or(Vec3::Y),
                        })
                    });
            self.cosmetic_faults.absorb("gun casings", moved);
            self.audio
                .sync_projectiles(&weapons.projectiles, &self.content.weapons.pack);
            // Physics Quality (or the console's maxdebris) picks the limit.
            let limit = bri_ui::screens::options::debris_limit(&self.ui.core.prefs);
            if limit != self.brick_debris.limit() {
                self.brick_debris.set_limit(limit);
            }
            // What debris costs this frame, so a PC it outgrows keeps less.
            let debris_started = std::time::Instant::now();
            let kills = std::mem::take(&mut self.brick_kills);
            let thrown = self.brick_debris.cues(&kills, building);
            // A kill announced after its brick started fading out stops the
            // fade (see the chunk rebuild's `observe`), and a dead brick
            // leaves its drawn chunk this frame.
            for cue in &kills {
                if let bri_sim::presentation::CueKind::BrickKill { brick, .. } = cue.kind {
                    self.brick_fades.settle(brick);
                    if self.brick_debris.is_dead(brick) {
                        self.chunk_hides
                            .entry(brick)
                            .or_insert((crate::world_chunks::chunk_key(cue.position), false));
                    }
                }
            }
            if self
                .cosmetic_faults
                .absorb("brick debris", thrown)
                .unwrap_or(0)
                > 0
            {
                // Newly dead bricks are not hidden bricks to reveal.
                self.hidden_uploaded = None;
            }
            // Debris and Add-On bodies are local and cosmetic: everyone
            // drawn here shoves them, and nothing about them goes back to
            // the server.
            let bodies = self.client_code.has_bodies();
            let (pushers, shots) = if !self.brick_debris.is_empty() || bodies {
                let pushers = Self::pushers(
                    self.motion.presented(),
                    view,
                    &self.vehicles,
                    &self.vehicle_assets,
                );
                let shots: Vec<_> = view
                    .weapons
                    .fired()
                    .map(|p| crate::local_physics::Shot {
                        id: p.id,
                        position: p.position,
                        velocity: p.velocity,
                    })
                    .collect();
                (pushers, shots)
            } else {
                (Vec::new(), Vec::new())
            };
            if !self.brick_debris.is_empty() {
                self.brick_debris.push(&pushers);
                self.brick_debris.shots(&shots);
            }
            let moved = self
                .brick_debris
                .advance(game_elapsed.as_secs_f32().min(0.25), building);
            self.cosmetic_faults.absorb("brick debris", moved);
            self.brick_debris.spent(debris_started.elapsed());
            if bodies {
                // A corpse does not shove bodies: it may be the one lying in
                // them (a ragdoll drawn over it).
                let alive: Vec<_> = pushers
                    .iter()
                    .filter(|p| {
                        p.id & 1 << 63 != 0 || view.vitals.get(&p.id).is_none_or(|v| v.alive)
                    })
                    .copied()
                    .collect();
                let moved = self.client_code.advance_physics(
                    game_elapsed.as_secs_f32().min(0.25),
                    building,
                    &alive,
                    &shots,
                );
                self.cosmetic_faults.absorb("Add-On bodies", moved);
            }
            self.brick_fades
                .advance(game_elapsed.as_secs_f32(), &self.chunks_left_out);
            // The avatar/image shell and sequence playback APIs are still a host
            // boundary. Retain requests in the adapter and expose its queue-drop
            // diagnostics; do not claim these have been rendered or played.
            let advanced = self.effects.advance(
                game_elapsed.as_secs_f32(),
                eye,
                Vec3::ZERO,
                |id, from, to| building.effect_visible(id, from, to),
            );
            self.cosmetic_faults.absorb("world effects", advanced);
            let weather = self.weather.advance(
                game_elapsed.as_secs_f32(),
                bri_weather::CameraState {
                    position: eye,
                    forward,
                    right: view_right,
                    up: view_up,
                    velocity: Vec3::from_array(local.velocity),
                },
                building,
            );
            self.cosmetic_faults.absorb("weather", weather);
        }
        self.audio.tick(elapsed.as_secs_f32(), listener);
        self.drawn_controls = Some(self.controls.clone());
        Ok(())
    }
    fn pump(&mut self) -> Result<Vec<PlatformCommand>> {
        for sound in self.ui.drain_sounds() {
            self.audio
                .profile(sound.profile, bri_audio::Placement::Listener);
        }
        let mut platform = Vec::new();
        for (id, action) in self.ui.drain_actions() {
            note_trigger(&mut self.controls, &action);
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
            // uses it only to fly faster. The dead click to respawn above.
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
                UiAction::LoadBricksColors(choice) => {
                    self.choose_color_load(choice);
                    Ok(())
                }
                UiAction::RequestSaveList { .. } | UiAction::LoadBricks { .. } => {
                    // Saves dropped in while the game runs convert too.
                    if matches!(action, UiAction::RequestSaveList { .. }) && self.old_saves_started
                    {
                        self.old_saves.start();
                    }
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
                    let max_fps = settings::startup_display(&value).max_fps;
                    if max_fps != self.frame_limit {
                        self.frame_limit = max_fps;
                        platform.push(PlatformCommand::FrameLimit(max_fps));
                    }
                    settings::save(&self.state_dir.join("settings.json"), &value).and_then(|()| {
                        self.audio.apply_settings(&value);
                        self.graphics = crate::graphics::Graphics::from_settings(&value);
                        self.weather.apply_settings(&value)
                    })
                }
                UiAction::SetVolume { channel, value } => self.audio.set_volume(&channel, value),
                UiAction::OpenSavesFolder => show_drop_folder(self.old_saves.saves_folder()),
                UiAction::OpenUrl(url) => {
                    // Only web pages, after the player confirmed them.
                    if bri_ui::ui::web_url(&url).as_deref() == Some(url.as_str())
                        && !bri_crash::open(&url)
                    {
                        bri_console::warn(format!("Could not open {url}"));
                    }
                    Ok(())
                }
                UiAction::HostGame {
                    map,
                    mode,
                    game_mode,
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
                        game_mode,
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
                UiAction::TrustNewServerIdentity { address } => {
                    if self.ui.session_request() != Some(id) {
                        continue;
                    }
                    let result = self
                        .forget_server_identity(&address)
                        .and_then(|()| self.join(id, address, String::new()));
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::TrustAddOnCode => self.client_code.accept_trust(&self.state_dir),
                UiAction::ForgetAddOnTrust => {
                    crate::client_code::ClientCode::forget_trust(&self.state_dir)
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
                UiAction::Game(GameAction::SavePerfCapture) => {
                    let dir = self.state_dir.join("captures");
                    let version = self.ui.core.version.clone();
                    let text = match crate::perf::save_capture(&dir, &self.ui.core, &version) {
                        Ok(path) => {
                            bri_console::echo(format!(
                                "Performance capture saved: {}",
                                path.display()
                            ));
                            format!(
                                "Performance capture saved: {}",
                                path.file_name()
                                    .map_or_else(String::new, |n| n.to_string_lossy().into())
                            )
                        }
                        Err(error) => format!("Performance capture failed: {error:#}"),
                    };
                    self.ui.apply(UiUpdate::BottomPrint {
                        text,
                        seconds: 3.0,
                        hide_bar: false,
                    });
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
                            if let Some(eye) = self.local_eye() {
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
                    // `serverCmdDropPlayerAtCamera`: the server moves the
                    // body, or the vehicle it rides, to the camera (where it
                    // was last left when none is flying), or respawns.
                    match self.network_view() {
                        Some(view) if view.administrator => {
                            let camera = self.camera_view();
                            // The body arrives facing the camera's heading.
                            if let Some(camera) = camera {
                                self.controls.yaw = camera.yaw;
                            }
                            let result = self.command(
                                id,
                                Command::DropPlayerAtCamera(camera),
                                action.clone(),
                            );
                            if result.is_ok() {
                                continue;
                            }
                            result
                        }
                        _ => Ok(()),
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
                UiAction::Game(GameAction::Package {
                    ref package,
                    ref command,
                }) => {
                    let request = Command::Package(bri_sim::session::PackageCommand {
                        package: package.clone(),
                        command: command.clone(),
                        args: Vec::new(),
                    });
                    let result = self.command(id, request, action.clone());
                    if result.is_ok() {
                        continue;
                    }
                    result
                }
                UiAction::Game(GameAction::ToolWheel { notches }) => {
                    if self.controls.aim_wheel(notches) {
                        continue;
                    }
                    // The image's `wheel` command names "package:command".
                    let Some((package, command)) = self
                        .tool_wheel
                        .as_deref()
                        .and_then(|c| c.split_once(':'))
                    else {
                        continue;
                    };
                    let request = Command::Package(bri_sim::session::PackageCommand {
                        package: package.to_string(),
                        command: command.to_string(),
                        args: vec![bri_sim::session::PackageArg::Int(notches.into())],
                    });
                    let result = self.command(id, request, action.clone());
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
                // `serverCmdClearInventory`: the brick cart empties. Ours is
                // kept here, so it empties as Buy Bricks with ten empty slots.
                UiAction::ChatCommand { ref name, .. }
                    if name.eq_ignore_ascii_case("clearinventory") =>
                {
                    let clear = UiAction::BuyBricks {
                        slots: vec![None; 10],
                    };
                    match self.handle_building(id, &clear) {
                        Ok(true) => continue,
                        Ok(false) => Err(anyhow::anyhow!("Not connected")),
                        Err(error) => Err(error),
                    }
                }
                UiAction::ChatCommand { ref name, .. } if name.eq_ignore_ascii_case("invite") => {
                    match self.invite.clone() {
                        Some(invite) => {
                            let copied = copy_to_clipboard(&invite);
                            let text = match &copied {
                                Ok(()) => format!("Invite copied to the clipboard: {invite}"),
                                Err(_) => format!("Your invite: {invite}"),
                            };
                            if let Some(a) = &self.attempt {
                                self.ui.apply_session(a.id, UiUpdate::Chat { text });
                            }
                            Ok(())
                        }
                        None => Err(anyhow::anyhow!(
                            "Only the host has an invite, and it appears once hosting has checked your connection."
                        )),
                    }
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
                            // `serverCmdRet`: back from `/spy` to one's own body.
                            "ret" => Some(Command::ControlPlayer),
                            // `serverCmdWtf` and `serverCmdZombie` repeat
                            // `/confusion` and `/hug`.
                            "wtf" => Some(Command::Emote("confusion".into())),
                            "zombie" => Some(Command::Emote("hug".into())),
                            "sit" | "love" | "hate" | "alarm" | "confusion" | "bsd" | "hug" => {
                                Some(Command::Emote(name.to_ascii_lowercase()))
                            }
                            // Every other slash command goes to the host, which
                            // runs the Add-On command of that name, or answers
                            // that there is none (v20's `/x` calls `serverCmdX`).
                            _ => Some(Command::Package(bri_sim::session::PackageCommand {
                                package: String::new(),
                                command: name.clone(),
                                args: args
                                    .iter()
                                    .cloned()
                                    .map(bri_sim::session::PackageArg::String)
                                    .collect(),
                            })),
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
                        bri_sim::tutorial::MAP_ID.into(),
                        ServerMode::SinglePlayer,
                        None,
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
                UiAction::SteeringPrefs {
                    strafe,
                    auto_return,
                } => {
                    if self.network_view().is_none() {
                        continue;
                    }
                    let result = self.command(
                        id,
                        Command::SteeringPrefs {
                            strafe,
                            auto_return,
                        },
                        action.clone(),
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
                UiAction::ClosePrintSelector | UiAction::CancelWrench { .. } => Ok(()),
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
                        self.send_name(prefs);
                        continue;
                    }
                    result
                }
                UiAction::PreviewSave { map, name } => {
                    let key = (map, name);
                    match self
                        .save_sources
                        .get(&key)
                        .and_then(|source| crate::save_picture::path_for(source))
                    {
                        Some(path) => self.save_previews.start(key, path, &self.runtime),
                        None => {
                            self.save_previews.cancel();
                            self.ui.apply(UiUpdate::SavePreview {
                                map: key.0,
                                name: key.1,
                                preview: IconRef::None,
                            });
                        }
                    }
                    Ok(())
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
                    let saved =
                        crate::servers::SavedServers::load(&self.state_dir.join("servers.json"));
                    let pins: BTreeMap<String, Vec<u8>> =
                        read_small_json(&self.state_dir.join("trusted-hosts.json"))
                            .unwrap_or_default();
                    self.runtime.spawn(async move {
                        let broadcast = [bri_net::discovery::broadcast()];
                        let lan =
                            bri_net::discovery::query(&broadcast, Duration::from_millis(1200));
                        // Every saved server is asked at once over its game
                        // port; a probe never pins anything.
                        let probes = saved.servers.into_iter().map(|server| {
                            let pin = pins.get(&server.address).cloned();
                            async move {
                                let probe = async {
                                    let target =
                                        bri_net::invite::JoinTarget::parse(server.target())?;
                                    let route = target.resolve().await?;
                                    let pin = match (route.key, pin) {
                                        (Some(key), _) => HostPin::Key(key),
                                        (None, Some(certificate)) => {
                                            HostPin::Certificate(certificate)
                                        }
                                        (None, None) => HostPin::FirstUse,
                                    };
                                    bri_net::client::probe(
                                        route.address,
                                        &pin,
                                        Duration::from_secs(2),
                                    )
                                    .await
                                }
                                .await
                                .map_err(|error| probe_failure(&error));
                                (server, probe)
                            }
                        });
                        let (lan, saved) = tokio::join!(lan, futures_join_all(probes));
                        let _ = send.send(JoinList {
                            lan: lan.unwrap_or_default(),
                            saved,
                        });
                    });
                    self.lan_query = Some(receive);
                    self.ui.apply(UiUpdate::LanServers {
                        servers: vec![],
                        querying: true,
                    });
                    Ok(())
                }
                UiAction::RequestAddOns => {
                    let view = crate::add_ons::view(&self.content.paths.root, crate::add_ons::machine());
                    self.ui.apply(UiUpdate::AddOns(view));
                    Ok(())
                }
                UiAction::SetAddOnEnabled { ref id, enabled } => {
                    crate::add_ons::set_enabled(
                        &self.content.paths.root,
                        crate::add_ons::machine(),
                        id,
                        enabled,
                    )
                        .map(|view| self.add_ons_changed(view))
                }
                UiAction::DefaultAddOns => crate::add_ons::defaults(&self.content.paths.root, crate::add_ons::machine())
                    .map(|view| self.add_ons_changed(view)),
                UiAction::ImportAddOn { id: ref row } => {
                    let root = self.content.paths.root.clone();
                    let started = if self.add_on_import.is_some() {
                        Err(anyhow::anyhow!(
                            "Another add-on is importing; wait for it to finish."
                        ))
                    } else {
                        crate::add_ons::importer().and_then(|importer| {
                            crate::add_ons::start_import(&root, crate::add_ons::machine(), row, &importer)
                        })
                    };
                    match started {
                        Ok(receiver) => {
                            let mut view = crate::add_ons::view(&root, crate::add_ons::machine());
                            crate::add_ons::mark_importing(&mut view, row);
                            view.notice = "Importing... the game keeps running meanwhile.".into();
                            self.ui.apply(UiUpdate::AddOns(view));
                            self.add_on_import = Some((id, row.clone(), receiver));
                            continue;
                        }
                        Err(error) => Err(error),
                    }
                }
                UiAction::ToggleFavorite { ref address } => {
                    let path = self.state_dir.join("servers.json");
                    let mut saved = crate::servers::SavedServers::load(&path);
                    // LAN rows are keyed by address; saved rows may be invites.
                    let target = bri_net::invite::JoinTarget::parse(address);
                    let key = target.as_ref().map_or(address.clone(), |t| t.address());
                    let invite = target
                        .ok()
                        .filter(|t| t.key().is_some())
                        .map(|t| t.to_string())
                        .or_else(|| {
                            // The same order as joining: a saved pin before
                            // an (unsigned) LAN listing, so starring a
                            // spoofed LAN row cannot pin its sender.
                            let pins: BTreeMap<String, Vec<u8>> =
                                read_small_json(&self.state_dir.join("trusted-hosts.json"))
                                    .unwrap_or_default();
                            let (pin, _) = crate::servers::join_pin(
                                None,
                                pins.get(&key).cloned(),
                                self.lan_hosts.get(address),
                            );
                            let HostPin::Certificate(certificate) = pin else {
                                return None;
                            };
                            Some(
                                bri_net::invite::JoinTarget::Direct {
                                    host: target_host(address),
                                    port: address
                                        .rsplit_once(':')
                                        .and_then(|(_, p)| p.parse().ok())
                                        .unwrap_or(bri_net::invite::DEFAULT_PORT),
                                    key: Some(bri_net::invite::host_key(&certificate)),
                                }
                                .to_string(),
                            )
                        });
                    let name = self
                        .ui
                        .core
                        .servers
                        .iter()
                        .find(|s| s.address == *address)
                        .map(|s| s.name.clone())
                        .unwrap_or_default();
                    saved.toggle_favorite(&key, invite, &name);
                    let result = saved.save(&path);
                    self.ui.core.request(UiAction::QueryLan);
                    result
                }
                UiAction::AllowFirewall { port } => {
                    let (send, receive) = mpsc::sync_channel(1);
                    std::thread::spawn(move || {
                        let _ =
                            send.send(crate::firewall::allow(port).map_err(|e| format!("{e:#}")));
                    });
                    self.firewall_fix = Some(receive);
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
                _ => Err(anyhow::anyhow!("This isn't available yet.")),
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
        if self.auto_quality {
            self.auto_quality = false;
            self.pick_quality(&device.adapter_info());
        }
        self.gpu_name = device.adapter_info().name;
        self.item_ui.gpu_stopped();
        self.world_items.clear_gpu();
        crate::vehicles::ClientVehicles::gpu_stopped(&mut self.vehicle_assets);
        self.explosion_shapes.gpu_stopped();
        self.beams.gpu_stopped();
        self.tutorial_targets.gpu_stopped();
        self.shell_gpu = None;
        for avatar in self.avatars.values_mut() {
            avatar.gpu = None;
            avatar.instance = None;
        }
        // The avatar preview and the world compile their pipelines on
        // worker threads; the menus draw meanwhile (see gpu_build).
        let preview_device = device.clone();
        self.avatar_preview = Some(crate::gpu_build::Building::spawn(
            "avatar preview pipelines",
            move || crate::avatar::Preview::new(&preview_device),
        ));
        self.preview_dirty = self.preview_request.is_some();
        bri_render::color::set_color_vision(bri_ui::screens::options::color_vision(
            &self.ui.core.prefs,
        ));
        let samples = self.graphics.samples;
        let (scene_device, shadows) = (device.clone(), self.graphics.shadows);
        self.renderer = Some(crate::gpu_build::Building::spawn(
            "scene pipelines",
            move || SceneRenderer::with_settings(&scene_device, format, samples, shadows),
        ));
        self.reflections = Some(bri_render::reflection::Reflections::new(
            device,
            format,
            samples,
            self.graphics.reflections,
        ));
        self.foliage.gpu_stopped();
        self.foliage.set_samples(samples);
        self.client_code.gpu_stopped();
        self.item_skins.gpu_stopped();
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
        self.hidden_lines = Some(bri_render::lines::LineRenderer::new(
            device,
            format,
            bri_render::scene::DEPTH_FORMAT,
            samples,
        ));
        self.vignette = Some(bri_render::vignette::VignetteRenderer::new(
            device,
            format,
            bri_render::scene::DEPTH_FORMAT,
            samples,
        ));
        self.selection_lines = Some(bri_render::lines::LineRenderer::new(
            device,
            format,
            bri_render::scene::DEPTH_FORMAT,
            samples,
        ));
        self.selection_uploaded = None;
        self.hidden_uploaded = None;
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
        self.ghost_look = None;
        self.ghost_uploaded = u64::MAX;
        self.remote_ghosts.clear();
        self.debris_models.clear();
        self.fade_models.clear();
        self.package_models.clear();
        if let Some(lines) = &mut self.hidden_lines {
            lines.clear();
        }
        self.hidden_uploaded = None;
        if let Some(lines) = &mut self.selection_lines {
            lines.clear();
        }
        self.selection_uploaded = None;
        self.depth = None;
        Ok(())
    }
    fn close_requested(&mut self) -> bool {
        // Closing again while being asked quits, so the window can always close.
        let asked = self
            .close_asked
            .replace(std::time::Instant::now())
            .is_some_and(|at| at.elapsed() < Duration::from_secs(30));
        if !self.ui.core.unsaved_changes || asked {
            return true;
        }
        self.ui.core.confirm_unsaved(bri_ui::ui::Callback::Quit);
        false
    }
    fn gpu_lost(&mut self) {
        self.client_code.device_lost();
    }
    fn gpu_stopped(&mut self) {
        self.client_code.gpu_stopped();
        self.item_skins.gpu_stopped();
        self.item_ui.gpu_stopped();
        self.world_items.clear_gpu();
        crate::vehicles::ClientVehicles::gpu_stopped(&mut self.vehicle_assets);
        self.explosion_shapes.gpu_stopped();
        self.beams.gpu_stopped();
        self.tutorial_targets.gpu_stopped();
        self.shell_gpu = None;
        for avatar in self.avatars.values_mut() {
            avatar.gpu = None;
            avatar.instance = None;
        }
        self.avatar_preview = None;
        self.renderer = None;
        self.reflections = None;
        self.environment_probe = None;
        self.foliage.gpu_stopped();
        self.weather_renderer = None;
        self.effects_renderer = None;
        self.hidden_lines = None;
        self.selection_lines = None;
        self.gpu_scene = None;
        self.gpu_terrain.clear();
        self.gpu_palette = None;
        self.gpu_chunks.clear();
        self.ghost_gpu = None;
        self.ghost_look = None;
        self.ghost_uploaded = u64::MAX;
        self.remote_ghosts.clear();
        self.debris_models.clear();
        self.fade_models.clear();
        self.package_models.clear();
        if let Some(lines) = &mut self.hidden_lines {
            lines.clear();
        }
        self.hidden_uploaded = None;
        if let Some(lines) = &mut self.selection_lines {
            lines.clear();
        }
        self.selection_uploaded = None;
        self.depth = None;
    }
    fn render_scene(&mut self, frame: &mut RenderContext<'_>) -> Result<bool> {
        // The last frame, holding any picture copied then, was submitted.
        // Failures are logged by the writer; success is not news.
        self.save_shots.submitted();
        self.save_shots.poll(frame.device);
        if let Some(path) = self.save_picture.take() {
            self.take_save_picture(frame, path)?;
        }
        // Anti-aliasing and shadow quality rebuild world pipelines and maps;
        // a map change needs renderers built for the new map.
        // Colour-vision assistance is a pipeline constant, too.
        let vision = bri_ui::screens::options::color_vision(&self.ui.core.prefs);
        if std::mem::take(&mut self.gpu_restart)
            || bri_render::color::color_vision() != vision
            || self.renderer.as_mut().and_then(|r| r.ready()).is_some_and(|r| {
                r.samples() != self.graphics.samples || r.shadow_settings() != self.graphics.shadows
            })
        {
            self.gpu_ready(frame.device, frame.queue, frame.format)?;
        }
        self.item_ui.register_icons(frame);
        // Until its pipelines finish compiling, the preview stays due.
        if self.preview_dirty
            && let Some((appearance, rotation, distance)) = &self.preview_request
            && let Some(preview) = self
                .avatar_preview
                .as_mut()
                .context("Avatar preview GPU not initialized")?
                .ready()
        {
            preview.render(&self.avatar_assets, appearance, *rotation, *distance, frame)?;
            self.ui.apply(UiUpdate::AvatarPreview(IconRef::External(
                crate::avatar::Preview::ID,
            )));
            self.preview_dirty = false;
        }
        if let Some(((map, name), picture)) = self.save_previews.ready.take() {
            crate::save_picture::upload(frame, &picture);
            self.ui.apply(UiUpdate::SavePreview {
                map,
                name,
                preview: IconRef::External(crate::save_picture::ID),
            });
        }

        // The map bake's leak cleanup patches the map's lightmaps once: the
        // scene kept for uploads, and the uploaded textures.
        if !self.light_volume.leaks.is_empty()
            && let Some(scene) = self.cpu_scene.as_mut()
        {
            let fixes = std::mem::take(&mut self.light_volume.leaks);
            let changed = bri_render::map_lighting::TexelFix::apply(&fixes, &mut scene.images);
            if let Some(gpu) = &self.gpu_scene {
                gpu.patch_images(frame.queue, &scene.images, &changed)?;
            }
        }
        // Once Dynamic is chosen, the map's lightmaps take its images (what
        // each light leaves and where each reaches, per texel) and the scene
        // uploads again with them, so the other modes never carry them.
        if self.graphics.lighting == 3
            && !self.light_volume.dynamic_equipped
            && self.light_volume.map.is_some()
            && let Some(scene) = self.cpu_scene.as_mut()
        {
            bri_render::map_lighting::DynamicSheet::equip(&self.light_volume.dynamic, scene);
            self.light_volume.dynamic_equipped = true;
            self.gpu_scene = None;
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
        // Draw what this frame's tick posed, not input that arrived since.
        let controls = self.drawn_controls.as_ref().unwrap_or(&self.controls);
        let third_person = draws_third_person(
            controls,
            view.vitals.get(&view.owner).is_none_or(|v| v.alive),
        );
        let mut hidden = self.combat.hidden_bodies(&view.vitals);
        // Players whose archetype looks like a package model draw as it, in
        // place of the Blockhead; the local player's in first person only
        // in other views (mirrors, portals), as the Blockhead does.
        let package_catalog = packages_for(&self.package_catalog, view);
        let mut package_placements: Vec<_> =
            crate::packages::entity_placements(self.ghosts.entities_at(view.tick, &view.entities))
                .collect();
        let mut own_package_body = None;
        if let Some(catalog) = package_catalog {
            for (owner, placement) in
                crate::packages::body_placements(catalog, &view.archetypes, self.motion.presented())
            {
                hidden.insert(owner);
                match placement {
                    Some(p) if owner == view.owner && !third_person => own_package_body = Some(p),
                    Some(p) => package_placements.push(p),
                    None => {}
                }
            }
        }
        let renderer = self
            .renderer
            .as_mut()
            .context("Scene GPU not initialized")?
            .wait();
        renderer.set_filtering(frame.device, self.graphics.filtering);
        let timing = self.time_passes || self.ui.core.perf.wants_net();
        renderer.time_passes(frame.device, frame.queue, timing);
        match renderer.pass_times(frame.device) {
            Some((_, passes)) => {
                self.gpu_passes = passes
                    .iter()
                    .map(|(pass, time)| (*pass, time.as_secs_f32() * 1000.0))
                    .collect();
            }
            None => self.gpu_passes.clear(),
        }
        if self.gpu_scene.is_none() {
            self.gpu_broken.clear();
            self.gpu_scene = Some(renderer.upload(frame.device, frame.queue, scene)?);
            self.light_volume.uploaded = false;
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
        self.light_volume
            .upload(renderer, frame.device, frame.queue, self.graphics.lighting)?;
        if let Some(view) = self.attempt.as_ref().and_then(|a| a.view.as_ref()) {
            self.light_volume
                .tint(renderer, frame.queue, &view.broken_shapes, &view.map_lights);
        }
        if self.gpu_palette.is_none()
            && let Some(palette) = &self.palette
        {
            self.gpu_palette = Some(renderer.upload(frame.device, frame.queue, &palette.scene)?);
            self.chunk_uploads.extend(self.cpu_chunks.keys().copied());
        }
        if let Some(palette) = &self.gpu_palette {
            let pending: Vec<&SceneData> = self
                .chunk_uploads
                .iter()
                .filter_map(|key| self.cpu_chunks.get(key))
                .collect();
            if pending.iter().map(|c| c.vertices.len()).sum::<usize>() > 1 << 16 {
                renderer.reserve_chunks(&pending)?;
            }
            for key in std::mem::take(&mut self.chunk_uploads) {
                if let Some(chunk) = self.cpu_chunks.get(&key) {
                    self.gpu_chunks
                        .insert(key, renderer.upload_chunk(frame.device, frame.queue, chunk, palette)?);
                    if let Some(bricks) = self.cpu_chunk_bricks.get(&key) {
                        self.gpu_chunk_bricks.insert(key, bricks.clone());
                    }
                    // A chunk built before a brick died still draws it.
                    for (hidden_key, applied) in self.chunk_hides.values_mut() {
                        if *hidden_key == key {
                            *applied = false;
                        }
                    }
                }
            }
        }
        // Dead bricks leave their drawn chunks now, not when the rebuilt
        // chunks land. A hide ends once the brick is back (respawned) or
        // the uploaded chunk no longer holds it.
        let (debris, uploads, drawn) = (
            &self.brick_debris,
            &self.chunk_uploads,
            &self.gpu_chunk_bricks,
        );
        self.chunk_hides.retain(|brick, (key, _)| {
            let back =
                !debris.is_dead(*brick) && view.world.bricks.get(brick).is_some_and(|b| b.visible);
            !back
                && (uploads.contains(key)
                    || drawn.get(key).is_some_and(|b| b.vertices(*brick).is_some()))
        });
        for (brick, (key, applied)) in &mut self.chunk_hides {
            if *applied {
                continue;
            }
            *applied = true;
            if let (Some(gpu), Some(bricks)) =
                (self.gpu_chunks.get(key), self.gpu_chunk_bricks.get(key))
                && let Some(vertices) = bricks.vertices(*brick)
            {
                gpu.hide_vertices(frame.queue, vertices);
            }
        }
        // Options > Advanced's temp brick colours and flash.
        let ghost_look = crate::world_scene::TempBrickLook::from_prefs(&self.ui.core.prefs);
        if let Some(building) = &self.building
            && self.ghost_uploaded != ghost_key(building)
        {
            // A copied build in hand shows instead of the single ghost.
            let ghosts: Option<Vec<bri_world::Brick>> = match building.copy_ghost() {
                Some(copy) => Some(copy.to_vec()),
                None => building.ghost().map(|g| vec![g.clone()]),
            };
            // Built around the first brick, so a moved ghost (or a world
            // change that leaves it as it was) only moves its transform;
            // only a new look rebuilds it, textures and all.
            let placed = ghosts.map(|mut bricks| {
                let anchor = Vec3::from(bricks[0].position);
                for brick in &mut bricks {
                    brick.position = (Vec3::from(brick.position) - anchor).to_array();
                }
                let look = GhostLook {
                    bricks,
                    blocked: building.ghost_blocked(),
                    temp: ghost_look,
                    palette: view.world.palette.clone(),
                };
                (anchor, look)
            });
            match &placed {
                Some((_, look)) if self.ghost_look.as_ref() == Some(look) => {}
                _ => {
                    self.ghost_gpu = None;
                    self.ghost_look = None;
                }
            }
            let anchor = placed.as_ref().map(|(anchor, _)| *anchor);
            if let Some((anchor, look)) = placed
                && self.ghost_look.is_none()
            {
                let world = bri_net::protocol::PublicWorld {
                    name: "Local unplanted ghost".into(),
                    map_id: view.world.map_id.clone(),
                    palette: look.palette.clone(),
                    bricks: look
                        .bricks
                        .iter()
                        .cloned()
                        .enumerate()
                        .map(|(i, b)| (i as u64, b))
                        .collect(),
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
                // Warn before a plant the server would refuse: the ghost
                // turns red (not in v20, which only showed the error icon).
                if look.blocked {
                    for vertex in &mut data.vertices {
                        vertex.color = BLOCKED_GHOST;
                    }
                }
                translucent_ghost(&mut data, &ghost_look);
                if !data.indices.is_empty() {
                    self.ghost_gpu = Some((
                        renderer.upload(frame.device, frame.queue, &data)?,
                        bri_render::scene::GpuInstances::new(frame.device, 1)?,
                    ));
                }
                self.ghost_look = Some(look);
                if let Some((_, instances)) = &mut self.ghost_gpu {
                    instances.update(
                        frame.queue,
                        &[bri_render::scene::SceneTransform {
                            transform: glam::Mat4::from_translation(anchor),
                            tint: [1.0; 4],
                        }],
                    )?;
                }
            } else if let (Some(anchor), Some((_, instances))) = (anchor, &mut self.ghost_gpu) {
                instances.update(
                    frame.queue,
                    &[bri_render::scene::SceneTransform {
                        transform: glam::Mat4::from_translation(anchor),
                        tint: [1.0; 4],
                    }],
                )?;
            }
            self.ghost_uploaded = ghost_key(building);
        }
        // Other players' ghost bricks, translucent in their colour and shape.
        self.remote_ghosts.retain(|owner, (ghost, _)| {
            *owner != view.owner
                && view
                    .vitals
                    .get(owner)
                    .and_then(|v| v.ghost.as_ref())
                    .is_some_and(|now| now == ghost)
        });
        for (owner, vitals) in &view.vitals {
            let Some(ghost) = vitals.ghost.as_ref().filter(|_| *owner != view.owner) else {
                continue;
            };
            if self.remote_ghosts.contains_key(owner) {
                continue;
            }
            let mut brick = bri_world::Brick::new(
                bri_world::ContentRef::Resolved(ghost.definition.clone()),
                ghost.position,
                *owner,
            );
            brick.quarter_turns = ghost.quarter_turns;
            brick.color = ghost.color;
            brick.print = ghost.print.clone().map(bri_world::ContentRef::Resolved);
            let world = bri_net::protocol::PublicWorld {
                name: "Remote unplanted ghost".into(),
                map_id: view.world.map_id.clone(),
                palette: view.world.palette.clone(),
                bricks: bri_world::Bricks::unit(0, brick),
            };
            // A brick this client cannot draw shows nothing.
            let gpu = match (&self.meshes, &self.materials) {
                (Some(meshes), Some(materials)) => crate::world_scene::build_world_scene_materials(
                    &world,
                    meshes,
                    100_000,
                    Some(materials),
                )
                .ok()
                .filter(|data| !data.indices.is_empty())
                .map(|mut data| {
                    translucent_ghost(&mut data, &ghost_look);
                    renderer.upload(frame.device, frame.queue, &data)
                })
                .transpose()?,
                _ => None,
            };
            self.remote_ghosts.insert(*owner, (ghost.clone(), gpu));
        }
        if let Some(building) = &self.building
            && let (Some(meshes), Some(materials)) = (&self.meshes, &self.materials)
        {
            // v20 `showBricks` images (hammer, wrench, printer, wands, bricks)
            // reveal non-rendering bricks as box outlines in their paint
            // colour (`fxDTSBrick::renderObject`), not as ghost bricks.
            let show = matches!(
                building.equipment(),
                crate::building::Equipment::Brick(_)
                    | crate::building::Equipment::Hammer
                    | crate::building::Equipment::Wrench
                    | crate::building::Equipment::Printer
                    | crate::building::Equipment::Wand
            );
            let fading = if show {
                self.brick_fades.outlined()
            } else {
                Vec::new()
            };
            if (self.hidden_uploaded != Some(show) || self.hidden_fading != fading)
                && let Some(lines) = &mut self.hidden_lines
            {
                let mut vertices = vec![];
                if show {
                    // Hidden bricks, and any fading in or out drawn under
                    // alpha 0.1 (`brick_fade::OUTLINE_ALPHA`).
                    let faint: BTreeSet<u64> = fading
                        .iter()
                        .filter(|(_, faint)| *faint)
                        .map(|(id, _)| *id)
                        .collect();
                    let easing: BTreeSet<u64> = fading.iter().map(|(id, _)| *id).collect();
                    let bricks = view
                        .world
                        .bricks
                        .iter()
                        .filter(|(id, b)| !b.visible && !easing.contains(*id))
                        .chain(
                            faint
                                .iter()
                                .filter_map(|id| Some((id, view.world.bricks.get(id)?))),
                        );
                    for (id, brick) in bricks {
                        if self.brick_debris.is_dead(*id) {
                            continue;
                        }
                        let Some(mesh) = crate::brick_cover::mesh(brick, meshes) else {
                            continue;
                        };
                        let Some(color) = view.world.palette.get(usize::from(brick.color)) else {
                            continue;
                        };
                        let (low, high) = hidden_brick_box(brick, mesh);
                        bri_render::lines::box_edges(
                            low,
                            high,
                            [color[0], color[1], color[2]],
                            &mut vertices,
                        );
                    }
                }
                lines.set_lines(frame.device, &vertices)?;
                self.hidden_uploaded = Some(show);
                self.hidden_fading = fading;
            }
            let selection = self.building.as_ref().and_then(|b| b.outline());
            if self.selection_uploaded != Some(selection)
                && let Some(lines) = &mut self.selection_lines
            {
                let mut vertices = vec![];
                if let Some((low, high)) = selection {
                    // Just outside the box, so its edges do not fight the
                    // faces of the bricks they frame.
                    let margin = Vec3::splat(0.02);
                    bri_render::lines::box_edges(
                        Vec3::from(low) - margin,
                        Vec3::from(high) + margin,
                        SELECTION_COLOR,
                        &mut vertices,
                    );
                }
                lines.set_lines(frame.device, &vertices)?;
                self.selection_uploaded = Some(selection);
            }
            if let (Some(palette), Some(gpu_palette)) = (&self.palette, &self.gpu_palette) {
                self.debris_models.upload(
                    &self.brick_debris,
                    renderer,
                    frame.device,
                    frame.queue,
                    meshes,
                    palette,
                    gpu_palette,
                    materials,
                    &view.world.palette,
                )?;
            }
            if let Some(world) = &self.world_source {
                self.fade_models.upload(
                    &self.brick_fades,
                    &self.chunks_left_out,
                    world,
                    renderer,
                    frame.device,
                    frame.queue,
                    meshes,
                    materials,
                )?;
            }
            self.package_models.upload(
                package_catalog,
                package_placements,
                own_package_body,
                renderer,
                frame.device,
                frame.queue,
                meshes,
                materials,
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
        self.beams.upload(renderer, frame.device, frame.queue)?;
        self.tutorial_targets
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
        let (eye, yaw, pitch, roll) = Self::view_camera(
            controls,
            self.motion.presented(),
            self.building
                .as_ref()
                .context("Camera collision mirror missing")?,
            &self.vehicle_assets,
            &self.vehicles,
            view,
            local,
            self.rider_eye
                .or(self.motion.local_eye())
                .unwrap_or_else(|| view.archetypes.eye(local)),
            &self.motion.passages(),
            orbit_drawn_offset(controls, &self.avatars),
        )?;
        self.rendered_camera = Some((eye, yaw, pitch));
        self.rendered_roll = roll;
        // Explosion `CameraShake`: 10 degrees of view rotation per unit of offset.
        let shake = self.actor_effects.camera_shake(eye) * 10f32.to_radians();
        let (forward, right, up) = rolled_view_basis(
            yaw + shake.z.clamp(-0.3, 0.3),
            pitch + shake.x.clamp(-0.3, 0.3),
            roll,
        );
        let aspect = frame.size.0 as f32 / frame.size.1 as f32;
        let mut camera = Camera::oriented(
            eye.to_array(),
            forward.to_array(),
            up.to_array(),
            aspect,
            vertical_fov(controls.fov().to_radians(), aspect),
            0.05,
            FAR_PLANE,
        );
        camera.apply_environment(scene);
        // The host's environment (Admin Menu, Add-Ons) over the map's own;
        // an untouched map skips it and draws exactly as authored.
        let live = (!view.environment.is_empty()).then(|| {
            bri_content::atmosphere::resolve(&authored_environment(scene), &view.environment, view.tick)
        });
        if let Some(live) = &live {
            camera.apply_atmosphere(live);
        }
        if let Some(vignette) = &mut self.vignette {
            vignette.update(
                frame.queue,
                live.and_then(|l| l.vignette).map(|v| (v.color, v.multiply)),
                aspect,
            );
        }
        camera.ambient[3] = f32::from(self.light_volume.mode(self.graphics.lighting));
        camera.atmosphere[2] = (self.animation_time % 86400.0) as f32;
        // `$pref::visibleDistanceMax` caps the map's visible distance; the
        // fog start scales with it so the fade keeps its shape.
        let cap = bri_ui::screens::options::visible_distance_max(&self.ui.core.prefs);
        if camera.atmosphere[3] > 0. && camera.atmosphere[1] > cap {
            let scale = cap / camera.atmosphere[1];
            camera.atmosphere[0] *= scale;
            camera.atmosphere[1] = cap;
        }
        renderer.update_camera(frame.queue, &camera);
        // Mirrors an Add-On's bricks carry: the planes that reflect live
        // this frame, each with its own view of the world.
        if self
            .reflections
            .as_ref()
            .is_none_or(|r| !r.matches(frame.format, renderer.samples()))
        {
            self.reflections = Some(bri_render::reflection::Reflections::new(
                frame.device,
                frame.format,
                renderer.samples(),
                self.graphics.reflections,
            ));
        }
        let reflections = self.reflections.as_mut().unwrap();
        reflections.set_settings(self.graphics.reflections);
        // A knocked-out mirror brick's mirrors leave its place and ride
        // its debris instead.
        let debris = &self.brick_debris;
        let eye = glam::Vec4::from(camera.eye).truncate();
        let mut mirrors = self.mirror_index.mirrors(|id| debris.is_dead(id), eye);
        crate::mirrors::debris(debris, &self.mirror_shapes, eye, &mut mirrors);
        reflections.prepare(
            frame.device,
            frame.queue,
            renderer,
            &camera,
            frame.size,
            &mirrors,
        )?;
        let reflecting = reflections.live() > 0;
        // Metal reflects the world around the nearest metal surface within
        // mirror distance, with mirrors on; otherwise only the sky.
        if self
            .environment_probe
            .as_ref()
            .is_none_or(|p| !p.matches(renderer, frame.format))
        {
            self.environment_probe = Some(bri_render::environment_probe::EnvironmentProbe::new(
                frame.device,
                renderer,
                frame.format,
                renderer.samples(),
            ));
        }
        let settings = self.graphics.reflections;
        let metal = (settings.planes > 0)
            .then(|| {
                self.vehicle_assets
                    .metal_centres()
                    .into_iter()
                    .filter(|c| c.distance(eye) <= settings.distance)
                    .min_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)))
            })
            .flatten();
        self.environment_probe.as_mut().unwrap().prepare(
            frame.device,
            frame.queue,
            renderer,
            &camera,
            metal,
            settings.distance,
        );
        // Bodies build their mesh here, once the view is known. Without
        // shadows one out of view draws nothing, so it is not built; with
        // shadows or a live mirror every body may show. A mirror shows the
        // player's own body in first person too.
        let in_view =
            crate::culling::Frustum::new(glam::Mat4::from_cols_array(&camera.view_projection));
        let probing = self
            .environment_probe
            .as_ref()
            .is_some_and(|p| !p.faces().is_empty());
        let anywhere = casts || reflecting || probing;
        let mut bodies_drawn = BTreeSet::new();
        let passages = self.motion.passages();
        // Riders are cut where their vehicle is.
        let ridden: BTreeMap<_, u64> = view
            .vehicles
            .iter()
            .flat_map(|(id, info)| info.occupants.iter().flatten().map(move |o| (*o, *id)))
            .collect();
        for (owner, avatar) in &mut self.avatars {
            if (*owner != view.owner || third_person || anywhere) && !hidden.contains(owner) {
                let (center, radius) = avatar.bounding_sphere();
                // A body part way through an opening draws on both sides.
                avatar.straddle = match ridden.get(owner) {
                    Some(vehicle) => self.vehicles.straddle(*vehicle).copied(),
                    None => crate::portal_view::Straddle::find(&passages, avatar.middle(), radius),
                };
                let seen = |c: Vec3| in_view.sees_sphere(c, radius);
                if !anywhere
                    && !seen(center)
                    && avatar
                        .straddle
                        .is_none_or(|s| !seen(s.carry.transform_point3(center)))
                {
                    continue;
                }
                avatar.build_pending(&self.avatar_assets)?;
                avatar.upload(renderer, frame.device, frame.queue)?;
                bodies_drawn.insert(*owner);
            }
        }
        let effects_camera = bri_fx_runtime::Camera {
            view_projection: glam::Mat4::from_cols_array(&camera.view_projection),
            position: eye,
            right,
            up,
        };
        if self.client_code.is_started() {
            let world = if self.client_code.reads_world() {
                let image_meshes = self.world_items.held_image_meshes();
                let skeletons = if self.client_code.poses_bodies() {
                    self.avatars
                        .iter_mut()
                        .map(|(owner, avatar)| (*owner, avatar.skeleton(&self.avatar_assets)))
                        .collect()
                } else {
                    Default::default()
                };
                // Death and respawn as drawn, not the newest vitals.
                let lives = self
                    .avatars
                    .iter()
                    .filter_map(|(owner, avatar)| Some((*owner, avatar.life()?)))
                    .collect();
                std::sync::Arc::new(crate::client_code::world_view(
                    view,
                    self.ghosts.entities_at(view.tick, &view.entities),
                    self.motion.presented(),
                    &self.vehicles,
                    &self.vehicle_assets,
                    &camera,
                    &self.world_items,
                    image_meshes,
                    crate::client_code::DrawnBodies { skeletons, lives },
                ))
            } else {
                Default::default()
            };
            let player_view = bri_client_sandbox::View {
                fov: self.controls.fov(),
                normal_fov: self.controls.normal_fov(),
                size: [frame.size.0, frame.size.1],
                first_person: !third_person,
                aiming: self.controls.aiming(),
                alive: view.vitals.get(&view.owner).is_none_or(|v| v.alive),
            };
            self.client_code
                .run_frame(self.animation_time, eye, forward, world, player_view);
            for (asset, at, volume) in self.client_code.take_sounds() {
                let placement = match at {
                    Some(at) => bri_audio::Placement::World(bri_audio::Vec3::from(at)),
                    None => bri_audio::Placement::Listener,
                };
                self.audio.play_asset(asset, placement, volume);
            }
            self.client_code.prepare(
                frame.device,
                frame.queue,
                frame.format,
                bri_render::scene::DEPTH_FORMAT,
                renderer.samples(),
                effects_camera.view_projection,
                eye,
                [frame.size.0, frame.size.1],
            );
        }
        // Every other view this frame (mirror and portal planes, the
        // environment probe's faces) and the eyes they all see from: the
        // terrain tiles and effect lights every view shares cover them all.
        let planes = self
            .reflections
            .as_ref()
            .map(|r| r.plan().planes.clone())
            .unwrap_or_default();
        let probe_views = self
            .environment_probe
            .as_ref()
            .map(|p| p.face_views())
            .unwrap_or_default();
        let weather_camera = self.weather.world.camera();
        let other_views = crate::views::other_views(
            &planes,
            &probe_views,
            &crate::views::PlayerCamera {
                forward: weather_camera.forward,
                right,
                up,
                velocity: weather_camera.velocity,
            },
        );
        let eyes = crate::views::eyes(eye, &other_views);
        let rgb = |v: [f32; 4]| [v[0], v[1], v[2]];
        self.item_skins.prepare(
            frame.device,
            frame.queue,
            frame.format,
            bri_render::scene::DEPTH_FORMAT,
            renderer.samples(),
            &mut self.world_items,
            self.client_code.trusts_server(),
            crate::item_skins::Light {
                sun_direction: rgb(camera.sun_direction),
                sun_color: rgb(camera.sun_color),
                ambient: rgb(camera.ambient),
            },
            effects_camera.view_projection,
            eye,
            [frame.size.0, frame.size.1],
            self.animation_time as f32,
        );
        let world_frame = self.effects.world.snapshot_in_view(&effects_camera);
        let weapon_frame = self
            .weapon_effects
            .world()
            .snapshot_in_view(&effects_camera);
        let actor_frame = self.actor_effects.world().snapshot_in_view(&effects_camera);
        let (effects_frame, deferred_lights) =
            combine_effect_frames(world_frame, [weapon_frame, actor_frame], &eyes);
        let (fog_start, fog_end) = if camera.atmosphere[3] > 0. {
            (camera.atmosphere[0], camera.atmosphere[1])
        } else {
            (cap, cap + 1.)
        };
        for terrain in &mut self.gpu_terrain {
            terrain.update(frame.device, frame.queue, &eyes, fog_end.max(1.))?;
        }
        self.ui.core.name_tags = name_tags(
            view,
            self.motion.presented(),
            self.building.as_ref(),
            glam::Mat4::from_cols_array(&camera.view_projection),
            eye,
            (fog_start.max(0.), fog_end.max(1.)),
            (frame.size.0 as f32, frame.size.1 as f32),
            self.ui.scale(),
            controls.observer().is_none(),
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
        if let Some(lines) = &self.hidden_lines {
            lines.prepare(frame.queue, effects_camera.view_projection);
        }
        if let Some(lines) = &self.selection_lines {
            lines.prepare(frame.queue, effects_camera.view_projection);
        }
        weather_renderer.prepare(
            frame.queue,
            effects_camera.view_projection,
            &self.weather.world.snapshot(),
        )?;
        // Each other view sees the sprites, plants, weather and Add-On
        // layers from its own eye: its own culling and far-to-near order,
        // and billboards turned to face it. The probe's faces also see the
        // mirrors in them, so metal reflects the world the player sees.
        let mut layers = crate::views::Layers {
            effects: [
                &self.effects.world,
                self.weapon_effects.world(),
                self.actor_effects.world(),
            ],
            sprites: &mut *effects_renderer,
            foliage: &mut self.foliage,
            weather: &self.weather.world,
            drops: &mut *weather_renderer,
            client_code: &mut self.client_code,
            item_skins: &mut self.item_skins,
            fog: (fog_start, fog_end),
        };
        for v in &other_views {
            layers.prepare(frame, v)?;
        }
        if let Some(reflections) = &mut self.reflections {
            let size = bri_render::environment_probe::PROBE_SIZE;
            for face in &probe_views {
                reflections.prepare_view(
                    frame.device,
                    frame.queue,
                    face.view,
                    face.view_projection,
                    face.eye,
                    (size, size),
                );
            }
        }
        let (depth, multisampled, _) = self.depth.as_ref().unwrap();
        let depth = depth.create_view(&Default::default());
        let multisampled = multisampled
            .as_ref()
            .map(|color| color.create_view(&Default::default()));
        let world_target = multisampled.as_ref().unwrap_or(frame.target);
        // A changed fog colour clears the frame with it too.
        let clear_color = match &live {
            Some(l) if l.fog_color != scene.fog.color => [l.fog_color[0], l.fog_color[1], l.fog_color[2], 1.0],
            _ => scene.clear_color,
        };
        let [r, g, b, a] = clear_color.map(f64::from);
        if let (Some(gpu), Some(view)) = (
            self.gpu_scene.as_mut(),
            self.attempt.as_ref().and_then(|a| a.view.as_ref()),
        ) && self.gpu_broken != view.broken_shapes
        {
            // Smashed shapes stop drawing (`renderWhenDestroyed = 0`); only a
            // new mission restores them, with a fresh upload.
            let ranges: Vec<_> = view
                .broken_shapes
                .difference(&self.gpu_broken)
                .filter_map(|node| self.shape_indices.get(node).cloned())
                .collect();
            gpu.hide_indices(&ranges);
            self.gpu_broken.extend(view.broken_shapes.iter().copied());
        }
        let mut scenes = vec![self.gpu_scene.as_ref().unwrap()];
        scenes.extend(self.gpu_chunks.values());

        scenes.extend(
            self.remote_ghosts
                .values()
                .filter_map(|(_, gpu)| gpu.as_ref()),
        );
        // Bodies draw through their one-instance body transform.
        let avatar_draws: Vec<_> = self
            .avatars
            .iter()
            .filter(|(owner, _)| bodies_drawn.contains(*owner))
            .filter_map(|(owner, avatar)| {
                Some((*owner, (avatar.gpu.as_ref()?, avatar.instance.as_ref()?)))
            })
            .collect();
        scenes.extend(self.mount_meshes.values().filter_map(|m| m.gpu.as_ref()));
        scenes.extend(self.fade_models.scenes());
        // Models every view draws; the player's own body and held items
        // differ between the player's view and a mirror's.
        let mut shared_draws = Vec::new();
        if let Some((ghost, placed)) = &self.ghost_gpu {
            shared_draws.push((ghost, placed));
        }
        shared_draws.extend(crate::vehicles::ClientVehicles::draws(&self.vehicle_assets));
        shared_draws.extend(self.gpu_terrain.iter().flat_map(|t| t.draws()));
        shared_draws.extend(self.explosion_shapes.draws());
        shared_draws.extend(self.beams.draws());
        shared_draws.extend(self.tutorial_targets.draws());
        if let Some((scene, instances)) = &self.shell_gpu
            && self.weapon_shells.active_count() > 0
        {
            shared_draws.push((scene, instances));
        }
        shared_draws.extend(self.debris_models.draws());
        shared_draws.extend(self.package_models.draws());
        let mut item_draws = self.world_items.draws();
        item_draws.extend(
            avatar_draws
                .iter()
                .filter(|(owner, _)| *owner != view.owner || third_person)
                .map(|(_, draw)| *draw),
        );
        item_draws.extend(shared_draws.iter().copied());
        {
            use bri_render::scene::ShadowCasters;
            // Players, vehicles and items (dropped and held) cast, like v20's
            // projected shape shadows; bricks only with the BrickShadows pref,
            // and bricks that do not cast still stop shadows passing through
            // them. The map (interiors and terrain) neither casts nor stops
            // them: its shadows are baked (see bri_render::shadow).
            let chunks: Vec<&GpuScene> = self
                .gpu_chunks
                .values()
                .chain(self.fade_models.scenes())
                .collect();
            let (mut bodies, blockers) = if self.graphics.brick_shadows {
                (chunks, Vec::new())
            } else {
                (Vec::new(), chunks)
            };
            let mut blocking = Vec::new();

            // Rigged mounts (the horse) draw through their own meshes, not
            // the vehicle models, but cast like every other vehicle.
            bodies.extend(self.mount_meshes.values().filter_map(|m| m.gpu.as_ref()));
            // The player's own items cast from their hands, as others see
            // them, not from the first-person copy at the eye.
            let mut models = self.world_items.reflection_draws();
            models.extend(avatar_draws.iter().map(|(_, draw)| *draw));
            models.extend(crate::vehicles::ClientVehicles::draws(&self.vehicle_assets));
            if let Some((scene, instances)) = &self.shell_gpu
                && self.weapon_shells.active_count() > 0
            {
                models.push((scene, instances));
            }
            // Debris is bricks, so it follows the same setting as the bricks
            // it broke from; Add-On models cast like items.
            if self.graphics.brick_shadows {
                models.extend(self.debris_models.draws());
            } else {
                blocking.extend(self.debris_models.draws());
            }
            models.extend(self.package_models.draws());
            models.extend(self.package_models.own_draws());
            // In the Unified modes the map's own walls shade objects from
            // the sun too (the map layer), so they are sunlit exactly where
            // the walls beside them are.
            let map: Vec<&GpuScene> = if self.graphics.lighting != 0 {
                self.gpu_scene.iter().collect()
            } else {
                Vec::new()
            };
            renderer.begin_timing(frame.encoder);
            renderer.render_shadows_with_map(
                frame.encoder,
                ShadowCasters {
                    scenes: &bodies,
                    instances: &models,
                },
                ShadowCasters {
                    scenes: &blockers,
                    instances: &blocking,
                },
                &map,
            );
        }
        let clear = wgpu::Color { r, g, b, a };
        let reflections = self.reflections.as_ref().unwrap();
        if reflecting {
            let mut mirrored = self.world_items.reflection_draws();
            mirrored.extend(avatar_draws.iter().map(|(_, draw)| *draw));
            mirrored.extend(shared_draws.iter().copied());
            mirrored.extend(self.package_models.own_draws());
            let (foliage, sprites, drops) = (&self.foliage, &*effects_renderer, &*weather_renderer);
            let (layers, skins) = (&self.client_code, &self.item_skins);
            // As the player's view draws them after the world.
            let late = |pass: &mut wgpu::RenderPass<'_>, view: usize| {
                foliage.render_view(pass, view);
                sprites.render_view(pass, view);
                drops.render_view(pass, view);
                skins.render_view(pass, view);
                layers.render_view(pass, view);
            };
            reflections.render(renderer, frame.encoder, &scenes, &mirrored, clear, &late);
            renderer.mark(frame.encoder, "mirrors");
        }
        let probe = self.environment_probe.as_ref().unwrap();
        if !probe.faces().is_empty() {
            let mut around = self.world_items.reflection_draws();
            around.extend(avatar_draws.iter().map(|(_, draw)| *draw));
            around.extend(shared_draws.iter().copied());
            around.extend(self.package_models.own_draws());
            let (foliage, sprites, drops) = (&self.foliage, &*effects_renderer, &*weather_renderer);
            let (layers, skins) = (&self.client_code, &self.item_skins);
            let late = |pass: &mut wgpu::RenderPass<'_>, view: usize| {
                foliage.render_view(pass, view);
                sprites.render_view(pass, view);
                drops.render_view(pass, view);
                skins.render_view(pass, view);
                layers.render_view(pass, view);
            };
            let surfaces =
                |pass: &mut wgpu::RenderPass<'_>, view: usize| reflections.draw_surfaces(pass, view);
            probe.render(renderer, frame.encoder, &scenes, &around, clear, &surfaces, &late);
        }
        let surfaces = |pass: &mut wgpu::RenderPass<'_>| reflections.draw_surfaces(pass, 0);
        renderer.render_world(
            frame.encoder,
            bri_render::scene::WorldPass {
                view: 0,
                color: world_target,
                resolve: None,
                depth: &depth,
                viewport: None,
                clear: Some(clear),
                after_opaque: (!mirrors.is_empty()).then_some(&surfaces as _),
                after_all: None,
            },
            &scenes,
            &item_draws,
        );
        renderer.mark(frame.encoder, "world");
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
        self.item_skins.render(&mut pass);
        self.client_code.render(&mut pass);
        if let Some(lines) = &self.hidden_lines {
            lines.render(&mut pass);
        }
        if let Some(lines) = &self.selection_lines {
            lines.render(&mut pass);
        }
        if let Some(vignette) = &self.vignette {
            vignette.render(&mut pass);
        }
        drop(pass);
        self.item_skins.resolve(frame.encoder);
        self.client_code.resolve(frame.encoder);
        renderer.end_timing(frame.encoder, "effects");
        Ok(true)
    }
}
/// The map's own sun, light and fog, which the host's environment
/// settings change.
fn authored_environment(scene: &SceneData) -> bri_content::atmosphere::Authored {
    bri_content::atmosphere::Authored {
        sun_direction: scene.sun_direction,
        direct_light: scene.sun_color,
        ambient_light: scene.ambient,
        fog_start: scene.fog.start,
        fog_end: scene.fog.end,
        fog_color: scene.fog.color,
    }
}
/// An Add-On selection box's outline: the Duplicator family's gold.
const SELECTION_COLOR: [f32; 3] = [1.0, 0.78, 0.12];
/// Brick triangles the client draws at most, after covered faces are culled:
/// a million simple bricks, about 1.7 GB of chunk vertices.
const WORLD_TRIANGLE_BUDGET: usize = 16_000_000;

/// Everything the local ghost's mesh depends on; its bricks sit around the
/// first one, which the ghost's transform places.
#[derive(PartialEq)]
struct GhostLook {
    bricks: Vec<bri_world::Brick>,
    blocked: bool,
    temp: crate::world_scene::TempBrickLook,
    palette: Vec<[f32; 4]>,
}

/// Liquids for one liquid generation of the collision mirror and palette.
struct LiquidCache {
    generation: u64,
    palette: Vec<[f32; 4]>,
    liquids: Arc<[bri_sim::water::TintedWater]>,
    waters: Arc<[bri_content::water::Water]>,
}

/// The saved name as the server accepts it (`clean_player_name`: v20's 23
/// characters, trimmed) and "Blockhead" when blank.
fn player_name(prefs: &AvatarPrefs) -> String {
    bri_sim::session::clean_player_name(&prefs.lan_name)
}
/// The Avatar screen's clan tags, as the host will clean them.
fn clan(prefs: &AvatarPrefs) -> bri_sim::session::Clan {
    bri_sim::session::Clan {
        prefix: prefs.clan_prefix.clone(),
        suffix: prefs.clan_suffix.clone(),
    }
    .cleaned()
}
/// Wait for every future (a small join_all, to avoid a dependency).
async fn futures_join_all<F: std::future::Future + Send + 'static>(
    futures: impl IntoIterator<Item = F>,
) -> Vec<F::Output>
where
    F::Output: Send + 'static,
{
    let handles: Vec<_> = futures.into_iter().map(tokio::spawn).collect();
    let mut out = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(value) = handle.await {
            out.push(value);
        }
    }
    out
}

/// What the join list shows for a saved server that did not answer.
fn probe_failure(error: &anyhow::Error) -> String {
    use bri_net::client::JoinError;
    match error.downcast_ref::<JoinError>() {
        Some(JoinError::NoAnswer(_)) => "(no answer)".into(),
        Some(JoinError::IdentityChanged(_)) => "(host changed)".into(),
        Some(JoinError::Rejected(reason)) if reason.contains("version") => "(other version)".into(),
        _ if error.to_string().contains("Could not find") => "(unknown name)".into(),
        _ => "(unreachable)".into(),
    }
}

/// The host part of a normalized `host:port` / `[v6]:port` address.
fn target_host(address: &str) -> String {
    address
        .rsplit_once(':')
        .map_or(address, |(host, _)| host)
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string()
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A small JSON file in the client state folder, or None when missing or
/// unreadable.
pub(crate) fn read_small_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    std::fs::metadata(path)
        .ok()
        .filter(|m| m.len() <= 1024 * 1024)
        .and_then(|_| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// Read, change and crash-safely write back a small JSON state file.
fn update_small_json<T: serde::de::DeserializeOwned + serde::Serialize + Default>(
    path: &Path,
    change: impl FnOnce(&mut T),
) -> Result<()> {
    let mut value: T = read_small_json(path).unwrap_or_default();
    change(&mut value);
    bri_files::replace(path, &serde_json::to_vec_pretty(&value)?)?;
    Ok(())
}
/// Automatic rejoins after a dropped connection before giving up.
const MAX_RECONNECTS: u8 = 3;
#[cfg(test)]
mod tests {
    /// A first-person image sits in the view's frame, so it stays put on
    /// screen however a seat pitches, rolls or loops: the frame's axes are
    /// the rendered camera's.
    #[test]
    fn a_first_person_image_stays_on_screen_through_a_loop() {
        use super::{Vec3, rolled_view_basis};
        // A held item's eye offset: right, forward and down of the eye.
        let offset = Vec3::new(0.5, -0.4, -1.1);
        let eye = Vec3::new(3.0, 40.0, -7.0);
        for (yaw, pitch, roll) in [
            (0.0, 0.0, 0.0),
            (0.7, 1.2, 0.0),
            (-2.1, 0.3, 2.8),
            (1.4, -1.5, -3.1),
            (0.2, 0.1, std::f32::consts::PI),
        ] {
            let frame = crate::controls::view_frame(eye, yaw, pitch, roll).unwrap();
            let (forward, right, up) = rolled_view_basis(yaw, pitch, roll);
            let placed = frame.transform_point3(offset) - eye;
            let on_screen = Vec3::new(placed.dot(right), placed.dot(up), -placed.dot(forward));
            assert!(
                on_screen.abs_diff_eq(offset, 1e-4),
                "yaw {yaw} pitch {pitch} roll {roll}: {on_screen} vs {offset}"
            );
        }
    }
    #[test]
    fn a_broken_bulb_switches_off_its_lights_and_rules_tint_the_rest() {
        use super::{BTreeSet, Vec3, map_light_tints};
        use bri_render::map_lighting::MapLight;
        use bri_sim::session::MapLightRule;
        let light = |x: f32, z: f32| MapLight {
            position: [x, 10.0, z],
            color: [1.0; 3],
            inner: 0.0,
            outer: 30.0,
            channel: Some(0),
        };
        // The bulb at x = 0 and the positions v20's Bedroom fit gives its
        // lights: 5.9, 11.8 and 19.9 units off. One light across the room.
        // Two tubes at x = 100 and 107 fit as one light between them.
        let lights = [light(5.9, 0.0), light(0.0, 11.8), light(-19.9, 0.0), light(60.0, 0.0), light(104.0, 14.0)];
        let shapes = [
            (7u32, Vec3::new(0.0, 10.0, 0.0)),
            (8, Vec3::new(100.0, 10.0, 0.0)),
            (9, Vec3::new(107.0, 10.0, 0.0)),
        ];
        let rule = MapLightRule { position: [60.0, 10.0, 0.0], radius: 2.0, tint: [1.0, 0.0, 0.0] };
        let whole = map_light_tints(&lights, &shapes, &BTreeSet::new(), &[rule]);
        assert_eq!(whole, [Vec3::ONE, Vec3::ONE, Vec3::ONE, Vec3::X, Vec3::ONE]);
        let broken = map_light_tints(&lights, &shapes, &BTreeSet::from([7, 8]), &[rule]);
        assert_eq!(broken, [Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, Vec3::X, Vec3::splat(0.5)]);
        let both = map_light_tints(&lights, &shapes, &BTreeSet::from([8, 9]), &[]);
        assert_eq!(both[4], Vec3::ZERO);
        // An Add-On cannot light a broken bulb again.
        let lit = MapLightRule { position: [0.0, 10.0, 0.0], radius: 30.0, tint: [2.0; 3] };
        assert_eq!(map_light_tints(&lights, &shapes, &BTreeSet::from([7]), &[lit])[0], Vec3::ZERO);
    }
    /// Max, v0.1.9: holding a jeep with the Gravity Gun, the wheel
    /// switched tools instead of reeling. Fire on foot goes to the
    /// building path, which never told `controls` the trigger was down, so
    /// the tool never got the wheel. The trigger is noted before routing.
    #[test]
    fn the_trigger_is_noted_whichever_path_takes_the_click() {
        use bri_ui::api::{GameAction, HeldControl, UiAction};
        let mut c = super::Controls::default();
        let fire = |down| UiAction::Game(GameAction::Held { control: HeldControl::Fire, down });
        assert!(super::building_action(&fire(true)), "on foot, building takes the click");
        super::note_trigger(&mut c, &fire(true));
        assert!(c.held(HeldControl::Fire));
        super::note_trigger(&mut c, &UiAction::Game(GameAction::DropTool));
        assert!(c.held(HeldControl::Fire), "other actions leave it");
        super::note_trigger(&mut c, &fire(false));
        assert!(!c.held(HeldControl::Fire));
    }
    /// Max, v0.1.10: "gravity gun scrolling still switches tool instead of
    /// letting me reel in or out whatever i am currently grabbed on to".
    /// Every frame `follow_control` told `controls` the player was in
    /// control of their body, which dropped the held trigger, so the tool
    /// never claimed the wheel. Here the real UI takes the mouse, and each
    /// frame runs as the game's does: actions drained and the trigger
    /// noted, control followed, the held tool's wheel claimed.
    #[test]
    fn rolling_the_wheel_with_the_trigger_held_reels_and_never_switches_tools() {
        use bri_ui::{
            api::{BindInput, GameAction, HeldControl, UiAction},
            binds::Platform,
            geom::Rect,
            input::{InputEvent, MouseButton},
            schema::UiPack,
            screens::ctrl,
            ui::UiConfig,
        };
        use super::{PathBuf, Ui, UiUpdate};
        let mut pack = UiPack::default();
        for name in ["PlayGui", "LoadingGui"] {
            pack.layouts.insert(name.into(), ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480)));
        }
        let mut ui = Ui::new(
            std::rc::Rc::new(bri_ui::pack::Pack::from_parts(pack, PathBuf::new())),
            UiConfig { size: (1280, 960), scale: Some(2.0), platform: Platform::Windows },
            bri_ui::api::Settings { binds: Some(vec![]), mouse_type: 2, ..Default::default() },
        );
        ui.core.binds.bind(BindInput::Wheel, "scrollInventory");
        ui.core.binds.bind(BindInput::Mouse(MouseButton::Left), "mouseFire");
        ui.apply(UiUpdate::Connection(bri_ui::api::ConnectionState::InGame {
            server_name: "Test".into(),
            max_players: 8,
            local: true,
            single_player: true,
            admin: true,
        }));
        ui.drain_actions();
        let mut controls = super::Controls::default();
        let mut tool_wheel = None;
        let mut frame = |ui: &mut Ui, controls: &mut super::Controls| -> Vec<UiAction> {
            let actions: Vec<_> = ui.drain_actions().into_iter().map(|(_, a)| a).collect();
            for action in &actions {
                super::note_trigger(controls, action);
                if let UiAction::Game(action) = action {
                    controls.action(action);
                }
            }
            controls.follow(bri_sim::session::ControlObject::Player, 1, None);
            super::claim_wheel(ui, &mut tool_wheel, Some("gravity-gun:reel".into()));
            actions
        };
        let reels = |actions: &[UiAction]| {
            actions.iter().filter(|a| matches!(a, UiAction::Game(GameAction::ToolWheel { .. }))).count()
        };
        let (x, y) = (640.0, 480.0);
        let button = MouseButton::Left;
        // Grab: the trigger held over many frames stays held.
        ui.handle_input(InputEvent::MouseDown { button, x, y });
        for _ in 0..10 {
            frame(&mut ui, &mut controls);
        }
        assert!(controls.held(HeldControl::Fire), "the trigger is still held");
        // Rolled forward and back: each notch reels, nothing else moves.
        for delta in [1.0, 1.0, -1.0] {
            ui.handle_input(InputEvent::Wheel { delta });
            let actions = frame(&mut ui, &mut controls);
            assert_eq!(
                actions,
                vec![UiAction::Game(GameAction::ToolWheel { notches: delta as i32 })],
                "only the tool sees the wheel"
            );
        }
        // Let go: the wheel is the inventory's again.
        ui.handle_input(InputEvent::MouseUp { button, x, y });
        frame(&mut ui, &mut controls);
        assert!(!controls.held(HeldControl::Fire));
        ui.handle_input(InputEvent::Wheel { delta: 1.0 });
        assert_eq!(reels(&frame(&mut ui, &mut controls)), 0);
    }
    #[test]
    fn only_a_steering_seat_drives_its_vehicle() {
        let steers = |yes: bool| move |_: u64, seat: usize| yes && seat == 0;
        assert_eq!(super::driven_vehicle(Some((7, 0)), steers(true)), Some(7));
        assert_eq!(super::driven_vehicle(Some((7, 1)), steers(true)), None, "a passenger");
        // A tumble's seat: its rider is drawn from the host's poses.
        assert_eq!(super::driven_vehicle(Some((7, 0)), steers(false)), None, "a tumble");
        assert_eq!(super::driven_vehicle(None, steers(true)), None);
    }
    #[test]
    fn the_own_body_hides_only_once_the_camera_reaches_the_eye() {
        use bri_ui::api::GameAction;
        let mut c = super::Controls::default();
        c.action(&GameAction::ToggleFirstPerson { fast: false });
        c.advance_view(1.0);
        assert!(super::draws_third_person(&c, true));
        c.action(&GameAction::ToggleFirstPerson { fast: false });
        c.advance_view(0.1);
        assert!(
            super::draws_third_person(&c, true),
            "halfway in, the body still draws"
        );
        c.advance_view(0.1);
        assert!(!super::draws_third_person(&c, true));
        assert!(
            super::draws_third_person(&c, false),
            "the dead see their body"
        );
        c.action(&GameAction::ToggleFirstPerson { fast: false });
        c.advance_view(1.0 / 60.0);
        assert!(
            super::draws_third_person(&c, true),
            "the body shows as soon as the camera starts out"
        );
    }
    /// Max, a16: in the Tutorial's horse lesson (no jet on foot) the jet
    /// key never reached the horse, so the rider could not get off.
    #[test]
    fn a_tutorial_rider_still_sends_jet_and_jump_to_the_mount() {
        let no_jet = bri_sim::session::Abilities {
            run: true,
            jump: false,
            jet: false,
        };
        let pressed = bri_sim::player::MoveInput {
            forward: 1.0,
            jump: true,
            jet: true,
            ..Default::default()
        };
        let riding = super::rider_input(no_jet, pressed, true);
        assert!(
            riding.jet && riding.jump,
            "dismount and horse jump reach the mount"
        );
        let walking = super::rider_input(no_jet, pressed, false);
        assert!(
            !walking.jet && !walking.jump,
            "the lesson's limits still hold on foot"
        );
        assert_eq!(walking.forward, 1.0);
    }
    /// Found by the screen harness: a joined guest's Player List read
    /// "127.0.0.1:28000 - 2/64 Players" instead of the host's name and size.
    #[test]
    fn a_joined_server_goes_by_its_listed_name_and_size() {
        let listing = |name: &str, max_players| bri_net::protocol::Listing {
            name: name.into(),
            map: "Bedroom".into(),
            players: 1,
            max_players,
        };
        assert_eq!(
            super::joined_server(&listing("Max's Build Server", 12), "127.0.0.1:28000", 64),
            ("Max's Build Server".to_string(), 12)
        );
        // A listing without a name or size keeps what the join had.
        assert_eq!(
            super::joined_server(&listing("  ", 0), "10.0.0.5:28000", 64),
            ("10.0.0.5:28000".to_string(), 64)
        );
    }
    #[test]
    fn looking_straight_down_or_past_it_keeps_turning_with_the_yaw() {
        use glam::Vec3;
        use std::f32::consts::FRAC_PI_2;
        // v20's look limits are exactly +-90 degrees; the chase camera adds
        // cameraTilt (0.261) past that.
        for pitch in [-FRAC_PI_2 - 0.261, -FRAC_PI_2, -1.2, 0.0, FRAC_PI_2] {
            for yaw in [0.0f32, 1.0, -2.5] {
                let (forward, right, up) = super::view_basis(yaw, pitch);
                for v in [forward, right, up] {
                    assert!(v.is_finite() && (v.length() - 1.0).abs() < 1e-5);
                }
                assert!(forward.dot(right).abs() < 1e-5 && forward.dot(up).abs() < 1e-5);
                assert!(right.y.abs() < 1e-6, "the horizon stays level");
                let camera = bri_render::scene::Camera::oriented(
                    [0.0; 3],
                    forward.to_array(),
                    up.to_array(),
                    1.5,
                    1.0,
                    0.05,
                    100.0,
                );
                assert!(camera.view_projection.iter().all(|v| v.is_finite()));
            }
        }
        // Straight down, turning spins the view (no snap to a fixed roll).
        let (_, a, _) = super::view_basis(0.0, -FRAC_PI_2);
        let (_, b, _) = super::view_basis(1.0, -FRAC_PI_2);
        assert!(a.angle_between(b) > 0.99);
        // The chase camera passes over the head smoothly.
        let (before, _, _) = super::view_basis(0.3, -FRAC_PI_2 + 0.01);
        let (after, _, _) = super::view_basis(0.3, -FRAC_PI_2 - 0.01);
        assert!(before.angle_between(after) < 0.021);
        assert!(after.dot(Vec3::new(0.3f32.sin(), 0.0, -0.3f32.cos())) < 0.0);
    }
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
    fn saved_pins_are_keyed_by_the_typed_host() {
        assert_eq!(
            super::target_host("play.example.com:28000"),
            "play.example.com"
        );
        assert_eq!(super::target_host("[2001:db8::1]:28000"), "2001:db8::1");
        assert_eq!(super::target_host("203.0.113.10:28001"), "203.0.113.10");
    }
    #[test]
    fn small_state_files_update_in_place() {
        let dir = std::env::temp_dir().join(format!("bri-recent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("state.json");
        for address in ["a.example.com", "b.example.com", "A.example.com"] {
            super::update_small_json(&file, |list: &mut Vec<String>| {
                list.retain(|a| !a.eq_ignore_ascii_case(address));
                list.insert(0, address.to_string());
            })
            .unwrap();
        }
        let list: Vec<String> = super::read_small_json(&file).unwrap();
        assert_eq!(list, ["A.example.com", "b.example.com"]);
        std::fs::remove_dir_all(&dir).unwrap();
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
            heading: None,
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
                .ends_with("effects-runtime-pack-005")
        );
        // v20's 21 items, plus any a loaded Add-On adds (the default
        // Add-Ons, once a checkout's content has them installed).
        let items = &app.content.weapons.pack.items;
        let base = items.keys().filter(|id| !id.contains(':')).count();
        assert_eq!(base, 21);
        let all = items.len();
        assert_eq!(app.tool_ui.server_catalog().items.len(), all);
        assert_eq!(app.content.datablocks["ItemData"].len(), all);
        assert_eq!(app.content.item_physics.bounds.len(), all);
        app.ui.core.request(UiAction::HostGame {
            map: "v20/add-ons/map_bedroom/bedroom.mis".into(),
            mode: ServerMode::SinglePlayer,
            game_mode: None,
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
            bri_weapons::PRINTER
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
        // v20 names it "wrench" (wrenchItem uiName), lower case.
        assert_eq!(app.ui.core.hud.tool_name, "wrench");
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
                .any(|d| d.item == bri_weapons::WRENCH)
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
        super::translucent_ghost(&mut scene, &Default::default());
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
        assert_eq!(
            material.parameters.map(|p| p[0]),
            Some([0.8, 0.3, 0.3, 0.0])
        );
    }
    /// A driver steers, and is predicted, by the steering prefs the host
    /// uses (its copy, in the pose), never by a copy the host lacks; with
    /// no pose yet, by their own, which the host assumes too.
    #[test]
    fn a_driver_is_predicted_with_the_hosts_steering_prefs() {
        let mut prefs = bri_ui::prefs::Prefs::default();
        assert_eq!(
            super::steering_in_use(None, &prefs),
            bri_sim::session::DEFAULT_STEERING,
            "the shipped prefs are the host's default"
        );
        prefs.set("$pref::Input::UseStrafeSteering", "1");
        assert_eq!(super::steering_in_use(None, &prefs), (true, false));
        let pose = bri_sim::session::VehiclePose {
            id: 1,
            tick: 3,
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            velocity: [0.0; 3],
            steering: 0.0,
            wheel_suspension: vec![],
            wheel_rotation: vec![],
            wheel_contact: vec![],
            wheel_tire: vec![],
            turret_aim: [0.0; 2],
            jetting: false,
            angular_velocity: [0.0; 3],
            mouse_steering: [0.0; 2],
            driver_input: 0,
            driver_steering: (false, false),
            steering_quiet: 0,
            actor: None,
        };
        // The host has not heard (or lost) the change: it still steers by
        // the mouse, so the client predicts the mouse too.
        assert_eq!(super::steering_in_use(Some(&pose), &prefs), (false, false));
    }
    #[test]
    fn temp_brick_options_colour_and_flash_the_ghost() {
        let mut prefs = bri_ui::prefs::Prefs::default();
        prefs.set("$pref::HUD::tempBrickOutsideUsePaintColor", "0");
        prefs.set("$pref::HUD::tempBrickOutsideGreen", "1");
        prefs.set("$pref::HUD::tempBrickInsideUsePaintColor", "1");
        prefs.set("$pref::HUD::tempBrickFlashTime", "2000");
        let look = crate::world_scene::TempBrickLook::from_prefs(&prefs);
        assert_eq!(look.outside, Some([0.0, 1.0, 0.0]));
        assert_eq!(look.inside, None);
        assert_eq!(look.flash_ms, 2000.0);
        assert_eq!(
            crate::world_scene::TempBrickLook::from_prefs(&Default::default()),
            Default::default()
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
        let others = [weapon.clone(), actor.clone()];
        let (combined, deferred) =
            super::combine_effect_frames(world.clone(), others, &[Vec3::ZERO]);
        assert_eq!(combined.particles[0].texture, 2);
        assert_eq!(combined.particles[1].texture, 7);
        assert_eq!(combined.lights.len(), bri_render::scene::MAX_POINT_LIGHTS);
        assert_eq!(combined.lights[0].handle.0, 9000);
        assert_eq!(deferred, 1);
        // A mirror's eye far down the row keeps the lights beside it: the
        // farthest from the player is kept, the next nearest dropped.
        let mirror = Vec3::new(1000. + bri_render::scene::MAX_POINT_LIGHTS as f32, 0., 0.);
        let (combined, _) =
            super::combine_effect_frames(world, [weapon, actor], &[Vec3::ZERO, mirror]);
        let kept = |id: u64| combined.lights.iter().any(|l| l.handle.0 == id);
        assert!(kept(9000) && kept(bri_render::scene::MAX_POINT_LIGHTS as u64 - 1));
        assert!(!kept(0), "the light nearest neither eye goes");
    }
    #[test]
    fn remote_chat_cannot_inject_color_stack_or_markup() {
        assert_eq!(
            super::plain_chat("<color:ff0000>A\u{e003}B\u{e00b}C\u{e00c}\n"),
            "‹color:ff0000›ABC"
        );
    }
    #[test]
    fn server_prints_keep_ml_markup_for_the_shared_renderer() {
        let binds = bri_ui::binds::BindMap::default();
        let event = "<color:FFFFFF>It's no longer Badspot's' Birthday.<br>Attempts\u{7} ignored";
        assert_eq!(
            super::print_markup(&binds, event),
            "<color:FFFFFF>It's no longer Badspot's' Birthday.<br>Attempts ignored"
        );
        assert_eq!(
            super::print_markup(
                &binds,
                "Press \u{E003}<key:jump>\u{E000} now\n<bitmap:base/client/ui/CI/trophy>"
            ),
            "Press \u{E003}(unbound)\u{E000} now\n<bitmap:base/client/ui/CI/trophy>"
        );
    }
    #[test]
    fn chat_links_like_v20() {
        assert_eq!(
            super::player_chat(&Default::default(), "Max", "see https://blockland.us/x<y now"),
            "\u{e007}\u{e003}Max\u{e007}\u{e006}: see <a:blockland.us/xy>blockland.us/xy</a>\u{e006} now"
        );
        assert_eq!(super::linked_chat("no link <b>", '\u{e006}'), "no link ‹b›");
    }
    #[test]
    fn chat_lines_carry_v20_colors() {
        // `'\c7%1\c3%2\c7%3\c6: %4'`: the name is yellow, the text white.
        assert_eq!(
            super::player_chat(&Default::default(), "Max", "hi \u{e003}<b>"),
            "\u{e007}\u{e003}Max\u{e007}\u{e006}: hi ‹b›"
        );
        // Clan tags sit grey around the name, stripped of colour escapes.
        let clan = bri_sim::session::Clan {
            prefix: "[B\u{e003}]".into(),
            suffix: "~".into(),
        };
        assert_eq!(
            super::player_chat(&clan, "Max", "hi"),
            "\u{e007}[B]\u{e003}Max\u{e007}~\u{e006}: hi"
        );
        // Server lines keep markup and colour escapes around a death icon.
        assert_eq!(
            bri_ui::ml::sanitize("\u{e003}Max<bitmap:base/client/ui/CI/skull>\u{e000}!"),
            "\u{e003}Max<bitmap:base/client/ui/CI/skull>\u{e000}!"
        );
    }
    #[test]
    fn a_player_camera_pivots_over_the_middle_of_the_box() {
        use glam::Vec3;
        let feet = Vec3::new(3.0, 1.0, -2.0);
        // PlayerStandardArmor: feet + 2.65 / 2 + 0.75, 8 back, tilted 0.261.
        let (distance, pivot, tilt) =
            super::pivot_camera(2.65, 1.0, super::PLAYER_CAMERA, feet, 1.0);
        assert_eq!(distance, 8.0);
        assert!(pivot.distance(feet + Vec3::Y * 2.075) < 1e-5, "{pivot}");
        assert_eq!(tilt, 0.261);
        // Sliding in, the offset eases to 0.75 and the distance to nothing.
        let (distance, pivot, _) = super::pivot_camera(2.4, 1.0, (8.0, 2.3, 0.261), feet, 0.0);
        assert_eq!(distance, 0.0);
        assert!(pivot.distance(feet + Vec3::Y * 1.95) < 1e-5, "{pivot}");
    }
    /// Max, a21: riding a horse, the chase camera sat 2.3 over the horse's
    /// feet. v20's rider looks through the horse's own player camera: the
    /// middle of its 2.4 tall box plus `cameraVerticalOffset` 2.3, 8 back.
    /// Which first seats the client predicts, and when it starts again: a
    /// live vehicle a player steers or a player-type mount they control; a
    /// respawn (new id), a new definition or scale restarts it; the tumble
    /// body (no controls) and a destroyed vehicle show the host's poses.
    #[test]
    #[ignore = "requires the converted native vehicle pack; CPU only"]
    fn the_client_predicts_the_live_vehicles_and_mounts_it_controls() -> anyhow::Result<()> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/vehicles-pack-012");
        let assets = crate::vehicles::VehicleAssets::load(&root)?;
        let info = |definition: &str| bri_sim::session::VehicleInfo {
            id: 7,
            definition: definition.into(),
            color: None,
            occupants: vec![Some(1)],
            destroyed: false,
            scale: 1.0,
        };
        let target = |info: &bri_sim::session::VehicleInfo, strafe: bool| {
            super::drive_target(info, assets.definition(&info.definition).unwrap(), strafe)
        };
        for (definition, predicted) in [
            ("v20.vehicle.jeepvehicle", true),
            ("v20.vehicle.tankvehicle", true),
            ("v20.vehicle.flyingwheeledjeepvehicle", true),
            ("v20.vehicle.magiccarpetvehicle", true),
            ("v20.vehicle.skivehicle", true),
            ("v20.vehicle.horsearmor", true),
            ("v20.vehicle.rowboatarmor", true),
            ("v20.vehicle.cannonturret", true),
            ("v20.vehicle.tankturretplayer", true),
            ("v20.vehicle.deathvehicle", false),
        ] {
            for strafe in [false, true] {
                assert_eq!(
                    target(&info(definition), strafe).is_some(),
                    predicted,
                    "{definition}, strafe steering {strafe}"
                );
            }
        }
        let jeep = info("v20.vehicle.jeepvehicle");
        let base = target(&jeep, false).unwrap();
        let destroyed = bri_sim::session::VehicleInfo {
            destroyed: true,
            ..jeep.clone()
        };
        assert_eq!(target(&destroyed, false), None, "a wreck is the host's");
        for changed in [
            bri_sim::session::VehicleInfo { id: 8, ..jeep.clone() },
            bri_sim::session::VehicleInfo {
                scale: 2.0,
                ..jeep.clone()
            },
            bri_sim::session::VehicleInfo {
                definition: "v20.vehicle.tankvehicle".into(),
                ..jeep.clone()
            },
        ] {
            assert_ne!(target(&changed, false), Some(base.clone()), "{changed:?}");
        }
        Ok(())
    }
    #[test]
    #[ignore = "requires the converted native vehicle pack; CPU only"]
    fn a_horse_rider_sees_the_horse_player_camera() -> anyhow::Result<()> {
        use glam::Vec3;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/vehicles-pack-012");
        let assets = crate::vehicles::VehicleAssets::load(&root)?;
        let horse = assets.definition("v20.vehicle.horsearmor").unwrap();
        assert_eq!(
            horse.seat_role(0),
            bri_vehicles::schema::SeatRole::Actor,
            "the horse's rider takes the actor path"
        );
        let feet = Vec3::new(10.0, 4.0, -6.0);
        let (distance, pivot, tilt) = super::mount_camera(horse, feet, 1.0);
        assert_eq!(distance, 8.0);
        assert!(pivot.distance(feet + Vec3::Y * 3.5) < 1e-4, "{pivot}");
        assert!((tilt - 0.261).abs() < 1e-6);
        // The other player-type mounts use their own boxes and offsets.
        let turret = assets.definition("v20.vehicle.tankturretplayer").unwrap();
        let (_, pivot, _) = super::mount_camera(turret, feet, 1.0);
        assert!(
            pivot.distance(feet + Vec3::Y * (0.85 + 2.3)) < 1e-4,
            "{pivot}"
        );
        // The Tank's gunner looks through that turret, not the Tank.
        let tank = assets.definition("v20.vehicle.tankvehicle").unwrap();
        assert_eq!(tank.seat_role(2), bri_vehicles::schema::SeatRole::Gunner);
        let carried = assets.attachment_definition(tank).unwrap();
        assert_eq!(carried.id, "v20.vehicle.tankturretplayer");
        assert_eq!(carried.camera.max_dist, 8.0);
        Ok(())
    }
}
