//! Cosmetic player and vehicle effects: emote, pain and burn images on the
//! head (their original image state emitters), jet exhaust, vehicle burning,
//! water splashes and vehicle weapon smoke. Driven by reliable presentation
//! cues and the presented poses; no gameplay authority.
use anyhow::Result;
use bri_fx_runtime::{
    EffectHandle, EffectsLimits, EffectsPack, EffectsWorld, SourceOptions, SourceTransform,
    StopMode,
};
use bri_sim::presentation::{Cue, CueKind};
use glam::{Mat4, Quat, Vec3};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

const MAX_MESSAGES: usize = 64;
const TICK_SECONDS: f32 = 1.0 / bri_weapons::TICK_HZ as f32;
const JET_EMITTER: &str = "v20/emitter/playerjetemitter";
const VEHICLE_BURN_EMITTER: &str = "v20/emitter/vehicleburnemitter";
/// `vehicleSplash` (SplashData): its two finite emitters.
const VEHICLE_SPLASH: [&str; 2] = [
    "v20/emitter/vehiclesplashemitter",
    "v20/emitter/vehiclesplashmistemitter",
];

/// `PlayerStandardArmor.splashEmitter[0..2]` burst for `PlayerSplash.lifetimeMS`
/// where a player breaks the surface.
const PLAYER_SPLASH: [&str; 3] = [
    "v20/emitter/playerfoamdropletsemitter",
    "v20/emitter/playerfoamemitter",
    "v20/emitter/playerbubbleemitter",
];
const PLAYER_SPLASH_SECONDS: f32 = 0.3;
/// `PlayerSplash`'s expanding rings, which the effects importer converts from
/// `SplashData` into a finite emitter of ring particles.
const PLAYER_SPLASH_RING: &str = "v20/emitter/playersplash";

/// Where an image or emitter is attached this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Anchor {
    /// A player's `Mount<n>` node.
    Actor { actor: u64, mount: u32 },
    /// A vehicle weapon's muzzle, emitting along the barrel.
    Muzzle { vehicle: u64 },
    /// A vehicle's body, emitting upward.
    Vehicle { vehicle: u64 },
}

/// An image slot: `Player::emote` and `Player::burn` share slot 3, so a new
/// emote replaces the flames; vehicle images are keyed by their datablock.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    Player(u64),
    Vehicle(u64, String),
}

#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub started_images: u64,
    pub missing: u64,
    pub capacity_rejections: u64,
    pub messages: BTreeSet<String>,
}

struct Source {
    emitter: String,
    handle: EffectHandle,
    expires: f32,
}

/// One mounted image stepping through its original states.
struct Playback {
    anchor: Anchor,
    image: bri_weapons::Image,
    state: usize,
    left: f32,
    clock: f32,
    /// `Player::burn` time; the burn image loops until `clearBurn`.
    until: Option<f32>,
    finished: bool,
    sources: Vec<Source>,
}

/// An explosion's `CameraShake`, amplitude fixed on first sight of the camera.
struct Shake {
    spec: bri_weapons::CameraShake,
    position: Vec3,
    elapsed: f32,
    phase: Vec3,
    amplitude: Option<Vec3>,
}

pub struct ActorEffects {
    world: EffectsWorld,
    shakes: Vec<Shake>,
    weapons: Arc<bri_weapons::Pack>,
    images: BTreeMap<Slot, Playback>,
    /// Finite emitters that follow their anchor until they expire.
    one_shots: Vec<(Anchor, EffectHandle)>,
    jets: BTreeMap<(u64, u8), EffectHandle>,
    burning: BTreeMap<u64, EffectHandle>,
    cursor: u64,
    pub diagnostics: Diagnostics,
}

impl ActorEffects {
    pub fn new(
        pack: Arc<EffectsPack>,
        weapons: Arc<bri_weapons::Pack>,
        limits: EffectsLimits,
    ) -> Result<Self> {
        Ok(Self {
            world: EffectsWorld::new(pack, limits, 0x4143544f52)?,
            shakes: Vec::new(),
            weapons,
            images: BTreeMap::new(),
            one_shots: Vec::new(),
            jets: BTreeMap::new(),
            burning: BTreeMap::new(),
            cursor: 0,
            diagnostics: Diagnostics::default(),
        })
    }
    pub fn world(&self) -> &EffectsWorld {
        &self.world
    }
    pub fn image_count(&self) -> usize {
        self.images.len()
    }
    pub fn jet_count(&self) -> usize {
        self.jets.len()
    }
    pub fn burning_count(&self) -> usize {
        self.burning.len()
    }
    /// A new session starts after its checkpoint; earlier one-shots never replay.
    pub fn reset(&mut self, checkpoint_cursor: u64) {
        self.world.teardown();
        self.shakes.clear();
        self.images.clear();
        self.one_shots.clear();
        self.jets.clear();
        self.burning.clear();
        self.cursor = checkpoint_cursor;
    }
    fn note(&mut self, message: String) {
        self.diagnostics.missing = self.diagnostics.missing.saturating_add(1);
        if self.diagnostics.messages.len() < MAX_MESSAGES {
            self.diagnostics.messages.insert(message);
        }
    }

    /// Consume one reliable cue once, in order.
    pub fn cue(&mut self, cue: &Cue) {
        if cue.id <= self.cursor {
            return;
        }
        self.cursor = cue.id;
        match &cue.kind {
            // `serverCmdLove`, `serverCmdHate`, `serverCmdConfusion`.
            CueKind::Emote { actor, name } => {
                let image = match name.as_str() {
                    "love" => "LoveImage",
                    "hate" => "HateImage",
                    "confusion" => "WtfImage",
                    _ => return,
                };
                self.mount_player(*actor, image, None);
            }
            // `Armor::damage`: PainHigh at 40, PainMid at 25, else PainLow.
            CueKind::Pain { actor, level, .. } => {
                let image = if *level >= 40.0 {
                    "PainHighImage"
                } else if *level >= 25.0 {
                    "PainMidImage"
                } else {
                    "PainLowImage"
                };
                self.mount_player(*actor, image, None);
            }
            CueKind::Burn { actor, seconds } => {
                self.mount_player(*actor, "PlayerBurnImage", Some(*seconds));
            }
            CueKind::VehicleEffect {
                vehicle,
                effect,
                active,
            } => self.vehicle_effect(*vehicle, effect, *active),
            CueKind::Water { entered: true, .. } => {
                let at = SourceTransform {
                    position: Vec3::from(cue.position),
                    ..Default::default()
                };
                for emitter in PLAYER_SPLASH {
                    let started = self
                        .world
                        .start_emitter(emitter, at, SourceOptions::default())
                        .and_then(|h| self.world.set_remaining_lifetime(h, PLAYER_SPLASH_SECONDS));
                    if started.is_err() {
                        self.note(format!("Player splash emitter unavailable: {emitter}"));
                    }
                }
                let ring =
                    self.world
                        .start_emitter(PLAYER_SPLASH_RING, at, SourceOptions::default());
                if ring.is_err() {
                    self.note(format!(
                        "Player splash emitter unavailable: {PLAYER_SPLASH_RING}"
                    ));
                }
            }
            CueKind::WeaponEffect { definition, .. } => {
                if let Some(spec) = self
                    .weapons
                    .explosions
                    .get(&definition.to_ascii_lowercase())
                    .and_then(|e| e.shake)
                    && self.shakes.len() < 32
                {
                    // `CameraShake::init`: x starts at zero, y and z at random offsets.
                    let seed = cue.id.wrapping_mul(0x9e37_79b9_7f4a_7c15);
                    let unit = |shift: u32| ((seed >> shift) & 0xffff) as f32 / 65536.0;
                    self.shakes.push(Shake {
                        spec,
                        position: Vec3::from(cue.position),
                        elapsed: 0.0,
                        phase: Vec3::new(0.0, unit(16), unit(32)),
                        amplitude: None,
                    });
                }
            }
            _ => {}
        }
    }

    fn image(&mut self, name: &str) -> Option<bri_weapons::Image> {
        let image = self
            .weapons
            .images
            .get(&bri_weapons::native_id("image", name))
            .cloned();
        if image.is_none() {
            self.note(format!("Missing image {name}"));
        }
        image
    }

    fn mount_player(&mut self, actor: u64, name: &str, until: Option<f32>) {
        if let Some(image) = self.image(name) {
            let anchor = Anchor::Actor {
                actor,
                mount: image.mount_point,
            };
            self.mount(Slot::Player(actor), anchor, image, until);
        }
    }

    fn vehicle_effect(&mut self, vehicle: u64, effect: &str, active: bool) {
        // Burning follows the replicated destroyed state (late joiners see it).
        if effect.eq_ignore_ascii_case("VehicleBurnEmitter") {
            return;
        }
        if effect.eq_ignore_ascii_case("vehicleSplash") {
            for emitter in VEHICLE_SPLASH {
                self.one_shot(Anchor::Vehicle { vehicle }, emitter);
            }
            return;
        }
        // Weapon images (`TankSmokeImage`, `CannonSmokeImage`, the charging
        // `CannonFuseImage`) mounted at the weapon like `mountImage`.
        let slot = Slot::Vehicle(vehicle, effect.to_ascii_lowercase());
        if !active {
            if let Some(old) = self.images.remove(&slot) {
                self.release(old);
            }
            return;
        }
        if let Some(image) = self.image(effect) {
            self.mount(slot, Anchor::Muzzle { vehicle }, image, None);
        }
    }

    fn mount(&mut self, slot: Slot, anchor: Anchor, image: bri_weapons::Image, until: Option<f32>) {
        if let Some(old) = self.images.remove(&slot) {
            self.release(old);
        }
        self.diagnostics.started_images = self.diagnostics.started_images.saturating_add(1);
        let left = image
            .states
            .first()
            .map_or(0.0, |s| s.ticks as f32 * TICK_SECONDS);
        let finished = image.states.is_empty();
        self.images.insert(
            slot.clone(),
            Playback {
                anchor,
                image,
                state: 0,
                left,
                clock: 0.0,
                until,
                finished,
                sources: Vec::new(),
            },
        );
        self.enter(&slot, 0);
    }

    fn release(&mut self, playback: Playback) {
        for s in playback.sources {
            self.world.stop(s.handle, StopMode::Drain);
        }
    }

    /// Enter a state: start its `stateEmitter` for `stateEmitterTime`, or
    /// extend the same emitter already running for this image. Sources start
    /// at the origin; `advance` places them before they emit.
    fn enter(&mut self, slot: &Slot, state: usize) {
        let Some(playback) = self.images.get_mut(slot) else {
            return;
        };
        let Some(s) = playback.image.states.get(state) else {
            playback.finished = true;
            return;
        };
        playback.state = state;
        if s.emitter.is_empty() || s.emitter_seconds <= 0.0 {
            return;
        }
        let emitter = format!("v20/emitter/{}", s.emitter.to_ascii_lowercase());
        let seconds = s.emitter_seconds;
        let expires = playback.clock + seconds;
        let world = &mut self.world;
        if let Some(source) = playback
            .sources
            .iter_mut()
            .find(|x| x.emitter == emitter && world.is_active(x.handle))
        {
            if expires > source.expires {
                source.expires = expires;
                let _ = world.set_remaining_lifetime(source.handle, seconds);
            }
            return;
        }
        let started = world
            .start_emitter(
                &emitter,
                SourceTransform::default(),
                SourceOptions::default(),
            )
            .and_then(|h| world.set_remaining_lifetime(h, seconds).map(|_| h));
        match started {
            Ok(handle) => playback.sources.push(Source {
                emitter,
                handle,
                expires,
            }),
            Err(_) => {
                self.diagnostics.capacity_rejections =
                    self.diagnostics.capacity_rejections.saturating_add(1);
            }
        }
    }

    fn one_shot(&mut self, anchor: Anchor, emitter: &str) {
        let finite = self
            .world
            .pack()
            .library
            .emitters
            .iter()
            .find(|e| e.id == emitter)
            .map(|e| e.lifetime > 0.0);
        match finite {
            None => self.note(format!("Missing emitter {emitter}")),
            Some(false) => self.note(format!("No finite duration for {emitter}")),
            Some(true) => match self.world.start_emitter(
                emitter,
                SourceTransform::default(),
                SourceOptions::default(),
            ) {
                Ok(handle) => self.one_shots.push((anchor, handle)),
                Err(_) => {
                    self.diagnostics.capacity_rejections =
                        self.diagnostics.capacity_rejections.saturating_add(1);
                }
            },
        }
    }

    /// The summed explosion shake for a camera at `eye`, in its own frame
    /// (x right, y forward, z up). Distance falloff as in `Explosion::explode`.
    pub fn camera_shake(&mut self, eye: Vec3) -> Vec3 {
        let mut offset = Vec3::ZERO;
        for shake in &mut self.shakes {
            let s = shake.spec;
            let amplitude = *shake.amplitude.get_or_insert_with(|| {
                let distance = eye.distance(shake.position);
                if distance >= s.radius {
                    return Vec3::ZERO;
                }
                let falloff = 1.0 + distance / s.radius * 10.0;
                Vec3::from(s.amplitude) / (falloff * falloff)
            });
            // `CameraShake::fadeAmplitude`.
            let done = (shake.elapsed / s.seconds.max(0.001)).min(1.0);
            let fade = 1.0 / (1.0 + done * s.falloff).powi(2);
            let time = shake.phase + Vec3::splat(shake.elapsed);
            let wave = (time * Vec3::from(s.frequency) * std::f32::consts::TAU)
                .to_array()
                .map(f32::sin);
            offset += amplitude * fade * Vec3::from(wave);
        }
        offset
    }
    /// Step image states, keep sources on their anchors and simulate particles.
    /// `jets`: jetting players' two foot transforms and velocity. `burning`:
    /// destroyed vehicles. A source whose anchor is gone drains.
    pub fn advance(
        &mut self,
        dt: f32,
        pose: impl Fn(Anchor) -> Option<Mat4>,
        jets: &[(u64, [Mat4; 2], Vec3)],
        burning: &[(u64, Mat4)],
    ) -> Result<()> {
        anyhow::ensure!(
            dt.is_finite() && (0.0..=86400.0).contains(&dt),
            "Invalid actor effects timestep"
        );
        let dt = dt.min(0.25);
        for shake in &mut self.shakes {
            shake.elapsed += dt;
        }
        self.shakes.retain(|s| s.elapsed < s.spec.seconds);
        let slots: Vec<Slot> = self.images.keys().cloned().collect();
        for slot in &slots {
            let mut entered = Vec::new();
            if let Some(p) = self.images.get_mut(slot) {
                p.clock += dt;
                p.left -= dt;
                if p.until.is_some_and(|t| p.clock >= t) {
                    p.finished = true;
                }
                // Bounded: zero-length state loops cannot spin a frame.
                while !p.finished && p.left <= 0.0 && entered.len() < 16 {
                    match p.image.states.get(p.state).and_then(|s| s.timeout) {
                        Some(target) => {
                            p.left += p.image.states[target].ticks as f32 * TICK_SECONDS;
                            p.state = target;
                            entered.push(target);
                        }
                        None => p.finished = true,
                    }
                }
            }
            for state in entered {
                self.enter(slot, state);
            }
        }
        let mut gone = Vec::new();
        for (slot, p) in &mut self.images {
            let transform = pose(p.anchor)
                .map(|m| source(m * Mat4::from_translation(Vec3::from(p.image.offset))));
            // Burning ends at `clearBurn`; an emote ends when its emitters do.
            let cleared = p.until.is_some() && p.finished;
            p.sources.retain(|s| self.world.is_active(s.handle));
            for s in &p.sources {
                match transform {
                    Some(t) if !cleared => self.world.update_source(s.handle, t)?,
                    _ => {
                        self.world.stop(s.handle, StopMode::Drain);
                    }
                }
            }
            if transform.is_none() || cleared || (p.finished && p.sources.is_empty()) {
                gone.push(slot.clone());
            }
        }
        for slot in gone {
            if let Some(p) = self.images.remove(&slot) {
                self.release(p);
            }
        }
        let world = &mut self.world;
        self.one_shots.retain(|(anchor, handle)| {
            if !world.is_active(*handle) {
                return false;
            }
            match pose(*anchor) {
                Some(m) => world.update_source(*handle, source(m)).is_ok(),
                None => {
                    world.stop(*handle, StopMode::Drain);
                    false
                }
            }
        });
        // `jetEmitter` at both feet while jetting, exhausting downward.
        let down = Quat::from_rotation_arc(Vec3::Y, Vec3::NEG_Y);
        let wanted: BTreeMap<(u64, u8), SourceTransform> = jets
            .iter()
            .flat_map(|(actor, feet, velocity)| {
                feet.iter().zip(0u8..).map(move |(m, i)| {
                    let t = SourceTransform {
                        rotation: down,
                        velocity: *velocity,
                        ..source(*m)
                    };
                    ((*actor, i), t)
                })
            })
            .collect();
        sync_sources(world, &mut self.jets, &wanted, JET_EMITTER)?;
        // `damageEmitter` fire on a destroyed vehicle until it is removed.
        let wanted: BTreeMap<u64, SourceTransform> = burning
            .iter()
            .map(|(vehicle, m)| {
                let t = SourceTransform {
                    rotation: Quat::IDENTITY,
                    ..source(*m)
                };
                (*vehicle, t)
            })
            .collect();
        sync_sources(world, &mut self.burning, &wanted, VEHICLE_BURN_EMITTER)?;
        world.advance(dt, Vec3::ZERO)?;
        Ok(())
    }
}

/// A vehicle weapon's muzzle as the server fires it: the authored muzzle
/// swung about the pivot by the turret aim, local up along the barrel.
pub fn muzzle(
    position: Vec3,
    rotation: Quat,
    aim: [f32; 2],
    definition: &bri_vehicles::Definition,
) -> Option<Mat4> {
    let (local, direction) = definition.muzzle(aim)?;
    let direction = rotation * direction;
    Some(Mat4::from_rotation_translation(
        Quat::from_rotation_arc(Vec3::Y, direction.normalize_or(Vec3::Y)),
        position + rotation * local,
    ))
}

/// Keep one continuous emitter per key; removed keys drain.
fn sync_sources<K: Ord + Copy>(
    world: &mut EffectsWorld,
    live: &mut BTreeMap<K, EffectHandle>,
    wanted: &BTreeMap<K, SourceTransform>,
    emitter: &str,
) -> Result<()> {
    live.retain(|key, handle| {
        let keep = wanted.contains_key(key) && world.is_active(*handle);
        if !keep {
            world.stop(*handle, StopMode::Drain);
        }
        keep
    });
    for (key, transform) in wanted {
        if let Some(handle) = live.get(key) {
            world.update_source(*handle, *transform)?;
        } else if let Ok(handle) =
            world.start_emitter(emitter, *transform, SourceOptions::default())
        {
            live.insert(*key, handle);
        }
    }
    Ok(())
}

/// A world matrix as an emitter transform: its origin and orientation, whose
/// local up is the ejection axis.
fn source(m: Mat4) -> SourceTransform {
    let (_, rotation, position) = m.to_scale_rotation_translation();
    SourceTransform {
        position,
        rotation: rotation.normalize(),
        velocity: Vec3::ZERO,
    }
}
