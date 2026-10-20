//! Distance fog and zone fog (Engine.dll `FLevelSceneNode::Render` 0x1ca970, `SetDistanceFog`
//! in D3DDrv.dll 0x17cf0; see `specs/fog.md`, port rules 1 to 12). A zone (or a physics
//! volume) states a linear fog; the main view blends toward the camera zone's fog over its
//! blend time; the driver fades each pixel toward the fog colour by view depth.
//!
//! Colours are plain 0..255 RGB (the original stores them B, G, R, A).

#![allow(clippy::manual_clamp)]

/// The far-plane value when there is no fog (`FAR_CLIPPING_PLANE`).
pub const FAR: f32 = 65536.0;
/// A time that is past any blend (the original's 1e6 snap).
const DONE: f32 = 1.0e6;

pub type Rgb = [u8; 3];

/// A fog colour, start and end (the normal set or the night-vision set).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogSet {
    pub color: Rgb,
    pub start: f32,
    pub end: f32,
}

/// The fog properties of a `ZoneInfo` (also `LevelInfo` and `SkyZoneInfo`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneFog {
    pub distance_fog: bool,
    pub clear_to_fog_color: bool,
    pub normal: FogSet,
    pub night_vision: FogSet,
    pub blend_time: f32,
    /// `DistanceFogEndMin`: the nearest the fog end may be pulled by the fog-distance option.
    pub end_min: f32,
    /// `RealDistanceFogEnd`: the authored end, kept by [`init_lod`].
    pub real_end: f32,
}

impl Default for ZoneFog {
    fn default() -> Self {
        let set = FogSet {
            color: [128, 128, 128],
            start: 3000.0,
            end: 8000.0,
        };
        Self {
            distance_fog: false,
            clear_to_fog_color: false,
            normal: set,
            night_vision: set,
            blend_time: 0.0,
            end_min: 0.0,
            real_end: set.end,
        }
    }
}

impl ZoneFog {
    fn set(&self, night_vision: bool) -> FogSet {
        if night_vision {
            self.night_vision
        } else {
            self.normal
        }
    }
}

/// The fog properties of a `PhysicsVolume` (nothing authored by default; no shipped map sets
/// any).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeFog {
    pub distance_fog: bool,
    pub normal: FogSet,
    pub night_vision: FogSet,
}

/// What the driver is told, plus what the scene derives from the same zone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fog {
    pub enabled: bool,
    pub start: f32,
    pub end: f32,
    pub color: Rgb,
}

impl Fog {
    pub const OFF: Fog = Fog {
        enabled: false,
        start: 0.0,
        end: 0.0,
        color: [0; 3],
    };

    /// `SetDistanceFog`'s range fix: start and end closer than 1 become `max(end - 1, 0)`
    /// to `end`.
    pub fn driver_range(&self) -> (f32, f32) {
        if (self.start - self.end).abs() < 1.0 {
            ((self.end - 1.0).max(0.0), self.end)
        } else {
            (self.start, self.end)
        }
    }

    /// The share of the pixel colour that stays at view depth `depth`:
    /// `clamp((end - d) / (end - start), 0, 1)`.
    pub fn factor(&self, depth: f32) -> f32 {
        let (start, end) = self.driver_range();
        ((end - depth) / (end - start)).max(0.0).min(1.0)
    }
}

/// What one frame's fog selection needs.
#[derive(Clone, Copy, Debug)]
pub struct FogInput<'a> {
    /// The camera's BSP zone number (0: none).
    pub zone_number: i32,
    /// The camera zone's actor (or the level's `LevelInfo` when it has none).
    pub zone: &'a ZoneFog,
    /// The physics volume at the camera, when one exists (main view only).
    pub volume: Option<&'a VolumeFog>,
    pub night_vision: bool,
    /// The glow-filter pass draws its fog black.
    pub glow_pass: bool,
    /// The viewer's show-fog flag (ShowFlags bit 19); volume fog ignores it.
    pub show_fog: bool,
}

/// The result for a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogFrame {
    pub fog: Fog,
    /// The colour to clear the target to, when the zone asks for it.
    pub clear_color: Option<Rgb>,
    /// The camera's far clipping plane.
    pub far_plane: f32,
}

/// A `PlayerController`'s fog blend state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogViewer {
    pub last_zone: i32,
    pub last_color: Rgb,
    pub last_start: f32,
    pub last_end: f32,
    pub current_end: f32,
    pub since_change: f32,
}

impl Default for FogViewer {
    fn default() -> Self {
        Self {
            last_zone: 0,
            last_color: [0; 3],
            last_start: FAR,
            last_end: FAR,
            current_end: FAR,
            since_change: 0.0,
        }
    }
}

impl FogViewer {
    /// `APlayerController::Tick` adds the frame time before rendering.
    pub fn tick(&mut self, dt: f32) {
        self.since_change += dt;
    }

    /// The main view: volume fog if the camera is in a volume with fog, otherwise the camera
    /// zone's fog blended from the last one over the new zone's blend time.
    pub fn main_view(&mut self, i: &FogInput<'_>) -> FogFrame {
        if let Some(v) = i.volume.filter(|v| v.distance_fog) {
            let set = if i.night_vision {
                v.night_vision
            } else {
                v.normal
            };
            let color = if i.glow_pass { [0; 3] } else { set.color };
            return FogFrame {
                fog: Fog {
                    enabled: true,
                    start: set.start,
                    end: set.end,
                    color,
                },
                clear_color: None,
                far_plane: FAR,
            };
        }
        let zone = i.zone;
        let set = zone.set(i.night_vision);
        if self.last_zone != i.zone_number {
            if zone.distance_fog && self.last_start == FAR {
                self.last_color = zone.normal.color;
            }
            self.since_change = if self.last_zone == 0 { DONE } else { 0.0 };
            self.last_zone = i.zone_number;
        }
        let mut ratio = if zone.blend_time == 0.0 {
            1.0
        } else {
            self.since_change / zone.blend_time
        };
        let (target_color, target_start, target_end) = if zone.distance_fog {
            (set.color, set.start, set.end)
        } else {
            (self.last_color, FAR, FAR)
        };
        if ratio > 1.0 || i.zone_number == 0 {
            self.last_color = target_color;
            self.last_start = target_start;
            self.last_end = target_end;
            self.since_change = DONE;
            ratio = 1.0;
        }
        let start = self.last_start + (target_start - self.last_start) * ratio;
        let end = self.last_end + (target_end - self.last_end) * ratio;
        self.current_end = end;
        let mut color = [0u8; 3];
        for k in 0..3 {
            let (t, l) = (f32::from(target_color[k]), f32::from(self.last_color[k]));
            color[k] = ((t - l) * ratio + l) as u8;
        }
        if i.glow_pass {
            color = [0; 3];
        }
        let enabled = (zone.distance_fog || ratio < 1.0) && i.show_fog;
        let clear_color = (zone.clear_to_fog_color && zone.distance_fog).then_some(set.color);
        let far_plane = if zone.distance_fog && i.show_fog {
            self.current_end.max(zone.normal.end)
        } else {
            FAR
        };
        FogFrame {
            fog: Fog {
                enabled,
                start,
                end,
                color,
            },
            clear_color,
            far_plane,
        }
    }
}

/// Nested scenes (the sky zone, mirrors): no blending, the zone's values as they are.
pub fn static_fog(i: &FogInput<'_>) -> Fog {
    let set = i.zone.set(i.night_vision);
    Fog {
        enabled: i.zone.distance_fog && i.show_fog,
        start: set.start,
        end: set.end,
        color: if i.glow_pass { [0; 3] } else { set.color },
    }
}

/// `InitDistanceFogLOD`: remember each zone's authored end.
pub fn init_lod(zones: &mut [ZoneFog]) {
    for z in zones {
        z.real_end = z.normal.end;
    }
}

/// `UpdateDistanceFogLOD(lod)`: pull each end toward `DistanceFogEndMin` by the fog-distance
/// option (client or standalone games only; 1 changes nothing).
pub fn update_lod(zones: &mut [ZoneFog], lod: f32) {
    let l = lod.max(0.0).min(1.0);
    for z in zones {
        let lo = if z.end_min > 0.0 {
            z.end_min
        } else {
            z.real_end
        };
        z.normal.end = lo + (z.real_end - lo) * l;
    }
}
