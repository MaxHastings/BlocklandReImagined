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
        let a = self.actors.get(&id).context("Unknown actor")?;
        ensure!(
            a.images[0].is_none() && self.tick >= a.ball_ready,
            "Cannot hold ball now"
        );
        let mut image = def.image.clone();
        let horse = native_id(
            "image",
            &format!("horse{}", image.rsplit('.').next().unwrap()),
        );
        if a.frame.horse && self.pack.images.contains_key(&horse) {
            image = horse;
        }
        let mut a = self.actors.remove(&id).unwrap();
        self.mount(id, &mut a, &image, 0);
        a.ball_ready = self.tick + 36;
        self.actors.insert(id, a);
        Ok(())
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
        let (contains, projectile, velocity, cooldown) = match action {
            SportAction::BasketballPass => (
                "basketball",
                "basketballProjectile",
                dir * 20.0 + Vec3::Y * 4.0,
                36,
            ),
            SportAction::FootballLateral => (
                "football",
                "footballProjectile",
                forward * (-10.0) + Vec3::Y * 5.0 + a.frame.velocity,
                36,
            ),
            SportAction::SoccerPop => (
                "soccer",
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
                "soccer",
                "soccerBallProjectile",
                forward * 2.0 + a.frame.velocity * 2.0 + Vec3::Y * 5.0,
                240,
            ),
        };
        ensure!(
            image.image.contains(contains),
            "Sport action does not match held ball"
        );
        let result = self.spawn(
            &native_id("projectile", projectile),
            id,
            a.frame.muzzle[0],
            velocity * a.frame.scale,
            a.frame.scale,
        )?;
        let mut a = self.actors.remove(&id).unwrap();
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
            .is_some_and(|e| e.image.contains("basketball"))
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
            .is_some_and(|e| e.image.contains("football"));
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
            + if projectile.contains("football") {
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
