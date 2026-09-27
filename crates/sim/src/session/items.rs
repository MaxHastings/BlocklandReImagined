//! Contact-driven host pickups. No remote position or Pickup command exists.
use super::*;
use bri_weapons::{ActorId, ItemBounds};
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
        let mut spawners = crate::item_spawners::ItemSpawners::new(bounds);
        for (&id, brick) in &self.simulation.state().bricks {
            spawners.reconcile(
                id,
                Some(brick),
                &self.simulation.definitions,
                self.simulation.state().tick,
            )?;
        }
        self.item_spawners = spawners;
        Ok(())
    }
    pub(super) fn drop_tool(&mut self, owner: OwnerId, slot: usize, direction: Vec3) -> Result<()> {
        let peer = self.peers.get(&owner).context("Unknown connection")?;
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
        self.weapon_triggers.remove(&owner);
        if was_selected {
            self.peers.get_mut(&owner).unwrap().inspection = None;
        }
        Ok(())
    }
    /// `spawnItem` event output.
    pub(super) fn spawn_event_item(&mut self, item: &str, at: Vec3, velocity: Vec3) -> Result<()> {
        self.weapons.spawn_drop(item, at, velocity)?;
        Ok(())
    }
    pub(super) fn step_items(&mut self) -> Result<()> {
        if self.item_spawners.bounds.is_empty() {
            return Ok(());
        }
        let tick = self.simulation.state().tick;
        // Replication retains dirty IDs until a network publish. Reconciliation is
        // idempotent and never resets an unchanged item's respawn clock.
        for &id in &self.dirty {
            self.item_spawners.reconcile(
                id,
                self.simulation.state().bricks.get(&id),
                &self.simulation.definitions,
                tick,
            )?;
        }
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
        for (&owner, peer) in &self.peers {
            let actor = ActorId(owner);
            let contact = peer.player.world_bounds();
            for id in self.item_spawners.contacts(contact) {
                let item = &self.item_spawners.items[&id];
                if tick < item.available_at {
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
                }
            }
            for id in dynamic.query(contact) {
                // Another peer can have consumed this candidate earlier this tick.
                let _ = self.weapons.pickup(actor, id);
            }
        }
        Ok(())
    }
}
