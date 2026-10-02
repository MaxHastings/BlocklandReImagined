//! One legacy Blockland Add-On to a native package and a report.
//!
//! The split is the importer split from `docs/architecture/platform-principles.md`:
//! Torque understanding (`bri_convert::tscript`, the DTS/BLB readers) →
//! native semantic content (the same lowering the vanilla weapon, vehicle and
//! brick importers use, with this Add-On's namespace) → a package directory.
//! Nothing is executed. What only a script run would know is reported, with
//! the source function, as behaviour an agent must build natively.
//! Findings: `docs/audits/spike-addon-import.md`.
pub mod behaviour;
mod help;
mod player_types;
pub mod porting;
pub mod ports;
pub mod reference;
pub mod report;
pub mod rtb;
pub mod source;
mod vehicle_script;
mod weapon_fx;

use anyhow::{Context, Result, ensure};
use bri_convert::tscript::{self, Datablock, Script};
use reference::Reference;
use report::*;
use serde_json::json;
use source::{Source, hash};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub fn literal(v: &str) -> &str {
    tscript::literal(v)
}

#[derive(Debug, Clone)]
pub struct Options {
    /// The Add-On zip or folder.
    pub input: PathBuf,
    /// A fresh output directory; it becomes the package directory.
    pub out: PathBuf,
    /// A read-only v20 install whose Add-Ons and base files satisfy references.
    pub reference: Option<PathBuf>,
    /// Recovered core scripts (`allGameScripts.cs`, `DamageTypes.cs`) for
    /// base datablocks and damage types.
    pub core: Vec<PathBuf>,
    /// The installed game's content root: base datablocks an Add-On
    /// inherits from or names (a brick's parent, a sound) are read from its
    /// brick catalog, weapons, sounds and effects.
    pub installed: Option<PathBuf>,
    pub version: String,
}

impl Default for Options {
    /// Version 1.0.0, no reference install, core scripts or installed
    /// game: set `input` and `out` and what else the import needs, with
    /// `..Default::default()` for the rest.
    fn default() -> Self {
        Self {
            input: PathBuf::new(),
            out: PathBuf::new(),
            reference: None,
            core: vec![],
            installed: None,
            version: "1.0.0".into(),
        }
    }
}

/// The package id (and content namespace) for an Add-On folder name.
pub fn namespace_for(addon: &str) -> Result<String> {
    let mut ns = String::new();
    for c in addon.chars() {
        let c = c.to_ascii_lowercase();
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
            c
        } else {
            '_'
        };
        if !(c == '_' && ns.ends_with('_')) {
            ns.push(c);
        }
    }
    let mut ns = ns.trim_matches(|c| c == '_' || c == '-').to_owned();
    if !ns.starts_with(|c: char| c.is_ascii_lowercase()) || bri_package::id::is_reserved(&ns) {
        ns = format!("addon_{ns}");
    }
    ns.truncate(bri_package::id::MAX_NAMESPACE);
    let ns = ns.trim_end_matches(['_', '-']).to_owned();
    if let Some(problem) = bri_package::id::namespace_problem(&ns) {
        anyhow::bail!("cannot derive a package id from `{addon}`: `{ns}` {problem}");
    }
    Ok(ns)
}

/// A content id in the platform grammar; names outside it are folded to `_`.
pub fn content_id(namespace: &str, kind: &str, name: &str) -> String {
    let mut n: String = name
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "_-./".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    n.truncate(bri_package::id::MAX_CONTENT_NAME);
    format!("{namespace}:{kind}/{}", n.trim_matches('/'))
}

const WEAPON_CLASSES: &[&str] = &[
    "ItemData",
    "ShapeBaseImageData",
    "ProjectileData",
    "ExplosionData",
];
const VEHICLE_CLASSES: &[(&str, bri_vehicles::schema::Family)] = &[
    ("WheeledVehicleData", bri_vehicles::schema::Family::Wheeled),
    ("FlyingVehicleData", bri_vehicles::schema::Family::Flying),
];
/// Fields that name another datablock. Keys are lower-case, without `[n]`.
const REFERENCE_FIELDS: &[&str] = &[
    "image",
    "item",
    "projectile",
    "explosion",
    "debris",
    "casing",
    "emitter",
    "emitters",
    "particles",
    "statesound",
    "stateemitter",
    "sound",
    "soundprofile",
    "defaulttire",
    "defaultspring",
    "flattire",
    "flatspring",
    "tireemitter",
    "splash",
    "splashemitter",
    "damageemitter",
    "initialexplosionprojectile",
    "finalexplosionprojectile",
    "holebot",
    "particleemitter",
    "bounceexplosion",
    "stickexplosion",
    "bloodexplosion",
    "softimpactsound",
    "hardimpactsound",
    "subexplosion",
    "description",
    "uimage",
    "sportballimage",
];
/// Fields that name a file.
pub(crate) const FILE_FIELDS: &[&str] = &[
    "shapefile",
    "shapename",
    "projectileshapename",
    "explosionshape",
    "filename",
    "brickfile",
    "iconname",
    "texturename",
    "collisionshapename",
    "dtsfile",
];

/// Rewrites `./` and `~/` file fields to root-relative virtual paths. Torque
/// resolves them against the declaring script's folder, which matters once
/// an Add-On keeps scripts in subfolders or a child inherits a parent's file.
pub(crate) fn resolve_file_fields(d: &mut Datablock, script: &str) {
    for (k, v) in d.fields.iter_mut() {
        let base = k.split('[').next().unwrap_or(k);
        let lit = literal(v);
        if FILE_FIELDS.contains(&base) && (lit.starts_with("./") || lit.starts_with("~/")) {
            *v = format!("\"{}\"", source::resolve(script, lit));
        }
    }
}

struct Owned {
    d: Datablock,
    path: String,
    sha256: String,
    /// Inherited fields merged, raw source expressions.
    fields: BTreeMap<String, String>,
}

struct Ctx<'a> {
    src: &'a Source,
    reference: &'a Reference,
    ns: String,
    out: PathBuf,
    report: Report,
    owned: BTreeMap<String, Owned>,
    /// Add-On name to what of it is used.
    uses: BTreeMap<String, BTreeSet<String>>,
    /// Converted shapes: lower virtual path to (package path, shape).
    shapes: BTreeMap<String, (String, bri_content::shape::Shape)>,
    /// Lower virtual path to package-relative output file.
    outputs: BTreeMap<String, String>,
    provides: Vec<serde_json::Value>,
    /// The projectiles of the Add-Ons this one depends on, by the ids their
    /// packages give them, as ports read them ([`ports::Import::dependencies`]).
    dependency_projectiles: BTreeMap<String, bri_weapons::ProjectileDef>,
    /// Scripts a port declares (`datablocks.cs`), by lower virtual path:
    /// read beside the Add-On's own, but not among its files.
    ported: BTreeMap<String, String>,
}

impl Ctx<'_> {
    /// The text of one of the scripts being imported: the Add-On's own or
    /// one its port declares.
    fn script_text(&self, path: &str) -> Option<String> {
        match self.src.get(path) {
            Some(f) => Some(script_text(&f.bytes)),
            None => self.ported.get(&path.to_ascii_lowercase()).cloned(),
        }
    }

    fn id(&mut self, kind: &str, name: &str, from: &str, file: &str) -> String {
        let id = content_id(&self.ns, kind, name);
        if !self.report.ids.iter().any(|e| e.id == id) {
            self.report.ids.push(IdEntry {
                id: id.clone(),
                kind: kind.into(),
                from: from.into(),
                file: file.into(),
            });
            self.provides
                .push(json!({ "kind": kind, "id": id, "file": file }));
        }
        id
    }
    fn write(&self, rel: &str, bytes: &[u8]) -> Result<()> {
        let path = self.out.join(rel);
        std::fs::create_dir_all(path.parent().context("output path")?)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
    fn is_owned(&self, name: &str) -> bool {
        self.owned.contains_key(&name.to_ascii_lowercase())
    }
    fn used(&mut self, addon: &str, what: String) {
        self.uses.entry(addon.to_owned()).or_default().insert(what);
    }
    fn ambiguous(
        &mut self,
        what: String,
        at: Option<Location>,
        detail: String,
        resolution: Option<String>,
    ) {
        self.report.ambiguous.push(Finding {
            what,
            source: at,
            detail,
            resolution,
        });
    }
    fn unsupported(&mut self, what: String, at: Option<Location>, detail: String) {
        self.report.unsupported.push(Finding {
            what,
            source: at,
            detail,
            resolution: None,
        });
    }
    fn entry(&mut self, name: &str) -> Option<&mut DatablockEntry> {
        self.report
            .datablocks
            .iter_mut()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }
    fn mark(
        &mut self,
        name: &str,
        as_: &str,
        status: &str,
        ids: Vec<String>,
        note: Option<String>,
    ) {
        if let Some(e) = self.entry(name) {
            e.recognised_as = as_.into();
            e.status = status.into();
            e.ids.extend(ids);
            e.notes.extend(note);
        }
    }
}

fn check_output(opts: &Options) -> Result<()> {
    ensure!(
        !opts.out.exists(),
        "Output {} already exists; choose a fresh directory",
        opts.out.display()
    );
    // A port's host rules go beside it (`ports::rules_dir`).
    let rules = ports::rules_dir(&opts.out);
    ensure!(
        !rules.exists(),
        "{} already exists; choose a fresh directory",
        rules.display()
    );
    let parent = opts
        .out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    for protected in opts.reference.iter().chain([&opts.input]) {
        let p = protected.canonicalize()?;
        ensure!(
            !parent.starts_with(&p),
            "Output must stay outside the read-only source {}",
            p.display()
        );
    }
    Ok(())
}

pub fn import(opts: &Options) -> Result<Report> {
    import_with(opts, &ports::Ports::builtin())
}

/// [`import`] with a given ports list instead of the one built in.
pub fn import_with(opts: &Options, ports: &ports::Ports) -> Result<Report> {
    check_output(opts)?;
    let src = source::read(&opts.input)?;
    let mut reference = match &opts.reference {
        Some(root) => Reference::load(root, &opts.core)?,
        None => Reference::core_only(&opts.core)?,
    };
    if let Some(content) = &opts.installed {
        reference.add_installed(content)?;
    }
    // The reference as it stood when this Add-On loaded, after the ones
    // it requires.
    let required: Vec<String> = src
        .files
        .iter()
        .filter(|(key, _)| key.ends_with(".cs"))
        .flat_map(|(_, f)| reference::required_addons(&String::from_utf8_lossy(&f.bytes)))
        .collect();
    // One it requires that the reference lacks is looked for beside it.
    if let Some(folder) = opts.input.parent() {
        let here = Path::new(".");
        let folder = if folder.as_os_str().is_empty() {
            here
        } else {
            folder
        };
        reference.add_beside(folder, &required);
    }
    reference.settle_for(&src.name, &required);
    let ns = namespace_for(&src.name)?;
    std::fs::create_dir_all(&opts.out)?;
    let mut cx = Ctx {
        src: &src,
        reference: &reference,
        ns: ns.clone(),
        out: opts.out.clone(),
        report: Report {
            schema_version: REPORT_SCHEMA,
            ..Report::default()
        },
        owned: BTreeMap::new(),
        uses: BTreeMap::new(),
        shapes: BTreeMap::new(),
        outputs: BTreeMap::new(),
        provides: vec![],
        dependency_projectiles: BTreeMap::new(),
        ported: BTreeMap::new(),
    };
    metadata(&mut cx);
    let mut scripts = read_scripts(&mut cx);
    if let Some(reached) = inventory(&mut cx, &scripts) {
        // v20 runs server.cs and what it execs; a file nothing execs never
        // ran (a gun left out by a commented-out exec).
        scripts.retain(|s| reached.contains(&s.path.to_ascii_lowercase()));
    }
    // What a port's patterns read: every function's body (without its
    // comments), and each script file's whole text by its path in the
    // Add-On (`server.cs`), for values set outside any function.
    let mut code = ports::Code::default();
    let bodies = &mut code.bodies;
    // Torque keeps the last definition of a function (names ignore case).
    // A packaged one only wraps it (`Parent::`), so ports read the plain
    // definition, and a packaged body only where there is none.
    let functions = || scripts.iter().flat_map(|s| &s.functions);
    for f in functions().filter(|f| f.package.is_none()) {
        bodies.insert(
            f.qualified().to_ascii_lowercase(),
            tscript::without_comments(&f.body),
        );
    }
    for f in functions().filter(|f| f.package.is_some()) {
        bodies
            .entry(f.qualified().to_ascii_lowercase())
            .or_insert_with(|| tscript::without_comments(&f.body));
    }
    // Top-level globals too, by `$name` (`$ND::Version`): their value's
    // source, the last one set.
    for g in scripts.iter().flat_map(|s| &s.globals) {
        bodies.insert(g.name.to_ascii_lowercase(), g.value.clone());
    }
    // Text files too (Slayer's first-names.txt), for ports' `data`, and
    // GUI files for the text their controls show.
    for f in src.files.values() {
        let lower = f.path.to_ascii_lowercase();
        if [".cs", ".txt", ".hfl", ".gui"].iter().any(|ext| lower.ends_with(ext)) {
            bodies.insert(
                src.member(f).to_ascii_lowercase(),
                script_text(&f.bytes),
            );
        }
    }
    port_datablocks(&mut cx, ports, &code.bodies, &mut scripts);
    top_level(&mut cx, &scripts);
    datablocks(&mut cx, &scripts);
    references(&mut cx);
    convert_files(&mut cx)?;
    weapons(&mut cx, &scripts)?;
    vehicles(&mut cx, &scripts)?;
    bricks(&mut cx, &scripts)?;
    sounds_and_rest(&mut cx);
    behaviours(&mut cx, &scripts);
    dependencies(&mut cx, &scripts);
    // Torque links a datablock's namespace to its `className`'s: a method
    // the datablock lacks runs the class's (`BatonImage::onPreFire` is
    // `TF2MeleeWeaponImage::onPreFire`). The readers see it by both names.
    let mut linked = vec![];
    for o in cx.owned.values() {
        let Some(class) = o
            .fields
            .get("classname")
            .map(|c| literal(c).trim().to_ascii_lowercase())
            .filter(|c| !c.is_empty())
        else {
            continue;
        };
        let name = o.d.name.to_ascii_lowercase();
        let prefix = format!("{class}::");
        for (function, body) in &code.bodies {
            if let Some(method) = function.strip_prefix(&prefix) {
                let own = format!("{name}::{method}");
                if !code.bodies.contains_key(&own) {
                    linked.push((own, body.clone()));
                }
            }
        }
    }
    code.inherited.extend(linked.iter().map(|(own, _)| own.clone()));
    code.bodies.extend(linked);
    code.reference = cx
        .reference
        .datablocks
        .iter()
        .filter(|(_, o)| {
            WEAPON_CLASSES
                .iter()
                .any(|w| w.eq_ignore_ascii_case(&o.datablock.class))
        })
        .map(|(name, o)| (name.clone(), reference_definition(o)))
        .collect();
    code.calls = scripts
        .iter()
        .flat_map(|s| s.calls.iter().cloned())
        .collect();
    // Player types by name: this Add-On's, its dependencies' and v20's.
    let player_types: Vec<String> = cx
        .owned
        .values()
        .map(|o| &o.d)
        .chain(cx.reference.datablocks.values().map(|o| &o.datablock))
        .filter(|d| d.class.eq_ignore_ascii_case("PlayerData"))
        .map(|d| d.name.clone())
        .collect();
    // Sounds of a base game file, this Add-On's and those of the Add-Ons it
    // builds on (Tier 1's clicks, which Explosive 2's reloads play): the
    // base sound by name.
    let own = cx
        .owned
        .values()
        .map(|o| (&o.d, o.fields.get("filename"), o.path.as_str()));
    let others = cx
        .reference
        .datablocks
        .values()
        .filter(|o| o.addon != "base")
        .map(|o| {
            (
                &o.datablock,
                o.datablock.fields.get("filename"),
                o.path.as_str(),
            )
        });
    code.sounds = others
        .chain(own)
        .filter(|(d, ..)| d.class.eq_ignore_ascii_case("AudioProfile"))
        .filter_map(|(d, file, path)| {
            let file = source::resolve(path, literal(file?));
            let sound = cx.reference.base_sound(&file)?;
            Some((d.name.to_ascii_lowercase(), sound.to_owned()))
        })
        .collect();
    code.archetypes = player_types
        .iter()
        .map(|name| (name.to_ascii_lowercase(), archetype_id(&cx, name)))
        .collect();
    finish(cx, opts, ports, &code)
}

/// A listed port's `datablocks.cs`: datablocks the Add-On makes at run time,
/// read as if it were one more of the Add-On's scripts, in its folder.
fn port_datablocks(
    cx: &mut Ctx,
    ports: &ports::Ports,
    bodies: &ports::Bodies,
    scripts: &mut Vec<Script>,
) {
    let Some(text) = ports::datablocks(ports, &cx.src.name, &cx.ns, bodies) else {
        return;
    };
    let path = format!("{}/port-{}", cx.src.dir(), ports::DATABLOCKS);
    let read = text.and_then(|text| Ok((tscript::read(&text, &path)?, text)));
    match read {
        Ok((script, text)) => {
            cx.ported.insert(path.to_ascii_lowercase(), text);
            cx.report.diagnostics.push(format!(
                "{path}: the port declares {} datablocks the Add-On makes at run time",
                script.datablocks.len()
            ));
            scripts.push(script);
        }
        Err(e) => cx.unsupported(
            format!("script {path}"),
            None,
            format!("the port's datablocks could not be read: {e:#}"),
        ),
    }
}

fn metadata(cx: &mut Ctx) {
    let src = cx.src;
    let text = |name: &str| {
        src.get(&format!("{}/{name}", src.dir()))
            .map(|f| script_text(&f.bytes))
    };
    let mut info = SourceInfo {
        name: src.name.clone(),
        path: src.origin.clone(),
        sha256: src.sha256.clone(),
        format: src.format.into(),
        licence_status: "unknown".into(),
        ..SourceInfo::default()
    };
    if let Some(d) = text("description.txt") {
        let mut rest = vec![];
        for line in d.lines() {
            if let Some(t) = line.strip_prefix("Title:") {
                info.title = t.trim().into();
            } else if let Some(a) = line.strip_prefix("Author:") {
                info.authors = a
                    .split([',', '&'])
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else {
                rest.push(line.trim());
            }
        }
        info.description = rest.join(" ").trim().into();
    }
    if let Some(r) = text("rtbInfo.txt") {
        for line in r.lines() {
            if let Some((k, v)) = line.split_once(':') {
                info.listing
                    .insert(k.trim().to_ascii_lowercase(), v.trim().into());
            }
        }
        if info.title.is_empty()
            && let Some(t) = info.listing.get("title")
        {
            info.title = t.clone();
        }
    }
    info.licence_files = src
        .files
        .values()
        .map(|f| src.member(f).to_owned())
        .filter(|m| {
            let l = m.to_ascii_lowercase();
            l.contains("licen") || l.contains("copying")
        })
        .collect();
    if !info.licence_files.is_empty() {
        info.licence_status = "known".into();
        const SPDX: &[&str] = &[
            "CC0-1.0",
            "CC-BY-4.0",
            "CC-BY-SA-4.0",
            "MIT",
            "Apache-2.0",
            "GPL-3.0",
            "GPL-2.0",
            "BSD-3-Clause",
            "Unlicense",
        ];
        info.licence_spdx = info.licence_files.iter().find_map(|m| {
            let text = String::from_utf8_lossy(&src.get(&format!("{}/{m}", src.dir()))?.bytes)
                .into_owned();
            SPDX.iter()
                .find(|id| text.contains(**id))
                .map(|id| (*id).to_owned())
        });
    }
    cx.report.source = info;
}

/// A script or text file as the importer reads it: UTF-8 (lossily) with
/// Unix line endings, so a copy saved on Windows, or checked out there with
/// `core.autocrlf`, imports exactly as it does elsewhere.
pub(crate) fn script_text(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.into_owned()
    }
}

fn read_scripts(cx: &mut Ctx) -> Vec<Script> {
    let mut scripts = vec![];
    for f in cx.src.files.values() {
        let lower = f.path.to_ascii_lowercase();
        if lower.ends_with(".cs") {
            match tscript::read(&script_text(&f.bytes), &f.path) {
                Ok(s) => {
                    cx.report
                        .diagnostics
                        .extend(s.diagnostics.iter().map(|d| format!("{}: {d}", f.path)));
                    scripts.push(s);
                }
                Err(e) => cx.unsupported(
                    format!("script {}", f.path),
                    Some(Location::new(&f.path, 0)),
                    format!("could not be read: {e:#}"),
                ),
            }
        }
    }
    scripts
}

fn kind_of(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "cs" => "script",
        "dso" => "compiled_script",
        "dts" => "shape",
        "dsq" => "animation",
        "png" | "jpg" | "jpeg" => "texture",
        "wav" | "ogg" => "sound",
        "blb" => "brick_geometry",
        "bls" => "save",
        "txt" => "text",
        "gui" => "gui",
        "dif" => "interior",
        "mis" => "mission",
        _ => "other",
    }
}

/// Lists the Add-On's files in the report. The scripts server.cs and
/// client.cs reach through their execs, when every exec in the Add-On is
/// one this can follow (a literal path outside any function); otherwise
/// `None`, and every script is read.
fn inventory(cx: &mut Ctx, scripts: &[Script]) -> Option<BTreeSet<String>> {
    // Scripts reachable from server.cs / client.cs through literal exec calls.
    let mut reachable = BTreeSet::new();
    let mut followed = 0;
    let mut queue: Vec<String> = ["server.cs", "client.cs"]
        .iter()
        .map(|m| format!("{}/{m}", cx.src.dir()).to_ascii_lowercase())
        .collect();
    while let Some(p) = queue.pop() {
        if !reachable.insert(p.clone()) {
            continue;
        }
        if let Some(s) = scripts.iter().find(|s| s.path.eq_ignore_ascii_case(&p)) {
            for c in s
                .calls
                .iter()
                .filter(|c| c.callee.eq_ignore_ascii_case("exec"))
            {
                if let Some(a) = c.args.first() {
                    queue.push(source::resolve(&s.path, literal(a)).to_ascii_lowercase());
                    if is_plain_string(a) {
                        followed += 1;
                    }
                }
            }
        }
    }
    // Every exec in the reached scripts' text is one followed above; any
    // other (a built path, one inside a function) could reach any file.
    let written: usize = cx
        .src
        .files
        .values()
        .filter(|f| reachable.contains(&f.path.to_ascii_lowercase()))
        .map(|f| {
            let text =
                tscript::without_comments(&String::from_utf8_lossy(&f.bytes)).to_ascii_lowercase();
            regex::Regex::new(r"\bexec\s*\(")
                .expect("pattern")
                .find_iter(&text)
                .count()
        })
        .sum();
    let has_server = scripts.iter().any(|s| {
        s.path
            .eq_ignore_ascii_case(&format!("{}/server.cs", cx.src.dir()))
    });
    let follows = has_server && written == followed;
    // What an Add-On says about itself, for people: read, not imported.
    let metadata = [
        "description.txt",
        "rtbinfo.txt",
        "namecheck.txt",
        "license.txt",
        "licence.txt",
        "readme.txt",
    ];
    // What the scripts could open by name: a file of a kind the game only
    // read when a script named it, that no script names, never loaded.
    let script_text: String = cx
        .src
        .files
        .values()
        .filter(|f| kind_of(&f.path) == "script")
        .map(|f| tscript::without_comments(&String::from_utf8_lossy(&f.bytes)).to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("\n");
    for f in cx.src.files.values() {
        let member = cx.src.member(f).to_ascii_lowercase();
        let kind = kind_of(&f.path);
        let (status, notes) = match kind {
            "script"
                if f.path
                    .to_ascii_lowercase()
                    .starts_with(&format!("{}/client", cx.src.dir().to_ascii_lowercase()))
                    || member == "client.cs" =>
            {
                (
                    "unsupported",
                    vec![
                        "client script: TorqueScript is not run; client code must be sandboxed WebAssembly (principle 10)"
                            .into(),
                    ],
                )
            }
            "script" => (
                "consumed",
                if reachable.contains(&f.path.to_ascii_lowercase()) {
                    vec!["read for datablocks, functions and calls; not executed".into()]
                } else if follows {
                    vec!["no exec from server.cs reaches it, so v20 never ran it; left out".into()]
                } else {
                    vec!["not reached by a literal exec from server.cs; read anyway".into()]
                },
            ),
            "compiled_script" => (
                "unsupported",
                vec!["compiled DSO bytecode; only source .cs is read".into()],
            ),
            "text" if metadata.contains(&member.as_str()) => ("consumed", vec!["metadata".into()]),
            // A folder's own description (`grenade/Description.txt`, left
            // from an Add-On merged into this one): Blockland reads only the
            // root's.
            "text"
                if member
                    .rsplit_once('/')
                    .is_some_and(|(_, file)| metadata.contains(&file)) =>
            {
                (
                    "skipped",
                    vec!["a subfolder's description; Blockland reads only the Add-On's own, so it is not game data".into()],
                )
            }
            "shape" | "texture" | "sound" | "brick_geometry" => ("pending", vec![]),
            "text" | "other"
                if !script_text.contains(member.rsplit('/').next().unwrap_or(&member)) =>
            {
                (
                    "skipped",
                    vec![
                        "no script names it, so the game never loaded it (an editor file or a copy's leftover)"
                            .into(),
                    ],
                )
            }
            _ => (
                "unsupported",
                vec![format!("no native importer for {kind} files")],
            ),
        };
        if status == "unsupported" {
            cx.report.unsupported.push(Finding {
                what: format!("file {member}"),
                source: None,
                detail: notes.join("; "),
                resolution: None,
            });
        }
        cx.report.assets.push(AssetEntry {
            source: f.path.clone(),
            sha256: hash(&f.bytes),
            kind: kind.into(),
            status: status.into(),
            output: None,
            id: None,
            notes,
        });
    }
    for (member, why) in &cx.src.refused {
        cx.report.unsupported.push(Finding {
            what: format!("archive member {member}"),
            source: None,
            detail: why.clone(),
            resolution: None,
        });
    }
    follows.then_some(reachable)
}

/// A script argument that is one quoted string, with nothing joined on.
fn is_plain_string(arg: &str) -> bool {
    let a = arg.trim();
    a.len() >= 2 && a.starts_with('"') && a.ends_with('"') && !a[1..a.len() - 1].contains('"')
}

/// Whether a script's quoted path is a file of an Add-On the base game
/// ships (`"add-ons/weapon_rocket_launcher/server.cs"`).
fn ships_with_game(arg: &str) -> bool {
    if !is_plain_string(arg) {
        return false;
    }
    let path = literal(arg).to_ascii_lowercase();
    path.strip_prefix("add-ons/")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(addon, _)| reference::base_package(addon).is_some())
}

const KNOWN_TOP_LEVEL: &[&str] = &[
    "exec",
    "forcerequiredaddon",
    "loadrequiredaddon",
    "adddamagetype",
    // Support_SpecialKills' messages, read as special kills.
    "addspecialdamagemsg",
    "activatepackage",
    "error",
    "echo",
    "warn",
];

fn top_level(cx: &mut Ctx, scripts: &[Script]) {
    let write = regex::Regex::new(r"(?m)^\s*([A-Za-z_]\w*)\.(\w+)\s*=\s*([^;=][^;]*);")
        .expect("static regex");
    for s in scripts {
        for c in &s.calls {
            let callee = c.callee.to_ascii_lowercase();
            let at = Some(Location::new(&s.path, c.line));
            if callee == "exec" {
                let target = c.args.first().map(|a| source::resolve(&s.path, literal(a)));
                if let Some(t) = target
                    && cx.src.get(&t).is_none()
                {
                    cx.ambiguous(
                        format!("exec {t}"),
                        at,
                        "executes a script this Add-On does not contain".into(),
                        None,
                    );
                }
            } else if callee == "isfile" && c.args.len() == 1 && ships_with_game(&c.args[0]) {
                // `isFile("add-ons/weapon_rocket_launcher/server.cs")`: a
                // check for an Add-On the base game ships, always there.
            } else if callee == "isfile" {
                // A query with no effect of its own: a top-level `if` choosing
                // between another Add-On's files and the Add-On's own.
                cx.ambiguous(
                    format!("isFile({})", c.args.join(", ")),
                    at,
                    "checks at load whether a file outside this Add-On exists; read as absent, so the Add-On uses its own".into(),
                    None,
                );
            } else if callee == "rtb_registerpref" && c.receiver.is_none() {
                for (_, pref) in rtb::prefs(std::slice::from_ref(c)) {
                    let reason = match &pref.setting {
                        Ok(def) => format!(
                            "an RTB server preference ({}): it becomes a server setting the host changes once a port's rules read it, and keeps its default {} until then",
                            def.title, def.default
                        ),
                        Err(e) => {
                            format!("an RTB server preference that cannot be a server setting: {e}")
                        }
                    };
                    cx.unsupported(
                        format!("RTB_registerPref {}", pref.global),
                        at.clone(),
                        reason,
                    );
                }
            } else if callee == "isfunction" && c.receiver.is_none() && c.args.len() == 1 {
                // `isFunction(registerPreferenceAddon)`: Blockland Glass's
                // preference grouping, or a script's own function.
                let name = literal(&c.args[0]).trim().to_ascii_lowercase();
                let defined = scripts
                    .iter()
                    .flat_map(|s| &s.functions)
                    .any(|f| f.qualified().eq_ignore_ascii_case(&name));
                cx.ambiguous(
                    format!("isFunction({})", c.args[0]),
                    at,
                    if defined {
                        "checks at load whether a function exists; this Add-On defines it, so read as there".into()
                    } else {
                        "checks at load whether a function exists; nothing outside this Add-On defines functions here (no Blockland Glass), so read as absent".into()
                    },
                    None,
                );
            } else if callee.starts_with("register") && callee.contains("event") {
                cx.unsupported(
                    format!("{}({})", c.callee, c.args.join(", ")),
                    at,
                    "registers a brick event; add-on events need the event system's open output set (door-closer 7)".into(),
                );
            } else if !KNOWN_TOP_LEVEL.contains(&callee.as_str())
                && c.receiver.is_none()
                && !behaviour_pure(&callee)
            {
                cx.unsupported(
                    format!("top-level call {}", c.callee),
                    at,
                    "runs when the Add-On loads; no load-time hook exists (principle 6 lifecycle)"
                        .into(),
                );
            }
        }
        for g in &s.globals {
            let at = Some(Location::new(&s.path, g.line));
            cx.ambiguous(
                format!("global {} = {}", g.name, g.value),
                at,
                "script global set at load; kept only as evidence".into(),
                None,
            );
        }
        for o in &s.objects {
            cx.unsupported(
                format!("new {} at load", o.callee),
                Some(Location::new(&s.path, o.line)),
                "creates a scene object when the Add-On loads".into(),
            );
        }
        // Writes into other objects' fields at load, e.g. `GunItem.uiName = "";`.
        let text = cx.script_text(&s.path).unwrap_or_default();
        let only_if_off = only_when_a_required_add_on_is_off(&text);
        for (i, line) in text.lines().enumerate() {
            if let Some(c) = write.captures(line) {
                let object = c[1].to_owned();
                if let Some((_, addon)) = only_if_off.iter().find(|(lines, _)| lines.contains(&i)) {
                    // v20 force-loads a required Add-On the player had
                    // off, and scripts hide what it adds; here turning this
                    // package on turns its dependencies on with it (the
                    // base game's are always on), so this never runs.
                    cx.ambiguous(
                        format!("{}.{} = {}", object, &c[2], c[3].trim()),
                        Some(Location::new(&s.path, i + 1)),
                        format!("runs only when the required {addon} was turned off"),
                        Some(format!(
                            "never runs: turning this package on turns {addon} on with it"
                        )),
                    );
                    continue;
                }
                let detail = if cx.is_owned(&object) {
                    "changes one of this Add-On's datablocks at load".to_string()
                } else if let Some(o) = cx.reference.datablocks.get(&object.to_ascii_lowercase()) {
                    let addon = o.addon.clone();
                    cx.used(&addon, object.clone());
                    format!(
                        "changes {addon}'s datablock at load; a package cannot edit another package's content"
                    )
                } else {
                    "changes an object this import cannot resolve".into()
                };
                cx.unsupported(
                    format!("{}.{} = {}", object, &c[2], c[3].trim()),
                    Some(Location::new(&s.path, i + 1)),
                    detail,
                );
            }
        }
    }
}

/// The lines (0-based) of each `if (%e == $Error::AddOn_Disabled)` body
/// whose `%e` came from `ForceRequiredAddOn("X")`, with that `X`: what a
/// script does only when v20 force-loaded an Add-On the player had off.
fn only_when_a_required_add_on_is_off(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let required = regex::Regex::new(r#"(?i)%(\w+)\s*=\s*forcerequiredaddon\s*\(\s*"([^"]+)""#)
        .expect("static regex");
    let test = regex::Regex::new(
        r"(?i)\bif\s*\(\s*(?:%(\w+)\s*==\s*\$error::addon_disabled|\$error::addon_disabled\s*==\s*%(\w+))\s*\)",
    )
    .expect("static regex");
    let addons: BTreeMap<String, String> = required
        .captures_iter(text)
        .map(|c| (c[1].to_ascii_lowercase(), c[2].to_owned()))
        .collect();
    let line_of = |at: usize| text[..at].matches('\n').count();
    let mut out = Vec::new();
    for c in test.captures_iter(text) {
        let var = c
            .get(1)
            .or(c.get(2))
            .map(|m| m.as_str().to_ascii_lowercase());
        let Some(addon) = var.and_then(|v| addons.get(&v)) else {
            continue;
        };
        let rest = &text[c.get(0).map_or(0, |m| m.end())..];
        let body_start = text.len() - rest.len();
        let trimmed = rest.trim_start();
        let open = body_start + (rest.len() - trimmed.len());
        // A braced body runs to its matching brace; a bare one to its `;`.
        let end = if trimmed.starts_with('{') {
            let mut depth = 0usize;
            trimmed
                .char_indices()
                .find_map(|(i, ch)| {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(open + i);
                            }
                        }
                        _ => {}
                    }
                    None
                })
                .unwrap_or(text.len())
        } else {
            trimmed.find(';').map_or(text.len(), |i| open + i)
        };
        out.push((line_of(open)..line_of(end) + 1, addon.clone()));
    }
    out
}

fn behaviour_pure(callee: &str) -> bool {
    ["if", "else"].contains(&callee)
}

fn datablocks(cx: &mut Ctx, scripts: &[Script]) {
    for s in scripts {
        for d in &s.datablocks {
            let key = d.name.to_ascii_lowercase();
            if cx.owned.contains_key(&key) {
                cx.ambiguous(
                    format!("datablock {}", d.name),
                    Some(Location::new(&s.path, d.line)),
                    "declared more than once; Torque keeps the last declaration".into(),
                    Some("last one kept".into()),
                );
            }
            if let Some(o) = cx.reference.datablocks.get(&key) {
                cx.ambiguous(
                    format!("datablock {}", d.name),
                    Some(Location::new(&s.path, d.line)),
                    format!("same name as {}'s datablock; in Torque the later load overwrites the other", o.addon),
                    Some("the Add-On's own declaration is imported under its namespace".into()),
                );
            }
            let mut d = d.clone();
            resolve_file_fields(&mut d, &s.path);
            cx.owned.insert(
                key,
                Owned {
                    d,
                    path: s.path.clone(),
                    sha256: s.sha256.clone(),
                    fields: BTreeMap::new(),
                },
            );
        }
    }
    // Merge inherited fields: own parents first, then the reference install's.
    let names: Vec<String> = cx.owned.keys().cloned().collect();
    for name in names {
        let mut chain = vec![];
        let mut next = Some(name.clone());
        let mut missing = None;
        while let Some(n) = next.take() {
            if chain.len() > 64 || chain.iter().any(|(c, _): &(String, _)| *c == n) {
                break;
            }
            if let Some(o) = cx.owned.get(&n) {
                chain.push((n.clone(), o.d.fields.clone()));
                next = o.d.parent.as_ref().map(|p| p.to_ascii_lowercase());
            } else if let Some(o) = cx.reference.datablocks.get(&n) {
                let addon = o.addon.clone();
                chain.push((n.clone(), o.datablock.fields.clone()));
                next = o.datablock.parent.as_ref().map(|p| p.to_ascii_lowercase());
                cx.used(&addon, format!("{} (inherited)", o.datablock.name));
            } else {
                missing = Some(n);
            }
        }
        let mut fields = BTreeMap::new();
        for (_, f) in chain.into_iter().rev() {
            fields.extend(f);
        }
        let o = cx.owned.get_mut(&name).expect("owned");
        o.fields = fields;
        let (dn, class, parent, at) = (
            o.d.name.clone(),
            o.d.class.clone(),
            o.d.parent.clone(),
            Location::new(&o.path, o.d.line),
        );
        if let Some(m) = missing {
            cx.ambiguous(
                format!("parent {m} of {dn}"),
                Some(at.clone()),
                "inherits from a datablock neither this Add-On nor the reference install declares; inherited fields are unknown".into(),
                None,
            );
        }
        cx.report.datablocks.push(DatablockEntry {
            name: dn,
            class,
            parent,
            source: at,
            recognised_as: "unknown".into(),
            status: "unsupported".into(),
            ids: vec![],
            notes: vec![],
        });
    }
    cx.report.datablocks.sort_by(|a, b| a.source.cmp(&b.source));
}

/// Checks every field that names a datablock, file or damage type.
fn references(cx: &mut Ctx) {
    let ident = regex::Regex::new(r"^[A-Za-z_]\w*$").expect("static regex");
    let mut checks = vec![];
    for o in cx.owned.values() {
        for (k, v) in &o.d.fields {
            checks.push((
                o.d.name.clone(),
                o.path.clone(),
                o.d.line,
                k.clone(),
                v.clone(),
            ));
        }
    }
    let damage: BTreeSet<String> = cx
        .src
        .files
        .values()
        .filter(|f| f.path.to_ascii_lowercase().ends_with(".cs"))
        .flat_map(|f| {
            bri_weapons_import::damage_types(&script_text(&f.bytes)).unwrap_or_default()
        })
        .map(|t| t.name.to_ascii_lowercase())
        .collect();
    for (owner, path, line, key, value) in checks {
        let base = key.split('[').next().unwrap_or(&key).to_owned();
        let at = Some(Location::new(&path, line));
        let v = literal(&value).trim().to_owned();
        if v.is_empty() {
            continue;
        }
        if let Some(t) = value.trim().strip_prefix("$DamageType::") {
            let t = t.to_ascii_lowercase();
            if !damage.contains(&t) && !cx.reference.damage_types.contains(&t) {
                cx.ambiguous(
                    format!("{owner}.{key} = {value}"),
                    at,
                    "damage type is declared by neither this Add-On nor the reference".into(),
                    None,
                );
            }
            continue;
        }
        if FILE_FIELDS.contains(&base.as_str()) {
            let resolved = source::resolve(&path, &v);
            let local = ["", ".png", ".jpg", ".dts", ".wav", ".blb"]
                .iter()
                .any(|e| cx.src.get(&format!("{resolved}{e}")).is_some());
            if local {
                continue;
            }
            if let Some(found) = cx.reference.has_file(&resolved) {
                let addon = cx.reference.addon_of(&found).unwrap_or_default();
                cx.used(&addon, found);
            } else {
                let case_only = cx
                    .src
                    .files
                    .keys()
                    .any(|k| k.eq_ignore_ascii_case(&resolved));
                cx.ambiguous(
                    format!("{owner}.{key} = {value}"),
                    at,
                    format!(
                        "file {resolved} is not in this Add-On{}",
                        match (cx.reference.root.is_some(), cx.reference.installed) {
                            (true, _) => " or the reference install",
                            (false, true) => " or the installed game",
                            (false, false) => "; no reference install or installed game given",
                        }
                    ),
                    if case_only {
                        Some("matches a member by case only".into())
                    } else {
                        (base == "explosionshape").then(|| {
                            "unless an enabled Add-On provides that file, the explosion shows the rocket's sphere, as v20 does for a shape it cannot load".into()
                        })
                    },
                );
            }
            continue;
        }
        if !REFERENCE_FIELDS.contains(&base.as_str()) {
            // `shotgunItem.colorShiftColor`: a field read from another datablock.
            if let Some((obj, _)) = v.split_once('.')
                && ident.is_match(obj)
                && !value.trim().starts_with('"')
                && !obj.chars().next().is_some_and(|c| c.is_ascii_digit())
                && !cx.is_owned(obj)
            {
                if let Some(o) = cx.reference.datablocks.get(&obj.to_ascii_lowercase()) {
                    let addon = o.addon.clone();
                    cx.used(&addon, obj.to_owned());
                } else {
                    cx.ambiguous(
                        format!("{owner}.{key} = {value}"),
                        at,
                        format!("reads a field of `{obj}`, which is not declared anywhere known"),
                        None,
                    );
                }
            }
            continue;
        }
        for name in v.split_whitespace() {
            if !ident.is_match(name) || cx.is_owned(name) {
                continue;
            }
            if let Some(o) = cx.reference.datablocks.get(&name.to_ascii_lowercase()) {
                let addon = o.addon.clone();
                cx.used(&addon, o.datablock.name.clone());
            } else {
                cx.ambiguous(
                    format!("{owner}.{key} = {value}"),
                    at.clone(),
                    format!(
                        "`{name}` is not declared by this Add-On{}; Torque leaves the field empty",
                        match (cx.reference.root.is_some(), cx.reference.knows_base()) {
                            (false, false) => "; no reference install or installed game given",
                            (false, true) => " or the base game",
                            (true, false) =>
                                " or the reference install (base datablocks need --installed or --core)",
                            (true, true) => ", the reference install or the base game",
                        }
                    ),
                    None,
                );
            }
        }
    }
}

fn rel_member(cx: &Ctx, path: &str) -> String {
    let dir = cx.src.dir();
    path.get(dir.len() + 1..).unwrap_or(path).to_owned()
}

fn set_asset(
    cx: &mut Ctx,
    path: &str,
    status: &str,
    output: Option<String>,
    id: Option<String>,
    note: Option<String>,
) {
    if let Some(a) = cx
        .report
        .assets
        .iter_mut()
        .find(|a| a.source.eq_ignore_ascii_case(path))
    {
        a.status = status.into();
        a.output = output;
        a.id = id;
        a.notes.extend(note);
    }
}

fn convert_files(cx: &mut Ctx) -> Result<()> {
    let files: Vec<_> = cx.src.files.values().cloned().collect();
    for f in files {
        let member = rel_member(cx, &f.path);
        let digest = hash(&f.bytes);
        match kind_of(&f.path) {
            "shape" => {
                match bri_convert::shape::read_dts(&f.bytes, content_id(&cx.ns, "shape", &member)) {
                    Ok((shape, provenance)) => {
                        let rel = format!("models/{}.shape.json", &digest[..24]);
                        cx.write(&format!("assets/{rel}"), &serde_json::to_vec(&shape)?)?;
                        let id = cx.id("asset", &member, &f.path, &format!("assets/{rel}"));
                        cx.shapes
                            .insert(f.path.to_ascii_lowercase(), (rel.clone(), shape));
                        cx.outputs.insert(f.path.to_ascii_lowercase(), rel.clone());
                        set_asset(
                            cx,
                            &f.path,
                            "converted",
                            Some(format!("assets/{rel}")),
                            Some(id),
                            None,
                        );
                        if let Some(a) = cx.report.assets.iter_mut().find(|a| a.source == f.path) {
                            a.notes.extend(provenance.warnings);
                        }
                    }
                    Err(e) => set_asset(
                        cx,
                        &f.path,
                        "failed",
                        None,
                        None,
                        Some(format!("DTS: {e:#}")),
                    ),
                }
            }
            kind @ ("texture" | "sound") => {
                let ext = f
                    .path
                    .rsplit('.')
                    .next()
                    .unwrap_or("bin")
                    .to_ascii_lowercase();
                let dir = if kind == "texture" {
                    "textures"
                } else {
                    "sounds"
                };
                let rel = format!("{dir}/{}.{ext}", &digest[..24]);
                cx.write(&format!("assets/{rel}"), &f.bytes)?;
                let id = cx.id("asset", &member, &f.path, &format!("assets/{rel}"));
                cx.outputs.insert(f.path.to_ascii_lowercase(), rel.clone());
                set_asset(
                    cx,
                    &f.path,
                    "copied",
                    Some(format!("assets/{rel}")),
                    Some(id),
                    None,
                );
            }
            "brick_geometry" => {
                let id = content_id(&cx.ns, "brick_geometry", &member);
                match bri_convert::brick::read(&f.bytes, id.clone()) {
                    Ok((brick, provenance)) => {
                        let rel = format!("bricks/{}.brick.json", &digest[..24]);
                        cx.write(&format!("assets/{rel}"), &serde_json::to_vec(&brick)?)?;
                        let id =
                            cx.id("brick_geometry", &member, &f.path, &format!("assets/{rel}"));
                        cx.outputs.insert(f.path.to_ascii_lowercase(), rel.clone());
                        set_asset(
                            cx,
                            &f.path,
                            "converted",
                            Some(format!("assets/{rel}")),
                            Some(id),
                            None,
                        );
                        if let Some(a) = cx.report.assets.iter_mut().find(|a| a.source == f.path) {
                            a.notes.extend(provenance.warnings);
                        }
                    }
                    Err(e) => set_asset(
                        cx,
                        &f.path,
                        "failed",
                        None,
                        None,
                        Some(format!("BLB: {e:#}")),
                    ),
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn weapon_definition(o: &Owned, fields: &BTreeMap<String, String>) -> bri_weapons::Definition {
    bri_weapons::Definition {
        name: o.d.name.clone(),
        class: o.d.class.clone(),
        parent: o.d.parent.clone(),
        source: bri_weapons::Evidence {
            path: o.path.clone(),
            sha256: o.sha256.clone(),
            line: o.d.line,
        },
        fields: fields.clone(),
    }
}

/// A dependency's (or the base game's) datablock, as the weapons lowering
/// reads it.
fn reference_definition(o: &reference::Owned) -> bri_weapons::Definition {
    bri_weapons::Definition {
        name: o.datablock.name.clone(),
        class: o.datablock.class.clone(),
        parent: o.datablock.parent.clone(),
        source: bri_weapons::Evidence {
            path: o.path.clone(),
            sha256: o.sha256.clone(),
            line: o.datablock.line,
        },
        fields: o.datablock.fields.clone(),
    }
}

fn weapons(cx: &mut Ctx, scripts: &[Script]) -> Result<()> {
    let is_weapon = |c: &str| WEAPON_CLASSES.iter().any(|w| w.eq_ignore_ascii_case(c));
    let mut defs: Vec<bri_weapons::Definition> = cx
        .owned
        .values()
        .filter(|o| is_weapon(&o.d.class))
        .map(|o| weapon_definition(o, &o.d.fields))
        .collect();
    // Emitters players put on bricks travel in the weapons pack's effects.
    let brick_emitters = cx.owned.values().any(|o| {
        o.d.class.eq_ignore_ascii_case("ParticleEmitterData")
            && o.fields
                .get("uiname")
                .is_some_and(|n| !literal(n).trim().is_empty())
    });
    // A field naming a global the game or the Add-On sets to a constant at
    // load (`mountPoint = $BackSlot;`) reads as its value.
    let globals = load_globals(cx, scripts);
    for d in &mut defs {
        for v in d.fields.values_mut() {
            let name = v.trim().to_ascii_lowercase();
            if name.starts_with('$')
                && let Some(value) = globals.get(&name)
            {
                *v = format!("\"{value}\"");
            }
        }
    }
    // An Add-On with sounds but no weapons (a game mode's countdown) still
    // gets a pack, holding just its sounds, so its rules play them by id.
    let has_sounds = cx.owned.values().any(|o| {
        o.d.class.eq_ignore_ascii_case("AudioProfile")
            && o.fields.get("filename").is_some_and(|f| {
                let file = source::resolve(&o.path, literal(f));
                cx.outputs.contains_key(&file.to_ascii_lowercase())
            })
    });
    if defs.is_empty() && !has_sounds && !brick_emitters {
        return Ok(());
    }
    // Pull in the dependency datablocks these name, so `lower` can resolve
    // parents and cross-references (explosion damage, colorShiftColor).
    let mut names: BTreeSet<String> = defs.iter().map(|d| d.name.to_ascii_lowercase()).collect();
    for _ in 0..8 {
        let refs: BTreeSet<String> = defs
            .iter()
            .flat_map(|d| d.fields.values().cloned().chain(d.parent.clone()))
            .flat_map(|v| {
                let v = literal(&v).to_ascii_lowercase();
                let head = v.split('.').next().unwrap_or("").to_owned();
                [v, head]
            })
            .collect();
        let add: Vec<_> = refs
            .iter()
            .filter(|r| !names.contains(*r))
            .filter_map(|r| cx.reference.datablocks.get(r))
            .filter(|o| is_weapon(&o.datablock.class))
            .map(reference_definition)
            .collect();
        if add.is_empty() {
            break;
        }
        for d in add {
            names.insert(d.name.to_ascii_lowercase());
            defs.push(d);
        }
    }
    // One image outside the native state limits (300 s, `bri_weapons` pack
    // validation) must not drop every other weapon, projectile and explosion
    // (lpsroo's fix: the Stunt Plane's contrail images wait 10000 s).
    let too_long = |d: &bri_weapons::Definition| {
        d.class.eq_ignore_ascii_case("ShapeBaseImageData")
            && d.fields.iter().any(|(k, v)| {
                (k.starts_with("statetimeoutvalue[") || k.starts_with("stateemittertime["))
                    && literal(v).trim().parse::<f32>().is_ok_and(|s| s > 300.)
            })
    };
    for d in defs.iter().filter(|d| too_long(d)) {
        cx.report.unsupported.push(Finding {
            what: format!("image {}", d.name),
            source: Some(Location::new(&d.source.path, d.source.line)),
            detail: "a state lasts over 300 s, beyond the native image state limit".into(),
            resolution: None,
        });
    }
    defs.retain(|d| !too_long(d));
    let mut pack = match bri_weapons_import::lower(defs) {
        Ok(p) => p,
        Err(e) => {
            cx.unsupported("weapon lowering".into(), None, format!("{e:#}"));
            return Ok(());
        }
    };
    // `lower` names everything `v20.<kind>.<name>`; this Add-On's content moves
    // into its namespace and a dependency's keeps the base game's (grammar
    // from docs/architecture/packages.md).
    let file = "assets/weapons.json";
    let owned_set: BTreeSet<String> = cx.owned.keys().cloned().collect();
    let owned = |name: &str| owned_set.contains(&name.to_ascii_lowercase());
    let name_of = |id: &str| id.rsplit('.').next().unwrap_or(id).to_owned();
    let remap = |cx: &Ctx, kind: &str, old: &str| {
        let n = name_of(old);
        if cx.owned.contains_key(&n) {
            content_id(&cx.ns, kind, &n)
        } else {
            dependency_id(cx, kind, &n)
        }
    };
    let mut items = BTreeMap::new();
    for (_, mut it) in std::mem::take(&mut pack.items) {
        if !owned(&it.name) {
            continue;
        }
        it.image = remap(cx, "image", &it.image);
        it.id = cx.id("weapon", &it.name, &it.name, file);
        cx.mark(
            &it.name.clone(),
            "weapon",
            "converted",
            vec![it.id.clone()],
            None,
        );
        items.insert(it.id.clone(), it);
    }
    let mut needed_projectiles = BTreeSet::new();
    let mut images = BTreeMap::new();
    for (_, mut im) in std::mem::take(&mut pack.images) {
        if !owned(&im.name) {
            continue;
        }
        im.projectile = im.projectile.map(|p| {
            if !owned(&name_of(&p)) {
                needed_projectiles.insert(p.clone());
            }
            remap(cx, "projectile", &p)
        });
        im.id = cx.id("image", &im.name, &im.name, file);
        let mut notes = vec![];
        for s in &im.states {
            if !s.script.is_empty() {
                notes.push(format!("state {} calls script {}", s.name, s.script));
            }
        }
        let status = if notes.is_empty() {
            "converted"
        } else {
            "converted_with_gaps"
        };
        if let Some(e) = cx.entry(&im.name) {
            e.recognised_as = "weapon_image".into();
            e.status = status.into();
            e.ids.push(im.id.clone());
            e.notes.extend(notes);
        }
        images.insert(im.id.clone(), im);
    }
    let mut projectiles = BTreeMap::new();
    let mut external = BTreeSet::new();
    for (old, mut p) in std::mem::take(&mut pack.projectiles) {
        if !owned(&p.name) {
            // The package the dependency becomes provides it; the packs
            // resolve it when merged.
            p.id = dependency_id(cx, "projectile", &p.name);
            if needed_projectiles.contains(&old) {
                external.insert(p.id.clone());
            }
            cx.dependency_projectiles.insert(p.id.clone(), p);
            continue;
        }
        p.sport_image = p.sport_image.map(|s| remap(cx, "image", &s));
        p.id = cx.id("projectile", &p.name, &p.name, file);
        cx.mark(
            &p.name.clone(),
            "projectile",
            "converted",
            vec![p.id.clone()],
            None,
        );
        projectiles.insert(p.id.clone(), p);
    }
    let explosions: BTreeMap<_, _> = std::mem::take(&mut pack.explosions)
        .into_iter()
        .filter(|(_, e)| owned(&e.name))
        .collect();
    for e in explosions.values() {
        let has_fx = cx.owned.get(&e.name.to_ascii_lowercase()).is_some_and(|o| {
            o.fields.keys().any(|k| {
                k.starts_with("debris") || k.starts_with("emitter") || k.starts_with("particle")
            })
        });
        let id = cx.id("explosion", &e.name, &e.name, file);
        cx.mark(
            &e.name.clone(),
            "explosion",
            if has_fx {
                "converted_with_gaps"
            } else {
                "converted"
            },
            vec![id],
            has_fx.then(|| {
                "debris and particle parts go to the effects importers, which are vanilla-only"
                    .into()
            }),
        );
    }
    let item_data: Vec<(String, bri_weapons::Definition, String, usize)> = cx
        .owned
        .values()
        .filter(|o| o.d.class.eq_ignore_ascii_case("ItemData"))
        .map(|o| {
            (
                o.d.name.clone(),
                weapon_definition(o, &o.fields),
                o.path.clone(),
                o.d.line,
            )
        })
        .collect();
    for (name, definition, path, line) in item_data {
        if items.values().any(|i| i.name.eq_ignore_ascii_case(&name)) {
            continue;
        }
        let at = Location::new(&path, line);
        let named = definition
            .fields
            .get("uiname")
            .is_some_and(|n| !literal(n).trim().is_empty());
        if let Some(mut it) = bri_weapons_import::pickup_item(&definition) {
            // Picked up, held by nobody: an ammo box an `on_pickup` rule
            // answers.
            it.id = cx.id("weapon", &name, &name, file);
            cx.mark(&name, "weapon", "converted", vec![it.id.clone()], None);
            items.insert(it.id.clone(), it);
        } else if !named {
            cx.ambiguous(
                format!("item {name}"),
                Some(at),
                "no uiName: v20 hides such an item from players and only scripts mount its image, so the item is left out and its image kept".into(),
                None,
            );
        } else {
            cx.report.unsupported.push(Finding {
                what: format!("item {name}"),
                source: Some(at),
                detail: "an ItemData whose image did not convert".into(),
                resolution: None,
            });
        }
    }
    pack.items = items;
    pack.images = images;
    pack.projectiles = projectiles;
    pack.external_projectiles = external;
    pack.explosions = explosions;
    pack.id = cx.ns.clone();
    pack.definitions.retain(|d| owned(&d.name));
    // Its debris, which its explosions throw (`ExplosionData.debris`).
    pack.definitions.extend(
        cx.owned
            .values()
            .filter(|o| o.d.class.eq_ignore_ascii_case("DebrisData"))
            .map(|o| weapon_definition(o, &o.d.fields)),
    );
    // Damage types this Add-On declares.
    let texts: Vec<_> = scripts
        .iter()
        .filter_map(|s| cx.script_text(&s.path))
        .collect();
    // Special kills (Support_SpecialKills' `addSpecialDamageMsg`) are laid
    // over them when a rule calls a kill special.
    for t in texts.iter().flat_map(|t| {
        bri_weapons_import::damage_types(t)
            .unwrap_or_default()
            .into_iter()
            .chain(bri_weapons_import::special_kills(t).unwrap_or_default())
    }) {
        let missing: Vec<_> = t
            .icons()
            .filter(|i| {
                cx.src.get(&format!("{i}.png")).is_none() && cx.reference.has_file(i).is_none()
            })
            .collect();
        // A kill icon that is not there: the messages lose it and keep
        // the rest, so kills still read as this weapon's.
        let mut t = t;
        if !missing.is_empty() {
            cx.report.diagnostics.push(format!(
                "damage type {} icon missing, left out of its messages: {}",
                t.name,
                missing.join(", ")
            ));
            t.remove_icons(&missing);
        }
        cx.id("damage_type", &t.name, &t.name, file);
        cx.ambiguous(
            format!("damage type {}", t.name),
            None,
            "damage types are keyed by bare name in the weapons pack; two Add-Ons declaring the same name collide".into(),
            None,
        );
        pack.damage_types.insert(t.name.to_ascii_lowercase(), t);
    }
    // Resources: the models and textures this pack names, from this package.
    for (vp, rel) in &cx.outputs {
        let f = cx.src.get(vp).expect("converted from source");
        pack.resources.push(bri_weapons::Resource {
            path: f.path.clone(),
            sha256: hash(&f.bytes),
            native_file: Some(rel.clone()),
            diagnostics: vec![],
            package: None,
        });
    }
    weapon_fx::weapon_effects(cx, &mut pack);
    cx.report
        .diagnostics
        .extend(pack.diagnostics.iter().map(|d| format!("weapons: {d}")));
    pack.validate().context("imported weapons pack")?;
    let bytes = serde_json::to_vec_pretty(&pack)?;
    cx.write(file, &bytes)?;
    presentation(cx, &pack, &hash(&bytes))?;
    Ok(())
}

/// Item presentation and drop physics for this package's weapons, in the
/// base game's `item-presentation` schema, so clients draw imported items
/// and hosts give them pickup bounds. Models and textures a dependency owns
/// are named by their source path and come from that package.
fn presentation(cx: &mut Ctx, pack: &bri_weapons::Pack, weapons_sha256: &str) -> Result<()> {
    use serde_json::Map;
    let (mut models, mut textures) = (Map::new(), Map::new());
    fn texture(
        cx: &mut Ctx,
        textures: &mut Map<String, serde_json::Value>,
        reference: &str,
    ) -> Option<String> {
        for ext in ["", ".png", ".jpg", ".jpeg"] {
            let key = format!("{reference}{ext}").to_ascii_lowercase();
            if textures.contains_key(&key) {
                return Some(key);
            }
            let (Some(f), Some(rel)) = (cx.src.get(&key), cx.outputs.get(&key)) else {
                continue;
            };
            let dims = image::ImageReader::new(std::io::Cursor::new(&f.bytes))
                .with_guessed_format()
                .ok()
                .and_then(|r| r.into_dimensions().ok());
            let Some((width, height)) = dims else {
                cx.report
                    .diagnostics
                    .push(format!("presentation: unreadable image {key}"));
                return None;
            };
            textures.insert(
                key.clone(),
                json!({ "file": rel, "sha256": hash(&f.bytes), "width": width, "height": height, "source": key }),
            );
            return Some(key);
        }
        None
    }
    // Casings and explosion debris draw their own models too.
    let debris: BTreeSet<String> = bri_weapons::debris::casings(pack)
        .into_values()
        .map(|c| c.debris.model)
        .chain(
            bri_weapons::debris::explosion_debris(pack)
                .into_values()
                .map(|d| d.model),
        )
        .map(|m| m.replace('\\', "/").to_ascii_lowercase())
        .filter(|m| !m.is_empty())
        .collect();
    let referenced: BTreeSet<String> = pack
        .items
        .values()
        .map(|i| i.model.to_ascii_lowercase())
        .chain(pack.images.values().map(|i| i.model.to_ascii_lowercase()))
        .chain(
            pack.projectiles
                .values()
                .map(|p| p.model.to_ascii_lowercase()),
        )
        .chain(
            debris
                .iter()
                .filter(|m| cx.shapes.contains_key(*m))
                .cloned(),
        )
        .collect();
    // The Add-On's own particle textures, loaded with its item images.
    for p in &pack.effects.particles {
        if !p.texture.starts_with("base/") && texture(cx, &mut textures, &p.texture).is_none() {
            cx.report.diagnostics.push(format!(
                "presentation: particle texture {} did not load",
                p.texture
            ));
        }
    }
    for key in &referenced {
        let (Some(f), Some((rel, shape))) = (cx.src.get(key).cloned(), cx.shapes.get(key).cloned())
        else {
            continue; // Another package's model, or none.
        };
        let Ok((lo, hi)) = bri_vehicles_import::dts_bounds(&f.bytes) else {
            cx.report
                .diagnostics
                .push(format!("presentation: no DTS bounds for {key}"));
            continue;
        };
        let folder = f.path.rsplit_once('/').map_or("", |(d, _)| d).to_owned();
        let mut bindings = vec![];
        for m in &shape.materials {
            // Torque looks for a material's texture beside the shape, then
            // in each folder above it; a material it finds nowhere is drawn
            // untextured (white, under the item's colour shift).
            let found = texture_folders(&folder)
                .find_map(|dir| texture(cx, &mut textures, &format!("{dir}/{}", m.name)))
                .or_else(|| cx.reference.base_texture(&m.name));
            match found {
                Some(t) => bindings.push(t),
                None => {
                    // Torque drew a material whose bitmap it could not find
                    // untextured, in the item's colour shift (white without
                    // one), and kept the rest of the model. A clear texture
                    // shows the tint through, as a colour-shift model's
                    // clear texels do.
                    cx.report.diagnostics.push(format!(
                        "presentation: {key} material {} has no texture; drawn in the colour shift (white without one), as Torque drew it",
                        m.name
                    ));
                    bindings.push(flat_texture(cx, &mut textures, Flat::Clear)?);
                }
            }
        }
        let native = std::fs::read(cx.out.join("assets").join(&rel))?;
        models.insert(
            key.clone(),
            json!({
                "file": rel, "sha256": hash(&native), "source": f.path, "source_sha256": hash(&f.bytes),
                "textures": bindings,
                // Torque (x, y, z) to native (x, z, -y); the Z interval flips.
                "bounds_min": [lo.x, lo.z, -hi.y], "bounds_max": [hi.x, hi.z, -lo.y],
            }),
        );
    }
    // This package's own models that did not convert get a small placeholder
    // cube, so every item it declares can be drawn and picked up. Models of
    // other packages keep their key; that package presents them.
    let own = format!("{}/", cx.src.dir().to_ascii_lowercase());
    let missing: Vec<String> = referenced
        .iter()
        .filter(|k| k.starts_with(&own) && !models.contains_key(*k))
        .cloned()
        .collect();
    if !missing.is_empty() {
        let shape = placeholder();
        let shape_bytes = serde_json::to_vec(&shape)?;
        let shape_rel = format!("models/{}.shape.json", &hash(&shape_bytes)[..24]);
        cx.write(&format!("assets/{shape_rel}"), &shape_bytes)?;
        flat_texture(cx, &mut textures, Flat::White)?;
        for key in missing {
            cx.report.ambiguous.push(Finding {
                what: format!("model {key}"),
                source: None,
                detail: "not in this Add-On or did not convert; presented as a placeholder cube"
                    .into(),
                resolution: Some(shape_rel.clone()),
            });
            models.insert(
                key,
                json!({
                    "file": shape_rel, "sha256": hash(&shape_bytes), "source": "placeholder",
                    "source_sha256": hash(&shape_bytes), "textures": ["placeholder:white"],
                    "bounds_min": [-0.1, -0.1, -0.1], "bounds_max": [0.1, 0.1, 0.1],
                }),
            );
        }
    }
    let tint = |on: bool, c: [f32; 4]| {
        if on {
            c.map(|v| v.clamp(0.0, 1.0))
        } else {
            [1.0; 4]
        }
    };
    let (mut items, mut images, mut projectiles, mut physics) =
        (Map::new(), Map::new(), Map::new(), Map::new());
    for (id, im) in &pack.images {
        // Read as the weapons pack reads it (axis-angle or `eulerToMatrix`,
        // inherited); the client applies `eulerToMatrix`'s turn by name.
        let eye = im.eye_rotation;
        images.insert(
            id.clone(),
            json!({
                "model": im.model.to_ascii_lowercase(), "mount_point": im.mount_point, "offset": im.offset,
                "eye_offset": im.eye_offset, "source_rotation_degrees": im.source_rotation_degrees,
                "eye_rotation_degrees": eye, "tint": tint(im.color_shift, im.color),
                "evidence": pack.definitions.iter().find(|d| d.name == im.name).map(|d| &d.source),
            }),
        );
    }
    for (id, it) in &pack.items {
        let model = it.model.to_ascii_lowercase();
        let Some(m) = models.get(&model) else {
            cx.report.diagnostics.push(format!(
                "presentation: item {id} model {model} was not converted"
            ));
            continue;
        };
        physics.insert(
            id.clone(),
            json!({ "min": m["bounds_min"], "max": m["bounds_max"] }),
        );
        // An item shows its image's colour shift.
        let image = pack.images.get(&it.image);
        let icon = if it.icon.is_empty() {
            None
        } else {
            texture(cx, &mut textures, &it.icon)
        };
        items.insert(
            id.clone(),
            json!({
                "model": model, "image": it.image,
                "tint": image.map_or([1.0; 4], |i| tint(i.color_shift, i.color)), "icon": icon,
                "evidence": pack.definitions.iter().find(|d| d.name == it.name).map(|d| &d.source),
            }),
        );
    }
    for (id, p) in &pack.projectiles {
        let model = p.model.to_ascii_lowercase();
        projectiles.insert(
            id.clone(),
            json!({ "model": (!model.is_empty()).then_some(model), "tint": [1.0, 1.0, 1.0, 1.0] }),
        );
    }
    let physics = serde_json::to_vec_pretty(&json!({ "schema_version": 1, "items": physics }))?;
    cx.write("assets/item-physics.json", &physics)?;
    let manifest = json!({
        "schema_version": 2, "id": format!("{}:item-presentation/main", cx.ns),
        "weapons_sha256": weapons_sha256, "item_physics_sha256": hash(&physics),
        "models": models, "textures": textures, "items": items, "images": images,
        "projectiles": projectiles, "diagnostics": [],
    });
    cx.write(
        "assets/presentation.json",
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}

/// A 1x1 texture of one colour, written once.
#[derive(Clone, Copy)]
enum Flat {
    /// Opaque white: the placeholder cube.
    White,
    /// Clear white: a material with no texture, showing the tint.
    Clear,
}

/// Where Torque looked for a shape's material texture: the shape's folder,
/// then each folder above it.
fn texture_folders(folder: &str) -> impl Iterator<Item = &str> {
    std::iter::successors(Some(folder), |d| d.rsplit_once('/').map(|(up, _)| up))
}

fn flat_texture(
    cx: &mut Ctx,
    textures: &mut serde_json::Map<String, serde_json::Value>,
    flat: Flat,
) -> Result<String> {
    let (key, alpha) = match flat {
        Flat::White => ("placeholder:white", 255),
        Flat::Clear => ("placeholder:clear", 0),
    };
    if !textures.contains_key(key) {
        let mut png = vec![];
        image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, alpha]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("in-memory PNG");
        let rel = format!("textures/{}.png", &hash(&png)[..24]);
        cx.write(&format!("assets/{rel}"), &png)?;
        textures.insert(
            key.into(),
            json!({ "file": rel, "sha256": hash(&png), "width": 1, "height": 1, "source": "placeholder" }),
        );
    }
    Ok(key.into())
}

/// A 0.2 unit cube with one white material.
fn placeholder() -> bri_content::shape::Shape {
    use bri_content::shape::*;
    let mut positions = vec![];
    let mut normals = vec![];
    let mut uv = vec![];
    let mut triangles = vec![];
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let base = positions.len() as u32;
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut p = [0.0f32; 3];
                p[axis] = 0.1 * sign;
                p[u] = 0.1 * a;
                p[v] = 0.1 * b * sign;
                let mut n = [0.0f32; 3];
                n[axis] = sign;
                positions.push(p);
                normals.push(n);
                uv.push([(a + 1.0) / 2.0, (b + 1.0) / 2.0]);
            }
            triangles.push([base, base + 1, base + 2]);
            triangles.push([base, base + 2, base + 3]);
        }
    }
    Shape {
        schema_version: 1,
        id: "placeholder:shape/cube".into(),
        nodes: vec![Node {
            name: "root".into(),
            parent: None,
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
        }],
        objects: vec![Object {
            name: "cube".into(),
            node: Some(0),
            meshes: vec![0],
            visibility: 1.0,
            frame: 0,
            material_frame: 0,
        }],
        details: vec![Detail {
            name: "detail0".into(),
            pixel_threshold: 0.0,
            object_start: 0,
            object_count: 1,
            mesh_offset: 0,
            collision: false,
        }],
        meshes: vec![Some(Mesh {
            frame_vertices: positions.len(),
            positions,
            normals,
            uv,
            primitives: vec![Primitive {
                material: Some(0),
                triangles,
            }],
            skin: None,
            billboard: false,
            billboard_y: false,
        })],
        materials: vec![Material {
            name: "white".into(),
            wrap_u: true,
            wrap_v: true,
            blend: "opaque".into(),
            unlit: false,
            environment: false,
            mipmaps: false,
            detail_map: None,
            bump_map: None,
            reflectance_map: None,
            detail_scale: 1.0,
            reflectance: 0.0,
            metal: None,
        }],
        animations: vec![],
    }
}

fn vehicles(cx: &mut Ctx, scripts: &[Script]) -> Result<()> {
    use bri_vehicles::schema::{Asset, Evidence, Pack, SCHEMA_VERSION};
    use bri_vehicles_import::Block;
    let wanted: Vec<(String, bri_vehicles::schema::Family)> = cx
        .owned
        .values()
        .filter_map(|o| {
            VEHICLE_CLASSES
                .iter()
                .find(|(c, _)| c.eq_ignore_ascii_case(&o.d.class))
                .map(|(_, f)| (o.d.name.clone(), *f))
        })
        .collect();
    for o in cx
        .owned
        .values()
        .filter(|o| o.d.class.to_ascii_lowercase().ends_with("vehicledata"))
    {
        if !wanted.iter().any(|(n, _)| n == &o.d.name) {
            cx.report.unsupported.push(Finding {
                what: format!("vehicle {}", o.d.name),
                source: Some(Location::new(&o.path, o.d.line)),
                detail: format!("{} has no native vehicle family", o.d.class),
                resolution: None,
            });
        }
    }
    if wanted.is_empty() {
        return Ok(());
    }
    let block = |addon: &str,
                 name: &str,
                 fields: &BTreeMap<String, String>,
                 path: &str,
                 sha: &str,
                 line| Block {
        package: addon.into(),
        fields: fields
            .iter()
            .map(|(k, v)| (k.clone(), literal(v).trim().to_owned()))
            .collect(),
        evidence: Evidence {
            source: path.into(),
            sha256: sha.into(),
            line,
            subject: name.into(),
        },
    };
    let mut blocks = BTreeMap::new();
    for (k, o) in &cx.reference.datablocks {
        blocks.insert(
            k.clone(),
            block(
                &o.addon,
                &o.datablock.name,
                &o.datablock.fields,
                &o.path,
                &o.sha256,
                o.datablock.line,
            ),
        );
    }
    for (k, o) in &cx.owned {
        blocks.insert(
            k.clone(),
            block(
                &cx.src.name,
                &o.d.name,
                &o.fields,
                &o.path,
                &o.sha256,
                o.d.line,
            ),
        );
    }
    let files: BTreeMap<String, (String, Vec<u8>)> = cx
        .src
        .files
        .iter()
        .map(|(k, f)| (k.clone(), (f.path.clone(), f.bytes.clone())))
        .collect();
    let ns = cx.ns.clone();
    let owned_names: BTreeSet<String> = cx.owned.keys().cloned().collect();
    let id = move |kind: &str, name: &str| {
        let n = name.to_ascii_lowercase();
        content_id(if owned_names.contains(&n) { &ns } else { "v20" }, kind, &n)
    };
    let mut definitions = vec![];
    for (name, family) in &wanted {
        let o = &cx.owned[&name.to_ascii_lowercase()];
        let at = Location::new(&o.path, o.d.line);
        let parts: Vec<String> = ["defaulttire", "defaultspring"]
            .iter()
            .filter_map(|p| o.fields.get(*p).map(|v| literal(v).to_owned()))
            .collect();
        match bri_vehicles_import::lower(name, *family, &blocks, &cx.shapes, &files, &id) {
            Ok(mut d) => {
                // Its onAdd sets wheels and animations in place of v20's
                // WheeledVehicleData::onAdd, or after it with Parent::onAdd.
                let own_fields: BTreeMap<String, String> = o
                    .fields
                    .iter()
                    .map(|(k, v)| (k.clone(), literal(v).trim().to_owned()))
                    .collect();
                let setup = vehicle_script::setup(scripts, name, &own_fields);
                if setup.found && !setup.calls_parent {
                    for w in &mut d.wheels {
                        (w.steering, w.powered) = (0., true);
                    }
                }
                for &(i, v) in &setup.steering {
                    if let Some(w) = d.wheels.get_mut(i) {
                        w.steering = v;
                    }
                }
                for &(i, v) in &setup.powered {
                    if let Some(w) = d.wheels.get_mut(i) {
                        w.powered = v;
                    }
                }
                // A thread naming a sequence the model lacks plays nothing in
                // v20 either; leave it out.
                let shape = cx
                    .shapes
                    .values()
                    .find(|(p, _)| *p == d.model)
                    .map(|(_, s)| s);
                let (known, unknown): (Vec<_>, Vec<_>) = setup.threads.into_iter().partition(|t| {
                    shape.is_some_and(|s| {
                        s.animations
                            .iter()
                            .any(|a| a.name.eq_ignore_ascii_case(&t.sequence))
                    })
                });
                let node_frames: Vec<(String, glam::Mat4)> = shape
                    .map(|s| {
                        s.nodes
                            .iter()
                            .map(|n| n.name.clone())
                            .zip(bri_vehicles_import::nodes(s))
                            .collect()
                    })
                    .unwrap_or_default();
                for t in unknown {
                    cx.report.diagnostics.push(format!(
                        "vehicle {name} plays sequence {}, which its model does not have",
                        t.sequence
                    ));
                }
                d.threads = known;
                vehicle_trails(cx, name, &mut d, &setup.images, &node_frames);
                vehicle_damage_emitters(cx, name, &mut d);
                if !d.threads.is_empty() {
                    d.adaptations.push(format!(
                        "Animation threads read from {name}::onAdd and the functions it calls: {}",
                        d.threads
                            .iter()
                            .map(|t| format!(
                                "slot {} {}{}{}",
                                t.slot,
                                t.sequence,
                                t.min_speed
                                    .map(|m| format!(" from speed {m}"))
                                    .unwrap_or_default(),
                                t.max_speed
                                    .map(|m| format!(" below speed {m}"))
                                    .unwrap_or_default()
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                let vid = cx.id("vehicle", name, name, "assets/vehicles.json");
                d.id = vid.clone();
                for p in [&d.initial_explosion, &d.final_explosion]
                    .into_iter()
                    .flatten()
                {
                    if !cx.report.ids.iter().any(|e| &e.id == p) && !p.starts_with("v20:") {
                        cx.report.diagnostics.push(format!("vehicle {name} names projectile {p}, which the weapons pack does not declare"));
                    }
                }
                cx.mark(name, "vehicle", "converted", vec![vid], None);
                for p in &parts {
                    cx.mark(
                        p,
                        "vehicle_part",
                        "consumed",
                        vec![],
                        Some(format!("folded into {name}")),
                    );
                }
                definitions.push(d);
            }
            Err(e) => cx.unsupported(
                format!("vehicle {name}"),
                Some(at),
                format!("vehicle lowering failed: {e:#}"),
            ),
        }
    }
    if definitions.is_empty() {
        return Ok(());
    }
    let mut assets = vec![];
    for (vp, rel) in &cx.outputs {
        let f = cx.src.get(vp).expect("converted from source");
        let bytes = std::fs::read(cx.out.join("assets").join(rel))?;
        assets.push(Asset {
            virtual_path: f.path.clone(),
            path: rel.clone(),
            sha256: hash(&bytes),
            kind: match kind_of(&f.path) {
                "shape" => "model",
                k => k,
            }
            .into(),
            source_sha256: hash(&f.bytes),
            package: None,
        });
    }
    let evidence = wanted
        .iter()
        .filter_map(|(n, _)| {
            blocks
                .get(&n.to_ascii_lowercase())
                .map(|b| b.evidence.clone())
        })
        .collect();
    let pack = Pack {
        schema_version: SCHEMA_VERSION,
        definitions,
        assets,
        evidence,
        unresolved: vec!["Imported by bri-import-addon; see import-report.json".into()],
        animation_aliases: BTreeMap::new(),
    };
    pack.validate().context("imported vehicles pack")?;
    cx.write("assets/vehicles.json", &serde_json::to_vec_pretty(&pack)?)?;
    pack.verify_assets(cx.out.join("assets"))?;
    Ok(())
}

/// The images a vehicle's script mounts on it (`mountImage`) that run an
/// emitter and hold it, as trails at their mount nodes, with the Add-On's own
/// particles and emitters converted beside them. The Stunt Plane's
/// `contrailCheck` mounts `contrailImage1`/`2` at the wing tips from
/// `minContrailSpeed`, and their `FireA` state runs `ContrailEmitter` for
/// 10000 s.
fn vehicle_trails(
    cx: &mut Ctx,
    vehicle: &str,
    d: &mut bri_vehicles::Definition,
    mounts: &[vehicle_script::ImageMount],
    nodes: &[(String, glam::Mat4)],
) {
    for m in mounts {
        match vehicle_trail(cx, d, m, nodes) {
            Ok(t) => {
                cx.mark(
                    &m.image,
                    "vehicle_trail",
                    "consumed",
                    vec![],
                    Some(format!(
                        "folded into {vehicle} as a trail at node {}",
                        t.node
                    )),
                );
                // A trail holds the emitter however long its state lasts.
                let what = format!("image {}", m.image);
                cx.report
                    .unsupported
                    .retain(|f| !f.what.eq_ignore_ascii_case(&what));
                d.adaptations.push(format!(
                    "Trail: {} at {} runs {}{}{}",
                    m.image,
                    t.node,
                    t.emitter,
                    t.min_speed
                        .map(|s| format!(" from speed {s}"))
                        .unwrap_or_default(),
                    t.max_speed
                        .map(|s| format!(" below speed {s}"))
                        .unwrap_or_default()
                ));
                d.trails.push(t);
            }
            Err(e) => cx.unsupported(
                format!("vehicle {vehicle} image {}", m.image),
                None,
                format!("{e:#}"),
            ),
        }
    }
}

fn vehicle_trail(
    cx: &mut Ctx,
    d: &mut bri_vehicles::Definition,
    m: &vehicle_script::ImageMount,
    nodes: &[(String, glam::Mat4)],
) -> Result<bri_vehicles::schema::Trail> {
    use glam::{Mat4, Quat, Vec3};
    let (class, fields) = if let Some(o) = cx.owned.get(&m.image) {
        (o.d.class.clone(), o.fields.clone())
    } else if let Some(o) = cx.reference.datablocks.get(&m.image) {
        (o.datablock.class.clone(), o.datablock.fields.clone())
    } else {
        anyhow::bail!("{} is not declared", m.image);
    };
    ensure!(
        class.eq_ignore_ascii_case("ShapeBaseImageData"),
        "{} is a {class}, not an image",
        m.image
    );
    let get = |k: &str| {
        fields
            .get(k)
            .map(|v| literal(v).trim().to_owned())
            .unwrap_or_default()
    };
    let seconds = |k: &str| get(k).parse::<f32>().unwrap_or(0.);
    // From state 0 along its timeouts to the first state with an emitter.
    let state_named =
        |name: &str| (0..32).find(|i| get(&format!("statename[{i}]")).eq_ignore_ascii_case(name));
    let mut state = 0;
    let mut seen = BTreeSet::new();
    while get(&format!("stateemitter[{state}]")).is_empty() {
        ensure!(
            seen.insert(state),
            "{} runs no emitter from its first state",
            m.image
        );
        state = state_named(&get(&format!("statetransitionontimeout[{state}]")))
            .with_context(|| format!("{} runs no emitter from its first state", m.image))?;
    }
    let emitter_seconds = seconds(&format!("stateemittertime[{state}]"));
    let timeout = seconds(&format!("statetimeoutvalue[{state}]"));
    let next = state_named(&get(&format!("statetransitionontimeout[{state}]")));
    // Held: it runs for minutes (the script remounts it when it ends), or
    // the state re-enters itself before the emitter stops.
    ensure!(
        emitter_seconds >= 60. || (next == Some(state) && emitter_seconds >= timeout),
        "{} runs its emitter for {emitter_seconds} s, not continuously",
        m.image
    );
    let emitter_name = get(&format!("stateemitter[{state}]")).to_ascii_lowercase();
    let emitter = if cx.is_owned(&emitter_name) {
        vehicle_emitter(cx, d, &emitter_name)?
    } else {
        ensure!(
            cx.reference.datablocks.contains_key(&emitter_name),
            "{} emits {emitter_name}, which is not declared",
            m.image
        );
        format!("v20/emitter/{emitter_name}")
    };
    let mount_point = get("mountpoint").parse::<u32>().unwrap_or(0);
    let node = format!("mount{mount_point}");
    // ShapeBase::getMountTransform: a missing node mounts at the origin.
    let at = match nodes.iter().find(|(n, _)| n.eq_ignore_ascii_case(&node)) {
        Some((_, m)) => *m,
        None => {
            cx.report.diagnostics.push(format!(
                "{} mounts at {node}, which the vehicle's model lacks; it sits at the model's origin",
                m.image
            ));
            Mat4::IDENTITY
        }
    };
    let (offset, degrees) = bri_weapons_import::image_placement(&fields)
        .with_context(|| format!("{} has a rotation that is not a literal", m.image))?;
    // As `actor_effects::image_emitter`: the image's source +Y, native -Z,
    // is the ejection axis.
    let frame =
        at * Mat4::from_rotation_translation(
            bri_weapons::rotation::native(degrees),
            Vec3::from(offset),
        ) * Mat4::from_quat(Quat::from_rotation_arc(Vec3::Y, Vec3::NEG_Z));
    Ok(bri_vehicles::schema::Trail {
        node,
        transform: bri_vehicles_import::transform(frame),
        emitter,
        min_speed: m.min_speed,
        max_speed: m.max_speed,
    })
}

/// A wreck burns with its `damageEmitter`s (`Definition::wreck_emitters`):
/// the Add-On's own are converted into the vehicle's `effects`; a base game
/// one is drawn from the base pack.
fn vehicle_damage_emitters(cx: &mut Ctx, vehicle: &str, d: &mut bri_vehicles::Definition) {
    for i in 0..3 {
        let Some(name) = d.authored.get(&format!("damageemitter[{i}]")) else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() || !cx.is_owned(&name) {
            continue;
        }
        if let Err(e) = vehicle_emitter(cx, d, &name) {
            cx.unsupported(
                format!("vehicle {vehicle} damage emitter {name}"),
                None,
                format!("{e:#}"),
            );
        }
    }
}

/// Converts one of the Add-On's emitters and its particles into the
/// vehicle's `effects` (once), returning the emitter's native id.
fn vehicle_emitter(cx: &mut Ctx, d: &mut bri_vehicles::Definition, name: &str) -> Result<String> {
    let id = content_id(&cx.ns, "emitter", name);
    if d.effects.emitters.iter().any(|e| e.id == id) {
        return Ok(id);
    }
    let (emitter, particles) = weapon_fx::convert_emitter(cx, name, "assets/vehicles.json")?;
    for p in particles {
        if !d.effects.particles.iter().any(|q| q.id == p.id) {
            d.effects.particles.push(p);
        }
    }
    let id = emitter.id.clone();
    d.effects.emitters.push(emitter);
    Ok(id)
}

/// Globals the Add-On sets at load to constant values (`$X::Path =
/// filePath(expandFileName("./server.cs"))`, `$X::Category = "Special"`), in
/// load order: a global set twice keeps the later value, as straight-line
/// execution of a fresh install (no prefs saved yet) leaves it.
fn load_globals(cx: &Ctx, scripts: &[Script]) -> bri_convert::catalog::Globals {
    let mut globals = cx.reference.globals.clone();
    for s in scripts {
        let dir = s.path.rsplit_once('/').map_or(s.path.as_str(), |(d, _)| d);
        for g in &s.globals {
            if let Some(v) = bri_convert::catalog::constant_global(&g.value, dir, &globals) {
                globals.insert(g.name.to_ascii_lowercase(), v);
            }
        }
    }
    globals
}

fn bricks(cx: &mut Ctx, scripts: &[Script]) -> Result<()> {
    let mut entries = vec![];
    let globals = load_globals(cx, scripts);
    for s in scripts {
        if !s
            .datablocks
            .iter()
            .any(|d| d.class.eq_ignore_ascii_case("fxDTSBrickData"))
        {
            continue;
        }
        let dir = s.path.rsplit_once('/').map_or(s.path.as_str(), |(d, _)| d);
        let text = cx.script_text(&s.path).expect("script");
        // Parents declared elsewhere, as declarations the reader can inherit
        // from: base bricks, and the Add-On's bricks in its other scripts
        // (a port's `datablocks.cs` inherits from the Add-On's own).
        let mut declared = BTreeMap::new();
        let elsewhere = cx
            .reference
            .datablocks
            .iter()
            .map(|(k, o)| (k, &o.datablock))
            .chain(
                cx.owned
                    .iter()
                    .filter(|(_, o)| !o.path.eq_ignore_ascii_case(&s.path))
                    .map(|(k, o)| (k, &o.d)),
            );
        for (key, d) in elsewhere {
            if d.class.eq_ignore_ascii_case("fxDTSBrickData") {
                declared.insert(
                    key.clone(),
                    format!(
                        "datablock fxDTSBrickData({}{}) {{ {} }};\n",
                        d.name,
                        d.parent
                            .as_ref()
                            .map_or(String::new(), |p| format!(" : {p}")),
                        d.fields
                            .iter()
                            .map(|(k, v)| format!("{k} = {v};"))
                            .collect::<String>()
                    ),
                );
            }
        }
        let parents: String = declared.into_values().collect();
        match bri_convert::catalog::read_with_globals(&text, dir, &parents, &globals) {
            Ok(catalog) => {
                for mut b in catalog.bricks {
                    let key = b.id.rsplit('/').next().unwrap_or("").to_owned();
                    // A newer copy of a base Add-On (Steam's Brick_Halloween)
                    // declares the base game's bricks again. In v20 that
                    // changes the same datablock, so a save's brick of that
                    // name stays the base game's, with its behaviour.
                    if cx.reference.base_bricks.contains(&key) {
                        let name = cx.owned.get(&key).map_or(key.clone(), |o| o.d.name.clone());
                        cx.mark(
                            &name,
                            "brick",
                            "consumed",
                            vec![],
                            Some("the base game's own brick, which stays the base game's".into()),
                        );
                        continue;
                    }
                    let mesh = b.mesh_id.trim_start_matches("v20/").to_owned();
                    b.mesh_id = if let Some(f) = cx
                        .outputs
                        .contains_key(&mesh)
                        .then(|| cx.src.get(&mesh))
                        .flatten()
                    {
                        content_id(&cx.ns, "brick_geometry", &rel_member(cx, &f.path))
                    } else if let Some(found) = cx.reference.has_file(&mesh) {
                        let addon = cx.reference.addon_of(&found).unwrap_or_default();
                        cx.used(&addon, found.clone());
                        // The base game's own id for that geometry, so the
                        // brick reuses the loaded shape (`Definitions::load_with`).
                        format!("v20/{found}")
                    } else {
                        let o = cx.owned.get(&key);
                        let at = o.map(|o| Location::new(&o.path, o.d.line));
                        cx.ambiguous(
                            format!("brick {key} geometry {mesh}"),
                            at,
                            "brickFile is in neither this Add-On nor the reference install".into(),
                            None,
                        );
                        String::new()
                    };
                    let name = cx.owned.get(&key).map_or(key.clone(), |o| o.d.name.clone());
                    b.id = cx.id("brick", &key, &name, "assets/bricks.json");
                    let special: Vec<String> = b
                        .other_properties
                        .keys()
                        .filter(|k| {
                            ["isbothole", "holebot", "isdoor", "isopen"].contains(&k.as_str())
                        })
                        .cloned()
                        .collect();
                    let status = if b.mesh_id.is_empty() || !special.is_empty() {
                        "converted_with_gaps"
                    } else {
                        "converted"
                    };
                    let note = (!special.is_empty()).then(|| {
                        format!(
                            "special behaviour fields {} need native behaviour",
                            special.join(", ")
                        )
                    });
                    if !special.is_empty() {
                        let o = cx.owned.get(&key);
                        let at = o.map(|o| Location::new(&o.path, o.d.line));
                        cx.unsupported(
                            format!("brick {name} special fields"),
                            at,
                            format!("{} drive script behaviour (spawning bots, doors) defined elsewhere", special.join(", ")),
                        );
                    }
                    cx.mark(&name, "brick", status, vec![b.id.clone()], note);
                    entries.push(b);
                }
            }
            Err(e) => {
                for d in s
                    .datablocks
                    .iter()
                    .filter(|d| d.class.eq_ignore_ascii_case("fxDTSBrickData"))
                {
                    cx.unsupported(
                        format!("brick {}", d.name),
                        Some(Location::new(&s.path, d.line)),
                        format!("the brick catalog reader refused {}: {e:#}", s.path),
                    );
                }
            }
        }
    }
    if !entries.is_empty() {
        let catalog = bri_content::brick::Catalog {
            schema_version: 1,
            bricks: entries,
        };
        cx.write("assets/bricks.json", &serde_json::to_vec_pretty(&catalog)?)?;
        loadable_bricks(cx, catalog)?;
    }
    Ok(())
}

/// The bricks whose geometry converted, in the base game's brick catalog
/// layout under `assets/brick-catalog/`, so `Definitions::load` reads them
/// like the stock catalog: `stock-catalog.json`, `catalog-audit.json` (mesh
/// bindings), `native-collisions.json` and the mesh files beside them.
fn loadable_bricks(cx: &mut Ctx, catalog: bri_content::brick::Catalog) -> Result<()> {
    let meshes: BTreeMap<String, String> = cx
        .outputs
        .iter()
        .filter(|(_, rel)| rel.starts_with("bricks/"))
        .filter_map(|(vp, rel)| {
            let f = cx.src.get(vp)?;
            Some((
                content_id(&cx.ns, "brick_geometry", &rel_member(cx, &f.path)),
                rel.clone(),
            ))
        })
        .collect();
    let (mut bricks, mut resolved, mut bodies) = (vec![], vec![], vec![]);
    let mut icons = serde_json::Map::new();
    for entry in catalog.bricks {
        let Some(rel) = meshes.get(&entry.mesh_id) else {
            // Geometry of the reference install (a datablock inheriting a
            // base brick's `brickFile`): no copy; the game lends the base
            // brick's shape and menu icon.
            if entry.mesh_id.starts_with("v20/") {
                resolved.push(json!({ "id": entry.id }));
                bricks.push(entry);
            }
            continue;
        };
        let bytes = std::fs::read(cx.out.join("assets").join(rel))?;
        let brick: bri_content::brick::Brick = serde_json::from_slice(&bytes)?;
        match bri_convert::collision::bake(entry.id.clone(), &brick, None) {
            Ok(body) => bodies.push(body),
            Err(e) => {
                cx.report
                    .diagnostics
                    .push(format!("brick {}: no collision: {e:#}", entry.id));
                continue;
            }
        }
        let file = rel.trim_start_matches("bricks/").to_owned();
        cx.write(&format!("assets/brick-catalog/{file}"), &bytes)?;
        resolved.push(json!({ "id": entry.id, "native_mesh": file }));
        // The brick menu's icon, stored beside the catalog.
        let icon = entry.icon_source.clone();
        if !icon.is_empty() && !icons.contains_key(&icon) {
            let found = ["", ".png", ".jpg"]
                .iter()
                .find_map(|e| cx.src.get(&format!("{icon}{e}")).cloned());
            let dims = found.as_ref().and_then(|f| {
                image::ImageReader::new(std::io::Cursor::new(&f.bytes))
                    .with_guessed_format()
                    .ok()?
                    .into_dimensions()
                    .ok()
            });
            match (found, dims) {
                (Some(f), Some((width, height))) => {
                    let ext = f
                        .path
                        .rsplit('.')
                        .next()
                        .unwrap_or("png")
                        .to_ascii_lowercase();
                    let digest = hash(&f.bytes);
                    let name = format!("icons/{}.{ext}", &digest[..24]);
                    cx.write(&format!("assets/brick-catalog/{name}"), &f.bytes)?;
                    icons.insert(
                        icon,
                        json!({ "file": name, "sha256": digest, "width": width, "height": height, "source": f.path }),
                    );
                }
                _ => cx.report.diagnostics.push(format!(
                    "brick {}: icon {icon} not found; the menu shows no icon",
                    entry.id
                )),
            }
        }
        bricks.push(entry);
    }
    if bricks.is_empty() {
        return Ok(());
    }
    let dir = "assets/brick-catalog";
    let catalog = bri_content::brick::Catalog {
        schema_version: 1,
        bricks,
    };
    cx.write(
        &format!("{dir}/stock-catalog.json"),
        &serde_json::to_vec_pretty(&catalog)?,
    )?;
    cx.write(
        &format!("{dir}/catalog-audit.json"),
        &serde_json::to_vec_pretty(&json!({ "resolved_meshes": resolved }))?,
    )?;
    cx.write(
        &format!("{dir}/brick-icons.json"),
        &serde_json::to_vec_pretty(&json!({ "schema_version": 1, "icons": icons }))?,
    )?;
    let library = bri_content::collision::CollisionLibrary {
        schema_version: 1,
        bodies,
    };
    cx.write(
        &format!("{dir}/native-collisions.json"),
        &serde_json::to_vec_pretty(&library)?,
    )?;
    Ok(())
}

/// Name, lower-case class, merged fields, own fields, file and line.
type Pending = (
    String,
    String,
    BTreeMap<String, String>,
    BTreeMap<String, String>,
    String,
    usize,
);

/// An Add-On's `PlayerData` as a package archetype ([`player_types`]): its
/// fields and those of its ancestors in this Add-On (the nearest wins),
/// over the archetype of the first one outside it, an Add-On's it depends
/// on or one of v20's player types. Returns the fields it set that no
/// archetype field carries.
fn player_archetype(cx: &Ctx, name: &str) -> Result<(serde_json::Value, Vec<String>)> {
    // v20's selectable player datablocks (`bri_motor::player_types`), which
    // every archetype table starts with.
    const V20_PLAYERS: [&str; 7] = [
        "playerstandardarmor",
        "playernojet",
        "playerfueljet",
        "playerjumpjet",
        "playerleapjet",
        "playerquakearmor",
        "horsearmor",
    ];
    let mut fields = BTreeMap::new();
    let mut at = name.to_ascii_lowercase();
    let mut depth = 0;
    let base = loop {
        let Some(o) = cx.owned.get(&at) else {
            let base = archetype_id(cx, &at);
            ensure!(
                !base.starts_with("v20.") || V20_PLAYERS.contains(&at.as_str()),
                "inherits from {at}, which is not one of v20's player types"
            );
            break base;
        };
        depth += 1;
        ensure!(depth <= 16, "{name}'s datablock parents loop");
        for (k, v) in &o.d.fields {
            fields
                .entry(k.to_ascii_lowercase())
                .or_insert_with(|| v.clone());
        }
        match &o.d.parent {
            Some(parent) => at = parent.to_ascii_lowercase(),
            None => break format!("v20.player.{}", V20_PLAYERS[0]),
        }
    };
    let converted = player_types::convert(&fields, Some(base));
    let parsed: bri_package_runtime::content::ArchetypeDef =
        serde_json::from_value(converted.archetype.clone()).context("archetype")?;
    parsed.validate()?;
    Ok((converted.archetype, converted.left_out))
}

/// The archetype a `PlayerData` named `name` is: this Add-On's own or that
/// of an Add-On it depends on, or else v20's (`v20.player.<datablock>`).
fn archetype_id(cx: &Ctx, name: &str) -> String {
    let key = name.to_ascii_lowercase();
    if cx.is_owned(&key) {
        return content_id(&cx.ns, "archetype", name);
    }
    match cx
        .reference
        .datablocks
        .get(&key)
        .filter(|o| o.addon != "base")
        .and_then(|o| namespace_for(&o.addon).ok())
    {
        Some(ns) => content_id(&ns, "archetype", name),
        None => format!("v20.player.{key}"),
    }
}

/// Whether `script` downloads `file` from a website at run time: it names
/// the file and fetches something over HTTP (`connectToUrl`, an
/// `HTTPObject` or `TCPObject` get), as Slayer's holiday greeting fetches
/// its music into `config/client/temp`.
fn downloads(script: &str, file: &str) -> bool {
    let script = script.to_ascii_lowercase();
    let file = file.to_ascii_lowercase();
    !file.is_empty()
        && script.contains(&format!("\"{file}\""))
        && ["connecttourl(", "httpobject", "tcpobject"]
            .iter()
            .any(|call| script.contains(call))
}

fn sounds_and_rest(cx: &mut Ctx) {
    let pending: Vec<Pending> = cx
        .report
        .datablocks
        .iter()
        .filter(|e| e.status == "unsupported" && e.recognised_as == "unknown")
        .filter_map(|e| {
            cx.owned.get(&e.name.to_ascii_lowercase()).map(|o| {
                (
                    o.d.name.clone(),
                    o.d.class.to_ascii_lowercase(),
                    o.fields.clone(),
                    o.d.fields.clone(),
                    o.path.clone(),
                    o.d.line,
                )
            })
        })
        .collect();
    // How often each name is written in the Add-On's scripts: a datablock
    // written only where it is declared is one nothing uses.
    let script_text: String = cx
        .src
        .files
        .values()
        .filter(|f| kind_of(&f.path) == "script")
        .map(|f| tscript::without_comments(&String::from_utf8_lossy(&f.bytes)).to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("\n");
    let mentions = |name: &str| {
        regex::Regex::new(&format!(
            r"\b{}\b",
            regex::escape(&name.to_ascii_lowercase())
        ))
        .map_or(usize::MAX, |re| re.find_iter(&script_text).count())
    };
    // An emitter written into an array slot past the engine's (an
    // `ExplosionData`'s 4 `emitter`s, a `DebrisData`'s 2 `emitters`) was
    // refused when the datablock loaded, so that mention draws nothing.
    let mut past_slots: BTreeMap<String, usize> = BTreeMap::new();
    for o in cx.owned.values() {
        let (field, slots) = match o.d.class.to_ascii_lowercase().as_str() {
            "explosiondata" => ("emitter[", 4),
            "debrisdata" => ("emitters[", 2),
            _ => continue,
        };
        for (key, value) in &o.fields {
            if key
                .strip_prefix(field)
                .and_then(|r| r.strip_suffix(']'))
                .and_then(|i| i.trim().parse::<usize>().ok())
                .is_some_and(|i| i >= slots)
            {
                *past_slots
                    .entry(literal(value).trim().to_ascii_lowercase())
                    .or_default() += 1;
            }
        }
    }
    let past = |name: &str| {
        past_slots
            .get(&name.to_ascii_lowercase())
            .copied()
            .unwrap_or(0)
    };
    let used = |name: &str| mentions(name).saturating_sub(past(name)) > 1;
    // The particles each emitter nothing uses names: those mentions draw
    // nothing either.
    let mut idle_mentions = past_slots.clone();
    for (name, class, fields, ..) in &pending {
        if class == "particleemitterdata" && !used(name) && !fields.contains_key("uiname") {
            for particle in fields
                .get("particles")
                .map(|p| literal(p).to_ascii_lowercase())
                .unwrap_or_default()
                .split_whitespace()
            {
                *idle_mentions.entry(particle.to_owned()).or_default() += 1;
            }
        }
    }
    let used = |name: &str| {
        let idle = if mentions(name) == usize::MAX {
            0
        } else {
            idle_mentions
                .get(&name.to_ascii_lowercase())
                .copied()
                .unwrap_or(0)
        };
        mentions(name).saturating_sub(idle) > 1
    };
    for (name, class, fields, own, path, line) in pending {
        let at = Location::new(&path, line);
        match class.as_str() {
            "audioprofile" => {
                let file = fields
                    .get("filename")
                    .map(|f| file_named(cx.src, &cx.outputs, &path, f))
                    .unwrap_or_default();
                if let Some(rel) = cx.outputs.get(&file.to_ascii_lowercase()).cloned() {
                    let id = cx.id("sound", &name, &name, &format!("assets/{rel}"));
                    cx.mark(&name, "sound", "converted_with_gaps", vec![id], Some("the audio system reads one fixed pack (role audio); this sound is packaged but nothing plays it by id yet".into()));
                } else if let Some(sound) = cx.reference.base_sound(&file) {
                    // Tier 1's `Block_MoveBrick_Sound` of the base click.
                    let note = format!("plays the base game's {sound}, the same file");
                    cx.mark(&name, "sound", "consumed", vec![], Some(note));
                } else if let Some(base) = fields
                    .get("filename")
                    .and_then(|f| f.rsplit_once('"').and_then(|(head, _)| head.rsplit_once('"')))
                    .map(|(_, tail)| tail.rsplit('/').next().unwrap_or(tail).to_ascii_lowercase())
                    .filter(|b| !b.is_empty())
                    .filter(|_| {
                        fields.get("filename").is_some_and(|f| f.trim_start().starts_with('%'))
                    })
                    .filter(|b| {
                        !cx.src
                            .files
                            .values()
                            .any(|f| f.path.to_ascii_lowercase().ends_with(&format!("/{b}")))
                    })
                {
                    // A path built at load (`%path @ "x.wav"`) whose file the
                    // Add-On has nowhere: v20 found nothing to play either.
                    cx.mark(
                        &name,
                        "sound",
                        "consumed",
                        vec![],
                        Some(format!(
                            "it names {base} by a path built at load, and the Add-On has no file of that name, so v20 played nothing"
                        )),
                    );
                } else if cx
                    .script_text(&path)
                    .is_some_and(|text| downloads(&text, &file))
                {
                    // Fetched from a website when the Add-On runs: not a
                    // gap in the port, and the game downloads nothing.
                    cx.mark(
                        &name,
                        "sound",
                        "external",
                        vec![],
                        Some(format!(
                            "needs a resource downloaded from an external site, not in the copy ({file})"
                        )),
                    );
                } else {
                    cx.mark(
                        &name,
                        "sound",
                        "recognised_only",
                        vec![],
                        Some(format!("sound file {file} is not in this Add-On")),
                    );
                }
            }
            "audiodescription" => {
                // Its volume, looping and 3D flag are read into each sound
                // that names it (`weapon_fx::sounds`).
                let users: Vec<String> = cx
                    .report
                    .datablocks
                    .iter()
                    .filter(|e| e.class.eq_ignore_ascii_case("AudioProfile"))
                    .filter(|e| {
                        cx.owned
                            .get(&e.name.to_ascii_lowercase())
                            .and_then(|o| o.fields.get("description"))
                            .is_some_and(|d| literal(d).trim().eq_ignore_ascii_case(&name))
                    })
                    .flat_map(|e| e.ids.clone())
                    .collect();
                let note = if users.is_empty() {
                    "no sound of this Add-On names it, so it changed nothing".to_owned()
                } else {
                    "its volume, looping and 3D flag are read into the sounds that name it".to_owned()
                };
                cx.mark(&name, "sound_description", "consumed", vec![], Some(note));
                if let Some(e) = cx.entry(&name) {
                    e.notes.extend(users.into_iter().map(|id| format!("used by {id}")));
                }
            }
            "playerdata" if !fields.contains_key("isholebot") => {
                match player_archetype(cx, &name) {
                    Ok((def, gaps)) => {
                        let file = format!("assets/archetypes/{}.json", name.to_ascii_lowercase());
                        match serde_json::to_vec_pretty(&def) {
                            Ok(bytes) if cx.write(&file, &bytes).is_ok() => {
                                let id = cx.id("archetype", &name, &name, &file);
                                if gaps.is_empty() {
                                    cx.mark(&name, "player_type", "converted", vec![id], None);
                                } else {
                                    cx.mark(
                                        &name,
                                        "player_type",
                                        "converted_with_gaps",
                                        vec![id],
                                        Some(format!("fields without a native equivalent: {}", gaps.join(", "))),
                                    );
                                }
                            }
                            _ => cx.unsupported(
                                format!("player type {name}"),
                                Some(at),
                                "its archetype could not be written".into(),
                            ),
                        }
                    }
                    Err(e) => {
                        cx.mark(&name, "player_type", "recognised_only", vec![], Some(format!("{e:#}")));
                        cx.unsupported(format!("player type {name}"), Some(at), format!("{e:#}"));
                    }
                }
            }
            "playerdata" => {
                // Bot_Hole's settings are the `h`-prefixed fields this datablock declares.
                let ai: Vec<_> = own
                    .keys()
                    .filter(|k| k.starts_with('h') && k.len() > 2)
                    .cloned()
                    .collect();
                cx.mark(
                    &name,
                    "bot",
                    "recognised_only",
                    vec![],
                    Some("no native schema for Add-On bots; bots are Rust brains that join as players".into()),
                );
                cx.unsupported(
                    format!("bot {name}"),
                    Some(at),
                    format!("Bot_Hole AI settings ({}) configure a script framework this import does not have", ai.join(", ")),
                );
            }
            // Images' casings and explosions' debris throw it
            // (`bri_weapons::debris`), drawn with its model.
            "debrisdata" if used(&name) => cx.mark(
                &name,
                "debris",
                "converted",
                vec![],
                Some("thrown by the images and explosions that name it".into()),
            ),
            "debrisdata" => cx.mark(
                &name,
                "debris",
                "consumed",
                vec![],
                Some("nothing throws it, so v20 never did".into()),
            ),
            // A vehicle trail converted the ones it uses.
            "particledata" | "particleemitterdata"
                if cx.entry(&name).is_some_and(|e| e.status == "converted") => {}
            "particledata" | "particleemitterdata" | "particleemitternodedata"
                if !used(&name) && !fields.contains_key("uiname") =>
            {
                cx.mark(
                    &name,
                    if class == "particledata" {
                        "particle"
                    } else {
                        "emitter"
                    },
                    "consumed",
                    vec![],
                    Some("nothing uses it (or only an emitter nothing uses, or an array slot past the engine's) and it has no uiName, so v20 never drew it".into()),
                )
            }
            "particledata" | "particleemitterdata" | "particleemitternodedata" => cx.mark(
                &name,
                if class == "particledata" {
                    "particle"
                } else {
                    "emitter"
                },
                "recognised_only",
                vec![],
                Some("the effects importers are fixed vanilla pipelines".into()),
            ),
            "wheeledvehicletire" | "wheeledvehiclespring" => cx.mark(
                &name,
                "vehicle_part",
                "recognised_only",
                vec![],
                Some("not used by an imported vehicle".into()),
            ),
            "fxdtsbrickdata" | "itemdata" | "shapebaseimagedata" | "projectiledata"
            | "explosiondata" => {}
            _ => {
                if let Some(e) = cx.entry(&name) {
                    e.notes.push(format!("no native concept for {class}"));
                }
            }
        }
    }
}

fn behaviours(cx: &mut Ctx, scripts: &[Script]) {
    let datablocks: BTreeMap<String, (String, BTreeMap<String, String>)> = cx
        .owned
        .iter()
        .map(|(k, o)| (k.clone(), (o.d.class.clone(), o.fields.clone())))
        .collect();
    let own_functions: BTreeSet<String> = scripts
        .iter()
        .flat_map(|s| {
            s.functions
                .iter()
                .map(|f| f.qualified().to_ascii_lowercase())
        })
        .collect();
    for s in scripts {
        for f in &s.functions {
            let bcx = behaviour::Context {
                namespace: &cx.ns,
                file: &s.path,
                datablocks: &datablocks,
                own_functions: &own_functions,
                reference_functions: &cx.reference.functions,
            };
            if let Some(b) = behaviour::analyse(f, &bcx) {
                if let Some(ns) = &f.namespace
                    && cx.is_owned(ns)
                    && let Some(e) = cx.entry(ns)
                {
                    e.notes.push(format!(
                        "script function {} needs native behaviour",
                        f.qualified()
                    ));
                    if e.status == "converted" {
                        e.status = "converted_with_gaps".into();
                    }
                }
                cx.report.needs_behaviour.push(b);
            }
        }
    }
    // A state script the Add-On does not define runs the engine's own, as
    // v20's stock `WeaponImage` functions did (`onFire` fires the image's
    // projectile).
    let native = bri_weapons::runtime::WeaponsWorld::NATIVE_STATE_SCRIPTS;
    for e in &mut cx.report.datablocks {
        let image = e.name.to_ascii_lowercase();
        settle_notes(e, |note| {
            let (_, script) = state_script(note)?;
            let script = script.to_ascii_lowercase();
            (native.contains(&script.as_str())
                && !own_functions.contains(&format!("{image}::{script}")))
            .then(|| note.replacen(" calls script ", " runs the engine's own ", 1))
        });
    }
}

/// `state Fire calls script onFire` → (`Fire`, `onFire`).
fn state_script(note: &str) -> Option<(&str, &str)> {
    note.strip_prefix("state ")?.split_once(" calls script ")
}

/// Rewrites each gap note `settle` resolves into what resolves it, and
/// marks the datablock converted once no gap note is left.
/// The [`ports::Port::handles`] key naming an unsupported finding: a
/// top-level call (`call:`), a file (`file:`), an object made at load
/// (`new:`) or one changed at load (`set:`), lower-case.
fn handles_key(what: &str) -> Option<String> {
    let key = if let Some(call) = what.strip_prefix("top-level call ") {
        format!("call:{call}")
    } else if let Some(file) = what.strip_prefix("file ") {
        format!("file:{file}")
    } else if let Some(class) = what.strip_prefix("new ").and_then(|w| w.strip_suffix(" at load")) {
        format!("new:{class}")
    } else {
        format!("set:{}", what.split_once(" = ")?.0.trim())
    };
    Some(key.to_ascii_lowercase())
}

fn settle_notes(e: &mut report::DatablockEntry, settle: impl Fn(&str) -> Option<String>) {
    for note in &mut e.notes {
        if let Some(settled) = settle(note) {
            *note = settled;
        }
    }
    let mut seen = BTreeSet::new();
    e.notes.retain(|n| seen.insert(n.clone()));
    let gap = |n: &String| n.ends_with(" needs native behaviour") || state_script(n).is_some();
    if e.status == "converted_with_gaps"
        && e.notes.iter().all(|n| {
            !gap(n) && (n.contains(" ported by ") || n.contains(" runs the engine's own "))
        })
    {
        e.status = "converted".into();
    }
}

/// The id a datablock this Add-On takes from another has where that one's
/// package declares it: the base game's (`v20.projectile.gunprojectile`)
/// for a vanilla Add-On or the core scripts, else the namespace importing
/// that Add-On makes (`weapon_package_tier1:projectile/...`).
fn dependency_id(cx: &Ctx, kind: &str, name: &str) -> String {
    let addon = cx
        .reference
        .datablocks
        .get(&name.to_ascii_lowercase())
        .map(|o| o.addon.as_str())
        .filter(|a| *a != "base" && reference::base_package(a).is_none());
    match addon.and_then(|a| namespace_for(a).ok()) {
        Some(ns) => content_id(&ns, kind, name),
        None => bri_weapons::native_id(kind, name),
    }
}

/// The package an Add-On this one requires by name
/// (`ForceRequiredAddOn`) becomes: the base game's for a vanilla one, else
/// the package importing it makes (its namespace), which the player
/// imports from their own copy too (Tier 2 needs Tier 1).
fn dependency_package(addon: &str) -> Option<String> {
    reference::base_package(addon)
        .map(str::to_owned)
        .or_else(|| namespace_for(addon).ok())
}

/// A file named by a path its script built at load (`filename = %path @
/// "x.wav"` after `%path = "./sounds/";`): the first value the script gives
/// `%path` under which this Add-On has the file. A branch for another
/// Add-On's folder (`if(isFile("Add-Ons/Other/..."))`) finds nothing here,
/// as `isFile` did in v20 without that Add-On, so the Add-On's own folder is
/// the one used.
/// The file a datablock's file field names, as its script `script` loads
/// it: a path built at load ([`load_path`]) or a literal one.
pub(crate) fn file_named(
    src: &source::Source,
    outputs: &BTreeMap<String, String>,
    script: &str,
    field: &str,
) -> String {
    load_path(src, outputs, script, field)
        .unwrap_or_else(|| source::resolve(script, literal(field)))
}

fn load_path(
    src: &source::Source,
    outputs: &BTreeMap<String, String>,
    script: &str,
    field: &str,
) -> Option<String> {
    static BUILT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"^\s*%(\w+)\s*@\s*"([^"]*)"\s*$"#).expect("pattern")
    });
    let c = BUILT.captures(field)?;
    let text = src
        .files
        .values()
        .find(|f| f.path.eq_ignore_ascii_case(script))
        .map(|f| tscript::without_comments(&String::from_utf8_lossy(&f.bytes)))?;
    let assigned =
        regex::RegexBuilder::new(&format!(r#"%{}\s*=\s*"([^"]*)"\s*;"#, regex::escape(&c[1])))
            .case_insensitive(true)
            .build()
            .ok()?;
    let has = |file: &str| {
        let lower = file.to_ascii_lowercase();
        outputs.contains_key(&lower)
            || src
                .files
                .values()
                .any(|f| f.path.eq_ignore_ascii_case(file))
    };
    assigned
        .captures_iter(&text)
        .map(|a| source::resolve(script, &format!("{}{}", &a[1], &c[2])))
        .find(|file| has(file))
}

/// Whether the call to `callee` on 1-based `line` of `text` sits in a
/// block or statement whose `if` checks for a file of `addon`
/// (`if(isFile("Add-Ons/<addon>/server.cs"))`), so it runs only where that
/// Add-On is present.
fn required_if_present(text: &str, line: usize, callee: &str, addon: &str) -> bool {
    let text = tscript::without_comments(text);
    let start: usize = text
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::len)
        .sum();
    // Up to the call itself, so a check earlier on its line counts.
    let at = text[start..]
        .lines()
        .next()
        .and_then(|l| l.to_ascii_lowercase().find(&callee.to_ascii_lowercase()))
        .map_or(start, |i| start + i);
    let before = &text.as_bytes()[..at];
    let wanted = format!("isfile(\"add-ons/{}/", addon.to_ascii_lowercase());
    let guards = |header: &[u8]| {
        let header: String = String::from_utf8_lossy(header)
            .to_ascii_lowercase()
            .split_whitespace()
            .collect();
        header.starts_with("if(") && header.contains(&wanted)
    };
    let statement = |end: usize| {
        before[..end]
            .iter()
            .rposition(|b| matches!(b, b';' | b'{' | b'}'))
            .map_or(0, |p| p + 1)
    };
    // `if(isFile(...)) ForceRequiredAddOn(...);` with no block.
    if guards(&before[statement(before.len())..]) {
        return true;
    }
    let mut depth = 0usize;
    for (i, b) in before.iter().enumerate().rev() {
        match b {
            b'}' => depth += 1,
            b'{' if depth > 0 => depth -= 1,
            b'{' => {
                // The header of a block the call is in.
                if guards(&before[statement(i)..i]) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn dependencies(cx: &mut Ctx, scripts: &[Script]) {
    let mut deps: BTreeMap<String, Dependency> = BTreeMap::new();
    for s in scripts {
        for c in &s.calls {
            let how = c.callee.to_ascii_lowercase();
            if how != "forcerequiredaddon" && how != "loadrequiredaddon" {
                continue;
            }
            let Some(a) = c.args.first() else { continue };
            let addon = literal(a).to_owned();
            if cx.src.get(&s.path).is_some_and(|f| {
                required_if_present(
                    &String::from_utf8_lossy(&f.bytes),
                    c.line,
                    &c.callee,
                    &addon,
                )
            }) {
                // `if(isFile("Add-Ons/Sound_Blockland/server.cs"))
                // ForceRequiredAddOn("Sound_Blockland");`: required only
                // where the player has it. `isFile` reads as absent, so the
                // Add-On takes its own branch and does not need it.
                deps.entry(addon.to_ascii_lowercase())
                    .or_insert(Dependency {
                        addon: addon.clone(),
                        how: c.callee.clone(),
                        source: Some(Location::new(&s.path, c.line)),
                        status: "if_present".into(),
                        package: None,
                        uses: vec![],
                    });
                continue;
            }
            let found = cx
                .reference
                .addons
                .get(&addon.to_ascii_lowercase())
                .cloned();
            // An Add-On v20 shipped (`Weapon_Gun`) is the game's own: its
            // content is a base package, there with or without a v20
            // folder to read.
            let base = reference::base_package(&addon);
            deps.entry(addon.to_ascii_lowercase())
                .or_insert(Dependency {
                    addon: found.clone().unwrap_or(addon.clone()),
                    how: c.callee.clone(),
                    source: Some(Location::new(&s.path, c.line)),
                    status: if found.is_some() {
                        "reference"
                    } else if base.is_some() {
                        "base"
                    } else {
                        "missing"
                    }
                    .into(),
                    package: found
                        .as_deref()
                        .and_then(dependency_package)
                        .or(base.map(str::to_owned)),
                    uses: vec![],
                });
        }
    }
    for (addon, uses) in std::mem::take(&mut cx.uses) {
        if addon.is_empty() {
            continue;
        }
        let d = deps
            .entry(addon.to_ascii_lowercase())
            .or_insert(Dependency {
                addon: addon.clone(),
                how: "reference".into(),
                source: None,
                status: "reference".into(),
                package: reference::base_package(&addon).map(str::to_owned),
                uses: vec![],
            });
        d.uses.extend(uses);
    }
    let own = cx.src.name.to_ascii_lowercase();
    // A function the scripts call that neither they, the engine nor the
    // reference install define may be the missing Add-On's (Bot_Zombie
    // calls Bot_Hole's AI framework), so such an Add-On is not unused.
    let calls_unknown = cx
        .report
        .needs_behaviour
        .iter()
        .any(|b| !b.unknown_calls.is_empty() || b.hook.kind == "framework_callback");
    for (key, d) in &mut deps {
        if *key == own {
            // Requiring itself does nothing: it is already loading.
            d.status = "self".into();
        } else if matches!(d.status.as_str(), "missing" | "base")
            && d.uses.is_empty()
            && cx.reference.root.is_some()
            && !(d.status == "missing" && calls_unknown)
        {
            // `forceRequiredAddOn` of a missing Add-On only printed an
            // error; with none of its content named (every name the
            // Add-On uses resolved against the install, or is reported as
            // declared nowhere), v20 ran the same. A base Add-On none of
            // whose content is named needs no base package either.
            d.status = "unused".into();
            d.package = None;
        }
    }
    cx.report.dependencies = deps.into_values().collect();
}

/// The package runtime's content kinds this package provides, one entry per
/// merged pack file (`crates/package-runtime/src/content.rs` `Kind`).
fn runtime_provides(out: &Path, namespace: &str) -> Vec<serde_json::Value> {
    [
        ("weapons", "assets/weapons.json"),
        ("vehicles", "assets/vehicles.json"),
        ("bricks", "assets/brick-catalog/stock-catalog.json"),
    ]
    .into_iter()
    .filter(|(_, file)| out.join(file).is_file())
    .map(|(kind, file)| json!({ "kind": kind, "id": format!("{namespace}:{kind}/main"), "file": file }))
    .chain(archetype_provides(out, namespace))
    .collect()
}

/// Each converted player type (`assets/archetypes/<name>.json`), declared so
/// the package runtime adds it to the host's archetype table.
fn archetype_provides(out: &Path, namespace: &str) -> Vec<serde_json::Value> {
    let Ok(dir) = std::fs::read_dir(out.join("assets/archetypes")) else {
        return Vec::new();
    };
    let mut names: Vec<String> = dir
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter_map(|n| n.strip_suffix(".json").map(str::to_owned))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|n| {
            json!({
                "kind": "archetype",
                "id": format!("{namespace}:archetype/{n}"),
                "file": format!("assets/archetypes/{n}.json"),
            })
        })
        .collect()
}

fn finish(mut cx: Ctx, opts: &Options, ports: &ports::Ports, code: &ports::Code) -> Result<Report> {
    for a in cx
        .report
        .assets
        .iter_mut()
        .filter(|a| a.status == "pending")
    {
        a.status = "unsupported".into();
    }
    let dir = opts
        .out
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dependencies: BTreeMap<String, String> = cx
        .report
        .dependencies
        .iter()
        .filter_map(|d| d.package.clone())
        .map(|p| (p, "*".to_string()))
        .collect();
    let src = &cx.report.source;
    // The per-package manifest the package runtime reads
    // (`bri_package_runtime::manifest`): the weapons, vehicles and bricks
    // packs and each converted player type. Every converted asset is also
    // listed in `assets/content.json`.
    let manifest = json!({
        "schema_version": 1,
        "id": cx.ns,
        "version": opts.version,
        "api": 1,
        "name": if src.title.is_empty() { src.name.clone() } else { src.title.clone() },
        "description": src.description,
        "authors": src.authors,
        "license": src.licence_spdx.clone().unwrap_or_else(|| "proprietary".into()),
        "provenance": {
            "source": format!("Blockland Add-On {} ({}), sha256 {}", src.name, src.format, src.sha256),
            "notes": if src.licence_status == "known" {
                format!("licence files: {}", src.licence_files.join(", "))
            } else {
                "No licence file in the Add-On; redistribution rights are unknown. Imported by bri-import-addon.".into()
            },
        },
        "dependencies": dependencies,
        "capabilities": [],
        "provides": runtime_provides(&cx.out, &cx.ns),
    });
    cx.write(
        "assets/content.json",
        &serde_json::to_vec_pretty(&json!({ "schema_version": 1, "content": cx.provides }))?,
    )?;
    cx.write("package.json", &serde_json::to_vec_pretty(&manifest)?)?;
    cx.report.package = PackageInfo {
        id: cx.ns.clone(),
        namespace: cx.ns.clone(),
        version: opts.version.clone(),
        dir: dir.clone(),
        packages_json_entry: json!({ "id": cx.ns, "version": opts.version, "side": "shared", "dir": dir }),
        files: vec![],
    };
    let pictures: BTreeMap<String, Vec<u8>> = cx
        .src
        .files
        .values()
        .filter(|f| {
            let lower = f.path.to_ascii_lowercase();
            lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")
        })
        .map(|f| (cx.src.member(f).to_ascii_lowercase(), f.bytes.clone()))
        .collect();
    let import = ports::Import {
        pictures: &pictures,
        addon: &cx.src.name,
        sha256: &cx.src.sha256,
        namespace: &cx.ns,
        version: &opts.version,
        name: manifest["name"].as_str().unwrap_or(&cx.ns),
        dependencies: &cx.dependency_projectiles,
    };
    if let Some(mut port) = ports::apply(ports, &import, code, &cx.out) {
        // A port reads what the Add-Ons it requires declare; say which
        // were not beside it, as that is the usual cause.
        let missing: Vec<&str> = cx
            .report
            .dependencies
            .iter()
            .filter(|d| d.status == "missing" && d.how.to_ascii_lowercase().contains("required"))
            .map(|d| d.addon.as_str())
            .collect();
        if let Some(reason) = &mut port.reason
            && !missing.is_empty()
        {
            reason.push_str(&format!(
                " ({} it requires was not found: put it in the Add-Ons folder beside it, or give --reference a folder whose Add-Ons/ holds it)",
                missing.join(", ")
            ));
        }
        for b in &mut cx.report.needs_behaviour {
            let how: Vec<String> = port
                .handled
                .get(&b.function.to_ascii_lowercase())
                .map(|h| h.iter().cloned().collect())
                .unwrap_or_default();
            if !how.is_empty()
                || port
                    .covers
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(&b.function))
            {
                b.port = Some(report::PortRef {
                    port: port.port.clone(),
                    status: port.status.clone(),
                    applied: port.applied,
                    how,
                });
            }
        }
        // What the port carries out is not unsupported: top-level calls,
        // files and objects made or changed at load, by their `handles` key,
        // and the RTB preferences its rules read or the game carries out.
        let how_of = |key: &str| {
            port.handled.get(key).map(|h| {
                format!(
                    "port {}: {}",
                    port.port,
                    h.iter().cloned().collect::<Vec<_>>().join("; ")
                )
            })
        };
        let pref = |what: &str| {
            what.strip_prefix("RTB_registerPref ")
                .and_then(|g| port.prefs.get(&g.to_ascii_lowercase()))
                .map(|how| format!("port {}: {how}", port.port))
        };
        let (ported, unsupported): (Vec<_>, Vec<_>) = std::mem::take(&mut cx.report.unsupported)
            .into_iter()
            .map(|mut f| {
                f.resolution =
                    pref(&f.what).or_else(|| handles_key(&f.what).and_then(|k| how_of(&k)));
                f
            })
            .partition(|f| f.resolution.is_some());
        cx.report.unsupported = unsupported;
        cx.report.ported = ported;
        for e in &mut cx.report.datablocks {
            if e.status == "unsupported"
                && let Some(how) = how_of(&format!("datablock:{}", e.name.to_ascii_lowercase()))
            {
                e.status = "ported".into();
                e.notes.push(how);
            }
        }
        // A datablock that only drove script callbacks (a trigger's
        // `onTickTrigger`) is done by the port's rules that rewrote them.
        if port.applied {
            for d in cx
                .report
                .datablocks
                .iter_mut()
                .filter(|d| d.status == "unsupported")
            {
                let prefix = format!("{}::", d.name.to_ascii_lowercase());
                if port
                    .covers
                    .iter()
                    .any(|c| c.to_ascii_lowercase().starts_with(&prefix))
                {
                    d.status = "consumed".into();
                    d.notes
                        .push(format!("port {}: its callbacks are host rules now", port.port));
                }
            }
            // One the port carries out as an engine feature
            // (`datablock:<name>` in its handles): a raycasting gun's line
            // shape, drawn as the engine's tracer.
            for d in cx
                .report
                .datablocks
                .iter_mut()
                .filter(|d| matches!(d.status.as_str(), "recognised_only" | "unsupported"))
            {
                if let Some(how) = port
                    .handled
                    .get(&format!("datablock:{}", d.name.to_ascii_lowercase()))
                {
                    d.status = "consumed".into();
                    d.notes.push(format!(
                        "port {}: {}",
                        port.port,
                        how.iter().cloned().collect::<Vec<_>>().join("; ")
                    ));
                }
            }
            // One the Add-On makes at run time, which the port declares.
            for d in cx.report.datablocks.iter_mut().filter(|d| {
                port.replaces.iter().any(|r| r.eq_ignore_ascii_case(&d.name))
                    && matches!(d.status.as_str(), "recognised_only" | "unsupported")
            }) {
                d.status = "consumed".into();
                d.notes.push(format!(
                    "port {}: its {} declares what this makes at run time",
                    port.port,
                    ports::DATABLOCKS
                ));
            }
        }
        // A global the copy sets at load and a ported function reads: the
        // port was written against this exact copy, value included.
        if port.applied && port.copy == "listed" {
            for a in &mut cx.report.ambiguous {
                let Some(global) = a
                    .what
                    .strip_prefix("global ")
                    .and_then(|g| g.split_once(" = "))
                    .map(|(name, _)| name.to_ascii_lowercase())
                else {
                    continue;
                };
                if a.resolution.is_none()
                    && let Some(f) = port.covers.iter().find(|f| {
                        code.bodies
                            .get(&f.to_ascii_lowercase())
                            .is_some_and(|b| b.to_ascii_lowercase().contains(&global))
                    })
                {
                    a.resolution = Some(format!(
                        "read by {f}, which {} ports for this copy with the value set here",
                        port.port
                    ));
                }
            }
        }
        if port.applied {
            let covered = |f: &str| {
                port.covers.iter().any(|c| c.eq_ignore_ascii_case(f))
                    || port.handled.contains_key(&f.to_ascii_lowercase())
            };
            for e in &mut cx.report.datablocks {
                let name = e.name.clone();
                settle_notes(e, |note| {
                    let function = match state_script(note) {
                        Some((_, script)) => format!("{name}::{script}"),
                        None => note
                            .strip_prefix("script function ")?
                            .strip_suffix(" needs native behaviour")?
                            .to_owned(),
                    };
                    covered(&function).then(|| format!("{function} ported by {}", port.port))
                });
            }
        }
        cx.report.ports.push(port);
    }
    cx.report.summarise();
    let mut files = vec![];
    let mut stack = vec![cx.out.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)?.flatten() {
            if e.path().is_dir() {
                stack.push(e.path());
            } else if let Ok(rel) = e.path().strip_prefix(&cx.out) {
                files.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    files.push("import-report.json".into());
    files.push("IMPORT-REPORT.md".into());
    files.sort();
    cx.report.package.files = files;
    cx.write(
        "import-report.json",
        &serde_json::to_vec_pretty(&cx.report)?,
    )?;
    cx.write("IMPORT-REPORT.md", cx.report.markdown().as_bytes())?;
    Ok(cx.report)
}

#[cfg(test)]
mod tests {
    #[test]
    fn material_textures_are_looked_for_up_the_folders() {
        assert_eq!(
            super::texture_folders("add-ons/weapon_x/shapes/items").collect::<Vec<_>>(),
            [
                "add-ons/weapon_x/shapes/items",
                "add-ons/weapon_x/shapes",
                "add-ons/weapon_x",
                "add-ons"
            ]
        );
    }

    #[test]
    fn a_require_inside_a_check_for_that_add_on_runs_only_where_it_is() {
        let text = "if(isFile(\"Add-Ons/Sound_X/server.cs\"))\n{\n   // the pack\n   ForceRequiredAddOn(\"Sound_X\");\n}\nelse\n{\n   ForceRequiredAddOn(\"Sound_X\");\n}\nif (isFile(\"add-ons/sound_x/a.wav\"))\n   forceRequiredAddOn(\"Sound_X\");\nForceRequiredAddOn(\"Sound_X\");\nif(isFile(\"Add-Ons/Other/server.cs\")) { ForceRequiredAddOn(\"Sound_X\"); }\nif(isFile(\"Add-Ons/Sound_X/b.cs\")) ForceRequiredAddOn(\"Sound_X\");\n";
        let guarded =
            |line| super::required_if_present(text, line, "ForceRequiredAddOn", "Sound_X");
        assert!(guarded(4), "in the check's block");
        assert!(!guarded(8), "in its else");
        assert!(guarded(11), "the check's one statement, with no block");
        assert!(!guarded(12), "after it");
        assert!(!guarded(13), "a check for another Add-On");
        assert!(guarded(14), "a check on the call's own line");
    }
}
