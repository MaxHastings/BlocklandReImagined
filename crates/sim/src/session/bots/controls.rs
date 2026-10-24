//! Native builder controls operate on the bot belonging to the event target
//! brick. Event dispatch checks editing authority before reaching this seam.
use super::*;
use bri_events::bots::BotOp;

impl Session {
    pub(in crate::session) fn fire_bot_spawn(&mut self, bot: OwnerId) {
        if let Some(brick) = self.bots.spawn_brick(bot) {
            self.fire_package_input(
                brick,
                "onBotSpawn",
                None,
                super::super::events::InputExtra {
                    bot: Some(self.player_entity(bot)),
                    ..Default::default()
                },
            );
        }
    }
    pub(in crate::session) fn control_brick_bot(
        &mut self,
        brick: BrickId,
        op: &BotOp,
    ) -> Result<()> {
        if matches!(op, BotOp::Respawn) {
            if let Some(bot) = self.bots.by_brick.get(&brick)
                && let Some(born) = self.spawn_tick(*bot)
            {
                // The supplied framework also rate-limits event respawns to
                // 200 ms. In particular a zero-delay onBotSpawn loop must
                // not repeatedly rebuild a physics body in one host tick.
                ensure!(
                    self.simulation.state().tick.saturating_sub(born) >= ticks(0.2),
                    "Bot respawn cooldown is 0.2 seconds"
                );
            }
            let kind = self
                .simulation
                .state()
                .bricks
                .get(&brick)
                .and_then(|b| self.brick_spawn_kind(b))
                .filter(|kind| self.is_bot_kind(kind))
                .context("This brick has no bot kind")?;
            return self.respawn_brick_bot(brick, &kind);
        }
        let bot = *self
            .bots
            .by_brick
            .get(&brick)
            .context("This brick has no bot")?;
        if let BotOp::Powered(on) = op {
            return self.set_bot_resting(bot, !on);
        }
        let home = Vec3::from(
            self.simulation
                .state()
                .bricks
                .get(&brick)
                .context("Bot's brick is gone")?
                .position,
        ) + Vec3::Y * 0.3;
        let template = self
            .bots
            .brains
            .get(&bot)
            .and_then(|b| self.bots.kind(&b.kind.id))
            .and_then(|k| k.melee.clone());
        let brain = self.bots.brains.get_mut(&bot).context("This bot is gone")?;
        let kind = std::sync::Arc::make_mut(&mut brain.kind);
        match *op {
            BotOp::SightRadius(radius) => {
                ensure!(
                    radius.is_finite() && (0.0..=400.0).contains(&radius),
                    "Bot sight radius must be 0–400 world units"
                );
                kind.sight = radius;
                brain.target = None;
                brain.memory = None;
                brain.evidence_search.clear();
                brain.set_goal(None);
                self.bots.hurt.remove(&bot);
            }
            BotOp::WanderRadius(radius) => {
                ensure!(
                    radius.is_finite() && (0.0..=64.0).contains(&radius),
                    "Bot wander radius must be 0–64 world units"
                );
                kind.wander_radius = radius;
                if matches!(brain.behaviour, Behaviour::Wander | Behaviour::Return) {
                    brain.set_goal(None);
                    brain.next_wander = 0;
                }
            }
            BotOp::ReturnHome(enabled) => {
                brain.return_home = enabled;
                if enabled {
                    brain.home = home;
                    brain.leash = home;
                }
                if matches!(brain.behaviour, Behaviour::Wander | Behaviour::Return) {
                    brain.set_goal(None);
                    brain.next_wander = 0;
                }
            }
            BotOp::AutoRespawn(enabled) => {
                brain.auto_respawn = enabled;
            }
            BotOp::MeleeDamage(damage) => {
                ensure!(
                    damage.is_finite() && (0.0..=1000.0).contains(&damage),
                    "Bot melee damage must be 0–1000"
                );
                if damage == 0.0 {
                    kind.melee = None;
                } else {
                    let melee = kind.melee.get_or_insert_with(|| {
                        template.unwrap_or_else(|| crate::bot_kind::BotMelee::with_damage(damage))
                    });
                    melee.damage = damage;
                }
            }
            BotOp::Powered(_) | BotOp::Respawn => unreachable!(),
        }
        Ok(())
    }
}
