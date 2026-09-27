//! Native UI pack schema (`ui-pack.json`), produced offline by `bri-ui-import`.
//!
//! The pack is the only way original v20 UI content reaches the runtime. No
//! Torque reader is linked into `bri-ui`. Every entry keeps its source path and
//! SHA-256 so it can be traced back to the original installation.
//!
//! Identifiers:
//! - image id: lower-case install path without extension, forward slashes,
//!   e.g. `base/client/ui/button1_n` or
//!   `add-ons/print_letters_default/icons/a` (ZIP members use the archive
//!   base name as a folder, exactly like Torque's virtual file system).
//! - font id: `<lower-case face>_<pixel size>`, e.g. `impact_18`,
//!   `palatino linotype_24`.
//! - style id: the original GuiControlProfile name, e.g. `BlockButtonProfile`.
//! - layout id: the original GUI object name, e.g. `BrickSelectorDlg`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PACK_SCHEMA_VERSION: u32 = 1;

/// RGBA, 0..=255, in the original display (gamma) space.
pub type Rgba = [u8; 4];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiPack {
    pub schema_version: u32,
    pub generator: String,
    pub images: BTreeMap<String, ImageEntry>,
    pub fonts: BTreeMap<String, FontEntry>,
    /// Bitmap arrays (Torque `hasBitmapArray` skins), keyed by image id.
    pub skins: BTreeMap<String, SkinEntry>,
    /// Flattened GuiControlProfiles (inheritance already resolved).
    pub styles: BTreeMap<String, Style>,
    /// Authored control trees, keyed by GUI object name.
    pub layouts: BTreeMap<String, Control>,
    pub data: UiData,
    pub maps: Vec<MapEntry>,
    pub sources: Vec<SourceRecord>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEntry {
    /// Path of the stored file, relative to the pack directory. The bytes
    /// are the original file bytes (PNG/JPEG are not re-encoded).
    pub file: String,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
    /// Original location, e.g. `base/client/ui/button1_n.png` or
    /// `Add-Ons/Print_Letters_Default.zip:icons/A.png`.
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontEntry {
    pub face: String,
    pub size: u32,
    pub line_height: u32,
    pub baseline: u32,
    /// Glyph atlas files (8-bit coverage PNGs, the original embedded sheets).
    pub sheets: Vec<String>,
    /// Indexed by character code 0..=255 (Windows-1252); `None` = unmapped.
    pub glyphs: Vec<Option<Glyph>>,
    pub source: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Glyph {
    pub sheet: u16,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    /// Draw at (pen_x + x_origin, top + baseline - y_origin).
    pub x_origin: i16,
    pub y_origin: i16,
    pub advance: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinEntry {
    /// Piece rectangles `[x, y, w, h]` in reading order, as sliced by the
    /// Torque separator-colour rule (pixel (0,0) is the separator).
    pub pieces: Vec<[u32; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Justify {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Style {
    pub font: Option<String>,
    pub font_color: Option<Rgba>,
    pub font_color_hl: Option<Rgba>,
    pub font_color_na: Option<Rgba>,
    pub font_color_sel: Option<Rgba>,
    /// `\c0` .. `\c9` palette.
    pub font_colors: Vec<Option<Rgba>>,
    pub fill_color: Option<Rgba>,
    pub fill_color_hl: Option<Rgba>,
    pub fill_color_na: Option<Rgba>,
    pub border_color: Option<Rgba>,
    pub border_color_hl: Option<Rgba>,
    pub opaque: bool,
    pub border: i32,
    /// Image id of the skin bitmap.
    pub bitmap: Option<String>,
    pub has_bitmap_array: bool,
    pub justify: Justify,
    /// Title/label justification is `center` on macOS for window profiles.
    pub justify_macos: Option<Justify>,
    pub text_offset: [i32; 2],
    pub font_outline: Option<Rgba>,
    pub modal: bool,
    pub can_key_focus: bool,
    pub tab: bool,
    pub source_line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HSizing {
    #[default]
    Right,
    Left,
    Center,
    Width,
    Relative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VSizing {
    #[default]
    Bottom,
    Top,
    Center,
    Height,
    Relative,
}

/// One authored GUI control. `class` keeps the original Torque class name.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Control {
    pub class: String,
    pub name: Option<String>,
    /// Source line in the vanilla GUI decompile (stable identity for unnamed
    /// controls together with the layout id).
    pub source_line: u32,
    pub position: [i32; 2],
    pub extent: [i32; 2],
    pub min_extent: [i32; 2],
    pub h_sizing: HSizing,
    pub v_sizing: VSizing,
    pub style: String,
    pub visible: bool,
    /// Text with `\cN` colour codes as U+E000+N (see `text::COLOR_CODE_BASE`).
    pub text: Option<String>,
    /// Image id (for bitmap buttons: the base id; states are `<id>_n/_h/_d/_i`).
    pub bitmap: Option<String>,
    /// Original TorqueScript command string, kept for identification and
    /// provenance only. It is never executed; screens map it to typed actions.
    pub command: Option<String>,
    pub alt_command: Option<String>,
    pub close_command: Option<String>,
    /// Accelerator key name in Torque spelling (`escape`, `return`, `tab`, `1`).
    pub accelerator: Option<String>,
    pub variable: Option<String>,
    /// `mColor` of bitmap buttons or `color` of swatches.
    pub color: Option<Rgba>,
    pub group: Option<i32>,
    pub button_type: Option<String>,
    /// Remaining authored fields verbatim (`wrap`, `maxLength`, `range`, …).
    pub fields: BTreeMap<String, String>,
    pub children: Vec<Control>,
}

impl Control {
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }
    /// Depth-first search by object name.
    pub fn find(&self, name: &str) -> Option<&Control> {
        if self.name.as_deref() == Some(name) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(name))
    }
    /// Depth-first search by the original command string.
    pub fn find_command(&self, command: &str) -> Option<&Control> {
        if self.command.as_deref() == Some(command) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find_command(command))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiData {
    /// Default server colorset (`setSprayCanColors`), divisions in order.
    pub brick_colorset: Vec<ColorDivision>,
    /// Default avatar colorset (`ColorSetGui::defaults`), 0..1 floats.
    pub avatar_colors: Vec<[f32; 4]>,
    /// Stock brick favorites by slot (`$Favorite::Brick<slot>_<i>`), uiNames.
    pub favorites: BTreeMap<u8, Vec<String>>,
    /// Options → Controls remap list in display order.
    pub remap: Vec<RemapEntry>,
    /// Default binds produced by `defaultControlsGui::apply`.
    pub default_binds: Vec<DefaultBind>,
    /// Global binds (`GlobalActionMap`).
    pub global_binds: Vec<DefaultBind>,
    /// Stock `$pref::` defaults (effective values, last assignment wins).
    pub prefs: BTreeMap<String, String>,
    /// Stock event registrations (`registerInputEvent`/`registerOutputEvent`)
    /// in registration order. These are the reference tables; the host's
    /// event catalog says which ones the rewrite supports.
    #[serde(default)]
    pub event_tables: EventTables,
    /// Avatar part lists (`base/data/shapes/player/*.txt`) and face/decal names.
    #[serde(default)]
    pub avatar: AvatarData,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventTables {
    pub inputs: Vec<InputEventDef>,
    pub outputs: Vec<OutputEventDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputEventDef {
    /// Owning class (always `fxDTSBrick` in stock v20).
    pub class: String,
    pub name: String,
    /// Target choices in order: (display name, target class), e.g.
    /// `("Self", "fxDTSBrick")`, `("Player", "Player")`.
    pub targets: Vec<(String, String)>,
    pub source_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputEventDef {
    pub class: String,
    pub name: String,
    pub params: Vec<ParamSpec>,
    /// Torque `appendClient` (4th argument, default 1).
    pub append_client: bool,
    pub source_line: u32,
}

/// Output parameter widget types (wrenchEventsDlg::createOutputParameters).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ParamSpec {
    Int {
        min: i64,
        max: i64,
        default: i64,
    },
    IntList {
        width: i32,
    },
    Float {
        min: f32,
        max: f32,
        step: f32,
        default: f32,
    },
    Bool,
    String {
        max_length: u32,
        width: i32,
    },
    Datablock {
        class: String,
    },
    Vector {
        max: f32,
    },
    List {
        items: Vec<(String, i64)>,
    },
    PaintColor {
        default: i64,
    },
    /// Unrecognised spec text kept verbatim.
    Unknown {
        text: String,
    },
}

impl ParamSpec {
    /// Parse one Torque parameter spec (`"int -1 300 5"`, `"list Up 0 Down 1"`).
    pub fn parse(spec: &str) -> ParamSpec {
        let w: Vec<&str> = spec.split_whitespace().collect();
        let num = |i: usize| w.get(i).and_then(|v| v.parse::<f64>().ok());
        let kind = w
            .first()
            .map(|k| k.to_ascii_lowercase())
            .unwrap_or_default();
        let unknown = || ParamSpec::Unknown {
            text: spec.to_string(),
        };
        match kind.as_str() {
            "int" => match (num(1), num(2)) {
                (Some(a), Some(b)) => ParamSpec::Int {
                    min: a as i64,
                    max: b as i64,
                    default: num(3).unwrap_or(a) as i64,
                },
                _ => unknown(),
            },
            "intlist" => ParamSpec::IntList {
                width: num(1).unwrap_or(100.0) as i32,
            },
            "float" => match (num(1), num(2), num(3)) {
                (Some(a), Some(b), Some(st)) => ParamSpec::Float {
                    min: a as f32,
                    max: b as f32,
                    step: st as f32,
                    default: num(4).unwrap_or(a) as f32,
                },
                _ => unknown(),
            },
            "bool" => ParamSpec::Bool,
            "string" => ParamSpec::String {
                max_length: num(1).unwrap_or(100.0) as u32,
                width: num(2).unwrap_or(100.0) as i32,
            },
            "datablock" => match w.get(1) {
                Some(c) => ParamSpec::Datablock {
                    class: c.to_string(),
                },
                None => unknown(),
            },
            "vector" => ParamSpec::Vector {
                max: num(1).unwrap_or(0.0) as f32,
            },
            "list" => {
                let mut items = Vec::new();
                let mut i = 1;
                while i + 1 < w.len() {
                    match w[i + 1].parse::<i64>() {
                        Ok(v) => items.push((w[i].to_string(), v)),
                        Err(_) => return unknown(),
                    }
                    i += 2;
                }
                ParamSpec::List { items }
            }
            "paintcolor" => ParamSpec::PaintColor {
                default: num(1).unwrap_or(0.0) as i64,
            },
            _ => unknown(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AvatarData {
    /// Part lists by slot (`hat`, `accent`, `pack`, `secondpack`, `chest`,
    /// `hip`, `rarm`, `larm`, `rhand`, `lhand`, `rleg`, `lleg`) in file order.
    pub parts: BTreeMap<String, Vec<String>>,
    /// Accents allowed per hat (`accent.txt` lines after the first).
    pub accents_allowed: BTreeMap<String, Vec<String>>,
    /// Face and decal names from the face/decal add-ons (and base `decals`).
    pub faces: Vec<String>,
    pub decals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorDivision {
    pub name: String,
    /// 0..1 floats as used by `setColorTable`.
    pub colors: Vec<[f32; 4]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemapEntry {
    /// Section heading that starts at this entry, if any.
    pub division: Option<String>,
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Device {
    Keyboard,
    Mouse,
}

/// A condition atom from the `defaultControlsGui::apply` branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum BindAtom {
    /// `%mouse == n` (0 one button, 1 two button, 2 wheel, 3 tilt wheel).
    Mouse(u8),
    /// `%keyboard == n` (0 standard/numpad, 1 laptop).
    Keyboard(u8),
    /// `isWindows()`.
    Windows,
    /// `getBuildString() $= "Debug"` (never true for players).
    DebugBuild,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultBind {
    pub device: Device,
    /// Torque key spelling including modifiers, e.g. `numpad8`, `ctrl z`,
    /// `alt numpad8`, `button1`, `zaxis`.
    pub key: String,
    /// Bound command (function name) or, for `bindCmd`, the make script.
    pub command: String,
    /// All atoms must hold with the given polarity.
    pub when: Vec<(BindAtom, bool)>,
    pub source_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapEntry {
    /// Mission file path as found, e.g. `Add-Ons/Map_Bedroom/bedroom.mis`.
    pub mission: String,
    pub display_name: String,
    pub description: String,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRecord {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}
