//! Pack fields a server setting decides ([`Binding`]): Tier+Tactical's
//! `$Pref::Server::TT::Recoil` turning its guns' kick off, its Display
//! Duration setting the ammo display's time. The pack is authored with its
//! bindings; the game plays [`Pack::with_settings`], derived again whenever
//! one of their settings changes, on the host and on every player's side
//! alike.
use crate::Pack;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The most bindings a pack carries.
pub const MAX_BINDINGS: usize = 8192;
/// The deepest field a binding sets, its kind and id included.
pub const MAX_DEPTH: usize = 10;

/// One field of the pack that a server setting decides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    /// The setting: `<package>:<key>` as Add-On settings name it, or the
    /// v20 global a running Add-On's setting stands for
    /// (`$Pref::Server::TT::Recoil`), whichever Add-On declares it.
    pub setting: String,
    /// The field, as its path in the pack's JSON: a kind (`items`,
    /// `images`, `projectiles`), one of the pack's ids of that kind, then
    /// field names and list indices (`["images", "ns:image/gun", "shot",
    /// "kick"]`).
    pub field: Vec<String>,
    /// The field's value for each value of the setting, by that value as
    /// text (`"true"`, `"3"`, an item id); `null` clears an optional field.
    /// A setting value not listed leaves the field as authored, or with
    /// `scale` too, scales.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, Value>,
    /// A number setting's value times `scale`, whole when it comes out
    /// whole (a duration in seconds as ticks), for values `values` does not
    /// list (Tier's shield durability: -1 for none, else the count).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    /// Applies only while each of these settings (named as `setting`) has
    /// the value given, as text. A later binding of the same field wins
    /// over an earlier one.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub when: BTreeMap<String, String>,
}

/// The kinds of definition a binding can reach.
const KINDS: [&str; 3] = ["items", "images", "projectiles"];

impl Binding {
    pub fn validate(&self) -> Result<()> {
        let name = |s: &str| {
            !s.is_empty() && s.len() <= 128 && !s.chars().any(|c| c.is_control() || c.is_whitespace())
        };
        ensure!(
            name(&self.setting) && self.when.len() <= 4 && self.when.keys().all(|k| name(k)),
            "A binding names a setting by `<package>:<key>` or its global"
        );
        ensure!(
            (3..=MAX_DEPTH).contains(&self.field.len())
                && KINDS.contains(&self.field[0].as_str())
                && self.field.iter().all(|f| !f.is_empty() && f.len() <= 128),
            "A binding's field is a kind ({}), an id and the path in it, at most {MAX_DEPTH} deep",
            KINDS.join(", ")
        );
        // What says which definition it is, and its states' order, stays.
        ensure!(
            !matches!(self.field[2].as_str(), "id" | "states" | "image" | "item"),
            "A binding cannot change a definition's id, states or what it names"
        );
        ensure!(
            !self.values.is_empty() || self.scale.is_some(),
            "A binding has `values` or `scale`"
        );
        ensure!(
            self.values.len() <= 64
                && self.values.keys().all(|k| k.len() <= 128)
                && self.when.values().all(|v| v.len() <= 128)
                && self.scale.is_none_or(|s| s.is_finite() && s.abs() <= 1e6),
            "A binding has at most 64 values and a finite scale"
        );
        Ok(())
    }
    /// Every setting it reads.
    pub fn settings(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.setting.as_str()).chain(self.when.keys().map(String::as_str))
    }
    /// The field's value for the settings `value` gives (as text, `None`
    /// for a setting no running Add-On declares), or `None` to leave it.
    fn pick(&self, value: &impl Fn(&str) -> Option<String>) -> Result<Option<Value>> {
        if self
            .when
            .iter()
            .any(|(setting, wanted)| value(setting).as_deref() != Some(wanted.as_str()))
        {
            return Ok(None);
        }
        let Some(v) = value(&self.setting) else {
            return Ok(None);
        };
        if let Some(listed) = self.values.get(&v) {
            return Ok(Some(listed.clone()));
        }
        if let Some(scale) = self.scale {
            let n: f64 = v
                .parse()
                .with_context(|| format!("`{}` is `{v}`, not a number", self.setting))?;
            let n = n * scale;
            return Ok(Some(if n.fract() == 0.0 && n.abs() < 9e15 {
                Value::from(n as i64)
            } else {
                Value::from(n)
            }));
        }
        Ok(None)
    }
}

impl Pack {
    /// The settings the pack's bindings read.
    pub fn bound_settings(&self) -> BTreeSet<&str> {
        self.bindings.iter().flat_map(Binding::settings).collect()
    }
    /// The pack with its bindings' fields set from the settings `value`
    /// gives as text (`None` for one no running Add-On declares, leaving
    /// its fields as authored). The result is checked as any pack: a value
    /// that takes a field out of its range is refused, and its
    /// definitions and their states are the authored ones.
    pub fn with_settings(&self, value: impl Fn(&str) -> Option<String>) -> Result<Pack> {
        if self.bindings.is_empty() {
            return Ok(self.clone());
        }
        let mut changes = Vec::new();
        for b in &self.bindings {
            if let Some(v) = b.pick(&value)? {
                changes.push((b, v));
            }
        }
        if changes.is_empty() {
            return Ok(self.clone());
        }
        let mut json = serde_json::to_value(self)?;
        for (b, v) in changes {
            set(&mut json, &b.field, v)
                .with_context(|| format!("{} (from {})", b.field.join("/"), b.setting))?;
        }
        let mut pack: Pack = serde_json::from_value(json).context("A setting's value")?;
        pack.fill_ids();
        pack.validate().context("A setting's value")?;
        ensure!(
            same_shape(self, &pack),
            "A setting's value changed what the weapons are"
        );
        Ok(pack)
    }
}

/// `json` at `path` set to `value`: every step but the last must be there;
/// the last is added when absent (an optional field left out).
fn set(json: &mut Value, path: &[String], value: Value) -> Result<()> {
    let (last, steps) = path.split_last().context("empty path")?;
    let mut at = json;
    for step in steps {
        at = match at {
            Value::Object(map) => map.get_mut(step),
            Value::Array(list) => step.parse::<usize>().ok().and_then(|i| list.get_mut(i)),
            _ => None,
        }
        .with_context(|| format!("no `{step}`"))?;
    }
    match at {
        Value::Object(map) => {
            if value.is_null() {
                map.remove(last);
            } else {
                map.insert(last.clone(), value);
            }
        }
        Value::Array(list) => {
            let slot = last
                .parse::<usize>()
                .ok()
                .and_then(|i| list.get_mut(i))
                .with_context(|| format!("no `{last}`"))?;
            *slot = value;
        }
        _ => bail!("`{last}` is not in an object or list"),
    }
    Ok(())
}

/// The same definitions, each image with as many states.
fn same_shape(a: &Pack, b: &Pack) -> bool {
    a.items.keys().eq(b.items.keys())
        && a.projectiles.keys().eq(b.projectiles.keys())
        && a.images.len() == b.images.len()
        && a.images
            .iter()
            .zip(&b.images)
            .all(|((ka, ia), (kb, ib))| ka == kb && ia.states.len() == ib.states.len())
}

/// The pack's bindings are within their limits and each reaches one of its
/// own definitions.
pub(crate) fn validate(pack: &Pack) -> Result<()> {
    ensure!(
        pack.bindings.len() <= MAX_BINDINGS,
        "A pack has at most {MAX_BINDINGS} bindings"
    );
    for b in &pack.bindings {
        b.validate()?;
        let id = &b.field[1];
        let there = match b.field[0].as_str() {
            "items" => pack.items.contains_key(id),
            "images" => pack.images.contains_key(id),
            _ => pack.projectiles.contains_key(id),
        };
        ensure!(there, "A binding names {id}, which the pack does not declare");
    }
    Ok(())
}
