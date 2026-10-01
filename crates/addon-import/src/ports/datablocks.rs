//! What a port reads from the imported datablocks themselves, beside the
//! script patterns of `covers`: the magazines a classic ammo system kept in
//! item fields ([`Magazines`]), the hitscans of a raycasting system's image
//! fields ([`Hitscans`]), what each image's own script methods did
//! ([`ScriptRule`]) and tables of datablock fields or top-level calls for
//! its host rules ([`Table`]). All read the imported `weapons.json` (its `definitions`, and
//! the script bodies for [`ScriptRule`]), so they follow each copy's own
//! numbers and names.
use anyhow::{Context, Result, bail, ensure};
use bri_weapons::Definition;
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
    /// The state script that loads one round (`onReloadSingle`): a gun
    /// whose image runs it reloads a round at a time, each taking the
    /// loop of states from that one back to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub one_by_one: Option<String>,
    /// Fields for one item's magazine, by item datablock name
    /// (`"one_by_one": true` for a gun loaded a shell at a time).
    #[serde(default)]
    pub items: BTreeMap<String, Value>,
    /// Fields every gun's magazine gets, before `items` (`light_states`,
    /// a script ammo system's `reload_state` and `checks`).
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub every: serde_json::Map<String, Value>,
    /// Items counted straight from the reserve, with no magazine of their
    /// own (Tier+Tactical's grenades).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counted: Option<Counted>,
}

/// [`Magazines::counted`]: every item with `field` set and an ammo type
/// gets a magazine counted from its reserve (`from_reserve`), each throw
/// taking one, with the type's reserve and display and `every`'s fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counted {
    /// The item field that marks one (`TT_grenade`).
    pub field: String,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub every: serde_json::Map<String, Value>,
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

/// A table for the host rules: in a rules file, `{{name}}` becomes a Rhai
/// map from each row's key to a map of its fields. The rows are the
/// datablocks of a `class` (each listed field it has, its own or
/// inherited), or the calls of a function made outside any function body
/// (`TT_registerAmmoType("9MM", ...)`, each argument named by `fields` in
/// order, `""` to skip one).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    /// The datablock class (`ProjectileData`, `ItemData`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub class: String,
    /// The function whose top-level calls are the rows, in place of `class`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub call: String,
    /// Fields to read, as the script spells them; with `call`, a name for
    /// each argument.
    pub fields: Vec<String>,
    /// Only rows where each of these fields is set and not false or 0.
    #[serde(default)]
    pub when: Vec<String>,
    /// What keys the table: `id` (the imported id, such as
    /// `<ns>:weapon/ammoitem`), `name` (the datablock's name) or
    /// `damage_type` (a projectile's damage type as `on_damage` names it);
    /// with `call`, one of `fields`.
    pub key: String,
}

/// The imported datablocks, with inheritance (`datablock A(x : B)`), a
/// parent the Add-On does not define read from the ones it depends on.
pub struct Datablocks<'a> {
    doc: &'a Value,
    by_name: BTreeMap<String, &'a Value>,
    reference: &'a BTreeMap<String, Definition>,
}

impl<'a> Datablocks<'a> {
    pub fn new(weapons: &'a Value, code: &'a super::Code) -> Self {
        let by_name = weapons["definitions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| Some((d["name"].as_str()?.to_ascii_lowercase(), d)))
            .collect();
        Self {
            doc: weapons,
            by_name,
            reference: &code.reference,
        }
    }

    /// `field` of the datablock `name`, its own or its nearest parent's, as
    /// written (string literals keep their quotes).
    pub(super) fn raw(&self, name: &str, field: &str) -> Option<&'a str> {
        let field = field.to_ascii_lowercase();
        let mut name = name.to_ascii_lowercase();
        for _ in 0..16 {
            let (value, parent) = match self.by_name.get(&name) {
                Some(d) => (d["fields"][&field].as_str(), d["parent"].as_str()),
                None => {
                    let d = self.reference.get(&name)?;
                    (
                        d.fields.get(&field).map(String::as_str),
                        d.parent.as_deref(),
                    )
                }
            };
            if value.is_some() {
                return value;
            }
            name = parent?.to_ascii_lowercase();
        }
        None
    }

    /// `field` as its value: the text of a string literal, else as written.
    pub(super) fn field(&self, name: &str, field: &str) -> Option<&'a str> {
        self.raw(name, field).map(crate::literal)
    }

    pub(super) fn of_class<'b>(&'b self, class: &'b str) -> impl Iterator<Item = &'a Value> + 'b {
        self.by_name.values().copied().filter(move |d| {
            d["class"]
                .as_str()
                .is_some_and(|c| c.eq_ignore_ascii_case(class))
        })
    }
}

pub(super) fn set(v: Option<&str>) -> bool {
    v.is_some_and(|v| {
        let v = v.trim();
        !(v.is_empty() || v == "0" || v.eq_ignore_ascii_case("false"))
    })
}

/// The `weapons.json` patch giving each ammo-system gun's image its
/// magazine, and the rules' values `magazine_items` (item id to engine ammo
/// name) and `magazine_types` (the item's type name to its numbers).
pub fn magazines(
    m: &Magazines,
    weapons: &Value,
    code: &super::Code,
    handled: &mut super::Handled,
) -> Result<(Value, BTreeMap<String, String>)> {
    let blocks = Datablocks::new(weapons, code);
    let mut images = serde_json::Map::new();
    let mut items = BTreeMap::new();
    for (id, item) in weapons["items"].as_object().into_iter().flatten() {
        let name = item["name"].as_str().unwrap_or_default();
        let counted = m
            .counted
            .as_ref()
            .filter(|c| set(blocks.field(name, &c.field)));
        let size = match counted {
            Some(_) => Some("1"),
            None => blocks.field(name, &m.size),
        };
        let (Some(size), Some(kind)) = (size, blocks.field(name, &m.ammo)) else {
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
        let single = m
            .one_by_one
            .as_deref()
            .filter(|_| counted.is_none())
            .and_then(|s| round_ticks(image, s));
        let mut magazine = json!({
            "size": size,
            "ammo": ty.ammo,
            "reload_ticks": single.or_else(|| reload_ticks(image)).unwrap_or(m.reload_ticks),
            "reserve": ty.reserve,
            "max_reserve": ty.max_reserve,
            "display": if ty.display.is_empty() { kind } else { ty.display.as_str() },
        });
        if single.is_some() {
            magazine["one_by_one"] = json!(true);
        }
        if let Some(counted) = counted {
            magazine["reload_ticks"] = json!(m.reload_ticks);
            magazine["from_reserve"] = json!(true);
            super::merge(&mut magazine, &Value::Object(counted.every.clone()));
        } else {
            super::merge(&mut magazine, &Value::Object(m.every.clone()));
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
        // A counted item's state scripts that ask whether any are left, or
        // take one, are what its magazine does.
        if counted.is_some() {
            let owner = image["name"].as_str().unwrap_or_default();
            for script in image["states"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| s["script"].as_str())
            {
                let what = format!(
                    "{}::{}",
                    owner.to_ascii_lowercase(),
                    script.to_ascii_lowercase()
                );
                let body = code.bodies.get(&what).map(|b| b.to_ascii_lowercase());
                if body
                    .is_some_and(|b| b.contains("tt_needsammo") || b.contains("tt_decrementammo"))
                {
                    super::handle(
                        handled,
                        &what,
                        "counted from the reserve (engine from_reserve): a throw takes one, and with none left it leaves the hand until more arrive",
                    );
                }
            }
        }
    }
    // An image another script mounts in the gun's place (a second fire
    // mode) loads from the gun's magazine: the system keeps its rounds on
    // the image's `item`, as the gun's own. A left hand's image (another
    // mount point) already fires from the right gun's magazine.
    for (image_id, image) in weapons["images"].as_object().into_iter().flatten() {
        if images.contains_key(image_id) || image["mount_point"].as_u64().unwrap_or(0) != 0 {
            continue;
        }
        let name = image["name"].as_str().unwrap_or_default();
        let Some(item) = blocks.field(name, "item") else {
            continue;
        };
        let item = crate::literal(item).trim();
        let gun = weapons["items"]
            .as_object()
            .into_iter()
            .flatten()
            .find(|(id, i)| {
                i["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(item))
                    && items.contains_key(*id)
            })
            .and_then(|(_, i)| images.get(i["image"].as_str()?));
        if let Some(gun) = gun.cloned() {
            images.insert(image_id.clone(), gun);
        }
    }
    // A gun's own reload, check and dry-pull scripts that only work its
    // magazine (`TT_reload`, `setImageLoaded`, `TT_displayAmmo`) are what
    // the magazine's states run.
    for (image_id, image) in &images {
        let magazine = &image["magazine"];
        let name = weapons["images"][image_id]["name"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let scripts = magazine["reload_state"]
            .as_str()
            .into_iter()
            .chain(["onFire"])
            .chain(
                magazine["checks"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(k, _)| k.as_str()),
            )
            .chain(
                magazine["display_scripts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str),
            );
        for script in scripts {
            let what = format!("{name}::{}", script.to_ascii_lowercase());
            if code
                .bodies
                .get(&what)
                .is_some_and(|b| only_magazine(b, script))
            {
                super::handle(
                    handled,
                    &what,
                    "its magazine's state: the engine moves the rounds, sets the flags and shows the ammo",
                );
            }
        }
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

/// Whether a gun's script body only works its magazine: every call it
/// makes is one the magazine's states carry out, and it plays no sound or
/// arm move of its own (those are read as the state's own). An `onFire`
/// may also take its rounds and fire the image's own shot.
fn only_magazine(body: &str, script: &str) -> bool {
    const MAGAZINE: [&str; 6] = [
        "tt_reload",
        "tt_incrementreload",
        "tt_displayammo",
        "setimageloaded",
        "setimageammo",
        "getdamagepercent",
    ];
    let call = regex::Regex::new(r"([A-Za-z_][A-Za-z0-9_]*)\s*\(").expect("call pattern");
    let calls: Vec<_> = call
        .captures_iter(body)
        .map(|c| c[1].to_ascii_lowercase())
        .filter(|c| !matches!(c.as_str(), "if" | "while" | "for" | "return"))
        .collect();
    let sounded = regex::Regex::new(r"(?i)tt_(reload|incrementreload)\s*\(\s*%obj\s*,\s*%slot\s*,")
        .expect("sound pattern");
    let fire = script.eq_ignore_ascii_case("onFire");
    !calls.is_empty()
        && calls.iter().all(|c| {
            MAGAZINE.contains(&c.as_str())
                || fire && matches!(c.as_str(), "tt_decrementammo" | "onfire")
        })
        && (!fire || calls.iter().any(|c| c == "onfire"))
        && !sounded.is_match(body)
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

/// What one script method did, read from its body for every image (or
/// projectile) of the import that has it: a pattern whose named groups fill
/// `set`, a JSON merge patch for the image, its `shot` or `magazine`, each
/// of its states running the method, the projectile, or a row of a table
/// for the host rules. In `set`, a string that is exactly `{group}` becomes
/// the group's value (a number when it reads as one), `{group|kick}` the
/// view kick of the projectile it names (its explosion's camera shake),
/// `{group|sound}` the sound it names, `{group|projectile}` the projectile
/// and `{group|image}` the image of this import, `{group|explosion}` the
/// explosion effect of the projectile it names, `{group|neg}` the number
/// negated, `{group|ticks}` milliseconds as ticks; before any of those,
/// `field` reads the datablock's field the group names
/// (`%obj.TT_ammoPickup[0]`'s value) and `word<N>` takes its Nth word,
/// from 0 (`getWord`): `{f|field|word1}`; `text` keeps a value that reads
/// as a number a string. `{=text}` starts from `text` itself in place of a
/// group: `{=PrjLoop_tickTime|field|ticks}` reads that field of the
/// datablock whose method matched. `{group}` inside a longer string
/// becomes its text.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRule {
    /// Whose method: `image` (the default), `projectile` or `item`.
    #[serde(default = "image_owner")]
    pub on: String,
    /// The method (`onFire`, `damage`), several as `onFire|onFire2`; `*`
    /// is every state script of an image (`*|onMount` those and onMount).
    pub method: String,
    /// For an image: `image`, `shot` (the shot the method fires: `onFire`'s
    /// is the image's `shot`, another state script's its entry in
    /// `state_shots`), `magazine`, `check` (the magazine's check for the
    /// state script: a gun's own `TT_onLoadCheck` or burst check) or
    /// `state`; for a projectile: `projectile`; for an item: `item`. Any
    /// may go into `table`.
    pub into: String,
    /// With `into: "table"`: the table's name, which the host rules use as
    /// `{{name}}`, a Rhai map from each image's or projectile's id to `set`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub table: String,
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

impl ScriptRule {
    /// What it reads, for the import report: where it puts what it read.
    fn reads(&self) -> String {
        let keys = self
            .set
            .as_object()
            .map(|m| m.keys().cloned().collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
        match self.into.as_str() {
            "table" => format!("script rule: the host rules' {} table ({keys})", self.table),
            into => format!("script rule: its {into}'s {keys}"),
        }
    }
}

fn image_owner() -> String {
    "image".to_owned()
}

/// What a port's script rules read: the `weapons.json` patch, and the
/// tables they fill for the host rules (rendered as Rhai maps).
pub struct ScriptReads {
    pub patch: Value,
    pub tables: BTreeMap<String, String>,
}

/// [`ScriptRule`]s applied to the import's script bodies (lowercase
/// `owner::method` to body).
pub fn scripts(
    rules: &[ScriptRule],
    weapons: &Value,
    code: &super::Code,
    handled: &mut super::Handled,
) -> Result<ScriptReads> {
    let bodies = &code.bodies;
    let blocks = Datablocks::new(weapons, code);
    let cx = |owner| Fill {
        weapons,
        code,
        blocks: &blocks,
        owner,
    };
    let mut images = serde_json::Map::new();
    let mut sections: BTreeMap<&str, serde_json::Map<String, Value>> = BTreeMap::new();
    let mut tables: BTreeMap<String, serde_json::Map<String, Value>> = BTreeMap::new();
    for rule in rules {
        let re = super::pattern(&rule.pattern).context("a script rule's pattern")?;
        let required = rule
            .required_by
            .as_deref()
            .map(super::pattern)
            .transpose()
            .context("a script rule's required_by")?;
        if rule.into == "table" {
            // Every table a rule names exists, empty when nothing matched,
            // so the rules can always read it.
            tables.entry(rule.table.clone()).or_default();
        }
        // The pack section of a projectile's or an item's methods.
        let section = match rule.on.as_str() {
            "image" => None,
            "projectile" => Some("projectiles"),
            "item" => Some("items"),
            other => bail!("a script rule is on `{other}`, not image, projectile or item"),
        };
        let targets: &[&str] = if section.is_none() {
            &["image", "shot", "magazine", "check", "state", "table"]
        } else {
            &[rule.on.as_str(), "table"]
        };
        ensure!(
            targets.contains(&rule.into.as_str()),
            "a script rule on {} goes into `{}`, not {}",
            rule.on,
            rule.into,
            targets.join(", ")
        );
        ensure!(
            (rule.into == "table") != rule.table.is_empty(),
            "a script rule names a table exactly when it goes into one"
        );
        ensure!(
            rule.table.len() <= 64
                && rule
                    .table
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "a script rule's table `{}` is not a plain name",
            rule.table
        );
        if let Some(section) = section {
            ensure!(
                !rule.method.split('|').any(|m| m == "*"),
                "a {}'s script rule names its method",
                rule.on
            );
            for method in rule.method.split('|').map(str::to_ascii_lowercase) {
                for (id, datablock) in weapons[section].as_object().into_iter().flatten() {
                    let name = datablock["name"].as_str().unwrap_or_default();
                    let Some(body) =
                        bodies.get(&format!("{}::{method}", name.to_ascii_lowercase()))
                    else {
                        continue;
                    };
                    let Some(values) = groups(&re, required.as_ref(), body, name, &method)? else {
                        continue;
                    };
                    let set = fill(&rule.set, &values, &cx(name))
                        .with_context(|| format!("{name}::{method}"))?;
                    super::handle(handled, &format!("{name}::{method}"), &rule.reads());
                    let into = if rule.into == "table" {
                        tables.entry(rule.table.clone()).or_default()
                    } else {
                        sections.entry(section).or_default()
                    };
                    super::compose(into.entry(id.clone()).or_insert_with(|| json!({})), &set);
                }
            }
            continue;
        }
        for (id, image) in weapons["images"].as_object().into_iter().flatten() {
            let name = image["name"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase();
            let states = image["states"].as_array().cloned().unwrap_or_default();
            let mut methods: Vec<String> = vec![];
            for part in rule.method.split('|') {
                if part == "*" {
                    methods.extend(
                        states
                            .iter()
                            .filter_map(|s| s["script"].as_str())
                            .filter(|s| !s.is_empty())
                            .map(str::to_ascii_lowercase),
                    );
                } else {
                    methods.push(part.to_ascii_lowercase());
                }
            }
            methods.sort();
            methods.dedup();
            for method in methods {
                let Some(body) = bodies.get(&format!("{name}::{method}")) else {
                    continue;
                };
                let owner = image["name"].as_str().unwrap_or_default();
                let Some(values) = groups(&re, required.as_ref(), body, owner, &method)? else {
                    continue;
                };
                let set = fill(&rule.set, &values, &cx(owner))
                    .with_context(|| format!("{name}::{method}"))?;
                super::handle(handled, &format!("{owner}::{method}"), &rule.reads());
                if rule.into == "table" {
                    // A row with a field that names nothing is left out.
                    if set
                        .as_object()
                        .is_some_and(|m| m.values().any(Value::is_null))
                    {
                        continue;
                    }
                    let table = tables.entry(rule.table.clone()).or_default();
                    super::merge(table.entry(id.clone()).or_insert_with(|| json!({})), &set);
                    continue;
                }
                let entry = images.entry(id.clone()).or_insert_with(|| json!({}));
                match rule.into.as_str() {
                    "image" => super::compose(entry, &set),
                    "shot" if method == "onfire" => super::compose(entry, &json!({ "shot": set })),
                    "shot" => super::compose(entry, &json!({ "state_shots": { &method: set } })),
                    "magazine" => super::compose(entry, &json!({ "magazine": set })),
                    "check" => {
                        // Keyed by the script as the states spell it, so it
                        // merges with a shared check of that name.
                        let script = states
                            .iter()
                            .filter_map(|s| s["script"].as_str())
                            .find(|s| s.eq_ignore_ascii_case(&method))
                            .unwrap_or(&method)
                            .to_owned();
                        super::compose(
                            entry,
                            &json!({ "magazine": { "checks": { script: set } } }),
                        );
                    }
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
                            super::compose(state, &set);
                        }
                    }
                }
            }
        }
    }
    let mut patch = json!({ "images": images });
    for (section, rows) in sections {
        patch[section] = Value::Object(rows);
    }
    Ok(ScriptReads {
        patch,
        tables: tables
            .into_iter()
            .map(|(name, rows)| (name, rhai(&Value::Object(rows))))
            .collect(),
    })
}

/// [`ScriptRule::set`] with its groups' values.
/// What [`fill`] reads besides the groups: the pack, the code, and the
/// datablock whose method matched, for `field`.
struct Fill<'a> {
    weapons: &'a Value,
    code: &'a super::Code,
    blocks: &'a Datablocks<'a>,
    owner: &'a str,
}

fn fill(v: &Value, values: &BTreeMap<String, String>, cx: &Fill) -> Result<Value> {
    let (weapons, code) = (cx.weapons, cx.code);
    Ok(match v {
        Value::String(s) if s.starts_with('{') && s.ends_with('}') && !s[1..].contains('{') => {
            let inner = &s[1..s.len() - 1];
            let mut filters = inner.split('|');
            let group = filters.next().unwrap_or_default();
            // `{=PrjLoop_tickTime|field}`: the text itself, not a group's.
            let mut value = match group.strip_prefix('=') {
                Some(text) => text.to_owned(),
                None => values
                    .get(group)
                    .with_context(|| format!("`{s}`: the pattern has no group `{group}`"))?
                    .clone(),
            };
            let mut filter = "";
            for f in filters {
                ensure!(filter.is_empty(), "`{s}`: `{filter}` comes last");
                if f == "field" {
                    // `%obj.TT_ammoPickup[0]`: the field the object has
                    // from its datablock.
                    let name = value.trim();
                    let name = name.rsplit_once('.').map_or(name, |(_, f)| f);
                    value = cx
                        .blocks
                        .field(cx.owner, name)
                        .with_context(|| format!("`{s}`: {} has no `{name}`", cx.owner))?
                        .to_owned();
                } else if let Some(n) = f.strip_prefix("word").and_then(|n| n.parse().ok()) {
                    value = value
                        .split_whitespace()
                        .nth(n)
                        .with_context(|| format!("`{s}`: `{value}` has no word {n}"))?
                        .to_owned();
                } else {
                    filter = f;
                }
            }
            let value = &value;
            match filter {
                "" => value_of(value),
                // As text even when it reads as a number (a label).
                "text" => json!(value),
                // A push the script wrote as negative (`TT_knockback(%obj,
                // -4, ...)`), as the speed taken off.
                "neg" => {
                    let n: f64 = value
                        .trim()
                        .parse()
                        .with_context(|| format!("`{s}`: `{value}` is no number"))?;
                    // Never -0, which reads as a push.
                    json!(-n + 0.0)
                }
                // Milliseconds (`getSimTime()` differences) as ticks, 120 a
                // second.
                "ticks" => {
                    let ms: f64 = value
                        .trim()
                        .parse()
                        .with_context(|| format!("`{s}`: `{value}` is no number"))?;
                    json!((ms * 0.12).round() as i64)
                }
                "kick" => kick(weapons, value)
                    .or_else(|| dependency_kick(&code.reference, value))
                    .with_context(|| format!("`{value}` is no projectile with a camera shake"))?,
                "sound" => json!(sound_ref(weapons, value)),
                "projectile" => json!(projectile_ref(weapons, value)),
                // The explosion effect of the projectile it names
                // (`spawnExplosion(tierFirePlayerProjectile, ...)`).
                "explosion" => {
                    let id = id_of(weapons, "ProjectileData", value)
                        .with_context(|| format!("`{value}` is no projectile of this import"))?;
                    let effect = weapons["projectiles"][&id]["explosion"]["effect"]
                        .as_str()
                        .filter(|e| !e.is_empty())
                        .with_context(|| format!("`{value}` has no explosion"))?;
                    json!(effect)
                }
                "image" => json!(
                    id_of(weapons, "ShapeBaseImageData", value)
                        .with_context(|| format!("`{value}` is no image of this import"))?
                ),
                // A player type (`pushDatablock(LMGArmor)`) as its archetype;
                // null when it is none of this import's, its dependencies'
                // or another reference Add-On's, as `LMGArmor.getID()` then
                // found nothing (a table leaves that row out).
                "archetype" => code
                    .archetypes
                    .get(&value.trim().to_ascii_lowercase())
                    .map_or(Value::Null, |id| json!(id)),
                other => {
                    bail!(
                        "`{s}`: no filter `{other}` (field, word<N>, text, neg, ticks, kick, sound, projectile, explosion, image or archetype)"
                    )
                }
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
                .map(|(k, v)| Ok((k.clone(), fill(v, values, cx)?)))
                .collect::<Result<_>>()?,
        ),
        Value::Array(a) => Value::Array(
            a.iter()
                .map(|v| fill(v, values, cx))
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

/// The view kick of the projectile `name`'s explosion (a recoil blast):
/// its camera shake, the largest of its amplitudes, the mean of its
/// frequencies, and
/// its radius, within which other players feel it too. None when it does
/// not shake.
pub(super) fn kick(weapons: &Value, name: &str) -> Option<Value> {
    let id = id_of(weapons, "ProjectileData", name)?;
    let effect = weapons["projectiles"][&id]["explosion"]["effect"].as_str()?;
    let (_, explosion) = weapons["explosions"]
        .as_object()?
        .iter()
        .find(|(id, _)| id.eq_ignore_ascii_case(effect))?;
    let shake = &explosion["shake"];
    let most = |key: &str| {
        shake[key]
            .as_array()?
            .iter()
            .filter_map(Value::as_f64)
            .reduce(f64::max)
    };
    let amplitude = most("amplitude")?;
    let frequency: Vec<f64> = shake["frequency"]
        .as_array()?
        .iter()
        .filter_map(Value::as_f64)
        .collect();
    let seconds = shake["seconds"].as_f64()?;
    if amplitude <= 0.0 || frequency.is_empty() || seconds <= 0.0 {
        return None;
    }
    let frequency = frequency.iter().sum::<f64>() / frequency.len() as f64;
    Some(json!({
        "amplitude": amplitude.min(1.0),
        "frequency": frequency.clamp(0.1, 30.0),
        "seconds": seconds.clamp(0.05, 2.0),
        "radius": shake["radius"].as_f64().unwrap_or(0.0).clamp(0.0, 100.0),
    }))
}

/// [`kick`] of a projectile another Add-On (or the base game) defines: it,
/// its explosion and their parents lowered as the import lowers its own.
fn dependency_kick(reference: &BTreeMap<String, Definition>, name: &str) -> Option<Value> {
    let mut defs: Vec<Definition> = vec![];
    let mut want = vec![name.to_ascii_lowercase()];
    while let Some(next) = want.pop() {
        if defs.iter().any(|d| d.name.eq_ignore_ascii_case(&next)) {
            continue;
        }
        let Some(d) = reference.get(&next) else {
            continue;
        };
        want.extend(d.parent.iter().map(|p| p.to_ascii_lowercase()));
        if let Some(e) = d.fields.get("explosion") {
            want.push(crate::literal(e).trim().to_ascii_lowercase());
        }
        defs.push(d.clone());
    }
    let pack = bri_weapons_import::lower(defs).ok()?;
    kick(&serde_json::to_value(pack).ok()?, name)
}

/// A sound by datablock name: the import's own id when it imported one of
/// that name, else the name (the base game's, or another Add-On's).
pub(super) fn sound_ref(weapons: &Value, name: &str) -> String {
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
pub(super) fn projectile_ref(weapons: &Value, name: &str) -> String {
    id_of(weapons, "ProjectileData", name).unwrap_or_else(|| name.to_owned())
}

/// How long one round of a one-at-a-time reload takes: the loop of states
/// from the one running `script` along their timeouts back to it.
fn round_ticks(image: &Value, script: &str) -> Option<u32> {
    let states = image["states"].as_array()?;
    let start = states.iter().position(|s| {
        s["script"]
            .as_str()
            .is_some_and(|n| n.eq_ignore_ascii_case(script))
    })?;
    let (mut at, mut ticks) = (start, 0u64);
    for _ in 0..states.len() {
        let s = states.get(at)?;
        ticks += s["ticks"].as_u64().unwrap_or(0);
        at = s["timeout"].as_u64()? as usize;
        if at == start {
            return (1..=1200).contains(&ticks).then_some(ticks as u32);
        }
    }
    None
}

/// A port's tables, as Rhai map literals by table name.
pub fn tables(
    tables: &BTreeMap<String, Table>,
    weapons: &Value,
    code: &super::Code,
    handled: &mut super::Handled,
) -> Result<BTreeMap<String, String>> {
    let blocks = Datablocks::new(weapons, code);
    let calls = &code.calls;
    let mut out = BTreeMap::new();
    for (name, t) in tables {
        ensure!(
            t.class.is_empty() != t.call.is_empty(),
            "table {name}: give a `class` or a `call`"
        );
        let mut rows = serde_json::Map::new();
        if !t.call.is_empty() {
            ensure!(
                t.fields.contains(&t.key) && !t.key.is_empty(),
                "table {name}: key `{}` names none of its fields",
                t.key
            );
            for c in calls
                .iter()
                .filter(|c| c.receiver.is_none() && c.callee.eq_ignore_ascii_case(&t.call))
            {
                let arg = |field: &str| {
                    let i = t.fields.iter().position(|f| f == field)?;
                    c.args.get(i).map(String::as_str)
                };
                if !t.when.iter().all(|f| set(arg(f))) {
                    continue;
                }
                let Some(key) = arg(&t.key) else {
                    continue;
                };
                let row = t
                    .fields
                    .iter()
                    .zip(&c.args)
                    .filter(|(f, _)| !f.is_empty())
                    .map(|(f, raw)| (f.to_ascii_lowercase(), value(raw)))
                    .collect();
                rows.insert(crate::literal(key).to_owned(), Value::Object(row));
            }
            super::handle(
                handled,
                &format!("call:{}", t.call),
                &format!("the host rules' {name} table reads each call"),
            );
            out.insert(name.clone(), rhai(&Value::Object(rows)));
            continue;
        }
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
pub(super) fn id_of(weapons: &Value, class: &str, name: &str) -> Option<String> {
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
pub(super) fn value(raw: &str) -> Value {
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
