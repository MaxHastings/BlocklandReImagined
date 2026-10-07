//! Explicit adaptations of Item_Sports callbacks; random words come from host PRNG.
use super::*;
#[derive(Debug, Clone, Copy)]
pub enum SportAction {
    BasketballPass,
    FootballLateral,
    SoccerPop,
    SoccerDrop,
}
impl WeaponsWorld {
    /// Use a brick/world sports item directly, without occupying a tool inventory slot.
    /// Host validates pickup radius, item ownership/minigame, and respawns/removes its item.
    pub fn use_sport_item(&mut self, id: ActorId, item: &str) -> Result<()> {
        ensure!(self.events.len() < 8192, "Command event budget");
        let def = self.pack.items.get(item).context("Unknown item")?;
        ensure!(def.sport, "Not a sports item");
        let image = def.image.clone();
        ensure!(
            self.mount_ball(id, &image).is_some(),
            "Cannot hold ball now"
        );
        Ok(())
    }
    /// `passBallCheck`: empty hands and no ball timeout mount the ball's image
    /// (its horse variant on a horse) and play the catch sound one unit
    /// below the eye. Returns the mounted image.
    pub(super) fn mount_ball(&mut self, id: ActorId, image: &str) -> Option<String> {
        let a = self.actors.get(&id)?;
        if a.images[0].is_some() || self.tick < a.ball_ready {
            return None;
        }
        let horse = native_id(
            "image",
            &format!("horse{}", image.rsplit('.').next().unwrap_or_default()),
        );
        let image = if a.frame.horse && self.pack.images.contains_key(&horse) {
            horse
        } else {
            image.to_owned()
        };
        let sound = a.frame.eye - Vec3::Y;
        let mut a = self.actors.remove(&id).unwrap();
        self.mount(id, &mut a, &image, 0);
        a.ball_ready = self.tick + 36;
        self.actors.insert(id, a);
        self.events.push(Event::Sound {
            source: TargetId::Actor(id),
            profile: "weaponSwitchSound".into(),
            position: sound,
        });
        Some(image)
    }
    /// `Player::spawnBall`'s sound: every ball a player throws, passes,
    /// pops, fumbles or loses to a steal plays the catch sound half a unit
    /// below its eye. (`dropBall` makes its projectile without it.)
    pub(super) fn ball_released(&mut self, id: ActorId, eye: Vec3) {
        self.events.push(Event::Sound {
            source: TargetId::Actor(id),
            profile: "weaponSwitchSound".into(),
            position: eye - Vec3::Y * 0.5,
        });
    }
    /// The minigame's StartBall (`armor::onAdd`, `updatePlayerBalls`): mounted
    /// into empty hands without the catch timeout.
    pub fn start_ball(&mut self, id: ActorId, image: &str) -> Result<bool> {
        ensure!(self.events.len() < 8192, "Command event budget");
        ensure!(self.pack.images.contains_key(image), "Unknown ball image");
        let a = self.actors.get_mut(&id).context("Unknown actor")?;
        a.ball_ready = 0;
        Ok(self.mount_ball(id, image).is_some())
    }
    /// A player walked into a ball projectile (`armor::onCollision`). The host
    /// has already checked contact and that both share a minigame.
    pub fn grab_ball(&mut self, id: ActorId, projectile: u64) -> Result<bool> {
        ensure!(self.events.len() < 8192, "Command event budget");
        let p = self
            .projectiles
            .get(&projectile)
            .context("Unknown projectile")?;
        let image = self.pack.projectiles[&p.definition]
            .sport_image
            .clone()
            .context("Not a ball")?;
        let Some(image) = self.mount_ball(id, &image) else {
            return Ok(false);
        };
        let p = self.projectiles.remove(&projectile).unwrap();
        let d = self.pack.projectiles[&p.definition].clone();
        self.football_catch(&p, &d, id);
        self.events.push(Event::Removed { projectile });
        self.events.push(Event::BallCaught {
            actor: id,
            projectile,
            image,
        });
        Ok(true)
    }
    /// `footballProjectile::onCollision`'s `CatchFootballMessage`: a
    /// football caught before it touched the ground, by whichever way the
    /// catcher met it.
    pub(super) fn football_catch(
        &mut self,
        p: &Projectile,
        d: &crate::ProjectileDef,
        catcher: ActorId,
    ) {
        if StockProjectile::of(d) != Some(StockProjectile::Football) || p.bounced {
            return;
        }
        let delta = self.actors[&catcher].frame.position - p.origin;
        self.events.push(Event::FootballCatch {
            source: p.source,
            catcher,
            distance_feet: (Vec3::new(delta.x, 0.0, delta.z).length() * 1.875).round() as u32,
            was_thrown: p.was_thrown,
        });
    }
    /// Touching a ball item mounts it instead of filling a tool slot.
    pub fn is_ball_drop(&self, drop: u64) -> bool {
        self.drops
            .get(&drop)
            .and_then(|d| self.pack.items.get(&d.item))
            .is_some_and(|item| item.sport)
    }
    pub fn pickup_ball(&mut self, id: ActorId, drop: u64) -> Result<bool> {
        ensure!(self.events.len() < 8192, "Command event budget");
        let d = self.drops.get(&drop).context("Unknown drop")?;
        ensure!(
            id != d.source || self.tick >= d.pickup_after,
            "Pickup cooldown"
        );
        let image = self
            .pack
            .items
            .get(&d.item)
            .filter(|i| i.sport)
            .context("Not a ball")?
            .image
            .clone();
        if self.mount_ball(id, &image).is_none() {
            return Ok(false);
        }
        self.drops.remove(&drop);
        self.events.push(Event::DropRemoved { drop });
        Ok(true)
    }
    pub fn sport_action(&mut self, id: ActorId, action: SportAction) -> Result<u64> {
        ensure!(
            self.events.len() < 8192 && self.projectiles.len() < MAX_PROJECTILES,
            "Projectile/event budget"
        );
        let a = self.actors.get(&id).context("Unknown actor")?;
        ensure!(self.tick >= a.ball_ready, "Ball release cooldown");
        let image = a.images[0].as_ref().context("No ball held")?;
        let dir = a.frame.direction.normalize();
        let forward = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
        let (projectile, velocity, cooldown) = match action {
            SportAction::BasketballPass => ("basketballProjectile", dir * 20.0 + Vec3::Y * 4.0, 36),
            SportAction::FootballLateral => (
                "footballProjectile",
                forward * (-10.0) + Vec3::Y * 5.0 + a.frame.velocity,
                36,
            ),
            SportAction::SoccerPop => (
                "soccerBallProjectile",
                forward
                    + a.frame.velocity
                    + Vec3::Y
                        * if a.frame.velocity.length_squared() == 0.0 {
                            9.0
                        } else {
                            10.0
                        },
                240,
            ),
            SportAction::SoccerDrop => (
                "soccerBallProjectile",
                forward * 2.0 + a.frame.velocity * 2.0 + Vec3::Y * 5.0,
                240,
            ),
        };
        let stock = self.pack.images.get(&image.image).map(Stock::of);
        let held = match action {
            SportAction::BasketballPass => stock.and_then(|s| s.ball) == Some(Ball::Basketball),
            SportAction::FootballLateral => stock.and_then(|s| s.ball) == Some(Ball::Football),
            SportAction::SoccerPop | SportAction::SoccerDrop => {
                stock.and_then(|s| s.sport_keys) == Some(SportKeys::Pop)
            }
        };
        ensure!(held, "Sport action does not match held ball");
        let result = self.spawn(
            &native_id("projectile", projectile),
            id,
            a.frame.muzzle[0],
            velocity * a.frame.scale,
            a.frame.scale,
        )?;
        let mut a = self.actors.remove(&id).unwrap();
        self.ball_released(id, a.frame.eye);
        if let Some(slot) = a.selected {
            a.inventory[slot] = None;
        }
        self.unmount(id, &mut a);
        a.ball_ready = self.tick + cooldown;
        self.actors.insert(id, a);
        self.animation(id, "root");
        Ok(result)
    }
    /// Native 1-in-6 steal, constrained to a visible target five units ahead.
    /// `random` is a trusted host PRNG word, never an unchecked client choice.
    pub fn steal_basketball(
        &mut self,
        id: ActorId,
        random: u64,
        q: &mut impl Query,
    ) -> Result<bool> {
        ensure!(
            self.events.len() < 8192 && self.projectiles.len() < MAX_PROJECTILES,
            "Projectile/event budget"
        );
        let a = self.actors.get(&id).context("Unknown actor")?;
        ensure!(a.images[0].is_none(), "Hands must be empty");
        let Some(hit) = q.sweep(
            a.frame.eye,
            a.frame.eye + a.frame.direction.normalize() * 5.0,
            Filter {
                projectile_age_ticks: None,
                source: id,
                players: true,
                world_only: false,
            },
        ) else {
            return Ok(false);
        };
        let TargetId::Actor(target) = hit.target else {
            return Ok(false);
        };
        if !q.can_catch(id, target) {
            return Ok(false);
        }
        let a = self.actors.get(&target).context("Unknown target actor")?;
        if !a.images[0]
            .as_ref()
            .is_some_and(|e| self.ball(&e.image) == Some(Ball::Basketball))
            || !random.is_multiple_of(6)
        {
            return Ok(false);
        }
        let dir = a.frame.direction;
        let forward = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
        let jitter = Vec3::new(signed_jitter(random >> 8), 4.0, signed_jitter(random >> 24));
        let inherited = Vec3::new(
            a.frame.velocity.x,
            a.frame.velocity.y / 2.0,
            a.frame.velocity.z,
        );
        self.spawn(
            &native_id("projectile", "basketballProjectile"),
            target,
            a.frame.muzzle[0],
            (forward * 2.0 + jitter + inherited) * a.frame.scale,
            a.frame.scale,
        )?;
        let mut a = self.actors.remove(&target).unwrap();
        self.ball_released(target, a.frame.eye);
        if let Some(slot) = a.selected {
            a.inventory[slot] = None;
        }
        self.unmount(target, &mut a);
        a.ball_ready = self.tick + 120;
        self.events.push(Event::Sound {
            source: TargetId::Actor(target),
            profile: "impact1ASound".into(),
            position: a.frame.eye,
        });
        self.actors.insert(target, a);
        Ok(true)
    }
    /// Called for an authoritative eligible football tackle collision after minigame checks.
    /// Implements five-second immunity, fumble, three-second tumble and launch velocity.
    pub fn tackle(
        &mut self,
        victim: ActorId,
        attacker: ActorId,
        random: u64,
        q: &impl Query,
    ) -> Result<bool> {
        ensure!(
            self.events.len() < 8192 && self.projectiles.len() < MAX_PROJECTILES,
            "Projectile/event budget"
        );
        ensure!(victim != attacker, "Cannot self tackle");
        let a = self.actors.get(&victim).context("Unknown victim")?;
        let other = self.actors.get(&attacker).context("Unknown attacker")?;
        if self.tick < a.tackle_until || !q.can_affect(attacker, TargetId::Actor(victim)) {
            return Ok(false);
        }
        let mut velocity = other.frame.velocity;
        if velocity.length() < 3.0 {
            velocity = (a.frame.position - other.frame.position).normalize_or_zero() * 3.0;
        }
        if velocity.normalize_or_zero().dot(-Vec3::Y) >= 0.95 {
            velocity += Vec3::new(
                signed_jitter(random >> 16),
                0.0,
                signed_jitter(random >> 32),
            ) * 6.0;
        }
        let held = a.images[0]
            .as_ref()
            .is_some_and(|e| self.ball(&e.image) == Some(Ball::Football));
        if held {
            let multiplier = 2.0 + (random % 3) as f32;
            let jitter = Vec3::new(signed_jitter(random >> 8), 1.5, signed_jitter(random >> 24))
                * multiplier
                + Vec3::Y * 6.0;
            self.spawn(
                &native_id("projectile", "footballProjectile"),
                victim,
                a.frame.muzzle[0],
                (jitter + a.frame.velocity) * a.frame.scale,
                a.frame.scale,
            )?;
        }
        let mut a = self.actors.remove(&victim).unwrap();
        if held {
            self.ball_released(victim, a.frame.eye);
            if let Some(slot) = a.selected {
                a.inventory[slot] = None;
            }
            self.unmount(victim, &mut a);
            a.ball_ready = self.tick + 120;
        }
        a.tackle_until = self.tick + 600;
        self.actors.insert(victim, a);
        self.events.push(Event::Tumble {
            actor: victim,
            ticks: 360,
            velocity,
        });
        Ok(true)
    }
    pub fn touchdown(&mut self, id: ActorId, brick: u64) -> Result<bool> {
        ensure!(self.events.len() < 8192, "Command event budget");
        let held = self
            .image_state(id, 0)
            .and_then(|(i, _)| i.projectile.as_ref())
            .and_then(|p| self.pack.projectiles.get(p))
            .is_some_and(|p| p.sport_image.is_some());
        if held {
            self.events.push(Event::Touchdown { actor: id, brick });
        }
        Ok(held)
    }
    /// The stock sports ball `image` is ([`Stock::ball`]).
    fn ball(&self, image: &str) -> Option<Ball> {
        self.pack.images.get(image).and_then(|i| Stock::of(i).ball)
    }
    /// Whether the right hand holds a sports ball image.
    pub fn holds_ball(&self, id: ActorId) -> bool {
        self.image_state(id, 0)
            .and_then(|(image, _)| image.projectile.as_ref())
            .and_then(|p| self.pack.projectiles.get(p))
            .is_some_and(|p| p.sport_image.is_some())
    }
    /// Source Player::dropBall for death/tool/brick/spray switching. Call before retiring actor.
    pub fn drop_ball(&mut self, id: ActorId) -> Result<Option<u64>> {
        ensure!(
            self.events.len() < 8192 && self.projectiles.len() < MAX_PROJECTILES,
            "Projectile/event budget"
        );
        let a = self.actors.get(&id).context("Unknown actor")?;
        let Some(e) = a.images[0].as_ref() else {
            return Ok(None);
        };
        let Some(projectile) = self.pack.images[&e.image].projectile.clone() else {
            return Ok(None);
        };
        if self.pack.projectiles[&projectile].sport_image.is_none() {
            return Ok(None);
        }
        let forward = Vec3::new(a.frame.direction.x, 0.0, a.frame.direction.z).normalize_or_zero();
        let inherited = Vec3::new(
            a.frame.velocity.x,
            a.frame.velocity.y / 2.0,
            a.frame.velocity.z,
        );
        let velocity = forward * 2.0 + Vec3::Y * 4.0 + inherited;
        let position = a.frame.muzzle[0]
            + if self.ball(&e.image) == Some(Ball::Football) {
                forward
            } else {
                Vec3::ZERO
            };
        let p = self.spawn(&projectile, id, position, velocity, a.frame.scale)?;
        let mut a = self.actors.remove(&id).unwrap();
        if let Some(slot) = a.selected {
            a.inventory[slot] = None;
        }
        self.unmount(id, &mut a);
        a.ball_ready = self.tick + 60;
        self.actors.insert(id, a);
        Ok(Some(p))
    }
    /// Host reports failed ski spawning or external dismount/crash; presentation follows state.
    pub fn cancel_skis(&mut self, id: ActorId) -> Result<()> {
        ensure!(self.events.len() < 8192, "Command event budget");
        self.actors.get_mut(&id).context("Unknown actor")?.skiing = false;
        self.events.push(Event::SkiNodes {
            actor: id,
            visible: false,
        });
        Ok(())
    }
}
fn signed_jitter(word: u64) -> f32 {
    let v = 5.0 + ((word >> 1) & 65535) as f32 / 65535.0 * 5.0;
    if word & 1 == 0 { v } else { -v }
}
