//! Dials: a kind's numbers read and set by their path in `bots.json`
//! (`surprise.strength`, `behaviours.interact`, `sight`), and the
//! user-local override file the host keeps them in (`/botset`, `/botsave`).
//!
//! Nothing here names a dial: any number a kind's `bots.json` has is one,
//! so a dial a lane adds to the schema can be read and set as soon as it
//! exists. A set value goes through the kind's own validation.
use super::BotKind;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const OVERRIDES_SCHEMA_VERSION: u32 = 1;
/// The override file's name in the user's data directory (beside
/// `settings.json`).
pub const OVERRIDES_FILE: &str = "bot-overrides.json";
/// Most dial overrides one file holds, over every kind.
pub const MAX_OVERRIDES: usize = 512;
const MAX_PATH: usize = 128;

/// The number at `path` (dot separated) in `kind`, if it has one there.
pub fn dial(kind: &BotKind, path: &str) -> Option<f64> {
    let value = serde_json::to_value(kind).ok()?;
    let mut at = &value;
    for part in path.split('.') {
        at = at.get(part)?;
    }
    at.as_f64()
}

/// `kind` with the number at `path` set to `value`, validated as a kind.
/// A path must lead to a number, or name a new entry of a map of numbers
/// (`behaviours.chase`).
pub fn with_dial(kind: &BotKind, path: &str, value: f64) -> Result<BotKind> {
    ensure!(value.is_finite(), "{path}: {value} is not a number");
    ensure!(
        settable(path),
        "{}: `{path}` is fixed in code; a dial is one of {}",
        kind.id,
        SETTABLE.join(", ")
    );
    ensure!(
        !path.is_empty() && path.len() <= MAX_PATH,
        "A dial path is 1 to {MAX_PATH} characters"
    );
    let mut root = serde_json::to_value(kind)?;
    let parts: Vec<&str> = path.split('.').collect();
    let (leaf, parents) = parts.split_last().context("empty dial path")?;
    let mut at = &mut root;
    for part in parents {
        at = at
            .get_mut(*part)
            .filter(|v| v.is_object())
            .with_context(|| format!("{}: no dial `{path}`", kind.id))?;
    }
    let map = at
        .as_object_mut()
        .with_context(|| format!("{}: no dial `{path}`", kind.id))?;
    match map.get(*leaf) {
        Some(old) if old.is_number() => {}
        Some(_) => bail!("{}: `{path}` is not a number", kind.id),
        // A new entry only in a map whose entries are all numbers (a
        // struct that is all numbers refuses an unknown one below).
        None if map.values().all(|v| v.is_number()) => {}
        None => bail!("{}: no dial `{path}`", kind.id),
    }
    let number = serde_json::Number::from_f64(value).context("not a number")?;
    map.insert(leaf.to_string(), serde_json::Value::Number(number));
    let out: BotKind = serde_json::from_value(root)
        .with_context(|| format!("{}: `{path}` cannot be {value}", kind.id))?;
    out.validate()
        .with_context(|| format!("{}: `{path}` cannot be {value}", kind.id))?;
    Ok(out)
}

/// The numbers a kind may set, in `bots.json` and as a dial: what the kind
/// is (its sight and ranges, behaviour and goof weights, its melee hit) and
/// the six main dials: surprise, teamwork, mood pull, alertness, hold time
/// and pressure. Every other number is fixed in code (`BotKind::default`),
/// so tuning a kind means turning a main dial. A path ending in `.` covers
/// every entry under it.
pub const SETTABLE: [&str; 15] = [
    "sight",
    "wander_radius",
    "chase_radius",
    "objective_radius",
    "out_of_water_seconds",
    "behaviours.",
    "melee.",
    "surprise.flavours.",
    "surprise.strength",
    "team.teamwork",
    "team.mood",
    "perception.strength",
    "hold_seconds",
    "team.pressure",
    "extras.strength",
];

/// Whether a kind may set the number at `path` ([`SETTABLE`]).
pub fn settable(path: &str) -> bool {
    SETTABLE.iter().any(|p| {
        if p.ends_with('.') {
            path.starts_with(p)
        } else {
            path == *p
        }
    })
}

/// Every settable number `kind` has, by path, in schema order.
pub fn dials(kind: &BotKind) -> Vec<(String, f64)> {
    serde_json::to_value(kind)
        .map(|value| numbers(&value))
        .unwrap_or_default()
        .into_iter()
        .filter(|(path, _)| settable(path))
        .collect()
}

/// Every number in `value`, by dot-separated path, in order.
pub(super) fn numbers(value: &serde_json::Value) -> Vec<(String, f64)> {
    fn walk(prefix: &str, value: &serde_json::Value, out: &mut Vec<(String, f64)>) {
        match value {
            serde_json::Value::Number(n) => {
                if let Some(n) = n.as_f64() {
                    out.push((prefix.to_string(), n));
                }
            }
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    let path = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    walk(&path, v, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk("", value, &mut out);
    out
}

/// Dial values over the shipped kinds, by kind id and path: what
/// `/botset` changed and `/botsave` keeps (`bot-overrides.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    pub schema_version: u32,
    pub kinds: BTreeMap<String, BTreeMap<String, f64>>,
}
impl Overrides {
    pub fn new() -> Self {
        Self {
            schema_version: OVERRIDES_SCHEMA_VERSION,
            kinds: BTreeMap::new(),
        }
    }
    pub fn len(&self) -> usize {
        self.kinds.values().map(BTreeMap::len).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        let out: Self = serde_json::from_slice(bytes).context("bot overrides")?;
        ensure!(
            out.schema_version == OVERRIDES_SCHEMA_VERSION,
            "bot overrides schema_version must be {OVERRIDES_SCHEMA_VERSION}"
        );
        ensure!(out.len() <= MAX_OVERRIDES, "Too many bot overrides");
        Ok(out)
    }
    /// The file at `path`; none there is no overrides.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => {
                ensure!(bytes.len() <= 256 * 1024, "{}: too large", path.display());
                Self::from_json(&bytes).with_context(|| path.display().to_string())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::new()),
            Err(e) => Err(e).with_context(|| path.display().to_string()),
        }
    }
    /// Writes the file (its folder made if needed), replacing it whole.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
    pub fn set(&mut self, kind: &str, path: &str, value: f64) {
        self.kinds
            .entry(kind.to_string())
            .or_default()
            .insert(path.to_string(), value);
    }
    /// `kinds` with every override applied. One that no longer fits (a
    /// kind or dial gone, a value out of range) is left out and reported.
    pub fn apply(&self, kinds: Vec<BotKind>) -> (Vec<BotKind>, Vec<String>) {
        let mut problems = Vec::new();
        let mut out = kinds;
        for (id, dials) in &self.kinds {
            let Some(kind) = out.iter_mut().find(|k| &k.id == id) else {
                problems.push(format!("no bot kind `{id}`"));
                continue;
            };
            for (path, value) in dials {
                match with_dial(kind, path, *value) {
                    Ok(changed) => *kind = changed,
                    Err(e) => problems.push(format!("{e:#}")),
                }
            }
        }
        (out, problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind() -> BotKind {
        BotKind {
            id: "test:bot/a".into(),
            name: "A".into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_dial_is_read_and_set_by_its_path() {
        let k = kind();
        assert!((dial(&k, "surprise.strength").unwrap() - 0.6).abs() < 1e-6);
        let k = with_dial(&k, "surprise.strength", 0.3).unwrap();
        assert!((k.surprise.strength - 0.3).abs() < 1e-6);
        assert!((dial(&k, "surprise.strength").unwrap() - 0.3).abs() < 1e-6);
        // A top-level number, and a new entry of a map of numbers.
        let k = with_dial(&k, "sight", 40.0).unwrap();
        assert_eq!(k.sight, 40.0);
        let k = with_dial(&k, "behaviours.objective", 1.0).unwrap();
        let k = with_dial(&k, "behaviours.chase", 0.0).unwrap();
        assert_eq!(k.behaviours["chase"], 0.0);
        assert!(dials(&k).iter().any(|(p, _)| p == "team.teamwork"));
        assert!(dials(&k).iter().any(|(p, _)| p == "hold_seconds"));
        assert!(!dials(&k).iter().any(|(p, _)| p == "team.mood_cap"));
    }

    #[test]
    fn a_bad_dial_is_refused_and_changes_nothing() {
        let k = kind();
        assert!(with_dial(&k, "surprise.nonsense", 1.0).is_err(), "unknown");
        assert!(with_dial(&k, "name", 1.0).is_err(), "not a number");
        assert!(with_dial(&k, "surprise.strength", f64::NAN).is_err());
        assert!(
            with_dial(&k, "surprise.strength", 7.0).is_err(),
            "out of range"
        );
        assert!(
            with_dial(&k, "behaviours.dance", 1.0).is_err(),
            "no such behaviour"
        );
    }

    #[test]
    fn overrides_round_trip_and_report_what_no_longer_fits() {
        let dir = std::env::temp_dir().join(format!("bri-bot-overrides-{}", std::process::id()));
        let path = dir.join(OVERRIDES_FILE);
        let mut o = Overrides::new();
        o.set("test:bot/a", "surprise.strength", 0.5);
        o.set("test:bot/a", "surprise.nonsense", 1.0);
        o.set("test:bot/gone", "sight", 10.0);
        o.save(&path).unwrap();
        let back = Overrides::load(&path).unwrap();
        assert_eq!(back, o);
        let (kinds, problems) = back.apply(vec![kind()]);
        assert!((kinds[0].surprise.strength - 0.5).abs() < 1e-6);
        assert_eq!(problems.len(), 2, "{problems:?}");
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(Overrides::load(&path).unwrap().is_empty(), "no file, none");
    }
}
