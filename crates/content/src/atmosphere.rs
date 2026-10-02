//! The server's live environment (the Admin Menu's Environment window and
//! Add-Ons' `set_environment`): sun, light, fog, sky, sun flare, vignette and
//! a day/night cycle, over what the map authored. Every setting is optional;
//! an unset one keeps the map's own value, so a new map starts as authored
//! and "Reset" is simply every setting unset.
//!
//! [`resolve`] turns the map's authored values, the settings and the
//! server's tick into what a frame draws. It is pure and deterministic, so
//! every client computes the same sky from the same replicated settings, and
//! a day/night cycle costs the network nothing after it is set. Clients pass
//! their smooth estimate of the server tick, so the sun turns every frame.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

/// Server ticks per second (`bri_world::TICKS_PER_SECOND`); the day/night
/// cycle runs on the replicated tick.
pub const TICKS_PER_SECOND: u64 = 120;
/// Shortest and longest day a cycle may have, seconds.
pub const DAY_LENGTH: std::ops::RangeInclusive<f32> = 10.0..=86_400.0;
/// v21's default day length, seconds.
pub const DEFAULT_DAY_LENGTH: f32 = 300.0;
/// Farthest visible and fog distances, units. Players' own Visible Distance
/// option still caps what they draw.
pub const MAX_DISTANCE: f32 = 1000.0;
/// Nearest visible distance, units.
pub const MIN_VISIBLE_DISTANCE: f32 = 20.0;
/// Sun flare size, times the standard flare.
pub const FLARE_SIZE: std::ops::RangeInclusive<f32> = 0.1..=4.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DayCycle {
    /// Seconds for a whole day and night.
    pub length_seconds: f32,
    /// Time of day when the cycle was set: 0 midnight, 0.25 sunrise, 0.5
    /// noon, 0.75 sunset.
    pub time: f32,
    /// Server tick the cycle was set at; the host stamps it, whatever a
    /// request says.
    #[serde(default)]
    pub anchor_tick: u64,
}
impl DayCycle {
    /// Time of day (0..1) at server tick `tick`, which may fall between
    /// ticks (a client's frame).
    pub fn time_at(&self, tick: f64) -> f64 {
        let elapsed = (tick - self.anchor_tick as f64).max(0.0) / TICKS_PER_SECOND as f64;
        (f64::from(self.time) + elapsed / f64::from(self.length_seconds)).rem_euclid(1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SunFlare {
    /// Colour and strength (alpha) of the sun's disc and glow.
    pub color: [f32; 4],
    /// Times the standard flare.
    pub size: f32,
}
impl Default for SunFlare {
    fn default() -> Self {
        Self {
            color: [1.0, 0.95, 0.8, 1.0],
            size: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vignette {
    /// Colour at the screen's edge; alpha is how strong.
    pub color: [f32; 4],
    /// Multiply the screen by the colour instead of blending it over.
    pub multiply: bool,
}

/// Every setting's name, as [`Settings::unset`] takes them.
pub const KEYS: [&str; 12] = [
    "day_cycle",
    "sun_azimuth",
    "sun_elevation",
    "direct_light",
    "ambient_light",
    "shadow_color",
    "sun_flare",
    "visible_distance",
    "fog_distance",
    "fog_color",
    "sky_color",
    "vignette",
];

/// What the server set. `None` keeps the map's own value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day_cycle: Option<DayCycle>,
    /// Degrees around, 0 to 360 (Torque's `Sun::azimuth`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sun_azimuth: Option<f32>,
    /// Degrees above the horizon, -90 to 90; with a day/night cycle, the
    /// sun's height at noon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sun_elevation: Option<f32>,
    /// The sun's light (`Sun::color`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direct_light: Option<[f32; 3]>,
    /// Light everywhere (`Sun::ambient`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ambient_light: Option<[f32; 3]>,
    /// Light where the sun does not reach, in place of the ambient light.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_color: Option<[f32; 3]>,
    /// A disc and glow drawn at the sun (v20 had none).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sun_flare: Option<SunFlare>,
    /// Where fog is complete, units.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible_distance: Option<f32>,
    /// Where fog starts, units.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fog_distance: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fog_color: Option<[f32; 3]>,
    /// Tint over the map's sky.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sky_color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vignette: Option<Vignette>,
}

fn unit(c: &[f32]) -> bool {
    c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        if let Some(d) = &self.day_cycle {
            ensure!(
                DAY_LENGTH.contains(&d.length_seconds) && (0.0..=1.0).contains(&d.time),
                "Day length must be 10 seconds to 24 hours"
            );
        }
        ensure!(
            self.sun_azimuth.is_none_or(|a| (0.0..=360.0).contains(&a)),
            "Sun azimuth must be 0 to 360"
        );
        ensure!(
            self.sun_elevation
                .is_none_or(|e| (-90.0..=90.0).contains(&e)),
            "Sun elevation must be -90 to 90"
        );
        for c in [
            &self.direct_light,
            &self.ambient_light,
            &self.shadow_color,
            &self.fog_color,
            &self.sky_color,
        ]
        .into_iter()
        .flatten()
        {
            ensure!(unit(c), "Colours must be 0 to 1");
        }
        if let Some(f) = &self.sun_flare {
            ensure!(
                unit(&f.color) && FLARE_SIZE.contains(&f.size),
                "Invalid sun flare"
            );
        }
        if let Some(v) = &self.vignette {
            ensure!(unit(&v.color), "Invalid vignette colour");
        }
        ensure!(
            self.visible_distance
                .is_none_or(|d| (MIN_VISIBLE_DISTANCE..=MAX_DISTANCE).contains(&d)),
            "Visible distance must be 20 to 1000"
        );
        ensure!(
            self.fog_distance
                .is_none_or(|d| (0.0..=MAX_DISTANCE).contains(&d)),
            "Fog distance must be 0 to 1000"
        );
        Ok(())
    }
    /// Every setting of `other` that is set replaces this one's.
    pub fn merge(&mut self, other: &Settings) {
        macro_rules! take {
            ($($f:ident),*) => {$(if other.$f.is_some() { self.$f = other.$f; })*};
        }
        take!(
            day_cycle,
            sun_azimuth,
            sun_elevation,
            direct_light,
            ambient_light,
            shadow_color,
            sun_flare,
            visible_distance,
            fog_distance,
            fog_color,
            sky_color,
            vignette
        );
    }
    /// Put setting `name` (a field name, as in [`KEYS`]) back to the map's
    /// own. False for an unknown name.
    pub fn unset(&mut self, name: &str) -> bool {
        match name {
            "day_cycle" => self.day_cycle = None,
            "sun_azimuth" => self.sun_azimuth = None,
            "sun_elevation" => self.sun_elevation = None,
            "direct_light" => self.direct_light = None,
            "ambient_light" => self.ambient_light = None,
            "shadow_color" => self.shadow_color = None,
            "sun_flare" => self.sun_flare = None,
            "visible_distance" => self.visible_distance = None,
            "fog_distance" => self.fog_distance = None,
            "fog_color" => self.fog_color = None,
            "sky_color" => self.sky_color = None,
            "vignette" => self.vignette = None,
            _ => return false,
        }
        true
    }
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The map's own environment, as its scene authored it (native Y-up).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Authored {
    /// Direction the sunlight travels.
    pub sun_direction: [f32; 3],
    pub direct_light: [f32; 3],
    pub ambient_light: [f32; 3],
    /// Fog start and end (0 end: no fog) and colour.
    pub fog_start: f32,
    pub fog_end: f32,
    pub fog_color: [f32; 3],
}

/// What a frame draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Live {
    /// Direction the sunlight (or, at night, the moonlight) travels.
    pub sun_direction: [f32; 3],
    pub direct_light: [f32; 3],
    pub ambient_light: [f32; 3],
    /// Light where the sun does not reach; the ambient light when unset.
    pub shadow_color: Option<[f32; 3]>,
    pub fog_start: f32,
    /// 0: no fog.
    pub fog_end: f32,
    pub fog_color: [f32; 3],
    /// Multiplies the map's sky.
    pub sky_tint: [f32; 3],
    /// The disc and glow's colour (alpha: strength) and size; none drawn
    /// when the alpha is 0.
    pub flare: ([f32; 4], f32),
    pub vignette: Option<Vignette>,
}

/// Direction sunlight travels (native Y-up) for a sun at `azimuth` and
/// `elevation` degrees, as `crate::scene::sun_direction` places it.
pub fn light_direction(azimuth: f32, elevation: f32) -> [f32; 3] {
    let toward = toward_sun(azimuth.to_radians(), elevation.to_radians());
    native([-toward[0], -toward[1], -toward[2]])
}
/// Unit vector toward the sun, Torque Z-up.
fn toward_sun(yaw: f32, pitch: f32) -> [f32; 3] {
    [
        yaw.sin() * pitch.cos(),
        yaw.cos() * pitch.cos(),
        pitch.sin(),
    ]
}
fn native(z_up: [f32; 3]) -> [f32; 3] {
    [z_up[0], z_up[2], -z_up[1]]
}
/// Azimuth (0..360) and elevation (-90..90) degrees of a light direction
/// (native Y-up): the inverse of [`light_direction`].
pub fn angles(direction: [f32; 3]) -> (f32, f32) {
    let [x, y, z] = direction;
    let length = (x * x + y * y + z * z).sqrt();
    if length <= f32::EPSILON {
        return (0.0, 90.0);
    }
    // Toward the sun, back in Torque Z-up.
    let (tx, ty, tz) = (-x / length, z / length, -y / length);
    let elevation = tz.clamp(-1.0, 1.0).asin().to_degrees();
    let azimuth = if tx.abs() < 1e-6 && ty.abs() < 1e-6 {
        0.0
    } else {
        tx.atan2(ty).to_degrees().rem_euclid(360.0)
    };
    (azimuth, elevation)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
fn mul(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] * b[i])
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    a.map(|v| v * s)
}
fn clamp01(a: [f32; 3]) -> [f32; 3] {
    a.map(|v| v.clamp(0.0, 1.0))
}

/// Twilight's warm tint, the night's cool one, and the moon's light.
const DUSK: [f32; 3] = [1.0, 0.55, 0.3];
const NIGHT: [f32; 3] = [0.35, 0.45, 0.8];
const MOON: [f32; 3] = [0.16, 0.19, 0.3];

/// The map's environment with the server's settings over it, at server
/// tick `tick` (fractional between ticks, so a frame's sun is its own).
pub fn resolve(authored: &Authored, settings: &Settings, tick: f64) -> Live {
    let (map_azimuth, map_elevation) = angles(authored.sun_direction);
    let azimuth = settings.sun_azimuth.unwrap_or(map_azimuth);
    let elevation = settings.sun_elevation.unwrap_or(map_elevation);
    let direct = settings.direct_light.unwrap_or(authored.direct_light);
    let ambient = settings.ambient_light.unwrap_or(authored.ambient_light);
    let fog_color = settings.fog_color.unwrap_or(authored.fog_color);
    let sky = settings.sky_color.unwrap_or([1.0; 3]);
    let (mut fog_start, mut fog_end) = (authored.fog_start, authored.fog_end);
    if let Some(end) = settings.visible_distance {
        // A map without fog gets it from its first distance.
        if fog_end <= 0.0 {
            fog_start = end * 0.5;
        } else {
            fog_start *= end / fog_end;
        }
        fog_end = end;
    }
    if let Some(start) = settings.fog_distance {
        fog_start = start;
        if fog_end <= 0.0 {
            fog_end = MAX_DISTANCE;
        }
    }
    if fog_end > 0.0 {
        fog_start = fog_start.clamp(0.0, fog_end);
    }
    let flare = settings
        .sun_flare
        .map_or(([0.0; 4], 1.0), |f| (f.color, f.size));
    let mut live = Live {
        sun_direction: if settings.sun_azimuth.is_some() || settings.sun_elevation.is_some() {
            light_direction(azimuth, elevation)
        } else {
            authored.sun_direction
        },
        direct_light: direct,
        ambient_light: ambient,
        shadow_color: settings.shadow_color,
        fog_start,
        fog_end,
        fog_color,
        sky_tint: sky,
        flare,
        vignette: settings.vignette,
    };
    let Some(cycle) = settings.day_cycle else {
        return live;
    };
    // The sun crosses the sky on a great circle: up on one side at 0.25,
    // at `elevation` over `azimuth` at noon, down on the other at 0.75.
    let (yaw, noon) = (
        azimuth.to_radians(),
        elevation.clamp(1.0, 90.0).to_radians(),
    );
    let at = |time: f64| -> [f32; 3] {
        let angle = TAU * (time as f32 - 0.5);
        let u = toward_sun(yaw, noon);
        let w = [-yaw.cos(), yaw.sin(), 0.0];
        std::array::from_fn(|i| angle.cos() * u[i] + angle.sin() * w[i])
    };
    let toward = at(cycle.time_at(tick));
    let height = toward[2];
    let day = smoothstep(-0.1, 0.25, height);
    let sun = smoothstep(-0.05, 0.15, height);
    let moon = 1.0 - smoothstep(-0.25, -0.05, height);
    let warm = 1.0 - smoothstep(0.0, 0.3, (height - 0.04).abs());
    let tint = mix([1.0; 3], DUSK, warm);
    live.sun_direction = if height >= -0.05 {
        native(toward.map(|v| -v))
    } else {
        // The moon, opposite the sun.
        native(toward)
    };
    live.direct_light = clamp01(if height >= -0.05 {
        scale(mul(direct, tint), sun)
    } else {
        scale(MOON, moon)
    });
    let night = |c: [f32; 3], share: f32| mix(scale(mul(c, NIGHT), share), c, day);
    live.ambient_light = clamp01(mul(night(ambient, 0.35), mix([1.0; 3], tint, 0.3)));
    live.shadow_color = settings
        .shadow_color
        .map(|c| clamp01(mul(night(c, 0.35), mix([1.0; 3], tint, 0.3))));
    live.fog_color = clamp01(mul(night(fog_color, 0.15), mix([1.0; 3], tint, 0.6)));
    live.sky_tint = clamp01(mul(night(sky, 0.15), mix([1.0; 3], tint, 0.6)));
    live.flare.0[3] *= sun;
    live
}

/// The Simple tab's looks: a name and the settings it sets. "Map Default"
/// (no settings) comes first.
pub fn presets() -> Vec<(&'static str, Settings)> {
    let look = |azimuth: f32,
                elevation: f32,
                direct: [f32; 3],
                ambient: [f32; 3],
                fog: [f32; 3],
                sky: [f32; 3]| Settings {
        sun_azimuth: Some(azimuth),
        sun_elevation: Some(elevation),
        direct_light: Some(direct),
        ambient_light: Some(ambient),
        fog_color: Some(fog),
        sky_color: Some(sky),
        ..Default::default()
    };
    vec![
        ("Map Default", Settings::default()),
        (
            "Clear Day",
            look(
                135.0,
                60.0,
                [0.85, 0.83, 0.75],
                [0.45, 0.47, 0.52],
                [0.75, 0.82, 0.9],
                [1.0, 1.0, 1.0],
            ),
        ),
        (
            "Golden Hour",
            Settings {
                sun_flare: Some(SunFlare {
                    color: [1.0, 0.8, 0.45, 0.9],
                    size: 1.5,
                }),
                ..look(
                    250.0,
                    12.0,
                    [1.0, 0.72, 0.42],
                    [0.42, 0.36, 0.38],
                    [0.95, 0.72, 0.5],
                    [1.0, 0.82, 0.65],
                )
            },
        ),
        (
            "Sunset",
            Settings {
                sun_flare: Some(SunFlare {
                    color: [1.0, 0.55, 0.25, 1.0],
                    size: 2.0,
                }),
                ..look(
                    270.0,
                    4.0,
                    [0.95, 0.45, 0.25],
                    [0.32, 0.24, 0.34],
                    [0.85, 0.45, 0.35],
                    [0.95, 0.6, 0.55],
                )
            },
        ),
        (
            "Night",
            look(
                45.0,
                40.0,
                [0.16, 0.19, 0.3],
                [0.1, 0.12, 0.2],
                [0.03, 0.04, 0.08],
                [0.12, 0.14, 0.25],
            ),
        ),
        (
            "Overcast",
            Settings {
                visible_distance: Some(350.0),
                fog_distance: Some(40.0),
                ..look(
                    160.0,
                    70.0,
                    [0.35, 0.36, 0.38],
                    [0.55, 0.56, 0.6],
                    [0.62, 0.64, 0.67],
                    [0.7, 0.72, 0.75],
                )
            },
        ),
        (
            "Thick Fog",
            Settings {
                visible_distance: Some(90.0),
                fog_distance: Some(5.0),
                fog_color: Some([0.78, 0.8, 0.82]),
                ..Default::default()
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }
    fn map() -> Authored {
        Authored {
            sun_direction: light_direction(315.0, 45.0),
            direct_light: [0.6, 0.6, 0.5],
            ambient_light: [0.4, 0.4, 0.45],
            fog_start: 100.0,
            fog_end: 400.0,
            fog_color: [0.7, 0.8, 0.9],
        }
    }

    #[test]
    fn directions_match_the_scene_sun_and_invert() {
        for (a, e) in [(315.0, 45.0), (0.0, 35.0), (90.0, 10.0), (200.0, 80.0)] {
            let d = crate::scene::sun_direction(a, e, f32::sin, f32::cos);
            assert!(close(light_direction(a, e), [d.x, d.z, -d.y]));
            let (az, el) = angles(light_direction(a, e));
            assert!(
                (az - a).abs() < 1e-2 && (el - e).abs() < 1e-2,
                "{a} {e}: {az} {el}"
            );
        }
    }

    #[test]
    fn unset_settings_keep_the_map() {
        let m = map();
        let live = resolve(&m, &Settings::default(), 12345.0);
        assert_eq!(live.sun_direction, m.sun_direction);
        assert_eq!(live.direct_light, m.direct_light);
        assert_eq!(live.ambient_light, m.ambient_light);
        assert_eq!((live.fog_start, live.fog_end), (100.0, 400.0));
        assert_eq!(live.fog_color, m.fog_color);
        assert_eq!(live.sky_tint, [1.0; 3]);
        assert_eq!(live.flare.0[3], 0.0);
        assert_eq!(live.shadow_color, None);
    }

    #[test]
    fn distances_keep_the_fade_and_give_fogless_maps_fog() {
        let s = Settings {
            visible_distance: Some(200.0),
            ..Default::default()
        };
        let live = resolve(&map(), &s, 0.0);
        assert_eq!((live.fog_start, live.fog_end), (50.0, 200.0));
        let fogless = Authored {
            fog_end: 0.0,
            fog_start: 0.0,
            ..map()
        };
        let live = resolve(&fogless, &s, 0.0);
        assert_eq!((live.fog_start, live.fog_end), (100.0, 200.0));
        let s = Settings {
            fog_distance: Some(500.0),
            ..s
        };
        // Fog never starts past where it is complete.
        assert_eq!(resolve(&map(), &s, 0.0).fog_start, 200.0);
    }

    #[test]
    fn a_day_rises_peaks_sets_and_brings_the_moon() {
        let length = 100.0;
        let s = Settings {
            sun_azimuth: Some(90.0),
            sun_elevation: Some(60.0),
            day_cycle: Some(DayCycle {
                length_seconds: length,
                time: 0.5,
                anchor_tick: 1000,
            }),
            ..Default::default()
        };
        let tick = |time: f64| 1000.0 + ((time - 0.5).rem_euclid(1.0)) * 100.0 * 120.0;
        let noon = resolve(&map(), &s, tick(0.5));
        // Noon: the set angles and colours.
        let (az, el) = angles(noon.sun_direction);
        assert!(
            (az - 90.0).abs() < 0.5 && (el - 60.0).abs() < 0.5,
            "{az} {el}"
        );
        assert!(close(noon.direct_light, map().direct_light));
        assert!(close(noon.ambient_light, map().ambient_light));
        // Sunrise and sunset sit on the horizon, warm and dim.
        for time in [0.25, 0.75] {
            let live = resolve(&map(), &s, tick(time));
            assert!(angles(live.sun_direction).1.abs() < 1.0);
            assert!(live.direct_light[0] > live.direct_light[2]);
        }
        // Midnight: moonlight from above, dark sky.
        let night = resolve(&map(), &s, tick(0.0));
        assert!(angles(night.sun_direction).1 > 30.0);
        assert!(close(night.direct_light, MOON));
        assert!(night.sky_tint.iter().all(|c| *c < 0.3));
        assert!(night.ambient_light[0] < map().ambient_light[0] * 0.5);
    }

    #[test]
    fn the_sun_turns_every_frame() {
        let cycle = DayCycle {
            length_seconds: 300.0,
            time: 0.3,
            anchor_tick: 50,
        };
        let s = Settings {
            day_cycle: Some(cycle),
            ..Default::default()
        };
        // A second of 60 fps frames, between server ticks: every frame's
        // sun is new and only a sliver past the last one, never a jump.
        let frame = TICKS_PER_SECOND as f64 / 60.0;
        let mut last = resolve(&map(), &s, 50.0).sun_direction;
        for i in 1..=60 {
            let now = resolve(&map(), &s, 50.0 + f64::from(i) * frame).sun_direction;
            // The angle between them, from their cross product (an arc
            // cosine loses so small an angle in f32).
            let cross = [
                now[1] * last[2] - now[2] * last[1],
                now[2] * last[0] - now[0] * last[2],
                now[0] * last[1] - now[1] * last[0],
            ];
            let degrees = cross
                .iter()
                .map(|c| c * c)
                .sum::<f32>()
                .sqrt()
                .asin()
                .to_degrees();
            assert!(
                degrees > 0.005 && degrees < 0.05,
                "frame {i}: {degrees} degrees"
            );
            last = now;
        }
        // Time runs from the anchor, wrapping at a day.
        assert!((cycle.time_at(50.0) - 0.3).abs() < 1e-6);
        assert!((cycle.time_at(50.0 + 300.0 * 120.0) - 0.3).abs() < 1e-6);
        assert!((cycle.time_at(50.0 + 150.0 * 120.0) - 0.8).abs() < 1e-6);
        assert!((cycle.time_at(50.5) - cycle.time_at(50.0)) > 0.0);
    }

    #[test]
    fn limits_and_merge() {
        let mut s = Settings {
            sun_azimuth: Some(10.0),
            ..Default::default()
        };
        s.validate().unwrap();
        s.merge(&Settings {
            fog_color: Some([0.1, 0.2, 0.3]),
            ..Default::default()
        });
        assert_eq!(
            (s.sun_azimuth, s.fog_color),
            (Some(10.0), Some([0.1, 0.2, 0.3]))
        );
        for bad in [
            Settings {
                sun_azimuth: Some(400.0),
                ..Default::default()
            },
            Settings {
                ambient_light: Some([2.0, 0.0, 0.0]),
                ..Default::default()
            },
            Settings {
                visible_distance: Some(f32::NAN),
                ..Default::default()
            },
            Settings {
                day_cycle: Some(DayCycle {
                    length_seconds: 1.0,
                    time: 0.0,
                    anchor_tick: 0,
                }),
                ..Default::default()
            },
        ] {
            assert!(bad.validate().is_err());
        }
        for (_, preset) in presets() {
            preset.validate().unwrap();
        }
    }
}
