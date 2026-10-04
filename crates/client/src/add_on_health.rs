//! The game's side of Add-On health (`bri_package::health`): what an
//! enabled Add-On names that this computer could not find or use, gathered
//! while the content loads and checked once against what loaded. Each
//! problem is logged once, listed under its Add-On in the Add-Ons screen,
//! summed up to an admin entering a game, and written to
//! `<state>/logs/add-on-health.json` for the release gate.
//!
//! Loading code reports through [`report`]: inside [`collecting`] (the
//! content and presentation loads) the problem joins that load's list,
//! elsewhere it goes straight to the log. Nothing here runs per frame.
use bri_package::health::{Health, Kind, Problem};
use bri_package::packages::PackageSet;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

thread_local! {
    static SINK: RefCell<Option<Vec<Problem>>> = const { RefCell::new(None) };
}

/// Run `load`, returning with it the problems [`report`]ed on this thread
/// meanwhile. A collection inside another also hands its problems out.
pub fn collecting<T>(load: impl FnOnce() -> T) -> (T, Vec<Problem>) {
    let outer = SINK.with(|s| s.replace(Some(Vec::new())));
    let value = load();
    let problems = SINK.with(|s| s.replace(outer)).unwrap_or_default();
    SINK.with(|s| {
        if let Some(outer) = s.borrow_mut().as_mut() {
            outer.extend(problems.iter().cloned());
        }
    });
    (value, problems)
}

/// Report an Add-On problem: to the [`collecting`] load running on this
/// thread, else to the log.
pub fn report(problem: Problem) {
    let unheard = SINK.with(|s| match s.borrow_mut().as_mut() {
        Some(list) => {
            list.push(problem);
            None
        }
        None => Some(problem),
    });
    if let Some(problem) = unheard {
        bri_console::warn(problem.to_string());
    }
}

/// Names the enabled Add-Ons go by in what reports them (id, folder,
/// content namespace, display name), each to its id and display name.
#[derive(Debug, Clone, Default)]
pub struct Owners {
    by_name: BTreeMap<String, (String, String)>,
}

impl Owners {
    pub fn new(root: &Path, set: &PackageSet) -> Self {
        let mut by_name = BTreeMap::new();
        for entry in set.packages.iter().filter(|p| p.role.is_none()) {
            let folder = bri_package::health::package_folder(&entry.dir);
            let name = bri_package::library::add_on_label(&root.join(&entry.dir), &entry.id);
            let owner = (entry.id.clone(), name.clone());
            for key in [
                entry.id.as_str(),
                entry.dir.as_str(),
                folder,
                folder.rsplit('/').next().unwrap_or(folder),
                bri_package::id::content_namespace(&entry.id),
                name.as_str(),
            ] {
                by_name
                    .entry(key.to_ascii_lowercase())
                    .or_insert_with(|| owner.clone());
            }
        }
        Self { by_name }
    }
    /// The id of the Add-On known as `name`, else `name` itself.
    pub fn id(&self, name: &str) -> String {
        self.by_name
            .get(&name.to_ascii_lowercase())
            .map_or_else(|| name.to_string(), |(id, _)| id.clone())
    }
    /// The display name of the Add-On known as `name`, else `name`.
    pub fn name(&self, name: &str) -> String {
        self.by_name
            .get(&name.to_ascii_lowercase())
            .map_or_else(|| name.to_string(), |(_, n)| n.clone())
    }
}

/// What loaded, to check an Add-On's references against: the same lookups
/// the game makes when it plays them.
pub struct Loaded<'a> {
    pub weapons: &'a bri_weapons::Pack,
    pub effects: &'a crate::weapon_effects::WeaponEffects,
    pub items: &'a crate::items::ItemAssets,
    /// `None` where no sound is played (a check without an audio device
    /// still has the sound bank, so this is rare).
    pub audio: Option<&'a dyn crate::audio::SoundLookup>,
}

/// Every sound, effect, damage type and model an Add-On's weapons name
/// that nothing loaded provides.
pub fn check_references(loaded: &Loaded) -> Vec<Problem> {
    let mut out = Vec::new();
    for r in loaded.weapons.add_on_references() {
        let (found, effect) = match r.kind {
            Kind::Sound => match loaded.audio {
                Some(audio) => (audio.has_sound(r.name), "plays silently"),
                None => continue,
            },
            Kind::Effect | Kind::Explosion => (loaded.effects.knows(r.name), "shows nothing"),
            Kind::DamageType => (
                loaded.weapons.has_damage_type(r.name),
                "kills by it read as the default kill message",
            ),
            _ => continue,
        };
        if !found {
            out.push(Problem::new(r.add_on, r.kind, r.name, effect).used_by(r.used_by));
        }
    }
    let presentation = &loaded.items.presentation;
    for (id, item) in &loaded.weapons.items {
        let Some(add_on) = bri_weapons::add_on_of(id) else {
            continue;
        };
        if !item.model.is_empty() && presentation.item_appearance(id).is_none() {
            out.push(
                Problem::new(add_on, Kind::Model, &item.model, "the item is invisible")
                    .used_by(format!("item {id}")),
            );
        }
    }
    for (id, image) in &loaded.weapons.images {
        let Some(add_on) = bri_weapons::add_on_of(id) else {
            continue;
        };
        if !image.model.is_empty() && presentation.image_appearance(id).is_none() {
            out.push(
                Problem::new(
                    add_on,
                    Kind::Model,
                    &image.model,
                    "nothing is drawn in the hand",
                )
                .used_by(format!("image {id}")),
            );
        }
    }
    out
}

/// A problem that left an Add-On's rules, HUD or modes out of loading.
pub fn rules_problem(diagnostic: &bri_package::diag::Diagnostic) -> Problem {
    let at = diagnostic.location.as_deref().unwrap_or_default();
    let (add_on, file) = at.split_once('/').unwrap_or((at, ""));
    Problem::new(
        add_on,
        Kind::Rules,
        if file.is_empty() {
            &diagnostic.code
        } else {
            file
        },
        format!(
            "{}; its rules, HUD and modes are left out",
            diagnostic.message.trim_end_matches('.')
        ),
    )
}

/// An Add-On that stopped the game loading and was left out, from
/// `ClientContent::load_leaving_out_broken`'s `id: reason`.
pub fn left_out_problem(line: &str) -> Problem {
    let (id, reason) = line.split_once(": ").unwrap_or((line, "it did not load"));
    Problem::new(
        id,
        Kind::LeftOut,
        id,
        format!("{reason}; the game started without it"),
    )
}

/// The Add-On health of the last load, with what names its Add-Ons.
#[derive(Debug, Clone, Default)]
pub struct AddOnHealth {
    pub health: Health,
    pub owners: Owners,
}

impl AddOnHealth {
    /// Gather `problems` under each Add-On's id, logging each once.
    pub fn new(owners: Owners, problems: impl IntoIterator<Item = Problem>) -> Self {
        let mut health = Health::default();
        for mut problem in problems {
            problem.add_on = owners.id(&problem.add_on);
            let line = format!(
                "Add-On {}: {}",
                owners.name(&problem.add_on),
                problem.line()
            );
            if health.note(problem) {
                bri_console::warn(line);
            }
        }
        Self { health, owners }
    }
    /// The one line for an admin entering a game, if anything is wrong.
    pub fn summary(&self) -> Option<String> {
        self.health.summary(|id| self.owners.name(id))
    }
    /// The problems of the Add-On `id` (or named `name`), one line each.
    pub fn lines_for(&self, id: &str, name: &str) -> Vec<String> {
        self.health.of(&[id, name]).map(Problem::line).collect()
    }
    /// Write `add-on-health.json` under `state/logs`; returns its path.
    pub fn write_report(&self, state: &Path) -> anyhow::Result<PathBuf> {
        self.health
            .report(&crate::updates::version())
            .write(&state.join("logs"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problems_reported_inside_a_load_are_collected_not_lost() {
        let problem = |n: &str| Problem::new("a", Kind::Sound, n, "plays silently");
        let ((), inner) = collecting(|| report(problem("one")));
        assert_eq!(inner, [problem("one")]);
        let ((), outer) = collecting(|| {
            report(problem("two"));
            let ((), nested) = collecting(|| report(problem("three")));
            assert_eq!(nested, [problem("three")]);
        });
        assert_eq!(outer, [problem("two"), problem("three")]);
    }

    #[test]
    fn rules_problems_name_their_add_on_and_file() {
        let d = bri_package::diag::Diagnostic::error("hud.key", "Key K is taken.")
            .at("my-hud/assets/hud.json");
        let p = rules_problem(&d);
        assert_eq!((p.add_on.as_str(), p.kind), ("my-hud", Kind::Rules));
        assert_eq!(p.reference, "assets/hud.json");
        assert_eq!(
            p.effect,
            "Key K is taken; its rules, HUD and modes are left out"
        );
    }
}
