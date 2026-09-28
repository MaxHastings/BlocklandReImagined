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
    /// Looping image state sounds (spray hiss, push broom) keyed by
    /// (player, hand); they play only while that state lasts.
    image_loops: BTreeMap<(u64, u8), (String, SoundHandle)>,
    /// Projectile `sound` loops keyed by projectile id.
    projectiles: BTreeSet<u64>,
    /// The player's master volume, before any background mute.
    master: f32,
    mute_in_background: bool,
    focused: bool,
}
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
            projectiles: BTreeSet::new(),
            image_loops: BTreeMap::new(),
            master: 1.,
            mute_in_background: false,
            focused: true,
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
            | CueKind::WeaponShell { .. } => return,
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
            CueKind::BrickKill { .. } => "brick.break",
            CueKind::HammerHit => "tool.hammer.hit",
            CueKind::WrenchHit => "tool.wrench.hit",
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
        let wanted: BTreeMap<u64, (String, [f32; 3])> = bricks
            .iter()
            .filter_map(|(id, b)| match &b.sound {
                Some(bri_world::ContentRef::Resolved(sound)) => {
                    Some((*id, (sound.clone(), b.position)))
                }
                _ => None,
            })
            .collect();
        let stale: Vec<u64> = self
            .music
            .iter()
            .filter(|(id, (sound, position, _))| {
                wanted.get(id) != Some(&(sound.clone(), *position))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if let Some((_, _, handle)) = self.music.remove(&id) {
                let result = self.runtime.stop_with_fade(handle, 0.2);
                self.record(result);
            }
        }
        for (id, (sound, position)) in wanted {
            if self.music.contains_key(&id) || self.music.len() >= 64 {
                continue;
            }
            match self.runtime.play(&sound, Placement::World(position)) {
                Ok(handle) => {
                    self.music.insert(id, (sound, position, handle));
                }
                Err(error) => self.record(Err(error)),
            }
        }
    }
    pub fn is_looping(&self, profile: &str) -> bool {
        self.runtime
            .bank()
            .resolve(profile)
            .is_ok_and(|asset| asset.playback.looping)
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
                let result = self
                    .runtime
                    .set_source_position(*handle, *position);
                self.record(result);
            } else if self.image_loops.len() < 64 {
                match self.runtime.play(sound, Placement::World(*position)) {
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
    pub fn sync_projectiles(&mut self, projectiles: &[bri_weapons::Projectile], pack: &bri_weapons::Pack) {
        let live: BTreeSet<u64> = projectiles.iter().map(|p| p.id).collect();
        for id in self.projectiles.difference(&live).copied().collect::<Vec<_>>() {
            self.projectiles.remove(&id);
            let result = self.runtime.despawn(projectile_entity(id));
            self.record(result);
        }
        for p in projectiles {
            let position = p.position.to_array();
            if self.projectiles.contains(&p.id) {
                let result = self.runtime.update_entity(projectile_entity(p.id), position);
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
            let result = self.runtime.play(sound, placement).map(|_| ());
            self.record(result);
        }
    }
    pub fn clear(&mut self) {
        self.music.clear();
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
            let result = self.runtime.play(&id, placement).map(|_| ());
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
    #[test]
    #[ignore = "uses delivered native audio pack; silent offline output only"]
    fn original_audio_defaults_listener_before_culling_preferences_and_teardown() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/audio-pack-001");
        let mut settings = Settings::default();
        settings
            .prefs
            .insert("$PREF::AUDIO::MasterVolume".into(), "0.8".into());
        let mut audio = ClientAudio::load(&root, &mut settings, OutputKind::Offline)?;
        assert_eq!(
            settings
                .prefs
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case("$pref::Audio::masterVolume"))
                .count(),
            1
        );
        assert_eq!(audio.prefs.f32_or("$pref::Audio::masterVolume", 0.), 0.8);
        assert!(audio.prefs.bool_or("$pref::Audio::MenuSounds", false));
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
        audio.profile("Note3Sound", Placement::Listener);
        audio.tick(0.2, Listener::default());
        assert_eq!(audio.stats().started, 2);
        audio.clear();
        audio.tick(0.1, Listener::default());
        assert_eq!(audio.stats().real_voices, 0);
        assert_eq!(audio.stats().non_finite_samples, 0);
        assert!(audio.warnings.is_empty());
        assert!(audio.set_volume("master", f32::NAN).is_err());
        assert!(audio.set_volume("unknown", 0.5).is_err());
        Ok(())
    }
}
