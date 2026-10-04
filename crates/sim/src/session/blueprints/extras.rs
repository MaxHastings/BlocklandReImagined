//! A copy's bricks' settings ([`CopyExtras`]) going into the world with
//! them, each under the player's own wrench rules: the server has the
//! light, emitter, item, music or vehicle, its limits have room, and every
//! Add-On that reviews event rows keeps the row. A setting that may not go
//! in is left off and the rest go in, as the New Duplicator's `setLight`
//! and `setItem` each failed on their own.
use super::*;
use crate::blueprint::CopyExtras;
use bri_world::authority::WrenchProperties;

impl Session {
    /// Give brick `id`, just planted by `actor` from `owner`'s copy, the
    /// settings it carried (already turned as the copy was placed).
    pub(in crate::session) fn give_copy_extras(
        &mut self,
        owner: OwnerId,
        actor: &Actor,
        id: BrickId,
        extras: CopyExtras,
    ) {
        let Some(brick) = self.simulation.state().bricks.get(&id).cloned() else {
            return;
        };
        let resolved = |r: &ContentRef| match r {
            ContentRef::Resolved(id) => Some(id.clone()),
            ContentRef::Unresolved(_) => None,
        };
        let plain = copy_edits::wrench_properties(&brick);
        let mut properties = plain.clone();
        if let Some(name) = &extras.name {
            self.add_setting(&brick, id, &mut properties, |p| p.name = Some(name.clone()));
        }
        if let Some(light) = extras.light.as_ref().and_then(resolved) {
            self.add_setting(&brick, id, &mut properties, |p| p.light = Some(light));
        }
        if let Some(emitter) = &extras.emitter
            && let Some(asset) = emitter.asset.as_ref().and_then(resolved)
        {
            self.add_setting(&brick, id, &mut properties, |p| {
                p.emitter = Some(asset);
                p.emitter_direction = emitter.direction;
            });
        }
        if let Some(item) = &extras.item {
            self.add_setting(&brick, id, &mut properties, |p| p.item_spawn = item.clone());
        }
        if let Some(sound) = extras.sound.as_ref().and_then(resolved) {
            self.add_setting(&brick, id, &mut properties, |p| p.sound = Some(sound));
        }
        if let Some(spawn) = &extras.vehicle
            && let Some(vehicle) = resolved(&spawn.vehicle)
        {
            self.add_setting(&brick, id, &mut properties, |p| {
                p.vehicle = Some(vehicle);
                p.recolor_vehicle = spawn.recolor;
                p.vehicle_team = spawn.team;
            });
        }
        if properties != plain {
            let stocked = properties.item_spawn.item.is_some();
            if self
                .simulation
                .edit(actor, id, Edit::Properties(properties))
                .is_ok()
            {
                self.dirty.insert(id);
                if stocked {
                    let tick = self.simulation.state().tick;
                    self.item_spawners.restock(id, tick);
                }
            }
        }
        let mut rows = extras.events;
        if rows.is_empty() {
            return;
        }
        // As the events dialog sends them: the Add-Ons' review, and the
        // relay floor for players who are not administrators.
        let _ = self.review_event_rows(owner, id, &mut rows);
        if !self.is_administrator(owner) {
            events::clamp_relay_delays(&mut rows);
        }
        if !rows.is_empty() && self.simulation.edit(actor, id, Edit::Events(rows)).is_ok() {
            self.dirty.insert(id);
        }
    }

    /// `set` on top of `properties` for `brick` (planted as `id`), if the
    /// wrench would allow it there: within the server's light, emitter and
    /// item limits, of things the server has.
    fn add_setting(
        &self,
        brick: &Brick,
        id: BrickId,
        properties: &mut WrenchProperties,
        set: impl FnOnce(&mut WrenchProperties),
    ) {
        let mut next = properties.clone();
        set(&mut next);
        let wanted = next.clone();
        self.quota_wrench(brick, &mut next);
        let edit = Edit::Properties(next);
        if let Edit::Properties(next) = &edit
            && *next == wanted
            && self.tool_catalog.validate_edit(brick, &edit).is_ok()
            && self
                .item_spawners
                .validate_edit(self.simulation.state(), id, &edit)
                .is_ok()
        {
            *properties = wanted;
        }
    }
}
