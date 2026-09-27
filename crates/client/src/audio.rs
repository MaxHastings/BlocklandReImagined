//! Client audio adapter. Tests construct Null; only the explicit executable --run
//! path selects Device. Sound failures remain diagnostic, never gameplay failures.
use anyhow::Result;
use bri_audio::*;
use bri_sim::presentation::{Cue, CueKind};
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
        for (key, channel) in [
            ("$pref::Audio::masterVolume", "master"),
            ("$pref::Audio::channelVolume1", "shell"),
            ("$pref::Audio::channelVolume2", "sim"),
        ] {
            let value = self.prefs.f32_or(key, 1.);
            let value = if value.is_finite() {
                value.clamp(0., 1.)
            } else {
                1.
            };
            let result = self.runtime.apply_ui_volume(channel, value).map(|_| ());
            self.record(result);
        }
        let result = self
            .runtime
            .set_music_enabled(self.prefs.bool_or("$pref::Audio::PlayMusic", true));
        self.record(result);
    }
    pub fn set_volume(&mut self, channel: &str, value: f32) -> Result<()> {
        anyhow::ensure!(
            value.is_finite() && (0.0..=1.).contains(&value),
            "Invalid audio volume"
        );
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
                self.profile(profile, Placement::World(cue.position));
                return;
            }
            CueKind::Jump => "player.jump",
            CueKind::Plant => "brick.plant",
            CueKind::Break => "brick.break",
            CueKind::HammerHit => "tool.hammer.hit",
            CueKind::WrenchHit => "tool.wrench.hit",
            CueKind::Pain { .. } => "player.pain_cry",
            CueKind::Death { .. } => "player.death_cry",
            CueKind::Spawn { .. } => "player.spawn",
            CueKind::Emote { name, .. } if name == "alarm" => "emote.alarm",
            CueKind::Emote { .. } | CueKind::VehicleEffect { .. } => return,
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
    pub fn sync_music(&mut self, bricks: &BTreeMap<u64, bri_world::Brick>) {
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
    pub fn clear(&mut self) {
        self.music.clear();
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
