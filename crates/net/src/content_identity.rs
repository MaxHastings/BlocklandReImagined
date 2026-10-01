//! Validated native weapon and item-physics content, and the startup
//! snapshots that keep it from changing under a running host. Which content a
//! peer loaded is identified by its package hashes (`bri_package`).
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

const WEAPON_INDEX_LIMIT: u64 = 32 * 1024 * 1024;
const WEAPON_RESOURCE_LIMIT: u64 = 64 * 1024 * 1024;
const WEAPON_TOTAL_LIMIT: u64 = 512 * 1024 * 1024;

/// Validated startup snapshot shared by host/join/dedicated catalog setup.
/// Source resource hashes describe originals, not converted native bytes.
#[derive(Clone)]
pub struct WeaponContent {
    /// The pack played: as authored, or with the server settings its
    /// bindings read applied ([`Self::apply_settings`]).
    pub pack: bri_weapons::Pack,
    /// The pack as authored, when bindings make the played one depend on
    /// server settings, and the values it was last played with.
    authored: Option<(std::sync::Arc<bri_weapons::Pack>, BTreeMap<String, String>)>,
    /// Each item's load rank, which orders items sharing a name.
    load_order: BTreeMap<String, usize>,
    /// (id, display name) by name; items sharing a name in load order.
    pub item_choices: Vec<(String, String)>,
    /// The emitters and lights Add-Ons give a name (`uiName`), as
    /// (id, name): a brick's wrench offers them beside the base game's.
    pub emitter_choices: Vec<(String, String)>,
    pub light_choices: Vec<(String, String)>,
    aliases: BTreeMap<String, String>,
    fingerprint: String,
    manifest_sha256: String,
    /// Items of the base weapons package; others come from merged packages.
    base_items: std::collections::BTreeSet<String>,
    /// What merging the Add-Ons' weapons left out or overrode.
    pub problems: Vec<bri_package::health::Problem>,
}
/// Packages beside a kind's base package that provide it too: every listed
/// package without a role whose directory holds `assets/<file>`, in
/// `packages.json` order, as (content-root-relative dir, absolute dir).
pub fn kind_providers(
    content_root: &Path,
    packages: &bri_package::packages::PackageSet,
    file: &str,
) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    for entry in packages.packages.iter().filter(|p| p.role.is_none()) {
        let dir = bri_package::packages::package_dir(content_root, entry)?.join("assets");
        if dir.join(file).is_file() {
            out.push((format!("{}/assets", entry.dir), dir));
        }
    }
    Ok(out)
}
/// [`kind_providers`] of brick catalogs, as (package dir, catalog dir).
pub fn brick_catalog_providers(
    content_root: &Path,
    packages: &bri_package::packages::PackageSet,
) -> Result<Vec<(String, PathBuf)>> {
    Ok(
        kind_providers(content_root, packages, "brick-catalog/stock-catalog.json")?
            .into_iter()
            .map(|(dir, abs)| (dir, abs.join("brick-catalog")))
            .collect(),
    )
}
/// Bot kinds from every package providing `assets/bots.json`, in
/// `packages.json` order (a later id replaces an earlier one). The base
/// game provides none.
pub fn bot_kinds(
    content_root: &Path,
    packages: &bri_package::packages::PackageSet,
) -> Result<Vec<bri_sim::bot_kind::BotKind>> {
    bot_kinds_from(&kind_providers(content_root, packages, "bots.json")?)
}
/// [`bot_kinds`] from already listed providers.
pub fn bot_kinds_from(providers: &[(String, PathBuf)]) -> Result<Vec<bri_sim::bot_kind::BotKind>> {
    let mut packs = Vec::new();
    for (dir, abs) in providers {
        let path = contained(abs, "bots.json")?;
        let bytes = bounded_bytes(&path, 128 * 1024)?;
        packs.push(
            bri_sim::bot_kind::BotPack::from_json(&bytes)
                .with_context(|| format!("Add-On {dir}: bots.json"))?,
        );
    }
    bri_sim::bot_kind::BotPack::merge(packs)
}
fn read_weapons(root: &Path) -> Result<(PathBuf, Vec<u8>, bri_weapons::Pack)> {
    let manifest = contained(root, "weapons.json")?;
    let mut bytes = Vec::new();
    std::fs::File::open(&manifest)?
        .take(WEAPON_INDEX_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    let pack = bri_weapons::Pack::from_json(&bytes)?;
    ensure!(
        pack.resources.len() <= 4096,
        "Weapon resource budget exceeded"
    );
    Ok((manifest, bytes, pack))
}
impl WeaponContent {
    pub fn load(root: &Path) -> Result<Self> {
        Self::load_with(root, &[])
    }
    /// The base weapons package at `root` merged with `extras` (see
    /// [`kind_providers`]). Without extras this is exactly [`Self::load`].
    pub fn load_with(root: &Path, extras: &[(String, PathBuf)]) -> Result<Self> {
        let root = root.canonicalize()?;
        let (manifest, bytes, pack) = read_weapons(&root)?;
        let base_items = pack.items.keys().cloned().collect();
        let mut files = BTreeMap::from([("weapons.json".into(), manifest)]);
        let mut expected = BTreeMap::from([(
            "weapons.json".into(),
            format!("{:x}", Sha256::digest(&bytes)),
        )]);
        let mut total = bytes.len() as u64;
        let mut parts = Vec::new();
        for (dir, abs) in extras {
            let add_on = || {
                format!(
                    "Add-On {}: weapons.json",
                    bri_package::library::add_on_label(abs, dir)
                )
            };
            let abs = abs.canonicalize().with_context(add_on)?;
            let (manifest, part_bytes, part) = read_weapons(&abs).with_context(add_on)?;
            let key = format!("{dir}/weapons.json");
            expected.insert(key.clone(), format!("{:x}", Sha256::digest(&part_bytes)));
            files.insert(key, manifest);
            total += part_bytes.len() as u64;
            parts.push((dir.clone(), abs, part));
        }
        // Native resource files and the sound files packs ship, each
        // relative to its own pack's folder. A sound playing the game's own
        // file ships none; the audio pack's identity covers that file.
        let native = |pack: &bri_weapons::Pack| -> Vec<String> {
            pack.resources
                .iter()
                .filter_map(|r| r.native_file.clone())
                .chain(
                    pack.sounds
                        .values()
                        .filter(|s| !s.stock)
                        .map(|s| s.file.clone()),
                )
                .collect()
        };
        let resources = native(&pack)
            .into_iter()
            .map(|name| (None, root.clone(), name))
            .chain(parts.iter().flat_map(|(dir, abs, part)| {
                native(part)
                    .into_iter()
                    .map(move |name| (Some(dir.clone()), abs.clone(), name))
            }))
            .collect::<Vec<_>>();
        for (dir, root, name) in resources {
            ensure!(name != "weapons.json", "Reserved weapon resource filename");
            let key = dir.map_or(name.clone(), |d| format!("{d}/{name}"));
            if files.contains_key(&key) {
                continue; // Shared source resources may bind the same native file.
            }
            let path = contained(&root, &name)?;
            let size = std::fs::metadata(&path)?.len();
            ensure!(
                size <= WEAPON_RESOURCE_LIMIT,
                "Oversized weapon resource: {name}"
            );
            total = total
                .checked_add(size)
                .context("Weapon byte budget overflow")?;
            ensure!(
                total <= WEAPON_TOTAL_LIMIT,
                "Weapon total byte budget exceeded"
            );
            files.insert(key, path);
        }
        // Load order of each item: the base game's first, then each Add-On's
        // in `packages.json` order. A duplicate id keeps the earlier, as
        // `merge` does.
        let mut load_order: BTreeMap<String, usize> = BTreeMap::new();
        for (rank, items) in std::iter::once(&pack.items)
            .chain(parts.iter().map(|(_, _, part)| &part.items))
            .enumerate()
        {
            for id in items.keys() {
                load_order.entry(id.clone()).or_insert(rank);
            }
        }
                // What the Add-Ons name of their effects, for the wrench.
        let named = |list: Vec<(&str, &str)>| -> Vec<(String, String)> {
            list.into_iter()
                .filter(|(_, name)| !name.trim().is_empty())
                .map(|(id, name)| (id.to_owned(), name.trim().to_owned()))
                .collect()
        };
        let mut emitter_choices = named(
            parts
                .iter()
                .flat_map(|(_, _, part)| &part.effects.emitters)
                .map(|e| (e.id.as_str(), e.name.as_str()))
                .collect(),
        );
        let mut light_choices = named(
            parts
                .iter()
                .flat_map(|(_, _, part)| &part.effects.lights)
                .map(|l| (l.id.as_str(), l.name.as_str()))
                .collect(),
        );
        // What each Add-On depends on, from its manifest beside `assets/`:
        // a damage type it re-declares from one replaces that one's.
        let depends_on = parts
            .iter()
            .filter_map(|(_, abs, part)| {
                let info = bri_package::library::package_info(abs.parent()?)?;
                let deps = info
                    .dependencies
                    .into_keys()
                    .chain(info.optional_dependencies.into_keys())
                    .collect();
                Some((part.id.clone(), deps))
            })
            .collect();
        let (mut pack, notes) = pack.merge_with(
            parts
                .into_iter()
                .map(|(dir, _, part)| (dir, part))
                .collect(),
            &depends_on,
        );
        pack.diagnostics
            .extend(notes.iter().map(|n| format!("merge: {n}")));
        pack.validate()?;
        // The bounded reads above supplied the actual parsed definitions. Require
        // identical bytes during hashing so a replacement cannot mix snapshots.
        let fingerprint = hash_files_bounded(
            b"BRI_WEAPONS_V1\0",
            files,
            &expected,
            WEAPON_RESOURCE_LIMIT,
            WEAPON_TOTAL_LIMIT,
        )?;
        let (item_choices, aliases) = item_choices(&pack, &load_order)?;
        // Only what the merge kept, each id once and no name twice.
        for (choices, kept) in [
            (
                &mut emitter_choices,
                pack.effects.emitters.iter().map(|e| &e.id).collect::<std::collections::BTreeSet<_>>(),
            ),
            (
                &mut light_choices,
                pack.effects.lights.iter().map(|l| &l.id).collect(),
            ),
        ] {
            let mut names = std::collections::BTreeSet::new();
            choices.retain(|(id, name)| {
                kept.contains(id)
                    && name.len() <= 128
                    && !name.chars().any(char::is_control)
                    && bri_world::ContentRef::Resolved(id.clone()).validate().is_ok()
                    && names.insert(name.to_ascii_lowercase())
            });
            ensure!(choices.len() <= 1024, "Add-On effect choice budget exceeded");
            choices.sort_by(|a, b| {
                a.1.to_ascii_lowercase()
                    .cmp(&b.1.to_ascii_lowercase())
                    .then(a.0.cmp(&b.0))
            });
        }
        let authored = (!pack.bindings.is_empty())
            .then(|| (std::sync::Arc::new(pack.clone()), BTreeMap::new()));
        Ok(Self {
            pack,
            authored,
            load_order,
            item_choices,
            emitter_choices,
            light_choices,
            aliases,
            fingerprint,
            manifest_sha256: format!("{:x}", Sha256::digest(&bytes)),
            base_items,
            problems: notes,
        })
    }

    /// The pack as its Add-Ons author it, which a host plays with its own
    /// settings.
    pub fn authored(&self) -> &bri_weapons::Pack {
        self.authored.as_ref().map_or(&self.pack, |(a, _)| a)
    }

    /// Plays the pack the server settings `values` make of the authored
    /// one (by the name each binding uses, as a server sends them); no
    /// values play it as authored.
    /// Plays the pack the server's `values` make of the authored one
    /// ([`bri_weapons::Binding`]); `Ok(true)` when the items players pick
    /// from changed with it (a setting showing or hiding some), so lists
    /// built from [`Self::item_choices`] are built again.
    pub fn apply_settings(&mut self, values: &BTreeMap<String, String>) -> Result<bool> {
        let Some((authored, applied)) = &mut self.authored else {
            return Ok(false);
        };
        if applied == values {
            return Ok(false);
        }
        applied.clone_from(values);
        // Values it cannot take play it as authored, once.
        let played = authored.with_settings(|name| values.get(name).cloned());
        let failed = played.as_ref().err().map(|e| anyhow::anyhow!("{e:#}"));
        self.pack = played.unwrap_or_else(|_| (**authored).clone());
        let (choices, aliases) = item_choices(&self.pack, &self.load_order)?;
        let changed = choices != self.item_choices;
        self.item_choices = choices;
        self.aliases = aliases;
        match failed {
            Some(error) => Err(error),
            None => Ok(changed),
        }
    }

    /// Catalogs are immutable during an App lifetime. Reloading requires restart
    /// so cached UI choices cannot disagree with newly loaded server definitions.
    pub fn ensure_same(&self, other: &Self) -> Result<()> {
        ensure!(
            self.fingerprint == other.fingerprint,
            "Native weapons changed after startup; restart before hosting or joining"
        );
        Ok(())
    }

    /// Resolve known imported display names; report the remaining unresolved count.
    /// Raw source records are preserved and never interpreted by this operation.
    pub fn resolve_world_items(&self, world: &mut bri_world::World) -> Result<usize> {
        let mut unresolved = 0;
        for id in world.bricks.keys().copied().collect::<Vec<_>>() {
            let Some(brick) = world.bricks.get_mut(&id) else {
                continue;
            };
            brick.item_spawn.resolve_item(&self.aliases)?;
            unresolved += usize::from(matches!(
                brick.item_spawn.item,
                Some(bri_world::ContentRef::Unresolved(_))
            ));
        }
        Ok(unresolved)
    }
}

/// Native metadata/resource integrity only: no renderer or original-file reader.
#[derive(Clone)]
pub struct ItemPhysicsContent {
    pub bounds: BTreeMap<String, bri_weapons::ItemBounds>,
    fingerprint: String,
}
#[derive(serde::Deserialize)]
struct PhysicsManifest {
    schema_version: u32,
    weapons_sha256: String,
    item_physics_sha256: String,
    models: BTreeMap<String, PhysicsModel>,
    textures: BTreeMap<String, PhysicsTexture>,
    items: BTreeMap<String, PhysicsItem>,
}
#[derive(serde::Deserialize)]
struct PhysicsModel {
    file: String,
    sha256: String,
    source_sha256: String,
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
}
#[derive(serde::Deserialize)]
struct PhysicsTexture {
    file: String,
    sha256: String,
}
#[derive(serde::Deserialize)]
struct PhysicsItem {
    model: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PhysicsCatalog {
    schema_version: u32,
    items: BTreeMap<String, bri_weapons::ItemBounds>,
}
fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn bounded_bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= limit,
        "Oversized native item metadata"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Native item metadata grew beyond limit"
    );
    Ok(bytes)
}
impl ItemPhysicsContent {
    /// [`Self::load`] plus the drop bounds other weapon packages provide in
    /// `assets/item-physics.json`, bound to their own `weapons.json` by the
    /// `presentation.json` beside it. `extras` are [`kind_providers`] of
    /// `weapons.json`; a package without these files keeps no drop bounds.
    pub fn load_with(
        root: &Path,
        weapons: &WeaponContent,
        extras: &[(String, PathBuf)],
    ) -> Result<Self> {
        let (mut content, models) = Self::load_models(root, weapons)?;
        let mut hash = Sha256::new();
        hash.update(&content.fingerprint);
        for (dir, abs) in extras {
            let abs = abs.canonicalize()?;
            if !abs.join("item-physics.json").is_file() {
                continue;
            }
            let manifest_bytes =
                bounded_bytes(&contained(&abs, "presentation.json")?, WEAPON_INDEX_LIMIT)?;
            let physics_bytes =
                bounded_bytes(&contained(&abs, "item-physics.json")?, 2 * 1024 * 1024)?;
            let weapons_bytes =
                bounded_bytes(&contained(&abs, "weapons.json")?, WEAPON_INDEX_LIMIT)?;
            let manifest: PhysicsManifest = serde_json::from_slice(&manifest_bytes)?;
            let physics: PhysicsCatalog = serde_json::from_slice(&physics_bytes)?;
            ensure!(
                manifest.schema_version == 2
                    && physics.schema_version == 1
                    && manifest.weapons_sha256 == format!("{:x}", Sha256::digest(&weapons_bytes))
                    && manifest.item_physics_sha256
                        == format!("{:x}", Sha256::digest(&physics_bytes)),
                "Add-On {}: item-physics.json does not match weapons.json; run the Add-On importer again",
                bri_package::library::add_on_label(&abs, dir)
            );
            for (id, bounds) in physics.items {
                bounds.validate()?;
                ensure!(
                    weapons.pack.items.contains_key(&id) && !content.bounds.contains_key(&id),
                    "Add-On {}: item-physics.json has bounds for unknown or already bound item {id}",
                    bri_package::library::add_on_label(&abs, dir)
                );
                content.bounds.insert(id, bounds);
            }
            hash.update(dir.as_bytes());
            hash.update(Sha256::digest(&manifest_bytes));
            hash.update(Sha256::digest(&physics_bytes));
        }
        if !extras.is_empty() {
            content.fingerprint = format!("{:x}", hash.finalize());
        }
        // A merged item nothing gave bounds (the Duplicator's wand, an
        // Add-On imported without its item physics) takes its stock model's
        // box, as its presentation borrows that model, else a stand-in box:
        // a gap in presentation never refuses the item. Derived only from
        // content already fingerprinted above.
        for (id, item) in &weapons.pack.items {
            if !content.bounds.contains_key(id) {
                let model = item.model.replace('\\', "/").to_ascii_lowercase();
                let bounds = models.get(&model).copied();
                content.bounds.insert(
                    id.clone(),
                    bounds.unwrap_or(bri_weapons::ItemBounds::FALLBACK),
                );
            }
        }
        Ok(content)
    }
    pub fn load(root: &Path, weapons: &WeaponContent) -> Result<Self> {
        Ok(Self::load_models(root, weapons)?.0)
    }
    /// [`Self::load`] and the bounds of every stock model it presents.
    fn load_models(
        root: &Path,
        weapons: &WeaponContent,
    ) -> Result<(Self, BTreeMap<String, bri_weapons::ItemBounds>)> {
        let root = root.canonicalize()?;
        let manifest_path = contained(&root, "presentation.json")?;
        let physics_path = contained(&root, "item-physics.json")?;
        let manifest_bytes = bounded_bytes(&manifest_path, WEAPON_INDEX_LIMIT)?;
        let physics_bytes = bounded_bytes(&physics_path, 2 * 1024 * 1024)?;
        let manifest: PhysicsManifest = serde_json::from_slice(&manifest_bytes)?;
        let physics: PhysicsCatalog = serde_json::from_slice(&physics_bytes)?;
        ensure!(
            manifest.schema_version == 2 && physics.schema_version == 1,
            "Unsupported native item physics/presentation schema"
        );
        ensure!(
            manifest.weapons_sha256 == weapons.manifest_sha256,
            "Item physics weapon checksum mismatch"
        );
        ensure!(
            manifest.item_physics_sha256 == format!("{:x}", Sha256::digest(&physics_bytes)),
            "Item physics checksum mismatch"
        );
        ensure!(
            manifest.models.len() <= 1024 && manifest.textures.len() <= 4096,
            "Item presentation resource budget exceeded"
        );
        // Presentation covers the base package's items; merged packages'
        // items have none yet (docs/audits/spike-addon-import.md, step e).
        ensure!(
            physics.items.len() == weapons.base_items.len()
                && manifest.items.len() == physics.items.len(),
            "Item physics catalog coverage mismatch"
        );
        for id in &weapons.base_items {
            let bounds = physics
                .items
                .get(id)
                .context("Missing native item bounds")?;
            bounds.validate()?;
            let item = manifest
                .items
                .get(id)
                .context("Missing presentation item")?;
            let model = manifest
                .models
                .get(&item.model)
                .context("Missing item bounds model")?;
            ensure!(
                bounds.min == model.bounds_min && bounds.max == model.bounds_max,
                "Item bounds disagree with authored model: {id}"
            );
            if let Some(weapon) = weapons.pack.items.get(id) {
                ensure!(
                    item.model == weapon.model.replace('\\', "/").to_ascii_lowercase(),
                    "Item model disagrees with weapon definition: {id}"
                );
            }
        }
        let mut files = BTreeMap::from([
            ("presentation.json".into(), manifest_path),
            ("item-physics.json".into(), physics_path),
        ]);
        let mut expected = BTreeMap::from([
            (
                "presentation.json".into(),
                format!("{:x}", Sha256::digest(&manifest_bytes)),
            ),
            ("item-physics.json".into(), manifest.item_physics_sha256),
        ]);
        for model in manifest.models.values() {
            ensure!(
                valid_sha256(&model.source_sha256),
                "Invalid original model provenance hash"
            );
            bri_weapons::ItemBounds {
                min: model.bounds_min,
                max: model.bounds_max,
            }
            .validate()?;
        }
        for (name, checksum) in manifest
            .models
            .values()
            .map(|r| (&r.file, &r.sha256))
            .chain(manifest.textures.values().map(|r| (&r.file, &r.sha256)))
        {
            ensure!(valid_sha256(checksum), "Invalid native item resource hash");
            ensure!(
                name != "presentation.json" && name != "item-physics.json",
                "Reserved item resource filename"
            );
            if let Some(previous) = expected.get(name) {
                ensure!(
                    previous == checksum,
                    "Conflicting native item resource hash"
                );
                continue;
            }
            files.insert(name.clone(), contained(&root, name)?);
            expected.insert(name.clone(), checksum.clone());
        }
        let fingerprint = hash_files_bounded(
            b"BRI_ITEM_PRESENTATION_PHYSICS_V1\0",
            files,
            &expected,
            WEAPON_RESOURCE_LIMIT,
            WEAPON_TOTAL_LIMIT,
        )?;
        let models = manifest
            .models
            .iter()
            .map(|(key, model)| {
                let bounds = bri_weapons::ItemBounds {
                    min: model.bounds_min,
                    max: model.bounds_max,
                };
                (key.clone(), bounds)
            })
            .collect();
        Ok((
            Self {
                bounds: physics.items,
                fingerprint,
            },
            models,
        ))
    }
    pub fn ensure_same(&self, other: &Self) -> Result<()> {
        ensure!(
            self.fingerprint == other.fingerprint,
            "Native item physics/presentation changed after startup; restart before hosting or joining"
        );
        Ok(())
    }
}

fn contained(root: &Path, relative: &str) -> Result<PathBuf> {
    ensure!(
        bri_content::brick_materials::safe_relative(relative),
        "Unsafe native content path: {relative}"
    );
    let path = root
        .join(relative)
        .canonicalize()
        .with_context(|| format!("Missing native resource {relative}"))?;
    ensure!(
        path.starts_with(root) && path.is_file(),
        "Native resource escapes package: {relative}"
    );
    Ok(path)
}

fn hash_files_bounded(
    domain: &[u8],
    files: BTreeMap<String, PathBuf>,
    expected: &BTreeMap<String, String>,
    file_limit: u64,
    total_limit: u64,
) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(domain);
    let mut buffer = [0_u8; 65536];
    let mut total = 0_u64;
    for (name, path) in files {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        let mut file = std::fs::File::open(&path)
            .with_context(|| format!("Reading native identity resource {}", path.display()))?;
        let size = file.metadata()?.len();
        ensure!(
            size <= file_limit,
            "Native identity resource exceeds byte budget: {name}"
        );
        total = total
            .checked_add(size)
            .context("Native identity byte budget overflow")?;
        ensure!(
            total <= total_limit,
            "Native identity total byte budget exceeded"
        );
        hash.update(size.to_le_bytes());
        let mut file_hash = Sha256::new();
        let mut read = 0u64;
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
            file_hash.update(&buffer[..n]);
            read += n as u64;
            ensure!(read <= size, "Native content changed while hashing: {name}");
        }
        ensure!(read == size, "Native content changed while hashing: {name}");
        if let Some(expected) = expected.get(&name) {
            ensure!(
                &format!("{:x}", file_hash.finalize()) == expected,
                "Native material hash mismatch: {name}"
            );
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// The items players pick, by id and name, and their names in lower case
/// to their ids ([`item_choices`]).
type ItemChoices = (Vec<(String, String)>, BTreeMap<String, String>);

/// The items players pick from a pack, by name, and their names in lower
/// case to their ids. Hidden items are put in the world by scripts only:
/// no one picks them from a list. Items that share a display name all
/// stay, as in v20; among them the first loaded (`load_order`) comes
/// first, and saved bricks naming one bind to it.
fn item_choices(
    pack: &bri_weapons::Pack,
    load_order: &BTreeMap<String, usize>,
) -> Result<ItemChoices> {
    let mut item_choices: Vec<_> = pack
        .items
        .values()
        .filter(|item| !item.hidden)
        .map(|item| (item.id.clone(), item.ui_name.trim().to_string()))
        .collect();
    ensure!(
        item_choices.len() <= 1024,
        "Weapon item catalog budget exceeded"
    );
    let mut ids = std::collections::BTreeSet::new();
    for (id, name) in &item_choices {
        bri_world::ContentRef::Resolved(id.clone()).validate()?;
        ensure!(!id.chars().any(char::is_control), "Invalid weapon item ID");
        ensure!(ids.insert(id.clone()), "Duplicate weapon item ID: {id}");
        ensure!(
            !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control),
            "Invalid weapon item name"
        );
    }
    let rank = |id: &str| load_order.get(id).copied().unwrap_or(usize::MAX);
    item_choices.sort_by(|a, b| {
        a.1.to_ascii_lowercase()
            .cmp(&b.1.to_ascii_lowercase())
            .then(rank(&a.0).cmp(&rank(&b.0)))
            .then(a.0.cmp(&b.0))
    });
    let aliases = bri_world::item_aliases(
        item_choices
            .iter()
            .map(|(id, name)| (id.as_str(), name.as_str())),
    );
    Ok((item_choices, aliases))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The four core tools as pack items, each with its own v20-style image,
    /// named like the stock pack (including Hammer's trailing space).
    fn core_tool_items() -> (
        BTreeMap<String, bri_weapons::Item>,
        BTreeMap<String, bri_weapons::Image>,
    ) {
        let mut items = BTreeMap::new();
        let mut images = BTreeMap::new();
        for (id, ui_name) in bri_weapons::CORE_TOOLS
            .into_iter()
            .zip(["Hammer ", "Wrench", "Printer", "Wand"])
        {
            let stem = id
                .trim_start_matches("v20.weapon.")
                .trim_end_matches("item");
            let image = format!("v20.image.{stem}image");
            images.insert(
                image.clone(),
                bri_weapons::Image {
                    id: image.clone(),
                    name: format!("{stem}Image"),
                    model: "source.dts".into(),
                    projectile: None,
                    mount_point: 0,
                    offset: [0.; 3],
                    eye_offset: [0.; 3],
                    source_rotation_degrees: [0.; 3],
                    correct_muzzle: false,
                    melee: true,
                    color: [1.; 4],
                    color_shift: false,
                    arm_ready: true,
                    casing: String::new(),
                    min_shot_ticks: 0,
                    states: vec![],
                    command: None,
                    commands: Default::default(),
                    shot: None,
                    eye_rotation: [0.0; 3],
                    zoom: None,
                    bot: None,
                    crosshair: true,
                    follow_arm: false,
                    hide_nodes: Vec::new(),
                    both_arms: false,
                    paint_tint: false,
                    left_image: None,
                    magazine: None,
                    volleys: vec![],
                    last_shot: None,
                    state_shots: Default::default(),
                    cook: None,
                    guard: None,
                    rope: None,
                    light: None,
                    paint_picker: false,
                    scripts: Default::default(),
                },
            );
            items.insert(
                id.into(),
                bri_weapons::Item {
                    id: id.into(),
                    name: format!("{stem}Item"),
                    ui_name: ui_name.into(),
                    image,
                    model: "source.dts".into(),
                    icon: String::new(),
                    can_drop: true,
                    sport: false,
                    ..Default::default()
                },
            );
        }
        (items, images)
    }
    fn weapon_fixture() -> (PathBuf, bri_weapons::Pack) {
        let root = tempfile::tempdir().unwrap().keep().join("weapons");
        std::fs::create_dir(&root).unwrap();
        let (items, images) = core_tool_items();
        let pack = bri_weapons::Pack {
            effects: Default::default(),
            schema_version: bri_weapons::SCHEMA,
            id: "test.weapons".into(),
            items,
            images,
            projectiles: BTreeMap::new(),
            external_projectiles: Default::default(),
            damage_types: BTreeMap::new(),
            explosions: BTreeMap::new(),
            sounds: Default::default(),
            definitions: vec![],
            resources: vec![bri_weapons::Resource {
                path: "original.dts".into(),
                sha256: "a".repeat(64),
                native_file: Some("shape.json".into()),
                diagnostics: vec![],
                package: None,
            }],
            diagnostics: vec![],
            bindings: vec![],
        };
        std::fs::write(root.join("shape.json"), b"native shape bytes").unwrap();
        write_weapons(&root, &pack);
        (root, pack)
    }
    fn write_weapons(root: &Path, pack: &bri_weapons::Pack) {
        std::fs::write(root.join("weapons.json"), serde_json::to_vec(pack).unwrap()).unwrap();
    }
    #[test]
    fn add_on_effects_with_names_are_offered_once_each() {
        let (root, _) = weapon_fixture();
        let part_root = root.parent().unwrap().join("crit");
        std::fs::create_dir(&part_root).unwrap();
        let emitter = |id: &str, name: &str| bri_content::effects::Emitter {
            id: id.into(),
            name: name.into(),
            particles: vec!["crit:particle/critparticle".into()],
            period: 0.035,
            period_variance: 0.0,
            speed: 0.0,
            speed_variance: 0.0,
            offset: 1.8,
            offset_variance: 0.0,
            theta_degrees: [0.0, 0.0],
            phi_rate_degrees: 0.0,
            phi_variance_degrees: 0.0,
            lifetime: 0.1,
            lifetime_variance: 0.0,
            orient: false,
            orient_on_velocity: false,
            override_advance: false,
            use_emitter_colors: false,
            use_emitter_sizes: false,
            use_placement_velocity: false,
            node_time_scale: 1.0,
            point_node_time_scale: 1.0,
        };
        let part = bri_weapons::Pack {
            schema_version: bri_weapons::SCHEMA,
            id: "crit".into(),
            items: BTreeMap::new(),
            images: BTreeMap::new(),
            projectiles: BTreeMap::new(),
            external_projectiles: Default::default(),
            damage_types: BTreeMap::new(),
            explosions: BTreeMap::new(),
            sounds: Default::default(),
            definitions: vec![],
            resources: vec![],
            diagnostics: vec![],
            effects: bri_weapons::PackEffects {
                particles: vec![bri_content::effects::Particle {
                    id: "crit:particle/critparticle".into(),
                    texture: "base/data/particles/dot".into(),
                    alpha_blend: false,
                    lifetime: 0.5,
                    lifetime_variance: 0.0,
                    drag: 5.0,
                    wind: 0.0,
                    gravity: 0.0,
                    inherited_velocity: 0.0,
                    acceleration: 0.0,
                    spin_degrees: 0.0,
                    random_spin: [0.0, 0.0],
                    keys: [0.0, 1.0]
                        .map(|time| bri_content::effects::ParticleKey {
                            time,
                            color: [0.0, 1.0, 0.0, 1.0],
                            size: 1.5,
                        })
                        .into(),
                }],
                emitters: vec![
                    emitter("crit:emitter/critemitter", " Emote - Critical Hit "),
                    emitter("crit:emitter/unnamed", ""),
                    emitter("crit:emitter/again", "emote - critical hit"),
                ],
                lights: vec![bri_content::effects::Light {
                    id: "crit:light/glow".into(),
                    name: "Glow".into(),
                    enabled: true,
                    color: [1.0, 1.0, 1.0],
                    brightness: 1.0,
                    radius: 4.0,
                    color_curves: None,
                    brightness_curve: None,
                    radius_curve: None,
                    flare: None,
                }],
                explosions: vec![],
            },
            bindings: vec![],
        };
        write_weapons(&part_root, &part);
        let content =
            WeaponContent::load_with(&root, &[("crit/assets".into(), part_root)]).unwrap();
        assert_eq!(
            content.emitter_choices,
            [(
                "crit:emitter/critemitter".to_string(),
                "Emote - Critical Hit".to_string()
            )]
        );
        assert_eq!(
            content.light_choices,
            [("crit:light/glow".to_string(), "Glow".to_string())]
        );
        assert!(
            WeaponContent::load(&root).unwrap().emitter_choices.is_empty(),
            "the base pack's own are the native library's"
        );
    }
    #[test]
    fn weapons_identity_hashes_native_bytes_and_rejects_catalog_replacement() {
        let (root, mut pack) = weapon_fixture();
        let before = WeaponContent::load(&root).unwrap();
        assert_eq!(before.item_choices.len(), 4);
        before
            .ensure_same(&WeaponContent::load(&root).unwrap())
            .unwrap();
        // Original DTS provenance is deliberately not a native integrity checksum.
        std::fs::write(root.join("shape.json"), b"different native bytes").unwrap();
        let changed = WeaponContent::load(&root).unwrap();
        assert!(before.ensure_same(&changed).is_err());
        pack.diagnostics.push("definition metadata change".into());
        write_weapons(&root, &pack);
        assert!(
            changed
                .ensure_same(&WeaponContent::load(&root).unwrap())
                .is_err()
        );
    }
    #[test]
    fn weapons_resources_are_contained_and_have_count_file_and_total_budgets() {
        let (root, mut pack) = weapon_fixture();
        pack.resources[0].native_file = Some("../shape.json".into());
        write_weapons(&root, &pack);
        assert!(WeaponContent::load(&root).is_err());
        pack.resources[0].native_file = Some("weapons.json".into());
        write_weapons(&root, &pack);
        assert!(WeaponContent::load(&root).is_err());
        pack.resources[0].native_file = Some("missing.json".into());
        write_weapons(&root, &pack);
        assert!(WeaponContent::load(&root).is_err());
        pack.resources[0].native_file = Some("shape.json".into());
        std::fs::File::create(root.join("shape.json"))
            .unwrap()
            .set_len(WEAPON_RESOURCE_LIMIT + 1)
            .unwrap();
        write_weapons(&root, &pack);
        assert!(
            WeaponContent::load(&root)
                .err()
                .unwrap()
                .to_string()
                .contains("Oversized")
        );
        let resource = pack.resources[0].clone();
        pack.resources = vec![resource.clone(); 4097];
        write_weapons(&root, &pack);
        assert!(
            WeaponContent::load(&root)
                .err()
                .unwrap()
                .to_string()
                .contains("resource budget")
        );
        pack.resources.clear();
        for index in 0..9 {
            let name = format!("large-{index}.json");
            std::fs::File::create(root.join(&name))
                .unwrap()
                .set_len(WEAPON_RESOURCE_LIMIT)
                .unwrap();
            let mut resource = resource.clone();
            resource.native_file = Some(name);
            pack.resources.push(resource);
        }
        write_weapons(&root, &pack);
        assert!(
            WeaponContent::load(&root)
                .err()
                .unwrap()
                .to_string()
                .contains("total byte budget")
        );
        // Tests leave only bounded metadata fixtures, not hundreds of MiB.
        for resource in &pack.resources {
            std::fs::File::create(root.join(resource.native_file.as_ref().unwrap())).unwrap();
        }
        std::fs::File::create(root.join("shape.json")).unwrap();
    }
    #[test]
    fn weapon_alias_resolution_preserves_unknown_names_and_source_records() {
        let (root, _) = weapon_fixture();
        let content = WeaponContent::load(&root).unwrap();
        let mut world = bri_world::World::new("test".into(), "map".into(), vec![[1.; 4]]);
        for (id, name) in [(1, " Hammer "), (2, "unknown addon")] {
            let mut brick =
                bri_world::Brick::new(bri_world::ContentRef::Resolved("brick".into()), [0.; 3], 0);
            brick.item_spawn.item = Some(bri_world::ContentRef::unresolved("item_ui", name));
            brick.source_records.push(bri_world::SourceRecord {
                line: 1,
                text: "+-ITEM untouched".into(),
                diagnostic: None,
            });
            world.bricks.insert(id, brick);
        }
        let original = world.bricks[&1].source_records.clone();
        assert_eq!(content.resolve_world_items(&mut world).unwrap(), 1);
        assert_eq!(
            world.bricks[&1].item_spawn.item,
            Some(bri_world::ContentRef::Resolved(bri_weapons::HAMMER.into()))
        );
        assert_eq!(world.bricks[&1].source_records, original);
        assert!(matches!(
            world.bricks[&2].item_spawn.item,
            Some(bri_world::ContentRef::Unresolved(_))
        ));
        assert_eq!(content.resolve_world_items(&mut world).unwrap(), 1);
    }
    #[test]
    fn items_sharing_a_display_name_all_load_and_saves_bind_the_first_loaded() {
        // Kaje's Sniper Rifle and the Adventure Pack's both say "Sniper
        // Rifle"; an Add-On's tool may also reuse a base game name. v20 lists
        // them all, so no Add-On is left out over a name.
        let (root, base) = weapon_fixture();
        let hammer = base.items[bri_weapons::HAMMER].clone();
        let content = root.parent().unwrap();
        let mut extras = Vec::new();
        // Loaded first, though its id sorts last.
        for (dir, items) in [
            (
                "zz_sniper",
                vec![("zz_sniper:weapon/sniperrifleitem", "Sniper Rifle")],
            ),
            (
                "aa_adventure",
                vec![
                    ("aa_adventure:weapon/sniperrifleitem", "sniper rifle "),
                    ("aa_adventure:weapon/hammeritem", "Hammer"),
                ],
            ),
        ] {
            let abs = content.join(dir);
            std::fs::create_dir(&abs).unwrap();
            let mut part = base.clone();
            part.id = dir.into();
            part.resources.clear();
            part.items = items
                .into_iter()
                .map(|(id, name)| {
                    let mut item = hammer.clone();
                    item.id = id.into();
                    item.ui_name = name.into();
                    (id.to_string(), item)
                })
                .collect();
            write_weapons(&abs, &part);
            extras.push((dir.to_string(), abs));
        }
        let weapons = WeaponContent::load_with(&root, &extras).unwrap();
        assert_eq!(weapons.item_choices.len(), 7, "every item stays");
        let snipers: Vec<_> = weapons
            .item_choices
            .iter()
            .filter(|(_, name)| name.trim().eq_ignore_ascii_case("sniper rifle"))
            .map(|(id, _)| id.as_str())
            .collect();
        assert_eq!(
            snipers,
            [
                "zz_sniper:weapon/sniperrifleitem",
                "aa_adventure:weapon/sniperrifleitem"
            ],
            "shared names list in load order"
        );
        let mut world = bri_world::World::new("test".into(), "map".into(), vec![[1.; 4]]);
        for (id, name) in [(1, "Sniper Rifle"), (2, "hammer")] {
            let mut brick =
                bri_world::Brick::new(bri_world::ContentRef::Resolved("brick".into()), [0.; 3], 0);
            brick.item_spawn.item = Some(bri_world::ContentRef::unresolved("item_ui", name));
            world.bricks.insert(id, brick);
        }
        assert_eq!(weapons.resolve_world_items(&mut world).unwrap(), 0);
        let bound = |id| world.bricks[&id].item_spawn.item.clone();
        assert_eq!(
            bound(1),
            Some(bri_world::ContentRef::Resolved(
                "zz_sniper:weapon/sniperrifleitem".into()
            ))
        );
        assert_eq!(
            bound(2),
            Some(bri_world::ContentRef::Resolved(bri_weapons::HAMMER.into())),
            "the base game's item first"
        );
    }
    /// The weapons package at `root` loads to the same identity twice and
    /// offers every item as a choice the tool catalog installs. Returns the
    /// item count.
    fn weapons_pack_identity_and_every_choice(root: &Path) -> usize {
        let content = WeaponContent::load(root).unwrap();
        let items = content.pack.items.len();
        assert!(items > 0);
        assert_eq!(content.item_choices.len(), items);
        content
            .ensure_same(&WeaponContent::load(root).unwrap())
            .unwrap();
        let mut catalog = bri_sim::session::ToolCatalog::default();
        catalog
            .install_items(content.item_choices.into_iter().map(|(id, _)| id))
            .unwrap();
        assert_eq!(catalog.items.len(), items);
        items
    }
    #[test]
    fn test_weapons_pack_identity_and_every_choice() {
        let root = tempfile::tempdir().unwrap().keep().join("weapons");
        std::fs::create_dir(&root).unwrap();
        let mut pack = bri_weapons::testing::pack();
        // A sound playing the game's own file, which the pack does not ship.
        pack.sounds.insert(
            "addon:sound/boom".into(),
            bri_weapons::SoundDef {
                file: "base/data/sound/vehicleexplosion.wav".into(),
                volume: 1.0,
                looping: false,
                local: false,
                package: None,
                stock: true,
            },
        );
        // Every native file the pack ships, with made-up bytes.
        for name in pack
            .resources
            .iter()
            .filter_map(|r| r.native_file.clone())
            .chain(
                pack.sounds
                    .values()
                    .filter(|s| !s.stock)
                    .map(|s| s.file.clone()),
            )
        {
            let path = root.join(&name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, format!("native {name}")).unwrap();
        }
        write_weapons(&root, &pack);
        assert_eq!(
            weapons_pack_identity_and_every_choice(&root),
            pack.items.len()
        );
    }
    #[test]
    #[ignore = "requires generated v20 content"]
    fn native_weapons_pack_identity_and_all_21_choices() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/weapons-pack-009");
        // 17 weapons plus the four core tools, which are v20 images too.
        assert_eq!(weapons_pack_identity_and_every_choice(&root), 21);
    }
    fn physics_fixture() -> (PathBuf, WeaponContent, serde_json::Value, serde_json::Value) {
        let (root, _) = weapon_fixture();
        let weapons = WeaponContent::load(&root).unwrap();
        std::fs::write(root.join("texture.png"), b"native texture").unwrap();
        let bounds = serde_json::json!({"min":[-0.2,-0.3,-0.4],"max":[0.4,0.6,0.8]});
        let physics = serde_json::json!({"schema_version":1,"items":weapons.item_choices.iter().map(|(id,_)|(id.clone(),bounds.clone())).collect::<BTreeMap<_,_>>()});
        let manifest = serde_json::json!({"schema_version":2,"weapons_sha256":weapons.manifest_sha256,
            "item_physics_sha256":"",
            "items":weapons.item_choices.iter().map(|(id,_)|(id.clone(),serde_json::json!({"model":"source.dts"}))).collect::<BTreeMap<_,_>>(),
            "models":{"source.dts":{"file":"shape.json","sha256":format!("{:x}",Sha256::digest(b"native shape bytes")),"source_sha256":"b".repeat(64),"bounds_min":bounds["min"],"bounds_max":bounds["max"]}},
            "textures":{"texture":{"file":"texture.png","sha256":format!("{:x}",Sha256::digest(b"native texture"))}}});
        write_physics(&root, &manifest, &physics);
        (root, weapons, manifest, physics)
    }
    fn write_physics(root: &Path, manifest: &serde_json::Value, physics: &serde_json::Value) {
        let bytes = serde_json::to_vec(physics).unwrap();
        let mut manifest = manifest.clone();
        manifest["item_physics_sha256"] =
            serde_json::json!(format!("{:x}", Sha256::digest(&bytes)));
        std::fs::write(root.join("item-physics.json"), bytes).unwrap();
        std::fs::write(
            root.join("presentation.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn item_physics_pins_bounds_catalog_and_resources_into_identity_nine() {
        let (root, weapons, mut manifest, mut physics) = physics_fixture();
        let before = ItemPhysicsContent::load(&root, &weapons).unwrap();
        assert_eq!(before.bounds.len(), 4);
        before
            .ensure_same(&ItemPhysicsContent::load(&root, &weapons).unwrap())
            .unwrap();
        // A changed bound must be re-certified by the matching model metadata.
        physics["items"][bri_weapons::HAMMER]["min"][0] = serde_json::json!(-0.5);
        write_physics(&root, &manifest, &physics);
        assert!(
            ItemPhysicsContent::load(&root, &weapons)
                .err()
                .unwrap()
                .to_string()
                .contains("disagree")
        );
        for item in physics["items"].as_object_mut().unwrap().values_mut() {
            item["min"][0] = serde_json::json!(-0.5);
        }
        manifest["models"]["source.dts"]["bounds_min"][0] = serde_json::json!(-0.5);
        write_physics(&root, &manifest, &physics);
        let after = ItemPhysicsContent::load(&root, &weapons).unwrap();
        assert!(before.ensure_same(&after).is_err());
        std::fs::write(root.join("shape.json"), b"corrupted native shape").unwrap();
        assert!(ItemPhysicsContent::load(&root, &weapons).is_err());
        std::fs::write(root.join("shape.json"), b"native shape bytes").unwrap();
        std::fs::write(root.join("texture.png"), b"corrupted texture").unwrap();
        assert!(ItemPhysicsContent::load(&root, &weapons).is_err());
    }
    #[test]
    fn item_physics_rejects_stale_pins_incomplete_catalog_and_invalid_bounds() {
        let (root, weapons, manifest, physics) = physics_fixture();
        let mut invalid = manifest.clone();
        invalid["weapons_sha256"] = serde_json::json!("0".repeat(64));
        write_physics(&root, &invalid, &physics);
        assert!(
            ItemPhysicsContent::load(&root, &weapons)
                .err()
                .unwrap()
                .to_string()
                .contains("weapon checksum")
        );
        write_physics(&root, &manifest, &physics);
        std::fs::write(
            root.join("item-physics.json"),
            b"{\"schema_version\":1,\"items\":{}}",
        )
        .unwrap();
        assert!(
            ItemPhysicsContent::load(&root, &weapons)
                .err()
                .unwrap()
                .to_string()
                .contains("physics checksum")
        );
        for change in 0..4 {
            let mut invalid = physics.clone();
            match change {
                0 => {
                    invalid["items"]
                        .as_object_mut()
                        .unwrap()
                        .remove(bri_weapons::HAMMER);
                }
                1 => {
                    let value = invalid["items"]
                        .as_object_mut()
                        .unwrap()
                        .remove(bri_weapons::HAMMER)
                        .unwrap();
                    invalid["items"]["unknown"] = value;
                }
                2 => invalid["items"][bri_weapons::HAMMER]["min"][0] = serde_json::json!(2000),
                _ => invalid["schema_version"] = serde_json::json!(2),
            }
            write_physics(&root, &manifest, &invalid);
            assert!(ItemPhysicsContent::load(&root, &weapons).is_err());
        }
    }
    #[test]
    fn item_physics_rejects_escape_oversize_and_conflicting_resource_pins() {
        let (root, weapons, manifest, physics) = physics_fixture();
        for name in ["../escape.json", "presentation.json", "missing.json"] {
            let mut invalid = manifest.clone();
            invalid["models"]["source.dts"]["file"] = serde_json::json!(name);
            write_physics(&root, &invalid, &physics);
            assert!(ItemPhysicsContent::load(&root, &weapons).is_err());
        }
        let mut invalid = manifest.clone();
        invalid["textures"]["texture"]["file"] = serde_json::json!("shape.json");
        write_physics(&root, &invalid, &physics);
        assert!(
            ItemPhysicsContent::load(&root, &weapons)
                .err()
                .unwrap()
                .to_string()
                .contains("Conflicting")
        );
        write_physics(&root, &manifest, &physics);
        std::fs::File::create(root.join("shape.json"))
            .unwrap()
            .set_len(WEAPON_RESOURCE_LIMIT + 1)
            .unwrap();
        assert!(
            ItemPhysicsContent::load(&root, &weapons)
                .err()
                .unwrap()
                .to_string()
                .contains("byte budget")
        );
        std::fs::File::create(root.join("shape.json")).unwrap();
        std::fs::File::create(root.join("item-physics.json"))
            .unwrap()
            .set_len(2 * 1024 * 1024 + 1)
            .unwrap();
        assert!(
            ItemPhysicsContent::load(&root, &weapons)
                .err()
                .unwrap()
                .to_string()
                .contains("Oversized")
        );
        std::fs::File::create(root.join("item-physics.json")).unwrap();
    }
    /// The presentation pack at `root` pins valid bounds for every item
    /// choice of `weapons`. Returns how many.
    fn item_physics_covers_every_item(root: &Path, weapons: &WeaponContent) -> usize {
        let physics = ItemPhysicsContent::load(root, weapons).unwrap();
        assert_eq!(physics.bounds.len(), weapons.item_choices.len());
        for bounds in physics.bounds.values() {
            bounds.validate().unwrap();
        }
        physics.bounds.len()
    }
    #[test]
    fn test_item_physics_covers_every_item() {
        let (root, weapons, _, _) = physics_fixture();
        assert!(item_physics_covers_every_item(&root, &weapons) > 0);
    }
    #[test]
    #[ignore = "requires generated v20 content"]
    fn native_item_physics_covers_all_21_and_pins_authored_bounds() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let weapons = WeaponContent::load(&root.join("weapons-pack-009")).unwrap();
        assert_eq!(
            item_physics_covers_every_item(&root.join("item-presentation-pack-010"), &weapons),
            21
        );
    }
}
