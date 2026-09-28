//! Loading the mod packages listed in `packages.json`: their manifests and
//! typed content. Identity, hashing and the join comparison are
//! `bri-package`'s; this adds what the packages provide.
use crate::content::{self, Kind};
use crate::manifest::{MANIFEST_FILE, Manifest, location};
use bri_package::diag::Diagnostic;
use bri_package::id::{Requirement, Version};
use bri_package::packages::{PackageEntry, PackageSet, Side};
use std::collections::BTreeMap;
use std::path::Path;

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
}

const MAX_PACKAGE_BYTES: usize = 32 * 1024 * 1024;

impl Package {
    /// Read a mod package directory. Every problem is returned, not just the
    /// first.
    pub fn load(dir: &Path, entry: &PackageEntry) -> Result<Self, Vec<Diagnostic>> {
        let manifest_bytes = bri_package::path::inside(dir, MANIFEST_FILE)
            .map_err(std::io::Error::other)
            .and_then(std::fs::read)
            .map_err(|e| {
                vec![
                    Diagnostic::error(
                        "package.manifest_missing",
                        format!("cannot read {MANIFEST_FILE}: {e}"),
                    )
                    .at(location(&entry.id, MANIFEST_FILE))
                    .hint("a mod package is a folder containing package.json"),
                ]
            })?;
        let manifest = Manifest::parse(&manifest_bytes, &entry.id)?;
        let id = entry.id.clone();
        let mut out = Vec::new();
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
            let kind = Kind::parse(&provide.kind).expect("checked by the manifest");
            let at = location(&id, &provide.file);
            let path = match bri_package::path::inside(dir, &provide.file) {
                Ok(path) => path,
                Err(problem) => {
                    out.push(
                        Diagnostic::error("package.file.path", problem)
                            .at(at)
                            .hint("provide files by plain relative paths inside the package"),
                    );
                    continue;
                }
            };
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
            let bytes = match std::fs::read(&path) {
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
            version: Version::parse(&manifest.version).expect("checked by the manifest"),
            behaviour: None,
            worlds: BTreeMap::new(),
            entities: BTreeMap::new(),
            models: BTreeMap::new(),
            huds: BTreeMap::new(),
            manifest,
            assets,
        };
        package.parse_content(&mut out);
        if out.is_empty() {
            Ok(package)
        } else {
            Err(out)
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
        let mut catalog = Self::default();
        let mut out = Vec::new();
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
            match Package::load(&dir, entry) {
                Ok(p) => {
                    catalog.packages.insert(entry.id.clone(), p);
                }
                Err(mut e) => out.append(&mut e),
            }
        }
        out.extend(catalog.check(&listed, server));
        if out.is_empty() {
            Ok(catalog)
        } else {
            Err(out)
        }
    }
    /// Cross-package checks: dependencies, model and state references.
    fn check(&self, listed: &BTreeMap<&str, &PackageEntry>, server: bool) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        // One key press sends one command: two panels may share a key only
        // if they send the same thing.
        let mut keys: BTreeMap<String, (&str, &str, &str)> = BTreeMap::new();
        for (id, p) in &self.packages {
            for (panel, hud) in &p.huds {
                for key in &hud.keys {
                    let target = (key.package.as_str(), key.command.as_str(), panel.as_str());
                    match keys.entry(key.key.to_ascii_uppercase()) {
                        std::collections::btree_map::Entry::Vacant(v) => {
                            v.insert(target);
                        }
                        std::collections::btree_map::Entry::Occupied(o) => {
                            let (package, command, first) = *o.get();
                            if (package, command) != (target.0, target.1) {
                                out.push(
                                    Diagnostic::error(
                                        "set.hud.key.conflict",
                                        format!(
                                            "key {} sends `{}:{}` in `{panel}` but `{package}:{command}` in `{first}`",
                                            key.key, key.package, key.command
                                        ),
                                    )
                                    .at(location(id, MANIFEST_FILE))
                                    .hint("give one of the panels a different key"),
                                );
                            }
                        }
                    }
                }
            }
        }
        for (id, p) in &self.packages {
            let at = location(id, MANIFEST_FILE);
            for (dependency, requirement) in &p.manifest.dependencies {
                let requirement = Requirement::parse(requirement).expect("checked by the manifest");
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
                    }
                }
            }
            for hud in p.huds.values() {
                for key in &hud.keys {
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
                    let visible = owner.behaviour.as_ref().is_some_and(|b| {
                        let (keys, needed): (_, &[content::Visible]) = match binding.scope {
                            content::Scope::Global => {
                                (&b.state.global, &[content::Visible::Everyone])
                            }
                            content::Scope::Player => (
                                &b.state.player,
                                &[content::Visible::Owner, content::Visible::Everyone],
                            ),
                            content::Scope::Players => {
                                (&b.state.player, &[content::Visible::Everyone])
                            }
                        };
                        keys.get(&binding.key)
                            .is_some_and(|k| needed.contains(&k.visible))
                    });
                    if !visible {
                        out.push(
                            Diagnostic::error("set.hud.binding", format!("`{}` is not a state key clients receive", row.bind))
                                .at(at.clone())
                                .hint("declare it under the owner's behaviour state with \"visible\": \"owner\" (a player's own key) or \"everyone\" (global keys and scoreboards)"),
                        );
                    }
                }
            }
        }
        let worlds: Vec<&String> = self
            .packages
            .values()
            .flat_map(|p| p.worlds.keys())
            .collect();
        if worlds.len() > 1 {
            out.push(
                Diagnostic::error(
                    "set.world.conflict",
                    format!("more than one world provider is enabled: {worlds:?}"),
                )
                .hint("a server runs one world provider"),
            );
        }
        out
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
