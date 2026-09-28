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
    content::{ArgType, ChunkWorld},
    ops::{Op, authorize},
    script::{self, Budget, Call, EntityView, PlayerView, Runtime, Snapshot},
    state::Namespace,
};
use bri_world::MAX_BRICKS;
use std::sync::Arc;

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
    pub label: String,
}
impl EntityInfo {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.kind.len() <= 128
                && self.model.len() <= 128
                && self.label.len() <= 32
                && self.position.iter().all(|v| v.is_finite())
                && self.yaw.is_finite(),
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
        let ns = self.packages.get(&binding.package)?;
        if binding.player {
            ns.players.get(&viewer)?.get(&binding.key)
        } else {
            ns.global.get(&binding.key)
        }
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
            save.world
                .as_ref()
                .is_none_or(|w| w.removed.len() <= MAX_BRICKS * 4),
            "Oversized world edits"
        );
        Ok(save)
    }
}

struct Entity {
    kind: String,
    package: String,
    model: String,
    body: Player,
    health: f32,
    label: String,
    steer: (Vec3, bool),
    vars: BTreeMap<String, serde_json::Value>,
    think: String,
    interval: u64,
    speed: f32,
    next_think: u64,
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
}

pub(super) struct PackageHost {
    catalog: Arc<Catalog>,
    runtime: Runtime,
    store: Store,
    world: Option<GeneratedWorld>,
    entities: BTreeMap<u64, Entity>,
    next_entity: u64,
    cooldowns: BTreeMap<(OwnerId, String, String), u64>,
    diagnostics: VecDeque<Diagnostic>,
    output: VecDeque<String>,
}

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
                let removed = save
                    .world
                    .filter(|w| &w.provider == provider)
                    .map(|w| w.removed)
                    .unwrap_or_default();
                Some(self.prepare_world(package.id(), provider, def, seed, removed)?)
            }
            None => None,
        };
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
                entity_vars: BTreeMap::new(),
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
            for [x, y, z, m] in voxels {
                let position = [x, y, z];
                // Voxels belong to the chunk that generated them.
                if (x.div_euclid(n), z.div_euclid(n)) != chunk
                    || world.removed.contains(&position)
                    || !seen.insert(position)
                {
                    continue;
                }
                let mut brick = Brick::new(
                    ContentRef::Resolved(world.def.voxel_brick.clone()),
                    world.center(position),
                    0,
                );
                brick.color = world.colors[m as usize];
                bricks.push(brick);
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
    fn package_snapshot(&self) -> Snapshot {
        let host = self.packages.as_ref();
        Snapshot {
            tick: self.simulation.state().tick,
            seed: host.and_then(|h| h.world.as_ref()).map_or(0, |w| w.seed),
            players: self
                .peers
                .iter()
                .filter(|(o, _)| !self.bots.is_bot(**o))
                .map(|(owner, p)| PlayerView {
                    id: *owner,
                    key: self.player_key(*owner),
                    name: p.name.clone(),
                    position: p.player.state().feet,
                    alive: p.combat.alive,
                    admin: p.actor.administrator,
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
    ) -> std::result::Result<(), Diagnostic> {
        let snapshot = Arc::new(self.package_snapshot());
        let Some(host) = self.packages.as_mut() else {
            return Err(Diagnostic::error("package.none", "No packages are enabled"));
        };
        let state = host.store.namespace(package).cloned().unwrap_or_default();
        let entity_vars = host
            .entities
            .iter()
            .filter(|(_, e)| e.package == package)
            .map(|(id, e)| (*id, e.vars.clone()))
            .collect();
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
        };
        let outcome = match host.runtime.call(package, call) {
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
        // State may only use declared keys.
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
            .keys()
            .find(|k| !schema.global.contains_key(*k))
            .or_else(|| {
                outcome
                    .state
                    .players
                    .values()
                    .flat_map(|m| m.keys())
                    .find(|k| !schema.player.contains_key(*k))
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
        *host.store.namespace_mut(package) = outcome.state;
        for (id, vars) in outcome.entity_vars {
            if let Some(e) = host.entities.get_mut(&id) {
                e.vars = vars;
            }
        }
        for op in outcome.ops {
            if let Err(error) = self.apply_package_op(package, op) {
                let host = self.packages.as_mut().expect("installed");
                note(
                    host,
                    Diagnostic::warning("op.failed", format!("{error:#}")).at(package.to_string()),
                );
            }
        }
        Ok(())
    }
    fn apply_package_op(&mut self, package: &str, op: Op) -> Result<()> {
        let tick = self.simulation.state().tick;
        match op {
            Op::RemoveBrick { brick } => self.package_remove_brick(brick, None),
            Op::Explode {
                position,
                radius,
                damage,
                brick_radius,
            } => self.explode(Vec3::from(position), radius, damage, brick_radius, package),
            Op::DamagePlayer { player, amount } => self.damage_player(
                player,
                amount,
                combat::DamageKind::Package {
                    name: package.into(),
                },
                None,
            ),
            Op::SpawnEntity { kind, position } => self
                .spawn_package_entity(&kind, Vec3::from(position))
                .map(|_| ()),
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
                self.notify(player, Notice::Chat(text));
                Ok(())
            }
            Op::Broadcast { text } => {
                let _ = tick;
                self.system_chat(text);
                Ok(())
            }
        }
    }
    /// Remove a brick for good, recording generated voxels as world edits.
    fn package_remove_brick(
        &mut self,
        brick: BrickId,
        blast: Option<super::debris::BrickBlast>,
    ) -> Result<()> {
        let b = self
            .simulation
            .state()
            .bricks
            .get(&brick)
            .context("No such brick")?;
        let definition = self.simulation.definitions.get(b)?;
        ensure!(
            !definition.indestructible && !b.base_plate,
            "Brick {brick} is indestructible"
        );
        let center = Vec3::from(b.position);
        if let Some(world) = self.packages.as_ref().and_then(|h| h.world.as_ref())
            && let Some(voxel) = world.voxels.get(&brick)
        {
            ensure!(
                !world.def.materials[voxel.material].indestructible,
                "{} cannot be removed",
                world.def.materials[voxel.material].name
            );
        }
        let admin = Actor {
            owner: 0,
            administrator: true,
            ..Default::default()
        };
        self.kill_brick(
            &admin,
            brick,
            blast.unwrap_or_else(|| super::debris::BrickBlast::pop(center)),
        )?;
        self.forget_voxel(brick);
        Ok(())
    }
    fn forget_voxel(&mut self, brick: BrickId) {
        if let Some(world) = self.packages.as_mut().and_then(|h| h.world.as_mut())
            && let Some(voxel) = world.voxels.remove(&brick)
        {
            world.removed.insert(voxel.position);
        }
    }
    /// The one explosion operation: damage players within `radius` (full at
    /// the centre, none at the edge), damage package entities the same way,
    /// and destroy bricks within `brick_radius`.
    pub fn explode(
        &mut self,
        center: Vec3,
        radius: f32,
        damage: f32,
        brick_radius: f32,
        source: &str,
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
            if let Some(e) = self.packages.as_mut().and_then(|h| h.entities.get_mut(&id)) {
                e.health -= amount;
                if e.health <= 0.0 {
                    self.remove_package_entity(id);
                }
            }
        }
        if brick_radius > 0.0 {
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
                // Indestructible bricks and materials simply survive.
                let _ = self.package_remove_brick(brick, Some(blast));
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
        ensure!(host.entities.len() < 1024, "Too many package entities");
        let id = host.next_entity;
        let (package, def) = (package.id().to_string(), def.clone());
        let tuning = PlayerTuning::default().scaled(def.scale);
        let mut body = None;
        for lift in 0..8 {
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
                health: def.health,
                label: String::new(),
                steer: (Vec3::ZERO, false),
                vars: BTreeMap::new(),
                think: def.think.clone(),
                interval: u64::from(def.think_interval),
                speed: def.speed.min(1.0),
                next_think: tick,
            },
        );
        Ok(id)
    }
    fn remove_package_entity(&mut self, id: u64) {
        if let Some(e) = self.packages.as_mut().and_then(|h| h.entities.remove(&id)) {
            e.body.despawn(&mut self.simulation.physics);
        }
    }

    /// A client asked to run a package command.
    pub(super) fn package_command(
        &mut self,
        owner: OwnerId,
        request: PackageCommand,
        direction: Vec3,
    ) -> Result<Reply> {
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
        let key = (owner, request.package.clone(), request.command.clone());
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
                hit.map(|hit| script::Aim {
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
                })
            }
            None => None,
        };
        let cooldown = u64::from(def.cooldown_ticks);
        let mut args = vec![Dynamic::from_int(owner as i64)];
        args.extend(request.args.iter().map(PackageArg::dynamic));
        let function = format!("cmd_{}", request.command);
        if cooldown > 0
            && let Some(host) = self.packages.as_mut()
        {
            host.cooldowns.insert(key, tick + cooldown);
        }
        self.run_package(
            &request.package,
            &function,
            args,
            Budget::Command,
            Some(owner),
            aim,
            None,
        )
        .map_err(|d| diagnostics_error(vec![d]))?;
        Ok(Reply::Accepted)
    }

    /// Package work for one tick: entity thinking and movement, world
    /// streaming around players, and `on_tick` hooks.
    pub(super) fn step_packages(&mut self) -> Result<()> {
        let Some(host) = self.packages.as_ref() else {
            return Ok(());
        };
        let tick = self.simulation.state().tick;
        // Bricks removed by other means (hammer, wand) are world edits too.
        let gone: Vec<BrickId> = self
            .dirty
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
        let host = self.packages.as_ref().expect("checked");
        let due: Vec<(u64, String, String)> = host
            .entities
            .iter()
            .filter(|(_, e)| tick >= e.next_think)
            .map(|(id, e)| (*id, e.package.clone(), e.think.clone()))
            .collect();
        for (id, package, think) in due {
            let Some(view) = self
                .package_snapshot()
                .entities
                .into_iter()
                .find(|e| e.id == id)
            else {
                continue;
            };
            if let Some(e) = self.packages.as_mut().and_then(|h| h.entities.get_mut(&id)) {
                e.next_think = tick + e.interval;
            }
            if self
                .run_package(
                    &package,
                    &think,
                    vec![script::entity_map(&view)],
                    Budget::Think,
                    None,
                    None,
                    Some(id),
                )
                .is_err()
            {
                // A broken think stops the entity rather than spamming errors.
                if let Some(e) = self.packages.as_mut().and_then(|h| h.entities.get_mut(&id)) {
                    e.steer = (Vec3::ZERO, false);
                    e.next_think = tick + 120;
                }
            }
        }
        let liquids = self.simulation.liquids();
        let mut fallen = Vec::new();
        if let Some(host) = self.packages.as_mut() {
            for (id, e) in host.entities.iter_mut() {
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
                let _ = e
                    .body
                    .step_in_water(&mut self.simulation.physics, input, &liquids);
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
        let hooks: Vec<String> = self
            .packages
            .as_ref()
            .expect("checked")
            .catalog
            .behaviours()
            .filter(|(_, b)| {
                b.tick_interval
                    .is_some_and(|i| tick > 0 && tick.is_multiple_of(u64::from(i)))
            })
            .map(|(id, _)| id.clone())
            .collect();
        for package in hooks {
            let _ = self.run_package(
                &package,
                "on_tick",
                Vec::new(),
                Budget::Tick,
                None,
                None,
                None,
            );
        }
        Ok(())
    }

    pub fn packages_enabled(&self) -> bool {
        self.packages.is_some()
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
                        label: e.label.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    /// Public package state for clients.
    pub fn package_state(&self) -> PackageStateView {
        let Some(host) = self.packages.as_ref() else {
            return PackageStateView::default();
        };
        let mut view = PackageStateView::default();
        for (id, behaviour) in host.catalog.behaviours() {
            let Some(ns) = host.store.namespace(id) else {
                continue;
            };
            let public_global: BTreeMap<_, _> = ns
                .global
                .iter()
                .filter(|(k, _)| behaviour.state.global.get(*k).is_some_and(|d| d.public))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let mut players = BTreeMap::new();
            for owner in self.peers.keys().filter(|o| !self.bots.is_bot(**o)) {
                if let Some(values) = ns.players.get(&self.player_key(*owner)) {
                    let public: BTreeMap<_, _> = values
                        .iter()
                        .filter(|(k, _)| behaviour.state.player.get(*k).is_some_and(|d| d.public))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    if !public.is_empty() {
                        players.insert(*owner, public);
                    }
                }
            }
            if !public_global.is_empty() || !players.is_empty() {
                view.packages.insert(
                    id.clone(),
                    NamespaceView {
                        global: public_global,
                        players,
                    },
                );
            }
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
