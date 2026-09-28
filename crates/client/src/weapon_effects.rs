//! Cosmetic weapon projection. Native definitions and authoritative views only.
//! Call reset on session replacement (with the checkpoint cue cursor), then sync,
//! cues and advance. The host provides animated attachment poses and consumes
//! shell/animation requests; this module never guesses a mount or gameplay hit.
use anyhow::{Result, ensure};
use bri_fx_runtime::{
    BlendMode, EffectHandle, EffectsLimits, EffectsPack, EffectsWorld, Recolor, SourceOptions,
    SourceTransform, StopMode,
};
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
    timed: Vec<Timed>,
    pending: VecDeque<HostRequest>,
    cursor: u64,
    limits: EffectsLimits,
    /// World palette for `color<N>Paint*` spray effects.
    palette: Vec<[f32; 4]>,
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
        let textures = pack
            .textures
            .iter()
            .map(|t| bri_fx_runtime::pack::TextureImage {
                id: t.id.clone(),
                width: t.width,
                height: t.height,
                rgba: t.rgba.clone(),
            })
            .collect();
        let pack = EffectsPack::from_parts(library, pack.manifest.clone(), textures)?;
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
        Ok(Self {
            world: EffectsWorld::new(pack, limits, 0x574541504f4e)?,
            weapons,
            bindings,
            trails: BTreeMap::new(),
            timed: Vec::new(),
            pending: VecDeque::new(),
            cursor: 0,
            limits,
            palette: Vec::new(),
            diagnostics: Diagnostics::default(),
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
            if let Some(a) = self.trails.get(&key) {
                // Keep a tombstone until the projectile disappears: a finite authored
                // emitter must not restart every network/render frame.
                if self.world.is_active(a.handle) {
                    self.world.update_source(a.handle, transform)?;
                }
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
                    self.trails.insert(key, Attached { resource, handle });
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
