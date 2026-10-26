//! Client audio adapter. Tests construct Null; only the explicit executable --run
//! path selects Device. Sound failures remain diagnostic, never gameplay failures.
use anyhow::Result;
use bri_audio::*;
use bri_console::Clamp;
use bri_sim::presentation::{Cue, CueKind};
use bri_ui::screens::options::{MUSIC_VOLUME, MUTE_IN_BACKGROUND, volume};
use bri_ui::{api::Settings, prefs::Prefs};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
    sync::Arc,
};

/// Decoded pack sounds can be prepared without the audio device.
pub(crate) struct PreparedSounds {
    bank: Arc<SoundBank>,
    sounds: BTreeMap<String, (Arc<SoundAsset>, f32)>,
    variants: BTreeMap<String, Vec<SoundChoice>>,
    /// Why each pack sound that did not load failed, by lowercase profile.
    failed: BTreeMap<String, String>,
    warnings: BTreeSet<String>,
}

/// How an Add-On asked for one sound to be played.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Voice {
    /// Times the profile's own volume.
    pub volume: f32,
    /// Times its own pitch.
    pub pitch: f32,
    /// The distance it is heard out to; 0 keeps its own.
    pub radius: f32,
}
impl Voice {
    pub const AS_AUTHORED: Self = Self {
        volume: 1.0,
        pitch: 1.0,
        radius: 0.0,
    };
}
struct SoundChoice {
    asset: Arc<SoundAsset>,
    volume: f32,
    weight: f32,
}

fn choose_sound(choices: &[SoundChoice], unit: f32) -> &SoundChoice {
    let mut at = unit * choices.iter().map(|c| c.weight).sum::<f32>();
    for choice in choices {
        if at < choice.weight {
            return choice;
        }
        at -= choice.weight;
    }
    choices.last().expect("nonempty prepared sound choices")
}

/// Local cosmetic randomness; no world-state or network dependency.
fn sound_choice_unit(counter: u64) -> f32 {
    let mut z = counter.wrapping_add(0x9e3779b97f4a7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((z ^ (z >> 31)) >> 40) as f32 / 16777216.
}

pub trait SoundLookup {
    fn has_sound(&self, profile: &str) -> bool;
    /// Why `profile`, an Add-On's own sound, could not be played (its file
    /// is missing or does not decode), when it was tried.
    fn why_missing(&self, _profile: &str) -> Option<String> {
        None
    }
}
impl SoundLookup for ClientAudio {
    fn has_sound(&self, profile: &str) -> bool {
        ClientAudio::has_sound(self, profile)
    }
    fn why_missing(&self, profile: &str) -> Option<String> {
        self.pack_sound_failures
            .get(&profile.to_ascii_lowercase())
            .cloned()
    }
}
impl SoundLookup for PreparedSounds {
    fn has_sound(&self, profile: &str) -> bool {
        self.sounds.contains_key(&profile.to_ascii_lowercase())
            || self.bank.resolve(profile).is_ok()
    }
    fn why_missing(&self, profile: &str) -> Option<String> {
        self.failed.get(&profile.to_ascii_lowercase()).cloned()
    }
}
impl PreparedSounds {
    pub(crate) fn load(bank: Arc<SoundBank>, pack: &bri_weapons::Pack, root: &Path) -> Self {
        let mut prepared = Self {
            bank: bank.clone(),
            sounds: BTreeMap::new(),
            variants: BTreeMap::new(),
            failed: BTreeMap::new(),
            warnings: BTreeSet::new(),
        };
        for (profile, def) in &pack.sounds {
            if def.variants.is_empty() {
                prepared.load_clip(profile, def, root);
            }
        }
        for (profile, def) in &pack.sounds {
            let choices: Vec<_> = def
                .variants
                .iter()
                .filter_map(|variant| {
                    let leaf = pack.sounds.get(&variant.profile)?;
                    if !leaf.variants.is_empty()
                        || !variant.weight.is_finite()
                        || variant.weight <= 0.
                    {
                        return None;
                    }
                    let (asset, volume) = prepared.sounds.get(&variant.profile)?;
                    Some(SoundChoice {
                        asset: asset.clone(),
                        volume: *volume * def.volume,
                        weight: variant.weight,
                    })
                })
                .collect();
            if let Some(first) = choices.first() {
                prepared
                    .sounds
                    .insert(profile.clone(), (first.asset.clone(), first.volume));
                prepared.failed.remove(profile);
                prepared.variants.insert(profile.clone(), choices);
            } else if !def.variants.is_empty() {
                prepared.load_clip(profile, def, root);
            }
        }
        prepared
    }
    fn load_clip(&mut self, profile: &str, def: &bri_weapons::SoundDef, root: &Path) {
        let (near, far) = PACK_SOUND_RANGE;
        // The game's own file, played from the bank's copy.
        let stock = def.stock.then(|| {
            self.bank
                .clip_at(&def.file)
                .map(|clip| SoundAsset::world(profile, clip.clone(), near, far))
                .ok_or_else(|| format!("the game has no sound {}", def.file))
        });
        let dir = bri_weapons::sound_root(root, def);
        let loaded = stock.unwrap_or_else(|| {
            bri_package::path::inside(&dir, &def.file).and_then(|path| {
                if def.music {
                    SoundAsset::music_file(profile, &path)
                } else {
                    SoundAsset::decoded_file(profile, &path, near, far)
                }
            })
        });
        match loaded {
            Ok(mut asset) => {
                asset.playback.looping = def.looping;
                self.sounds
                    .insert(profile.to_string(), (Arc::new(asset), def.volume));
            }
            Err(error) => {
                if self.failed.len() < 512 {
                    self.failed
                        .insert(profile.to_ascii_lowercase(), error.to_string());
                }
                if self.warnings.len() < 64 {
                    self.warnings
                        .insert(format!("Weapon sound {profile}: {error}"));
                }
            }
        }
    }
}

/// A sound that cannot play is a counted fallback (`fallbacks.json`), not
/// only a log line: the game carries on silent.
fn report_fallback(error: &AudioError) {
    use bri_console::fallback::{kind, report};
    match error {
        AudioError::UnknownSound(sound) => report(
            kind::MISSING_SOUND,
            "",
            sound,
            "plays silently (no such sound)",
        ),
        AudioError::SoundUnavailable { sound, reason } => report(
            kind::MISSING_SOUND,
            "",
            sound,
            &format!("plays silently ({reason})"),
        ),
        AudioError::Decode { clip, message } => report(
            kind::MISSING_SOUND,
            "",
            clip,
            &format!("plays silently (cannot decode: {message})"),
        ),
        AudioError::Integrity { clip, .. } => report(
            kind::MISSING_SOUND,
            "",
            clip,
            "plays silently (the file does not match its manifest)",
        ),
        other => report(
            kind::OTHER,
            "",
            &other.to_string(),
            "the audio request is skipped",
        ),
    }
}

/// A vehicle's engine sound this frame ([`bri_vehicles::EngineSound`]).
#[derive(Clone, Debug, PartialEq)]
pub struct EngineLoop {
    pub sound: String,
    pub position: [f32; 3],
    /// Multiples of the sound's own volume and pitch.
    pub volume: f32,
    pub pitch: f32,
}
/// The engine loop vehicle `info` plays this frame, if its definition has
/// one and it is running: someone is in the seat that drives it (any
/// seat, when none does) and it is not destroyed. Its pitch and volume
/// follow the vehicle's presented speed, so every client hears every
/// vehicle the same way, the driver included.
pub fn engine_loop(
    d: &bri_vehicles::Definition,
    info: &bri_sim::session::VehicleInfo,
    frame: &crate::vehicles::VehicleFrame,
) -> Option<EngineLoop> {
    engine_loops(d, info, frame).into_iter().next()
}
/// Every loop of vehicle `info`'s engine this frame: its `engine_sound`
/// first, then each of its `engine_layers` that is audible, with the layer's
/// number (0 for the first).
pub fn engine_loops(
    d: &bri_vehicles::Definition,
    info: &bri_sim::session::VehicleInfo,
    frame: &crate::vehicles::VehicleFrame,
) -> Vec<EngineLoop> {
    let Some(engine) = d.engine_sound.as_ref() else {
        return Vec::new();
    };
    let running = match d.control_seat() {
        Some(seat) => info.occupants.get(seat).is_some_and(Option::is_some),
        None => info.occupants.iter().any(Option::is_some),
    };
    if !running || info.destroyed || !frame.position.is_finite() {
        return Vec::new();
    }
    let speed = frame.velocity.length();
    std::iter::once(engine)
        .chain(&d.engine_layers)
        .enumerate()
        .filter_map(|(layer, engine)| {
            let (volume, pitch) = engine.at(speed, d.max_speed);
            // The first loop always plays: it is what marks the engine on.
            (layer == 0 || volume > 0.001).then(|| EngineLoop {
                sound: engine.sound.clone(),
                position: frame.position.to_array(),
                volume,
                pitch,
            })
        })
        .collect()
}

/// The squeal of vehicle `info`'s sliding tyres this frame, if its
/// definition has a skid sound and a wheel skids
/// ([`crate::actor_effects::tire_skids`]): louder and a little higher the
/// harder it slides.
pub fn skid_loop(
    d: &bri_vehicles::Definition,
    info: &bri_sim::session::VehicleInfo,
    frame: &crate::vehicles::VehicleFrame,
) -> Option<EngineLoop> {
    let sound = d.skid.as_ref()?.sound.as_ref()?;
    if info.destroyed || !frame.position.is_finite() {
        return None;
    }
    let (_, strength) = crate::actor_effects::tire_skids(0, d, frame);
    (strength > 0.0).then(|| EngineLoop {
        sound: sound.clone(),
        position: frame.position.to_array(),
        volume: strength,
        pitch: 0.9 + 0.2 * strength,
    })
}

/// The world's music bricks (a resolved `sound`), kept in step with each
/// replica revision from what changed in it (`Bricks::diff` skips the
/// shared tree), so a world change costs its changed bricks, not a pass
/// over every brick with a copy of every sound name.
#[derive(Default)]
pub struct MusicBricks {
    bricks: Option<bri_world::Bricks>,
    wanted: BTreeMap<u64, (String, [f32; 3])>,
    /// Passes over a whole world: the first replica only.
    pub full_scans: u64,
    /// Bricks examined, in passes and in changes.
    pub visited: u64,
}
impl MusicBricks {
    fn music(brick: &bri_world::Brick) -> Option<(String, [f32; 3])> {
        match &brick.sound {
            Some(bri_world::ContentRef::Resolved(sound)) => Some((sound.clone(), brick.position)),
            _ => None,
        }
    }
    /// Bring the music bricks up to `bricks`; true when any changed.
    pub fn update(&mut self, bricks: &bri_world::Bricks) -> bool {
        if self.bricks.as_ref().is_some_and(|old| old.ptr_eq(bricks)) {
            return false;
        }
        let mut changed = false;
        match &self.bricks {
            Some(old) => {
                for change in old.diff(bricks) {
                    self.visited += 1;
                    match change {
                        imbl::ordmap::DiffItem::Remove(id, _) => {
                            changed |= self.wanted.remove(id).is_some();
                        }
                        imbl::ordmap::DiffItem::Add(id, b)
                        | imbl::ordmap::DiffItem::Update { new: (id, b), .. } => {
                            let want = Self::music(b);
                            if self.wanted.get(id) != want.as_ref() {
                                changed = true;
                                match want {
                                    Some(want) => self.wanted.insert(*id, want),
                                    None => self.wanted.remove(id),
                                };
                            }
                        }
                    }
                }
            }
            None => {
                self.full_scans += 1;
                self.visited += bricks.len() as u64;
                self.wanted = bricks
                    .iter()
                    .filter_map(|(id, b)| Some((*id, Self::music(b)?)))
                    .collect();
                changed = true;
            }
        }
        self.bricks = Some(bricks.clone());
        changed
    }
    /// Music brick id, loop and position.
    pub fn wanted(&self) -> &BTreeMap<u64, (String, [f32; 3])> {
        &self.wanted
    }
}

pub struct ClientAudio {
    runtime: AudioRuntime,
    pending: VecDeque<(String, Placement, Voice)>,
    contact_pending: VecDeque<(String, [f32; 3], u64)>,
    pub warnings: BTreeSet<String>,
    pub dropped: u64,
    pub server_dropped: u64,
    pub requested: BTreeMap<String, u64>,
    prefs: Prefs,
    /// Music-brick loops keyed by brick: (loop id, position, voice).
    music: BTreeMap<u64, (String, [f32; 3], SoundHandle)>,
    /// The music bricks the world has.
    pub music_bricks: MusicBricks,
    /// Looping image state sounds (spray hiss, push broom) keyed by
    /// (player, hand); they play only while that state lasts.
    image_loops: BTreeMap<(u64, u8), (String, SoundHandle)>,
    /// Vehicle engine loops keyed by vehicle id; see [`engine_loop`].
    engines: BTreeMap<u64, (String, SoundHandle)>,
    /// Projectile `sound` loops keyed by projectile id.
    projectiles: BTreeSet<u64>,
    /// The player's master volume, before any background mute.
    master: f32,
    mute_in_background: bool,
    focused: bool,
    /// Tick of the last brick break heard; see [`BREAK_SOUND_GAP_MS`].
    last_break: Option<u64>,
    /// Sounds Add-On weapons packs ship, by lower-case profile, with their
    /// volume: played in place of a bank sound of the same name.
    pack_sounds: BTreeMap<String, (Arc<SoundAsset>, f32)>,
    pack_sound_variants: BTreeMap<String, Vec<SoundChoice>>,
    sound_choice_counter: u64,
    /// [`PreparedSounds`]' reasons for the pack sounds that did not load.
    pack_sound_failures: BTreeMap<String, String>,
    /// The portals the listener hears through, as last sent.
    windows: Vec<bri_audio::Window>,
    /// The track an Add-On rule plays ([`ClientAudio::play_music`]).
    rule_music: Option<RuleMusic>,
    /// The ambient beds Add-On rules play, by key
    /// ([`ClientAudio::play_ambient`]).
    rule_ambient: BTreeMap<String, RuleMusic>,
    /// The world turned down by a rule ([`ClientAudio::set_world_volume`]).
    world_volume: Option<WorldVolume>,
    /// The looping positional sources a game mode asked for, by key ([`ClientAudio::sync_positional_sources`]).
    positional: BTreeMap<u64, PlayingSource>,
    /// Sounds placed in the world that have not yet had their way to the listener judged ([`ClientAudio::hear_world`]).
    fresh_world: Vec<(SoundHandle, [f32; 3])>,
    /// The reverb room last set, to send only changes.
    room: Room,
}
/// A looping sound a game mode places in the world and keeps current ([`ClientAudio::sync_positional_sources`]). Distances are in world
/// units; the sound fades by inverse distance, full inside `reference_distance` and silent beyond `max_distance`.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionalSource {
    /// Identifies the source across ticks (the emitting actor).
    pub key: u64,
    pub sound: String,
    pub position: [f32; 3],
    /// Linear volume multiple of the sound's own.
    pub volume: f32,
    /// Playback rate multiple.
    pub pitch: f32,
    pub reference_distance: f32,
    pub max_distance: f32,
}
impl PositionalSource {
    /// Whether `other` can be the same playing voice, moved and re-volumed, rather than a restart.
    fn same_voice(&self, other: &Self) -> bool {
        self.sound == other.sound
            && self.pitch == other.pitch
            && self.reference_distance == other.reference_distance
            && self.max_distance == other.max_distance
    }
}
struct PlayingSource {
    handle: SoundHandle,
    pack_volume: f32,
    source: PositionalSource,
}
/// A rule's music track as it plays, fading in over `fade` seconds.
struct RuleMusic {
    handle: SoundHandle,
    volume: f32,
    fade: f32,
    elapsed: f32,
}
/// How near an Add-On weapon sound plays at full volume, and how far it
/// carries, in world units: v20's `AudioClose3d`/`AudioDefault3d` range.
/// Most world sounds waiting to have their way to the listener judged; past it they play as authored.
const FRESH_WORLD_LIMIT: usize = 256;
const PACK_SOUND_RANGE: (f32, f32) = (10.0, 60.0);
/// v20's client schedules a `BrickBreakSoundEvent` for a dying brick only
/// when its death time is at least 80 ms from the last one scheduled, for
/// any brick (`blocklandv20.exe` 0x539c10-0x539c57, last time at 0x81ac44).
/// A chain kill or blast of many bricks is therefore one break sound, heard
/// at the first brick.
pub const BREAK_SOUND_GAP_MS: u64 = 80;
/// Attached-sound entity keys for projectiles, apart from other entities.
fn projectile_entity(id: u64) -> EntityKey {
    EntityKey(id | 1 << 63)
}

impl ClientAudio {
    pub fn load(path: &Path, settings: &mut Settings, output: OutputKind) -> Result<Self> {
        let bank = Arc::new(SoundBank::load(path, &BankOptions::default())?);
        let d = bank.defaults();
        let values = [
            ("$pref::Audio::masterVolume", d.master_volume.to_string()),
            (
                "$pref::Audio::channelVolume1",
                d.channel_volumes[1].to_string(),
            ),
            (
                "$pref::Audio::channelVolume2",
                d.channel_volumes[2].to_string(),
            ),
            (
                "$pref::Audio::PlayMusic",
                u8::from(d.play_music).to_string(),
            ),
            (
                "$pref::Audio::MenuSounds",
                u8::from(d.menu_sounds).to_string(),
            ),
            (
                "$pref::Audio::PlayBrickMoveSound",
                u8::from(d.play_brick_move_sound).to_string(),
            ),
            (
                "$pref::Audio::PlayBrickPlantSound",
                u8::from(d.play_brick_plant_sound).to_string(),
            ),
            (
                "$pref::Audio::PlantErrorSound",
                u8::from(d.plant_error_sound).to_string(),
            ),
        ];
        for (key, value) in values {
            if !settings.prefs.keys().any(|k| k.eq_ignore_ascii_case(key)) {
                settings.prefs.insert(key.into(), value);
            }
        }
        let mut warnings = BTreeSet::new();
        let runtime = if output == OutputKind::Device {
            let (runtime, error) = AudioRuntime::open_or_null(bank, RuntimeConfig::default());
            if let Some(error) = error {
                warnings.insert(format!("Audio device unavailable: {error}"));
            }
            runtime
        } else {
            AudioRuntime::new(bank, RuntimeConfig::default(), output)?
        };
        let mut audio = Self {
            runtime,
            pending: VecDeque::new(),
            contact_pending: VecDeque::new(),
            warnings,
            dropped: 0,
            server_dropped: 0,
            requested: BTreeMap::new(),
            prefs: Prefs::default(),
            music: BTreeMap::new(),
            music_bricks: MusicBricks::default(),
            rule_music: None,
            rule_ambient: BTreeMap::new(),
            world_volume: None,
            positional: BTreeMap::new(),
            fresh_world: Vec::new(),
            room: Room::DRY,
            projectiles: BTreeSet::new(),
            image_loops: BTreeMap::new(),
            engines: BTreeMap::new(),
            master: 1.,
            mute_in_background: false,
            focused: true,
            last_break: None,
            pack_sounds: BTreeMap::new(),
            pack_sound_variants: BTreeMap::new(),
            sound_choice_counter: 0,
            pack_sound_failures: BTreeMap::new(),
            windows: Vec::new(),
        };
        audio.apply_settings(settings);
        Ok(audio)
    }
    fn record(&mut self, result: std::result::Result<(), AudioError>) {
        if let Err(error) = result {
            report_fallback(&error);
            // Keep diagnostics bounded even for a malformed remote catalog.
            if self.warnings.len() < 64 {
                self.warnings.insert(error.to_string());
            }
        }
    }
    pub fn apply_settings(&mut self, settings: &Settings) {
        self.prefs = Prefs::new(&BTreeMap::new(), &settings.prefs);
        self.master = volume(&self.prefs, "$pref::Audio::masterVolume");
        self.mute_in_background = self.prefs.bool_or(MUTE_IN_BACKGROUND, false);
        self.apply_master();
        for (key, channel) in [
            ("$pref::Audio::channelVolume1", "shell"),
            ("$pref::Audio::channelVolume2", "sim"),
            (MUSIC_VOLUME, "music"),
        ] {
            let result = self
                .runtime
                .apply_ui_volume(channel, volume(&self.prefs, key))
                .map(|_| ());
            self.record(result);
        }
        let result = self
            .runtime
            .set_music_enabled(self.prefs.bool_or("$pref::Audio::PlayMusic", true));
        self.record(result);
    }
    /// The master gain the player chose, silenced while the game is in the
    /// background when they asked for that.
    fn apply_master(&mut self) {
        let muted = self.mute_in_background && !self.focused;
        let value = if muted { 0. } else { self.master };
        let result = self.runtime.apply_ui_volume("master", value).map(|_| ());
        self.record(result);
    }
    /// The game window gained or lost focus.
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.apply_master();
    }
    pub fn set_volume(&mut self, channel: &str, value: f32) -> Result<()> {
        anyhow::ensure!(
            value.is_finite() && (0.0..=1.).contains(&value),
            "Invalid audio volume"
        );
        if channel == "master" {
            self.master = value;
            self.apply_master();
            return Ok(());
        }
        anyhow::ensure!(
            self.runtime.apply_ui_volume(channel, value)?,
            "Unknown audio channel"
        );
        Ok(())
    }
    pub fn trigger(&mut self, key: &str, placement: Placement) {
        let permitted = match key {
            "brick.plant" => self
                .prefs
                .bool_or("$pref::Audio::PlayBrickPlantSound", true),
            "brick.move" | "brick.rotate" => {
                self.prefs.bool_or("$pref::Audio::PlayBrickMoveSound", true)
            }
            key if key.starts_with("ui.menu_note.") => {
                self.prefs.bool_or("$pref::Audio::MenuSounds", true)
            }
            _ => true,
        };
        if !permitted {
            return;
        }
        if let Some(id) = self.runtime.bank().trigger(key).map(str::to_string) {
            self.enqueue(id, placement, key, Voice::AS_AUTHORED);
        } else if self.warnings.len() < 64 {
            self.warnings
                .insert(format!("Unbound audio trigger: {key}"));
        }
    }
    /// Play a sound from outside the pack (an Add-On's own clip).
    pub fn play_asset(
        &mut self,
        asset: Arc<bri_audio::SoundAsset>,
        placement: Placement,
        gain: f32,
    ) {
        if self.runtime.play_asset(asset, placement, gain).is_err() {
            self.dropped = self.dropped.saturating_add(1);
        }
    }
    pub fn profile(&mut self, profile: &str, placement: Placement) {
        self.enqueue(profile.into(), placement, profile, Voice::AS_AUTHORED);
    }
    /// `profile` at `volume` times its own, an Add-On's choice.
    pub fn profile_scaled(&mut self, profile: &str, placement: Placement, volume: f32) {
        let voice = Voice {
            volume,
            ..Voice::AS_AUTHORED
        };
        self.enqueue(profile.into(), placement, profile, voice);
    }
    /// `profile` as an Add-On asked for it.
    pub fn profile_shaped(&mut self, profile: &str, placement: Placement, voice: Voice) {
        self.enqueue(profile.into(), placement, profile, voice);
    }
    /// Bounded local cosmetic starts. No backlog survives a tick or session reset.
    /// Unknown profiles are silent; this path never guesses a bank fallback.
    pub fn contact_profile(&mut self, profile: &str, point: [f32; 3], seed: u64) {
        if self.contact_pending.len() >= 16
            || profile.is_empty()
            || profile.len() > 256
            || !point.iter().all(|v| v.is_finite())
        {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        self.contact_pending
            .push_back((profile.to_owned(), point, seed));
    }
    fn enqueue(&mut self, id: String, placement: Placement, label: &str, voice: Voice) {
        if self.pending.len() == bri_sim::presentation::MAX_CUES {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        // Only native host-bound IDs reach here; counts are diagnostic rather than a replay log.
        if self.requested.len() < 512 || self.requested.contains_key(label) {
            *self.requested.entry(label.into()).or_default() += 1;
        }
        self.pending.push_back((id, placement, voice));
    }
    pub fn cue(&mut self, cue: &Cue) {
        let key = match &cue.kind {
            CueKind::WeaponEffect { .. }
            | CueKind::WeaponAnimation { .. }
            | CueKind::WeaponShell { .. }
            | CueKind::ScreenFlash { .. }
            | CueKind::Beam { .. }
            | CueKind::Tracer { .. }
            | CueKind::Decal { .. }
            | CueKind::MoverDecal { .. }
            | CueKind::BodyFlash { .. }
            | CueKind::BodyShell { .. } => return,
            CueKind::SoundShaped {
                profile,
                volume,
                pitch,
                radius,
            } => {
                let voice = Voice {
                    volume: *volume,
                    pitch: *pitch,
                    radius: *radius,
                };
                self.profile_shaped(profile, Placement::World(cue.position), voice);
                return;
            }
            CueKind::WeaponSound { profile } => {
                // A looping state sound belongs to the state, not to its
                // entry; `sync_image_loops` owns it.
                if !self.is_looping(profile) {
                    self.profile(profile, Placement::World(cue.position));
                }
                return;
            }
            CueKind::Jump { sound } => {
                self.body_sound(sound.as_deref(), "player.jump", cue.position);
                return;
            }
            CueKind::Plant => "brick.plant",
            // One break sound per `BREAK_SOUND_GAP_MS`, at the brick
            // (the event plays at the ghost brick's own transform).
            CueKind::BrickKill { .. } => {
                let gap = BREAK_SOUND_GAP_MS * u64::from(bri_weapons::TICK_HZ);
                if self
                    .last_break
                    .is_some_and(|last| cue.tick.abs_diff(last) * 1000 < gap)
                {
                    return;
                }
                self.last_break = Some(cue.tick);
                self.trigger("brick.break", Placement::World(cue.position));
                return;
            }
            CueKind::Pain {
                cry: true, sound, ..
            } => {
                self.body_sound(sound.as_deref(), "player.pain_cry", cue.position);
                return;
            }
            CueKind::Death { sound, .. } => {
                self.body_sound(sound.as_deref(), "player.death_cry", cue.position);
                return;
            }
            // `mediumSplashSoundVelocity` 10, `hardSplashSoundVelocity` 20.
            CueKind::Water {
                entered: true,
                speed,
                ..
            } => match *speed {
                s if s >= 20.0 => "player.water.impact_hard",
                s if s >= 10.0 => "player.water.impact_medium",
                _ => "player.water.impact_easy",
            },
            // The server sends an exit only past `exitSplashSoundVelocity`.
            CueKind::Water { .. } => "player.water.exit",
            // Emote, spawn and corpse sounds belong to their explosions.
            CueKind::Pain { .. }
            | CueKind::Burn { .. }
            | CueKind::Emote { .. }
            | CueKind::VehicleEffect { .. } => return,
            // The engine explosion operation sounds like v20's rocket.
            CueKind::Explosion { .. } => {
                self.profile("rocketExplodeSound", Placement::World(cue.position));
                return;
            }
            // PlayerTeleportExplosion has no `soundProfile`.
            CueKind::Teleport { .. } => return,
            CueKind::VehicleSound { sound, .. } => {
                if sound.contains('.') {
                    self.trigger(sound, Placement::World(cue.position));
                } else {
                    self.profile(sound, Placement::World(cue.position));
                }
                return;
            }
        };
        self.trigger(key, Placement::World(cue.position));
    }
    /// A player's jump or cry: its archetype's own `sound`, an empty one
    /// none, else the standard player's `standard` trigger.
    fn body_sound(&mut self, sound: Option<&str>, standard: &str, at: [f32; 3]) {
        match sound {
            Some("") => {}
            Some(profile) => self.profile(profile, Placement::World(at)),
            None => self.trigger(standard, Placement::World(at)),
        }
    }
    /// Keep one positional loop per music brick in step with the world.
    pub fn sync_music(&mut self, bricks: &bri_world::Bricks) {
        // Loops that would not start (64 at once, or a failed play) are
        // tried again on later revisions, as before.
        if !self.music_bricks.update(bricks) && self.music.len() == self.music_bricks.wanted.len() {
            return;
        }
        let wanted = &self.music_bricks.wanted;
        let stale: Vec<u64> = self
            .music
            .iter()
            .filter(|(id, (sound, position, _))| {
                wanted
                    .get(id)
                    .is_none_or(|(want, at)| want != sound || at != position)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if let Some((_, _, handle)) = self.music.remove(&id) {
                let result = self.runtime.stop_with_fade(handle, 0.2);
                self.record(result);
            }
        }
        let missing: Vec<(u64, String, [f32; 3])> = self
            .music_bricks
            .wanted
            .iter()
            .filter(|(id, _)| !self.music.contains_key(id))
            .map(|(id, (sound, position))| (*id, sound.clone(), *position))
            .collect();
        for (id, sound, position) in missing {
            if self.music.len() >= 64 {
                break;
            }
            match self.runtime.play(&sound, Placement::World(position)) {
                Ok(handle) => {
                    self.music.insert(id, (sound, position, handle));
                }
                Err(error) => self.record(Err(error)),
            }
        }
    }
    /// Play an Add-On rule's music track at the ears on the music bus,
    /// replacing the one before, or stop it (`track` none); either fades
    /// over `fade` seconds. The track is a pack sound or the bank's.
    pub fn play_music(&mut self, play: &bri_sim::session::MusicPlay) {
        if let Some(old) = self.rule_music.take() {
            let result = self.runtime.stop_with_fade(old.handle, play.fade);
            self.record(result);
        }
        let Some(track) = &play.track else {
            return;
        };
        let found = match self.pack_asset(track) {
            Some(asset) => Ok(asset),
            None => self.runtime.bank().resolve(track).map(|a| (a.clone(), 1.0)),
        };
        let (asset, volume) = match found {
            Ok(found) => found,
            Err(error) => return self.record(Err(error)),
        };
        let fade = play.fade.max(0.0);
        // A one-shot starts above the gain below which it would not start.
        let start = if fade == 0.0 {
            1.0
        } else if play.looping {
            0.0
        } else {
            0.1
        };
        let asset = Arc::new(asset.as_music(play.looping));
        match self
            .runtime
            .play_asset(asset, Placement::Listener, volume * start)
        {
            Ok(handle) => {
                self.rule_music = Some(RuleMusic {
                    handle,
                    volume,
                    fade,
                    elapsed: 0.0,
                });
            }
            Err(error) => self.record(Err(error)),
        }
    }
    /// Turn the world's sounds down as an Add-On rule asks, replacing any
    /// before; the ramp runs in [`ClientAudio::update`].
    pub fn set_world_volume(&mut self, set: bri_sim::session::WorldVolumeSet) {
        self.world_volume = Some(WorldVolume { set, elapsed: 0.0 });
    }
    /// Play an Add-On rule's ambient bed under its key on the effects bus,
    /// looping at the ears or at a point in the world, replacing the bed
    /// under that key, or stop it (`track` none); either fades over
    /// `fade` seconds.
    pub fn play_ambient(&mut self, play: &bri_sim::session::AmbientPlay) {
        if let Some(old) = self.rule_ambient.remove(&play.key) {
            let result = self.runtime.stop_with_fade(old.handle, play.fade);
            self.record(result);
        }
        let Some(track) = &play.track else {
            return;
        };
        let found = match self.pack_asset(track) {
            Some(asset) => Ok(asset),
            None => self.runtime.bank().resolve(track).map(|a| (a.clone(), 1.0)),
        };
        let (asset, volume) = match found {
            Ok(found) => found,
            Err(error) => return self.record(Err(error)),
        };
        let volume = volume * play.volume.clamped(0.0, 1.0);
        let fade = play.fade.max(0.0);
        let start = if fade == 0.0 { 1.0 } else { 0.0 };
        let (placement, range) = match play.at {
            Some(at) => (Placement::World(at), Some(play.range)),
            None => (Placement::Listener, None),
        };
        let asset = Arc::new(asset.as_ambient(range));
        match self.runtime.play_asset(asset, placement, volume * start) {
            Ok(handle) => {
                self.rule_ambient.insert(
                    play.key.clone(),
                    RuleMusic {
                        handle,
                        volume,
                        fade,
                        elapsed: 0.0,
                    },
                );
            }
            Err(error) => self.record(Err(error)),
        }
    }
    /// Make the looping positional sources playing match `sources`: a key not yet playing starts, one still wanted is moved and
    /// re-volumed, one no longer listed stops. A game mode calls this each tick with whatever its actors currently emit, so a
    /// source follows its actor and ends when the actor does. A sound no package offers is a counted fallback, not an error.
    pub fn sync_positional_sources(&mut self, sources: &[PositionalSource]) {
        let wanted: BTreeSet<u64> = sources.iter().map(|s| s.key).collect();
        let gone: Vec<u64> = self
            .positional
            .keys()
            .copied()
            .filter(|key| !wanted.contains(key))
            .collect();
        for key in gone {
            if let Some(old) = self.positional.remove(&key) {
                let result = self.runtime.stop(old.handle);
                self.record(result);
            }
        }
        for source in sources {
            if let Some(playing) = self.positional.get(&source.key) {
                if playing.source.same_voice(source) {
                    let (handle, scale) = (playing.handle, playing.pack_volume);
                    let moved = self
                        .runtime
                        .set_source_position(handle, source.position)
                        .and_then(|()| {
                            self.runtime
                                .set_voice_params(handle, scale * source.volume, 1.0)
                        });
                    self.record(moved);
                    if let Some(playing) = self.positional.get_mut(&source.key) {
                        playing.source = source.clone();
                    }
                    continue;
                }
                if let Some(old) = self.positional.remove(&source.key) {
                    let result = self.runtime.stop(old.handle);
                    self.record(result);
                }
            }
            let found = match self.pack_asset(&source.sound) {
                Some(found) => Ok(found),
                None => self
                    .runtime
                    .bank()
                    .resolve(&source.sound)
                    .map(|asset| (asset.clone(), 1.0)),
            };
            let (asset, pack_volume) = match found {
                Ok(found) => found,
                Err(error) => {
                    self.record(Err(error));
                    continue;
                }
            };
            let asset = Arc::new(asset.placed(
                source.reference_distance,
                source.max_distance,
                source.pitch,
                bri_audio::schema::Falloff::InverseClamped,
            ));
            match self.runtime.play_asset(
                asset,
                Placement::World(source.position),
                pack_volume * source.volume,
            ) {
                Ok(handle) => {
                    self.positional.insert(
                        source.key,
                        PlayingSource {
                            handle,
                            pack_volume,
                            source: source.clone(),
                        },
                    );
                }
                Err(error) => self.record(Err(error)),
            }
        }
    }
    /// Whether `profile` names a sound this client can play: an Add-On
    /// pack's own that loaded, else the bank's ([`Self::start`]).
    pub fn has_sound(&self, profile: &str) -> bool {
        self.pack_sounds.contains_key(&profile.to_ascii_lowercase())
            || self.runtime.bank().resolve(profile).is_ok()
    }
    pub fn is_looping(&self, profile: &str) -> bool {
        match self.pack_sounds.get(&profile.to_ascii_lowercase()) {
            Some((asset, _)) => asset.playback.looping,
            None => self
                .runtime
                .bank()
                .resolve(profile)
                .is_ok_and(|asset| asset.playback.looping),
        }
    }
    /// Decode the sounds `pack` ships (Add-On weapons), each read from
    /// beside its own `weapons.json` under `root` (the base weapons
    /// folder). A sound that fails to load is a warning, never an error:
    /// its weapon plays silently.
    pub fn set_pack_sounds(&mut self, pack: &bri_weapons::Pack, root: &Path) {
        self.install_pack_sounds(PreparedSounds::load(self.sound_bank(), pack, root));
    }
    pub(crate) fn sound_bank(&self) -> Arc<SoundBank> {
        self.runtime.bank().clone()
    }
    pub(crate) fn install_pack_sounds(&mut self, prepared: PreparedSounds) {
        self.pack_sounds = prepared.sounds;
        self.pack_sound_variants = prepared.variants;
        self.pack_sound_failures = prepared.failed;
        self.warnings.extend(
            prepared
                .warnings
                .into_iter()
                .take(64usize.saturating_sub(self.warnings.len())),
        );
    }
    /// Start `profile`: an Add-On pack's own sound, else the bank's.
    fn start(
        &mut self,
        profile: &str,
        placement: Placement,
        voice: Voice,
    ) -> Result<SoundHandle, AudioError> {
        let shaped = |asset: Arc<SoundAsset>| {
            if voice.pitch == 1.0 && voice.radius == 0.0 {
                asset
            } else {
                Arc::new(asset.shaped(voice.pitch, voice.radius))
            }
        };
        let handle = match self.pack_asset(profile) {
            Some((asset, volume)) => {
                self.runtime
                    .play_asset(shaped(asset), placement, volume * voice.volume)
            }
            None => {
                let asset = self.runtime.bank().resolve(profile)?.clone();
                self.runtime
                    .play_asset(shaped(asset), placement, voice.volume)
            }
        }?;
        if let Placement::World(at) = placement
            && self.fresh_world.len() < FRESH_WORLD_LIMIT
        {
            self.fresh_world.push((handle, at));
        }
        Ok(handle)
    }
    /// Judge the way each world sound takes to the listener ([`bri_audio::Hearing`]) and cut its range to match: every looping
    /// source, and a sound just started once, where it began. `hearing_at` is asked for a point in the world. `room` is the
    /// reverb of the space the listener is in.
    pub fn hear_world(
        &mut self,
        mut hearing_at: impl FnMut([f32; 3]) -> bri_audio::Hearing,
        room: Room,
    ) {
        if room != self.room {
            self.room = room;
            let result = self.runtime.set_reverb(room);
            self.record(result);
        }
        let judged: Vec<(SoundHandle, bri_audio::Hearing)> = self
            .positional
            .values()
            .map(|playing| (playing.handle, playing.source.position))
            .chain(self.fresh_world.drain(..))
            .map(|(handle, at)| (handle, hearing_at(at)))
            .collect();
        for (handle, hearing) in judged {
            let result = self.runtime.set_voice_hearing(handle, hearing);
            self.record(result);
        }
    }
    fn pack_asset(&mut self, profile: &str) -> Option<(Arc<SoundAsset>, f32)> {
        let seed = self.sound_choice_counter;
        if self
            .pack_sound_variants
            .contains_key(&profile.to_ascii_lowercase())
        {
            self.sound_choice_counter = self.sound_choice_counter.wrapping_add(1);
        }
        self.seeded_pack_asset(profile, seed)
    }
    fn seeded_pack_asset(&self, profile: &str, seed: u64) -> Option<(Arc<SoundAsset>, f32)> {
        let key = profile.to_ascii_lowercase();
        if let Some(choices) = self.pack_sound_variants.get(&key) {
            let unit = sound_choice_unit(seed);
            let choice = choose_sound(choices, unit);
            Some((choice.asset.clone(), choice.volume))
        } else {
            self.pack_sounds.get(&key).cloned()
        }
    }
    /// Keep one loop per mounted image whose current state has a looping
    /// sound, following its player, and stop it when the state ends.
    pub fn sync_image_loops(&mut self, wanted: &BTreeMap<(u64, u8), (String, [f32; 3])>) {
        let stale: Vec<_> = self
            .image_loops
            .iter()
            .filter(|(key, (sound, _))| wanted.get(key).is_none_or(|(want, _)| want != sound))
            .map(|(key, _)| *key)
            .collect();
        for key in stale {
            if let Some((_, handle)) = self.image_loops.remove(&key) {
                let result = self.runtime.stop_with_fade(handle, 0.05);
                self.record(result);
            }
        }
        for (key, (sound, position)) in wanted {
            if let Some((_, handle)) = self.image_loops.get(key) {
                let result = self.runtime.set_source_position(*handle, *position);
                self.record(result);
            } else if self.image_loops.len() < 64 {
                match self.start(sound, Placement::World(*position), Voice::AS_AUTHORED) {
                    Ok(handle) => {
                        self.image_loops.insert(*key, (sound.clone(), handle));
                    }
                    Err(error) => self.record(Err(error)),
                }
            }
        }
    }
    /// Keep one engine loop per vehicle in `wanted` (see [`engine_loop`]),
    /// following it with its pitch and volume, and fade out the rest.
    pub fn sync_engines(&mut self, wanted: &BTreeMap<u64, EngineLoop>) {
        let stale: Vec<_> = self
            .engines
            .iter()
            .filter(|(id, (sound, _))| wanted.get(id).is_none_or(|want| &want.sound != sound))
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if let Some((_, handle)) = self.engines.remove(&id) {
                let result = self.runtime.stop_with_fade(handle, 0.15);
                self.record(result);
            }
        }
        for (id, engine) in wanted {
            // The volume the pack declares the sound at, scaled.
            let volume = engine.volume
                * self
                    .pack_sounds
                    .get(&engine.sound.to_ascii_lowercase())
                    .map_or(1.0, |(_, v)| *v);
            let handle = match self.engines.get(id) {
                Some((_, handle)) => {
                    let handle = *handle;
                    let result = self.runtime.set_source_position(handle, engine.position);
                    self.record(result);
                    handle
                }
                None if self.engines.len() < 64 => {
                    match self.start(
                        &engine.sound,
                        Placement::World(engine.position),
                        Voice::AS_AUTHORED,
                    ) {
                        Ok(handle) => {
                            self.engines.insert(*id, (engine.sound.clone(), handle));
                            handle
                        }
                        Err(error) => {
                            self.record(Err(error));
                            continue;
                        }
                    }
                }
                None => continue,
            };
            let result = self.runtime.set_voice_params(handle, volume, engine.pitch);
            self.record(result);
        }
    }
    /// `ProjectileData.sound`: a loop that flies with each projectile and
    /// stops when it explodes or expires.
    pub fn sync_projectiles(
        &mut self,
        projectiles: &[bri_weapons::Projectile],
        pack: &bri_weapons::Pack,
    ) {
        let live: BTreeSet<u64> = projectiles.iter().map(|p| p.id).collect();
        for id in self
            .projectiles
            .difference(&live)
            .copied()
            .collect::<Vec<_>>()
        {
            self.projectiles.remove(&id);
            let result = self.runtime.despawn(projectile_entity(id));
            self.record(result);
        }
        for p in projectiles {
            let position = p.position.to_array();
            if self.projectiles.contains(&p.id) {
                let result = self
                    .runtime
                    .update_entity(projectile_entity(p.id), position);
                self.record(result);
                continue;
            }
            let Some(sound) = pack
                .projectiles
                .get(&p.definition)
                .map(|d| d.sound.as_str())
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            if self.projectiles.len() >= 64 {
                break;
            }
            self.projectiles.insert(p.id);
            let placement = Placement::Attached {
                entity: projectile_entity(p.id),
                position,
            };
            let result = self.start(sound, placement, Voice::AS_AUTHORED).map(|_| ());
            self.record(result);
        }
    }
    /// The portals the listener hears through this frame
    /// (`portal_view::hearing`), sent when they change.
    pub fn hear_through(&mut self, windows: Vec<bri_audio::Window>) {
        if windows != self.windows {
            let result = self.runtime.set_windows(&windows);
            self.record(result);
            self.windows = windows;
        }
    }
    pub fn clear(&mut self) {
        self.hear_through(Vec::new());
        self.music.clear();
        self.music_bricks = MusicBricks::default();
        self.rule_music = None;
        self.rule_ambient.clear();
        self.world_volume = None;
        let result = self
            .runtime
            .set_volume(bri_audio::VolumeControl::World, 1.0);
        self.record(result);
        for (_, old) in std::mem::take(&mut self.positional) {
            let result = self.runtime.stop(old.handle);
            self.record(result);
        }
        self.projectiles.clear();
        self.image_loops.clear();
        self.engines.clear();
        self.pending.clear();
        self.contact_pending.clear();
        let result = self.runtime.stop_all();
        self.record(result);
        // Flush stop even if another session is started before the next normal frame.
        self.runtime.update(1. / 48000.);
    }
    pub fn tick(&mut self, seconds: f32, listener: Listener) {
        let result = self.runtime.set_listener(listener);
        self.record(result);
        // Listener precedes starts so distance culling cannot use last session's pose.
        while let Some((id, placement, voice)) = self.pending.pop_front() {
            let result = self.start(&id, placement, voice).map(|_| ());
            self.record(result);
        }
        while let Some((id, point, seed)) = self.contact_pending.pop_front() {
            let asset = self
                .seeded_pack_asset(&id, seed)
                .or_else(|| {
                    self.runtime
                        .bank()
                        .resolve(&id)
                        .ok()
                        .map(|asset| (asset.clone(), 1.0))
                })
                .filter(|(asset, _)| !asset.playback.looping);
            if let Some((asset, volume)) = asset {
                match self
                    .runtime
                    .play_asset(asset, Placement::World(point), volume)
                {
                    Ok(handle) => self.fresh_world.push((handle, point)),
                    Err(error) => self.record(Err(error)),
                }
            } else {
                self.dropped = self.dropped.saturating_add(1);
            }
        }
        if let Some(w) = self.world_volume.as_mut() {
            w.elapsed += seconds;
            let (gain, over) = (w.gain(), w.elapsed >= w.set.seconds);
            if over {
                self.world_volume = None;
            }
            let result = self
                .runtime
                .set_volume(bri_audio::VolumeControl::World, gain);
            self.record(result);
        }
        let fading: Vec<(SoundHandle, f32)> = self
            .rule_music
            .iter_mut()
            .chain(self.rule_ambient.values_mut())
            .filter(|m| m.elapsed < m.fade)
            .map(|m| {
                m.elapsed += seconds;
                (m.handle, m.volume * (m.elapsed / m.fade).min(1.0))
            })
            .collect();
        for (handle, gain) in fading {
            let result = self.runtime.set_voice_params(handle, gain, 1.0);
            self.record(result);
        }
        self.runtime.update(seconds);
        self.runtime.drain_events();
    }
    pub fn stats(&self) -> AudioStats {
        self.runtime.stats()
    }
    pub fn take_capture(&mut self) -> Vec<f32> {
        self.runtime.take_capture()
    }
}

/// A rule's `world_volume` as it plays out.
struct WorldVolume {
    set: bri_sim::session::WorldVolumeSet,
    elapsed: f32,
}
impl WorldVolume {
    /// The world's volume now: easing down over `fade`, held, and easing
    /// back up over the last `fade` seconds.
    fn gain(&self) -> f32 {
        let bri_sim::session::WorldVolumeSet {
            scale,
            seconds,
            fade,
        } = self.set;
        let into = if fade > 0.0 {
            (self.elapsed / fade).min(1.0)
        } else {
            1.0
        };
        let out = if fade > 0.0 {
            ((seconds - self.elapsed) / fade).clamped(0.0, 1.0)
        } else {
            1.0
        };
        1.0 - (1.0 - scale) * into.min(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The world eases down over the fade, holds, and eases back up over
    /// the last fade seconds.
    #[test]
    fn a_world_volume_eases_in_holds_and_eases_out() {
        let at = |elapsed| {
            WorldVolume {
                set: bri_sim::session::WorldVolumeSet {
                    scale: 0.2,
                    seconds: 10.0,
                    fade: 2.0,
                },
                elapsed,
            }
            .gain()
        };
        assert_eq!(at(0.0), 1.0);
        assert!((at(1.0) - 0.6).abs() < 1e-6);
        assert!((at(2.0) - 0.2).abs() < 1e-6);
        assert!((at(5.0) - 0.2).abs() < 1e-6);
        assert!((at(9.0) - 0.6).abs() < 1e-6);
        assert!((at(10.0) - 1.0).abs() < 1e-6);
    }

    /// The engine runs while its driver's seat is taken, louder and higher
    /// with speed, and stops when the driver gets out or it is destroyed.
    #[test]
    fn an_engine_runs_with_its_driver_and_rises_with_speed() {
        let mut d = bri_vehicles::testing::car();
        d.engine_sound =
            Some(serde_json::from_value(serde_json::json!({ "sound": "buggy:engine" })).unwrap());
        let mut info = bri_sim::session::VehicleInfo {
            id: 1,
            definition: d.id.clone(),
            color: None,
            occupants: vec![None, Some(9), None, None],
            destroyed: false,
            turret_broken: false,
            scale: 1.0,
            lights: false,
        };
        let frame = |speed: f32| crate::vehicles::VehicleFrame {
            position: glam::Vec3::new(1.0, 2.0, 3.0),
            rotation: glam::Quat::IDENTITY,
            velocity: glam::Vec3::new(speed, 0.0, 0.0),
            steering: 0.0,
            wheel_suspension: vec![],
            wheel_rotation: vec![],
            wheel_contact: vec![],
            driver_jump_held: false,
            turret_aim: [0.0; 2],
        };
        // Only a passenger: the engine is off.
        assert_eq!(engine_loop(&d, &info, &frame(0.0)), None);
        info.occupants[0] = Some(7);
        let idle = engine_loop(&d, &info, &frame(0.0)).unwrap();
        assert_eq!(idle.sound, "buggy:engine");
        assert_eq!(idle.position, [1.0, 2.0, 3.0]);
        assert_eq!((idle.volume, idle.pitch), (0.6, 0.8));
        // Half the car's top speed (30) is half way; past it holds.
        let half = engine_loop(&d, &info, &frame(15.0)).unwrap();
        assert!((half.volume - 0.8).abs() < 1e-5 && (half.pitch - 1.2).abs() < 1e-5);
        let flat_out = engine_loop(&d, &info, &frame(90.0)).unwrap();
        assert_eq!((flat_out.volume, flat_out.pitch), (1.0, 1.6));
        info.destroyed = true;
        assert_eq!(engine_loop(&d, &info, &frame(15.0)), None);
        // A vehicle without an engine sound plays none.
        info.destroyed = false;
        d.engine_sound = None;
        assert_eq!(engine_loop(&d, &info, &frame(15.0)), None);
    }

    /// A layered engine plays its main loop and every layer whose curve has
    /// volume at this speed, and none that has gone quiet.
    #[test]
    fn engine_layers_play_only_while_audible_and_follow_their_curves() {
        let mut d = bri_vehicles::testing::car();
        d.max_speed = 30.0;
        let sound = |name: &str, curve: serde_json::Value| -> bri_vehicles::schema::EngineSound {
            serde_json::from_value(serde_json::json!({ "sound": name, "curve": curve })).unwrap()
        };
        d.engine_sound = Some(sound(
            "buggy:low",
            serde_json::json!([[0.0, 0.5, 1.0], [0.5, 0.5, 1.5], [0.6, 0.0, 1.6]]),
        ));
        d.engine_layers = vec![sound(
            "buggy:high",
            serde_json::json!([
                [0.0, 0.0, 1.0],
                [0.5, 0.0, 1.4],
                [0.6, 0.8, 1.5],
                [1.0, 0.8, 2.0]
            ]),
        )];
        let info = bri_sim::session::VehicleInfo {
            id: 1,
            definition: d.id.clone(),
            color: None,
            occupants: vec![Some(7), None, None, None],
            destroyed: false,
            turret_broken: false,
            scale: 1.0,
            lights: false,
        };
        let frame = |speed: f32| crate::vehicles::VehicleFrame {
            position: glam::Vec3::ZERO,
            rotation: glam::Quat::IDENTITY,
            velocity: glam::Vec3::new(speed, 0.0, 0.0),
            steering: 0.0,
            wheel_suspension: vec![],
            wheel_rotation: vec![],
            wheel_contact: vec![],
            driver_jump_held: false,
            turret_aim: [0.0; 2],
        };
        let at = |speed: f32| engine_loops(&d, &info, &frame(speed));
        // Parked: only the main loop, at its first key.
        let idle = at(0.0);
        assert_eq!(idle.len(), 1);
        assert_eq!(
            (idle[0].sound.as_str(), idle[0].volume, idle[0].pitch),
            ("buggy:low", 0.5, 1.0)
        );
        // Half way along the first segment: pitch is half way up.
        assert!((at(7.5)[0].pitch - 1.25).abs() < 1e-5);
        // Past the crossover both are heard, then only the high layer sounds
        // but the main loop is kept as the engine's marker.
        let fast = at(27.0);
        assert_eq!(fast.len(), 2);
        assert_eq!(fast[0].volume, 0.0);
        assert!((fast[1].volume - 0.8).abs() < 1e-5 && fast[1].sound == "buggy:high");
        // Past the last key the curve holds.
        assert_eq!(at(300.0)[1].pitch, 2.0);
    }

    /// A car sliding sideways skids at its grounded wheels, harder and
    /// louder the faster it slides; rolling straight or airborne it does not.
    #[test]
    fn sliding_tyres_skid_and_squeal_with_the_slide() {
        let mut d = bri_vehicles::testing::car();
        d.skid = Some(
            serde_json::from_value(serde_json::json!({
                "emitter": "car:skid", "sound": "car:squeal"
            }))
            .unwrap(),
        );
        d.skid.as_ref().unwrap().validate().unwrap();
        let info = bri_sim::session::VehicleInfo {
            id: 1,
            definition: d.id.clone(),
            color: None,
            occupants: vec![None; 4],
            destroyed: false,
            lights: false,
            turret_broken: false,
            scale: 1.0,
        };
        let wheels = d.wheels.len();
        let frame = |velocity: glam::Vec3, grounded: bool| crate::vehicles::VehicleFrame {
            position: glam::Vec3::ZERO,
            rotation: glam::Quat::IDENTITY,
            velocity,
            steering: 0.0,
            wheel_suspension: vec![],
            wheel_rotation: vec![],
            wheel_contact: vec![grounded; wheels],
            driver_jump_held: false,
            turret_aim: [0.0; 2],
        };
        let straight = frame(glam::Vec3::new(0.0, 0.0, 25.0), true);
        assert!(
            crate::actor_effects::tire_skids(1, &d, &straight)
                .0
                .is_empty()
        );
        assert_eq!(skid_loop(&d, &info, &straight), None);
        let drift = frame(glam::Vec3::new(7.5, 0.0, 20.0), true);
        let (sprays, hardest) = crate::actor_effects::tire_skids(1, &d, &drift);
        assert_eq!(sprays.len(), wheels);
        assert!(
            sprays
                .iter()
                .all(|s| s.emitter == "car:skid" && s.wheel >= wheels)
        );
        assert!((hardest - 0.5).abs() < 1e-4, "{hardest}");
        let squeal = skid_loop(&d, &info, &drift).unwrap();
        assert_eq!(squeal.sound, "car:squeal");
        let hard = skid_loop(&d, &info, &frame(glam::Vec3::new(30.0, 0.0, 0.0), true)).unwrap();
        assert!(hard.volume > squeal.volume && hard.pitch > squeal.pitch);
        let airborne = frame(glam::Vec3::new(30.0, 0.0, 0.0), false);
        assert!(
            crate::actor_effects::tire_skids(1, &d, &airborne)
                .0
                .is_empty()
        );
        assert_eq!(skid_loop(&d, &info, &airborne), None);
    }

    /// A world change costs the bricks it changed: only the first replica
    /// is read whole, however many revisions follow.
    #[test]
    fn music_bricks_follow_world_changes_without_rereading_the_world() {
        let brick = |x: f32, sound: Option<&str>| {
            let mut b = bri_world::Brick::new(
                bri_world::ContentRef::Resolved("brick".into()),
                [x, 0.0, 0.0],
                1,
            );
            b.sound = sound.map(|s| bri_world::ContentRef::Resolved(s.into()));
            b
        };
        let mut bricks: bri_world::Bricks = (0..5000u64)
            .map(|id| (id, brick(id as f32, (id == 7).then_some("music.loop"))))
            .collect();
        let mut index = MusicBricks::default();
        assert!(index.update(&bricks));
        assert_eq!(index.wanted().len(), 1);
        let after_first = index.visited;
        assert!(!index.update(&bricks.clone()), "the same replica");
        for revision in 0..50u64 {
            // A brick knocked out and back, a repaint: no music changes.
            let id = 100 + revision;
            let mut b = bricks[&id].clone();
            b.visible = !b.visible;
            bricks.insert(id, b);
            assert!(!index.update(&bricks));
        }
        bricks.insert(9000, brick(9.0, Some("music.other")));
        assert!(index.update(&bricks));
        bricks.remove(&7);
        assert!(index.update(&bricks));
        assert_eq!(
            index.wanted().keys().copied().collect::<Vec<_>>(),
            vec![9000]
        );
        assert_eq!(index.full_scans, 1, "the world was read whole again");
        assert!(
            index.visited - after_first < 200,
            "{} bricks examined for 52 changed",
            index.visited - after_first
        );
    }
    /// An audio pack folder and the interface sound it plays as a profile.
    struct Pack {
        root: std::path::PathBuf,
        note: String,
        _scratch: Option<crate::testing::ScratchDir>,
    }
    impl Pack {
        fn synthetic() -> Result<Self> {
            let scratch = crate::testing::ScratchDir::new("client-audio")?;
            crate::testing::audio::write_pack(scratch.path())?;
            Ok(Self {
                root: scratch.path().to_path_buf(),
                note: crate::testing::audio::NOTE.into(),
                _scratch: Some(scratch),
            })
        }
        fn content() -> Result<Self> {
            Ok(Self {
                root: bri_package::testing::pack_dir(
                    &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
                    "audio",
                ),
                note: "Note3Sound".into(),
                _scratch: None,
            })
        }
    }
    crate::testing::synthetic_and_content!(
        Pack: original_audio_defaults_listener_before_culling_preferences_and_teardown
    );
    fn original_audio_defaults_listener_before_culling_preferences_and_teardown(
        fx: &Pack,
    ) -> Result<()> {
        let root = &fx.root;
        let mut settings = Settings::default();
        settings
            .prefs
            .insert("$PREF::AUDIO::MasterVolume".into(), "0.8".into());
        let mut audio = ClientAudio::load(root, &mut settings, OutputKind::Offline)?;
        let defaults = audio.runtime.bank().defaults().clone();
        assert_eq!(
            settings
                .prefs
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case("$pref::Audio::masterVolume"))
                .count(),
            1
        );
        assert_eq!(audio.prefs.f32_or("$pref::Audio::masterVolume", 0.), 0.8);
        // Prefs the player has not set take the pack's defaults.
        assert_eq!(
            audio
                .prefs
                .bool_or("$pref::Audio::MenuSounds", !defaults.menu_sounds),
            defaults.menu_sounds
        );
        assert_eq!(
            audio.prefs.bool_or(
                "$pref::Audio::PlayBrickPlantSound",
                !defaults.play_brick_plant_sound
            ),
            defaults.play_brick_plant_sound
        );
        let position = [1000., 1., 0.];
        audio.trigger("brick.plant", Placement::World(position));
        audio.tick(
            0.2,
            Listener {
                position,
                ..Default::default()
            },
        );
        let samples = audio.take_capture();
        assert!(samples.iter().all(|v| v.is_finite()));
        assert!(samples.iter().any(|v| v.abs() > 0.00001));
        assert_eq!(audio.stats().started, 1);
        assert_eq!(audio.stats().culled, 0);
        settings
            .prefs
            .insert("$pref::Audio::PlayBrickPlantSound".into(), "0".into());
        audio.apply_settings(&settings);
        audio.trigger("brick.plant", Placement::World(position));
        audio.tick(
            0.2,
            Listener {
                position,
                ..Default::default()
            },
        );
        assert_eq!(audio.stats().started, 1);
        audio.profile(&fx.note, Placement::Listener);
        audio.tick(0.2, Listener::default());
        assert_eq!(audio.stats().started, 2);
        audio.clear();
        audio.tick(0.1, Listener::default());
        assert_eq!(audio.stats().real_voices, 0);
        assert_eq!(audio.stats().non_finite_samples, 0);
        assert!(audio.warnings.is_empty());
        // A Destructo Wand chain kill pops every brick from its own spot;
        // like a blast, it makes one break sound, heard at the first brick.
        let kill = |id: u64, tick: u64, brick: u64| Cue {
            id,
            tick,
            position: [1000., 1., brick as f32],
            kind: CueKind::BrickKill {
                brick,
                death: bri_sim::presentation::BrickDeath::Kill,
                definition: bri_world::ContentRef::Resolved("brick".into()),
                quarter_turns: 0,
                color: 0,
                color_effect: 0,
                shape_effect: 0,
                print: None,
                origin: [1000., 0., brick as f32],
                force: 12.,
                radius: 0.,
            },
        };
        let breaks = |audio: &ClientAudio| audio.requested.get("brick.break").copied();
        for brick in 1..=30 {
            audio.cue(&kill(brick, 50, brick));
        }
        assert_eq!(breaks(&audio), Some(1));
        assert_eq!(
            audio.pending.back().map(|(_, p, _)| *p),
            Some(Placement::World([1000., 1., 1.]))
        );
        // 80 ms is 9.6 ticks at 120 Hz: a death 9 ticks on is silent, the
        // next one 10 ticks on is heard.
        audio.cue(&kill(31, 59, 1));
        assert_eq!(breaks(&audio), Some(1), "within 80 ms of the last");
        audio.cue(&kill(32, 60, 1));
        assert_eq!(breaks(&audio), Some(2), "80 ms later");
        // A 250-brick blast arrives as three of v20's 100-brick explosion
        // messages in one tick; the client gate makes it one sound.
        for brick in 1..=250 {
            audio.cue(&kill(100 + brick, 120, brick));
        }
        assert_eq!(breaks(&audio), Some(3));
        assert!(audio.set_volume("master", f32::NAN).is_err());
        assert!(audio.set_volume("unknown", 0.5).is_err());
        Ok(())
    }

    /// A map's placed sounds loop where they are put: heard when the listener is within their range, silent beyond it, and still
    /// looping when it returns; a sound nothing offers is skipped.
    #[test]
    fn positional_sources_follow_their_key_and_are_heard_only_within_their_range() -> Result<()> {
        let fx = Pack::synthetic()?;
        let mut audio = ClientAudio::load(&fx.root, &mut Settings::default(), OutputKind::Offline)?;
        let source = |key: u64, sound: &str, position: [f32; 3]| PositionalSource {
            key,
            sound: sound.into(),
            position,
            volume: 1.0,
            pitch: 1.0,
            reference_distance: 5.0,
            max_distance: 40.0,
        };
        audio.sync_positional_sources(&[
            source(1, &fx.note, [1000., 1., 0.]),
            source(2, "nothing:offers_this", [1000., 1., 0.]),
        ]);
        assert_eq!(audio.positional.len(), 1, "the unknown sound is skipped");
        let near = Listener {
            position: [1000., 1., 0.],
            ..Default::default()
        };
        audio.tick(0.3, near);
        let samples = audio.take_capture();
        assert!(
            samples.iter().any(|v| v.abs() > 0.00001),
            "heard at its place"
        );
        // Moving the source far from the listener silences it; it keeps looping and does not restart.
        let handle = audio.positional[&1].handle;
        audio.sync_positional_sources(&[source(1, &fx.note, [2000., 1., 0.])]);
        assert_eq!(audio.positional[&1].handle, handle, "moved, not restarted");
        audio.tick(0.3, near);
        audio.tick(0.3, near);
        let silent = audio.take_capture();
        assert!(silent.iter().rev().take(2048).all(|v| v.abs() < 0.00001));
        assert_eq!(audio.stats().finished, 0, "a loop does not end");
        // A source no longer listed stops.
        audio.sync_positional_sources(&[]);
        assert!(audio.positional.is_empty());
        Ok(())
    }

    /// A wall between a placed sound and the listener cuts the distance it is heard at; with the way clear it is heard as authored.
    #[test]
    fn a_blocked_way_cuts_a_placed_sound_and_a_clear_one_restores_it() -> Result<()> {
        let fx = Pack::synthetic()?;
        let mut audio = ClientAudio::load(&fx.root, &mut Settings::default(), OutputKind::Offline)?;
        audio.sync_positional_sources(&[PositionalSource {
            key: 1,
            sound: fx.note.clone(),
            position: [1030., 1., 0.],
            volume: 1.0,
            pitch: 1.0,
            reference_distance: 5.0,
            max_distance: 40.0,
        }]);
        let listener = Listener {
            position: [1000., 1., 0.],
            ..Default::default()
        };
        let loud = |audio: &mut ClientAudio| {
            audio.tick(0.3, listener);
            audio.tick(0.3, listener);
            audio
                .take_capture()
                .iter()
                .rev()
                .take(2048)
                .any(|v| v.abs() > 0.00001)
        };
        audio.hear_world(|_| bri_audio::Hearing::CLEAR, Room::DRY);
        assert!(loud(&mut audio), "30 away is inside 40");
        // Behind a wall the way carries a third as far: 30 is now beyond it.
        let wall = bri_audio::Hearing {
            reach: 0.3,
            reference: 0.35,
        };
        audio.hear_world(|_| wall, Room::DRY);
        assert!(!loud(&mut audio), "cut to 12");
        audio.hear_world(|_| bri_audio::Hearing::CLEAR, Room::DRY);
        assert!(loud(&mut audio));
        Ok(())
    }

    /// A player type's own jump and cries play in place of the standard
    /// player's (the horse's `HorseJumpSound`), and an empty one plays
    /// nothing (a Bot_Hole bot's `jumpSound = ""`).
    #[test]
    fn a_player_types_own_sounds_replace_the_standard_ones() -> Result<()> {
        let fx = Pack::synthetic()?;
        let mut audio = ClientAudio::load(&fx.root, &mut Settings::default(), OutputKind::Offline)?;
        let cue = |id: u64, kind: CueKind| Cue {
            id,
            tick: id,
            position: [1000., 1., 0.],
            kind,
        };
        let own = Some(fx.note.clone());
        audio.cue(&cue(1, CueKind::Jump { sound: own.clone() }));
        audio.cue(&cue(
            2,
            CueKind::Pain {
                actor: 1,
                level: 20.,
                cry: true,
                sound: own.clone(),
            },
        ));
        audio.cue(&cue(
            3,
            CueKind::Death {
                actor: 1,
                sound: own,
            },
        ));
        assert_eq!(audio.requested.get(&fx.note).copied(), Some(3));
        assert_eq!(audio.pending.len(), 3);
        let silent = Some(String::new());
        audio.cue(&cue(
            4,
            CueKind::Jump {
                sound: silent.clone(),
            },
        ));
        audio.cue(&cue(
            5,
            CueKind::Death {
                actor: 1,
                sound: silent,
            },
        ));
        assert_eq!(audio.pending.len(), 3, "an empty sound plays nothing");
        // The standard player's jump is the `player.jump` trigger, which
        // this pack leaves unbound.
        audio.cue(&cue(6, CueKind::Jump { sound: None }));
        assert_eq!(audio.pending.len(), 3);
        assert!(
            audio
                .warnings
                .contains("Unbound audio trigger: player.jump")
        );
        let position = [1000., 1., 0.];
        audio.tick(
            0.2,
            Listener {
                position,
                ..Default::default()
            },
        );
        assert_eq!(audio.stats().started, 3);
        Ok(())
    }

    /// An Add-On's `AudioProfile` naming a file of the game itself (the HE
    /// Grenade's explosion is `base/data/sound/vehicleExplosion.wav`) plays
    /// the bank's copy of that file, at the Add-On's volume.
    #[test]
    fn an_add_on_sound_naming_the_games_own_file_plays_the_banks_copy() -> Result<()> {
        let fx = Pack::synthetic()?;
        let mut audio = ClientAudio::load(&fx.root, &mut Settings::default(), OutputKind::Offline)?;
        let mut pack: bri_weapons::Pack =
            serde_json::from_str(r#"{ "schema_version": 1, "id": "addon" }"#)?;
        let def = |file: &str| bri_weapons::SoundDef {
            variants: Vec::new(),
            file: file.into(),
            volume: 0.5,
            looping: false,
            local: false,
            music: false,
            package: None,
            stock: true,
            seconds: None,
        };
        let boom = "addon:sound/boomsound";
        pack.sounds.insert(
            boom.into(),
            def(&crate::testing::audio::TONE_PATH.to_ascii_uppercase()),
        );
        pack.sounds.insert(
            "addon:sound/gone".into(),
            def("base/data/sound/missing.wav"),
        );
        audio.set_pack_sounds(&pack, &fx.root);
        let (asset, volume) = &audio.pack_sounds[boom];
        assert_eq!(*volume, 0.5);
        assert_eq!(
            asset.data.sample_rate(),
            audio
                .runtime
                .bank()
                .resolve(crate::testing::audio::NOTE)?
                .data
                .sample_rate()
        );
        assert!(asset.playback.spatial.is_some(), "heard where it goes off");
        // A file the game does not have is a warning; its sound is silent.
        assert!(!audio.pack_sounds.contains_key("addon:sound/gone"));
        assert!(
            audio.warnings.iter().any(|w| w.contains("missing.wav")),
            "{:?}",
            audio.warnings
        );
        let position = [1000., 1., 0.];
        audio.profile(boom, Placement::World(position));
        audio.tick(
            0.2,
            Listener {
                position,
                ..Default::default()
            },
        );
        assert_eq!(audio.stats().started, 1);
        assert!(audio.take_capture().iter().any(|v| v.abs() > 0.00001));
        Ok(())
    }

    #[test]
    fn contact_audio_has_seeded_choices_bounded_starts_and_no_session_backlog() -> Result<()> {
        let fx = Pack::synthetic()?;
        let mut audio = ClientAudio::load(&fx.root, &mut Settings::default(), OutputKind::Offline)?;
        let data = audio
            .runtime
            .bank()
            .resolve(crate::testing::audio::NOTE)?
            .data
            .clone();
        let asset = Arc::new(SoundAsset::world("contact_a", data.clone(), 1., 10.));
        let other = Arc::new(SoundAsset::world("contact_b", data.clone(), 1., 10.));
        audio.pack_sound_variants.insert(
            "impact".into(),
            vec![
                SoundChoice {
                    asset: asset.clone(),
                    volume: 0.4,
                    weight: 1.,
                },
                SoundChoice {
                    asset: other,
                    volume: 0.2,
                    weight: 3.,
                },
            ],
        );
        for seed in 0..100 {
            let (before, gain) = audio.seeded_pack_asset("impact", seed).unwrap();
            audio.pack_asset("impact");
            let (after, later_gain) = audio.seeded_pack_asset("impact", seed).unwrap();
            assert!(Arc::ptr_eq(&before, &after));
            assert_eq!(gain, later_gain);
            assert_eq!(
                gain,
                if Arc::ptr_eq(&before, &asset) {
                    0.4
                } else {
                    0.2
                }
            );
        }
        let counter = audio.sound_choice_counter;
        for _ in 0..1000 {
            audio.contact_profile("impact", [1000., 1., 0.], 12);
        }
        assert_eq!(audio.contact_pending.len(), 16);
        assert_eq!(audio.dropped, 984);
        audio.tick(
            0.05,
            Listener {
                position: [1000., 1., 0.],
                ..Default::default()
            },
        );
        assert_eq!(audio.sound_choice_counter, counter);
        assert!(audio.contact_pending.is_empty());
        assert_eq!(
            audio.stats().started,
            16,
            "listener must precede local starts"
        );
        assert!(audio.take_capture().iter().any(|v| v.abs() > 1e-5));
        let started = audio.stats().started;
        audio.tick(0.05, Listener::default());
        assert_eq!(audio.stats().started, started);
        audio.contact_profile("missing", [0.; 3], 0);
        let mut looping = SoundAsset::world("contact_loop", data, 1., 10.);
        looping.playback.looping = true;
        audio
            .pack_sounds
            .insert("loop".into(), (Arc::new(looping), 1.));
        audio.contact_profile("loop", [0.; 3], 0);
        audio.tick(0.01, Listener::default());
        assert_eq!(audio.stats().started, started);
        audio.contact_profile("impact", [0.; 3], 1);
        audio.clear();
        assert!(audio.contact_pending.is_empty());
        audio.tick(0.01, Listener::default());
        assert_eq!(audio.stats().started, started);
        Ok(())
    }

    #[test]
    fn one_shot_variants_choose_shared_clips_keep_gains_and_reload_cleanly() -> Result<()> {
        let fx = Pack::synthetic()?;
        let dir = tempfile::tempdir()?;
        for (file, frequency) in [("a.wav", 220.), ("b.wav", 660.)] {
            let pcm: Vec<_> = (0..2205)
                .map(|i| (i as f32 / 22050. * frequency * std::f32::consts::TAU).sin() * 0.25)
                .collect();
            std::fs::write(
                dir.path().join(file),
                bri_audio::wav::encode_pcm16(22050, 1, &pcm),
            )?;
        }
        let mut value: serde_json::Value = serde_json::from_str(
            r#"{"id":"test","sounds":{
            "a":{"file":"a.wav","volume":0.8},
            "b":{"file":"b.wav","volume":0.4},
            "hit":{"file":"a.wav","volume":0.5,"variants":[
                {"profile":"a","weight":1.0},{"profile":"b","weight":3.0}]}
        }}"#,
        )?;
        value["schema_version"] = bri_weapons::SCHEMA.into();
        let mut pack: bri_weapons::Pack = serde_json::from_value(value)?;
        pack.validate()?;
        let mut audio = ClientAudio::load(&fx.root, &mut Settings::default(), OutputKind::Offline)?;
        audio.set_pack_sounds(&pack, dir.path());
        let a = audio.pack_sounds["a"].0.clone();
        let b = audio.pack_sounds["b"].0.clone();
        let choices = &audio.pack_sound_variants["hit"];
        assert!(Arc::ptr_eq(&choose_sound(choices, 0.).asset, &a));
        assert!(Arc::ptr_eq(&choose_sound(choices, 0.249).asset, &a));
        assert!(Arc::ptr_eq(&choose_sound(choices, 0.25).asset, &b));
        assert!(Arc::ptr_eq(&choose_sound(choices, 0.999).asset, &b));
        let mut selected = [0; 2];
        for _ in 0..128 {
            let (asset, gain) = audio.pack_asset("HIT").unwrap();
            if Arc::ptr_eq(&asset, &a) {
                assert!((gain - 0.4).abs() < 1e-6);
                selected[0] += 1;
            } else {
                assert!(Arc::ptr_eq(&asset, &b));
                assert!((gain - 0.2).abs() < 1e-6);
                selected[1] += 1;
            }
        }
        assert!(selected[0] > 0 && selected[1] > selected[0]);
        audio.profile("hit", Placement::World([0.; 3]));
        audio.tick(0.05, Listener::default());
        assert_eq!(audio.stats().started, 1);
        assert!(audio.take_capture().iter().any(|v| v.abs() > 1e-5));
        let mut invalid = pack.clone();
        invalid.sounds.get_mut("a").unwrap().variants = invalid.sounds["hit"].variants.clone();
        assert!(
            invalid.validate().is_err(),
            "recursive variant references rejected"
        );
        invalid = pack.clone();
        invalid.sounds.get_mut("hit").unwrap().looping = true;
        assert!(
            invalid.validate().is_err(),
            "loop contents must remain stable"
        );
        invalid = pack.clone();
        invalid.sounds.get_mut("b").unwrap().local = true;
        assert!(
            invalid.validate().is_err(),
            "variant placement cannot change per choice"
        );
        let mut missing = pack.clone();
        missing.sounds.get_mut("b").unwrap().file = "missing.wav".into();
        audio.set_pack_sounds(&missing, dir.path());
        assert!(audio.has_sound("hit"));
        assert_eq!(audio.pack_sound_variants["hit"].len(), 1);
        assert!(Arc::ptr_eq(
            &audio.pack_asset("hit").unwrap().0,
            &audio.pack_sounds["a"].0
        ));
        assert!(audio.pack_sound_failures.contains_key("b"));
        pack.sounds.get_mut("hit").unwrap().variants.clear();
        audio.set_pack_sounds(&pack, dir.path());
        assert!(!audio.pack_sound_variants.contains_key("hit"));
        assert!((audio.pack_asset("hit").unwrap().1 - 0.5).abs() < 1e-6);
        Ok(())
    }

    /// An Add-On weapon's own stereo sounds (the Mini-Nuke's launch and
    /// blast are 44.1 kHz stereo `.wav`s with 3D descriptions) are heard
    /// where they go off and fade with distance, like every other weapon's,
    /// rather than at full volume in both ears wherever the player is.
    #[test]
    fn an_add_on_weapons_stereo_sounds_fade_with_distance() -> Result<()> {
        let fx = Pack::synthetic()?;
        let dir = tempfile::tempdir()?;
        let rate = 44_100;
        let samples: Vec<f32> = (0..rate / 2)
            .flat_map(|i| {
                let s = (i as f32 / rate as f32 * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                [s, s]
            })
            .collect();
        std::fs::write(
            dir.path().join("blast.wav"),
            bri_audio::wav::encode_pcm16(rate, 2, &samples),
        )?;
        let mut pack: bri_weapons::Pack =
            serde_json::from_str(r#"{ "schema_version": 1, "id": "addon" }"#)?;
        let blast = "addon:sound/mininukeexplodesound";
        pack.sounds.insert(
            blast.into(),
            bri_weapons::SoundDef {
                variants: Vec::new(),
                file: "blast.wav".into(),
                volume: 1.0,
                looping: false,
                local: false,
                music: false,
                package: None,
                stock: false,
                seconds: None,
            },
        );
        let heard = |distance: f32| -> Result<f32> {
            let mut audio =
                ClientAudio::load(&fx.root, &mut Settings::default(), OutputKind::Offline)?;
            audio.set_pack_sounds(&pack, dir.path());
            assert!(audio.warnings.is_empty(), "{:?}", audio.warnings);
            audio.profile(blast, Placement::World([0.0, 0.0, -distance]));
            audio.tick(0.2, Listener::default());
            let capture = audio.take_capture();
            Ok((capture.iter().map(|v| v * v).sum::<f32>() / capture.len().max(1) as f32).sqrt())
        };
        let (near, mid, far) = (heard(5.0)?, heard(35.0)?, heard(100.0)?);
        assert!(near > 0.01, "heard up close: {near}");
        assert!(
            mid > 0.0 && mid < near * 0.8,
            "fainter further off: {near} then {mid}"
        );
        assert_eq!(far, 0.0, "out of earshot");
        Ok(())
    }

    #[test]
    fn a_sound_that_cannot_play_is_a_counted_fallback_of_kind_sound() {
        report_fallback(&AudioError::UnknownSound(
            "audio-fallback-test-track".into(),
        ));
        report_fallback(&AudioError::UnknownSound(
            "audio-fallback-test-track".into(),
        ));
        let mine: Vec<_> = bri_console::fallback::snapshot()
            .into_iter()
            .filter(|e| e.item == "audio-fallback-test-track")
            .collect();
        assert_eq!(mine.len(), 1, "{mine:?}");
        assert_eq!(mine[0].kind, "sound");
        assert_eq!(mine[0].count, 2);
    }
}
