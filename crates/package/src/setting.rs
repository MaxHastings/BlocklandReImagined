//! Add-On settings: typed values an Add-On declares, a host edits in a menu
//! and the Add-On's rules read. Slayer's preferences (`Slayer_PrefSO`: a
//! category, a title, a type and a default) and RTB-style server
//! preferences are this.
//!
//! The engine owns the mechanism (where values live, who may change them,
//! the menu that shows them, telling the rules); the Add-On owns the policy
//! (which settings exist and what they mean).
use serde::{Deserialize, Serialize};

/// One setting's value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingValue {
    Bool(bool),
    Int(i64),
    Text(String),
}
impl SettingValue {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(t) => Some(t),
            _ => None,
        }
    }
}
impl std::fmt::Display for SettingValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(n) => write!(f, "{n}"),
            Self::Text(t) => f.write_str(t),
        }
    }
}

/// Most settings one Add-On declares.
pub const MAX_SETTINGS: usize = 128;
/// Most items one list setting offers, its own and other Add-Ons' together.
pub const MAX_ITEMS: usize = 64;
/// Longest text value.
pub const MAX_TEXT: usize = 256;
/// Longest help text, in bytes.
pub const MAX_HELP: usize = 2048;
/// Longest key, title or category.
const MAX_KEY: usize = 48;
const MAX_TITLE: usize = 64;
const MAX_CATEGORY: usize = 32;

/// Where a setting's value lives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingScope {
    /// One value per mini-game (Slayer's `%mini.lives`).
    #[default]
    Minigame,
    /// One value per team of a mini-game (Slayer's team preferences). A
    /// mini-game shows its team list for editing when any running Add-On
    /// declares one.
    Team,
    /// One value for the whole server, kept between games and restarts
    /// (Slayer's `$Pref::Slayer::Server::*`). Every mini-game's window
    /// shows it.
    Server,
}

/// What kind of value a setting holds, and so how the menu shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingType {
    /// A check box.
    Bool,
    /// A whole number from `min` to `max`.
    Int,
    /// One of `items`, picked from a drop-down.
    List,
    /// Up to `max_length` characters on one line.
    Text,
    /// One of the server's items, or none (`""`): a start tool (Slayer's
    /// team `startEquip`, v20's `object` preference of `ItemData`). The
    /// value is the item's content id; one the server lacks reads as none.
    Item,
    /// One of the server's player types (Slayer's team `playerDatablock`).
    /// The value is the archetype's content id; one the server lacks, or
    /// `""`, reads as `""`, the mini-game's own.
    PlayerType,
    /// A paint palette colour, by index from `min` (0 when left out) to
    /// `max` (63): the menu shows the palette's swatches (Slayer's
    /// mini-game Color).
    PaintColor,
}
/// Longest content id an item or player type setting holds.
pub const MAX_CONTENT_ID: usize = 128;

/// Who may change a setting, among those who may edit the mini-game
/// (Slayer's permission levels). The host may change every one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingEditor {
    /// Whoever may edit the mini-game (Slayer's `Any`).
    #[default]
    Owner,
    /// Admins only.
    Admin,
    /// Super admins only.
    SuperAdmin,
    /// The host only (Slayer's `Host`).
    Host,
    /// The mini-game's creator, or a player on the same account (Slayer's
    /// `Owner`).
    Creator,
    /// Its creator or a player they trust fully (Slayer's
    /// `OwnerFullTrust`).
    FullTrust,
    /// Its creator or a player they trust to build (Slayer's
    /// `OwnerBuildTrust`).
    BuildTrust,
}

/// One choice of a list setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingItem {
    pub value: SettingValue,
    pub name: String,
}

/// Show a setting only while another holds one of some values (a game
/// mode's own settings, shown while that mode is picked).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShownWhen {
    /// The other setting: `key` for the same Add-On's, `namespace:key` for
    /// one of an Add-On this one depends on.
    pub setting: String,
    pub is: Vec<SettingValue>,
}

/// One setting an Add-On declares (in its rules' `behaviour.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingDef {
    /// How rules name it: lowercase letters, digits and `_`.
    pub key: String,
    /// What the menu calls it.
    pub title: String,
    /// The menu's heading it sits under.
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub scope: SettingScope,
    #[serde(rename = "type")]
    pub kind: SettingType,
    pub default: SettingValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<SettingItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(default)]
    pub editor: SettingEditor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shown_when: Option<ShownWhen>,
    /// Changing it is not announced to the game (Slayer's
    /// `notifyPlayersOnChange` false).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub quiet: bool,
    /// Changing it resets the game once the edit is done (Slayer's
    /// `requiresMiniGameReset`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resets: bool,
    /// A longer explanation the menu shows beside it (Slayer's help).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub help: String,
    /// The part of a look it holds ([`AVATAR_KEYS`]: `Hat` as its place in
    /// v20's list, `HatColor` as `"r g b a"`, `FaceName`, ...): the menu
    /// edits a category of these together in the avatar editor (Slayer's
    /// Edit Uniform) instead of row by row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

/// What a look's setting may hold ([`SettingDef::avatar`]): v20's
/// `$pref::Avatar::*` names.
pub const AVATAR_KEYS: [&str; 27] = [
    "Hat", "Accent", "Pack", "SecondPack", "Chest", "Hip", "LArm", "RArm", "LHand", "RHand",
    "LLeg", "RLeg", "HatColor", "AccentColor", "PackColor", "SecondPackColor", "HeadColor",
    "TorsoColor", "HipColor", "LArmColor", "RArmColor", "LHandColor", "RHandColor", "LLegColor",
    "RLegColor", "FaceName", "DecalName",
];

/// More items for another Add-On's list setting: a game mode joining
/// Slayer's mode picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingItems {
    /// `namespace:key` of a list setting of an Add-On this one depends on.
    pub setting: String,
    pub items: Vec<SettingItem>,
}

fn ident(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_KEY
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
fn label(s: &str, max: usize, empty: bool) -> bool {
    (empty || !s.trim().is_empty())
        && s.chars().count() <= max
        && !s.chars().any(char::is_control)
}
/// `key` or `namespace:key`.
pub fn is_setting_ref(s: &str) -> bool {
    match s.split_once(':') {
        Some((ns, key)) => crate::id::namespace_problem(ns).is_none() && ident(key),
        None => ident(s),
    }
}
fn items_ok(items: &[SettingItem]) -> Result<(), String> {
    if items.len() > MAX_ITEMS {
        return Err(format!("at most {MAX_ITEMS} items"));
    }
    for item in items {
        if !label(&item.name, MAX_TITLE, false) {
            return Err(format!("item names are 1 to {MAX_TITLE} characters"));
        }
        if matches!(item.value, SettingValue::Bool(_)) {
            return Err("an item's value is a number or text".into());
        }
    }
    Ok(())
}

impl SettingDef {
    pub fn validate(&self) -> Result<(), String> {
        let what = &self.key;
        if !ident(what) {
            return Err(format!(
                "setting key `{what}` must be lowercase letters, digits and _ (at most {MAX_KEY})"
            ));
        }
        if !label(&self.title, MAX_TITLE, false) {
            return Err(format!("setting `{what}`: title is 1 to {MAX_TITLE} characters"));
        }
        if !label(&self.category, MAX_CATEGORY, true) {
            return Err(format!(
                "setting `{what}`: category is at most {MAX_CATEGORY} characters"
            ));
        }
        let extra = |field: &str, set: bool| {
            if set {
                Err(format!(
                    "setting `{what}`: `{field}` is not for a {:?} setting",
                    self.kind
                ))
            } else {
                Ok(())
            }
        };
        match self.kind {
            SettingType::Bool => {
                extra("min", self.min.is_some())?;
                extra("max", self.max.is_some())?;
                extra("items", !self.items.is_empty())?;
                extra("max_length", self.max_length.is_some())?;
            }
            SettingType::Int => {
                extra("items", !self.items.is_empty())?;
                extra("max_length", self.max_length.is_some())?;
                let (Some(min), Some(max)) = (self.min, self.max) else {
                    return Err(format!("setting `{what}`: a whole number needs `min` and `max`"));
                };
                if min > max || min.abs() > 1_000_000_000 || max.abs() > 1_000_000_000 {
                    return Err(format!(
                        "setting `{what}`: `min` and `max` are -1000000000 to 1000000000, min first"
                    ));
                }
            }
            SettingType::List => {
                extra("min", self.min.is_some())?;
                extra("max", self.max.is_some())?;
                extra("max_length", self.max_length.is_some())?;
                if self.items.is_empty() {
                    return Err(format!("setting `{what}`: a list needs `items`"));
                }
                items_ok(&self.items).map_err(|e| format!("setting `{what}`: {e}"))?;
            }
            SettingType::Item | SettingType::PlayerType => {
                extra("min", self.min.is_some())?;
                extra("max", self.max.is_some())?;
                extra("items", !self.items.is_empty())?;
                extra("max_length", self.max_length.is_some())?;
            }
            SettingType::PaintColor => {
                extra("items", !self.items.is_empty())?;
                extra("max_length", self.max_length.is_some())?;
                let (min, max) = (self.min.unwrap_or(0), self.max.unwrap_or(63));
                if !(-1..=63).contains(&min) || !(0..=63).contains(&max) || min > max {
                    return Err(format!(
                        "setting `{what}`: a paint colour runs -1 (none) to 63, min first"
                    ));
                }
            }
            SettingType::Text => {
                extra("min", self.min.is_some())?;
                extra("max", self.max.is_some())?;
                extra("items", !self.items.is_empty())?;
                if !self
                    .max_length
                    .is_some_and(|n| (1..=MAX_TEXT as u32).contains(&n))
                {
                    return Err(format!(
                        "setting `{what}`: text needs `max_length`, 1 to {MAX_TEXT}"
                    ));
                }
            }
        }
        // A list's default may be one of another Add-On's items; the full
        // list is checked when the Add-Ons run together.
        if self.kind != SettingType::List {
            self.check(&self.default, &[])
                .map_err(|e| format!("setting `{what}`: default: {e}"))?;
        } else if matches!(self.default, SettingValue::Bool(_)) {
            return Err(format!("setting `{what}`: a list's default is a number or text"));
        }
        if self.help.len() > MAX_HELP || self.help.chars().any(|c| c.is_control() && c != '\n') {
            return Err(format!("setting `{what}`: help is at most {MAX_HELP} bytes"));
        }
        if let Some(part) = &self.avatar {
            let fits = match part.as_str() {
                p if !AVATAR_KEYS.contains(&p) => false,
                p if p.ends_with("Color") || p.ends_with("Name") => self.kind == SettingType::Text,
                _ => self.kind == SettingType::Int,
            };
            if !fits {
                return Err(format!(
                    "setting `{what}`: avatar is one of {} (a part a number, a colour or name text)",
                    AVATAR_KEYS.join(", ")
                ));
            }
        }
        if let Some(when) = &self.shown_when
            && (!is_setting_ref(&when.setting) || when.is.is_empty() || when.is.len() > MAX_ITEMS)
        {
            return Err(format!(
                "setting `{what}`: shown_when names a setting and 1 to {MAX_ITEMS} values"
            ));
        }
        Ok(())
    }
    /// Whether `value` is one this setting may hold, with `more` items
    /// other Add-Ons added to a list.
    pub fn check(&self, value: &SettingValue, more: &[SettingItem]) -> Result<(), String> {
        match (self.kind, value) {
            (SettingType::Bool, SettingValue::Bool(_)) => Ok(()),
            (SettingType::Int, SettingValue::Int(n)) => {
                let (min, max) = (self.min.unwrap_or(i64::MIN), self.max.unwrap_or(i64::MAX));
                if (min..=max).contains(n) {
                    Ok(())
                } else {
                    Err(format!("{} is {min} to {max}", self.title))
                }
            }
            (SettingType::List, v) => {
                if self.items.iter().chain(more).any(|i| i.value == *v) {
                    Ok(())
                } else {
                    Err(format!("{} has no choice {v}", self.title))
                }
            }
            (SettingType::Text, SettingValue::Text(t)) => {
                let max = self.max_length.unwrap_or(0) as usize;
                if t.chars().count() <= max && !t.chars().any(char::is_control) {
                    Ok(())
                } else {
                    Err(format!("{} is at most {max} characters on one line", self.title))
                }
            }
            (SettingType::PaintColor, SettingValue::Int(n)) => {
                let (min, max) = (self.min.unwrap_or(0), self.max.unwrap_or(63));
                if (min..=max).contains(n) {
                    Ok(())
                } else {
                    Err(format!("{} is a paint colour from {min} to {max}", self.title))
                }
            }
            (SettingType::Item | SettingType::PlayerType, SettingValue::Text(t)) => {
                if t.len() <= MAX_CONTENT_ID && t.chars().all(|c| c.is_ascii_graphic()) {
                    Ok(())
                } else {
                    Err(format!("{} is a content id", self.title))
                }
            }
            _ => Err(format!(
                "{} takes {}",
                self.title,
                match self.kind {
                    SettingType::Bool => "on or off",
                    SettingType::Int => "a whole number",
                    SettingType::List => "one of its choices",
                    SettingType::Text => "text",
                    SettingType::Item => "an item",
                    SettingType::PlayerType => "a player type",
                    SettingType::PaintColor => "a paint colour",
                }
            )),
        }
    }
}
impl SettingItems {
    pub fn validate(&self) -> Result<(), String> {
        if !self.setting.contains(':') || !is_setting_ref(&self.setting) {
            return Err(format!(
                "setting_items `{}` must name another Add-On's setting as namespace:key",
                self.setting
            ));
        }
        if self.items.is_empty() {
            return Err(format!("setting_items `{}` needs items", self.setting));
        }
        items_ok(&self.items).map_err(|e| format!("setting_items `{}`: {e}", self.setting))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(json: &str) -> SettingDef {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn slayer_style_settings_validate_and_check_values() {
        let lives = def(
            r#"{ "key": "lives", "title": "Lives", "category": "Victory Method",
                 "type": "int", "default": 0, "min": 0, "max": 99 }"#,
        );
        lives.validate().unwrap();
        assert!(lives.check(&SettingValue::Int(5), &[]).is_ok());
        assert!(lives.check(&SettingValue::Int(100), &[]).is_err());
        assert!(lives.check(&SettingValue::Bool(true), &[]).is_err());
        let mode = def(
            r#"{ "key": "mode", "title": "Game Mode", "type": "list", "default": "slyr",
                 "items": [ { "value": "slyr", "name": "Slayer" } ] }"#,
        );
        mode.validate().unwrap();
        let ctf = [SettingItem {
            value: SettingValue::Text("ctf".into()),
            name: "Capture the Flag".into(),
        }];
        assert!(mode.check(&SettingValue::Text("ctf".into()), &[]).is_err());
        assert!(mode.check(&SettingValue::Text("ctf".into()), &ctf).is_ok());
        let bad = def(r#"{ "key": "x", "title": "X", "type": "int", "default": 0 }"#);
        assert!(bad.validate().is_err(), "a number needs its range");
        let bad = def(r#"{ "key": "Bad", "title": "X", "type": "bool", "default": true }"#);
        assert!(bad.validate().is_err());
    }

    #[test]
    fn item_and_player_type_settings_hold_a_content_id_or_none() {
        let equip = def(
            r#"{ "key": "equip", "title": "Start Equip 0", "type": "item",
                 "default": "v20.weapon.hammeritem", "scope": "team" }"#,
        );
        equip.validate().unwrap();
        assert!(equip.check(&SettingValue::Text(String::new()), &[]).is_ok());
        assert!(equip.check(&SettingValue::Text("a b".into()), &[]).is_err());
        assert!(equip.check(&SettingValue::Text("x".repeat(129)), &[]).is_err());
        assert!(equip.check(&SettingValue::Int(1), &[]).is_err());
        let body = def(
            r#"{ "key": "body", "title": "Playertype", "type": "player_type",
                 "default": "v20.player.playerstandardarmor", "max": 3 }"#,
        );
        assert!(body.validate().is_err(), "a player type has no range");
    }
}
