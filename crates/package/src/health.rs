//! Add-On health: what an enabled Add-On names that the game could not find
//! or use. A missing shape, sound, texture, datablock, damage type or rules
//! companion never silently becomes nothing: every subsystem that resolves an
//! Add-On's references reports the ones it could not resolve here, once each,
//! and the game shows them in the log, to admins in game, in the Add-Ons
//! screen and in a machine-readable report (`add-on-health.json`).
//!
//! Problems are deduplicated by Add-On, kind and reference, so a sound that
//! three weapon states name is one problem, however often it is played.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Schema of [`Report`].
pub const SCHEMA: u32 = 1;
/// The report's file name, in the game's `logs` folder.
pub const REPORT_FILE: &str = "add-on-health.json";
/// Problems kept; past this many the rest are only counted, so a hostile or
/// badly broken Add-On cannot grow the list without bound.
pub const MAX_PROBLEMS: usize = 512;
/// Add-Ons named in a [`Health::summary`] before "and N more".
const SUMMARY_ADD_ONS: usize = 3;

/// The package folder of a merged part's content-root-relative directory
/// (`<folder>/assets`), which names its Add-On until its id is known.
pub fn package_folder(dir: &str) -> &str {
    dir.strip_suffix("/assets").unwrap_or(dir)
}

/// What kind of thing could not be found or used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// The whole Add-On was left out: it stopped the game loading.
    LeftOut,
    /// Its rules, HUD, mode or script part did not load.
    Rules,
    /// A rules companion (`<id>-rules`) it needs is missing or off.
    Companion,
    /// Another Add-On it needs is not installed or not on.
    Dependency,
    Item,
    Image,
    Projectile,
    Explosion,
    Vehicle,
    /// A particle emitter, light or explosion effect.
    Effect,
    Sound,
    DamageType,
    /// A shape (model).
    Model,
    Texture,
    Icon,
    /// Other art or presentation data shown with a stand-in.
    Presentation,
    /// Something the import could not convert or resolve.
    Import,
    /// A script operation that failed while the game ran.
    Script,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::LeftOut => "Add-On",
            Kind::Rules => "rules",
            Kind::Companion => "rules companion",
            Kind::Dependency => "required Add-On",
            Kind::Item => "item",
            Kind::Image => "image",
            Kind::Projectile => "projectile",
            Kind::Explosion => "explosion",
            Kind::Vehicle => "vehicle",
            Kind::Effect => "effect",
            Kind::Sound => "sound",
            Kind::DamageType => "damage type",
            Kind::Model => "model",
            Kind::Texture => "texture",
            Kind::Icon => "icon",
            Kind::Presentation => "art",
            Kind::Import => "import",
            Kind::Script => "script",
        }
    }
}

/// One thing an Add-On names that the game could not find or use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    /// The Add-On's id, or its name where only that is known.
    pub add_on: String,
    pub kind: Kind,
    /// What was named: an id, a file or a datablock name.
    pub reference: String,
    /// What names it (an item, image, state...), when known.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub used_by: String,
    /// What goes wrong in game, in plain words.
    pub effect: String,
}

impl Problem {
    pub fn new(
        add_on: impl Into<String>,
        kind: Kind,
        reference: impl Into<String>,
        effect: impl Into<String>,
    ) -> Self {
        Self {
            add_on: add_on.into(),
            kind,
            reference: reference.into(),
            used_by: String::new(),
            effect: effect.into(),
        }
    }
    pub fn used_by(mut self, used_by: impl Into<String>) -> Self {
        self.used_by = used_by.into();
        self
    }
    fn key(&self) -> (String, Kind, String) {
        (
            self.add_on.to_ascii_lowercase(),
            self.kind,
            self.reference.to_ascii_lowercase(),
        )
    }
    /// The problem without the Add-On's name, for a list under it.
    pub fn line(&self) -> String {
        let mut line = format!("{} {}", self.kind.word(), self.reference);
        if !self.used_by.is_empty() {
            line.push_str(&format!(" (used by {})", self.used_by));
        }
        line.push_str(": ");
        line.push_str(&self.effect);
        line
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Add-On {}: {}", self.add_on, self.line())
    }
}

/// The deduplicated problems of the enabled Add-Ons.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Health {
    problems: BTreeMap<(String, Kind, String), Problem>,
    /// Problems past [`MAX_PROBLEMS`], counted only.
    overflow: usize,
}

impl Health {
    /// Record `problem`; true the first time it is seen (log it then).
    pub fn note(&mut self, problem: Problem) -> bool {
        let key = problem.key();
        if self.problems.contains_key(&key) {
            return false;
        }
        if self.problems.len() >= MAX_PROBLEMS {
            self.overflow += 1;
            return false;
        }
        self.problems.insert(key, problem);
        true
    }
    pub fn is_empty(&self) -> bool {
        self.problems.is_empty() && self.overflow == 0
    }
    pub fn len(&self) -> usize {
        self.problems.len() + self.overflow
    }
    /// Problems ordered by Add-On, then kind and reference.
    pub fn problems(&self) -> impl Iterator<Item = &Problem> {
        self.problems.values()
    }
    /// The problems of `add_on`, matched by id or name, case aside.
    pub fn of<'a>(&'a self, names: &'a [&'a str]) -> impl Iterator<Item = &'a Problem> + 'a {
        self.problems
            .values()
            .filter(move |p| names.iter().any(|n| n.eq_ignore_ascii_case(&p.add_on)))
    }
    /// Add-Ons with problems, in order, each once.
    pub fn add_ons(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for p in self.problems.values() {
            if !out.iter().any(|a| a.eq_ignore_ascii_case(&p.add_on)) {
                out.push(&p.add_on);
            }
        }
        out
    }
    /// One line for admins: how many problems and in which Add-Ons, or
    /// `None` when there are none. `name` turns an id into a display name.
    pub fn summary(&self, name: impl Fn(&str) -> String) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let count = self.len();
        let add_ons = self.add_ons();
        let mut named: Vec<String> = add_ons
            .iter()
            .take(SUMMARY_ADD_ONS)
            .map(|a| name(a))
            .collect();
        if add_ons.len() > SUMMARY_ADD_ONS {
            named.push(format!("{} more", add_ons.len() - SUMMARY_ADD_ONS));
        }
        let noun = if count == 1 { "problem" } else { "problems" };
        Some(format!(
            "{count} Add-On {noun}: {}. Each is listed under its Add-On in Add-Ons.",
            named.join(", ")
        ))
    }
    /// The machine-readable report of these problems.
    pub fn report(&self, game_version: &str) -> Report {
        Report {
            schema_version: SCHEMA,
            game_version: game_version.into(),
            count: self.len(),
            unlisted: self.overflow,
            problems: self.problems.values().cloned().collect(),
        }
    }
}

/// `add-on-health.json`: what the game found wrong with its enabled
/// Add-Ons when it last loaded them. The release gate reads it from a
/// packaged build run with `--check` against real copies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub game_version: String,
    /// Every problem, listed or not.
    pub count: usize,
    /// Problems past the list's limit, counted only.
    pub unlisted: usize,
    pub problems: Vec<Problem>,
}

impl Report {
    /// Write the report to `dir/add-on-health.json`; returns the path.
    pub fn write(&self, dir: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(REPORT_FILE);
        let temporary = dir.join(format!("{REPORT_FILE}.tmp"));
        std::fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&temporary, &path)?;
        Ok(path)
    }
}

/// The import's machine-readable report, beside an imported Add-On.
pub const IMPORT_REPORT: &str = "import-report.json";
/// Largest import report read; a bigger one is itself a problem.
const MAX_IMPORT_REPORT: u64 = 16 * 1024 * 1024;

/// Problems of the Add-Ons `set` lists that show before any content loads:
/// a companion (an original's host rules) or a dependency that is not
/// installed or not on, and what its import could not resolve (a required
/// Add-On it does not know, a port that does not fit this copy). Base
/// packages (with a role) are checked by content tests, not here.
pub fn check_set(root: &std::path::Path, set: &crate::packages::PackageSet) -> Vec<Problem> {
    let on = |id: &str| set.packages.iter().any(|p| p.id == id);
    let mut out = Vec::new();
    for entry in set.packages.iter().filter(|p| p.role.is_none()) {
        let dir = root.join(&entry.dir);
        let id = entry.id.as_str();
        if let Some(info) = crate::library::read_info(&dir.join(crate::library::MANIFEST_FILE)) {
            for companion in info.companions.iter().filter(|c| !on(c)) {
                out.push(Problem::new(
                    id,
                    Kind::Companion,
                    companion.clone(),
                    "is not installed or not on, so this Add-On's rules do not run (its weapons may do nothing)",
                ));
            }
            for dependency in info.dependencies.keys().filter(|d| !on(d)) {
                out.push(Problem::new(
                    id,
                    Kind::Dependency,
                    dependency.clone(),
                    "is not installed or not on, so what this Add-On uses from it is missing",
                ));
            }
        }
        out.extend(import_problems(id, &dir.join(IMPORT_REPORT), &on));
    }
    out
}

/// What an Add-On's import report says it could not resolve.
fn import_problems(id: &str, path: &std::path::Path, on: &dyn Fn(&str) -> bool) -> Vec<Problem> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Vec::new();
    };
    let report = (meta.len() <= MAX_IMPORT_REPORT)
        .then(|| std::fs::read(path).ok())
        .flatten()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    let Some(report) = report else {
        return vec![Problem::new(
            id,
            Kind::Import,
            IMPORT_REPORT,
            "could not be read, so what the import left unresolved is unknown",
        )];
    };
    let text = |v: &serde_json::Value, key: &str| {
        v.get(key)
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let list = |key: &str| {
        report
            .get(key)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    for dependency in list("dependencies") {
        let addon = text(&dependency, "addon");
        let package = text(&dependency, "package");
        if text(&dependency, "status") == "missing" {
            out.push(
                Problem::new(
                    id,
                    Kind::Dependency,
                    addon,
                    "was not found when it was imported, so what it uses from it is missing",
                )
                .used_by(text(&dependency, "how")),
            );
        } else if !package.is_empty() && !on(&package) {
            out.push(Problem::new(
                id,
                Kind::Dependency,
                package,
                format!("is not on, so what it uses from {addon} is missing"),
            ));
        }
    }
    for port in list("ports") {
        if port.get("applied").and_then(|a| a.as_bool()) == Some(false) {
            let reason = text(&port, "reason");
            out.push(Problem::new(
                id,
                Kind::Rules,
                text(&port, "port"),
                if reason.is_empty() {
                    "the game's rewrite of its scripts does not fit this copy, so its scripted behaviour does nothing".to_string()
                } else {
                    format!("the game's rewrite of its scripts does not fit this copy ({reason}), so its scripted behaviour does nothing")
                },
            ));
        }
        if let Some(rules) = port
            .get("rules")
            .map(|r| text(r, "id"))
            .filter(|r| !r.is_empty())
            && port.get("applied").and_then(|a| a.as_bool()) != Some(false)
            && !on(&rules)
        {
            out.push(Problem::new(
                id,
                Kind::Companion,
                rules,
                "is not installed or not on, so this Add-On's rules do not run (its weapons may do nothing)",
            ));
        }
    }
    for asset in list("assets") {
        if text(&asset, "status") == "failed" {
            out.push(Problem::new(
                id,
                Kind::Import,
                text(&asset, "source"),
                "did not convert, so it is missing in game",
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sound(add_on: &str, name: &str, used_by: &str) -> Problem {
        Problem::new(add_on, Kind::Sound, name, "plays silently").used_by(used_by)
    }

    #[test]
    fn one_problem_per_add_on_kind_and_reference() {
        let mut health = Health::default();
        assert!(health.note(sound("sniper", "SniperShot", "Fire")));
        assert!(!health.note(sound("sniper", "snipershot", "Reload")));
        assert!(health.note(sound("sniper", "SniperBolt", "Reload")));
        assert!(health.note(sound("grenade", "SniperShot", "Throw")));
        assert_eq!(health.len(), 3);
        assert_eq!(health.add_ons(), ["grenade", "sniper"]);
        assert_eq!(health.of(&["Sniper"]).count(), 2);
        let first = health.of(&["sniper"]).find(|p| p.reference == "SniperShot");
        assert_eq!(first.map(|p| p.used_by.as_str()), Some("Fire"));
    }

    #[test]
    fn summary_names_the_add_ons_and_counts_every_problem() {
        let mut health = Health::default();
        assert_eq!(health.summary(str::to_string), None);
        for (i, add_on) in ["a", "b", "c", "d", "e"].iter().enumerate() {
            health.note(sound(add_on, &format!("s{i}"), ""));
        }
        health.note(sound("a", "other", ""));
        assert_eq!(
            health.summary(|id| id.to_uppercase()).as_deref(),
            Some("6 Add-On problems: A, B, C, 2 more. Each is listed under its Add-On in Add-Ons.")
        );
    }

    #[test]
    fn the_list_is_bounded_and_the_rest_counted() {
        let mut health = Health::default();
        for i in 0..MAX_PROBLEMS + 5 {
            health.note(sound("a", &format!("s{i}"), ""));
        }
        assert_eq!(health.problems().count(), MAX_PROBLEMS);
        assert_eq!(health.len(), MAX_PROBLEMS + 5);
        let report = health.report("test");
        assert_eq!((report.count, report.unlisted), (MAX_PROBLEMS + 5, 5));
    }

    #[test]
    fn a_problem_reads_as_one_line() {
        let p = sound("Sniper Rifle", "SniperShot", "image sniper state Fire");
        assert_eq!(
            p.to_string(),
            "Add-On Sniper Rifle: sound SniperShot (used by image sniper state Fire): plays silently"
        );
    }

    fn entry(id: &str) -> crate::packages::PackageEntry {
        crate::packages::PackageEntry {
            id: id.into(),
            version: "1.0.0".into(),
            side: crate::packages::Side::Server,
            dir: format!("addons/{id}"),
            role: None,
        }
    }

    #[test]
    fn an_off_companion_and_what_the_import_left_unresolved_are_problems() {
        let root = std::env::temp_dir().join(format!("bri-health-{}", std::process::id()));
        let dir = root.join("addons/sniper");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(crate::library::MANIFEST_FILE),
            r#"{"id":"sniper","version":"1.0.0","name":"Sniper Rifle","companions":["sniper-rules"],"dependencies":{"scope":">=1.0.0"}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join(IMPORT_REPORT),
            r#"{"dependencies":[
                {"addon":"Weapon_Gun","how":"ForceRequiredAddOn","status":"reference","package":"v20-weapons"},
                {"addon":"Weapon_Scope","how":"ForceRequiredAddOn","status":"missing","package":null}],
              "ports":[{"port":"sniper","applied":false,"reason":"onFire differs"}],
              "assets":[{"source":"shot.wav","status":"failed"},{"source":"gun.dts","status":"converted"}]}"#,
        )
        .unwrap();
        let set = crate::packages::PackageSet {
            schema_version: 1,
            packages: vec![entry("sniper"), entry("v20-weapons")],
        };
        let found: Vec<(Kind, String)> = check_set(&root, &set)
            .into_iter()
            .map(|p| {
                assert_eq!(p.add_on, "sniper");
                (p.kind, p.reference)
            })
            .collect();
        assert_eq!(
            found,
            [
                (Kind::Companion, "sniper-rules".to_string()),
                (Kind::Dependency, "scope".into()),
                (Kind::Dependency, "Weapon_Scope".into()),
                (Kind::Rules, "sniper".into()),
                (Kind::Import, "shot.wav".into()),
            ]
        );
        // With its companion and dependency on, only the import's gaps stay.
        let set = crate::packages::PackageSet {
            schema_version: 1,
            packages: vec![
                entry("sniper"),
                entry("sniper-rules"),
                entry("scope"),
                entry("v20-weapons"),
            ],
        };
        assert_eq!(check_set(&root, &set).len(), 3);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
