//! Native options and input capture. Script strings identify authored widgets;
//! they are never evaluated. Settings that only configured Torque's renderer,
//! audio drivers or network stack are hidden and the remaining authored rows
//! close up, so every visible control does something.
use super::*;
use crate::api::{BindInput, DisplayModes, UiAction};
use crate::binds::{BindMap, RemapOutcome};
use crate::input::Chord;
use crate::prefs::Prefs;
use crate::ui::Callback;
use crate::view::EventKind;
use std::collections::HashMap;

const FULLSCREEN: &str = "$pref::Video::fullScreen";
const NO_VSYNC: &str = "$pref::Video::disableVerticalSync";
const RESOLUTION: &str = "$pref::Video::resolution";
/// v20's stock display defaults (800x600, VSync off) described the machines
/// of 2009, not this one. They are not defaults here, so a first Options
/// visit shows the real window and Done does not store them for next launch.
pub const MACHINE_PREFS: &[&str] = &[
    FULLSCREEN,
    NO_VSYNC,
    RESOLUTION,
    "$pref::Video::windowedRes",
];
pub const CHAT_SIZE: &str = "$Pref::Gui::ChatSize";
pub const KEYBOARD_TURN_SPEED: &str = "$pref::Input::KeyboardTurnSpeed";
const ANISOTROPY: &str = "$pref::OpenGL::anisotropy";
const SHADOW_QUALITY: &str = "$pref::ShadowQuality";
/// Not a v20 setting: 4x MSAA, on unless turned off.
const ANTI_ALIASING: &str = "$pref::Video::AntiAliasing";
/// Not a v20 setting (v20 shadowed only players, vehicles and items): bricks
/// cast sun shadows too, off unless turned on.
const BRICK_SHADOWS: &str = "$pref::Video::BrickShadows";
const SHADOW_RADIO: &str = "OPT_ShadowQuality";
/// v20's Physics Quality radios (0 Best .. 4 Off; the stock default is 1,
/// High): how many knocked-out bricks tumble as debris at once.
pub const PHYSICS_QUALITY: &str = "$pref::PhysicsQuality";
const PHYSICS_RADIO: &str = "OPT_PhysicsQuality";
/// v20's `$pref::Physics::MaxBricks`: the debris limit itself. A radio sets
/// it to that quality's limit; the console sets any other number.
pub const MAX_BRICKS: &str = "$pref::Physics::MaxBricks";
/// Debris bodies alive at once for each Physics Quality, Best first. Off
/// leaves no debris: dead bricks just vanish.
pub const PHYSICS_LIMITS: [i64; 5] = [2048, 512, 256, 128, 0];
/// The most debris `$pref::Physics::MaxBricks` may ask for.
pub const MAX_BRICKS_RANGE: (i64, i64) = (0, 4096);
/// The debris limit the player chose: `$pref::Physics::MaxBricks` when they
/// set it, else their Physics Quality's.
pub fn debris_limit(p: &Prefs) -> usize {
    let limit = if p.is_set(MAX_BRICKS) {
        p.i64_or(MAX_BRICKS, PHYSICS_LIMITS[1])
    } else {
        PHYSICS_LIMITS[physics_quality(p)]
    };
    limit.clamp(MAX_BRICKS_RANGE.0, MAX_BRICKS_RANGE.1) as usize
}
fn physics_quality(p: &Prefs) -> usize {
    p.i64_or(PHYSICS_QUALITY, 1).clamp(0, 4) as usize
}
const PRECIPITATION: &str = "$pref::precipitationOn";
/// Not a v20 setting: the frame-rate cap in frames per second, 0 for none.
pub const MAX_FPS: &str = "$pref::Video::MaxFps";
/// The Max FPS menu's choices; 0 is Unlimited.
pub const MAX_FPS_CHOICES: &[u32] = &[30, 60, 75, 120, 144, 165, 240, 0];
const MAX_FPS_MENU: &str = "OptGraphicsMaxFpsMenu";
/// Not a v20 setting: colour-vision assistance for the 3D view (0 off,
/// 1 protanopia, 2 deuteranopia, 3 tritanopia).
pub const COLOR_VISION: &str = "$pref::Gui::ColorVision";
const COLOR_VISION_MENU: &str = "OptGraphicsColorVisionMenu";
const COLOR_VISION_CHOICES: [&str; 4] = ["Off", "Red-weak", "Green-weak", "Blue-weak"];
/// The colour-vision assistance `$pref::Gui::ColorVision` asks for.
pub fn color_vision(p: &Prefs) -> u32 {
    p.i64_or(COLOR_VISION, 0).clamp(0, 3) as u32
}
/// The UI Size menu (`$pref::Gui::Scale`, percent; 0 is Auto).
const UI_SCALE_MENU: &str = "OptGraphicsUiScaleMenu";
pub const UI_SCALE_CHOICES: &[i64] = &[0, 100, 125, 150, 200, 250, 300];
const QUALITY_MENU: &str = "OptGraphicsQualityMenu";
/// Not a v20 setting (v20 had no mirrors): mirrors that reflect live, 0 Off
/// through 3 High, 2 unless chosen. The client's graphics settings read it.
pub const REFLECTIONS: &str = "$pref::Video::Reflections";
const REFLECTIONS_MENU: &str = "OptGraphicsReflectionsMenu";
const REFLECTIONS_CHOICES: [&str; 4] = ["Off", "Low", "Medium", "High"];
/// The Reflections level `$pref::Video::Reflections` asks for.
pub fn reflections(p: &Prefs) -> i64 {
    p.i64_or(REFLECTIONS, 2).clamp(0, 3)
}
/// Not a v20 setting: how maps and what stands on them are lit. 0 Classic
/// (v20: baked maps, sun-lit bricks), 1 Unified (bricks share the map's
/// recovered lights, sun and shadows), 2 Unified with highlights, the
/// default, 3 Dynamic (2, with the map's own surfaces lit live by those
/// lights instead of its baked lightmaps). The client's graphics settings
/// read it.
pub const LIGHTING: &str = "$pref::Video::Lighting";
const LIGHTING_MENU: &str = "OptGraphicsLightingMenu";
const LIGHTING_CHOICES: [&str; 4] = ["Classic", "Unified", "Unified+Shine", "Dynamic"];
/// The lighting mode `$pref::Video::Lighting` asks for.
pub fn lighting(p: &Prefs) -> i64 {
    p.i64_or(LIGHTING, 2).clamp(0, LIGHTING_CHOICES.len() as i64 - 1)
}
/// Not a v20 setting: music bricks' volume (v20 only had Play Music).
pub const MUSIC_VOLUME: &str = "$pref::Audio::musicVolume";
/// Not a v20 setting: silence the game while another window has focus.
pub const MUTE_IN_BACKGROUND: &str = "$pref::Audio::MuteInBackground";
/// Not a v20 setting: short captions for game sounds ("[Explosion]").
pub const CAPTIONS: &str = "$pref::Audio::Captions";
/// v20's Advanced "Max Draw Distance" (`SliderGraphicsDistanceMax`): caps a
/// map's visible distance, 110 to 1000 units, 1000 by default.
pub const VISIBLE_DISTANCE_MAX: &str = "$pref::visibleDistanceMax";
const DISTANCE_SLIDER: &str = "SliderGraphicsDistanceMax";
const VISIBLE_DISTANCE_RANGE: (f32, f32) = (110.0, 1000.0);
/// The draw distance cap `$pref::visibleDistanceMax` asks for.
pub fn visible_distance_max(p: &Prefs) -> f32 {
    let v = p.f32_or(VISIBLE_DISTANCE_MAX, VISIBLE_DISTANCE_RANGE.1);
    if v.is_finite() {
        v.clamp(VISIBLE_DISTANCE_RANGE.0, VISIBLE_DISTANCE_RANGE.1)
    } else {
        VISIBLE_DISTANCE_RANGE.1
    }
}
/// Not a v20 setting: Crouch toggles instead of holding.
pub const TOGGLE_CROUCH: &str = "$pref::Input::ToggleCrouch";
const MUSIC_SLIDER: &str = "OptAudioVolumeMusic";
/// The renderer's anisotropy when the player never set one (8x, as
/// `bri_render::scene::TextureFiltering::default`).
const DEFAULT_ANISOTROPY: f32 = 7.0 / 15.0;

/// One Graphics Quality choice. Each sets the options that cost the most
/// frame time; the menu shows Custom when they match no preset.
struct Preset {
    name: &'static str,
    shadows: i64,
    anti_aliasing: bool,
    brick_shadows: bool,
    anisotropy: f32,
    precipitation: bool,
    reflections: i64,
}
/// High is the renderer's defaults, so a new player sees High.
const PRESETS: &[Preset] = &[
    Preset {
        name: "Low",
        shadows: 4,
        anti_aliasing: false,
        brick_shadows: false,
        anisotropy: 0.0,
        precipitation: false,
        reflections: 0,
    },
    Preset {
        name: "Medium",
        shadows: 2,
        anti_aliasing: true,
        brick_shadows: false,
        anisotropy: 3.0 / 15.0,
        precipitation: true,
        reflections: 1,
    },
    Preset {
        name: "High",
        shadows: 0,
        anti_aliasing: true,
        brick_shadows: false,
        anisotropy: DEFAULT_ANISOTROPY,
        precipitation: true,
        reflections: 2,
    },
    Preset {
        name: "Ultra",
        shadows: 0,
        anti_aliasing: true,
        brick_shadows: true,
        anisotropy: 1.0,
        precipitation: true,
        reflections: 3,
    },
];
/// The Quality menu id for "Custom".
const CUSTOM_QUALITY: i64 = PRESETS.len() as i64;
/// Preferences a preset writes. Done always stores them, even when they
/// equal a stock default the renderer does not read.
const PRESET_PREFS: &[&str] = &[
    SHADOW_QUALITY,
    ANTI_ALIASING,
    BRICK_SHADOWS,
    ANISOTROPY,
    PRECIPITATION,
    REFLECTIONS,
];
/// `$pref::Player::defaultFov`, the normal camera FOV in degrees (v20
/// default 90). The B4v21 patch of the reference v20 install adds its slider.
pub const DEFAULT_FOV: &str = "$pref::Player::defaultFov";
/// The patch's `SliderFOV` range; `validateFOV` rounds to whole degrees.
pub const FOV_RANGE: (f32, f32) = (70.0, 140.0);
const FOV_SLIDER: &str = "SliderFOV";
/// Not a v20 setting: look for a newer release once per start (on unless
/// turned off). The client's update check reads it.
pub const CHECK_FOR_UPDATES: &str = "$pref::Net::CheckForUpdates";
/// Controls' "Invert Mouse In Vehicles"; the client reads it while driving
/// a mouse-steered vehicle. On by default, as stock v20's
/// `client/defaults.cs` ships it: moving the mouse up dips a plane's nose.
/// The reference install and v21 ship it off; Maxwell's v0.1.3 test with it
/// off reported the plane's pitch inverted.
pub const VEHICLE_MOUSE_INVERT: &str = "$Pref::Input::VehicleMouseInvert";
/// `$pref::Input::UseStrafeSteering`: the strafe keys steer a vehicle that
/// allows it, and the mouse looks around; off, the mouse steers it.
pub const USE_STRAFE_STEERING: &str = "$pref::Input::UseStrafeSteering";
/// `$pref::Input::UseAutoReturnSteering`.
pub const USE_AUTO_RETURN_STEERING: &str = "$pref::Input::UseAutoReturnSteering";
/// Defaults that replace the UI pack's, which come from stock v20's
/// `client/defaults.cs` (both steering prefs 1). The designated reference
/// install (`base/client/defaults.cs`) and Maxwell's own v20 prefs ship
/// both 0: a Jeep's driver steers with the mouse, as Maxwell expects.
pub const NATIVE_DEFAULTS: &[(&str, &str)] = &[
    (USE_STRAFE_STEERING, "0"),
    (USE_AUTO_RETURN_STEERING, "0"),
];
/// Checkboxes whose v20 default is on.
const DEFAULT_ON: &[&str] = &[
    "$pref::OpenGL::textureTrilinear",
    VEHICLE_MOUSE_INVERT,
    ANTI_ALIASING,
    PRECIPITATION,
    CHECK_FOR_UPDATES,
];
/// Checkbox preferences the native game honours.
const CHECKBOX_PREFS: &[&str] = &[
    FULLSCREEN,
    NO_VSYNC,
    PRECIPITATION,
    "$pref::OpenGL::textureTrilinear",
    "$pref::OpenGL::useGLNearest",
    ANTI_ALIASING,
    BRICK_SHADOWS,
    CHECK_FOR_UPDATES,
    "$Pref::Audio::PlayMusic",
    "$Pref::Audio::MenuSounds",
    "$Pref::Audio::PlayBrickPlantSound",
    "$Pref::Audio::PlayBrickMoveSound",
    "$Pref::Audio::PlantErrorSound",
    "$pref::HUD::showToolTips",
    "$pref::HUD::HidePaintBox",
    "$pref::HUD::HideToolBox",
    "$pref::HUD::HideBrickBox",
    "$pref::Hud::RecolorBrickIcons",
    "$pref::Gui::ShowBrickSlotNumbers",
    "$Pref::Gui::ColorEscapeMenu",
    super::play::SMALL_PLANT_ERRORS,
    "$pref::Input::FastFirstThirdPerson",
    "$pref::Input::UseSuperShiftSmartToggle",
    "$pref::Input::UseSuperShiftToggle",
    "$pref::Input::QueueBrickBuying",
    "$pref::Input::ReverseBrickScroll",
    "$pref::Input::noobjet",
    "$pref::Input::MouseInvert",
    VEHICLE_MOUSE_INVERT,
    MUTE_IN_BACKGROUND,
    TOGGLE_CROUCH,
    CAPTIONS,
    "$Pref::Chat::CurseFilter",
    "$pref::Chat::ChatRepeat",
    "$pref::Input::AutoLight",
    TEMP_BRICK_OUTSIDE_PAINT,
    TEMP_BRICK_INSIDE_PAINT,
    // v20 authored Render Items outside the Advanced pane, where nobody
    // could reach it; the game still honours the pref.
    "$pref::Player::renderMyJets",
    // Sent to the host (`SteeringPrefsEvent`); final touches builds them.
    USE_STRAFE_STEERING,
    USE_AUTO_RETURN_STEERING,
];
/// Advanced's temp brick rows: the ghost's outside and inside colours come
/// from the paint can unless these are off (`OptionsDlg::UpdateTempBrickBlockers`).
pub const TEMP_BRICK_OUTSIDE_PAINT: &str = "$pref::HUD::tempBrickOutsideUsePaintColor";
pub const TEMP_BRICK_INSIDE_PAINT: &str = "$pref::HUD::tempBrickInsideUsePaintColor";
/// The temp brick's number fields: (control, label, pref, min, max), clamped
/// as `optionsDlg::apply` did.
const TEMP_BRICK_FIELDS: &[(&str, &str, &str, f32, f32)] = &[
    (
        "Opt_TempBrickFlashTime",
        "Temp Brick Flash Time",
        "$pref::HUD::tempBrickFlashTime",
        100.0,
        10000.0,
    ),
    (
        "Opt_TempBrickFlashRange",
        "Temp Brick Flash Range",
        "$pref::HUD::tempBrickFlashRange",
        0.0,
        1.0,
    ),
    (
        "Opt_TempBrickFlashOffset",
        "Temp Brick Flash Offset",
        "$pref::HUD::tempBrickFlashoffset",
        0.0,
        1.0,
    ),
    (
        "Opt_TempBrickOutsideRed",
        "Temp Brick Outside Color",
        "$pref::HUD::tempBrickOutsideRed",
        0.0,
        1.0,
    ),
    (
        "Opt_TempBrickOutsideGreen",
        "Temp Brick Outside Color",
        "$pref::HUD::tempBrickOutsideGreen",
        0.0,
        1.0,
    ),
    (
        "Opt_TempBrickOutsideBlue",
        "Temp Brick Outside Color",
        "$pref::HUD::tempBrickOutsideBlue",
        0.0,
        1.0,
    ),
    (
        "Opt_TempBrickInsideRed",
        "Temp Brick Inside Color",
        "$pref::HUD::tempBrickInsideRed",
        0.0,
        1.0,
    ),
    (
        "Opt_TempBrickInsideGreen",
        "Temp Brick Inside Color",
        "$pref::HUD::tempBrickInsideGreen",
        0.0,
        1.0,
    ),
    (
        "Opt_TempBrickInsideBlue",
        "Temp Brick Inside Color",
        "$pref::HUD::tempBrickInsideBlue",
        0.0,
        1.0,
    ),
];
/// Other authored controls with native behaviour.
const SUPPORTED_CONTROLS: &[&str] = &[
    "OptGraphicsResolutionMenu",
    "OptAudioVolumeMaster",
    "OptAudioVolumeShell",
    "OptAudioVolumeSim",
    "SliderControlsMouseSensitivity",
    "slider_KeyboardTurnSpeed",
    "Opt_ChatLineTime",
    "Opt_MaxChatLines",
    "Opt_TempBrickFlashTime",
    "Opt_TempBrickFlashRange",
    "Opt_TempBrickFlashOffset",
    "Opt_TempBrickOutsideRed",
    "Opt_TempBrickOutsideGreen",
    "Opt_TempBrickOutsideBlue",
    "Opt_TempBrickInsideRed",
    "Opt_TempBrickInsideGreen",
    "Opt_TempBrickInsideBlue",
    "OptRemapList",
    "SliderGraphicsAnisotropy",
    DISTANCE_SLIDER,
    FOV_SLIDER,
];
const CHAT_SIZE_RADIO: &str = "OPT_ChatSize";
const VALUE_CLASSES: &[&str] = &[
    "GuiCheckBoxCtrl",
    "GuiRadioCtrl",
    "GuiSliderCtrl",
    "GuiPopUpMenuCtrl",
    "GuiTextEditCtrl",
    "GuiTextListCtrl",
];
pub(crate) const VOLUMES: &[(&str, &str, &str)] = &[
    (
        "OptAudioVolumeMaster",
        "$pref::Audio::masterVolume",
        "master",
    ),
    (
        "OptAudioVolumeShell",
        "$pref::Audio::channelVolume1",
        "shell",
    ),
    ("OptAudioVolumeSim", "$pref::Audio::channelVolume2", "sim"),
    (MUSIC_SLIDER, MUSIC_VOLUME, "music"),
];
/// Sliders that show their value to their right.
const READOUTS: &[&str] = &[
    "OptAudioVolumeMaster",
    "OptAudioVolumeShell",
    "OptAudioVolumeSim",
    MUSIC_SLIDER,
    "SliderControlsMouseSensitivity",
    "slider_KeyboardTurnSpeed",
    "SliderGraphicsAnisotropy",
    DISTANCE_SLIDER,
    FOV_SLIDER,
];

/// The text a slider's readout shows for `value`.
fn readout(slider: &str, value: f32) -> String {
    match slider {
        FOV_SLIDER | DISTANCE_SLIDER => format!("{value:.0}"),
        "SliderGraphicsAnisotropy" => {
            // As `TextureFiltering::from_v20` rounds it.
            let samples = 1.0 + value.clamp(0.0, 1.0) * 15.0;
            match [16, 8, 4, 2].into_iter().find(|n| samples >= *n as f32) {
                Some(n) => format!("{n}x"),
                None => "Off".into(),
            }
        }
        "SliderControlsMouseSensitivity" | "slider_KeyboardTurnSpeed" => format!("{value:.2}"),
        _ => format!("{:.0}%", value.clamp(0.0, 1.0) * 100.0),
    }
}

/// A volume pref as a gain in 0..=1 (full when unset or unreadable).
pub fn volume(p: &Prefs, pref: &str) -> f32 {
    let v = p.f32_or(pref, 1.0);
    if v.is_finite() { v.clamp(0.0, 1.0) } else { 1.0 }
}

/// The frame-rate cap `$pref::Video::MaxFps` asks for, `None` for none.
pub fn max_fps(p: &Prefs) -> Option<u32> {
    let fps = p.i64_or(MAX_FPS, 0);
    (fps > 0).then(|| fps.clamp(15, 1000) as u32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DisplaySettings {
    resolution: (u32, u32),
    fullscreen: bool,
    vsync: bool,
}

fn display(p: &Prefs, fallback: (i32, i32)) -> DisplaySettings {
    let words: Vec<_> = p.str_or(RESOLUTION, "").split_whitespace().collect();
    let resolution = words
        .first()
        .and_then(|w| w.parse().ok())
        .zip(words.get(1).and_then(|h| h.parse().ok()))
        .filter(|&(w, h)| w >= 640 && h >= 480)
        .unwrap_or((fallback.0.max(640) as u32, fallback.1.max(480) as u32));
    DisplaySettings {
        resolution,
        fullscreen: p.bool_or(FULLSCREEN, false),
        vsync: !p.bool_or(NO_VSYNC, false),
    }
}
/// A display change the platform made itself (Alt+Enter, or a saved mode
/// the monitor cannot show), kept so the next launch matches it.
pub fn record_display(p: &mut Prefs, resolution: (u32, u32), fullscreen: bool) {
    let vsync = !p.bool_or(NO_VSYNC, false);
    put_display(
        p,
        DisplaySettings {
            resolution,
            fullscreen,
            vsync,
        },
    );
}

/// Headless and pre-window fallback when the platform has not reported the
/// monitor's modes.
const FALLBACK_RESOLUTIONS: &[(u32, u32)] = &[
    (640, 480),
    (800, 600),
    (1024, 768),
    (1280, 720),
    (1280, 800),
    (1366, 768),
    (1600, 900),
    (1920, 1080),
    (2560, 1440),
    (3840, 2160),
];

/// v20's `OptGraphicsResolutionMenu::init`: the list depends on the
/// fullscreen toggle. Fullscreen is borderless at the monitor's own size;
/// windowed sizes come from the monitor's modes that fit on the desktop.
fn resolution_list(
    modes: Option<&DisplayModes>,
    fullscreen: bool,
    current: (u32, u32),
) -> Vec<(u32, u32)> {
    let mut list = match modes {
        Some(m) if fullscreen => vec![m.native],
        Some(m) if !m.windowed.is_empty() => m.windowed.clone(),
        _ => {
            let mut l = FALLBACK_RESOLUTIONS.to_vec();
            l.push(current);
            l
        }
    };
    list.sort_unstable();
    list.dedup();
    list
}

fn put_display(p: &mut Prefs, d: DisplaySettings) {
    p.set(
        RESOLUTION,
        format!("{} {} 32", d.resolution.0, d.resolution.1),
    );
    p.set_bool(FULLSCREEN, d.fullscreen);
    p.set_bool(NO_VSYNC, !d.vsync);
}

/// The normal camera FOV in whole degrees within the slider's range.
pub fn default_fov(p: &Prefs) -> f32 {
    let fov = p.f32_or(DEFAULT_FOV, 90.0);
    if fov.is_finite() {
        fov.round().clamp(FOV_RANGE.0, FOV_RANGE.1)
    } else {
        90.0
    }
}

/// `$pref::Input::MouseSensitivity`: v20 default 0.75, the Options slider's
/// range. Every reader and writer uses this range.
pub const MOUSE_SENSITIVITY: &str = "$pref::Input::MouseSensitivity";
pub const MOUSE_SENSITIVITY_RANGE: (f32, f32) = (0.02, 2.0);
pub fn mouse_sensitivity(p: &Prefs) -> f32 {
    let v = p.f32_or(MOUSE_SENSITIVITY, 0.75);
    if v.is_finite() {
        v.clamp(MOUSE_SENSITIVITY_RANGE.0, MOUSE_SENSITIVITY_RANGE.1)
    } else {
        0.75
    }
}

/// `$Pref::Chat::MaxDisplayLines`: v20 default 8, the Options field's range.
pub const CHAT_LINES: &str = "$Pref::Chat::MaxDisplayLines";
pub const CHAT_LINES_RANGE: (i64, i64) = (4, 100);
pub fn chat_lines(p: &Prefs) -> usize {
    p.i64_or(CHAT_LINES, 8)
        .clamp(CHAT_LINES_RANGE.0, CHAT_LINES_RANGE.1) as usize
}

/// `$Pref::player::CurrentFOV`, the zoom FOV: v20 default 10; the wheel
/// steps it by 5 within 5–85 while zoomed.
pub const ZOOM_FOV: &str = "$Pref::player::CurrentFOV";
pub const ZOOM_FOV_RANGE: (f32, f32) = (5.0, 85.0);
pub fn zoom_fov(p: &Prefs) -> f32 {
    let v = p.f32_or(ZOOM_FOV, 10.0);
    if v.is_finite() {
        v.clamp(ZOOM_FOV_RANGE.0, ZOOM_FOV_RANGE.1)
    } else {
        10.0
    }
}

/// `$Pref::Gui::ChatSize` (0–10, v20 default 4) selects the chat HUD font
/// profiles `BlockChatTextSize<n>Profile` and friends.
pub fn chat_size(p: &Prefs) -> i64 {
    p.i64_or(CHAT_SIZE, 4).clamp(0, 10)
}

fn supported(v: &View, n: NodeId) -> bool {
    let c = &v.node(n).ctrl;
    let name = c.name.as_deref().unwrap_or_default();
    c.variable
        .as_deref()
        .is_some_and(|var| CHECKBOX_PREFS.iter().any(|p| p.eq_ignore_ascii_case(var)))
        || SUPPORTED_CONTROLS.contains(&name)
        || name.starts_with(CHAT_SIZE_RADIO)
        || name.starts_with(SHADOW_RADIO)
        || name.starts_with(PHYSICS_RADIO)
}

fn is_value(v: &View, n: NodeId) -> bool {
    VALUE_CLASSES.contains(&v.node(n).ctrl.class.as_str())
}

/// Authored option sections are swatches whose first row is a title bar
/// swatch at (2, 2).
fn is_section(v: &View, n: NodeId) -> bool {
    let node = v.node(n);
    node.ctrl.class == "GuiSwatchCtrl"
        && node.children.iter().any(|&k| {
            let c = &v.node(k).ctrl;
            c.class == "GuiSwatchCtrl" && c.position == [2, 2]
        })
}

fn section_title(v: &View, n: NodeId) -> String {
    v.node(n)
        .children
        .iter()
        .find(|&&k| v.node(k).ctrl.class == "GuiTextCtrl" && v.node(k).ctrl.position[1] <= 3)
        .map(|&k| v.text_of(k))
        .unwrap_or_default()
}

fn find_section(v: &View, title: &str) -> Option<NodeId> {
    v.walk()
        .find(|&n| is_section(v, n) && section_title(v, n) == title)
}

fn shows_values(v: &View, n: NodeId) -> bool {
    v.node(n).state.visible
        && (is_value(v, n) || v.node(n).children.iter().any(|&k| shows_values(v, k)))
}

/// A label names the control that starts just right of it on its row.
fn labels(label: &Control, value: &Control) -> bool {
    let gap = value.position[0] - (label.position[0] + label.extent[0]);
    (-4..=24).contains(&gap)
        && label.position[1] < value.position[1] + value.extent[1]
        && value.position[1] < label.position[1] + label.extent[1]
}

/// Hide a section's labels whose control is gone, then move the
/// remaining rows up over the rows that are now empty. Returns the bottom
/// of the visible content in section coordinates.
fn close_rows(v: &mut View, section: NodeId) -> i32 {
    let body: Vec<NodeId> = v
        .node(section)
        .children
        .iter()
        .copied()
        .filter(|&k| v.node(k).ctrl.position[1] > 3)
        .collect();
    for &k in &body {
        if v.node(k).ctrl.class == "GuiTextCtrl"
            && !body.iter().any(|&o| {
                is_value(v, o)
                    && v.node(o).state.visible
                    && labels(&v.node(k).ctrl, &v.node(o).ctrl)
            })
        {
            v.set_visible(k, false);
        }
    }
    let mut rows: Vec<i32> = body.iter().map(|&k| v.node(k).ctrl.position[1]).collect();
    rows.sort_unstable();
    rows.dedup();
    let mut row_shifts = Vec::with_capacity(rows.len());
    let mut shift = 0;
    for (i, &row) in rows.iter().enumerate() {
        row_shifts.push((row, shift));
        let kept = body
            .iter()
            .any(|&k| v.node(k).ctrl.position[1] == row && v.node(k).state.visible);
        if !kept && let Some(next) = rows.get(i + 1) {
            shift += next - row;
        }
    }
    let mut kept: Vec<NodeId> = body
        .iter()
        .copied()
        .filter(|&k| v.node(k).state.visible)
        .collect();
    kept.sort_by_key(|&k| v.node(k).ctrl.position[1]);
    // Closing rows never slides a control onto one above it in the same
    // column (Fullscreen and Vsync, Precipitation and Trilinear): it stays
    // at least the upper one's height below, a checkbox's being its 16-pixel
    // row rather than its generous extent.
    let mut placed: Vec<(NodeId, i32)> = Vec::with_capacity(kept.len());
    for &k in &kept {
        let c = &v.node(k).ctrl;
        let y = c.position[1];
        let mut new_y = y - row_shifts
            .iter()
            .find(|(row, _)| *row == y)
            .map_or(0, |(_, s)| *s);
        let (x, w) = (c.position[0], c.extent[0]);
        for &(p, p_y) in &placed {
            let pc = &v.node(p).ctrl;
            let same_column = pc.position[0] < x + w && x < pc.position[0] + pc.extent[0];
            if same_column && pc.position[1] < y {
                let height = if pc.class == "GuiCheckBoxCtrl" {
                    16
                } else {
                    pc.extent[1]
                };
                new_y = new_y.max(p_y + (y - pc.position[1]).min(height));
            }
        }
        placed.push((k, new_y));
    }
    let mut bottom = 23;
    for (k, y) in placed {
        let c = &mut v.nodes[k].ctrl;
        c.position[1] = y;
        bottom = bottom.max(y + c.extent[1]);
    }
    bottom
}

/// v20 sized each volume label to its own text, ending where its slider
/// starts. The longer names ("Interface Volume") would be clipped at both
/// ends, so the label keeps its right edge and grows to the pane's edge.
fn widen_label(label: &mut Control) {
    let right = label.position[0] + label.extent[0];
    label.position[0] = 1;
    label.extent[0] = (right - 1).max(label.extent[0]);
}

/// Audio additions: v20's Shell and Sim volumes get the names players know,
/// and a Music volume and Mute in Background follow them.
fn audio_rows(v: &mut View) {
    let (Some(shell), Some(sim)) = (v.id("OptAudioVolumeShell"), v.id("OptAudioVolumeSim")) else {
        return;
    };
    let Some(parent) = v.node(sim).parent else {
        return;
    };
    let label_of = |v: &View, slider: NodeId| {
        v.node(parent)
            .children
            .iter()
            .copied()
            .find(|&k| {
                v.node(k).ctrl.class == "GuiTextCtrl" && labels(&v.node(k).ctrl, &v.node(slider).ctrl)
            })
    };
    for (slider, from, to) in [(shell, "Shell", "Interface"), (sim, "Sim", "Effects")] {
        if let Some(l) = label_of(v, slider) {
            let text = v.text_of(l).replace(from, to);
            v.nodes[l].ctrl.text = Some(text);
            widen_label(&mut v.nodes[l].ctrl);
        }
    }
    let (s, sim_ctrl) = (v.node(shell).ctrl.clone(), v.node(sim).ctrl.clone());
    let step = match sim_ctrl.position[1] - s.position[1] {
        d if d > 0 => d,
        _ => sim_ctrl.extent[1] + 12,
    };
    let mut music = sim_ctrl.clone();
    music.name = Some(MUSIC_SLIDER.into());
    music.variable = None;
    music.command = None;
    music.position[1] += step;
    let y = music.position[1];
    if let Some(l) = label_of(v, sim) {
        let mut label = v.node(l).ctrl.clone();
        let text = v.text_of(l).replace("Effects", "Music");
        label.text = Some(if text.contains("Music") { text } else { "Music:".into() });
        label.position[1] += step;
        widen_label(&mut label);
        v.add(parent, label);
    }
    v.add(parent, music);
    let x = label_of(v, sim).map_or(sim_ctrl.position[0], |l| v.node(l).ctrl.position[0]);
    let mut mute = ctrl("GuiCheckBoxCtrl", "GuiCheckBoxProfile", Rect::new(x, y + step, 220, 20));
    // Look like the pane's own checkboxes.
    if let Some(style) = v
        .walk()
        .find(|&n| {
            v.node(n)
                .ctrl
                .variable
                .as_deref()
                .is_some_and(|var| var.eq_ignore_ascii_case("$Pref::Audio::PlayMusic"))
        })
        .map(|n| v.node(n).ctrl.clone())
    {
        mute.style = style.style;
        mute.extent[1] = style.extent[1];
    }
    mute.name = Some("OptAudioMuteInBackground".into());
    mute.variable = Some(MUTE_IN_BACKGROUND.into());
    mute.text = Some("Mute when in background".into());
    let mut captions = mute.clone();
    captions.position[1] += step;
    captions.name = Some("OptAudioCaptions".into());
    captions.variable = Some(CAPTIONS.into());
    captions.text = Some("Show captions for sounds".into());
    v.add(parent, mute);
    v.add(parent, captions);
}

/// Controls addition: a Toggle Crouch checkbox under v20's own input
/// checkboxes, styled like them.
fn input_rows(v: &mut View) {
    let Some(anchor) = v.walk().find(|&n| {
        v.node(n)
            .ctrl
            .variable
            .as_deref()
            .is_some_and(|var| var.eq_ignore_ascii_case("$pref::Input::noobjet"))
    }) else {
        return;
    };
    let Some(parent) = v.node(anchor).parent else {
        return;
    };
    let mut toggle = v.node(anchor).ctrl.clone();
    let bottom = v
        .node(parent)
        .children
        .iter()
        .map(|&k| v.node(k).ctrl.position[1] + v.node(k).ctrl.extent[1])
        .max()
        .unwrap_or(0);
    toggle.position[1] = bottom + 4;
    toggle.extent[0] = toggle.extent[0].max(220);
    toggle.name = Some("OptInputToggleCrouch".into());
    toggle.variable = Some(TOGGLE_CROUCH.into());
    toggle.command = None;
    toggle.text = Some("Toggle crouch (press once)".into());
    v.add(parent, toggle);
    // v20's Options section ends where the pane's sections do; its rows
    // close up to keep Invert Mouse In Vehicles and Toggle Crouch inside.
    let room = PANE_BOTTOM - v.node(parent).ctrl.position[1];
    let checks: Vec<NodeId> = v
        .node(parent)
        .children
        .iter()
        .copied()
        .filter(|&k| v.node(k).state.visible && v.node(k).ctrl.class == "GuiCheckBoxCtrl")
        .collect();
    let mut rows: Vec<i32> = checks.iter().map(|&k| v.node(k).ctrl.position[1]).collect();
    rows.sort_unstable();
    rows.dedup();
    let (Some(&first), Some(&last)) = (rows.first(), rows.last()) else {
        return;
    };
    let height = checks
        .iter()
        .map(|&k| v.node(k).ctrl.extent[1])
        .max()
        .unwrap_or(23);
    if last + height + 4 > room && rows.len() > 1 {
        let step = (room - 4 - height - first) / (rows.len() as i32 - 1);
        for &k in &checks {
            let i = rows
                .iter()
                .position(|&r| r == v.node(k).ctrl.position[1])
                .unwrap_or(0);
            v.nodes[k].ctrl.position[1] = first + step * i as i32;
        }
    }
    let h = &mut v.nodes[parent].ctrl.extent[1];
    *h = (*h).max(room);
}

/// Where v20's option panes end: every tab's sections stop above Done.
const PANE_BOTTOM: i32 = 368;

/// The resolution menu fits "1920 x 1080"; Fullscreen, Vsync and Apply
/// shift right to make room.
fn widen_resolution_menu(v: &mut View) {
    let Some(menu) = v.id("OptGraphicsResolutionMenu") else {
        return;
    };
    let Some(parent) = v.node(menu).parent else {
        return;
    };
    let c = &v.node(menu).ctrl;
    let (right, grow) = (c.position[0] + c.extent[0], 90 - c.extent[0]);
    if grow <= 0 {
        return;
    }
    v.nodes[menu].ctrl.extent[0] += grow;
    for k in v.node(parent).children.clone() {
        let c = &mut v.nodes[k].ctrl;
        if c.position[0] >= right && c.position[1] > 3 {
            c.position[0] += grow;
            if c.class == "GuiButtonCtrl" {
                c.extent[0] -= grow;
            }
        }
    }
}

/// Swatches directly on a pane that frame nothing any more (v20's fillers
/// between the quality sections, emptied driver panels) are hidden.
fn hide_empty_fillers(v: &mut View, pane: NodeId) {
    for k in v.node(pane).children.clone() {
        let n = v.node(k);
        if n.ctrl.class == "GuiSwatchCtrl"
            && n.state.visible
            && !n.children.iter().any(|&c| v.node(c).state.visible)
        {
            v.set_visible(k, false);
        }
    }
}

/// The bottom of a section's visible rows, in section coordinates.
fn content_bottom(v: &View, section: NodeId) -> i32 {
    v.node(section)
        .children
        .iter()
        .filter(|&&k| v.node(k).state.visible)
        .map(|&k| v.node(k).ctrl.position[1] + v.node(k).ctrl.extent[1])
        .max()
        .unwrap_or(0)
}

/// Graphics: Display Settings grows to hold the added rows, and Shadow
/// Quality and Physics Quality (the quality sections left) go side by side
/// under whichever column has room for them, the other column running the
/// full height. Nothing is clipped and no empty band is left where v20's
/// other quality sections were.
fn graphics_pane(v: &mut View) {
    let (Some(pane), Some(display), Some(gui), Some(shadow)) = (
        v.id("OptGraphicsPane"),
        find_section(v, "Display Settings"),
        find_section(v, "Gui Settings"),
        find_section(v, "Shadow Quality"),
    ) else {
        return;
    };
    hide_empty_fillers(v, pane);
    let d = v.node(display).ctrl.clone();
    let g = v.node(gui).ctrl.clone();
    let display_h = d.extent[1].max(content_bottom(v, display) + 8);
    let gui_h = g.extent[1].max(content_bottom(v, gui) + 8);
    let physics = find_section(v, "Physics Quality").filter(|&n| v.node(n).state.visible);
    let shadow_h = physics
        .map_or(0, |p| content_bottom(v, p))
        .max(content_bottom(v, shadow))
        + 8;
    let (above, full) = if d.position[1] + display_h + 3 + shadow_h <= PANE_BOTTOM {
        ((display, d.clone(), display_h), gui)
    } else {
        ((gui, g.clone(), gui_h), display)
    };
    let (column, c, height) = above;
    v.nodes[column].ctrl.extent[1] = height;
    let top = c.position[1] + height + 3;
    let sections: Vec<NodeId> = std::iter::once(shadow).chain(physics).collect();
    let gap = 3;
    let width = (c.extent[0] - gap * (sections.len() as i32 - 1)) / sections.len() as i32;
    for (i, &section) in sections.iter().enumerate() {
        let s = &mut v.nodes[section].ctrl;
        s.position = [c.position[0] + i as i32 * (width + gap), top];
        s.extent = [width, PANE_BOTTOM - top];
        // The title bar spans the section.
        for k in v.node(section).children.clone() {
            let c = &mut v.nodes[k].ctrl;
            if c.class == "GuiSwatchCtrl" && c.position == [2, 2] {
                c.extent[0] = width - 4;
            }
        }
    }
    let f = &mut v.nodes[full].ctrl;
    f.extent[1] = PANE_BOTTOM - f.position[1];
}

/// Audio: the driver panel under Volume is gone; Volume and Audio Options
/// run the full height.
fn audio_pane(v: &mut View) {
    let Some(pane) = v.id("OptAudioPane") else {
        return;
    };
    hide_empty_fillers(v, pane);
    for title in ["Volume", "Audio Options"] {
        if let Some(n) = find_section(v, title) {
            let c = &mut v.nodes[n].ctrl;
            c.extent[1] = PANE_BOTTOM - c.position[1];
        }
    }
}

pub struct Options {
    view: View,
    draft: Prefs,
    initial: Prefs,
    saved_binds: BindMap,
    saved_hardware: (u8, u8),
    applied_display: DisplaySettings,
    pending_display: Option<(RequestId, DisplaySettings, bool)>,
    pending_enabled: Vec<NodeId>,
    resolutions: Vec<(u32, u32)>,
    modes: Option<DisplayModes>,
    committed: bool,
    /// A volume slider was dragged, so its channel plays the draft value.
    previewed: bool,
}

impl Options {
    pub fn new(core: &Core) -> Self {
        let current = display(&core.prefs, core.logical);
        let modes = core.display_modes.clone();
        let resolutions = resolution_list(modes.as_ref(), current.fullscreen, current.resolution);
        let mut s = Self {
            view: layout_view(core, "optionsDlg"),
            draft: core.prefs.clone(),
            initial: core.prefs.clone(),
            saved_binds: core.binds.clone(),
            saved_hardware: (core.settings.mouse_type, core.settings.keyboard_type),
            applied_display: current,
            pending_display: None,
            pending_enabled: Vec::new(),
            resolutions,
            modes,
            committed: false,
            previewed: false,
        };
        s.native_layout();
        // v20 let Options be resized, but its panes' sizing flags only push
        // their sections down; the tabs' fitted layout (native_layout) is
        // for the authored size, so this window keeps it.
        if let Some(n) = window(&s.view) {
            for field in ["resizeWidth", "resizeHeight"] {
                s.view.nodes[n].ctrl.fields.insert(field.into(), "0".into());
            }
        }
        // Duplicate authored names occur throughout Options. Preference identity
        // is the variable on each node, never the last matching widget name.
        for n in s.view.walk().collect::<Vec<_>>() {
            if let Some(var) = s.view.node(n).ctrl.variable.clone()
                && var.starts_with('$')
            {
                let on = DEFAULT_ON.iter().any(|d| d.eq_ignore_ascii_case(&var));
                s.view.set_bool(n, core.prefs.bool_or(&var, on));
            }
        }
        s.resolution_menu(current.resolution);
        for &(name, pref, _) in VOLUMES {
            s.slider(name, volume(&core.prefs, pref));
        }
        s.slider(
            "SliderControlsMouseSensitivity",
            mouse_sensitivity(&core.prefs),
        );
        s.slider(
            "slider_KeyboardTurnSpeed",
            core.prefs.f32_or(KEYBOARD_TURN_SPEED, 0.5).clamp(0.02, 1.0),
        );
        for (name, pref, fallback) in [
            ("Opt_ChatLineTime", "$Pref::Chat::LineTime", 6500),
            ("Opt_MaxChatLines", "$Pref::Chat::MaxDisplayLines", 8),
        ] {
            if let Some(n) = s.view.id(name) {
                s.view
                    .set_text(n, core.prefs.i64_or(pref, fallback).to_string());
            }
        }
        for &(name, _, pref, min, max) in TEMP_BRICK_FIELDS {
            if let Some(n) = s.view.id(name) {
                let v = core.prefs.f32_or(pref, min).clamp(min, max);
                s.view.set_text(n, v.to_string());
            }
        }
        s.temp_brick_blockers();
        // The renderer ignores the stock default; show what it draws.
        let anisotropy = if core.prefs.is_set(ANISOTROPY) {
            core.prefs.f32_or(ANISOTROPY, 0.0)
        } else {
            DEFAULT_ANISOTROPY
        };
        s.slider(
            "SliderGraphicsAnisotropy",
            if anisotropy.is_finite() { anisotropy.clamp(0.0, 1.0) } else { 0.0 },
        );
        s.slider(FOV_SLIDER, default_fov(&core.prefs));
        s.slider(DISTANCE_SLIDER, visible_distance_max(&core.prefs));
        s.set_chat_size(chat_size(&core.prefs));
        s.set_shadow_quality(core.prefs.i64_or(SHADOW_QUALITY, 0));
        // A limit set in the console matches no radio: none is shown.
        let limit = debris_limit(&core.prefs) as i64;
        if let Some(quality) = PHYSICS_LIMITS.iter().position(|&l| l == limit)
            && let Some(n) = s.view.id(&format!("{PHYSICS_RADIO}{quality}"))
        {
            s.view.select_radio(n);
        }
        let fps = max_fps(&core.prefs).unwrap_or(0);
        let fps_items = MAX_FPS_CHOICES
            .iter()
            .copied()
            .chain((!MAX_FPS_CHOICES.contains(&fps)).then_some(fps))
            .map(|f| {
                let label = if f == 0 { "Unlimited".into() } else { f.to_string() };
                (label, i64::from(f))
            })
            .collect();
        s.menu(MAX_FPS_MENU, fps_items, i64::from(fps));
        let vision = COLOR_VISION_CHOICES
            .iter()
            .enumerate()
            .map(|(i, t)| (t.to_string(), i as i64))
            .collect();
        s.menu(COLOR_VISION_MENU, vision, i64::from(color_vision(&core.prefs)));
        let scale = core.prefs.i64_or(crate::ui::UI_SCALE, 0).max(0);
        let scale_items = UI_SCALE_CHOICES
            .iter()
            .copied()
            .chain((!UI_SCALE_CHOICES.contains(&scale)).then_some(scale))
            .map(|p| (if p == 0 { "Auto".into() } else { format!("{p}%") }, p))
            .collect();
        s.menu(UI_SCALE_MENU, scale_items, scale);
        s.set_reflections(reflections(&core.prefs));
        let items = LIGHTING_CHOICES
            .iter()
            .enumerate()
            .map(|(i, t)| (t.to_string(), i as i64))
            .collect();
        s.menu(LIGHTING_MENU, items, lighting(&core.prefs));
        s.refresh_quality();
        s.refresh_readouts();
        s.pane("Graphics");
        s.refresh_binds(core);
        s.smart_toggle();
        s
    }

    /// Hide the Torque-only settings and close up the authored layout
    /// around the ones that remain.
    fn native_layout(&mut self) {
        let v = &mut self.view;
        for n in v.walk().collect::<Vec<_>>() {
            let c = &v.node(n).ctrl;
            let hide = (is_value(v, n) && !supported(v, n))
                || c.name.as_deref().is_some_and(|n| n.ends_with("Blocker"))
                || c.name.as_deref() == Some("OptNetworkPane")
                || c.command.as_deref() == Some("optionsDlg.setPane(Network);")
                // The Advanced pane's Apply only applied Torque renderer state.
                || (c.class == "GuiBitmapButtonCtrl"
                    && c.command.as_deref() == Some("optionsDlg.applyGraphics();"));
            if hide {
                v.set_visible(n, false);
            }
        }
        // The patched v20 FOV row (`SliderFOV`), in Advanced Graphics
        // Options below Anisotropy; close_rows moves it up with the rest.
        let aniso = v.id("SliderGraphicsAnisotropy");
        let section = aniso.and_then(|k| v.walk().find(|&n| v.node(n).children.contains(&k)));
        if let (Some(aniso), Some(section)) = (aniso, section) {
            let mut label = ctrl("GuiTextCtrl", "GuiTextProfile", Rect::new(235, 141, 25, 18));
            label.text = Some("FOV:".into());
            v.add(section, label);
            let mut slider = v.node(aniso).ctrl.clone();
            slider.name = Some(FOV_SLIDER.into());
            slider.position = [265, 141];
            slider
                .fields
                .insert("range".into(), format!("{} {}", FOV_RANGE.0, FOV_RANGE.1));
            slider.fields.insert("ticks".into(), "40".into());
            slider.fields.insert("snap".into(), "0".into());
            v.add(section, slider);
        }
        // "Check for new versions" has no v20 control; it ends Gui Options.
        if let Some(section) = find_section(v, "Gui Options") {
            let checks: Vec<NodeId> = v
                .node(section)
                .children
                .iter()
                .copied()
                .filter(|&k| v.node(k).state.visible && v.node(k).ctrl.class == "GuiCheckBoxCtrl")
                .collect();
            let last = checks.iter().copied().max_by_key(|&k| v.node(k).ctrl.position[1]);
            let bottom = v
                .node(section)
                .children
                .iter()
                .map(|&k| v.node(k).ctrl.position[1] + v.node(k).ctrl.extent[1])
                .max();
            if let (Some(last), Some(bottom)) = (last, bottom) {
                let mut c = v.node(last).ctrl.clone();
                c.name = Some("OptCheckForUpdatesToggle".into());
                c.variable = Some(CHECK_FOR_UPDATES.into());
                c.text = Some("Check for new versions".into());
                c.command = None;
                c.position[1] = bottom + 2;
                c.extent[0] = c.extent[0].max(170);
                let grow = c.extent[1] + 2;
                v.add(section, c);
                v.nodes[section].ctrl.extent[1] += grow;
            }
        }
        let sections: Vec<NodeId> = v.walk().filter(|&n| is_section(v, n)).collect();
        for &n in &sections {
            if !shows_values(v, n) {
                v.set_visible(n, false);
            }
        }
        // Slider labels, paired while the authored rows still line up.
        let slider_labels: Vec<(NodeId, NodeId)> = v
            .walk()
            .filter(|&n| v.node(n).state.visible && v.node(n).ctrl.class == "GuiSliderCtrl")
            .filter_map(|slider| {
                let parent = v.node(slider).parent?;
                let label = v.node(parent).children.iter().copied().find(|&k| {
                    v.node(k).ctrl.class == "GuiTextCtrl"
                        && labels(&v.node(k).ctrl, &v.node(slider).ctrl)
                })?;
                Some((label, slider))
            })
            .collect();
        let mut bottoms = HashMap::new();
        for &n in &sections {
            if v.node(n).state.visible {
                bottoms.insert(n, close_rows(v, n));
            }
        }
        // Closing rows moves a label and its slider by different amounts
        // when they sit on different authored rows; keep them level.
        for (label, slider) in slider_labels {
            v.nodes[label].ctrl.position[1] = v.node(slider).ctrl.position[1];
        }
        widen_resolution_menu(v);
        // Anti-aliasing and brick shadows have no v20 control; they join
        // Display Settings.
        let vsync = v.walk().find(|&n| {
            v.node(n)
                .ctrl
                .variable
                .as_deref()
                .is_some_and(|var| var.eq_ignore_ascii_case(NO_VSYNC))
        });
        let parent = vsync.and_then(|k| v.walk().find(|&n| v.node(n).children.contains(&k)));
        if let (Some(vsync), Some(parent)) = (vsync, parent) {
            let mut c = v.node(vsync).ctrl.clone();
            c.name = Some("OptGraphicsAntiAliasingToggle".into());
            c.variable = Some(ANTI_ALIASING.into());
            c.text = Some("Anti-Aliasing".into());
            // Under the resolution menu, left of Apply.
            let menu = v.id("OptGraphicsResolutionMenu").map(|m| v.node(m).ctrl.clone());
            let (x, y) = menu.as_ref().map_or((60, 85), |m| (m.position[0] - 40, m.position[1] + m.extent[1] + 4));
            c.position = [x, y];
            c.extent = [110, 23];
            c.command = None;
            v.add(parent, c.clone());
            // Brick Shadows follows it: the authored Shadow Quality section
            // is too narrow and clipped for another row.
            c.name = Some("OptGraphicsBrickShadowsToggle".into());
            c.variable = Some(BRICK_SHADOWS.into());
            c.text = Some("Brick Shadows".into());
            c.position[1] += c.extent[1] - 3;
            let below = c.position[1] + c.extent[1] + 8;
            v.add(parent, c);
            // Quality presets and the frame-rate cap follow as menu rows
            // shaped like Resolution.
            if let Some(menu) = menu {
                let label = v
                    .node(parent)
                    .children
                    .iter()
                    .map(|&k| v.node(k).ctrl.clone())
                    .find(|k| k.class == "GuiTextCtrl" && labels(k, &menu));
                let mut y = below;
                for (name, text) in [
                    (QUALITY_MENU, "Quality:"),
                    (MAX_FPS_MENU, "Max FPS:"),
                    (UI_SCALE_MENU, "UI Size:"),
                    (COLOR_VISION_MENU, "Colors:"),
                    (REFLECTIONS_MENU, "Mirrors:"),
                    (LIGHTING_MENU, "Lighting:"),
                ] {
                    let mut m = menu.clone();
                    m.name = Some(name.into());
                    m.position[1] = y;
                    m.extent[0] = m.extent[0].max(84);
                    m.command = None;
                    m.variable = None;
                    let mut l = label.clone().unwrap_or_else(|| {
                        ctrl(
                            "GuiTextCtrl",
                            "GuiTextProfile",
                            Rect::new(menu.position[0] - 60, 0, 56, 18),
                        )
                    });
                    l.name = None;
                    l.text = Some(text.into());
                    l.position[1] = y + (menu.extent[1] - l.extent[1]) / 2;
                    v.add(parent, l);
                    v.add(parent, m);
                    y += menu.extent[1] + 6;
                }
            }
        }
        // Audio: Volume takes the driver section's place.
        if let Some(n) = find_section(v, "Volume") {
            v.nodes[n].ctrl.position[1] = 7;
            v.nodes[n].ctrl.extent[1] = 301;
        }
        audio_rows(v);
        input_rows(v);
        // Advanced: stack the remaining sections of the scrolled page.
        let page = v.walk().find(|&n| {
            v.node(n)
                .children
                .iter()
                .any(|&k| is_section(v, k) && section_title(v, k) == "Gui Options")
        });
        if let Some(page) = page {
            let mut kids = v.node(page).children.clone();
            kids.sort_by_key(|&k| v.node(k).ctrl.position[1]);
            let mut y = 0;
            for k in kids {
                if !v.node(k).state.visible {
                    continue;
                }
                let bottom = bottoms.get(&k).copied().unwrap_or(v.node(k).ctrl.extent[1]);
                let c = &mut v.nodes[k].ctrl;
                c.position[1] = y;
                c.extent[1] = bottom + 6;
                y += c.extent[1] + 3;
            }
            v.nodes[page].ctrl.extent[1] = y;
        }
        graphics_pane(v);
        audio_pane(v);
        // Tabs close ranks without Network.
        let mut tabs: Vec<NodeId> = v
            .walk()
            .filter(|&n| {
                v.node(n).state.visible
                    && v.node(n)
                        .ctrl
                        .command
                        .as_deref()
                        .is_some_and(|c| c.starts_with("optionsDlg.setPane("))
            })
            .collect();
        tabs.sort_by_key(|&n| v.node(n).ctrl.position[0]);
        for (i, n) in tabs.into_iter().enumerate() {
            v.nodes[n].ctrl.position[0] = 12 + 90 * i as i32;
        }
        // Sliders give up room on their right for their value.
        for &name in READOUTS {
            let Some(n) = v.id(name) else { continue };
            let Some(parent) = v.node(n).parent else {
                continue;
            };
            let c = &mut v.nodes[n].ctrl;
            c.extent[0] = (c.extent[0] - 44).max(40);
            let r = Rect::new(c.position[0] + c.extent[0] + 4, c.position[1], 40, c.extent[1]);
            let mut t = ctrl("GuiTextCtrl", "GuiTextProfile", r);
            t.name = Some(format!("{name}Value"));
            v.add(parent, t);
        }
    }
    fn menu(&mut self, name: &str, items: Vec<(String, i64)>, selected: i64) {
        if let Some(n) = self.view.id(name) {
            self.view.state(n).items = items;
            self.view.select(n, Some(selected));
        }
    }
    /// Fill the resolution menu, selecting `want` or else the largest size.
    fn resolution_menu(&mut self, want: (u32, u32)) {
        let items = self
            .resolutions
            .iter()
            .enumerate()
            .map(|(i, (w, h))| (format!("{w} x {h}"), i as i64))
            .collect();
        let selected = self
            .resolutions
            .iter()
            .position(|r| *r == want)
            .unwrap_or(self.resolutions.len().saturating_sub(1));
        self.menu("OptGraphicsResolutionMenu", items, selected as i64);
    }
    fn slider(&mut self, name: &str, value: f32) {
        if let Some(n) = self.view.id(name) {
            self.view.set_num(n, value);
        }
    }
    fn refresh_readouts(&mut self) {
        for &name in READOUTS {
            if let (Some(n), Some(t)) = (self.view.id(name), self.view.id(&format!("{name}Value"))) {
                self.view.state(t).text = Some(readout(name, self.view.num(n)));
            }
        }
    }
    /// Checkboxes bound to `var` (authored names repeat; variables identify).
    fn checkboxes(&self, var: &str) -> Vec<NodeId> {
        self.view
            .walk()
            .filter(|&n| {
                let c = &self.view.node(n).ctrl;
                c.class == "GuiCheckBoxCtrl"
                    && c.variable.as_deref().is_some_and(|v| v.eq_ignore_ascii_case(var))
            })
            .collect()
    }
    fn check(&mut self, var: &str, on: bool) {
        for n in self.checkboxes(var) {
            self.view.set_bool(n, on);
        }
        self.draft.set_bool(var, on);
    }
    /// The preset the dialog's current values match, else Custom.
    fn quality(&self) -> i64 {
        let on = |var: &str| {
            self.checkboxes(var)
                .first()
                .map_or_else(|| self.draft.bool_or(var, false), |&n| self.view.bool_value(n))
        };
        let anisotropy = self
            .view
            .id("SliderGraphicsAnisotropy")
            .map_or(DEFAULT_ANISOTROPY, |n| self.view.num(n));
        let shadows = self.draft.i64_or(SHADOW_QUALITY, 0).clamp(0, 4);
        let mirrors = self
            .view
            .id(REFLECTIONS_MENU)
            .and_then(|n| self.view.selected(n))
            .unwrap_or_else(|| reflections(&self.draft));
        PRESETS
            .iter()
            .position(|p| {
                p.shadows == shadows
                    && p.reflections == mirrors
                    && p.anti_aliasing == on(ANTI_ALIASING)
                    && p.brick_shadows == on(BRICK_SHADOWS)
                    && p.precipitation == on(PRECIPITATION)
                    && (p.anisotropy - anisotropy).abs() < 0.02
            })
            .map_or(CUSTOM_QUALITY, |i| i as i64)
    }
    fn refresh_quality(&mut self) {
        let mut items: Vec<(String, i64)> = PRESETS
            .iter()
            .enumerate()
            .map(|(i, p)| (p.name.to_string(), i as i64))
            .collect();
        let quality = self.quality();
        // Custom is a state, not a choice: listed only while it applies.
        if quality == CUSTOM_QUALITY {
            items.push(("Custom".into(), CUSTOM_QUALITY));
        }
        self.menu(QUALITY_MENU, items, quality);
    }
    fn apply_preset(&mut self, index: usize) {
        let Some(p) = PRESETS.get(index) else {
            return;
        };
        self.set_shadow_quality(p.shadows);
        self.check(ANTI_ALIASING, p.anti_aliasing);
        self.check(BRICK_SHADOWS, p.brick_shadows);
        self.check(PRECIPITATION, p.precipitation);
        self.set_reflections(p.reflections);
        self.slider("SliderGraphicsAnisotropy", p.anisotropy);
        self.refresh_readouts();
    }
    fn set_reflections(&mut self, level: i64) {
        let items = REFLECTIONS_CHOICES
            .iter()
            .enumerate()
            .map(|(i, t)| (t.to_string(), i as i64))
            .collect();
        self.menu(REFLECTIONS_MENU, items, level.clamp(0, 3));
    }
    /// `optionsDlg::setShadowQuality`: 0 = Best through 4 = Minimum.
    fn set_shadow_quality(&mut self, quality: i64) {
        let quality = quality.clamp(0, 4);
        self.draft.set(SHADOW_QUALITY, quality.to_string());
        if let Some(n) = self.view.id(&format!("{SHADOW_RADIO}{quality}")) {
            self.view.select_radio(n);
        }
    }
    /// `optionsDlg::setPhysicsQuality`: 0 = Best through 4 = Off, and its
    /// debris limit.
    fn set_physics_quality(&mut self, quality: i64) {
        let quality = quality.clamp(0, 4);
        self.draft.set(PHYSICS_QUALITY, quality.to_string());
        self.draft
            .set(MAX_BRICKS, PHYSICS_LIMITS[quality as usize].to_string());
        if let Some(n) = self.view.id(&format!("{PHYSICS_RADIO}{quality}")) {
            self.view.select_radio(n);
        }
    }
    fn set_chat_size(&mut self, size: i64) {
        self.draft.set(CHAT_SIZE, size.to_string());
        if let Some(n) = self.view.id(&format!("{CHAT_SIZE_RADIO}{size}")) {
            self.view.select_radio(n);
        }
        if let Some(n) = self.view.id("ExampleChat") {
            self.view.nodes[n].ctrl.style = format!("HUDChatTextEditSize{size}Profile");
        }
    }
    fn pane(&mut self, name: &str) {
        for p in ["Graphics", "Audio", "Controls", "AdvGraphics"] {
            if let Some(n) = self.view.id(&format!("Opt{p}Pane")) {
                self.view.set_visible(n, p == name);
            }
        }
        self.view.close_popup();
        self.view.focus = None;
    }
    /// `OptionsDlg::UpdateTempBrickBlockers`: a colour taken from the paint
    /// can greys its red, green and blue fields out.
    fn temp_brick_blockers(&mut self) {
        for (blocker, pref) in [
            ("Opt_TempBrickOutsideColorBlocker", TEMP_BRICK_OUTSIDE_PAINT),
            ("Opt_TempBrickInsideColorBlocker", TEMP_BRICK_INSIDE_PAINT),
        ] {
            if let Some(n) = self.view.id(blocker) {
                let on = self.draft.bool_or(pref, false);
                self.view.set_visible(n, on);
            }
        }
    }
    fn smart_toggle(&mut self) {
        if let Some(n) = self.view.id("Opt_SSSmartToggle") {
            self.view.set_visible(
                n,
                self.draft
                    .bool_or("$pref::Input::UseSuperShiftToggle", true),
            );
        }
    }
    fn refresh_binds(&mut self, core: &Core) {
        if let Some(n) = self.view.id("OptRemapList") {
            let mut rows = Vec::new();
            for (i, r) in core.remap.iter().enumerate() {
                if let Some(division) = &r.division {
                    rows.push((format!("   {division}"), -(i as i64) - 1));
                }
                rows.push((
                    format!("{}\t{}", r.name, core.binds.display(&r.command)),
                    i as i64,
                ));
            }
            if self.view.node(n).state.items != rows {
                self.view.state(n).items = rows;
            }
        }
    }
    fn collect(&mut self) -> Result<(), String> {
        for n in self.view.walk().collect::<Vec<_>>() {
            let c = &self.view.node(n).ctrl;
            if self.view.node(n).state.visible
                && c.class == "GuiCheckBoxCtrl"
                && let Some(var) = c.variable.clone()
            {
                self.draft.set_bool(&var, self.view.bool_value(n));
            }
        }
        for (name, pref, lo, hi) in [
            (
                "SliderControlsMouseSensitivity",
                MOUSE_SENSITIVITY,
                MOUSE_SENSITIVITY_RANGE.0,
                MOUSE_SENSITIVITY_RANGE.1,
            ),
            ("slider_KeyboardTurnSpeed", KEYBOARD_TURN_SPEED, 0.02, 1.0),
            ("SliderGraphicsAnisotropy", ANISOTROPY, 0.0, 1.0),
            (
                DISTANCE_SLIDER,
                VISIBLE_DISTANCE_MAX,
                VISIBLE_DISTANCE_RANGE.0,
                VISIBLE_DISTANCE_RANGE.1,
            ),
        ] {
            if let Some(n) = self.view.id(name) {
                let v = self.view.num(n);
                if !v.is_finite() {
                    return Err("Sliders must hold finite numbers.".into());
                }
                self.draft.set(pref, v.clamp(lo, hi).to_string());
            }
        }
        if let Some(n) = self.view.id(FOV_SLIDER) {
            let v = self.view.num(n);
            if !v.is_finite() {
                return Err("Sliders must hold finite numbers.".into());
            }
            let fov = v.round().clamp(FOV_RANGE.0, FOV_RANGE.1);
            self.draft.set(DEFAULT_FOV, fov.to_string());
            self.view.set_num(n, fov);
        }
        if let Some(fps) = self
            .view
            .id(MAX_FPS_MENU)
            .and_then(|n| self.view.selected(n))
        {
            self.draft.set(MAX_FPS, fps.clamp(0, 1000).to_string());
        }
        if let Some(mode) = self
            .view
            .id(COLOR_VISION_MENU)
            .and_then(|n| self.view.selected(n))
        {
            self.draft.set(COLOR_VISION, mode.clamp(0, 3).to_string());
        }
        if let Some(level) = self
            .view
            .id(REFLECTIONS_MENU)
            .and_then(|n| self.view.selected(n))
        {
            self.draft.set(REFLECTIONS, level.clamp(0, 3).to_string());
        }
        if let Some(mode) = self
            .view
            .id(LIGHTING_MENU)
            .and_then(|n| self.view.selected(n))
        {
            self.draft.set(LIGHTING, mode.clamp(0, LIGHTING_CHOICES.len() as i64 - 1).to_string());
        }
        if let Some(scale) = self
            .view
            .id(UI_SCALE_MENU)
            .and_then(|n| self.view.selected(n))
        {
            self.draft
                .set(crate::ui::UI_SCALE, scale.clamp(0, 800).to_string());
        }
        for &(name, pref, _) in VOLUMES {
            if let Some(n) = self.view.id(name) {
                let v = self.view.num(n);
                if !v.is_finite() {
                    return Err("Volume must be a finite number.".into());
                }
                self.draft.set(pref, v.clamp(0.0, 1.0).to_string());
            }
        }
        for (name, label, pref, min, max) in [
            (
                "Opt_ChatLineTime",
                "Chat Line Time",
                "$Pref::Chat::LineTime",
                0,
                30000,
            ),
            (
                "Opt_MaxChatLines",
                "Max Chat Lines",
                CHAT_LINES,
                CHAT_LINES_RANGE.0,
                CHAT_LINES_RANGE.1,
            ),
        ] {
            if let Some(n) = self.view.id(name) {
                let v = self
                    .view
                    .edit_text(n)
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| format!("{label} must be a whole number."))?
                    .clamp(min, max);
                self.draft.set(pref, v.to_string());
                self.view.set_text(n, v.to_string());
            }
        }
        for &(name, label, pref, min, max) in TEMP_BRICK_FIELDS {
            if let Some(n) = self.view.id(name) {
                let v = self
                    .view
                    .edit_text(n)
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| format!("{label} must be a number."))?
                    .clamp(min, max);
                self.draft.set(pref, v.to_string());
                self.view.set_text(n, v.to_string());
            }
        }
        Ok(())
    }
    fn selected_display(&self) -> DisplaySettings {
        let mut d = display(
            &self.draft,
            (
                self.applied_display.resolution.0 as i32,
                self.applied_display.resolution.1 as i32,
            ),
        );
        d.resolution = self
            .view
            .id("OptGraphicsResolutionMenu")
            .and_then(|n| self.view.selected(n))
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| self.resolutions.get(i))
            .copied()
            .unwrap_or(self.applied_display.resolution);
        d
    }
    /// Apply (`close` = false) or Done/Escape (`close` = true). A changed
    /// display mode is requested first; the dialog commits once it lands.
    fn apply(&mut self, core: &mut Core, close: bool) {
        if self.pending_display.is_some() {
            return;
        }
        if let Err(e) = self.collect() {
            core.message_ok("Options", &e);
            return;
        }
        let d = self.selected_display();
        if d != self.applied_display {
            let id = core.request_pending(
                UiAction::ApplyDisplay {
                    resolution: d.resolution,
                    fullscreen: d.fullscreen,
                    vsync: d.vsync,
                },
                Pending::Other,
            );
            self.pending_display = Some((id, d, close));
            self.pending_enabled = self
                .view
                .walk()
                .filter(|&n| {
                    self.view.node(n).state.active
                        && (is_value(&self.view, n)
                            || matches!(
                                self.view.node(n).ctrl.class.as_str(),
                                "GuiButtonCtrl" | "GuiBitmapButtonCtrl"
                            ))
                })
                .collect();
            for &n in &self.pending_enabled {
                self.view.set_active(n, false);
            }
            self.view.focus = None;
            self.view.close_popup();
        } else if close {
            self.commit(core);
        }
    }
    fn commit(&mut self, core: &mut Core) {
        // Copy only supported edited values. Preserve concurrent avatar/favorite
        // changes made by other dialogs and preferences owned by the host.
        for (key, value) in self.draft.overrides() {
            let preset = PRESET_PREFS.iter().any(|p| p.eq_ignore_ascii_case(&key));
            if preset || self.initial.get(&key) != Some(value.as_str()) {
                core.prefs.set(&key, value);
            }
        }
        put_display(&mut core.prefs, self.applied_display);
        core.apply_prefs();
        self.committed = true;
        core.save_settings();
        core.pop(ScreenId::Options);
    }
    fn begin_remap(&mut self, core: &mut Core, all: bool) {
        let index = if all {
            Some(0)
        } else {
            self.view
                .id("OptRemapList")
                .and_then(|n| self.view.selected(n))
                .and_then(|i| usize::try_from(i).ok())
        };
        if let Some(i) = index.filter(|i| *i < core.remap_commands.len()) {
            core.remap_target = Some(i);
            core.remap_all = all;
            core.push(ScreenId::Remap);
        }
    }
}
impl Screen for Options {
    fn id(&self) -> ScreenId {
        ScreenId::Options
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn blocks_accelerators(&self) -> bool {
        true
    }
    fn on_wake(&mut self, core: &mut Core) {
        core.options_open = true;
    }
    fn on_sleep(&mut self, core: &mut Core) {
        if !self.committed {
            // Volumes play live while dragged; undo the preview.
            for &(_, pref, channel) in VOLUMES.iter().filter(|_| self.previewed) {
                core.request(UiAction::SetVolume {
                    channel: channel.into(),
                    value: volume(&self.initial, pref),
                });
            }
            core.binds = self.saved_binds.clone();
            core.settings.mouse_type = self.saved_hardware.0;
            core.settings.keyboard_type = self.saved_hardware.1;
        }
        core.options_open = false;
        core.remap_all = false;
        core.remap_target = None;
    }
    fn tick(&mut self, _dt: u64, core: &mut Core) {
        self.refresh_binds(core);
    }
    fn on_update(&mut self, core: &mut Core) {
        self.refresh_binds(core);
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        if self.pending_display.is_some() || !self.view.node(ev.node).state.active {
            return;
        }
        if ev.kind == EventKind::Close {
            self.apply(core, true);
            return;
        }
        if ev.kind == EventKind::Changed {
            let name = self.view.node(ev.node).ctrl.name.clone().unwrap_or_default();
            if let Some(&(_, _, channel)) = VOLUMES.iter().find(|(n, _, _)| *n == name) {
                let value = self.view.num(ev.node);
                if value.is_finite() {
                    self.previewed = true;
                    core.request(UiAction::SetVolume {
                        channel: channel.into(),
                        value: value.clamp(0.0, 1.0),
                    });
                }
            }
            if name == QUALITY_MENU {
                if let Some(i) = self.view.selected(ev.node).and_then(|i| usize::try_from(i).ok()) {
                    self.apply_preset(i);
                }
                self.refresh_quality();
                return;
            }
            self.refresh_readouts();
            if let Some(var) = self
                .view
                .node(ev.node)
                .ctrl
                .variable
                .clone()
                .filter(|v| v.starts_with('$'))
            {
                self.draft.set_bool(&var, self.view.bool_value(ev.node));
                self.smart_toggle();
                self.temp_brick_blockers();
                if var.eq_ignore_ascii_case(FULLSCREEN) {
                    // OptGraphicsFullscreenToggle::onAction rebuilds the list.
                    let keep = self.selected_display().resolution;
                    self.resolutions = resolution_list(
                        self.modes.as_ref(),
                        self.view.bool_value(ev.node),
                        self.applied_display.resolution,
                    );
                    self.resolution_menu(keep);
                }
            }
            self.refresh_quality();
            return;
        }
        if !matches!(ev.kind, EventKind::Click | EventKind::Submit) {
            return;
        }
        if self.view.id("OptRemapList") == Some(ev.node) {
            if ev.kind == EventKind::Submit {
                self.begin_remap(core, false);
            }
            return;
        }
        let cmd = command_of(&self.view, ev.node);
        if let Some(pane) = cmd
            .strip_prefix("optionsDlg.setPane(")
            .and_then(|c| c.strip_suffix(");"))
        {
            self.pane(pane);
            return;
        }
        if let Some(size) = cmd
            .strip_prefix("OPT_SetChatSize(")
            .and_then(|c| c.strip_suffix(");"))
            .and_then(|c| c.parse().ok())
        {
            self.set_chat_size(size);
            return;
        }
        if let Some(quality) = cmd
            .strip_prefix("optionsDlg.setShadowQuality(")
            .and_then(|c| c.strip_suffix(");"))
            .and_then(|c| c.parse().ok())
        {
            self.set_shadow_quality(quality);
            self.refresh_quality();
            return;
        }
        if let Some(quality) = cmd
            .strip_prefix("optionsDlg.setPhysicsQuality(")
            .and_then(|c| c.strip_suffix(");"))
            .and_then(|c| c.parse().ok())
        {
            self.set_physics_quality(quality);
            return;
        }
        match cmd.as_str() {
            "Canvas.popDialog(optionsDlg);" => self.apply(core, true),
            "optionsDlg.applyGraphics();" => self.apply(core, false),
            "optionsDlg.RemapAll();" => self.begin_remap(core, true),
            "optionsDlg.clearAllBinds();" => core.message_yes_no(
                "Clear All Binds?",
                "Are you sure you want to clear your control configuration?",
                Callback::ClearBinds,
            ),
            "canvas.pushDialog(DefaultControlsGui);" => core.push(ScreenId::DefaultControls),
            "Canvas.pushDialog(AvatarGui);" => {
                // Remaps are live, as in v20; Player Appearance saves settings
                // with the current controls, so they become the baseline.
                self.saved_binds = core.binds.clone();
                self.saved_hardware = (core.settings.mouse_type, core.settings.keyboard_type);
                core.push(ScreenId::Avatar);
            }
            _ => {}
        }
    }
    fn on_result(
        &mut self,
        id: RequestId,
        _kind: Option<&Pending>,
        result: &Result<(), String>,
        core: &mut Core,
    ) -> bool {
        let Some((pending, d, close)) = self.pending_display else {
            return false;
        };
        if pending != id {
            return false;
        }
        self.pending_display = None;
        for n in self.pending_enabled.drain(..) {
            self.view.set_active(n, true);
        }
        match result {
            Ok(()) => {
                self.applied_display = d;
                put_display(&mut self.draft, d);
                put_display(&mut core.prefs, d);
                if close {
                    self.commit(core);
                } else {
                    // Applying display is its own committed boundary.
                    let mut settings = core.settings.clone();
                    let mut prefs = core.prefs.clone();
                    put_display(&mut prefs, d);
                    settings.prefs = prefs.overrides();
                    settings.binds = Some(self.saved_binds.entries.clone());
                    settings.mouse_type = self.saved_hardware.0;
                    settings.keyboard_type = self.saved_hardware.1;
                    core.request(UiAction::SaveSettings(Box::new(settings)));
                }
            }
            Err(reason) => core.message_ok("Display Settings", reason),
        }
        true
    }
}

pub struct Remap {
    view: View,
    index: Option<usize>,
    conflict: Option<BindInput>,
}
impl Remap {
    pub fn new(core: &Core) -> Self {
        let mut s = Self {
            view: layout_view(core, "RemapDlg"),
            index: core.remap_target,
            conflict: None,
        };
        // Keep the authored skin and footer, but allow enough room for native
        // conflict instructions without clipping or an ambiguous hidden answer.
        if let Some(n) = window(&s.view) {
            s.view.nodes[n].ctrl.position = [120, 165];
            s.view.nodes[n].ctrl.extent = [400, 150];
        }
        for n in s.view.walk().collect::<Vec<_>>() {
            match s.view.text_of(n).as_str() {
                "Escape to cancel" => s.view.nodes[n].ctrl.position = [8, 130],
                "Backspace to clear" => s.view.nodes[n].ctrl.position = [298, 130],
                _ => {}
            }
        }
        if let Some(n) = s.view.id("OptRemapText") {
            s.view.nodes[n].ctrl.position = [10, 28];
            s.view.nodes[n].ctrl.extent = [380, 96];
            s.view.nodes[n].ctrl.class = "GuiMLTextCtrl".into();
        }
        s.prompt(core);
        s
    }
    fn set_text(&mut self, t: String) {
        if let Some(n) = self.view.id("OptRemapText") {
            self.view.set_text(n, t);
        }
    }
    fn prompt(&mut self, core: &Core) {
        let name = self
            .index
            .and_then(|i| core.remap.get(i))
            .map(|r| r.name.as_str())
            .unwrap_or("No control selected");
        self.set_text(format!("REMAP \"{name}\""));
    }
    fn next(&mut self, core: &mut Core) {
        if core.remap_all
            && let Some(i) = self
                .index
                .and_then(|i| i.checked_add(1))
                .filter(|i| *i < core.remap_commands.len())
        {
            self.index = Some(i);
            core.remap_target = Some(i);
            self.conflict = None;
            self.prompt(core);
            return;
        }
        self.cancel(core);
    }
    fn cancel(&mut self, core: &mut Core) {
        core.remap_target = None;
        core.remap_all = false;
        core.pop(ScreenId::Remap);
    }
    fn capture(&mut self, input: BindInput, core: &mut Core) -> bool {
        if let BindInput::Key(c) = input {
            if c.key == Key::Escape {
                self.cancel(core);
                return true;
            }
            if self.conflict.is_some() {
                match c.key {
                    Key::Return | Key::Letter('y') => {
                        if let Some(command) =
                            self.index.and_then(|i| core.remap_commands.get(i)).cloned()
                        {
                            core.binds
                                .force_remap(&command, self.conflict.take().unwrap());
                            self.next(core);
                        }
                    }
                    Key::Letter('n') => {
                        self.conflict = None;
                        self.prompt(core);
                    }
                    _ => {}
                }
                return true;
            }
            if c.key == Key::Backspace {
                if let Some(command) = self.index.and_then(|i| core.remap_commands.get(i)) {
                    core.binds.unbind_command(command);
                }
                self.next(core);
                return true;
            }
        }
        if self.conflict.is_some() {
            return true;
        }
        let Some(command) = self.index.and_then(|i| core.remap_commands.get(i)).cloned() else {
            self.cancel(core);
            return true;
        };
        let reserved = match input {
            BindInput::Key(c) => core.globals.command_for_key(c.key, c.mods).is_some(),
            _ => core.globals.command_for(&input).is_some(),
        };
        if reserved {
            self.set_text(format!(
                "{} is reserved. Choose another input, or Esc.",
                input.label()
            ));
            return true;
        }
        match core.binds.remap(&command, input, &core.remap_commands) {
            RemapOutcome::Bound => {
                core.binds.force_remap(&command, input);
                self.next(core);
            }
            RemapOutcome::Conflict { other } => {
                let name = core
                    .remap
                    .iter()
                    .find(|r| r.command.eq_ignore_ascii_case(&other))
                    .map_or(other.as_str(), |r| r.name.as_str());
                self.conflict = Some(input);
                self.set_text(format!(
                    "{} is bound to {name}. Replace? Enter/Y = yes; N = no; Esc = cancel.",
                    input.label()
                ));
            }
            RemapOutcome::NotRemappable { .. } => self.set_text(format!(
                "{} is reserved. Choose another input, or Esc.",
                input.label()
            )),
        }
        true
    }
}
impl Screen for Remap {
    fn id(&self) -> ScreenId {
        ScreenId::Remap
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn captures_keyboard(&self) -> bool {
        true
    }
    fn blocks_accelerators(&self) -> bool {
        true
    }
    fn on_bind_input(&mut self, input: BindInput, core: &mut Core) -> bool {
        self.capture(input, core)
    }
    fn on_key(&mut self, key: Key, mods: Modifiers, core: &mut Core) -> bool {
        self.capture(
            BindInput::Key(if key.is_modifier() {
                Chord::plain(key)
            } else {
                Chord { key, mods }
            }),
            core,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Settings, UiUpdate};
    use crate::binds::Platform;
    use crate::input::{InputEvent, MouseButton};
    use crate::schema::{RemapEntry, UiPack};
    use crate::ui::{Ui, UiConfig};
    use std::rc::Rc;

    fn fixture() -> Ui {
        let mut data = UiPack::default();
        let mut layout = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        for (class, name, var, command) in [
            ("GuiCheckBoxCtrl", "duplicate", FULLSCREEN, ""),
            ("GuiCheckBoxCtrl", "duplicate", NO_VSYNC, ""),
            (
                "GuiCheckBoxCtrl",
                "audioDuplicate",
                "$Pref::Audio::PlayMusic",
                "",
            ),
            (
                "GuiCheckBoxCtrl",
                "audioDuplicate",
                "$Pref::Audio::MenuSounds",
                "",
            ),
            (
                "GuiCheckBoxCtrl",
                "audioDuplicate",
                "$Pref::Audio::PlayBrickPlantSound",
                "",
            ),
            (
                "GuiCheckBoxCtrl",
                "audioDuplicate",
                "$Pref::Audio::PlayBrickMoveSound",
                "",
            ),
            (
                "GuiCheckBoxCtrl",
                "audioDuplicate",
                "$Pref::Audio::PlantErrorSound",
                "",
            ),
            (
                "GuiCheckBoxCtrl",
                "duplicate",
                "$pref::HUD::HideBrickBox",
                "",
            ),
            (
                "GuiCheckBoxCtrl",
                "unsupported",
                "$pref::OpenGL::doAnimatedLights",
                "",
            ),
            ("GuiPopUpMenuCtrl", "OptGraphicsResolutionMenu", "", ""),
            (
                "GuiSliderCtrl",
                "SliderControlsMouseSensitivity",
                "value",
                "",
            ),
            ("GuiSliderCtrl", "OptAudioVolumeMaster", "value", ""),
            ("GuiSliderCtrl", "OptAudioVolumeShell", "value", ""),
            ("GuiSliderCtrl", "OptAudioVolumeSim", "value", ""),
            ("GuiTextEditCtrl", "Opt_ChatLineTime", "", ""),
            ("GuiTextEditCtrl", "Opt_MaxChatLines", "", ""),
            ("GuiTextListCtrl", "OptRemapList", "", ""),
            ("GuiButtonCtrl", "apply", "", "optionsDlg.applyGraphics();"),
            ("GuiButtonCtrl", "done", "", "Canvas.popDialog(optionsDlg);"),
            ("GuiButtonCtrl", "clear", "", "optionsDlg.clearAllBinds();"),
            ("GuiRadioCtrl", "OPT_ChatSize2", "", "OPT_SetChatSize(2);"),
            ("GuiRadioCtrl", "OPT_ChatSize4", "", "OPT_SetChatSize(4);"),
            (
                "GuiCheckBoxCtrl",
                "OptGraphicsTrilinearToggle",
                "$pref::OpenGL::textureTrilinear",
                "",
            ),
            (
                "GuiCheckBoxCtrl",
                "OptGraphicsTexturedFog",
                "$pref::OpenGL::useGLNearest",
                "",
            ),
            ("GuiSliderCtrl", "SliderGraphicsAnisotropy", "value", ""),
            ("GuiCheckBoxCtrl", "OptPrecipitation", PRECIPITATION, ""),
            ("GuiCheckBoxCtrl", "OptNoobJet", "$pref::Input::noobjet", ""),
            (
                "GuiCheckBoxCtrl",
                "OptVehicleInvert",
                VEHICLE_MOUSE_INVERT,
                "",
            ),
            ("GuiSliderCtrl", DISTANCE_SLIDER, "value", ""),
            (
                "GuiRadioCtrl",
                "OPT_ShadowQuality0",
                "",
                "optionsDlg.setShadowQuality(0);",
            ),
            (
                "GuiRadioCtrl",
                "OPT_ShadowQuality3",
                "",
                "optionsDlg.setShadowQuality(3);",
            ),
            (
                "GuiRadioCtrl",
                "OPT_PhysicsQuality0",
                "",
                "optionsDlg.setPhysicsQuality(0);",
            ),
            (
                "GuiRadioCtrl",
                "OPT_PhysicsQuality1",
                "",
                "optionsDlg.setPhysicsQuality(1);",
            ),
            (
                "GuiRadioCtrl",
                "OPT_PhysicsQuality4",
                "",
                "optionsDlg.setPhysicsQuality(4);",
            ),
        ] {
            let mut c = ctrl(class, "GuiDefaultProfile", Rect::new(0, 0, 100, 20));
            c.name = Some(name.into());
            if !var.is_empty() {
                c.variable = Some(var.into());
            }
            if !command.is_empty() {
                c.command = Some(command.into());
            }
            if name == "done" {
                c.accelerator = Some("escape".into());
            }
            if name == DISTANCE_SLIDER {
                c.fields.insert("range".into(), "110 1000".into());
            }
            // Authored radio sets sit in their own sections.
            if name.starts_with(SHADOW_RADIO) {
                c.group = Some(1);
            }
            if name.starts_with(PHYSICS_RADIO) {
                c.group = Some(2);
            }
            layout.children.push(c);
        }
        data.layouts.insert("optionsDlg".into(), layout);
        data.data.remap = vec![
            RemapEntry {
                division: Some("Movement".into()),
                name: "Forward".into(),
                command: "moveforward".into(),
            },
            RemapEntry {
                division: None,
                name: "Backward".into(),
                command: "movebackward".into(),
            },
        ];
        let mut ui = Ui::new(
            Rc::new(Pack::from_parts(data, Default::default())),
            UiConfig {
                size: (640, 480),
                scale: Some(1.0),
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        );
        ui.core.binds.bind(
            BindInput::Key(Chord::plain(Key::Letter('w'))),
            "moveforward",
        );
        ui.core.binds.bind(
            BindInput::Key(Chord::plain(Key::Letter('s'))),
            "movebackward",
        );
        ui.core.remap_commands = vec!["moveforward".into(), "movebackward".into()];
        ui.core.prefs.set(RESOLUTION, "640 480 32");
        ui.drain_actions();
        ui
    }
    fn click(s: &mut Options, name: &str, ui: &mut Ui) {
        let node = s.view.id(name).unwrap();
        s.on_event(
            &ViewEvent {
                node,
                kind: EventKind::Click,
            },
            &mut ui.core,
        );
    }
    fn key(key: Key) -> BindInput {
        BindInput::Key(Chord::plain(key))
    }

    const AUDIO_PREFS: [&str; 5] = [
        "$Pref::Audio::PlayMusic",
        "$Pref::Audio::MenuSounds",
        "$Pref::Audio::PlayBrickPlantSound",
        "$Pref::Audio::PlayBrickMoveSound",
        "$Pref::Audio::PlantErrorSound",
    ];
    fn audio_node(options: &Options, pref: &str) -> NodeId {
        options
            .view
            .walk()
            .find(|&n| {
                options
                    .view
                    .node(n)
                    .ctrl
                    .variable
                    .as_deref()
                    .is_some_and(|v| v.eq_ignore_ascii_case(pref))
            })
            .unwrap()
    }
    fn toggle_audio(options: &mut Options, ui: &mut Ui, pref: &str, value: bool) {
        let n = audio_node(options, pref);
        assert!(options.view.node(n).state.active);
        options.view.set_bool(n, value);
        options.on_event(
            &ViewEvent {
                node: n,
                kind: EventKind::Changed,
            },
            &mut ui.core,
        );
    }

    #[test]
    fn gui_options_ends_with_a_check_for_new_versions_toggle() {
        let mut data = UiPack::default();
        let mut layout = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        let mut section = ctrl("GuiSwatchCtrl", "GuiDefaultProfile", Rect::new(0, 0, 300, 60));
        section
            .children
            .push(ctrl("GuiSwatchCtrl", "GuiDefaultProfile", Rect::new(2, 2, 296, 14)));
        let mut title = ctrl("GuiTextCtrl", "GuiDefaultProfile", Rect::new(4, 0, 100, 14));
        title.text = Some("Gui Options".into());
        section.children.push(title);
        let mut tips = ctrl("GuiCheckBoxCtrl", "GuiDefaultProfile", Rect::new(10, 20, 120, 18));
        tips.variable = Some("$pref::HUD::showToolTips".into());
        tips.text = Some("Show Tooltips".into());
        section.children.push(tips);
        layout.children.push(section);
        let mut done = ctrl("GuiButtonCtrl", "GuiDefaultProfile", Rect::new(0, 400, 100, 20));
        done.name = Some("done".into());
        done.command = Some("Canvas.popDialog(optionsDlg);".into());
        layout.children.push(done);
        data.layouts.insert("optionsDlg".into(), layout);
        let mut ui = Ui::new(
            Rc::new(Pack::from_parts(data, Default::default())),
            UiConfig {
                size: (640, 480),
                scale: Some(1.0),
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        );
        let mut options = Options::new(&ui.core);
        let toggle = options.view.id("OptCheckForUpdatesToggle").expect("added");
        let tips = audio_node(&options, "$pref::HUD::showToolTips");
        let (t, c) = (&options.view.node(tips).ctrl, &options.view.node(toggle).ctrl);
        assert!(c.position[1] >= t.position[1] + t.extent[1], "below the last row");
        assert_eq!(c.position[0], t.position[0]);
        assert!(options.view.node(toggle).state.visible);
        assert!(options.view.bool_value(toggle), "on unless turned off");
        toggle_audio(&mut options, &mut ui, CHECK_FOR_UPDATES, false);
        click(&mut options, "done", &mut ui);
        let saved = ui.drain_actions().into_iter().find_map(|(_, a)| match a {
            UiAction::SaveSettings(s) => Some(s),
            _ => None,
        });
        let saved = saved.expect("Done saves");
        let prefs = Prefs::new(&Default::default(), &saved.prefs);
        assert!(!prefs.bool_or(CHECK_FOR_UPDATES, true));
    }

    #[test]
    fn audio_preferences_use_seeded_defaults_and_apply_only_on_done() {
        let mut ui = fixture();
        for pref in AUDIO_PREFS {
            ui.core
                .prefs
                .set_bool(pref, pref != "$Pref::Audio::PlantErrorSound");
        }
        let before = ui.core.prefs.clone();
        let mut options = Options::new(&ui.core);
        for pref in AUDIO_PREFS {
            let initial = pref != "$Pref::Audio::PlantErrorSound";
            assert_eq!(options.view.bool_value(audio_node(&options, pref)), initial);
            toggle_audio(&mut options, &mut ui, pref, !initial);
        }
        ui.apply(UiUpdate::PlantError(crate::api::PlantError::Overlap));
        assert!(ui.drain_sounds().is_empty());
        assert!(ui.drain_actions().is_empty());
        assert_eq!(ui.core.prefs, before);
        options.on_sleep(&mut ui.core);
        assert_eq!(ui.core.prefs, before);
        assert!(ui.drain_actions().is_empty());
    }

    #[test]
    fn done_commits_all_five_audio_toggles_once_through_save_settings() {
        let mut ui = fixture();
        for pref in AUDIO_PREFS {
            ui.core.prefs.set_bool(pref, false);
        }
        let mut options = Options::new(&ui.core);
        for pref in AUDIO_PREFS {
            toggle_audio(&mut options, &mut ui, pref, true);
        }
        assert!(
            !options
                .view
                .node(options.view.id("unsupported").unwrap())
                .state
                .visible
        );
        assert!(ui.drain_actions().is_empty());
        click(&mut options, "done", &mut ui);
        let actions = ui.drain_actions();
        assert_eq!(
            actions
                .iter()
                .filter(|(_, a)| matches!(a, UiAction::SetVolume { .. }))
                .count(),
            VOLUMES.len()
        );
        let saves: Vec<_> = actions
            .iter()
            .filter_map(|(_, a)| {
                if let UiAction::SaveSettings(s) = a {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(saves.len(), 1);
        let saved = Prefs::new(&Default::default(), &saves[0].prefs);
        for pref in AUDIO_PREFS {
            assert!(ui.core.prefs.bool_or(pref, false));
            assert!(saved.bool_or(pref, false));
        }
        ui.apply(UiUpdate::PlantError(crate::api::PlantError::Overlap));
        assert_eq!(ui.drain_sounds().len(), 1);
    }

    fn change(s: &mut Options, ui: &mut Ui, node: NodeId) {
        s.on_event(
            &ViewEvent {
                node,
                kind: EventKind::Changed,
            },
            &mut ui.core,
        );
    }
    fn saved_prefs(ui: &mut Ui) -> Prefs {
        ui.drain_actions()
            .into_iter()
            .find_map(|(_, a)| match a {
                UiAction::SaveSettings(s) => Some(Prefs::new(&Default::default(), &s.prefs)),
                _ => None,
            })
            .expect("Done saves settings")
    }

    #[test]
    fn stock_v20_display_defaults_are_not_shown_or_saved() {
        // The v20 pack seeds 800x600 and VSync off; a first visit and Done
        // must keep the real window size and VSync on for the next launch.
        let mut data = fixture().core.pack.data.clone();
        data.data.prefs.insert(NO_VSYNC.into(), "1".into());
        data.data.prefs.insert(RESOLUTION.into(), "800 600 32".into());
        let mut ui = Ui::new(
            Rc::new(Pack::from_parts(data, Default::default())),
            UiConfig {
                size: (1280, 720),
                scale: Some(1.0),
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        );
        let mut s = Options::new(&ui.core);
        for n in s.checkboxes(NO_VSYNC) {
            assert!(!s.view.bool_value(n), "Disable Vsync shown ticked");
        }
        click(&mut s, "done", &mut ui);
        let saved = saved_prefs(&mut ui);
        assert!(!saved.bool_or(NO_VSYNC, false));
        assert_eq!(saved.get(RESOLUTION), Some("1280 720 32"));
    }

    #[test]
    fn lighting_defaults_to_unified_with_highlights_and_saves_a_choice() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let menu = s.view.id(LIGHTING_MENU).unwrap();
        assert_eq!(s.view.selected_text(menu).as_deref(), Some("Unified+Shine"));
        // The row fits inside its section, below Mirrors.
        let parent = s.view.walk().find(|&n| s.view.node(n).children.contains(&menu)).unwrap();
        let (row, section) = (&s.view.node(menu).ctrl, &s.view.node(parent).ctrl);
        assert!(row.position[1] + row.extent[1] <= section.extent[1], "{row:?} in {section:?}");
        let mirrors = &s.view.node(s.view.id(REFLECTIONS_MENU).unwrap()).ctrl;
        assert!(row.position[1] >= mirrors.position[1] + mirrors.extent[1]);
        s.view.select(menu, Some(0));
        change(&mut s, &mut ui, menu);
        click(&mut s, "done", &mut ui);
        let saved = saved_prefs(&mut ui);
        assert_eq!(lighting(&saved), 0);
        let mut s = Options::new(&ui.core);
        let menu = s.view.id(LIGHTING_MENU).unwrap();
        assert_eq!(s.view.selected_text(menu).as_deref(), Some("Classic"));
        s.view.select(menu, Some(3));
        change(&mut s, &mut ui, menu);
        click(&mut s, "done", &mut ui);
        assert_eq!(lighting(&saved_prefs(&mut ui)), 3);
        let s = Options::new(&ui.core);
        let menu = s.view.id(LIGHTING_MENU).unwrap();
        assert_eq!(s.view.selected_text(menu).as_deref(), Some("Dynamic"));
    }

    #[test]
    fn a_new_player_sees_high_quality_and_presets_set_every_option() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let menu = s.view.id(QUALITY_MENU).unwrap();
        assert_eq!(s.view.selected_text(menu).as_deref(), Some("High"));
        assert!(!s.view.node(menu).state.items.iter().any(|(t, _)| t == "Custom"));
        // Low turns off everything costly.
        s.view.select(menu, Some(0));
        change(&mut s, &mut ui, menu);
        assert_eq!(s.view.selected_text(menu).as_deref(), Some("Low"));
        let aa = s.checkboxes(ANTI_ALIASING)[0];
        assert!(!s.view.bool_value(aa));
        assert!(!s.view.bool_value(s.checkboxes(PRECIPITATION)[0]));
        let aniso = s.view.id("SliderGraphicsAnisotropy").unwrap();
        assert_eq!(s.view.num(aniso), 0.0);
        let mirrors = s.view.id(REFLECTIONS_MENU).unwrap();
        assert_eq!(s.view.selected_text(mirrors).as_deref(), Some("Off"));
        // Changing one option by hand makes it Custom.
        s.view.set_bool(aa, true);
        change(&mut s, &mut ui, aa);
        assert_eq!(s.view.selected_text(menu).as_deref(), Some("Custom"));
        // Ultra, then Done, stores every preset option explicitly.
        s.view.select(menu, Some(3));
        change(&mut s, &mut ui, menu);
        assert!(!s.view.node(menu).state.items.iter().any(|(t, _)| t == "Custom"));
        click(&mut s, "done", &mut ui);
        let saved = saved_prefs(&mut ui);
        assert_eq!(saved.get(SHADOW_QUALITY), Some("0"));
        assert!(saved.bool_or(BRICK_SHADOWS, false));
        assert!(saved.bool_or(ANTI_ALIASING, false));
        assert!(saved.bool_or(PRECIPITATION, false));
        assert_eq!(saved.f32_or(ANISOTROPY, 0.0), 1.0);
        assert_eq!(reflections(&saved), 3);
        // The next open recognises Ultra.
        let s = Options::new(&ui.core);
        let menu = s.view.id(QUALITY_MENU).unwrap();
        assert_eq!(s.view.selected_text(menu).as_deref(), Some("Ultra"));
    }

    #[test]
    fn ui_size_menu_saves_the_scale_and_the_interface_follows() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let menu = s.view.id(UI_SCALE_MENU).unwrap();
        assert_eq!(s.view.selected_text(menu).as_deref(), Some("Auto"));
        s.view.select(menu, Some(150));
        click(&mut s, "done", &mut ui);
        assert_eq!(saved_prefs(&mut ui).get(crate::ui::UI_SCALE), Some("150"));
        // 1920x1080: Auto is 2x; 150% gives 1280x720 logical pixels.
        ui.resize((1920, 1080), None);
        assert_eq!(ui.scale(), 1.5);
        assert_eq!(ui.logical_size(), (1280, 720));
        // Never below the 640x480 layouts: 300% fits 2.25x at 1080p.
        ui.core.prefs.set(crate::ui::UI_SCALE, "300");
        ui.update(0);
        assert_eq!(ui.scale(), 2.25);
        ui.core.prefs.set(crate::ui::UI_SCALE, "0");
        ui.update(0);
        assert_eq!(ui.scale(), 2.0);
    }

    #[test]
    fn color_vision_menu_saves_the_mode() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let menu = s.view.id(COLOR_VISION_MENU).unwrap();
        assert_eq!(s.view.selected_text(menu).as_deref(), Some("Off"));
        s.view.select(menu, Some(2));
        click(&mut s, "done", &mut ui);
        assert_eq!(color_vision(&saved_prefs(&mut ui)), 2);
        ui.core.prefs.set(COLOR_VISION, "9");
        assert_eq!(color_vision(&ui.core.prefs), 3);
    }

    #[test]
    fn max_fps_defaults_to_unlimited_and_saves_the_chosen_cap() {
        let mut ui = fixture();
        assert_eq!(max_fps(&ui.core.prefs), None);
        let mut s = Options::new(&ui.core);
        let menu = s.view.id(MAX_FPS_MENU).unwrap();
        assert_eq!(s.view.selected_text(menu).as_deref(), Some("Unlimited"));
        s.view.select(menu, Some(144));
        click(&mut s, "done", &mut ui);
        assert_eq!(saved_prefs(&mut ui).get(MAX_FPS), Some("144"));
        assert_eq!(max_fps(&ui.core.prefs), Some(144));
        // A value typed in the console that the menu lacks still shows.
        ui.core.prefs.set(MAX_FPS, "50");
        let s = Options::new(&ui.core);
        let menu = s.view.id(MAX_FPS_MENU).unwrap();
        assert_eq!(s.view.selected_text(menu).as_deref(), Some("50"));
        ui.core.prefs.set(MAX_FPS, "-3");
        assert_eq!(max_fps(&ui.core.prefs), None);
    }

    #[test]
    fn music_volume_and_readouts_follow_their_sliders() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let music = s.view.id(MUSIC_SLIDER).unwrap();
        let text = |s: &Options, name: &str| s.view.text_of(s.view.id(&format!("{name}Value")).unwrap());
        assert_eq!(text(&s, MUSIC_SLIDER), "100%");
        assert_eq!(text(&s, "SliderGraphicsAnisotropy"), "8x");
        // Dragging previews the volume at once.
        s.view.set_num(music, 0.4);
        change(&mut s, &mut ui, music);
        assert_eq!(text(&s, MUSIC_SLIDER), "40%");
        let previews: Vec<_> = ui
            .drain_actions()
            .into_iter()
            .filter_map(|(_, a)| match a {
                UiAction::SetVolume { channel, value } => Some((channel, value)),
                _ => None,
            })
            .collect();
        assert_eq!(previews, [("music".to_string(), 0.4)]);
        // Leaving without Done restores what was playing before.
        s.on_sleep(&mut ui.core);
        let restored: Vec<_> = ui
            .drain_actions()
            .into_iter()
            .filter_map(|(_, a)| match a {
                UiAction::SetVolume { channel, value } => Some((channel, value)),
                _ => None,
            })
            .collect();
        assert!(restored.contains(&("music".to_string(), 1.0)));
        assert_eq!(ui.core.prefs.get(MUSIC_VOLUME), None);
        // Done keeps it.
        let mut s = Options::new(&ui.core);
        let music = s.view.id(MUSIC_SLIDER).unwrap();
        s.view.set_num(music, 0.25);
        click(&mut s, "done", &mut ui);
        assert_eq!(saved_prefs(&mut ui).f32_or(MUSIC_VOLUME, 1.0), 0.25);
        assert_eq!(readout(FOV_SLIDER, 90.0), "90");
        assert_eq!(readout("SliderControlsMouseSensitivity", 0.75), "0.75");
        assert_eq!(readout("SliderGraphicsAnisotropy", 0.0), "Off");
    }

    #[test]
    fn max_draw_distance_slider_saves_the_cap() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let n = s.view.id(DISTANCE_SLIDER).unwrap();
        assert!(s.view.node(n).state.visible);
        assert_eq!(s.view.num(n), 1000.0);
        s.view.set_num(n, 400.0);
        change(&mut s, &mut ui, n);
        click(&mut s, "done", &mut ui);
        let saved = saved_prefs(&mut ui);
        assert_eq!(visible_distance_max(&saved), 400.0);
        assert_eq!(readout(DISTANCE_SLIDER, 400.4), "400");
        let mut p = Prefs::default();
        p.set(VISIBLE_DISTANCE_MAX, "5");
        assert_eq!(visible_distance_max(&p), 110.0);
    }

    #[test]
    fn toggle_crouch_is_a_controls_checkbox_saved_on_done() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let n = s.view.id("OptInputToggleCrouch").unwrap();
        assert!(!s.view.bool_value(n));
        s.view.set_bool(n, true);
        change(&mut s, &mut ui, n);
        click(&mut s, "done", &mut ui);
        assert!(saved_prefs(&mut ui).bool_or(TOGGLE_CROUCH, false));
    }

    #[test]
    fn captions_are_an_audio_checkbox_saved_on_done() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let n = s.view.id("OptAudioCaptions").unwrap();
        assert!(!s.view.bool_value(n));
        s.view.set_bool(n, true);
        change(&mut s, &mut ui, n);
        click(&mut s, "done", &mut ui);
        assert!(saved_prefs(&mut ui).bool_or(CAPTIONS, false));
    }
    #[test]
    fn invert_mouse_in_vehicles_shows_on_by_default_and_saves_on_done() {
        // The pack carries stock v20's defaults: invert on, both steering
        // prefs on, which the native defaults turn off.
        let mut data = fixture().core.pack.data.clone();
        data.data.prefs.insert(VEHICLE_MOUSE_INVERT.into(), "1".into());
        data.data.prefs.insert(USE_STRAFE_STEERING.into(), "1".into());
        data.data.prefs.insert(USE_AUTO_RETURN_STEERING.into(), "1".into());
        let mut ui = Ui::new(
            Rc::new(Pack::from_parts(data, Default::default())),
            UiConfig {
                size: (1280, 720),
                scale: Some(1.0),
                platform: Platform::Windows,
            },
            Settings {
                binds: Some(vec![]),
                ..Default::default()
            },
        );
        let mut s = Options::new(&ui.core);
        assert!(!ui.core.prefs.bool_or(USE_STRAFE_STEERING, true));
        assert!(!ui.core.prefs.bool_or(USE_AUTO_RETURN_STEERING, true));
        let n = s.view.id("OptVehicleInvert").unwrap();
        assert!(s.view.node(n).state.visible);
        assert!(s.view.bool_value(n));
        s.view.set_bool(n, false);
        change(&mut s, &mut ui, n);
        click(&mut s, "done", &mut ui);
        assert!(!saved_prefs(&mut ui).bool_or(VEHICLE_MOUSE_INVERT, true));
    }

    #[test]
    fn mute_in_background_is_an_audio_checkbox_saved_on_done() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let n = s.view.id("OptAudioMuteInBackground").unwrap();
        assert!(!s.view.bool_value(n));
        s.view.set_bool(n, true);
        change(&mut s, &mut ui, n);
        click(&mut s, "done", &mut ui);
        assert!(saved_prefs(&mut ui).bool_or(MUTE_IN_BACKGROUND, false));
    }

    #[test]
    fn closing_without_done_discards_draft_audio_prefs_and_remaps() {
        let mut ui = fixture();
        let before = ui.core.binds.clone();
        let mut s = Options::new(&ui.core);
        s.on_wake(&mut ui.core);
        assert!(ui.core.options_open);
        let n = s.view.id("OptAudioVolumeMaster").unwrap();
        s.view.set_num(n, 0.25);
        ui.core.binds.force_remap("moveforward", key(Key::Up));
        s.on_sleep(&mut ui.core);
        assert_eq!(ui.core.binds, before);
        assert!(!ui.core.options_open);
        assert!(ui.drain_actions().is_empty());
        assert_eq!(ui.core.prefs.get("$pref::Audio::masterVolume"), None);
    }
    #[test]
    fn done_commits_supported_values_and_emits_typed_audio() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let n = s.view.id("OptAudioVolumeMaster").unwrap();
        s.view.set_num(n, 0.25);
        let n = s.view.id("Opt_MaxChatLines").unwrap();
        s.view.set_text(n, "900");
        // Three controls intentionally share one authored name.
        let n = s
            .view
            .walk()
            .find(|&n| s.view.node(n).ctrl.variable.as_deref() == Some("$pref::HUD::HideBrickBox"))
            .unwrap();
        s.view.set_bool(n, true);
        assert!(!s.view.node(s.view.id("unsupported").unwrap()).state.visible);
        click(&mut s, "done", &mut ui);
        assert_eq!(ui.core.chat.max_lines, 100);
        assert!(ui.core.hud.prefs.hide_brick_box);
        let actions = ui.drain_actions();
        assert!(actions.iter().any(|(_,a)|matches!(a,UiAction::SetVolume{channel,value} if channel=="master" && *value==0.25)));
        assert!(
            actions
                .iter()
                .any(|(_, a)| matches!(a, UiAction::SaveSettings(_)))
        );
        assert!(
            !actions
                .iter()
                .any(|(_, a)| matches!(a, UiAction::ApplyDisplay { .. }))
        );
    }
    #[test]
    fn escape_is_done_as_in_v20() {
        let mut ui = fixture();
        ui.core.push(ScreenId::Options);
        ui.apply(UiUpdate::Maps(vec![]));
        assert_eq!(ui.top_id(), ScreenId::Options);
        ui.core.binds.force_remap("moveforward", key(Key::Up));
        ui.handle_input(InputEvent::KeyDown {
            key: Key::Escape,
            mods: Modifiers::NONE,
            repeat: false,
        });
        assert!(!ui.is_open(ScreenId::Options));
        assert_eq!(ui.core.binds.binding_of("moveforward"), Some(key(Key::Up)));
        assert!(
            ui.drain_actions()
                .iter()
                .any(|(_, a)| matches!(a, UiAction::SaveSettings(_)))
        );
    }
    #[test]
    fn clear_all_confirms_then_unbinds_only_remappable_controls() {
        let mut ui = fixture();
        ui.core.binds.bind(key(Key::F(9)), "toggleConsole");
        let mut s = Options::new(&ui.core);
        click(&mut s, "clear", &mut ui);
        ui.apply(UiUpdate::Maps(vec![]));
        assert_eq!(ui.top_id(), ScreenId::MessageBox);
        assert!(ui.core.binds.binding_of("moveforward").is_some());
        ui.handle_input(InputEvent::KeyDown {
            key: Key::Return,
            mods: Modifiers::NONE,
            repeat: false,
        });
        assert!(!ui.is_open(ScreenId::MessageBox));
        assert_eq!(ui.core.binds.binding_of("moveforward"), None);
        assert_eq!(ui.core.binds.binding_of("movebackward"), None);
        assert_eq!(
            ui.core.binds.binding_of("toggleConsole"),
            Some(key(Key::F(9)))
        );
    }
    #[test]
    fn chat_size_radio_restyles_example_and_saves_on_done() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let four = s.view.id("OPT_ChatSize4").unwrap();
        assert!(s.view.bool_value(four), "v20 default chat size is 4");
        click(&mut s, "OPT_ChatSize2", &mut ui);
        assert!(s.view.bool_value(s.view.id("OPT_ChatSize2").unwrap()));
        assert_eq!(ui.core.prefs.get(CHAT_SIZE), None);
        click(&mut s, "done", &mut ui);
        assert_eq!(chat_size(&ui.core.prefs), 2);
    }
    #[test]
    fn graphics_filters_shadows_and_anti_aliasing_save_on_done() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        assert!(
            s.view.bool_value(s.view.id("OPT_ShadowQuality0").unwrap()),
            "v20 default shadow quality is Best"
        );
        for pref in ["$pref::OpenGL::textureTrilinear", ANTI_ALIASING] {
            let n = audio_node(&s, pref);
            assert!(s.view.node(n).state.visible && s.view.bool_value(n), "{pref}");
        }
        let sharp = audio_node(&s, "$pref::OpenGL::useGLNearest");
        assert!(s.view.node(sharp).state.visible && !s.view.bool_value(sharp));
        let bricks = audio_node(&s, BRICK_SHADOWS);
        assert!(s.view.node(bricks).state.visible && !s.view.bool_value(bricks));
        toggle_audio(&mut s, &mut ui, BRICK_SHADOWS, true);
        toggle_audio(&mut s, &mut ui, ANTI_ALIASING, false);
        toggle_audio(&mut s, &mut ui, "$pref::OpenGL::useGLNearest", true);
        s.slider("SliderGraphicsAnisotropy", 0.5);
        click(&mut s, "OPT_ShadowQuality3", &mut ui);
        assert_eq!(ui.core.prefs.get(SHADOW_QUALITY), None);
        click(&mut s, "done", &mut ui);
        let p = &ui.core.prefs;
        assert_eq!(p.i64_or(SHADOW_QUALITY, 0), 3);
        assert!(!p.bool_or(ANTI_ALIASING, true));
        assert!(p.bool_or(BRICK_SHADOWS, false));
        assert!(p.bool_or("$pref::OpenGL::useGLNearest", false));
        assert!(p.bool_or("$pref::OpenGL::textureTrilinear", false));
        assert_eq!(p.f32_or(ANISOTROPY, 0.0), 0.5);
    }
    #[test]
    fn physics_quality_picks_the_debris_limit_and_the_console_any_other() {
        let mut ui = fixture();
        // A player who never chose: High, as v20's default.
        assert_eq!(debris_limit(&ui.core.prefs), PHYSICS_LIMITS[1] as usize);
        let mut s = Options::new(&ui.core);
        let radio = |s: &Options, q: usize| s.view.bool_value(s.view.id(&format!("{PHYSICS_RADIO}{q}")).unwrap());
        assert!(radio(&s, 1) && !radio(&s, 0));
        assert!(s.view.node(s.view.id("OPT_PhysicsQuality0").unwrap()).state.visible);
        click(&mut s, "OPT_PhysicsQuality0", &mut ui);
        assert!(radio(&s, 0) && !radio(&s, 1));
        assert!(!ui.core.prefs.is_set(MAX_BRICKS), "only Done saves");
        click(&mut s, "done", &mut ui);
        assert_eq!(debris_limit(&ui.core.prefs), PHYSICS_LIMITS[0] as usize);
        assert_eq!(ui.core.prefs.i64_or(PHYSICS_QUALITY, 1), 0);
        // Off: no debris at all.
        let mut s = Options::new(&ui.core);
        click(&mut s, "OPT_PhysicsQuality4", &mut ui);
        click(&mut s, "done", &mut ui);
        assert_eq!(debris_limit(&ui.core.prefs), 0);
        // A console value matches no radio, and reopening Options keeps it.
        ui.core.prefs.set(MAX_BRICKS, "300");
        let mut s = Options::new(&ui.core);
        assert!((0..5).all(|q| s.view.id(&format!("{PHYSICS_RADIO}{q}")).is_none_or(|n| !s.view.bool_value(n))));
        click(&mut s, "done", &mut ui);
        assert_eq!(debris_limit(&ui.core.prefs), 300);
        ui.core.prefs.set(MAX_BRICKS, "100000");
        assert_eq!(debris_limit(&ui.core.prefs), MAX_BRICKS_RANGE.1 as usize);
    }
    #[test]
    fn fov_slider_sits_right_of_its_label_and_saves_whole_degrees() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let n = s.view.id(FOV_SLIDER).unwrap();
        assert!(s.view.node(n).state.visible);
        assert_eq!(s.view.num(n), 90.0, "v20 default");
        let label = s.view.by_text("GuiTextCtrl", "FOV:").unwrap();
        assert!(s.view.node(label).state.visible);
        assert!(labels(&s.view.node(label).ctrl, &s.view.node(n).ctrl));
        s.view.set_num(n, 101.6);
        click(&mut s, "done", &mut ui);
        assert_eq!(ui.core.prefs.get(DEFAULT_FOV), Some("102"));
        ui.core.prefs.set(DEFAULT_FOV, "500");
        assert_eq!(default_fov(&ui.core.prefs), FOV_RANGE.1);
    }
    #[test]
    fn resolution_list_follows_the_fullscreen_toggle_and_the_monitor() {
        let mut ui = fixture();
        ui.core.display_modes = Some(DisplayModes {
            native: (1920, 1080),
            windowed: vec![(800, 600), (1280, 720)],
        });
        let mut s = Options::new(&ui.core);
        let n = s.view.id("OptGraphicsResolutionMenu").unwrap();
        assert_eq!(s.resolutions, vec![(800, 600), (1280, 720)]);
        toggle_audio(&mut s, &mut ui, FULLSCREEN, true);
        assert_eq!(s.resolutions, vec![(1920, 1080)]);
        assert_eq!(s.view.selected(n), Some(0));
        toggle_audio(&mut s, &mut ui, FULLSCREEN, false);
        assert_eq!(s.view.selected(n), Some(1));
        s.view.select(n, Some(0));
        toggle_audio(&mut s, &mut ui, FULLSCREEN, true);
        toggle_audio(&mut s, &mut ui, FULLSCREEN, false);
        assert_eq!(s.selected_display().resolution, (1280, 720));
        assert_eq!(resolution_list(None, true, (1000, 700)).len(), 11);
    }
    #[test]
    fn display_rejection_does_not_commit_then_success_retains_applied_boundary() {
        let mut ui = fixture();
        let mut s = Options::new(&ui.core);
        let n = s.view.id("OptGraphicsResolutionMenu").unwrap();
        let i = s
            .resolutions
            .iter()
            .position(|r| *r == (1280, 720))
            .unwrap();
        s.view.select(n, Some(i as i64));
        click(&mut s, "apply", &mut ui);
        let actions = ui.drain_actions();
        let (id, a) = actions.first().unwrap();
        assert!(matches!(
            a,
            UiAction::ApplyDisplay {
                resolution: (1280, 720),
                ..
            }
        ));
        assert_eq!(ui.core.prefs.get(RESOLUTION), Some("640 480 32"));
        assert!(s.on_result(*id, None, &Err("unsupported mode".into()), &mut ui.core));
        assert!(ui.drain_actions().is_empty());
        assert_eq!(ui.core.prefs.get(RESOLUTION), Some("640 480 32"));
        click(&mut s, "apply", &mut ui);
        let id = ui.drain_actions()[0].0;
        assert!(s.on_result(id, None, &Ok(()), &mut ui.core));
        assert_eq!(ui.core.prefs.get(RESOLUTION), Some("1280 720 32"));
        s.on_sleep(&mut ui.core);
        assert_eq!(ui.core.prefs.get(RESOLUTION), Some("1280 720 32"));
    }
    #[test]
    fn invalid_numbers_do_not_emit_settings_and_do_not_overwrite_concurrent_prefs() {
        let mut ui = fixture();
        ui.core.prefs.set("$pref::Avatar::Hat", "1");
        let mut s = Options::new(&ui.core);
        ui.core.prefs.set("$pref::Avatar::Hat", "3");
        let n = s.view.id("Opt_ChatLineTime").unwrap();
        s.view.set_text(n, "not a number");
        click(&mut s, "done", &mut ui);
        assert!(ui.drain_actions().is_empty());
        s.view.set_text(n, "6500");
        click(&mut s, "done", &mut ui);
        assert_eq!(ui.core.prefs.get("$pref::Avatar::Hat"), Some("3"));
    }
    #[test]
    fn remap_conflict_decline_confirm_clear_and_reserved_inputs() {
        let mut ui = fixture();
        ui.core.remap_target = Some(0);
        let mut r = Remap::new(&ui.core);
        r.capture(key(Key::Letter('s')), &mut ui.core);
        assert!(r.conflict.is_some());
        assert_eq!(
            ui.core.binds.binding_of("moveforward"),
            Some(key(Key::Letter('w')))
        );
        r.capture(key(Key::Letter('n')), &mut ui.core);
        assert!(r.conflict.is_none());
        r.capture(key(Key::Letter('s')), &mut ui.core);
        r.capture(key(Key::Return), &mut ui.core);
        assert_eq!(
            ui.core.binds.binding_of("moveforward"),
            Some(key(Key::Letter('s')))
        );
        assert_eq!(ui.core.binds.binding_of("movebackward"), None);
        ui.core.remap_target = Some(0);
        let mut r = Remap::new(&ui.core);
        ui.core.globals.bind(key(Key::F(1)), "help");
        r.capture(key(Key::F(1)), &mut ui.core);
        assert_eq!(
            ui.core.binds.binding_of("moveforward"),
            Some(key(Key::Letter('s')))
        );
        r.capture(key(Key::Backspace), &mut ui.core);
        assert_eq!(ui.core.binds.binding_of("moveforward"), None);
    }
    #[test]
    fn manager_captures_mouse_wheel_and_reserved_keys_before_globals() {
        let mut ui = fixture();
        ui.core.remap_target = Some(0);
        ui.core.remap_all = true;
        ui.core.push(ScreenId::Remap);
        // A model update flushes the stack; no desktop input or window exists.
        ui.apply(UiUpdate::Maps(vec![]));
        ui.core.globals.bind(key(Key::F(1)), "toggleConsole");
        ui.handle_input(InputEvent::KeyDown {
            key: Key::F(1),
            mods: Modifiers::NONE,
            repeat: false,
        });
        assert!(ui.drain_actions().is_empty());
        ui.handle_input(InputEvent::MouseDown {
            button: MouseButton::Right,
            x: 200.0,
            y: 200.0,
        });
        assert_eq!(
            ui.core.binds.binding_of("moveforward"),
            Some(BindInput::Mouse(MouseButton::Right))
        );
        assert_eq!(ui.top_id(), ScreenId::Remap);
        ui.handle_input(InputEvent::Wheel { delta: 1.0 });
        assert_eq!(
            ui.core.binds.binding_of("movebackward"),
            Some(BindInput::Wheel)
        );
        assert!(!ui.is_open(ScreenId::Remap));
    }

    #[test]
    #[cfg(feature = "gpu")]
    #[ignore = "bounded headless rendering requires local converted original content and GPU"]
    fn authored_options_save_players_offscreen() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = Rc::new(Pack::load(&root.join("content/ui-pack-001")).unwrap());
        let mut ui = fixture();
        ui.core.pack = pack.clone();
        ui.core.remap = crate::binds::remap_entries(&pack.data.data);
        ui.core.remap_commands = ui.core.remap.iter().map(|r| r.command.clone()).collect();
        ui.core.prefs = Prefs::new(&pack.data.data.prefs, &Default::default());
        ui.core.save_context = Some(("Bedroom".into(), crate::api::IconRef::None));
        ui.core.save_maps = vec!["Bedroom".into()];
        ui.core.save_files = vec![crate::api::SaveFileInfo {
            name: "Test.world.json".into(),
            map: "Bedroom".into(),
            modified: "2026-09-26".into(),
            description: "Native save test".into(),
            brick_count: Some(42),
            damaged: false,
        }];
        let output = root.join("artifacts/ui-native-dialogs");
        std::fs::create_dir_all(&output).unwrap();
        let gpu = crate::gpu::Headless::new().unwrap();
        let mut renderer = crate::gpu::UiRenderer::new(&gpu.device, &gpu.queue);
        let mut render = |name: &str, screen: &mut dyn Screen, core: &mut Core| {
            screen.layout(640, 480, core);
            let mut dl = DrawList::new(Rect::new(0, 0, 640, 480));
            screen.draw(&pack, &mut dl, core);
            assert!(dl.glyph_count() > 20, "{name}");
            let rgba = gpu
                .render_rgba(
                    &mut renderer,
                    &pack,
                    &dl,
                    (640, 480),
                    1.0,
                    [0.12, 0.12, 0.15, 1.0],
                )
                .unwrap();
            image::save_buffer(
                output.join(format!("{name}.png")),
                &rgba,
                640,
                480,
                image::ColorType::Rgba8,
            )
            .unwrap();
        };
        let mut options = Options::new(&ui.core);
        for pane in ["Graphics", "Controls", "Audio", "Network", "AdvGraphics"] {
            options.pane(pane);
            render(&format!("options-{pane}"), &mut options, &mut ui.core);
        }
        ui.core.remap_target = Some(0);
        let mut remap = Remap::new(&ui.core);
        remap.capture(key(Key::Letter('s')), &mut ui.core);
        render("remap-conflict", &mut remap, &mut ui.core);
        for id in [ScreenId::SaveBricks, ScreenId::LoadBricks] {
            let mut screen = crate::screens::saveload::SaveLoad::new(id, &ui.core);
            render(&format!("{id:?}"), &mut screen, &mut ui.core);
        }
        let mut players = crate::screens::players::Players::new(&ui.core);
        render("players", &mut players, &mut ui.core);
    }
}
