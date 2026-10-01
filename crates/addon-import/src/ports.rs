//! Native ports of v20 Add-On scripts. `ports/ports.json` lists each v20
//! Add-On (by folder name and content hash) that has a port, with its status;
//! `ports/<port>/` holds the port: JSON merge patches for the files the
//! importer writes, and any files it adds. A port carries only the native
//! rewrite, never the original Add-On's files, so the list ships with the
//! game and each player's own copy supplies the rest. Recipe:
//! `docs/modding/porting.md`.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

mod datablocks;
mod settings;
mod shots;
pub use datablocks::{AmmoType, Magazines, ScriptRule, Table};
pub use settings::{FieldSpec, WeaponSetting};
pub use shots::{Hitscans, Last, Shots, TracerField};

mod builtin {
    include!(concat!(env!("OUT_DIR"), "/ports.rs"));
}

pub const LIST_SCHEMA: u32 = 1;
const STATUSES: &[&str] = &["verified", "partial"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct List {
    pub schema_version: u32,
    pub ports: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The v20 Add-On's folder or zip name (`Weapon_Shotgun`).
    pub addon: String,
    pub title: String,
    /// The folder under `ports/` holding the port.
    pub port: String,
    /// `verified`: its tests show it behaves like v20 for everything it
    /// covers. `partial`: some of the Add-On's behaviour is still missing.
    pub status: String,
    /// Source hashes (`import-report.json` `source.sha256`) of copies the
    /// port was checked against. Other copies still get the port when every
    /// function it covers matches.
    #[serde(default)]
    pub sha256: Vec<String>,
    /// Each script function the port replaces (or top-level `$global`,
    /// read as its value), with named patterns its body must match
    /// (case-insensitive). A pattern's first group, when it has one, is the
    /// value the port's patches use as `{name}`. A key that is a script's
    /// path in the Add-On (`server.cs`) matches that file's whole text, for
    /// values it sets outside any function.
    pub covers: BTreeMap<String, BTreeMap<String, String>>,
    /// Tests that prove the port, `path name`.
    #[serde(default)]
    pub tests: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Port {
    pub schema_version: u32,
    #[serde(default)]
    pub notes: String,
    /// Package-relative JSON file to an RFC 7396 merge patch. A string value
    /// that is exactly `{name}` becomes that captured value (a number when it
    /// reads as one); `{name}` inside a longer string becomes its text.
    #[serde(default)]
    pub patch: BTreeMap<String, Value>,
    /// Host rules the Add-On's scripts ran (server commands, an image's
    /// `onFire` that edits the world). The files under `ports/<port>/rules/`
    /// become a companion Add-On beside the import, which only the host
    /// loads: an imported Add-On's items and bricks are shared with every
    /// player, and a shared Add-On cannot carry host code.
    #[serde(default)]
    pub rules: Option<Rules>,
    /// Magazines a classic ammo system kept in item fields, given to each
    /// gun's image (`docs/modding/porting.md`, "Magazines from item fields").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magazines: Option<Magazines>,
    /// Shots read from the images' `onFire` written with v20's spread code
    /// (`docs/modding/porting.md`, "Shots from the guns' scripts").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shots: Option<Shots>,
    /// Hitscan guns read from their image fields (`docs/modding/porting.md`,
    /// "Hitscan guns from image fields").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hitscans: Option<Hitscans>,
    /// What each image's own script methods did, read from their bodies
    /// (`docs/modding/porting.md`, "Script rules").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<ScriptRule>,
    /// Shared parts several ports use, by name: `ports/_shared/<name>.json`
    /// holds any of this file's fields (an Add-On family's ammo system and
    /// script rules, written once). They apply first: the port's own
    /// fields merge over them, and its `scripts` follow theirs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
    /// Files the port adds to the import (`ports/<port>/files/`) that are
    /// content the import provides, to their kind (`binds.json` →
    /// `binds`). `{{name}}` in them is filled in, as in the rules.
    #[serde(default)]
    pub provides: BTreeMap<String, String>,
    /// What the port carries out that no reader reads from the copy: a
    /// function (`WeaponImage::TT_canFire`) or a top-level call
    /// (`call:TT_registerAmmoType`) to how the game does it now (an engine
    /// seam, the port's rules). The import report counts it as ported when
    /// the copy has it. Say only what the game really does. An RTB
    /// preference the game carries out with no setting (a bug fix it always
    /// makes) is `pref:$Pref::Server::TT::X`. Other findings
    /// the importer could not convert are named as the report spells them,
    /// by kind: `file:client.cs`, `new:ScriptGroup` (an object made at
    /// load), `set:MessageBoxYesNoDlg.yesCallBack` (an object changed at
    /// load) and `datablock:ND_SelectionBoxOuter`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub handles: BTreeMap<String, String>,
    /// Datablocks of the Add-On its `datablocks.cs` declares in their
    /// place: ones the Add-On makes at run time under a name the importer
    /// cannot read (Slayer's countdown voices, one `slayerSound` renamed in
    /// a loop). The report counts them as the port's.
    #[serde(default)]
    pub replaces: Vec<String>,
}

/// What a port accounts for, by lower-case function (`pistolimage::onfire`)
/// or top-level call (`call:tt_registerammotype`): how each is carried out,
/// from its readers and its [`Port::handles`].
pub type Handled = BTreeMap<String, BTreeSet<String>>;

/// Records that `how` carries out `what` ([`Handled`]).
pub(crate) fn handle(handled: &mut Handled, what: &str, how: &str) {
    handled
        .entry(what.to_ascii_lowercase())
        .or_default()
        .insert(how.to_owned());
}

/// Where [`Port::include`] files live.
pub const SHARED_DIR: &str = "_shared";

/// The companion host-rules Add-On a port adds ([`Port::rules`]). Its id is
/// the import's with `-rules` added ([`rules_id`]), its folder the import's
/// with `-rules` added, and the import names it in `companions`, so turning
/// the import on or off turns its rules with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// What the rules may do (`player`, `world.edit`, `chat`, ...), as in any
    /// Add-On's `package.json`; none for rules that only declare settings.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Tables of datablock fields, by the `{{name}}` the rules use them as.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tables: BTreeMap<String, Table>,
    /// Constants by the `{{name}}` the rules use them as, for rules shared
    /// by Add-Ons that differ in small ways.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, Value>,
    /// Another port whose `rules/` these are, when two Add-Ons share one
    /// ruleset (two releases of a pack); this port then has no `rules/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Other Add-Ons (by v20 folder name, `Emote_Critical`) whose content
    /// the rules use while their imports are on too, as the scripts tested
    /// `isObject` on their datablocks: each is an optional dependency, and
    /// `{uses:Emote_Critical}` in a value is its import's id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<String>,
    /// Other Add-Ons the scripts loaded themselves when they were there
    /// (`exec("add-ons/Emote_Critical/server.cs")`): optional dependencies
    /// the rules also name as companions, so turning the Add-On on turns
    /// each installed one on with it, and the Add-On runs without one that
    /// is missing, as the `exec` of a missing file did nothing. `{uses:X}`
    /// names its id as for `uses`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loads: Vec<String>,
    /// Other Add-Ons' host rules these rules build on, by the name rules
    /// files use for them: `{"slayer_rules": "Gamemode_Slayer"}` makes the
    /// rules depend on Gamemode_Slayer's rules and `{{slayer_rules}}` their
    /// id, as the importer names them (Slayer CTF reads Slayer's settings).
    #[serde(default)]
    pub needs: BTreeMap<String, String>,
    /// The copy's RTB server preferences the rules read with `pref(name)`,
    /// by their global (`$Pref::Server::TT::MedicHealBots`; a trailing `*`
    /// matches the rest, `$Pref::Server::TT::Start*`), to how the rules
    /// use them. Each becomes a server-wide setting of the rules
    /// ([`crate::rtb`]); the copy's others stay unsupported.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub prefs: BTreeMap<String, String>,
    /// The copy's server preferences that set weapon fields, by their
    /// global: the import's pack binds each field to the preference
    /// ([`bri_weapons::Binding`]), so the weapons follow it as the host
    /// changes it (`docs/modding/porting.md`, "Weapon fields from
    /// settings"). A preference here the copy registers is a setting of
    /// the rules, as for `prefs`; the bindings name its global, so a pack
    /// that leaves the preference to another (Tier 2 to Tier 1) binds its
    /// guns to it all the same.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, WeaponSetting>,
}

impl Rules {
    /// How the rules use the preference `global`, if they read it.
    pub fn pref(&self, global: &str) -> Option<&str> {
        if let Some((_, s)) = self
            .settings
            .iter()
            .find(|(g, _)| g.eq_ignore_ascii_case(global))
        {
            return Some(&s.how);
        }
        let global = global.to_ascii_lowercase();
        self.prefs.iter().find_map(|(pattern, how)| {
            let pattern = pattern.to_ascii_lowercase();
            let hit = match pattern.strip_suffix('*') {
                Some(prefix) => global.starts_with(prefix),
                None => global == pattern,
            };
            hit.then_some(how.as_str())
        })
    }
}

/// The companion host-rules Add-On's id for the import `namespace`.
pub fn rules_id(namespace: &str) -> String {
    format!("{namespace}-rules")
}

/// The companion host-rules Add-On's folder for the import in `out`.
pub fn rules_dir(out: &Path) -> std::path::PathBuf {
    let mut name = out.file_name().unwrap_or_default().to_os_string();
    name.push("-rules");
    out.with_file_name(name)
}

/// The ports list and every port's files.
#[derive(Debug, Clone)]
pub struct Ports {
    pub list: List,
    /// `port/relative/path` to bytes.
    files: BTreeMap<String, Vec<u8>>,
}

impl Ports {
    /// The ports this importer was built with.
    pub fn builtin() -> Self {
        Self::from_files(
            builtin::FILES
                .iter()
                .map(|(p, b)| ((*p).to_owned(), b.to_vec()))
                .collect(),
        )
        .expect("crates/addon-import/ports is valid; tests/ports.rs checks it")
    }

    /// No ports: the plain import.
    pub fn empty() -> Self {
        Self {
            list: List {
                schema_version: LIST_SCHEMA,
                ports: vec![],
            },
            files: BTreeMap::new(),
        }
    }

    /// One port being written: its list entry and its folder.
    pub fn single(entry: Entry, port_dir: &Path) -> Result<Self> {
        let mut files = BTreeMap::new();
        let list = List {
            schema_version: LIST_SCHEMA,
            ports: vec![entry],
        };
        files.insert("ports.json".to_owned(), serde_json::to_vec(&list)?);
        let port = &list.ports[0].port;
        let mut stack = vec![port_dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d)
                .with_context(|| format!("reading {}", d.display()))?
                .flatten()
            {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path
                        .strip_prefix(port_dir)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    files.insert(format!("{port}/{rel}"), std::fs::read(&path)?);
                }
            }
        }
        Self::from_files(files)
    }

    /// A ports folder laid out like `crates/addon-import/ports`.
    pub fn from_dir(root: &Path) -> Result<Self> {
        let mut files = BTreeMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d)?.flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path
                        .strip_prefix(root)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    files.insert(rel, std::fs::read(&path)?);
                }
            }
        }
        Self::from_files(files)
    }

    fn from_files(mut files: BTreeMap<String, Vec<u8>>) -> Result<Self> {
        let list: List = serde_json::from_slice(
            &files
                .remove("ports.json")
                .context("ports.json is missing")?,
        )
        .context("ports.json")?;
        ensure!(
            list.schema_version == LIST_SCHEMA,
            "ports.json schema_version"
        );
        let ports = Self { list, files };
        for e in &ports.list.ports {
            ensure!(
                STATUSES.contains(&e.status.as_str()),
                "{}: status must be verified or partial",
                e.addon
            );
            ensure!(!e.covers.is_empty(), "{}: covers nothing", e.addon);
            for h in &e.sha256 {
                ensure!(
                    h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()),
                    "{}: bad sha256 {h}",
                    e.addon
                );
            }
            for patterns in e.covers.values() {
                for p in patterns.values() {
                    pattern(p).with_context(|| e.addon.clone())?;
                }
            }
            ports.port(e).with_context(|| format!("port {}", e.port))?;
        }
        Ok(ports)
    }

    /// A port.json with its [`Port::include`]s applied under it, keeping
    /// its `null`s.
    fn with_includes(&self, port: Value) -> Result<Value> {
        let names: Vec<String> = match port.get("include") {
            None => return Ok(port),
            Some(v) => serde_json::from_value(v.clone()).context("include")?,
        };
        let mut out = Value::Object(Default::default());
        let mut scripts = vec![];
        for name in names.iter().map(String::as_str).chain([""]) {
            let mut part = if name.is_empty() {
                port.clone()
            } else {
                ensure!(
                    !name.contains(['/', '\\', '.']),
                    "include `{name}`: a name, not a path"
                );
                let bytes = self
                    .files
                    .get(&format!("{SHARED_DIR}/{name}.json"))
                    .with_context(|| format!("{SHARED_DIR}/{name}.json is missing"))?;
                serde_json::from_slice(bytes)
                    .with_context(|| format!("{SHARED_DIR}/{name}.json"))?
            };
            if let Some(s) = part.as_object_mut().and_then(|m| m.remove("scripts")) {
                scripts.extend(
                    s.as_array()
                        .cloned()
                        .with_context(|| format!("`scripts` in {name}: a list"))?,
                );
            }
            // A `null` in a port is a value too (an empty rules slot, or a
            // patch removing a field), so it stays.
            compose(&mut out, &part);
        }
        out["scripts"] = Value::Array(scripts);
        Ok(out)
    }

    fn port(&self, e: &Entry) -> Result<Port> {
        let bytes = self
            .files
            .get(&format!("{}/port.json", e.port))
            .with_context(|| format!("{}/port.json is missing", e.port))?;
        let port: Port = serde_json::from_value(self.with_includes(
            serde_json::from_slice(bytes).with_context(|| format!("{}/port.json", e.port))?,
        )?)
        .with_context(|| format!("{}/port.json", e.port))?;
        ensure!(port.schema_version == 1, "port.json schema_version");
        for file in port.patch.keys() {
            safe_relative(file)?;
            ensure!(
                file.ends_with(".json"),
                "{file}: only JSON files are patched"
            );
        }
        ensure!(
            port.replaces.is_empty() || self.files.contains_key(&format!("{}/{DATABLOCKS}", e.port)),
            "replaces datablocks without a {DATABLOCKS} to declare them"
        );
        let files = self.added_files(&e.port);
        for (file, kind) in &port.provides {
            ensure!(
                files.iter().any(|(f, _)| f == file),
                "provides {file}, which is not under files/"
            );
            ensure!(
                bri_package_runtime::content::Kind::parse(kind)
                    .is_some_and(|k| k.side() == bri_package::packages::Side::Client),
                "{file}: an import provides only content players load, not `{kind}`"
            );
        }
        let rules = self.rules_files(&e.port, &port.include, port.rules.as_ref())?;
        if port.rules.as_ref().is_some_and(|r| r.from.is_some()) {
            ensure!(
                self.port_files(&e.port, "rules").is_empty(),
                "rules/ files and rules.from: the rules come from one place"
            );
        }
        match &port.rules {
            Some(_) => {
                ensure!(
                    rules.iter().any(|(f, _)| f == RULES_BEHAVIOUR),
                    "rules/{RULES_BEHAVIOUR} is missing"
                );
                for (file, _) in &rules {
                    safe_relative(file)?;
                    ensure!(
                        file == RULES_BEHAVIOUR
                            || (file.ends_with(".rhai") && !file.contains('/'))
                            || rules_archetype(file).is_some(),
                        "rules/{file}: rules are {RULES_BEHAVIOUR}, .rhai scripts and archetypes/<name>.json"
                    );
                }
            }
            None => ensure!(rules.is_empty(), "rules/ files need `rules` in port.json"),
        }
        Ok(port)
    }

    /// Files a port adds, under `ports/<port>/files/`, by package-relative path.
    fn added_files(&self, port: &str) -> Vec<(String, &[u8])> {
        self.port_files(port, "files")
    }

    /// A port's host-rules files: each included `_shared/<name>/rules/`
    /// in order, then its own `rules/` (or those of the port they come
    /// `from`). A later script replaces an earlier one of the same name; a
    /// later JSON file (`behaviour.json`) is merged over the earlier one,
    /// so a port adds its own commands and hooks to the shared ones.
    fn rules_files(
        &self,
        port: &str,
        include: &[String],
        rules: Option<&Rules>,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        let from = rules.and_then(|r| r.from.as_deref()).unwrap_or(port);
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for dir in include
            .iter()
            .map(|name| format!("{SHARED_DIR}/{name}"))
            .chain([from.to_owned()])
        {
            for (file, bytes) in self.port_files(&dir, "rules") {
                let layered = match files.get(&file) {
                    Some(below) if file.ends_with(".json") => {
                        let mut doc: Value = serde_json::from_slice(below)
                            .with_context(|| format!("rules/{file}"))?;
                        let over: Value = serde_json::from_slice(bytes)
                            .with_context(|| format!("{dir}/rules/{file}"))?;
                        merge(&mut doc, &over);
                        serde_json::to_vec_pretty(&doc)?
                    }
                    _ => bytes.to_vec(),
                };
                files.insert(file, layered);
            }
        }
        Ok(files.into_iter().collect())
    }

    /// The files under `ports/<port>/<folder>/`, by path inside it.
    fn port_files(&self, port: &str, folder: &str) -> Vec<(String, &[u8])> {
        let prefix = format!("{port}/{folder}/");
        self.files
            .iter()
            .filter_map(|(k, v)| Some((k.strip_prefix(&prefix)?.to_owned(), v.as_slice())))
            .collect()
    }

    pub fn find(&self, addon: &str) -> Option<&Entry> {
        self.list
            .ports
            .iter()
            .find(|e| e.addon.eq_ignore_ascii_case(addon))
    }
}

/// The rules' behaviour file; the others are scripts and archetypes.
const RULES_BEHAVIOUR: &str = "behaviour.json";

/// The name of a rules archetype (`archetypes/<name>.json`): a player
/// archetype, or an adjustment to a v20 player type as the Add-On's
/// `PlayerNoJet.maxStepHeight = 1.2;` made.
fn rules_archetype(file: &str) -> Option<&str> {
    let name = file.strip_prefix("archetypes/")?.strip_suffix(".json")?;
    (!name.is_empty() && !name.contains('/')).then_some(name)
}

fn pattern(p: &str) -> Result<regex::Regex> {
    let re = regex::RegexBuilder::new(p)
        .case_insensitive(true)
        .size_limit(1 << 20)
        .build()?;
    Ok(re)
}

fn safe_relative(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && !path.starts_with('/')
            && !path.contains('\\')
            && !path.contains(':')
            && path
                .split('/')
                .all(|s| !s.is_empty() && s != "." && s != ".."),
        "{path}: not a plain package-relative path"
    );
    Ok(())
}

/// What the importer did with the port listed for this Add-On.
#[derive(Debug, Clone, Serialize)]
pub struct Applied {
    pub addon: String,
    pub port: String,
    pub status: String,
    pub applied: bool,
    /// `listed` when this copy's hash is in the list, otherwise `unlisted`.
    pub copy: String,
    pub covers: Vec<String>,
    /// Values read from this copy's script, by pattern name.
    pub values: BTreeMap<String, String>,
    pub files_changed: Vec<String>,
    /// The companion host-rules Add-On the port wrote beside the import.
    pub rules: Option<RulesPackage>,
    /// Why it was not applied.
    pub reason: Option<String>,
    pub notes: String,
    /// What it carries out of the copy's scripts, and how.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub handled: Handled,
    /// The copy's RTB preferences its rules read, by lower-case global, to
    /// the server setting each became and how the rules use it, or how the
    /// game carries out one with no setting (`handles`' `pref:`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub prefs: BTreeMap<String, String>,
    /// The Add-On's datablocks the port declares in their place.
    pub replaces: Vec<String>,
}

/// A companion host-rules Add-On written beside an import.
#[derive(Debug, Clone, Serialize)]
pub struct RulesPackage {
    pub id: String,
    /// Its folder, beside the import's.
    pub dir: String,
    /// Its line for `packages.json` (`side` server: players never get it).
    pub packages_json_entry: Value,
    pub files: Vec<String>,
}

/// What the import is, for a port's patches and rules: `{namespace}` (the
/// import's id), `{rules}` (its rules' id) and `{version}`.
#[derive(Debug, Clone)]
pub struct Import<'a> {
    pub addon: &'a str,
    pub sha256: &'a str,
    pub namespace: &'a str,
    pub version: &'a str,
    pub name: &'a str,
    /// The projectiles of the Add-Ons it depends on, by their packages'
    /// ids: the readers see them beside the import's own (a gun's tracer
    /// from Tier 1), though the import never carries them.
    pub dependencies: &'a BTreeMap<String, bri_weapons::ProjectileDef>,
}

/// Script function bodies by lower-case qualified name, and top-level
/// globals' values by lower-case `$name`.
pub type Bodies = BTreeMap<String, String>;

/// What a port reads of the import's scripts: each function's body, and
/// the calls made outside any function, in the order the scripts load.
#[derive(Debug, Default)]
pub struct Code {
    pub bodies: Bodies,
    /// The [`Code::bodies`] a datablock has only through its `className`
    /// (lower-case `name::method`): the class's own method, judged under
    /// the class's name.
    pub inherited: std::collections::BTreeSet<String>,
    pub calls: Vec<bri_convert::tscript::Call>,
    /// The weapon datablocks of the Add-Ons it depends on and the base
    /// game, by lower-case name: what a script names from them (another
    /// pack's recoil projectile) is read from there.
    pub reference: BTreeMap<String, bri_weapons::Definition>,
    /// The archetype each player type (`PlayerData`) this import or one it
    /// depends on declares became, by lower-case name.
    pub archetypes: BTreeMap<String, String>,
    /// This import's sounds of a base game file, by lower-case name, to the
    /// base sound playing that file (`block_movebrick_sound` to
    /// `clickMoveSound`).
    pub sounds: BTreeMap<String, String>,
}

/// Applies the listed port for `import`, if any, to the package in `out`.
/// All or nothing: a port that does not fit this copy changes no file.
pub fn apply(ports: &Ports, import: &Import, code: &Code, out: &Path) -> Option<Applied> {
    let e = ports.find(import.addon)?;
    let mut applied = Applied {
        addon: e.addon.clone(),
        port: e.port.clone(),
        status: e.status.clone(),
        applied: false,
        copy: if e
            .sha256
            .iter()
            .any(|h| h.eq_ignore_ascii_case(import.sha256))
        {
            "listed"
        } else {
            "unlisted"
        }
        .into(),
        covers: e.covers.keys().cloned().collect(),
        values: BTreeMap::new(),
        files_changed: vec![],
        rules: None,
        reason: None,
        notes: String::new(),
        handled: Handled::new(),
        prefs: BTreeMap::new(),
        replaces: Vec::new(),
    };
    match try_apply(ports, e, import, code, out, &mut applied) {
        Ok(()) => applied.applied = true,
        Err(err) => {
            applied.reason = Some(format!("{err:#}"));
            applied.values.clear();
            applied.handled.clear();
            applied.prefs.clear();
        }
    }
    Some(applied)
}

/// The values `e`'s patterns read from this copy's scripts. Fails when a
/// covered function is missing or does not match.
fn capture(e: &Entry, bodies: &Bodies) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    for (function, patterns) in &e.covers {
        let Some(body) = bodies.get(&function.to_ascii_lowercase()) else {
            bail!("this copy has no `{function}`");
        };
        for (name, p) in patterns {
            let caps = pattern(p)?
                .captures(body)
                .with_context(|| format!("`{function}` does not match the port's `{name}`"))?;
            if let Some(value) = caps.get(1) {
                values.insert(name.clone(), value.as_str().to_owned());
            }
        }
    }
    Ok(values)
}

/// The port's `datablocks.cs` for the Add-On `addon`, with `{{name}}`
/// filled in from this copy's scripts and `{{namespace}}`: datablocks the
/// Add-On makes at run time (in a function or a loop, as Slayer CTF makes
/// its flags), declared as the importer reads them. The importer reads it
/// beside the Add-On's own scripts, in its folder, before converting.
/// `None` when no port is listed or it has no datablocks.
pub fn datablocks(
    ports: &Ports,
    addon: &str,
    namespace: &str,
    bodies: &Bodies,
) -> Option<Result<String>> {
    let e = ports.find(addon)?;
    let bytes = ports.files.get(&format!("{}/{DATABLOCKS}", e.port))?;
    Some((|| {
        let text = port_text(bytes).context("datablocks.cs is not UTF-8 text")?;
        let text = text.as_str();
        let mut values = capture(e, bodies)?;
        values.insert("namespace".to_owned(), namespace.to_owned());
        fill_text(text, &values).context(DATABLOCKS)
    })())
}

/// A port's declarations of datablocks its Add-On makes at run time.
pub const DATABLOCKS: &str = "datablocks.cs";

fn try_apply(
    ports: &Ports,
    e: &Entry,
    import: &Import,
    code: &Code,
    out: &Path,
    applied: &mut Applied,
) -> Result<()> {
    let bodies = &code.bodies;
    applied.values = capture(e, bodies)?;
    let port = ports.port(e)?;
    applied.notes = port.notes.clone();
    for (what, how) in &port.handles {
        handle(&mut applied.handled, what, how);
    }
    applied.replaces = port.replaces.clone();
    // What every port may use besides the values its patterns read.
    let mut values = applied.values.clone();
    for (name, value) in [
        ("namespace", import.namespace.to_owned()),
        ("rules", rules_id(import.namespace)),
        ("version", import.version.to_owned()),
    ] {
        ensure!(
            values.insert(name.to_owned(), value).is_none(),
            "a pattern is named `{name}`, which every port already has"
        );
    }
    if let Some(rules) = &port.rules {
        for (name, addon) in &rules.needs {
            ensure!(
                values
                    .insert(name.clone(), rules_id(&crate::namespace_for(addon)?))
                    .is_none(),
                "the rules need `{name}`, which is already a value"
            );
        }
    }
    for addon in port
        .rules
        .iter()
        .flat_map(|r| r.uses.iter().chain(&r.loads))
    {
        let namespace = crate::namespace_for(addon)?;
        ensure!(
            namespace != import.namespace,
            "the rules use `{addon}`, the Add-On they are for"
        );
        values.insert(format!("uses:{addon}"), namespace);
    }
    // What the port reads from the imported datablocks and scripts: a
    // patch for weapons.json (each reader's fields over the earlier
    // ones'), the definitions it makes, and values for the rules. Tables
    // read the pack as patched, so they see what the readers made (a
    // hitscan gun's projectile).
    let reads = port.magazines.is_some()
        || port.shots.is_some()
        || port.hitscans.is_some()
        || !port.scripts.is_empty()
        || port.rules.as_ref().is_some_and(|r| !r.tables.is_empty());
    let mut read_patch = None;
    let mut definitions = vec![];
    for (what, how) in &port.handles {
        // A preference the game carries out with no setting to change it.
        match what.strip_prefix("pref:") {
            Some(global) => {
                applied
                    .prefs
                    .insert(global.to_ascii_lowercase(), how.clone());
            }
            None => handle(&mut applied.handled, what, how),
        }
    }
    let handled = &mut applied.handled;
    if reads {
        let mut weapons: Value = serde_json::from_slice(
            &std::fs::read(out.join(WEAPONS)).context("the import wrote no weapons")?,
        )
        .context(WEAPONS)?;
        // What the readers see: the import's pack with its dependencies'
        // projectiles beside its own. Only their patch is written back.
        if let Some(own) = weapons["projectiles"].as_object_mut() {
            for (id, p) in import.dependencies {
                own.entry(id.clone()).or_insert(serde_json::to_value(p)?);
            }
        }
        let mut patch: Option<Value> = None;
        let mut read = BTreeMap::new();
        let mut add = |p: Value| match &mut patch {
            Some(g) => compose(g, &p),
            None => patch = Some(p),
        };
        if let Some(m) = &port.magazines {
            let (p, v) = datablocks::magazines(m, &weapons, code, handled).context("magazines")?;
            add(p);
            read.extend(v);
        }
        if let Some(s) = &port.shots {
            add(shots::shots(s, &weapons, bodies, handled).context("shots")?);
        }
        if let Some(h) = &port.hitscans {
            let r = shots::hitscans(h, &weapons, code, handled).context("hitscans")?;
            add(r.patch);
            definitions = r.definitions;
        }
        if !port.scripts.is_empty() {
            let reads = datablocks::scripts(&port.scripts, &weapons, code, handled)
                .context("script rules")?;
            add(reads.patch);
            read.extend(reads.tables);
        }
        let mut patch = patch.unwrap_or_else(|| Value::Object(Default::default()));
        if let Some(limit) = port.hitscans.as_ref().and_then(|h| h.damage_limit) {
            shots::limit_ray_damage(&mut patch, limit);
        }
        merge(&mut weapons, &patch);
        add_definitions(&mut weapons, &definitions)?;
        if let Some(r) = &port.rules {
            read.extend(datablocks::tables(&r.tables, &weapons, code, handled)?);
            // Constants, with the values the patterns read filled in.
            for (name, value) in &r.values {
                let value = fill(value, &values).with_context(|| format!("rules value {name}"))?;
                read.insert(name.clone(), datablocks::rhai(&value));
            }
        }
        for (name, value) in read {
            ensure!(
                values.insert(name.clone(), value).is_none(),
                "`{name}` is named twice"
            );
        }
        read_patch = Some(patch);
    }
    let mut patches = port.patch.clone();
    let weapon_settings = port
        .rules
        .as_ref()
        .map(|r| &r.settings)
        .filter(|s| !s.is_empty());
    if read_patch.is_some() || weapon_settings.is_some() {
        patches
            .entry(WEAPONS.to_owned())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    if port.rules.is_some() {
        // The import names its rules, so they are turned on and off with it.
        let manifest = patches
            .entry("package.json".to_owned())
            .or_insert_with(|| Value::Object(Default::default()));
        ensure!(
            manifest.is_object(),
            "the package.json patch is not an object"
        );
        manifest["companions"] = serde_json::json!(["{rules}"]);
    }
    let mut writes: Vec<(String, Vec<u8>)> = vec![];
    for (file, patch) in &patches {
        let path = out.join(file);
        let mut doc: Value = serde_json::from_slice(
            &std::fs::read(&path).with_context(|| format!("the import wrote no {file}"))?,
        )
        .with_context(|| file.clone())?;
        if file == WEAPONS
            && let Some(read) = &read_patch
        {
            // What the readers found first, so the port's own patch can
            // still correct it.
            merge(&mut doc, read);
            add_definitions(&mut doc, &definitions)?;
        }
        merge(&mut doc, &fill(patch, &values)?);
        if file == WEAPONS
            && let Some(s) = weapon_settings
        {
            settings::write(s, &mut doc, code).context("weapon settings")?;
        }
        let bytes = serde_json::to_vec_pretty(&doc)?;
        if file == WEAPONS {
            bri_weapons::Pack::from_json(&bytes).context("the patched weapons.json")?;
        }
        writes.push((file.clone(), bytes));
    }
    let mut provided = vec![];
    for (file, bytes) in ports.added_files(&e.port) {
        safe_relative(&file)?;
        ensure!(
            !out.join(&file).exists(),
            "{file} would replace an imported file"
        );
        let Some(kind) = port.provides.get(&file) else {
            writes.push((file, bytes.to_vec()));
            continue;
        };
        let text = std::str::from_utf8(bytes)
            .with_context(|| format!("files/{file} is not UTF-8 text"))?;
        let text = fill_text(text, &values).with_context(|| format!("files/{file}"))?;
        let stem = file.rsplit('/').next().unwrap_or(&file).trim_end_matches(".json");
        provided.push(serde_json::json!({
            "kind": kind,
            "id": crate::content_id(import.namespace, kind, stem),
            "file": file,
        }));
        writes.push((file, text.into_bytes()));
    }
    if !provided.is_empty() {
        // The import's manifest lists what the port added.
        let at = writes.iter().position(|(f, _)| f == "package.json");
        let bytes = match at {
            Some(i) => writes[i].1.clone(),
            None => std::fs::read(out.join("package.json")).context("the import wrote no package.json")?,
        };
        let mut manifest: Value = serde_json::from_slice(&bytes).context("package.json")?;
        manifest["provides"]
            .as_array_mut()
            .context("package.json has no provides")?
            .extend(provided);
        let bytes = serde_json::to_vec_pretty(&manifest)?;
        match at {
            Some(i) => writes[i].1 = bytes,
            None => writes.push(("package.json".into(), bytes)),
        }
    }
    repin(out, &mut writes)?;
    let rules = match &port.rules {
        Some(r) => {
            // The copy's RTB preferences the rules read, as their settings.
            let mut settings: Vec<bri_package::setting::SettingDef> = vec![];
            for (_, pref) in crate::rtb::prefs(&code.calls) {
                let (Some(how), Ok(def)) = (r.pref(&pref.global), pref.setting) else {
                    continue;
                };
                if settings.iter().any(|d| d.key == def.key) {
                    continue;
                }
                applied.prefs.insert(
                    pref.global.to_ascii_lowercase(),
                    format!(
                        "server setting {} (the Admin menu's Add-On Settings): {how}",
                        def.title
                    ),
                );
                settings.push(def);
            }
            Some(rules_package(
                ports,
                e,
                &port.include,
                r,
                import,
                &Fill {
                    values: &values,
                    settings: &settings,
                },
                out,
            )?)
        }
        None => None,
    };
    for (file, bytes) in writes {
        let path = out.join(&file);
        std::fs::create_dir_all(path.parent().context("output path")?)?;
        std::fs::write(path, bytes)?;
        applied.files_changed.push(file);
    }
    if let Some((package, files)) = rules {
        let dir = rules_dir(out);
        for (file, bytes) in files {
            let path = dir.join(&file);
            std::fs::create_dir_all(path.parent().context("rules path")?)?;
            std::fs::write(path, bytes)?;
        }
        applied.rules = Some(package);
    }
    Ok(())
}

const WEAPONS: &str = "assets/weapons.json";

/// Appends the readers' `definitions` to a `weapons.json`.
fn add_definitions(weapons: &mut Value, definitions: &[Value]) -> Result<()> {
    if definitions.is_empty() {
        return Ok(());
    }
    let list = weapons["definitions"]
        .as_array_mut()
        .context("weapons.json has no definitions")?;
    list.extend(definitions.iter().cloned());
    Ok(())
}

/// A file to write: its path and bytes.
type Written = (String, Vec<u8>);

/// What [`rules_package`] fills in: `{{name}}` values for the rules files
/// and the copy's preferences the rules read, as `behaviour.json` settings.
struct Fill<'a> {
    values: &'a BTreeMap<String, String>,
    settings: &'a [bri_package::setting::SettingDef],
}

/// The companion host-rules Add-On for `import`: its manifest and the
/// port's rules files, with `{{name}}` in them filled in. Checked here, so
/// a port whose rules would not load is not applied.
fn rules_package(
    ports: &Ports,
    e: &Entry,
    include: &[String],
    rules: &Rules,
    import: &Import,
    fill: &Fill,
    out: &Path,
) -> Result<(RulesPackage, Vec<Written>)> {
    let dir = rules_dir(out);
    ensure!(
        !dir.exists(),
        "{} already exists; the port's rules go there",
        dir.display()
    );
    let id = rules_id(import.namespace);
    let mut files: Vec<Written> = vec![];
    let mut provides = vec![];
    for (file, bytes) in ports.rules_files(&e.port, include, Some(rules))? {
        let text = port_text(&bytes).with_context(|| format!("rules/{file} is not UTF-8 text"))?;
        let mut text = fill_text(&text, fill.values).with_context(|| format!("rules/{file}"))?;
        if file == RULES_BEHAVIOUR && !fill.settings.is_empty() {
            let mut doc: Value =
                serde_json::from_str(&text).with_context(|| format!("rules/{file}"))?;
            let list = doc
                .as_object_mut()
                .context("rules/behaviour.json is not an object")?
                .entry("settings")
                .or_insert_with(|| Value::Array(vec![]))
                .as_array_mut()
                .context("rules/behaviour.json's settings is not a list")?;
            for def in fill.settings {
                list.push(serde_json::to_value(def)?);
            }
            text = serde_json::to_string_pretty(&doc)?;
        }
        let (kind, stem) = if file == RULES_BEHAVIOUR {
            ("behaviour", "behaviour")
        } else if let Some(name) = rules_archetype(&file) {
            ("archetype", name)
        } else {
            ("script", file.trim_end_matches(".rhai"))
        };
        provides.push(serde_json::json!({
            "kind": kind,
            "id": crate::content_id(&id, kind, stem),
            "file": file,
        }));
        files.push((file, text.into_bytes()));
    }
    let mut dependencies = serde_json::Map::new();
    dependencies.insert(
        import.namespace.to_owned(),
        format!("={}", import.version).into(),
    );
    for addon in rules.needs.values() {
        dependencies.insert(rules_id(&crate::namespace_for(addon)?), "*".into());
    }
    let mut manifest = serde_json::json!({
        "schema_version": 1,
        "id": id,
        "version": import.version,
        "api": 1,
        "name": format!("{} (host rules)", import.name),
        "description": format!(
            "What {}'s scripts did, rewritten for this game. Only the host runs it; it is turned on and off with {}.",
            import.name, import.name
        ),
        "authors": ["Blockland ReImagined"],
        "license": "CC0-1.0",
        "provenance": {
            "source": format!("Port {} of Blockland Add-On {}", e.port, e.addon),
            "notes": port_notes(ports, e),
        },
        "dependencies": dependencies,
        "optional_dependencies": rules
            .uses
            .iter()
            .chain(&rules.loads)
            .map(|addon| Ok((crate::namespace_for(addon)?, Value::from("*"))))
            .collect::<Result<serde_json::Map<_, _>>>()?,
        "capabilities": rules.capabilities,
        "provides": provides,
    });
    if !rules.loads.is_empty() {
        manifest["companions"] = rules
            .loads
            .iter()
            .map(|addon| crate::namespace_for(addon).map(Value::from))
            .collect::<Result<Value>>()?;
    }
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    files.push(("package.json".to_owned(), bytes.clone()));
    check_rules(&id, &bytes, &files)?;
    let folder = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let package = RulesPackage {
        id: id.clone(),
        dir: folder.clone(),
        packages_json_entry: serde_json::json!({
            "id": id,
            "version": import.version,
            "side": "server",
            "dir": folder,
        }),
        files: files.iter().map(|(f, _)| f.clone()).collect(),
    };
    Ok((package, files))
}

fn port_notes(ports: &Ports, e: &Entry) -> String {
    ports.port(e).map(|p| p.notes).unwrap_or_default()
}

/// The rules' manifest and behaviour, read as the game will read them.
fn check_rules(id: &str, manifest: &[u8], files: &[Written]) -> Result<()> {
    if let Err(problems) = bri_package_runtime::manifest::Manifest::parse(manifest, id) {
        bail!(
            "the rules' package.json: {}",
            problems
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    let (_, behaviour) = files
        .iter()
        .find(|(f, _)| f == RULES_BEHAVIOUR)
        .context("rules/behaviour.json is missing")?;
    let behaviour: bri_package_runtime::content::Behaviour =
        serde_json::from_slice(behaviour).context("rules/behaviour.json")?;
    behaviour.validate().context("rules/behaviour.json")?;
    ensure!(
        files.iter().any(|(f, _)| *f == behaviour.script),
        "rules/behaviour.json runs `{}`, which the rules do not have",
        behaviour.script
    );
    for (file, bytes) in files {
        if file.ends_with(".rhai") {
            bri_package_runtime::script::check_syntax(&String::from_utf8_lossy(bytes))
                .map_err(|e| anyhow::anyhow!("rules/{file}: {e}"))?;
        }
        if rules_archetype(file).is_some() {
            let archetype: bri_package_runtime::content::ArchetypeDef =
                serde_json::from_slice(bytes).with_context(|| format!("rules/{file}"))?;
            archetype
                .validate()
                .with_context(|| format!("rules/{file}"))?;
        }
    }
    Ok(())
}

/// `{{name}}` in a rules file becomes that value's text,
/// `{{name|bool}}` `true` or `false` for a TorqueScript truth value (`1`,
/// `0`, `true`, `false`), as a JSON setting's default needs, and
/// `{{name|event_params}}` the JSON parameter list of a
/// `registerOutputEvent` parameter string (see [`event_params`]), and
/// `{{name|lower}}` the text in lower case, as content ids spell a Torque
/// name (`v20.weapon.{{equip|lower}}`). A
/// `{{word}}` that names no value is an error, so a misspelt name is
/// caught, as is a value its filter cannot read.
/// A port's text file with Unix line endings, however it was checked out
/// (`core.autocrlf` on Windows), so its rules come out the same everywhere.
fn port_text(bytes: &[u8]) -> Result<String> {
    Ok(crate::script_text(std::str::from_utf8(bytes)?.as_bytes()))
}

fn fill_text(text: &str, values: &BTreeMap<String, String>) -> Result<String> {
    let re = regex::Regex::new(r"\{\{([A-Za-z_][A-Za-z0-9_]*)(\|bool|\|event_params|\|lower)?\}\}")?;
    let mut problem = None;
    let filled = re.replace_all(text, |c: &regex::Captures| {
        let Some(v) = values.get(&c[1]) else {
            problem.get_or_insert_with(|| format!("uses `{}`, which no pattern captures", &c[0]));
            return String::new();
        };
        match c.get(2).map(|m| m.as_str()) {
            None => v.clone(),
            Some("|lower") => v.to_ascii_lowercase(),
            Some("|event_params") => event_params(v).unwrap_or_else(|e| {
                problem.get_or_insert_with(|| format!("`{}`: {e:#}", &c[0]));
                String::new()
            }),
            Some(_) => match v.trim().to_ascii_lowercase().as_str() {
                "1" | "true" => "true".to_owned(),
                "0" | "false" => "false".to_owned(),
                _ => {
                    problem.get_or_insert_with(|| {
                        format!("`{}` is `{v}`, not 1, 0, true or false", &c[0])
                    });
                    String::new()
                }
            },
        }
    });
    if let Some(problem) = problem {
        bail!("{problem}");
    }
    Ok(filled.into_owned())
}

/// The parameters of `registerOutputEvent(class, name, params)` as
/// `behaviour.json` `brick_outputs` writes them. `source` is the params
/// argument as the script spells it: quoted strings joined by `TAB` (or
/// holding `\t`), each field a v20 parameter (`int min max default`,
/// `float min max step default`, `bool`, `string length width`,
/// `paintColor default`, `list name value ...`).
fn event_params(source: &str) -> Result<String> {
    let token = regex::Regex::new(r#"^\s*(?:"((?:[^"\\]|\\.)*)"|(TAB))"#)?;
    let mut text = String::new();
    let mut rest = source.trim();
    let mut want_string = true;
    while !rest.is_empty() {
        let c = token
            .captures(rest)
            .with_context(|| format!("cannot read `{rest}` as parameter text"))?;
        if let Some(quoted) = c.get(1) {
            ensure!(want_string, "two strings in a row in `{source}`");
            text.push_str(&quoted.as_str().replace("\\t", "\t"));
        } else {
            ensure!(!want_string, "TAB without a string before it in `{source}`");
            text.push('\t');
        }
        want_string = !want_string;
        rest = rest[c.get(0).unwrap().end()..].trim_start();
    }
    let number = |w: Option<&&str>, what: &str| -> Result<f64> {
        w.with_context(|| format!("{what} is missing"))?
            .parse::<f64>()
            .with_context(|| format!("{what} is not a number"))
    };
    let mut params = Vec::new();
    for field in text.split('\t').filter(|f| !f.trim().is_empty()) {
        let w: Vec<&str> = field.split_whitespace().collect();
        let kind = w[0].to_ascii_lowercase();
        params.push(match kind.as_str() {
            "int" => serde_json::json!({
                "type": "int",
                "min": number(w.get(1), "int min")? as i64,
                "max": number(w.get(2), "int max")? as i64,
                "default": number(w.get(3), "int default")? as i64,
            }),
            "float" => serde_json::json!({
                "type": "float",
                "min": number(w.get(1), "float min")?,
                "max": number(w.get(2), "float max")?,
                "step": number(w.get(3), "float step")?,
                "default": number(w.get(4), "float default")?,
            }),
            "bool" => serde_json::json!({ "type": "bool" }),
            "string" => serde_json::json!({
                "type": "string",
                "max_length": number(w.get(1), "string length")? as u32,
                "width": number(w.get(2), "string width")? as i32,
            }),
            "paintcolor" => serde_json::json!({
                "type": "paint_color",
                "default": number(w.get(1), "paintColor default")? as u8,
            }),
            "list" => {
                ensure!(w.len() >= 3 && w.len() % 2 == 1, "list `{field}` is not name value pairs");
                let items = w[1..]
                    .chunks(2)
                    .map(|p| Ok(serde_json::json!([p[0], number(p.get(1), "list value")? as i64])))
                    .collect::<Result<Vec<_>>>()?;
                serde_json::json!({ "type": "list", "items": items })
            }
            other => bail!("parameter type `{other}` is not one an Add-On output can take"),
        });
    }
    Ok(serde_json::to_string(&params)?)
}

/// The item presentation the importer writes pins the exact bytes of the
/// weapons pack and item physics beside it (hosts and players refuse a
/// mismatch). After a port changes either, pin the new bytes.
const PRESENTATION: &str = "assets/presentation.json";
const PINNED: [(&str, &str); 2] = [
    ("assets/weapons.json", "weapons_sha256"),
    ("assets/item-physics.json", "item_physics_sha256"),
];

fn repin(out: &Path, writes: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
    let changed: Vec<(&str, String)> = PINNED
        .iter()
        .filter_map(|(file, field)| {
            let (_, bytes) = writes.iter().find(|(f, _)| f == file)?;
            Some((*field, crate::source::hash(bytes)))
        })
        .collect();
    if changed.is_empty() {
        return Ok(());
    }
    let at = writes.iter().position(|(f, _)| f == PRESENTATION);
    let bytes = match at {
        Some(i) => writes[i].1.clone(),
        None => match std::fs::read(out.join(PRESENTATION)) {
            Ok(b) => b,
            // No presentation, nothing pinned.
            Err(_) => return Ok(()),
        },
    };
    let mut doc: Value = serde_json::from_slice(&bytes).context(PRESENTATION)?;
    for (field, sha) in changed {
        doc[field] = Value::String(sha);
    }
    let bytes = serde_json::to_vec_pretty(&doc)?;
    match at {
        Some(i) => writes[i].1 = bytes,
        None => writes.push((PRESENTATION.into(), bytes)),
    }
    Ok(())
}

/// Checks that the presentation in the package `out` pins the weapons pack
/// and item physics beside it, as hosts and players do when they load it.
pub fn check_pins(out: &Path) -> Result<()> {
    let Ok(bytes) = std::fs::read(out.join(PRESENTATION)) else {
        return Ok(());
    };
    let doc: Value = serde_json::from_slice(&bytes).context(PRESENTATION)?;
    for (file, field) in PINNED {
        let sha =
            crate::source::hash(&std::fs::read(out.join(file)).with_context(|| file.to_owned())?);
        ensure!(
            doc[field].as_str() == Some(sha.as_str()),
            "{PRESENTATION} does not match {file}; the game would refuse to load this Add-On"
        );
    }
    Ok(())
}

/// Replaces every string that is exactly `{name}` with that value, as a
/// number when it reads as one, and `{name}` inside longer strings and in
/// keys with its text.
fn fill(v: &Value, values: &BTreeMap<String, String>) -> Result<Value> {
    Ok(match v {
        // A whole `{name}` takes the captured value's type; names inside
        // other text (`{namespace}:sound/{a}`) are replaced as text.
        Value::String(s)
            if s.len() > 2
                && s.starts_with('{')
                && s.ends_with('}')
                && !s[1..s.len() - 1].contains(['{', '}']) =>
        {
            let name = &s[1..s.len() - 1];
            let (name, lower) = name
                .strip_suffix(":lower")
                .map_or((name, false), |n| (n, true));
            let value = values
                .get(name)
                .with_context(|| format!("the patch uses `{s}`, which no pattern captures"))?;
            if lower {
                return Ok(Value::String(value.to_ascii_lowercase()));
            }
            if let Ok(n) = value.parse::<i64>() {
                serde_json::json!(n)
            } else {
                match value.parse::<f64>() {
                    Ok(n) if n.is_finite() => serde_json::json!(n),
                    _ => Value::String(value.clone()),
                }
            }
        }
        Value::String(s) => {
            let mut s = s.clone();
            for (name, value) in values {
                s = s.replace(&format!("{{{name}}}"), value);
                // Ids are lower case, as Torque's names ignore it.
                s = s.replace(&format!("{{{name}:lower}}"), &value.to_ascii_lowercase());
            }
            ensure!(
                !s.contains(":lower}"),
                "the patch uses `{s}`, which no pattern captures"
            );
            Value::String(s)
        }
        // Keys too, so a patch names the import's ids (`{namespace}:image/x`).
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| {
                    let mut k = k.clone();
                    for (name, value) in values {
                        k = k.replace(&format!("{{{name}}}"), value);
                    }
                    Ok((k, fill(v, values)?))
                })
                .collect::<Result<_>>()?,
        ),
        Value::Array(a) => Value::Array(a.iter().map(|v| fill(v, values)).collect::<Result<_>>()?),
        other => other.clone(),
    })
}

/// RFC 7396 JSON merge patch.
fn merge(target: &mut Value, patch: &Value) {
    let Value::Object(p) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Default::default());
    }
    let t = target.as_object_mut().expect("object");
    for (k, v) in p {
        if v.is_null() {
            t.remove(k);
        } else {
            merge(t.entry(k.clone()).or_insert(Value::Null), v);
        }
    }
}

/// Folds `patch` into the merge patch `target`, so applying the result is
/// applying `target` and then `patch`: unlike [`merge`], a `null` stays, to
/// remove the field from the document the patches end up on.
fn compose(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                match t.get_mut(k) {
                    Some(old) if old.is_object() && v.is_object() => compose(old, v),
                    _ => {
                        t.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_patch_follows_rfc_7396() {
        let mut doc = json!({"a": {"b": 1, "c": 2}, "d": [1, 2]});
        merge(&mut doc, &json!({"a": {"b": null, "e": 3}, "d": [3]}));
        assert_eq!(doc, json!({"a": {"c": 2, "e": 3}, "d": [3]}));
    }

    #[test]
    fn composed_patches_apply_as_both_in_turn() {
        let doc = json!({"m": {"on_loaded": {"loaded": true}, "size": 2}});
        let (first, second) = (
            json!({"m": {"on_loaded": {"loaded": true}, "size": 6}}),
            json!({"m": {"on_loaded": null, "one_by_one": true}}),
        );
        let mut both = first.clone();
        compose(&mut both, &second);
        let (mut once, mut twice) = (doc.clone(), doc);
        merge(&mut once, &both);
        merge(&mut twice, &first);
        merge(&mut twice, &second);
        assert_eq!(once, twice);
        assert_eq!(once, json!({"m": {"size": 6, "one_by_one": true}}));
    }

    #[test]
    fn fill_reads_numbers_and_refuses_unknown_names() {
        let values = BTreeMap::from([("n".to_string(), "3".to_string())]);
        assert_eq!(
            fill(&json!({"x": "{n}"}), &values).unwrap(),
            json!({"x": 3})
        );
        assert!(fill(&json!("{missing}"), &values).is_err());
    }

    #[test]
    fn event_params_read_registeroutputevent_text() {
        let read = |t: &str| serde_json::from_str::<Value>(&event_params(t).unwrap()).unwrap();
        assert_eq!(
            read(r#""list TriggerTeam 0 TeamColor 1 ALL 2" TAB "paintColor 0" TAB "bool""#),
            json!([
                {"type": "list", "items": [["TriggerTeam", 0], ["TeamColor", 1], ["ALL", 2]]},
                {"type": "paint_color", "default": 0},
                {"type": "bool"}
            ])
        );
        assert_eq!(
            read(r#""int -999 999 1\tbool 1""#),
            json!([{"type": "int", "min": -999, "max": 999, "default": 1}, {"type": "bool"}])
        );
        assert_eq!(read(r#""""#), json!([]));
        assert!(event_params(r#""datablock ItemData""#).is_err());
        assert!(event_params(r#""int 0 1 0" "bool""#).is_err());
    }

    #[test]
    fn fill_lowers_names_for_ids() {
        let values = BTreeMap::from([("p".to_string(), "knifeProjectile".to_string())]);
        assert_eq!(
            fill(
                &json!(["kit:projectile/{p:lower}", "{p:lower}", "{p}"]),
                &values
            )
            .unwrap(),
            json!([
                "kit:projectile/knifeprojectile",
                "knifeprojectile",
                "knifeProjectile"
            ])
        );
        assert!(fill(&json!("kit:{missing:lower}"), &values).is_err());
        assert_eq!(
            fill_text("v20.weapon.{{p|lower}} {{p}}", &values).unwrap(),
            "v20.weapon.knifeprojectile knifeProjectile"
        );
    }
}
