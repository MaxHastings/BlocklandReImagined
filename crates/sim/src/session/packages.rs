//! Package-defined gameplay inside the session.
//!
//! The session knows nothing about what a package's game is. It offers the
//! seams in `bri-package-runtime` and applies what package scripts ask for:
//!
//! - **commands** clients send by name, checked against the declaring
//!   package's behaviour (arguments, cooldown, administrator-only);
//! - **state** per durable player and per server, committed only when a
//!   script call succeeds, replicated when public and saved by the host;
//! - **entities** with character bodies, driven by the package's think
//!   function, drawn by clients from a package model id;
//! - a **chunked world** whose chunks the package generates around players;
//!   the engine inserts, streams and saves the edits (removed voxels);
//! - **operations**, each passing `ops::authorize` (the one capability gate)
//!   and then an ownership check here.
use super::*;
use bri_package_runtime::{
    Catalog, Diagnostic, Dynamic, PlayerKey, Store,
    content::{ArgType, ChunkWorld, Visible},
    ops::{ObjectRef, Op, authorize},
    script::{self, Budget, Call, EntityView, PlayerView, Runtime, Snapshot},
    state::{self, Namespace},
};
use bri_world::MAX_BRICKS;
use std::sync::Arc;

mod item_hooks;
pub(super) use item_hooks::Pickup;

/// Collider tag kind for package entities (players are 1, vehicles 2).
pub const ENTITY_TAG: u128 = 3 << 64;
/// Chunks generated per tick while players explore.
const CHUNKS_PER_TICK: usize = 1;
/// Package diagnostics kept for `package_diagnostics`.
const MAX_DIAGNOSTICS: usize = 256;
/// Most bricks one explosion destroys.
const MAX_BLAST_BRICKS: usize = 256;
/// Entities below this height fell out of the world.
const KILL_Y: f32 = -64.0;

/// A client's request to run a package command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageCommand {
    /// The package whose command this is. Empty for a command typed in chat,
    /// which the host resolves to the one package declaring it.
    pub package: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<PackageArg>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum PackageArg {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
}
impl PackageArg {
    fn matches(&self, expected: ArgType) -> bool {
        matches!(
            (self, expected),
            (Self::Int(_), ArgType::Int)
                | (Self::Float(_), ArgType::Float)
                | (Self::String(_), ArgType::String)
                | (Self::Bool(_), ArgType::Bool)
        )
    }
    fn dynamic(&self) -> Dynamic {
        match self {
            Self::Int(v) => Dynamic::from_int(*v),
            Self::Float(v) => Dynamic::from_float(*v),
            Self::String(v) => v.clone().into(),
            Self::Bool(v) => (*v).into(),
        }
    }
}

/// A package entity as clients see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityInfo {
    pub id: u64,
    pub kind: String,
    /// Model id the client draws (`namespace:model/name`).
    pub model: String,
    pub position: [f32; 3],
    pub yaw: f32,
    /// Size relative to a player: its kind's `scale`, which clients apply
    /// to the model too.
    pub scale: f32,
    pub label: String,
}
impl EntityInfo {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.kind.len() <= 128
                && self.model.len() <= 128
                && self.label.len() <= 32
                && self.position.iter().all(|v| v.is_finite())
                && self.yaw.is_finite()
                && self.scale.is_finite()
                && (0.2..=4.0).contains(&self.scale),
            "Invalid package entity"
        );
        Ok(())
    }
}
/// Public package state as clients see it: per package, server-wide keys
/// and per connected player keys.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PackageStateView {
    pub packages: BTreeMap<String, NamespaceView>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NamespaceView {
    pub global: BTreeMap<String, serde_json::Value>,
    pub players: BTreeMap<OwnerId, BTreeMap<String, serde_json::Value>>,
}
impl PackageStateView {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.packages.len() <= 256, "Too many package namespaces");
        for ns in self.packages.values() {
            ensure!(
                ns.players.len() <= 64 && ns.global.len() <= 256,
                "Oversized package state"
            );
            for v in ns
                .global
                .values()
                .chain(ns.players.values().flat_map(|m| m.values()))
            {
                bri_package_runtime::state::check_value(v)?;
            }
        }
        Ok(())
    }
    /// A bound value: `package:player/key` for `viewer`, or `package:global/key`.
    pub fn get(
        &self,
        binding: &bri_package_runtime::content::Binding,
        viewer: OwnerId,
    ) -> Option<&serde_json::Value> {
        use bri_package_runtime::content::Scope;
        let ns = self.packages.get(&binding.package)?;
        match binding.scope {
            Scope::Player => ns.players.get(&viewer)?.get(&binding.key),
            Scope::Global => ns.global.get(&binding.key),
            Scope::Players => None,
        }
    }
    /// Every player's value of a `package:players/key` binding, by player.
    pub fn rows(
        &self,
        binding: &bri_package_runtime::content::Binding,
    ) -> Vec<(OwnerId, &serde_json::Value)> {
        self.packages
            .get(&binding.package)
            .map(|ns| {
                ns.players
                    .iter()
                    .filter_map(|(owner, values)| Some((*owner, values.get(&binding.key)?)))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// What a host saves to resume a package world: durable package state and
/// the generated world's seed and edits.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PackageSave {
    pub schema_version: u32,
    pub store: Store,
    pub world: Option<WorldSave>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldSave {
    pub provider: String,
    pub seed: i64,
    /// Voxels removed from their generated chunks.
    pub removed: BTreeSet<[i64; 3]>,
    /// Voxels placed since (`place_voxel`), by material id.
    #[serde(default)]
    pub added: BTreeMap<String, Vec<[i64; 3]>>,
}
pub const PACKAGE_SAVE_SCHEMA: u32 = 1;
impl PackageSave {
    pub fn encode(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= 256 * 1024 * 1024,
            "Package save is too large"
        );
        let save: Self = serde_json::from_slice(bytes).context("Package save is damaged")?;
        ensure!(
            save.schema_version == PACKAGE_SAVE_SCHEMA,
            "Unsupported package save schema"
        );
        ensure!(
            save.world.as_ref().is_none_or(|w| {
                w.removed.len() <= MAX_BRICKS * 4
                    && w.added.values().map(Vec::len).sum::<usize>() <= MAX_BRICKS
            }),
            "Oversized world edits"
        );
        // Saved values meet the same limits as values a script can commit.
        for ns in save.store.namespaces.values() {
            for value in ns
                .global
                .values()
                .chain(ns.players.values().flat_map(|m| m.values()))
            {
                bri_package_runtime::state::check_value(value)
                    .map_err(|e| anyhow::anyhow!("Package save is damaged: {e}"))?;
            }
        }
        Ok(save)
    }
}

pub(super) struct Entity {
    kind: String,
    package: String,
    model: String,
    pub(super) body: Player,
    /// The kind's `scale`, for clients to draw its model at.
    scale: f32,
    health: f32,
    label: String,
    steer: (Vec3, bool),
    vars: BTreeMap<String, serde_json::Value>,
    think: String,
    interval: u64,
    speed: f32,
    next_think: u64,
    /// The latest move of the player driving this entity (`control`), in
    /// place of its think's steering until released.
    drive: Option<MoveInput>,
}

struct Voxel {
    position: [i64; 3],
    material: usize,
}
struct GeneratedWorld {
    provider: String,
    package: String,
    def: ChunkWorld,
    seed: i64,
    chunks: BTreeSet<(i64, i64)>,
    voxels: BTreeMap<BrickId, Voxel>,
    removed: BTreeSet<[i64; 3]>,
    /// Voxels placed by operations, with their material: generated with
    /// their chunk after the generator's own.
    added: BTreeMap<[i64; 3], usize>,
    /// Palette index of each material.
    colors: Vec<u8>,
}
impl GeneratedWorld {
    fn chunk_of(&self, position: Vec3) -> (i64, i64) {
        let size = self.def.chunk_size();
        (
            (position.x / size).floor() as i64,
            (position.z / size).floor() as i64,
        )
    }
    fn in_bounds(&self, chunk: (i64, i64)) -> bool {
        let r = i64::from(self.def.radius_chunks);
        chunk.0.abs() < r && chunk.1.abs() < r
    }
    fn center(&self, v: [i64; 3]) -> [f32; 3] {
        let s = self.def.voxel_size;
        v.map(|c| c as f32 * s + s * 0.5)
    }
    /// The world-owned brick drawing voxel `position` of `material`.
    fn brick(&self, position: [i64; 3], material: usize) -> Brick {
        let mut brick = Brick::new(
            ContentRef::Resolved(self.def.voxel_brick.clone()),
            self.center(position),
            0,
        );
        brick.color = self.colors[material];
        brick.look = self.def.materials[material]
            .block
            .clone()
            .map(|block| bri_world::BlockLook {
                block,
                state: String::new(),
            });
        brick
    }
    fn chunk_of_voxel(&self, [x, _, z]: [i64; 3]) -> (i64, i64) {
        let n = i64::from(self.def.chunk_voxels);
        (x.div_euclid(n), z.div_euclid(n))
    }
    fn material(&self, id: &str) -> Option<usize> {
        self.def.materials.iter().position(|m| m.id == id)
    }
}

pub(super) struct PackageHost {
    pub(super) catalog: Arc<Catalog>,
    runtime: Runtime,
    store: Store,
    world: Option<GeneratedWorld>,
    pub(super) entities: BTreeMap<u64, Entity>,
    next_entity: u64,
    /// Keyed by the durable player, so reconnecting does not reset a
    /// cooldown (stress campaign W3).
    cooldowns: BTreeMap<(PlayerKey, String, String), u64>,
    diagnostics: VecDeque<Diagnostic>,
    output: VecDeque<String>,
    /// Deaths since the last tick, for `on_death` hooks: victim, killer.
    deaths: VecDeque<(OwnerId, Option<OwnerId>)>,
    /// Players whose items were set afresh since the last tick, for
    /// `on_loadout` hooks.
    loadouts: VecDeque<OwnerId>,
    /// Players who came to life since the last tick, for `on_spawn` hooks.
    spawns: VecDeque<OwnerId>,
    /// An `on_damage` hook is running: damage it causes is not filtered
    /// again, so a hook can never recurse.
    in_damage_hook: bool,
    /// Pending `on_projectile_hit` calls and what dropped items carry.
    item_hooks: item_hooks::ItemHooks,
    /// Per-origin shares of the server's package capacity (stress campaign
    /// W1): no one package, or one player's commands, can take a pool
    /// every player needs.
    shares: Shares,
    /// Package state as [`state::stored_size`] counts it, kept within
    /// [`state::MAX_STATE_BYTES`] so any admitted state can be saved (W7).
    state_bytes: usize,
    /// `on_tick` hooks that are due but wait for their package's share.
    hooks_due: BTreeSet<String>,
    /// Hooks paused after failing, until the given tick.
    hooks_paused: BTreeMap<String, u64>,
    /// While the engine runs a tick's package work, every call shares one
    /// view of the world taken at its start, so a call costs what it does,
    /// not the size of the world (stress campaign W12).
    view: Option<TickView>,
    /// Wall-clock time each package's script calls took since the host last
    /// took them (the performance overlay's per-Add-On script time).
    script_time: BTreeMap<String, std::time::Duration>,
}

struct TickView {
    snapshot: Arc<Snapshot>,
    vars: BTreeMap<String, Arc<script::EntityVars>>,
}

/// Script operations per tick the server gives package work it runs itself
/// (thinks, `on_tick`, generation), split evenly between packages with
/// scripts. About 8 ms of script time on a desktop (one 120 Hz tick); a
/// slow machine takes two or three times that, still under the six-tick
/// stall bound. A single call may run past it (a `Tick` or `Generate` call
/// has 400k operations); the package then repays the debt over later ticks.
const SERVER_WORK_PER_TICK: i64 = 200_000;
/// Most script operations one player's commands may use in a burst (one
/// full command), and what refills every second.
const PLAYER_COMMAND_BURST: i64 = 200_000;
const PLAYER_COMMAND_WORK: i64 = 400_000;
/// Bricks one package may place or destroy in a burst; refills every second.
const PACKAGE_WORLD_EDITS: i64 = 2048;
/// Chat lines (broadcasts and tells) per package and calling player in a
/// burst; refills every second, like player chat.
const PACKAGE_CHAT_LINES: i64 = 8;
/// Prints and sounds per package in a burst; refills every second. A print
/// to everyone counts once.
const PACKAGE_CUES: i64 = 64;
/// Projectiles per package in a burst (`fire`); refills every second.
const PACKAGE_SHOTS: i64 = 240;
/// Environment changes per package in a burst (`set_environment`); refills
/// every second. Each is sent to every player; a moving sun is a day cycle,
/// which costs nothing to keep turning.
const PACKAGE_ENVIRONMENT_CHANGES: i64 = 8;
/// The shooter a package's own `fire` names: nobody's shot, which hurts
/// any living player and credits no one.
pub(super) const PACKAGE_SHOOTER: u64 = u64::MAX;
/// Package entities on the server.
const MAX_ENTITIES: usize = 1024;
/// Entity slots kept free for every other package that declares entities.
const ENTITY_RESERVE: usize = 64;
/// A failing `on_tick` hook pauses this long, as a failing think does.
const FAILURE_PAUSE: u64 = 120;
const SECOND: u64 = 120;

/// A refilling allowance per origin. Levels are held scaled by `window` so
/// slow refills (8 lines per 120 ticks) are exact. A level may go into debt
/// by what one call actually used; the origin then waits until it is repaid.
struct Allowance<K: Ord> {
    capacity: i64,
    refill: i64,
    window: u64,
    levels: BTreeMap<K, (i64, u64)>,
}
impl<K: Ord + Clone> Allowance<K> {
    fn new(capacity: i64, refill: i64, window: u64) -> Self {
        Self {
            capacity,
            refill,
            window,
            levels: BTreeMap::new(),
        }
    }
    fn available(&mut self, key: &K, tick: u64) -> i64 {
        let window = self.window as i64;
        let full = self.capacity * window;
        let Some((level, at)) = self.levels.get_mut(key) else {
            return self.capacity;
        };
        let elapsed = tick.saturating_sub(*at).min(self.window * 1024) as i64;
        *level = level
            .saturating_add(self.refill.saturating_mul(elapsed))
            .min(full);
        *at = tick;
        level.div_euclid(window)
    }
    fn spend(&mut self, key: &K, tick: u64, amount: i64) {
        self.available(key, tick);
        let window = self.window as i64;
        let full = self.capacity * window;
        let entry = self.levels.entry(key.clone()).or_insert((full, tick));
        entry.0 = entry.0.saturating_sub(amount.saturating_mul(window));
        if self.levels.len() > 4096 {
            // Origins back at full carry no information.
            let (refill, window) = (self.refill, self.window as i64);
            self.levels.retain(|_, (level, at)| {
                *level + refill * (tick.saturating_sub(*at) as i64).min(window * 1024) < full
            });
        }
    }
}

struct Shares {
    work: Allowance<String>,
    commands: Allowance<PlayerKey>,
    edits: Allowance<String>,
    chat: Allowance<(String, Option<PlayerKey>)>,
    /// Prints and sounds, per package.
    cues: Allowance<String>,
    /// Projectiles, per package.
    shots: Allowance<String>,
    /// Environment changes, per package.
    environment: Allowance<String>,
}
impl Shares {
    fn new(script_packages: usize) -> Self {
        let share = SERVER_WORK_PER_TICK / script_packages.max(1) as i64;
        Self {
            work: Allowance::new(Budget::Tick.operations() as i64, share, 1),
            commands: Allowance::new(PLAYER_COMMAND_BURST, PLAYER_COMMAND_WORK, SECOND),
            edits: Allowance::new(PACKAGE_WORLD_EDITS, PACKAGE_WORLD_EDITS, SECOND),
            chat: Allowance::new(PACKAGE_CHAT_LINES, PACKAGE_CHAT_LINES, SECOND),
            cues: Allowance::new(PACKAGE_CUES, PACKAGE_CUES, SECOND),
            shots: Allowance::new(PACKAGE_SHOTS, PACKAGE_SHOTS, SECOND),
            environment: Allowance::new(
                PACKAGE_ENVIRONMENT_CHANGES,
                PACKAGE_ENVIRONMENT_CHANGES,
                SECOND,
            ),
        }
    }
}

/// Deaths held for `on_death` between ticks; more in one tick are dropped
/// with a diagnostic rather than growing without bound.
const MAX_PENDING_DEATHS: usize = 1024;
/// Cooldown entries kept before expired ones are swept.
const MAX_COOLDOWNS: usize = 4096;

fn note(host: &mut PackageHost, diagnostic: Diagnostic) {
    if host.diagnostics.len() == MAX_DIAGNOSTICS {
        host.diagnostics.pop_front();
    }
    host.diagnostics.push_back(diagnostic);
}
fn diagnostics_error(problems: Vec<Diagnostic>) -> anyhow::Error {
    anyhow::Error::new(bri_package::diag::Rejected(bri_package::diag::Diagnostics(
        problems,
    )))
}

/// A package archetype: its base with the declared overrides. Movement
/// constants merge by name, so a package names only what it changes and the
/// motor's own check decides what is valid.
fn archetype(
    table: &crate::archetype::Archetypes,
    id: &str,
    def: &bri_package_runtime::content::ArchetypeDef,
) -> Result<crate::archetype::Archetype> {
    let base = match &def.base {
        Some(base) => table
            .find(base)
            .with_context(|| format!("base {base} is not a known archetype"))?,
        None => Default::default(),
    };
    let mut archetype = table.resolve(base).clone();
    let mut movement = serde_json::to_value(&archetype.movement)?;
    let fields = movement
        .as_object_mut()
        .context("motor constants are an object")?;
    for (name, value) in &def.movement {
        ensure!(
            fields.contains_key(name),
            "movement has no constant `{name}`"
        );
        fields.insert(name.clone(), value.clone());
    }
    archetype.movement = serde_json::from_value(movement).context("movement")?;
    archetype.id = id.into();
    archetype.name = def.name.clone();
    if let Some(v) = def.max_health {
        archetype.max_health = v;
    }
    if let Some(v) = def.energy_bar {
        archetype.energy_bar = v;
    }
    if let Some(v) = def.rideable {
        archetype.rideable = v;
    }
    if let Some(v) = def.can_ride {
        archetype.can_ride = v;
    }
    if let Some(points) = &def.mount_points {
        archetype.mount_points = points
            .iter()
            .map(|m| crate::archetype::MountPoint {
                node: m.node.clone(),
                position: m.position,
                pose: m.pose.clone(),
            })
            .collect();
    }
    if let Some(v) = &def.model {
        archetype.look.model = v.clone();
    }
    if let Some(v) = def.camera_distance {
        archetype.look.camera_distance = v;
    }
    archetype.validate()?;
    Ok(archetype)
}

impl Session {
    /// Enable a set of mod packages before any player joins: compile their
    /// scripts, set up defaults, restore a save, and generate the world
    /// around the origin. Returns spawn points on the generated ground when
    /// a package provides the world.
    pub fn install_packages(
        &mut self,
        catalog: Arc<Catalog>,
        save: Option<PackageSave>,
    ) -> Result<Vec<Vec3>> {
        ensure!(
            self.peers.is_empty() && self.departed.is_empty() && self.packages.is_none(),
            "Packages are enabled before players join"
        );
        let runtime = Runtime::compile(&catalog).map_err(diagnostics_error)?;
        let mut archetypes = crate::archetype::Archetypes::default();
        for (id, def) in catalog.archetypes() {
            let archetype =
                archetype(&archetypes, id, def).with_context(|| format!("Archetype {id}"))?;
            archetypes.add(archetype)?;
        }
        self.archetypes = archetypes;
        self.minigames = combat::new_world(self.minigames.catalog().clone(), &self.archetypes);
        if let Some((id, mode)) = catalog.running_mode()
            && let Some(minigame) = &mode.minigame
        {
            let settings = combat::mode_settings(&mode.name, minigame);
            self.minigames
                .host_create(0, settings)
                .map_err(|e| anyhow::anyhow!("Game mode {id}: its mini-game settings: {e}"))?;
        }
        let save = save.unwrap_or_default();
        let mut store = save.store;
        for (id, behaviour) in catalog.behaviours() {
            let ns = store.namespace_mut(id);
            for (key, def) in &behaviour.state.global {
                ns.global
                    .entry(key.clone())
                    .or_insert_with(|| def.default.clone());
            }
        }
        let world = match catalog.world() {
            Some((package, provider, def)) => {
                let seed = match &save.world {
                    Some(w) if &w.provider == provider => w.seed,
                    _ => def.seed,
                };
                let (removed, added) = save
                    .world
                    .filter(|w| &w.provider == provider)
                    .map(|w| (w.removed, w.added))
                    .unwrap_or_default();
                let mut world = self.prepare_world(package.id(), provider, def, seed, removed)?;
                for (material, positions) in added {
                    // A material the world no longer has is dropped with
                    // its voxels.
                    if let Some(m) = world.material(&material) {
                        world.added.extend(positions.into_iter().map(|p| (p, m)));
                    }
                }
                Some(world)
            }
            None => None,
        };
        let scripts = catalog
            .behaviours()
            .filter(|(id, _)| runtime.has_script(id))
            .count();
        let state_bytes = store.stored_size();
        self.packages = Some(Box::new(PackageHost {
            catalog,
            runtime,
            store,
            world,
            entities: BTreeMap::new(),
            next_entity: 1,
            cooldowns: BTreeMap::new(),
            diagnostics: VecDeque::new(),
            output: VecDeque::new(),
            deaths: VecDeque::new(),
            loadouts: VecDeque::new(),
            spawns: VecDeque::new(),
            in_damage_hook: false,
            item_hooks: Default::default(),
            shares: Shares::new(scripts),
            script_time: BTreeMap::new(),
            state_bytes,
            hooks_due: BTreeSet::new(),
            hooks_paused: BTreeMap::new(),
            view: None,
        }));
        let Some(view) = self
            .packages
            .as_ref()
            .and_then(|h| h.world.as_ref())
            .map(|w| w.def.view_chunks as i64)
        else {
            return Ok(Vec::new());
        };
        for cx in -view..=view {
            for cz in -view..=view {
                self.generate_chunk((cx, cz))?;
            }
        }
        Ok(self.generated_spawns())
    }
    fn prepare_world(
        &mut self,
        package: &str,
        provider: &str,
        def: &ChunkWorld,
        seed: i64,
        removed: BTreeSet<[i64; 3]>,
    ) -> Result<GeneratedWorld> {
        let brick = Brick::new(ContentRef::Resolved(def.voxel_brick.clone()), [0.0; 3], 0);
        let definition = self.simulation.definitions.get(&brick).with_context(|| {
            format!(
                "World provider {provider} draws voxels with `{}`, which is not a known brick",
                def.voxel_brick
            )
        })?;
        let (min, max) = crate::definitions::brick_box(&brick, &definition.mesh);
        let size = max - min;
        ensure!(
            (size - Vec3::splat(def.voxel_size)).abs().max_element() < 1e-3,
            "World provider {provider}: `{}` is {size} units, not a {} unit cube",
            def.voxel_brick,
            def.voxel_size
        );
        let mut palette = self.simulation.state().palette.clone();
        let mut colors = Vec::new();
        for material in &def.materials {
            let index = match palette.iter().position(|c| *c == material.color) {
                Some(i) => i,
                None => {
                    ensure!(
                        palette.len() < 256,
                        "World provider materials overflow the 256-colour palette"
                    );
                    palette.push(material.color);
                    palette.len() - 1
                }
            };
            colors.push(u8::try_from(index)?);
        }
        if palette != self.simulation.state().palette {
            let plan = bri_world::build::LoadPlan::batch(
                self.simulation.state(),
                &palette,
                Vec::new(),
                self.next_owner,
            )?;
            self.simulation.load_build(
                &Actor {
                    owner: 0,
                    administrator: true,
                    ..Default::default()
                },
                plan,
            )?;
        }
        Ok(GeneratedWorld {
            provider: provider.into(),
            package: package.into(),
            def: def.clone(),
            seed,
            chunks: BTreeSet::new(),
            voxels: BTreeMap::new(),
            removed,
            added: BTreeMap::new(),
            colors,
        })
    }
    /// Spawn points on top of the generated ground near the origin.
    fn generated_spawns(&self) -> Vec<Vec3> {
        let Some(world) = self.packages.as_ref().and_then(|h| h.world.as_ref()) else {
            return Vec::new();
        };
        let mut tops: BTreeMap<(i64, i64), i64> = BTreeMap::new();
        for v in world.voxels.values() {
            let [x, y, z] = v.position;
            if x.abs() <= 3 && z.abs() <= 3 {
                let top = tops.entry((x, z)).or_insert(y);
                *top = (*top).max(y);
            }
        }
        tops.into_iter()
            .map(|((x, z), y)| {
                let c = world.center([x, y, z]);
                Vec3::new(c[0], c[1] + world.def.voxel_size * 0.5 + 0.05, c[2])
            })
            .collect()
    }
    /// Generate one chunk through the package's script and add its voxels as
    /// world-owned bricks.
    fn generate_chunk(&mut self, chunk: (i64, i64)) -> Result<()> {
        let Some(mut host) = self.packages.take() else {
            return Ok(());
        };
        let result = (|| -> Result<()> {
            let world = host.world.as_mut().context("No world provider")?;
            if !world.chunks.insert(chunk) || !world.in_bounds(chunk) {
                return Ok(());
            }
            let n = i64::from(world.def.chunk_voxels);
            let limit = (n * n) as usize * 128;
            let call = Call {
                function: &world.def.generate.clone(),
                args: vec![chunk.0.into(), chunk.1.into()],
                budget: Budget::Generate,
                snapshot: Arc::new(Snapshot {
                    seed: world.seed,
                    tick: self.simulation.state().tick,
                    ..Default::default()
                }),
                caller: None,
                aim: None,
                entity: None,
                state: Namespace::default(),
                entity_vars: Default::default(),
                world: None,
            };
            let package = world.package.clone();
            let outcome = host
                .runtime
                .call(&package, call)
                .map_err(|d| diagnostics_error(vec![d]))?;
            let world = host.world.as_mut().expect("checked above");
            let voxels = script::voxels(&outcome.returned, world.def.materials.len(), limit)
                .map_err(|m| {
                    diagnostics_error(vec![
                        Diagnostic::error("world.voxels", m).at(package.clone()),
                    ])
                })?;
            let mut bricks = Vec::new();
            let mut placed = Vec::new();
            let mut seen = BTreeSet::new();
            // Placed voxels go in after the generator's own, which never
            // stand where one was placed (it was dug out first).
            let added = world
                .added
                .iter()
                .filter(|(p, _)| world.chunk_of_voxel(**p) == chunk)
                .map(|(p, m)| [p[0], p[1], p[2], *m as i64]);
            for [x, y, z, m] in voxels.into_iter().chain(added.collect::<Vec<_>>()) {
                let position = [x, y, z];
                let generated = !world.added.contains_key(&position);
                // Voxels belong to the chunk that generated them.
                if (x.div_euclid(n), z.div_euclid(n)) != chunk
                    || (generated && world.removed.contains(&position))
                    || !seen.insert(position)
                {
                    continue;
                }
                bricks.push(world.brick(position, m as usize));
                placed.push(Voxel {
                    position,
                    material: m as usize,
                });
            }
            ensure!(
                self.simulation.state().bricks.len() + bricks.len() <= MAX_BRICKS,
                "The generated world reached the brick limit"
            );
            if bricks.is_empty() {
                return Ok(());
            }
            let palette = self.simulation.state().palette.clone();
            let plan = bri_world::build::LoadPlan::batch(
                self.simulation.state(),
                &palette,
                bricks,
                self.next_owner,
            )?;
            let ids = self.simulation.load_build(
                &Actor {
                    owner: 0,
                    administrator: true,
                    ..Default::default()
                },
                plan,
            )?;
            for (id, voxel) in ids.into_iter().zip(placed) {
                self.dirty.insert(id);
                world.voxels.insert(id, voxel);
            }
            Ok(())
        })();
        if let Err(error) = &result {
            let diagnostic = error
                .downcast_ref::<bri_package::diag::Rejected>()
                .and_then(|r| r.0.0.first().cloned())
                .unwrap_or_else(|| Diagnostic::error("world.generate", format!("{error:#}")));
            note(&mut host, diagnostic);
        }
        self.packages = Some(host);
        result
    }

    /// The durable key a player's package state lives under.
    fn player_key(&self, owner: OwnerId) -> PlayerKey {
        match self.peers.get(&owner).and_then(|p| p.principal) {
            Some(principal) => PlayerKey::principal(&principal.0),
            None => PlayerKey::session(owner),
        }
    }
    fn package_vars(&self, package: &str) -> script::EntityVars {
        self.packages
            .as_ref()
            .map(|h| {
                h.entities
                    .iter()
                    .filter(|(_, e)| e.package == package)
                    .map(|(id, e)| (*id, e.vars.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn tick_view(&self) -> TickView {
        let mut vars: BTreeMap<String, script::EntityVars> = BTreeMap::new();
        if let Some(h) = self.packages.as_ref() {
            for (id, e) in &h.entities {
                vars.entry(e.package.clone())
                    .or_default()
                    .insert(*id, e.vars.clone());
            }
        }
        TickView {
            snapshot: Arc::new(self.package_snapshot()),
            vars: vars.into_iter().map(|(k, v)| (k, Arc::new(v))).collect(),
        }
    }
    fn package_snapshot(&self) -> Snapshot {
        let host = self.packages.as_ref();
        Snapshot {
            tick: self.simulation.state().tick,
            environment: self.environment.clone(),
            seed: host.and_then(|h| h.world.as_ref()).map_or(0, |w| w.seed),
            players: self
                .peers
                .iter()
                .filter(|(o, _)| !self.bots.is_bot(**o))
                .map(|(owner, p)| {
                    let actor = self.weapons.actor(bri_weapons::ActorId(*owner));
                    let item = actor
                        .and_then(|a| a.inventory.get(a.selected?)?.clone())
                        .unwrap_or_default();
                    let (image, image_state) = self
                        .weapons
                        .image_state(bri_weapons::ActorId(*owner), 0)
                        .map(|(image, state)| (image.id.clone(), state.name.clone()))
                        .unwrap_or_default();
                    let state = p.player.state();
                    let tuning = p.player.tuning();
                    let height = if state.crouched {
                        tuning.crouch_height
                    } else {
                        tuning.stand_height
                    };
                    PlayerView {
                        id: *owner,
                        key: self.player_key(*owner),
                        name: p.name.clone(),
                        position: p.player.state().feet,
                        alive: p.combat.alive,
                        admin: p.actor.administrator,
                        eye: p.player.eye().to_array(),
                        look: p.player.state().forward().to_array(),
                        velocity: p.player.state().velocity,
                        item,
                        minigame: self
                            .minigames
                            .player(p.combat.player)
                            .ok()
                            .and_then(|m| m.game)
                            .map(|g| g.0),
                        health: p.combat.health,
                        max_health: self
                            .archetypes
                            .resolve(p.player.state().archetype)
                            .max_health,
                        archetype: self
                            .archetypes
                            .resolve(p.player.state().archetype)
                            .id
                            .clone(),
                        crouched: state.crouched,
                        mounted: self.seated(*owner),
                        scale: state.scale,
                        center: [
                            state.feet[0],
                            state.feet[1] + height * 0.5,
                            state.feet[2],
                        ],
                        slot: actor.and_then(|a| a.selected).map(|s| s as u64),
                        muzzle: actor
                            .filter(|_| !image.is_empty())
                            .map_or(p.player.eye(), |a| a.frame.muzzle[0])
                            .to_array(),
                        tools: actor.map_or_else(Vec::new, |a| {
                            a.inventory
                                .iter()
                                .map(|t| t.clone().unwrap_or_default())
                                .collect()
                        }),
                        image,
                        image_state,
                        paint: p.current_color,
                    }
                })
                .collect(),
            entities: host
                .map(|h| {
                    h.entities
                        .iter()
                        .map(|(id, e)| {
                            let v = e.body.state().velocity;
                            EntityView {
                                id: *id,
                                kind: e.kind.clone(),
                                position: e.body.state().feet,
                                yaw: e.body.state().yaw,
                                label: e.label.clone(),
                                health: e.health,
                                speed: (v[0] * v[0] + v[2] * v[2]).sqrt(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
            objects: self.movable_views(),
            holds: self.hold_views(),
        }
    }
    /// Give a joining player every package's player defaults and run
    /// `on_join` hooks. State saved under the player's durable key is kept.
    pub(super) fn packages_joined(&mut self, owner: OwnerId) {
        if self.packages.is_none() || self.bots.is_bot(owner) {
            return;
        }
        let key = self.player_key(owner);
        let hooks: Vec<String> = {
            let host = self.packages.as_mut().expect("checked");
            let catalog = host.catalog.clone();
            let mut hooks = Vec::new();
            for (id, behaviour) in catalog.behaviours() {
                let values = host
                    .store
                    .namespace_mut(id)
                    .players
                    .entry(key.clone())
                    .or_default();
                for (k, def) in &behaviour.state.player {
                    values
                        .entry(k.clone())
                        .or_insert_with(|| def.default.clone());
                }
                if behaviour.on_join {
                    hooks.push(id.clone());
                }
            }
            hooks
        };
        for package in hooks {
            let _ = self.run_package(
                &package,
                "on_join",
                vec![Dynamic::from_int(owner as i64)],
                Budget::Command,
                Some(owner),
                None,
                None,
            );
            self.charge_work(&package);
        }
    }
    /// Run one package function, then commit its state and apply its
    /// operations. A failed call changes nothing and is reported.
    #[allow(clippy::too_many_arguments)]
    fn run_package(
        &mut self,
        package: &str,
        function: &str,
        args: Vec<Dynamic>,
        budget: Budget,
        caller: Option<OwnerId>,
        aim: Option<script::Aim>,
        entity: Option<u64>,
    ) -> std::result::Result<Dynamic, Diagnostic> {
        let shared = self
            .packages
            .as_ref()
            .and_then(|h| h.view.as_ref())
            .map(|v| {
                (
                    v.snapshot.clone(),
                    v.vars.get(package).cloned().unwrap_or_default(),
                )
            });
        let (snapshot, entity_vars) = match shared {
            Some(shared) => shared,
            None => (
                Arc::new(self.package_snapshot()),
                Arc::new(self.package_vars(package)),
            ),
        };
        let Some(host) = self.packages.as_ref() else {
            return Err(Diagnostic::error("package.none", "No packages are enabled"));
        };
        let state = host.store.namespace(package).cloned().unwrap_or_default();
        let input = state.clone();
        // Scripts ask the live world mid-call (`raycast`, `can_damage`), so
        // the session is only read while the script runs.
        let world = super::script_world::ScriptWorld::new(self);
        let call = Call {
            function,
            args,
            budget,
            snapshot,
            caller,
            aim,
            entity,
            state,
            entity_vars,
            world: Some(&world),
        };
        let started = std::time::Instant::now();
        let result = host.runtime.call(package, call);
        let took = started.elapsed();
        drop(world);
        let host = self.packages.as_mut().expect("checked above");
        match host.script_time.get_mut(package) {
            Some(total) => *total += took,
            None => {
                host.script_time.insert(package.to_string(), took);
            }
        }
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(diagnostic) => {
                note(host, diagnostic.clone());
                return Err(diagnostic);
            }
        };
        for line in outcome.output {
            if host.output.len() == MAX_DIAGNOSTICS {
                host.output.pop_front();
            }
            host.output.push_back(format!("{package}: {line}"));
        }
        // A call may only write declared keys. Keys it left as they were (a
        // save from an older version of the package, say) are kept, not
        // blamed on this call.
        let schema = host
            .catalog
            .packages
            .get(package)
            .and_then(|p| p.behaviour.as_ref())
            .map(|b| b.state.clone())
            .unwrap_or_default();
        let undeclared = outcome
            .state
            .global
            .iter()
            .find(|(k, v)| !schema.global.contains_key(*k) && input.global.get(*k) != Some(v))
            .map(|(k, _)| k)
            .or_else(|| {
                outcome.state.players.iter().find_map(|(player, m)| {
                    let before = input.players.get(player);
                    m.iter()
                        .find(|(k, v)| {
                            !schema.player.contains_key(*k)
                                && before.and_then(|b| b.get(*k)) != Some(v)
                        })
                        .map(|(k, _)| k)
                })
            });
        if let Some(key) = undeclared {
            let d = Diagnostic::error(
                "state.undeclared",
                format!("{function} wrote state key `{key}`, which the behaviour does not declare"),
            )
            .at(package.to_string())
            .hint("declare every state key under behaviour.json `state`");
            note(host, d.clone());
            return Err(d);
        }
        // State is budgeted per player, per package and in total, and only
        // growth is refused, so a call can always shrink state (W7).
        let mut growth = 0_isize;
        let mut over = None;
        if outcome.state.global != input.global {
            let (before, after) = (
                state::stored_size(&input.global),
                state::stored_size(&outcome.state.global),
            );
            growth += after as isize - before as isize;
            if after > before && after > state::MAX_GLOBAL_STATE_BYTES {
                over = Some(format!(
                    "{function} would grow server-wide state to {after} bytes; the limit is {}",
                    state::MAX_GLOBAL_STATE_BYTES
                ));
            }
        }
        let empty = BTreeMap::new();
        for (player, values) in &outcome.state.players {
            let old = input.players.get(player).unwrap_or(&empty);
            if old == values {
                continue;
            }
            let (before, after) = (state::stored_size(old), state::stored_size(values));
            growth += after as isize - before as isize;
            if after > before && after > state::MAX_PLAYER_STATE_BYTES {
                over = Some(format!(
                    "{function} would grow a player's state to {after} bytes; the limit is {}",
                    state::MAX_PLAYER_STATE_BYTES
                ));
            }
        }
        for (player, values) in &input.players {
            if !outcome.state.players.contains_key(player) {
                growth -= state::stored_size(values) as isize;
            }
        }
        let total = host.state_bytes.saturating_add_signed(growth);
        if over.is_none() && growth > 0 && total > state::MAX_STATE_BYTES {
            over = Some(format!(
                "{function} would grow package state past the server's {} bytes",
                state::MAX_STATE_BYTES
            ));
        }
        if let Some(message) = over {
            let d = Diagnostic::error("state.budget", message)
                .at(package.to_string())
                .hint("keep per-player state small; store counts, not logs");
            note(host, d.clone());
            return Err(d);
        }
        let capabilities = host
            .catalog
            .packages
            .get(package)
            .map(|p| p.manifest.capabilities.clone())
            .unwrap_or_default();
        for op in &outcome.ops {
            let owned = match op {
                Op::RemoveEntity { entity }
                | Op::Steer { entity, .. }
                | Op::Label { entity, .. } => host
                    .entities
                    .get(entity)
                    .is_some_and(|e| e.package == package),
                _ => true,
            };
            let checked = authorize(package, &capabilities, op).and_then(|()| {
                if owned {
                    Ok(())
                } else {
                    Err(Diagnostic::error(
                        "op.not_owner",
                        format!(
                            "{} targets an entity this package does not own",
                            bri_package_runtime::ops::op_name(op)
                        ),
                    )
                    .at(package.to_string()))
                }
            });
            if let Err(d) = checked {
                note(host, d.clone());
                return Err(d);
            }
        }
        host.state_bytes = total;
        *host.store.namespace_mut(package) = outcome.state;
        for (id, vars) in outcome.entity_vars {
            if let Some(e) = host.entities.get_mut(&id) {
                e.vars = vars;
            }
        }
        let returned = outcome.returned;
        for op in outcome.ops {
            if let Err(error) = self.apply_package_op(package, op, caller) {
                let host = self.packages.as_mut().expect("installed");
                note(
                    host,
                    Diagnostic::warning("op.failed", format!("{error:#}")).at(package.to_string()),
                );
            }
        }
        Ok(returned)
    }
    /// Apply an authorized operation. `caller` is the player whose command
    /// asked for it: brick changes then need that player's trust, so a
    /// package cannot be used to reach another player's build (stress
    /// campaign W9). Without a caller (hooks, think, generation) a package
    /// acts only on world-owned bricks, such as its generated world.
    fn apply_package_op(&mut self, package: &str, op: Op, caller: Option<OwnerId>) -> Result<()> {
        let tick = self.simulation.state().tick;
        match op {
            Op::RemoveBrick { brick } => self.package_remove_brick(package, brick, None, caller),
            Op::PlaceBrick {
                shape,
                position,
                color,
            } => self.package_place_brick(package, &shape, position, color),
            Op::PlaceVoxel { position, material } => {
                self.package_place_voxel(package, position, &material)
            }
            Op::SetAvatarColors { player, colors } => {
                let peer = self.peers.get_mut(&player).context("No such player")?;
                peer.uniform = colors;
                Ok(())
            }
            Op::Explode {
                position,
                radius,
                damage,
                brick_radius,
            } => self.explode(
                Vec3::from(position),
                radius,
                damage,
                brick_radius,
                package,
                caller,
            ),
            Op::Damage {
                target,
                amount,
                by,
                damage_type,
            } => self.package_damage_op(package, target, amount, by, damage_type),
            Op::Teleport { player, position } => {
                let peer = self.peers.get_mut(&player).context("No such player")?;
                ensure!(peer.combat.alive, "Only living players can be moved");
                let yaw = peer.player.state().yaw;
                peer.player
                    .teleport(&mut self.simulation.physics, Vec3::from(position), yaw)?;
                peer.inputs.clear();
                Ok(())
            }
            Op::SetArchetype { player, archetype } => {
                let chosen = if archetype.is_empty() {
                    None
                } else {
                    Some(
                        self.archetypes
                            .find(&archetype)
                            .with_context(|| format!("No archetype {archetype}"))?,
                    )
                };
                let peer = self.peers.get_mut(&player).context("No such player")?;
                peer.package_archetype = chosen;
                if let Some(chosen) = chosen
                    && peer.combat.alive
                {
                    self.set_player_archetype(player, chosen)?;
                }
                Ok(())
            }
            Op::Respawn { player } => {
                let target = self
                    .peers
                    .get(&player)
                    .context("No such player")?
                    .combat
                    .player;
                let effects = self
                    .minigames
                    .execute(bri_minigames::Command::ForceRespawn { target })
                    .map_err(|e| anyhow::anyhow!("Respawn rejected: {e}"))?;
                self.apply_minigame_effects(effects)
            }
            Op::SetBlockState { brick, state } => {
                let host = self.packages.as_ref().context("No packages are enabled")?;
                let look = self
                    .simulation
                    .state()
                    .bricks
                    .get(&brick)
                    .context("No such brick")?
                    .look
                    .as_ref()
                    .context("That brick shows no block")?;
                let block = host
                    .catalog
                    .block(&look.block)
                    .with_context(|| format!("No block {}", look.block))?;
                ensure!(
                    state.is_empty() || block.states.contains_key(&state),
                    "Block {} has no state `{state}`",
                    look.block
                );
                if look.state != state {
                    self.simulation.mutate(brick, |b| {
                        if let Some(look) = &mut b.look {
                            look.state = state;
                        }
                    })?;
                    self.dirty.insert(brick);
                }
                Ok(())
            }
            Op::Control { player, entity } => {
                let peer = self.peers.get(&player).context("No such player")?;
                let Some(entity) = entity else {
                    if matches!(peer.control, ControlObject::Entity(_)) {
                        self.return_to_body(player)?;
                    }
                    return Ok(());
                };
                ensure!(peer.combat.alive, "Only living players can drive");
                ensure!(
                    !self.seated(player),
                    "A seated player cannot drive an entity"
                );
                let host = self.packages.as_mut().context("No packages are enabled")?;
                let e = host
                    .entities
                    .get_mut(&entity)
                    .with_context(|| format!("No entity {entity}"))?;
                ensure!(
                    e.package == package,
                    "Packages hand players only their own entities"
                );
                ensure!(
                    !self
                        .peers
                        .iter()
                        .any(|(o, p)| *o != player && p.control == ControlObject::Entity(entity)),
                    "Another player drives that entity"
                );
                e.drive = Some(MoveInput {
                    yaw: e.body.state().yaw,
                    ..Default::default()
                });
                self.peers.get_mut(&player).expect("checked").control =
                    ControlObject::Entity(entity);
                Ok(())
            }
            Op::SpawnEntity {
                kind,
                position,
                vars,
            } => {
                let id = self.spawn_package_entity(&kind, Vec3::from(position))?;
                if let Some(e) = self.packages.as_mut().and_then(|h| h.entities.get_mut(&id)) {
                    e.vars = vars;
                }
                Ok(())
            }
            Op::RemoveEntity { entity } => {
                self.remove_package_entity(entity);
                Ok(())
            }
            Op::Steer {
                entity,
                direction,
                jump,
            } => {
                if let Some(e) = self
                    .packages
                    .as_mut()
                    .and_then(|h| h.entities.get_mut(&entity))
                {
                    e.steer = (Vec3::new(direction[0], 0.0, direction[1]), jump);
                }
                Ok(())
            }
            Op::Label { entity, label } => {
                if let Some(e) = self
                    .packages
                    .as_mut()
                    .and_then(|h| h.entities.get_mut(&entity))
                {
                    e.label = label;
                }
                Ok(())
            }
            Op::Tell { player, text } => {
                ensure!(self.peers.contains_key(&player), "No player {player}");
                self.take_chat_line(package, caller)?;
                self.notify(player, Notice::Chat(text));
                Ok(())
            }
            Op::Broadcast { text } => {
                let _ = tick;
                self.take_chat_line(package, caller)?;
                self.system_chat(text);
                Ok(())
            }
            Op::CopyBuild {
                player,
                brick,
                limit,
                above_only,
                tool,
            } => {
                // A copy is taken with the trust of the player who asked.
                ensure!(
                    caller == Some(player),
                    "A build is copied only for the player whose command asked"
                );
                // The player hears why a copy failed; nothing went wrong
                // with the Add-On.
                match self.copy_build(player, brick, limit as usize, above_only, &tool) {
                    Ok(count) => self.bottom_count(player, "Copied", count),
                    Err(error) => self.center_print(player, format!("{error:#}")),
                }
                Ok(())
            }
            Op::CopyBox {
                player,
                min,
                max,
                limit,
                tool,
            } => {
                ensure!(
                    caller == Some(player),
                    "A build is copied only for the player whose command asked"
                );
                match self.copy_box(player, min, max, limit as usize, &tool) {
                    Ok(count) => self.bottom_count(player, "Copied", count),
                    Err(error) => self.center_print(player, format!("{error:#}")),
                }
                Ok(())
            }
            Op::MirrorCopy { player, axis } => {
                ensure!(
                    caller == Some(player),
                    "A copy is mirrored only for the player whose command asked"
                );
                if let Err(error) = self.mirror_copy(player, axis) {
                    self.center_print(player, format!("{error:#}"));
                }
                Ok(())
            }
            Op::CutCopy { player } => {
                // Bricks go with the trust of the player who asked.
                ensure!(
                    caller == Some(player),
                    "A copy's bricks are cut only for the player whose command asked"
                );
                match self.cut_copy(player) {
                    Ok(count) => self.bottom_count(player, "Cut", count),
                    Err(error) => self.center_print(player, format!("{error:#}")),
                }
                Ok(())
            }
            Op::PaintCopy { player, color } => {
                ensure!(
                    caller == Some(player),
                    "A copy's bricks are painted only for the player whose command asked"
                );
                match self.paint_copy(player, color) {
                    Ok(count) => self.bottom_count(player, "Painted", count),
                    Err(error) => self.center_print(player, format!("{error:#}")),
                }
                Ok(())
            }
            Op::PaintFill {
                player,
                brick,
                color,
                limit,
            } => {
                ensure!(
                    caller == Some(player),
                    "Bricks are filled only for the player whose command asked"
                );
                match self.paint_fill(player, brick, color, limit as usize) {
                    Ok(fill) => self.bottom_fill(player, fill),
                    Err(error) => self.center_print(player, format!("{error:#}")),
                }
                Ok(())
            }
            Op::ShowBox { player, area, tool } => self.show_box(player, area, &tool),
            Op::GiveItem {
                player,
                item,
                equip,
            } => {
                ensure!(self.peers.contains_key(&player), "No such player");
                self.give_tool(player, &item, equip)
            }
            Op::TakeItem { player, item } => self.package_take_item(player, &item),
            Op::DropItem {
                item,
                position,
                velocity,
            } => self.package_drop_item(package, &item, position, velocity),
            op @ (Op::Push { .. }
            | Op::Tumble { .. }
            | Op::Hold { .. }
            | Op::HoldDistance { .. }
            | Op::LetGo { .. }
            | Op::Reach { .. }
            | Op::SpawnVehicle { .. }
            | Op::RemoveVehicle { .. }) => self.apply_physics_op(package, op, caller),
            Op::Fire {
                projectile,
                position,
                velocity,
                by,
            } => {
                let tick = self.simulation.state().tick;
                let host = self.packages.as_mut().context("No packages are enabled")?;
                ensure!(
                    item_hooks::owns(&host.catalog, package, &projectile),
                    "`{projectile}` is not a projectile of `{package}` or an Add-On it depends on"
                );
                let origin = package.to_string();
                ensure!(
                    host.shares.shots.available(&origin, tick) >= 1,
                    "Dropped: more than {PACKAGE_SHOTS} projectiles a second"
                );
                host.shares.shots.spend(&origin, tick, 1);
                let shooter = by
                    .filter(|p| self.peers.contains_key(p))
                    .unwrap_or(PACKAGE_SHOOTER);
                self.weapons.spawn(
                    &projectile,
                    bri_weapons::ActorId(shooter),
                    Vec3::from(position),
                    Vec3::from(velocity),
                    1.0,
                )?;
                Ok(())
            }
            Op::Heal { player, amount } => {
                let max = {
                    let peer = self.peers.get(&player).context("No such player")?;
                    self.archetypes
                        .resolve(peer.player.state().archetype)
                        .max_health
                };
                let peer = self.peers.get_mut(&player).context("No such player")?;
                ensure!(peer.combat.alive, "Only the living heal");
                peer.combat.health = (peer.combat.health + amount).min(max);
                Ok(())
            }
            Op::Print {
                player,
                text,
                seconds,
                bottom,
            } => {
                self.take_cue(package)?;
                let notice = if bottom {
                    Notice::Bottom {
                        text,
                        seconds,
                        hide_bar: false,
                    }
                } else {
                    Notice::Center { text, seconds }
                };
                match player {
                    Some(player) => {
                        ensure!(self.peers.contains_key(&player), "No such player");
                        self.notify(player, notice);
                    }
                    None => {
                        let everyone: Vec<OwnerId> = self.peers.keys().copied().collect();
                        for owner in everyone {
                            self.notify(owner, notice.clone());
                        }
                    }
                }
                Ok(())
            }
            Op::Beam {
                from,
                to,
                color,
                width,
                seconds,
                muzzle,
            } => {
                self.take_cue(package)?;
                self.cues.emit(
                    tick,
                    crate::presentation::CueKind::Beam {
                        to,
                        color,
                        width,
                        seconds,
                        muzzle: muzzle.filter(|m| self.peers.contains_key(m)),
                    },
                    from,
                );
                Ok(())
            }
            Op::PlayThread {
                player,
                thread,
                sequence,
            } => {
                let feet = self
                    .peers
                    .get(&player)
                    .context("No such player")?
                    .player
                    .state()
                    .feet;
                self.take_cue(package)?;
                self.cues.emit(
                    tick,
                    crate::presentation::CueKind::WeaponAnimation {
                        actor: player,
                        thread,
                        sequence,
                        image_hand: None,
                    },
                    feet,
                );
                Ok(())
            }
            Op::SetMapLights {
                position,
                radius,
                tint,
            } => self.set_map_lights(MapLightRule {
                position,
                radius,
                tint,
            }),
            Op::SetEnvironment { changes, unset } => {
                let tick = self.simulation.state().tick;
                let host = self.packages.as_mut().context("No packages are enabled")?;
                let origin = package.to_string();
                ensure!(
                    host.shares.environment.available(&origin, tick) >= 1,
                    "Dropped: more than {PACKAGE_ENVIRONMENT_CHANGES} environment changes a second"
                );
                host.shares.environment.spend(&origin, tick, 1);
                let mut settings = self.environment();
                settings.merge(&changes);
                for key in &unset {
                    ensure!(settings.unset(key), "No environment setting {key}");
                }
                self.set_environment(settings)
            }
            Op::SetFov { player, fov } => {
                ensure!(self.peers.contains_key(&player), "No such player");
                self.notify(player, Notice::Fov(fov));
                Ok(())
            }
            Op::SetImageAmmo { player, ammo } => {
                ensure!(self.peers.contains_key(&player), "No such player");
                self.weapons.set_ammo(bri_weapons::ActorId(player), ammo)
            }
            Op::MountImage { player, image } => {
                let peer = self.peers.get(&player).context("No such player")?;
                ensure!(peer.combat.alive, "Only living players hold things");
                let actor = bri_weapons::ActorId(player);
                match image {
                    Some(image) => {
                        let host = self.packages.as_ref().context("No packages are enabled")?;
                        let namespace = image.split(':').next().unwrap_or_default();
                        let depends = host
                            .catalog
                            .packages
                            .get(package)
                            .is_some_and(|p| p.manifest.dependencies.contains_key(namespace));
                        ensure!(
                            namespace == package || depends,
                            "`{image}` is not an image of `{package}` or an Add-On it depends on"
                        );
                        self.weapons.swap_image(actor, Some(&image))
                    }
                    None => self.weapons.swap_image(actor, None),
                }
            }
            Op::Sound { profile, at } => {
                self.take_cue(package)?;
                match at {
                    bri_package_runtime::ops::SoundAt::Player(player) => {
                        ensure!(self.peers.contains_key(&player), "No such player");
                        self.notify(player, Notice::Sound(profile));
                    }
                    bri_package_runtime::ops::SoundAt::Position(position) => self.cues.emit(
                        tick,
                        crate::presentation::CueKind::WeaponSound { profile },
                        position,
                    ),
                }
                Ok(())
            }
        }
    }
    /// `%obj.damage` from a script, with the same scaling and hooks as a
    /// weapon's hit of that damage type.
    fn package_damage_op(
        &mut self,
        package: &str,
        target: ObjectRef,
        amount: f32,
        by: Option<OwnerId>,
        damage_type: Option<String>,
    ) -> Result<()> {
        let by = by.filter(|by| self.peers.contains_key(by));
        if let Some(name) = &damage_type {
            ensure!(
                self.weapons.pack.damage_type(name).is_some(),
                "No damage type `{name}`"
            );
        }
        match target {
            ObjectRef::Player(player) => {
                ensure!(self.peers.contains_key(&player), "No such player");
                let kind = match damage_type {
                    Some(name) => {
                        let direct = self
                            .weapons
                            .pack
                            .damage_type(&name)
                            .is_some_and(|t| t.direct);
                        combat::DamageKind::Weapon { name, direct }
                    }
                    None => combat::DamageKind::Package {
                        name: package.into(),
                    },
                };
                self.damage_player(player, amount, kind, by)
            }
            ObjectRef::Vehicle(vehicle) => {
                let world = self.vehicles.world.as_ref().context("No such vehicle")?;
                let centre = world
                    .vehicle_snapshot(
                        &self.simulation.physics,
                        bri_vehicles::VehicleId(vehicle),
                    )
                    .filter(|v| !v.destroyed)
                    .context("No such vehicle")?
                    .transform
                    .position;
                self.damage_vehicle(
                    vehicle,
                    amount,
                    by.unwrap_or(PACKAGE_SHOOTER),
                    damage_type.as_deref().unwrap_or(package),
                    Vec3::from(centre),
                )
            }
            ObjectRef::Entity(entity) => {
                let (kind, name) = match &damage_type {
                    Some(name) => ("weapon", name.as_str()),
                    None => ("package", package),
                };
                self.damage_entity(entity, amount, by, kind, name);
                Ok(())
            }
        }
    }
    /// One print or sound from `package`, within its share.
    fn take_cue(&mut self, package: &str) -> Result<()> {
        let tick = self.simulation.state().tick;
        let host = self.packages.as_mut().context("No packages are enabled")?;
        let origin = package.to_string();
        ensure!(
            host.shares.cues.available(&origin, tick) >= 1,
            "Dropped: more than {PACKAGE_CUES} prints and sounds a second"
        );
        host.shares.cues.spend(&origin, tick, 1);
        Ok(())
    }
    /// One chat line from `package` on behalf of `caller`, within their share.
    fn take_chat_line(&mut self, package: &str, caller: Option<OwnerId>) -> Result<()> {
        let tick = self.simulation.state().tick;
        let origin = (package.to_string(), caller.map(|c| self.player_key(c)));
        let host = self.packages.as_mut().context("No packages are enabled")?;
        ensure!(
            host.shares.chat.available(&origin, tick) >= 1,
            "Chat line dropped: more than {PACKAGE_CHAT_LINES} lines a second"
        );
        host.shares.chat.spend(&origin, tick, 1);
        Ok(())
    }
    /// Add a world-owned brick through the same load path as a build, so
    /// the grid, overlap and storage checks all apply.
    fn package_place_brick(
        &mut self,
        package: &str,
        shape: &str,
        position: [f32; 3],
        color: [f32; 4],
    ) -> Result<()> {
        let tick = self.simulation.state().tick;
        let origin = package.to_string();
        let host = self.packages.as_mut().context("No packages are enabled")?;
        ensure!(
            host.shares.edits.available(&origin, tick) >= 1,
            "`{package}` used its share of world edits for now"
        );
        let state = self.simulation.state();
        let distance =
            |c: &[f32; 4]| -> f32 { c.iter().zip(color).map(|(a, b)| (a - b).powi(2)).sum() };
        let index = state
            .palette
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b)))
            .map(|(i, _)| i)
            .context("The world has no palette")?;
        let mut brick = Brick::new(ContentRef::Resolved(shape.into()), position, 0);
        brick.color = u8::try_from(index)?;
        let palette = state.palette.clone();
        let plan =
            bri_world::build::LoadPlan::batch(state, &palette, vec![brick], self.next_owner)?;
        let ids = self.simulation.load_build(
            &Actor {
                owner: 0,
                administrator: true,
                ..Default::default()
            },
            plan,
        )?;
        self.dirty.extend(ids);
        let host = self.packages.as_mut().expect("checked");
        host.shares.edits.spend(&origin, tick, 1);
        Ok(())
    }
    /// Put a voxel into the generated world: a world-owned brick of the
    /// material, recorded as a world edit and saved with the world.
    fn package_place_voxel(
        &mut self,
        package: &str,
        position: [i64; 3],
        material: &str,
    ) -> Result<()> {
        let tick = self.simulation.state().tick;
        let origin = package.to_string();
        let host = self.packages.as_mut().context("No packages are enabled")?;
        let world = host.world.as_ref().context("No generated world is running")?;
        let m = world
            .material(material)
            .with_context(|| format!("The world has no material `{material}`"))?;
        let chunk = world.chunk_of_voxel(position);
        ensure!(
            world.in_bounds(chunk) && world.chunks.contains(&chunk),
            "Voxel {position:?} is outside the generated world"
        );
        ensure!(
            host.shares.edits.available(&origin, tick) >= 1,
            "`{package}` used its share of world edits for now"
        );
        let brick = world.brick(position, m);
        let ids = self
            .simulation
            .restore_group(vec![brick])
            .with_context(|| format!("Voxel {position:?} is not free"))?;
        self.dirty.extend(ids.iter().copied());
        let host = self.packages.as_mut().expect("checked");
        host.shares.edits.spend(&origin, tick, 1);
        let world = host.world.as_mut().expect("checked");
        world.added.insert(position, m);
        for id in ids {
            world.voxels.insert(
                id,
                Voxel {
                    position,
                    material: m,
                },
            );
        }
        Ok(())
    }
    /// Whether a voxel could go at `position` now (`can_place_voxel`).
    pub(super) fn voxel_fits(&self, position: [i64; 3]) -> bool {
        let Some(world) = self.packages.as_ref().and_then(|h| h.world.as_ref()) else {
            return false;
        };
        let chunk = world.chunk_of_voxel(position);
        world.in_bounds(chunk)
            && world.chunks.contains(&chunk)
            && self.simulation.fits(&world.brick(position, 0))
    }
    /// Remove a brick for good, recording generated voxels as world edits.
    fn package_remove_brick(
        &mut self,
        package: &str,
        brick: BrickId,
        blast: Option<super::debris::BrickBlast>,
        caller: Option<OwnerId>,
    ) -> Result<()> {
        let b = self
            .simulation
            .state()
            .bricks
            .get(&brick)
            .context("No such brick")?;
        let trusted = b.owner == 0
            || caller
                .and_then(|c| self.peers.get(&c))
                .is_some_and(|p| p.actor.trusted(b.owner, bri_world::authority::trust::FULL));
        ensure!(
            trusted,
            "Brick {brick} belongs to a build the caller has no trust on"
        );
        let definition = self.simulation.definitions.get(b)?;
        ensure!(
            !definition.indestructible && !b.base_plate,
            "Brick {brick} is indestructible"
        );
        if let Some(world) = self.packages.as_ref().and_then(|h| h.world.as_ref())
            && let Some(voxel) = world.voxels.get(&brick)
        {
            ensure!(
                !world.def.materials[voxel.material].indestructible,
                "{} cannot be removed",
                world.def.materials[voxel.material].name
            );
        }
        // Destruction draws on the package's share, so one call cannot
        // level a world (W1).
        let tick = self.simulation.state().tick;
        let origin = package.to_string();
        let host = self.packages.as_mut().context("No packages are enabled")?;
        ensure!(
            host.shares.edits.available(&origin, tick) >= 1,
            "`{package}` destroyed its share of bricks for now"
        );
        host.shares.edits.spend(&origin, tick, 1);
        let admin = Actor {
            owner: 0,
            administrator: true,
            ..Default::default()
        };
        self.kill_one_brick(&admin, brick, blast)?;
        self.forget_voxel(brick);
        Ok(())
    }
    /// "Copied 1 brick", "Cut 40 bricks": what a copy operation did, at
    /// the bottom of the player's screen.
    fn bottom_count(&mut self, player: OwnerId, verb: &str, count: usize) {
        let text = match count {
            1 => format!("{verb} 1 brick"),
            n => format!("{verb} {n} bricks"),
        };
        self.notify(
            player,
            Notice::Bottom {
                text,
                seconds: 2.0,
                hide_bar: false,
            },
        );
    }
    fn bottom_fill(&mut self, player: OwnerId, fill: super::Fill) {
        let painted = match fill.painted {
            1 => "Filled 1 brick".to_string(),
            n => format!("Filled {n} bricks"),
        };
        let text = match fill.refused {
            0 => painted,
            1 => format!("{painted}; 1 more is not yours to paint"),
            n => format!("{painted}; {n} more are not yours to paint"),
        };
        self.notify(
            player,
            Notice::Bottom {
                text,
                seconds: 2.0,
                hide_bar: false,
            },
        );
    }
    fn forget_voxel(&mut self, brick: BrickId) {
        if let Some(world) = self.packages.as_mut().and_then(|h| h.world.as_mut())
            && let Some(voxel) = world.voxels.remove(&brick)
        {
            world.added.remove(&voxel.position);
            world.removed.insert(voxel.position);
        }
    }
    /// The one explosion operation: damage players within `radius` (full at
    /// the centre, none at the edge) whom the caller may hurt, damage package
    /// entities the same way, and destroy bricks within `brick_radius`.
    pub fn explode(
        &mut self,
        center: Vec3,
        radius: f32,
        damage: f32,
        brick_radius: f32,
        source: &str,
        caller: Option<OwnerId>,
    ) -> Result<()> {
        ensure!(
            center.is_finite()
                && radius.is_finite()
                && damage.is_finite()
                && brick_radius.is_finite(),
            "Invalid explosion"
        );
        let victims: Vec<(OwnerId, f32)> = self
            .peers
            .iter()
            .filter_map(|(owner, p)| {
                let d = (Vec3::from(p.player.state().feet) + Vec3::Y).distance(center);
                (d < radius).then(|| (*owner, damage * (1.0 - d / radius)))
            })
            // A player's blast obeys the minigame's radius damage rule, as
            // their weapons' blasts do.
            .filter(|(owner, _)| caller.is_none_or(|c| self.can_damage_player(c, *owner, true)))
            .collect();
        for (owner, amount) in victims {
            self.damage_player(
                owner,
                amount,
                combat::DamageKind::Package {
                    name: source.into(),
                },
                None,
            )?;
            if let Some(p) = self.peers.get_mut(&owner) {
                let away = (Vec3::from(p.player.state().feet) - center).normalize_or_zero();
                p.player.push((away + Vec3::Y * 0.5) * amount * 0.2);
            }
        }
        let hit_entities: Vec<(u64, f32)> = self
            .packages
            .as_ref()
            .map(|h| {
                h.entities
                    .iter()
                    .filter_map(|(id, e)| {
                        let d = Vec3::from(e.body.state().feet).distance(center);
                        (d < radius).then(|| (*id, damage * (1.0 - d / radius)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (id, amount) in hit_entities {
            self.damage_entity(id, amount, None, "package", source);
        }
        let (origin, tick) = (source.to_string(), self.simulation.state().tick);
        let can_destroy = self
            .packages
            .as_mut()
            .is_some_and(|h| h.shares.edits.available(&origin, tick) >= 1);
        if brick_radius > 0.0 && can_destroy {
            let reach = Vec3::splat(brick_radius);
            let mut hit: Vec<(f32, BrickId)> = self
                .simulation
                .bricks_in_box(center - reach, center + reach)
                .into_iter()
                .filter_map(|id| {
                    let (min, max) = self.simulation.brick_box(id)?;
                    let d = center.clamp(min, max).distance(center);
                    (d <= brick_radius).then_some((d, id))
                })
                .collect();
            hit.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let blast = super::debris::BrickBlast {
                origin: center,
                force: 20.0,
                radius: brick_radius,
            };
            for (_, brick) in hit.into_iter().take(MAX_BLAST_BRICKS) {
                let spent = self
                    .packages
                    .as_mut()
                    .is_none_or(|h| h.shares.edits.available(&origin, tick) < 1);
                if spent {
                    break;
                }
                // Indestructible bricks and materials simply survive.
                let _ = self.package_remove_brick(source, brick, Some(blast), caller);
            }
        }
        let tick = self.simulation.state().tick;
        self.cues.emit(
            tick,
            crate::presentation::CueKind::Explosion {
                radius,
                source: source.into(),
            },
            center.to_array(),
        );
        Ok(())
    }
    /// Spawn a package entity with a character body, lifting it until it
    /// fits. Returns its id.
    pub fn spawn_package_entity(&mut self, kind: &str, position: Vec3) -> Result<u64> {
        let host = self.packages.as_ref().context("No packages are enabled")?;
        let (package, def) = host
            .catalog
            .entity(kind)
            .with_context(|| format!("Unknown entity kind `{kind}`"))?;
        let alive = host.entities.values().filter(|e| e.kind == kind).count();
        ensure!(
            alive < def.max_alive as usize,
            "{} already has {alive} of at most {} alive",
            def.name,
            def.max_alive
        );
        ensure!(
            host.entities.len() < MAX_ENTITIES,
            "Too many package entities"
        );
        // One package may not take every slot: each other package that
        // declares entities keeps a reserve (W1).
        let others = host
            .catalog
            .packages
            .values()
            .filter(|p| p.id() != package.id() && !p.entities.is_empty())
            .count();
        let mine = host
            .entities
            .values()
            .filter(|e| e.package == package.id())
            .count();
        ensure!(
            mine < MAX_ENTITIES - ENTITY_RESERVE * others,
            "`{}` has {mine} entities, its share of the server's {MAX_ENTITIES}",
            package.id()
        );
        let id = host.next_entity;
        let (package, def) = (package.id().to_string(), def.clone());
        let tuning = match &def.archetype {
            Some(archetype) => {
                let archetype = self
                    .archetypes
                    .find(archetype)
                    .with_context(|| format!("{}: no archetype {archetype}", def.name))?;
                self.archetypes.tuning(archetype, def.scale)
            }
            None => PlayerTuning::default().scaled(def.scale),
        };
        let mut body = None;
        // Lift the spawn out of the ground: packages rarely know its height.
        for lift in 0..32 {
            let feet = position + Vec3::Y * (lift as f32);
            if let Ok(b) = Player::spawn_tagged(
                &mut self.simulation.physics,
                id,
                ENTITY_TAG | u128::from(id),
                feet,
                tuning.clone(),
            ) {
                body = Some(b);
                break;
            }
        }
        let body = body.with_context(|| format!("No room to spawn {} there", def.name))?;
        let tick = self.simulation.state().tick;
        let host = self.packages.as_mut().expect("checked");
        host.next_entity += 1;
        host.entities.insert(
            id,
            Entity {
                kind: kind.into(),
                package,
                model: def.model.clone(),
                body,
                scale: def.scale,
                health: def.health,
                label: String::new(),
                steer: (Vec3::ZERO, false),
                vars: BTreeMap::new(),
                think: def.think.clone(),
                interval: u64::from(def.think_interval),
                speed: def.speed.min(1.0),
                next_think: tick,
                drive: None,
            },
        );
        Ok(id)
    }
    fn remove_package_entity(&mut self, id: u64) {
        if let Some(e) = self.packages.as_mut().and_then(|h| h.entities.remove(&id)) {
            e.body.despawn(&mut self.simulation.physics);
        }
        self.release_entity(id);
    }
    /// Players driving a gone entity return to their own bodies.
    fn release_entity(&mut self, id: u64) {
        let drivers: Vec<OwnerId> = self
            .peers
            .iter()
            .filter(|(_, p)| p.control == ControlObject::Entity(id))
            .map(|(owner, _)| *owner)
            .collect();
        for owner in drivers {
            let _ = self.return_to_body(owner);
        }
    }
    /// This tick's moves of players driving package entities. An entity no
    /// player drives any more (its driver died, left or was handed back
    /// their body) returns to its think's steering.
    pub(super) fn drive_package_entities(&mut self, moves: Vec<(u64, MoveInput)>) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        let driven: BTreeSet<u64> = self
            .peers
            .values()
            .filter_map(|p| match p.control {
                ControlObject::Entity(id) => Some(id),
                _ => None,
            })
            .collect();
        for (id, e) in host.entities.iter_mut() {
            if !driven.contains(id) {
                e.drive = None;
            }
        }
        for (id, input) in moves {
            if let Some(e) = host.entities.get_mut(&id)
                && driven.contains(&id)
            {
                e.drive = Some(input);
            }
        }
    }

    /// A client asked to run a package command.
    /// A command typed in chat (`/sell stone`) names no package, like any
    /// other slash command: the host finds the one package that declares it
    /// and reads each word as that command's argument type, a final string
    /// taking the rest of the line. HUD keys name their package already.
    fn resolve_typed_command(&self, request: PackageCommand) -> Result<PackageCommand> {
        if !request.package.is_empty() {
            return Ok(request);
        }
        let unknown = || anyhow::anyhow!("Unknown command: /{}", request.command);
        let host = self.packages.as_ref().ok_or_else(unknown)?;
        let mut declaring = host.catalog.packages.iter().filter_map(|(id, p)| {
            let def = p
                .behaviour
                .as_ref()?
                .commands
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(&request.command))?;
            Some((id, def))
        });
        let (package, def) = declaring.next().ok_or_else(unknown)?;
        ensure!(
            declaring.next().is_none(),
            "More than one Add-On declares /{}",
            request.command
        );
        let words: Vec<&str> = request
            .args
            .iter()
            .map(|a| match a {
                PackageArg::String(word) => Ok(word.as_str()),
                _ => Err(anyhow::anyhow!("Typed commands carry words")),
            })
            .collect::<Result<_>>()?;
        let mut args = Vec::with_capacity(def.args.len());
        for (i, kind) in def.args.iter().enumerate() {
            let Some(word) = words.get(i) else { break };
            let last = i + 1 == def.args.len();
            args.push(match kind {
                ArgType::String if last => PackageArg::String(words[i..].join(" ")),
                ArgType::Int => word
                    .parse()
                    .map_or_else(|_| PackageArg::String((*word).into()), PackageArg::Int),
                ArgType::Float => word
                    .parse()
                    .map_or_else(|_| PackageArg::String((*word).into()), PackageArg::Float),
                ArgType::Bool => match word.to_ascii_lowercase().as_str() {
                    "true" | "yes" | "on" | "1" => PackageArg::Bool(true),
                    "false" | "no" | "off" | "0" => PackageArg::Bool(false),
                    _ => PackageArg::String((*word).into()),
                },
                ArgType::String => PackageArg::String((*word).into()),
            });
        }
        // Extra words make the count differ, which the command check refuses.
        if words.len() > def.args.len() && def.args.last() != Some(&ArgType::String) {
            args.extend(
                words[def.args.len()..]
                    .iter()
                    .map(|w| PackageArg::String((*w).into())),
            );
        }
        Ok(PackageCommand {
            package: package.clone(),
            command: def.name.clone(),
            args,
        })
    }

    pub(super) fn package_command(
        &mut self,
        owner: OwnerId,
        request: PackageCommand,
        direction: Vec3,
    ) -> Result<Reply> {
        self.run_command(owner, request, direction, false)
    }

    /// A command, typed or sent by a HUD or client (`from_image` false), or
    /// run by the held image (a state, jet, light or cancel command).
    pub(super) fn run_command(
        &mut self,
        owner: OwnerId,
        request: PackageCommand,
        direction: Vec3,
        from_image: bool,
    ) -> Result<Reply> {
        // `serverCmdBrickCount`: anyone may ask how many bricks the server
        // has, unless an Add-On declares its own /brickCount.
        let typed_brick_count =
            request.package.is_empty() && request.command.eq_ignore_ascii_case("brickcount");
        // `ServerCmdClearBricks`: likewise anyone may clear their own bricks.
        let typed_clear_bricks =
            request.package.is_empty() && request.command.eq_ignore_ascii_case("clearbricks");
        // `serverCmdCancelEvents`: a player stops their own events.
        let typed_cancel_events =
            request.package.is_empty() && request.command.eq_ignore_ascii_case("cancelevents");
        // `serverCmdTripOut`: an administrator's joke.
        let typed_trip_out =
            request.package.is_empty() && request.command.eq_ignore_ascii_case("tripout");
        let request = match self.resolve_typed_command(request) {
            Ok(request) => request,
            Err(_) if typed_brick_count => {
                self.brick_count(owner);
                return Ok(Reply::Accepted);
            }
            Err(_) if typed_clear_bricks => {
                self.clear_own_bricks(owner)?;
                return Ok(Reply::Accepted);
            }
            Err(_) if typed_cancel_events => {
                self.cancel_own_events(owner)?;
                return Ok(Reply::Accepted);
            }
            Err(_) if typed_trip_out => {
                self.trip_out(owner)?;
                return Ok(Reply::Accepted);
            }
            Err(error) => return Err(error),
        };
        let host = self
            .packages
            .as_ref()
            .context("This server runs no packages")?;
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        let reject = |code: &str, message: String| {
            diagnostics_error(vec![
                Diagnostic::error(code, message).at(request.package.clone()),
            ])
        };
        let behaviour = host
            .catalog
            .packages
            .get(&request.package)
            .and_then(|p| p.behaviour.as_ref())
            .ok_or_else(|| {
                reject(
                    "command.package",
                    format!("The server does not run package `{}`", request.package),
                )
            })?;
        let def = behaviour
            .commands
            .iter()
            .find(|c| c.name == request.command)
            .ok_or_else(|| {
                reject(
                    "command.unknown",
                    format!(
                        "`{}` declares no command `{}`",
                        request.package, request.command
                    ),
                )
            })?;
        // A tool's command runs from its image. The mouse wheel's command
        // comes from the client as a command like any other, so the held
        // image's wheel command is let through too.
        if def.tool_only
            && !from_image
            && !self
                .weapons
                .image_state(bri_weapons::ActorId(owner), 0)
                .and_then(|(image, _)| image.commands.wheel.as_deref())
                .is_some_and(|wheel| {
                    wheel.split_once(':') == Some((&request.package, &request.command))
                })
        {
            return Err(reject(
                "command.tool_only",
                format!("`{}` is run by its tool, not typed", request.command),
            ));
        }
        if def.admin && !peer.actor.administrator {
            return Err(reject(
                "command.admin",
                format!("`{}` is for administrators", request.command),
            ));
        }
        if !def.while_dead && !peer.combat.alive {
            return Err(reject(
                "command.dead",
                format!("Dead players cannot use `{}`", request.command),
            ));
        }
        if request.args.len() != def.args.len()
            || !request
                .args
                .iter()
                .zip(&def.args)
                .all(|(a, t)| a.matches(*t))
        {
            return Err(reject(
                "command.args",
                format!("`{}` takes ({:?})", request.command, def.args),
            ));
        }
        if request.args.iter().any(|a| {
            matches!(a, PackageArg::String(s) if s.len() > 256 || s.chars().any(char::is_control))
                || matches!(a, PackageArg::Float(f) if !f.is_finite())
        }) {
            return Err(reject("command.args", "Invalid command argument".into()));
        }
        let tick = self.simulation.state().tick;
        let key = (
            self.player_key(owner),
            request.package.clone(),
            request.command.clone(),
        );
        if host.cooldowns.get(&key).is_some_and(|until| tick < *until) {
            return Err(reject(
                "command.cooldown",
                format!("`{}` is cooling down", request.command),
            ));
        }
        let aim = match def.aim_reach {
            Some(reach) => {
                let eye = peer.player.eye();
                let hit = self.simulation.target(eye, direction, reach)?;
                // Also the nearest movable object before the brick, reported
                // beside it: a script aiming at bricks sees what it did.
                let object = self
                    .aim_object(
                        owner,
                        eye,
                        direction,
                        hit.as_ref().map_or(reach, |h| h.distance),
                    )
                    .filter(|(_, _, d)| hit.as_ref().is_none_or(|h| *d < h.distance))
                    .map(|(object, at, distance)| script::AimObject {
                        object,
                        position: at.to_array(),
                        distance,
                        movable: self.may_move(owner, object),
                    });
                match hit {
                    Some(hit) => Some(script::Aim {
                        look: hit
                            .brick
                            .and_then(|b| self.simulation.state().bricks.get(&b)?.look.clone())
                            .map(|l| (l.block, l.state)),
                        tag: hit.brick.and_then(|b| {
                            let world = host.world.as_ref()?;
                            world
                                .voxels
                                .get(&b)
                                .map(|v| world.def.materials[v.material].id.clone())
                        }),
                        brick: hit.brick,
                        position: hit.position.to_array(),
                        distance: hit.distance,
                        object,
                    }),
                    None => object.map(|o| script::Aim {
                        brick: None,
                        tag: None,
                        look: None,
                        position: o.position,
                        distance: o.distance,
                        object: Some(o),
                    }),
                }
            }
            None => None,
        };
        let player = key.0.clone();
        let cooldown = u64::from(def.cooldown_ticks);
        let mut args = vec![Dynamic::from_int(owner as i64)];
        args.extend(request.args.iter().map(PackageArg::dynamic));
        let function = format!("cmd_{}", request.command);
        // Each player's commands draw on their own share of script work.
        if let Some(host) = self.packages.as_mut()
            && host.shares.commands.available(&player, tick) <= 0
        {
            return Err(reject(
                "command.busy",
                format!(
                    "Your commands used their share of server time; `{}` is refused for now",
                    request.command
                ),
            ));
        }
        if cooldown > 0
            && let Some(host) = self.packages.as_mut()
        {
            if host.cooldowns.len() >= MAX_COOLDOWNS {
                host.cooldowns.retain(|_, until| *until > tick);
            }
            host.cooldowns.insert(key, tick + cooldown);
        }
        let result = self.run_package(
            &request.package,
            &function,
            args,
            Budget::Command,
            Some(owner),
            aim,
            None,
        );
        if let Some(host) = self.packages.as_mut() {
            let used = host.runtime.last_operations() as i64;
            host.shares.commands.spend(&player, tick, used);
        }
        let _ = result.map_err(|d| diagnostics_error(vec![d]))?;
        Ok(Reply::Accepted)
    }

    /// Package work for one tick: entity thinking and movement, world
    /// streaming around players, and `on_tick` hooks.
    pub(super) fn step_packages(&mut self) -> Result<()> {
        self.deliver_deaths();
        self.deliver_loadouts();
        self.deliver_spawns();
        self.deliver_hits();
        let changed = self.dirty.read(super::dirty::Reader::Packages);
        let Some(host) = self.packages.as_ref() else {
            return Ok(());
        };
        let tick = self.simulation.state().tick;
        // Bricks removed by other means (hammer, wand) are world edits too.
        let gone: Vec<BrickId> = changed
            .iter()
            .filter(|id| {
                host.world
                    .as_ref()
                    .is_some_and(|w| w.voxels.contains_key(*id))
                    && !self.simulation.state().bricks.contains_key(*id)
            })
            .copied()
            .collect();
        for id in gone {
            self.forget_voxel(id);
        }
        let host = self.packages.as_mut().expect("checked");
        let hooks: Vec<String> = host
            .catalog
            .behaviours()
            .filter(|(_, b)| {
                b.tick_interval
                    .is_some_and(|i| tick > 0 && tick.is_multiple_of(u64::from(i)))
            })
            .map(|(id, _)| id.clone())
            .collect();
        host.hooks_due.extend(hooks);
        // Longest-waiting first, so thinks a package's share could not fit
        // this tick go first next tick and no entity starves.
        let mut due: Vec<(u64, u64, String, String)> = host
            .entities
            .iter()
            .filter(|(_, e)| tick >= e.next_think)
            .map(|(id, e)| (e.next_think, *id, e.package.clone(), e.think.clone()))
            .collect();
        due.sort_unstable_by_key(|d| (d.0, d.1));
        let view = self.tick_view();
        let snapshot = view.snapshot.clone();
        self.packages.as_mut().expect("checked").view = Some(view);
        for (_, id, package, think) in due {
            // Past its share this tick, a package's thinks wait (and yield to
            // its due hook); they are not dropped.
            let host = self.packages.as_mut().expect("checked");
            if host.hooks_due.contains(&package) || host.shares.work.available(&package, tick) <= 0
            {
                continue;
            }
            let Ok(at) = snapshot.entities.binary_search_by_key(&id, |e| e.id) else {
                continue;
            };
            let view = &snapshot.entities[at];
            if let Some(e) = self.packages.as_mut().and_then(|h| h.entities.get_mut(&id)) {
                e.next_think = tick + e.interval;
            }
            let result = self.run_package(
                &package,
                &think,
                vec![script::entity_map(view)],
                Budget::Think,
                None,
                None,
                Some(id),
            );
            self.charge_work(&package);
            if result.is_err() {
                // A broken think stops the entity rather than spamming errors.
                if let Some(e) = self.packages.as_mut().and_then(|h| h.entities.get_mut(&id)) {
                    e.steer = (Vec3::ZERO, false);
                    e.next_think = tick + 120;
                }
            }
        }
        self.packages.as_mut().expect("checked").view = None;
        let liquids = self.simulation.liquids();
        let mut fallen = Vec::new();
        if let Some(host) = self.packages.as_mut() {
            for (id, e) in host.entities.iter_mut() {
                if let Some(input) = e.drive {
                    let _ = self.simulation.step_body(&mut e.body, input, &liquids);
                    if e.body.state().feet[1] < KILL_Y {
                        fallen.push(*id);
                    }
                    continue;
                }
                let (direction, jump) = e.steer;
                let flat = Vec3::new(direction.x, 0.0, direction.z);
                let moving = flat.length_squared() > 1e-6;
                let yaw = if moving {
                    flat.x.atan2(-flat.z)
                } else {
                    e.body.state().yaw
                };
                let input = MoveInput {
                    forward: if moving { e.speed } else { 0.0 },
                    yaw: (yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI,
                    jump,
                    ..Default::default()
                };
                let _ = self.simulation.step_body(&mut e.body, input, &liquids);
                if e.body.state().feet[1] < KILL_Y {
                    fallen.push(*id);
                }
            }
        }
        for id in fallen {
            self.remove_package_entity(id);
        }
        // Stream chunks in around players, nearest first.
        let wanted: Vec<(i64, i64)> = {
            let host = self.packages.as_ref().expect("checked");
            match &host.world {
                Some(world) => {
                    let view = i64::from(world.def.view_chunks);
                    let mut wanted = Vec::new();
                    for p in self.peers.values() {
                        let (cx, cz) = world.chunk_of(Vec3::from(p.player.state().feet));
                        for dx in -view..=view {
                            for dz in -view..=view {
                                let c = (cx + dx, cz + dz);
                                if world.in_bounds(c) && !world.chunks.contains(&c) {
                                    wanted.push((dx.abs().max(dz.abs()), c));
                                }
                            }
                        }
                    }
                    wanted.sort();
                    wanted.dedup();
                    wanted
                        .into_iter()
                        .map(|(_, c)| c)
                        .take(CHUNKS_PER_TICK)
                        .collect()
                }
                None => Vec::new(),
            }
        };
        for chunk in wanted {
            // A failing generator is reported; the chunk stays empty.
            let _ = self.generate_chunk(chunk);
        }
        // Due hooks wait for their package's share of script work; a hook
        // that failed stays paused, as a failing think does.
        let pending: Vec<String> = self
            .packages
            .as_ref()
            .expect("checked")
            .hooks_due
            .iter()
            .cloned()
            .collect();
        for package in pending {
            let host = self.packages.as_mut().expect("checked");
            if host
                .hooks_paused
                .get(&package)
                .is_some_and(|until| tick < *until)
            {
                host.hooks_due.remove(&package);
            } else if host.shares.work.available(&package, tick) > 0 {
                host.hooks_due.remove(&package);
                let result = self.run_package(
                    &package,
                    "on_tick",
                    Vec::new(),
                    Budget::Tick,
                    None,
                    None,
                    None,
                );
                self.charge_work(&package);
                let host = self.packages.as_mut().expect("checked");
                if result.is_err() {
                    host.hooks_paused.insert(package, tick + FAILURE_PAUSE);
                } else {
                    host.hooks_paused.remove(&package);
                }
            }
        }
        Ok(())
    }

    /// Ask every package that declares `policy` whether `owner` may go
    /// ahead: the engine owns the mechanism, packages the rule. A package
    /// whose policy call fails is reported and does not block the game.
    pub(super) fn package_policy(&mut self, policy: &str, owner: OwnerId) -> Result<()> {
        let Some(host) = self.packages.as_ref() else {
            return Ok(());
        };
        if self.bots.is_bot(owner) {
            return Ok(());
        }
        let asked: Vec<String> = host
            .catalog
            .behaviours()
            .filter(|(_, b)| b.policies.iter().any(|p| p == policy))
            .map(|(id, _)| id.clone())
            .collect();
        let function = format!("allow_{policy}");
        for package in asked {
            let answer = self.run_package(
                &package,
                &function,
                vec![Dynamic::from_int(owner as i64)],
                Budget::Command,
                Some(owner),
                None,
                None,
            );
            self.charge_work(&package);
            let Ok(answer) = answer else {
                continue;
            };
            if let Some(allowed) = answer.clone().try_cast::<bool>() {
                ensure!(allowed, "`{package}` does not allow that now");
            } else if let Some(reason) = answer
                .clone()
                .try_cast::<bri_package_runtime::rhai::ImmutableString>()
            {
                anyhow::bail!("{reason}");
            } else {
                let host = self.packages.as_mut().expect("checked");
                note(
                    host,
                    Diagnostic::warning(
                        "policy.answer",
                        format!(
                            "{function} must return true, false or a reason, not {}",
                            answer.type_name()
                        ),
                    )
                    .at(package.clone()),
                );
            }
        }
        Ok(())
    }
    /// Charge the last script call to its package's share of server work.
    fn charge_work(&mut self, package: &str) {
        let tick = self.simulation.state().tick;
        if let Some(host) = self.packages.as_mut() {
            let used = host.runtime.last_operations() as i64;
            host.shares.work.spend(&package.to_string(), tick, used);
        }
    }
    /// Note a death for packages' `on_death` hooks.
    pub(super) fn package_death(&mut self, victim: OwnerId, killer: Option<OwnerId>) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        if host.deaths.len() == MAX_PENDING_DEATHS {
            note(
                host,
                Diagnostic::warning("hook.dropped", "Too many deaths in one tick for on_death"),
            );
            return;
        }
        host.deaths.push_back((victim, killer));
    }
    /// `on_death(victim, killer)` for every death since the last tick, in
    /// order. Deaths the hooks cause are delivered next tick, so a hook can
    /// never recurse.
    /// A player's items were set afresh: `on_loadout` hooks hear of it next
    /// tick.
    pub(super) fn package_loadout(&mut self, owner: OwnerId) {
        if let Some(host) = self.packages.as_mut()
            && !self.bots.is_bot(owner)
            && host.loadouts.len() < 1024
            && !host.loadouts.contains(&owner)
        {
            host.loadouts.push_back(owner);
        }
    }
    fn deliver_loadouts(&mut self) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        let owners = std::mem::take(&mut host.loadouts);
        self.deliver_player_hook(owners, |b| b.on_loadout, "on_loadout");
    }
    /// A player came to life (joined, respawned): `on_spawn` hooks hear of
    /// it next tick, after `on_loadout`.
    pub(super) fn package_spawn(&mut self, owner: OwnerId) {
        if let Some(host) = self.packages.as_mut()
            && !self.bots.is_bot(owner)
            && host.spawns.len() < 1024
            && !host.spawns.contains(&owner)
        {
            host.spawns.push_back(owner);
        }
    }
    fn deliver_spawns(&mut self) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        let owners = std::mem::take(&mut host.spawns);
        self.deliver_player_hook(owners, |b| b.on_spawn, "on_spawn");
    }
    /// `on_leave(player)` as `owner` leaves, while they are still readable.
    pub(super) fn package_leave(&mut self, owner: OwnerId) {
        if self.bots.is_bot(owner) {
            return;
        }
        self.deliver_player_hook([owner].into(), |b| b.on_leave, "on_leave");
    }
    /// `on_damage(victim, attacker, amount, info)` from every package that
    /// declares it, in order, each seeing the amount the one before
    /// returned. A failed call leaves the amount as it was.
    pub(super) fn package_damage(
        &mut self,
        victim: OwnerId,
        attacker: Option<OwnerId>,
        amount: f32,
        kind: &combat::DamageKind,
    ) -> f32 {
        let Some(host) = self.packages.as_mut() else {
            return amount;
        };
        if host.in_damage_hook {
            return amount;
        }
        let hooks: Vec<String> = host
            .catalog
            .behaviours()
            .filter(|(_, b)| b.on_damage)
            .map(|(id, _)| id.clone())
            .collect();
        if hooks.is_empty() {
            return amount;
        }
        host.in_damage_hook = true;
        let mut info = bri_package_runtime::rhai::Map::new();
        info.insert("kind".into(), kind.hook_kind().into());
        info.insert("type".into(), kind.type_name().to_string().into());
        info.insert("direct".into(), kind.direct().into());
        let mut amount = amount;
        for package in hooks {
            let answer = self.run_package(
                &package,
                "on_damage",
                vec![
                    Dynamic::from_int(victim as i64),
                    attacker.map_or(Dynamic::UNIT, |a| Dynamic::from_int(a as i64)),
                    Dynamic::from_float(f64::from(amount)),
                    Dynamic::from_map(info.clone()),
                ],
                Budget::Command,
                None,
                None,
                None,
            );
            self.charge_work(&package);
            if let Ok(answer) = answer {
                amount = self.hook_amount(&package, "on_damage", &answer, amount);
            }
        }
        if let Some(host) = self.packages.as_mut() {
            host.in_damage_hook = false;
        }
        amount
    }
    /// A damage hook's answer: a number replaces `amount` (clamped to 0 to
    /// 100000), `()` keeps it, anything else keeps it with a warning.
    fn hook_amount(&mut self, package: &str, hook: &str, answer: &Dynamic, amount: f32) -> f32 {
        let number = answer
            .as_float()
            .ok()
            .or_else(|| answer.as_int().ok().map(|i| i as f64));
        match number {
            Some(n) if n.is_finite() => (n as f32).clamp(0.0, 100_000.0),
            Some(_) => amount,
            None if answer.is_unit() => amount,
            None => {
                if let Some(host) = self.packages.as_mut() {
                    note(
                        host,
                        Diagnostic::warning(
                            "hook.answer",
                            format!(
                                "{hook} must return a number or (), not {}",
                                answer.type_name()
                            ),
                        )
                        .at(package.to_string()),
                    );
                }
                amount
            }
        }
    }
    /// Hurt a package entity: a shot, a blast or a package's `explode`.
    /// Its own package decides first (`on_entity_damage`), and hears of its
    /// death (`on_entity_death`) while it can still be read, before it goes.
    /// `kind` is the hook's `info.kind` (`weapon` or `package`) and `name`
    /// the damage type or the package responsible.
    pub(super) fn damage_entity(
        &mut self,
        id: u64,
        amount: f32,
        attacker: Option<OwnerId>,
        kind: &str,
        name: &str,
    ) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        let Some(entity) = host.entities.get(&id) else {
            return;
        };
        if !(amount.is_finite() && amount > 0.0) {
            return;
        }
        let package = entity.package.clone();
        let behaviour = host
            .catalog
            .behaviours()
            .find(|(p, _)| **p == package)
            .map(|(_, b)| (b.on_entity_damage, b.on_entity_death));
        let (asks, hears) = behaviour.unwrap_or((false, false));
        let nested = host.in_damage_hook;
        let mut info = bri_package_runtime::rhai::Map::new();
        info.insert("kind".into(), kind.into());
        info.insert("type".into(), name.into());
        let attacker_arg = || attacker.map_or(Dynamic::UNIT, |a| Dynamic::from_int(a as i64));
        let mut amount = amount.min(100_000.0);
        // Damage the hook's own operations cause is not asked about again.
        if asks && !nested {
            self.packages.as_mut().expect("checked").in_damage_hook = true;
            let answer = self.run_package(
                &package,
                "on_entity_damage",
                vec![
                    Dynamic::from_int(id as i64),
                    attacker_arg(),
                    Dynamic::from_float(f64::from(amount)),
                    Dynamic::from_map(info.clone()),
                ],
                Budget::Command,
                None,
                None,
                Some(id),
            );
            self.charge_work(&package);
            if let Ok(answer) = answer {
                amount = self.hook_amount(&package, "on_entity_damage", &answer, amount);
            }
            if let Some(host) = self.packages.as_mut() {
                host.in_damage_hook = false;
            }
        }
        let Some(entity) = self.packages.as_mut().and_then(|h| h.entities.get_mut(&id)) else {
            return; // the hook removed it
        };
        if amount <= 0.0 {
            return;
        }
        entity.health -= amount;
        if entity.health > 0.0 {
            return;
        }
        if hears {
            let _ = self.run_package(
                &package,
                "on_entity_death",
                vec![
                    Dynamic::from_int(id as i64),
                    attacker_arg(),
                    Dynamic::from_map(info),
                ],
                Budget::Command,
                None,
                None,
                Some(id),
            );
            self.charge_work(&package);
        }
        self.remove_package_entity(id);
    }
    /// A shot's or blast's push on a package entity, as on a player.
    pub(super) fn push_entity(&mut self, id: u64, impulse: Vec3) {
        if let Some(e) = self.packages.as_mut().and_then(|h| h.entities.get_mut(&id))
            && impulse.is_finite()
        {
            e.body.push(impulse / combat::PLAYER_MASS);
        }
    }
    /// Run a one-player hook (`on_loadout`, `on_spawn`, `on_leave`) of every
    /// package whose behaviour `declares` it, for each of `owners`.
    fn deliver_player_hook(
        &mut self,
        owners: VecDeque<OwnerId>,
        declares: fn(&bri_package_runtime::content::Behaviour) -> bool,
        function: &str,
    ) {
        let Some(host) = self.packages.as_ref() else {
            return;
        };
        let hooks: Vec<String> = host
            .catalog
            .behaviours()
            .filter(|(_, b)| declares(b))
            .map(|(id, _)| id.clone())
            .collect();
        if hooks.is_empty() {
            return;
        }
        for owner in owners {
            if !self.peers.contains_key(&owner) {
                continue;
            }
            for package in &hooks {
                let _ = self.run_package(
                    package,
                    function,
                    vec![Dynamic::from_int(owner as i64)],
                    Budget::Command,
                    None,
                    None,
                    None,
                );
                self.charge_work(package);
            }
        }
    }
    fn deliver_deaths(&mut self) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        let deaths = std::mem::take(&mut host.deaths);
        let hooks: Vec<String> = host
            .catalog
            .behaviours()
            .filter(|(_, b)| b.on_death)
            .map(|(id, _)| id.clone())
            .collect();
        for (victim, killer) in deaths {
            for package in &hooks {
                let _ = self.run_package(
                    package,
                    "on_death",
                    vec![
                        Dynamic::from_int(victim as i64),
                        killer.map_or(Dynamic::UNIT, |k| Dynamic::from_int(k as i64)),
                    ],
                    Budget::Command,
                    None,
                    None,
                    None,
                );
                self.charge_work(package);
            }
        }
    }

    pub fn packages_enabled(&self) -> bool {
        self.packages.is_some()
    }
    /// Each package entity's variables (tests, tools).
    pub fn package_entity_vars(&self) -> Vec<(u64, BTreeMap<String, serde_json::Value>)> {
        self.packages
            .as_ref()
            .map(|h| {
                h.entities
                    .iter()
                    .map(|(id, e)| (*id, e.vars.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn package_entities(&self) -> Vec<EntityInfo> {
        self.packages
            .as_ref()
            .map(|h| {
                h.entities
                    .iter()
                    .map(|(id, e)| EntityInfo {
                        id: *id,
                        kind: e.kind.clone(),
                        model: e.model.clone(),
                        position: e.body.state().feet,
                        yaw: e.body.state().yaw,
                        scale: e.scale,
                        label: e.label.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    /// Package state every client receives: keys visible to everyone.
    pub fn package_state(&self) -> PackageStateView {
        self.package_view(None)
    }
    /// Package state one client receives: keys visible to everyone, plus
    /// that player's own keys visible to their owner.
    pub fn package_state_for(&self, viewer: OwnerId) -> PackageStateView {
        self.package_view(Some(viewer))
    }
    fn package_view(&self, viewer: Option<OwnerId>) -> PackageStateView {
        let Some(host) = self.packages.as_ref() else {
            return PackageStateView::default();
        };
        let mut view = PackageStateView::default();
        for (id, behaviour) in host.catalog.behaviours() {
            // Every running package has a namespace, even before it stores
            // anything: clients read the keys as "this server runs it" and
            // show an Add-On's HUD only then.
            let Some(ns) = host.store.namespace(id) else {
                view.packages.insert(id.clone(), NamespaceView::default());
                continue;
            };
            let public_global: BTreeMap<_, _> = ns
                .global
                .iter()
                .filter(|(k, _)| {
                    behaviour
                        .state
                        .global
                        .get(*k)
                        .is_some_and(|d| d.visible == Visible::Everyone)
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let mut players = BTreeMap::new();
            for owner in self.peers.keys().filter(|o| !self.bots.is_bot(**o)) {
                if let Some(values) = ns.players.get(&self.player_key(*owner)) {
                    let public: BTreeMap<_, _> = values
                        .iter()
                        .filter(|(k, _)| {
                            behaviour.state.player.get(*k).is_some_and(|d| {
                                d.visible == Visible::Everyone
                                    || (d.visible == Visible::Owner && viewer == Some(*owner))
                            })
                        })
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    if !public.is_empty() {
                        players.insert(*owner, public);
                    }
                }
            }
            view.packages.insert(
                id.clone(),
                NamespaceView {
                    global: public_global,
                    players,
                },
            );
        }
        view
    }
    /// The server-side value of a player's package state (tests, tools).
    pub fn package_value(
        &self,
        package: &str,
        owner: OwnerId,
        key: &str,
    ) -> Option<serde_json::Value> {
        self.packages
            .as_ref()?
            .store
            .player(package, &self.player_key(owner), key)
            .cloned()
    }
    /// Everything a host saves to resume this package world.
    pub fn package_save(&self) -> Option<PackageSave> {
        let host = self.packages.as_ref()?;
        Some(PackageSave {
            schema_version: PACKAGE_SAVE_SCHEMA,
            store: host.store.persistent(&host.catalog),
            world: host.world.as_ref().map(|w| WorldSave {
                provider: w.provider.clone(),
                seed: w.seed,
                removed: w.removed.clone(),
                added: w.added.iter().fold(BTreeMap::new(), |mut out, (p, m)| {
                    out.entry(w.def.materials[*m].id.clone())
                        .or_insert_with(Vec::new)
                        .push(*p);
                    out
                }),
            }),
        })
    }
    /// Recent package problems, newest last.
    pub fn package_diagnostics(&self) -> Vec<Diagnostic> {
        self.packages
            .as_ref()
            .map(|h| h.diagnostics.iter().cloned().collect())
            .unwrap_or_default()
    }
    /// Counts for tools and soak reports.
    pub fn package_stats(&self) -> PackageStats {
        let Some(host) = self.packages.as_ref() else {
            return PackageStats::default();
        };
        PackageStats {
            entities: host.entities.len(),
            chunks: host.world.as_ref().map_or(0, |w| w.chunks.len()),
            voxels: host.world.as_ref().map_or(0, |w| w.voxels.len()),
            removed_voxels: host.world.as_ref().map_or(0, |w| w.removed.len()),
            diagnostics: host.diagnostics.len(),
        }
    }
    /// Script time per package since the last call, then start again. Only
    /// packages whose scripts ran appear.
    pub fn take_package_script_time(&mut self) -> BTreeMap<String, std::time::Duration> {
        self.packages
            .as_mut()
            .map(|h| std::mem::take(&mut h.script_time))
            .unwrap_or_default()
    }
    /// The generated voxel a brick draws, as (voxel, material id).
    pub fn package_voxel(&self, brick: BrickId) -> Option<([i64; 3], String)> {
        let world = self.packages.as_ref()?.world.as_ref()?;
        let v = world.voxels.get(&brick)?;
        Some((v.position, world.def.materials[v.material].id.clone()))
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageStats {
    pub entities: usize,
    pub chunks: usize,
    pub voxels: usize,
    pub removed_voxels: usize,
    pub diagnostics: usize,
}
