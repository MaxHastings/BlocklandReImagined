//! Proximity voice chat in the client: opens the microphone and speakers
//! while a game session lasts, sends the player's voice to the host and plays
//! the voices it relays from where their speakers stand.
//!
//! The audio work (capture, codec, jitter buffers, mixing, devices) is
//! `bri_audio::voice`; the transport is `bri_net::voice`. This ties them
//! together once a frame: it holds no network or audio state of its own beyond
//! what a session needs, and a missing microphone or speakers, or a device that
//! fails, costs the voice chat and never the game.
//!
//! Settings are prefs, read through [`Settings`](bri_ui::api::Settings) like
//! every other preference; Options has the first four:
//!
//! * `$pref::Voice::Mode` (default 0): how the player's voice is sent, 0
//!   Push-to-Talk, 1 Voice Activity Detection, 2 Disabled (no microphone is
//!   opened; other players are still heard).
//! * `$pref::Voice::InputDevice` (default empty): the microphone by name;
//!   empty, or a name that is not plugged in, is the system's default.
//! * `$pref::Voice::Volume` (default 1): other players' voices, 0 to 1; 0
//!   silences them without turning voice chat off.
//! * `$pref::Voice::VoiceActivation`: before there was a mode, 1 chose voice
//!   activity; it still does when no mode is saved.
//! * `$pref::Voice::Enabled` (default 1): 0 turns voice chat off entirely;
//!   no device is opened and no indicator shows.
//! * `$pref::Voice::Threshold` (default 0.02): the microphone level that
//!   counts as a voice, 0 to 1 (the indicator and voice activity use it).
//! * `$pref::Voice::MicVolume` (default 1): microphone gain, 0 to 4.
//!
//! Spatial range is selected by host packages, separately for each voice route.
use bri_audio::{
    Listener, VoiceChat,
    voice::{Activation, Settings as VoiceSettings},
};
use bri_console::Clamp;
use bri_net::voice::{Heard, VoiceActivity, VoiceLink};
use bri_ui::{
    api::{Settings, VoiceIndicator, VoiceMode, VoicePreview},
    prefs::Prefs,
    screens::options::{VOICE_VOLUME, voice_device, voice_mode},
};
use bri_world::OwnerId;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

/// How long to wait before opening the devices again after they failed or
/// were not there.
const RETRY: Duration = Duration::from_secs(10);
/// Refresh active speaker state so packet loss and late joiners recover.
const ACTIVITY_REFRESH: Duration = Duration::from_millis(500);
/// A missing activity refresh is treated as a disconnect/stale signal.
const REMOTE_ACTIVITY_TIMEOUT: Duration = Duration::from_millis(1_500);

/// What the player's prefs ask of voice chat.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub settings: VoiceSettings,
    pub mode: VoiceMode,
    pub master_volume: f32,
    pub mute_in_background: bool,
    /// The microphone by name; None is the system's default.
    pub device: Option<String>,
}

/// The voice chat the player's prefs ask for.
pub fn config_from(prefs: &BTreeMap<String, String>) -> Config {
    let prefs = Prefs::new(&BTreeMap::new(), prefs);
    let mode = voice_mode(&prefs);
    Config {
        settings: VoiceSettings {
            enabled: prefs.bool_or("$pref::Voice::Enabled", true),
            activation: match mode {
                VoiceMode::PushToTalk => Activation::PushToTalk,
                VoiceMode::VoiceActivity => Activation::Voice,
                VoiceMode::Disabled => Activation::Disabled,
            },
            threshold: prefs.f32_or("$pref::Voice::Threshold", 0.02),
            mic_gain: prefs.f32_or("$pref::Voice::MicVolume", 1.0),
            volume: prefs.f32_or(VOICE_VOLUME, 1.0),
            range: bri_net::voice::RANGE,
        }
        .sanitized(),
        mode,
        master_volume: bri_ui::screens::options::volume(&prefs, "$pref::Audio::masterVolume"),
        mute_in_background: prefs.bool_or("$pref::Audio::MuteInBackground", false),
        device: voice_device(&prefs),
    }
}

/// The microphones voice chat can listen to, by name.
pub fn input_devices() -> Vec<String> {
    bri_audio::input_devices()
}

pub struct VoiceClient {
    config: Config,
    chat: Option<VoiceChat>,
    /// Driver startup can block; poll completion without blocking a game frame.
    opening: Option<(
        Config,
        std::thread::JoinHandle<Result<VoiceChat, bri_audio::AudioError>>,
    )>,
    /// Push To Talk is held. Kept here so a chat opened (or reopened) while
    /// it is held starts with it.
    talking: bool,
    /// A session is on and voice chat is enabled: the HUD indicator shows.
    active: bool,
    session_link: Option<usize>,
    routes: bri_net::voice::RouteTable,
    hud_activity: BTreeMap<OwnerId, Instant>,
    focused: bool,
    server_blocked: bool,
    /// Options is open: the existing capture stream also feeds its input meter,
    /// even when the player is at the main menu and no network link exists.
    preview_active: bool,
    sequence: u16,
    /// No attempt to open the devices before this.
    retry_at: Option<Instant>,
    /// Whether the player was told the devices could not be opened, so a
    /// machine without a microphone is told once, not every ten seconds.
    told: bool,
    outgoing: Vec<bri_audio::CapturedFrame>,
    sequence_base: u16,
    server_muted: BTreeSet<OwnerId>,
    heard: Vec<Heard>,
    activity_in: Vec<VoiceActivity>,
    speaking: Vec<OwnerId>,
    /// The players this player has muted locally (the F2 list): their voice
    /// is not played back, and they show no speech state at all.
    muted: BTreeSet<OwnerId>,
    remote_activity: BTreeMap<OwnerId, Instant>,
    sent_activity: Option<bool>,
    activity_sent_at: Option<Instant>,
    /// The indicator the UI was last given.
    shown: Option<VoiceIndicator>,
}

impl VoiceClient {
    pub fn new(settings: &Settings) -> Self {
        Self {
            config: config_from(&settings.prefs),
            chat: None,
            opening: None,
            talking: false,
            active: false,
            session_link: None,
            routes: bri_net::voice::RouteTable::default(),
            hud_activity: BTreeMap::new(),
            focused: true,
            server_blocked: false,
            preview_active: false,
            sequence: 0,
            retry_at: None,
            told: false,
            outgoing: Vec::new(),
            sequence_base: 0,
            server_muted: BTreeSet::new(),
            heard: Vec::new(),
            activity_in: Vec::new(),
            speaking: Vec::new(),
            muted: BTreeSet::new(),
            remote_activity: BTreeMap::new(),
            sent_activity: None,
            activity_sent_at: None,
            shown: None,
        }
    }
    /// The player saved new preferences.
    pub fn apply_settings(&mut self, settings: &Settings) {
        let config = config_from(&settings.prefs);
        // A different microphone, or a microphone that must now be opened or
        // closed (Disabled opens none), means opening the devices again.
        let reopen = config.device != self.config.device
            || (config.settings.activation == Activation::Disabled)
                != (self.config.settings.activation == Activation::Disabled);
        self.config = config;
        if let Some(chat) = &self.chat {
            chat.apply_settings(self.audio_settings());
        }
        if reopen || !self.config.settings.enabled {
            // Closing the devices is the only honest "off".
            self.chat = None;
            self.speaking.clear();
            self.retry_at = None;
            self.told = false;
        }
    }
    fn audio_settings(&self) -> VoiceSettings {
        let mut settings = self.config.settings;
        settings.volume *= if !self.focused && self.config.mute_in_background {
            0.0
        } else {
            self.config.master_volume
        };
        settings
    }
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.set_talking(false);
        }
        if let Some(chat) = &self.chat {
            chat.apply_settings(self.audio_settings());
            chat.set_transmit_allowed(focused && self.active && !self.server_blocked);
        }
    }
    pub fn set_master_volume(&mut self, volume: f32) {
        self.config.master_volume = volume;
        if let Some(chat) = &self.chat {
            chat.apply_settings(self.audio_settings());
        }
    }
    /// The volume slider is being dragged: other players' voices at `volume`
    /// (0 to 1) until the preferences are saved again.
    pub fn set_volume(&mut self, volume: f32) {
        self.config.settings.volume = volume;
        if let Some(chat) = &self.chat {
            chat.apply_settings(self.audio_settings());
        }
    }
    /// The Push To Talk key went down or up.
    pub fn set_talking(&mut self, talking: bool) {
        self.talking = talking;
        if let Some(chat) = &self.chat {
            chat.set_talking(talking);
        }
    }
    /// Whether the player's voice is going out right now.
    pub fn transmitting(&self) -> bool {
        self.chat.as_ref().is_some_and(VoiceChat::transmitting)
    }
    /// The players heard in the last moment.
    pub fn speaking(&self) -> &[OwnerId] {
        &self.speaking
    }
    /// What the HUD's voice indicator shows now; None outside a session or
    /// with voice chat off.
    pub fn indicator(&self) -> Option<VoiceIndicator> {
        if !self.active {
            return None;
        }
        Some(VoiceIndicator {
            mode: self.config.mode,
            key_held: self.talking,
            active: self.chat.as_ref().is_some_and(VoiceChat::activity),
            microphone: self.chat.as_ref().is_some_and(VoiceChat::has_input),
            server_muted: self.server_blocked,
        })
    }
    /// The indicator, when it differs from what the UI was last given.
    pub fn indicator_change(&mut self) -> Option<Option<VoiceIndicator>> {
        let now = self.indicator();
        (now != self.shown).then(|| {
            self.shown = now;
            now
        })
    }
    /// Keep capture alive in Options so the meter can reuse its live input
    /// stream, without creating a second microphone capture.
    pub fn set_preview_active(&mut self, active: bool) {
        self.preview_active = active;
    }
    /// Current input level and status for the settings meter.
    pub fn preview(&self) -> VoicePreview {
        let allowed = self.preview_active
            && self.config.settings.enabled
            && self.config.mode != VoiceMode::Disabled;
        let available = self.chat.as_ref().is_some_and(VoiceChat::has_input);
        let level = self
            .chat
            .as_ref()
            .filter(|chat| allowed && chat.has_input())
            .map_or(0.0, VoiceChat::level);
        VoicePreview {
            level: if level.is_finite() {
                level.clamped(0.0, 1.0)
            } else {
                0.0
            },
            available,
            enabled: allowed,
        }
    }
    /// The local voice detector and explicitly relayed remote speech signals.
    /// Audio-frame arrival alone is not enough: a sender can transmit silence.
    /// Locally muted players are left out: they are silent to this player,
    /// so their names stay unbolded and their poses keep talking to no one.
    pub fn speaking_owners(&self, local_owner: OwnerId) -> BTreeSet<OwnerId> {
        let mut owners: BTreeSet<_> = self
            .remote_activity
            .keys()
            .copied()
            .filter(|owner| !self.muted.contains(owner) && !self.server_muted.contains(owner))
            .collect();
        if self.local_speaking() && !self.muted.contains(&local_owner) {
            owners.insert(local_owner);
        }
        owners
    }
    /// How loudly each speaker is being heard right now, 0..=1: the name
    /// tag's blue shade follows it. Locally muted players are absent.
    fn apply_routes(&mut self, routes: bri_net::voice::RouteTable) {
        if self.routes != routes {
            for (owner, old) in &self.routes.routes {
                if routes.routes.get(owner) != Some(old) {
                    if let Some(chat) = &self.chat {
                        chat.forget(*owner);
                    }
                    self.remote_activity.remove(owner);
                    self.hud_activity.remove(owner);
                }
            }
            self.routes = routes;
        }
    }
    pub fn speaking_labels(&self, local_owner: OwnerId) -> BTreeMap<u64, (String, f32)> {
        let now = Instant::now();
        let mut labels: BTreeMap<_, _> = self
            .hud_activity
            .iter()
            .filter_map(|(id, at)| {
                if self.muted.contains(id) || self.server_muted.contains(id) {
                    return None;
                }
                let route = self.routes.routes.get(id)?;
                let age = now.saturating_duration_since(*at).as_secs_f32();
                (age < 0.4).then(|| {
                    (
                        *id,
                        (route.label.clone(), (1.0 - age / 0.4).clamped(0.0, 1.0)),
                    )
                })
            })
            .collect();
        if self.local_speaking() {
            labels.insert(local_owner, (String::new(), 1.0));
        }
        labels
    }
    pub fn speaking_levels(&self, local_owner: OwnerId) -> BTreeMap<OwnerId, f32> {
        let mut levels = self
            .chat
            .as_ref()
            .map_or_else(BTreeMap::new, |chat| chat.speaker_levels());
        levels.retain(|owner, _| !self.muted.contains(owner) && !self.server_muted.contains(owner));
        if self.local_speaking() && !self.muted.contains(&local_owner) {
            let level = self.chat.as_ref().map_or(0.0, VoiceChat::level);
            if level > 0.0 {
                levels.insert(local_owner, level.clamped(0.0, 1.0));
            }
        }
        levels
    }
    /// Mute a player's voice for this player only (the F2 list); the host is
    /// not told.
    pub fn mute(&mut self, owner: OwnerId) {
        self.muted.insert(owner);
        if let Some(chat) = &self.chat {
            chat.forget(owner);
        }
    }
    /// Apply host moderation immediately, including speech already buffered.
    pub fn set_server_muted(&mut self, muted: &BTreeSet<OwnerId>, local_owner: OwnerId) {
        for owner in muted.difference(&self.server_muted) {
            self.remote_activity.remove(owner);
            if let Some(chat) = &self.chat {
                chat.forget(*owner);
            }
        }
        self.server_muted.clone_from(muted);
        self.server_blocked = muted.contains(&local_owner);
        if self.server_blocked {
            self.set_talking(false);
        }
    }
    /// Take that local mute off again.
    pub fn unmute(&mut self, owner: OwnerId) {
        self.muted.remove(&owner);
    }
    /// Whether a player's voice is muted locally right now.
    pub fn is_muted(&self, owner: OwnerId) -> bool {
        self.muted.contains(&owner)
    }
    /// The players this player has muted locally (the F2 list).
    pub fn muted(&self) -> &BTreeSet<OwnerId> {
        &self.muted
    }
    fn local_speaking(&self) -> bool {
        if !self.active || !self.focused || self.server_blocked {
            return false;
        }
        let Some(chat) = &self.chat else { return false };
        if !chat.has_input() || !chat.activity() {
            return false;
        }
        match self.config.mode {
            VoiceMode::PushToTalk => self.talking,
            VoiceMode::VoiceActivity => true,
            VoiceMode::Disabled => false,
        }
    }
    /// One frame. `link` is the joined session's voice connection; when
    /// Options is open, the same capture stream remains available for the
    /// microphone meter even without a joined session.
    pub fn update(&mut self, transport: Option<&VoiceLink>, listener: Listener, now: Instant) {
        // Keep the raw session handle briefly so a settings change that turns
        // voice off can publish a final false state before we stop processing.
        let session_link = transport.map(VoiceLink::id);
        if self.session_link != session_link {
            self.session_link = session_link;
            self.chat = None;
            self.opening = None;
            self.muted.clear();
            self.remote_activity.clear();
            self.hud_activity.clear();
            self.speaking.clear();
            self.talking = false;
            self.sent_activity = None;
            self.activity_sent_at = None;
            self.retry_at = None;
        }
        let link = transport.filter(|_| self.config.settings.enabled);
        self.active = link.is_some();
        let keep_capture = self.active
            || (self.preview_active
                && self.focused
                && self.config.settings.enabled
                && self.config.mode != VoiceMode::Disabled);
        if !keep_capture {
            if self.sent_activity == Some(true)
                && let Some(transport) = transport
            {
                let _ = transport.send_activity(false);
            }
            self.chat = None;
            self.opening = None;
            self.speaking.clear();
            self.remote_activity.clear();
            self.hud_activity.clear();
            if transport.is_none() {
                self.muted.clear();
                self.server_muted.clear();
            }
            self.active = false;
            self.retry_at = None;
            self.told = false;
            self.talking = false;
            self.sent_activity = None;
            self.activity_sent_at = None;
            return;
        }

        if self.chat.as_ref().is_some_and(|chat| {
            (!chat.has_output() || (!chat.has_input() && self.config.mode != VoiceMode::Disabled))
                && self.retry_at.is_some_and(|at| now >= at)
        }) {
            self.chat = None;
        }
        if self.chat.as_ref().is_some_and(VoiceChat::is_broken) {
            bri_console::warn("Voice chat lost its audio device; reopening shortly");
            self.chat = None;
            self.retry_at = Some(now + RETRY / 2);
        }
        if self
            .opening
            .as_ref()
            .is_some_and(|(_, task)| task.is_finished())
        {
            let (requested, task) = self.opening.take().expect("finished opener");
            let result = task.join().unwrap_or_else(|_| {
                Err(bri_audio::AudioError::Device(
                    "voice device worker stopped".into(),
                ))
            });
            let matches = requested.device == self.config.device
                && (requested.mode == VoiceMode::Disabled)
                    == (self.config.mode == VoiceMode::Disabled);
            if matches {
                match result {
                    Ok(chat) => {
                        chat.apply_settings(self.audio_settings());
                        chat.set_talking(self.talking);
                        let disabled = self.config.mode == VoiceMode::Disabled;
                        if !chat.has_input() && !disabled && !self.told {
                            self.told = true;
                            bri_console::warn(
                                "Voice chat found no microphone; listening only and retrying",
                            );
                        }
                        if !chat.has_output() && !self.told {
                            self.told = true;
                            bri_console::warn(
                                "Voice playback device unavailable; microphone can send, but listening is unavailable. Retrying.",
                            );
                        }
                        self.sequence_base = self.sequence.wrapping_add(16);
                        let partial = !chat.has_output() || (!chat.has_input() && !disabled);
                        self.chat = Some(chat);
                        self.retry_at = partial.then_some(now + RETRY);
                    }
                    Err(error) => {
                        if !self.told {
                            self.told = true;
                            bri_console::warn(format!("Voice chat is off: {error}"));
                        }
                        self.retry_at = Some(now + RETRY);
                    }
                }
            }
        }
        if self.chat.is_none() && self.opening.is_none() && self.retry_at.is_none_or(|at| now >= at)
        {
            let settings = self.audio_settings();
            let device = self.config.device.clone();
            match std::thread::Builder::new()
                .name("bri-voice-open".into())
                .spawn(move || VoiceChat::open(settings, device))
            {
                Ok(task) => self.opening = Some((self.config.clone(), task)),
                Err(error) => {
                    bri_console::warn(format!("Cannot start voice device worker: {error}"));
                    self.retry_at = Some(now + RETRY);
                }
            }
        }

        let routes = link.map_or_else(bri_net::voice::RouteTable::default, |l| l.routes());
        self.apply_routes(routes);
        if let Some(chat) = &self.chat {
            chat.set_transmit_allowed(self.active && self.focused && !self.server_blocked);
        }
        // State packets are independent of the audio playback device and
        // carry no audio analysis beyond a boolean, periodically refreshed.
        if let Some(link) = link {
            self.activity_in.clear();
            link.take_activity(&mut self.activity_in);
            for activity in self.activity_in.drain(..) {
                if self
                    .routes
                    .routes
                    .get(&activity.speaker)
                    .is_none_or(|r| r.generation != activity.revision)
                {
                    continue;
                }
                if activity.active {
                    self.remote_activity.insert(activity.speaker, now);
                } else {
                    self.remote_activity.remove(&activity.speaker);
                }
            }
            self.remote_activity
                .retain(|_, at| now.saturating_duration_since(*at) <= REMOTE_ACTIVITY_TIMEOUT);

            for id in self.remote_activity.keys() {
                self.hud_activity.insert(*id, now);
            }
            self.hud_activity
                .retain(|_, at| now.saturating_duration_since(*at) < Duration::from_millis(400));
            let active = self.local_speaking();
            let refresh = active
                && self
                    .activity_sent_at
                    .is_none_or(|at| now.saturating_duration_since(at) >= ACTIVITY_REFRESH);
            if self.sent_activity != Some(active) || refresh {
                // A failed send is harmless: a later true keepalive recovers
                // the state, and receivers expire it if the connection is gone.
                let _ = link.send_activity(active);
                self.sent_activity = Some(active);
                self.activity_sent_at = Some(now);
            }
        } else {
            self.remote_activity.clear();
            self.sent_activity = None;
            self.activity_sent_at = None;
        }

        let Some(chat) = &mut self.chat else {
            self.speaking.clear();
            return;
        };
        chat.set_listener(listener);
        self.outgoing.clear();
        chat.take_frames(&mut self.outgoing);
        if let Some(link) = link {
            for frame in self
                .outgoing
                .drain(..)
                .filter(|_| self.focused && !self.server_blocked)
            {
                // The connection may be closing; a lost frame is a gap in
                // someone's ears, nothing more.
                let sequence = self.sequence_base.wrapping_add(frame.sequence);
                let _ = link.send(sequence, frame.payload.as_bytes());
                self.sequence = sequence;
            }
            // Everyone else's voice, in.
            self.heard.clear();
            link.take_heard(&mut self.heard);
            for voice in self.heard.drain(..) {
                // A locally muted player is not played back at all.
                if self.muted.contains(&voice.speaker) || self.server_muted.contains(&voice.speaker)
                {
                    continue;
                }
                let Some(route) = self.routes.routes.get(&voice.speaker) else {
                    continue;
                };
                if voice.revision != route.generation {
                    continue;
                }
                chat.hear_routed(
                    voice.speaker,
                    voice.position,
                    voice.sequence,
                    &voice.payload,
                    route.spatial.then_some(route.range),
                );
            }
            self.speaking = chat.speaking();
        } else {
            // Consume/drop menu preview frames; they must never queue for a
            // future network session or grow while the user sits in Options.
            self.outgoing.clear();
            self.speaking.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }
    fn settings(pairs: &[(&str, &str)]) -> Settings {
        Settings {
            prefs: prefs(pairs),
            ..Settings::default()
        }
    }

    #[test]
    fn voice_chat_is_on_with_push_to_talk_by_default() {
        let config = config_from(&BTreeMap::new());
        assert!(config.settings.enabled);
        assert_eq!(config.settings.activation, Activation::PushToTalk);
        assert_eq!(config.mode, VoiceMode::PushToTalk);
        assert_eq!(config.device, None);
        assert_eq!(config.settings.range, bri_net::voice::RANGE);
        assert_eq!(config.settings.mic_gain, 1.0);
        assert_eq!(config.settings.volume, 1.0);
    }

    #[test]
    fn prefs_choose_the_voice_settings_and_bad_values_are_tamed() {
        let config = config_from(&prefs(&[
            ("$pref::Voice::Enabled", "0"),
            ("$pref::Voice::Mode", "1"),
            ("$pref::Voice::InputDevice", "USB Mic"),
            ("$pref::Voice::Threshold", "0.2"),
            ("$pref::Voice::MicVolume", "99"),
            ("$pref::Voice::Volume", "nan"),
            ("$pref::Voice::Range", "20"),
        ]));
        assert!(!config.settings.enabled);
        assert_eq!(config.settings.activation, Activation::Voice);
        assert_eq!(config.mode, VoiceMode::VoiceActivity);
        assert_eq!(config.device.as_deref(), Some("USB Mic"));
        assert_eq!(config.settings.threshold, 0.2);
        assert_eq!(config.settings.mic_gain, 4.0);
        assert_eq!(config.settings.volume, 1.0);
        assert_eq!(
            config.settings.range,
            bri_net::voice::RANGE,
            "local prefs cannot override server-selected route distance"
        );
        let disabled = config_from(&prefs(&[("$pref::Voice::Mode", "2")]));
        assert_eq!(disabled.settings.activation, Activation::Disabled);
        assert!(
            disabled.settings.enabled,
            "Disabled does not turn voice chat off"
        );
    }

    #[test]
    fn without_a_session_nothing_is_open_and_the_key_is_forgotten() {
        let mut voice = VoiceClient::new(&Settings::default());
        voice.set_talking(true);
        voice.update(None, Listener::default(), Instant::now());
        assert!(voice.chat.is_none());
        assert!(!voice.talking, "a key held across a session does not stick");
        assert!(!voice.transmitting());
        assert!(voice.speaking().is_empty());
        assert_eq!(voice.indicator(), None);
        assert_eq!(voice.indicator_change(), None, "nothing shown, nothing new");
    }

    #[test]
    fn the_indicator_follows_the_key_the_mode_and_the_microphone() {
        let mut voice = VoiceClient::new(&settings(&[("$pref::Voice::Mode", "0")]));
        voice.active = true;
        assert_eq!(
            voice.indicator(),
            Some(VoiceIndicator {
                mode: VoiceMode::PushToTalk,
                key_held: false,
                active: false,
                microphone: false,
                server_muted: false,
            })
        );
        voice.set_talking(true);
        assert!(voice.indicator().unwrap().key_held);
        // A change is reported once.
        assert!(voice.indicator_change().is_some());
        assert_eq!(voice.indicator_change(), None);
        voice.set_talking(false);
        assert_eq!(
            voice.indicator_change().map(|i| i.unwrap().key_held),
            Some(false)
        );
        // Leaving the session hides it.
        voice.active = false;
        assert_eq!(voice.indicator_change(), Some(None));
    }

    #[test]
    fn world_speaking_state_uses_vad_packets_not_received_audio_frames() {
        let mut voice = VoiceClient::new(&Settings::default());
        // Audio may still be buffered or carry a quiet frame after speech ends;
        // it is not authoritative evidence that the player is talking.
        voice.speaking.push(99);
        assert!(voice.speaking_owners(7).is_empty());
        voice.remote_activity.insert(99, Instant::now());
        assert_eq!(voice.speaking_owners(7), BTreeSet::from([99]));
    }

    #[test]
    fn a_locally_muted_player_is_silent_and_unbolded_for_this_player() {
        let mut voice = VoiceClient::new(&Settings::default());
        voice.remote_activity.insert(99, Instant::now());
        assert_eq!(voice.speaking_owners(7), BTreeSet::from([99]));
        voice.mute(99);
        assert!(voice.is_muted(99));
        assert!(
            voice.speaking_owners(7).is_empty(),
            "a muted player shows no speech state"
        );
        assert!(
            !voice.speaking_levels(7).contains_key(&99),
            "their loudness is absent, so the name tag keeps its base colour"
        );
        voice.unmute(99);
        assert_eq!(voice.speaking_owners(7), BTreeSet::from([99]));
    }

    #[test]
    fn a_new_microphone_or_disabling_reopens_the_devices_but_volume_does_not() {
        let mut voice = VoiceClient::new(&Settings::default());
        voice.retry_at = Some(Instant::now());
        voice.apply_settings(&settings(&[("$pref::Voice::Volume", "0.3")]));
        assert_eq!(
            voice.retry_at.map(|_| ()),
            Some(()),
            "volume alone keeps state"
        );
        voice.apply_settings(&settings(&[("$pref::Voice::InputDevice", "USB Mic")]));
        assert_eq!(voice.retry_at, None, "another microphone reopens");
        voice.retry_at = Some(Instant::now());
        voice.apply_settings(&settings(&[
            ("$pref::Voice::InputDevice", "USB Mic"),
            ("$pref::Voice::Mode", "2"),
        ]));
        assert_eq!(voice.retry_at, None, "Disabling closes the microphone");
        // The slider's preview changes the volume at once.
        voice.set_volume(0.0);
        assert_eq!(voice.config.settings.volume, 0.0);
    }
    #[test]
    fn voice_route_revocation_clears_activity_and_keeps_unchanged_speakers() {
        let mut voice = VoiceClient::new(&Settings::default());
        let route = |generation| bri_net::voice::RouteState {
            generation,
            playback: bri_sim::session::VoicePlayback {
                spatial: false,
                range: 48.0,
                label: "Radio 1".into(),
            },
        };
        voice.routes = bri_net::voice::RouteTable {
            revision: 1,
            routes: BTreeMap::from([(8, route(1)), (9, route(2))]),
        };
        voice.remote_activity = BTreeMap::from([(8, Instant::now()), (9, Instant::now())]);
        voice.hud_activity = voice.remote_activity.clone();
        voice.apply_routes(bri_net::voice::RouteTable {
            revision: 2,
            routes: BTreeMap::from([(9, route(2))]),
        });
        assert!(!voice.remote_activity.contains_key(&8));
        assert!(!voice.hud_activity.contains_key(&8));
        assert!(voice.remote_activity.contains_key(&9));
        voice.apply_routes(bri_net::voice::RouteTable {
            revision: 3,
            routes: BTreeMap::from([(9, route(3))]),
        });
        assert!(
            voice.remote_activity.is_empty(),
            "same speaker on a new channel/map generation cannot retain old speech activity"
        );
    }
}
