//! Per-user Add-On choices across release folders. Only stable package IDs and
//! explicit intent are stored; each installation supplies its own entries.
use anyhow::{Context, Result, ensure};
use bri_package::library::Library;
use bri_ui::api::AddOnsView;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

const FILE: &str = "add-on-choices.json";
const SCHEMA: u32 = 1;
const LIMIT: u64 = 256 * 1024;
const MAX_CHOICES: usize = 4096;

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Choices {
    schema_version: u32,
    /// Explicit Default restores the current release's default policy first.
    defaults: bool,
    /// Missing IDs remain here until installed again or explicitly reset.
    packages: BTreeMap<String, bool>,
}
impl Choices {
    fn load(state: &Path) -> Result<Self> {
        let path = state.join(FILE);
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    schema_version: SCHEMA,
                    ..Default::default()
                });
            }
            Err(e) => return Err(e).with_context(|| format!("Reading {}", path.display())),
        };
        let mut bytes = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= LIMIT,
            "Add-On choices exceed size limit"
        );
        let choices: Self = serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "Invalid Add-On choices; preserve {} for recovery",
                path.display()
            )
        })?;
        choices.validate()?;
        Ok(choices)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SCHEMA,
            "Unsupported Add-On choices schema {}",
            self.schema_version
        );
        ensure!(
            self.packages.len() <= MAX_CHOICES,
            "Too many saved Add-On choices"
        );
        for id in self.packages.keys() {
            ensure!(
                bri_package::id::namespace_problem(id).is_none(),
                "Invalid saved Add-On id `{id}`"
            );
        }
        Ok(())
    }
    fn save(&self, state: &Path) -> Result<()> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self)?;
        ensure!(
            bytes.len() as u64 <= LIMIT,
            "Add-On choices exceed size limit"
        );
        bri_files::replace(&state.join(FILE), &bytes).context("Saving per-user Add-On choices")?;
        Ok(())
    }
}

/// Actual launch only: preserve shipped defaults unless there is explicit user
/// intent. The read-only --check path does not replay or write preferences.
/// Missing or newly rejected choices are retained and reported, never copied
/// from an old release's PackageEntry or forced past dependency validation.
pub fn restore(root: &Path, state: &Path) -> Result<Vec<String>> {
    let choices = Choices::load(state)?;
    if choices.defaults {
        crate::add_ons::defaults(root)?;
    }
    let mut library = Library::scan(root)?;
    let mut notices = Vec::new();
    // Disable first so enabled dependents can then request their current,
    // validated dependencies, rather than relying on stored load order.
    for enabled in [false, true] {
        for (id, wanted) in &choices.packages {
            if *wanted != enabled {
                continue;
            }
            let Some(entry) = library.get(id) else {
                notices.push(format!("Saved Add-On choice `{id}` is unavailable in this installation; the choice was kept."));
                continue;
            };
            if entry.required || library.companion_of(id).is_some() {
                continue;
            }
            if entry.enabled == enabled {
                continue;
            }
            let plan = library.plan(id, enabled);
            if !plan.allowed() {
                notices.push(format!(
                    "Saved Add-On choice `{id}` could not be applied: {}",
                    plan.refused
                        .iter()
                        .map(|d| d.message.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
                continue;
            }
            library.apply(&plan)?;
        }
    }
    // A changed dependency may imply an outcome different from an older saved
    // choice. Report the canonical outcome rather than weakening its rules.
    for (id, wanted) in &choices.packages {
        if let Some(entry) = library.get(id)
            && entry.enabled != *wanted
        {
            notices.push(format!("Saved Add-On choice `{id}` differs from the current dependency outcome; the choice was kept."));
        }
    }
    Ok(notices)
}

/// Persist exactly this ordinary command and its implied changes. Retained
/// choices for currently unavailable packages are not replaced by the catalog.
pub fn set_enabled(root: &Path, state: &Path, id: &str, enabled: bool) -> Result<AddOnsView> {
    let mut choices = Choices::load(state)?;
    let before = Library::scan(root)?;
    let plan = before.plan(id, enabled);
    let view = crate::add_ons::set_enabled(root, id, enabled)?;
    let after = Library::scan(root)?;
    for changed in plan.also.iter().map(String::as_str).chain([id]) {
        if let Some(entry) = after.get(changed)
            && !entry.required
            && after.companion_of(changed).is_none()
        {
            choices.packages.insert(changed.to_string(), entry.enabled);
        }
    }
    choices.save(state)?;
    Ok(view)
}

/// An explicit reset drops old overrides and follows future release defaults.
pub fn defaults(root: &Path, state: &Path) -> Result<AddOnsView> {
    // Do not overwrite unreadable/corrupt preference data after changing lists.
    Choices::load(state)?;
    let view = crate::add_ons::defaults(root)?;
    Choices {
        schema_version: SCHEMA,
        defaults: true,
        packages: BTreeMap::new(),
    }
    .save(state)?;
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::ScratchDir;
    use serde_json::json;
    use std::fs;

    // The same IDs live at different release-relative paths. These are actual
    // Library package lists/manifests, not stored copies of PackageEntry.
    fn release(root: &Path, folder: &str, ids: &[&str], on: &[&str]) -> Result<()> {
        fs::create_dir_all(root)?;
        let mut entries = Vec::new();
        for id in ids {
            let dir = format!("addons/{folder}/{id}");
            fs::create_dir_all(root.join(&dir))?;
            fs::write(root.join(&dir).join("package.json"), json!({
                "schema_version": 1, "id": id, "version": "1.0.0", "api": 1,
                "name": id, "provides": [], "dependencies": if *id == "tool" { json!({"rules": "*"}) } else { json!({}) },
                "capabilities": []
            }).to_string())?;
            if on.contains(id) {
                entries.push(json!({"id": id, "version": "1.0.0", "side": "shared", "dir": dir}));
            }
        }
        fs::write(
            root.join("packages.json"),
            json!({"schema_version": 1, "packages": entries}).to_string(),
        )?;
        Ok(())
    }

    #[test]
    fn fresh_release_uses_saved_ids_current_paths_and_canonical_dependencies() -> Result<()> {
        let scratch = ScratchDir::new("cross-release-choices")?;
        let old = scratch.path().join("old");
        let new = scratch.path().join("new");
        let state = scratch.path().join("user");
        release(
            &old,
            "old-layout",
            &["tool", "rules", "bundled"],
            &["bundled"],
        )?;
        set_enabled(&old, &state, "tool", true)?;
        set_enabled(&old, &state, "bundled", false)?;
        release(
            &new,
            "new-layout",
            &["tool", "rules", "bundled", "fresh"],
            &["bundled", "fresh"],
        )?;
        // Current release-folder-only behavior before replay: the user's
        // previous enabled/disabled choices are absent in a fresh extraction.
        let fresh = Library::scan(&new)?;
        assert!(!fresh.get("tool").unwrap().enabled);
        assert!(fresh.get("bundled").unwrap().enabled);
        assert!(restore(&new, &state)?.is_empty());
        let library = Library::scan(&new)?;
        assert!(library.get("tool").unwrap().enabled);
        assert!(library.get("rules").unwrap().enabled);
        assert!(!library.get("bundled").unwrap().enabled);
        assert!(
            library.get("fresh").unwrap().enabled,
            "unspecified fresh shipped choices survive"
        );
        let set = bri_package::packages::PackageSet::load_root(&new)?;
        let tool = set.packages.iter().position(|p| p.id == "tool").unwrap();
        let rules = set.packages.iter().position(|p| p.id == "rules").unwrap();
        assert!(rules < tool, "canonical dependency load order");
        assert!(
            set.packages
                .iter()
                .all(|p| p.dir.starts_with("addons/new-layout/"))
        );
        let saved = fs::read_to_string(state.join(FILE))?;
        assert!(
            !saved.contains("layout")
                && !saved.contains("\"version\"")
                && !saved.contains("\"dir\""),
            "preferences contain no stale content objects"
        );
        Ok(())
    }

    #[test]
    fn missing_or_rejected_choices_are_retained_without_forcing_content() -> Result<()> {
        let scratch = ScratchDir::new("retained-choices")?;
        let old = scratch.path().join("old");
        let new = scratch.path().join("new");
        let state = scratch.path().join("user");
        release(&old, "a", &["tool", "rules"], &[])?;
        set_enabled(&old, &state, "tool", true)?;
        // A changed release lacks tool's required rules: enabling is refused.
        release(&new, "b", &["tool", "other"], &[])?;
        let notices = restore(&new, &state)?;
        assert!(notices.iter().any(|n| n.contains("unavailable")));
        assert!(notices.iter().any(|n| n.contains("could not be applied")));
        assert!(!Library::scan(&new)?.get("tool").unwrap().enabled);
        set_enabled(&new, &state, "other", true)?;
        assert_eq!(Choices::load(&state)?.packages.get("tool"), Some(&true));
        assert_eq!(Choices::load(&state)?.packages.get("rules"), Some(&true));
        // The unavailable ID returns at a new path and takes the retained choice.
        let third = scratch.path().join("third");
        release(&third, "c", &["tool", "rules", "other"], &[])?;
        assert!(restore(&third, &state)?.is_empty());
        assert!(Library::scan(&third)?.entries.iter().all(|e| e.enabled));
        Ok(())
    }

    #[test]
    fn absent_choices_keep_defaults_and_portable_state_is_isolated() -> Result<()> {
        let scratch = ScratchDir::new("choice-state")?;
        let root = scratch.path().join("release");
        let state = scratch.path().join("user");
        let portable = scratch.path().join("portable");
        release(&root, "a", &["other"], &["other"])?;
        let before = fs::read(root.join("packages.json"))?;
        assert!(restore(&root, &state)?.is_empty());
        assert_eq!(fs::read(root.join("packages.json"))?, before);
        assert!(
            !state.join(FILE).exists(),
            "first launch does not freeze shipped defaults"
        );
        set_enabled(&root, &state, "other", false)?;
        assert!(Choices::load(&portable)?.packages.is_empty());
        // Default removes explicit overrides and applies current default policy.
        defaults(&root, &state)?;
        let choices = Choices::load(&state)?;
        assert!(choices.defaults && choices.packages.is_empty());
        let new = scratch.path().join("new");
        release(&new, "b", &["other", "fresh"], &["fresh"])?;
        assert!(restore(&new, &state)?.is_empty());
        assert!(Library::scan(&new)?.entries.iter().all(|e| !e.enabled));
        // Corruption stays intact and errors before changing the release lists.
        fs::write(state.join(FILE), b"broken")?;
        let before = fs::read(new.join("packages.json"))?;
        assert!(restore(&new, &state).is_err());
        assert!(set_enabled(&new, &state, "fresh", true).is_err());
        assert_eq!(fs::read(new.join("packages.json"))?, before);
        assert_eq!(fs::read(state.join(FILE))?, b"broken");
        Ok(())
    }

    #[test]
    fn a_new_companion_role_reports_changed_intent_without_overriding_its_owner() -> Result<()> {
        let scratch = ScratchDir::new("changed-companion-choice")?;
        let old = scratch.path().join("old");
        let new = scratch.path().join("new");
        let state = scratch.path().join("user");
        release(&old, "a", &["owner", "helper"], &["owner", "helper"])?;
        set_enabled(&old, &state, "helper", false)?;
        // The same installed ID now follows its owner's enabled state in the
        // current release. Replay must retain the old explicit choice, honor
        // canonical companion policy and report the differing actual outcome.
        release(&new, "b", &["owner", "helper"], &["owner", "helper"])?;
        let manifest = new.join("addons/b/owner/package.json");
        let mut package: serde_json::Value = serde_json::from_slice(&fs::read(&manifest)?)?;
        package["companions"] = json!(["helper"]);
        fs::write(manifest, serde_json::to_vec(&package)?)?;
        let notices = restore(&new, &state)?;
        assert!(
            notices
                .iter()
                .any(|notice| notice.contains("`helper`") && notice.contains("differs")),
            "changed protected-role intent was silent: {notices:?}"
        );
        let library = Library::scan(&new)?;
        assert_eq!(library.companion_of("helper"), Some("owner"));
        assert!(library.get("helper").unwrap().enabled);
        assert_eq!(Choices::load(&state)?.packages.get("helper"), Some(&false));
        Ok(())
    }

    #[test]
    fn settings_and_colorset_id_survive_but_release_local_colorset_data_does_not() -> Result<()> {
        let scratch = ScratchDir::new("colorset-location")?;
        let old = scratch.path().join("old");
        let new = scratch.path().join("new");
        let state = scratch.path().join("user");
        fs::create_dir_all(old.join("addons/Example"))?;
        fs::create_dir_all(&new)?;
        fs::create_dir_all(state.join("colorsets"))?;
        fs::write(old.join("addons/Example/colorSet.txt"), "0 0 255 255")?;
        fs::write(state.join("colorsets/Personal.txt"), "255 0 0 255")?;
        let mut settings = bri_ui::api::Settings::default();
        settings.prefs.insert(
            bri_ui::api::HOST_COLORSET_PREF.into(),
            "addon:Example".into(),
        );
        settings
            .prefs
            .insert("$pref::Video::MaxFps".into(), "144".into());
        crate::settings::save(&state.join("settings.json"), &settings)?;
        assert!(crate::colorsets::selected(&old, &state, "addon:Example")?.is_some());
        assert_eq!(
            crate::settings::load(&state.join("settings.json"))?,
            settings
        );
        assert!(
            crate::colorsets::selected(&new, &state, "addon:Example").is_err(),
            "missing data is reported, never replaced with a stale palette"
        );
        assert_eq!(
            crate::colorsets::selected(&old, &state, "user:Personal.txt")?,
            crate::colorsets::selected(&new, &state, "user:Personal.txt")?
        );
        Ok(())
    }
}
