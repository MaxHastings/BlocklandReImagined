//! HUD regions: named parts of the screen an Add-On can claim and draw
//! into from its own data, with textured images, bars, frame strips,
//! sprite-font counters and text. A claim can depend on the active body,
//! the held item and the vehicle seat, so the HUD switches with them. A
//! region an Add-On claims hides the stock element there (the crosshair,
//! the jet energy bar, the center and bottom prints); a region nobody
//! claims shows the stock HUD as before.
//!
//! Coordinates are HUD units: the screen is 480 units tall and as wide as
//! its aspect makes it, so a layout keeps its proportions at any size (an
//! N64 layout drawn at 320x240 doubles straight into it). Each region is a
//! box on that screen ([`Region::rect`]); an element's position is from
//! its region's top left corner, x right and y down. Elements may reach
//! outside their box: the box is where they are placed from, not a clip.
//!
//! What wins when two Add-Ons claim one region ([`Claim::rank`]): the
//! claim whose condition is the most specific (a vehicle seat, then a held
//! item, then a body, then none), then the higher `priority`, then the
//! package id in alphabetical order. The engine reports each such
//! conflict once; the losing layout is not drawn.
use anyhow::{Result, bail, ensure};
use bri_console::Clamp;
use serde::{Deserialize, Serialize};

/// The HUD's height in units.
pub const HEIGHT: f32 = 480.0;
/// Most elements in one layout.
pub const MAX_ELEMENTS: usize = 64;

/// A named part of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    TopLeft,
    TopCenter,
    TopRight,
    Left,
    Right,
    BottomLeft,
    BottomCenter,
    BottomRight,
    /// The stock crosshair.
    Crosshair,
    /// The stock jet energy bar.
    Energy,
    /// The stock center print (rules' `center_print`).
    CenterPrint,
    /// The stock bottom print (rules' `bottom_print`, the ammo display).
    BottomPrint,
    /// Where the stock ammo display shows: claiming it hides the bottom
    /// print only while that print is the ammo display, so rules' other
    /// bottom prints still show.
    Ammo,
}
impl Region {
    pub const ALL: [Region; 13] = [
        Region::TopLeft,
        Region::TopCenter,
        Region::TopRight,
        Region::Left,
        Region::Right,
        Region::BottomLeft,
        Region::BottomCenter,
        Region::BottomRight,
        Region::Crosshair,
        Region::Energy,
        Region::CenterPrint,
        Region::BottomPrint,
        Region::Ammo,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Region::TopLeft => "top_left",
            Region::TopCenter => "top_center",
            Region::TopRight => "top_right",
            Region::Left => "left",
            Region::Right => "right",
            Region::BottomLeft => "bottom_left",
            Region::BottomCenter => "bottom_center",
            Region::BottomRight => "bottom_right",
            Region::Crosshair => "crosshair",
            Region::Energy => "energy",
            Region::CenterPrint => "center_print",
            Region::BottomPrint => "bottom_print",
            Region::Ammo => "ammo",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.name() == name)
    }
    /// The stock element a claim on this region hides, if any.
    pub fn stock(self) -> Option<&'static str> {
        match self {
            Region::Crosshair => Some("crosshair"),
            Region::Energy => Some("jet energy bar"),
            Region::CenterPrint => Some("center print"),
            Region::BottomPrint => Some("bottom print"),
            Region::Ammo => Some("ammo display"),
            _ => None,
        }
    }
    /// The region's box in HUD units (x, y, width, height) on a screen
    /// `width` units wide and [`HEIGHT`] tall.
    pub fn rect(self, width: f32) -> [f32; 4] {
        let w = if width.is_finite() {
            width.max(1.0)
        } else {
            HEIGHT
        };
        let center = |box_width: f32| (w - box_width) / 2.0;
        match self {
            Region::TopLeft => [0.0, 0.0, 240.0, 120.0],
            Region::TopCenter => [center(240.0), 0.0, 240.0, 120.0],
            Region::TopRight => [w - 240.0, 0.0, 240.0, 120.0],
            Region::Left => [0.0, 180.0, 120.0, 120.0],
            Region::Right => [w - 120.0, 180.0, 120.0, 120.0],
            Region::BottomLeft => [0.0, 360.0, 240.0, 120.0],
            Region::BottomCenter => [center(240.0), 360.0, 240.0, 120.0],
            Region::BottomRight => [w - 240.0, 360.0, 240.0, 120.0],
            Region::Crosshair => [center(64.0), 208.0, 64.0, 64.0],
            Region::Energy => [w - 48.0, 240.0, 48.0, 120.0],
            Region::CenterPrint => [center(480.0), 160.0, 480.0, 120.0],
            Region::BottomPrint => [0.0, 400.0, w, 80.0],
            Region::Ammo => [w - 240.0, 400.0, 240.0, 80.0],
        }
    }
}

/// A HUD file that claims a region (`"region"`): when it applies and what
/// it draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub schema_version: u32,
    pub region: Region,
    #[serde(default, skip_serializing_if = "When::is_always")]
    pub when: When,
    /// Breaks a tie with another claim of the same specificity; higher
    /// wins. -100 to 100.
    #[serde(default)]
    pub priority: i32,
    /// Draw images with sharp pixels (nearest filtering) rather than
    /// smoothed: pixel art and sprite fonts.
    #[serde(default)]
    pub pixelated: bool,
    /// What it draws, back to front. Empty claims the region to hide the
    /// stock element there, or for the Add-On's own client code to draw
    /// into (`hud_region`).
    #[serde(default)]
    pub elements: Vec<Element>,
}

/// When a claim applies: every list given must match (an empty list always
/// does). Ids are full content ids.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct When {
    /// The viewer's body is one of these archetypes
    /// (`namespace:archetype/name`, or a v20 `v20.player.<datablock>`).
    pub body: Vec<String>,
    /// The viewer holds one of these in hand: a weapons Add-On's id (any
    /// of its images) or an image id.
    pub holding: Vec<String>,
    /// The viewer sits in one of these vehicles (`namespace:vehicle/name`,
    /// or a v20 `v20.vehicle.<datablock>`).
    pub vehicle: Vec<String>,
}
impl When {
    pub fn is_always(&self) -> bool {
        self.body.is_empty() && self.holding.is_empty() && self.vehicle.is_empty()
    }
}

/// What the viewer is this frame, for [`When`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Viewer<'a> {
    /// The body's archetype id.
    pub body: &'a str,
    /// The image in the right hand, `""` for none.
    pub holding: &'a str,
    /// The vehicle definition sat in, `""` on foot.
    pub vehicle: &'a str,
}
fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}
impl When {
    pub fn matches(&self, viewer: &Viewer<'_>) -> bool {
        let body = self.body.is_empty() || self.body.iter().any(|b| same(b, viewer.body));
        let holding = self.holding.is_empty()
            || (!viewer.holding.is_empty()
                && self.holding.iter().any(|h| {
                    same(h, viewer.holding)
                        || viewer
                            .holding
                            .split_once(':')
                            .is_some_and(|(package, _)| same(package, h))
                }));
        let vehicle = self.vehicle.is_empty()
            || (!viewer.vehicle.is_empty() && self.vehicle.iter().any(|v| same(v, viewer.vehicle)));
        body && holding && vehicle
    }
    /// How specific the claim is: a vehicle seat (8), then a held item (4),
    /// then a body (2), added together.
    pub fn specificity(&self) -> u8 {
        u8::from(!self.vehicle.is_empty()) * 8
            + u8::from(!self.holding.is_empty()) * 4
            + u8::from(!self.body.is_empty()) * 2
    }
}

/// A claim on a region by one Add-On's layout, for ranking.
#[derive(Debug, Clone, Copy)]
pub struct Claim<'a> {
    pub package: &'a str,
    pub layout: &'a Layout,
}
impl Claim<'_> {
    /// Sorts the winning claim first: most specific, then highest
    /// priority, then package id.
    pub fn rank(&self) -> (std::cmp::Reverse<u8>, std::cmp::Reverse<i32>, String) {
        (
            std::cmp::Reverse(self.layout.when.specificity()),
            std::cmp::Reverse(self.layout.priority),
            self.package.to_ascii_lowercase(),
        )
    }
}

/// One thing a layout draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Element {
    /// A picture, or one frame of a strip picked by a value.
    Image {
        texture: String,
        /// x, y, width, height in units from the region's corner.
        rect: [f32; 4],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        frames: Option<Frames>,
        #[serde(default = "white")]
        tint: [f32; 4],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show: Option<Show>,
    },
    /// A bar filled by a value's share of its maximum, as a colour or a
    /// texture revealed from one end.
    Bar {
        rect: [f32; 4],
        value: String,
        /// The full bar: a number or a source. Left out, the value's own
        /// maximum (`health_max` for `health`, 1 for `energy`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<Amount>,
        fill: Paint,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        back: Option<Paint>,
        #[serde(default)]
        direction: Direction,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show: Option<Show>,
    },
    /// Text: a fixed line, a value, or a line with `{}` where the value
    /// goes. With `glyphs`, drawn from the Add-On's own sprite font.
    Text {
        /// Where the line starts (left), centres or ends (right), and its
        /// top, in units from the region's corner.
        at: [f32; 2],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        /// Numbers are padded with zeros to this many digits.
        #[serde(default)]
        digits: u8,
        #[serde(default)]
        align: Align,
        #[serde(default = "white")]
        color: [f32; 4],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        glyphs: Option<Glyphs>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show: Option<Show>,
    },
}
fn white() -> [f32; 4] {
    [1.0; 4]
}

/// A strip of equal frames in one texture, left to right then down. The
/// value's share of its maximum picks the frame: the first frame at 0
/// (empty), the last at the maximum (full).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frames {
    pub count: u16,
    /// Frames across one row of the texture; all of them when left out.
    #[serde(default)]
    pub columns: u16,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<Amount>,
}

/// A number, or a source to read one from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Amount {
    Number(f32),
    Source(String),
}

/// A bar's colour, or a texture tinted by it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<String>,
    #[serde(default = "white")]
    pub color: [f32; 4],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Fills from the left.
    #[default]
    Right,
    Left,
    /// Fills from the bottom.
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// A sprite font: one texture of equal cells holding `charset`'s
/// characters in order, left to right then down.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Glyphs {
    pub texture: String,
    pub charset: String,
    /// Cells across one row of the texture; all of them when left out.
    #[serde(default)]
    pub columns: u16,
    /// One character's drawn size, units.
    pub size: [f32; 2],
    /// Extra room after each character, units (negative packs them).
    #[serde(default)]
    pub spacing: f32,
}

/// Show an element only while a value is above `above` and below `below`
/// (either left out is no limit): a low-health warning, a reload prompt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Show {
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub above: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub below: Option<f32>,
}

/// Where a value comes from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// The viewer's health, and their body's full health.
    Health,
    HealthMax,
    /// Jet energy, 0 to 1 (0 for a body with no jet).
    Energy,
    /// 1 while alive, 0 while dead.
    Alive,
    /// The stock prints' text, without markup (`""` when none shows).
    CenterPrint,
    BottomPrint,
    /// The held gun's rounds ready to fire (for a gun counted from its
    /// reserve, the reserve), its full magazine, the rounds carried beyond
    /// it, and 1 while it reloads. Missing with no such gun, and `ammo` and
    /// `ammo_reserve` when they never run out.
    Ammo,
    AmmoMax,
    AmmoReserve,
    Reloading,
    /// A rule's public state: `package:player/key` or `package:global/key`,
    /// read as a HUD panel row reads it.
    State(String),
}
impl Source {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "health" => Source::Health,
            "health_max" => Source::HealthMax,
            "energy" => Source::Energy,
            "alive" => Source::Alive,
            "center_print" => Source::CenterPrint,
            "bottom_print" => Source::BottomPrint,
            "ammo" => Source::Ammo,
            "ammo_max" => Source::AmmoMax,
            "ammo_reserve" => Source::AmmoReserve,
            "reloading" => Source::Reloading,
            state => {
                let (package, rest) = state.split_once(':')?;
                let (scope, key) = rest.split_once('/')?;
                let ok = |s: &str| {
                    !s.is_empty()
                        && s.len() <= 64
                        && s.chars()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
                };
                if !(ok(package) && ok(key) && matches!(scope, "player" | "global")) {
                    return None;
                }
                Source::State(state.to_string())
            }
        })
    }
    /// The value's natural maximum, when a bar or frame strip names none.
    pub fn natural_max(&self) -> Amount {
        match self {
            Source::Health => Amount::Source("health_max".into()),
            Source::Ammo => Amount::Source("ammo_max".into()),
            _ => Amount::Number(1.0),
        }
    }
}

/// A value read for a HUD.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f32),
    Text(String),
}
impl Value {
    pub fn number(&self) -> Option<f32> {
        match self {
            Value::Number(n) if n.is_finite() => Some(*n),
            Value::Number(_) => None,
            Value::Text(t) => t.trim().parse().ok().filter(|n: &f32| n.is_finite()),
        }
    }
}

fn color_ok(c: &[f32; 4]) -> bool {
    c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}
fn rect_ok(r: &[f32; 4]) -> bool {
    r.iter().all(|v| v.is_finite() && v.abs() <= 4096.0) && r[2] >= 0.0 && r[3] >= 0.0
}
fn source_ok(s: &str) -> Result<()> {
    ensure!(
        Source::parse(s).is_some(),
        "`{s}` is not a HUD value: health, health_max, energy, alive, center_print, bottom_print, ammo, ammo_max, ammo_reserve, reloading, or a rule's package:player/key or package:global/key"
    );
    Ok(())
}
fn amount_ok(a: &Amount) -> Result<()> {
    match a {
        Amount::Number(n) => ensure!(n.is_finite(), "a max must be a finite number"),
        Amount::Source(s) => source_ok(s)?,
    }
    Ok(())
}
fn texture_ok(t: &str) -> Result<()> {
    ensure!(
        !t.is_empty() && t.len() <= 160 && t.contains(":texture/"),
        "`{t}` is not a texture id (namespace:texture/name)"
    );
    Ok(())
}
fn show_ok(s: &Option<Show>) -> Result<()> {
    if let Some(s) = s {
        source_ok(&s.value)?;
        ensure!(
            s.above.is_none_or(f32::is_finite) && s.below.is_none_or(f32::is_finite),
            "show limits must be finite"
        );
    }
    Ok(())
}
impl Layout {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "hud region schema_version must be 1"
        );
        ensure!(
            (-100..=100).contains(&self.priority),
            "priority must be -100 to 100"
        );
        let ids = |list: &[String], what: &str| -> Result<()> {
            ensure!(list.len() <= 32, "at most 32 {what} entries");
            for id in list {
                ensure!(
                    !id.trim().is_empty() && id.len() <= 160 && !id.chars().any(char::is_control),
                    "`{id}` is not a {what} id"
                );
            }
            Ok(())
        };
        ids(&self.when.body, "body")?;
        ids(&self.when.holding, "holding")?;
        ids(&self.when.vehicle, "vehicle")?;
        ensure!(
            self.elements.len() <= MAX_ELEMENTS,
            "at most {MAX_ELEMENTS} elements"
        );
        for e in &self.elements {
            match e {
                Element::Image {
                    texture,
                    rect,
                    frames,
                    tint,
                    show,
                } => {
                    texture_ok(texture)?;
                    ensure!(
                        rect_ok(rect),
                        "image rect must be finite with no negative size"
                    );
                    ensure!(color_ok(tint), "tint must be 0..1 RGBA");
                    if let Some(f) = frames {
                        ensure!(
                            (1..=256).contains(&f.count) && f.columns <= f.count,
                            "frames: count 1 to 256, columns at most count"
                        );
                        source_ok(&f.value)?;
                        if let Some(max) = &f.max {
                            amount_ok(max)?;
                        }
                    }
                    show_ok(show)?;
                }
                Element::Bar {
                    rect,
                    value,
                    max,
                    fill,
                    back,
                    show,
                    ..
                } => {
                    ensure!(
                        rect_ok(rect),
                        "bar rect must be finite with no negative size"
                    );
                    source_ok(value)?;
                    if let Some(max) = max {
                        amount_ok(max)?;
                    }
                    for paint in std::iter::once(fill).chain(back) {
                        ensure!(color_ok(&paint.color), "bar colors must be 0..1 RGBA");
                        if let Some(t) = &paint.texture {
                            texture_ok(t)?;
                        }
                    }
                    show_ok(show)?;
                }
                Element::Text {
                    at,
                    text,
                    value,
                    digits,
                    color,
                    glyphs,
                    show,
                    ..
                } => {
                    ensure!(
                        at.iter().all(|v| v.is_finite() && v.abs() <= 4096.0),
                        "text position must be finite"
                    );
                    ensure!(
                        text.is_some() || value.is_some(),
                        "text needs `text`, `value` or both"
                    );
                    if let Some(t) = text {
                        ensure!(
                            t.chars().count() <= 64 && !t.chars().any(char::is_control),
                            "text is at most 64 printable characters"
                        );
                    }
                    if let Some(v) = value {
                        source_ok(v)?;
                    }
                    ensure!(*digits <= 9, "digits is at most 9");
                    ensure!(color_ok(color), "text color must be 0..1 RGBA");
                    if let Some(g) = glyphs {
                        texture_ok(&g.texture)?;
                        let count = g.charset.chars().count();
                        ensure!(
                            (1..=256).contains(&count) && usize::from(g.columns) <= count,
                            "glyphs: 1 to 256 characters, columns at most that"
                        );
                        ensure!(
                            g.size
                                .iter()
                                .all(|v| v.is_finite() && *v > 0.0 && *v <= 480.0)
                                && g.spacing.is_finite()
                                && g.spacing.abs() <= 480.0,
                            "glyph size must be positive and spacing finite"
                        );
                    }
                    show_ok(show)?;
                }
            }
        }
        Ok(())
    }
    /// Every texture id the layout draws.
    pub fn textures(&self) -> Vec<&str> {
        let mut out = Vec::new();
        for e in &self.elements {
            match e {
                Element::Image { texture, .. } => out.push(texture.as_str()),
                Element::Bar { fill, back, .. } => out.extend(
                    std::iter::once(fill)
                        .chain(back)
                        .filter_map(|p| p.texture.as_deref()),
                ),
                Element::Text { glyphs, .. } => {
                    out.extend(glyphs.as_ref().map(|g| g.texture.as_str()));
                }
            }
        }
        out
    }
    /// Every source the layout reads.
    pub fn sources(&self) -> Vec<Source> {
        fn amount<'a>(a: &'a Option<Amount>, names: &mut Vec<&'a str>) {
            if let Some(Amount::Source(s)) = a {
                names.push(s.as_str());
            }
        }
        let mut names: Vec<&str> = Vec::new();
        for e in &self.elements {
            match e {
                Element::Image { frames, show, .. } => {
                    if let Some(f) = frames {
                        names.push(&f.value);
                        amount(&f.max, &mut names);
                    }
                    names.extend(show.as_ref().map(|s| s.value.as_str()));
                }
                Element::Bar {
                    value, max, show, ..
                } => {
                    names.push(value);
                    amount(max, &mut names);
                    names.extend(show.as_ref().map(|s| s.value.as_str()));
                }
                Element::Text { value, show, .. } => {
                    names.extend(value.as_deref());
                    names.extend(show.as_ref().map(|s| s.value.as_str()));
                }
            }
        }
        let mut out: Vec<_> = names.into_iter().filter_map(Source::parse).collect();
        out.sort();
        out.dedup();
        out
    }
}

/// What a layout draws this frame, resolved: positions in units from the
/// region's corner.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// Part of a texture (`uv`: left, top, right, bottom, 0 to 1).
    Image {
        texture: u64,
        uv: [f32; 4],
        rect: [f32; 4],
        tint: [f32; 4],
    },
    Fill {
        rect: [f32; 4],
        color: [f32; 4],
    },
    /// A line in the stock HUD font.
    Text {
        at: [f32; 2],
        text: String,
        color: [f32; 4],
        align: Align,
    },
}

/// A texture a layout draws: its key for the renderer and its size in
/// pixels.
pub type TextureInfo = (u64, [u32; 2]);

/// Resolve `layout` for this frame: `value` reads a source, `texture`
/// finds a texture. An element whose texture is missing is left out; a
/// value that cannot be read shows as empty (0, or `-` in text).
pub fn resolve(
    layout: &Layout,
    value: &dyn Fn(&Source) -> Option<Value>,
    texture: &dyn Fn(&str) -> Option<TextureInfo>,
) -> Vec<Item> {
    let read = |name: &str| Source::parse(name).and_then(|s| value(&s));
    let number = |name: &str| read(name).and_then(|v| v.number());
    let amount = |a: &Amount| match a {
        Amount::Number(n) => Some(*n),
        Amount::Source(s) => number(s),
    };
    let share = |name: &str, max: &Option<Amount>| -> f32 {
        let max = max
            .clone()
            .or_else(|| Source::parse(name).map(|s| s.natural_max()));
        let (Some(v), Some(m)) = (number(name), max.as_ref().and_then(amount)) else {
            return 0.0;
        };
        if m <= 0.0 {
            0.0
        } else {
            (v / m).clamped(0.0, 1.0)
        }
    };
    let shown = |show: &Option<Show>| {
        show.as_ref().is_none_or(|s| {
            let v = number(&s.value).unwrap_or(0.0);
            s.above.is_none_or(|a| v > a) && s.below.is_none_or(|b| v < b)
        })
    };
    let mut out = Vec::new();
    for e in &layout.elements {
        match e {
            Element::Image {
                texture: id,
                rect,
                frames,
                tint,
                show,
            } => {
                let Some((key, _)) = texture(id).filter(|_| shown(show)) else {
                    continue;
                };
                let uv = match frames {
                    None => [0.0, 0.0, 1.0, 1.0],
                    Some(f) => {
                        let last = f32::from(f.count - 1);
                        let frame = (share(&f.value, &f.max) * last).round() as u16;
                        cell(frame, f.count, f.columns)
                    }
                };
                out.push(Item::Image {
                    texture: key,
                    uv,
                    rect: *rect,
                    tint: *tint,
                });
            }
            Element::Bar {
                rect,
                value,
                max,
                fill,
                back,
                direction,
                show,
            } => {
                if !shown(show) {
                    continue;
                }
                if let Some(back) = back {
                    out.extend(paint(back, *rect, [0.0, 0.0, 1.0, 1.0], texture));
                }
                let s = share(value, max);
                let [x, y, w, h] = *rect;
                let (part, uv) = match direction {
                    Direction::Right => ([x, y, w * s, h], [0.0, 0.0, s, 1.0]),
                    Direction::Left => ([x + w * (1.0 - s), y, w * s, h], [1.0 - s, 0.0, 1.0, 1.0]),
                    Direction::Up => ([x, y + h * (1.0 - s), w, h * s], [0.0, 1.0 - s, 1.0, 1.0]),
                    Direction::Down => ([x, y, w, h * s], [0.0, 0.0, 1.0, s]),
                };
                if s > 0.0 {
                    out.extend(paint(fill, part, uv, texture));
                }
            }
            Element::Text {
                at,
                text,
                value: source,
                digits,
                align,
                color,
                glyphs,
                show,
            } => {
                if !shown(show) {
                    continue;
                }
                let shown_value = source.as_deref().map(|s| match read(s) {
                    Some(Value::Number(n)) if n.is_finite() => {
                        let n = n.round() as i64;
                        let width = usize::from(*digits);
                        if n < 0 {
                            format!("-{:0width$}", -n)
                        } else {
                            format!("{n:0width$}")
                        }
                    }
                    Some(Value::Text(t)) => plain(&t),
                    _ => "-".into(),
                });
                let line = match (text, shown_value) {
                    (Some(t), Some(v)) if t.contains("{}") => t.replacen("{}", &v, 1),
                    (Some(t), Some(v)) => format!("{t}{v}"),
                    (Some(t), None) => t.clone(),
                    (None, Some(v)) => v,
                    (None, None) => String::new(),
                };
                let line: String = line.chars().take(128).collect();
                match glyphs {
                    None => out.push(Item::Text {
                        at: *at,
                        text: line,
                        color: *color,
                        align: *align,
                    }),
                    Some(g) => {
                        let Some((key, _)) = texture(&g.texture) else {
                            continue;
                        };
                        let chars: Vec<char> = g.charset.chars().collect();
                        let count = chars.len() as u16;
                        let advance = g.size[0] + g.spacing;
                        let width = line.chars().count() as f32 * advance - g.spacing;
                        let mut x = match align {
                            Align::Left => at[0],
                            Align::Center => at[0] - width / 2.0,
                            Align::Right => at[0] - width,
                        };
                        for c in line.chars() {
                            if let Some(i) = chars.iter().position(|g| *g == c) {
                                out.push(Item::Image {
                                    texture: key,
                                    uv: cell(i as u16, count, g.columns),
                                    rect: [x, at[1], g.size[0], g.size[1]],
                                    tint: *color,
                                });
                            }
                            x += advance;
                        }
                    }
                }
            }
        }
    }
    out
}
/// Cell `index` of `count` equal cells laid `columns` to a row.
fn cell(index: u16, count: u16, columns: u16) -> [f32; 4] {
    let columns = if columns == 0 { count } else { columns }.max(1);
    let rows = count.div_ceil(columns).max(1);
    let index = index.min(count.saturating_sub(1));
    let (col, row) = (index % columns, index / columns);
    let (w, h) = (1.0 / f32::from(columns), 1.0 / f32::from(rows));
    [
        f32::from(col) * w,
        f32::from(row) * h,
        f32::from(col + 1) * w,
        f32::from(row + 1) * h,
    ]
}
fn paint(
    paint: &Paint,
    rect: [f32; 4],
    uv: [f32; 4],
    texture: &dyn Fn(&str) -> Option<TextureInfo>,
) -> Option<Item> {
    match &paint.texture {
        Some(id) => texture(id).map(|(key, _)| Item::Image {
            texture: key,
            uv,
            rect,
            tint: paint.color,
        }),
        None => Some(Item::Fill {
            rect,
            color: paint.color,
        }),
    }
}
/// Text without Torque markup (`<color:ff0000>`, `<just:right>`).
pub fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in text.chars() {
        match c {
            '<' => tag = true,
            '>' if tag => tag = false,
            c if !tag && !c.is_control() => out.push(c),
            '\n' if !tag => out.push(' '),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Whether `json` is a region layout rather than a HUD panel.
pub fn is_layout(json: &serde_json::Value) -> bool {
    json.get("region").is_some()
}

/// Check a parsed region name, for messages.
pub fn region(name: &str) -> Result<Region> {
    match Region::parse(name) {
        Some(r) => Ok(r),
        None => bail!(
            "unknown HUD region `{name}`; known: {}",
            Region::ALL.map(Region::name).join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(json: serde_json::Value) -> Layout {
        let l: Layout = serde_json::from_value(json).unwrap();
        l.validate().unwrap();
        l
    }
    fn values(source: &Source) -> Option<Value> {
        match source {
            Source::Health => Some(Value::Number(75.0)),
            Source::HealthMax => Some(Value::Number(100.0)),
            Source::Energy => Some(Value::Number(0.5)),
            Source::BottomPrint => Some(Value::Text(
                "<just:right><color:ffffff>Carbine 12 / 48".into(),
            )),
            Source::State(s) if s == "game:player/coins" => Some(Value::Number(7.0)),
            _ => None,
        }
    }
    fn textures(id: &str) -> Option<TextureInfo> {
        match id {
            "pack:texture/meter" => Some((1, [288, 32])),
            "pack:texture/digits" => Some((2, [100, 10])),
            "pack:texture/bar" => Some((3, [64, 8])),
            _ => None,
        }
    }

    #[test]
    fn every_region_has_a_name_and_a_box() {
        for r in Region::ALL {
            assert_eq!(Region::parse(r.name()), Some(r));
            let [x, y, w, h] = r.rect(640.0);
            assert!(
                x >= 0.0 && y >= 0.0 && x + w <= 640.0 && y + h <= HEIGHT,
                "{r:?}"
            );
        }
        assert!(region("health_bar").is_err());
    }

    #[test]
    fn a_frame_strip_picks_its_frame_by_the_values_share() {
        let l = layout(serde_json::json!({
            "schema_version": 1, "region": "top_right",
            "elements": [{ "type": "image", "texture": "pack:texture/meter",
                "rect": [0, 0, 64, 64], "frames": { "count": 9, "value": "health" } }]
        }));
        let items = resolve(&l, &values, &textures);
        // 75 of 100 is frame 6 of 0..8.
        let Item::Image { uv, .. } = &items[0] else {
            panic!()
        };
        assert!(
            (uv[0] - 6.0 / 9.0).abs() < 1e-6 && (uv[2] - 7.0 / 9.0).abs() < 1e-6,
            "{uv:?}"
        );
    }

    #[test]
    fn a_bar_reveals_its_texture_by_share() {
        let l = layout(serde_json::json!({
            "schema_version": 1, "region": "top_left",
            "elements": [{ "type": "bar", "rect": [10, 10, 100, 8], "value": "energy",
                "fill": { "texture": "pack:texture/bar" }, "back": { "color": [0, 0, 0, 0.5] },
                "direction": "left" }]
        }));
        let items = resolve(&l, &values, &textures);
        assert_eq!(
            items[0],
            Item::Fill {
                rect: [10.0, 10.0, 100.0, 8.0],
                color: [0.0, 0.0, 0.0, 0.5]
            }
        );
        assert_eq!(
            items[1],
            Item::Image {
                texture: 3,
                uv: [0.5, 0.0, 1.0, 1.0],
                rect: [60.0, 10.0, 50.0, 8.0],
                tint: [1.0; 4]
            }
        );
    }

    #[test]
    fn a_sprite_counter_draws_one_cell_per_character() {
        let l = layout(serde_json::json!({
            "schema_version": 1, "region": "top_left",
            "elements": [{ "type": "text", "at": [100, 0], "value": "game:player/coins",
                "text": "x{}", "digits": 2, "align": "right",
                "glyphs": { "texture": "pack:texture/digits", "charset": "0123456789x",
                    "size": [16, 16], "spacing": 2 } }]
        }));
        let items = resolve(&l, &values, &textures);
        assert_eq!(items.len(), 3, "x, 0 and 7: {items:?}");
        let Item::Image { rect, uv, .. } = &items[2] else {
            panic!()
        };
        // Right-aligned at 100: three glyphs of 16 with 2 between end at 100.
        assert_eq!(rect[0] + rect[2], 100.0);
        assert!((uv[0] - 7.0 / 11.0).abs() < 1e-6);
    }

    #[test]
    fn text_reads_prints_without_markup_and_hides_by_its_show() {
        let l = layout(serde_json::json!({
            "schema_version": 1, "region": "bottom_print",
            "elements": [
                { "type": "text", "at": [0, 0], "value": "bottom_print" },
                { "type": "text", "at": [0, 20], "text": "LOW", "show": { "value": "health", "below": 25 } }
            ]
        }));
        let items = resolve(&l, &values, &textures);
        assert_eq!(
            items,
            vec![Item::Text {
                at: [0.0, 0.0],
                text: "Carbine 12 / 48".into(),
                color: [1.0; 4],
                align: Align::Left
            }]
        );
    }

    #[test]
    fn a_missing_texture_leaves_its_element_out() {
        let l = layout(serde_json::json!({
            "schema_version": 1, "region": "top_left",
            "elements": [{ "type": "image", "texture": "pack:texture/gone", "rect": [0, 0, 8, 8] }]
        }));
        assert!(resolve(&l, &values, &textures).is_empty());
    }

    #[test]
    fn the_most_specific_claim_wins_then_priority_then_package() {
        let plain = layout(serde_json::json!({ "schema_version": 1, "region": "crosshair" }));
        let body = layout(
            serde_json::json!({ "schema_version": 1, "region": "crosshair",
            "when": { "body": ["a:archetype/b"] } }),
        );
        let held = layout(
            serde_json::json!({ "schema_version": 1, "region": "crosshair",
            "when": { "holding": ["guns"] }, "priority": -5 }),
        );
        let loud = layout(
            serde_json::json!({ "schema_version": 1, "region": "crosshair",
            "priority": 9 }),
        );
        let mut claims = [
            Claim {
                package: "zed",
                layout: &plain,
            },
            Claim {
                package: "amy",
                layout: &plain,
            },
            Claim {
                package: "bob",
                layout: &body,
            },
            Claim {
                package: "cat",
                layout: &held,
            },
            Claim {
                package: "dan",
                layout: &loud,
            },
        ];
        claims.sort_by_key(|c| c.rank());
        let order: Vec<_> = claims.iter().map(|c| c.package).collect();
        assert_eq!(order, ["cat", "bob", "dan", "amy", "zed"]);
    }

    #[test]
    fn when_matches_body_item_and_seat_together() {
        let when = When {
            body: vec!["a:archetype/runner".into()],
            holding: vec!["guns".into()],
            vehicle: vec![],
        };
        let viewer = |body, holding| Viewer {
            body,
            holding,
            vehicle: "",
        };
        assert!(when.matches(&viewer("a:archetype/runner", "guns:image/rifle")));
        assert!(!when.matches(&viewer("a:archetype/runner", "")));
        assert!(!when.matches(&viewer(
            "v20.player.playerstandardarmor",
            "guns:image/rifle"
        )));
        assert!(When::default().matches(&viewer("", "")));
    }

    #[test]
    fn bad_layouts_are_refused() {
        for bad in [
            serde_json::json!({ "schema_version": 1, "region": "top_left", "priority": 500 }),
            serde_json::json!({ "schema_version": 1, "region": "top_left",
                "elements": [{ "type": "bar", "rect": [0, 0, 1, 1], "value": "mana", "fill": {} }] }),
            serde_json::json!({ "schema_version": 1, "region": "top_left",
                "elements": [{ "type": "image", "texture": "x.png", "rect": [0, 0, 1, 1] }] }),
            serde_json::json!({ "schema_version": 1, "region": "top_left",
                "elements": [{ "type": "text", "at": [0, 0] }] }),
        ] {
            let l: Layout = serde_json::from_value(bad.clone()).unwrap();
            assert!(l.validate().is_err(), "{bad}");
        }
        assert!(
            serde_json::from_value::<Layout>(
                serde_json::json!({ "schema_version": 1, "region": "chat" })
            )
            .is_err(),
            "chat is the player's"
        );
    }
}
