//! Weapon, actor and world effects fed by the session's cues.
use super::*;
use bri_console::Clamp;
use glam::{Mat4, Quat};

/// How far past a blast's radius a knocked-out brick's centre may lie: the
/// blast reaches the brick's box, not its centre.
const BLAST_REACH_MARGIN: f32 = 4.0;

/// Presentation effects: weapon, actor and world effects, debris, fades and the cue queues feeding them.
pub(super) struct Effects {
    pub(super) effects: crate::effects::WorldEffects,
    pub(super) weapon_effects: crate::weapon_effects::WeaponEffects,
    pub(super) actor_effects: crate::actor_effects::ActorEffects,
    pub(super) explosion_shapes: crate::explosion_shapes::ExplosionShapes,
    /// Add-On beams: tracers, lasers.
    pub(super) beams: crate::beams::Beams,
    /// Marks projectiles leave on bricks: bullet holes.
    pub(super) decals: crate::decals::Decals,
    pub(super) footsteps: crate::footsteps::Footsteps,
    pub(super) clip_sounds: crate::clip_sounds::ClipSounds,
    /// What each vehicle's slipping tyres throw up, by surface.
    pub(super) wheel_contacts: crate::wheel_contacts::WheelContactEffects,
    pub(super) body_flashes: crate::body_flash::BodyFlashes,
    pub(super) body_shells: crate::body_shell::BodyShells,
    /// The Tutorial's target practice targets.
    pub(super) tutorial_targets: crate::tutorial_targets::TutorialTargets,
    /// The map's keyed movers (doors) where the host has them.
    pub(super) map_movers: crate::map_movers::ClientMovers,
    /// The map's scatter layers (grass, rocks) as instanced models.
    pub(super) map_scatter: crate::map_scatter::ClientScatter,
    /// Pieces thrown by explosions with `debris` (vehicle wrecks, tank shells).
    pub(super) explosion_debris: crate::explosion_debris::ExplosionDebris,
    /// Ejected gun casings (`stateEjectShell`) and their GPU model.
    pub(super) weapon_shells: crate::weapon_debris::WeaponDebris,
    pub(super) weapon_cues: VecDeque<(bri_sim::presentation::Cue, f32)>,
    pub(super) weapon_cue_drops: u64,
    /// Casing throws the console has been told of (the first few).
    pub(super) casings_told: u8,
    /// Screen flashes cued for players this frame (actor, image, cue id),
    /// and the one now showing with the time it has left.
    pub(super) flash_cues: Vec<(u64, String, u64)>,
    pub(super) flash_showing: Option<(f32, bri_ui::api::ScreenFlashDraw)>,
    /// Killed-brick debris (v20 brick explosions) and its GPU models.
    pub(super) brick_debris: crate::brick_debris::BrickDebris,
    /// Bodies gone limp when they died (`Look::ragdoll`).
    pub(super) ragdolls: crate::ragdoll::Ragdolls,
    pub(super) debris_models: crate::brick_debris::DebrisModels,
    /// Bricks easing to a new paint colour, drawn apart from their chunks.
    pub(super) brick_fades: crate::brick_fade::BrickFades,
    pub(super) fade_models: crate::brick_fade::FadeModels,
    pub(super) brick_kills: Vec<bri_sim::presentation::Cue>,
    pub(super) weapon_light_deferred: usize,
    /// The farthest sprites the last frame left out past the renderer's budget.
    pub(super) effect_sprites_cut: usize,
    pub(super) weapon_effect_session: Option<RequestId>,
    pub(super) weapon_frame_probe: bool,
    pub(super) weapon_frame_observation: Option<crate::weapon_effects::WeaponFrameObservation>,
    pub(super) weapon_animation_cues: VecDeque<(bri_sim::presentation::Cue, f32, f64)>,
    pub(super) weapon_animation_drops: u64,
    pub(super) weapon_animation_cursor: u64,
}

/// A blast announces at most `MAX_BLAST_DEBRIS` of the bricks it knocks
/// out; the rest are hidden by the same update with no cue of their
/// own. Like the announced ones they go at once instead of fading out:
/// every easing brick that stopped rendering, colliding and taking aim
/// within reach of one of `cues`' blasts.
pub(super) fn settle_blasted(
    fades: &mut crate::brick_fade::BrickFades,
    bricks: &bri_world::Bricks,
    cues: &[bri_sim::presentation::Cue],
) {
    let mut blasts: Vec<([f32; 3], f32)> = cues
        .iter()
        .filter_map(|cue| match cue.kind {
            bri_sim::presentation::CueKind::BrickKill {
                death: bri_sim::presentation::BrickDeath::Blast,
                origin,
                radius,
                ..
            } if radius > 0.5 => Some((origin, radius)),
            _ => None,
        })
        .collect();
    // One blast's cues name the same origin and radius.
    blasts.dedup();
    if blasts.is_empty() {
        return;
    }
    fades.settle_where(|id| {
        bricks.get(&id).is_some_and(|b| {
            let at = Vec3::from(b.position);
            !b.visible
                && !b.colliding
                && !b.raycast
                // The blast reaches a brick's box; its centre lies at
                // most half a big brick further out.
                && blasts.iter().any(|(origin, radius)| {
                    at.distance(Vec3::from(*origin)) <= radius + BLAST_REACH_MARGIN
                })
        })
    });
}

/// Most screen flashes kept between two frames.
const MAX_FLASH_CUES: usize = 16;
impl App {
    /// Bricks whose kill cues wait for this frame's debris.
    pub(super) fn pending_kills(&self) -> BTreeSet<bri_world::BrickId> {
        self.fx
            .brick_kills
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
    pub(super) fn queue_cue_heard_at(
        &mut self,
        cue: bri_sim::presentation::Cue,
        listener: Option<Vec3>,
    ) {
        if matches!(
            cue.kind,
            bri_sim::presentation::CueKind::WeaponAnimation { .. }
        ) && cue.id > self.fx.weapon_animation_cursor
        {
            self.fx.weapon_animation_cursor = cue.id;
            if self.fx.weapon_animation_cues.len() < bri_sim::presentation::MAX_CUES {
                self.fx.weapon_animation_cues.push_back((
                    cue.clone(),
                    0.,
                    self.avatar.animation_time,
                ));
            } else {
                self.fx.weapon_animation_drops = self.fx.weapon_animation_drops.saturating_add(1);
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
                        contact: None,
                        scale: (radius / 4.).clamped(0.5, 3.),
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
            self.fx
                .beams
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
            self.fx.beams.add(
                from,
                Vec3::from(cue.position),
                tracer.color,
                tracer.width,
                tracer.seconds,
            );
        }
        // A projectile's mark on the brick it hit, as this client's copy
        // of the projectile draws it.
        let mark = match &cue.kind {
            bri_sim::presentation::CueKind::Decal {
                brick,
                normal,
                projectile,
                scale,
            } => Some((*brick, None, normal, projectile, *scale)),
            bri_sim::presentation::CueKind::MoverDecal {
                mover,
                normal,
                projectile,
                scale,
            } => Some((None, Some(mover.as_str()), normal, projectile, *scale)),
            _ => None,
        };
        if let Some((brick, mover, normal, projectile, scale)) = mark
            && let Some(decal) = self
                .content
                .weapons
                .pack
                .projectiles
                .get(projectile)
                .and_then(|p| p.decal.as_ref())
        {
            let at = Vec3::from(cue.position);
            let normal = Vec3::from(*normal);
            let kind = self.building().and_then(|b| {
                b.surface_at(at + normal * 0.02, at - normal * 0.02)
                    .ok()
                    .flatten()
                    .map(|(_, kind)| kind.to_owned())
            });
            if let Some(decal) = decal.for_surface(kind.as_deref(), cue.id) {
                let texture = decal.texture.as_ref();
                let image = texture.and_then(|texture| self.item_assets.texture(texture));
                if texture.is_none() || image.is_some() {
                    self.fx
                        .decals
                        .add_textured(brick, at, normal, &decal, scale, image);
                    // A mark on a door is kept in the door's frame.
                    if let Some(placed) = mover.and_then(|id| {
                        self.fx
                            .map_movers
                            .transform_by_id(id)
                            .map(|placed| (id, placed))
                    }) {
                        self.fx.decals.attach_latest(placed.0, placed.1.inverse());
                    }
                }
            }
        }
        if let bri_sim::presentation::CueKind::BodyFlash {
            owner,
            color,
            seconds,
        } = &cue.kind
        {
            self.fx.body_flashes.add(*owner, *color, *seconds);
        }
        if let bri_sim::presentation::CueKind::BodyShell {
            owner,
            shell,
            strength,
            seconds,
        } = &cue.kind
        {
            // A shell the pack lacks, or whose noise did not load, shows nothing.
            let effects = &self.content.weapons.pack.effects;
            if let Some(shell) = effects.shells.iter().find(|s| &s.id == shell) {
                let image = |i: usize| self.item_assets.texture(&shell.noise[i].texture).cloned();
                if let (Some(a), Some(b)) = (image(0), image(1)) {
                    self.fx
                        .body_shells
                        .add(*owner, shell, [a, b], *strength, *seconds);
                }
            }
        }
        if let bri_sim::presentation::CueKind::ScreenFlash { actor, image } = &cue.kind
            && self.fx.flash_cues.len() < MAX_FLASH_CUES
        {
            self.fx.flash_cues.push((*actor, image.clone(), cue.id));
        }
        self.fx.actor_effects.cue(&cue);
        self.fx.explosion_shapes.cue(&cue);
        self.fx.explosion_debris.cue(&cue);
        self.fx
            .brick_debris
            .explosion_cue(&cue, &self.content.weapons.pack);
        if matches!(
            cue.kind,
            bri_sim::presentation::CueKind::BrickKill { .. }
                | bri_sim::presentation::CueKind::BrickSectionReady { .. }
                | bri_sim::presentation::CueKind::BrickSectionRetired { .. }
        ) && self.fx.brick_kills.len() < bri_sim::presentation::MAX_CUES
        {
            self.fx.brick_kills.push(cue.clone());
        }
        if matches!(
            cue.kind,
            bri_sim::presentation::CueKind::WeaponEffect { .. }
                | bri_sim::presentation::CueKind::WeaponShell { .. }
                | bri_sim::presentation::CueKind::WeaponAnimation { .. }
        ) {
            if self.fx.weapon_cues.len() < bri_sim::presentation::MAX_CUES {
                self.fx.weapon_cues.push_back((cue, 0.));
            } else {
                self.fx.weapon_cue_drops = self.fx.weapon_cue_drops.saturating_add(1);
            }
        }
    }
    /// Head images, jets and vehicle fire follow this frame's presented bodies.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn update_actor_effects(
        actor_effects: &mut crate::actor_effects::ActorEffects,
        shells: &mut crate::weapon_debris::WeaponDebris,
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
            crate::actor_effects::Anchor::Actor { actor, mount } => {
                avatars.get(&actor)?.mount_node(assets, mount as usize)
            }
            crate::actor_effects::Anchor::Vehicle { vehicle } => body(vehicle),
            crate::actor_effects::Anchor::Muzzle { vehicle } => {
                let info = view.vehicles.get(&vehicle)?;
                let definition = vehicle_assets.definition(&info.definition)?;
                let frame = vehicles.frame(vehicle)?;
                vehicles
                    .weapon_node(vehicle_assets, info, "muzzlePoint")
                    .map(|at| at * Mat4::from_quat(Quat::from_rotation_arc(Vec3::Y, Vec3::NEG_Z)))
                    .or_else(|| {
                        crate::actor_effects::muzzle(
                            frame.position,
                            frame.rotation,
                            frame.turret_aim,
                            definition,
                        )
                    })
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
            sprays.extend(crate::actor_effects::tire_skids(*id, d, frame).0);
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
        actor_effects.advance(elapsed, pose, &jets, &burning, &lights)?;
        // Casings a vehicle's gun threw leave from where it fires, carried
        // on with the vehicle.
        for ejected in actor_effects.take_ejected() {
            let socket = match ejected.anchor {
                crate::actor_effects::Anchor::Muzzle { vehicle } => view
                    .vehicles
                    .get(&vehicle)
                    .and_then(|info| vehicles.weapon_node(vehicle_assets, info, "ejectPoint"))
                    .map(|at| at * Mat4::from_quat(Quat::from_rotation_arc(Vec3::Y, Vec3::NEG_Z))),
                _ => None,
            };
            let Some(at) = socket.or_else(|| pose(ejected.anchor)) else {
                continue;
            };
            let carried = match ejected.anchor {
                crate::actor_effects::Anchor::Muzzle { vehicle }
                | crate::actor_effects::Anchor::Vehicle { vehicle } => {
                    vehicles.frame(vehicle).map_or(Vec3::ZERO, |f| f.velocity)
                }
                crate::actor_effects::Anchor::Actor { actor, .. } => presented
                    .get(&actor)
                    .map_or(Vec3::ZERO, |p| Vec3::from(p.velocity)),
            };
            if !actor_effects.eject_effect(&ejected.image, at, carried)? {
                shells.eject(&ejected.image, at, carried)?;
            }
        }
        Ok(())
    }
    /// Show the local player's screen flash (`bri_weapons::ScreenFlash`)
    /// for the time it lasts, in first person only, and take it away after.
    pub(super) fn update_screen_flash(&mut self, first_person: bool, seconds: f32) {
        let cues = std::mem::take(&mut self.fx.flash_cues);
        let owner = self.network_view().map(|v| v.owner);
        let mut update = None;
        if let Some((left, _)) = &mut self.fx.flash_showing {
            *left -= seconds;
            if *left <= 0.0 || !first_person {
                self.fx.flash_showing = None;
                update = Some(None);
            }
        }
        if !first_person {
            return;
        }
        let latest = cues.into_iter().rfind(|(actor, ..)| Some(*actor) == owner);
        if let Some((_, image, id)) = latest
            && let Some(flash) = self
                .content
                .weapons
                .pack
                .images
                .get(&image)
                .and_then(|i| i.screen_flash.as_ref())
            && let Some((key, [width, height])) = self.item_ui.screen_flash(&image)
        {
            // Each showing is turned over at random, from the cue's id.
            let mix = id.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 62;
            let draw = bri_ui::api::ScreenFlashDraw {
                key,
                size: [width as f32 * flash.scale, height as f32 * flash.scale],
                center: flash.center,
                flip: [flash.flip && mix & 1 == 1, flash.flip && mix & 2 == 2],
            };
            self.fx.flash_showing = Some((flash.seconds, draw));
            update = Some(Some(draw));
        }
        if let Some(draw) = update {
            self.ui.apply(UiUpdate::ScreenFlash(draw));
        }
    }
    pub(super) fn reset_weapon_effect_session(
        &mut self,
        session: RequestId,
        checkpoint_cursor: u64,
    ) {
        if self.fx.weapon_effect_session == Some(session) {
            return;
        }
        self.fx.weapon_frame_observation = None;
        self.fx.weapon_effects.reset(checkpoint_cursor);
        self.fx.actor_effects.reset(checkpoint_cursor);
        self.fx.explosion_shapes.reset(checkpoint_cursor);
        self.fx.beams.clear();
        self.fx.decals.clear();
        self.fx.body_flashes.clear();
        self.fx.body_shells.clear();
        self.fx.explosion_debris.reset(checkpoint_cursor);
        self.fx.weapon_shells.reset(checkpoint_cursor);
        self.fx
            .weapon_cues
            .retain(|(cue, _)| cue.id > checkpoint_cursor);
        self.fx
            .weapon_animation_cues
            .retain(|(cue, _, _)| cue.id > checkpoint_cursor);
        self.fx.weapon_animation_cursor = checkpoint_cursor;
        self.fx.weapon_effect_session = Some(session);
    }
    #[cfg(test)]
    pub(super) fn update_weapon_effects(
        &mut self,
        view: &bri_sim::session::WeaponView,
        elapsed: f32,
    ) -> Result<()> {
        Self::update_weapon_effect_parts(
            &mut self.fx.weapon_effects,
            &mut self.fx.weapon_cues,
            &self.world_items,
            None,
            view,
            elapsed,
        )
    }
    pub(super) fn update_weapon_effect_parts(
        weapon_effects: &mut crate::weapon_effects::WeaponEffects,
        weapon_cues: &mut VecDeque<(bri_sim::presentation::Cue, f32)>,
        world_items: &crate::world_items::WorldItems,
        building: Option<&crate::building::Building>,
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
            ready.push(
                world_items
                    .effect_cue(weapon_cues.pop_front().unwrap().0, weapon_effects.weapons()),
            );
        }
        weapon_effects.cues_with_local_view(
            &ready,
            |cue| world_items.effect_pose(cue),
            |cue| world_items.local_effect_affinity(cue),
        )?;
        // queue_weapon_cue already transfers these same reliable IDs into App's
        // bounded avatar queue (including its own observable overflow policy).
        // Do not retain a second copy indefinitely in the effects adapter.
        drop(weapon_effects.take_avatar_animation_requests());
        weapon_effects.advance_with_contacts(
            elapsed,
            Vec3::ZERO,
            |cue| world_items.effect_pose(cue),
            |from, to| {
                let length = from.distance(to);
                if !(0.0001..=2000.).contains(&length) {
                    return None;
                }
                let hit = building?.solid_segment(from, to).ok()??;
                Some(bri_fx_runtime::ContactHit {
                    fraction: (hit.distance / length).clamped(0., 1.),
                    normal: hit.normal,
                })
            },
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_net::protocol::PublicWorld;
    use bri_sim::presentation::{BrickDeath, Cue, CueKind};
    use bri_world::{Brick, ContentRef};

    fn world(knocked_out: bool) -> PublicWorld {
        let mut bricks = bri_world::Bricks::default();
        for (id, x) in [(1, 0.0), (2, 10.0), (3, 60.0)] {
            let mut brick = Brick::new(ContentRef::Resolved("a".into()), [x, 0.3, 0.0], 1);
            brick.color = 1;
            if knocked_out {
                (brick.visible, brick.raycast, brick.colliding) = (false, false, false);
            }
            bricks.insert(id, brick);
        }
        PublicWorld {
            name: "Test".into(),
            map_id: "map/test".into(),
            palette: vec![[0.0, 0.0, 0.0, 1.0], [1.0; 4]],
            bricks,
        }
    }

    /// A blast announces one of the bricks it knocks out; the others in its
    /// reach go at once with it instead of fading out. A brick out of its
    /// reach that stopped rendering still fades, as v20 eases it.
    #[test]
    fn bricks_a_blast_knocks_out_without_a_cue_do_not_fade() {
        let mut fades = crate::brick_fade::BrickFades::default();
        let (before, after) = (world(false), world(true));
        fades.observe(&before, &after, [1, 2, 3]);
        assert_eq!(fades.left_out(), BTreeSet::from([1, 2, 3]));
        let cue = Cue {
            id: 1,
            tick: 1,
            position: [0.0, 0.3, 0.0],
            kind: CueKind::BrickKill {
                brick: 1,
                death: BrickDeath::Blast,
                definition: ContentRef::Resolved("a".into()),
                quarter_turns: 0,
                color: 1,
                color_effect: 0,
                shape_effect: 0,
                print: None,
                origin: [0.0, 0.0, 0.0],
                force: 70.0,
                radius: 29.0,
            },
        };
        settle_blasted(&mut fades, &after.bricks, &[cue]);
        // Settled at their target: no longer easing, left to the chunks.
        assert_eq!(fades.left_out(), BTreeSet::from([3]));
    }
}
