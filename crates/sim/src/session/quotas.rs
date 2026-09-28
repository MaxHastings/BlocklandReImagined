//! v20's per-builder quotas (`QuotaObject`, one per brick group): events
//! waiting to run, lights and emitters, items and projectiles. A LAN server
//! reads `$Pref::Server::QuotaLAN::*`, an Internet one `$Pref::Server::Quota::*`,
//! each clamped by `verifyQuotaNumber` to `$Game::MinQuota`..`MaxQuota`.
//! (`Quota::Misc` covers explosions, which are instantaneous here, so it has
//! nothing lasting to count.)
use super::*;

#[derive(Clone, Copy)]
pub(super) enum Quota {
    Schedules,
    Environment,
    Items,
    Projectiles,
}

impl Session {
    pub(super) fn quota(&self, quota: Quota) -> usize {
        let settings = &self.admin.settings;
        let set = if self.lan_host {
            &settings.lan
        } else {
            &settings.per_player
        };
        let (value, min, max) = match quota {
            Quota::Schedules => (set.schedules, 10, 1000),
            Quota::Environment => (set.environment, 20, 5000),
            Quota::Items => (set.items, 5, 1000),
            Quota::Projectiles => (set.projectiles, 5, 1000),
        };
        value.clamp(min, max) as usize
    }
    /// Lights and emitters on `owner`'s bricks.
    pub(super) fn environment_used(&self, owner: OwnerId) -> usize {
        self.simulation
            .state()
            .bricks
            .values()
            .filter(|b| b.owner == owner)
            .map(|b| {
                usize::from(b.light.is_some())
                    + usize::from(b.emitter.as_ref().is_some_and(|e| e.asset.is_some()))
            })
            .sum()
    }
    /// Item spawns on `owner`'s bricks and items their events dropped.
    pub(super) fn items_used(&self, owner: OwnerId) -> usize {
        let spawns = self
            .simulation
            .state()
            .bricks
            .values()
            .filter(|b| b.owner == owner && b.item_spawn.item.is_some())
            .count();
        let dropped = self.events.dropped.get(&owner).map_or(0, |ids| {
            self.weapons.drops().filter(|d| ids.contains(&d.id)).count()
        });
        spawns + dropped
    }
    /// Live projectiles `owner`'s bricks' events spawned.
    pub(super) fn projectiles_used(&self, owner: OwnerId) -> usize {
        self.events.spawned.get(&owner).map_or(0, |ids| {
            self.weapons
                .projectiles()
                .filter(|p| ids.contains(&p.id))
                .count()
        })
    }
    /// `serverCmdSetWrenchData` under the quota: a light, emitter or item the
    /// brick did not have is left off once its owner's quota is full.
    pub(super) fn quota_wrench(
        &self,
        brick: &Brick,
        properties: &mut bri_world::authority::WrenchProperties,
    ) {
        let owner = brick.owner;
        let had_emitter = brick.emitter.as_ref().is_some_and(|e| e.asset.is_some());
        let adds = usize::from(properties.light.is_some() && brick.light.is_none())
            + usize::from(properties.emitter.is_some() && !had_emitter);
        if adds > 0 {
            let free = self
                .quota(Quota::Environment)
                .saturating_sub(self.environment_used(owner));
            if free < adds && brick.light.is_none() {
                properties.light = None;
            }
            if free == 0 && !had_emitter {
                properties.emitter = None;
            }
        }
        if properties.item_spawn.item.is_some()
            && brick.item_spawn.item.is_none()
            && self.items_used(owner) >= self.quota(Quota::Items)
        {
            properties.item_spawn.item = None;
        }
    }
    /// `ProcessInputEvent`: when the rows an input would schedule exceed
    /// what is left of the brick owner's schedule quota, none run and the
    /// player who set it off is told.
    pub(super) fn schedules_exceeded(
        &mut self,
        brick: BrickId,
        input: &str,
        player: Option<OwnerId>,
    ) -> bool {
        let Some(owner) = self.simulation.state().bricks.get(&brick).map(|b| b.owner) else {
            return false;
        };
        let Some(world) = self.events.world.as_ref() else {
            return false;
        };
        let count = world
            .activation_count(super::events::id(brick), input)
            .unwrap_or(0);
        if count == 0 {
            return false;
        }
        let free = self
            .quota(Quota::Schedules)
            .saturating_sub(world.pending_for_scope(owner));
        if count <= free {
            return false;
        }
        if let Some(player) = player.filter(|p| self.peers.contains_key(p)) {
            self.notify(
                player,
                Notice::Center {
                    text: format!("<color:FFFFFF>Too many events at once!\n({input})"),
                    seconds: 1.0,
                },
            );
        }
        true
    }
}
