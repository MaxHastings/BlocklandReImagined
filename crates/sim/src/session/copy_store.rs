//! Copies kept by name on the host: a duplicator's `/saveDup` and
//! `/loadDup`. The engine owns the mechanism (which copy, how many bricks of
//! it, its colours on this world's palette); where the copies live is the
//! host's ([`CopyStore`]): files beside its saves, v20 duplication files
//! among them. Reading and writing them never holds up a tick: a request
//! goes to the store, and its answer is taken at the start of a later tick
//! and reported to the Add-On's `on_copy` like any other copy.
use super::*;
use crate::blueprint::{Blueprint, MAX_BLUEPRINT_BRICKS, SavedCopy};
use std::sync::{Arc, Mutex};

/// Requests waiting on the store at once, across all players.
const MAX_REQUESTS: usize = 64;

/// A copy as the store found it.
pub enum LoadedCopy {
    /// One this engine saved.
    Saved(SavedCopy),
    /// Bricks in a frame of their own, as a v20 duplication file holds
    /// them, with the colours its palette indices meant.
    Loose {
        bricks: Vec<Brick>,
        palette: Vec<[f32; 4]>,
    },
}

/// What a save did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Saved {
    Written,
    /// A copy was saved under that name before, and the save was asked
    /// not to replace it.
    Exists,
}

/// The store's answer to one request.
pub enum StoreDone {
    Saved(Result<Saved>),
    /// `None` when there is no copy by that name.
    Loaded(Result<Option<LoadedCopy>>),
    /// The names copies are kept under, sorted without regard to case.
    Listed(Result<Vec<String>>),
}

/// Where a host keeps saved copies. Each call only starts the work; the
/// answers come back from [`CopyStore::poll`], in any order. Names match
/// without regard to case.
pub trait CopyStore: Send + Sync {
    /// Keep `copy` as `name`; one kept as `name` before is replaced only
    /// with `overwrite`.
    fn save(&self, request: u64, name: &str, copy: SavedCopy, overwrite: bool);
    fn load(&self, request: u64, name: &str);
    /// The names of the copies it keeps (v20 duplication files among
    /// them) containing `filter`, any case; all of them when it is empty.
    fn list(&self, request: u64, filter: &str);
    fn poll(&self) -> Vec<(u64, StoreDone)>;
}

/// Whether `name` holds `filter`, without regard to case.
pub fn name_matches(name: &str, filter: &str) -> bool {
    name.to_ascii_lowercase()
        .contains(&filter.to_ascii_lowercase())
}

/// A store in memory, answering at the next poll: for tests, and hosts
/// that keep nothing on disk.
#[derive(Default)]
pub struct MemoryCopies {
    /// By name in lower case: the name as saved, and the copy.
    copies: Mutex<BTreeMap<String, (String, LoadedCopySource)>>,
    done: Mutex<Vec<(u64, StoreDone)>>,
}
enum LoadedCopySource {
    Saved(SavedCopy),
    Loose(Vec<Brick>, Vec<[f32; 4]>),
}
impl MemoryCopies {
    /// Keep `bricks` under `name` as a v20 duplication file would.
    pub fn put_loose(&self, name: &str, bricks: Vec<Brick>, palette: Vec<[f32; 4]>) {
        lock(&self.copies).insert(
            name.to_ascii_lowercase(),
            (name.into(), LoadedCopySource::Loose(bricks, palette)),
        );
    }
    pub fn saved(&self, name: &str) -> Option<SavedCopy> {
        match lock(&self.copies).get(&name.to_ascii_lowercase()) {
            Some((_, LoadedCopySource::Saved(copy))) => Some(copy.clone()),
            _ => None,
        }
    }
}
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
impl CopyStore for MemoryCopies {
    fn save(&self, request: u64, name: &str, copy: SavedCopy, overwrite: bool) {
        let mut copies = lock(&self.copies);
        let key = name.to_ascii_lowercase();
        let saved = if !overwrite && copies.contains_key(&key) {
            Saved::Exists
        } else {
            // The same name in other case is the same copy, kept as first named.
            let shown = copies.get(&key).map_or_else(|| name.to_string(), |(n, _)| n.clone());
            copies.insert(key, (shown, LoadedCopySource::Saved(copy)));
            Saved::Written
        };
        lock(&self.done).push((request, StoreDone::Saved(Ok(saved))));
    }
    fn list(&self, request: u64, filter: &str) {
        let names = lock(&self.copies)
            .values()
            .map(|(name, _)| name)
            .filter(|name| name_matches(name, filter))
            .cloned()
            .collect();
        lock(&self.done).push((request, StoreDone::Listed(Ok(names))));
    }
    fn load(&self, request: u64, name: &str) {
        let found = lock(&self.copies)
            .get(&name.to_ascii_lowercase())
            .map(|(_, c)| match c {
                LoadedCopySource::Saved(copy) => LoadedCopy::Saved(copy.clone()),
                LoadedCopySource::Loose(bricks, palette) => LoadedCopy::Loose {
                    bricks: bricks.clone(),
                    palette: palette.clone(),
                },
            });
        lock(&self.done).push((request, StoreDone::Loaded(Ok(found))));
    }
    fn poll(&self) -> Vec<(u64, StoreDone)> {
        std::mem::take(&mut *lock(&self.done))
    }
}

enum Want {
    Save { bricks: usize },
    List,
    Load {
        limit: usize,
        tool: String,
        partial: bool,
        whole: bool,
    },
}
struct Request {
    owner: OwnerId,
    package: String,
    name: String,
    want: Want,
}

#[derive(Default)]
pub(super) struct SavedCopies {
    store: Option<Arc<dyn CopyStore>>,
    next: u64,
    waiting: BTreeMap<u64, Request>,
}

/// How a copy, save or load went, for the Add-On's `on_copy` or else the
/// player.
pub(super) struct CopyOutcome {
    /// `select`, `save`, `list`, `load`, `cut`, `paint`, `wrench`,
    /// `supercut`, `fill` or `plant_as`.
    pub action: &'static str,
    pub name: Option<String>,
    /// The names a list found.
    pub names: Vec<String>,
    /// Bricks now held (or saved; or changed, cut or filled in).
    pub bricks: usize,
    /// Bricks put in (a supercut's bricks over what stuck out of its box).
    pub placed: usize,
    /// Bricks there were to take: more than `bricks` when the limit cut
    /// the copy short or some could not be had.
    pub total: usize,
    pub limit_reached: bool,
    pub refused: usize,
    /// `trust`, `public`, `empty`, `invalid`, `missing`, `unavailable`,
    /// `busy`, `limit`, `failed` or (a cut) `refused`, and the engine's
    /// words for it.
    pub error: Option<(&'static str, String)>,
}
impl CopyOutcome {
    /// `action` refused: with no copy held (`held` false), for that.
    pub fn failed(action: &'static str, held: bool, error: anyhow::Error) -> Self {
        let error = if held {
            ("refused", format!("{error:#}"))
        } else {
            ("empty", "Copy a build first.".to_string())
        };
        Self::about(action, None, Some(error))
    }
    /// An `action` that changed no bricks: what it was about, or why not.
    pub fn about(
        action: &'static str,
        name: Option<String>,
        error: Option<(&'static str, String)>,
    ) -> Self {
        Self {
            action,
            names: Vec::new(),
            name,
            bricks: 0,
            placed: 0,
            total: 0,
            limit_reached: false,
            refused: 0,
            error,
        }
    }
}
impl From<blueprints::Copied> for CopyOutcome {
    fn from(copied: blueprints::Copied) -> Self {
        let bricks = copied.selection.bricks.len();
        Self {
            action: "select",
            names: Vec::new(),
            name: None,
            bricks,
            placed: 0,
            total: bricks + copied.selection.refused,
            limit_reached: copied.selection.limit_reached,
            refused: copied.selection.refused,
            error: copied.error,
        }
    }
}

impl Session {
    /// The game version this host runs, as its players see it.
    pub fn set_game_version(&mut self, version: impl Into<String>) {
        self.game_version = version.into();
    }

    /// Where this host keeps saved copies. Without one, saving and loading
    /// copies tells the player it is not available here.
    pub fn set_copy_store(&mut self, store: Arc<dyn CopyStore>) {
        self.saved_copies.store = Some(store);
    }

    fn copy_request(&mut self, request: Request) -> Option<(u64, Arc<dyn CopyStore>)> {
        let failed = |code, message: &str| CopyOutcome {
            names: Vec::new(),
            action: match request.want {
                Want::Save { .. } => "save",
                Want::Load { .. } => "load",
                Want::List => "list",
            },
            name: Some(request.name.clone()),
            bricks: 0,
            total: 0,
            placed: 0,
            limit_reached: false,
            refused: 0,
            error: Some((code, message.to_string())),
        };
        let Some(store) = self.saved_copies.store.clone() else {
            let outcome = failed("unavailable", "This server does not keep copies.");
            self.report_copy(&request.package, request.owner, outcome);
            return None;
        };
        if self.saved_copies.waiting.len() >= MAX_REQUESTS
            || self
                .saved_copies
                .waiting
                .values()
                .any(|r| r.owner == request.owner)
        {
            let outcome = failed("busy", "Your last copy is still being saved or loaded.");
            self.report_copy(&request.package, request.owner, outcome);
            return None;
        }
        self.saved_copies.next += 1;
        let id = self.saved_copies.next;
        self.saved_copies.waiting.insert(id, request);
        Some((id, store))
    }

    /// Keep the copy `owner` holds under `name`, replacing one kept so
    /// before only with `overwrite`.
    pub(super) fn save_copy(
        &mut self,
        owner: OwnerId,
        name: String,
        overwrite: bool,
        package: &str,
    ) {
        let Some(copy) = self.blueprints.get(&owner).cloned() else {
            let outcome = CopyOutcome {
                names: Vec::new(),
                action: "save",
                name: Some(name),
                bricks: 0,
                total: 0,
                placed: 0,
                limit_reached: false,
                refused: 0,
                error: Some(("empty", "Copy a build first.".into())),
            };
            self.report_copy(package, owner, outcome);
            return;
        };
        let saved = SavedCopy {
            schema_version: SavedCopy::SCHEMA_VERSION,
            saved_by: self
                .peers
                .get(&owner)
                .map(|p| p.name.clone())
                .unwrap_or_default(),
            palette: self.simulation.state().palette.clone(),
            copy,
        };
        let bricks = saved.copy.bricks.len();
        if let Some((id, store)) = self.copy_request(Request {
            owner,
            package: package.into(),
            name: name.clone(),
            want: Want::Save { bricks },
        }) {
            store.save(id, &name, saved, overwrite);
        }
    }

    /// Tell `owner`'s Add-On the names copies are kept under that contain
    /// `filter`.
    pub(super) fn list_copies(&mut self, owner: OwnerId, filter: String, package: &str) {
        if let Some((id, store)) = self.copy_request(Request {
            owner,
            package: package.into(),
            name: filter.clone(),
            want: Want::List,
        }) {
            store.list(id, &filter);
        }
    }

    /// Give `owner` the copy saved as `name`, once the store has it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn load_copy(
        &mut self,
        owner: OwnerId,
        name: String,
        limit: usize,
        tool: String,
        partial: bool,
        whole: bool,
        package: &str,
    ) {
        if let Some((id, store)) = self.copy_request(Request {
            owner,
            package: package.into(),
            name: name.clone(),
            want: Want::Load {
                limit,
                tool,
                partial,
                whole,
            },
        }) {
            store.load(id, &name);
        }
    }

    /// Answers the store has ready, each reported to its Add-On.
    pub(super) fn step_saved_copies(&mut self) {
        let Some(store) = self.saved_copies.store.clone() else {
            return;
        };
        for (id, done) in store.poll() {
            let Some(request) = self.saved_copies.waiting.remove(&id) else {
                continue;
            };
            let outcome = match (request.want, done) {
                (Want::Save { bricks }, StoreDone::Saved(result)) => CopyOutcome {
                    names: Vec::new(),
                    action: "save",
                    name: Some(request.name.clone()),
                    bricks: if matches!(result, Ok(Saved::Written)) { bricks } else { 0 },
                    total: bricks,
                    placed: 0,
                    limit_reached: false,
                    refused: 0,
                    error: match result {
                        Ok(Saved::Written) => None,
                        Ok(Saved::Exists) => Some((
                            "exists",
                            format!("A copy is already saved as '{}'.", request.name),
                        )),
                        Err(e) => Some(("failed", format!("Could not save the copy: {e:#}"))),
                    },
                },
                (Want::List, StoreDone::Listed(result)) => {
                    let (names, error) = match result {
                        Ok(names) => (names, None),
                        Err(e) => (
                            Vec::new(),
                            Some(("failed", format!("Could not list the saved copies: {e:#}"))),
                        ),
                    };
                    CopyOutcome {
                        action: "list",
                        name: Some(request.name.clone()),
                        bricks: names.len(),
                        names,
                        total: 0,
                        placed: 0,
                        limit_reached: false,
                        refused: 0,
                        error,
                    }
                }
                (
                    Want::Load {
                        limit,
                        tool,
                        partial,
                        whole,
                    },
                    StoreDone::Loaded(found),
                ) => self.hold_loaded(
                    request.owner,
                    &request.name,
                    found,
                    (limit, whole),
                    &tool,
                    partial,
                    &request.package,
                ),
                _ => continue,
            };
            self.report_copy(&request.package, request.owner, outcome);
        }
    }

    /// The loaded copy, at most `limit` bricks of it on this world's
    /// palette, held by `owner`.
    #[allow(clippy::too_many_arguments)]
    fn hold_loaded(
        &mut self,
        owner: OwnerId,
        name: &str,
        found: Result<Option<LoadedCopy>>,
        (limit, whole): (usize, bool),
        tool: &str,
        partial: bool,
        package: &str,
    ) -> CopyOutcome {
        let mut outcome = CopyOutcome {
            names: Vec::new(),
            action: "load",
            name: Some(name.into()),
            bricks: 0,
            total: 0,
            placed: 0,
            limit_reached: false,
            refused: 0,
            error: None,
        };
        let (mut bricks, palette, loose) = match found {
            Ok(Some(LoadedCopy::Saved(saved))) => (saved.copy.bricks, saved.palette, false),
            Ok(Some(LoadedCopy::Loose { bricks, palette })) => (bricks, palette, true),
            Ok(None) => {
                outcome.error = Some(("missing", format!("There is no copy saved as '{name}'.")));
                return outcome;
            }
            Err(error) => {
                outcome.error = Some(("failed", format!("Could not load '{name}': {error:#}")));
                return outcome;
            }
        };
        outcome.total = bricks.len();
        let limit = limit.min(MAX_BLUEPRINT_BRICKS);
        if whole && bricks.len() > limit {
            outcome.limit_reached = true;
            outcome.error = Some((
                "limit",
                format!(
                    "'{name}' has {} bricks, more than the {limit} you may copy.",
                    bricks.len()
                ),
            ));
            return outcome;
        }
        if bricks.len() > limit {
            bricks.truncate(limit);
            outcome.limit_reached = true;
        }
        // Palette indices mean what they meant where the copy was made.
        let world = &self.simulation.state().palette;
        if *world != palette {
            let mut nearest = BTreeMap::new();
            for brick in &mut bricks {
                let color = brick.color;
                brick.color = *nearest.entry(color).or_insert_with(|| {
                    palette
                        .get(usize::from(color))
                        .map_or(color, |rgba| self.closest_paint(*rgba))
                });
            }
        }
        let made = if loose {
            Blueprint::from_loose(tool, &bricks, &self.simulation.definitions)
        } else {
            let unknown = bricks
                .iter()
                .filter(|b| self.simulation.definitions.get(b).is_err())
                .count();
            bricks.retain(|b| self.simulation.definitions.get(b).is_ok());
            Blueprint::capture(tool, &bricks, &self.simulation.definitions).map(|b| (b, unknown))
        };
        match made {
            Ok((blueprint, left_out)) => {
                outcome.bricks = blueprint.bricks.len();
                outcome.refused = left_out;
                let held = blueprints::HeldCopy::new(vec![], package, partial);
                self.hold_blueprint(owner, blueprint, held);
            }
            Err(error) => {
                outcome.error = Some(("invalid", format!("Could not load '{name}': {error:#}")));
            }
        }
        outcome
    }

    /// A player who leaves has nothing left to load.
    pub(super) fn forget_copy_requests(&mut self, owner: OwnerId) {
        self.saved_copies.waiting.retain(|_, r| r.owner != owner);
    }
}
