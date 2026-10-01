//! Where a host keeps the copies duplicators save by name: `Duplications`
//! in its saves folder, one file per copy. Loading also reads v20
//! duplication files a player dropped in that same folder, Plornt's
//! Duplorcator's and Zeblote's New Duplicator's alike, and never changes
//! them. The game never looks in a player's Blockland folders for them.
//! Each request runs on a thread of its own, so the game never waits on
//! the disk.
use crate::old_saves::OldSaves;
use anyhow::{Context, Result, ensure};
use bri_sim::blueprint::SavedCopy;
use bri_sim::session::{CopyStore, LoadedCopy, Saved, StoreDone, name_matches};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// The folder in a saves folder that copies are kept in.
pub const FOLDER: &str = "Duplications";
/// A saved copy's file ending.
const NATIVE: &str = "copy.json";
/// Largest copy file read: room for a million bricks
/// (`bri_sim::blueprint::MAX_BLUEPRINT_BRICKS`), about 50 bytes each.
const MAX_BYTES: u64 = 256 * 1024 * 1024;

pub struct CopyFiles {
    /// Copies saved here, and v20 files a player put beside them.
    own: PathBuf,
    old_saves: Arc<OldSaves>,
    done: Arc<Mutex<Vec<(u64, StoreDone)>>>,
}

impl CopyFiles {
    pub fn new(old_saves: Arc<OldSaves>) -> Self {
        Self {
            own: old_saves.saves_folder().join(FOLDER),
            old_saves,
            done: Default::default(),
        }
    }

    fn finish(&self, request: u64, done: StoreDone) {
        self.done
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((request, done));
    }
}

/// The file in `folder` named `name` + `.ending`, matched without regard
/// to case as Windows does.
fn find(folder: &Path, name: &str, ending: &str) -> Option<PathBuf> {
    let wanted = format!("{name}.{ending}");
    let exact = folder.join(&wanted);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(folder)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(&wanted))
        })
}

fn read(path: &Path) -> Result<Vec<u8>> {
    let length = std::fs::metadata(path)?.len();
    ensure!(length <= MAX_BYTES, "{} is too big", path.display());
    Ok(std::fs::read(path)?)
}

fn save(folder: &Path, name: &str, copy: &SavedCopy, overwrite: bool) -> Result<Saved> {
    // A v20 file by that name is a copy kept under it too.
    if !overwrite && (find(folder, name, NATIVE).is_some() || find(folder, name, "bls").is_some()) {
        return Ok(Saved::Exists);
    }
    std::fs::create_dir_all(folder)?;
    // The same name in other case is the same copy.
    let path = find(folder, name, NATIVE).unwrap_or_else(|| folder.join(format!("{name}.{NATIVE}")));
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, serde_json::to_vec(copy)?)?;
    std::fs::rename(&partial, &path)?;
    Ok(Saved::Written)
}

/// The names of the copies in `folder` containing `filter`, saved ones
/// and v20 files alike, each once, sorted without regard to case.
fn list(folder: &Path, filter: &str) -> Result<Vec<String>> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    // Each name once: a saved copy's spelling before a v20 file's.
    let mut found: Vec<(String, bool)> = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let file = entry.file_name();
        let Some(file) = file.to_str() else {
            continue;
        };
        let lower = file.to_ascii_lowercase();
        let base = [NATIVE, "bls"].iter().find_map(|ending| {
            lower
                .strip_suffix(&format!(".{ending}"))
                .map(|b| (file[..b.len()].to_string(), *ending == NATIVE))
        });
        if let Some((base, native)) = base.filter(|_| entry.path().is_file())
            && name_matches(&base, filter)
        {
            found.push((base, native));
        }
    }
    found.sort_by_key(|(name, native)| (name.to_ascii_lowercase(), !native));
    found.dedup_by(|b, a| a.0.eq_ignore_ascii_case(&b.0));
    let names = found.into_iter().map(|(name, _)| name).collect();
    Ok(names)
}

fn load(files: &CopyFiles, name: &str) -> Result<Option<LoadedCopy>> {
    if let Some(path) = find(&files.own, name, NATIVE) {
        let saved: SavedCopy = serde_json::from_slice(&read(&path)?)
            .with_context(|| path.display().to_string())?;
        saved.validate()?;
        return Ok(Some(LoadedCopy::Saved(saved)));
    }
    let Some(path) = find(&files.own, name, "bls") else {
        return Ok(None);
    };
    let converter = files
        .old_saves
        .converter()
        .context("Old duplication files can be read once the game has finished loading")?;
    let (bricks, palette) = converter.read_duplication(&read(&path)?, name)?;
    Ok(Some(LoadedCopy::Loose { bricks, palette }))
}

impl CopyStore for CopyFiles {
    fn save(&self, request: u64, name: &str, copy: SavedCopy, overwrite: bool) {
        let (folder, name, done) = (self.own.clone(), name.to_string(), self.done.clone());
        std::thread::spawn(move || {
            let result = save(&folder, &name, &copy, overwrite);
            if let Err(error) = &result {
                eprintln!("Saving copy {name}: {error:#}");
            }
            done.lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((request, StoreDone::Saved(result)));
        });
    }

    fn load(&self, request: u64, name: &str) {
        let files = Self {
            own: self.own.clone(),
            old_saves: self.old_saves.clone(),
            done: self.done.clone(),
        };
        let name = name.to_string();
        std::thread::spawn(move || {
            let result = load(&files, &name);
            files.finish(request, StoreDone::Loaded(result));
        });
    }

    fn list(&self, request: u64, filter: &str) {
        let (folder, filter, done) = (self.own.clone(), filter.to_string(), self.done.clone());
        std::thread::spawn(move || {
            let result = list(&folder, &filter);
            done.lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((request, StoreDone::Listed(result)));
        });
    }

    fn poll(&self) -> Vec<(u64, StoreDone)> {
        std::mem::take(&mut *self.done.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_sim::blueprint::{Blueprint, CopyBrick};

    fn wait(files: &CopyFiles) -> StoreDone {
        for _ in 0..500 {
            if let Some((_, done)) = files.poll().pop() {
                return done;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the store never answered");
    }

    #[test]
    fn a_copy_saved_loads_back_by_any_case_and_a_missing_one_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let old = OldSaves::new(dir.path().join("saves"), dir.path().join("cache"));
        let files = CopyFiles::new(old);
        let copy = SavedCopy {
            schema_version: SavedCopy::SCHEMA_VERSION,
            saved_by: "Host".into(),
            palette: vec![[1.0; 4]; 3],
            copy: Blueprint {
                tool: "dup:weapon/wand".into(),
                origin: [0.0; 3],
                size: [2, 1, 1],
                kinds: vec!["plate".into()],
                prints: Vec::new(),
                bricks: vec![CopyBrick {
                    kind: 0,
                    position: [0.5, 0.1, 0.25],
                    quarter_turns: 0,
                    color: 2,
                    color_effect: 0,
                    shape_effect: 0,
                    print: None,
                    off: 0,
                }],
            },
        };
        files.save(1, "My House", copy.clone(), true);
        assert!(matches!(wait(&files), StoreDone::Saved(Ok(Saved::Written))));
        assert!(
            dir.path()
                .join("saves/Duplications/My House.copy.json")
                .is_file()
        );
        files.load(2, "my house");
        match wait(&files) {
            StoreDone::Loaded(Ok(Some(LoadedCopy::Saved(found)))) => assert_eq!(found, copy),
            _ => panic!("the copy did not load back"),
        }
        files.load(3, "nothing");
        assert!(matches!(wait(&files), StoreDone::Loaded(Ok(None))));
        // Kept unless the save may replace it, in any case.
        files.save(4, "MY HOUSE", copy.clone(), false);
        assert!(matches!(wait(&files), StoreDone::Saved(Ok(Saved::Exists))));
        // A v20 file is a kept copy too; listing names each once.
        let folder = dir.path().join("saves/Duplications");
        std::fs::write(folder.join("Barn.bls"), "Duplorcation save file\t0\n").unwrap();
        std::fs::write(folder.join("my house.bls"), "Duplorcation save file\t0\n").unwrap();
        std::fs::write(folder.join("notes.txt"), "").unwrap();
        files.list(5, "");
        match wait(&files) {
            StoreDone::Listed(Ok(names)) => assert_eq!(names, ["Barn", "My House"]),
            _ => panic!("the copies were not listed"),
        }
        files.list(6, "OUS");
        match wait(&files) {
            StoreDone::Listed(Ok(names)) => assert_eq!(names, ["My House"]),
            _ => panic!("the copies were not listed"),
        }
    }

    #[test]
    fn a_v20_duplication_file_waits_for_the_converter() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("saves").join(FOLDER);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("Old.bls"), "Duplorcation save file\t0\n").unwrap();
        let old = OldSaves::new(dir.path().join("saves"), dir.path().join("cache"));
        let files = CopyFiles::new(old);
        files.load(1, "old");
        assert!(matches!(wait(&files), StoreDone::Loaded(Err(_))));
    }

    /// Both mods' v20 files load from the game's own folder, and a file in
    /// a Blockland install beside it is never read.
    #[test]
    fn v20_duplication_files_load_only_from_the_games_own_folder() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("saves").join(FOLDER);
        std::fs::create_dir_all(&folder).unwrap();
        let body = format!(
            "{}Linecount 2\n2x2 Brick\" 0 0 0 0 0 5  0 0 1 1 1\n2x2 Brick\" 0 0 0.6 0 0 3  0 0 1 1 1\n",
            "0.5 0.25 0 1\n".repeat(64)
        );
        let plornt = format!("Duplorcation save file\t2\n1\nDuplication saved by Plornt\n{body}");
        let zeblote = format!(
            "Do not modify this file at all. You will break it.\n1\nSaved by Zeblote (4928)\n{body}"
        );
        std::fs::write(folder.join("Plornt.bls"), &plornt).unwrap();
        std::fs::write(folder.join("Zeblote.bls"), &zeblote).unwrap();
        // An old install's folders, which the game must leave alone.
        let install = dir.path().join("Blockland");
        for elsewhere in ["saves/Duplications", "config/NewDuplicator/Saves"] {
            std::fs::create_dir_all(install.join(elsewhere)).unwrap();
            std::fs::write(install.join(elsewhere).join("Away.bls"), &plornt).unwrap();
        }
        let old = OldSaves::new(dir.path().join("saves"), dir.path().join("cache"));
        old.set_converter(crate::old_saves::Converter::bricks_only(
            serde_json::from_value(serde_json::json!({"schema_version": 1, "bricks": []}))
                .unwrap(),
            "a",
        ));
        let files = CopyFiles::new(old);
        for (request, name) in [(1, "plornt"), (2, "ZEBLOTE")] {
            files.load(request, name);
            match wait(&files) {
                StoreDone::Loaded(Ok(Some(LoadedCopy::Loose { bricks, palette }))) => {
                    assert_eq!(bricks.len(), 2, "{name}");
                    assert_eq!(palette.len(), 64, "{name}");
                }
                _ => panic!("{name} did not load"),
            }
        }
        files.load(3, "away");
        assert!(matches!(wait(&files), StoreDone::Loaded(Ok(None))));
    }
}
