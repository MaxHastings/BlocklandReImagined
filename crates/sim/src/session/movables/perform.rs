//! What the engine does for the `physics` operations that move things
//! (`bri_package_runtime::ops::physics`): they change this system's own
//! holds, tethers and spawned vehicles, so they are performed here.
use super::*;
use crate::session::packages::perform::{OpCall, Perform};

impl Perform for ops::Push {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::Push {
            target,
            velocity,
            by,
        } = self;
        let by = by.filter(|b| session.peers.contains_key(b));
        // Anyone living may shove themselves, as v20's setVelocity
        // on a shot's own shooter did (a round turned back on them).
        let own = caller.or(by).is_some_and(|mover| {
            target == ObjectRef::Player(mover)
                && session.peers.get(&mover).is_some_and(|p| p.combat.alive)
        });
        if !own {
            session.ensure_may_move(caller, target, by)?;
        }
        let velocity = Vec3::from(velocity);
        session.push_object(target, velocity)?;
        if let Some(by) = by.or(caller) {
            session.credit(target, by);
        }
        Ok(())
    }
}
impl Perform for ops::Tumble {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::Tumble {
            player,
            velocity,
            by,
            seconds,
        } = self;
        let by = by.filter(|b| session.peers.contains_key(b));
        let target = ObjectRef::Player(player);
        session.ensure_may_move(caller, target, by)?;
        let peer = session.peers.get(&player).context("No such player")?;
        ensure!(peer.combat.alive, "Only living players tumble");
        let velocity = Vec3::from(velocity);
        match session.ridden(player) {
            // Already tumbling: fling the tumble.
            Some(v) if session.vehicles.mounted_family(player) == Some(veh::Family::Tumble) => {
                session.push_object(
                    ObjectRef::Vehicle(v.0),
                    velocity - session.object_velocity(target).unwrap_or_default(),
                )?;
            }
            Some(_) => anyhow::bail!("A seated player cannot tumble"),
            None => {
                session.tumble_player(player, velocity)?;
            }
        }
        if let Some(seconds) = seconds
            && let Some(v) = session.ridden(player)
            && session.vehicles.mounted_family(player) == Some(veh::Family::Tumble)
            && let Some(world) = &mut session.vehicles.world
        {
            world.set_tumble_ticks(veh::VehicleId(v.0), (seconds * 120.0).round() as u64)?;
        }
        if let Some(by) = by.or(caller) {
            session.credit(target, by);
            if let Some(v) = session.ridden(player) {
                session.credit(ObjectRef::Vehicle(v.0), by);
            }
        }
        Ok(())
    }
}
impl Perform for ops::Hold {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::Hold {
            player,
            target,
            distance,
            at,
            force,
            turn,
        } = self;
        ensure!(
            caller.is_none_or(|c| c == player),
            "A player holds things only by their own command"
        );
        session.start_hold(player, target, distance, at, force, turn)
    }
}
impl Perform for ops::Reach {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::Reach {
            player,
            distance,
            near,
            force,
            turn,
        } = self;
        ensure!(
            caller.is_none_or(|c| c == player),
            "A player reaches only by their own command"
        );
        let peer = session.peers.get(&player).context("No such player")?;
        ensure!(peer.combat.alive, "Only living players hold things");
        session.movables.reaching.insert(
            player,
            Reach {
                distance,
                near,
                force,
                turn,
            },
        );
        Ok(())
    }
}
impl Perform for ops::HoldDistance {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::HoldDistance { player, distance } = self;
        ensure!(
            caller.is_none_or(|c| c == player),
            "A player reels in only by their own command"
        );
        if let Some(hold) = session.movables.holds.get_mut(&player) {
            hold.distance = distance;
        }
        Ok(())
    }
}
impl Perform for ops::LetGo {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::LetGo { player } = self;
        session.movables.reaching.remove(&player);
        if let Some(hold) = session.movables.holds.remove(&player) {
            session.set_down(hold.target);
        }
        Ok(())
    }
}
impl Perform for ops::Tether {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::Tether {
            player,
            anchor,
            length,
            brick,
            reel,
            swing,
            object,
            keys,
            straight,
        } = self;
        ensure!(
            caller.is_none_or(|c| c == player),
            "A player is roped only by their own command"
        );
        ensure!(
            object != Some(ObjectRef::Player(player)),
            "A player cannot tie a rope to themselves"
        );
        let tie_object = match object {
            None => None,
            Some(target) => {
                ensure!(
                    session.target_alive(target),
                    "No living {target} to tie a rope to"
                );
                let in_body =
                    !matches!(target, ObjectRef::Player(_)) && session.held_body(target).is_some();
                let at = if in_body {
                    let b = &session.simulation.physics.bodies
                        [session.held_body(target).context("No body")?];
                    b.position().rotation.inverse() * (Vec3::from(anchor) - b.center_of_mass())
                } else {
                    Vec3::from(anchor)
                        - session
                            .object_centre(target)
                            .with_context(|| format!("No {target} to tie a rope to"))?
                };
                Some((target, at, in_body))
            }
        };
        ensure!(!session.seated(player), "A seated player cannot be roped");
        ensure!(
            session.ridden(player).is_none(),
            "A tumbling player cannot be roped"
        );
        if let Some(b) = brick {
            ensure!(
                session.simulation.state().bricks.contains_key(&b),
                "No brick {b} to tie a rope to"
            );
        }
        let peer = session.peers.get_mut(&player).context("No such player")?;
        ensure!(peer.combat.alive, "Only living players are roped");
        let grip =
            crate::player::Tether::grip(Vec3::from(peer.player.state().feet), peer.player.tuning());
        let spans = grip.distance(Vec3::from(anchor));
        let length = length.unwrap_or(spans.max(crate::player::MIN_TETHER_LENGTH));
        ensure!(
            spans <= length + TETHER_REACH,
            "A rope {length} long cannot reach that far"
        );
        peer.player.set_tether(Some(crate::player::Tether {
            anchor,
            length,
            target: length,
            reel: reel.unwrap_or(TETHER_REEL),
            swing: swing.unwrap_or(TETHER_SWING),
            drift: [0.0; 3],
            keys,
            winding: 0,
            straight,
        }))?;
        session.movables.tethers.insert(
            player,
            Tie {
                brick,
                object: tie_object,
            },
        );
        Ok(())
    }
}
impl Perform for ops::TetherLength {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { caller, .. } = cx;
        let ops::TetherLength { player, length } = self;
        ensure!(
            caller.is_none_or(|c| c == player),
            "A player reels their rope only by their own command"
        );
        if let Some(peer) = session.peers.get_mut(&player)
            && let Some(mut tether) = peer.player.state().tether
        {
            // The script takes over from the winch keys: letting go of
            // one later does not stop this reel.
            tether.target = length;
            tether.winding = 0;
            peer.player.set_tether(Some(tether))?;
        }
        Ok(())
    }
}
impl Perform for ops::Untether {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::Untether { player, keep } = self;
        if let Some(keep) = keep
            && let Some(peer) = session.peers.get_mut(&player)
            && let Some(tether) = peer.player.state().tether
        {
            // The grip slows them relative to what it was tied to.
            let drift = Vec3::from(tether.drift);
            let relative = Vec3::from(peer.player.state().velocity) - drift;
            peer.player.push(relative * (keep.clamp(0.0, 1.0) - 1.0));
        }
        session.untether(player);
        Ok(())
    }
}
impl Perform for ops::SpawnVehicle {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::SpawnVehicle {
            definition,
            position,
            yaw,
            velocity,
            owner,
        } = self;
        ensure!(
            session.owns_kind(package, &definition),
            "`{definition}` is not a vehicle of `{package}` or an Add-On it depends on"
        );
        let count = session
            .movables
            .spawned
            .values()
            .filter(|p| p.as_str() == package)
            .count();
        ensure!(
            count < MAX_PACKAGE_VEHICLES,
            "`{package}` already has {MAX_PACKAGE_VEHICLES} vehicles out"
        );
        let owner = owner.filter(|o| session.peers.contains_key(o));
        // Add-On vehicles count toward the server's vehicle limits.
        if let Err(text) = session.vehicle_room(owner.unwrap_or(0), &definition) {
            if let Some(owner) = owner {
                session.notify(
                    owner,
                    Notice::Center {
                        text: text.clone(),
                        seconds: 2.0,
                    },
                );
            }
            anyhow::bail!("{}", text.trim_start_matches('\u{E000}'));
        }
        let transform = veh::Transform {
            position,
            rotation: glam::Quat::from_rotation_y(-yaw).to_array(),
        };
        let id = session
            .spawn_transient(
                owner.unwrap_or(0),
                &definition,
                transform,
                Vec3::from(velocity),
                1.0,
            )
            .with_context(|| format!("`{definition}` could not spawn there"))?;
        session.movables.spawned.insert(id.0, package.to_string());
        if let Some(owner) = owner {
            session.credit(ObjectRef::Vehicle(id.0), owner);
        }
        Ok(())
    }
}
impl Perform for ops::RemoveVehicle {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall {
            package, caller, ..
        } = cx;
        let ops::RemoveVehicle { vehicle } = self;
        if session.movables.spawned.get(&vehicle).map(String::as_str) != Some(package) {
            // One of the Add-On's own kind from elsewhere (a spawn
            // brick): put away for the player it belongs to, or an
            // administrator.
            let found =
                session.vehicles.world.as_ref().and_then(|w| {
                    w.vehicle_snapshot(&session.simulation.physics, VehicleId(vehicle))
                });
            let found = found.with_context(|| format!("No vehicle {vehicle}"))?;
            ensure!(
                session.owns_kind(package, &found.definition),
                "Vehicle {vehicle} is not `{package}`'s to remove"
            );
            let by = caller.context("Only a player's request removes a built vehicle")?;
            ensure!(
                found.owner.0 == by || session.is_administrator(by),
                "Vehicle {vehicle} is not player {by}'s"
            );
        }
        session.movables.spawned.remove(&vehicle);
        session.remove_vehicle(VehicleId(vehicle))
    }
}
