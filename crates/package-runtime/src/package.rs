//! Loading the mod packages listed in `packages.json`: their manifests and
//! typed content. Identity, hashing and the join comparison are
//! `bri-package`'s; this adds what the packages provide.
use crate::content::{self, Kind};
use crate::manifest::{MANIFEST_FILE, Manifest, Rejected, location};
use bri_package::diag::Diagnostic;
use bri_package::id::{Requirement, Version};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

/// One provided file with its bytes.
#[derive(Debug, Clone)]
pub struct Asset {
    pub id: String,
    pub kind: Kind,
    pub file: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub side: Side,
    pub manifest: Manifest,
    pub version: Version,
    pub assets: Vec<Asset>,
    pub behaviour: Option<content::Behaviour>,
    pub worlds: BTreeMap<String, content::ChunkWorld>,
    pub entities: BTreeMap<String, content::EntityKind>,
    pub models: BTreeMap<String, content::BoxModel>,
    pub huds: BTreeMap<String, content::HudPanel>,
    pub modes: BTreeMap<String, content::GameMode>,
}

const MAX_PACKAGE_BYTES: usize = 32 * 1024 * 1024;

fn safe_relative(file: &str) -> bool {
    !file.is_empty()
        && file.len() <= 256
        && !file.contains('\\')
        && Path::new(file)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

impl Package {
    /// Read a mod package directory. Every problem is returned, not just the
    /// first.
    pub fn load(dir: &Path, entry: &PackageEntry) -> Result<Self, Vec<Diagnostic>> {
        match Self::inspect(dir, entry) {
            (Some(package), problems) if problems.is_empty() => Ok(package),
            (_, problems) => Err(problems),
        }
    }
    /// Read as much of a package as its manifest allows, with every problem.
    /// A package with problems is only for reporting (`Catalog::inspect`).
    pub fn inspect(dir: &Path, entry: &PackageEntry) -> (Option<Self>, Vec<Diagnostic>) {
        match Self::read(dir, entry) {
            Ok(package) => (Some(package), Vec::new()),
            Err(rejected) => *rejected,
        }
    }
    fn read(dir: &Path, entry: &PackageEntry) -> Result<Self, Rejected<Self>> {
        let manifest_bytes = std::fs::read(dir.join(MANIFEST_FILE)).map_err(|e| {
            (
                None,
                vec![
                    Diagnostic::error(
                        "package.manifest_missing",
                        format!("cannot read {MANIFEST_FILE}: {e}"),
                    )
                    .at(location(&entry.id, MANIFEST_FILE))
                    .hint("a mod package is a folder containing package.json"),
                ],
            )
        })?;
        let (manifest, mut out) = Manifest::inspect(&manifest_bytes, &entry.id);
        let Some(manifest) = manifest else {
            return Err(Box::new((None, out)));
        };
        let id = entry.id.clone();
        if manifest.version != entry.version {
            out.push(
                Diagnostic::error(
                    "package.version.mismatch",
                    format!(
                        "package.json is version {} but packages.json lists {}",
                        manifest.version, entry.version
                    ),
                )
                .at(location(&id, MANIFEST_FILE)),
            );
        }
        let mut assets = Vec::new();
        let mut total = manifest_bytes.len();
        for provide in &manifest.provides {
            // Unknown kinds were reported with the manifest.
            let Some(kind) = Kind::parse(&provide.kind) else {
                continue;
            };
            let at = location(&id, &provide.file);
            if !safe_relative(&provide.file) {
                out.push(
                    Diagnostic::error(
                        "package.file.path",
                        format!(
                            "`{}` must be a relative path inside the package",
                            provide.file
                        ),
                    )
                    .at(at),
                );
                continue;
            }
            match (kind.side(), entry.side) {
                (Side::Server, Side::Server) | (Side::Client, Side::Client | Side::Shared) => {}
                (Side::Server, _) => {
                    out.push(
                        Diagnostic::error(
                            "package.side.server_content",
                            format!("{:?} content is server behaviour, but this package is `{:?}` and would be loaded by clients", kind, entry.side),
                        )
                        .at(at.clone())
                        .hint("move server behaviour into a package listed with \"side\": \"server\"; clients never receive package code"),
                    );
                }
                _ => {
                    out.push(
                        Diagnostic::error(
                            "package.side.client_content",
                            format!("{:?} content is drawn by clients, but clients never load `server` packages", kind),
                        )
                        .at(at.clone())
                        .hint("put models and HUD panels in a `client` package"),
                    );
                }
            }
            let bytes = match std::fs::read(dir.join(&provide.file)) {
                Ok(bytes) => bytes,
                Err(e) => {
                    out.push(
                        Diagnostic::error(
                            "package.file.missing",
                            format!("cannot read `{}`: {e}", provide.file),
                        )
                        .at(at),
                    );
                    continue;
                }
            };
            if bytes.len() > kind.max_bytes() {
                out.push(
                    Diagnostic::error(
                        "package.file.too_large",
                        format!("{} bytes; the limit is {}", bytes.len(), kind.max_bytes()),
                    )
                    .at(at),
                );
                continue;
            }
            total += bytes.len();
            assets.push(Asset {
                id: provide.id.clone(),
                kind,
                file: provide.file.clone(),
                bytes,
            });
        }
        if total > MAX_PACKAGE_BYTES {
            out.push(
                Diagnostic::error("package.too_large", format!("{total} bytes is over 32 MiB"))
                    .at(location(&id, "")),
            );
        }
        let mut package = Self {
            side: entry.side,
            version: Version::parse(&manifest.version).unwrap_or(Version {
                major: 0,
                minor: 0,
                patch: 0,
            }),
            behaviour: None,
            worlds: BTreeMap::new(),
            entities: BTreeMap::new(),
            models: BTreeMap::new(),
            huds: BTreeMap::new(),
            modes: BTreeMap::new(),
            manifest,
            assets,
        };
        package.parse_content(&mut out);
        if out.is_empty() {
            Ok(package)
        } else {
            Err(Box::new((Some(package), out)))
        }
    }

    fn parse_content(&mut self, out: &mut Vec<Diagnostic>) {
        fn parse<T: serde::de::DeserializeOwned>(
            asset: &Asset,
            package: &str,
            validate: impl Fn(&T) -> anyhow::Result<()>,
            out: &mut Vec<Diagnostic>,
        ) -> Option<T> {
            let value: T = match serde_json::from_slice(&asset.bytes) {
                Ok(v) => v,
                Err(e) => {
                    out.push(Diagnostic::error("content.json", e.to_string()).at(format!(
                        "{}:{}:{}",
                        location(package, &asset.file),
                        e.line(),
                        e.column()
                    )));
                    return None;
                }
            };
            if let Err(e) = validate(&value) {
                out.push(
                    Diagnostic::error("content.invalid", format!("{e:#}"))
                        .at(location(package, &asset.file)),
                );
                return None;
            }
            Some(value)
        }
        let id = self.manifest.id.clone();
        for asset in &self.assets {
            let at = location(&id, &asset.file);
            match asset.kind {
                Kind::Behaviour => {
                    if self.behaviour.is_some() {
                        out.push(
                            Diagnostic::error(
                                "content.behaviour.duplicate",
                                "a package has at most one behaviour",
                            )
                            .at(at),
                        );
                        continue;
                    }
                    if let Some(b) = parse::<content::Behaviour>(asset, &id, |b| b.validate(), out)
                    {
                        if !self
                            .assets
                            .iter()
                            .any(|a| a.kind == Kind::Script && a.file == b.script)
                        {
                            out.push(
                                Diagnostic::error(
                                    "content.behaviour.script",
                                    format!(
                                        "script `{}` is not provided as kind `script`",
                                        b.script
                                    ),
                                )
                                .at(at),
                            );
                        }
                        self.behaviour = Some(b);
                    }
                }
                Kind::Script => {
                    if std::str::from_utf8(&asset.bytes).is_err() {
                        out.push(
                            Diagnostic::error("content.script.utf8", "script is not UTF-8").at(at),
                        );
                    }
                }
                Kind::World => {
                    if let Some(w) = parse::<content::ChunkWorld>(asset, &id, |w| w.validate(), out)
                    {
                        self.worlds.insert(asset.id.clone(), w);
                    }
                }
                Kind::Entity => {
                    if let Some(e) = parse::<content::EntityKind>(asset, &id, |e| e.validate(), out)
                    {
                        self.entities.insert(asset.id.clone(), e);
                    }
                }
                Kind::Model => {
                    if let Some(m) = parse::<content::BoxModel>(asset, &id, |m| m.validate(), out) {
                        self.models.insert(asset.id.clone(), m);
                    }
                }
                Kind::Hud => {
                    if let Some(h) = parse::<content::HudPanel>(asset, &id, |h| h.validate(), out) {
                        self.huds.insert(asset.id.clone(), h);
                    }
                }
                Kind::Mode => {
                    if let Some(m) = parse::<content::GameMode>(asset, &id, |m| m.validate(), out) {
                        self.modes.insert(asset.id.clone(), m);
                    }
                }
            }
        }
        if (!self.worlds.is_empty() || !self.entities.is_empty()) && self.behaviour.is_none() {
            out.push(
                Diagnostic::error(
                    "content.behaviour.missing",
                    "worlds and entities call script functions, so the package needs a behaviour",
                )
                .at(location(&id, MANIFEST_FILE)),
            );
        }
    }

    pub fn id(&self) -> &str {
        &self.manifest.id
    }
    pub fn asset(&self, file: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.file == file)
    }
    pub fn script_source(&self) -> Option<&str> {
        let b = self.behaviour.as_ref()?;
        std::str::from_utf8(&self.asset(&b.script)?.bytes).ok()
    }
}

/// The mod packages a peer loaded, by id. Base game packages (listed with a
/// `role`) are read by their engine systems, not here.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub packages: BTreeMap<String, Package>,
}
impl Catalog {
    /// Load every package in `set` that carries its own `package.json`.
    /// `server` packages are skipped when `server` is false (a client never
    /// loads them).
    pub fn load(root: &Path, set: &PackageSet, server: bool) -> Result<Self, Vec<Diagnostic>> {
        let (catalog, problems) = Self::inspect(root, set, server);
        if problems.is_empty() {
            Ok(catalog)
        } else {
            Err(problems)
        }
    }
    /// Load for reporting: every package that could be read, even with
    /// problems, and every problem once. Never run a catalog with problems.
    pub fn inspect(root: &Path, set: &PackageSet, server: bool) -> (Self, Vec<Diagnostic>) {
        let mut catalog = Self::default();
        let mut out = Vec::new();
        let mut failed = BTreeSet::new();
        let listed: BTreeMap<&str, &PackageEntry> =
            set.packages.iter().map(|p| (p.id.as_str(), p)).collect();
        for entry in &set.packages {
            if entry.role.is_some() || (!server && entry.side == Side::Server) {
                continue;
            }
            let dir = match bri_package::packages::package_dir(root, entry) {
                Ok(dir) => dir,
                Err(e) => {
                    out.push(
                        Diagnostic::error("package.dir", format!("{e:#}"))
                            .at(location(&entry.id, "")),
                    );
                    continue;
                }
            };
            if !dir.join(MANIFEST_FILE).is_file() {
                continue;
            }
            let (package, mut problems) = Package::inspect(&dir, entry);
            if !problems.is_empty() {
                failed.insert(entry.id.clone());
                out.append(&mut problems);
            }
            if let Some(package) = package {
                catalog.packages.insert(entry.id.clone(), package);
            }
        }
        out.extend(catalog.check(&listed, &failed, server));
        (catalog, out)
    }
    /// Cross-package checks: dependencies, model and state references.
    fn check(
        &self,
        listed: &BTreeMap<&str, &PackageEntry>,
        failed: &std::collections::BTreeSet<String>,
        server: bool,
    ) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        // References into a package that failed to load are reported once,
        // there, rather than again for every reference.
        let broken = |package: &str| failed.contains(package);
        for (id, p) in &self.packages {
            let at = location(id, MANIFEST_FILE);
            for (dependency, requirement) in &p.manifest.dependencies {
                let Ok(requirement) = Requirement::parse(requirement) else {
                    continue;
                };
                if broken(dependency) {
                    out.push(
                        Diagnostic::error(
                            "set.dependency.broken",
                            format!(
                                "`{id}` needs `{dependency}`, which has errors of its own (above)"
                            ),
                        )
                        .at(at.clone()),
                    );
                    continue;
                }
                let Some(entry) = listed.get(dependency.as_str()) else {
                    out.push(
                        Diagnostic::error(
                            "set.dependency.missing",
                            format!("`{id}` needs `{dependency}`, which is not enabled"),
                        )
                        .at(at.clone())
                        .hint(format!("add `{dependency}` to packages.json")),
                    );
                    continue;
                };
                // A client does not load server packages, so it cannot check them.
                if !server && entry.side == Side::Server {
                    continue;
                }
                if !Version::parse(&entry.version).is_ok_and(|v| requirement.matches(v)) {
                    out.push(
                        Diagnostic::error(
                            "set.dependency.version",
                            format!(
                                "`{id}` needs `{dependency}` {requirement:?} but {} is enabled",
                                entry.version
                            ),
                        )
                        .at(at.clone()),
                    );
                }
            }
            if server {
                for kind in p.entities.values() {
                    let model = &kind.model;
                    let owner = model.split(':').next().unwrap_or_default();
                    if broken(owner) {
                        continue;
                    }
                    if !listed.get(owner).is_some_and(|e| e.side.on_client()) {
                        out.push(
                            Diagnostic::error(
                                "set.model.missing",
                                format!(
                                    "entity model `{model}` has no enabled client package `{owner}`"
                                ),
                            )
                            .at(at.clone()),
                        );
                    } else if let Some(o) = self.packages.get(owner)
                        && !o.models.contains_key(model)
                    {
                        let known: Vec<&String> = o.models.keys().collect();
                        out.push(
                            Diagnostic::error(
                                "set.model.unknown",
                                format!("`{owner}` provides no model `{model}`"),
                            )
                            .at(at.clone())
                            .hint(format!("it provides {known:?}")),
                        );
                    }
                }
            }
            for hud in p.huds.values() {
                for key in &hud.keys {
                    if broken(&key.package) {
                        continue;
                    }
                    if let Some(owner) = self.packages.get(&key.package) {
                        let declared = owner.behaviour.as_ref().is_some_and(|b| {
                            b.commands
                                .iter()
                                .any(|c| c.name == key.command && c.args.is_empty())
                        });
                        if !declared {
                            out.push(
                                Diagnostic::error("set.hud.key", format!("key {} sends `{}` which `{}` does not declare without arguments", key.key, key.command, key.package))
                                    .at(at.clone()),
                            );
                        }
                    } else if server || !listed.contains_key(key.package.as_str()) {
                        out.push(
                            Diagnostic::error(
                                "set.hud.key",
                                format!(
                                    "key {} sends to `{}`, which is not enabled",
                                    key.key, key.package
                                ),
                            )
                            .at(at.clone()),
                        );
                    }
                }
                for row in &hud.rows {
                    let binding = content::Binding::parse(&row.bind).expect("checked by the HUD");
                    if broken(&binding.package) {
                        continue;
                    }
                    // Clients do not load the server package that owns the state.
                    let Some(owner) = self.packages.get(&binding.package) else {
                        if server || !listed.contains_key(binding.package.as_str()) {
                            out.push(
                                Diagnostic::error(
                                    "set.hud.binding",
                                    format!("`{}` binds a package that is not enabled", row.bind),
                                )
                                .at(at.clone()),
                            );
                        }
                        continue;
                    };
                    let public = owner.behaviour.as_ref().is_some_and(|b| {
                        let keys = if binding.player {
                            &b.state.player
                        } else {
                            &b.state.global
                        };
                        keys.get(&binding.key).is_some_and(|k| k.public)
                    });
                    if !public {
                        out.push(
                            Diagnostic::error("set.hud.binding", format!("`{}` is not a public state key", row.bind))
                                .at(at.clone())
                                .hint("declare it under the owner's behaviour state with \"public\": true"),
                        );
                    }
                }
            }
        }
        // Several world providers may be enabled; a hosted game runs one
        // (`for_world`, `for_mode`). Each mode names only what it can run.
        for (id, p) in &self.packages {
            for (mode_id, mode) in &p.modes {
                let at = location(id, mode_id);
                for add_on in &mode.add_ons {
                    if add_on != id && !p.manifest.dependencies.contains_key(add_on) {
                        out.push(
                            Diagnostic::error(
                                "set.mode.add_on",
                                format!("mode runs `{add_on}`, which `{id}` does not depend on"),
                            )
                            .at(at.clone())
                            .hint(format!(
                                "add `{add_on}` to dependencies, so turning the mode on turns it on"
                            )),
                        );
                    }
                }
                if let Some(map) = mode.map.as_ref().filter(|m| m.contains(':')) {
                    let roots = mode.add_ons.iter().map(String::as_str).chain([id.as_str()]);
                    if !self
                        .closure(roots)
                        .iter()
                        .any(|p| self.packages[*p].worlds.contains_key(map))
                    {
                        out.push(
                            Diagnostic::error(
                                "set.mode.map",
                                format!(
                                    "mode plays on `{map}`, which none of its Add-Ons provides"
                                ),
                            )
                            .at(at.clone()),
                        );
                    }
                }
            }
        }
        out
    }
    /// Every game mode the enabled packages declare, by content id.
    pub fn modes(&self) -> impl Iterator<Item = (&String, &content::GameMode)> {
        self.packages.values().flat_map(|p| p.modes.iter())
    }
    /// `roots` and every enabled package they depend on, transitively.
    fn closure<'a>(&'a self, roots: impl IntoIterator<Item = &'a str>) -> BTreeSet<&'a str> {
        let mut out = BTreeSet::new();
        let mut queue: Vec<&str> = roots.into_iter().collect();
        while let Some(id) = queue.pop() {
            let Some((id, p)) = self.packages.get_key_value(id) else {
                continue;
            };
            if out.insert(id.as_str()) {
                queue.extend(p.manifest.dependencies.keys().map(String::as_str));
            }
        }
        out
    }
    fn only(&self, keep: &BTreeSet<&str>) -> Result<Catalog, Vec<Diagnostic>> {
        let catalog = Catalog {
            packages: self
                .packages
                .iter()
                .filter(|(id, _)| keep.contains(id.as_str()))
                .map(|(id, p)| (id.clone(), p.clone()))
                .collect(),
        };
        let worlds: Vec<&String> = catalog
            .packages
            .values()
            .flat_map(|p| p.worlds.keys())
            .collect();
        if worlds.len() > 1 {
            return Err(vec![
                Diagnostic::error(
                    "set.world.conflict",
                    format!("more than one world provider would run: {worlds:?}"),
                )
                .hint("a game runs one world provider; pick a game mode that names one"),
            ]);
        }
        Ok(catalog)
    }
    /// What a host runs for game mode `mode`: its package, the Add-Ons it
    /// names and their dependencies.
    pub fn for_mode(&self, mode: &str) -> Result<Catalog, Vec<Diagnostic>> {
        let owner = mode.split(':').next().unwrap_or_default();
        let Some(m) = self.packages.get(owner).and_then(|p| p.modes.get(mode)) else {
            return Err(vec![Diagnostic::error(
                "set.mode.unknown",
                format!("no enabled Add-On provides game mode `{mode}`"),
            )]);
        };
        let roots = m.add_ons.iter().map(String::as_str).chain([owner]);
        self.only(&self.closure(roots))
    }
    /// What a host runs on package world `world` without a game mode: every
    /// enabled package except those that need a different world.
    pub fn for_world(&self, world: &str) -> Result<Catalog, Vec<Diagnostic>> {
        let catalog = self.only(&self.needing_only(world))?;
        if catalog.world().is_none_or(|(_, id, _)| id != world) {
            return Err(vec![Diagnostic::error(
                "set.world.unknown",
                format!("no enabled Add-On provides world `{world}`"),
            )]);
        }
        Ok(catalog)
    }
    /// Packages whose dependencies provide no world other than `world`.
    fn needing_only(&self, world: &str) -> BTreeSet<&str> {
        self.packages
            .keys()
            .map(String::as_str)
            .filter(|id| {
                self.closure([*id])
                    .iter()
                    .all(|dep| self.packages[*dep].worlds.keys().all(|w| w == world))
            })
            .collect()
    }
    pub fn world(&self) -> Option<(&Package, &String, &content::ChunkWorld)> {
        self.packages
            .values()
            .find_map(|p| p.worlds.iter().next().map(|(id, w)| (p, id, w)))
    }
    pub fn entity(&self, id: &str) -> Option<(&Package, &content::EntityKind)> {
        let p = self.packages.get(id.split(':').next()?)?;
        Some((p, p.entities.get(id)?))
    }
    pub fn model(&self, id: &str) -> Option<&content::BoxModel> {
        self.packages.get(id.split(':').next()?)?.models.get(id)
    }
    pub fn huds(&self) -> impl Iterator<Item = (&String, &content::HudPanel)> {
        self.packages.values().flat_map(|p| p.huds.iter())
    }
    pub fn behaviours(&self) -> impl Iterator<Item = (&String, &content::Behaviour)> {
        self.packages
            .iter()
            .filter_map(|(id, p)| p.behaviour.as_ref().map(|b| (id, b)))
    }
}
