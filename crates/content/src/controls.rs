//! Control profiles: which bindings and mouse settings a kind of player uses.
//!
//! An Add-On's `binds.json` declares *actions* (named commands with a suggested
//! default key or mouse input) and may declare *profiles* that hold them. The
//! engine owns the mechanism (switching, layering, saving player overrides);
//! the Add-On owns the policy (which player types use which profile, which
//! vehicle contexts change the mouse).

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// The profile every player has, holding the stock Blockland controls and
/// every action that names no profile.
pub const DEFAULT_PROFILE: &str = "blockland";

/// Most profiles one file may declare, and layers per profile.
pub const MAX_PROFILES: usize = 8;
pub const MAX_LAYERS: usize = 8;

/// When a layer is on: from what the local player is riding, or from a
/// rule's flag on the player.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LayerWhen {
    /// Seated in any vehicle.
    Vehicle,
    /// Driving any vehicle.
    Driver,
    /// Driving a vehicle the mouse steers (a plane, a carpet).
    MouseDriver,
    /// Seated in a vehicle without driving it.
    Passenger,
    /// While a rule's player value is on (`"rule:package:player/key"`, a
    /// HUD `bind`'s form): a menu a rule has open, say. A number other than
    /// 0, `true` or a non-empty string is on.
    Rule(String),
}

impl LayerWhen {
    fn parse(text: &str) -> Result<Self, String> {
        Ok(match text {
            "vehicle" => Self::Vehicle,
            "driver" => Self::Driver,
            "mouse_driver" => Self::MouseDriver,
            "passenger" => Self::Passenger,
            other => match other.strip_prefix("rule:") {
                Some(key) if rule_key_ok(key) => Self::Rule(key.to_owned()),
                Some(_) => {
                    return Err(format!(
                        "`{other}` is not a rule flag: rule:package:player/key"
                    ));
                }
                None => {
                    return Err(format!(
                        "`{other}` is not a layer condition: vehicle, driver, mouse_driver, passenger or rule:package:player/key"
                    ));
                }
            },
        })
    }
}
impl std::fmt::Display for LayerWhen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Vehicle => f.write_str("vehicle"),
            Self::Driver => f.write_str("driver"),
            Self::MouseDriver => f.write_str("mouse_driver"),
            Self::Passenger => f.write_str("passenger"),
            Self::Rule(key) => write!(f, "rule:{key}"),
        }
    }
}
impl Serialize for LayerWhen {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for LayerWhen {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}
/// `package:player/key`, as a HUD `bind` writes the viewer's own value.
fn rule_key_ok(key: &str) -> bool {
    key.len() <= 128
        && key
            .split_once(":player/")
            .is_some_and(|(package, name)| !package.is_empty() && !name.is_empty())
        && key.chars().all(|c| c.is_ascii_graphic())
}

/// A context stacked on a profile while it holds, removed on exit. The
/// declared values replace the profile's own for as long as it is on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerDef {
    pub id: String,
    /// The Controls screen's name for the context ("In aircraft"); the id
    /// when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub when: LayerWhen,
    /// Invert the mouse's pitch while the layer is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invert_mouse: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mouse_sensitivity: Option<f32>,
}

/// A named set of bindings. The profile becomes the active one while the
/// local player's body is one of its `player_types`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileDef {
    /// Lower-case letters, digits, `_` and `-`.
    pub id: String,
    /// The Controls tab's title.
    pub name: String,
    /// Inputs this profile leaves unbound fall through to the Blockland
    /// profile (movement, chat, menus), so a profile lists only its own
    /// actions. Turn off for a profile that replaces every input.
    #[serde(default = "yes")]
    pub inherit: bool,
    /// Player type (archetype) ids that use this profile.
    #[serde(default)]
    pub player_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<LayerDef>,
    /// The profile's default for Invert Mouse on foot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invert_mouse: Option<bool>,
    /// The profile's default for Invert Mouse in vehicles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vehicle_invert_mouse: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mouse_sensitivity: Option<f32>,
}
fn yes() -> bool {
    true
}

fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Mouse inputs a bind may name as its default.
pub const MOUSE_INPUTS: [&str; 6] = [
    "button0", "button1", "button2", "button3", "button4", "zaxis",
];

impl LayerDef {
    /// What the Controls screen calls the layer.
    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}

impl ProfileDef {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            id_ok(&self.id) && self.id != DEFAULT_PROFILE,
            "profile id `{}` must be 1 to 32 of a-z, 0-9, `_`, `-` and not `{DEFAULT_PROFILE}`",
            self.id
        );
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 24,
            "profile `{}` needs a tab name of 1 to 24 characters",
            self.id
        );
        ensure!(
            self.player_types.len() <= 32 && self.player_types.iter().all(|t| !t.is_empty()),
            "profile `{}` lists up to 32 player types",
            self.id
        );
        ensure!(
            self.layers.len() <= MAX_LAYERS,
            "profile `{}` has up to {MAX_LAYERS} layers",
            self.id
        );
        for (i, l) in self.layers.iter().enumerate() {
            ensure!(
                id_ok(&l.id) && !self.layers[..i].iter().any(|o| o.id == l.id),
                "profile `{}` layer id `{}` must be unique, a-z, 0-9, `_`, `-`",
                self.id,
                l.id
            );
            ensure!(
                l.name
                    .as_ref()
                    .is_none_or(|n| !n.trim().is_empty() && n.len() <= 16),
                "profile `{}` layer `{}` needs a name of 1 to 16 characters",
                self.id,
                l.id
            );
        }
        for s in self.mouse_sensitivity.iter().chain(
            self.layers
                .iter()
                .filter_map(|l| l.mouse_sensitivity.as_ref()),
        ) {
            ensure!(
                s.is_finite() && (0.02..=2.0).contains(s),
                "mouse_sensitivity must be 0.02 to 2"
            );
        }
        Ok(())
    }
}
