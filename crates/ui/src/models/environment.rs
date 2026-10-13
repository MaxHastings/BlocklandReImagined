//! The Admin Menu's Environment window (v21's `EnvironmentGui`, built
//! natively): what the host set over the map's own sun, sky and fog, and
//! the admin's draft of a change. Nothing here has authority: the host
//! checks the rank and validates every value when the draft is applied.
//!
//! Its controls own separate settings, so none silently undoes another:
//! - The sky, ground and water. A picked sky's light and fog are the base
//!   the rest starts from, as the map's own are.
//! - The time of day ([`DayCycle`], held or passing). A sky never changes
//!   it, and it works with or without the day/night cycle.
//! - The weather: cloud cover and how far one sees.
//! - Advanced values set by hand, which outlast a new sky.
//!
//! The Simple tab's Looks are made of these: a time of day, a weather, or
//! both.
use crate::api::IconRef;
use bri_console::Clamp;
use bri_content::atmosphere::{self, Authored, DayCycle, Settings, SunFlare, Vignette};
use serde::{Deserialize, Serialize};

/// What the client knows of the environment: the map's own values, the
/// host's settings over them and the server tick (for the time of day).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentView {
    pub authored: Authored,
    pub settings: Settings,
    pub tick: u64,
}

/// A colour the picker edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorField {
    DirectLight,
    AmbientLight,
    ShadowColor,
    SunFlare,
    FogColor,
    SkyColor,
    Vignette,
    GroundColor,
    WaterColor,
    UnderwaterColor,
}
impl ColorField {
    pub const ALL: [Self; 10] = [
        Self::DirectLight,
        Self::AmbientLight,
        Self::ShadowColor,
        Self::SunFlare,
        Self::FogColor,
        Self::SkyColor,
        Self::Vignette,
        Self::GroundColor,
        Self::WaterColor,
        Self::UnderwaterColor,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::DirectLight => "Direct Light",
            Self::AmbientLight => "Ambient Light",
            Self::ShadowColor => "Shadow Color",
            Self::SunFlare => "Sun Flare Color",
            Self::FogColor => "Fog Color",
            Self::SkyColor => "Sky Color",
            Self::Vignette => "Vignette Color",
            Self::GroundColor => "Ground Color",
            Self::WaterColor => "Water Color",
            Self::UnderwaterColor => "Underwater Color",
        }
    }
    /// The picker shows an alpha slider (a strength, or how much shows
    /// through).
    pub fn has_alpha(self) -> bool {
        matches!(
            self,
            Self::SunFlare
                | Self::Vignette
                | Self::GroundColor
                | Self::WaterColor
                | Self::UnderwaterColor
        )
    }
    /// The picker this colour belongs to: its row is used only while one
    /// of that picker's is picked.
    pub fn scenery(self) -> Option<ChoiceKind> {
        match self {
            Self::GroundColor => Some(ChoiceKind::Ground),
            Self::WaterColor | Self::UnderwaterColor => Some(ChoiceKind::Water),
            _ => None,
        }
    }
    /// The setting it is (as in [`atmosphere::KEYS`]).
    pub fn key(self) -> &'static str {
        match self {
            Self::DirectLight => "direct_light",
            Self::AmbientLight => "ambient_light",
            Self::ShadowColor => "shadow_color",
            Self::SunFlare => "sun_flare",
            Self::FogColor => "fog_color",
            Self::SkyColor => "sky_color",
            Self::Vignette => "vignette",
            Self::GroundColor => "ground_color",
            Self::WaterColor => "water_color",
            Self::UnderwaterColor => "underwater_color",
        }
    }
}

/// A number a slider edits: its range and step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberField {
    DayLength,
    TimeOfDay,
    SunAzimuth,
    SunElevation,
    FlareSize,
    Overcast,
    VisibleDistance,
    FogDistance,
    WaterHeight,
}
impl NumberField {
    pub const ALL: [Self; 9] = [
        Self::DayLength,
        Self::TimeOfDay,
        Self::SunAzimuth,
        Self::SunElevation,
        Self::FlareSize,
        Self::Overcast,
        Self::VisibleDistance,
        Self::FogDistance,
        Self::WaterHeight,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::DayLength => "Day Length",
            Self::TimeOfDay => "Time of Day",
            Self::SunAzimuth => "Sun Azimuth",
            Self::SunElevation => "Sun Elevation",
            Self::FlareSize => "Sun Flare Size",
            Self::Overcast => "Cloud Cover",
            Self::VisibleDistance => "Visible Distance",
            Self::FogDistance => "Fog Distance",
            Self::WaterHeight => "Water Height",
        }
    }
    /// The setting it is part of (as in [`atmosphere::KEYS`]).
    pub fn key(self) -> &'static str {
        match self {
            Self::DayLength | Self::TimeOfDay => "day_cycle",
            Self::SunAzimuth => "sun_azimuth",
            Self::SunElevation => "sun_elevation",
            Self::FlareSize => "sun_flare",
            Self::Overcast => "overcast",
            Self::VisibleDistance => "visible_distance",
            Self::FogDistance => "fog_distance",
            Self::WaterHeight => "water_height",
        }
    }
    /// Slider range. The day length slider stops at an hour; Add-Ons may
    /// set up to a whole real day.
    pub fn range(self) -> (f32, f32) {
        match self {
            Self::DayLength => (*atmosphere::DAY_LENGTH.start(), 3600.0),
            Self::TimeOfDay => (0.0, 24.0),
            Self::SunAzimuth => (0.0, 360.0),
            Self::SunElevation => (-90.0, 90.0),
            Self::FlareSize => (
                *atmosphere::FLARE_SIZE.start(),
                *atmosphere::FLARE_SIZE.end(),
            ),
            Self::Overcast => (0.0, 1.0),
            Self::VisibleDistance => (atmosphere::MIN_VISIBLE_DISTANCE, atmosphere::MAX_DISTANCE),
            Self::FogDistance => (0.0, atmosphere::MAX_DISTANCE),
            Self::WaterHeight => (0.0, atmosphere::MAX_WATER_HEIGHT),
        }
    }
    /// Values snap to this.
    pub fn step(self) -> f32 {
        match self {
            Self::DayLength => 10.0,
            Self::TimeOfDay => 0.25,
            Self::SunAzimuth | Self::SunElevation | Self::VisibleDistance | Self::FogDistance => {
                1.0
            }
            Self::FlareSize | Self::Overcast => 0.05,
            Self::WaterHeight => 0.5,
        }
    }
    /// The value as the row shows it.
    pub fn format(self, v: f32) -> String {
        match self {
            Self::DayLength => {
                let s = v.round() as u32;
                if s >= 60 {
                    format!("{}m {:02}s", s / 60, s % 60)
                } else {
                    format!("{s}s")
                }
            }
            Self::TimeOfDay => clock(v),
            Self::SunAzimuth | Self::SunElevation => format!("{v:.0}°"),
            Self::FlareSize => format!("{v:.2}x"),
            Self::Overcast => format!("{:.0}%", v * 100.0),
            Self::VisibleDistance | Self::FogDistance => format!("{v:.0}"),
            Self::WaterHeight => format!("{v:.1}"),
        }
    }
    pub fn snap(self, v: f32) -> f32 {
        let (lo, hi) = self.range();
        ((v / self.step()).round() * self.step()).clamped(lo, hi)
    }
}
/// Hours (0..24) as a 24-hour clock.
pub fn clock(hours: f32) -> String {
    let minutes = (hours * 60.0).round() as u32 % (24 * 60);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// A Simple tab Look: a time of day (hours), a weather (an index in
/// [`WEATHERS`]), or both. Sunrise is at 6:00 and sunset at 18:00 on every
/// map; the times sit where the light is warmest or darkest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub name: &'static str,
    pub hours: Option<f32>,
    pub weather: Option<usize>,
}
/// The Looks after Default (the sky's or map's own).
pub const LOOKS: [Look; 6] = [
    Look {
        name: "Clear Day",
        hours: Some(12.0),
        weather: Some(0),
    },
    Look {
        name: "Golden Hour",
        hours: Some(16.75),
        weather: None,
    },
    Look {
        name: "Sunset",
        hours: Some(17.75),
        weather: None,
    },
    Look {
        name: "Night",
        hours: Some(0.0),
        weather: None,
    },
    Look {
        name: "Overcast",
        hours: None,
        weather: Some(2),
    },
    Look {
        name: "Thick Fog",
        hours: None,
        weather: Some(4),
    },
];

/// A weather: cloud cover, visible distance and where fog starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weather {
    pub name: &'static str,
    pub overcast: Option<f32>,
    pub visible_distance: Option<f32>,
    pub fog_distance: Option<f32>,
}
/// The settings a weather owns.
const WEATHER_KEYS: [&str; 3] = ["overcast", "visible_distance", "fog_distance"];
/// The weathers a Look may set.
pub const WEATHERS: [Weather; 5] = [
    Weather {
        name: "Clear",
        overcast: None,
        visible_distance: Some(1000.0),
        fog_distance: Some(600.0),
    },
    Weather {
        name: "Haze",
        overcast: Some(0.15),
        visible_distance: Some(450.0),
        fog_distance: Some(60.0),
    },
    Weather {
        name: "Overcast",
        overcast: Some(0.65),
        visible_distance: Some(700.0),
        fog_distance: Some(150.0),
    },
    Weather {
        name: "Fog",
        overcast: Some(0.3),
        visible_distance: Some(200.0),
        fog_distance: Some(15.0),
    },
    Weather {
        name: "Thick Fog",
        overcast: Some(0.45),
        visible_distance: Some(80.0),
        fog_distance: Some(0.0),
    },
];

/// Which picture picker is open. A sky fits every map; a ground or a water
/// only a flat one (`Choices::flat`), as on Steam, where every map is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChoiceKind {
    Sky,
    Ground,
    Water,
}
impl ChoiceKind {
    pub const ALL: [Self; 3] = [Self::Sky, Self::Ground, Self::Water];
    pub fn label(self) -> &'static str {
        match self {
            Self::Sky => "Sky",
            Self::Ground => "Ground",
            Self::Water => "Water",
        }
    }
}
/// A sky, ground or water to pick: its name in the settings, what the
/// picker calls it, and its picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub icon: IconRef,
    /// What picking it brings besides its name: a Steam sky's light and
    /// fog, a ground's colour, a water's height and colours.
    pub look: Settings,
}
/// What the pickers offer on the map being played. The client fills it
/// from its own content when a map loads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Choices {
    pub skies: Vec<Choice>,
    pub grounds: Vec<Choice>,
    /// The waters, "None" (`atmosphere::NO_WATER`) first.
    pub waters: Vec<Choice>,
    /// The sky the map draws on its own, when it is one of `skies`.
    pub map_sky: Option<String>,
    /// The map stands on a flat plate, which a ground and a water may cover.
    pub flat: bool,
}
impl Choices {
    pub fn list(&self, kind: ChoiceKind) -> &[Choice] {
        match kind {
            ChoiceKind::Sky => &self.skies,
            ChoiceKind::Ground => &self.grounds,
            ChoiceKind::Water => &self.waters,
        }
    }
    /// Whether this picker may be used on this map.
    pub fn offered(&self, kind: ChoiceKind) -> bool {
        !self.list(kind).is_empty() && (kind == ChoiceKind::Sky || self.flat)
    }
    fn find(&self, kind: ChoiceKind, id: &str) -> Option<&Choice> {
        self.list(kind).iter().find(|c| c.id == id)
    }
}

/// The settings a picked sky's look may bring: its light and fog. Its day
/// cycle and any scenery are not (a sky never changes the time).
const SKY_KEYS: [&str; 13] = [
    "sun_azimuth",
    "sun_elevation",
    "direct_light",
    "ambient_light",
    "shadow_color",
    "sun_flare",
    "visible_distance",
    "fog_distance",
    "fog_color",
    "overcast",
    "sky_color",
    "vignette",
    "day_cycle",
];

#[derive(Debug, Clone, Default)]
pub struct EnvironmentModel {
    /// The host's environment, once in a game.
    pub view: Option<EnvironmentView>,
    /// The Environment window's unsaved change, while it is open.
    pub draft: Option<Settings>,
    /// The colour the picker is editing.
    pub picking: Option<ColorField>,
    /// The picture picker open, if any.
    pub choosing: Option<ChoiceKind>,
    /// The skies this map offers.
    pub choices: Choices,
    /// Counts changes, so open windows know to show them.
    pub revision: u64,
}
impl EnvironmentModel {
    /// Start editing from what the host has now.
    pub fn begin(&mut self) {
        self.revision += 1;
        self.draft = Some(
            self.view
                .as_ref()
                .map(|v| v.settings.clone())
                .unwrap_or_default(),
        );
        self.picking = None;
        self.choosing = None;
    }
    /// Put the rows back as they were (a colour picker cancelled).
    pub fn restore_draft(&mut self, draft: Option<Settings>) {
        self.revision += 1;
        self.draft = draft;
    }
    pub fn end(&mut self) {
        self.revision += 1;
        self.draft = None;
        self.picking = None;
        self.choosing = None;
    }
    /// The map's skies (a new map, or its pictures ready).
    pub fn set_choices(&mut self, choices: Choices) {
        if self.choices != choices {
            self.revision += 1;
            self.choices = choices;
        }
    }
    /// The sky, ground or water the rows pick; None keeps the map's own.
    pub fn choice(&self, kind: ChoiceKind) -> Option<String> {
        let s = self.settings();
        match kind {
            ChoiceKind::Sky => s.sky,
            ChoiceKind::Ground => s.ground,
            ChoiceKind::Water => s.water,
        }
    }
    /// What a picked sky brings: its light and fog, the base the other
    /// values start from (empty for the map's own or a v20 sky).
    pub fn base(&self) -> Settings {
        self.sky_look(self.choice(ChoiceKind::Sky).as_deref())
    }
    fn sky_look(&self, sky: Option<&str>) -> Settings {
        let mut base = Settings::default();
        if let Some(look) = sky.and_then(|id| self.choices.find(ChoiceKind::Sky, id)) {
            for key in SKY_KEYS {
                base.take(&look.look, key);
            }
        }
        base.day_cycle = None;
        base
    }
    /// Pick one. A sky brings its light and fog in place of the old sky's,
    /// keeping whatever was set by hand, the time of day and a weather a
    /// Look set (Steam's window let a sky's look replace them all). A
    /// ground or water starts from its own colours and height.
    pub fn set_choice(&mut self, kind: ChoiceKind, id: Option<String>) {
        let old_base = self.base();
        let new_base = self.sky_look(id.as_deref());
        self.revision += 1;
        let s = self.draft.get_or_insert_with(Settings::default);
        match kind {
            ChoiceKind::Sky => {
                for key in SKY_KEYS {
                    if key != "day_cycle" && s.same(&old_base, key) {
                        s.take(&new_base, key);
                    }
                }
                s.sky = id;
            }
            ChoiceKind::Ground => {
                s.ground = id;
                s.ground_color = None;
            }
            ChoiceKind::Water => {
                s.water = id;
                s.water_height = None;
                s.water_color = None;
                s.underwater_color = None;
            }
        }
    }
    /// The picture a picker's button shows: the picked one's, else the
    /// map's own sky's.
    pub fn choice_icon(&self, kind: ChoiceKind) -> IconRef {
        let id = self.choice(kind).or(match kind {
            ChoiceKind::Sky => self.choices.map_sky.clone(),
            _ => None,
        });
        id.and_then(|id| self.choices.find(kind, &id).map(|c| c.icon.clone()))
            .unwrap_or_default()
    }
    /// What a picker's button says: the picked one's name, or that the
    /// map's own is drawn.
    pub fn choice_label(&self, kind: ChoiceKind) -> String {
        match self.choice(kind) {
            None => "Map's own".into(),
            Some(id) => self.choices.find(kind, &id).map_or(id, |c| c.label.clone()),
        }
    }
    pub fn apply(&mut self, view: EnvironmentView) {
        self.revision += 1;
        // The host stamps a cycle it is given with its own tick. Once it
        // runs the draft's cycle, the draft takes that stamp: the window
        // then shows the change as applied, and a later Apply keeps the
        // cycle turning rather than restarting it.
        if let Some(d) = self.draft.as_mut().and_then(|d| d.day_cycle.as_mut())
            && let Some(host) = view.settings.day_cycle
            && (d.length_seconds, d.time, d.paused) == (host.length_seconds, host.time, host.paused)
        {
            d.anchor_tick = host.anchor_tick;
        }
        self.view = Some(view);
    }
    fn authored(&self) -> Authored {
        self.view.as_ref().map_or(
            Authored {
                sun_direction: atmosphere::light_direction(0.0, 45.0),
                direct_light: [0.7; 3],
                ambient_light: [0.35; 3],
                fog_start: 0.0,
                fog_end: 0.0,
                fog_color: [0.5; 3],
            },
            |v| v.authored,
        )
    }
    fn tick(&self) -> u64 {
        self.view.as_ref().map_or(0, |v| v.tick)
    }
    /// The draft (or the host's settings): what each row shows.
    pub fn settings(&self) -> Settings {
        self.draft
            .clone()
            .or_else(|| self.view.as_ref().map(|v| v.settings.clone()))
            .unwrap_or_default()
    }

    // Time.

    /// The time of day the sun is at, hours; None while it is the map's
    /// own.
    pub fn time_of_day(&self) -> Option<f32> {
        self.settings()
            .day_cycle
            .map(|d| d.time_at(self.tick() as f64) as f32 * 24.0)
    }
    /// Put the sun at a time of day; time keeps passing if it was.
    pub fn set_time(&mut self, hours: f32) {
        if !hours.is_finite() {
            return;
        }
        let tick = self.tick();
        let time = (NumberField::TimeOfDay.snap(hours) / 24.0).rem_euclid(1.0);
        self.revision += 1;
        let s = self.draft.get_or_insert_with(Settings::default);
        let (length_seconds, paused) = s
            .day_cycle
            .map_or((atmosphere::DEFAULT_DAY_LENGTH, true), |d| {
                (d.length_seconds, d.paused)
            });
        s.day_cycle = Some(DayCycle {
            length_seconds,
            time,
            anchor_tick: tick,
            paused,
        });
    }
    pub fn time_passes(&self) -> bool {
        self.settings().day_cycle.is_some_and(|d| !d.paused)
    }
    /// Let time pass from where the sun is (noon, from the map's own), or
    /// hold it there.
    pub fn set_time_passes(&mut self, on: bool) {
        let tick = self.tick();
        let now = self.time_of_day().map_or(0.5, |h| h / 24.0);
        self.revision += 1;
        let s = self.draft.get_or_insert_with(Settings::default);
        let length_seconds = s
            .day_cycle
            .map_or(atmosphere::DEFAULT_DAY_LENGTH, |d| d.length_seconds);
        s.day_cycle = Some(DayCycle {
            length_seconds,
            time: now.rem_euclid(1.0),
            anchor_tick: tick,
            paused: !on,
        });
    }
    /// The day/night cycle's check box: time passes.
    pub fn day_cycle(&self) -> bool {
        self.time_passes()
    }
    pub fn set_day_cycle(&mut self, on: bool) {
        self.set_time_passes(on);
    }

    // Weather.

    /// The weather the settings are: 0 the sky's own, else 1 + its index
    /// in [`WEATHERS`]; None for distances or cloud set by hand.
    fn current_weather(&self) -> Option<usize> {
        let s = self.settings();
        let base = self.base();
        if WEATHER_KEYS.iter().all(|k| s.same(&base, k)) {
            return Some(0);
        }
        WEATHERS
            .iter()
            .position(|w| {
                (s.overcast, s.visible_distance, s.fog_distance)
                    == (w.overcast, w.visible_distance, w.fog_distance)
            })
            .map(|i| i + 1)
    }
    /// 0 the sky's own, else 1 + an index in [`WEATHERS`].
    fn set_weather(&mut self, index: usize) {
        let base = self.base();
        self.revision += 1;
        let s = self.draft.get_or_insert_with(Settings::default);
        match index.checked_sub(1).and_then(|i| WEATHERS.get(i)) {
            None => {
                for key in WEATHER_KEYS {
                    s.take(&base, key);
                }
            }
            Some(w) => {
                s.overcast = w.overcast;
                s.visible_distance = w.visible_distance;
                s.fog_distance = w.fog_distance;
            }
        }
    }

    /// The colour a row shows: the draft's, else the map's own.
    pub fn color(&self, field: ColorField) -> [f32; 4] {
        let s = self.settings();
        let a = self.authored();
        let rgb = |c: [f32; 3]| [c[0], c[1], c[2], 1.0];
        match field {
            ColorField::DirectLight => rgb(s.direct_light.unwrap_or(a.direct_light)),
            ColorField::AmbientLight => rgb(s.ambient_light.unwrap_or(a.ambient_light)),
            ColorField::ShadowColor => rgb(s
                .shadow_color
                .unwrap_or(s.ambient_light.unwrap_or(a.ambient_light))),
            ColorField::SunFlare => s.sun_flare.map_or([1.0, 0.95, 0.8, 0.0], |f| f.color),
            ColorField::FogColor => rgb(s.fog_color.unwrap_or(a.fog_color)),
            ColorField::SkyColor => rgb(s.sky_color.unwrap_or([1.0; 3])),
            ColorField::Vignette => s.vignette.map_or([0.0, 0.0, 0.0, 0.0], |v| v.color),
            ColorField::GroundColor => s
                .ground_color
                .or_else(|| self.picked_look(ChoiceKind::Ground)?.ground_color)
                .unwrap_or([1.0; 4]),
            ColorField::WaterColor => s
                .water_color
                .or_else(|| self.picked_look(ChoiceKind::Water)?.water_color)
                .unwrap_or([1.0; 4]),
            ColorField::UnderwaterColor => s
                .underwater_color
                .or_else(|| self.picked_look(ChoiceKind::Water)?.underwater_color)
                .unwrap_or([0.0; 4]),
        }
    }
    /// What the picked ground or water brings (its own colours and height).
    fn picked_look(&self, kind: ChoiceKind) -> Option<&Settings> {
        let id = self.choice(kind)?;
        self.choices.find(kind, &id).map(|c| &c.look)
    }
    /// Whether a colour or number row is used now: the ground's and the
    /// water's only while one is picked on a flat map.
    pub fn row_active(&self, kind: ChoiceKind) -> bool {
        self.choices.flat
            && self.choice(kind).is_some_and(|id| {
                id != atmosphere::NO_WATER && self.choices.find(kind, &id).is_some()
            })
    }
    pub fn set_color(&mut self, field: ColorField, c: [f32; 4]) {
        self.revision += 1;
        let c = c.map(|v| {
            if v.is_finite() {
                v.clamped(0.0, 1.0)
            } else {
                0.0
            }
        });
        let rgb = [c[0], c[1], c[2]];
        let s = self.draft.get_or_insert_with(Settings::default);
        match field {
            ColorField::DirectLight => s.direct_light = Some(rgb),
            ColorField::AmbientLight => s.ambient_light = Some(rgb),
            ColorField::ShadowColor => s.shadow_color = Some(rgb),
            ColorField::SunFlare => {
                let size = s.sun_flare.map_or(1.0, |f| f.size);
                s.sun_flare = Some(SunFlare { color: c, size });
            }
            ColorField::FogColor => s.fog_color = Some(rgb),
            ColorField::SkyColor => s.sky_color = Some(rgb),
            ColorField::Vignette => {
                let multiply = s.vignette.is_some_and(|v| v.multiply);
                s.vignette = Some(Vignette { color: c, multiply });
            }
            ColorField::GroundColor => s.ground_color = Some(c),
            ColorField::WaterColor => s.water_color = Some(c),
            ColorField::UnderwaterColor => s.underwater_color = Some(c),
        }
    }
    /// A slider's value: the draft's, else what the map and host give now.
    pub fn number(&self, field: NumberField) -> f32 {
        let s = self.settings();
        let a = self.authored();
        let live = atmosphere::resolve(
            &a,
            &Settings {
                day_cycle: None,
                ..s.clone()
            },
            0.0,
        );
        let (azimuth, elevation) = atmosphere::angles(a.sun_direction);
        match field {
            NumberField::DayLength => s
                .day_cycle
                .map_or(atmosphere::DEFAULT_DAY_LENGTH, |d| d.length_seconds),
            NumberField::TimeOfDay => self.time_of_day().unwrap_or(12.0),
            NumberField::SunAzimuth => s.sun_azimuth.unwrap_or(azimuth),
            NumberField::SunElevation => s.sun_elevation.unwrap_or(elevation),
            NumberField::FlareSize => s.sun_flare.map_or(1.0, |f| f.size),
            NumberField::Overcast => s.overcast.unwrap_or(0.0),
            NumberField::VisibleDistance => {
                if live.fog_end > 0.0 {
                    live.fog_end
                } else {
                    atmosphere::MAX_DISTANCE
                }
            }
            NumberField::FogDistance => {
                if live.fog_end > 0.0 {
                    live.fog_start
                } else {
                    atmosphere::MAX_DISTANCE
                }
            }
            NumberField::WaterHeight => s
                .water_height
                .or_else(|| self.picked_look(ChoiceKind::Water)?.water_height)
                .unwrap_or(0.0),
        }
        .clamped(field.range().0, field.range().1.max(field.range().0))
    }
    pub fn set_number(&mut self, field: NumberField, v: f32) {
        if !v.is_finite() {
            return;
        }
        if field == NumberField::TimeOfDay {
            self.set_time(v);
            return;
        }
        self.revision += 1;
        let v = field.snap(v);
        let tick = self.tick();
        let s = self.draft.get_or_insert_with(Settings::default);
        match field {
            NumberField::DayLength => {
                let now = s.day_cycle;
                let time = now.map_or(0.5, |d| d.time_at(tick as f64) as f32);
                // Keep the sun where it is now; a new length lets time pass.
                s.day_cycle = Some(DayCycle {
                    length_seconds: v,
                    time,
                    anchor_tick: tick,
                    paused: now.is_some_and(|d| d.paused),
                });
            }
            NumberField::TimeOfDay => unreachable!(),
            NumberField::SunAzimuth => s.sun_azimuth = Some(v),
            NumberField::SunElevation => s.sun_elevation = Some(v),
            NumberField::FlareSize => {
                let color = s.sun_flare.map_or(SunFlare::default().color, |f| f.color);
                s.sun_flare = Some(SunFlare { color, size: v });
            }
            NumberField::Overcast => s.overcast = Some(v),
            NumberField::VisibleDistance => {
                s.visible_distance = Some(v);
                if s.fog_distance.is_some_and(|f| f > v) {
                    s.fog_distance = Some(v);
                }
            }
            NumberField::FogDistance => s.fog_distance = Some(v),
            NumberField::WaterHeight => s.water_height = Some(v),
        }
    }
    pub fn vignette_multiply(&self) -> bool {
        self.settings().vignette.is_some_and(|v| v.multiply)
    }
    pub fn set_vignette_multiply(&mut self, multiply: bool) {
        self.revision += 1;
        let s = self.draft.get_or_insert_with(Settings::default);
        let color = s.vignette.map_or([0.0, 0.0, 0.0, 0.6], |v| v.color);
        s.vignette = Some(Vignette { color, multiply });
    }

    // Looks.

    /// The Simple tab's list: Default, then [`LOOKS`].
    pub fn look_names() -> Vec<&'static str> {
        ["Default"]
            .into_iter()
            .chain(LOOKS.iter().map(|l| l.name))
            .collect()
    }
    /// Pick a Look (0 Default). Default gives back the sky's (or map's)
    /// own light, weather and sun, and keeps time passing if it was. A Look
    /// sets its time of day (moving a running day there) and its weather,
    /// and leaves the rest: Sunset then Thick Fog is a foggy sunset. The
    /// sky, ground and water stay.
    pub fn look(&mut self, index: usize) {
        let Some(look) = index.checked_sub(1).map(|i| LOOKS.get(i)) else {
            let base = self.base();
            let passes = self.time_passes();
            self.revision += 1;
            let s = self.draft.get_or_insert_with(Settings::default);
            for key in SKY_KEYS.iter().filter(|k| **k != "day_cycle") {
                s.take(&base, key);
            }
            if !passes {
                s.day_cycle = None;
            }
            return;
        };
        let Some(look) = look.copied() else {
            return;
        };
        if let Some(hours) = look.hours {
            self.set_time(hours);
        }
        if let Some(w) = look.weather {
            self.set_weather(w + 1);
        }
    }
    /// The Look the settings are, if any: Default while nothing but the
    /// scenery (and a passing day) is set; a time Look while its sun is
    /// held and the weather is not another Look's.
    pub fn current_look(&self) -> Option<usize> {
        let s = self.settings();
        let base = self.base();
        let held = s.day_cycle.filter(|d| d.paused).map(|d| d.time * 24.0);
        let weather = self.current_weather();
        let own_light = SKY_KEYS
            .iter()
            .filter(|k| !WEATHER_KEYS.contains(k) && **k != "day_cycle")
            .all(|k| s.same(&base, k));
        if !own_light {
            return None;
        }
        if held.is_none() && weather == Some(0) {
            return Some(0);
        }
        LOOKS
            .iter()
            .position(|l| {
                let time_fits = match l.hours {
                    Some(h) => held.is_some_and(|t| (t - h).abs() < 0.01),
                    None => true,
                };
                let weather_fits = match l.weather {
                    Some(w) => weather == Some(w + 1),
                    None => weather == Some(0),
                };
                time_fits && weather_fits && (l.hours.is_some() || held.is_none())
            })
            .map(|i| i + 1)
    }

    // Favourites.

    /// The rows as a favourite: a running day is kept by its length and
    /// time of day, so the tick it was set at (another server's) is
    /// dropped.
    pub fn favorite(&self) -> Settings {
        let mut s = self.settings();
        if let Some(d) = s.day_cycle.as_mut() {
            d.time = d.time_at(self.tick() as f64) as f32;
            d.anchor_tick = 0;
        }
        s
    }
    /// Fill the rows from a favourite (nothing is applied). Its day starts
    /// at the saved time of day from now.
    pub fn load_favorite(&mut self, mut s: Settings) {
        self.revision += 1;
        if let Some(d) = s.day_cycle.as_mut() {
            d.anchor_tick = self.tick();
        }
        self.draft = Some(s);
    }
    /// Every setting back to the map's own (on Apply).
    pub fn reset(&mut self) {
        self.revision += 1;
        self.draft = Some(Settings::default());
    }
    /// The draft differs from what the host has.
    pub fn changed(&self) -> bool {
        self.draft.as_ref().is_some_and(|d| {
            Some(d) != self.view.as_ref().map(|v| &v.settings)
                && !(d.is_empty() && self.view.is_none())
        })
    }
}

/// Hue (0..360), saturation and value (0..1) of an RGB colour.
pub fn hsv(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= f32::EPSILON {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max <= f32::EPSILON { 0.0 } else { d / max };
    [h, s, max]
}
/// The RGB colour of a hue (degrees), saturation and value.
pub fn rgb(hsv: [f32; 3]) -> [f32; 3] {
    let [h, s, v] = hsv;
    let c = v * s;
    let h = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> EnvironmentView {
        EnvironmentView {
            authored: Authored {
                sun_direction: atmosphere::light_direction(90.0, 30.0),
                direct_light: [0.6, 0.6, 0.5],
                ambient_light: [0.3; 3],
                fog_start: 100.0,
                fog_end: 400.0,
                fog_color: [0.7, 0.8, 0.9],
            },
            settings: Settings::default(),
            tick: 1200,
        }
    }
    fn choice(id: &str, look: Settings) -> Choice {
        Choice {
            id: id.into(),
            label: id.to_uppercase(),
            icon: IconRef::External(1),
            look,
        }
    }
    /// A Steam sky's look: its light and fog, and a day cycle it turned on.
    fn storm() -> Settings {
        Settings {
            fog_color: Some([0.3; 3]),
            direct_light: Some([0.4; 3]),
            visible_distance: Some(300.0),
            day_cycle: Some(DayCycle {
                length_seconds: 60.0,
                time: 0.5,
                anchor_tick: 0,
                paused: false,
            }),
            ..Default::default()
        }
    }
    fn model() -> EnvironmentModel {
        let mut m = EnvironmentModel::default();
        m.apply(view());
        m.set_choices(Choices {
            skies: vec![
                choice("blue", Settings::default()),
                choice("storm", storm()),
            ],
            map_sky: Some("blue".into()),
            ..Default::default()
        });
        m.begin();
        m
    }
    fn look(name: &str) -> usize {
        EnvironmentModel::look_names()
            .iter()
            .position(|n| *n == name)
            .unwrap()
    }

    #[test]
    fn rows_show_the_maps_own_values_until_changed() {
        let mut m = model();
        assert!((m.number(NumberField::SunAzimuth) - 90.0).abs() < 0.01);
        assert!((m.number(NumberField::SunElevation) - 30.0).abs() < 0.01);
        assert_eq!(m.number(NumberField::VisibleDistance), 400.0);
        assert_eq!(m.number(NumberField::FogDistance), 100.0);
        assert_eq!(m.color(ColorField::FogColor), [0.7, 0.8, 0.9, 1.0]);
        // Shadow colour shows the ambient light it stands in for.
        assert_eq!(m.color(ColorField::ShadowColor), [0.3, 0.3, 0.3, 1.0]);
        assert!(!m.changed());
        assert_eq!(m.current_look(), Some(0), "Default");
        m.set_number(NumberField::SunAzimuth, 181.3);
        assert_eq!(m.number(NumberField::SunAzimuth), 181.0);
        assert!(m.changed());
        assert_eq!(m.current_look(), None);
        m.draft.as_ref().unwrap().validate().unwrap();
        // Visible distance pulls the fog distance in with it.
        m.set_number(NumberField::FogDistance, 300.0);
        m.set_number(NumberField::VisibleDistance, 200.0);
        assert_eq!(m.draft.as_ref().unwrap().fog_distance, Some(200.0));
        // Reset is the map's own look, which the host already has.
        m.reset();
        assert!(m.draft.as_ref().unwrap().is_empty());
        assert!(!m.changed());
    }

    #[test]
    fn the_time_of_day_works_with_the_cycle_off_and_looks_set_it() {
        let mut m = model();
        assert_eq!(m.time_of_day(), None, "the map's own sun");
        // The Time of Day slider holds the sun without the cycle.
        m.set_number(NumberField::TimeOfDay, 18.0);
        let held = m.settings().day_cycle.unwrap();
        assert_eq!(
            (held.time, held.anchor_tick, held.paused),
            (0.75, 1200, true)
        );
        assert!(!m.day_cycle());
        // Night is a time of day, so with the cycle on it is now night,
        // not a dark noon, and the night goes on turning.
        m.set_day_cycle(true);
        m.look(look("Night"));
        let cycle = m.settings().day_cycle.unwrap();
        assert_eq!((cycle.time, cycle.paused), (0.0, false));
        assert_eq!(m.settings().direct_light, None, "no night colours");
        // Held, it is recognised.
        m.set_day_cycle(false);
        assert_eq!(m.current_look(), Some(look("Night")));
        // Default gives the map its sun back.
        m.look(0);
        assert_eq!(m.time_of_day(), None);
        assert_eq!(m.current_look(), Some(0));
        // But keeps a running day running.
        m.set_day_cycle(true);
        m.look(0);
        assert!(m.day_cycle());
        m.draft.as_ref().unwrap().validate().unwrap();
    }

    #[test]
    fn looks_layer_a_time_and_a_weather() {
        let mut m = model();
        m.look(look("Sunset"));
        assert_eq!(m.current_look(), Some(look("Sunset")));
        m.look(look("Thick Fog"));
        let s = m.settings();
        assert_eq!(s.visible_distance, Some(80.0));
        assert!(
            s.day_cycle
                .is_some_and(|d| (d.time * 24.0 - 17.75).abs() < 0.01),
            "the sunset stays under the fog"
        );
        // A foggy sunset is neither Look alone.
        assert_eq!(m.current_look(), None);
        // Clear Day: noon and clear air.
        m.look(look("Clear Day"));
        assert_eq!(m.current_look(), Some(look("Clear Day")));
        assert_eq!(m.settings().overcast, None);
        m.draft.as_ref().unwrap().validate().unwrap();
    }

    #[test]
    fn a_sky_keeps_the_look_the_time_and_values_set_by_hand() {
        let mut m = model();
        m.look(look("Golden Hour"));
        // A Steam sky's look is the base: its light and fog, never its
        // day cycle; the Look stays picked.
        m.set_choice(ChoiceKind::Sky, Some("storm".into()));
        let s = m.settings();
        assert_eq!(s.sky.as_deref(), Some("storm"));
        assert_eq!(s.fog_color, Some([0.3; 3]));
        assert_eq!(s.visible_distance, Some(300.0));
        assert!(s.day_cycle.is_some_and(|d| d.paused), "the time stays");
        assert_eq!(m.current_look(), Some(look("Golden Hour")));
        // A value set by hand outlasts another sky; a weather over the sky
        // outlasts going back to the map's own sky.
        m.set_color(ColorField::SkyColor, [0.2, 0.3, 0.9, 1.0]);
        m.look(look("Overcast"));
        m.set_choice(ChoiceKind::Sky, None);
        let s = m.settings();
        assert_eq!((s.fog_color, s.direct_light), (None, None));
        assert_eq!(s.sky_color, Some([0.2, 0.3, 0.9]));
        assert_eq!(s.overcast, WEATHERS[2].overcast);
        // Default puts the storm's own light back over a hand value.
        m.set_choice(ChoiceKind::Sky, Some("storm".into()));
        m.look(0);
        let s = m.settings();
        assert_eq!((s.sky_color, s.fog_color), (None, Some([0.3; 3])));
        assert_eq!(m.current_look(), Some(0));
        m.draft.as_ref().unwrap().validate().unwrap();
    }

    #[test]
    fn steam_grounds_and_waters_need_a_flat_map() {
        use atmosphere::NO_WATER;
        let lava = Settings {
            water_height: Some(5.0),
            water_color: Some([1.0, 0.3, 0.0, 1.0]),
            underwater_color: Some([1.0, 0.0, 0.0, 0.5]),
            ..Default::default()
        };
        let mut m = model();
        let mut c = m.choices.clone();
        c.grounds = vec![choice(
            "tt",
            Settings {
                ground_color: Some([0.5, 0.5, 0.5, 1.0]),
                ..Default::default()
            },
        )];
        c.waters = vec![choice(NO_WATER, Settings::default()), choice("lava", lava)];
        m.set_choices(c);
        assert!(!m.choices.offered(ChoiceKind::Ground));
        m.choices.flat = true;
        assert!(m.choices.offered(ChoiceKind::Water));
        m.set_choice(ChoiceKind::Ground, Some("tt".into()));
        m.set_choice(ChoiceKind::Water, Some("lava".into()));
        assert!(m.row_active(ChoiceKind::Water));
        assert_eq!(m.number(NumberField::WaterHeight), 5.0);
        assert_eq!(m.color(ColorField::GroundColor), [0.5, 0.5, 0.5, 1.0]);
        m.set_number(NumberField::WaterHeight, 12.3);
        assert_eq!(m.settings().water_height, Some(12.5));
        // Looks and skies keep the scenery.
        m.look(look("Night"));
        m.set_choice(ChoiceKind::Sky, Some("storm".into()));
        m.look(0);
        assert_eq!(m.settings().water_height, Some(12.5));
        m.set_choice(ChoiceKind::Water, Some(NO_WATER.into()));
        assert!(!m.row_active(ChoiceKind::Water));
        m.draft.as_ref().unwrap().validate().unwrap();
    }

    #[test]
    fn a_cycle_the_host_runs_shows_as_applied_and_keeps_turning() {
        let mut m = model();
        m.set_day_cycle(true);
        m.set_number(NumberField::DayLength, 60.0);
        let sent = m.settings();
        let mut host = sent.clone();
        host.day_cycle.as_mut().unwrap().anchor_tick = 1500;
        m.apply(EnvironmentView {
            settings: host.clone(),
            tick: 1560,
            ..view()
        });
        assert!(!m.changed());
        m.set_number(NumberField::SunAzimuth, 200.0);
        assert_eq!(m.settings().day_cycle, host.day_cycle);
        m.set_number(NumberField::TimeOfDay, 6.0);
        assert!(m.changed());
        assert_ne!(m.settings().day_cycle, host.day_cycle);
    }

    #[test]
    fn a_favorite_keeps_the_time_of_day_and_starts_its_cycle_from_now() {
        let mut m = model();
        m.set_day_cycle(true);
        m.set_number(NumberField::DayLength, 60.0);
        m.set_number(NumberField::TimeOfDay, 18.0);
        let mut later = view();
        later.settings = m.settings();
        later.tick = 1200 + 10 * atmosphere::TICKS_PER_SECOND;
        m.apply(later);
        let fav = m.favorite();
        let cycle = fav.day_cycle.unwrap();
        assert_eq!(cycle.anchor_tick, 0);
        assert!((cycle.time - (0.75 + 10.0 / 60.0)).abs() < 0.01);
        fav.validate().unwrap();
        let mut other = EnvironmentModel::default();
        other.apply(EnvironmentView {
            tick: 9000,
            ..view()
        });
        other.begin();
        other.load_favorite(fav.clone());
        assert_eq!(other.settings().day_cycle.unwrap().anchor_tick, 9000);
        assert!(other.changed());
    }

    #[test]
    fn hsv_round_trips() {
        for c in [
            [0.2, 0.4, 0.9],
            [1.0, 0.0, 0.0],
            [0.5, 0.5, 0.5],
            [0.9, 0.8, 0.1],
        ] {
            let back = rgb(hsv(c));
            assert!(
                (0..3).all(|i| (back[i] - c[i]).abs() < 1e-5),
                "{c:?} {back:?}"
            );
        }
    }
}
