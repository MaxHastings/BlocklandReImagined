//! v20 `.bls` saves players bring over. Any `.bls` dropped in the saves
//! folder, loose or in v20's own `saves/<Map>/` layout, converts once in the
//! background and joins Load Bricks. An old Blockland install's saves are
//! listed the same way. Original files are only ever read: the native copies
//! live in a cache folder of their own, and a save that will not convert is
//! skipped and logged.
use anyhow::{Context, Result, ensure};
use bri_bls::events::Aliases;
use bri_content::{brick::Catalog, effects::Library};
use bri_world::World;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

/// Bump when conversion output changes so cached saves convert again.
const CONVERTER_VERSION: u32 = 2;
const MAX_SOURCE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_SOURCES: usize = 2000;
/// Where loose `.bls` files, in no map folder, are listed.
pub const LOOSE_FOLDER: &str = "Other";

/// The v20 import the offline converter gives the stock saves: bricks by UI
/// name, lights and emitters, wrench events and spawn bricks, then items.
pub struct Converter {
    catalog: Catalog,
    /// Absent only in tests of the folder handling, which read bricks alone.
    bindings: Option<Bindings>,
    fingerprint: String,
}
struct Bindings {
    effects: Library,
    events: bri_events::Catalog,
    aliases: Aliases,
    weapons: bri_net::content_identity::WeaponContent,
}
impl Converter {
    pub fn new(content: &crate::content::ClientContent) -> Result<Self> {
        let paths = &content.paths;
        let read = |path: PathBuf| std::fs::read(&path).with_context(|| path.display().to_string());
        let audio = read(paths.audio.join("manifest.json"))?;
        let weapons = read(paths.weapons.join("weapons.json"))?;
        let vehicles = read(paths.vehicles.join("vehicles.json"))?;
        let aliases = Aliases::from_packs(
            &serde_json::from_slice(&audio)?,
            &serde_json::from_slice(&weapons)?,
            &serde_json::from_slice(&vehicles)?,
            &content.effects,
        )?;
        // Later names win in the reader, so Add-On bricks go first and a
        // stock brick keeps its v20 name.
        let mut sources = vec![];
        for (_, dir) in &paths.brick_extras {
            sources.push(read(dir.join("stock-catalog.json"))?);
        }
        sources.push(read(paths.brick_catalog.join("stock-catalog.json"))?);
        let mut catalog = Catalog {
            schema_version: 1,
            bricks: vec![],
        };
        let mut hash = Sha256::new();
        hash.update(CONVERTER_VERSION.to_le_bytes());
        for bytes in &sources {
            let part: Catalog = serde_json::from_slice(bytes)?;
            catalog.bricks.extend(part.bricks);
            hash.update(Sha256::digest(bytes));
        }
        for bytes in [&audio, &weapons, &vehicles] {
            hash.update(Sha256::digest(bytes));
        }
        hash.update(serde_json::to_vec(&content.effects)?);
        hash.update(content.events.fingerprint());
        Ok(Self {
            catalog,
            bindings: Some(Bindings {
                effects: content.effects.clone(),
                events: content.events.clone(),
                aliases,
                weapons: content.weapons.clone(),
            }),
            fingerprint: format!("{:x}", hash.finalize()),
        })
    }
    #[cfg(test)]
    fn bricks_only(catalog: Catalog, fingerprint: &str) -> Self {
        Self {
            catalog,
            bindings: None,
            fingerprint: format!("{fingerprint:0>16}"),
        }
    }
    pub fn convert(&self, bytes: &[u8], name: &str, map_id: &str) -> Result<World> {
        let (mut world, skipped) = bri_bls::bls::read_counting(bytes, &self.catalog, name, map_id)?;
        if skipped > 0 {
            bri_console::warn(format!(
                "{name}: skipped {skipped} brick lines v20 could not load either"
            ));
        }
        if let Some(b) = &self.bindings {
            bri_bls::effect_bindings::bind(&mut world, &b.effects)?;
            bri_bls::events::bind(&mut world, &b.events, &b.aliases)?;
            b.weapons.resolve_world_items(&mut world)?;
        }
        world.validate()?;
        Ok(world)
    }
}

/// One `.bls` as last seen, and its native copy.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Converted {
    source: PathBuf,
    length: u64,
    modified_ns: u128,
    fingerprint: String,
    /// The v20 map folder it sat in; [`LOOSE_FOLDER`] for a loose file.
    folder: String,
    /// The native copy in the cache folder, absent when it did not convert.
    file: Option<String>,
    map_id: String,
    description: Vec<String>,
    bricks: u32,
}
#[derive(Default, Serialize, Deserialize)]
struct Index {
    schema_version: u32,
    saves: Vec<Converted>,
}

/// A converted save ready for Load Bricks.
#[derive(Clone, Debug)]
pub struct Listed {
    /// The file's name without `.bls`.
    pub name: String,
    pub folder: String,
    pub map_id: String,
    pub description: Vec<String>,
    pub bricks: u32,
    pub modified_s: u64,
    /// The native copy, inside [`OldSaves::cache`].
    pub path: PathBuf,
    /// Found in an old Blockland install rather than the saves folder.
    pub old_install: bool,
}

pub struct OldSaves {
    saves: PathBuf,
    old_installs: Vec<PathBuf>,
    cache: PathBuf,
    converter: Mutex<Option<Arc<Converter>>>,
    index: Mutex<Option<BTreeMap<PathBuf, Converted>>>,
    running: AtomicBool,
    again: AtomicBool,
    changed: AtomicBool,
}
impl OldSaves {
    /// `saves` is the folder players drop saves into; `cache` holds the
    /// native copies and is ours alone.
    pub fn new(saves: PathBuf, cache: PathBuf, old_installs: Vec<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            saves,
            old_installs,
            cache,
            converter: Mutex::new(None),
            index: Mutex::new(None),
            running: AtomicBool::new(false),
            again: AtomicBool::new(false),
            changed: AtomicBool::new(false),
        })
    }
    pub fn saves_folder(&self) -> &Path {
        &self.saves
    }
    pub fn cache(&self) -> &Path {
        &self.cache
    }
    /// The saves folders of old Blockland installs in their usual places.
    pub fn find_old_installs() -> Vec<PathBuf> {
        let mut roots = vec![];
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(dir) = std::env::var_os(var).map(PathBuf::from) {
                roots.push(dir.join("Blockland"));
                roots.push(dir.join("Steam/steamapps/common/Blockland"));
            }
        }
        roots.push(PathBuf::from("C:/Blockland"));
        let mut found: Vec<PathBuf> = roots
            .into_iter()
            .map(|r| r.join("saves"))
            .filter(|s| s.is_dir())
            .filter_map(|s| s.canonicalize().ok())
            .collect();
        found.dedup();
        found
    }
    /// Use this content's bricks, events and items from now on; saves
    /// converted against other content convert again.
    pub fn set_converter(&self, converter: Converter) {
        *self.converter.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(converter));
    }
    /// Convert new and changed saves on a background thread. A call while
    /// one runs makes it look again when it finishes.
    pub fn start(self: &Arc<Self>) {
        self.again.store(true, Ordering::SeqCst);
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let this = self.clone();
        let spawned = std::thread::Builder::new()
            .name("old-saves".into())
            .spawn(move || {
                while this.again.swap(false, Ordering::SeqCst) {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| this.sync()))
                        .unwrap_or_else(|_| Err(anyhow::anyhow!("converter failed")));
                    if let Err(error) = result {
                        bri_console::warn(format!("Old saves: {error:#}"));
                    }
                }
                this.running.store(false, Ordering::SeqCst);
                // A start that raced the last check above.
                if this.again.load(Ordering::SeqCst) {
                    this.start();
                }
            });
        if spawned.is_err() {
            self.running.store(false, Ordering::SeqCst);
        }
    }
    /// Whether a conversion is running or queued.
    pub fn busy(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
    /// Whether saves appeared, changed or went away since the last call.
    pub fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::SeqCst)
    }
    /// Converted saves, old installs first: listed in order, the saves
    /// folder's copy of a name wins.
    pub fn list(&self) -> Vec<Listed> {
        let index = self.index.lock().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<Listed> = index
            .iter()
            .flat_map(|m| m.values())
            .filter_map(|c| {
                let file = c.file.as_ref()?;
                let name = c.source.file_stem()?.to_string_lossy().into_owned();
                Some(Listed {
                    name,
                    folder: c.folder.clone(),
                    map_id: c.map_id.clone(),
                    description: c.description.clone(),
                    bricks: c.bricks,
                    modified_s: (c.modified_ns / 1_000_000_000) as u64,
                    path: self.cache.join(file),
                    old_install: !c.source.starts_with(&self.saves),
                })
            })
            .collect();
        out.sort_by_key(|l| !l.old_install);
        out
    }
    fn index_path(&self) -> PathBuf {
        self.cache.join("index.json")
    }
    /// Every `.bls` to consider: the drop folder's loose files and map
    /// folders (not the game's own `map-` folders), then old installs' map
    /// folders, as v20's Load Bricks read them.
    fn sources(&self) -> Vec<(PathBuf, String)> {
        let bls = |p: &Path| {
            p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bls")) && p.is_file()
        };
        let entries = |dir: &Path| -> Vec<PathBuf> {
            let mut v: Vec<_> = std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .collect();
            v.sort();
            v
        };
        let map_folders = |root: &Path, out: &mut Vec<(PathBuf, String)>| {
            for dir in entries(root) {
                let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
                if !dir.is_dir() || name.starts_with("map-") || name.starts_with('.') {
                    continue;
                }
                out.extend(
                    entries(&dir)
                        .into_iter()
                        .filter(|p| bls(p))
                        .map(|p| (p, name.clone())),
                );
            }
        };
        let mut out: Vec<_> = entries(&self.saves)
            .into_iter()
            .filter(|p| bls(p))
            .map(|p| (p, LOOSE_FOLDER.to_string()))
            .collect();
        map_folders(&self.saves, &mut out);
        for install in &self.old_installs {
            map_folders(install, &mut out);
        }
        out.truncate(MAX_SOURCES);
        out
    }
    fn sync(&self) -> Result<()> {
        let converter = self
            .converter
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .context("No content to convert saves against")?;
        std::fs::create_dir_all(&self.cache)?;
        {
            let mut index = self.index.lock().unwrap_or_else(|e| e.into_inner());
            if index.is_none() {
                let saved: Index = std::fs::read(self.index_path())
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .filter(|i: &Index| i.schema_version == 1)
                    .unwrap_or_default();
                *index = Some(saved.saves.into_iter().map(|c| (c.source.clone(), c)).collect());
                self.changed.store(true, Ordering::SeqCst);
            }
        }
        let sources = self.sources();
        for (path, folder) in &sources {
            let Ok(meta) = std::fs::metadata(path) else {
                continue;
            };
            let modified_ns = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            let current = self
                .index
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .and_then(|i| i.get(path).cloned());
            if current.is_some_and(|c| {
                c.length == meta.len()
                    && c.modified_ns == modified_ns
                    && c.fingerprint == converter.fingerprint
                    && c.folder == *folder
                    && c.file.as_ref().is_none_or(|f| self.cache.join(f).is_file())
            }) {
                continue;
            }
            let record = self.convert(&converter, path, folder, meta.len(), modified_ns);
            if let Some(index) = self.index.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                index.insert(path.clone(), record);
            }
            // Load Bricks shows each save as soon as it is ready.
            self.changed.store(true, Ordering::SeqCst);
        }
        let saves = {
            let mut index = self.index.lock().unwrap_or_else(|e| e.into_inner());
            let index = index.get_or_insert_default();
            let before = index.len();
            index.retain(|path, _| sources.iter().any(|(p, _)| p == path));
            if index.len() != before {
                self.changed.store(true, Ordering::SeqCst);
            }
            index.values().cloned().collect::<Vec<_>>()
        };
        // Native copies nothing refers to any more are ours to remove.
        for entry in std::fs::read_dir(&self.cache)?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".world.json") && !saves.iter().any(|c| c.file.as_ref() == Some(&name)) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        let index = Index {
            schema_version: 1,
            saves,
        };
        bri_files::replace(&self.index_path(), &serde_json::to_vec_pretty(&index)?)?;
        Ok(())
    }
    fn convert(
        &self,
        converter: &Converter,
        path: &Path,
        folder: &str,
        length: u64,
        modified_ns: u128,
    ) -> Converted {
        let map_id = crate::content::map_for_save_folder(folder)
            .map(str::to_string)
            .unwrap_or_else(|| format!("v20/saves/{}", folder.to_ascii_lowercase()));
        let mut record = Converted {
            source: path.into(),
            length,
            modified_ns,
            fingerprint: converter.fingerprint.clone(),
            folder: folder.into(),
            file: None,
            map_id: map_id.clone(),
            description: vec![],
            bricks: 0,
        };
        let result = (|| -> Result<(String, World)> {
            ensure!(length <= MAX_SOURCE_BYTES, "file is too large");
            let mut bytes = vec![];
            std::fs::File::open(path)?
                .take(MAX_SOURCE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            let name = path
                .file_stem()
                .context("no file name")?
                .to_string_lossy()
                .into_owned();
            ensure!(
                crate::saves::v20_save_name(&name).is_some(),
                "its name can't be used for a save"
            );
            let world = converter.convert(&bytes, &name, &map_id)?;
            let file = format!(
                "{:x}-{}.world.json",
                Sha256::digest(&bytes),
                &converter.fingerprint[..16]
            );
            Ok((file, world))
        })();
        match result.and_then(|(file, world)| {
            // Packed and compressed: a big save reads back in a fraction
            // of the time its JSON would take.
            let build = bri_world::build::SavedBuild::new(world.clone());
            bri_files::replace(&self.cache.join(&file), &bri_world::build::encode(&build)?)?;
            Ok((file, world))
        }) {
            Ok((file, world)) => {
                record.file = Some(file);
                record.description = world.description;
                record.bricks = world.bricks.len() as u32;
            }
            Err(error) => bri_console::warn(format!(
                "Skipped old save {}: {error:#}",
                path.display()
            )),
        }
        record
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bls(description: &str, bricks: &[&str]) -> Vec<u8> {
        let mut text = format!(
            "This is a Blockland save file.\n1\n{description}\n{}Linecount {}\n",
            "1 0.5 0 1\n".repeat(64),
            bricks.len()
        );
        for (i, brick) in bricks.iter().enumerate() {
            text += &format!("{brick}\" {i} 0 0.1 0 1 2  0 0 1 1 1\n+-OWNER 9999\n");
        }
        text.into_bytes()
    }
    fn catalog() -> Catalog {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "bricks": [],
        }))
        .unwrap()
    }
    /// Every file under `dir` with its bytes.
    fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut out = BTreeMap::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                if e.path().is_dir() {
                    stack.push(e.path());
                } else {
                    out.insert(e.path(), std::fs::read(e.path()).unwrap());
                }
            }
        }
        out
    }
    struct Fixture {
        _dir: tempfile::TempDir,
        saves: PathBuf,
        old: PathBuf,
        cache: PathBuf,
    }
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let saves = dir.path().join("saves");
        let old = dir.path().join("Blockland/saves");
        let cache = dir.path().join("converted-saves");
        for (path, bytes) in [
            (
                saves.join("Slate/House.bls"),
                bls("A house", &["2x2 Brick", "1x1 Plate"]),
            ),
            (saves.join("Loose.bls"), bls("Dropped in", &["2x2 Brick"])),
            (saves.join("Bedroom/Broken.bls"), b"not a save".to_vec()),
            (saves.join("Moon Base/Crater.bls"), bls("", &["4x4 Brick"])),
            // The game's own save folders are not v20 map folders.
            (saves.join("map-0123/Ignored.bls"), bls("", &["2x2 Brick"])),
            (saves.join("Slate/Notes.txt"), b"not a save".to_vec()),
            (old.join("Slate/House.bls"), bls("Older house", &["2x2 Brick"])),
            (old.join("Kitchen/Table.bls"), bls("A table", &["1x1 Plate"])),
            // v20 only read saves inside a map folder.
            (old.join("Stray.bls"), bls("", &["1x1 Plate"])),
        ] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        Fixture {
            _dir: dir,
            saves,
            old,
            cache,
        }
    }
    fn names(o: &OldSaves) -> Vec<(String, String, bool, u32)> {
        let mut v: Vec<_> = o
            .list()
            .into_iter()
            .map(|l| (l.folder, l.name, l.old_install, l.bricks))
            .collect();
        v.sort();
        v
    }
    fn native_copies(cache: &Path) -> usize {
        std::fs::read_dir(cache)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".world.json"))
            .count()
    }

    #[test]
    fn saves_in_the_folder_and_an_old_install_convert_once_without_touching_originals() {
        let f = fixture();
        let before = (snapshot(&f.saves), snapshot(&f.old));
        let o = OldSaves::new(f.saves.clone(), f.cache.clone(), vec![f.old.clone()]);
        o.set_converter(Converter::bricks_only(catalog(), "a"));
        o.sync().unwrap();
        assert!(o.take_changed());
        let s = |x: &str| x.to_string();
        assert_eq!(
            names(&o),
            vec![
                (s("Kitchen"), s("Table"), true, 1),
                (s("Moon Base"), s("Crater"), false, 1),
                (s("Other"), s("Loose"), false, 1),
                (s("Slate"), s("House"), false, 2),
                (s("Slate"), s("House"), true, 1),
            ]
        );
        let house = o
            .list()
            .into_iter()
            .find(|l| l.name == "House" && !l.old_install)
            .unwrap();
        assert_eq!(house.map_id, "v20/add-ons/map_slate/slate.mis");
        assert_eq!(house.description, vec!["A house"]);
        assert!(house.path.starts_with(&f.cache));
        let world = bri_world::build::decode(&std::fs::read(&house.path).unwrap())
            .unwrap()
            .world;
        assert_eq!(world.bricks.len(), 2);
        assert_eq!(world.palette.len(), 64);
        // v20 ownership stays metadata, as for the stock saves.
        assert!(world.bricks.values().all(|b| b.owner == 0));
        assert_eq!(
            (snapshot(&f.saves), snapshot(&f.old)),
            before,
            "an original save was changed"
        );

        // Nothing new: nothing converts again.
        let cached = snapshot(&f.cache);
        o.sync().unwrap();
        assert!(!o.take_changed());
        assert_eq!(snapshot(&f.cache), cached);

        // A changed save converts again; a removed one leaves the list and
        // its native copy goes.
        std::fs::write(
            f.saves.join("Loose.bls"),
            bls("Edited", &["2x2 Brick", "2x2 Brick"]),
        )
        .unwrap();
        std::fs::remove_file(f.saves.join("Moon Base/Crater.bls")).unwrap();
        o.sync().unwrap();
        assert!(o.take_changed());
        let listed = o.list();
        let loose = listed.iter().find(|l| l.name == "Loose").unwrap();
        assert_eq!(
            (loose.bricks, loose.description.clone()),
            (2, vec![s("Edited")])
        );
        assert!(!listed.iter().any(|l| l.name == "Crater"));
        assert_eq!(native_copies(&f.cache), 4);

        // Other content: everything converts against it.
        o.set_converter(Converter::bricks_only(catalog(), "b"));
        o.sync().unwrap();
        assert!(o.list().iter().all(|l| l
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("-000000000000000b.world.json")));
        assert_eq!(native_copies(&f.cache), 4);

        // A new session reads the index instead of converting again.
        let cached = snapshot(&f.cache);
        let again = OldSaves::new(f.saves.clone(), f.cache.clone(), vec![f.old.clone()]);
        again.set_converter(Converter::bricks_only(catalog(), "b"));
        again.sync().unwrap();
        assert_eq!(names(&again), names(&o));
        assert_eq!(snapshot(&f.cache), cached);
    }

    #[test]
    fn background_conversion_fills_load_bricks_and_game_saves_take_precedence() -> Result<()> {
        let f = fixture();
        let o = OldSaves::new(f.saves.clone(), f.cache.clone(), vec![f.old.clone()]);
        o.set_converter(Converter::bricks_only(catalog(), "a"));
        o.start();
        while o.busy() {
            std::thread::yield_now();
        }
        let store = crate::saves::Store::for_tests(
            f.saves.clone(),
            [("v20/add-ons/map_slate/slate.mis".into(), "Slate".into())].into(),
            Some(o.clone()),
        );
        let listed: Vec<_> = store
            .list()?
            .into_iter()
            .map(|e| (e.info.map, e.info.name, e.info.description))
            .collect();
        let row = |m: &str, n: &str, d: &str| (m.to_string(), n.to_string(), d.to_string());
        for expected in [
            row("Slate", "House.world.json", "A house"),
            row("Kitchen", "Table.world.json", "A table"),
            row("Other", "Loose.world.json", "Dropped in"),
            row("Moon Base", "Crater.world.json", ""),
        ] {
            assert!(
                listed.contains(&expected),
                "{expected:?} missing from {listed:?}"
            );
        }
        assert_eq!(listed.len(), 4, "{listed:?}");
        let build = store.load("Slate", "House.world.json")?;
        assert_eq!(build.world.bricks.len(), 2);
        // A save of the same name made in this game is listed instead, and
        // the original stays as it was.
        let mut world = build.world.clone();
        world.bricks.remove(&2);
        store.save(
            "House.world.json",
            "Rebuilt",
            bri_world::build::SavedBuild::capture(&world, true, true)?,
            true,
        )?;
        assert_eq!(
            store.load("Slate", "House.world.json")?.world.bricks.len(),
            1
        );
        assert_eq!(
            std::fs::read(f.saves.join("Slate/House.bls"))?,
            bls("A house", &["2x2 Brick", "1x1 Plate"])
        );
        Ok(())
    }
}
