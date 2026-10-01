//! What the engine does for the `damage` operations
//! (`bri_package_runtime::ops::damage`).
use super::*;

impl Perform for ops::Explode {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::Explode {
            position,
            radius,
            damage,
            brick_radius,
            explosion,
        } = self;
        session.explode(
            Vec3::from(position),
            radius,
            damage,
            brick_radius,
            explosion.as_deref(),
            package,
            caller,
        )
    }
}
impl Perform for ops::Damage {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::Damage {
            target,
            amount,
            by,
            damage_type,
        } = self;
        session.package_damage_op(package, target, amount, by, damage_type)
    }
}
impl Perform for ops::Fire {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::Fire {
            projectile,
            position,
            velocity,
            by,
        } = self;
        let tick = session.simulation.state().tick;
        let host = session
            .packages
            .as_mut()
            .context("No packages are enabled")?;
        ensure!(
            item_hooks::owns(&host.catalog, package, &projectile),
            "`{projectile}` is not a projectile of `{package}` or an Add-On it depends on"
        );
        let origin = package.to_string();
        ensure!(
            host.shares.shots.available(&origin, tick) >= 1,
            "Dropped: more than {PACKAGE_SHOTS} projectiles a second"
        );
        host.shares.shots.spend(&origin, tick, 1);
        let shooter = by
            .filter(|p| session.peers.contains_key(p))
            .unwrap_or(PACKAGE_SHOOTER);
        session.weapons.spawn(
            &projectile,
            bri_weapons::ActorId(shooter),
            Vec3::from(position),
            Vec3::from(velocity),
            1.0,
        )?;
        Ok(())
    }
}
impl Perform for ops::SpawnExplosion {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::SpawnExplosion {
            player,
            projectile,
            scale,
        } = self;
        let host = session
            .packages
            .as_ref()
            .context("No packages are enabled")?;
        ensure!(
            item_hooks::owns(&host.catalog, package, &projectile),
            "`{projectile}` is not a projectile of `{package}` or an Add-On it depends on"
        );
        let peer = session.peers.get(&player).context("No such player")?;
        ensure!(peer.combat.alive, "Only living players");
        let at = session.explosion_point(player)?;
        let origin = package.to_string();
        let tick = session.simulation.state().tick;
        let host = session
            .packages
            .as_mut()
            .context("No packages are enabled")?;
        ensure!(
            host.shares.shots.available(&origin, tick) >= 1,
            "Dropped: more than {PACKAGE_SHOTS} projectiles a second"
        );
        host.shares.shots.spend(&origin, tick, 1);
        session.weapons.spawn_explosion(
            &projectile,
            bri_weapons::ActorId(PACKAGE_SHOOTER),
            at,
            scale,
        )?;
        Ok(())
    }
}
impl Perform for ops::Heal {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::Heal { player, amount } = self;
        let max = {
            let peer = session.peers.get(&player).context("No such player")?;
            session
                .archetypes
                .resolve(peer.player.state().archetype)
                .max_health
        };
        let peer = session.peers.get_mut(&player).context("No such player")?;
        ensure!(peer.combat.alive, "Only the living heal");
        peer.combat.health = (peer.combat.health + amount).min(max);
        Ok(())
    }
}
