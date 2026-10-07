//! Client audio adapter. Tests construct Null; only the explicit executable --run
//! path selects Device. Sound failures remain diagnostic, never gameplay failures.
use anyhow::Result;
use bri_audio::*;
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
    warnings: BTreeSet<String>,
}

pub trait SoundLookup {
    fn has_sound(&self, profile: &str) -> bool;
}
impl SoundLookup for ClientAudio {
    fn has_sound(&self, profile: &str) -> bool {
        ClientAudio::has_sound(self, profile)
    }
}
impl SoundLookup for PreparedSounds {
    fn has_sound(&self, profile: &str) -> bool {
        self.sounds.contains_key(&profile.to_ascii_lowercase())
            || self.bank.resolve(profile).is_ok()
    }
}
impl PreparedSounds {
    pub(crate) fn load(bank: Arc<SoundBank>, pack: &bri_weapons::Pack, root: &Path) -> Self {
        let mut prepared = Self {
            bank: bank.clone(),
            sounds: BTreeMap::new(),
            warnings: BTreeSet::new(),
        };
        for (profile, def) in &pack.sounds {
            let (near, far) = PACK_SOUND_RANGE;
            // The game's own file, played from the bank's copy.
            let stock = def.stock.then(|| {
                bank.clip_at(&def.file)
                    .map(|clip| SoundAsset::world(profile, clip.clone(), near, far))
                    .ok_or_else(|| format!("the game has no sound {}", def.file))
            });
            let dir = bri_weapons::sound_root(root, def);
            let loaded = stock.unwrap_or_else(|| {
                bri_package::path::inside(&dir, &def.file).and_then(|path| {
                    use std::io::Read;
                    let mut bytes = Vec::new();
                    std::fs::File::open(&path)
                        .and_then(|f| {
                            f.take(bri_audio::bank::MAX_DECODED_CLIP_BYTES as u64 + 1)
                                .read_to_end(&mut bytes)
                        })
                        .map_err(|e| e.to_string())?;
                    let extension = Path::new(&def.file)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or_default();
                    SoundAsset::decoded(profile, &bytes, extension, near, far)
                })
            });
            match loaded {
                Ok(mut asset) => {
                    asset.playback.looping = def.looping;
                    prepared
                        .sounds
                        .insert(profile.clone(), (Arc::new(asset), def.volume));
                }
                Err(error) => {
                    if prepared.warnings.len() < 64 {
                        prepared
                            .warnings
                            .insert(format!("Weapon sound {profile}: {error}"));
                    }
                }
            }
        }
        prepared
    }
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
    pending: VecDeque<(String, Placement)>,
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
    /// The portals the listener hears through, as last sent.
    windows: Vec<bri_audio::Window>,
}
/// How near an Add-On weapon sound plays at full volume, and how far it
/// carries, in world units: v20's `AudioClose3d`/`AudioDefault3d` range.
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
            warnings,
            dropped: 0,
            server_dropped: 0,
            requested: BTreeMap::new(),
            prefs: Prefs::default(),
            music: BTreeMap::new(),
            music_bricks: MusicBricks::default(),
            projectiles: BTreeSet::new(),
            image_loops: BTreeMap::new(),
            master: 1.,
            mute_in_background: false,
            focused: true,
            last_break: None,
            pack_sounds: BTreeMap::new(),
            windows: Vec::new(),
        };
        audio.apply_settings(settings);
        Ok(audio)
    }
    fn record(&mut self, result: std::result::Result<(), AudioError>) {
        if let Err(error) = result {
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
            self.enqueue(id, placement, key);
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
        self.enqueue(profile.into(), placement, profile);
    }
    fn enqueue(&mut self, id: String, placement: Placement, label: &str) {
        if self.pending.len() == bri_sim::presentation::MAX_CUES {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        // Only native host-bound IDs reach here; counts are diagnostic rather than a replay log.
        if self.requested.len() < 512 || self.requested.contains_key(label) {
            *self.requested.entry(label.into()).or_default() += 1;
        }
        self.pending.push_back((id, placement));
    }
    pub fn cue(&mut self, cue: &Cue) {
        let key = match &cue.kind {
            CueKind::WeaponEffect { .. }
            | CueKind::WeaponAnimation { .. }
            | CueKind::WeaponShell { .. }
            | CueKind::Beam { .. }
            | CueKind::Tracer { .. } => return,
            CueKind::WeaponSound { profile } => {
                // A looping state sound belongs to the state, not to its
                // entry; `sync_image_loops` owns it.
                if !self.is_looping(profile) {
                    self.profile(profile, Placement::World(cue.position));
                }
                return;
            }
            CueKind::Jump => "player.jump",
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
            CueKind::Pain { cry: true, .. } => "player.pain_cry",
            CueKind::Death { .. } => "player.death_cry",
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
        self.warnings.extend(
            prepared
                .warnings
                .into_iter()
                .take(64usize.saturating_sub(self.warnings.len())),
        );
    }
    /// Start `profile`: an Add-On pack's own sound, else the bank's.
    fn start(&mut self, profile: &str, placement: Placement) -> Result<SoundHandle, AudioError> {
        match self.pack_sounds.get(&profile.to_ascii_lowercase()) {
            Some((asset, volume)) => self.runtime.play_asset(asset.clone(), placement, *volume),
            None => self.runtime.play(profile, placement),
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
                match self.start(sound, Placement::World(*position)) {
                    Ok(handle) => {
                        self.image_loops.insert(*key, (sound.clone(), handle));
                    }
                    Err(error) => self.record(Err(error)),
                }
            }
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
            let result = self.start(sound, placement).map(|_| ());
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
        self.projectiles.clear();
        self.image_loops.clear();
        self.pending.clear();
        let result = self.runtime.stop_all();
        self.record(result);
        // Flush stop even if another session is started before the next normal frame.
        self.runtime.update(1. / 48000.);
    }
    pub fn tick(&mut self, seconds: f32, listener: Listener) {
        let result = self.runtime.set_listener(listener);
        self.record(result);
        // Listener precedes starts so distance culling cannot use last session's pose.
        while let Some((id, placement)) = self.pending.pop_front() {
            let result = self.start(&id, placement).map(|_| ());
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

#[cfg(test)]
mod tests {
    use super::*;

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
            audio.pending.back().map(|(_, p)| *p),
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
            file: file.into(),
            volume: 0.5,
            looping: false,
            local: false,
            package: None,
            stock: true,
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
}
