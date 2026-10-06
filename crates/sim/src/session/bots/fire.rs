//! What a bot fights with and when it may: its weapon read from the item,
//! the hand's fire gates and charge, arming and a body's bite.
use super::*;

impl Session {
    /// The held weapon's reach and flight.
    pub(super) fn bot_vehicle_weapon(&self, bot: OwnerId) -> Option<Weapon> {
        if let Some((vehicle, seat)) = self.mounted(bot)
            && let Some(w) = &self.vehicles.world
            && w.weapon_available(bri_vehicles::VehicleId(vehicle))
            && let Some(d) = w.definition_of(bri_vehicles::VehicleId(vehicle))
            && d.weapon_seat() == Some(usize::from(seat))
            && let Some(gun) = &d.weapon
            && let Some(p) = self.weapons.pack.projectiles.get(&gun.projectile)
            && let Some(v) = self.bots.objects.iter().find(|v| v.id.0 == vehicle)
        {
            let speed = gun.speed * f32::from(gun.charge_steps.max(1)) * v.scale;
            return Some(Weapon {
                melee: false,
                hold: false,
                charge: gun.charge_ticks > 0,
                near: None,
                reach: speed * p.lifetime_ticks as f32 * TICK,
                speed,
                fall: bri_weapons::runtime::fall_per_tick(p) * 120.0,
                splash: p.explosion.radius,
                spread: 0.0,
            });
        }
        None
    }
    pub(super) fn bot_weapon(&self, bot: OwnerId) -> Option<Weapon> {
        if let Some(gun) = self.bot_vehicle_weapon(bot) {
            return Some(gun);
        }
        let (image, _) = self.weapons.image_state(ActorId(bot), 0)?;
        let using = image.bot.unwrap_or_default();
        let hold = using.fire == bri_weapons::BotFire::Hold;
        // v20's `%spread`: each projectile turns by up to 5π·spread about
        // each axis.
        let spread = image
            .shot
            .as_ref()
            .filter(|s| s.spread > 0.0)
            .map_or(0.0, |s| (5.0 * std::f32::consts::PI * s.spread).min(1.4));
        if let Some(ray) = image.shot.as_ref().and_then(|s| s.hitscan.as_ref()) {
            return Some(Weapon {
                melee: false,
                hold,
                charge: image.charges(),
                near: using.near,
                reach: using.reach.unwrap_or(ray.range),
                speed: 0.0,
                fall: 0.0,
                splash: 0.0,
                spread,
            });
        }
        let projectile = image
            .projectile
            .as_ref()
            .and_then(|p| self.weapons.pack.projectiles.get(p));
        let Some(p) = projectile else {
            let reach = using.reach.unwrap_or(3.0);
            return Some(Weapon {
                melee: reach < 6.0,
                hold,
                charge: image.charges(),
                near: using.near,
                reach,
                speed: 0.0,
                fall: 0.0,
                splash: 0.0,
                spread,
            });
        };
        let reach = using
            .reach
            .unwrap_or(p.speed * p.lifetime_ticks as f32 * TICK);
        Some(Weapon {
            melee: image.melee || reach < 6.0,
            hold,
            charge: image.charges(),
            near: using.near,
            reach,
            speed: p.speed,
            fall: bri_weapons::runtime::fall_per_tick(p) * 120.0,
            splash: p.explosion.radius,
            spread,
        })
    }
    /// Supported inventory intent is checked at the actual post-movement
    /// launch frame. None preserves existing package/mounted executors.
    pub(in crate::session) fn bot_hand_fire_gate(
        &mut self,
        bot: OwnerId,
        direction: Vec3,
        tick: u64,
    ) -> Option<FireAdmission> {
        let brain = self.bots.brains.get(&bot)?;
        let plan_tick = tick.checked_sub(1)?;
        if brain.native_combat_tick != Some(plan_tick) {
            return None;
        }
        let intent = brain.combat.intent(plan_tick);
        // Judged where it believes it aims: its aim error misses for real.
        // The miss itself must still spare its side.
        let actual = direction;
        let direction = perception::believed(&brain.kind.perception, direction, brain.error);
        let mut budget = std::mem::take(&mut self.bots.combat_budget);
        let allowed = intent.as_ref().map_or(FireAdmission::Abort, |intent| {
            hand_combat::validate_intent(self, bot, intent, direction, &mut budget)
        });
        self.bots.combat_budget = budget;
        if allowed == FireAdmission::Allow
            && actual != direction
            && !self.bot_miss_spares_allies(bot, actual)
        {
            return Some(FireAdmission::Abort);
        }
        Some(allowed)
    }
    /// Replace a speculative release with a safe native hold, without a
    /// re-press (which itself would release/restart a charged image).
    pub(in crate::session) fn bot_hold_hand_charge(&mut self, bot: OwnerId) -> Result<()> {
        self.weapon_triggers.remove(&bot);
        if let Some(brain) = self.bots.brains.get_mut(&bot) {
            brain.fire_down = true;
        }
        self.weapons.trigger(ActorId(bot), true)
    }
    pub(in crate::session) fn bot_abort_unsafe_hand_fire(&mut self, bot: OwnerId) -> Result<()> {
        if let Some(brain) = self.bots.brains.get_mut(&bot) {
            brain.fire_down = false;
        }
        if !self.abort_bot_hand_charge(bot)? {
            self.weapons.trigger(ActorId(bot), false)?;
        }
        Ok(())
    }
    /// A hit with the bot's body (`BotKind::melee`) on `target`, if it
    /// reaches them and the damage rules let it hurt them.
    pub(super) fn bot_bite(
        &mut self,
        bot: OwnerId,
        target: OwnerId,
        melee: &crate::bot_kind::BotMelee,
        tick: u64,
    ) -> Result<()> {
        let (Some(me), Some(them)) = (self.peers.get(&bot), self.peers.get(&target)) else {
            return Ok(());
        };
        if !them.combat.alive {
            return Ok(());
        }
        let eye = me.player.eye();
        // The nearest point of their body, feet to head.
        let state = them.player.state();
        let feet = Vec3::from(state.feet);
        let height = crate::water::body_height(state, them.player.tuning()) * state.scale;
        let point = Vec3::new(feet.x, eye.y.clamp(feet.y, feet.y + height), feet.z);
        let width = them.player.tuning().width * state.scale * 0.5;
        if point.distance(eye) > melee.reach + width || !self.can_damage_player(bot, target, false)
        {
            return Ok(());
        }
        if let Some(action) = &melee.action {
            self.play_thread(tick, bot, MELEE_THREAD, action);
        }
        self.damage_player_at(
            target,
            melee.damage,
            combat::DamageKind::weapon(melee.name.clone(), true),
            Some(bot),
            Some(point),
        )?;
        // A brick's bot of another side, worn down far enough, turns into
        // one of its kind, whole again (`holeZombieInfect`); players never do.
        let Some(below) = melee.converts_below else {
            return Ok(());
        };
        let worn = self.is_alive(target)
            && self.bots.is_brick_bot(target)
            && self.peers[&target].combat.health <= self.max_health(target) * below;
        let kind = self.bots.brains[&bot].kind.clone();
        let other = self.bots.brains.get(&target).map(|b| b.kind.side.clone());
        if !worn || other.is_none() || other == Some(kind.side.clone()) {
            return Ok(());
        }
        let brain = self.bots.brains.get_mut(&target).unwrap();
        let born = std::mem::replace(&mut brain.kind, kind.clone());
        brain.born.get_or_insert(born);
        brain.objective_threat = None;
        brain.posed = false;
        brain.target = None;
        brain.memory = None;
        brain.evidence_search.clear();
        self.embody_bot(target, &kind)?;
        let max = self.max_health(target);
        self.peers.get_mut(&target).unwrap().combat.health = max;
        Ok(())
    }
    /// Equip the best weapon in the inventory by its data
    /// (`hand_combat::item_worth`), for a weapon the native chooser does not
    /// handle (a script fires it, or a seat holds the bot), unless it holds
    /// one already that is as good (the rules may have put it in its hand).
    /// With none that attacks by its data, the first real tool.
    pub(super) fn bot_arm(&mut self, bot: OwnerId) -> Result<()> {
        // Holding or reaching for something with its tool: the tool stays.
        if self.held_by(bot).is_some() || self.is_reaching(bot) {
            return Ok(());
        }
        let Some(actor) = self.weapons.actor(ActorId(bot)) else {
            return Ok(());
        };
        let scale = self.peers.get(&bot).map_or(1.0, |p| p.player.state().scale);
        let worth = |slot: usize| {
            actor.inventory[slot]
                .as_deref()
                .map_or(0.0, |id| hand_combat::item_worth(self, id, scale))
        };
        let held = actor
            .selected
            .filter(|s| *s < actor.inventory.len())
            .map_or(0.0, worth);
        let best = (0..actor.inventory.len())
            .map(|slot| (slot, worth(slot)))
            .filter(|(_, w)| *w > 0.0)
            .fold(None::<(usize, f32)>, |best, (slot, w)| {
                if best.is_none_or(|(_, b)| w > b) {
                    Some((slot, w))
                } else {
                    best
                }
            });
        // Nothing that attacks by its data (a tool that grabs, say): the
        // first that is not a building tool, unless it holds one.
        let real = |item: &Option<String>| {
            item.as_deref()
                .is_some_and(|id| !bri_weapons::CORE_TOOLS.contains(&id))
        };
        let slot = match best {
            Some((slot, w)) if w > held => Some(slot),
            Some(_) => None,
            None if actor
                .selected
                .and_then(|s| actor.inventory.get(s))
                .is_some_and(real) =>
            {
                None
            }
            None => actor.inventory.iter().position(real),
        };
        if let Some(slot) = slot
            && actor.selected != Some(slot)
        {
            let _ = self.equip_tool(bot, Some(slot));
        }
        Ok(())
    }
}
