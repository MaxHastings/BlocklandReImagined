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
pub mod reference;
pub mod report;
pub mod source;

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
    pub version: String,
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
}

impl Ctx<'_> {
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
    check_output(opts)?;
    let src = source::read(&opts.input)?;
    let reference = match &opts.reference {
        Some(root) => Reference::load(root, &opts.core)?,
        None => Reference::default(),
    };
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
    };
    metadata(&mut cx);
    let scripts = read_scripts(&mut cx);
    inventory(&mut cx, &scripts);
    top_level(&mut cx, &scripts);
    datablocks(&mut cx, &scripts);
    references(&mut cx);
    convert_files(&mut cx)?;
    weapons(&mut cx, &scripts)?;
    vehicles(&mut cx)?;
    bricks(&mut cx, &scripts)?;
    sounds_and_rest(&mut cx);
    behaviours(&mut cx, &scripts);
    dependencies(&mut cx, &scripts);
    finish(cx, opts)
}

fn metadata(cx: &mut Ctx) {
    let src = cx.src;
    let text = |name: &str| {
        src.get(&format!("{}/{name}", src.dir()))
            .map(|f| String::from_utf8_lossy(&f.bytes).replace('\r', ""))
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

fn read_scripts(cx: &mut Ctx) -> Vec<Script> {
    let mut scripts = vec![];
    for f in cx.src.files.values() {
        let lower = f.path.to_ascii_lowercase();
        if lower.ends_with(".cs") {
            match tscript::read(&String::from_utf8_lossy(&f.bytes), &f.path) {
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

fn inventory(cx: &mut Ctx, scripts: &[Script]) {
    // Scripts reachable from server.cs / client.cs through literal exec calls.
    let mut reachable = BTreeSet::new();
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
                }
            }
        }
    }
    let metadata = ["description.txt", "rtbinfo.txt", "namecheck.txt"];
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
                } else {
                    vec!["not reached by a literal exec from server.cs; read anyway".into()]
                },
            ),
            "compiled_script" => (
                "unsupported",
                vec!["compiled DSO bytecode; only source .cs is read".into()],
            ),
            "text" if metadata.contains(&member.as_str()) => ("consumed", vec!["metadata".into()]),
            "shape" | "texture" | "sound" | "brick_geometry" => ("pending", vec![]),
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
}

const KNOWN_TOP_LEVEL: &[&str] = &[
    "exec",
    "forcerequiredaddon",
    "loadrequiredaddon",
    "adddamagetype",
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
        let text = cx
            .src
            .get(&s.path)
            .map(|f| String::from_utf8_lossy(&f.bytes).into_owned())
            .unwrap_or_default();
        for (i, line) in text.lines().enumerate() {
            if let Some(c) = write.captures(line) {
                let object = c[1].to_owned();
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
            bri_weapons_import::damage_types(&String::from_utf8_lossy(&f.bytes)).unwrap_or_default()
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
                        if cx.reference.root.is_some() {
                            " or the reference install"
                        } else {
                            "; no reference install given"
                        }
                    ),
                    case_only.then(|| "matches a member by case only".into()),
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
                        match (cx.reference.root.is_some(), cx.reference.has_core) {
                            (false, _) => "; no reference install given",
                            (true, false) =>
                                " or the reference install (base datablocks need --core)",
                            (true, true) => ", the reference install or the core scripts",
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

fn weapons(cx: &mut Ctx, scripts: &[Script]) -> Result<()> {
    let is_weapon = |c: &str| WEAPON_CLASSES.iter().any(|w| w.eq_ignore_ascii_case(c));
    let mut defs: Vec<bri_weapons::Definition> = cx
        .owned
        .values()
        .filter(|o| is_weapon(&o.d.class))
        .map(|o| weapon_definition(o, &o.d.fields))
        .collect();
    if defs.is_empty() {
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
            .map(|o| bri_weapons::Definition {
                name: o.datablock.name.clone(),
                class: o.datablock.class.clone(),
                parent: o.datablock.parent.clone(),
                source: bri_weapons::Evidence {
                    path: o.path.clone(),
                    sha256: o.sha256.clone(),
                    line: o.datablock.line,
                },
                fields: o.datablock.fields.clone(),
            })
            .collect();
        if add.is_empty() {
            break;
        }
        for d in add {
            names.insert(d.name.to_ascii_lowercase());
            defs.push(d);
        }
    }
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
            content_id("v20", kind, &n)
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
    for (old, mut p) in std::mem::take(&mut pack.projectiles) {
        let own = owned(&p.name);
        if !own && !needed_projectiles.contains(&old) {
            continue;
        }
        p.sport_image = p.sport_image.map(|s| remap(cx, "image", &s));
        if own {
            p.id = cx.id("projectile", &p.name, &p.name, file);
            cx.mark(
                &p.name.clone(),
                "projectile",
                "converted",
                vec![p.id.clone()],
                None,
            );
        } else {
            p.id = content_id("v20", "projectile", &p.name);
            cx.ambiguous(
                format!("projectile {}", p.name),
                None,
                "a dependency's projectile is copied into this pack: the weapons pack format cannot reference another package's projectile".into(),
                Some(p.id.clone()),
            );
        }
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
    for o in cx
        .owned
        .values()
        .filter(|o| o.d.class.eq_ignore_ascii_case("ItemData"))
    {
        if !items
            .values()
            .any(|i| i.name.eq_ignore_ascii_case(&o.d.name))
        {
            cx.report.unsupported.push(Finding {
                what: format!("item {}", o.d.name),
                source: Some(Location::new(&o.path, o.d.line)),
                detail:
                    "an ItemData without a weapon image; non-weapon items have no native schema"
                        .into(),
                resolution: None,
            });
        }
    }
    pack.items = items;
    pack.images = images;
    pack.projectiles = projectiles;
    pack.explosions = explosions;
    pack.id = cx.ns.clone();
    pack.definitions.retain(|d| owned(&d.name));
    // Damage types this Add-On declares.
    let texts: Vec<_> = scripts
        .iter()
        .filter_map(|s| cx.src.get(&s.path))
        .map(|f| String::from_utf8_lossy(&f.bytes).into_owned())
        .collect();
    for t in texts
        .iter()
        .flat_map(|t| bri_weapons_import::damage_types(t).unwrap_or_default())
    {
        let missing: Vec<_> = t
            .icons()
            .filter(|i| {
                cx.src.get(&format!("{i}.png")).is_none() && cx.reference.has_file(i).is_none()
            })
            .collect();
        if !missing.is_empty() {
            cx.report.diagnostics.push(format!(
                "damage type {} icon missing: {}",
                t.name,
                missing.join(", ")
            ));
            continue;
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
        .collect();
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
            match texture(cx, &mut textures, &format!("{folder}/{}", m.name)) {
                Some(t) => bindings.push(t),
                None => {
                    cx.report.diagnostics.push(format!(
                        "presentation: {key} material {} has no texture",
                        m.name
                    ));
                }
            }
        }
        if bindings.len() != shape.materials.len() {
            continue;
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
        let (shape, white) = placeholder();
        let shape_bytes = serde_json::to_vec(&shape)?;
        let shape_rel = format!("models/{}.shape.json", &hash(&shape_bytes)[..24]);
        cx.write(&format!("assets/{shape_rel}"), &shape_bytes)?;
        let white_rel = format!("textures/{}.png", &hash(&white)[..24]);
        cx.write(&format!("assets/{white_rel}"), &white)?;
        textures.insert(
            "placeholder:white".into(),
            json!({ "file": white_rel, "sha256": hash(&white), "width": 1, "height": 1, "source": "placeholder" }),
        );
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
        let eye = cx
            .owned
            .get(&im.name.to_ascii_lowercase())
            .and_then(|o| o.fields.get("eyerotation"))
            .map(|v| {
                literal(v)
                    .split_whitespace()
                    .filter_map(|n| n.parse::<f32>().ok())
                    .collect::<Vec<_>>()
            })
            .filter(|v| v.len() == 3)
            .unwrap_or(vec![0.0; 3]);
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

/// A 0.2 unit cube with one white material, and its 1x1 white PNG.
fn placeholder() -> (bri_content::shape::Shape, Vec<u8>) {
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
    let shape = Shape {
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
        }],
        animations: vec![],
    };
    let mut png = vec![];
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("in-memory PNG");
    (shape, png)
}

fn vehicles(cx: &mut Ctx) -> Result<()> {
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
                d.adaptations.push(
                    "Wheel steering and power follow the vanilla Jeep convention (front two steer, the rest drive); Torque sets both from script (setWheelSteering/setWheelPowered), which this Add-On does not call".into(),
                );
                let vid = cx.id("vehicle", name, name, "assets/vehicles.json");
                d.id = vid.clone();
                cx.ambiguous(
                    format!("vehicle {name} wheel steering and power"),
                    Some(at),
                    "no script sets them; the vanilla front-steer rear-drive convention was applied".into(),
                    Some("front two wheels steer, the rest are powered".into()),
                );
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

fn bricks(cx: &mut Ctx, scripts: &[Script]) -> Result<()> {
    let mut entries = vec![];
    for s in scripts {
        if !s
            .datablocks
            .iter()
            .any(|d| d.class.eq_ignore_ascii_case("fxDTSBrickData"))
        {
            continue;
        }
        let dir = s.path.rsplit_once('/').map_or(s.path.as_str(), |(d, _)| d);
        let text =
            String::from_utf8_lossy(&cx.src.get(&s.path).expect("script").bytes).into_owned();
        // Parents declared elsewhere (base bricks), as declarations the reader can inherit from.
        let mut parents = String::new();
        for o in cx.reference.datablocks.values() {
            let d = &o.datablock;
            if d.class.eq_ignore_ascii_case("fxDTSBrickData") {
                parents.push_str(&format!(
                    "datablock fxDTSBrickData({}{}) {{ {} }};
",
                    d.name,
                    d.parent
                        .as_ref()
                        .map_or(String::new(), |p| format!(" : {p}")),
                    d.fields
                        .iter()
                        .map(|(k, v)| format!("{k} = {v};"))
                        .collect::<String>()
                ));
            }
        }
        match bri_convert::catalog::read_with_parents(&text, dir, &parents) {
            Ok(catalog) => {
                for mut b in catalog.bricks {
                    let key = b.id.rsplit('/').next().unwrap_or("").to_owned();
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
                        content_id("v20", "brick_geometry", &found)
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
    for (name, class, fields, own, path, line) in pending {
        let at = Location::new(&path, line);
        match class.as_str() {
            "audioprofile" => {
                let file = fields
                    .get("filename")
                    .map(|f| source::resolve(&path, literal(f)))
                    .unwrap_or_default();
                if let Some(rel) = cx.outputs.get(&file.to_ascii_lowercase()).cloned() {
                    let id = cx.id("sound", &name, &name, &format!("assets/{rel}"));
                    cx.mark(&name, "sound", "converted_with_gaps", vec![id], Some("the audio system reads one fixed pack (role audio); this sound is packaged but nothing plays it by id yet".into()));
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
            "playerdata" => {
                let bot = fields.contains_key("isholebot");
                // Bot_Hole's settings are the `h`-prefixed fields this datablock declares.
                let ai: Vec<_> = own
                    .keys()
                    .filter(|k| k.starts_with('h') && k.len() > 2)
                    .cloned()
                    .collect();
                cx.mark(
                    &name,
                    if bot { "bot" } else { "player_type" },
                    "recognised_only",
                    vec![],
                    Some("no native schema for Add-On player types; bots are Rust brains that join as players".into()),
                );
                cx.unsupported(
                    format!("{} {name}", if bot { "bot" } else { "player type" }),
                    Some(at),
                    if bot {
                        format!("Bot_Hole AI settings ({}) configure a script framework this import does not have", ai.join(", "))
                    } else {
                        "PlayerData movement and armour are not importable from Add-Ons".into()
                    },
                );
            }
            "debrisdata" => cx.mark(
                &name,
                "debris",
                "recognised_only",
                vec![],
                Some("the weapon debris importer is a fixed vanilla pipeline".into()),
            ),
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
            let found = cx
                .reference
                .addons
                .get(&addon.to_ascii_lowercase())
                .cloned();
            deps.entry(addon.to_ascii_lowercase())
                .or_insert(Dependency {
                    addon: found.clone().unwrap_or(addon.clone()),
                    how: c.callee.clone(),
                    source: Some(Location::new(&s.path, c.line)),
                    status: if found.is_some() {
                        "reference"
                    } else {
                        "missing"
                    }
                    .into(),
                    package: found
                        .as_deref()
                        .and_then(reference::base_package)
                        .map(str::to_owned),
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
    .collect()
}

fn finish(mut cx: Ctx, opts: &Options) -> Result<Report> {
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
    // (`bri_package_runtime::manifest`). Its `provides` kinds are the ones the
    // runtime consumes (behaviour, script, world, entity, model, hud); none of
    // an Add-On's weapons, vehicles or bricks is one of them yet, so the
    // imported content is declared in `assets/content.json` instead.
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
