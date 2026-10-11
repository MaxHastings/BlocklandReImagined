//! The Admin Menu's Environment window (v21's `EnvironmentGui`, built
//! natively): what the host set over the map's own sun, sky and fog, and
//! the admin's draft of a change. Nothing here has authority: the host
//! checks the rank and validates every value when the draft is applied.
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
}
impl ColorField {
    pub fn label(self) -> &'static str {
        match self {
            Self::DirectLight => "Direct Light",
            Self::AmbientLight => "Ambient Light",
            Self::ShadowColor => "Shadow Color",
            Self::SunFlare => "Sun Flare Color",
            Self::FogColor => "Fog Color",
            Self::SkyColor => "Sky Color",
            Self::Vignette => "Vignette Color",
        }
    }
    /// The picker shows an alpha slider (the flare's and vignette's strength).
    pub fn has_alpha(self) -> bool {
        matches!(self, Self::SunFlare | Self::Vignette)
    }
}

/// A number the Advanced tab's sliders edit: its range and step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberField {
    DayLength,
    TimeOfDay,
    SunAzimuth,
    SunElevation,
    FlareSize,
    VisibleDistance,
    FogDistance,
}
impl NumberField {
    pub const ALL: [Self; 7] = [
        Self::DayLength,
        Self::TimeOfDay,
        Self::SunAzimuth,
        Self::SunElevation,
        Self::FlareSize,
        Self::VisibleDistance,
        Self::FogDistance,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::DayLength => "Day Length",
            Self::TimeOfDay => "Time of Day",
            Self::SunAzimuth => "Sun Azimuth",
            Self::SunElevation => "Sun Elevation",
            Self::FlareSize => "Sun Flare Size",
            Self::VisibleDistance => "Visible Distance",
            Self::FogDistance => "Fog Distance",
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
            Self::VisibleDistance => (atmosphere::MIN_VISIBLE_DISTANCE, atmosphere::MAX_DISTANCE),
            Self::FogDistance => (0.0, atmosphere::MAX_DISTANCE),
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
            Self::FlareSize => 0.05,
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
            Self::TimeOfDay => {
                let minutes = (v * 60.0).round() as u32 % (24 * 60);
                format!("{:02}:{:02}", minutes / 60, minutes % 60)
            }
            Self::SunAzimuth | Self::SunElevation => format!("{v:.0}°"),
            Self::FlareSize => format!("{v:.2}x"),
            Self::VisibleDistance | Self::FogDistance => format!("{v:.0}"),
        }
    }
    pub fn snap(self, v: f32) -> f32 {
        let (lo, hi) = self.range();
        ((v / self.step()).round() * self.step()).clamped(lo, hi)
    }
}

#[derive(Debug, Clone, Default)]
pub struct EnvironmentModel {
    /// The host's environment, once in a game.
    pub view: Option<EnvironmentView>,
    /// The Environment window's unsaved change, while it is open.
    pub draft: Option<Settings>,
    /// The colour the picker is editing.
    pub picking: Option<ColorField>,
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
    }
    pub fn end(&mut self) {
        self.revision += 1;
        self.draft = None;
        self.picking = None;
    }
    pub fn apply(&mut self, view: EnvironmentView) {
        self.revision += 1;
        // The host stamps a cycle it is given with its own tick. Once it
        // runs the draft's day length from the draft's time of day, the
        // draft takes that stamp: the window then shows the change as
        // applied, and a later Apply keeps the cycle turning rather than
        // restarting it.
        if let Some(d) = self.draft.as_mut().and_then(|d| d.day_cycle.as_mut())
            && let Some(host) = view.settings.day_cycle
            && (d.length_seconds, d.time) == (host.length_seconds, host.time)
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
    /// The draft (or the host's settings), resolved without the day
    /// cycle's colours: what each row shows.
    pub fn settings(&self) -> Settings {
        self.draft
            .clone()
            .or_else(|| self.view.as_ref().map(|v| v.settings.clone()))
            .unwrap_or_default()
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
        }
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
            NumberField::TimeOfDay => s
                .day_cycle
                .map_or(12.0, |d| d.time_at(self.tick() as f64) as f32 * 24.0),
            NumberField::SunAzimuth => s.sun_azimuth.unwrap_or(azimuth),
            NumberField::SunElevation => s.sun_elevation.unwrap_or(elevation),
            NumberField::FlareSize => s.sun_flare.map_or(1.0, |f| f.size),
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
        }
        .clamped(field.range().0, field.range().1.max(field.range().0))
    }
    pub fn set_number(&mut self, field: NumberField, v: f32) {
        if !v.is_finite() {
            return;
        }
        self.revision += 1;
        let v = field.snap(v);
        let tick = self.tick();
        let s = self.draft.get_or_insert_with(Settings::default);
        match field {
            NumberField::DayLength => {
                if let Some(d) = &mut s.day_cycle {
                    // Keep the time of day where it is now.
                    let time = d.time_at(tick as f64) as f32;
                    *d = DayCycle {
                        length_seconds: v,
                        time,
                        anchor_tick: tick,
                    };
                }
            }
            NumberField::TimeOfDay => {
                if let Some(d) = &mut s.day_cycle {
                    d.time = (v / 24.0).rem_euclid(1.0);
                    d.anchor_tick = tick;
                }
            }
            NumberField::SunAzimuth => s.sun_azimuth = Some(v),
            NumberField::SunElevation => s.sun_elevation = Some(v),
            NumberField::FlareSize => {
                let color = s.sun_flare.map_or(SunFlare::default().color, |f| f.color);
                s.sun_flare = Some(SunFlare { color, size: v });
            }
            NumberField::VisibleDistance => {
                s.visible_distance = Some(v);
                if s.fog_distance.is_some_and(|f| f > v) {
                    s.fog_distance = Some(v);
                }
            }
            NumberField::FogDistance => s.fog_distance = Some(v),
        }
    }
    pub fn day_cycle(&self) -> bool {
        self.settings().day_cycle.is_some()
    }
    /// Turn the day/night cycle on (from noon) or off.
    pub fn set_day_cycle(&mut self, on: bool) {
        self.revision += 1;
        let tick = self.tick();
        let s = self.draft.get_or_insert_with(Settings::default);
        s.day_cycle = on.then_some(s.day_cycle.unwrap_or(DayCycle {
            length_seconds: atmosphere::DEFAULT_DAY_LENGTH,
            time: 0.5,
            anchor_tick: tick,
        }));
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
    /// A Simple tab look: its settings, keeping the day/night cycle.
    pub fn preset(&mut self, index: usize) {
        let Some((_, look)) = atmosphere::presets().into_iter().nth(index) else {
            return;
        };
        let cycle = self.settings().day_cycle;
        self.revision += 1;
        self.draft = Some(Settings {
            day_cycle: cycle,
            ..look
        });
    }
    /// The preset the draft is exactly, if any.
    pub fn current_preset(&self) -> Option<usize> {
        let s = Settings {
            day_cycle: None,
            ..self.settings()
        };
        atmosphere::presets()
            .iter()
            .position(|(_, look)| *look == s)
    }
    /// The rows as a favourite: a day cycle is kept by its length and time
    /// of day, so the tick it was set at (another server's) is dropped.
    pub fn favorite(&self) -> Settings {
        let mut s = self.settings();
        if let Some(d) = s.day_cycle.as_mut() {
            d.time = d.time_at(self.tick() as f64) as f32;
            d.anchor_tick = 0;
        }
        s
    }
    /// Fill the rows from a favourite (nothing is applied). Its day cycle
    /// starts at the saved time of day from now.
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

    #[test]
    fn rows_show_the_maps_own_values_until_changed() {
        let mut m = EnvironmentModel::default();
        m.apply(view());
        m.begin();
        assert!((m.number(NumberField::SunAzimuth) - 90.0).abs() < 0.01);
        assert!((m.number(NumberField::SunElevation) - 30.0).abs() < 0.01);
        assert_eq!(m.number(NumberField::VisibleDistance), 400.0);
        assert_eq!(m.number(NumberField::FogDistance), 100.0);
        assert_eq!(m.color(ColorField::FogColor), [0.7, 0.8, 0.9, 1.0]);
        // Shadow colour shows the ambient light it stands in for.
        assert_eq!(m.color(ColorField::ShadowColor), [0.3, 0.3, 0.3, 1.0]);
        assert!(!m.changed());
        m.set_number(NumberField::SunAzimuth, 181.3);
        assert_eq!(m.number(NumberField::SunAzimuth), 181.0);
        assert!(m.changed());
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
    fn presets_keep_the_day_cycle_and_are_recognised() {
        let mut m = EnvironmentModel::default();
        m.apply(view());
        m.begin();
        m.set_day_cycle(true);
        m.set_number(NumberField::TimeOfDay, 18.0);
        let cycle = m.draft.as_ref().unwrap().day_cycle.unwrap();
        assert_eq!((cycle.time, cycle.anchor_tick), (0.75, 1200));
        m.preset(3);
        assert_eq!(m.current_preset(), Some(3));
        assert_eq!(m.draft.as_ref().unwrap().day_cycle, Some(cycle));
        m.set_color(ColorField::SkyColor, [2.0, 0.5, f32::NAN, 1.0]);
        assert_eq!(m.draft.as_ref().unwrap().sky_color, Some([1.0, 0.5, 0.0]));
        assert_eq!(m.current_preset(), None);
        m.draft.as_ref().unwrap().validate().unwrap();
    }

    #[test]
    fn a_cycle_the_host_runs_shows_as_applied_and_keeps_turning() {
        let mut m = EnvironmentModel::default();
        m.apply(view());
        m.begin();
        m.set_day_cycle(true);
        m.set_number(NumberField::DayLength, 60.0);
        let sent = m.settings();
        // The host anchors it at its own, later tick.
        let mut host = sent.clone();
        host.day_cycle.as_mut().unwrap().anchor_tick = 1500;
        m.apply(EnvironmentView {
            settings: host.clone(),
            tick: 1560,
            ..view()
        });
        assert!(
            !m.changed(),
            "the window still says the cycle is not applied"
        );
        // Applying another change sends the running cycle untouched.
        m.set_number(NumberField::SunAzimuth, 200.0);
        assert_eq!(m.settings().day_cycle, host.day_cycle);
        // A time of day the admin picks is a new cycle again.
        m.set_number(NumberField::TimeOfDay, 6.0);
        assert!(m.changed());
        assert_ne!(m.settings().day_cycle, host.day_cycle);
    }

    #[test]
    fn a_favorite_keeps_the_time_of_day_and_starts_its_cycle_from_now() {
        let mut m = EnvironmentModel::default();
        m.apply(view());
        m.begin();
        m.set_day_cycle(true);
        m.set_number(NumberField::DayLength, 60.0);
        m.set_number(NumberField::TimeOfDay, 18.0);
        m.set_color(ColorField::SkyColor, [0.2, 0.3, 0.9, 1.0]);
        // The cycle has run ten seconds since it was set.
        let mut later = view();
        later.settings = m.settings();
        later.tick = 1200 + 10 * atmosphere::TICKS_PER_SECOND;
        m.apply(later);
        let fav = m.favorite();
        let cycle = fav.day_cycle.unwrap();
        assert_eq!(cycle.anchor_tick, 0);
        assert!(
            (cycle.time - (0.75 + 10.0 / 60.0)).abs() < 0.01,
            "{}",
            cycle.time
        );
        assert_eq!(fav.sky_color, Some([0.2, 0.3, 0.9]));
        fav.validate().unwrap();
        // Loading on another server anchors the cycle at its tick.
        let mut other = EnvironmentModel::default();
        other.apply(EnvironmentView {
            tick: 9000,
            ..view()
        });
        other.begin();
        other.load_favorite(fav.clone());
        let loaded = other.draft.as_ref().unwrap().day_cycle.unwrap();
        assert_eq!(loaded.anchor_tick, 9000);
        assert_eq!(loaded.time, cycle.time);
        assert!((other.number(NumberField::TimeOfDay) - 24.0 * cycle.time).abs() < 0.01);
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
