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
    session::{Command, InspectMode, Reply, ToolAction},
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

mod actions;
mod addons;
mod avatars;
mod building;
mod frame;
mod fx;
mod gpu;
mod hud;
mod lighting;
mod load;
mod lobby;
mod mounts;
mod net_events;
mod perf;
mod probes;
mod render;
mod saves;
mod scene;
mod session;
mod view;
use addons::*;
use avatars::*;
use building::*;
use fx::*;
use gpu::*;
use hud::*;
use lighting::*;
use lobby::*;
use mounts::*;
use perf::*;
use saves::*;
use scene::*;
use session::*;
use view::*;
type Meshes = BTreeMap<String, bri_content::brick::Brick>;

/// Supply actual automatic bounds to the editor, so selecting Custom starts
/// from the inspected brick's current footprint rather than an invented size.
fn region_defaults(update: &mut UiUpdate, reply: &Reply, meshes: Option<&Meshes>) {
    if let UiUpdate::OpenWrench { data, .. } = update
        && let Reply::Inspected { brick, .. } = reply
        && let Some(mesh) = meshes.and_then(|meshes| crate::brick_cover::mesh(brick, meshes))
    {
        let (lo, hi) =
            bri_world::regions::bounds(None, bri_sim::definitions::brick_box(brick, mesh));
        data.rule_region_default = Some((hi - lo).to_array());
    }
}
/// One player's script-thread animations by thread number (`playThread`).
type AvatarThreads = [Option<crate::avatar::ActionAnimation>; 4];

/// Threads 0, 1 and 3 are not tied to a mounted image (an image's own
/// thread 0 names its hand): each holds until the next animation on it
/// replaces it, or `root` stops it. False for any other animation.
fn play_free_thread(
    threads: &mut BTreeMap<u64, AvatarThreads>,
    actor: u64,
    thread: u8,
    sequence: &str,
    image_hand: Option<u8>,
    started_at: f64,
) -> bool {
    if !matches!(thread, 0 | 1 | 3) || image_hand.is_some() {
        return false;
    }
    let playing = threads.entry(actor).or_default();
    playing[usize::from(thread)] =
        (!sequence.eq_ignore_ascii_case("root")).then(|| crate::avatar::ActionAnimation {
            sequence: sequence.into(),
            started_at,
        });
    if playing.iter().all(Option::is_none) {
        threads.remove(&actor);
    }
    true
}
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
        // Items first: an Add-On's particle textures are among theirs.
        let mut item_assets = crate::items::ItemAssets::load_with(
            &content.paths.item_presentation,
            &content.paths.weapons,
            &content.paths.weapon_extras,
        )?;
        item_assets.draw_icons(Some(icon_cache));
        let item_assets = Arc::new(item_assets);
        let interface =
            crate::weapon_effects::interface_textures(&weapon_pack.effects, &content.ui_pack);
        let weapon_effects = crate::weapon_effects::WeaponEffects::with_textures(
            effects_pack,
            weapon_pack.clone(),
            Default::default(),
            |key| item_assets.texture(key).or_else(|| interface.get(key)),
        )?;
        // Bodies draw from the weapons' effects, an Add-On's own among them
        // (an image it wears in the emote slot), and vehicle trails bring
        // their Add-On's particles and emitters.
        let (actor_pack, notes) = crate::actor_effects::with_vehicle_effects(
            weapon_effects.world().pack().clone(),
            &content.vehicles,
        )?;
        for note in notes {
            bri_console::warn(format!("Vehicle effects: {note}"));
        }
        let actor_effects =
            crate::actor_effects::ActorEffects::new(actor_pack, weapon_pack, Default::default())?;
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
        tool_ui.install_effects(
            content.weapons.emitter_choices.clone(),
            content.weapons.light_choices.clone(),
        )?;
        tool_ui.install_special(
            content.music.clone(),
            content
                .vehicles
                .definitions
                .iter()
                .filter(|d| d.family.spawnable())
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
fn load_visual_map(
    root: &Path,
    id: &str,
    lighting: u8,
) -> Result<bri_render::scene_loader::MapScene> {
    if lighting == 3 {
        bri_render::scene_loader::load_map_bundle_dynamic(root, id)
    } else {
        load_map_bundle(root, id)
    }
}
fn prepare_map(
    paths: &crate::content::ContentPaths,
    map: &str,
    selected: Vec<(String, u8)>,
    catalog: &bri_sim::session::ToolCatalog,
    light_cache: &std::path::Path,
    lighting: u8,
) -> Result<Prepared> {
    let map = map.to_owned();
    let visual = load_visual_map(&paths.map_bundle, &map, lighting)?;
    let mut light_volume =
        LightVolumeState::start(&visual.scene, light_cache, visual.modern_lights.as_deref());
    // The same definitions the host's session loads, Add-On bricks included.
    let definitions = Definitions::load_with_geometry(
        &paths.brick_catalog,
        &paths.geometry,
        &paths.brick_extras,
        &bri_net::content_identity::brick_geometry_assets(&paths.root, &paths.packages)?,
    )?;
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
/// Request ID for unsolicited state reports; their replies are not awaited.
const REPORT_REQUEST: RequestId = RequestId::MAX;
/// How often a moving ghost brick is reported to the server.
const GHOST_REPORT_INTERVAL: Duration = Duration::from_millis(100);

pub struct App {
    /// The connection to a game: the attempt in flight, its pending requests and what was last sent.
    net: SessionState,
    /// Building: the brick hand and ghosts, tool dialogs and build macros.
    build: BuildState,
    /// Last place of the copy in hand reported to the server, and when.
    copy_report: Option<(
        Option<(u64, bri_sim::session::CopyPose)>,
        std::time::Instant,
    )>,
    pub(crate) item_assets: Arc<crate::items::ItemAssets>,
    item_ui: crate::item_ui::ItemUi,
    world_items: crate::world_items::WorldItems,
    foliage: crate::foliage::ClientFoliage,
    weather: crate::weather::ClientWeather,
    /// Everything uploaded to the graphics card, and the renderers that draw it.
    gpu: GpuState,
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
    /// The world as the CPU sees it: map scene, brick chunks, the world log and the query mirror.
    scene: SceneState,
    /// What the camera shows beyond the controls: observer and rendered eyes, the drawn controls, crosshair and wheels.
    view: ViewState,
    /// Presentation effects: weapon, actor and world effects, debris, fades and the cue queues feeding them.
    fx: Effects,
    /// Presentation faults absorbed instead of closing the game.
    pub cosmetic_faults: crate::cosmetic::CosmeticFaults,
    /// Add-On packages: the catalog, client code, server packages and imports.
    addons: AddOns,
    /// Add-On world shapes (`show_shapes`), and the sets last uploaded.
    world_shapes: Option<bri_render::world_shapes::ShapeRenderer>,
    shapes_uploaded:
        Option<BTreeMap<String, std::sync::Arc<Vec<bri_package_runtime::ops::WorldShape>>>>,
    /// View kick: hitscan shots seen this frame (actor, hand), and the
    /// newest projectile id the kick has looked at (None before the first
    /// view, so a join does not kick).
    shot_kicks: Vec<(u64, u8)>,
    kick_seen: Option<u64>,
    /// Map lighting: the baked light volume, reflections and the environment probe.
    lighting: Lighting,
    graphics: crate::graphics::Graphics,
    load_limit: Arc<tokio::sync::Semaphore>,
    /// Host on a port the system picks instead of `$Pref::Server::Port`
    /// ([`App::host_on_any_port`]).
    host_any_port: bool,
    /// Avatar bodies, their actions and gestures, and the avatar screen's preview.
    avatar: Avatars,
    /// Saves and their pictures, file jobs, old saves and colour-set loads.
    files: Saves,
    /// Whether today's Add-On splash has been looked for (once a run).
    splash_checked: bool,
    motion: crate::motion::Motion,
    /// Projectiles, drops and package entities smoothed between host updates.
    ghosts: crate::ghosts::Ghosts,
    /// Shots drawn from the shooter's muzzle (`crate::shot_origins`).
    shot_origins: crate::shot_origins::ShotOrigins,
    vehicle_assets: crate::vehicles::VehicleAssets,
    vehicles: crate::vehicles::ClientVehicles,
    /// Seats and riders: what the local player sits on and how riders are posed.
    mounts: Mounts,
    music_world: Option<Arc<bri_net::protocol::PublicWorld>>,
    /// Performance: frame stats and log, network sampling, lag watch and quality.
    perf: Perf,
    /// Finding games: LAN hosts and queries, the update check and the firewall fix.
    lobby: Lobby,
    /// Add-On problems reported while the content last loaded.
    content_problems: Vec<bri_package::health::Problem>,
    combat: CombatPresentation,
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
    fn rebuild_effects_renderer(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Result<()> {
        let limits = bri_fx_runtime::EffectsLimits::default();
        self.gpu.effects_renderer = Some(bri_fx_runtime::gpu::EffectsRenderer::new(
            device,
            queue,
            self.fx.weapon_effects.world().pack(),
            format,
            bri_render::scene::DEPTH_FORMAT,
            self.graphics.samples,
            limits.particles.saturating_mul(2) + limits.lights.saturating_mul(2),
        )?);
        Ok(())
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
/// shape's name distance (8192 unless `setShapeNameDistance`; a
/// mini-game's rules set it for its members, Slayer's Name Distance): names
/// show out to `min(nameDistance, visibleDistance)` and fade from
/// `min(fogDistance, max(0.8 × nameDistance, nameDistance - 5))`. None past
/// the far end.
pub fn name_opacity(
    distance: f32,
    name_distance: f32,
    fog_distance: f32,
    visible_distance: f32,
) -> Option<f32> {
    let far = name_distance.min(visible_distance);
    let fade = fog_distance
        .min((name_distance * 0.8).max(name_distance - 5.0))
        .min(far);
    if distance <= 0.0 || distance > far {
        return None;
    }
    Some(if distance < fade || far <= fade {
        1.0
    } else {
        1.0 - (distance - fade) / (far - fade)
    })
}
/// `GuiShapeNameHud::onRender`: every other living player's name above their
/// eye point (`verticalOffset` 0.85), and any named dropped item's, hidden behind the map and raycasting
/// bricks ([`crate::building::Building::name_visible`]), faded by
/// [`name_opacity`] and drawn in the mini-game colour a member's player is
/// given at spawn (`GameConnection::createPlayer`), or their team's (Slayer's
/// `setShapeNameColor`), white otherwise. Items with a `label` (v20's
/// `setShapeName` on an item: an ammo box's count) show it the same way, in
/// white, above where they lie.
#[allow(clippy::too_many_arguments)]
fn name_tags(
    view: &network::View,
    presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
    pack: &bri_weapons::Pack,
    building: Option<&crate::building::Building>,
    view_projection: glam::Mat4,
    camera: Vec3,
    (fog_distance, visible_distance): (f32, f32),
    size: (f32, f32),
    scale: f32,
    controlling_body: bool,
    passages: &bri_content::passage::Passages,
    drop_center: impl Fn(&bri_weapons::Drop) -> Vec3,
) -> Vec<bri_ui::api::NameTag> {
    const VERTICAL_OFFSET: f32 = 0.85;
    let sees =
        |from: Vec3, to: Vec3| building.is_none_or(|b| b.name_visible(from, to).unwrap_or(true));
    // Where a name anchored at `target` goes on screen, and how strongly:
    // where its body shows, straight on or in a portal's view, never
    // through a portal's view of somewhere else.
    let place = |target: Vec3, name_distance: f32| -> Option<(f32, f32, f32)> {
        let seen = crate::portal_view::seen_at(passages, camera, target)
            .into_iter()
            .find(|s| match s.through {
                None => sees(camera, target),
                Some((near, far)) => sees(camera, near) && sees(far, target),
            })?;
        let target = seen.at;
        let opacity = name_opacity(
            target.distance(camera),
            name_distance,
            fog_distance,
            visible_distance,
        )?;
        let clip = view_projection * (target + Vec3::Y * VERTICAL_OFFSET).extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 {
            return None;
        }
        Some((
            (ndc.x + 1.0) * 0.5 * size.0 / scale,
            (1.0 - ndc.y) * 0.5 * size.1 / scale,
            opacity,
        ))
    };
    let paint = |color: u8| {
        let rgba = view.world.palette.get(usize::from(color))?;
        Some([0, 1, 2].map(|i| (rgba[i].clamp(0.0, 1.0) * 255.0).round() as u8))
    };
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
        let game = view.minigames.iter().find(|m| m.members.contains(owner));
        let name_distance = game
            .and_then(|m| m.name_distance)
            .map_or(8192.0, |d| d as f32);
        let Some((x, y, opacity)) = place(view.archetypes.eye(state), name_distance) else {
            continue;
        };
        // A team member's name is in their team's paint colour.
        let team = view
            .vitals
            .get(owner)
            .and_then(|v| v.team)
            .and_then(|team| paint(game?.teams.iter().find(|t| t.id.0 == team)?.color));
        // A game's own paint colour (Slayer's Color) over its v20 one.
        let color = team
            .or_else(|| game.and_then(|m| m.paint_color).and_then(paint))
            .or_else(|| game.and_then(|m| crate::minigame_ui::color_rgb(m.color)))
            .unwrap_or([255; 3]);
        tags.push(bri_ui::api::NameTag {
            x,
            y,
            text: plain_chat(name),
            opacity,
            color,
        });
    }
    // An item with a `label` (v20's `setShapeName` on an item: an ammo
    // box's count) shows it in white above where it lies.
    let lying = view
        .weapons
        .static_items
        .iter()
        .map(|i| (i.item.as_str(), Vec3::from(i.position)))
        .chain(
            view.weapons
                .drops
                .iter()
                .map(|d| (d.item.as_str(), d.position)),
        );
    for (item, at) in lying {
        let Some(label) = pack
            .items
            .get(item)
            .map(|i| &i.label)
            .filter(|l| !l.is_empty())
        else {
            continue;
        };
        let Some((x, y, opacity)) = place(at, 8192.0) else {
            continue;
        };
        tags.push(bri_ui::api::NameTag {
            x,
            y,
            text: label.clone(),
            opacity,
            color: [255; 3],
        });
    }
    // Add-On world shapes' labels, over each shape's top centre in its
    // colour.
    for shape in view.world_shapes.values().flat_map(|s| s.iter()) {
        if shape.label.is_empty() {
            continue;
        }
        let (min, max) = (Vec3::from(shape.min), Vec3::from(shape.max));
        let top = Vec3::new((min.x + max.x) / 2.0, max.y, (min.z + max.z) / 2.0);
        let Some((x, y, opacity)) = place(top, 8192.0) else {
            continue;
        };
        tags.push(bri_ui::api::NameTag {
            x,
            y,
            text: shape.label.clone(),
            opacity,
            color: [shape.color[0], shape.color[1], shape.color[2]],
        });
    }
    // Any other shape's name sits above the middle of its box
    // (`getBoxCenter`): a dropped flag's countdown in its team's colour.
    for drop in &view.weapons.drops {
        let Some(name) = &drop.name else {
            continue;
        };
        let Some((x, y, opacity)) = place(drop_center(drop), 8192.0) else {
            continue;
        };
        tags.push(bri_ui::api::NameTag {
            x,
            y,
            text: plain_chat(&name.text),
            opacity,
            color: paint(name.color).unwrap_or([255; 3]),
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
/// Only portal-adjacent segments need the walking mirror's clipped geometry.
/// Ordinary player and driver booms retain the cheap indexed camera query.
fn camera_segment_near_portal(
    passages: &bri_content::passage::Passages,
    from: Vec3,
    to: Vec3,
) -> bool {
    passages.first(from, to).is_some()
        || passages.near(from, 0.2).any(|p| p.within(from, 0.15))
        || passages
            .closed
            .iter()
            .any(|p| p.crossing(from, to).is_some())
}

#[allow(clippy::too_many_arguments)]
fn camera_eye(
    controls: &Controls,
    presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
    entities: &BTreeMap<u64, bri_sim::session::EntityInfo>,
    drawn_offset: Option<Vec3>,
    building: &crate::building::Building,
    collision: Option<&bri_sim::prediction::CollisionMirror>,
    own_eye: Vec3,
    forward: Vec3,
    chase: Option<(Vec3, f32)>,
    passages: &bri_content::passage::Passages,
) -> Result<(Vec3, Option<glam::Affine3A>)> {
    use crate::controls::ObserverMode;
    let boom = |from, pivot, distance| {
        crate::portal_view::boom(
            from,
            pivot,
            forward,
            distance,
            passages,
            |eye, forward, d| {
                // Use the walking mirror near openings: it cuts the backing wall
                // and sees the destination geometry. Keep the ordinary indexed
                // sweep elsewhere so a portal never adds work to unrelated views.
                let end = eye - forward.normalize() * d;
                let nearby = camera_segment_near_portal(passages, eye, end);
                match collision.filter(|_| nearby) {
                    Some(mirror) => mirror.portal_camera_position(eye, forward, d),
                    None => building.camera_position(eye, forward, d),
                }
            },
        )
    };
    match controls.observer().map(|o| o.mode) {
        Some(ObserverMode::Free(position) | ObserverMode::Path(position)) => Ok((position, None)),
        // `setOrbitMode(target, ..., 0, 8, 8)` from `Observer::setMode("Corpse")`,
        // or an Add-On's own distance. Its boom goes back through a portal
        // behind the focus, as a chase camera's does.
        Some(ObserverMode::Orbit(_) | ObserverMode::Drive(_) | ObserverMode::Point(..)) => {
            let focus = controls
                .orbit_focus(presented, building.archetypes(), entities)
                .map(|focus| focus + drawn_offset.unwrap_or(Vec3::ZERO))
                .unwrap_or(own_eye);
            boom(focus, focus, controls.orbit_distance())
        }
        None => match chase {
            // A chase camera's boom from `own_eye`, its pivot, which rides
            // on the body at `from`.
            Some((from, distance)) => boom(from, own_eye, distance),
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
/// The plant-error icon for why a plant, or an Add-On's
/// `MsgPlantError_…`, was refused.
fn plant_error(failure: bri_sim::simulation::PlantFailure) -> PlantError {
    use bri_sim::simulation::PlantFailure as F;
    match failure {
        F::Overlap => PlantError::Overlap,
        F::Float => PlantError::Float,
        F::Buried => PlantError::Buried,
        F::Stuck => PlantError::Stuck,
        F::TooFar => PlantError::TooFar,
        F::Forbidden => PlantError::Forbidden,
        F::Limit => PlantError::Limit,
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
/// A score report as the Report window shows it: plain text, each column
/// in the host's order, team names in their paint.
fn report_view(
    report: &bri_package_runtime::report::Report,
    palette: &[[f32; 4]],
) -> bri_ui::api::ReportView {
    use bri_ui::api::{ReportRowView, ReportSectionView, ReportView};
    ReportView {
        title: plain_chat(&report.title),
        banner: report.banner.as_deref().map(plain_chat),
        columns: report
            .columns
            .iter()
            .map(|c| plain_chat(&c.title))
            .collect(),
        sections: report
            .sections
            .iter()
            .map(|s| ReportSectionView {
                title: plain_chat(&s.title),
                rows: s
                    .rows
                    .iter()
                    .map(|r| ReportRowView {
                        name: plain_chat(&r.name),
                        color: r
                            .color
                            .and_then(|c| palette.get(usize::from(c)))
                            .map(|c| bri_ui::geom::from_f32([c[0], c[1], c[2], 1.0])),
                        cells: report
                            .columns
                            .iter()
                            .map(|c| {
                                r.cells
                                    .get(&c.key)
                                    .map_or_else(String::new, |v| plain_chat(v))
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
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
    let (vehicle, seat) = mounted?;
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
            | UiAction::SendFillWrench { .. }
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
/// Where the local player hears from: their presented feet in the game
/// `view` shows, None outside a game.
fn listener(motion: &crate::motion::Motion, view: Option<&network::View>) -> Option<Vec3> {
    let owner = view?.owner;
    motion.presented().get(&owner).map(|p| Vec3::from(p.feet))
}

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
use crate::motion::DriveTarget;
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

type LightVolumeReceiver = std::sync::mpsc::Receiver<Baked>;
/// Breakable map shapes that are lights (v20 `Glass` datablocks): the
/// Bedroom lamp's bulb and the Kitchen's fluorescent tubes.
const LIGHT_SHAPES: &[&str] = &["lightBulbA", "fluorescentLight"];

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
        if focused
            && (self.ui.is_open(bri_ui::screens::ScreenId::StartMission)
                || self.ui.is_open(bri_ui::screens::ScreenId::HostColorsets))
        {
            self.ui
                .apply(UiUpdate::HostColorsets(crate::colorsets::catalog(
                    &self.content.paths.root,
                    &self.state_dir,
                    &self.content.paint,
                )));
        }
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
        self.frame(elapsed)
    }
    fn pump(&mut self) -> Result<Vec<PlatformCommand>> {
        self.dispatch()
    }
    fn gpu_ready(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Result<()> {
        if self.perf.auto_quality {
            self.perf.auto_quality = false;
            self.pick_quality(&device.adapter_info());
        }
        self.gpu.gpu_name = device.adapter_info().name;
        self.item_ui.gpu_stopped();
        self.world_items.clear_gpu();
        crate::vehicles::ClientVehicles::gpu_stopped(&mut self.vehicle_assets);
        self.fx.explosion_shapes.gpu_stopped();
        self.fx.beams.gpu_stopped();
        self.fx.tutorial_targets.gpu_stopped();
        self.gpu.shell_gpu = None;
        for avatar in self.avatar.avatars.values_mut() {
            avatar.gpu = None;
            avatar.instance = None;
        }
        // The avatar preview and the world compile their pipelines on
        // worker threads; the menus draw meanwhile (see gpu_build).
        let preview_device = device.clone();
        self.avatar.avatar_preview = Some(crate::gpu_build::Building::spawn(
            "avatar preview pipelines",
            move || crate::avatar::Preview::new(&preview_device),
        ));
        self.avatar.preview_dirty = self.avatar.preview_request.is_some();
        bri_render::color::set_color_vision(bri_ui::screens::options::color_vision(
            &self.ui.core.prefs,
        ));
        let samples = self.graphics.samples;
        let effective = self
            .graphics
            .with_lighting(self.lighting.light_volume.mode(self.graphics.lighting));
        let (scene_device, shadows) = (device.clone(), effective.shadows);
        self.gpu.renderer = Some(crate::gpu_build::Building::spawn(
            "scene pipelines",
            move || SceneRenderer::with_settings(&scene_device, format, samples, shadows),
        ));
        self.lighting.reflections = Some(bri_render::reflection::Reflections::new(
            device,
            format,
            samples,
            self.graphics.reflections,
        ));
        self.foliage.gpu_stopped();
        self.foliage.set_samples(samples);
        self.addons.client_code.gpu_stopped();
        self.addons.item_skins.gpu_stopped();
        let weather_limits = bri_weather::WeatherLimits::default();
        self.gpu.weather_renderer = Some(bri_weather::gpu::WeatherRenderer::new(
            device,
            queue,
            self.weather.world.pack(),
            format,
            bri_render::scene::DEPTH_FORMAT,
            samples,
            weather_limits.drops + weather_limits.splashes,
        )?);
        self.gpu.hidden_lines = Some(bri_render::lines::LineRenderer::new(
            device,
            format,
            bri_render::scene::DEPTH_FORMAT,
            samples,
        ));
        self.gpu.vignette = Some(bri_render::vignette::VignetteRenderer::new(
            device,
            format,
            bri_render::scene::DEPTH_FORMAT,
            samples,
        ));
        self.gpu.region_lines = Some(bri_render::lines::LineRenderer::new(
            device,
            format,
            bri_render::scene::DEPTH_FORMAT,
            samples,
        ));
        self.gpu.region_outlines.clear();
        self.gpu.selection_lines = Some(bri_render::lines::LineRenderer::new(
            device,
            format,
            bri_render::scene::DEPTH_FORMAT,
            samples,
        ));
        self.gpu.selection_uploaded = None;
        self.world_shapes = Some(bri_render::world_shapes::ShapeRenderer::new(
            device,
            format,
            bri_render::scene::DEPTH_FORMAT,
            samples,
        ));
        self.shapes_uploaded = None;
        self.gpu.hidden_uploaded = None;
        self.rebuild_effects_renderer(device, queue, format)?;
        self.gpu.gpu_scene = None;
        self.gpu.gpu_terrain.clear();
        self.gpu.gpu_palette = None;
        self.gpu.gpu_chunks.clear();
        self.gpu.ghost_gpu = None;
        self.gpu.ghost_look = None;
        self.gpu.ghost_uploaded = u64::MAX;
        self.build.remote_ghosts.clear();
        self.fx.debris_models.clear();
        self.fx.fade_models.clear();
        self.addons.package_models.clear();
        if let Some(lines) = &mut self.gpu.hidden_lines {
            lines.clear();
        }
        self.gpu.hidden_uploaded = None;
        if let Some(lines) = &mut self.gpu.selection_lines {
            lines.clear();
        }
        self.gpu.selection_uploaded = None;
        if let Some(shapes) = &mut self.world_shapes {
            shapes.clear();
        }
        self.shapes_uploaded = None;
        self.gpu.depth = None;
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
        self.addons.client_code.device_lost();
    }
    fn gpu_stopped(&mut self) {
        self.addons.client_code.gpu_stopped();
        self.addons.item_skins.gpu_stopped();
        self.item_ui.gpu_stopped();
        self.world_items.clear_gpu();
        crate::vehicles::ClientVehicles::gpu_stopped(&mut self.vehicle_assets);
        self.fx.explosion_shapes.gpu_stopped();
        self.fx.beams.gpu_stopped();
        self.fx.tutorial_targets.gpu_stopped();
        self.gpu.shell_gpu = None;
        for avatar in self.avatar.avatars.values_mut() {
            avatar.gpu = None;
            avatar.instance = None;
        }
        self.avatar.avatar_preview = None;
        self.gpu.renderer = None;
        self.lighting.reflections = None;
        self.lighting.environment_probe = None;
        self.foliage.gpu_stopped();
        self.gpu.weather_renderer = None;
        self.gpu.effects_renderer = None;
        self.gpu.hidden_lines = None;
        self.gpu.selection_lines = None;
        self.gpu.region_lines = None;
        self.gpu.region_outlines.clear();
        self.world_shapes = None;
        self.gpu.gpu_scene = None;
        self.gpu.gpu_terrain.clear();
        self.gpu.gpu_palette = None;
        self.gpu.gpu_chunks.clear();
        self.gpu.ghost_gpu = None;
        self.gpu.ghost_look = None;
        self.gpu.ghost_uploaded = u64::MAX;
        self.build.remote_ghosts.clear();
        self.fx.debris_models.clear();
        self.fx.fade_models.clear();
        self.addons.package_models.clear();
        if let Some(lines) = &mut self.gpu.hidden_lines {
            lines.clear();
        }
        self.gpu.hidden_uploaded = None;
        if let Some(lines) = &mut self.gpu.selection_lines {
            lines.clear();
        }
        self.gpu.selection_uploaded = None;
        if let Some(shapes) = &mut self.world_shapes {
            shapes.clear();
        }
        self.shapes_uploaded = None;
        self.gpu.depth = None;
    }
    fn render_scene(&mut self, frame: &mut RenderContext<'_>) -> Result<bool> {
        self.render_frame(frame)
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

/// A small JSON file in the client state folder (saved servers, host
/// pins), or None when missing. One that cannot be read as what it holds
/// is moved aside as `<name>.damaged-<unix seconds>.json` (as damaged
/// settings are kept) before anything writes a new one, and the player is
/// told ([`take_damaged_files`]): favourites and host pins are never wiped
/// without a copy.
pub(crate) fn read_small_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = match std::fs::metadata(path) {
        Ok(meta) if meta.len() <= 1024 * 1024 => std::fs::read(path),
        Ok(_) => Err(std::io::Error::other("it is too big")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => Err(error),
    };
    let error = match bytes.map(|bytes| serde_json::from_slice(&bytes)) {
        Ok(Ok(value)) => return Some(value),
        Ok(Err(error)) => error.to_string(),
        Err(error) => error.to_string(),
    };
    let copy = crate::settings::damaged_copy(path);
    let moved = std::fs::rename(path, &copy);
    bri_console::warn(format!(
        "{} could not be read ({error}){}",
        path.display(),
        if moved.is_ok() {
            format!("; moved to {}", copy.display())
        } else {
            String::new()
        }
    ));
    // Not moved (in use, no permission): reading it again tries again.
    if moved.is_ok() {
        DAMAGED_FILES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((path.to_path_buf(), copy));
    }
    None
}

/// State files [`read_small_json`] found damaged and moved aside since
/// last asked: (file, where its old contents are now).
static DAMAGED_FILES: std::sync::Mutex<Vec<(PathBuf, PathBuf)>> = std::sync::Mutex::new(Vec::new());
pub(crate) fn take_damaged_files() -> Vec<(PathBuf, PathBuf)> {
    std::mem::take(&mut *DAMAGED_FILES.lock().unwrap_or_else(|e| e.into_inner()))
}
/// What the player is told about a damaged state file.
fn damaged_file_message(file: &Path, copy: &Path) -> String {
    let what = match file.file_name().and_then(|n| n.to_str()) {
        Some("servers.json") => "Your saved and favourite servers",
        Some("trusted-hosts.json") => "The servers you trusted",
        _ => "A saved list",
    };
    format!(
        "{what} could not be read, so the list starts empty. The old file was kept as {}.",
        copy.display()
    )
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
/// The opening `owner`'s body is part way through, if any, so it and what
/// it holds draw on both sides. Riders are cut where their vehicle is.
fn body_straddle(
    vehicles: &crate::vehicles::ClientVehicles,
    view: &network::View,
    passages: &bri_content::passage::Passages,
    owner: u64,
    avatar: &crate::avatar::AvatarMesh,
) -> Option<crate::portal_view::Straddle> {
    match view
        .vehicles
        .iter()
        .find(|(_, info)| info.occupants.iter().flatten().any(|o| *o == owner))
    {
        Some((vehicle, _)) => vehicles.straddle(*vehicle).copied(),
        None => crate::portal_view::Straddle::find(
            passages,
            avatar.middle(),
            avatar.bounding_sphere().1,
        ),
    }
}

impl App {
    /// Tell the player about state files found damaged (kept aside).
    pub(super) fn show_damaged_files(&mut self) {
        for (file, copy) in take_damaged_files() {
            self.ui.apply(UiUpdate::MessageBox {
                title: "Saved List Problem".into(),
                text: damaged_file_message(&file, &copy),
            });
        }
    }
}

#[cfg(test)]
mod tests;
