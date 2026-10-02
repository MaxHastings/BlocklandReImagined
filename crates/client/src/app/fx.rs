//! Weapon, actor and world effects fed by the session's cues.
use super::*;

impl App {
    /// Bricks whose kill cues wait for this frame's debris.
    pub(super) fn pending_kills(&self) -> BTreeSet<bri_world::BrickId> {
        self.brick_kills
            .iter()
            .filter_map(|cue| match cue.kind {
                bri_sim::presentation::CueKind::BrickKill { brick, .. } => Some(brick),
                _ => None,
            })
            .collect()
    }
    /// Queue a cue heard where the local player stands now.
    #[cfg(test)]
    pub(super) fn queue_weapon_cue(&mut self, cue: bri_sim::presentation::Cue) {
        let listener = listener(&self.motion, self.network_view());
        self.queue_cue_heard_at(cue, listener);
    }
    /// Queue a cue for `listener` (the local player's feet, None when not in
    /// a game): its caption shows only within earshot.
    pub(super) fn queue_cue_heard_at(&mut self, cue: bri_sim::presentation::Cue, listener: Option<Vec3>) {
        if matches!(
            cue.kind,
            bri_sim::presentation::CueKind::WeaponAnimation { .. }
        ) && cue.id > self.weapon_animation_cursor
        {
            self.weapon_animation_cursor = cue.id;
            if self.weapon_animation_cues.len() < bri_sim::presentation::MAX_CUES {
                self.weapon_animation_cues
                    .push_back((cue.clone(), 0., self.animation_time));
            } else {
                self.weapon_animation_drops = self.weapon_animation_drops.saturating_add(1);
            }
        }
        // Sitting is replicated state (`Vitals::sitting`); `/hug` is a pose
        // only the cue starts.
        if let bri_sim::presentation::CueKind::Emote { actor, name } = &cue.kind
            && name == "hug"
        {
            self.combat.hugging.insert(*actor, None);
        }
        self.audio.cue(&cue);
        if let Some(text) = caption(&cue, listener) {
            self.ui.apply(UiUpdate::Caption(text.into()));
        }
        // The engine explosion operation looks like v20's rocket blast.
        let cue = match &cue.kind {
            bri_sim::presentation::CueKind::Explosion { radius, .. } => {
                bri_sim::presentation::Cue {
                    kind: bri_sim::presentation::CueKind::WeaponEffect {
                        source: bri_weapons::TargetId::Map(0),
                        definition: "rocketexplosion".into(),
                        node: String::new(),
                        seconds: 0.,
                        image: None,
                        hand: None,
                        direction: None,
                        scale: (radius / 4.).clamp(0.5, 3.),
                    },
                    ..cue
                }
            }
            _ => cue,
        };
        // A beam fired with `muzzle` starts where this client draws that
        // player's muzzle.
        if let bri_sim::presentation::CueKind::Beam {
            to,
            color,
            width,
            seconds,
            muzzle,
        } = &cue.kind
        {
            let from = muzzle
                .and_then(|actor| self.world_items.held_muzzle(actor, 0))
                .unwrap_or(Vec3::from(cue.position));
            self.beams
                .add(from, Vec3::from(*to), *color, *width, *seconds);
        }
        if let bri_sim::presentation::CueKind::Tracer { actor, hand } = &cue.kind
            && self.shot_kicks.len() < 64
        {
            self.shot_kicks.push((*actor, *hand));
        }
        // A hitscan shot's streak, in the style of this client's copy of
        // the image, from where this client draws that hand's muzzle.
        if let bri_sim::presentation::CueKind::Tracer { actor, hand } = &cue.kind
            && let Some(image) = self.world_items.held_image(*actor, *hand)
            && let Some(tracer) = self
                .content
                .weapons
                .pack
                .images
                .get(image)
                .and_then(|i| i.shot.as_ref()?.hitscan.as_ref()?.tracer)
            && let Some(from) = self.world_items.held_muzzle(*actor, *hand)
        {
            self.beams.add(
                from,
                Vec3::from(cue.position),
                tracer.color,
                tracer.width,
                tracer.seconds,
            );
        }
        self.actor_effects.cue(&cue);
        self.explosion_shapes.cue(&cue);
        self.explosion_debris.cue(&cue);
        if matches!(cue.kind, bri_sim::presentation::CueKind::BrickKill { .. })
            && self.brick_kills.len() < bri_sim::presentation::MAX_CUES
        {
            self.brick_kills.push(cue.clone());
        }
        if matches!(
            cue.kind,
            bri_sim::presentation::CueKind::WeaponEffect { .. }
                | bri_sim::presentation::CueKind::WeaponShell { .. }
                | bri_sim::presentation::CueKind::WeaponAnimation { .. }
        ) {
            if self.weapon_cues.len() < bri_sim::presentation::MAX_CUES {
                self.weapon_cues.push_back((cue, 0.));
            } else {
                self.weapon_cue_drops = self.weapon_cue_drops.saturating_add(1);
            }
        }
    }
    /// Head images, jets and vehicle fire follow this frame's presented bodies.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn update_actor_effects(
        actor_effects: &mut crate::actor_effects::ActorEffects,
        assets: &crate::avatar::AvatarAssets,
        avatars: &BTreeMap<bri_world::OwnerId, crate::avatar::AvatarMesh>,
        vehicles: &crate::vehicles::ClientVehicles,
        vehicle_assets: &crate::vehicles::VehicleAssets,
        view: &network::View,
        presented: &BTreeMap<bri_world::OwnerId, bri_sim::player::PlayerState>,
        elapsed: f32,
        flare_visible: impl Fn(Vec3) -> Result<bool>,
        ground: impl Fn(Vec3, Vec3, f32) -> Option<(f32, Vec3)>,
    ) -> Result<()> {
        let body = |id: u64| {
            vehicles
                .frame(id)
                .map(|f| glam::Mat4::from_rotation_translation(f.rotation, f.position))
        };
        let jets: Vec<_> = presented
            .iter()
            .filter(|(owner, player)| {
                player.jetting
                    && view
                        .vitals
                        .get(owner)
                        .is_some_and(|v| v.alive && v.mounted.is_none())
            })
            .filter_map(|(owner, player)| {
                let avatar = avatars.get(owner)?;
                let feet = [
                    avatar.world_node(assets, "RFoot")?,
                    avatar.world_node(assets, "LFoot")?,
                ];
                Some((*owner, feet, Vec3::from(player.velocity)))
            })
            .collect();
        // The jet exhausts straight down (`ActorEffects::advance`); v20 casts
        // its ground dust along the same axis.
        let dust: Vec<_> = jets
            .iter()
            .flat_map(|(owner, feet, _)| {
                feet.iter().zip(0u8..).filter_map(|(m, i)| {
                    let origin = m.w_axis.truncate();
                    let hit = ground(
                        origin,
                        Vec3::NEG_Y,
                        crate::actor_effects::JET_GROUND_DISTANCE,
                    );
                    crate::actor_effects::jet_dust(*owner, i, origin, Vec3::NEG_Y, hit)
                })
            })
            .collect();
        actor_effects.update_jet_dust(&dust)?;
        // A wreck burns with its own damage emitters, from the replicated
        // destroyed state alone.
        let burning: Vec<_> = view
            .vehicles
            .values()
            .filter(|info| info.destroyed)
            .filter_map(|info| {
                let at = body(info.id)?;
                let d = vehicle_assets.definition(&info.definition)?;
                Some(
                    d.wreck_emitters()
                        .into_iter()
                        .map(move |e| (info.id, e, at)),
                )
            })
            .flatten()
            .collect();
        let pose = |anchor| match anchor {
            crate::actor_effects::Anchor::Actor { actor, mount } => avatars
                .get(&actor)?
                .mount_node(assets, mount as usize),
            crate::actor_effects::Anchor::Vehicle { vehicle } => body(vehicle),
            crate::actor_effects::Anchor::Muzzle { vehicle } => {
                let info = view.vehicles.get(&vehicle)?;
                let definition = vehicle_assets.definition(&info.definition)?;
                let frame = vehicles.frame(vehicle)?;
                crate::actor_effects::muzzle(
                    frame.position,
                    frame.rotation,
                    frame.turret_aim,
                    definition,
                )
            }
        };
        // `serverCmdLight` attaches `PlayerLight` to the player; v20's
        // `fxLight` follows the player's mount point 1 (the left hand, via
        // `getRenderMountTransform(1)`), so light and corona move with the arm.
        let mut lights = Vec::new();
        for (owner, _) in view.vitals.iter().filter(|(_, v)| v.light && v.alive) {
            let hand = avatars
                .get(owner)
                .and_then(|a| a.world_node(assets, "Mount1"))
                .map(|m| m.w_axis.truncate());
            let Some(position) = hand.or_else(|| {
                presented
                    .get(owner)
                    .map(|p| Vec3::from(p.feet) + Vec3::Y * 1.5)
            }) else {
                continue;
            };
            lights.push(crate::actor_effects::PlayerLight {
                actor: *owner,
                position,
                flare_visible: flare_visible(position)?,
            });
        }
        let swimmers: Vec<_> = presented
            .iter()
            .map(|(owner, player)| crate::actor_effects::Swimmer {
                actor: *owner,
                feet: Vec3::from(player.feet),
                height: bri_sim::water::body_height(
                    player,
                    &view.archetypes.tuning(player.archetype, player.scale),
                ),
                velocity: Vec3::from(player.velocity),
            })
            .collect();
        actor_effects.update_water(elapsed, &swimmers)?;
        let mut sprays = Vec::new();
        let mut trails = Vec::new();
        for (id, info) in &view.vehicles {
            let (Some(d), Some(frame)) = (
                vehicle_assets.definition(&info.definition),
                vehicles.frame(*id),
            ) else {
                continue;
            };
            sprays.extend(crate::actor_effects::tire_sprays(*id, d, frame));
            trails.extend(crate::actor_effects::vehicle_trails(*id, d, frame));
        }
        actor_effects.update_tires(&sprays)?;
        actor_effects.update_trails(&trails)?;
        // Other admins' free cameras; the controller does not see its own
        // (`firstPersonParticles = 0`).
        actor_effects.set_orbs(
            view.orbs
                .iter()
                .filter(|(owner, _)| {
                    **owner != view.owner
                        && view
                            .vitals
                            .get(owner)
                            .is_some_and(|v| v.control == bri_sim::session::ControlObject::Camera)
                })
                .map(|(owner, orb)| (*owner, Vec3::from(orb.eye)))
                .collect(),
        );
        actor_effects.advance(elapsed, pose, &jets, &burning, &lights)
    }
    pub(super) fn reset_weapon_effect_session(&mut self, session: RequestId, checkpoint_cursor: u64) {
        if self.weapon_effect_session == Some(session) {
            return;
        }
        self.weapon_effects.reset(checkpoint_cursor);
        self.actor_effects.reset(checkpoint_cursor);
        self.explosion_shapes.reset(checkpoint_cursor);
        self.beams.clear();
        self.explosion_debris.reset(checkpoint_cursor);
        self.weapon_shells.reset(checkpoint_cursor);
        self.weapon_cues
            .retain(|(cue, _)| cue.id > checkpoint_cursor);
        self.weapon_animation_cues
            .retain(|(cue, _, _)| cue.id > checkpoint_cursor);
        self.weapon_animation_cursor = checkpoint_cursor;
        self.weapon_effect_session = Some(session);
    }
    #[cfg(test)]
    pub(super) fn update_weapon_effects(
        &mut self,
        view: &bri_sim::session::WeaponView,
        elapsed: f32,
    ) -> Result<()> {
        Self::update_weapon_effect_parts(
            &mut self.weapon_effects,
            &mut self.weapon_cues,
            &self.world_items,
            view,
            elapsed,
        )
    }
    pub(super) fn update_weapon_effect_parts(
        weapon_effects: &mut crate::weapon_effects::WeaponEffects,
        weapon_cues: &mut VecDeque<(bri_sim::presentation::Cue, f32)>,
        world_items: &crate::world_items::WorldItems,
        view: &bri_sim::session::WeaponView,
        elapsed: f32,
    ) -> Result<()> {
        weapon_effects.sync(view)?;
        weapon_effects.sync_image_lights(view, |owner, hand| {
            world_items
                .mounted_transform(owner, hand)
                .map(|m| m.w_axis.truncate())
        })?;
        let elapsed = elapsed.min(0.25);
        for (_, age) in weapon_cues.iter_mut() {
            *age += elapsed;
        }
        let mut ready = Vec::new();
        while let Some((cue, age)) = weapon_cues.front() {
            let needs_pose = matches!(
                &cue.kind,
                bri_sim::presentation::CueKind::WeaponEffect {
                    image,
                    node,
                    seconds,
                    ..
                } if image.is_some() || !node.is_empty() || *seconds > 0.
            );
            if needs_pose && world_items.effect_pose(cue).is_none() && *age < 0.5 {
                break;
            }
            ready.push(weapon_cues.pop_front().unwrap().0);
        }
        weapon_effects.cues(&ready, |cue| world_items.effect_pose(cue))?;
        // queue_weapon_cue already transfers these same reliable IDs into App's
        // bounded avatar queue (including its own observable overflow policy).
        // Do not retain a second copy indefinitely in the effects adapter.
        drop(weapon_effects.take_avatar_animation_requests());
        weapon_effects.advance(elapsed, Vec3::ZERO, |cue| world_items.effect_pose(cue))?;
        Ok(())
    }
}
