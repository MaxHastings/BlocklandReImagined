//! Cosmetic weapon projection. Native definitions and authoritative views only.
//! Call reset on session replacement (with the checkpoint cue cursor), then sync,
//! cues and advance. The host provides animated attachment poses and consumes
//! shell/animation requests; this module never guesses a mount or gameplay hit.
use anyhow::{Result, ensure};
use bri_content::passage::Passages;
use bri_fx_runtime::{
    BlendMode, EffectHandle, EffectsLimits, EffectsPack, EffectsWorld, Recolor, SourceOptions,
    SourceTransform, StopMode,
};
use bri_package::health::{self, Problem};
use bri_sim::{
    presentation::{Cue, CueKind, MAX_CUES},
    session::WeaponView,
};
use glam::{Quat, Vec3};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

const MAX_MESSAGES: usize = 128;

#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub accepted_cues: u64,
    pub duplicate_cues: u64,
    pub missing_bindings: u64,
    pub missing_poses: u64,
    pub capacity_rejections: u64,
    pub host_queue_drops: u64,
    pub deferred_attachments: usize,
    pub messages: BTreeSet<String>,
}

#[derive(Clone)]
struct Binding {
    id: String,
    kind: Kind,
}
#[derive(Clone, Copy)]
enum Kind {
    Emitter,
    Light,
    Composite,
}
struct Attached {
    resource: String,
    handle: EffectHandle,
    /// Where the projectile was drawn last.
    position: Vec3,
}
/// A held image's rope being drawn: which end its sweep finished at.
struct RopeSweep {
    resource: String,
    handle: EffectHandle,
    at_anchor: bool,
}

/// A player holding an image while they hang on a rope: the rope runs from
/// the image's muzzle (`from`) to its anchor (`to`).
#[derive(Clone, Debug)]
pub struct HeldRope {
    pub owner: u64,
    pub image: String,
    pub from: Vec3,
    pub to: Vec3,
}
struct Timed {
    cue: Cue,
    handle: EffectHandle,
}

/// Shell and avatar/image animation need the host's original posed models.
/// Ownership and sequence/hand are preserved in the original reliable cue.
#[derive(Clone, Debug)]
pub enum HostRequest {
    Shell(Cue),
    Animation(Cue),
}

pub struct WeaponEffects {
    world: EffectsWorld,
    weapons: Arc<bri_weapons::Pack>,
    bindings: BTreeMap<String, Binding>,
    trails: BTreeMap<(u64, bool), Attached>,
    /// Mounted images' lights by holder and slot: the image and paint each
    /// was started for.
    image_lights: BTreeMap<(u64, u8), (String, Option<u8>, EffectHandle)>,
    ropes: BTreeMap<u64, RopeSweep>,
    timed: Vec<Timed>,
    pending: VecDeque<HostRequest>,
    cursor: u64,
    limits: EffectsLimits,
    /// World palette for `color<N>Paint*` spray effects.
    palette: Vec<[f32; 4]>,
    /// The world's portals: a trail carried through one goes on from the
    /// far side instead of streaking across between them.
    passages: Passages,
    pub diagnostics: Diagnostics,
}

impl WeaponEffects {
    /// Lowers typed projectile light fields to native lights; no legacy fields
    /// are interpreted. Original particle/texture/composite definitions survive.
    pub fn new(
        pack: Arc<EffectsPack>,
        weapons: Arc<bri_weapons::Pack>,
        limits: EffectsLimits,
    ) -> Result<Self> {
        Self::with_textures(pack, weapons, limits, |_| None)
    }
    /// As [`Self::new`], with an Add-On's own particle textures: a particle
    /// whose texture the effects pack lacks draws `texture(key)` (the item
    /// presentation's decoded image), fitted within [`ADD_ON_TEXTURE_SIDE`].
    pub fn with_textures<'t>(
        pack: Arc<EffectsPack>,
        weapons: Arc<bri_weapons::Pack>,
        limits: EffectsLimits,
        texture: impl Fn(&str) -> Option<&'t bri_render::scene::SceneImage>,
    ) -> Result<Self> {
        weapons.validate()?;
        let mut library = pack.library.clone();
        for p in weapons.projectiles.values().filter(|p| p.light_radius > 0.) {
            library.lights.push(bri_content::effects::Light {
                id: projectile_light(&p.id),
                name: String::new(),
                enabled: true,
                color: p.light_color,
                brightness: 1.,
                radius: p.light_radius,
                color_curves: None,
                brightness_curve: None,
                radius_curve: None,
                flare: None,
            });
        }
        for i in weapons.images.values() {
            let Some(light) = i.light else {
                continue;
            };
            library.lights.push(bri_content::effects::Light {
                id: image_light(&i.id),
                name: String::new(),
                enabled: true,
                color: light.color,
                brightness: 1.,
                radius: light.radius,
                color_curves: None,
                brightness_curve: None,
                radius_curve: None,
                flare: None,
            });
        }
        let mut manifest = pack.manifest.clone();
        let mut notes = Vec::new();
        let mut textures: Vec<_> = pack
            .textures
            .iter()
            .map(|t| bri_fx_runtime::pack::TextureImage {
                id: t.id.clone(),
                width: t.width,
                height: t.height,
                rgba: t.rgba.clone(),
            })
            .collect();
        add_add_on_textures(
            &weapons.effects,
            &mut library,
            &mut textures,
            texture,
            &mut notes,
        );
        add_pack_effects(&weapons.effects, &mut library, &mut manifest, &mut notes);
        let pack = EffectsPack::from_parts(library, manifest, textures)?;
        let mut bindings = BTreeMap::new();
        for (id, name, kind) in pack
            .library
            .emitters
            .iter()
            .map(|e| (&e.id, &e.name, Kind::Emitter))
            .chain(
                pack.library
                    .lights
                    .iter()
                    .map(|l| (&l.id, &l.name, Kind::Light)),
            )
        {
            insert_binding(&mut bindings, id, id, kind)?;
            if !name.is_empty() {
                insert_binding(&mut bindings, name, id, kind)?;
            }
            // Native conversion has stable authored-symbol IDs even when UI names differ.
            if let Some(symbol) = id
                .strip_prefix("v20/emitter/")
                .or_else(|| id.strip_prefix("v20/light/"))
            {
                insert_binding(&mut bindings, symbol, id, kind)?;
            }
        }
        for c in &pack.manifest.composites {
            insert_binding(&mut bindings, &c.id, &c.id, Kind::Composite)?;
            if let Some(symbol) = c.id.strip_prefix("v20/explosion/") {
                insert_binding(&mut bindings, symbol, &c.id, Kind::Composite)?;
            }
        }
        // An Add-On's explosion is named by its explosion's name, as the
        // base game's are; where that name is taken, the first keeps it.
        for c in &weapons.effects.explosions {
            let symbol = bri_weapons::effect_symbol(&c.id).to_ascii_lowercase();
            if pack.manifest.composites.iter().any(|x| x.id == c.id) {
                bindings.entry(symbol).or_insert(Binding {
                    id: c.id.clone(),
                    kind: Kind::Composite,
                });
            }
        }
        Ok(Self {
            world: EffectsWorld::new(pack, limits, 0x574541504f4e)?,
            weapons,
            bindings,
            trails: BTreeMap::new(),
            image_lights: BTreeMap::new(),
            ropes: BTreeMap::new(),
            timed: Vec::new(),
            pending: VecDeque::new(),
            cursor: 0,
            limits,
            palette: Vec::new(),
            passages: Passages::default(),
            diagnostics: Diagnostics {
                messages: notes.into_iter().take(MAX_MESSAGES).collect(),
                ..Default::default()
            },
        })
    }

    pub fn world(&self) -> &EffectsWorld {
        &self.world
    }
    pub fn cue_cursor(&self) -> u64 {
        self.cursor
    }
    pub fn attachment_count(&self) -> usize {
        self.trails.len()
    }
    pub fn timed_count(&self) -> usize {
        self.timed.len()
    }
    pub fn set_passages(&mut self, passages: &Passages) {
        if &self.passages != passages {
            self.passages = passages.clone();
        }
        self.world.set_passages(passages);
    }
    pub fn set_palette(&mut self, palette: &[[f32; 4]]) {
        if self.palette != palette {
            self.palette = palette.to_vec();
        }
    }
    /// Binding and source options for a cue effect. `color<N>Paint*` is the
    /// `setSprayCanColor` copy of the blue can's effect in palette colour N.
    fn resolve(&self, definition: &str) -> Option<(Binding, SourceOptions)> {
        if let Some(binding) = self.bindings.get(&definition.to_ascii_lowercase()) {
            return Some((binding.clone(), SourceOptions::default()));
        }
        let (paint, base) = bri_weapons::paint_effect_base(definition)?;
        let binding = self.bindings.get(&base.to_ascii_lowercase())?.clone();
        let color = self.palette.get(usize::from(paint))?;
        let recolor = paint_recolor(*color, matches!(binding.kind, Kind::Composite));
        Some((
            binding,
            SourceOptions {
                recolor: Some(recolor),
                ..Default::default()
            },
        ))
    }
    /// Whether a cue naming this effect would draw anything.
    pub fn resolves(&self, definition: &str) -> bool {
        self.resolve(definition).is_some()
    }
    /// Whether this effect is defined, whatever the palette: a
    /// `color<N>Paint*` copy is known when its `bluePaint*` base is.
    pub fn knows(&self, definition: &str) -> bool {
        let base = bri_weapons::paint_effect_base(definition).map(|(_, base)| base);
        self.bindings.contains_key(&definition.to_ascii_lowercase())
            || base.is_some_and(|b| self.bindings.contains_key(&b.to_ascii_lowercase()))
    }
    pub fn take_host_requests(&mut self) -> impl Iterator<Item = HostRequest> + '_ {
        self.pending.drain(..)
    }
    /// Transfer thread-2 avatar animation requests to App's per-actor pose queue.
    /// Shells and other animation threads remain pending for their own host adapters.
    /// Returning the original cues lets App acknowledge them by reliable cue ID.
    pub fn take_avatar_animation_requests(&mut self) -> Vec<Cue> {
        let mut transferred = Vec::new();
        self.pending.retain(|request| match request {
            HostRequest::Animation(cue)
                if matches!(cue.kind, CueKind::WeaponAnimation { thread: 2, .. }) =>
            {
                transferred.push(cue.clone());
                false
            }
            _ => true,
        });
        transferred
    }
    /// Checkpoint cursor prevents replaying pre-join one-shots. Trails are rebuilt
    /// from the current view without synthesizing explosions on removal.
    pub fn reset(&mut self, checkpoint_cursor: u64) {
        self.world.teardown();
        self.trails.clear();
        self.image_lights.clear();
        self.ropes.clear();
        self.timed.clear();
        self.pending.clear();
        self.cursor = checkpoint_cursor;
        self.diagnostics = Diagnostics::default();
    }
    fn missing(&mut self, message: String) {
        self.diagnostics.missing_bindings = self.diagnostics.missing_bindings.saturating_add(1);
        if self.diagnostics.messages.len() < MAX_MESSAGES {
            self.diagnostics.messages.insert(message);
        }
    }

    /// Reconcile stable projectile identities each render frame. The caller can
    /// supply interpolated positions in the view; particles retain previous poses.
    /// Drops/images have no implicit particle emissions: image-state cues own those.
    pub fn sync(&mut self, view: &WeaponView) -> Result<()> {
        ensure!(
            view.projectiles.len() <= bri_weapons::MAX_PROJECTILES,
            "Weapon effects view exceeds projectile limit"
        );
        let mut ids = BTreeSet::new();
        let mut desired = BTreeMap::new();
        for p in &view.projectiles {
            ensure!(
                p.id > 0
                    && ids.insert(p.id)
                    && p.position.is_finite()
                    && p.velocity.is_finite()
                    && p.position.abs().max_element() < 1e7
                    && p.velocity.length() <= 10000.,
                "Invalid effects projectile"
            );
            let Some(def) = self.weapons.projectiles.get(&p.definition) else {
                self.missing(format!("Missing projectile definition {}", p.definition));
                continue;
            };
            let trail = def.trail.clone();
            let light = (def.light_radius > 0.).then(|| projectile_light(&def.id));
            let transform = SourceTransform {
                position: p.position,
                velocity: p.velocity,
                rotation: if p.velocity.length_squared() > 1e-12 {
                    // Projectile emission points backward along motion in the
                    // Torque engine-family implementation (see research notes).
                    Quat::from_rotation_arc(Vec3::Y, -p.velocity.normalize())
                } else {
                    Quat::IDENTITY
                },
            };
            if !trail.is_empty() {
                if let Some(binding) = self
                    .bindings
                    .get(&trail.to_ascii_lowercase())
                    .filter(|b| matches!(b.kind, Kind::Emitter))
                {
                    desired.insert((p.id, false), (binding.id.clone(), transform));
                } else {
                    self.missing(format!("Missing projectile trail {trail}"));
                }
            }
            if let Some(light) = light {
                desired.insert((p.id, true), (light, transform));
            }
        }
        let removed: Vec<_> = self
            .trails
            .iter()
            .filter(|(key, a)| desired.get(key).is_none_or(|(id, _)| id != &a.resource))
            .map(|(key, _)| *key)
            .collect();
        for key in removed {
            let a = self.trails.remove(&key).unwrap();
            self.world.stop(a.handle, StopMode::Drain);
        }
        self.diagnostics.deferred_attachments = 0;
        for (key, (resource, transform)) in desired {
            if let Some(a) = self.trails.get_mut(&key) {
                // Keep a tombstone until the projectile disappears: a finite authored
                // emitter must not restart every network/render frame.
                if self.world.is_active(a.handle) {
                    let through = self.passages.bridge(a.position, transform.position);
                    if through.is_some() {
                        self.world.jump_source(a.handle, transform)?;
                    } else {
                        self.world.update_source(a.handle, transform)?;
                    }
                }
                a.position = transform.position;
                continue;
            }
            let result = if key.1 {
                self.world
                    .start_light(&resource, transform, SourceOptions::default())
            } else {
                self.world
                    .start_emitter(&resource, transform, SourceOptions::default())
            };
            match result {
                Ok(handle) => {
                    let position = transform.position;
                    self.trails.insert(
                        key,
                        Attached {
                            resource,
                            handle,
                            position,
                        },
                    );
                }
                Err(_) => {
                    self.diagnostics.deferred_attachments += 1;
                    self.diagnostics.capacity_rejections =
                        self.diagnostics.capacity_rejections.saturating_add(1);
                }
            }
        }
        Ok(())
    }

    /// Light up the images players wear or hold that give off light
    /// (`Image::light`), each at the image as drawn (`at(holder, slot)`),
    /// in the colour it is worn in when painted. One gone or changed goes
    /// out at once, as v20's image light does with its image.
    pub fn sync_image_lights(
        &mut self,
        view: &WeaponView,
        at: impl Fn(u64, u8) -> Option<Vec3>,
    ) -> Result<()> {
        let mut desired = BTreeMap::new();
        for (owner, images) in &view.images {
            for m in images {
                if self
                    .weapons
                    .images
                    .get(&m.image)
                    .is_some_and(|i| i.light.is_some())
                    && let Some(position) = at(*owner, m.hand).filter(|p| p.is_finite())
                {
                    desired.insert((*owner, m.hand), (m.image.clone(), m.paint, position));
                }
            }
        }
        let world = &mut self.world;
        self.image_lights.retain(|key, (image, paint, handle)| {
            let keep = desired
                .get(key)
                .is_some_and(|(i, p, _)| i == image && p == paint)
                && world.is_active(*handle);
            if !keep {
                world.stop(*handle, StopMode::Immediate);
            }
            keep
        });
        for (key, (image, paint, position)) in desired {
            let transform = SourceTransform {
                position,
                ..Default::default()
            };
            if let Some((_, _, handle)) = self.image_lights.get(&key) {
                self.world.update_source(*handle, transform)?;
                continue;
            }
            let options = SourceOptions {
                paint: paint
                    .and_then(|p| self.palette.get(usize::from(p)))
                    .map(|c| [c[0], c[1], c[2]].map(|v| v.clamp(0., 1.))),
                ..Default::default()
            };
            match self
                .world
                .start_light(&image_light(&image), transform, options)
            {
                Ok(handle) => {
                    self.image_lights.insert(key, (image, paint, handle));
                }
                Err(_) => {
                    self.diagnostics.capacity_rejections =
                        self.diagnostics.capacity_rejections.saturating_add(1);
                }
            }
        }
        Ok(())
    }

    /// Draw the ropes of players whose held image has one (`Image::rope`)
    /// for a frame `dt` seconds long. v20 Add-Ons drew a rope by firing a
    /// projectile from the muzzle to the rope's end every few milliseconds,
    /// its trail tracing the rope; here that trail's emitter sweeps the
    /// whole rope each frame, from the end it reached last, with its
    /// emission clock sped up so it lays as many particles along the rope
    /// as the projectile flying it at the rope's `speed` would. Nothing is
    /// sent for it.
    pub fn sync_ropes(&mut self, ropes: &[HeldRope], dt: f32) -> Result<()> {
        let mut desired = BTreeMap::new();
        for r in ropes {
            let Some(rope) = self
                .weapons
                .images
                .get(&r.image)
                .and_then(|i| i.rope.as_ref())
            else {
                continue;
            };
            if !(r.from.is_finite() && r.to.is_finite()) {
                continue;
            }
            let trail = self
                .weapons
                .projectiles
                .get(&rope.projectile)
                .map(|p| p.trail.to_ascii_lowercase())
                .unwrap_or_default();
            let Some(binding) = self
                .bindings
                .get(&trail)
                .filter(|b| matches!(b.kind, Kind::Emitter))
            else {
                self.missing(format!("Missing rope trail of {}", rope.projectile));
                continue;
            };
            desired.insert(r.owner, (binding.id.clone(), r, rope.speed));
        }
        let world = &mut self.world;
        self.ropes.retain(|owner, sweep| {
            let keep = desired
                .get(owner)
                .is_some_and(|(id, _, _)| *id == sweep.resource)
                && world.is_active(sweep.handle);
            if !keep {
                world.stop(sweep.handle, StopMode::Drain);
            }
            keep
        });
        for (owner, (resource, r, speed)) in desired {
            let length = r.from.distance(r.to);
            let options = SourceOptions {
                time_scale: (length / (speed * dt.max(1e-3))).clamp(1e-3, 1000.),
                ..SourceOptions::default()
            };
            let place = |position: Vec3| SourceTransform {
                position,
                rotation: Quat::IDENTITY,
                velocity: Vec3::ZERO,
            };
            match self.ropes.get_mut(&owner) {
                Some(sweep) => {
                    sweep.at_anchor = !sweep.at_anchor;
                    let end = if sweep.at_anchor { r.to } else { r.from };
                    self.world.update_source(sweep.handle, place(end))?;
                    self.world.update_options(sweep.handle, options)?;
                }
                None => match self.world.start_emitter(&resource, place(r.from), options) {
                    Ok(handle) => {
                        self.world.update_source(handle, place(r.to))?;
                        self.ropes.insert(
                            owner,
                            RopeSweep {
                                resource,
                                handle,
                                at_anchor: true,
                            },
                        );
                    }
                    Err(_) => {
                        self.diagnostics.capacity_rejections =
                            self.diagnostics.capacity_rejections.saturating_add(1);
                    }
                },
            }
        }
        Ok(())
    }

    /// Consume an ordered reliable stream once. Duplicates/older deliveries are
    /// ignored even after their particles expire. Validate the complete batch
    /// before advancing its cursor. Missing/capacity-limited cosmetics are counted
    /// and consumed, never replayed unexpectedly on a later frame.
    /// The pose callback receives the whole cue, preserving future pose metadata.
    pub fn cues(
        &mut self,
        cues: &[Cue],
        mut pose: impl FnMut(&Cue) -> Option<SourceTransform>,
    ) -> Result<()> {
        ensure!(cues.len() <= MAX_CUES, "Weapon effects cue batch too large");
        for cue in cues {
            cue.validate()?;
        }
        ensure!(
            cues.windows(2).all(|p| p[0].id <= p[1].id),
            "Unordered reliable presentation batch"
        );
        for cue in cues {
            if cue.id <= self.cursor {
                self.diagnostics.duplicate_cues = self.diagnostics.duplicate_cues.saturating_add(1);
                continue;
            }
            self.cursor = cue.id;
            match &cue.kind {
                CueKind::WeaponShell { .. } | CueKind::WeaponAnimation { .. } => {
                    if self.pending.len() == MAX_CUES {
                        self.diagnostics.host_queue_drops =
                            self.diagnostics.host_queue_drops.saturating_add(1);
                    } else {
                        self.pending.push_back(
                            if matches!(cue.kind, CueKind::WeaponShell { .. }) {
                                HostRequest::Shell(cue.clone())
                            } else {
                                HostRequest::Animation(cue.clone())
                            },
                        );
                    }
                }
                CueKind::WeaponEffect {
                    definition,
                    node,
                    seconds,
                    image,
                    direction,
                    ..
                } => {
                    let Some((binding, options)) = self.resolve(definition) else {
                        self.missing(format!("Missing cue effect {definition}"));
                        continue;
                    };
                    let attached = image.is_some() || !node.is_empty() || *seconds > 0.;
                    let transform = if attached {
                        let Some(transform) = pose(cue) else {
                            self.diagnostics.missing_poses =
                                self.diagnostics.missing_poses.saturating_add(1);
                            continue;
                        };
                        transform
                    } else {
                        SourceTransform {
                            position: cue.position.into(),
                            rotation: direction.map_or(Quat::IDENTITY, |v| {
                                Quat::from_rotation_arc(Vec3::Y, Vec3::from(v))
                            }),
                            ..Default::default()
                        }
                    };
                    if !valid_transform(transform) {
                        self.diagnostics.missing_poses =
                            self.diagnostics.missing_poses.saturating_add(1);
                        continue;
                    }
                    let result = match binding.kind {
                        Kind::Composite => {
                            self.world.play_composite(&binding.id, transform, options)
                        }
                        Kind::Emitter | Kind::Light => {
                            let authored_finite = matches!(binding.kind, Kind::Emitter)
                                && self
                                    .world
                                    .pack()
                                    .library
                                    .emitters
                                    .iter()
                                    .any(|e| e.id == binding.id && e.lifetime > 0.);
                            if *seconds == 0. && !authored_finite {
                                self.missing(format!(
                                    "No finite duration for cue effect {definition}"
                                ));
                                continue;
                            }
                            let started = if matches!(binding.kind, Kind::Emitter) {
                                self.world.start_emitter(&binding.id, transform, options)
                            } else {
                                self.world.start_light(
                                    &binding.id,
                                    transform,
                                    SourceOptions::default(),
                                )
                            };
                            started.and_then(|h| {
                                if *seconds > 0. {
                                    self.world.set_remaining_lifetime(h, *seconds)?;
                                }
                                Ok(vec![h])
                            })
                        }
                    };
                    match result {
                        Ok(handles) => {
                            self.diagnostics.accepted_cues =
                                self.diagnostics.accepted_cues.saturating_add(1);
                            if attached {
                                for handle in handles {
                                    if self.world.is_active(handle) {
                                        self.timed.push(Timed {
                                            cue: cue.clone(),
                                            handle,
                                        });
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            self.diagnostics.capacity_rejections =
                                self.diagnostics.capacity_rejections.saturating_add(1);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Missing/unmounted sources stop immediately while already emitted particles
    /// drain. A finite source's clock is capped inside EffectsWorld, not after dt.
    pub fn advance(
        &mut self,
        dt: f32,
        wind: Vec3,
        mut pose: impl FnMut(&Cue) -> Option<SourceTransform>,
    ) -> Result<()> {
        ensure!(
            dt.is_finite() && (0.0..=86400.).contains(&dt) && wind.is_finite(),
            "Invalid weapon effects timestep/wind"
        );
        self.timed.retain(|t| self.world.is_active(t.handle));
        for timed in &self.timed {
            if let Some(transform) = pose(&timed.cue).filter(|t| valid_transform(*t)) {
                self.world.update_source(timed.handle, transform)?;
            } else {
                self.world.stop(timed.handle, StopMode::Drain);
                self.diagnostics.missing_poses = self.diagnostics.missing_poses.saturating_add(1);
            }
        }
        self.world.advance(dt, wind)?;
        self.timed.retain(|t| self.world.is_active(t.handle));
        debug_assert!(self.timed.len() <= self.limits.sources);
        Ok(())
    }
}

/// `setSprayCanColor`: the explosion and droplet copies take the palette RGB
/// (at least 8/255 per channel when translucent) and draw translucent colours
/// additively (`useInvAlpha = 0`). The nozzle's `bluePaintEmitter` is tinted
/// by the image's colour shift through `useEmitterColors`; its particle keeps
/// its authored blend.
fn paint_recolor(color: [f32; 4], explosion: bool) -> Recolor {
    let opaque = color[3] > 0.99;
    let mut rgb = [color[0], color[1], color[2]].map(|c| c.clamp(0., 1.));
    if !opaque && rgb.iter().all(|c| *c < 8. / 255.) {
        rgb = [8. / 255.; 3];
    }
    Recolor {
        rgb,
        blend: explosion.then_some(if opaque {
            BlendMode::Alpha
        } else {
            BlendMode::Additive
        }),
    }
}
fn projectile_light(id: &str) -> String {
    format!("weapon/projectile-light/{id}")
}
fn image_light(id: &str) -> String {
    format!("weapon/image-light/{id}")
}
fn valid_transform(t: SourceTransform) -> bool {
    t.position.is_finite()
        && t.velocity.is_finite()
        && t.rotation.is_finite()
        && (t.rotation.length_squared() - 1.).abs() < 0.001
}
fn insert_binding(
    bindings: &mut BTreeMap<String, Binding>,
    alias: &str,
    id: &str,
    kind: Kind,
) -> Result<()> {
    let key = alias.to_ascii_lowercase();
    ensure!(
        bindings.get(&key).is_none_or(|b| b.id == id),
        "Ambiguous native effect alias {alias}"
    );
    bindings.insert(
        key,
        Binding {
            id: id.into(),
            kind,
        },
    );
    Ok(())
}

/// Add an Add-On weapons pack's own effects to the library the weapon
/// effects draw from. An id already there keeps its definition; a particle
/// whose texture the library lacks, or an emitter, light or explosion
/// missing a part, is left out with a note.
/// Most particle textures Add-Ons bring, and their longest side: every
/// effect texture is a layer of one array as large as the largest.
pub const ADD_ON_TEXTURES: usize = 64;
pub const ADD_ON_TEXTURE_SIDE: u32 = 256;

/// The textures an Add-On's particles draw that the effects pack lacks,
/// from `texture` (keyed as the particle names it), each fitted within
/// [`ADD_ON_TEXTURE_SIDE`]; at most [`ADD_ON_TEXTURES`].
fn add_add_on_textures<'t>(
    effects: &bri_weapons::PackEffects,
    library: &mut bri_content::effects::Library,
    textures: &mut Vec<bri_fx_runtime::pack::TextureImage>,
    texture: impl Fn(&str) -> Option<&'t bri_render::scene::SceneImage>,
    notes: &mut Vec<String>,
) {
    let mut added = 0;
    for p in &effects.particles {
        if library.textures.contains_key(&p.texture) {
            continue;
        }
        let Some(image) = texture(&p.texture) else {
            continue;
        };
        if added == ADD_ON_TEXTURES {
            fault(
                notes,
                Problem::new(
                    owner(&p.id),
                    health::Kind::Effect,
                    &p.id,
                    format!("left out: more than {ADD_ON_TEXTURES} Add-On particle textures"),
                ),
            );
            continue;
        }
        let Some(fitted) = fit_texture(image) else {
            fault(
                notes,
                Problem::new(
                    owner(&p.id),
                    health::Kind::Texture,
                    &p.texture,
                    "is not a valid image, so its particle draws nothing",
                )
                .used_by(format!("particle {}", p.id)),
            );
            continue;
        };
        // The library names a texture by a plain file name.
        let file: String = p
            .texture
            .chars()
            .map(|c| {
                if matches!(c, '/' | '\\' | ':') {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        library.textures.insert(p.texture.clone(), file);
        textures.push(bri_fx_runtime::pack::TextureImage {
            id: p.texture.clone(),
            width: fitted.0,
            height: fitted.1,
            rgba: fitted.2,
        });
        added += 1;
    }
}

/// The particle textures Add-Ons name from the game's own interface art
/// (`base/client/ui/...`: v20 particles drew any texture of the game, the
/// Duplorcator's a brick icon), decoded from the UI pack, keyed as each
/// particle names it. The effects pack holds only effect textures.
pub fn interface_textures(
    effects: &bri_weapons::PackEffects,
    ui: &bri_ui::pack::Pack,
) -> BTreeMap<String, bri_render::scene::SceneImage> {
    let mut found = BTreeMap::new();
    for p in &effects.particles {
        let id = p.texture.replace('\\', "/").to_ascii_lowercase();
        if !id.starts_with("base/client/ui/") || found.contains_key(&p.texture) {
            continue;
        }
        if let Some(pixels) = ui.pixels(&bri_ui::pack::TexKey::Image(id.clone())) {
            found.insert(
                p.texture.clone(),
                bri_render::scene::SceneImage {
                    label: id,
                    width: pixels.width,
                    height: pixels.height,
                    rgba: pixels.rgba.clone(),
                    srgb: true,
                },
            );
        }
    }
    found
}

/// An image's RGBA, scaled down to fit [`ADD_ON_TEXTURE_SIDE`] if larger.
fn fit_texture(image: &bri_render::scene::SceneImage) -> Option<(u32, u32, Vec<u8>)> {
    let rgba = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())?;
    if image.width <= ADD_ON_TEXTURE_SIDE && image.height <= ADD_ON_TEXTURE_SIDE {
        return Some((image.width, image.height, rgba.into_raw()));
    }
    let scale = ADD_ON_TEXTURE_SIDE as f32 / image.width.max(image.height) as f32;
    let (w, h) = (
        ((image.width as f32 * scale).round() as u32).clamp(1, ADD_ON_TEXTURE_SIDE),
        ((image.height as f32 * scale).round() as u32).clamp(1, ADD_ON_TEXTURE_SIDE),
    );
    let fitted = image::imageops::resize(&rgba, w, h, image::imageops::FilterType::Triangle);
    Some((w, h, fitted.into_raw()))
}

fn add_pack_effects(
    effects: &bri_weapons::PackEffects,
    library: &mut bri_content::effects::Library,
    manifest: &mut bri_fx_runtime::pack::Manifest,
    notes: &mut Vec<String>,
) {
    for p in &effects.particles {
        if library.particles.iter().any(|q| q.id == p.id) {
            continue;
        }
        if library.textures.contains_key(&p.texture) {
            library.particles.push(p.clone());
        } else {
            fault(
                notes,
                Problem::new(
                    owner(&p.id),
                    health::Kind::Texture,
                    &p.texture,
                    "is missing, so its particle draws nothing",
                )
                .used_by(format!("particle {}", p.id)),
            );
        }
    }
    for e in &effects.emitters {
        if library.emitters.iter().any(|x| x.id == e.id) {
            continue;
        }
        if e.particles
            .iter()
            .all(|p| library.particles.iter().any(|q| &q.id == p))
        {
            // Bound by id alone: an Add-On's display name must not take
            // (or clash with) one the base game binds.
            library.emitters.push(bri_content::effects::Emitter {
                name: String::new(),
                ..e.clone()
            });
        } else {
            let missing = e
                .particles
                .iter()
                .find(|p| !library.particles.iter().any(|q| &q.id == *p))
                .cloned()
                .unwrap_or_default();
            fault(
                notes,
                Problem::new(
                    owner(&e.id),
                    health::Kind::Effect,
                    missing,
                    "is missing, so the emitter shows nothing",
                )
                .used_by(format!("emitter {}", e.id)),
            );
        }
    }
    for l in &effects.lights {
        if !library.lights.iter().any(|x| x.id == l.id) {
            library.lights.push(bri_content::effects::Light {
                name: String::new(),
                ..l.clone()
            });
        }
    }
    let has_emitter = |library: &bri_content::effects::Library, id: &str| {
        library.emitters.iter().any(|e| e.id == id)
    };
    for x in &effects.explosions {
        if manifest.composites.iter().any(|c| c.id == x.id) {
            continue;
        }
        let emitters: Vec<String> = x
            .emitters
            .iter()
            .filter(|e| has_emitter(library, e))
            .cloned()
            .collect();
        for missing in x.emitters.iter().filter(|e| !has_emitter(library, e)) {
            fault(
                notes,
                Problem::new(
                    owner(&x.id),
                    health::Kind::Effect,
                    missing,
                    "is missing, so the explosion shows less",
                )
                .used_by(format!("explosion {}", x.id)),
            );
        }
        let unlit = x
            .light
            .as_ref()
            .filter(|l| !library.lights.iter().any(|x| &x.id == *l));
        let unburst = x
            .burst
            .as_ref()
            .map(|(e, _, _)| e)
            .filter(|e| !has_emitter(library, e));
        for missing in unlit.into_iter().chain(unburst) {
            fault(
                notes,
                Problem::new(
                    owner(&x.id),
                    health::Kind::Effect,
                    missing,
                    "is missing, so the explosion shows less",
                )
                .used_by(format!("explosion {}", x.id)),
            );
        }
        manifest.composites.push(bri_fx_runtime::pack::Composite {
            id: x.id.clone(),
            lifetime: x.lifetime,
            emitters,
            light: x
                .light
                .clone()
                .filter(|l| library.lights.iter().any(|x| &x.id == l)),
            burst: x.burst.clone().filter(|(e, _, _)| has_emitter(library, e)),
        });
    }
}

/// The Add-On an effect id belongs to (`namespace:...`), else the id.
fn owner(id: &str) -> &str {
    bri_weapons::add_on_of(id).unwrap_or(id)
}
/// An Add-On effect problem: kept among the effects' own notes and
/// reported to Add-On health ([`crate::add_on_health::report`]).
fn fault(notes: &mut Vec<String>, problem: Problem) {
    notes.push(problem.to_string());
    crate::add_on_health::report(problem);
}
