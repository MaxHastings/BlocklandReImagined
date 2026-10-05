//! Classic Blockland Add-Ons the player drops into the content root's
//! [`DROP_DIR`], the way old `.bls` saves go in the saves folder. The game
//! never looks for a Blockland install: whatever zips (or folders) are in
//! that one folder are converted by the importer (`bri-import-addon`, run by
//! the game), each once, into an Add-On that starts off. A zip that changes
//! is converted again; one that is removed takes its conversion with it.
//! A dropped copy of an Add-On the game already ships (a bundled original,
//! [`crate::library::PackageInfo::bundled`]) is not converted: the shipped
//! one is never adopted, replaced or removed by this folder.
//! Add-Ons there are also each other's reference, so one that requires
//! another (`ForceRequiredAddOn`) finds it beside it.
//!
//! What was converted from what is kept in [`STATE_FILE`]: each dropped
//! Add-On's [`Stamp`] when it was converted, the package it became, or why
//! it could not be. This module only plans ([`plan`]); the game runs the
//! importer and records the results.
use crate::library::{DROP_DIR, IMPORT_DIR, LegacyAddOn, Library};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// What was converted from the drop folder, under the content root.
pub const STATE_FILE: &str = "classic-imports.json";
pub const STATE_SCHEMA: u32 = 1;
/// Files of a dropped folder looked at for its [`Stamp`].
const MAX_STAMP_FILES: u64 = 20_000;

/// A dropped Add-On's size and time, which change when it is replaced or
/// edited: a zip's own, or a folder's files' total size, count and latest
/// time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stamp {
    pub bytes: u64,
    pub files: u64,
    pub modified_ns: u128,
}

/// The [`Stamp`] of the zip or folder at `path`.
pub fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    let time = |m: &std::fs::Metadata| {
        m.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos())
    };
    if meta.is_file() {
        return Some(Stamp {
            bytes: meta.len(),
            files: 1,
            modified_ns: time(&meta),
        });
    }
    let mut out = Stamp {
        bytes: 0,
        files: 0,
        modified_ns: 0,
    };
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).ok()?.flatten() {
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() {
                stack.push(e.path());
            } else {
                out.bytes += meta.len();
                out.files += 1;
                out.modified_ns = out.modified_ns.max(time(&meta));
                if out.files >= MAX_STAMP_FILES {
                    return Some(out);
                }
            }
        }
    }
    Some(out)
}

/// One dropped Add-On's last conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// Its name as dropped (`Weapon_Shotgun`).
    pub name: String,
    pub stamp: Stamp,
    /// The package it became and its content-root-relative folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// Why it could not be converted; tried again once it changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The package the game ships as it, so it was not converted
    /// ([`Step::Included`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub included: Option<String>,
}

/// [`STATE_FILE`]: records by lower-case name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub schema_version: u32,
    #[serde(default)]
    pub records: BTreeMap<String, Record>,
}

impl State {
    /// The root's state; empty when there is none or it does not read (the
    /// next sync then adopts what is already converted).
    pub fn load(root: &Path) -> Self {
        std::fs::read(root.join(STATE_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .filter(|s| s.schema_version == STATE_SCHEMA)
            .unwrap_or(Self {
                schema_version: STATE_SCHEMA,
                records: BTreeMap::new(),
            })
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let path = root.join(STATE_FILE);
        let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)
            .with_context(|| format!("Writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("Replacing {}", path.display()))
    }

    pub fn get(&self, name: &str) -> Option<&Record> {
        self.records.get(&name.to_ascii_lowercase())
    }

    pub fn set(&mut self, record: Record) {
        self.records
            .insert(record.name.to_ascii_lowercase(), record);
    }

    pub fn forget(&mut self, name: &str) {
        self.records.remove(&name.to_ascii_lowercase());
    }

    /// Whether `legacy`'s last conversion failed and it has not changed
    /// since: its error.
    pub fn failed(&self, legacy: &LegacyAddOn) -> Option<&str> {
        let record = self.get(&legacy.name)?;
        (legacy.imported_as.is_none() && Some(record.stamp) == stamp(&legacy.path))
            .then_some(record.error.as_deref())
            .flatten()
    }
}

/// One thing the game does to bring its conversions in line with the drop
/// folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Convert a dropped Add-On in place of `replaces`, the package an
    /// earlier copy of it became ([`Library::install_staged`]).
    Import {
        name: String,
        path: std::path::PathBuf,
        stamp: Stamp,
        replaces: Option<String>,
    },
    /// Remove the package `id` (in `dir`), converted from an Add-On no
    /// longer in the folder.
    Remove {
        name: String,
        id: String,
        dir: String,
    },
    /// Remember that `id` is the conversion of `name` as it is now: a
    /// conversion the player made before the game kept records.
    Adopt {
        name: String,
        id: String,
        stamp: Stamp,
    },
    /// Tell the player `name` is already included: the game ships it as
    /// `id`, so it is not converted. Remembered, so it is said once.
    Included {
        name: String,
        id: String,
        stamp: Stamp,
    },
}

/// What brings the root's conversions in line with its drop folder.
/// Removals come first, so a folder an Add-On taken out held is free
/// before a new conversion may take its name.
pub fn plan(library: &Library, state: &State) -> Vec<Step> {
    let mut removals = vec![];
    let mut rest = vec![];
    for legacy in &library.legacy {
        let Some(now) = stamp(&legacy.path) else {
            continue;
        };
        let record = state.get(&legacy.name);
        if let Some(id) = &legacy.included {
            if record.is_none_or(|r| r.included.as_ref() != Some(id)) {
                rest.push(Step::Included {
                    name: legacy.name.clone(),
                    id: id.clone(),
                    stamp: now,
                });
            }
            continue;
        }
        match (&legacy.imported_as, record) {
            (Some(_), Some(r)) if r.stamp == now => {}
            (Some(id), None) => rest.push(Step::Adopt {
                name: legacy.name.clone(),
                id: id.clone(),
                stamp: now,
            }),
            (None, Some(r)) if r.stamp == now && r.error.is_some() => {}
            (imported, _) => rest.push(Step::Import {
                name: legacy.name.clone(),
                path: legacy.path.clone(),
                stamp: now,
                replaces: imported.clone(),
            }),
        }
    }
    for record in state.records.values() {
        let dropped = library
            .legacy
            .iter()
            .any(|l| l.name.eq_ignore_ascii_case(&record.name));
        if dropped {
            continue;
        }
        // Only what the game converted for the player, in its own import
        // folder: never a package the game ships, whatever a record says.
        if let (Some(id), Some(dir)) = (&record.id, &record.dir)
            && dir.starts_with(&format!("{IMPORT_DIR}/"))
            && library.get(id).is_some_and(|e| e.package.dir == *dir)
            && !library.shipped(id)
        {
            removals.push(Step::Remove {
                name: record.name.clone(),
                id: id.clone(),
                dir: dir.clone(),
            });
        } else {
            // Gone already, or never converted: only the record is left.
            removals.push(Step::Remove {
                name: record.name.clone(),
                id: String::new(),
                dir: String::new(),
            });
        }
    }
    removals.extend(rest);
    removals
}

/// The drop folder of the content root `root`.
pub fn folder(root: &Path) -> std::path::PathBuf {
    root.join(DROP_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bri-classic-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(DROP_DIR)).unwrap();
        dir
    }

    /// A package as the importer makes it from the dropped `name`.
    fn converted(root: &Path, name: &str) -> String {
        let id = name.to_ascii_lowercase();
        let dir = format!("{IMPORT_DIR}/{id}");
        std::fs::create_dir_all(root.join(&dir)).unwrap();
        std::fs::write(
            root.join(&dir).join("package.json"),
            json!({ "schema_version": 1, "id": id, "version": "1.0.0", "api": 1,
                "provides": [{ "kind": "weapons", "id": format!("{id}:weapons/w"), "file": "w.json" }],
                "provenance": { "source": format!("Blockland Add-On {name} (zip), sha256 00") } })
            .to_string(),
        )
        .unwrap();
        dir
    }

    fn names(steps: &[Step]) -> Vec<String> {
        steps
            .iter()
            .map(|s| match s {
                Step::Import { name, replaces, .. } => format!("import {name} {replaces:?}"),
                Step::Remove { name, id, .. } => format!("remove {name} {id}"),
                Step::Adopt { name, id, .. } => format!("adopt {name} {id}"),
                Step::Included { name, id, .. } => format!("included {name} {id}"),
            })
            .collect()
    }

    #[test]
    fn dropped_zips_convert_once_again_when_changed_and_go_when_removed() {
        let root = temp("plan");
        let zip = folder(&root).join("Weapon_Gun.zip");
        std::fs::write(&zip, b"PK one").unwrap();
        std::fs::write(folder(&root).join("readme.txt"), b"not an add-on").unwrap();
        let mut state = State::load(&root);
        let steps = plan(&Library::scan(&root).unwrap(), &state);
        assert_eq!(names(&steps), ["import Weapon_Gun None"]);
        // Converted: nothing more to do.
        let dir = converted(&root, "Weapon_Gun");
        let Step::Import { stamp: now, .. } = steps[0] else {
            unreachable!()
        };
        state.set(Record {
            name: "Weapon_Gun".into(),
            stamp: now,
            id: Some("weapon_gun".into()),
            dir: Some(dir.clone()),
            error: None,
            included: None,
        });
        state.save(&root).unwrap();
        let state = State::load(&root);
        assert!(plan(&Library::scan(&root).unwrap(), &state).is_empty());
        // A new copy is converted again, replacing the old conversion.
        std::fs::write(&zip, b"PK two, longer").unwrap();
        assert_eq!(
            names(&plan(&Library::scan(&root).unwrap(), &state)),
            ["import Weapon_Gun Some(\"weapon_gun\")"]
        );
        // Taken out of the folder, its conversion goes.
        std::fs::remove_file(&zip).unwrap();
        assert_eq!(
            names(&plan(&Library::scan(&root).unwrap(), &state)),
            ["remove Weapon_Gun weapon_gun"]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn failures_wait_for_a_change_and_older_conversions_are_adopted() {
        let root = temp("failures");
        let bad = folder(&root).join("Broken.zip");
        std::fs::write(&bad, b"not a zip").unwrap();
        std::fs::write(folder(&root).join("Weapon_Old.zip"), b"PK").unwrap();
        converted(&root, "Weapon_Old");
        let mut state = State::load(&root);
        let library = Library::scan(&root).unwrap();
        assert_eq!(
            names(&plan(&library, &state)),
            ["import Broken None", "adopt Weapon_Old weapon_old"]
        );
        state.set(Record {
            name: "Broken".into(),
            stamp: stamp(&bad).unwrap(),
            id: None,
            dir: None,
            error: Some("not a zip".into()),
            included: None,
        });
        let broken = library.legacy.iter().find(|l| l.name == "Broken").unwrap();
        assert_eq!(state.failed(broken), Some("not a zip"));
        assert_eq!(
            names(&plan(&library, &state)),
            ["adopt Weapon_Old weapon_old"]
        );
        std::fs::write(&bad, b"PK fixed and longer").unwrap();
        assert_eq!(state.failed(broken), None);
        assert_eq!(
            names(&plan(&library, &state)),
            ["import Broken None", "adopt Weapon_Old weapon_old"]
        );
        // Removed after failing: only its record goes.
        std::fs::remove_file(&bad).unwrap();
        assert_eq!(
            names(&plan(&Library::scan(&root).unwrap(), &state)),
            ["remove Broken ", "adopt Weapon_Old weapon_old"]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_dropped_folders_stamp_follows_its_files() {
        let root = temp("stamp");
        let dir = folder(&root).join("Brick_Mine");
        std::fs::create_dir_all(dir.join("icons")).unwrap();
        std::fs::write(dir.join("server.cs"), b"// one").unwrap();
        let before = stamp(&dir).unwrap();
        std::fs::write(dir.join("icons/a.png"), b"png").unwrap();
        let after = stamp(&dir).unwrap();
        assert_eq!((before.files, after.files), (1, 2));
        assert_ne!(before, after);
        assert!(stamp(&root.join("missing")).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_or_unreadable_state_is_empty() {
        let root = temp("state");
        assert!(State::load(&root).records.is_empty());
        std::fs::write(root.join(STATE_FILE), b"{ nonsense").unwrap();
        assert!(State::load(&root).records.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn uninstalling_a_conversion_turns_it_off_and_deletes_only_imports() {
        let root = temp("uninstall");
        let dir = converted(&root, "Weapon_Gun");
        // A port's host rules beside it, needing it.
        let rules = format!("{dir}-rules");
        std::fs::create_dir_all(root.join(&rules)).unwrap();
        std::fs::write(
            root.join(&rules).join("package.json"),
            json!({ "schema_version": 1, "id": "weapon_gun-rules", "version": "1.0.0", "api": 1,
                "dependencies": { "weapon_gun": "^1.0" },
                "provides": [{ "kind": "behaviour", "id": "weapon_gun-rules:behaviour/b", "file": "b.json" }] })
            .to_string(),
        )
        .unwrap();
        let mut library = Library::scan(&root).unwrap();
        let plan = library.plan("weapon_gun-rules", true);
        library.apply(&plan).unwrap();
        assert!(library.get("weapon_gun").unwrap().enabled);
        library.uninstall("weapon_gun").unwrap();
        assert!(library.get("weapon_gun").is_none() && library.get("weapon_gun-rules").is_none());
        assert!(!root.join(&dir).exists() && !root.join(&rules).exists());
        // A package the game did not convert stays.
        std::fs::create_dir_all(root.join("mods/mine")).unwrap();
        std::fs::write(
            root.join("mods/mine/package.json"),
            json!({ "schema_version": 1, "id": "mine", "version": "1.0.0", "api": 1,
                "provides": [{ "kind": "weapons", "id": "mine:weapons/w", "file": "w.json" }] })
            .to_string(),
        )
        .unwrap();
        let mut library = Library::scan(&root).unwrap();
        assert!(library.uninstall("mine").is_err());
        assert!(root.join("mods/mine").is_dir());
        let _ = std::fs::remove_dir_all(&root);
    }
}
