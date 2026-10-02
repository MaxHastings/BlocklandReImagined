//! Weapon fields a copy's server preference decides ([`WeaponSetting`]):
//! the port says which fields of the imported pack each preference sets,
//! and the importer writes them as the pack's bindings
//! ([`bri_weapons::Binding`]), each naming the preference's global so
//! whichever Add-On declares it (Tier+Tactical's Tier 1 declares the Ammo
//! System every pack's guns follow) decides it.
use super::Code;
use super::datablocks::{Datablocks, set};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// What one preference does to the weapons.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponSetting {
    /// How the game carries the preference out, for the import report.
    pub how: String,
    /// The fields it sets, in order: a later one setting the same field
    /// wins while its `when` holds.
    pub fields: Vec<FieldSpec>,
}

/// Fields of the imported pack one preference sets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldSpec {
    /// The field's path in `weapons.json`: a kind (`items`, `images`,
    /// `projectiles`), an id, then field names. `*` stands for every id
    /// of the kind the import declares, or every key of the object (index
    /// of the list) there.
    pub path: Vec<String>,
    /// The field's value for each of the preference's values, as text.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, Value>,
    /// The preference's number times this, for values `values` does not
    /// list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    /// Only while these other preferences (by global) have these values.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub when: BTreeMap<String, String>,
    /// Only definitions whose datablock has these fields: `true` for one
    /// that is set (not empty, 0 or false), `false` for one that is not, a
    /// string for that value (any case; `*` at its start or end matches
    /// the rest), a list for any of them. `item.<field>` reads an image's
    /// item; `datablock` is the definition's own datablock name
    /// (`"*staticItem"`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub only: BTreeMap<String, Value>,
    /// The field must already be there (a kick to turn off); `false` adds
    /// one the pack leaves out at its default. Every step before it must be
    /// there either way.
    #[serde(default = "yes")]
    pub existing: bool,
}

fn yes() -> bool {
    true
}

/// The bindings `settings` make of the imported pack `weapons`, as
/// `weapons.json` writes them; settings in order of their global.
pub fn bindings(
    settings: &BTreeMap<String, WeaponSetting>,
    weapons: &Value,
    code: &Code,
) -> Result<Vec<Value>> {
    let blocks = Datablocks::new(weapons, code);
    let mut out = vec![];
    for (global, setting) in settings {
        ensure!(
            bri_package::setting::is_pref_global(global),
            "settings: `{global}` is not a $Pref::Server:: global"
        );
        for (i, spec) in setting.fields.iter().enumerate() {
            ensure!(
                spec.path.len() >= 3 && (!spec.values.is_empty() || spec.scale.is_some()),
                "settings {global} field {i}: a path of a kind, an id and a field, and values or scale"
            );
            for field in expand(weapons, &spec.path, spec.existing) {
                if !spec
                    .only
                    .iter()
                    .all(|(name, wanted)| matches(&blocks, weapons, &field, name, wanted))
                {
                    continue;
                }
                let mut b = serde_json::json!({ "setting": global, "field": field });
                if !spec.values.is_empty() {
                    b["values"] = serde_json::to_value(&spec.values)?;
                }
                if let Some(scale) = spec.scale {
                    b["scale"] = scale.into();
                }
                if !spec.when.is_empty() {
                    b["when"] = serde_json::to_value(&spec.when)?;
                }
                out.push(b);
            }
        }
    }
    Ok(out)
}

/// Every path `path` stands for in `doc`, `*` expanded.
fn expand(doc: &Value, path: &[String], existing: bool) -> Vec<Vec<String>> {
    let mut found = vec![];
    walk(doc, path, existing, &mut vec![], &mut found);
    found
}

fn walk(
    at: &Value,
    rest: &[String],
    existing: bool,
    so_far: &mut Vec<String>,
    found: &mut Vec<Vec<String>>,
) {
    let Some((step, rest)) = rest.split_first() else {
        found.push(so_far.clone());
        return;
    };
    let keys: Vec<String> = match (step.as_str(), at) {
        ("*", Value::Object(map)) => map.keys().cloned().collect(),
        ("*", Value::Array(list)) => (0..list.len()).map(|i| i.to_string()).collect(),
        _ => vec![step.clone()],
    };
    for key in keys {
        let next = match at {
            Value::Object(map) => map.get(&key),
            Value::Array(list) => key.parse::<usize>().ok().and_then(|i| list.get(i)),
            _ => None,
        };
        let next = match next {
            Some(v) => v,
            // Only the last step may be absent, and only when it may be added.
            None if rest.is_empty() && !existing && at.is_object() => &Value::Null,
            None => continue,
        };
        so_far.push(key);
        walk(next, rest, existing, so_far, found);
        so_far.pop();
    }
}

/// Whether the definition `field` reaches has the datablock field `name`
/// as `wanted` ([`FieldSpec::only`]).
fn matches(
    blocks: &Datablocks,
    weapons: &Value,
    field: &[String],
    name: &str,
    wanted: &Value,
) -> bool {
    let (kind, id) = (field[0].as_str(), field[1].as_str());
    if name.eq_ignore_ascii_case("datablock") {
        return is(weapons[kind][id]["name"].as_str(), wanted);
    }
    let (datablock, name) = match name.split_once('.') {
        Some((of, rest)) if of.eq_ignore_ascii_case("item") && kind == "images" => {
            let item = weapons["items"]
                .as_object()
                .into_iter()
                .flatten()
                .find(|(_, item)| item["image"].as_str() == Some(id));
            (item.and_then(|(_, item)| item["name"].as_str()), rest)
        }
        _ => (weapons[kind][id]["name"].as_str(), name),
    };
    is(datablock.and_then(|d| blocks.field(d, name)), wanted)
}

/// Whether a datablock field's `value` is as `wanted` ([`FieldSpec::only`]).
fn is(value: Option<&str>, wanted: &Value) -> bool {
    match wanted {
        Value::Bool(b) => set(value) == *b,
        Value::String(w) => value.is_some_and(|v| {
            let (v, w) = (v.trim().to_ascii_lowercase(), w.to_ascii_lowercase());
            match (w.strip_prefix('*'), w.strip_suffix('*')) {
                (Some(end), _) => v.ends_with(end),
                (_, Some(start)) => v.starts_with(start),
                _ => v == w,
            }
        }),
        Value::Array(any) => any.iter().any(|w| !w.is_array() && is(value, w)),
        _ => false,
    }
}

/// Writes `settings`' bindings into the patched `weapons.json`.
pub fn write(
    settings: &BTreeMap<String, WeaponSetting>,
    doc: &mut Value,
    code: &Code,
) -> Result<()> {
    if settings.is_empty() {
        return Ok(());
    }
    let found = bindings(settings, doc, code)?;
    let list = doc
        .as_object_mut()
        .context("weapons.json is not an object")?
        .entry("bindings")
        .or_insert_with(|| Value::Array(vec![]));
    list.as_array_mut()
        .context("weapons.json bindings is not a list")?
        .extend(found);
    Ok(())
}
