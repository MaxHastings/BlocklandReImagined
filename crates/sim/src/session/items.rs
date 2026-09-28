//! Contact-driven host pickups. No remote position or Pickup command exists.
use super::*;
use bri_weapons::{ActorId, ItemBounds};
/// `MsgItemPickup`'s client `ItemPickup` sound, heard on picking up and on
/// dropping a tool; loadouts fill slots silently.
const ITEM_SOUND: &str = "ItemPickup";
/// Item_Sports ball shapes are about 0.36 units in radius.
const BALL_RADIUS: f32 = 0.36;
impl Session {
    /// Native authored DTS bounds converted offline; immutable during a session.
    /// Prepare all static objects before publishing any new catalog/state.
    pub fn set_item_bounds(&mut self, bounds: BTreeMap<String, ItemBounds>) -> Result<()> {
        ensure!(
            self.peers.is_empty() && self.departed.is_empty(),
            "Cannot replace live item bounds"
        );
        ensure!(bounds.len() <= 1024, "Item bounds catalog budget");
        for (id, shape) in &bounds {
            ensure!(self.weapons.contains_item(id), "Unknown item bounds: {id}");
            shape.validate()?;
        }
        let mut spawners = crate::item_spawners::ItemSpawners::new(bounds.clone());
        for (&id, brick) in &self.simulation.state().bricks {
            spawners.reconcile(
                id,
                Some(brick),
                &self.simulation.definitions,
                self.simulation.state().tick,
            )?;
        }
        self.item_spawners = spawners;
        self.weapons.set_item_bounds(bounds);
        Ok(())
    }
    pub(super) fn drop_tool(&mut self, owner: OwnerId, slot: usize, direction: Vec3) -> Result<()> {
        let peer = self.peers.get(&owner).context("Unknown connection")?;
        ensure!(peer.combat.alive, "Dead players cannot drop tools");
        let actor = ActorId(owner);
        let item = self
            .weapons
            .actor(actor)
            .and_then(|a| a.inventory.get(slot))
            .and_then(Option::as_ref)
            .context("Empty tool slot")?;
        ensure!(
            self.item_spawners.bounds.contains_key(item),
            "Item physics catalog is not installed"
        );
        // A reliable action may precede the first simulation tick after joining.
        // Refresh its frame now rather than throwing from stale/default coordinates.
        let mut frame = self.weapons.actor(actor).unwrap().frame.clone();
        let state = peer.player.state();
        frame.position = Vec3::from(state.feet);
        frame.eye = peer.player.eye();
        frame.direction = direction;
        frame.body_yaw = state.yaw;
        frame.velocity = Vec3::from(state.velocity);
        self.weapons.set_frame(actor, frame)?;
        let was_selected = self.weapons.actor(actor).unwrap().selected == Some(slot);
        self.weapons.drop_item(actor, slot)?;
        self.notify(owner, Notice::Sound(ITEM_SOUND.into()));
        self.weapon_triggers.remove(&owner);
        if was_selected {
            self.peers.get_mut(&owner).unwrap().inspection = None;
        }
        Ok(())
    }
    /// `spawnItem` event output.
    pub(super) fn spawn_event_item(&mut self, item: &str, at: Vec3, velocity: Vec3) -> Result<u64> {
        self.weapons.spawn_drop(item, at, velocity)
    }
    /// Bring the item spawners up to date with the bricks changed since the
    /// last network publish. Commands change bricks before a tick and the
    /// tick's own rules (layout swaps, streamed loads, events) change them
    /// after `step_items`, so this runs at both ends of `step`: the publish
    /// that follows a tick clears the dirty set. Reconciliation is idempotent
    /// and never resets an unchanged item's respawn clock.
    pub(super) fn reconcile_items(&mut self) -> Result<()> {
        if self.item_spawners.bounds.is_empty() {
            return Ok(());
        }
        let tick = self.simulation.state().tick;
        for &id in &self.dirty {
            self.item_spawners.reconcile(
                id,
                self.simulation.state().bricks.get(&id),
                &self.simulation.definitions,
                tick,
            )?;
        }
        Ok(())
    }
    pub(super) fn step_items(&mut self) -> Result<()> {
        if self.item_spawners.bounds.is_empty() {
            return Ok(());
        }
        let tick = self.simulation.state().tick;
        self.reconcile_items()?;
        let mut dynamic = crate::item_spawners::ContactIndex::default();
        for drop in self.weapons.drops() {
            if let Some(shape) = self.item_spawners.bounds.get(&drop.item) {
                let shape = ItemBounds {
                    min: (Vec3::from(shape.min) * drop.scale).to_array(),
                    max: (Vec3::from(shape.max) * drop.scale).to_array(),
                };
                dynamic.insert(drop.id, shape.transformed(drop.position, drop.rotation));
            }
        }
        // Deterministic connection order arbitrates simultaneous contact. Outside
        // minigames v20 permits pickups even from another builder's brick.
        // Balls in flight or at rest collide with players in v20
        // (`armor::onCollision` and `passBallCheck`).
        let balls: Vec<(u64, ActorId, ItemBounds)> = self
            .weapons
            .projectiles()
            .filter(|p| {
                self.weapons
                    .pack
                    .projectiles
                    .get(&p.definition)
                    .is_some_and(|d| d.sport_image.is_some())
            })
            .map(|p| {
                let r = BALL_RADIUS * p.scale;
                let bounds = ItemBounds {
                    min: (p.position - Vec3::splat(r)).to_array(),
                    max: (p.position + Vec3::splat(r)).to_array(),
                };
                (p.id, p.source, bounds)
            })
            .collect();
        let owners: Vec<OwnerId> = self.peers.keys().copied().collect();
        for owner in owners {
            let peer = &self.peers[&owner];
            if !peer.combat.alive {
                continue;
            }
            let actor = ActorId(owner);
            let contact = crate::player::item_bounds(&peer.player);
            let game = self.game_of(owner);
            let locked =
                self.teleport_lockout(owner, super::admin_players::TELEPORT_PICKUP_LOCK_MS, true);
            for (projectile, source, bounds) in &balls {
                // `sportIsInSameMinigame`: both in one game or both outside.
                if bounds.overlaps(&contact) && self.game_of(source.0) == game {
                    let _ = self.weapons.grab_ball(actor, *projectile);
                }
            }
            for id in self.item_spawners.contacts(contact) {
                let item = &self.item_spawners.items[&id];
                if locked || tick < item.available_at {
                    continue;
                }
                let sport = self
                    .weapons
                    .pack
                    .items
                    .get(&item.item)
                    .is_some_and(|d| d.sport);
                let picked = if sport {
                    self.weapons.use_sport_item(actor, &item.item)
                } else {
                    self.weapons.give(actor, &item.item).map(|_| ())
                };
                // Full inventory, duplicate item or occupied hands leave the item
                // available. Neither circumstance starts its respawn timer.
                if picked.is_ok() {
                    let respawn = self.simulation.state().bricks[&id]
                        .item_spawn
                        .respawn_ticks();
                    self.item_spawners.picked_up(id, tick, respawn)?;
                    if !sport {
                        self.notify(owner, Notice::Sound(ITEM_SOUND.into()));
                    }
                }
            }
            for id in dynamic.query(contact) {
                // Another peer can have consumed this candidate earlier this tick.
                if self.weapons.is_ball_drop(id) {
                    if locked {
                        continue;
                    }
                    let source = self.weapons.drops().find(|d| d.id == id).map(|d| d.source);
                    if source.is_some_and(|s| s.0 == 0 || self.game_of(s.0) == game) {
                        let _ = self.weapons.pickup_ball(actor, id);
                    }
                } else if !locked && self.weapons.pickup(actor, id).is_ok() {
                    self.notify(owner, Notice::Sound(ITEM_SOUND.into()));
                }
            }
        }
        Ok(())
    }
}
