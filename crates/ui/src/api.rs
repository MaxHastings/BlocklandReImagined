//! The integration boundary between the UI and the game client.
//!
//! - The host feeds authoritative state in as [`UiUpdate`] view models.
//! - The UI emits requests as [`UiAction`]s tagged with a [`RequestId`]. A
//!   request only *asks* for a change. The host answers requests that need
//!   an answer with [`UiUpdate::ActionResult`]; screens show a pending state
//!   until then and report rejections.
//! - Gameplay input that the original client forwarded to the simulation
//!   (movement, triggers, brick shifting, …) arrives as [`GameAction`]s
//!   inside [`UiAction::Game`].
//!
//! Nothing here depends on Torque. Resource lists (maps, bricks, tools,
//! prints, event tables, datablock menus) come from catalogs the host
//! supplies; the UI pack only provides the original art, fonts and layouts.

use crate::geom::Rgba;
use crate::input::{Chord, MouseButton};
use crate::schema::ParamSpec;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type RequestId = u64;

/// How to draw an icon: an image from the UI pack, a host texture
/// (registered with the renderer, e.g. a brick icon render target), or none.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum IconRef {
    #[default]
    None,
    Pack(String),
    External(u64),
}

// ----------------------------------------------------------------- catalogs

/// A game mode an enabled Add-On declares, for Start Game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameModeInfo {
    /// Content id the host understands (`package:mode/name`).
    pub id: String,
    pub name: String,
    pub description: String,
    /// The map id it always plays on, or None when the host picks.
    pub map: Option<String>,
}

pub const HOST_COLORSET_PREF: &str = "$Pref::Server::ColorSet";

/// Local host choices. The client revalidates the selected file before hosting;
/// the resulting world palette remains authoritative for joined players.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostColorset {
    /// Empty means the stock colorset; other IDs come from the local catalog.
    pub id: String,
    pub name: String,
    pub divisions: Vec<PaintDivision>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapInfo {
    /// Stable id the host understands (e.g. the converted map bundle id).
    pub id: String,
    pub name: String,
    pub description: String,
    pub preview: IconRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub address: String,
    pub name: String,
    pub password: bool,
    pub dedicated: bool,
    pub ping_ms: Option<u32>,
    pub players: u32,
    pub max_players: u32,
    pub bricks: u32,
    pub map: String,
    /// Starred by the player (listed first; Favorite button toggles it).
    #[serde(default)]
    pub favorite: bool,
}

/// One brick in the server's catalog, in datablock (registration) order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrickInfo {
    /// Content id the host understands.
    pub id: String,
    /// `uiName` (favorites are stored by this name, like v20).
    pub ui_name: String,
    pub category: String,
    pub subcategory: String,
    pub icon: IconRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub icon: IconRef,
    /// Icon colour shift (`doColorShift`/`colorShiftColor`).
    pub tint: Option<[u8; 4]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrintInfo {
    pub id: String,
    /// Display/letter name (`A`, `-bang`, …).
    pub name: String,
    pub icon: IconRef,
}

/// A server colour division (`DIV:Standard` …). Colours are 0..1 floats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaintDivision {
    pub name: String,
    pub colors: Vec<[f32; 4]>,
}

/// A datablock choice in wrench/event menus (`uiName` + host id).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub id: String,
    pub name: String,
}

/// Datablock menus for the wrench dialogs and datablock event parameters,
/// keyed by Torque class (`FxLightData`, `ParticleEmitterData`, `ItemData`,
/// `AudioProfile`, `Music`, `Sound`, `ProjectileData`, `PlayerData`,
/// `Vehicle`). Lists are shown in the order given except where v20 sorted
/// them (emitters, items, sounds, vehicles are sorted by the UI).
pub type DatablockMenus = BTreeMap<String, Vec<Choice>>;

/// Event tables the host supports. The UI lists only `supported` inputs and
/// outputs in its menus; rows referencing anything else are shown read-only
/// (see `EventRow::Preserved`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EventCatalog {
    pub inputs: Vec<EventInputInfo>,
    pub outputs: Vec<EventOutputInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventInputInfo {
    pub name: String,
    /// (target name, target class). `<NAMED BRICK>` is appended by the UI
    /// when the target class is `fxDTSBrick` and named targets are allowed.
    pub targets: Vec<(String, String)>,
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventOutputInfo {
    #[serde(default)]
    pub provider: String,
    pub class: String,
    pub name: String,
    pub params: Vec<ParamSpec>,
    pub supported: bool,
}

impl EventCatalog {
    /// Stock tables from the UI pack, marking `supported` by name. Hosts can
    /// start from this and replace the flags with their own capability list.
    pub fn from_tables(
        t: &crate::schema::EventTables,
        supported_inputs: &[&str],
        supported_outputs: &[&str],
    ) -> Self {
        let outputs: Vec<_> = supported_outputs
            .iter()
            .map(|&name| ("fxDTSBrick", name))
            .collect();
        Self::from_capabilities(t, supported_inputs, &outputs)
    }

    /// Runtime capabilities are class-qualified: Player/Client/MiniGame outputs
    /// with the same spelling never accidentally inherit brick support.
    pub fn from_capabilities(
        t: &crate::schema::EventTables,
        supported_inputs: &[&str],
        supported_outputs: &[(&str, &str)],
    ) -> Self {
        EventCatalog {
            inputs: t
                .inputs
                .iter()
                .map(|i| EventInputInfo {
                    name: i.name.clone(),
                    targets: i.targets.clone(),
                    supported: supported_inputs
                        .iter()
                        .any(|s| s.eq_ignore_ascii_case(&i.name)),
                })
                .collect(),
            outputs: t
                .outputs
                .iter()
                .map(|o| EventOutputInfo {
                    provider: "Blockland".into(),
                    class: o.class.clone(),
                    name: o.name.clone(),
                    params: o.params.clone(),
                    supported: supported_outputs.iter().any(|(class, name)| {
                        class.eq_ignore_ascii_case(&o.class) && name.eq_ignore_ascii_case(&o.name)
                    }),
                })
                .collect(),
        }
    }
}

/// The subset that matches `bri-world::Action` one-to-one (audit 05 §P0-7).
pub const CURRENT_BRICK_EVENT_INPUTS: &[&str] = &["onActivate", "onPlayerTouch"];
pub const CURRENT_BRICK_EVENT_OUTPUTS: &[&str] = &[
    "setColor",
    "setColorFX",
    "setColliding",
    "setRendering",
    "setRayCasting",
    "setLight",
    "setEmitter",
];

// ---------------------------------------------------------- brick editing

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WrenchVariant {
    Normal,
    Sound,
    VehicleSpawn,
}

/// Wrench fields (`SetWrenchData` N/LDB/EDB/EDIR/IDB/IPOS/IDIR/IRT/RC/C/R,
/// plus the sound and vehicle-spawn variants). Datablock ids are `Choice::id`s;
/// `None` = NONE.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WrenchData {
    /// Directly authored detection size, in world X/Y/Z units; None: automatic.
    pub rule_region: Option<[f32; 3]>,
    /// Effective automatic dimensions from the client geometry adapter.
    pub rule_region_default: Option<[f32; 3]>,
    pub region_inputs: bool,
    pub name: String,
    pub light: Option<String>,
    pub emitter: Option<String>,
    /// 0..=5 Up Down North East South West.
    pub emitter_dir: u8,
    pub item: Option<String>,
    /// 0..=5 Up Down North East South West.
    pub item_pos: u8,
    /// 2..=5 North East South West.
    pub item_dir: u8,
    pub item_respawn_ms: u32,
    pub raycasting: bool,
    pub colliding: bool,
    pub rendering: bool,
    pub sound: Option<String>,
    pub vehicle: Option<String>,
    pub recolor_vehicle: bool,
}

/// One event row as the dialog edits it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventRow {
    Editable(EventLine),
    /// An imported event the host cannot run (unsupported input/output or
    /// parameters). Shown read-only with its original text and sent back
    /// unchanged so saves round-trip without silent loss.
    Preserved {
        enabled: bool,
        text: String,
        token: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventLine {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<bri_events::rules::Condition>,
    pub enabled: bool,
    pub delay_ms: u32,
    pub input: String,
    /// Target name as listed (`Self`, `Player`, …) or `<NAMED BRICK>`.
    pub target: String,
    pub named_target: Option<String>,
    pub output: String,
    pub params: Vec<ParamValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParamValue {
    Int(i64),
    Float(f32),
    Bool(bool),
    Text(String),
    /// Datablock id (`Choice::id`), `None` = NONE.
    Datablock(Option<String>),
    Vector([f32; 3]),
    /// List value (the number registered with the label).
    List(i64),
    PaintColor(u32),
}

/// The avatar part keys of `$pref::Avatar::*`.
pub const AVATAR_PART_KEYS: [&str; 12] = [
    "Hat",
    "Accent",
    "Pack",
    "SecondPack",
    "Chest",
    "Hip",
    "LArm",
    "RArm",
    "LHand",
    "RHand",
    "LLeg",
    "RLeg",
];

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AvatarPrefs {
    /// `$pref::Avatar::*` values by short name: parts (`Hat`, `Accent`,
    /// `Pack`, `SecondPack`, `Chest`, `Hip`, `LArm`…) by the lowercase name of
    /// the chosen part (`helmet`, `visor`), never a position in the pack's
    /// lists; `FaceName`/`DecalName` as file base names; colours (`HatColor`,
    /// `HeadColor`, `TorsoColor`…) as `"r g b a"` 0..1 floats. `FaceColor` and
    /// `DecalColor` are v20's image-list frames, re-derived from the names.
    pub values: BTreeMap<String, String>,
    pub symmetry: bool,
    pub lan_name: String,
    pub clan_prefix: String,
    pub clan_suffix: String,
}

impl AvatarPrefs {
    pub fn get(&self, k: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.as_str())
    }
    pub fn set(&mut self, k: &str, v: impl Into<String>) {
        let key = self
            .values
            .keys()
            .find(|key| key.eq_ignore_ascii_case(k))
            .cloned()
            .unwrap_or_else(|| k.to_string());
        self.values.insert(key, v.into());
    }
    /// Each part as its place in the pack's lists (`helmet` -> `Hat` 1), as
    /// v20's prefs and Slayer's uniforms keep them; [`Self::name_parts`]
    /// the other way. A part the lists lack is left out.
    pub fn part_positions(&self, data: &crate::schema::AvatarData) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for key in AVATAR_PART_KEYS {
            let name = self.part(key).to_ascii_lowercase();
            let list = if key == "Accent" {
                data.accents_allowed
                    .get(&self.part("Hat").to_ascii_lowercase())
            } else {
                data.parts.get(&key.to_ascii_lowercase())
            };
            let at = list.and_then(|l| l.iter().position(|p| p.eq_ignore_ascii_case(&name)));
            match at {
                Some(i) => {
                    out.insert(key.to_owned(), i);
                }
                // No accent (or `none` not listed) is the first place.
                None if key == "Accent" => {
                    out.insert(key.to_owned(), 0);
                }
                None => {}
            }
        }
        out
    }
    /// The chosen part's name for `k` (`Hat` -> `helmet`), empty when unset.
    pub fn part(&self, k: &str) -> &str {
        self.get(k).unwrap_or_default()
    }
    /// v20 prefs store parts as positions in the pack's lists; name them.
    /// Values that are already names are kept.
    pub fn name_parts(&mut self, data: &crate::schema::AvatarData) {
        let position = |v: &str| v.trim().parse::<f64>().ok().map(|i| i.max(0.0) as usize);
        for key in AVATAR_PART_KEYS.iter().filter(|k| **k != "Accent") {
            if let Some(index) = self.get(key).and_then(position) {
                let list = data.parts.get(&key.to_ascii_lowercase());
                let name = list.and_then(|l| l.get(index).or(l.first()));
                self.set(key, name.map_or("none".into(), |n| n.to_ascii_lowercase()));
            }
        }
        if let Some(index) = self.get("Accent").and_then(position) {
            let hat = self.part("Hat").to_ascii_lowercase();
            let name = data.accents_allowed.get(&hat).and_then(|l| l.get(index));
            self.set(
                "Accent",
                name.map_or("none".into(), |n| n.to_ascii_lowercase()),
            );
        }
    }
    pub fn color(&self, k: &str) -> [f32; 4] {
        let v: Vec<f32> = self
            .get(k)
            .unwrap_or("1 1 1 1")
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        [
            v.first().copied().unwrap_or(1.0),
            v.get(1).copied().unwrap_or(1.0),
            v.get(2).copied().unwrap_or(1.0),
            v.get(3).copied().unwrap_or(1.0),
        ]
    }
    pub fn set_color(&mut self, k: &str, c: [f32; 4]) {
        self.set(k, format!("{} {} {} {}", c[0], c[1], c[2], c[3]));
    }
    /// Stock defaults from the pack's `$pref::Avatar::*` / `$pref::Player::*`,
    /// with v20's part positions named from `data`.
    pub fn from_prefs(
        p: &crate::prefs::Prefs,
        pack_prefs: &BTreeMap<String, String>,
        data: &crate::schema::AvatarData,
    ) -> Self {
        let mut a = AvatarPrefs::default();
        for k in pack_prefs.keys() {
            let low = k.to_ascii_lowercase();
            if let Some(short) = low.strip_prefix("$pref::avatar::") {
                let orig = &k[k.len() - short.len()..];
                if let Some(v) = p.get(k) {
                    a.values.insert(orig.to_string(), v.to_string());
                }
            }
        }
        a.symmetry = p.bool_or("$pref::Player::Symmetry", true);
        a.lan_name = p.str_or("$pref::Player::LANName", "Blockhead").to_string();
        a.clan_prefix = p.str_or("$Pref::Player::ClanPrefix", "").to_string();
        a.clan_suffix = p.str_or("$Pref::Player::ClanSuffix", "").to_string();
        a.name_parts(data);
        a
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveFileInfo {
    pub name: String,
    pub map: String,
    pub modified: String,
    pub description: String,
    pub brick_count: Option<u32>,
    /// The file could not be read; it is listed so it can be saved over.
    pub damaged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerMode {
    SinglePlayer,
    Lan,
    /// Reachable by direct IP. No master-server listing exists.
    Internet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatChannel {
    Say,
    Team,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenshotKind {
    Normal,
    NoHud,
    DepthOfField,
}

/// Held controls forwarded to the simulation. `down` follows the physical
/// key; releases are always delivered even if the key went down before a
/// dialog opened or a text field took focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HeldControl {
    Forward,
    Backward,
    Left,
    Right,
    /// Trigger 2.
    Jump,
    /// Trigger 3.
    Crouch,
    /// Trigger 4.
    Jet,
    /// Trigger 0 (use tool/brick/fire; activate when empty-handed).
    Fire,
    /// `$RunMultiplier = 0.4` while held.
    Walk,
    /// FOV = zoom FOV while held.
    Zoom,
    FreeLook,
}

/// Gameplay requests produced from key binds (client-script behaviour such as
/// brick key repeat, super-shift and jump/jet combo is already applied).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GameAction {
    Held {
        control: HeldControl,
        down: bool,
    },
    /// Mouse look: raw counts × sensitivity × 0.005 (the host multiplies by
    /// cameraFov/90, `getMouseAdjustAmount`@c:20768). Pitch already includes
    /// Invert Mouse; vehicle inversion is the host's (it knows when driving).
    Look {
        yaw: f32,
        pitch: f32,
    },
    ToggleFirstPerson {
        fast: bool,
    },
    DropCameraAtPlayer,
    DropPlayerAtCamera,
    Suicide,
    NextSeat,
    PrevSeat,
    UseLight,
    DropTool,
    /// x away(+)/towards(−), y left(+)/right(−), z plates (±3 = one brick).
    ShiftBrick {
        x: i32,
        y: i32,
        z: i32,
    },
    SuperShiftBrick {
        x: i32,
        y: i32,
        z: i32,
    },
    /// +1 clockwise, −1 counter-clockwise.
    RotateBrick {
        dir: i32,
    },
    PlantBrick,
    CancelBrick,
    UndoBrick,
    /// Wheel while zoomed: new zoom FOV (5..=85).
    SetZoomFov {
        fov: f32,
    },
    Screenshot {
        kind: ScreenshotKind,
    },
    /// Write the performance overlay's recent history to a file.
    SavePerfCapture,
    ToggleFullscreen,
    Emote {
        name: String,
    },
    ToggleBuildMacroRecording,
    PlayBackBuildMacro,
    /// The wheel while the held tool takes it: that tool image's `wheel`
    /// command, with whole notches (positive rolled forward, away from you).
    ToolWheel {
        notches: i32,
    },
    /// The wheel in an Add-On's orbit camera that zooms: whole notches,
    /// positive rolled forward (closer).
    CameraZoom {
        notches: i32,
    },
    /// A key a package HUD or bind declared: send that package's command,
    /// with whether the key went down or up when it is held
    /// ([`PackageBind::hold`]).
    Package {
        package: String,
        command: String,
        #[serde(default)]
        pressed: Option<bool>,
    },
}

/// Where a package HUD panel sits on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}
/// A HUD panel an enabled package declared, with values already resolved
/// from replicated state by the host. Data only: the UI draws it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackagePanel {
    pub anchor: PanelAnchor,
    pub title: String,
    pub background: Rgba,
    pub accent: Rgba,
    pub text: Rgba,
    /// (label, value, colour)
    pub rows: Vec<(String, String, Rgba)>,
    /// (key letter, label) hints.
    pub keys: Vec<(char, String)>,
}
/// A key players can bind to a package's command in Options → Controls,
/// from an enabled package's `binds.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageBind {
    /// The Controls heading it goes under.
    pub division: String,
    pub name: String,
    pub package: String,
    pub command: String,
    /// Default key on this platform, as Controls writes it.
    pub key: Option<String>,
    /// Sent as the key goes down and again as it comes up.
    pub hold: bool,
    /// A screen it opens instead of sending `command`: `minigame_addons`
    /// or `help`.
    #[serde(default)]
    pub screen: Option<String>,
}
impl PackageBind {
    /// The bind's command in the key map and saved controls.
    pub fn bind_command(&self) -> String {
        format!("package:{}:{}", self.package, self.command)
    }
}
/// A splash over the main menu (an Add-On's `splash.json`), its pictures
/// already in UI textures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplashView {
    /// Back to front: texture, rect on a 640x480 screen (none: the whole
    /// screen) and fade-in.
    pub layers: Vec<(u64, Option<[i32; 4]>, u32)>,
    pub falling: Option<SplashFallingView>,
    /// ML text, rect on a 640x480 screen, shown after this many ms.
    pub tip: Option<(String, [i32; 4], u32)>,
    pub close_after_ms: u32,
    pub fade_out_ms: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplashFallingView {
    pub textures: Vec<u64>,
    pub size: u32,
    pub chance: u32,
    pub speed: [u32; 2],
    pub step_ms: u32,
    pub closing_speed: u32,
}
/// A help page a running Add-On adds to the Help dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddOnHelpPage {
    /// The package it comes from.
    pub package: String,
    pub name: String,
    pub text: String,
    /// Opened by itself the first time the player meets the Add-On.
    #[serde(default)]
    pub welcome: bool,
}
/// A key a package HUD binds to one of its commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageKey {
    /// Lower-case letter.
    pub key: char,
    pub package: String,
    pub command: String,
}

/// How a save whose colours differ from the world's is loaded
/// (`ColorWarning_Click*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorLoad {
    /// Each colour becomes the nearest one the world has.
    Match,
    /// The save's colours are added to the world's.
    Append,
    /// Back to Load Bricks without loading.
    Cancel,
}

/// A load the player canceled in `LoadBricksColorGui`: Load Bricks stays
/// open without an error.
pub const LOAD_CANCELED: &str = "Load canceled";

/// Requests from the UI. See the module docs for the request/answer rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiAction {
    /// Actor is resolved by the authenticated connection, never this payload.
    Admin(crate::models::admin::AdminAction),
    Game(GameAction),
    // ---- application / menus
    Quit,
    /// Main menu "Tutorial" (stock v20 loads Map_Tutorial, absent here).
    StartTutorial,
    /// Refresh local host colorsets when opening Start Game.
    RefreshHostColorsets,
    /// Open the user's colorsets folder, then refresh the local catalog.
    ColorsetsFolder,
    HostGame {
        map: String,
        mode: ServerMode,
        /// A game mode an enabled Add-On declares (content id), or None for
        /// Custom: every enabled Add-On that fits the map.
        game_mode: Option<String>,
        max_players: u32,
        server_name: String,
        password: String,
        admin_password: String,
        super_admin_password: String,
    },
    /// Search the LAN and check the servers the player joined or starred.
    QueryLan,
    /// Star or unstar a server in the join list.
    ToggleFavorite {
        address: String,
    },
    /// Let the game through Windows Firewall (one Windows permission prompt).
    AllowFirewall {
        port: u16,
    },
    JoinServer {
        address: String,
        password: String,
    },
    /// Join a saved server whose identity changed, trusting its new one
    /// (the player chose Continue).
    TrustNewServerIdentity {
        address: String,
    },
    /// Run the Add-On code the join's trust question showed ("Trust and
    /// join").
    TrustAddOnCode,
    /// Stop trusting every server's Add-On code (Add-Ons screen).
    ForgetAddOnTrust,
    /// Cancel a pending connection attempt or leave the loading screen.
    CancelConnect,
    /// Leave the game (disconnect, or stop hosting).
    Disconnect,
    /// Persist settings (prefs, binds, favorites, avatar). Sent whenever the
    /// user commits a change; the host owns the storage format/location.
    SaveSettings(Box<Settings>),
    /// Apply display settings now (Options → Graphics → APPLY).
    ApplyDisplay {
        resolution: (u32, u32),
        fullscreen: bool,
        vsync: bool,
    },
    SetVolume {
        channel: String,
        value: f32,
    },
    /// Open a web page in the player's browser (a new release's page).
    OpenUrl(String),
    /// Show the saves folder, where old `.bls` saves can be dropped, in
    /// the file browser.
    OpenSavesFolder,
    /// Show the Add-Ons folder, where classic Blockland Add-On zips can be
    /// dropped, in the file browser.
    OpenAddOnsFolder,
    // ---- in game
    Chat {
        channel: ChatChannel,
        text: String,
    },
    /// `/cmd a b` typed in chat.
    ChatCommand {
        name: String,
        args: Vec<String>,
    },
    StartTyping,
    /// `SteeringPrefsEvent`: `$pref::Input::UseStrafeSteering` and
    /// `$pref::Input::UseAutoReturnSteering`, sent on joining and on change.
    SteeringPrefs {
        strafe: bool,
        auto_return: bool,
    },
    StopTyping,
    /// Brick selector DONE: buy all ten slots (brick ids, `None` = empty).
    BuyBricks {
        slots: Vec<Option<String>>,
    },
    /// Right-click in the selector: use this brick without touching the cart.
    InstantUseBrick {
        brick: String,
    },
    /// Select a brick slot (0..=9) in the hand.
    UseBrickSlot {
        slot: usize,
    },
    UseTool {
        slot: usize,
    },
    /// Put away the current tool/spray can (`unUseTool`).
    UnUseTool,
    /// Paint colour index into the flattened colorset.
    UseSprayCan {
        color: u32,
    },
    /// FX can 0..=8 (none, pearl, chrome, glow, blink, swirl, rainbow, stable, undulo).
    UseFxCan {
        fx: u32,
    },
    SetPrint {
        print: String,
    },
    ClosePrintSelector,
    SendWrench {
        brick: u64,
        variant: WrenchVariant,
        data: WrenchData,
    },
    /// The fill wrench's settings, with `fields` the ones ticked to put on
    /// every brick.
    SendFillWrench {
        data: WrenchData,
        fields: Vec<crate::models::wrench::WrenchField>,
    },
    /// Vehicle spawn wrench `< Respawn >`.
    RespawnVehicle {
        brick: u64,
        vehicle: Option<String>,
    },
    RequestEvents {
        brick: u64,
    },
    SendEvents {
        brick: u64,
        rows: Vec<EventRow>,
    },
    CancelWrench {
        brick: u64,
    },
    /// Render a local preview; never persist or publish the avatar.
    PreviewAvatar {
        avatar: AvatarPrefs,
        camera_rotation: [f32; 3],
        orbit_distance: f32,
    },
    SetAvatar(AvatarPrefs),
    SaveBricks {
        name: String,
        description: String,
        events: bool,
        ownership: bool,
        overwrite: bool,
    },
    LoadBricks {
        map: String,
        name: String,
        ownership: bool,
    },
    /// The choice in `LoadBricksColorGui` for the load waiting on it.
    LoadBricksColors(ColorLoad),
    RequestSaveList {
        map: Option<String>,
    },
    /// Load Bricks picked this save: send its picture, if it has one, as
    /// [`UiUpdate::SavePreview`].
    PreviewSave {
        map: String,
        name: String,
    },
    OpenAdmin,
    /// A console statement for a command the host registered with
    /// `Ui::set_console_commands`. Output goes to `bri_console`'s log.
    Console {
        line: String,
    },
    // ---- vanilla mini-games. The host resolves the caller from the authenticated session.
    RequestMiniGameList,
    CreateMiniGame {
        color: u8,
        rules: MiniGameRules,
    },
    ConfigureMiniGame {
        game: MiniGameId,
        rules: MiniGameRules,
    },
    JoinMiniGame {
        game: MiniGameId,
    },
    LeaveMiniGame {
        game: MiniGameId,
    },
    InviteMiniGame {
        target: MiniGamePlayerId,
    },
    AcceptMiniGameInvite {
        game: MiniGameId,
    },
    RejectMiniGameInvite {
        game: MiniGameId,
        ignore_owner: bool,
    },
    RemoveMiniGameMember {
        target: MiniGamePlayerId,
        /// Another game its editor manages (not their own).
        #[serde(default)]
        game: Option<MiniGameId>,
    },
    /// Put a player on a team of a game (`None`: on none), bringing them
    /// into it: the Add-On Settings window's team members.
    SetMiniGameTeam {
        game: MiniGameId,
        target: MiniGamePlayerId,
        team: Option<u32>,
    },
    /// `commandToServer('Trust_Invite')`: level 1 build, 2 full.
    TrustInvite {
        target: u64,
        level: u8,
    },
    /// `commandToServer('Trust_Demote')`: level 0 none, 1 build.
    TrustDemote {
        target: u64,
        level: u8,
    },
    /// `commandToServer('UnIgnore')`.
    UnIgnore {
        target: u64,
    },
    /// Trust invitation dialog answer.
    AnswerTrustInvite {
        from: u64,
        answer: TrustAnswer,
    },
    ResetMiniGame {
        game: MiniGameId,
    },
    RespawnMiniGameMembers {
        game: MiniGameId,
    },
    EndMiniGame {
        game: MiniGameId,
    },
    /// The Add-On Settings window's Apply: changed settings (`None` back to
    /// the default) and, when the game's teams were edited, its whole team
    /// list.
    EditMiniGameAddOns {
        game: MiniGameId,
        settings: Vec<(String, Option<MiniGameSettingValue>)>,
        teams: Option<Vec<MiniGameTeamEdit>>,
        /// Do not tell the game's players what changed.
        quiet: bool,
        /// Reset the game with the change (Apply & Reset).
        reset: bool,
    },
    // ---- add-ons (the package library; see docs/architecture/mod-manager.md)
    /// Read the installed packages; answered with [`UiUpdate::AddOns`].
    RequestAddOns,
    /// Turn a package on or off. The host also turns on what it needs, or
    /// off what needs it, and answers with the new [`UiUpdate::AddOns`].
    /// It only writes the lists, so it answers at once; loading what they
    /// name waits for [`UiAction::ApplyAddOns`].
    SetAddOnEnabled {
        id: String,
        enabled: bool,
    },
    /// Load the Add-Ons the lists now name (bricks, weapons, maps, game
    /// modes), if they changed. The Add-Ons screen sends it as it closes,
    /// so a run of clicks costs one load.
    ApplyAddOns,
    /// Turn off every package that is not part of the base game.
    DefaultAddOns,
    /// Convert again a classic Add-On in the Add-Ons folder that could not
    /// be converted (a row with `importable`). Answered once it is under way;
    /// [`UiUpdate::AddOns`] follows as it goes.
    ImportAddOn {
        id: String,
    },
}

// -------------------------------------------------------------- view models

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConnectionState {
    /// At the menus.
    Idle,
    /// Waiting for the server ("Connecting to Local Host…").
    Connecting { text: String },
    /// Loading a mission: what is happening, in v20's upper-case
    /// `LoadingProgressTxt` style (`bri_progress::Snapshot::status`), and the
    /// current stage's progress 0..1.
    Loading {
        map: String,
        preview: IconRef,
        status: String,
        progress: f32,
    },
    InGame {
        server_name: String,
        max_players: u32,
        local: bool,
        single_player: bool,
        admin: bool,
    },
    /// Fetching the packages a server needs before joining it.
    DownloadingPackages(PackageDownload),
    /// Connection failed or was dropped; shown in a message box.
    Failed { reason: String },
}

/// A question with two named answers (Continue/Cancel, Download/Leave).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub title: String,
    pub text: String,
    pub yes: String,
    pub no: String,
    pub on_yes: Box<UiAction>,
    /// `None`: NO only closes the question.
    pub on_no: Option<Box<UiAction>>,
}

/// A join refused because this player's add-ons differ from the server's
/// shared ones, as the Can't Join dialog lists them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddOnMismatch {
    pub rows: Vec<MismatchRow>,
    /// What differs, in plain words, when the version numbers alone do not
    /// say (the same version with different files). Empty otherwise.
    #[serde(default)]
    pub explanation: String,
    /// Some row is part of the base game rather than an Add-On.
    #[serde(default)]
    pub base_game: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MismatchRow {
    pub name: String,
    /// The server's version, or empty when it does not use it.
    pub server: String,
    /// This player's version, or empty when they do not have it on.
    pub yours: String,
}

/// Join-time package download, as the join screen shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageDownload {
    pub server: String,
    pub packages: Vec<DownloadRow>,
    /// Bytes fetched and to fetch over every package.
    pub done_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadRow {
    pub name: String,
    pub version: String,
    pub bytes: u64,
    pub state: DownloadState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    /// Already in the download cache; nothing to fetch.
    Cached,
    Waiting,
    Downloading,
    Done,
}

/// One package as the Add-Ons screen shows it. Everything is display text
/// the host prepared; the screen does not interpret package data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddOnRow {
    /// Package id, sent back in [`UiAction::SetAddOnEnabled`].
    pub id: String,
    pub name: String,
    pub version: String,
    /// Group heading ("Game Modes", "Weapons & Items", ...).
    pub category: String,
    pub enabled: bool,
    /// Base game: shown on and cannot be turned off.
    pub locked: bool,
    /// Where it runs, in words ("Server only: players never download it").
    pub runs: String,
    pub description: String,
    pub authors: String,
    pub license: String,
    pub source: String,
    /// "3 weapons", "a world", ...
    pub provides: Vec<String>,
    /// Package names this one needs.
    pub needs: Vec<String>,
    /// Names of enabled packages that need this one; turning it off turns
    /// them off too, so the screen asks first.
    pub needed_by: Vec<String>,
    /// What the package is allowed to do, in words.
    pub allowed: Vec<String>,
    /// Problems in words, worst first.
    pub problems: Vec<String>,
    /// A problem stops it from loading.
    pub broken: bool,
    /// An old Blockland add-on not converted yet: the screen offers Import
    /// instead of Enabled.
    pub importable: bool,
    /// Being imported right now.
    pub importing: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddOnsView {
    pub rows: Vec<AddOnRow>,
    /// Result of the last change ("Also turned on: ...") or a list problem.
    pub notice: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlantError {
    Overlap,
    Float,
    Stuck,
    Unstable,
    Buried,
    Forbidden,
    TooFar,
    Limit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerRow {
    pub id: u64,
    /// This viewer ignores the player's trust invites.
    pub ignoring: bool,
    pub name: String,
    pub score: i32,
    pub admin: bool,
    pub super_admin: bool,
    pub bl_id: Option<u64>,
    /// Trust text as shown (`You`, `Full`, `Build`, `None`, or an ownership
    /// label chosen by the host).
    pub trust: String,
}

/// Session-scoped minigame identities supplied by the authenticated host.
/// These are selectors only: UI requests never carry an acting identity or role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MiniGamePlayerId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MiniGameId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameChoice {
    pub id: String,
    pub name: String,
}

/// A setting (`namespace:key`) that shows another only while it holds one
/// of `is`, or, when `is_not` lists values, none of those.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameShownWhen {
    pub setting: String,
    #[serde(default)]
    pub is: Vec<MiniGameSettingValue>,
    #[serde(default)]
    pub is_not: Vec<MiniGameSettingValue>,
}
impl MiniGameShownWhen {
    /// Whether that setting holding `value` shows the other.
    pub fn holds(&self, value: &MiniGameSettingValue) -> bool {
        if self.is_not.is_empty() {
            self.is.contains(value)
        } else {
            !self.is_not.contains(value)
        }
    }
}

/// Stock v20 Create/Edit Mini-Game fields; lives are unlimited in v20 and have
/// no UI field. Time fields are seconds here and are converted at the host edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameRules {
    pub title: String,
    pub invite_only: bool,
    pub use_all_players_bricks: bool,
    pub players_use_own_bricks: bool,
    pub use_spawn_bricks: bool,
    pub points_break_brick: i32,
    pub points_plant_brick: i32,
    pub points_kill_player: i32,
    pub points_kill_self: i32,
    pub points_die: i32,
    pub respawn_seconds: u32,
    pub vehicle_respawn_seconds: u32,
    pub brick_respawn_seconds: u32,
    pub falling_damage: bool,
    pub weapon_damage: bool,
    pub self_damage: bool,
    pub vehicle_damage: bool,
    pub brick_damage: bool,
    pub enable_wand: bool,
    pub enable_building: bool,
    pub enable_painting: bool,
    pub player_type: String,
    pub loadout: [Option<String>; 5],
}
impl Default for MiniGameRules {
    fn default() -> Self {
        Self {
            title: "Default Mini-Game".into(),
            invite_only: false,
            use_all_players_bricks: false,
            players_use_own_bricks: false,
            use_spawn_bricks: true,
            points_break_brick: 0,
            points_plant_brick: 0,
            points_kill_player: 1,
            points_kill_self: -1,
            points_die: 0,
            respawn_seconds: 1,
            vehicle_respawn_seconds: 5,
            brick_respawn_seconds: 30,
            falling_damage: true,
            weapon_damage: true,
            self_damage: true,
            vehicle_damage: true,
            brick_damage: true,
            enable_wand: false,
            enable_building: true,
            enable_painting: true,
            player_type: "v20.player.playerstandardarmor".into(),
            loadout: [
                "v20.weapon.hammeritem",
                "v20.weapon.wrenchitem",
                "v20.weapon.printgun",
                "v20.weapon.gunitem",
                "v20.weapon.rocketlauncheritem",
            ]
            .map(|s| Some(s.into())),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameSummary {
    pub id: MiniGameId,
    pub title: String,
    pub owner: MiniGamePlayerId,
    pub owner_name: String,
    pub color: u8,
    pub member_count: u32,
    pub invite_only: bool,
    pub rules: MiniGameRules,
    /// Its teams (an Add-On's, such as Slayer's).
    #[serde(default)]
    pub teams: Vec<MiniGameTeam>,
    /// Add-On settings changed from their defaults, by `namespace:key`.
    #[serde(default)]
    pub addon_settings: BTreeMap<String, MiniGameSettingValue>,
    /// The server's default game, which players in none join.
    #[serde(default)]
    pub default: bool,
    /// The paint colour (by palette index) the host's rules gave it in
    /// place of `color`.
    #[serde(default)]
    pub paint_color: Option<u8>,
    /// Its members and the team each plays for.
    #[serde(default)]
    pub members: Vec<MiniGameTeamMember>,
}
/// A mini-game member and their team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameTeamMember {
    pub id: MiniGamePlayerId,
    pub name: String,
    pub team: Option<u32>,
}
/// An Add-On setting's value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MiniGameSettingValue {
    Bool(bool),
    Int(i64),
    Text(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MiniGameSettingKind {
    Bool,
    Int {
        min: i64,
        max: i64,
    },
    /// Choices: value and name.
    List {
        items: Vec<(MiniGameSettingValue, String)>,
    },
    /// Same picker as List, retaining the authored gameplay purpose for
    /// presenting starting equipment before less common team settings.
    Item {
        items: Vec<(MiniGameSettingValue, String)>,
    },
    PlayerType {
        items: Vec<(MiniGameSettingValue, String)>,
    },
    Text {
        max_length: u32,
    },
    /// A paint colour from `min` to `max` (-1: none), shown with its
    /// swatch.
    PaintColor {
        min: i64,
        max: i64,
    },
}
/// One setting an Add-On declares, for the Add-On Settings window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameAddOnSetting {
    /// `namespace:key`.
    pub key: String,
    /// The Add-On's name, heading its settings.
    pub add_on: String,
    pub category: String,
    pub title: String,
    /// Each team has its own value.
    pub team: bool,
    /// One value for the whole server, which only the host changes (the
    /// Admin menu's Add-On Settings).
    #[serde(default)]
    pub server: bool,
    /// A server setting the game reads only as it starts or loads a map:
    /// a change waits for the next start.
    #[serde(default)]
    pub restart: bool,
    pub kind: MiniGameSettingKind,
    pub default: MiniGameSettingValue,
    /// Shown only while that setting (`namespace:key`) holds one of some
    /// values, or none of them.
    pub shown_when: Option<MiniGameShownWhen>,
    /// What the '?' beside it explains.
    #[serde(default)]
    pub help: String,
    /// The part of a look it holds (`Hat`, `HatColor`, `FaceName`, ...):
    /// edited with the others of its category in the avatar editor.
    #[serde(default)]
    pub avatar: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameTeam {
    pub id: u32,
    pub name: String,
    /// A paint colour index.
    pub color: u8,
    pub settings: BTreeMap<String, MiniGameSettingValue>,
}
/// A team as the Add-On Settings window leaves it: `id` keeps an existing
/// team (and its players), none makes a new one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameTeamEdit {
    pub id: Option<u32>,
    pub name: String,
    pub color: u8,
    pub settings: Vec<(String, Option<MiniGameSettingValue>)>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameColor {
    pub index: u8,
    pub name: String,
    pub rgb: [u8; 3],
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameMemberRow {
    pub id: MiniGamePlayerId,
    pub name: String,
    pub score: i64,
    pub is_owner: bool,
    pub admin: bool,
    pub in_local_game: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NameTag {
    /// Anchor in logical pixels: the name is centered above it.
    pub x: f32,
    pub y: f32,
    pub text: String,
    /// Distance fade, 0..=1.
    pub opacity: f32,
    /// `ShapeBase::setShapeNameColor`: white, or the player's mini-game colour.
    #[serde(default = "white_name")]
    pub color: [u8; 3],
}
fn white_name() -> [u8; 3] {
    [255; 3]
}
/// The outline `GuiShapeNameHud::drawName` (blocklandv20.exe 0x527630) draws
/// under a name: white under a dark name (red and green both below 0.3),
/// otherwise black.
pub fn name_outline(color: [u8; 3]) -> [u8; 3] {
    if f32::from(color[0]) / 255.0 < 0.3 && f32::from(color[1]) / 255.0 < 0.3 {
        [255; 3]
    } else {
        [0; 3]
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustInvitation {
    pub from: u64,
    pub name: String,
    pub bl_id: String,
    /// 1 build, 2 full.
    pub level: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustAnswer {
    Accept,
    Reject,
    Ignore,
}
/// A score report the host showed (Slayer's End of Round Report): plain
/// text, laid out by the Report window.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReportView {
    pub title: String,
    /// Large text over the table ("VICTORY").
    pub banner: Option<String>,
    /// Column titles after the name column.
    pub columns: Vec<String>,
    pub sections: Vec<ReportSectionView>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReportSectionView {
    pub title: String,
    pub rows: Vec<ReportRowView>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReportRowView {
    pub name: String,
    /// The name's colour (a team's paint), else the window's text colour.
    pub color: Option<Rgba>,
    /// One per column, blank where the row has none.
    pub cells: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameInvitation {
    pub game: MiniGameId,
    pub title: String,
    pub owner: MiniGamePlayerId,
    pub owner_name: String,
    pub owner_display_id: String,
}
/// Capabilities come from the session's host adapter. False is fail-closed;
/// a local admin flag never grants any minigame permission.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameCapabilities {
    pub list: bool,
    pub create: bool,
    pub configure: bool,
    pub join: bool,
    pub leave: bool,
    pub invite: bool,
    pub respond_invite: bool,
    pub remove_member: bool,
    pub reset: bool,
    pub respawn_all: bool,
    pub end: bool,
    pub scoreboard: bool,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameUiState {
    pub ready: bool,
    pub revision: u64,
    pub capabilities: MiniGameCapabilities,
    pub games: Vec<MiniGameSummary>,
    pub colors: Vec<MiniGameColor>,
    pub active_game: Option<MiniGameId>,
    /// Explicit authoritative ownership result; never derive from admin status.
    pub owns_active_game: bool,
    pub local_player: Option<MiniGamePlayerId>,
    pub members: Vec<MiniGameMemberRow>,
    pub invitations: Vec<MiniGameInvitation>,
    pub player_types: Vec<MiniGameChoice>,
    pub items: Vec<MiniGameChoice>,
    pub status: String,
    /// The running Add-Ons' settings (the Add-On Settings window).
    #[serde(default)]
    pub addon_settings: Vec<MiniGameAddOnSetting>,
    /// The games the local player may manage (the Add-On Settings window,
    /// rules, Reset, End, invites and removals) though they may not own
    /// them: the host's answer (its owner, or an admin), by game. The host
    /// checks every request again.
    #[serde(default)]
    pub addon_editable: Vec<MiniGameId>,
    /// The paint colours a team may take, by index.
    #[serde(default)]
    pub palette: Vec<[u8; 3]>,
    /// Settings (`namespace:key`) of each game the local player may edit
    /// but whose own level they lack (admin, super admin, host or the
    /// creator's trust): shown greyed.
    #[serde(default)]
    pub addon_locked: Vec<(MiniGameId, Vec<String>)>,
    /// The team list shows only while that setting holds one of these
    /// (Slayer's teams, hidden in a mode without them).
    #[serde(default)]
    pub teams_shown_when: Option<MiniGameShownWhen>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MiniGameOperation {
    List,
    Create,
    Configure,
    Join,
    Leave,
    Invite,
    AcceptInvite,
    RejectInvite,
    IgnoreInvite,
    RemoveMember,
    Reset,
    RespawnAll,
    End,
    AddOnSettings,
}

/// Fullscreen is borderless at the monitor's `native` size; a window may take
/// any `windowed` size, each smaller than the desktop as v20's list was.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayModes {
    pub native: (u32, u32),
    pub windowed: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiUpdate {
    /// Show a splash over the main menu.
    Splash(SplashView),
    Admin(crate::models::admin::AdminUpdate),
    /// The host's environment over the map's own (the Environment window).
    Environment(crate::models::environment::EnvironmentView),
    /// Answer to a request (`Err` carries the user-visible reason).
    ActionResult {
        id: RequestId,
        result: Result<(), String>,
    },
    Connection(ConnectionState),
    Maps(Vec<MapInfo>),
    /// Game modes the enabled Add-Ons declare (Start Game).
    GameModes(Vec<GameModeInfo>),
    HostColorsets(Vec<HostColorset>),
    LanServers {
        servers: Vec<ServerInfo>,
        querying: bool,
    },
    MainMenuBackgrounds(Vec<IconRef>),
    /// The window's monitor: what Options may offer (platform to UI).
    DisplayModes(DisplayModes),
    /// The platform changed the display itself (Alt+Enter, or a saved mode
    /// the monitor cannot show); the UI records it in the video prefs.
    DisplayChanged {
        resolution: (u32, u32),
        fullscreen: bool,
    },
    // ---- server content (sent after mission download)
    Bricks(Vec<BrickInfo>),
    Colorset(Vec<PaintDivision>),
    Prints {
        aspect: String,
        prints: Vec<PrintInfo>,
    },
    Events(EventCatalog),
    Datablocks(DatablockMenus),
    // ---- live game state
    BuildingAllowed(bool),
    /// Authoritative brick inventory (brick ids per slot).
    BrickInventory(Vec<Option<String>>),
    /// Authoritative tool slots.
    Tools(Vec<Option<ToolInfo>>),
    /// Authoritative selection (no outgoing request). Inventory must arrive
    /// first; empty/out-of-range selections safely clear selection.
    SetActiveTool(Option<usize>),
    SetActiveBrick(Option<usize>),
    /// `clientCmdSetScrollMode`: the host switches the inventory box shown.
    ScrollMode(crate::models::hud::ScrollMode),
    /// The tool in hand takes the paint cans, or stays out for them (a
    /// paint picker, as the Fill Can), so opening paint from it keeps it in
    /// hand.
    ToolTakesPaint(bool),
    /// First spawn of the session: the UI buys favorites slot 1.
    FirstSpawn,
    Chat {
        text: String,
    },
    /// Names typing in the chat box, in the order they started (`WhoTalkSO`).
    Talking(Vec<String>),
    CenterPrint {
        text: String,
        seconds: f32,
    },
    BottomPrint {
        text: String,
        seconds: f32,
        hide_bar: bool,
    },
    ClearPrints,
    PlantError(PlantError),
    /// A sound caption ("[Explosion]"), shown while captions are on.
    Caption(String),
    /// Red damage flash (`Armor::onDamage`: +delta/maxDamage*2, capped 0.75).
    DamageFlash(f32),
    /// Jet energy fraction for `HUD_EnergyBar`; `None` hides it
    /// (`clientCmdShowEnergyBar`, from the datablock's `showEnergyBar`).
    Energy(Option<f32>),
    /// White screen (`setWhiteout`) that fades over a second per unit.
    Whiteout(f32),
    /// The camera's liquid tints (`GameRenderFilters`): a water brick zone's
    /// colour and/or map water's, alpha already clamped.
    Underwater(Vec<[f32; 4]>),
    /// Whether the camera is a first-person eye; the crosshair shows only
    /// then (`GuiCrossHairHud` checks `isFirstPerson`).
    FirstPerson(bool),
    /// The held weapon hides the crosshair (its own, or its scope while
    /// aiming, draws the aim instead).
    HideCrosshair(bool),
    /// The held tool's image has a `wheel` command: while the trigger
    /// (`mouseFire`) is held down here, the mouse wheel goes to that tool
    /// and nothing else sees it. The UI tracks the trigger itself, so a
    /// press and a roll in the same frame already reach the tool.
    ToolWheel(bool),
    /// An Add-On's orbit camera zooms: the wheel goes to it
    /// ([`GameAction::CameraZoom`]) instead of the inventory.
    CameraWheel(bool),
    /// Aimed through a scope with steps (`Zoom::levels`): the mouse wheel
    /// zooms instead of scrolling the inventory, with no trigger held.
    AimWheel(bool),
    /// A scope's picture over the whole screen while aiming
    /// (`Zoom::overlay`): the host's texture key and its width over its
    /// height; `None` takes it away.
    ScopeOverlay(Option<(u64, f32)>),
    /// A net graph sample (`NetGraph::updateStats`); dropped while hidden.
    NetSample(crate::models::perf::NetSample),
    /// A presented frame's timing; dropped while the overlay is hidden.
    PerfFrame(crate::models::perf::FrameSample),
    /// The performance overlay's slower figures.
    PerfStats(crate::models::perf::PerfStats),
    Players {
        rows: Vec<PlayerRow>,
        server_name: String,
        max_players: u32,
    },
    MiniGames(MiniGameUiState),
    MiniGameInvite(MiniGameInvitation),
    /// Open the Report window on this report, or close it.
    Report(Option<ReportView>),
    /// Server `MessageBoxOK`.
    MessageBox {
        title: String,
        text: String,
    },
    /// A yes/no question from the platform; YES sends `action`.
    Confirm {
        title: String,
        text: String,
        action: Box<UiAction>,
    },
    /// `clientCmdTrustInvite`.
    TrustInvite(TrustInvitation),
    Lagging(bool),
    /// A duplicator opened the fill wrench on `bricks` bricks.
    OpenFillWrench {
        bricks: u32,
    },
    /// Open the wrench for a brick the server says we may edit.
    OpenWrench {
        brick: u64,
        variant: WrenchVariant,
        /// Heading: owner name on LAN.
        owner: String,
        data: WrenchData,
        admin_override: bool,
        events_allowed: bool,
    },
    /// Rows for the events dialog of `brick`.
    OpenEvents {
        brick: u64,
        /// The brick's builder (owner id), whose mini-game its rows run in;
        /// `None` takes the local player's.
        builder: Option<u64>,
        rows: Vec<EventRow>,
        named_targets: Vec<String>,
        allow_named: bool,
    },
    /// A named target used by the open dialog was removed.
    NamedTargetsInvalidated,
    OpenPrintSelector {
        aspect: String,
        current: Option<String>,
    },
    /// A save's colours differ from the world's: ask how to load them
    /// (`LoadBricksColorGui`). `append` is whether they fit added on.
    ColorWarning {
        append: bool,
    },
    SaveFiles {
        maps: Vec<String>,
        files: Vec<SaveFileInfo>,
    },
    /// Map preview / save info for the save dialog.
    SaveContext {
        map: String,
        preview: IconRef,
    },
    /// The picture of the save [`UiAction::PreviewSave`] asked for;
    /// [`IconRef::None`] when it has none.
    SavePreview {
        map: String,
        name: String,
        preview: IconRef,
    },
    /// Avatar preview texture for the Player Appearance screen.
    AvatarPreview(IconRef),
    /// The installed packages, for the Add-Ons screen.
    AddOns(AddOnsView),
    /// The next connection failure is a refused join over differing
    /// add-ons: show these instead of a plain message box.
    AddOnMismatch(AddOnMismatch),
    /// Ask now; each answer sends its request.
    Question(Question),
    /// The next connection failure asks this instead of showing its reason.
    FailureQuestion(Question),
    /// The hosted world changed since it was last saved under a name (or
    /// loaded); leaving and quitting ask first.
    UnsavedChanges(bool),
    /// This build's version, shown on the main menu.
    Version(String),
    /// A newer release exists: say so once and offer its page.
    NewerVersion {
        name: String,
        url: String,
    },
    /// The host chose preferences for the player (the first run's graphics
    /// quality); they are saved like the player's own.
    SetPrefs(Vec<(String, String)>),
}

// ----------------------------------------------------------------- settings

/// Everything the UI persists through the host (`UiAction::SaveSettings`).
/// Missing fields take their defaults, so adding a field never makes an
/// older settings file unreadable.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// `$pref::`-style values by their original names (stock defaults from
    /// the UI pack are used for missing keys).
    pub prefs: BTreeMap<String, String>,
    /// Current key binds (command name → chords). `None` = never configured
    /// (first run shows Default Controls).
    pub binds: Option<Vec<BindEntry>>,
    /// Hardware choice from Default Controls.
    pub mouse_type: u8,
    pub keyboard_type: u8,
    /// Brick favorites by slot 0..=9, uiNames (v20 `Favorites.cs`).
    pub brick_favorites: BTreeMap<u8, Vec<String>>,
    pub avatar: AvatarPrefs,
    pub avatar_favorites: BTreeMap<u8, AvatarPrefs>,
    pub avatar_colors: Vec<[f32; 4]>,
    /// Create Mini-Game favourites by slot 0..=9 (v20
    /// `config/client/MiniGameFavorites/<slot>.cs`).
    pub minigame_favorites: BTreeMap<u8, MiniGameFavorite>,
    /// Add-On Settings favourites by slot 0..=9 (Slayer's
    /// `config/client/Slayer/config_saved`): every setting and the teams.
    pub addon_favorites: BTreeMap<u8, AddOnFavorite>,
}

/// One Add-On Settings favourite: the settings by `namespace:key` and the
/// team list with each team's settings.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AddOnFavorite {
    /// The game's vanilla rules, put back with the Add-On settings (none in
    /// a favourite saved before they were kept).
    pub rules: Option<MiniGameRules>,
    pub settings: BTreeMap<String, MiniGameSettingValue>,
    pub teams: Vec<AddOnFavoriteTeam>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AddOnFavoriteTeam {
    pub name: String,
    pub color: u8,
    pub settings: BTreeMap<String, MiniGameSettingValue>,
}

/// One Create Mini-Game favourite: the form's rules and its colour's name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MiniGameFavorite {
    pub rules: MiniGameRules,
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindEntry {
    pub command: String,
    pub input: BindInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BindInput {
    Key(Chord),
    Mouse(MouseButton),
    Wheel,
    MouseX,
    MouseY,
}

impl BindInput {
    pub fn parse(device: crate::schema::Device, key: &str) -> Option<BindInput> {
        match device {
            crate::schema::Device::Keyboard => Chord::parse(key).map(BindInput::Key),
            crate::schema::Device::Mouse => match key.to_ascii_lowercase().as_str() {
                "button0" => Some(BindInput::Mouse(MouseButton::Left)),
                "button1" => Some(BindInput::Mouse(MouseButton::Right)),
                "button2" => Some(BindInput::Mouse(MouseButton::Middle)),
                "button3" => Some(BindInput::Mouse(MouseButton::Back)),
                "button4" => Some(BindInput::Mouse(MouseButton::Forward)),
                "zaxis" => Some(BindInput::Wheel),
                "xaxis" => Some(BindInput::MouseX),
                "yaxis" => Some(BindInput::MouseY),
                _ => None,
            },
        }
    }
    pub fn label(&self) -> String {
        match self {
            BindInput::Key(c) => c.label(),
            BindInput::Mouse(MouseButton::Left) => "Left Mouse".into(),
            BindInput::Mouse(MouseButton::Right) => "Right Mouse".into(),
            BindInput::Mouse(MouseButton::Middle) => "Middle Mouse".into(),
            BindInput::Mouse(MouseButton::Back) => "Mouse 4".into(),
            BindInput::Mouse(MouseButton::Forward) => "Mouse 5".into(),
            BindInput::Wheel => "Mouse Wheel".into(),
            BindInput::MouseX => "Mouse X".into(),
            BindInput::MouseY => "Mouse Y".into(),
        }
    }
}
