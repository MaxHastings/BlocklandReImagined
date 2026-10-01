//! What a port reads from the imported datablocks themselves, beside the
//! script patterns of `covers`: the magazines a classic ammo system kept in
//! item fields ([`Magazines`]), the hitscans of a raycasting system's image
//! fields ([`Hitscans`]), what each image's own script methods did
//! ([`ScriptRule`]) and tables of datablock fields for its host rules
//! ([`Table`]). All read the imported `weapons.json` (its `definitions`, and
//! the script bodies for [`ScriptRule`]), so they follow each copy's own
//! numbers and names.
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
    /// Fields every magazine gets, before `items` (a script ammo system's
    /// `reload_state` and `checks`).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub common: Value,
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
    /// What the ammo display calls it; the items' name for it when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display: String,
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
        if size == 0 {
            // No magazine: the system counts these straight from the reserve.
            continue;
        }
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
            "display": if ty.display.is_empty() { kind } else { ty.display.as_str() },
        });
        if !m.common.is_null() {
            super::merge(&mut magazine, &m.common);
        }
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

/// The hitscans of a raycasting weapon system that kept them in image
/// fields, as Tier+Tactical (`TT_raycast*`) and Space Guy's raycasting
/// weapons (`raycast*`) did: each field names the image field holding it.
/// Every image with `enabled` set gets a `shot` with a hitscan whose ray
/// does its own damage, push, explosion and sounds ([`bri_weapons::RayHit`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hitscans {
    pub enabled: String,
    pub range: String,
    pub spread: String,
    pub count: String,
    pub damage: String,
    pub damage_type: String,
    pub impulse: String,
    pub vertical_impulse: String,
    /// A projectile exploded where the ray lands.
    pub explosion: String,
    pub player_sound: String,
    pub other_sound: String,
    /// A projectile flown from the muzzle to where the ray ended; it
    /// becomes the image's projectile.
    pub tracer: String,
    /// Set: cast from the muzzle. Unset: from the eye.
    pub from_muzzle: String,
}

/// The `weapons.json` patch giving each raycasting image its hitscan.
pub fn hitscans(h: &Hitscans, weapons: &Value) -> Result<Value> {
    let blocks = Datablocks::new(weapons);
    let mut images = serde_json::Map::new();
    for (id, image) in weapons["images"].as_object().into_iter().flatten() {
        let name = image["name"].as_str().unwrap_or_default();
        if !set(blocks.field(name, &h.enabled)) {
            continue;
        }
        let number = |field: &str, default: f64| -> Result<f64> {
            match blocks.field(name, field) {
                None => Ok(default),
                Some(v) => v
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .with_context(|| format!("{name}: {field} `{v}` is not a number")),
            }
        };
        let text = |field: &str| blocks.field(name, field).unwrap_or_default().trim();
        let range = number(&h.range, 0.0)?;
        ensure!(range > 0.0, "{name}: {} is not set", h.range);
        let tracer = text(&h.tracer);
        let tracer_id = if tracer.is_empty() {
            None
        } else {
            Some(
                id_of(weapons, "ProjectileData", tracer)
                    .with_context(|| format!("{name}: its tracer {tracer} did not import"))?,
            )
        };
        let mut patch = json!({
            "shot": {
                "projectiles": number(&h.count, 1.0)?.max(1.0) as u32,
                "spread": number(&h.spread, 0.0)?,
                "hitscan": {
                    "range": range,
                    "from_eye": !set(blocks.field(name, &h.from_muzzle)),
                    "hit": {
                        "damage": number(&h.damage, 0.0)?,
                        "damage_type": text(&h.damage_type),
                        "impulse": number(&h.impulse, 0.0)?,
                        "vertical_impulse": number(&h.vertical_impulse, 0.0)?,
                        "explosion": projectile_ref(weapons, text(&h.explosion)),
                        "player_sound": sound_ref(weapons, text(&h.player_sound)),
                        "other_sound": sound_ref(weapons, text(&h.other_sound)),
                        "tracer": tracer_id.is_some(),
                    }
                }
            }
        });
        if let Some(tracer) = tracer_id {
            patch["projectile"] = Value::String(tracer);
        }
        images.insert(id.clone(), patch);
    }
    Ok(json!({ "images": images }))
}

/// A rule's named groups in `owner::method`'s `body`, None when it does
/// not match; an error when it matches `required` instead.
fn groups(
    re: &regex::Regex,
    required: Option<&regex::Regex>,
    body: &str,
    owner: &str,
    method: &str,
) -> Result<Option<BTreeMap<String, String>>> {
    let Some(caps) = re.captures(body) else {
        if required.is_some_and(|r| r.is_match(body)) {
            bail!("{owner}::{method} does what a script rule reads, but not as its pattern says");
        }
        return Ok(None);
    };
    Ok(Some(
        re.capture_names()
            .flatten()
            .filter_map(|g| Some((g.to_owned(), caps.name(g)?.as_str().to_owned())))
            .collect(),
    ))
}

/// What one image script method did, read from its body for every image
/// of the import that has it: a pattern whose named groups fill `set`, a
/// JSON merge patch for the image, its `shot` or `magazine`, or each of
/// its states running the method. In `set`, a string that is exactly
/// `{group}` becomes the group's value (a number when it reads as one),
/// `{group|kick}` the view kick of the projectile it names (its
/// explosion's camera shake), `{group|sound}` the sound it names and
/// `{group|projectile}` the projectile; `{group}` inside a longer string
/// becomes its text.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRule {
    /// The method (`onFire`), or `*` for every state script of the image.
    pub method: String,
    /// `image`, `shot`, `magazine` or `state` read the image's method;
    /// `projectile` reads the projectile's (`damage`, never `*`) and sets
    /// its fields.
    pub into: String,
    /// Case-insensitive; `.` does not match a line break unless `(?s)`.
    pub pattern: String,
    /// When the body matches this but not `pattern`, the port stops and
    /// names the image: a copy that does what the rule reads, some other
    /// way, is not guessed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_by: Option<String>,
    pub set: Value,
    /// With `state`: only fields the state leaves empty.
    #[serde(default)]
    pub keep: bool,
}

/// The `weapons.json` patch from a port's script rules, given the import's
/// script bodies (lowercase `image::method` to body).
pub fn scripts(
    rules: &[ScriptRule],
    weapons: &Value,
    bodies: &BTreeMap<String, String>,
) -> Result<Value> {
    let mut images = serde_json::Map::new();
    let mut projectiles = serde_json::Map::new();
    for rule in rules {
        let re = super::pattern(&rule.pattern).context("a script rule's pattern")?;
        let required = rule
            .required_by
            .as_deref()
            .map(super::pattern)
            .transpose()
            .context("a script rule's required_by")?;
        ensure!(
            ["image", "shot", "magazine", "state", "projectile"].contains(&rule.into.as_str()),
            "a script rule goes into `{}`, not image, shot, magazine, state or projectile",
            rule.into
        );
        if rule.into == "projectile" {
            ensure!(
                rule.method != "*",
                "a projectile's script rule names its method"
            );
            let method = rule.method.to_ascii_lowercase();
            for (id, projectile) in weapons["projectiles"].as_object().into_iter().flatten() {
                let name = projectile["name"].as_str().unwrap_or_default();
                let Some(body) = bodies.get(&format!("{}::{method}", name.to_ascii_lowercase()))
                else {
                    continue;
                };
                let Some(values) = groups(&re, required.as_ref(), body, name, &method)? else {
                    continue;
                };
                let set = fill(&rule.set, &values, weapons)
                    .with_context(|| format!("{name}::{method}"))?;
                super::merge(
                    projectiles.entry(id.clone()).or_insert_with(|| json!({})),
                    &set,
                );
            }
            continue;
        }
        for (id, image) in weapons["images"].as_object().into_iter().flatten() {
            let name = image["name"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase();
            let states = image["states"].as_array().cloned().unwrap_or_default();
            let methods: Vec<String> = if rule.method == "*" {
                let mut m: Vec<String> = states
                    .iter()
                    .filter_map(|s| s["script"].as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_ascii_lowercase)
                    .collect();
                m.sort();
                m.dedup();
                m
            } else {
                vec![rule.method.to_ascii_lowercase()]
            };
            for method in methods {
                let Some(body) = bodies.get(&format!("{name}::{method}")) else {
                    continue;
                };
                let owner = image["name"].as_str().unwrap_or_default();
                let Some(values) = groups(&re, required.as_ref(), body, owner, &method)? else {
                    continue;
                };
                let set = fill(&rule.set, &values, weapons)
                    .with_context(|| format!("{name}::{method}"))?;
                let entry = images.entry(id.clone()).or_insert_with(|| json!({}));
                match rule.into.as_str() {
                    "image" => super::merge(entry, &set),
                    "shot" => {
                        if entry.get("shot").is_none() && image.get("shot").is_none() {
                            entry["shot"] = json!({ "projectiles": 1 });
                        }
                        super::merge(entry, &json!({ "shot": set }));
                    }
                    "magazine" => super::merge(entry, &json!({ "magazine": set })),
                    _ => {
                        // States are an array: patch the whole list.
                        if entry.get("states").is_none() {
                            entry["states"] = Value::Array(states.clone());
                        }
                        for state in entry["states"].as_array_mut().into_iter().flatten() {
                            if !state["script"]
                                .as_str()
                                .is_some_and(|s| s.eq_ignore_ascii_case(&method))
                            {
                                continue;
                            }
                            let mut set = set.clone();
                            if rule.keep
                                && let Some(m) = set.as_object_mut()
                            {
                                m.retain(|k, _| {
                                    state
                                        .get(k)
                                        .is_none_or(|v| v.is_null() || v.as_str() == Some(""))
                                });
                            }
                            super::merge(state, &set);
                        }
                    }
                }
            }
        }
    }
    let mut patch = json!({ "images": images });
    if !projectiles.is_empty() {
        patch["projectiles"] = Value::Object(projectiles);
    }
    Ok(patch)
}

/// [`ScriptRule::set`] with its groups' values.
fn fill(v: &Value, values: &BTreeMap<String, String>, weapons: &Value) -> Result<Value> {
    Ok(match v {
        Value::String(s) if s.starts_with('{') && s.ends_with('}') && !s[1..].contains('{') => {
            let inner = &s[1..s.len() - 1];
            let (group, filter) = inner.split_once('|').unwrap_or((inner, ""));
            let value = values
                .get(group)
                .with_context(|| format!("`{s}`: the pattern has no group `{group}`"))?;
            match filter {
                "" => value_of(value),
                "kick" => kick(weapons, value)
                    .with_context(|| format!("`{value}` is no projectile with a camera shake"))?,
                "sound" => json!(sound_ref(weapons, value)),
                "projectile" => json!(projectile_ref(weapons, value)),
                other => bail!("`{s}`: no filter `{other}` (kick, sound or projectile)"),
            }
        }
        Value::String(s) => {
            let mut s = s.clone();
            for (name, value) in values {
                s = s.replace(&format!("{{{name}}}"), value);
            }
            Value::String(s)
        }
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| Ok((k.clone(), fill(v, values, weapons)?)))
                .collect::<Result<_>>()?,
        ),
        Value::Array(a) => Value::Array(
            a.iter()
                .map(|v| fill(v, values, weapons))
                .collect::<Result<_>>()?,
        ),
        other => other.clone(),
    })
}

/// A captured value: a number when it reads as one.
fn value_of(text: &str) -> Value {
    let t = text.trim();
    if let Ok(n) = t.parse::<i64>() {
        return json!(n);
    }
    match t.parse::<f64>() {
        Ok(n) if n.is_finite() => json!(n),
        _ => json!(text),
    }
}

/// The view kick of the projectile `name`'s explosion: its camera shake,
/// the largest of its amplitudes and frequencies.
fn kick(weapons: &Value, name: &str) -> Option<Value> {
    let id = id_of(weapons, "ProjectileData", name)?;
    let effect = weapons["projectiles"][&id]["explosion"]["effect"].as_str()?;
    let shake = &weapons["explosions"][effect.to_ascii_lowercase()]["shake"];
    let most = |key: &str| {
        shake[key]
            .as_array()?
            .iter()
            .filter_map(Value::as_f64)
            .reduce(f64::max)
    };
    Some(json!({
        "amplitude": most("amplitude")?.clamp(0.0, 1.0),
        "frequency": most("frequency")?.clamp(0.1, 30.0),
        "seconds": shake["seconds"].as_f64()?.clamp(0.05, 2.0),
    }))
}

/// A sound by datablock name: the import's own id when it imported one of
/// that name, else the name (the base game's, or another Add-On's).
fn sound_ref(weapons: &Value, name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    weapons["sounds"]
        .as_object()
        .and_then(|m| {
            m.keys()
                .find(|id| id.rsplit_once('/').is_some_and(|(_, n)| n == lower))
        })
        .cloned()
        .unwrap_or_else(|| name.to_owned())
}

/// A projectile by datablock name: the import's own id, else the name
/// (the runtime finds the base game's and other Add-Ons' by name).
fn projectile_ref(weapons: &Value, name: &str) -> String {
    id_of(weapons, "ProjectileData", name).unwrap_or_else(|| name.to_owned())
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
