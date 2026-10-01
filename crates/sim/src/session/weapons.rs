//! Fixed-tick host binding. Player damage/minigames and remaining event adapters
//! are tracked explicitly; this module does not grant free-build PvP authority.
use super::*;
use bri_weapons::{ActorId, Event as WeaponEvent, Frame, TargetId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountedImage {
    pub image: String,
    pub state: String,
    pub hand: u8,
    /// Palette index tinting a colour spray can.
    #[serde(default)]
    pub paint: Option<u8>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponView {
    pub static_items: Vec<crate::item_spawners::StaticItem>,
    pub images: BTreeMap<OwnerId, Vec<MountedImage>>,
    pub projectiles: Vec<bri_weapons::Projectile>,
    pub drops: Vec<bri_weapons::Drop>,
}
impl WeaponView {
    /// Projectiles something fired, without the spawn and death effects
    /// that ride the same projectile system (as in v20).
    pub fn fired(&self) -> impl Iterator<Item = &bri_weapons::Projectile> {
        self.projectiles.iter().filter(|p| {
            p.definition != super::SPAWN_PROJECTILE && p.definition != super::DEATH_PROJECTILE
        })
    }
    pub fn validate(&self, names: &BTreeMap<OwnerId, String>) -> Result<()> {
        ensure!(
            self.static_items.len() <= crate::item_spawners::MAX_STATIC_ITEMS
                && self.images.len() <= 64
                && self.images.keys().all(|id| names.contains_key(id))
                && self.projectiles.len() <= bri_weapons::MAX_PROJECTILES
                && self.drops.len() <= bri_weapons::MAX_DROPS,
            "Weapon view bounds/owners"
        );
        let text = |s: &str| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control);
        let vector = |v: Vec3| v.is_finite() && v.abs().max_element() < 1e7;
        for images in self.images.values() {
            ensure!(images.len() <= 2, "Too many mounted images");
            let mut hands = BTreeSet::new();
            for image in images {
                ensure!(
                    image.hand < 2
                        && hands.insert(image.hand)
                        && text(&image.image)
                        && text(&image.state),
                    "Invalid mounted image"
                );
            }
        }
        let mut ids = BTreeSet::new();
        for item in &self.static_items {
            item.validate()?;
            ensure!(ids.insert(item.brick), "Duplicate static item brick");
        }
        ids.clear(); // Brick identities are a different namespace from runtime entities.
        for p in &self.projectiles {
            ensure!(
                p.id > 0
                    && ids.insert(p.id)
                    && p.source.0 > 0
                    && text(&p.definition)
                    && vector(p.position)
                    && vector(p.origin)
                    && vector(p.velocity)
                    && p.velocity.length() <= 10000.
                    && (0.01..=100.).contains(&p.scale),
                "Invalid projectile view"
            );
        }
        for d in &self.drops {
            ensure!(
                d.id > 0
                    && ids.insert(d.id)
                    && d.source.0 > 0
                    && text(&d.item)
                    && vector(d.position)
                    && vector(d.velocity)
                    && d.rotation.is_finite()
                    && (d.rotation.length_squared() - 1.).abs() < 0.001
                    && (0.01..=100.).contains(&d.scale)
                    && d.velocity.length() <= 10000.,
                "Invalid item drop view"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) struct Trigger {
    down: bool,
    direction: Vec3,
    /// The click carried its own aim (`ActionAim`), not the body's facing.
    aimed: bool,
    /// A press that arrived while the trigger was already held: its release
    /// was lost (a dialog took the mouse-up), since a mouse cannot press
    /// twice without letting go. It lets go first, so it is a fresh click,
    /// with its own aim, for the image in hand.
    repress: bool,
    /// A re-press waits, released, until this tick at most for the image in
    /// hand to take a press (its state reacts to the trigger going down): a
    /// shot started under the stale hold waits out its timeout and would
    /// never see a one-tick release.
    ready_by: Option<u64>,
}

/// How long a click's own aim may wait for the shot it starts: longer than
/// any stock image takes from Ready to firing (the wrench's PreFire is two
/// ticks), short enough that a click nothing fired from is soon forgotten.
const CLICK_AIM_TICKS: u64 = 60;

/// One player's trigger presses waiting for the weapon step, and the aim of
/// the last aimed click until its shot is taken. v20 sent the trigger in the
/// same move as the look, so a shot a few ticks after the click still went
/// where the click aimed; here the look arrives separately, and may be late.
#[derive(Default)]
pub(super) struct Triggers {
    queue: VecDeque<Trigger>,
    click_aim: Option<(Vec3, u64)>,
}

impl Session {
    pub fn weapon_view(&self) -> WeaponView {
        WeaponView {
            static_items: self.item_spawners.items.values().cloned().collect(),
            images: self
                .peers
                .keys()
                .filter_map(|owner| {
                    let images: Vec<_> = (0..2)
                        .filter_map(|hand| {
                            self.weapons
                                .image_state(ActorId(*owner), hand)
                                .map(|(image, state)| MountedImage {
                                    image: image.id.clone(),
                                    state: state.name.clone(),
                                    hand,
                                    paint: self.weapons.image_paint(ActorId(*owner), hand),
                                })
                        })
                        .collect();
                    (!images.is_empty()).then_some((*owner, images))
                })
                .collect(),
            projectiles: self.weapons.projectiles().cloned().collect(),
            drops: self.weapons.drops().cloned().collect(),
        }
    }
    /// How fast each falling projectile definition drops, so clients can
    /// coast projectiles between the host's corrections.
    pub fn projectile_falls(&self) -> BTreeMap<String, f32> {
        self.weapons.projectile_falls()
    }
    /// Counts unfinished gameplay/presentation adapters instead of pretending
    /// that emitted intentions have already changed authoritative game state.
    pub fn weapon_adapter_gaps(&self) -> &BTreeMap<String, u64> {
        &self.weapon_gaps
    }

    /// Queue a trigger press or release. `aimed` says `direction` is the
    /// click's own aim rather than the body's facing when it arrived.
    ///
    /// The trigger is the player's held fire button, not the image's (v20's
    /// move trigger): it is accepted with nothing in hand and holds across
    /// tool, colour and image changes, so switching while holding fires the
    /// new image. Edges therefore stay queued through equips; dropping a
    /// queued release would leave the trigger stuck down.
    pub(super) fn weapon_trigger(
        &mut self,
        owner: OwnerId,
        down: bool,
        direction: Vec3,
        aimed: bool,
    ) -> Result<()> {
        if down
            && self.teleport_lockout(owner, super::admin_players::TELEPORT_WEAPON_LOCK_MS, false)
        {
            return Ok(());
        }
        let held = self
            .weapons
            .actor(ActorId(owner))
            .is_some_and(|a| a.trigger_held());
        let queue = &mut self.weapon_triggers.entry(owner).or_default().queue;
        let repress = down && queue.back().map_or(held, |t| t.down);
        if queue.len() >= 32 {
            ensure!(!down, "Weapon trigger queue full");
            let cancelled = queue.len() as u64;
            queue.clear();
            queue.push_back(Trigger {
                down: false,
                direction,
                aimed,
                repress: false,
                ready_by: None,
            });
            self.note_weapon_gap("trigger backlog cancelled for release", cancelled);
            return Ok(());
        }
        queue.push_back(Trigger {
            down,
            direction,
            aimed,
            repress,
            ready_by: None,
        });
        Ok(())
    }

    /// Trusted host entry point: let go of a player's fire button now,
    /// ahead of any presses still queued. Network players release through
    /// sequenced `WeaponTrigger` commands.
    pub fn release_trigger(&mut self, owner: OwnerId) -> Result<()> {
        ensure!(self.peers.contains_key(&owner), "Unknown connection");
        self.weapon_triggers.remove(&owner);
        self.weapons.trigger(ActorId(owner), false)
    }

    fn take_click_aim(&mut self, owner: OwnerId) {
        if let Some(t) = self.weapon_triggers.get_mut(&owner) {
            t.click_aim = None;
        }
    }

    pub(super) fn step_weapons(&mut self) -> Result<()> {
        let tick = self.simulation.state().tick;
        for (owner, peer) in &self.peers {
            let actor = ActorId(*owner);
            let expired = tick.saturating_sub(peer.last_input_tick) > 60;
            let takes_press = self
                .weapons
                .image_state(actor, 0)
                .is_none_or(|(_, state)| state.down.is_some());
            let triggers = self.weapon_triggers.entry(*owner).or_default();
            let trigger = if expired {
                triggers.queue.clear();
                triggers.click_aim = None;
                None
            } else {
                match triggers.queue.front().copied() {
                    // Let go now; the press waits until the image takes one.
                    Some(t) if t.repress => {
                        triggers.queue[0] = Trigger {
                            repress: false,
                            ready_by: Some(tick + CLICK_AIM_TICKS),
                            ..t
                        };
                        Some(Trigger {
                            down: false,
                            aimed: false,
                            ..t
                        })
                    }
                    Some(t) if t.ready_by.is_some_and(|by| tick < by) && !takes_press => None,
                    _ => triggers.queue.pop_front(),
                }
            };
            match trigger {
                Some(t) if t.down && t.aimed => {
                    triggers.click_aim = Some((t.direction, tick + CLICK_AIM_TICKS));
                }
                Some(t) if t.down => triggers.click_aim = None,
                _ => {}
            }
            if triggers.click_aim.is_some_and(|(_, until)| tick > until) {
                triggers.click_aim = None;
            }
            let state = peer.player.state();
            // A click's shot goes where the click aimed until it is taken.
            let direction = match (trigger, triggers.click_aim) {
                (Some(t), _) => t.direction,
                (None, Some((aim, _))) => aim,
                (None, None) => state.forward(),
            };
            let eye = peer.player.eye();
            // Temporary host mount origin until original animated mount poses
            // are bound. This is not claimed to reproduce authored muzzle offsets.
            self.weapons.set_frame(
                actor,
                Frame {
                    body_yaw: state.yaw,
                    position: Vec3::from(state.feet),
                    eye,
                    muzzle: [eye; 2],
                    direction,
                    velocity: Vec3::from(state.velocity),
                    grounded: state.grounded,
                    mount: match self.vehicles.mounted_family(*owner) {
                        None => bri_weapons::Mount::None,
                        Some(bri_vehicles::Family::Skis) => bri_weapons::Mount::Skis,
                        Some(_) => bri_weapons::Mount::Other,
                    },
                    scale: state.scale,
                    can_jet: peer.player.tuning().can_jet,
                    horse: self.archetypes.resolve(state.archetype).look.is_horse(),
                    middle: Some(Vec3::from(state.feet) + Vec3::Y * peer.player.middle()),
                    ..Frame::default()
                },
            )?;
            if expired {
                self.weapons.trigger(actor, false)?;
            } else if let Some(trigger) = trigger {
                self.weapons.trigger(actor, trigger.down)?;
            }
        }
        // Player and vehicle damage follow minigame policy, asked pair by
        // pair as hits happen; the policy borrows only players and
        // minigames, so it reads alongside the weapon world's step.
        let policy = combat::DamagePolicy {
            peers: &self.peers,
            minigames: &self.minigames,
            tick,
        };
        let vehicle_owners: BTreeMap<u64, OwnerId> = self
            .vehicles
            .world
            .as_ref()
            .map(|w| w.owners().map(|(id, owner, _)| (id.0, owner.0)).collect())
            .unwrap_or_default();
        // A player never hurts the entity they are driving.
        let driving: BTreeMap<u64, OwnerId> = self
            .peers
            .iter()
            .filter_map(|(owner, p)| match p.control {
                ControlObject::Entity(id) => Some((id, *owner)),
                _ => None,
            })
            .collect();
        let world = self.simulation.state();
        let affect = |source: ActorId, target| match target {
            TargetId::Entity(id) => driving.get(&id) != Some(&source.0),
            TargetId::Vehicle(vehicle) => {
                policy.vehicle(source.0, vehicle_owners.get(&vehicle).copied())
            }
            TargetId::Brick(id) => world
                .bricks
                .get(&id)
                .is_some_and(|b| b.owner == source.0 || b.owner == 0),
            // A package's own `fire` hurts any living player; the
            // package could `damage` them anyway.
            TargetId::Actor(target) if source.0 == packages::PACKAGE_SHOOTER => {
                policy.alive(target.0)
            }
            TargetId::Actor(target) => policy.player(source.0, target.0, false),
            _ => false,
        };
        let affect_radius = |source: ActorId, target| match target {
            TargetId::Actor(_) if source.0 == packages::PACKAGE_SHOOTER => affect(source, target),
            TargetId::Actor(target) => policy.player(source.0, target.0, true),
            other => affect(source, other),
        };
        // `passBallCheck`: a living player catches a ball thrown from the
        // same minigame, or when neither is in one (`sportIsInSameMinigame`).
        let games: BTreeMap<OwnerId, Option<bri_minigames::GameId>> = self
            .peers
            .keys()
            .map(|owner| (*owner, self.game_of(*owner)))
            .collect();
        let alive: BTreeSet<OwnerId> = self
            .peers
            .iter()
            .filter(|(_, p)| p.combat.alive)
            .map(|(owner, _)| *owner)
            .collect();
        let catch = |source: ActorId, target: ActorId| {
            alive.contains(&target.0)
                && games.get(&source.0).copied().flatten()
                    == games.get(&target.0).copied().flatten()
        };
        let shapes = self.tutorial_shape_targets();
        let mut query = crate::weapon_query::WeaponQuery {
            simulation: &self.simulation,
            affect: &affect,
            affect_radius: &affect_radius,
            catch: &catch,
            responses: &self.events.projectile_responses,
            truncated_targets: 0,
            shapes: &shapes,
        };
        let events = self.weapons.step(&mut query);
        let truncated = query.truncated_targets;
        if truncated > 0 {
            self.note_weapon_gap("radius targets truncated", truncated as u64);
        }
        for event in events {
            match event {
                // An Add-On's `local` sound is for its holder's ears only.
                WeaponEvent::Sound {
                    profile,
                    source: TargetId::Actor(actor),
                    ..
                } if self.peers.contains_key(&actor.0)
                    && self.weapons.pack.sound(&profile).is_some_and(|s| s.local) =>
                {
                    self.notify(actor.0, Notice::Sound(profile));
                }
                WeaponEvent::Sound {
                    profile, position, ..
                } => {
                    self.cues.emit(
                        tick,
                        crate::presentation::CueKind::WeaponSound { profile },
                        position.to_array(),
                    );
                }
                // The click's shot is taken: later shots of a held trigger
                // follow the body's look.
                WeaponEvent::Spawned { source, .. } => self.take_click_aim(source.0),
                WeaponEvent::Mounted { .. }
                | WeaponEvent::Unmounted { .. }
                | WeaponEvent::ImageState { .. }
                | WeaponEvent::Removed { .. }
                | WeaponEvent::Bounced { .. }
                | WeaponEvent::Dropped { .. }
                | WeaponEvent::BallCaught { .. }
                | WeaponEvent::BallRest { .. } => {} // authoritative view
                WeaponEvent::FootballCatch {
                    source,
                    catcher,
                    distance_feet,
                    was_thrown,
                } => self.football_catch(source.0, catcher.0, distance_feet, was_thrown),
                WeaponEvent::DropRemoved { drop } => self.forget_drop(drop),
                WeaponEvent::Diagnostic { message, .. } => {
                    if self.notices.len() == 64 {
                        self.notices.pop_front();
                    }
                    self.notices.push_back(format!("Weapon runtime: {message}"));
                }
                WeaponEvent::ToolFire {
                    actor,
                    command: Some(command),
                    ..
                } => {
                    self.take_click_aim(actor.0);
                    self.addon_tool_fire(actor.0, &command);
                }
                WeaponEvent::ToolFire { actor, image, .. } => {
                    self.take_click_aim(actor.0);
                    self.tool_fire(actor.0, &image)?;
                }
                // `brickDeployProjectile::onCollision` only moves the ghost
                // (client side here) and never calls the parent that raises
                // `onProjectileHit`; its explosion still shows.
                WeaponEvent::Contact { impact }
                    if impact
                        .definition
                        .eq_ignore_ascii_case("v20.projectile.brickdeployprojectile") => {}
                WeaponEvent::Contact { impact } => {
                    self.package_hit(&impact);
                    self.tutorial_contact(&impact);
                    self.spray_player(&impact);
                    if let TargetId::Brick(brick) = impact.target {
                        self.paint_contact(&impact)?;
                        self.special_projectile_hit(impact.source.0, brick, &impact.definition)?;
                        let source = Some(impact.source.0).filter(|o| self.peers.contains_key(o));
                        self.fire_input(brick, "onProjectileHit", source);
                    }
                }
                WeaponEvent::Effect {
                    source,
                    definition,
                    position,
                    node,
                    seconds,
                    image,
                    hand,
                    direction,
                    scale,
                } => self.cues.emit(
                    tick,
                    crate::presentation::CueKind::WeaponEffect {
                        source,
                        definition,
                        node,
                        seconds,
                        image,
                        hand,
                        direction: direction.map(|v| v.to_array()),
                        scale,
                    },
                    position.to_array(),
                ),
                WeaponEvent::Animation {
                    actor,
                    thread,
                    sequence,
                    image_hand,
                } => {
                    let position = self
                        .peers
                        .get(&actor.0)
                        .map_or([0.; 3], |p| p.player.state().feet);
                    self.cues.emit(
                        tick,
                        crate::presentation::CueKind::WeaponAnimation {
                            actor: actor.0,
                            thread,
                            sequence,
                            image_hand,
                        },
                        position,
                    );
                }
                WeaponEvent::Shell { actor, image, hand } => {
                    let position = self
                        .weapons
                        .actor(actor)
                        .map_or([0.; 3], |a| a.frame.muzzle[usize::from(hand)].to_array());
                    self.cues.emit(
                        tick,
                        crate::presentation::CueKind::WeaponShell {
                            actor: actor.0,
                            image,
                            hand,
                        },
                        position,
                    );
                }
                WeaponEvent::BrickImpact {
                    source,
                    target,
                    position,
                    parameters,
                } => {
                    let brick = match target {
                        Some(TargetId::Brick(id)) => Some(id),
                        _ => None,
                    };
                    self.blow_up_bricks(source.0, brick, position, &parameters)?;
                }
                WeaponEvent::Damage {
                    source,
                    target: TargetId::Actor(target),
                    amount,
                    kind,
                    position,
                } => {
                    let direct = self
                        .weapons
                        .pack
                        .damage_type(&kind)
                        .is_some_and(|t| t.direct);
                    self.damage_player_at(
                        target.0,
                        amount,
                        combat::DamageKind::Weapon { name: kind, direct },
                        shooter(source),
                        Some(position),
                    )?;
                }
                WeaponEvent::Impulse {
                    target: TargetId::Actor(target),
                    impulse,
                    ..
                } => self.push_player(target.0, impulse),
                WeaponEvent::Recoil { actor, velocity } => {
                    self.push_player(actor.0, velocity * combat::PLAYER_MASS)
                }
                WeaponEvent::Damage {
                    source,
                    target: TargetId::Entity(entity),
                    amount,
                    kind,
                    ..
                } => self.damage_entity(entity, amount, shooter(source), "weapon", &kind),
                WeaponEvent::Impulse {
                    target: TargetId::Entity(entity),
                    impulse,
                    ..
                } => self.push_entity(entity, impulse),
                // Entities do not burn: an Add-On that wants it answers
                // `on_entity_damage` for the fire's own damage instead.
                WeaponEvent::Burn {
                    target: TargetId::Entity(_),
                    ..
                } => {}
                WeaponEvent::Damage {
                    source,
                    target: TargetId::Vehicle(vehicle),
                    amount,
                    kind,
                    position,
                } => self.damage_vehicle(vehicle, amount, source.0, &kind, position)?,
                WeaponEvent::Impulse {
                    target: TargetId::Vehicle(vehicle),
                    impulse,
                    position,
                    ..
                } => self.blast_vehicle(vehicle, position, impulse),
                WeaponEvent::Key {
                    actor,
                    brick,
                    matched,
                } => {
                    let input = if matched {
                        "OnKeyMatch"
                    } else {
                        "OnKeyMismatch"
                    };
                    self.fire_input(brick, input, Some(actor.0));
                }
                WeaponEvent::Touchdown { actor, brick } => {
                    self.fire_input(brick, "onTouchdown", Some(actor.0));
                }
                WeaponEvent::BallHit { source, brick, .. } => {
                    self.fire_input(brick, "onBallHit", Some(source.0));
                }
                WeaponEvent::Burn {
                    target: TargetId::Actor(target),
                    seconds,
                    ..
                } => {
                    if let Some(peer) = self.peers.get(&target.0).filter(|p| p.combat.alive)
                        && !self.passenger_protected(target.0, bri_vehicles::DamageKind::Burn)
                    {
                        let feet = peer.player.state().feet;
                        self.burn_player(target.0, seconds);
                        self.cues.emit(
                            tick,
                            crate::presentation::CueKind::Burn {
                                actor: target.0,
                                seconds: seconds.min(300.0),
                            },
                            feet,
                        );
                    }
                }
                // `HorseRayProjectile::Damage`: the player becomes a horse
                // until respawn and is thrown off any mount.
                WeaponEvent::HorseTransform { target, .. } => {
                    if self.is_alive(target.0) {
                        self.set_player_archetype(
                            target.0,
                            crate::player_types::PlayerType::Horse.archetype(),
                        )?;
                    }
                }
                WeaponEvent::SportMovement { actor, locked } => {
                    self.sport_movement(actor.0, locked)?
                }
                WeaponEvent::StartSkis {
                    actor,
                    position,
                    velocity,
                    mount_after_ticks,
                } => self.start_skis(actor.0, position, velocity, mount_after_ticks)?,
                WeaponEvent::StopSkis { actor } => self.stop_skis(actor.0),
                // `commandToClient(..., 'CenterPrint', "\c4Can't use skis right now.", 2)`.
                WeaponEvent::SkisUnavailable { actor } => self.notify(
                    actor.0,
                    Notice::Center {
                        text: "\u{E004}Can't use skis right now.".into(),
                        seconds: 2.0,
                    },
                ),
                // Ski nodes follow the ski vehicle the avatar rides.
                WeaponEvent::SkiNodes { .. } => {}
                WeaponEvent::Tumble {
                    actor, velocity, ..
                } => self.tumble_player(actor.0, velocity)?,
                _ => self.note_weapon_gap("player/vehicle/minigame weapon adapter", 1),
            }
        }
        Ok(())
    }
    /// `CatchFootballMessage`: bottom prints for the passer and receiver, and
    /// a server-wide announcement when a thrown pass sets the record.
    fn football_catch(&mut self, source: OwnerId, catcher: OwnerId, feet: u32, thrown: bool) {
        let name = |owner: OwnerId| self.peers.get(&owner).map(|p| p.name.clone());
        let (Some(receiver), passer) = (name(catcher), name(source)) else {
            return;
        };
        let passer = passer.unwrap_or_default();
        let (color, white, red) = ("<color:ffff00>", "<color:FFFFFF>", "<color:FF0000>");
        let prefix = format!("<bitmap:base/client/ui/CI/star> {color}FOOTBALL -");
        let mut base = format!("{red}at{white} {feet}ft!");
        if thrown && source != catcher && feet > self.football_record {
            self.football_record = feet;
            self.system_chat(format!(
                "{color}{passer} {red}&{color} {receiver} {red}set a new football record, {white}{feet}ft!"
            ));
            base.push_str(&format!(" {red}<just:center>NEW RECORD!!!"));
        }
        let text = format!("{prefix} {red}To {color}{receiver} {base}");
        self.notify(
            source,
            Notice::Bottom {
                text,
                seconds: 5.0,
                hide_bar: false,
            },
        );
        let text = format!("{prefix} {red}From {color}{passer} {base}");
        self.notify(
            catcher,
            Notice::Bottom {
                text,
                seconds: 5.0,
                hide_bar: false,
            },
        );
    }
    /// `basketballShootImage::onMount`/`onUnMount`: a no-jet Blockhead lining
    /// up a shot becomes `BallShootPlayer` and gets its datablock back after.
    fn sport_movement(&mut self, owner: OwnerId, locked: bool) -> Result<()> {
        use crate::player_types::PlayerType;
        let Some(peer) = self.peers.get_mut(&owner) else {
            return Ok(());
        };
        let current = peer.player.state().archetype;
        let ball_shoot = PlayerType::BallShoot.archetype();
        if locked {
            if current != PlayerType::Horse.archetype()
                && current != ball_shoot
                && peer.sport_datablock.is_none()
            {
                peer.sport_datablock = Some(current);
                self.set_player_archetype(owner, ball_shoot)?;
            }
        } else if let Some(previous) = peer.sport_datablock.take()
            && current == ball_shoot
        {
            self.set_player_archetype(owner, previous)?;
        }
        Ok(())
    }
    fn note_weapon_gap(&mut self, name: &str, count: u64) {
        let entry = self.weapon_gaps.entry(name.into()).or_default();
        if *entry == 0 {
            if self.notices.len() == 64 {
                self.notices.pop_front();
            }
            self.notices
                .push_back(format!("Weapon integration pending: {name}"));
        }
        *entry = entry.saturating_add(count);
    }
}

/// The player a projectile's hit is credited to: none for a package's own.
fn shooter(source: ActorId) -> Option<OwnerId> {
    (source.0 != packages::PACKAGE_SHOOTER).then_some(source.0)
}
