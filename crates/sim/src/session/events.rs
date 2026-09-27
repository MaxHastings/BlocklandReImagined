//! Wrench events: the vanilla input/output system executed by `bri-events`.
//!
//! The session installs every named or evented brick as an event program,
//! fires inputs from gameplay (activation, touch, projectile hits, respawns)
//! and applies the engine's intents to bricks, players, clients and
//! minigames under the host's permission rules.
use super::*;
use bri_events::{
    self as ev, Apply, BrickOp, Class, ClientOp, Dispatch, Entity, EventWorld, Id, Intent,
    MessageKind, MiniGameOp, PlayerOp, Slot, Trigger,
};
use bri_minigames as mg;
use bri_weapons::ActorId;

/// Print IDs `printCountUp` and friends display (Letters, digits 0-9).
const DIGIT_PRINTS: &str = "print/print_letters_default/";
/// The only player datablock; `changeDatablock` accepts it as a no-op.
const PLAYER_DATA: &str = "PlayerStandardArmor";
const TICKS_PER_SECOND: u64 = 120;

fn id(index: u64) -> Id {
    Id {
        index,
        generation: 1,
    }
}
fn entity(class: Class, index: u64) -> Entity {
    Entity {
        class,
        id: id(index),
    }
}

#[derive(Default)]
pub(super) struct Events {
    world: Option<EventWorld>,
    bindings: ev::Bindings,
    sounds: BTreeSet<String>,
    installed: BTreeSet<BrickId>,
    scanned: bool,
    origin: u64,
    /// Projectile outputs of zero-delay `onProjectileHit` rows, applied by the
    /// weapon runtime at the moment of contact.
    pub(super) projectile_responses: BTreeMap<BrickId, bri_weapons::ContactResponse>,
    /// Bricks killed with `fakeKillBrick` and the tick they come back.
    pub(super) respawns: BTreeMap<BrickId, u64>,
    diagnostics: VecDeque<String>,
}

fn note(queue: &mut VecDeque<String>, text: String) {
    if queue.len() == 64 {
        queue.pop_front();
    }
    queue.push_back(text);
}

impl Session {
    /// Install the vanilla event catalog. Datablock parameters are bound to
    /// the content this session serves (tool catalog, weapon and vehicle
    /// packs) and follow later changes to it. `sounds` lists the sound IDs
    /// `playSound` may use.
    pub fn set_event_catalog(
        &mut self,
        catalog: ev::Catalog,
        sounds: impl IntoIterator<Item = String>,
    ) -> Result<()> {
        self.events.sounds = sounds.into_iter().collect();
        self.install_event_world(catalog)
    }
    /// Rebind datablocks after the server's content catalogs change.
    pub(super) fn refresh_event_bindings(&mut self) -> Result<()> {
        match self.events.world.as_ref().map(|w| w.catalog().clone()) {
            Some(catalog) => self.install_event_world(catalog),
            None => Ok(()),
        }
    }
    fn install_event_world(&mut self, catalog: ev::Catalog) -> Result<()> {
        // Pending delayed events are dropped; this happens during setup.
        let tools = &self.tool_catalog;
        let mut datablocks = BTreeMap::new();
        datablocks.insert("ItemData".into(), tools.items.clone());
        datablocks.insert("FxLightData".into(), tools.lights.clone());
        datablocks.insert("ParticleEmitterData".into(), tools.emitters.clone());
        datablocks.insert("Music".into(), tools.sounds.clone());
        datablocks.insert("Vehicle".into(), tools.vehicles.clone());
        datablocks.insert(
            "ProjectileData".into(),
            self.weapons.pack.projectiles.keys().cloned().collect(),
        );
        datablocks.insert("PlayerData".into(), BTreeSet::from([PLAYER_DATA.into()]));
        datablocks.insert("Sound".into(), self.events.sounds.clone());
        let bindings = ev::Bindings {
            palette_len: self.simulation.state().palette.len(),
            datablocks,
        };
        let world = EventWorld::new(catalog, bindings.clone(), ev::Limits::default())?;
        self.events = Events {
            world: Some(world),
            bindings,
            sounds: std::mem::take(&mut self.events.sounds),
            ..Default::default()
        };
        Ok(())
    }
    pub fn event_catalog(&self) -> Option<&ev::Catalog> {
        self.events.world.as_ref().map(EventWorld::catalog)
    }
    /// Datablock choices for the event editor, by parameter class.
    pub fn event_datablocks(&self) -> &BTreeMap<String, BTreeSet<String>> {
        &self.events.bindings.datablocks
    }
    /// Cancel every pending event job (delayed rows and chains in flight).
    pub(super) fn cancel_all_events(&mut self) {
        if let Some(world) = self.events.world.as_mut() {
            for brick in &self.events.installed {
                world.cancel_source(id(*brick), ev::CancelMode::All);
            }
        }
    }
    /// `GameConnection::ClearEventSchedules`: cancel the owner's pending events.
    pub(super) fn cancel_owner_events(&mut self, owner: OwnerId) {
        let bricks = &self.simulation.state().bricks;
        if let Some(world) = self.events.world.as_mut() {
            for brick in &self.events.installed {
                if bricks.get(brick).is_some_and(|b| b.owner == owner) {
                    world.cancel_source(id(*brick), ev::CancelMode::All);
                }
            }
        }
    }
    pub fn take_event_diagnostics(&mut self) -> Vec<String> {
        self.events.diagnostics.drain(..).collect()
    }
    /// Reject rows the engine cannot run before they reach the world.
    pub(super) fn validate_event_rows(&self, rows: &[ev::Row]) -> Result<()> {
        let Some(world) = &self.events.world else {
            anyhow::bail!("Events are not available on this server");
        };
        for (index, row) in rows.iter().enumerate() {
            world
                .catalog()
                .validate_row(row, &self.events.bindings)
                .with_context(|| format!("Event line {}", index + 1))?;
        }
        Ok(())
    }
    fn install_program(&mut self, brick_id: BrickId) {
        let Some(world) = self.events.world.as_mut() else {
            return;
        };
        let brick = self
            .simulation
            .state()
            .bricks
            .get(&brick_id)
            .filter(|b| !b.events.is_empty() || b.name.is_some());
        let Some(brick) = brick else {
            if self.events.installed.remove(&brick_id) {
                world.remove_brick(id(brick_id));
            }
            self.events.projectile_responses.remove(&brick_id);
            return;
        };
        // `Explode` is the default collision; the others change it.
        let response = brick.events.iter().find_map(|row| {
            use bri_weapons::ContactResponse as R;
            let immediate = row.enabled
                && row.preserved.is_none()
                && row.delay_ms == 0
                && row.input.eq_ignore_ascii_case("onProjectileHit")
                && row.target == ev::Target::Slot(Slot::Projectile);
            if !immediate {
                return None;
            }
            match (row.output.to_ascii_lowercase().as_str(), row.params.as_slice()) {
                ("delete", _) => Some(R::Delete),
                ("bounce", [ev::Value::Float(f)]) => Some(R::Bounce(*f)),
                ("redirect", [ev::Value::Vector(v), ev::Value::Bool(n)]) => Some(R::Redirect {
                    vector: *v,
                    normalized: *n,
                }),
                _ => None,
            }
        });
        match response {
            Some(response) => self.events.projectile_responses.insert(brick_id, response),
            None => self.events.projectile_responses.remove(&brick_id),
        };
        let print_count = match &brick.print {
            Some(bri_world::ContentRef::Resolved(print)) => print
                .strip_prefix(DIGIT_PRINTS)
                .and_then(|d| d.parse::<u8>().ok())
                .filter(|d| *d < 10)
                .unwrap_or(0),
            _ => 0,
        };
        // A row this server cannot run (for example one naming content it
        // does not have) is kept in the world but disabled in the engine.
        let rows = brick
            .events
            .iter()
            .map(
                |row| match world.catalog().validate_row(row, &self.events.bindings) {
                    Ok(_) => row.clone(),
                    Err(error) => {
                        note(
                            &mut self.events.diagnostics,
                            format!(
                                "Brick {brick_id} {} -> {}: {error:#}",
                                row.input, row.output
                            ),
                        );
                        ev::Row {
                            preserved: Some(ev::PreservedRow {
                                original: format!("{} -> {}", row.input, row.output),
                                diagnostic: format!("{error:#}").chars().take(1000).collect(),
                            }),
                            enabled: false,
                            input: String::new(),
                            delay_ms: 0,
                            target: ev::Target::Slot(Slot::SelfBrick),
                            output: String::new(),
                            params: vec![],
                        }
                    }
                },
            )
            .collect();
        let program = ev::BrickProgram {
            id: id(brick_id),
            owner_scope: brick.owner,
            name: brick.name.clone(),
            rows,
            print_count,
            implicit_cancel_relays: false,
        };
        match world.install_brick(program) {
            Ok(()) => {
                self.events.installed.insert(brick_id);
            }
            Err(error) => {
                if self.events.installed.remove(&brick_id) {
                    world.remove_brick(id(brick_id));
                }
                note(
                    &mut self.events.diagnostics,
                    format!("Brick {brick_id} events disabled: {error:#}"),
                );
            }
        }
    }
    /// Keep engine programs in step with changed bricks.
    fn sync_event_programs(&mut self, changed: &BTreeSet<BrickId>) {
        if self.events.world.is_none() {
            return;
        }
        let ids: Vec<BrickId> = if self.events.scanned {
            changed.iter().copied().collect()
        } else {
            self.events.scanned = true;
            self.simulation.state().bricks.keys().copied().collect()
        };
        for brick in ids {
            self.install_program(brick);
        }
    }
    /// Fire an input on a brick. `player` supplies the Player/Bot, Client
    /// and MiniGame targets the input exposes.
    pub(super) fn fire_input(&mut self, brick: BrickId, input: &str, player: Option<OwnerId>) {
        // Bricks edited since the last event phase run their new program.
        if self.dirty.contains(&brick) || !self.events.scanned {
            self.install_program(brick);
        }
        if !self.events.installed.contains(&brick) {
            return;
        }
        let Some(definition) = self
            .events
            .world
            .as_ref()
            .and_then(|w| w.catalog().input(input))
        else {
            return;
        };
        let slots: BTreeSet<Slot> = definition
            .targets
            .iter()
            .filter_map(|(slot, _)| Slot::parse(slot))
            .collect();
        self.events.origin += 1;
        let mut trigger = Trigger::new(id(brick), input, self.events.origin);
        if let Some(owner) = player.filter(|o| self.peers.contains_key(o)) {
            let bot = self.is_bot(owner);
            let body = if bot { Slot::Bot } else { Slot::Player };
            if slots.contains(&body) {
                trigger.targets.insert(body, entity(Class::Player, owner));
            }
            if !bot && slots.contains(&Slot::Client) {
                trigger
                    .targets
                    .insert(Slot::Client, entity(Class::Client, owner));
                trigger.client = Some(entity(Class::Client, owner));
            }
            let brick_owner = self.simulation.state().bricks.get(&brick).map(|b| b.owner);
            let game = ev::semantics::minigame_target(
                false,
                brick_owner
                    .and_then(|o| self.game_of(o))
                    .map(|g| entity(Class::MiniGame, g.0)),
                self.game_of(owner).map(|g| entity(Class::MiniGame, g.0)),
            );
            if let Some(game) = game.filter(|_| slots.contains(&Slot::MiniGame)) {
                trigger.targets.insert(Slot::MiniGame, game);
            }
        }
        let world = self.events.world.as_mut().unwrap();
        if let Err(error) = world.trigger(trigger) {
            note(
                &mut self.events.diagnostics,
                format!("Brick {brick} {input}: {error:#}"),
            );
        }
    }
    /// One event phase per tick, after gameplay has fired this tick's inputs.
    pub(super) fn step_events(&mut self, changed: &BTreeSet<BrickId>) -> Result<()> {
        self.sync_event_programs(changed);
        let tick = self.simulation.state().tick;
        let due: Vec<BrickId> = self
            .events
            .respawns
            .iter()
            .filter(|(_, at)| **at <= tick)
            .map(|(id, _)| *id)
            .collect();
        for brick in due {
            self.events.respawns.remove(&brick);
            self.respawn_brick(brick)?;
        }
        let Some(mut world) = self.events.world.take() else {
            return Ok(());
        };
        let report = ev::migration::world_tick_to_us(tick)
            .and_then(|now| world.advance(now, &mut EventHost { session: self }));
        let result = report.map(|report| {
            // Toggled rows are part of the brick's saved state.
            for program in &report.changed_programs {
                if let Some(rows) = world.program(*program).map(|p| p.rows.clone()) {
                    let _ = self.simulation.mutate(program.index, |b| {
                        for (row, updated) in b.events.iter_mut().zip(rows) {
                            row.enabled = updated.enabled;
                        }
                    });
                    self.dirty.insert(program.index);
                }
            }
            for text in report.diagnostics {
                note(&mut self.events.diagnostics, text);
            }
        });
        self.events.world = Some(world);
        result
    }
    fn respawn_brick(&mut self, brick: BrickId) -> Result<()> {
        if !self.simulation.state().bricks.contains_key(&brick) {
            return Ok(());
        }
        self.simulation.mutate(brick, |b| {
            b.visible = true;
            b.raycast = true;
            b.colliding = true;
        })?;
        self.dirty.insert(brick);
        self.fire_input(brick, "onRespawn", None);
        Ok(())
    }
    /// Fire a brick input from host tooling (admin commands, probes).
    pub fn fire_brick_input(&mut self, brick: BrickId, input: &str, player: Option<OwnerId>) {
        self.fire_input(brick, input, player);
    }
    /// Event rows waiting in the engine (delayed and chained events).
    pub fn pending_events(&self) -> usize {
        self.events.world.as_ref().map_or(0, EventWorld::pending)
    }
    /// Explosions and heavy hits knock small bricks out under v20's brick
    /// damage rules (`fxDTSBrick::onBlownUp`). They come back after the
    /// minigame's brick respawn time.
    pub(super) fn blow_up_bricks(
        &mut self,
        source: OwnerId,
        target: Option<BrickId>,
        position: Vec3,
        impact: &bri_weapons::BrickImpact,
    ) -> Result<()> {
        let Some(player) = self.peers.get(&source).map(|p| p.combat.player) else {
            return Ok(());
        };
        let Ok(damage) = self.minigames.projectile_source(player) else {
            return Ok(());
        };
        let mut hit: Vec<BrickId> = Vec::new();
        if impact.direct
            && let Some(brick) = target
        {
            hit.push(brick);
        }
        if impact.radius > 0.0 {
            let reach = Vec3::splat(impact.radius);
            hit.extend(
                self.simulation
                    .bricks_in_box(position - reach, position + reach)
                    .into_iter()
                    .filter(|id| {
                        self.simulation.brick_box(*id).is_some_and(|(min, max)| {
                            position.clamp(min, max).distance(position) <= impact.radius
                        })
                    }),
            );
        }
        hit.sort_unstable();
        hit.dedup();
        let game = self.game_of(source);
        let delay = self
            .minigames
            .respawn_delay(game, mg::RespawnObject::Brick)
            .unwrap_or(3600);
        for brick in hit.into_iter().take(64) {
            let Some(b) = self.simulation.state().bricks.get(&brick) else {
                continue;
            };
            let Ok(definition) = self.simulation.definitions.get(b) else {
                continue;
            };
            let mesh = &definition.mesh;
            let volume = (mesh.footprint_studs[0] * mesh.footprint_studs[1]) as f32
                * mesh.height_plates as f32;
            if self.events.respawns.contains_key(&brick)
                || !b.colliding
                || b.base_plate
                || definition.indestructible
                || volume > impact.max_volume
            {
                continue;
            }
            // v20 `ProjectileData::onExplode`: single-player and LAN hosts
            // ($Server::LAN) only ask the shooter's minigame for brick damage;
            // internet servers use miniGameCanDamage, or ownership outside
            // minigames.
            let allowed = match game {
                Some(g) if self.lan_host => self
                    .minigames
                    .game(g)
                    .is_ok_and(|g| g.settings.brick_damage),
                None if self.lan_host => true,
                Some(_) => {
                    let target = mg::Target::Object {
                        kind: mg::ObjectKind::Brick,
                        owner: Some(mg::AccountId(b.owner)),
                        membership: mg::Membership::Owner,
                        spawn_brick: false,
                    };
                    self.minigames.can_radius_damage(damage, target) == mg::Decision::Allow
                }
                None => b.owner == source,
            };
            if !allowed {
                continue;
            }
            // v20 throws direct hits with a 0.02 falloff radius.
            let blast = super::debris::BrickBlast {
                origin: position,
                force: impact.force,
                radius: if impact.radius > 0.0 {
                    impact.radius
                } else {
                    0.02
                },
            };
            self.fake_kill_brick(brick, blast, delay)?;
            self.fire_input(brick, "onBlownUp", Some(source));
        }
        Ok(())
    }
    /// Bricks a player's touch (contact entry) reached this tick. Special
    /// bricks (checkpoints, teledoors) act first and may consume the touch.
    pub(super) fn fire_touches(&mut self, touches: Vec<(OwnerId, BrickId)>) {
        for (owner, brick) in touches {
            match self.special_touch(owner, brick) {
                Ok(true) => {}
                Ok(false) => self.fire_touch_events(owner, brick),
                Err(error) => note(
                    &mut self.events.diagnostics,
                    format!("Brick {brick} touch: {error:#}"),
                ),
            }
        }
    }
    pub(super) fn fire_touch_events(&mut self, owner: OwnerId, brick: BrickId) {
        let input = if self.is_bot(owner) {
            "onBotTouch"
        } else {
            "onPlayerTouch"
        };
        self.fire_input(brick, input, Some(owner));
    }
}

struct EventHost<'a> {
    session: &'a mut Session,
}

fn message(kind: MessageKind, text: String, seconds: u32) -> Notice {
    let seconds = seconds.clamp(1, 10) as f32;
    match kind {
        MessageKind::Chat => Notice::Chat(text),
        MessageKind::Center => Notice::Center { text, seconds },
        MessageKind::Bottom => Notice::Bottom { text, seconds },
    }
}
fn direction_index(direction: ev::Direction) -> u8 {
    match direction {
        ev::Direction::Up => 0,
        ev::Direction::Down => 1,
        ev::Direction::North => 2,
        ev::Direction::East => 3,
        ev::Direction::South => 4,
        ev::Direction::West => 5,
    }
}
/// Outputs that hurt or disadvantage a player need a shared minigame.
fn harmful(output: &str) -> bool {
    matches!(
        output.to_ascii_lowercase().as_str(),
        "kill"
            | "addhealth"
            | "sethealth"
            | "burnplayer"
            | "cleartools"
            | "instantrespawn"
            | "spawnexplosion"
            | "spawnprojectile"
            | "changedatablock"
            | "setplayerscale"
            | "incscore"
    )
}

impl EventHost<'_> {
    fn edit(&mut self, brick: BrickId, change: impl FnOnce(&mut Brick)) -> Result<()> {
        self.session.simulation.mutate(brick, change)?;
        self.session.dirty.insert(brick);
        Ok(())
    }
    /// Projectiles an event spawns belong to whoever set the event off,
    /// otherwise to the brick's owner.
    fn instigator(&self, d: &Dispatch) -> OwnerId {
        d.client
            .map(|c| c.id.index)
            .or_else(|| {
                self.session
                    .simulation
                    .state()
                    .bricks
                    .get(&d.source.index)
                    .map(|b| b.owner)
            })
            .unwrap_or(0)
    }
    fn spawn_projectile(
        &mut self,
        d: &Dispatch,
        projectile: &str,
        at: Vec3,
        velocity: Vec3,
        scale: f32,
    ) {
        let source = ActorId(self.instigator(d));
        if let Err(error) =
            self.session
                .weapons
                .spawn(projectile, source, at, velocity, scale.clamp(0.1, 10.0))
        {
            note(
                &mut self.session.events.diagnostics,
                format!("Event projectile {projectile}: {error:#}"),
            );
        }
    }
    fn random3(&mut self) -> [f32; 3] {
        let seed = &mut self.session.spawn_seed;
        std::array::from_fn(|_| {
            *seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (*seed >> 40) as f32 / (1u64 << 24) as f32
        })
    }
    fn brick_op(&mut self, d: &Dispatch, op: &BrickOp) -> Result<Apply> {
        let brick = d.target.id.index;
        let tick = self.session.simulation.state().tick;
        let Some((min, max)) = self.session.simulation.brick_box(brick) else {
            return Ok(Apply::Rejected("brick is gone".into()));
        };
        let center = (min + max) * 0.5;
        match op {
            BrickOp::Color(c) => self.edit(brick, |b| b.color = *c)?,
            BrickOp::ColorFx(c) => self.edit(brick, |b| b.color_effect = (*c).min(6))?,
            BrickOp::ShapeFx(c) => self.edit(brick, |b| b.shape_effect = (*c).min(2))?,
            BrickOp::Colliding(v) => self.edit(brick, |b| b.colliding = *v)?,
            BrickOp::Rendering(v) => self.edit(brick, |b| b.visible = *v)?,
            BrickOp::RayCasting(v) => self.edit(brick, |b| b.raycast = *v)?,
            BrickOp::Presence {
                rendering,
                ray_casting,
                colliding,
                revive_fake_dead,
            } => {
                if *revive_fake_dead {
                    self.session.events.respawns.remove(&brick);
                }
                self.edit(brick, |b| {
                    b.visible = *rendering;
                    b.raycast = *ray_casting;
                    b.colliding = *colliding;
                })?
            }
            // The engine turns `disappear` into Presence changes.
            BrickOp::Disappear { .. } => {}
            BrickOp::FakeKill { velocity, seconds } => {
                let delay = u64::from((*seconds).clamp(1, 300)) * TICKS_PER_SECOND;
                let blast = super::debris::BrickBlast::fake_kill(center, *velocity);
                self.session.fake_kill_brick(brick, blast, delay)?;
            }
            BrickOp::Respawn => {
                self.session.events.respawns.remove(&brick);
                self.session.respawn_brick(brick)?;
            }
            BrickOp::Emitter(emitter) => self.edit(brick, |b| {
                let direction = b.emitter.as_ref().map_or(0, |e| e.direction);
                b.emitter = emitter.clone().map(|asset| bri_world::Emitter {
                    asset: Some(bri_world::ContentRef::Resolved(asset)),
                    direction,
                });
            })?,
            BrickOp::EmitterDirection(direction) => {
                let direction = direction_index(*direction);
                self.edit(brick, |b| {
                    if let Some(e) = &mut b.emitter {
                        e.direction = direction;
                    }
                })?
            }
            BrickOp::Light(light) => self.edit(brick, |b| {
                b.light = light.clone().map(|asset| bri_world::Light {
                    asset: bri_world::ContentRef::Resolved(asset),
                    enabled: true,
                })
            })?,
            BrickOp::Item(item) => self.edit(brick, |b| {
                b.item_spawn.item = item.clone().map(bri_world::ContentRef::Resolved)
            })?,
            BrickOp::ItemDirection(direction) => {
                let direction = direction_index(*direction).clamp(2, 5);
                self.edit(brick, |b| b.item_spawn.direction = direction)?
            }
            BrickOp::ItemPosition(direction) => {
                let position = direction_index(*direction);
                self.edit(brick, |b| b.item_spawn.position = position)?
            }
            BrickOp::Music(music) => self.edit(brick, |b| {
                b.sound = music.clone().map(bri_world::ContentRef::Resolved)
            })?,
            BrickOp::Vehicle(vehicle) => self.edit(brick, |b| {
                let recolor = b.vehicle.as_ref().is_some_and(|v| v.recolor);
                b.vehicle = vehicle.clone().map(|id| bri_world::VehicleSpawn {
                    vehicle: bri_world::ContentRef::Resolved(id),
                    recolor,
                })
            })?,
            BrickOp::RespawnVehicle | BrickOp::RecoverVehicle => {
                self.session.respawn_vehicle_brick(brick)?
            }
            // `fxDTSBrick::playSound` is silent while the brick is fake-dead.
            BrickOp::PlaySound(sound) => {
                if let Some(profile) = sound.clone()
                    && !self.session.events.respawns.contains_key(&brick)
                {
                    self.session.cues.emit(
                        tick,
                        crate::presentation::CueKind::WeaponSound { profile },
                        center.to_array(),
                    );
                }
            }
            BrickOp::PrintDigit(digit) => {
                let print = format!("{DIGIT_PRINTS}{digit}");
                self.edit(brick, |b| {
                    b.print = Some(bri_world::ContentRef::Resolved(print))
                })?
            }
            BrickOp::SpawnProjectile {
                velocity,
                projectile,
                variance,
                scale,
            } => {
                if let Some(projectile) = projectile {
                    let random = self.random3();
                    let at = ev::semantics::brick_projectile_position(min, max, random);
                    let random = self.random3();
                    let velocity = ev::semantics::projectile_velocity(*velocity, *variance, random);
                    self.spawn_projectile(d, projectile, at, velocity, *scale);
                }
            }
            BrickOp::SpawnExplosion { projectile, scale } => {
                if let Some(projectile) = projectile {
                    self.spawn_projectile(d, projectile, center, Vec3::ZERO, *scale);
                }
            }
            BrickOp::SpawnItem { item, velocity } => {
                let Some(item) = item else {
                    return Ok(Apply::Applied);
                };
                self.session
                    .spawn_event_item(item, center + Vec3::Y * 0.5, *velocity)?;
            }
            BrickOp::RadiusImpulse {
                radius,
                force,
                vertical_force,
            } => {
                let owners: Vec<_> = self.session.peers.keys().copied().collect();
                for owner in owners {
                    let feet = Vec3::from(self.session.peers[&owner].player.state().feet);
                    let body = feet + Vec3::Y;
                    if body.distance(center) <= *radius {
                        let push = ev::semantics::radius_impulse(
                            center,
                            body,
                            *radius,
                            *force,
                            *vertical_force,
                        );
                        if let Some(peer) = self.session.peers.get_mut(&owner) {
                            peer.player.push(push);
                        }
                    }
                }
            }
        }
        Ok(Apply::Applied)
    }
    fn player_op(&mut self, d: &Dispatch, op: &PlayerOp) -> Result<Apply> {
        let owner = d.target.id.index;
        if !self.session.is_alive(owner) {
            return Ok(Apply::Rejected("player is not alive".into()));
        }
        match op {
            PlayerOp::Kill => self.session.kill(owner, None, combat::DamageKind::Event)?,
            PlayerOp::SetVelocity(v) => {
                let peer = self.session.peers.get_mut(&owner).unwrap();
                let current = Vec3::from(peer.player.state().velocity);
                peer.player.push(*v - current);
            }
            PlayerOp::AddVelocity(v) => self.session.peers.get_mut(&owner).unwrap().player.push(*v),
            PlayerOp::AddHealth(amount) => {
                let damage = MAX_HEALTH - self.session.peers[&owner].combat.health;
                let change = ev::semantics::add_health(MAX_HEALTH, damage, *amount);
                self.session.change_health(owner, change)?
            }
            PlayerOp::SetHealth(amount) => {
                let damage = MAX_HEALTH - self.session.peers[&owner].combat.health;
                let change = ev::semantics::set_health(MAX_HEALTH, damage, *amount);
                self.session.change_health(owner, change)?
            }
            PlayerOp::ClearTools => {
                self.session
                    .weapons
                    .set_inventory(ActorId(owner), &vec![None; TOOL_SLOTS])?;
                self.session.weapon_triggers.remove(&owner);
            }
            PlayerOp::InstantRespawn => {
                let player = self.session.peers[&owner].combat.player;
                let effects = self
                    .session
                    .minigames
                    .event_respawn(player)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                self.session.apply_minigame_effects(effects)?;
            }
            PlayerOp::Dismount => self.session.eject(owner),
            PlayerOp::SpawnProjectile {
                speed,
                projectile,
                variance,
                scale,
            } => {
                if let Some(projectile) = projectile {
                    let peer = &self.session.peers[&owner];
                    let eye = peer.player.eye();
                    let forward = peer.player.state().forward();
                    let random = self.random3();
                    let velocity =
                        ev::semantics::projectile_velocity(forward * *speed, *variance, random);
                    self.spawn_projectile(d, projectile, eye + forward, velocity, *scale);
                }
            }
            PlayerOp::SpawnExplosion { projectile, scale } => {
                if let Some(projectile) = projectile {
                    let feet = Vec3::from(self.session.peers[&owner].player.state().feet);
                    self.spawn_projectile(d, projectile, feet + Vec3::Y, Vec3::ZERO, *scale);
                }
            }
            // One player type exists.
            PlayerOp::DataBlock(_) => {}
            // `Player::BurnPlayer`/`clearBurn`: PlayerBurnImage flames for the
            // given seconds; clearing ends them at once.
            PlayerOp::Burn { seconds } => self.burn(owner, *seconds as f32),
            PlayerOp::ClearBurn => self.burn(owner, 0.0),
            PlayerOp::Scale(_) => {
                return Ok(Apply::Rejected(
                    "player scaling is not available yet".into(),
                ));
            }
        }
        Ok(Apply::Applied)
    }
    fn burn(&mut self, owner: OwnerId, seconds: f32) {
        let tick = self.session.simulation.state().tick;
        let feet = self.session.peers[&owner].player.state().feet;
        self.session.cues.emit(
            tick,
            crate::presentation::CueKind::Burn {
                actor: owner,
                seconds: seconds.min(300.0),
            },
            feet,
        );
    }
    fn client_op(&mut self, d: &Dispatch, op: &ClientOp) -> Result<Apply> {
        let owner = d.target.id.index;
        let s = &mut *self.session;
        let Some(peer) = s.peers.get(&owner) else {
            return Ok(Apply::Rejected("client left".into()));
        };
        let player = peer.combat.player;
        match op {
            ClientOp::Message {
                kind,
                text,
                seconds,
            } => {
                let score = s.minigames.player(player).map_or(0, |p| p.score);
                let text = ev::semantics::client_message(
                    text,
                    &peer.name,
                    score,
                    *kind == MessageKind::Chat,
                );
                s.notify(owner, message(*kind, text, *seconds));
            }
            ClientOp::IncScore(amount) => {
                let amount = (*amount).clamp(-99_999, 99_999) as i32;
                let effects = s
                    .minigames
                    .event_score(player, amount, true)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                s.apply_minigame_effects(effects)?;
            }
            // `GameConnection::playSound`: 2D, heard by this client only.
            ClientOp::PlaySound(sound) => {
                if let Some(profile) = sound.clone() {
                    s.notify(owner, Notice::Sound(profile));
                }
            }
        }
        Ok(Apply::Applied)
    }
    fn minigame_op(&mut self, d: &Dispatch, op: &MiniGameOp) -> Result<Apply> {
        let game = mg::GameId(d.target.id.index);
        let s = &mut *self.session;
        // Brick events act with the brick owner's authority over their game.
        let owner = s
            .simulation
            .state()
            .bricks
            .get(&d.source.index)
            .map_or(0, |b| b.owner);
        let instigator = d
            .client
            .and_then(|c| s.peers.get(&c.id.index))
            .map(|p| p.combat.player);
        let authority = match (instigator, s.peers.get(&owner)) {
            (Some(instigator), _) => mg::EventAuthority::OwnerBrick {
                instigator,
                brick_owner: mg::AccountId(owner),
            },
            (None, Some(peer)) => mg::EventAuthority::Owner(peer.combat.player),
            (None, None) => {
                return Ok(Apply::Rejected("brick owner is not connected".into()));
            }
        };
        let command = match op {
            MiniGameOp::Message {
                kind,
                text,
                seconds,
            } => {
                let seconds = (*seconds).clamp(1, 10) as u8;
                mg::Command::Message {
                    game,
                    authority,
                    kind: match kind {
                        MessageKind::Chat => mg::MessageKind::Chat,
                        MessageKind::Center => mg::MessageKind::Center { seconds },
                        MessageKind::Bottom => mg::MessageKind::Bottom { seconds },
                    },
                    text: text.chars().take(200).collect(),
                }
            }
            MiniGameOp::Reset => mg::Command::Reset { game, authority },
            MiniGameOp::RespawnAll => mg::Command::RespawnAll { game, authority },
        };
        match s.minigames.execute(command) {
            Ok(effects) => {
                // `MiniGameSO::Reset` names the client that set it off.
                if matches!(op, MiniGameOp::Reset) {
                    let resetter = d.client.map_or(owner, |c| c.id.index);
                    if let Some(name) = s.peers.get(&resetter).map(|p| p.name.clone()) {
                        s.chat_game(
                            Some(game),
                            None,
                            format!("\u{E003}{name}\u{E005} reset the mini-game"),
                        );
                    }
                }
                s.apply_minigame_effects(effects)?;
                Ok(Apply::Applied)
            }
            Err(error) => Ok(Apply::Rejected(error.to_string())),
        }
    }
}

impl ev::Host for EventHost<'_> {
    fn alive(&self, entity: Entity) -> bool {
        let s = &self.session;
        match entity.class {
            Class::Brick => s.simulation.state().bricks.contains_key(&entity.id.index),
            Class::Player => s.is_alive(entity.id.index),
            Class::Client => s.peers.contains_key(&entity.id.index),
            Class::MiniGame => s.minigames.game(mg::GameId(entity.id.index)).is_ok(),
            Class::Projectile | Class::Vehicle => false,
        }
    }
    fn permitted(&self, context: &Trigger, target: Entity, output: &str) -> bool {
        let s = &self.session;
        let bricks = &s.simulation.state().bricks;
        let Some(owner) = bricks.get(&context.source.index).map(|b| b.owner) else {
            return false;
        };
        match target.class {
            Class::Brick => bricks
                .get(&target.id.index)
                .is_some_and(|b| b.owner == owner),
            Class::Player | Class::Client => {
                let player = target.id.index;
                !harmful(output)
                    || player == owner
                    || s.game_of(player)
                        .is_some_and(|g| s.game_of(owner) == Some(g))
            }
            // The minigame rules check the brick owner's authority.
            Class::MiniGame => true,
            Class::Projectile | Class::Vehicle => false,
        }
    }
    fn relay_neighbors(
        &mut self,
        brick: Id,
        direction: ev::Direction,
        limit: usize,
    ) -> std::result::Result<Vec<Id>, String> {
        let simulation = &self.session.simulation;
        let (min, max) = simulation
            .brick_box(brick.index)
            .ok_or("relay brick is gone")?;
        let (center, size) = ev::semantics::relay_box(min, max, direction);
        let half = size * 0.5;
        let owner = simulation.state().bricks[&brick.index].owner;
        Ok(simulation
            .bricks_in_box(center - half, center + half)
            .into_iter()
            .filter(|b| *b != brick.index && simulation.state().bricks[b].owner == owner)
            .take(limit)
            .map(id)
            .collect())
    }
    fn apply(&mut self, dispatch: &Dispatch) -> Apply {
        let result = match &dispatch.intent {
            Intent::Brick(op) => self.brick_op(dispatch, op),
            Intent::Player(op) => self.player_op(dispatch, op),
            Intent::Client(op) => self.client_op(dispatch, op),
            Intent::MiniGame(op) => self.minigame_op(dispatch, op),
            Intent::Projectile(_) => Ok(Apply::Rejected(
                "projectile outputs are not available yet".into(),
            )),
        };
        result.unwrap_or_else(|error| Apply::Rejected(format!("{error:#}")))
    }
}
