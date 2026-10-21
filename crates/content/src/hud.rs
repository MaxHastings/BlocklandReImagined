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
use std::collections::BTreeMap;

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
    /// The stock chat lines: claiming it hides them (typing still works),
    /// as a match HUD with its own kill feed does.
    Chat,
    /// The stock tool bar down the right edge (the five-slot inventory):
    /// claiming it hides the bar, for a game that shows what is carried
    /// its own way.
    Tools,
    /// The names drawn over other players' heads: claiming it hides the
    /// stock ones, and a layout's `name_tags` draws them its own way.
    NameTags,
    /// The building interface: the brick bar, paint box, tool bar and
    /// their prompts. Claiming it hides all of it and turns off the stock
    /// building keys and wheel, so a game that has no building shows none
    /// of Blockland's build HUD and nothing of its binds.
    Building,
}
impl Region {
    pub const ALL: [Region; 17] = [
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
        Region::Chat,
        Region::Tools,
        Region::NameTags,
        Region::Building,
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
            Region::Chat => "chat",
            Region::Tools => "tools",
            Region::NameTags => "name_tags",
            Region::Building => "building",
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
            Region::Chat => Some("chat"),
            Region::Tools => Some("tool bar"),
            Region::NameTags => Some("player names"),
            Region::Building => Some("building interface and keys"),
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
            Region::Chat => [0.0, 0.0, 320.0, 120.0],
            Region::Tools => [w - 64.0, 0.0, 64.0, HEIGHT],
            Region::NameTags | Region::Building => [0.0, 0.0, w, HEIGHT],
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
    /// The game's own prompts restyled or hidden while this layout is
    /// drawn, by key ([`STOCK_TEXTS`]): `""` hides one. Markup and colour
    /// codes work as in a center print; `{}` is the respawn countdown's
    /// seconds.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub texts: BTreeMap<String, String>,
    /// How the names over other players' heads look, for a `name_tags`
    /// claim. Left out, the claim hides them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_tags: Option<NameTagStyle>,
}

/// Names over other players' heads, as a game shows them: whose, when,
/// and in what colour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NameTagStyle {
    /// Players on the viewer's team in the viewer's mini-game.
    pub teammates: TagShow,
    /// Everyone else.
    pub others: TagShow,
    /// One colour for every name (RGBA 0..1). Left out, the stock colour:
    /// the team's, then the mini-game's, else white.
    pub color: Option<[f32; 4]>,
    /// The stock one-pixel dark outline around each name.
    pub outline: bool,
    /// How far above the eye the name sits, in units (the stock 0.85).
    pub height: f32,
}
impl Default for NameTagStyle {
    fn default() -> Self {
        Self {
            teammates: TagShow::Always,
            others: TagShow::Always,
            color: None,
            outline: true,
            height: 0.85,
        }
    }
}

impl NameTagStyle {
    /// Whether a name shows: `teammate` on the viewer's team, `aimed` with
    /// the viewer's aim on them.
    pub fn shows(&self, teammate: bool, aimed: bool) -> bool {
        match if teammate {
            self.teammates
        } else {
            self.others
        } {
            TagShow::Always => true,
            TagShow::Aimed => aimed,
            TagShow::Never => false,
        }
    }
}

/// When a player's name shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagShow {
    /// Whenever they are in sight, as v20 shows every name.
    #[default]
    Always,
    /// Only while the viewer's aim is on them.
    Aimed,
    Never,
}

/// The game's prompts a layout's `texts` can replace.
pub const STOCK_TEXTS: [&str; 4] = [
    "click_to_respawn",
    "respawn_countdown",
    "respawn_countdown_one",
    "building_disabled",
];

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
    /// The viewer sits in one of these seats of their vehicle (from 0, the
    /// driver's first): a driver's reticle, a gunner's hull meter.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seat: Vec<u8>,
    /// `true` only while the viewer plays in a mini-game, `false` only
    /// while they do not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minigame: Option<bool>,
}
impl When {
    pub fn is_always(&self) -> bool {
        self.body.is_empty()
            && self.holding.is_empty()
            && self.vehicle.is_empty()
            && self.seat.is_empty()
            && self.minigame.is_none()
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
    /// The seat sat in, from 0, while `vehicle` is not empty.
    pub seat: Option<u8>,
    /// Playing in a mini-game.
    pub minigame: bool,
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
        let seat = self.seat.is_empty() || viewer.seat.is_some_and(|s| self.seat.contains(&s));
        let minigame = self.minigame.is_none_or(|m| m == viewer.minigame);
        body && holding && vehicle && seat && minigame
    }
    /// How specific the claim is: a vehicle seat (8), then a held item (4),
    /// then a body (2), then being in a mini-game or not (1), added
    /// together.
    pub fn specificity(&self) -> u8 {
        u8::from(!self.vehicle.is_empty() || !self.seat.is_empty()) * 8
            + u8::from(!self.holding.is_empty()) * 4
            + u8::from(!self.body.is_empty()) * 2
            + u8::from(self.minigame.is_some())
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

/// A screen edge ([`Element::EdgeArrow`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
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
    /// A hit indicator: draws only while the last hit came from the `edge`
    /// this arrow marks. `x` and `z` (rule values) say where it came from as
    /// a horizontal world offset from the viewer (any length), and `strength`
    /// (0 to 1, counted down by the rule) fades it (`tint` alpha times
    /// `strength`; 0 draws nothing). The edge is the nearest of ahead (top),
    /// behind (bottom), left and right by the viewer's facing, so it follows
    /// the camera. Claim `top_center`, `bottom_center`, `left` and `right`
    /// with one arrow each.
    EdgeArrow {
        texture: String,
        edge: Edge,
        /// x, y, width, height in units from the region's corner.
        rect: [f32; 4],
        x: String,
        z: String,
        strength: String,
        #[serde(default = "white")]
        tint: [f32; 4],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show: Option<Show>,
    },
    /// A picture strip scrolled by a heading, as AA's compass: `textures`
    /// (one to eight, equal width) are laid end to end to make one whole turn,
    /// and `rect` shows the `span` degrees (a number or a source such as
    /// `fov`) centred on `heading`'s source, wrapping round.
    Strip {
        textures: Vec<String>,
        /// x, y, width, height in units from the region's corner.
        rect: [f32; 4],
        heading: String,
        span: Amount,
        #[serde(default = "white")]
        tint: [f32; 4],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show: Option<Show>,
    },
    /// A picture drawn on a heading strip ([`Element::Strip`]) at the bearing
    /// of a point in the world, as AA's objective letters on its compass.
    /// `rect`, `heading` and `span` are the strip's own; `x` and `z` are
    /// sources (a rule's state) giving the point's world position, and the
    /// picture is `size` units wide and high, centred on the strip's height.
    /// Nothing is drawn while the point lies outside the `span` degrees shown
    /// or while either source has no value.
    StripMarker {
        texture: String,
        /// The strip's x, y, width, height in units from the region's corner.
        rect: [f32; 4],
        heading: String,
        span: Amount,
        x: String,
        z: String,
        /// The picture's width and height in units.
        size: [f32; 2],
        #[serde(default = "white")]
        tint: [f32; 4],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show: Option<Show>,
    },
    /// The latest kills, one line each, oldest at the top: the killer, the
    /// weapon (an icon or its name) and the victim, in their teams'
    /// colours. While a layout with one is drawn, kills stay out of chat.
    KillFeed {
        /// Where the first line starts (or centres, or ends, by `align`).
        at: [f32; 2],
        #[serde(default)]
        align: Align,
        /// Most lines shown, 1 to 16.
        #[serde(default = "feed_lines")]
        lines: u8,
        /// How long a line stays, 0.5 to 60 seconds; it fades out over the
        /// last half second.
        #[serde(default = "feed_seconds")]
        seconds: f32,
        /// Units from one line to the next; negative stacks upward.
        #[serde(default = "feed_spacing")]
        spacing: f32,
        /// A kill's line: `{killer}`, `{weapon}` and `{victim}`.
        #[serde(default = "feed_kill")]
        format: String,
        /// A death with no killer (a fall, a suicide): `{weapon}` and
        /// `{victim}`.
        #[serde(default = "feed_death")]
        death: String,
        #[serde(default = "white")]
        color: [f32; 4],
        /// Names in their teams' colours (else `color`).
        #[serde(default = "yes")]
        team_colors: bool,
        /// A damage type's icon by its name (`Gun`, `Rocket Direct`, any
        /// case), drawn for `{weapon}`.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        icons: BTreeMap<String, String>,
        /// An icon's drawn size, units.
        #[serde(default = "feed_icon")]
        icon_size: [f32; 2],
        /// A damage type's name as the line writes it, by its name; one
        /// left out is written as it is named.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        names: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show: Option<Show>,
    },
    /// Lines a rule posts with `hud_line` to a feed of this name, one each,
    /// oldest at the top: radio calls, countdown words, match notices.
    /// Each line stays for the seconds its rule gave it and fades out over
    /// the last half second.
    MessageFeed {
        /// The feed a rule's `hud_line` names.
        feed: String,
        /// Where the first line starts (or centres, or ends, by `align`).
        at: [f32; 2],
        #[serde(default)]
        align: Align,
        /// Most lines shown, 1 to 16.
        #[serde(default = "feed_lines")]
        lines: u8,
        /// Units from one line to the next; negative stacks upward.
        #[serde(default = "feed_spacing")]
        spacing: f32,
        /// A line's colour when its rule gave none.
        #[serde(default = "white")]
        color: [f32; 4],
        /// A picture a line may name (`icon`), drawn before its text.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        icons: BTreeMap<String, String>,
        /// An icon's drawn size, units.
        #[serde(default = "feed_icon")]
        icon_size: [f32; 2],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show: Option<Show>,
    },
    /// A motion tracker: a blip for each player within `range` units who
    /// moves faster than `min_speed` or fired within `fired_within`
    /// seconds, placed by range and bearing from the middle of `rect`
    /// (straight ahead is up), tinted by side.
    Tracker {
        rect: [f32; 4],
        /// Units from the middle of `rect` to its edge.
        range: f32,
        blip: String,
        /// Drawn for a contact more than `height` units above or below the
        /// viewer, in place of `blip`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blip_above: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blip_below: Option<String>,
        /// Each side's own blip, in place of `blip` (`blip_above` and
        /// `blip_below` still win off the viewer's level).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blip_ally: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blip_enemy: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blip_neutral: Option<String>,
        #[serde(default = "tracker_height")]
        height: f32,
        #[serde(default = "tracker_blip")]
        blip_size: [f32; 2],
        #[serde(default = "tracker_ally")]
        ally: [f32; 4],
        #[serde(default = "tracker_enemy")]
        enemy: [f32; 4],
        /// A player in no team of the viewer's game, or in no game.
        #[serde(default = "tracker_neutral")]
        neutral: [f32; 4],
        /// Slower movers do not show (a player standing still).
        #[serde(default = "tracker_speed")]
        min_speed: f32,
        /// Show players moving crouched too; off, crouch-walkers stay off
        /// the tracker.
        #[serde(default)]
        crouched: bool,
        /// Seconds after a player fires during which they show however
        /// they move; 0 never.
        #[serde(default = "tracker_fired")]
        fired_within: f32,
        /// Which sides show.
        #[serde(default = "yes")]
        allies: bool,
        #[serde(default = "yes")]
        enemies: bool,
        #[serde(default = "yes")]
        others: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show: Option<Show>,
    },
}
fn white() -> [f32; 4] {
    [1.0; 4]
}
fn yes() -> bool {
    true
}
fn feed_lines() -> u8 {
    4
}
fn feed_seconds() -> f32 {
    6.0
}
fn feed_spacing() -> f32 {
    16.0
}
fn feed_kill() -> String {
    "{killer} [{weapon}] {victim}".into()
}
fn feed_death() -> String {
    "[{weapon}] {victim}".into()
}
fn feed_icon() -> [f32; 2] {
    [24.0, 12.0]
}
fn tracker_height() -> f32 {
    3.0
}
fn tracker_blip() -> [f32; 2] {
    [8.0, 8.0]
}
fn tracker_ally() -> [f32; 4] {
    [1.0, 0.85, 0.2, 1.0]
}
fn tracker_enemy() -> [f32; 4] {
    [1.0, 0.2, 0.15, 1.0]
}
fn tracker_neutral() -> [f32; 4] {
    [0.85, 0.85, 0.85, 1.0]
}
fn tracker_speed() -> f32 {
    1.0
}
fn tracker_fired() -> f32 {
    2.0
}

/// A kill as the feed shows it ([`Element::KillFeed`]).
#[derive(Debug, Clone, PartialEq)]
pub struct FeedKill {
    /// Seconds since it happened.
    pub age: f32,
    /// The killer's name and team colour; `None` for a death with no
    /// killer.
    pub killer: Option<(String, Option<[f32; 4]>)>,
    pub victim: (String, Option<[f32; 4]>),
    /// The damage type's name (`Gun`).
    pub weapon: String,
}

/// A line a rule posted to a message feed ([`Element::MessageFeed`]).
#[derive(Debug, Clone, PartialEq)]
pub struct FeedLine {
    /// The feed it was posted to.
    pub feed: String,
    /// Seconds since it was posted, and how long it stays.
    pub age: f32,
    pub seconds: f32,
    pub text: String,
    pub color: Option<[f32; 4]>,
    /// The name of one of the feed's `icons`.
    pub icon: Option<String>,
}

/// Which side another player is on, from the viewer's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Relation {
    /// Not in the viewer's game, or no teams to tell.
    #[default]
    Neutral,
    /// The viewer's team.
    Ally,
    /// Another team of the viewer's game, or anyone in a game without
    /// teams.
    Enemy,
}

/// Another player as a tracker sees them, from the viewer's eye and
/// facing ([`Element::Tracker`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Contact {
    pub id: u64,
    /// Units to the viewer's right, ahead and above.
    pub right: f32,
    pub ahead: f32,
    pub above: f32,
    /// Units a second.
    pub speed: f32,
    pub crouched: bool,
    /// Seconds since they last fired (0 while they hold the trigger).
    pub fired: Option<f32>,
    pub relation: Relation,
}
impl Contact {
    /// Horizontal distance from the viewer.
    pub fn range(&self) -> f32 {
        self.right.hypot(self.ahead)
    }
    /// Whether a tracker that shows movers above `min_speed` (crouched
    /// ones only with `crouched`) and shooters for `fired_within` seconds
    /// shows this contact.
    pub fn noticed(&self, min_speed: f32, crouched: bool, fired_within: f32) -> bool {
        let fired = self.fired.is_some_and(|s| s < fired_within);
        let moving = self.speed > min_speed && (crouched || !self.crouched);
        fired || moving
    }
}

/// What a frame's kill feeds and trackers read besides values.
#[derive(Debug, Clone, Copy, Default)]
pub struct Live<'a> {
    pub kills: &'a [FeedKill],
    pub lines: &'a [FeedLine],
    pub contacts: &'a [Contact],
    /// The viewer's level facing as a unit (x, z) in the world; (0, 0)
    /// when unknown (edge arrows then read "ahead" as -z).
    pub facing: [f32; 2],
    /// The viewer's position (x, z) in the world, for [`Element::StripMarker`].
    pub position: [f32; 2],
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
    /// What is left of the hull of the vehicle the viewer rides, 0 to 1
    /// (missing on foot).
    VehicleHealth,
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
    /// Seconds until the dead viewer may respawn, 0 once they may click;
    /// missing while alive or while a rule holds their respawn.
    Respawn,
    /// The way the viewer's body faces, degrees 0 to 360 turning clockwise
    /// from straight ahead along the world's -z (`Element::Strip`'s compass).
    Heading,
    /// The camera's field of view now, degrees.
    Fov,
    /// 1 while the viewer aims down a gun's sights, else 0.
    Aiming,
    /// The scope's zoom step while the viewer aims: 1 for the gun's own field of view, 2 and up for each
    /// wheel level (`Zoom::levels`), 0 when not aiming.
    ZoomLevel,
    /// Owner-private ammunition for an inventory item, including stowed items.
    ItemAmmo {
        item: String,
        field: AmmoField,
    },
    /// A rule's public state: `package:player/key` or `package:global/key`,
    /// read as a HUD panel row reads it.
    State(String),
}
/// Which part of an inventory item's ammunition a HUD reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AmmoField {
    Rounds,
    Maximum,
    Reserve,
    Reloading,
}
impl Source {
    pub fn parse(text: &str) -> Option<Self> {
        for (prefix, field) in [
            ("item_ammo:", AmmoField::Rounds),
            ("item_ammo_max:", AmmoField::Maximum),
            ("item_ammo_reserve:", AmmoField::Reserve),
            ("item_reloading:", AmmoField::Reloading),
        ] {
            if let Some(item) = text.strip_prefix(prefix) {
                return (!item.is_empty()
                    && item.len() <= 160
                    && item.chars().all(|c| {
                        c.is_ascii_alphanumeric() || matches!(c, ':' | '/' | '.' | '_' | '-')
                    }))
                .then(|| Self::ItemAmmo {
                    item: item.to_string(),
                    field,
                });
            }
        }
        Some(match text {
            "health" => Source::Health,
            "health_max" => Source::HealthMax,
            "vehicle_health" => Source::VehicleHealth,
            "energy" => Source::Energy,
            "alive" => Source::Alive,
            "center_print" => Source::CenterPrint,
            "bottom_print" => Source::BottomPrint,
            "ammo" => Source::Ammo,
            "ammo_max" => Source::AmmoMax,
            "ammo_reserve" => Source::AmmoReserve,
            "reloading" => Source::Reloading,
            "respawn" => Source::Respawn,
            "heading" => Source::Heading,
            "fov" => Source::Fov,
            "aiming" => Source::Aiming,
            "zoom_level" => Source::ZoomLevel,
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
            Source::ItemAmmo {
                item,
                field: AmmoField::Rounds,
            } => Amount::Source(format!("item_ammo_max:{item}")),
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
        "`{s}` is not a HUD value: health, health_max, vehicle_health, energy, alive, center_print, bottom_print, ammo, ammo_max, ammo_reserve, reloading, respawn, or a rule's package:player/key or package:global/key"
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
        if let Some(style) = &self.name_tags {
            ensure!(
                self.region == Region::NameTags,
                "`name_tags` belongs in a `name_tags` region layout"
            );
            ensure!(
                style.height.is_finite() && (-8.0..=8.0).contains(&style.height),
                "name_tags height must be -8 to 8 units"
            );
            ensure!(
                style
                    .color
                    .is_none_or(|c| c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))),
                "name_tags color must be RGBA from 0 to 1"
            );
        }
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
                Element::KillFeed {
                    at,
                    lines,
                    seconds,
                    spacing,
                    format,
                    death,
                    color,
                    icons,
                    icon_size,
                    names,
                    show,
                    ..
                } => {
                    ensure!(
                        at.iter()
                            .chain([spacing])
                            .all(|v| v.is_finite() && v.abs() <= 4096.0),
                        "kill feed position and spacing must be finite"
                    );
                    ensure!((1..=16).contains(lines), "a kill feed shows 1 to 16 lines");
                    ensure!(
                        (0.5..=60.0).contains(seconds),
                        "a kill feed line stays 0.5 to 60 seconds"
                    );
                    for line in [format, death] {
                        ensure!(
                            line.chars().count() <= 64 && !line.chars().any(char::is_control),
                            "a kill feed format is at most 64 printable characters"
                        );
                    }
                    ensure!(color_ok(color), "kill feed color must be 0..1 RGBA");
                    ensure!(
                        icons.len() <= 64 && names.len() <= 64,
                        "a kill feed names at most 64 icons and 64 names"
                    );
                    for icon in icons.values() {
                        texture_ok(icon)?;
                    }
                    for name in names.values() {
                        ensure!(
                            name.chars().count() <= 32 && !name.chars().any(char::is_control),
                            "a kill feed weapon name is at most 32 printable characters"
                        );
                    }
                    ensure!(
                        icon_size
                            .iter()
                            .all(|v| v.is_finite() && *v > 0.0 && *v <= 480.0),
                        "a kill feed icon size must be positive"
                    );
                    show_ok(show)?;
                }
                Element::MessageFeed {
                    feed,
                    at,
                    lines,
                    spacing,
                    color,
                    icons,
                    icon_size,
                    show,
                    ..
                } => {
                    ensure!(
                        !feed.is_empty()
                            && feed.len() <= 32
                            && feed
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')),
                        "a message feed is named by 1 to 32 letters, digits, `_` or `-`"
                    );
                    ensure!(
                        at.iter()
                            .chain([spacing])
                            .all(|v| v.is_finite() && v.abs() <= 4096.0),
                        "message feed position and spacing must be finite"
                    );
                    ensure!(
                        (1..=16).contains(lines),
                        "a message feed shows 1 to 16 lines"
                    );
                    ensure!(color_ok(color), "message feed color must be 0..1 RGBA");
                    ensure!(icons.len() <= 32, "a message feed names at most 32 icons");
                    for (name, icon) in icons {
                        ensure!(
                            !name.is_empty()
                                && name.len() <= 32
                                && !name.chars().any(char::is_control),
                            "a message feed icon is named by 1 to 32 printable characters"
                        );
                        texture_ok(icon)?;
                    }
                    ensure!(
                        icon_size
                            .iter()
                            .all(|v| v.is_finite() && *v > 0.0 && *v <= 480.0),
                        "a message feed icon size must be positive"
                    );
                    show_ok(show)?;
                }
                Element::Strip {
                    textures,
                    rect,
                    heading,
                    span,
                    tint,
                    show,
                } => {
                    ensure!(
                        (1..=8).contains(&textures.len()),
                        "a strip lays one to eight textures end to end"
                    );
                    for t in textures {
                        texture_ok(t)?;
                    }
                    ensure!(
                        rect_ok(rect),
                        "strip rect must be finite with no negative size"
                    );
                    source_ok(heading)?;
                    amount_ok(span)?;
                    ensure!(color_ok(tint), "tint must be 0..1 RGBA");
                    show_ok(show)?;
                }
                Element::StripMarker {
                    texture,
                    rect,
                    heading,
                    span,
                    x,
                    z,
                    size,
                    tint,
                    show,
                } => {
                    texture_ok(texture)?;
                    ensure!(
                        rect_ok(rect),
                        "strip marker rect must be finite with no negative size"
                    );
                    for s in [heading, x, z] {
                        source_ok(s)?;
                    }
                    amount_ok(span)?;
                    ensure!(
                        size.iter()
                            .all(|v| v.is_finite() && *v >= 0.0 && *v <= 4096.0),
                        "strip marker size must be finite with no negative size"
                    );
                    ensure!(color_ok(tint), "tint must be 0..1 RGBA");
                    show_ok(show)?;
                }
                Element::EdgeArrow {
                    texture,
                    rect,
                    x,
                    z,
                    strength,
                    tint,
                    show,
                    ..
                } => {
                    texture_ok(texture)?;
                    ensure!(
                        rect_ok(rect),
                        "edge arrow rect must be finite with no negative size"
                    );
                    for s in [x, z, strength] {
                        source_ok(s)?;
                    }
                    ensure!(color_ok(tint), "tint must be 0..1 RGBA");
                    show_ok(show)?;
                }
                Element::Tracker {
                    rect,
                    range,
                    blip,
                    blip_above,
                    blip_below,
                    blip_ally,
                    blip_enemy,
                    blip_neutral,
                    height,
                    blip_size,
                    ally,
                    enemy,
                    neutral,
                    min_speed,
                    fired_within,
                    show,
                    ..
                } => {
                    ensure!(
                        rect_ok(rect),
                        "tracker rect must be finite with no negative size"
                    );
                    ensure!(
                        range.is_finite() && (1.0..=1000.0).contains(range),
                        "a tracker's range is 1 to 1000 units"
                    );
                    for t in std::iter::once(blip)
                        .chain(blip_above)
                        .chain(blip_below)
                        .chain(blip_ally)
                        .chain(blip_enemy)
                        .chain(blip_neutral)
                    {
                        texture_ok(t)?;
                    }
                    ensure!(
                        [height, min_speed, fired_within]
                            .iter()
                            .all(|v| v.is_finite() && (0.0..=1000.0).contains(*v)),
                        "a tracker's height, min_speed and fired_within are 0 to 1000"
                    );
                    ensure!(
                        blip_size
                            .iter()
                            .all(|v| v.is_finite() && *v > 0.0 && *v <= 480.0),
                        "a tracker's blip size must be positive"
                    );
                    ensure!(
                        [ally, enemy, neutral].into_iter().all(color_ok),
                        "tracker colors must be 0..1 RGBA"
                    );
                    show_ok(show)?;
                }
            }
        }
        ensure!(self.texts.len() <= STOCK_TEXTS.len(), "too many texts");
        for (key, text) in &self.texts {
            ensure!(
                STOCK_TEXTS.contains(&key.as_str()),
                "`{key}` is not a stock text: {}",
                STOCK_TEXTS.join(", ")
            );
            ensure!(
                text.chars().count() <= 256 && !text.chars().any(|c| c.is_control() && c != '\n'),
                "text `{key}` is at most 256 characters"
            );
        }
        Ok(())
    }
    /// Whether the layout draws a kill feed (kills then stay out of chat).
    pub fn has_kill_feed(&self) -> bool {
        self.elements
            .iter()
            .any(|e| matches!(e, Element::KillFeed { .. }))
    }
    pub fn has_tracker(&self) -> bool {
        self.elements
            .iter()
            .any(|e| matches!(e, Element::Tracker { .. }))
    }
    /// Every texture id the layout draws.
    pub fn textures(&self) -> Vec<&str> {
        let mut out = Vec::new();
        for e in &self.elements {
            match e {
                Element::Image { texture, .. }
                | Element::EdgeArrow { texture, .. }
                | Element::StripMarker { texture, .. } => {
                    out.push(texture.as_str());
                }
                Element::Strip { textures, .. } => out.extend(textures.iter().map(String::as_str)),
                Element::Bar { fill, back, .. } => out.extend(
                    std::iter::once(fill)
                        .chain(back)
                        .filter_map(|p| p.texture.as_deref()),
                ),
                Element::Text { glyphs, .. } => {
                    out.extend(glyphs.as_ref().map(|g| g.texture.as_str()));
                }
                Element::KillFeed { icons, .. } | Element::MessageFeed { icons, .. } => {
                    out.extend(icons.values().map(String::as_str));
                }
                Element::Tracker {
                    blip,
                    blip_above,
                    blip_below,
                    blip_ally,
                    blip_enemy,
                    blip_neutral,
                    ..
                } => out.extend(
                    std::iter::once(blip)
                        .chain(blip_above)
                        .chain(blip_below)
                        .chain(blip_ally)
                        .chain(blip_enemy)
                        .chain(blip_neutral)
                        .map(String::as_str),
                ),
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
                Element::EdgeArrow {
                    x,
                    z,
                    strength,
                    show,
                    ..
                } => {
                    names.extend([x.as_str(), z.as_str(), strength.as_str()]);
                    names.extend(show.as_ref().map(|s| s.value.as_str()));
                }
                Element::Strip {
                    heading,
                    span,
                    show,
                    ..
                } => {
                    names.push(heading);
                    if let Amount::Source(s) = span {
                        names.push(s.as_str());
                    }
                    names.extend(show.as_ref().map(|s| s.value.as_str()));
                }
                Element::StripMarker {
                    heading,
                    span,
                    x,
                    z,
                    show,
                    ..
                } => {
                    names.extend([heading.as_str(), x.as_str(), z.as_str()]);
                    if let Amount::Source(s) = span {
                        names.push(s.as_str());
                    }
                    names.extend(show.as_ref().map(|s| s.value.as_str()));
                }
                Element::KillFeed { show, .. }
                | Element::MessageFeed { show, .. }
                | Element::Tracker { show, .. } => {
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
    /// A line of text in several colours and icons, placed as a whole by
    /// `align` (a kill feed's line).
    Line {
        at: [f32; 2],
        align: Align,
        spans: Vec<Span>,
    },
}

/// Part of a [`Item::Line`].
#[derive(Debug, Clone, PartialEq)]
pub enum Span {
    Text {
        text: String,
        color: [f32; 4],
    },
    /// A texture drawn `size` units, its bottom on the text's.
    Image {
        texture: u64,
        size: [f32; 2],
        tint: [f32; 4],
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
    resolve_live(layout, value, texture, &Live::default())
}

/// [`resolve`] with this frame's kills and tracker contacts.
pub fn resolve_live(
    layout: &Layout,
    value: &dyn Fn(&Source) -> Option<Value>,
    texture: &dyn Fn(&str) -> Option<TextureInfo>,
    live: &Live<'_>,
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
            Element::KillFeed {
                at,
                align,
                lines,
                seconds,
                spacing,
                format,
                death,
                color,
                team_colors,
                icons,
                icon_size,
                names,
                show,
            } => {
                if !shown(show) {
                    continue;
                }
                let recent: Vec<&FeedKill> = live
                    .kills
                    .iter()
                    .filter(|k| k.age.is_finite() && k.age >= 0.0 && k.age < *seconds)
                    .collect();
                let first = recent.len().saturating_sub(usize::from(*lines));
                for (row, kill) in recent[first..].iter().enumerate() {
                    // Fades over its last half second.
                    let fade = ((seconds - kill.age) / 0.5).clamped(0.0, 1.0);
                    let faded = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * fade];
                    let name = |(name, team): &(String, Option<[f32; 4]>)| Span::Text {
                        text: name.clone(),
                        color: faded(team.filter(|_| *team_colors).unwrap_or(*color)),
                    };
                    let weapon = icons
                        .iter()
                        .find(|(k, _)| same(k, &kill.weapon))
                        .and_then(|(_, t)| texture(t))
                        .map_or_else(
                            || Span::Text {
                                text: names
                                    .iter()
                                    .find(|(k, _)| same(k, &kill.weapon))
                                    .map_or(kill.weapon.clone(), |(_, n)| n.clone()),
                                color: faded(*color),
                            },
                            |(key, _)| Span::Image {
                                texture: key,
                                size: *icon_size,
                                tint: faded([1.0; 4]),
                            },
                        );
                    let template = if kill.killer.is_some() { format } else { death };
                    let mut spans = Vec::new();
                    let mut rest = template.as_str();
                    while !rest.is_empty() {
                        let next = ["{killer}", "{weapon}", "{victim}"]
                            .into_iter()
                            .filter_map(|tag| rest.find(tag).map(|at| (at, tag)))
                            .min();
                        let Some((start, tag)) = next else {
                            spans.push(Span::Text {
                                text: rest.to_string(),
                                color: faded(*color),
                            });
                            break;
                        };
                        if start > 0 {
                            spans.push(Span::Text {
                                text: rest[..start].to_string(),
                                color: faded(*color),
                            });
                        }
                        match tag {
                            "{killer}" => spans.extend(kill.killer.as_ref().map(name)),
                            "{weapon}" => spans.push(weapon.clone()),
                            _ => spans.push(name(&kill.victim)),
                        }
                        rest = &rest[start + tag.len()..];
                    }
                    out.push(Item::Line {
                        at: [at[0], at[1] + row as f32 * spacing],
                        align: *align,
                        spans,
                    });
                }
            }
            Element::MessageFeed {
                feed,
                at,
                align,
                lines,
                spacing,
                color,
                icons,
                icon_size,
                show,
            } => {
                if !shown(show) {
                    continue;
                }
                let recent: Vec<&FeedLine> = live
                    .lines
                    .iter()
                    .filter(|l| {
                        l.feed == *feed && l.age.is_finite() && l.age >= 0.0 && l.age < l.seconds
                    })
                    .collect();
                let first = recent.len().saturating_sub(usize::from(*lines));
                for (row, line) in recent[first..].iter().enumerate() {
                    // Fades over its last half second.
                    let fade = ((line.seconds - line.age) / 0.5).clamped(0.0, 1.0);
                    let faded = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * fade];
                    let mut spans = Vec::new();
                    if let Some((key, _)) = line
                        .icon
                        .as_ref()
                        .and_then(|name| icons.get(name))
                        .and_then(|t| texture(t))
                    {
                        spans.push(Span::Image {
                            texture: key,
                            size: *icon_size,
                            tint: faded([1.0; 4]),
                        });
                    }
                    spans.push(Span::Text {
                        text: line.text.clone(),
                        color: faded(line.color.unwrap_or(*color)),
                    });
                    out.push(Item::Line {
                        at: [at[0], at[1] + row as f32 * spacing],
                        align: *align,
                        spans,
                    });
                }
            }
            Element::Strip {
                textures,
                rect,
                heading,
                span,
                tint,
                show,
            } => {
                if !shown(show) {
                    continue;
                }
                let keys: Vec<u64> = textures
                    .iter()
                    .filter_map(|t| texture(t).map(|(key, _)| key))
                    .collect();
                if keys.len() != textures.len() {
                    continue;
                }
                let (Some(heading), Some(span)) = (number(heading), amount(span)) else {
                    continue;
                };
                let n = keys.len() as f32;
                let span = span.clamped(1.0, 360.0) / 360.0;
                let start = (heading / 360.0 - span / 2.0).rem_euclid(1.0);
                let end = start + span;
                let mut at = start;
                while at < end - 1e-6 {
                    let wrapped = at.rem_euclid(1.0);
                    let cell = ((wrapped * n).floor() as usize).min(keys.len() - 1);
                    let cell_end = (at - wrapped) + (cell as f32 + 1.0) / n;
                    let stop = cell_end.min(end);
                    let u0 = wrapped * n - cell as f32;
                    let u1 = (wrapped + (stop - at)) * n - cell as f32;
                    out.push(Item::Image {
                        texture: keys[cell],
                        uv: [u0, 0.0, u1, 1.0],
                        rect: [
                            rect[0] + rect[2] * (at - start) / span,
                            rect[1],
                            rect[2] * (stop - at) / span,
                            rect[3],
                        ],
                        tint: *tint,
                    });
                    at = stop;
                }
            }
            Element::StripMarker {
                texture: id,
                rect,
                heading,
                span,
                x,
                z,
                size,
                tint,
                show,
            } => {
                let Some((key, _)) = texture(id).filter(|_| shown(show)) else {
                    continue;
                };
                let (Some(heading), Some(span), Some(px), Some(pz)) =
                    (number(heading), amount(span), number(x), number(z))
                else {
                    continue;
                };
                let span = span.clamped(1.0, 360.0);
                let (dx, dz) = (px - live.position[0], pz - live.position[1]);
                if dx.hypot(dz) < 1e-4 {
                    continue;
                }
                // Degrees clockwise from -z, as the heading is, so the
                // offset from the heading wraps to -180..180.
                let bearing = dx.atan2(-dz).to_degrees();
                let offset = (bearing - heading + 540.0).rem_euclid(360.0) - 180.0;
                if offset.abs() > span / 2.0 {
                    continue;
                }
                let centre = rect[0] + rect[2] * (0.5 + offset / span);
                out.push(Item::Image {
                    texture: key,
                    uv: [0.0, 0.0, 1.0, 1.0],
                    rect: [
                        centre - size[0] / 2.0,
                        rect[1] + (rect[3] - size[1]) / 2.0,
                        size[0],
                        size[1],
                    ],
                    tint: *tint,
                });
            }
            Element::EdgeArrow {
                texture: id,
                edge,
                rect,
                x,
                z,
                strength,
                tint,
                show,
            } => {
                let Some((key, _)) = texture(id).filter(|_| shown(show)) else {
                    continue;
                };
                let strength = number(strength).unwrap_or(0.0).clamped(0.0, 1.0);
                let (dx, dz) = (number(x).unwrap_or(0.0), number(z).unwrap_or(0.0));
                if strength <= 0.0 || dx.hypot(dz) < 1e-4 {
                    continue;
                }
                // The tracker's frame: right = (-ahead.z, ahead.x).
                let ahead = if live.facing == [0.0, 0.0] {
                    [0.0, -1.0]
                } else {
                    live.facing
                };
                let to_ahead = dx * ahead[0] + dz * ahead[1];
                let to_right = -dx * ahead[1] + dz * ahead[0];
                let nearest = if to_ahead.abs() >= to_right.abs() {
                    if to_ahead > 0.0 {
                        Edge::Top
                    } else {
                        Edge::Bottom
                    }
                } else if to_right > 0.0 {
                    Edge::Right
                } else {
                    Edge::Left
                };
                if nearest == *edge {
                    out.push(Item::Image {
                        texture: key,
                        uv: [0.0, 0.0, 1.0, 1.0],
                        rect: *rect,
                        tint: [tint[0], tint[1], tint[2], tint[3] * strength],
                    });
                }
            }
            Element::Tracker {
                rect,
                range,
                blip,
                blip_above,
                blip_below,
                blip_ally,
                blip_enemy,
                blip_neutral,
                height,
                blip_size,
                ally,
                enemy,
                neutral,
                min_speed,
                crouched,
                fired_within,
                allies,
                enemies,
                others,
                show,
            } => {
                if !shown(show) {
                    continue;
                }
                let [x, y, w, h] = *rect;
                let (cx, cy) = (x + w / 2.0, y + h / 2.0);
                for c in live.contacts {
                    let side = match c.relation {
                        Relation::Ally => allies.then_some((ally, blip_ally)),
                        Relation::Enemy => enemies.then_some((enemy, blip_enemy)),
                        Relation::Neutral => others.then_some((neutral, blip_neutral)),
                    };
                    let Some((tint, own)) = side else {
                        continue;
                    };
                    if c.range() > *range || !c.noticed(*min_speed, *crouched, *fired_within) {
                        continue;
                    }
                    let id = if c.above > *height {
                        blip_above.as_ref().unwrap_or(blip)
                    } else if c.above < -height {
                        blip_below.as_ref().unwrap_or(blip)
                    } else {
                        own.as_ref().unwrap_or(blip)
                    };
                    let Some((key, _)) = texture(id) else {
                        continue;
                    };
                    let px = cx + c.right / range * w / 2.0;
                    let py = cy - c.ahead / range * h / 2.0;
                    out.push(Item::Image {
                        texture: key,
                        uv: [0.0, 0.0, 1.0, 1.0],
                        rect: [
                            px - blip_size[0] / 2.0,
                            py - blip_size[1] / 2.0,
                            blip_size[0],
                            blip_size[1],
                        ],
                        tint: *tint,
                    });
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

    #[test]
    fn a_name_tags_claim_styles_whose_names_show_and_a_tools_claim_is_a_region() {
        let style = layout(
            serde_json::json!({ "schema_version": 1, "region": "name_tags",
            "name_tags": { "teammates": "always", "others": "aimed", "outline": false,
                "color": [0.4, 0.8, 1.0, 1.0] } }),
        );
        let s = style.name_tags.as_ref().unwrap();
        assert!(s.shows(true, false));
        assert!(!s.shows(false, false) && s.shows(false, true));
        assert!(!s.outline && s.height == 0.85);
        // Stock: every name; a bare claim hides them (no style).
        assert!(NameTagStyle::default().shows(false, false));
        let bare = layout(serde_json::json!({ "schema_version": 1, "region": "name_tags" }));
        assert!(bare.name_tags.is_none());
        // A style anywhere else is refused.
        let wrong: Layout = serde_json::from_value(serde_json::json!({ "schema_version": 1,
            "region": "top_left", "name_tags": {} }))
        .unwrap();
        assert!(wrong.validate().is_err());
        let tools = layout(serde_json::json!({ "schema_version": 1, "region": "tools" }));
        assert_eq!(tools.region.stock(), Some("tool bar"));
        assert_eq!(Region::parse("name_tags"), Some(Region::NameTags));
        let building = layout(serde_json::json!({ "schema_version": 1, "region": "building" }));
        assert_eq!(Region::parse("building"), Some(Region::Building));
        assert!(building.region.stock().is_some());
        assert_eq!(Region::Tools.rect(640.0), [576.0, 0.0, 64.0, HEIGHT]);
    }

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
            seat: vec![],
            minigame: None,
        };
        let viewer = |body, holding| Viewer {
            body,
            holding,
            vehicle: "",
            seat: None,
            minigame: false,
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
    fn when_can_ask_for_a_minigame() {
        let viewer = |minigame| Viewer {
            body: "",
            holding: "",
            vehicle: "",
            seat: None,
            minigame,
        };
        let playing = When {
            minigame: Some(true),
            ..When::default()
        };
        assert!(playing.matches(&viewer(true)) && !playing.matches(&viewer(false)));
        assert!(!playing.is_always());
    }

    fn feed_kill(age: f32, killer: Option<&str>, victim: &str, weapon: &str) -> FeedKill {
        FeedKill {
            age,
            killer: killer.map(|k| (k.to_string(), Some([1.0, 0.0, 0.0, 1.0]))),
            victim: (victim.to_string(), None),
            weapon: weapon.into(),
        }
    }
    fn spans_text(item: &Item) -> String {
        let Item::Line { spans, .. } = item else {
            panic!("{item:?}")
        };
        spans
            .iter()
            .map(|s| match s {
                Span::Text { text, .. } => text.clone(),
                Span::Image { texture, .. } => format!("#{texture}"),
            })
            .collect()
    }

    #[test]
    fn a_kill_feed_writes_the_newest_lines_with_icons_and_team_colours() {
        let l = layout(serde_json::json!({
            "schema_version": 1, "region": "top_right",
            "elements": [{ "type": "kill_feed", "at": [100, 10], "lines": 2,
                "seconds": 5, "icons": { "gun": "pack:texture/bar" },
                "names": { "Fall": "fell" } }]
        }));
        assert!(l.has_kill_feed());
        let kills = [
            feed_kill(4.0, Some("old"), "x", "Gun"),
            feed_kill(1.0, Some("amy"), "bob", "Gun"),
            feed_kill(4.9, None, "cat", "Fall"),
            feed_kill(9.0, Some("gone"), "y", "Gun"),
        ];
        let live = Live {
            kills: &kills,
            contacts: &[],
            ..Live::default()
        };
        let items = resolve_live(&l, &values, &textures, &live);
        // Three are recent; the newest two show, one line apart.
        assert_eq!(items.len(), 2);
        assert_eq!(spans_text(&items[0]), "amy [#3] bob");
        assert_eq!(spans_text(&items[1]), "[fell] cat");
        let Item::Line { at, spans, .. } = &items[0] else {
            panic!()
        };
        assert_eq!(*at, [100.0, 10.0]);
        assert!(matches!(spans[0], Span::Text { color, .. } if color == [1.0, 0.0, 0.0, 1.0]));
        let Item::Line { at, spans, .. } = &items[1] else {
            panic!()
        };
        assert_eq!(at[1], 26.0);
        // Fading in its last half second.
        assert!(matches!(spans[3], Span::Text { color, .. } if (color[3] - 0.2).abs() < 1e-4));
    }

    fn feed_line(feed: &str, age: f32, seconds: f32, text: &str, icon: Option<&str>) -> FeedLine {
        FeedLine {
            feed: feed.into(),
            age,
            seconds,
            text: text.into(),
            color: None,
            icon: icon.map(String::from),
        }
    }

    #[test]
    fn a_message_feed_shows_its_own_lines_for_the_seconds_each_was_given() {
        let l = layout(serde_json::json!({
            "schema_version": 1, "region": "top_left",
            "elements": [{ "type": "message_feed", "feed": "radio", "at": [8, 8],
                "lines": 2, "spacing": 12, "color": [0, 1, 0, 1],
                "icons": { "radio": "pack:texture/bar" } }]
        }));
        let lines = [
            feed_line("radio", 1.0, 3.0, "old", None),
            feed_line("radio", 4.0, 3.0, "expired", None),
            feed_line("other", 0.5, 3.0, "elsewhere", None),
            feed_line("radio", 0.5, 3.0, "Moving out", Some("radio")),
            feed_line("radio", 2.75, 3.0, "Clear", None),
        ];
        let live = Live {
            lines: &lines,
            ..Live::default()
        };
        let items = resolve_live(&l, &values, &textures, &live);
        // The newest two of this feed's unexpired lines, a row apart.
        assert_eq!(items.len(), 2);
        assert_eq!(spans_text(&items[0]), "#3Moving out");
        assert_eq!(spans_text(&items[1]), "Clear");
        let Item::Line { at, spans, .. } = &items[1] else {
            panic!()
        };
        assert_eq!(*at, [8.0, 20.0]);
        // Its own colour is the element's, faded over the last half second.
        assert!(matches!(spans[0], Span::Text { color, .. }
            if color[1] == 1.0 && (color[3] - 0.5).abs() < 1e-4));
    }

    #[test]
    fn a_message_feed_is_named_and_bounded() {
        for bad in [
            serde_json::json!({ "type": "message_feed", "feed": "", "at": [0, 0] }),
            serde_json::json!({ "type": "message_feed", "feed": "a b", "at": [0, 0] }),
            serde_json::json!({ "type": "message_feed", "feed": "radio", "at": [0, 0], "lines": 17 }),
            serde_json::json!({ "type": "message_feed", "feed": "radio", "at": [0, 0],
                "icons": { "x": "not a texture" } }),
        ] {
            let doc = serde_json::json!({ "schema_version": 1, "region": "top_left",
                "elements": [bad] });
            let parsed: Result<Layout, _> = serde_json::from_value(doc);
            assert!(parsed.map_or(true, |l| l.validate().is_err()));
        }
    }

    fn contact(id: u64, right: f32, ahead: f32, speed: f32, relation: Relation) -> Contact {
        Contact {
            id,
            right,
            ahead,
            above: 0.0,
            speed,
            crouched: false,
            fired: None,
            relation,
        }
    }

    #[test]
    fn a_tracker_places_moving_contacts_by_bearing_and_hides_the_quiet() {
        let l = layout(serde_json::json!({
            "schema_version": 1, "region": "bottom_left",
            "elements": [{ "type": "tracker", "rect": [0, 0, 100, 100], "range": 25,
                "blip": "pack:texture/bar", "blip_size": [4, 4] }]
        }));
        assert!(l.has_tracker());
        let mut crouching = contact(4, 0.0, 5.0, 3.0, Relation::Enemy);
        crouching.crouched = true;
        let mut shooting = crouching;
        shooting.id = 5;
        shooting.fired = Some(0.5);
        let contacts = [
            contact(1, 12.5, 0.0, 3.0, Relation::Ally),
            contact(2, 0.0, 25.0, 3.0, Relation::Enemy),
            contact(3, 0.0, 30.0, 3.0, Relation::Enemy),
            crouching,
            shooting,
            contact(6, 0.0, 5.0, 0.0, Relation::Enemy),
        ];
        let live = Live {
            kills: &[],
            contacts: &contacts,
            ..Live::default()
        };
        let items = resolve_live(&l, &values, &textures, &live);
        let at: Vec<[f32; 2]> = items
            .iter()
            .map(|i| match i {
                Item::Image { rect, .. } => [rect[0] + 2.0, rect[1] + 2.0],
                _ => panic!("{i:?}"),
            })
            .collect();
        // Right is right, ahead is up; out of range, crouch-walking and
        // standing still do not show; firing does.
        assert_eq!(at, [[75.0, 50.0], [50.0, 0.0], [50.0, 40.0]]);
    }

    #[test]
    fn edge_arrows_pick_the_edge_the_hit_came_from_and_fade() {
        let arrow = |edge: &str| {
            layout(serde_json::json!({
                "schema_version": 1, "region": "top_left",
                "elements": [{ "type": "edge_arrow", "texture": "pack:texture/bar",
                    "edge": edge, "rect": [10, 0, 8, 8],
                    "x": "game:player/hit_x", "z": "game:player/hit_z",
                    "strength": "game:player/hit_strength", "tint": [1, 1, 1, 0.5] }]
            }))
        };
        assert_eq!(arrow("top").sources().len(), 3);
        let read = |x: f32, z: f32, strength: f32| {
            move |source: &Source| match source {
                Source::State(s) if s == "game:player/hit_x" => Some(Value::Number(x)),
                Source::State(s) if s == "game:player/hit_z" => Some(Value::Number(z)),
                Source::State(s) if s == "game:player/hit_strength" => {
                    Some(Value::Number(strength))
                }
                _ => None,
            }
        };
        // The viewer faces +x (ahead = (1, 0)), so right = (0, 1).
        let live = Live {
            facing: [1.0, 0.0],
            ..Live::default()
        };
        let drawn = |edge: &str, x: f32, z: f32, strength: f32| -> Option<f32> {
            let items = resolve_live(&arrow(edge), &read(x, z, strength), &textures, &live);
            match items.as_slice() {
                [] => None,
                [Item::Image { tint, .. }] => Some(tint[3]),
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(drawn("top", 5.0, 1.0, 1.0), Some(0.5), "ahead is the top");
        assert_eq!(drawn("bottom", 5.0, 1.0, 1.0), None);
        assert_eq!(drawn("bottom", -5.0, 1.0, 1.0), Some(0.5));
        assert_eq!(
            drawn("left", 1.0, -5.0, 1.0),
            Some(0.5),
            "to -z is the left"
        );
        assert_eq!(
            drawn("right", 1.0, 5.0, 0.5),
            Some(0.25),
            "to +z is the right, faded"
        );
        assert_eq!(drawn("top", 5.0, 0.0, 0.0), None, "no strength, no arrow");
        assert_eq!(drawn("top", 0.0, 0.0, 1.0), None, "no direction, no arrow");
    }

    #[test]
    fn stock_texts_are_known_keys() {
        let l = layout(serde_json::json!({
            "schema_version": 1, "region": "chat",
            "texts": { "respawn_countdown": "Respawn in {}", "building_disabled": "" }
        }));
        assert_eq!(l.texts["building_disabled"], "");
        let bad: Layout = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "region": "chat", "texts": { "nope": "x" }
        }))
        .unwrap();
        assert!(bad.validate().is_err());
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
                serde_json::json!({ "schema_version": 1, "region": "radar" })
            )
            .is_err(),
            "unknown regions are refused"
        );
    }
    #[test]
    fn inventory_sources_are_bounded_and_keep_their_capacity_binding() {
        let source = Source::parse("item_ammo:gear:weapon/grenade").unwrap();
        assert_eq!(
            source.natural_max(),
            Amount::Source("item_ammo_max:gear:weapon/grenade".into())
        );
        assert!(Source::parse("item_ammo:").is_none());
        assert!(Source::parse("item_ammo:bad id").is_none());
        assert!(Source::parse(&format!("item_ammo:{}", "x".repeat(161))).is_none());
    }
}

#[cfg(test)]
mod strip_tests {
    use super::*;

    fn compass() -> Layout {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1, "region": "top_center",
            "elements": [{
                "type": "strip",
                "textures": ["t:texture/left", "t:texture/right"],
                "rect": [0.0, 0.0, 100.0, 10.0],
                "heading": "heading", "span": 90.0
            }]
        }))
        .expect("layout")
    }

    fn items(heading: f32) -> Vec<Item> {
        let layout = compass();
        layout.validate().expect("valid");
        let value = move |s: &Source| match s {
            Source::Heading => Some(Value::Number(heading)),
            _ => None,
        };
        let texture = |t: &str| match t {
            "t:texture/left" => Some((1, [1024, 32])),
            "t:texture/right" => Some((2, [1024, 32])),
            _ => None,
        };
        resolve(&layout, &value, &texture)
    }

    fn pieces(items: &[Item]) -> Vec<(u64, [f32; 4], [f32; 4])> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::Image {
                    texture, uv, rect, ..
                } => Some((*texture, *uv, *rect)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_strip_scrolls_by_heading_and_wraps_round_the_turn() {
        // Facing 180 degrees, a quarter turn wide: it straddles the seam of
        // the two textures (the left's last quarter, the right's first).
        let mid = pieces(&items(180.0));
        assert_eq!(mid.len(), 2, "{mid:?}");
        assert_eq!((mid[0].0, mid[1].0), (1, 2));
        assert!(
            (mid[0].1[0] - 0.75).abs() < 1e-4 && (mid[0].1[2] - 1.0).abs() < 1e-4,
            "{mid:?}"
        );
        assert!(
            mid[1].1[0].abs() < 1e-4 && (mid[1].1[2] - 0.25).abs() < 1e-4,
            "{mid:?}"
        );
        // Facing 0 the window straddles the end and start of the strip.
        let wrap = pieces(&items(0.0));
        assert_eq!(wrap.len(), 2, "{wrap:?}");
        assert_eq!(wrap[0].0, 2);
        assert_eq!(wrap[1].0, 1);
        assert!(
            (wrap[0].2[2] + wrap[1].2[2] - 100.0).abs() < 1e-3,
            "the pieces fill the rect"
        );
        assert!((wrap[1].2[0] - wrap[0].2[2]).abs() < 1e-3, "side by side");
    }

    #[test]
    fn a_marker_sits_on_the_strip_at_the_bearing_of_its_world_point() {
        let layout: Layout = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "region": "top_center",
            "elements": [{
                "type": "strip_marker", "texture": "t:texture/mark",
                "rect": [0.0, 0.0, 100.0, 10.0], "heading": "heading", "span": 90.0,
                "x": "t:global/x", "z": "t:global/z", "size": [10.0, 8.0]
            }]
        }))
        .expect("layout");
        layout.validate().expect("valid");
        let drawn = |heading: f32, point: Option<[f32; 2]>| {
            let value = move |s: &Source| match s {
                Source::Heading => Some(Value::Number(heading)),
                Source::State(k) if k == "t:global/x" => point.map(|p| Value::Number(p[0])),
                Source::State(k) if k == "t:global/z" => point.map(|p| Value::Number(p[1])),
                _ => None,
            };
            let live = Live {
                position: [10.0, 10.0],
                ..Live::default()
            };
            pieces(&resolve_live(
                &layout,
                &value,
                &|_| Some((7, [16, 16])),
                &live,
            ))
        };
        // Facing north (-z) with the point straight ahead: centred.
        let ahead = drawn(0.0, Some([10.0, -40.0]));
        assert_eq!(ahead.len(), 1, "{ahead:?}");
        assert!((ahead[0].2[0] - 45.0).abs() < 1e-3 && (ahead[0].2[1] - 1.0).abs() < 1e-3);
        // A point due east is 90 degrees clockwise: off a 90 degree strip facing north,
        // and at its middle once the viewer faces east.
        assert!(drawn(0.0, Some([60.0, 10.0])).is_empty());
        let east = drawn(90.0, Some([60.0, 10.0]));
        assert!((east[0].2[0] - 45.0).abs() < 1e-3, "{east:?}");
        // 22.5 degrees right of the heading is a quarter of the way from the middle to the right edge.
        let bearing = 112.5f32.to_radians();
        let right = drawn(
            90.0,
            Some([10.0 + 50.0 * bearing.sin(), 10.0 - 50.0 * bearing.cos()]),
        );
        assert!((right[0].2[0] - 70.0).abs() < 1e-2, "{right:?}");
        // A point with no value, or the viewer standing on it, draws nothing.
        assert!(drawn(0.0, None).is_empty());
        assert!(drawn(0.0, Some([10.0, 10.0])).is_empty());
    }

    #[test]
    fn a_strip_reads_a_field_of_view_source_for_its_span() {
        let layout: Layout = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "region": "top_center",
            "elements": [{
                "type": "strip", "textures": ["t:texture/left"],
                "rect": [0.0, 0.0, 100.0, 10.0], "heading": "heading", "span": "fov"
            }]
        }))
        .expect("layout");
        layout.validate().expect("valid");
        let value = |s: &Source| match s {
            Source::Heading => Some(Value::Number(180.0)),
            Source::Fov => Some(Value::Number(36.0)),
            _ => None,
        };
        let drawn = pieces(&resolve(&layout, &value, &|_| Some((1, [1024, 32]))));
        assert_eq!(drawn.len(), 1);
        assert!(
            (drawn[0].1[2] - drawn[0].1[0] - 0.1).abs() < 1e-4,
            "36 of 360 degrees: {drawn:?}"
        );
    }
}
