//! Local user saves. Original converted saves are read-only templates; a user
//! save of the same name shadows the template without modifying native content.
use anyhow::{Context, Result, ensure};
use bri_ui::api::{RequestId, SaveFileInfo, UiAction};
use bri_world::build::{MAX_BUILD_BYTES, SavedBuild};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Entry {
    pub info: SaveFileInfo,
    pub map_id: String,
    pub path: PathBuf,
    pub root: PathBuf,
}
#[derive(Clone)]
pub struct Store {
    directory: PathBuf,
    templates: Vec<Entry>,
    map_names: BTreeMap<String, String>,
}
/// What the save dialogs say about a save file that cannot be read.
const DAMAGED: &str =
    "This save is damaged and can't be loaded. Saving over it keeps a copy of the old file.";
/// How often a client-hosted game autosaves, and how many autosaves each map
/// keeps (the dedicated server's `bri-server` uses the same numbers).
pub const AUTOSAVE_EVERY: std::time::Duration = std::time::Duration::from_secs(60);
pub const AUTOSAVE_KEEP: usize = 3;
pub fn valid_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".world.json") else {
        return false;
    };
    let reserved = stem.split('.').next().unwrap_or("").to_ascii_uppercase();
    !stem.trim().is_empty()
        && name.len() < 255
        && name.trim() == name
        && !stem.ends_with(['.', ' '])
        && !name
            .chars()
            .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
        && ![
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]
        .contains(&reserved.as_str())
}
fn modified_date(seconds: u64) -> String {
    // Gregorian calendar in March-based 400-year eras. Fixed-width UTC text
    // keeps the existing UI's lexicographic date sort chronological.
    let days = seconds / 86400 + 719468;
    let era = days / 146097;
    let day_of_era = days % 146097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let march_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * march_month + 2) / 5 + 1;
    let month = if march_month < 10 {
        march_month + 3
    } else {
        march_month - 9
    };
    let year = year + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}Z",
        (seconds / 3600) % 24,
        (seconds / 60) % 60
    )
}
impl Store {
    pub fn new(state: &Path, content: &crate::content::ClientContent) -> Self {
        let map_names = content
            .maps
            .iter()
            .map(|m| (m.id.clone(), m.name.clone()))
            .collect();
        let mut store = Self {
            directory: state.join("saves"),
            templates: vec![],
            map_names,
        };
        for world in &content.worlds {
            let name = format!("{}.world.json", world.name);
            if !valid_name(&name) {
                continue;
            }
            store.templates.push(Entry {
                info: SaveFileInfo {
                    name,
                    map: store.map_name(&world.map_id),
                    modified: "Converted original".into(),
                    description: "Original converted build".into(),
                    brick_count: Some(world.brick_count as u32),
                    damaged: false,
                },
                map_id: world.map_id.clone(),
                path: content.paths.worlds.join(&world.file),
                root: content.paths.worlds.clone(),
            });
        }
        store
    }
    pub fn map_name(&self, id: &str) -> String {
        self.map_names.get(id).cloned().unwrap_or_else(|| {
            let stem = id.rsplit('/').next().unwrap_or(id).trim_end_matches(".mis");
            let mut chars = stem.chars();
            chars.next().map_or_else(
                || "Unknown map".into(),
                |first| first.to_uppercase().collect::<String>() + chars.as_str(),
            )
        })
    }
    fn root(&self) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.directory)?;
        Ok(self.directory.canonicalize()?)
    }
    pub fn read(entry: &Entry) -> Result<SavedBuild> {
        let root = entry.root.canonicalize()?;
        let path = entry.path.canonicalize()?;
        ensure!(
            path.starts_with(&root) && path.is_file(),
            "Save path escapes its storage directory"
        );
        let mut bytes = vec![];
        File::open(path)?
            .take(MAX_BUILD_BYTES + 1)
            .read_to_end(&mut bytes)?;
        bri_world::build::decode(&bytes)
    }
    pub fn list(&self) -> Result<Vec<Entry>> {
        let root = self.root()?;
        let mut files: BTreeMap<(String, String), Entry> = self
            .templates
            .iter()
            .cloned()
            .map(|e| {
                (
                    (
                        e.info.map.to_ascii_lowercase(),
                        e.info.name.to_ascii_lowercase(),
                    ),
                    e,
                )
            })
            .collect();
        let mut count = 0;
        let mut bytes = 0_u64;
        for directory in std::fs::read_dir(&root)? {
            let directory = directory?;
            if !directory.file_type()?.is_dir()
                || !directory.file_name().to_string_lossy().starts_with("map-")
            {
                continue;
            }
            let folder = directory.file_name().to_string_lossy().into_owned();
            let directory = directory.path().canonicalize()?;
            ensure!(
                directory.starts_with(&root),
                "Save directory escapes storage"
            );
            for entry in std::fs::read_dir(directory)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if !valid_name(&name) || !entry.file_type()?.is_file() {
                    continue;
                }
                let meta = entry.metadata()?;
                count += 1;
                bytes = bytes.saturating_add(meta.len());
                ensure!(
                    count <= 1000 && bytes <= 512 * 1024 * 1024,
                    "Save listing exceeds scan budget"
                );
                let mut record = Entry {
                    info: SaveFileInfo {
                        name,
                        map: String::new(),
                        modified: modified_date(
                            meta.modified()?
                                .duration_since(std::time::UNIX_EPOCH)?
                                .as_secs(),
                        ),
                        description: String::new(),
                        brick_count: None,
                        damaged: false,
                    },
                    map_id: String::new(),
                    path: entry.path(),
                    root: root.clone(),
                };
                match Self::read(&record) {
                    Ok(saved) => {
                        record.map_id = saved.world.map_id;
                        record.info.map = self.map_name(&record.map_id);
                        record.info.description = saved.world.description.join("\n");
                        record.info.brick_count = Some(saved.world.bricks.len() as u32);
                    }
                    // One bad file must not hide every other save: list it as
                    // damaged under the map its folder names.
                    Err(error) => {
                        bri_console::warn(format!(
                            "Save {} is damaged: {error:#}",
                            record.path.display()
                        ));
                        record.map_id = self.map_for_folder(&folder).unwrap_or_default();
                        record.info.map = if record.map_id.is_empty() {
                            "Unknown map".into()
                        } else {
                            self.map_name(&record.map_id)
                        };
                        record.info.description = DAMAGED.into();
                        record.info.damaged = true;
                    }
                }
                files.insert(
                    (
                        record.info.map.to_ascii_lowercase(),
                        record.info.name.to_ascii_lowercase(),
                    ),
                    record,
                );
            }
        }
        Ok(files.into_values().collect())
    }
    /// Save `world` as the newest autosave of its map, keeping the last
    /// [`AUTOSAVE_KEEP`]. It is an ordinary build in the map's save folder, so
    /// the Load dialog lists it.
    pub fn autosave(&self, world: &bri_world::World) -> Result<PathBuf> {
        let mut build = SavedBuild::capture(world, true, true)?;
        build.world.name = "Autosave".into();
        build.world.description = vec!["Saved automatically while you played.".into()];
        let bytes = bri_world::build::encode(&build)?;
        let root = self.root()?;
        let map = format!("map-{:x}", Sha256::digest(world.map_id.as_bytes()));
        std::fs::create_dir_all(root.join(&map))?;
        let directory = root.join(map).canonicalize()?;
        ensure!(
            directory.starts_with(&root),
            "Save directory escapes storage"
        );
        bri_world::persistence::autosave_bytes(&directory, &bytes, AUTOSAVE_KEEP)
    }
    /// The host's autosave hook: writes only when the world changed since the
    /// last autosave (or since `start`, the world the host began with), so an
    /// idle game never pushes older autosaves out.
    pub fn autosaver(&self, start: &bri_world::World) -> bri_net::server::SaveWorld {
        let store = self.clone();
        let last = std::sync::Mutex::new((start.map_id.clone(), start.revision));
        std::sync::Arc::new(move |world: &bri_world::World| {
            let key = (world.map_id.clone(), world.revision);
            {
                let last = last.lock().unwrap_or_else(|e| e.into_inner());
                // Unchanged, or a fresh map nobody has built on yet.
                if *last == key || (last.0 != key.0 && world.bricks.is_empty()) {
                    return Ok(());
                }
            }
            store.autosave(world)?;
            *last.lock().unwrap_or_else(|e| e.into_inner()) = key;
            Ok(())
        })
    }
    pub fn load(&self, map: &str, name: &str) -> Result<SavedBuild> {
        ensure!(valid_name(name), "Invalid native save filename");
        let entry = self
            .list()?
            .into_iter()
            .find(|e| e.info.map == map && e.info.name == name)
            .context("Selected save no longer exists")?;
        ensure!(!entry.info.damaged, "{DAMAGED}");
        Self::read(&entry)
    }
    /// The map id whose save folder (`map-<sha256 of the id>`) is `folder`.
    fn map_for_folder(&self, folder: &str) -> Option<String> {
        self.map_names
            .keys()
            .find(|id| folder == format!("map-{:x}", Sha256::digest(id.as_bytes())))
            .cloned()
    }
    pub fn save(
        &self,
        name: &str,
        description: &str,
        mut build: SavedBuild,
        overwrite: bool,
    ) -> Result<()> {
        ensure!(valid_name(name), "Invalid native save filename");
        ensure!(
            description.len() <= 64 * 1024,
            "Save description is too long"
        );
        build.world.name = name.trim_end_matches(".world.json").into();
        build.world.description = description.lines().map(str::to_string).collect();
        build.validate()?;
        let bytes = bri_world::build::encode(&build)?;
        ensure!(
            overwrite
                || !self
                    .templates
                    .iter()
                    .any(|e| e.map_id == build.world.map_id
                        && e.info.name.eq_ignore_ascii_case(name)),
            "An original save has this name; confirm overwrite to create a local copy"
        );
        let root = self.root()?;
        let map = format!("map-{:x}", Sha256::digest(build.world.map_id.as_bytes()));
        std::fs::create_dir_all(root.join(&map))?;
        let directory = root.join(map).canonicalize()?;
        ensure!(
            directory.starts_with(&root),
            "Save directory escapes storage"
        );
        // Match the UI's case-insensitive names on every supported filesystem.
        let matching = std::fs::read_dir(&directory)?
            .filter_map(|entry| match entry {
                Ok(entry)
                    if entry
                        .file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(name) =>
                {
                    Some(Ok(entry.path()))
                }
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<std::io::Result<Vec<_>>>()?;
        ensure!(
            matching.len() <= 1,
            "Ambiguous save filenames differing only by case"
        );
        let path = matching
            .into_iter()
            .next()
            .unwrap_or_else(|| directory.join(name));
        let exists = path.try_exists()?;
        if exists {
            ensure!(overwrite, "Save already exists; confirm overwrite");
            ensure!(
                path.canonicalize()?.starts_with(&root)
                    && std::fs::symlink_metadata(&path)?.is_file(),
                "Unsafe existing save"
            );
        }
        if exists {
            // Keep the overwritten save in history before replacing it.
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos();
            let history = directory.join(".history");
            std::fs::create_dir_all(&history)?;
            ensure!(
                history.canonicalize()?.starts_with(&root),
                "Save history escapes storage"
            );
            std::fs::hard_link(
                &path,
                history.join(format!(
                    "{stamp}-{:x}.world.json",
                    Sha256::digest(name.as_bytes())
                )),
            )?;
            bri_files::replace(&path, &bytes)?;
        } else {
            bri_files::create_new(&path, &bytes)?;
        }
        Ok(())
    }
}

pub struct Request {
    pub id: RequestId,
    pub session: Option<RequestId>,
    pub action: UiAction,
    pub build: Option<Box<SavedBuild>>,
}
pub enum Outcome {
    Listed(Vec<Entry>),
    Loaded(Box<SavedBuild>),
}
type Completed = (Request, std::result::Result<Outcome, String>);
#[derive(Default)]
pub struct Jobs {
    pending: std::collections::VecDeque<Request>,
    running: Option<std::sync::mpsc::Receiver<Completed>>,
}
impl Jobs {
    pub fn len(&self) -> usize {
        self.pending.len() + usize::from(self.running.is_some())
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn enqueue(&mut self, request: Request) -> Result<()> {
        ensure!(self.len() < 8, "Too many pending file operations");
        self.pending.push_back(request);
        Ok(())
    }
    pub fn poll(&mut self, store: &Store, runtime: &tokio::runtime::Runtime) -> Option<Completed> {
        let completed = self.running.as_ref().and_then(|r| r.try_recv().ok());
        if completed.is_some() {
            self.running = None;
        }
        if self.running.is_none()
            && let Some(mut request) = self.pending.pop_front()
        {
            let store = store.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            self.running = Some(rx);
            runtime.spawn_blocking(move || {
                // Convert a worker panic into a completed request, never an
                // eternally pending UI operation that blocks later saves.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                    || -> Result<Outcome> {
                        match &request.action {
                            UiAction::RequestSaveList { .. } => Ok(Outcome::Listed(store.list()?)),
                            UiAction::LoadBricks { map, name, .. } => {
                                Ok(Outcome::Loaded(Box::new(store.load(map, name)?)))
                            }
                            UiAction::SaveBricks {
                                name,
                                description,
                                overwrite,
                                ..
                            } => {
                                store.save(
                                    name,
                                    description,
                                    *request
                                        .build
                                        .take()
                                        .context("Missing authoritative save snapshot")?,
                                    *overwrite,
                                )?;
                                Ok(Outcome::Listed(store.list()?))
                            }
                            _ => anyhow::bail!("Unsupported file operation"),
                        }
                    },
                ))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("File worker failed")))
                .map_err(|e| format!("{e:#}"));
                let _ = tx.send((request, result));
            });
        }
        completed
    }
}

/// `LoadBricks_GetColorDifference`: `None` when every colour the save's
/// bricks use is already in the world's set, otherwise whether the save's
/// new colours fit added on (the world holds 256).
pub fn color_difference(world: &[[f32; 4]], build: &SavedBuild) -> Option<bool> {
    let saved = &build.world.palette;
    let used: std::collections::BTreeSet<u8> = build
        .world
        .bricks
        .values()
        .chain(&build.world.unloaded)
        .map(|b| b.color)
        .collect();
    let missing = |c: &&[f32; 4]| !world.contains(c);
    if !used
        .iter()
        .filter_map(|&i| saved.get(usize::from(i)))
        .any(|c| missing(&c))
    {
        return None;
    }
    let mut new: Vec<&[f32; 4]> = saved.iter().filter(missing).collect();
    new.dedup_by(|a, b| a == b);
    Some(world.len() + new.len() <= 256)
}

/// `ColorWarning_ClickMatch` (colour method 3): each of the save's colours
/// becomes the world's nearest, by v20's summed RGB difference, alpha
/// counting half and a solid/translucent mismatch never matching first.
pub fn match_colors(world: &[[f32; 4]], build: &mut SavedBuild) {
    for color in &mut build.world.palette {
        let diff = |c: &[f32; 4]| {
            let rgb: f32 = (0..3).map(|i| (c[i].abs() - color[i].abs()).abs()).sum();
            let alpha = if (c[3] > 0.99) != (color[3] > 0.99) {
                1000.0
            } else {
                (c[3].abs() - color[3].abs()).abs() * 0.5
            };
            rgb + alpha
        };
        if let Some(nearest) = world.iter().min_by(|a, b| diff(a).total_cmp(&diff(b))) {
            *color = *nearest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_saves_colours_load_as_v20s_color_warning_offers() {
        let world = vec![
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0, 0.5],
        ];
        let mut saved = bri_world::World::new(
            "Save".into(),
            "map".into(),
            vec![
                [0.9, 0.1, 0.0, 1.0],
                [0.0, 0.9, 0.1, 1.0],
                [1.0, 0.0, 0.0, 1.0],
            ],
        );
        let mut brick =
            bri_world::Brick::new(bri_world::ContentRef::Resolved("plate".into()), [0.0; 3], 1);
        brick.color = 2;
        saved.bricks.insert(1, brick.clone());
        let mut build = SavedBuild::new(saved);
        // Only the world's own red is used: nothing to ask.
        assert_eq!(color_difference(&world, &build), None);
        brick.color = 0;
        build.world.bricks.insert(1, brick);
        assert_eq!(color_difference(&world, &build), Some(true));
        assert_eq!(color_difference(&[[0.5; 4]; 255], &build), Some(false));
        match_colors(&world, &mut build);
        // Near-red becomes red; solid green takes solid blue, not translucent green.
        assert_eq!(build.world.palette[0], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(build.world.palette[1], [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(color_difference(&world, &build), None);
    }
    #[test]
    fn utc_save_dates_are_sortable_across_leap_year_boundaries() {
        assert_eq!(modified_date(0), "1970-01-01 00:00Z");
        assert_eq!(modified_date(951782400), "2000-02-29 00:00Z");
        assert_eq!(modified_date(951868800), "2000-03-01 00:00Z");
        assert_eq!(modified_date(1709251199), "2024-02-29 23:59Z");
        assert!(modified_date(1709251199) < modified_date(1709251200));
    }
    #[test]
    fn local_saves_preserve_templates_refuse_clobber_and_archive_overwrites() -> Result<()> {
        let directory = std::env::temp_dir().join(format!(
            "bri-save-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir_all(directory.join("original"))?;
        let mut world = bri_world::World::new("Original".into(), "map".into(), vec![[1.0; 4]]);
        world.bricks.insert(
            1,
            bri_world::Brick::new(bri_world::ContentRef::Resolved("plate".into()), [0.0; 3], 3),
        );
        world.next_brick_id = 2;
        let original = serde_json::to_vec(&world)?;
        let path = directory.join("original/source.json");
        std::fs::write(&path, &original)?;
        let store = Store {
            directory: directory.join("user"),
            map_names: [("map".into(), "Map".into())].into(),
            templates: vec![Entry {
                info: SaveFileInfo {
                    name: "Original.world.json".into(),
                    map: "Map".into(),
                    modified: "Original".into(),
                    description: String::new(),
                    brick_count: Some(1),
                    damaged: false,
                },
                map_id: "map".into(),
                path: path.clone(),
                root: directory.join("original"),
            }],
        };
        assert_eq!(store.load("Map", "Original.world.json")?.world, world);
        let build = SavedBuild::capture(&world, true, true)?;
        assert!(
            store
                .save("Original.world.json", "First", build.clone(), false)
                .is_err()
        );
        store.save("Original.world.json", "First", build.clone(), true)?;
        let entries = store.list()?;
        assert_eq!(entries.len(), 1);
        let saved_path = entries[0].path.clone();
        let first = std::fs::read(&saved_path)?;
        assert_eq!(
            store.load("Map", "Original.world.json")?.world.description,
            vec!["First"]
        );
        assert!(
            store
                .save("Original.world.json", "Wrong", build.clone(), false)
                .is_err()
        );
        assert_eq!(std::fs::read(&saved_path)?, first);
        store.save("Original.world.json", "Second", build.clone(), true)?;
        assert_eq!(
            store.load("Map", "Original.world.json")?.world.description,
            vec!["Second"]
        );
        assert_eq!(
            bri_world::persistence::load_startup(&saved_path)?,
            store.load("Map", "Original.world.json")?.world
        );
        assert!(
            store
                .save(
                    "original.world.json",
                    "Case collision",
                    build.clone(),
                    false
                )
                .is_err()
        );
        let revisions: Vec<_> = std::fs::read_dir(saved_path.parent().unwrap().join(".history"))?
            .collect::<std::io::Result<_>>()?;
        assert_eq!(revisions.len(), 1);
        assert_eq!(std::fs::read(revisions[0].path())?, first);
        for invalid in [
            "../escape.world.json",
            "CON.world.json",
            "C:\\escape.world.json",
            "bad/escape.world.json",
            ".world.json",
        ] {
            assert!(store.save(invalid, "", build.clone(), false).is_err());
        }
        assert_eq!(
            std::fs::read(path)?,
            original,
            "Original template was changed"
        );
        // Deliberately leave no recursive deletion against a computed directory.
        // These small fixtures stay in the OS temporary directory.
        Ok(())
    }
    #[test]
    fn a_damaged_save_is_listed_as_damaged_and_hides_nothing_else() -> Result<()> {
        let directory = std::env::temp_dir().join(format!(
            "bri-save-damaged-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let store = Store {
            directory: directory.clone(),
            map_names: [("map".into(), "Map".into())].into(),
            templates: Vec::new(),
        };
        let world = bri_world::World::new("Good".into(), "map".into(), vec![[1.0; 4]]);
        let build = SavedBuild::capture(&world, true, true)?;
        store.save("Good.world.json", "Fine", build.clone(), false)?;
        let good = store.list()?[0].path.clone();
        let broken = good.with_file_name("Broken.world.json");
        std::fs::write(&broken, b"{\"truncated")?;
        let entries = store.list()?;
        assert_eq!(entries.len(), 2);
        let damaged = entries
            .iter()
            .find(|e| e.info.name == "Broken.world.json")
            .unwrap();
        assert!(damaged.info.damaged);
        // The folder names the map even though the file does not.
        assert_eq!(damaged.info.map, "Map");
        assert_eq!(damaged.info.brick_count, None);
        assert!(store.load("Map", "Good.world.json").is_ok());
        let error = format!("{:#}", store.load("Map", "Broken.world.json").unwrap_err());
        assert!(error.contains("damaged and can't be loaded"), "{error}");
        // Saving over it asks first, then keeps the old file in history.
        assert!(
            store
                .save("Broken.world.json", "", build.clone(), false)
                .is_err()
        );
        store.save("Broken.world.json", "Fixed", build, true)?;
        assert!(store.list()?.iter().all(|e| !e.info.damaged));
        let history: Vec<_> = std::fs::read_dir(good.parent().unwrap().join(".history"))?
            .collect::<std::io::Result<_>>()?;
        assert_eq!(std::fs::read(history[0].path())?, b"{\"truncated");
        Ok(())
    }
    #[test]
    fn hosted_games_autosave_changes_into_the_load_list() -> Result<()> {
        let directory = std::env::temp_dir().join(format!(
            "bri-autosave-client-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let store = Store {
            directory,
            map_names: [("map".into(), "Map".into())].into(),
            templates: Vec::new(),
        };
        let mut world = bri_world::World::new("Live".into(), "map".into(), vec![[1.0; 4]]);
        let save = store.autosaver(&world);
        // Nothing changed since the host started: nothing is written.
        save(&world)?;
        assert!(store.list()?.is_empty());
        for n in 0..5 {
            world.bricks.insert(
                n + 1,
                bri_world::Brick::new(
                    bri_world::ContentRef::Resolved("plate".into()),
                    [n as f32, 0.0, 0.0],
                    3,
                ),
            );
            world.next_brick_id = n + 2;
            world.revision += 1;
            save(&world)?;
            // The same revision again (an idle interval) writes nothing.
            save(&world)?;
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let saves = store.list()?;
        assert_eq!(saves.len(), AUTOSAVE_KEEP);
        assert!(
            saves
                .iter()
                .all(|e| e.info.map == "Map" && bri_world::persistence::is_autosave(&e.info.name))
        );
        let newest = saves.iter().max_by_key(|e| e.info.name.clone()).unwrap();
        assert_eq!(newest.info.brick_count, Some(5));
        assert_eq!(store.load("Map", &newest.info.name)?.world.bricks.len(), 5);
        // A fresh map nobody built on is not autosaved.
        let empty = bri_world::World::new("Next".into(), "other".into(), vec![[1.0; 4]]);
        save(&empty)?;
        assert_eq!(store.list()?.len(), AUTOSAVE_KEEP);
        Ok(())
    }
}
