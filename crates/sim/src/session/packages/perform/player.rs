//! What the engine does for the `player` operations
//! (`bri_package_runtime::ops::player`).
use super::*;

impl Perform for ops::SetAvatarColors {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetAvatarColors { player, colors } = self;
        let peer = session.peers.get_mut(&player).context("No such player")?;
        peer.uniform = colors;
        Ok(())
    }
}
impl Perform for ops::SetAvatarParts {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetAvatarParts {
            player,
            parts,
            face,
            decal,
        } = self;
        let peer = session.peers.get_mut(&player).context("No such player")?;
        peer.uniform_parts = (!parts.is_empty() || face.is_some() || decal.is_some())
            .then_some(UniformParts { parts, face, decal });
        Ok(())
    }
}
impl Perform for ops::Teleport {
    fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
        let peer = session.peers.get(&self.player).context("No such player")?;
        ensure!(peer.combat.alive, "Only living players can be moved");
        Ok(())
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::Teleport {
            player,
            position,
            yaw,
        } = self;
        let peer = session.peers.get_mut(&player).context("No such player")?;
        ensure!(peer.combat.alive, "Only living players can be moved");
        let yaw = yaw.unwrap_or_else(|| peer.player.state().yaw);
        peer.player
            .teleport(&mut session.simulation.physics, Vec3::from(position), yaw)?;
        peer.discard_inputs();
        Ok(())
    }
}
impl Perform for ops::SetArchetype {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetArchetype { player, archetype } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        if archetype.is_empty() {
            session.forget_body(player);
            return Ok(());
        }
        let chosen = session
            .archetypes
            .find(&archetype)
            .with_context(|| format!("No archetype {archetype}"))?;
        session
            .choose_body(
                player,
                chosen,
                crate::session::bodies::Chooser::AddOn,
                crate::session::bodies::Room::Forced,
            )
            .map(drop)
    }
}
impl Perform for ops::PushArchetype {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::PushArchetype { player, archetype } = self;
        let laid = session
            .archetypes
            .find(&archetype)
            .with_context(|| format!("No archetype {archetype}"))?;
        session.lay_archetype(player, laid)
    }
}
impl Perform for ops::PopArchetype {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::PopArchetype { player, archetype } = self;
        let lifted = session
            .archetypes
            .find(&archetype)
            .with_context(|| format!("No archetype {archetype}"))?;
        session.lift_archetype(player, lifted)
    }
}
impl Perform for ops::Respawn {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::Respawn { player } = self;
        let target = session
            .peers
            .get(&player)
            .context("No such player")?
            .combat
            .player;
        let effects = session
            .minigames
            .execute(bri_minigames::Command::ForceRespawn { target })
            .map_err(|e| anyhow::anyhow!("Respawn rejected: {e}"))?;
        session.apply_minigame_effects(effects)
    }
}
impl Perform for ops::RemoveBody {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::RemoveBody { player } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session.remove_body(player)
    }
}
impl Perform for ops::Control {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::Control { player, entity } = self;
        let peer = session.peers.get(&player).context("No such player")?;
        let Some(entity) = entity else {
            if matches!(peer.control, ControlObject::Entity(_)) {
                session.return_to_body(player)?;
            }
            return Ok(());
        };
        ensure!(peer.combat.alive, "Only living players can drive");
        ensure!(
            !session.seated(player),
            "A seated player cannot drive an entity"
        );
        let host = session
            .packages
            .as_mut()
            .context("No packages are enabled")?;
        let e = host
            .entities
            .get_mut(&entity)
            .with_context(|| format!("No entity {entity}"))?;
        ensure!(
            e.package == package,
            "Packages hand players only their own entities"
        );
        ensure!(
            !session
                .peers
                .iter()
                .any(|(o, p)| *o != player && p.control == ControlObject::Entity(entity)),
            "Another player drives that entity"
        );
        e.drive = Some(MoveInput {
            yaw: e.body.state().yaw,
            ..Default::default()
        });
        session.peers.get_mut(&player).expect("checked").control = ControlObject::Entity(entity);
        Ok(())
    }
}
impl Perform for ops::SetScrollMode {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetScrollMode { player, mode } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session.notify(player, Notice::ScrollMode(mode));
        Ok(())
    }
}
impl Perform for ops::SetTempLook {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetTempLook {
            player,
            look,
            seconds,
        } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session.temp_look(player, look, seconds);
        Ok(())
    }
}
impl Perform for ops::GiveItem {
    /// Whether the player can take the item: they are here, the item is
    /// one this server has, and they carry it already, have a free slot,
    /// or `equip` will drop the held tool to make room (`give_tool`).
    fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
        ensure!(session.peers.contains_key(&self.player), "No such player");
        ensure!(
            session.weapons.contains_item(&self.item),
            "`{}` is not an item of this server",
            self.item
        );
        let actor = session
            .weapons
            .actor(bri_weapons::ActorId(self.player))
            .context("Unknown connection")?;
        let holds = actor
            .inventory
            .iter()
            .any(|held| held.as_deref() == Some(self.item.as_str()));
        let room = actor.inventory.iter().any(Option::is_none);
        ensure!(
            holds || room || self.equip,
            "{}'s tools are full",
            session
                .peers
                .get(&self.player)
                .map_or("The player", |p| p.name.as_str())
        );
        Ok(())
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::GiveItem {
            player,
            item,
            equip,
        } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session.give_tool(player, &item, equip)
    }
}
impl Perform for ops::SetTools {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetTools { player, tools } = self;
        session.package_set_tools(player, tools)
    }
}
impl Perform for ops::TakeItem {
    /// Whether there is an item to take: `take_item` of an item the player
    /// does not carry does nothing, which a `require` counts as failure.
    fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
        ensure!(session.peers.contains_key(&self.player), "No such player");
        let carries = session
            .weapons
            .actor(bri_weapons::ActorId(self.player))
            .is_some_and(|a| {
                a.inventory
                    .iter()
                    .any(|held| held.as_deref() == Some(self.item.as_str()))
            });
        ensure!(carries, "The player does not carry `{}`", self.item);
        Ok(())
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::TakeItem { player, item } = self;
        session.package_take_item(player, &item)
    }
}
impl Perform for ops::DropItem {
    /// Whether the item can be put in the world: this server has it and
    /// the package has room for another lying about.
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.package_check_drop_item(cx.package, &self.item, self.data.as_ref())
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::DropItem {
            item,
            position,
            velocity,
            paint,
            data,
            seconds,
        } = self;
        session.package_drop_item(package, &item, position, velocity, paint, data, seconds)
    }
}
impl Perform for ops::RemoveDrop {
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.package_check_drop(cx.package, self.drop)
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::RemoveDrop { drop } = self;
        session.package_remove_drop(package, drop)
    }
}
impl Perform for ops::RenewDrop {
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.package_check_drop(cx.package, self.drop)
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        session.package_renew_drop(cx.package, self.drop, self.seconds)
    }
}
impl Perform for ops::NameDrop {
    fn check(&self, session: &Session, cx: OpCall<'_>) -> Result<()> {
        session.package_check_drop(cx.package, self.drop)
    }
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::NameDrop { drop, text, color } = self;
        session.package_name_drop(package, drop, text, color)
    }
}
impl Perform for ops::WearImage {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::WearImage {
            player,
            slot,
            image,
            paint,
            keep,
        } = self;
        let peer = session.peers.get(&player).context("No such player")?;
        ensure!(peer.combat.alive, "Only living players wear things");
        let actor = bri_weapons::ActorId(player);
        let worn = session.weapons.image_id(actor, slot).is_some();
        let host = session
            .packages
            .as_mut()
            .context("No packages are enabled")?;
        if let Some(image) = &image {
            ensure!(
                item_hooks::owns(&host.catalog, package, image),
                "`{image}` is not an image of `{package}` or an Add-On it depends on"
            );
        }
        // A kept image is its package's to change while it is worn
        // (one taken off another way, by death, keeps nothing).
        if let Some(keeper) = host.kept_worn.get(&(player, slot))
            && worn
        {
            ensure!(
                keeper == package,
                "`{keeper}` keeps the image worn in slot {slot}"
            );
        }
        let kept = keep && image.is_some();
        session.weapons.wear(actor, slot, image.as_deref(), paint)?;
        let host = session
            .packages
            .as_mut()
            .context("No packages are enabled")?;
        if kept {
            host.kept_worn.insert((player, slot), package.to_owned());
        } else {
            host.kept_worn.remove(&(player, slot));
        }
        Ok(())
    }
}
impl Perform for ops::SetFov {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetFov { player, fov } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session.notify(player, Notice::Fov(fov));
        Ok(())
    }
}
impl Perform for ops::SetCrouched {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetCrouched { player, crouched } = self;
        session
            .peers
            .get_mut(&player)
            .context("No such player")?
            .player
            .set_held_crouch(crouched);
        Ok(())
    }
}
impl Perform for ops::SetCanJump {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetCanJump { player, can } = self;
        session
            .peers
            .get_mut(&player)
            .context("No such player")?
            .player
            .set_can_jump(can);
        Ok(())
    }
}
impl Perform for ops::SetWalking {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetWalking { player, walking } = self;
        session
            .peers
            .get_mut(&player)
            .context("No such player")?
            .player
            .set_walking(walking);
        Ok(())
    }
}
impl Perform for ops::SetSpeedScale {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetSpeedScale { player, scale } = self;
        ensure!(
            scale.is_finite() && (0.0..=bri_package_runtime::ops::MAX_SPEED_SCALE).contains(&scale),
            "Invalid speed scale"
        );
        session
            .peers
            .get_mut(&player)
            .context("No such player")?
            .combat
            .speed_rule = scale;
        session.apply_speed(player)
    }
}
impl Perform for ops::SetSpeedModifier {
    fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
        let peer = session.peers.get(&self.player).context("No such player")?;
        ensure!(
            peer.combat.speed_mods.contains_key(&self.key)
                || peer.combat.speed_mods.len() < bri_package_runtime::ops::MAX_SPEED_KEYS,
            "Too many speed modifiers"
        );
        Ok(())
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetSpeedModifier { player, key, scale } = self;
        let peer = session.peers.get_mut(&player).context("No such player")?;
        if scale == 1.0 {
            peer.combat.speed_mods.remove(&key);
        } else {
            peer.combat.speed_mods.insert(key, scale);
        }
        session.apply_speed(player)
    }
}
impl Perform for ops::ScaleVelocity {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::ScaleVelocity {
            player,
            horizontal,
            vertical,
        } = self;
        session
            .peers
            .get_mut(&player)
            .context("No such player")?
            .player
            .scale_velocity(horizontal, vertical);
        Ok(())
    }
}
impl Perform for ops::SetProne {
    fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
        let peer = session.peers.get(&self.player).context("No such player")?;
        ensure!(
            !self.prone || peer.player.tuning().prone.is_some(),
            "This body cannot lie prone"
        );
        Ok(())
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetProne { player, prone } = self;
        let peer = session.peers.get_mut(&player).context("No such player")?;
        // Getting up under something low simply stays down: the rules may
        // ask again.
        peer.player
            .set_prone(&mut session.simulation.physics, prone);
        Ok(())
    }
}
impl Perform for ops::SetLean {
    fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
        let peer = session.peers.get(&self.player).context("No such player")?;
        ensure!(peer.player.tuning().lean.is_some(), "This body cannot lean");
        ensure!(
            self.lean.is_finite() && (-1.0..=1.0).contains(&self.lean),
            "Invalid lean: -1 to 1"
        );
        Ok(())
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetLean { player, lean } = self;
        let peer = session.peers.get_mut(&player).context("No such player")?;
        ensure!(peer.player.set_lean(lean), "This body cannot lean");
        Ok(())
    }
}
impl Perform for ops::SetDrive {
    fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
        ensure!(session.peers.contains_key(&self.player), "No such player");
        Ok(())
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetDrive {
            player,
            direction,
            accel,
            max_speed,
            seconds,
        } = self;
        let peer = session.peers.get_mut(&player).context("No such player")?;
        let length = direction[0].hypot(direction[1]);
        let drive = (seconds > 0.0 && length > 1e-6).then(|| bri_motor::player::Drive {
            direction: [direction[0] / length, direction[1] / length],
            accel,
            max_speed,
            seconds,
        });
        ensure!(
            peer.player.set_drive(drive),
            "Invalid drive: speed up to {}, up to {} seconds",
            bri_motor::player::MAX_SPEED,
            bri_motor::player::MAX_DRIVE_SECONDS
        );
        Ok(())
    }
}
impl Perform for ops::GiveAmmo {
    fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
        ensure!(session.peers.contains_key(&self.player), "No such player");
        session
            .weapons
            .check_reserve(bri_weapons::ActorId(self.player), &self.ammo)
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::GiveAmmo {
            player,
            ammo,
            rounds,
        } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session
            .weapons
            .give_ammo(bri_weapons::ActorId(player), &ammo, rounds as u32)
    }
}
impl Perform for ops::SetReserve {
    fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
        ensure!(session.peers.contains_key(&self.player), "No such player");
        session
            .weapons
            .check_reserve(bri_weapons::ActorId(self.player), &self.ammo)
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetReserve {
            player,
            ammo,
            rounds,
        } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        let reserve = rounds.map_or(bri_weapons::Reserve::Endless, |r| {
            bri_weapons::Reserve::Rounds(r as u32)
        });
        session
            .weapons
            .set_reserve(bri_weapons::ActorId(player), &ammo, reserve)
    }
}
impl Perform for ops::SetRounds {
    fn check(&self, session: &Session, _cx: OpCall<'_>) -> Result<()> {
        ensure!(session.peers.contains_key(&self.player), "No such player");
        session
            .weapons
            .check_set_rounds(bri_weapons::ActorId(self.player), &self.item)
    }
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetRounds {
            player,
            item,
            rounds,
        } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session
            .weapons
            .set_rounds(bri_weapons::ActorId(player), &item, rounds as u32)
    }
}
impl Perform for ops::Reload {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::Reload { player } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session
            .weapons
            .reload(bri_weapons::ActorId(player))
            .map(|_| ())
    }
}
impl Perform for ops::SetImageState {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetImageState { player, state } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session
            .weapons
            .set_state(bri_weapons::ActorId(player), &state)
    }
}
impl Perform for ops::QuickUse {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::QuickUse { player, item } = self;
        session.quick_use(player, &item)
    }
}
impl Perform for ops::SetImageAmmo {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetImageAmmo { player, ammo } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session.weapons.set_ammo(bri_weapons::ActorId(player), ammo)
    }
}
impl Perform for ops::SetImageLoaded {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetImageLoaded { player, loaded } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session
            .weapons
            .set_loaded(bri_weapons::ActorId(player), loaded)
    }
}
impl Perform for ops::SetAimScale {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetAimScale { player, scale } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session
            .weapons
            .set_aim_scale(bri_weapons::ActorId(player), scale)
    }
}
impl Perform for ops::KickView {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::KickView { player, pitch, yaw } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session.notify(player, Notice::ViewKick { pitch, yaw });
        Ok(())
    }
}
impl Perform for ops::MountImage {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::MountImage { player, image } = self;
        let peer = session.peers.get(&player).context("No such player")?;
        ensure!(peer.combat.alive, "Only living players hold things");
        let actor = bri_weapons::ActorId(player);
        match image {
            Some(image) => {
                let host = session
                    .packages
                    .as_ref()
                    .context("No packages are enabled")?;
                ensure!(
                    item_hooks::owns(&host.catalog, package, &image),
                    "`{image}` is not an image of `{package}` or an Add-On it depends on"
                );
                session.weapons.swap_image(actor, Some(&image))
            }
            None => session.weapons.swap_image(actor, None),
        }
    }
}
impl Perform for ops::Emote {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::Emote {
            player,
            image,
            skip_spam,
        } = self;
        let peer = session.peers.get(&player).context("No such player")?;
        ensure!(peer.combat.alive, "Only living players wear emotes");
        let feet = peer.player.state().feet;
        let tick = session.simulation.state().tick;
        let Some(image) = image else {
            session.emote_cue(
                tick,
                crate::presentation::CueKind::Emote {
                    actor: player,
                    name: String::new(),
                },
                feet,
            );
            return Ok(());
        };
        let host = session
            .packages
            .as_ref()
            .context("No packages are enabled")?;
        ensure!(
            item_hooks::owns(&host.catalog, package, &image),
            "`{image}` is not an image of `{package}` or an Add-On it depends on"
        );
        ensure!(
            session.weapons.pack.images.contains_key(&image),
            "There is no image `{image}`"
        );
        let peer = session.peers.get_mut(&player).expect("checked");
        if !skip_spam && !peer.combat.emote_allowed(tick) {
            // Dropped, as `Player::emote` returns; not an error.
            return Ok(());
        }
        session.emote_cue(
            tick,
            crate::presentation::CueKind::Emote {
                actor: player,
                name: image,
            },
            feet,
        );
        Ok(())
    }
}
impl Perform for ops::FollowPath {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::FollowPath { player, knots } = self;
        session.follow_path(
            package,
            player,
            knots.map(|k| k.iter().map(super::camera_path::Knot::from_op).collect()),
        )
    }
}
impl Perform for ops::ViewShake {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::ViewShake {
            player,
            direction,
            min,
            max,
            rate,
        } = self;
        session.take_cue(package)?;
        ensure!(session.peers.contains_key(&player), "No such player");
        session.notify(
            player,
            Notice::ViewShake {
                direction,
                min,
                max,
                rate,
            },
        );
        Ok(())
    }
}
impl Perform for ops::CameraShake {
    fn perform(self, session: &mut Session, cx: OpCall<'_>) -> Result<()> {
        let OpCall { package, .. } = cx;
        let ops::CameraShake {
            player,
            translation,
            rotation,
            seconds,
            hold,
        } = self;
        session.take_cue(package)?;
        ensure!(session.peers.contains_key(&player), "No such player");
        session.notify(
            player,
            Notice::CameraShake {
                translation,
                rotation,
                seconds,
                hold,
            },
        );
        Ok(())
    }
}
impl Perform for ops::Camera {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::Camera { player, camera } = self;
        let camera = match camera {
            bri_package_runtime::ops::CameraOp::Free => RulesCamera::Free,
            bri_package_runtime::ops::CameraOp::Point { at, distance } => {
                RulesCamera::Point(super::OrbitPoint {
                    at,
                    distance,
                    object: None,
                })
            }
            bri_package_runtime::ops::CameraOp::Object { object, distance } => {
                let at = session
                    .object_centre(object)
                    .context("No such object")?
                    .to_array();
                RulesCamera::Point(super::OrbitPoint {
                    at,
                    distance,
                    object: Some(object),
                })
            }
        };
        session.rules_camera(player, camera)
    }
}
impl Perform for ops::UnmountImage {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::UnmountImage { player } = self;
        session.put_away_hand(player)
    }
}
impl Perform for ops::SetScale {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetScale { player, scale } = self;
        ensure!(session.peers.contains_key(&player), "No such player");
        session.set_player_scale(player, scale)?;
        session.follow_player_mounts();
        Ok(())
    }
}
impl Perform for ops::SetLookLimits {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::SetLookLimits { player, limits } = self;
        let peer = session.peers.get_mut(&player).context("No such player")?;
        peer.look_limits = limits;
        Ok(())
    }
}
impl Perform for ops::OrbitCamera {
    fn perform(self, session: &mut Session, _cx: OpCall<'_>) -> Result<()> {
        let ops::OrbitCamera {
            player,
            body,
            orbit,
        } = self;
        session.orbit_camera(player, body, orbit)
    }
}
