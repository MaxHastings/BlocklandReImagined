//! What a port reads from the imported datablocks themselves, beside the
//! script patterns of `covers`: the magazines a classic ammo system kept in
//! item fields ([`Magazines`]) and tables of datablock fields for its host
//! rules ([`Table`]). Both read `definitions` in the imported
//! `weapons.json`, so they follow each copy's own numbers and names.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The magazines of a v20 ammo system that keeps them in item fields, as
/// Jack's hl2 ammo system does (`maxmag`, `ammotype`). Every item with both
/// fields gets a `magazine` on its image: the size from the item, the
/// reserve from its ammo type's line in `types`, the reload length from the
/// image's own reload states. An item naming a type `types` lacks stops
/// the port, so a copy with other ammo is named in the report, not guessed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Magazines {
    /// The item field holding the rounds a magazine holds (`maxmag`).
    pub size: String,
    /// The item field naming its ammo type (`ammotype`).
    pub ammo: String,
    /// Each ammo type, by the name the items give it.
    pub types: BTreeMap<String, AmmoType>,
    /// Ticks a reload lasts when the image's states do not show it.
    pub reload_ticks: u32,
    /// Fields for one item's magazine, by item datablock name
    /// (`"one_by_one": true` for a gun loaded a shell at a time).
    #[serde(default)]
    pub items: BTreeMap<String, Value>,
}

/// One ammo type of [`Magazines`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AmmoType {
    /// The engine's name for it: letters, digits, `.`, `_` or `-`.
    pub ammo: String,
    /// The reserve a player starts with.
    pub reserve: u32,
    /// The most reserve a player carries.
    pub max_reserve: u32,
}

/// A table of datablock fields for the host rules: in a rules file,
/// `{{name}}` becomes a Rhai map from each matching datablock's key to a
/// map of the listed fields it has (its own or inherited).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    /// The datablock class (`ProjectileData`, `ItemData`).
    pub class: String,
    /// Fields to read, as the script spells them.
    pub fields: Vec<String>,
    /// Only datablocks where each of these fields is set and not false or 0.
    #[serde(default)]
    pub when: Vec<String>,
    /// What keys the table: `id` (the imported id, such as
    /// `<ns>:weapon/ammoitem`), `name` (the datablock's name) or
    /// `damage_type` (a projectile's damage type as `on_damage` names it).
    pub key: String,
}

/// The imported datablocks, with inheritance (`datablock A(x : B)`).
pub struct Datablocks<'a> {
    doc: &'a Value,
    by_name: BTreeMap<String, &'a Value>,
}

impl<'a> Datablocks<'a> {
    pub fn new(weapons: &'a Value) -> Self {
        let by_name = weapons["definitions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| Some((d["name"].as_str()?.to_ascii_lowercase(), d)))
            .collect();
        Self {
            doc: weapons,
            by_name,
        }
    }

    /// `field` of the datablock `name`, its own or its nearest parent's, as
    /// written (string literals keep their quotes).
    fn raw(&self, name: &str, field: &str) -> Option<&'a str> {
        let field = field.to_ascii_lowercase();
        let mut at = self.by_name.get(&name.to_ascii_lowercase())?;
        for _ in 0..16 {
            if let Some(v) = at["fields"][&field].as_str() {
                return Some(v);
            }
            at = self
                .by_name
                .get(&at["parent"].as_str()?.to_ascii_lowercase())?;
        }
        None
    }

    /// `field` as its value: the text of a string literal, else as written.
    fn field(&self, name: &str, field: &str) -> Option<&'a str> {
        self.raw(name, field).map(crate::literal)
    }

    fn of_class<'b>(&'b self, class: &'b str) -> impl Iterator<Item = &'a Value> + 'b {
        self.by_name.values().copied().filter(move |d| {
            d["class"]
                .as_str()
                .is_some_and(|c| c.eq_ignore_ascii_case(class))
        })
    }
}

fn set(v: Option<&str>) -> bool {
    v.is_some_and(|v| {
        let v = v.trim();
        !(v.is_empty() || v == "0" || v.eq_ignore_ascii_case("false"))
    })
}

/// The `weapons.json` patch giving each ammo-system gun's image its
/// magazine, and the rules' values `magazine_items` (item id to engine ammo
/// name) and `magazine_types` (the item's type name to its numbers).
pub fn magazines(m: &Magazines, weapons: &Value) -> Result<(Value, BTreeMap<String, String>)> {
    let blocks = Datablocks::new(weapons);
    let mut images = serde_json::Map::new();
    let mut items = BTreeMap::new();
    for (id, item) in weapons["items"].as_object().into_iter().flatten() {
        let name = item["name"].as_str().unwrap_or_default();
        let (Some(size), Some(kind)) = (blocks.field(name, &m.size), blocks.field(name, &m.ammo))
        else {
            continue;
        };
        let size: u32 = size
            .trim()
            .parse()
            .with_context(|| format!("{name}: {} `{size}` is not a whole number", m.size))?;
        let ty = m
            .types
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(kind))
            .map(|(_, t)| t)
            .with_context(|| format!("{name}: ammo type `{kind}` is not in the port's types"))?;
        let image_id = item["image"].as_str().unwrap_or_default();
        let image = &weapons["images"][image_id];
        ensure!(
            image.is_object(),
            "{name}: its image {image_id} did not import"
        );
        let mut magazine = json!({
            "size": size,
            "ammo": ty.ammo,
            "reload_ticks": reload_ticks(image).unwrap_or(m.reload_ticks),
            "reserve": ty.reserve,
            "max_reserve": ty.max_reserve,
            "display": kind,
        });
        if let Some((_, extra)) = m.items.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)) {
            super::merge(&mut magazine, extra);
        }
        if let Some(other) = images.get(image_id)
            && *other != magazine
        {
            bail!("{image_id} is the image of guns with different magazines");
        }
        images.insert(image_id.to_owned(), json!({ "magazine": magazine }));
        items.insert(id.clone(), Value::String(ty.ammo.clone()));
    }
    let types = m
        .types
        .iter()
        .map(|(k, t)| {
            (
                k.clone(),
                json!({ "ammo": t.ammo, "reserve": t.reserve, "max_reserve": t.max_reserve }),
            )
        })
        .collect();
    let values = BTreeMap::from([
        (
            "magazine_items".to_owned(),
            rhai(&Value::Object(items.into_iter().collect())),
        ),
        ("magazine_types".to_owned(), rhai(&Value::Object(types))),
    ]);
    Ok((json!({ "images": images }), values))
}

/// How long the image's own reload takes: from the state its ready state
/// goes to without ammo, along each state's timeout, up to the state that
/// waits for the trigger or checks the ammo again. The rounds arrive as
/// that check runs, as the ammo system's `onReload` moved them.
fn reload_ticks(image: &Value) -> Option<u32> {
    let states = image["states"].as_array()?;
    let index = |v: &Value| v.as_u64().map(|i| i as usize);
    let waits = |s: &Value| !s["down"].is_null() || !s["ammo"].is_null() || !s["no_ammo"].is_null();
    let ready = states
        .iter()
        .find(|s| !s["down"].is_null() && !s["no_ammo"].is_null())?;
    let mut at = index(&ready["no_ammo"])?;
    let (mut ticks, mut seen) = (0u64, vec![]);
    while let Some(s) = states.get(at) {
        if waits(s) || seen.contains(&at) {
            break;
        }
        seen.push(at);
        ticks += s["ticks"].as_u64().unwrap_or(0);
        at = index(&s["timeout"])?;
    }
    (1..=1200).contains(&ticks).then_some(ticks as u32)
}

/// A port's tables, as Rhai map literals by table name.
pub fn tables(
    tables: &BTreeMap<String, Table>,
    weapons: &Value,
) -> Result<BTreeMap<String, String>> {
    let blocks = Datablocks::new(weapons);
    let mut out = BTreeMap::new();
    for (name, t) in tables {
        let mut rows = serde_json::Map::new();
        for d in blocks.of_class(&t.class) {
            let block = d["name"].as_str().unwrap_or_default();
            if !t.when.iter().all(|f| set(blocks.field(block, f))) {
                continue;
            }
            let key = match t.key.as_str() {
                "name" => block.to_owned(),
                "id" => match id_of(blocks.doc, &t.class, block) {
                    Some(id) => id,
                    None => continue,
                },
                "damage_type" => match damage_type_of(blocks.doc, block) {
                    Some(t) => t,
                    None => continue,
                },
                other => bail!("table {name}: key `{other}` is not id, name or damage_type"),
            };
            let row = t
                .fields
                .iter()
                .filter_map(|f| {
                    let raw = blocks.raw(block, f)?;
                    Some((f.to_ascii_lowercase(), value(raw)))
                })
                .collect();
            rows.insert(key, Value::Object(row));
        }
        out.insert(name.clone(), rhai(&Value::Object(rows)));
    }
    Ok(out)
}

/// The imported id of the datablock `name` of `class`.
fn id_of(weapons: &Value, class: &str, name: &str) -> Option<String> {
    let section = match class.to_ascii_lowercase().as_str() {
        "itemdata" => "items",
        "shapebaseimagedata" => "images",
        "projectiledata" => "projectiles",
        _ => return None,
    };
    weapons[section]
        .as_object()?
        .iter()
        .find(|(_, v)| {
            v["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .map(|(id, _)| id.clone())
}

/// A projectile's damage type as `on_damage` names it (no `$DamageType::`).
fn damage_type_of(weapons: &Value, name: &str) -> Option<String> {
    let id = id_of(weapons, "ProjectileData", name)?;
    let t = weapons["projectiles"][&id]["damage_type"].as_str()?;
    let t = t.strip_prefix("$DamageType::").unwrap_or(t);
    (!t.is_empty()).then(|| t.to_owned())
}

/// A field as written, as a value: a number, `true`/`false`, or the text.
fn value(raw: &str) -> Value {
    let text = crate::literal(raw);
    if text.len() == raw.len() {
        if let Ok(n) = text.trim().parse::<i64>() {
            return json!(n);
        }
        if let Ok(n) = text.trim().parse::<f64>()
            && n.is_finite()
        {
            return json!(n);
        }
        if text.eq_ignore_ascii_case("true") || text.eq_ignore_ascii_case("false") {
            return json!(text.eq_ignore_ascii_case("true"));
        }
    }
    json!(text)
}

/// A JSON value as a Rhai literal: maps as `#{ "key": value }`.
pub fn rhai(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let items: Vec<String> = m
                .iter()
                .map(|(k, v)| format!("{}: {}", Value::String(k.clone()), rhai(v)))
                .collect();
            format!("#{{{}}}", items.join(", "))
        }
        Value::Array(a) => format!("[{}]", a.iter().map(rhai).collect::<Vec<_>>().join(", ")),
        Value::Number(n) if n.is_f64() => {
            let f = n.as_f64().unwrap_or_default();
            if f.fract() == 0.0 {
                format!("{f:.1}")
            } else {
                format!("{f}")
            }
        }
        Value::Null => "()".to_owned(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rhai_literals() {
        assert_eq!(
            rhai(&json!({"a b": {"x": 1, "y": 1.5, "z": "q\"", "t": true}})),
            r#"#{"a b": #{"t": true, "x": 1, "y": 1.5, "z": "q\""}}"#
        );
        assert_eq!(rhai(&json!(2.0)), "2.0");
    }

    #[test]
    fn values_read_numbers_bools_and_text() {
        assert_eq!(value("1.5"), json!(1.5));
        assert_eq!(value("12"), json!(12));
        assert_eq!(value("true"), json!(true));
        assert_eq!(value("\"12\""), json!("12"));
        assert_eq!(value("$DamageType::Gun"), json!("$DamageType::Gun"));
    }
}
