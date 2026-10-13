//! The host's Tune window (`/tune`): a body's numbers, from the replicated
//! archetype table, and saving them into the body's Add-On file. The host
//! applies each change ([`bri_sim::session::Session::tune_archetype`]) and
//! every player gets the tuned body; this side remembers the numbers as
//! they were when the window opened (or last saved), for Undo Changes and
//! to know what Save to Add-On writes. Design: `docs/modding/tuning.md`.
use anyhow::{Context, Result, ensure};
use bri_sim::archetype::{Archetype, Archetypes};
use bri_sim::tuning;
use bri_ui::api::{TuneField, TuneView};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Two numbers this close are the same number (a slider and a file's text
/// round to f32).
fn same(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
}

pub struct TuneWindow {
    /// The body's archetype id.
    pub body: String,
    /// Every number as it was when the window opened or last saved.
    saved: BTreeMap<String, f32>,
    /// Each number's slider range, fixed while the window is open so a
    /// slider does not move under the mouse as its value changes.
    ranges: BTreeMap<String, (f32, f32)>,
    /// Numbers sent to the host that its tuned body does not show yet: a
    /// save straight after a change writes them too.
    sent: BTreeMap<String, f32>,
    /// What the last change or save did.
    pub status: String,
}

impl TuneWindow {
    /// The window over `body`, as it plays now.
    pub fn open(body: &Archetype) -> Self {
        let fields = tuning::fields(body);
        Self {
            body: body.id.clone(),
            saved: fields.iter().map(|f| (f.path.clone(), f.value)).collect(),
            ranges: fields.iter().map(|f| (f.path.clone(), f.range())).collect(),
            sent: BTreeMap::new(),
            status: String::new(),
        }
    }
    fn current<'a>(&self, archetypes: &'a Archetypes) -> Option<&'a Archetype> {
        archetypes.find(&self.body).map(|i| archetypes.resolve(i))
    }
    /// Every number as the body plays it, or as just sent to the host.
    fn values(&self, archetypes: &Archetypes) -> BTreeMap<String, f32> {
        let Some(body) = self.current(archetypes) else {
            return BTreeMap::new();
        };
        let mut values: BTreeMap<String, f32> = tuning::fields(body)
            .into_iter()
            .map(|f| (f.path, f.value))
            .collect();
        values.extend(self.sent.iter().map(|(p, v)| (p.clone(), *v)));
        values
    }
    /// The numbers changed since the window opened or last saved.
    pub fn changes(&self, archetypes: &Archetypes) -> Vec<(String, f32)> {
        self.values(archetypes)
            .into_iter()
            .filter(|(path, value)| self.saved.get(path).is_none_or(|v| !same(*v, *value)))
            .collect()
    }
    /// `changes` went to the host.
    pub fn sending(&mut self, changes: &[(String, f32)]) {
        self.sent.extend(changes.iter().cloned());
    }
    /// The host's tuned body arrived: what it shows is no longer waiting.
    pub fn settle(&mut self, archetypes: &Archetypes) {
        let Some(body) = self.current(archetypes) else {
            return;
        };
        let now: BTreeMap<String, f32> = tuning::fields(body)
            .into_iter()
            .map(|f| (f.path, f.value))
            .collect();
        self.sent
            .retain(|path, value| now.get(path).is_none_or(|v| !same(*v, *value)));
    }
    /// The host refused the last change: nothing sent is waiting any more.
    pub fn refused(&mut self, why: &str) {
        self.sent.clear();
        self.status = why.to_string();
    }
    /// What Undo Changes sends: each changed number as it was.
    pub fn undo(&mut self, archetypes: &Archetypes) -> Vec<(String, f32)> {
        let undo: Vec<(String, f32)> = self
            .changes(archetypes)
            .into_iter()
            .filter_map(|(path, _)| Some((path.clone(), *self.saved.get(&path)?)))
            .collect();
        self.sent.clear();
        undo
    }
    /// The body's numbers, and those just sent, are now its file's.
    pub fn saved(&mut self, archetypes: &Archetypes) {
        let values = self.values(archetypes);
        if !values.is_empty() {
            self.saved = values;
        }
    }
    /// The window's model; `cannot_save` says why Save to Add-On is off.
    pub fn view(&self, archetypes: &Archetypes, cannot_save: &str) -> Option<TuneView> {
        let body = self.current(archetypes)?;
        let fields = tuning::fields(body)
            .into_iter()
            .map(|f| {
                let (min, max) = self.ranges.get(&f.path).copied().unwrap_or(f.range());
                TuneField {
                    group: f.group.name().into(),
                    label: f.label,
                    value: f.value,
                    min: min.min(f.value),
                    max: max.max(f.value),
                    path: f.path,
                }
            })
            .collect();
        Some(TuneView {
            body: body.id.clone(),
            name: if body.name.is_empty() {
                body.id.clone()
            } else {
                body.name.clone()
            },
            fields,
            status: self.status.clone(),
            unsaved: !self.changes(archetypes).is_empty(),
            cannot_save: cannot_save.into(),
            movement_note: if body.movement.controller.is_some() || body.controller.is_some() {
                "This body has its own movement controller, which can set its speed and \
                 jumps itself: a Movement number here may change nothing while it is in charge."
                    .into()
            } else {
                String::new()
            },
        })
    }
}

/// The archetype file of the Add-On body `id` under the content `root`, as
/// the host's catalog loaded it.
pub fn archetype_file(
    root: &Path,
    catalog: &bri_package_runtime::Catalog,
    id: &str,
) -> Result<PathBuf> {
    let (package, file) = catalog
        .packages
        .iter()
        .find_map(|(name, p)| {
            p.assets
                .iter()
                .find(|a| {
                    a.id == id && matches!(a.kind, bri_package_runtime::content::Kind::Archetype)
                })
                .map(|a| (name.clone(), a.file.clone()))
        })
        .with_context(|| format!("{id} is not an enabled Add-On's own body"))?;
    let set = bri_package::packages::PackageSet::load_root(root)?;
    let entry = set
        .packages
        .iter()
        .find(|e| e.id == package)
        .with_context(|| format!("{package} is not in this computer's Add-On list"))?;
    let dir = bri_package::packages::package_dir(root, entry)?;
    bri_package::path::inside(&dir, &file).map_err(anyhow::Error::msg)
}

/// Write `changes` (field path, value) into the archetype file at `path`,
/// keeping the rest of its text as written. `body` is the body as it plays
/// now, for parts the file leaves out.
pub fn save(path: &Path, body: &Archetype, changes: &[(String, f32)]) -> Result<()> {
    ensure!(
        !changes.is_empty(),
        "Nothing has changed since the last save"
    );
    let text =
        std::fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
    let mut def: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("Reading {}", path.display()))?;
    let mut paths = Vec::new();
    for (field, value) in changes {
        tuning::set(&mut def, body, field, *value)?;
        paths.push(field.split('.').map(str::to_owned).collect::<Vec<_>>());
    }
    let out = bri_package::json_edit::set_paths(&text, &def, &paths)?;
    // The host checked these numbers as it applied them; check that the
    // file still reads as a body, so a save never leaves one the next game
    // cannot load.
    let check: bri_package_runtime::content::ArchetypeDef =
        serde_json::from_str(&out).context("The saved body would not load")?;
    check.validate().context("The saved body would not load")?;
    // The file is swapped whole, so a crash never leaves half of it.
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, &out).with_context(|| format!("Writing {}", partial.display()))?;
    std::fs::rename(&partial, path).with_context(|| format!("Writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_knows_what_changed_and_saves_only_that() {
        let mut table = Archetypes::default();
        let base = table.resolve(Default::default()).clone();
        let mut runner = base.clone();
        runner.id = "runner:archetype/runner".into();
        runner.name = "Runner".into();
        let id = table.add(runner.clone()).unwrap();
        let mut window = TuneWindow::open(&runner);
        let view = window.view(&table, "").unwrap();
        assert_eq!(view.name, "Runner");
        assert!(!view.unsaved);
        assert!(view.fields.iter().any(|f| f.group == "Movement"));
        // The host tunes it.
        let field = view
            .fields
            .iter()
            .find(|f| f.path.starts_with("movement."))
            .unwrap()
            .clone();
        let mut tuned = runner.clone();
        let mut json = serde_json::to_value(&tuned.movement).unwrap();
        let key = field.path.trim_start_matches("movement.");
        json[key] = serde_json::json!(f64::from(field.value) + 1.5);
        tuned.movement = serde_json::from_value(json).unwrap();
        table.replace(id, tuned.clone()).unwrap();
        let changes = window.changes(&table);
        assert_eq!(changes, [(field.path.clone(), field.value + 1.5)]);
        assert_eq!(window.undo(&table), [(field.path.clone(), field.value)]);
        assert!(window.view(&table, "").unwrap().unsaved);
        // Saved into a file that left movement out: added, the rest kept.
        let dir = std::env::temp_dir().join(format!("bri-tune-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("runner.json");
        let text =
            "{\n  \"schema_version\": 1,\n  \"name\": \"Runner\",\n  \"max_health\": 100\n}\n";
        std::fs::write(&file, text).unwrap();
        save(&file, &tuned, &changes).unwrap();
        let out = std::fs::read_to_string(&file).unwrap();
        assert!(
            out.starts_with(
                "{\n  \"schema_version\": 1,\n  \"name\": \"Runner\",\n  \"max_health\": 100,\n  \"movement\": {\n    \""
            ),
            "{out}"
        );
        let back: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(back["movement"].as_object().unwrap().len(), 1);
        window.saved(&table);
        assert!(window.changes(&table).is_empty());
        // Sent and not yet shown by the host: a save writes it anyway.
        let jump = view
            .fields
            .iter()
            .find(|f| f.path != field.path && f.path.starts_with("movement."))
            .unwrap()
            .clone();
        window.sending(&[(jump.path.clone(), jump.value + 2.0)]);
        assert_eq!(
            window.changes(&table),
            [(jump.path.clone(), jump.value + 2.0)]
        );
        // The host refused it: nothing waits, and the window says why.
        window.refused("Not tuned: too fast");
        assert!(window.changes(&table).is_empty());
        assert_eq!(
            window.view(&table, "").unwrap().status,
            "Not tuned: too fast"
        );
        // Once the table shows a sent number it is the table's.
        window.sending(&[(field.path.clone(), field.value + 1.5)]);
        window.settle(&table);
        assert!(window.changes(&table).is_empty());
        // A file the game could not read is never written.
        std::fs::write(&file, "{\n  \"name\": \"Runner\"\n}\n").unwrap();
        assert!(save(&file, &tuned, &changes).is_err());
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "{\n  \"name\": \"Runner\"\n}\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
