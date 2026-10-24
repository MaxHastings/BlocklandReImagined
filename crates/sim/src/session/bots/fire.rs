//! What a bot fights with and when it may: its weapon read from the item,
//! the hand's fire gates and charge, arming and a body's bite.
use super::*;

/// A shot at the bot's target as it stands now ([`Session::bot_shot`]).
pub(super) struct Shot {
    pub crew_ready: bool,
    /// No body has stepped into what the planned shot sweeps since it was
    /// planned (`harm::Shape`).
    pub attack_clear: bool,
    /// An ally standing in it, if one is (`team`: it is told to move).
    pub ally_in_way: bool,
    /// What it will sweep, for its side to keep out of (`team`).
    pub harm: Option<claims::Harmed>,
    pub mounted_charging: bool,
    pub charged_ready: bool,
    pub target_velocity: Vec3,
}
/// What a bot fights with this tick ([`Session::bot_hand`]).
pub(super) struct Hand {
    pub native: hand_combat::Decision,
    pub native_choice: Option<hand_combat::Choice>,
    /// The weapon in its hand, by the native pick or its data.
    pub held: Option<Weapon>,
    /// Its kind's bite, when its hands are empty.
    pub bite: Option<crate::bot_kind::BotMelee>,
    /// What it attacks with: the held weapon, or the bite.
    pub weapon: Option<Weapon>,
}

/// How wide a shot's way is kept clear for its side (`team`): a bullet's
/// width and a little.
const BULLET_WIDTH: f32 = 0.3;

impl Session {
    /// What a shot at `target` would be now: whether the crew is ready,
    /// whether it is clear of its own side (a miss included), where it
    /// would hit for its side to keep out of, a mount's charge, and how
    /// fast the target moves across its line.
    pub(super) fn bot_shot(
        &self,
        bot: OwnerId,
        weapon: Option<Weapon>,
        target: Option<Seen>,
        eye: Vec3,
        tick: u64,
    ) -> Shot {
        let crew_ready = self.bot_crew_ready(bot, tick);
        let ranged = weapon.is_some_and(|w| !w.melee);
        // What the shot sweeps: the plan's (`harm`), else, for a weapon the
        // shot chooser does not plan (a mount's gun, a script's), the line
        // to its target and its blast there.
        let shape = target.filter(|_| ranged).map(|seen| {
            self.bots.brains[&bot]
                .combat
                .intent(tick)
                .filter(|i| i.seen.owner == seen.owner && !i.shape.chords.is_empty())
                .map_or_else(
                    || harm::Shape {
                        chords: vec![harm::Chord {
                            from: eye,
                            to: seen.aim,
                            seconds: 0.0,
                        }],
                        burst: weapon
                            .filter(|w| w.splash > 0.0)
                            .map(|w| (seen.aim, w.splash)),
                        priced: vec![seen.owner],
                    },
                    |i| i.shape,
                )
        });
        // Fired only while no body the plan did not count has stepped into
        // it, the way widened by how far the aim may have wandered since.
        let (yaw_error, pitch_error) = self.bots.brains[&bot].error;
        let wander = yaw_error.hypot(pitch_error).tan();
        let mount = |o: OwnerId| self.mounted(o).map(|(v, _)| v);
        // A body its blast cannot hurt (a teammate with friendly fire off,
        // one under spawn protection) is no harm to price: it stops a shot
        // only by being in the way, not by standing in the blast.
        let in_way = shape.as_ref().and_then(|shape| {
            self.peers.iter().find(|&(&o, p)| {
                if o == bot
                    || !p.combat.alive
                    || shape.priced.contains(&o)
                    || (mount(o).is_some() && mount(o) == mount(bot))
                {
                    return false;
                }
                let at = Vec3::from(p.player.state().feet)
                    + Vec3::Y * p.player.tuning().stand_height * 0.5;
                let half = p.player.tuning().stand_height * 0.5;
                shape.on_way(at, half, wander)
                    || (shape.in_burst(at, half)
                        && !self.spawn_protected(o)
                        && self.can_damage_player(bot, o, true))
            })
        });
        let attack_clear = in_way.is_none();
        let ally_in_way = in_way.is_some_and(|(o, _)| self.bot_allies(bot, *o));
        // What it will sweep, for its side to keep out of (`team`).
        let harm = shape.and_then(|shape| {
            let end = shape.end()?;
            Some(claims::Harmed {
                way: interactions::shot_space(eye, end, BULLET_WIDTH, 0.0)?,
                burst: shape.burst.map(|(at, radius)| claims::Space {
                    from: at,
                    to: at,
                    radius,
                    spread: 0.0,
                }),
            })
        });
        let mounted_charging = self
            .mounted(bot)
            .and_then(|(id, _)| self.bots.objects.iter().find(|v| v.id.0 == id))
            .is_some_and(|v| v.charge > 0);
        let charged_ready = self
            .mounted(bot)
            .and_then(|(id, _)| self.bots.objects.iter().find(|v| v.id.0 == id))
            .is_some_and(|v| {
                self.vehicles
                    .world
                    .as_ref()
                    .and_then(|w| w.definition(&v.definition))
                    .and_then(|d| d.weapon.as_ref())
                    .is_some_and(|g| g.charge_ticks > 0 && v.charge >= g.charge_steps)
            });
        let target_velocity = target.map_or(Vec3::ZERO, |seen| {
            self.peers.get(&seen.owner).map_or(Vec3::ZERO, |p| {
                seen.way.seen_vector(Vec3::from(p.player.state().velocity))
            })
        });
        Shot {
            crew_ready,
            attack_clear,
            ally_in_way,
            harm,
            mounted_charging,
            charged_ready,
            target_velocity,
        }
    }
    /// What it fights with this tick: the native chooser's pick of its hand
    /// weapons against `target` (or why none), the weapon it holds, and its
    /// kind's bite when its hands are empty.
    pub(super) fn bot_hand(
        &mut self,
        bot: OwnerId,
        target: Option<Seen>,
        tick: u64,
    ) -> Result<Hand> {
        // Inventory capabilities are grounded only in current sight. The
        // desired path uses a shared budget; actual firing is checked after
        // movement against the live launch frame in step_weapons.
        // An observed native grip is an ordinary held-tool sequence.
        // Finish its carry/release before considering another hand weapon;
        // switching would make the package drop the actual held participant.
        let hold_sequence =
            self.bot_weapon(bot).is_some_and(|w| w.hold) && self.held_by(bot).is_some();
        let weapons = self.bots.brains[&bot].kind.weapons;
        // A kind without hand weapons puts away any the rules gave it.
        if !weapons
            && !hold_sequence
            && self
                .weapons
                .actor(ActorId(bot))
                .is_some_and(|a| a.selected.is_some())
        {
            let _ = self.equip_tool(bot, None);
        }
        let native = if hold_sequence || !weapons {
            hand_combat::Decision::Unsupported
        } else if let Some(seen) = target {
            let mut combat = std::mem::take(&mut self.bots.brains.get_mut(&bot).unwrap().combat);
            let mut budget = std::mem::take(&mut self.bots.combat_budget);
            budget.begin_tick(tick);
            let mut mind = std::mem::take(&mut self.bots.brains.get_mut(&bot).unwrap().surprise);
            let eye = self.peers[&bot].player.eye();
            let decision = hand_combat::choose(
                self,
                bot,
                seen,
                eye,
                tick,
                &mut combat,
                &mut budget,
                &mut mind,
            );
            let brain = self.bots.brains.get_mut(&bot).unwrap();
            brain.combat = combat;
            brain.surprise = mind;
            self.bots.combat_budget = budget;
            decision
        } else {
            hand_combat::Decision::Unsupported
        };
        let native_choice = match &native {
            hand_combat::Decision::Ready(c) | hand_combat::Decision::Charging(c) => Some(*c),
            _ => None,
        };
        // A gun seat's shot is not a hand weapon's: no hand fire gate.
        let native_gate = !hold_sequence
            && !self.vehicles.weapon_seat(bot)
            && (!matches!(native, hand_combat::Decision::Unsupported)
                || (target.is_none() && self.bots.brains[&bot].native_combat_tick.is_some()));
        self.bots.brains.get_mut(&bot).unwrap().native_combat_tick = native_gate.then_some(tick);
        // Fighting empty-handed with an enemy in sight (just respawned
        // mid-fight, say), it takes out its weapon now, so it keeps the band
        // of that weapon and does not drop into a chase for a tick.
        if matches!(native, hand_combat::Decision::Unsupported)
            && target.is_some()
            && self.bots.brains[&bot].behaviour == Behaviour::Fight
            && !self.vehicles.weapon_seat(bot)
            && self.bot_weapon(bot).is_none()
        {
            self.bot_arm(bot)?;
        }
        // Empty-handed, a kind that hits with its body fights with that.
        let held = native_choice
            .map(|c| c.weapon)
            .or_else(|| {
                (!matches!(native, hand_combat::Decision::Unsupported))
                    .then(|| self.bots.brains[&bot].combat.movement_hint())
                    .flatten()
            })
            .or_else(|| self.bot_weapon(bot));
        let bite = held
            .is_none()
            .then(|| self.bots.brains[&bot].kind.melee.clone())
            .flatten();
        // Its body's reach: a hit's, an ability's (`ability`), whichever is
        // shorter, so it gets near enough for both; a touch's is its body.
        let kind = &self.bots.brains[&bot].kind;
        let body_reach = [
            bite.as_ref().map(|m| m.reach),
            kind.ability.as_ref().map(|a| a.range),
            kind.contact
                .as_ref()
                .map(|_| self.peers[&bot].player.tuning().width),
        ]
        .into_iter()
        .flatten()
        .reduce(f32::min)
        .filter(|_| held.is_none());
        let weapon = held.or(body_reach.map(|reach| Weapon {
            melee: true,
            hold: false,
            charge: false,
            near: None,
            reach,
            speed: 0.0,
            fall: 0.0,
            splash: 0.0,
            spread: 0.0,
        }));
        Ok(Hand {
            native,
            native_choice,
            held,
            bite,
            weapon,
        })
    }
    /// The held weapon's reach and flight.
    pub(super) fn bot_vehicle_weapon(&self, bot: OwnerId) -> Option<Weapon> {
        let (vehicle, seat) = self.mounted(bot)?;
        let d = self
            .vehicles
            .world
            .as_ref()?
            .definition_of(bri_vehicles::VehicleId(vehicle))?;
        (d.weapon_seat() == Some(usize::from(seat)))
            .then(|| self.vehicle_gun(vehicle))
            .flatten()
    }
    /// The reach and flight of `vehicle`'s gun, while it has one to fire.
    pub(super) fn vehicle_gun(&self, vehicle: u64) -> Option<Weapon> {
        let w = self.vehicles.world.as_ref()?;
        let d = w.definition_of(bri_vehicles::VehicleId(vehicle))?;
        let gun = d.weapon.as_ref()?;
        let p = self.weapons.pack.projectiles.get(&gun.projectile)?;
        let v = self.bots.objects.iter().find(|v| v.id.0 == vehicle)?;
        if !w.weapon_available(bri_vehicles::VehicleId(vehicle)) {
            return None;
        }
        let speed = gun.speed * f32::from(gun.charge_steps.max(1)) * v.scale;
        Some(Weapon {
            melee: false,
            hold: false,
            charge: gun.charge_ticks > 0,
            near: None,
            reach: speed * p.lifetime_ticks as f32 * TICK,
            speed,
            fall: bri_weapons::runtime::fall_per_tick(p) * bri_weapons::TICK_HZ as f32,
            splash: p.explosion.radius,
            spread: 0.0,
        })
    }
    /// How it handles what it holds: a natively modelled weapon as the
    /// fight reads it (`hand_combat::weapon_of`), else by its shot or projectile,
    /// a melee image by its reach, with its data's `BotUse` over either. An
    /// image with none of those is not a weapon to it (an unknown scripted
    /// item stays unsupported, never guessed at).
    pub(super) fn bot_weapon(&self, bot: OwnerId) -> Option<Weapon> {
        if let Some(gun) = self.bot_vehicle_weapon(bot) {
            return Some(gun);
        }
        if self.bots.brains.get(&bot).is_some_and(|b| !b.kind.weapons) {
            return None;
        }
        let (image, _) = self.weapons.image_state(ActorId(bot), 0)?;
        // An image whose rules say the bot has no rounds left for it is not
        // a weapon to it now (a reload the gun's own states fetch counts as
        // rounds: `rules_rounds` adds the spare ones).
        if hand_combat::rules_rounds(self, bot, image) == Some(0) {
            return None;
        }
        let scale = self.peers.get(&bot).map_or(1.0, |p| p.player.state().scale);
        self.image_weapon(image, scale)
    }
    /// How a bot handles an image as a weapon (`bot_weapon`): the one
    /// reader, which `hand_combat::has_possible_attack` asks too, so a bot
    /// never goes after someone with what it cannot use.
    pub(super) fn image_weapon(&self, image: &bri_weapons::Image, scale: f32) -> Option<Weapon> {
        let default_use = bri_weapons::BotUse::default();
        let using = image.bot.as_ref().unwrap_or(&default_use);
        let hold = using.fire == bri_weapons::BotFire::Hold;
        let spread = image_spread(image);
        let projectile = image
            .projectile
            .as_ref()
            .and_then(|p| self.weapons.pack.projectiles.get(p));
        if let Some(cap) =
            hand_combat::capability(image, projectile, scale, &self.weapons.pack.projectiles)
        {
            let w = hand_combat::weapon_of(cap, spread);
            return Some(Weapon {
                hold: w.hold || hold,
                near: using.near.or(w.near),
                reach: using.reach.unwrap_or(w.reach),
                ..w
            });
        }
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
        let Some(p) = projectile else {
            if !image.melee && using.reach.is_none() {
                return None;
            }
            let reach = using.reach.unwrap_or(MELEE_DEFAULT_REACH);
            return Some(Weapon {
                melee: reach < MELEE_REACH_LIMIT,
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
            melee: image.melee || reach < MELEE_REACH_LIMIT,
            hold,
            charge: image.charges(),
            near: using.near,
            reach,
            speed: p.speed,
            fall: bri_weapons::runtime::fall_per_tick(p) * bri_weapons::TICK_HZ as f32,
            splash: p.explosion.radius,
            spread,
        })
    }
    /// A bot's press of its trigger, checked at the actual post-movement
    /// launch frame: a planned shot as it was planned
    /// (`hand_combat::validate_intent`); a press no plan made (an
    /// objective's tool controls, a goof's click) of an attack it reads, as
    /// one that must do its own side no harm
    /// (`hand_combat::validate_unplanned`). None for a press of a tool that
    /// is no attack (package and mounted executors keep their own).
    /// `pressing`: a press this tick; `releasing`: a let-go this tick.
    pub(in crate::session) fn bot_hand_fire_gate(
        &mut self,
        bot: OwnerId,
        direction: Vec3,
        tick: u64,
        pressing: bool,
        releasing: bool,
    ) -> Option<FireAdmission> {
        let brain = self.bots.brains.get(&bot)?;
        let plan_tick = tick.checked_sub(1)?;
        let planned = brain.native_combat_tick == Some(plan_tick);
        if !planned && !pressing {
            return None;
        }
        let intent = planned.then(|| brain.combat.intent(plan_tick)).flatten();
        let (allowed, why) = if planned
            && intent.is_none()
            && !releasing
            && self
                .weapons
                .image_state(ActorId(bot), 0)
                .is_some_and(|(image, _)| image.charges() && charged_control::release_only(image))
        {
            // A release-only wind-up held while its target is out of view a
            // moment fires nothing until it is let go: only its release is an
            // attack, and that is planned and judged. Cancelling it here would
            // restart the wind-up every time sight flickers.
            (FireAdmission::Allow, "wind-up held, fires nothing")
        } else if !planned {
            match hand_combat::validate_unplanned(self, bot, direction)? {
                true => (FireAdmission::Allow, "unplanned, harmless to its side"),
                false => (FireAdmission::Abort, "unplanned, would hurt its side"),
            }
        } else {
            // Judged where it believes it aims: its aim error misses for
            // real (the harm check prices that miss,
            // `hand_combat::validate_fire`).
            let direction = perception::believed(&brain.kind.perception, direction, brain.error);
            let mut budget = std::mem::take(&mut self.bots.combat_budget);
            let judged = intent
                .as_ref()
                .map_or((FireAdmission::Abort, "no plan"), |intent| {
                    hand_combat::validate_intent(self, bot, intent, direction, &mut budget)
                });
            self.bots.combat_budget = budget;
            judged
        };
        if let Some(brain) = self.bots.brains.get_mut(&bot) {
            brain.gate = Some((tick, why));
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
        if !self.can_damage_player(bot, target, false) {
            return Ok(());
        }
        if point.distance(eye) > melee.reach + width {
            // Seated out of reach: the hull it sits in is what it lands on.
            let hull = self
                .bite_hull(melee, target, eye)
                .filter(|(_, at)| at.distance(eye) <= melee.reach);
            if let Some((vehicle, at)) = hull {
                if let Some(action) = &melee.action {
                    self.play_thread(tick, bot, MELEE_THREAD, action);
                }
                self.damage_vehicle(
                    vehicle,
                    melee.damage,
                    bot,
                    &melee.name,
                    at,
                    crate::session::vehicles::VehicleHarm::Weapon { projectile: None },
                )?;
            }
            return Ok(());
        }
        if let Some(action) = &melee.action {
            self.play_thread(tick, bot, MELEE_THREAD, action);
        }
        // The source collision policy tests existing health before its
        // ordinary damage handler. An eligible contact converts instead of
        // first killing a critically hurt bot or converting a healthy one
        // on the bite that merely crosses the threshold.
        if self.try_convert_bitten_bot(bot, target, melee)? {
            return Ok(());
        }
        self.damage_player_at(
            target,
            melee.damage,
            combat::DamageKind::weapon(melee.name.clone(), true),
            Some(bot),
            Some(point),
        )?;
        Ok(())
    }
    fn try_convert_bitten_bot(
        &mut self,
        bot: OwnerId,
        target: OwnerId,
        melee: &crate::bot_kind::BotMelee,
    ) -> Result<bool> {
        // A brick's bot of another side, worn down far enough, turns into
        // one of its kind, whole again (`holeZombieInfect`); players never do.
        let Some(below) = melee.converts_below else {
            return Ok(false);
        };
        let worn = self.is_alive(target)
            && self.bots.is_brick_bot(target)
            && self.peers[&target].combat.health <= self.max_health(target) * below;
        // The declared conversion attack supplies the replacement kind.
        let Some(kind) = self.bots.brains.get(&bot).map(|b| b.kind.clone()) else {
            return Ok(false);
        };
        let other = self.bots.brains.get(&target).map(|b| b.kind.side.clone());
        if !worn || other.is_none() || other == Some(kind.side.clone()) {
            return Ok(false);
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
        Ok(true)
    }
    /// Where a bite meets `target` seated: the point of the hull it sits in
    /// (its bounds as it lies now) nearest `from`, when the bite wears a
    /// hull down (`DamageType::vehicle_scale`, as any weapon's hit does);
    /// None on foot, in a wreck, or for a bite a hull shrugs off.
    pub(super) fn bite_hull(
        &self,
        melee: &crate::bot_kind::BotMelee,
        target: OwnerId,
        from: Vec3,
    ) -> Option<(u64, Vec3)> {
        let wears = self
            .weapons
            .pack
            .damage_type(&melee.name)
            .map_or(1.0, |t| t.vehicle_scale)
            > 0.0;
        let (vehicle, _) = self.mounted(target).filter(|_| wears)?;
        let world = self.vehicles.world.as_ref()?;
        let v = world
            .vehicle_snapshot(&self.simulation.physics, bri_vehicles::VehicleId(vehicle))
            .filter(|v| !v.destroyed)?;
        let d = world.definition(&v.definition)?;
        let at = Vec3::from(v.transform.position);
        let turn = glam::Quat::from_array(v.transform.rotation);
        let local = (turn.inverse() * (from - at)).clamp(
            Vec3::from(d.bounds_min) * v.scale,
            Vec3::from(d.bounds_max) * v.scale,
        );
        Some((vehicle, at + turn * local))
    }
    /// Equip the best weapon in the inventory by its data
    /// (`hand_combat::item_worth`), for a weapon the native chooser does not
    /// handle (a script fires it, or a seat holds the bot), unless it holds
    /// one already that is as good (the rules may have put it in its hand).
    /// With none that attacks by its data, the first real tool.
    pub(super) fn bot_arm(&mut self, bot: OwnerId) -> Result<()> {
        // A kind without hand weapons never draws one.
        if self.bots.brains.get(&bot).is_some_and(|b| !b.kind.weapons) {
            return Ok(());
        }
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
                .is_some_and(|id| !self.weapons.building_tool(id))
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

/// v20's `%spread`: each projectile turns by up to 5π·spread about each
/// axis, in radians.
pub(super) fn image_spread(image: &bri_weapons::Image) -> f32 {
    image
        .shot
        .as_ref()
        .filter(|s| s.spread > 0.0)
        .map_or(0.0, |s| (5.0 * std::f32::consts::PI * s.spread).min(1.4))
}
