//! The Add-Ons screen's host side: reads the package library under the
//! content root, turns it into display rows, and applies the player's
//! changes. Players only ever see "Add-Ons"; "package" is the code's word.
//! The mechanism (lists, dependencies, refusals) is `bri_package::library`;
//! the words and grouping here are presentation only.
use anyhow::{Context, Result};
use bri_package::classic::{self, Record, State, Step};
use bri_package::defaults;
use bri_package::diag::Severity;
use bri_package::library::{Library, LibraryEntry};
use bri_package::packages::Side;
use bri_ui::api::{AddOnRow, AddOnsView};
use std::io::Read;
use std::path::Path;

/// The one row standing for every base game package.
pub const BASE_ROW: &str = "base";
const BASE_CATEGORY: &str = "Base Game";

/// Group headings by provided kind, first match wins; anything else is
/// "Other". Kinds are open-ended, so unknown ones still show.
const CATEGORIES: &[(&str, &[&str])] = &[
    (
        "Game Modes & Worlds",
        &["world", "gamemode", "mode", "map", "minigame"],
    ),
    (
        "Weapons & Items",
        &["weapons", "weapon", "item", "items", "tool"],
    ),
    ("Bricks", &["bricks", "brick", "print", "prints"]),
    ("Vehicles & Bots", &["vehicles", "vehicle", "bots", "bot"]),
    (
        "Gameplay",
        &["behaviour", "script", "entity", "event", "events"],
    ),
    (
        "Looks, Sounds & HUD",
        &[
            "hud", "ui", "model", "sound", "sounds", "texture", "avatar", "effects", "music",
        ],
    ),
];

pub fn view(root: &Path) -> AddOnsView {
    match Library::scan(root) {
        Ok(library) => AddOnsView {
            rows: rows(&library, &State::load(root)),
            notice: library
                .problems
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>()
                .join(" "),
        },
        Err(error) => AddOnsView {
            rows: vec![],
            notice: format!("The add-on list could not be read: {error:#}"),
        },
    }
}

/// List under each row the problems [`crate::add_on_health`] found with
/// that Add-On when the game last loaded, and sum them up in the notice.
pub fn with_health(view: &mut AddOnsView, health: &crate::add_on_health::AddOnHealth) {
    for row in &mut view.rows {
        for line in health.lines_for(&row.id, &row.name) {
            if !row.problems.contains(&line) {
                row.problems.push(line);
            }
        }
    }
    if let Some(summary) = health.summary() {
        view.notice = if view.notice.is_empty() {
            summary
        } else {
            format!("{summary} {}", view.notice)
        };
    }
}

/// The Can't Join rows for a join refused over differing add-ons, named as
/// the player's own list names them. `None` for any other failure.
pub fn mismatch(root: &Path, reason: &str) -> Option<bri_ui::api::AddOnMismatch> {
    let refused = bri_package::environment::parse_refusal(reason)?;
    let library = Library::scan(root).ok();
    let name = |id: &str| {
        library
            .as_ref()
            .and_then(|l| l.get(id))
            .map_or(id.to_string(), |e| e.name().to_string())
    };
    let base: Vec<String> = bri_package::packages::PackageSet::base()
        .packages
        .into_iter()
        .map(|p| p.id)
        .collect();
    let is_base = |id: &str| base.iter().any(|b| b == id);
    // Equal version numbers explain nothing: say what differs instead.
    let same: Vec<&bri_package::environment::RefusedPackage> = refused
        .iter()
        .filter(|r| r.server.is_some() && r.server == r.client)
        .collect();
    let explanation = match same.as_slice() {
        [] => String::new(),
        [r, ..] if is_base(&r.id) => format!(
            "{} has the same version on both computers but different files.              The base game is imported from each computer's own Blockland v20 folder,              so one copy came from an older build of this game or was changed after              importing. Install both computers from the same game package.",
            name(&r.id)
        ),
        [r, ..] => format!(
            "{} has the same version on both computers but different files:              one copy was edited or rebuilt without a new version number.              Use the same copy as the host.",
            name(&r.id)
        ),
    };
    Some(bri_ui::api::AddOnMismatch {
        base_game: refused.iter().any(|r| is_base(&r.id)),
        explanation,
        rows: refused
            .into_iter()
            .map(|r| bri_ui::api::MismatchRow {
                name: name(&r.id),
                server: r.server.unwrap_or_default(),
                yours: r.client.unwrap_or_default(),
            })
            .collect(),
    })
}

/// Turn one add-on on or off, with what it needs or what needs it.
pub fn set_enabled(root: &Path, id: &str, enabled: bool) -> Result<AddOnsView> {
    let mut library = Library::scan(root)?;
    anyhow::ensure!(id != BASE_ROW, "The base game stays on.");
    let plan = library.plan(id, enabled);
    if !plan.allowed() {
        anyhow::bail!(
            "{}",
            plan.refused
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    let also: Vec<String> = plan
        .also
        .iter()
        .map(|i| library.get(i).map_or(i.clone(), |e| e.name().to_string()))
        .collect();
    let name = library
        .get(id)
        .map_or(id.to_string(), |e| e.name().to_string());
    library.apply(&plan)?;
    let mut out = AddOnsView {
        rows: rows(&library, &State::load(root)),
        notice: format!("{name} is {}.", if enabled { "on" } else { "off" }),
    };
    if !also.is_empty() {
        out.notice.push_str(&format!(
            " Also turned {}: {}.",
            if enabled { "on" } else { "off" },
            also.join(", ")
        ));
    }
    out.notice
        .push_str(" Changes apply the next time you start a game.");
    Ok(out)
}

/// Back to the defaults (v20's "Default"): the base game and the default
/// Add-Ons (`packages/default-addons.json`) on, every other add-on off.
pub fn defaults(root: &Path) -> Result<AddOnsView> {
    let mut library = Library::scan(root)?;
    let (mut off, mut on) = (0, 0);
    // Dependents go with the package they need, so one pass settles it.
    // Defaults need only the base game and each other, so none goes.
    while let Some(id) = library
        .entries
        .iter()
        .find(|e| {
            e.enabled
                && !e.required
                && !defaults::is_default(e.id())
                && library.companion_of(e.id()).is_none()
        })
        .map(|e| e.id().to_string())
    {
        let plan = library.plan(&id, false);
        off += 1 + plan.also.len();
        library.apply(&plan)?;
    }
    for addon in defaults::list().iter().filter(|a| a.enabled) {
        if library.get(&addon.id).is_some_and(|e| !e.enabled) {
            let plan = library.plan(&addon.id, true);
            if plan.allowed() {
                on += 1 + plan.also.len();
                library.apply(&plan)?;
            }
        }
    }
    let plural = |n: usize| if n == 1 { "" } else { "s" };
    let notice = match (off, on) {
        (0, 0) => "Only the base game and the default Add-Ons are on.".into(),
        (off, 0) => format!("Turned off {off} add-on{}.", plural(off)),
        (0, on) => format!("Turned on {on} default add-on{}.", plural(on)),
        (off, on) => format!(
            "Turned off {off} add-on{} and turned on {on} default add-on{}.",
            plural(off),
            plural(on)
        ),
    };
    Ok(AddOnsView {
        rows: rows(&library, &State::load(root)),
        notice: match (off, on) {
            (0, 0) => notice,
            _ => format!("{notice} Changes apply the next time you start a game."),
        },
    })
}

pub fn rows(library: &Library, state: &State) -> Vec<AddOnRow> {
    let base: Vec<&LibraryEntry> = library.entries.iter().filter(|e| e.required).collect();
    let mut out = Vec::new();
    if !base.is_empty() {
        let problems: Vec<String> = base
            .iter()
            .flat_map(|e| {
                e.problems
                    .iter()
                    .map(move |d| format!("{}: {}", e.id(), d.message))
            })
            .collect();
        out.push(AddOnRow {
            id: BASE_ROW.into(),
            name: "Blockland v20".into(),
            version: String::new(),
            category: BASE_CATEGORY.into(),
            enabled: true,
            locked: true,
            runs: "Everyone. Every server and player has it.".into(),
            description: "The original game's bricks, maps, weapons, vehicles, sounds and interface, converted to run natively.".into(),
            // Player-facing text never says "package".
            provides: vec![format!("{} parts of the original game", base.len())],
            broken: base.iter().any(|e| e.has_errors()),
            problems,
            ..Default::default()
        });
    }
    // A companion (an import's host rules) is part of its Add-On's row,
    // never a row of its own: it turns on and off with it.
    for e in library.entries.iter().filter(|e| {
        !e.required && library.companion_of(e.id()).is_none() && !palette_only(library, e)
    }) {
        out.push(row(library, e));
    }
    // Base first, then categories in table order, then Other; the library's
    // order (load order, then name) inside each.
    let rank = |c: &str| {
        if c == BASE_CATEGORY {
            0
        } else {
            CATEGORIES
                .iter()
                .position(|(n, _)| *n == c)
                .map_or(usize::MAX, |i| i + 1)
        }
    };
    out.sort_by_key(|r| rank(&r.category));
    // Classic Add-Ons dropped in the Add-Ons folder and not converted yet
    // come last: converting, or why they could not be.
    for l in library.legacy.iter().filter(|l| l.imported_as.is_none()) {
        let failed = state.failed(l);
        out.push(AddOnRow {
            id: format!("{LEGACY}{}", l.name),
            name: l.name.clone(),
            category: if failed.is_some() { FAILED_CATEGORY } else { LEGACY_CATEGORY }.into(),
            description: match failed {
                Some(_) => "A classic Blockland Add-On in your Add-Ons folder that could not be converted. Replace it with a working copy and it converts by itself, or press Retry.".into(),
                None => "A classic Blockland Add-On in your Add-Ons folder, being converted into an Add-On this game can load. It starts off; players who join you download it from you. Its scripts are never run: the game's own rewrites of them come with it, and its report lists anything still missing.".into(),
            },
            problems: failed.map(|e| vec![e.to_string()]).unwrap_or_default(),
            broken: failed.is_some(),
            importable: failed.is_some(),
            importing: failed.is_none(),
            ..Default::default()
        });
    }
    out
}

/// Imported palette-only content is selected in Colorsets, independently of
/// Add-On switches. Keep mixed content and damaged imports visible here.
fn palette_only(library: &Library, entry: &LibraryEntry) -> bool {
    let Some(info) = &entry.info else {
        return false;
    };
    if entry.has_errors()
        || entry.package.role.is_some()
        || !info.provides.is_empty()
        || !info.capabilities.is_empty()
        || !info.companions.is_empty()
        || !info.dependencies.is_empty()
        || !info.optional_dependencies.is_empty()
        || info.client.is_some()
    {
        return false;
    }
    let Ok(dir) = bri_package::packages::package_dir(library.root(), &entry.package) else {
        return false;
    };
    if crate::colorsets::read(&dir.join("colorSet.txt")).is_err() {
        return false;
    }
    let Ok(file) = std::fs::File::open(dir.join("assets/content.json")) else {
        return false;
    };
    let mut bytes = Vec::new();
    if file.take(65_537).read_to_end(&mut bytes).is_err() || bytes.len() > 65_536 {
        return false;
    }
    let Ok(index) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    let Some(content) = index.get("content").and_then(|c| c.as_array()) else {
        return false;
    };
    index.get("schema_version").and_then(|v| v.as_u64()) == Some(1)
        && content.len() == 1
        && content[0].get("kind").and_then(|v| v.as_str()) == Some("asset")
        && content[0]
            .get("file")
            .and_then(|v| v.as_str())
            .is_some_and(|file| file.eq_ignore_ascii_case("colorSet.txt"))
}

/// Row ids of classic Add-Ons in the drop folder not converted yet.
pub const LEGACY: &str = "legacy:";
const LEGACY_CATEGORY: &str = "Converting";
const FAILED_CATEGORY: &str = "Could Not Convert";

/// The importer ships next to the game (`bri-import-addon`). It is a
/// separate program so conversion tooling stays out of the game itself.
pub fn importer() -> Result<std::path::PathBuf> {
    let exe = std::env::current_exe()?;
    let path = exe.with_file_name(format!("bri-import-addon{}", std::env::consts::EXE_SUFFIX));
    anyhow::ensure!(
        path.is_file(),
        "The add-on importer is not installed next to the game ({}).",
        path.display()
    );
    Ok(path)
}

/// Progress of [`start_sync`]: a notice for the Add-Ons screen, the last
/// one with `finished`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncNote {
    pub notice: String,
    pub finished: bool,
}

/// Bring the conversions in line with the Add-Ons folder on a worker
/// thread ([`classic::plan`]): convert what was dropped or changed, remove
/// what was taken out. The receiver yields its progress.
pub fn start_sync(root: &Path, importer: &Path) -> Result<std::sync::mpsc::Receiver<SyncNote>> {
    let steps = classic::plan(&Library::scan(root)?, &State::load(root));
    let (send, receive) = std::sync::mpsc::channel();
    let (root, importer) = (root.to_path_buf(), importer.to_path_buf());
    std::thread::spawn(move || {
        let run = |input: &Path, out: &Path, reference: &Path| {
            run_importer(&importer, input, out, reference)
        };
        let notice = sync(&root, &run, steps, &send);
        let _ = send.send(SyncNote {
            notice,
            finished: true,
        });
    });
    Ok(receive)
}

fn run_importer(importer: &Path, input: &Path, out: &Path, reference: &Path) -> Result<()> {
    let output = std::process::Command::new(importer)
        .arg(input)
        .arg(out)
        .arg("--json")
        // The other dropped Add-Ons, for one it requires, and the game's
        // own content for the base bricks, sounds and the rest.
        .arg("--reference")
        .arg(reference)
        .arg("--installed")
        .arg(reference)
        .stdin(std::process::Stdio::null())
        .output()
        .with_context(|| format!("Running {}", importer.display()))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let reason = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("no details");
        anyhow::bail!("{}", reason.trim());
    }
    Ok(())
}

/// Convert `name` again: forget that it failed.
pub fn retry(root: &Path, id: &str) -> Result<()> {
    let name = id
        .strip_prefix(LEGACY)
        .context("That add-on is already converted.")?;
    let mut state = State::load(root);
    state.forget(name);
    state.save(root)
}

/// Runs the importer: the dropped Add-On, the package folder to make, and
/// the folder whose `Add-Ons/` it may build on.
type Run<'a> = dyn Fn(&Path, &Path, &Path) -> Result<()> + 'a;

fn sync(
    root: &Path,
    run: &Run,
    steps: Vec<Step>,
    send: &std::sync::mpsc::Sender<SyncNote>,
) -> String {
    let (mut converted, mut failed, mut removed) = (vec![], vec![], vec![]);
    let mut state = State::load(root);
    for step in steps {
        match step {
            Step::Adopt { name, id, stamp } => {
                let dir = Library::scan(root)
                    .ok()
                    .and_then(|l| Some(l.get(&id)?.package.dir.clone()));
                state.set(Record {
                    name,
                    stamp,
                    id: Some(id),
                    dir,
                    error: None,
                });
            }
            Step::Remove { name, id, .. } => {
                if !id.is_empty()
                    && let Err(error) = Library::scan(root).and_then(|mut l| l.uninstall(&id))
                {
                    bri_console::warn(format!("Removing {name}'s conversion: {error:#}"));
                    continue;
                }
                state.forget(&name);
                removed.push(name);
            }
            Step::Import {
                name,
                path,
                stamp,
                replaces,
            } => {
                let _ = send.send(SyncNote {
                    notice: format!("Converting {name}..."),
                    finished: false,
                });
                let mut record = Record {
                    name: name.clone(),
                    stamp,
                    id: None,
                    dir: None,
                    error: None,
                };
                match convert(root, run, &name, &path, replaces.as_deref()) {
                    Ok((id, dir)) => {
                        record.id = Some(id);
                        record.dir = Some(dir);
                        converted.push(name);
                    }
                    Err(error) => {
                        record.error = Some(format!("{error:#}"));
                        failed.push(name);
                    }
                }
                state.set(record);
            }
        }
        // Saved after every step, so a game closed midway redoes nothing.
        if let Err(error) = state.save(root) {
            bri_console::warn(format!(
                "Saving the Add-Ons folder's conversions: {error:#}"
            ));
        }
    }
    let list = |names: &[String]| names.join(", ");
    let mut notice = vec![];
    match converted.len() {
        0 => {}
        1 => notice.push(format!("Converted {}. It starts off.", converted[0])),
        n => notice.push(format!(
            "Converted {n} classic Add-Ons: {}. They start off.",
            list(&converted)
        )),
    }
    if !failed.is_empty() {
        notice.push(format!("Could not convert {}.", list(&failed)));
    }
    if !removed.is_empty() {
        notice.push(format!(
            "Removed {}, no longer in the Add-Ons folder.",
            list(&removed)
        ));
    }
    notice.join(" ")
}

/// Convert the dropped Add-On `name` at `input` with the importer, first
/// removing `replaces`, its earlier conversion (kept on if it was on). The
/// other Add-Ons in the folder are its reference, so one it requires
/// (`ForceRequiredAddOn`) is found. Its id and folder.
fn convert(
    root: &Path,
    run: &Run,
    name: &str,
    input: &Path,
    replaces: Option<&str>,
) -> Result<(String, String)> {
    let mut was_on = false;
    if let Some(old) = replaces {
        let mut library = Library::scan(root)?;
        was_on = library.get(old).is_some_and(|e| e.enabled);
        library.uninstall(old)?;
    }
    let dir = Library::scan(root)?.import_dir(name);
    let out = root.join(&dir);
    if let Err(error) = run(input, &out, root) {
        // Leave no half-written package behind.
        let _ = std::fs::remove_dir_all(&out);
        let mut rules = out.as_os_str().to_owned();
        rules.push("-rules");
        let _ = std::fs::remove_dir_all(std::path::PathBuf::from(rules));
        return Err(error);
    }
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(out.join("package.json")).context("the conversion has no package.json")?,
    )?;
    let id = manifest["id"]
        .as_str()
        .context("the conversion's package.json has no id")?
        .to_string();
    if was_on {
        let mut library = Library::scan(root)?;
        let plan = library.plan(&id, true);
        if plan.allowed() {
            library.apply(&plan)?;
        }
    }
    Ok((id, dir))
}

fn row(library: &Library, e: &LibraryEntry) -> AddOnRow {
    let info = e.info.clone().unwrap_or_default();
    let kinds = info.kinds();
    let category = CATEGORIES
        .iter()
        .find(|(_, members)| kinds.iter().any(|(k, _)| members.contains(&k.as_str())))
        .map_or("Other", |(name, _)| name);
    let name_of = |id: &str| {
        library
            .get(id)
            .map_or(id.to_string(), |d| d.name().to_string())
    };
    AddOnRow {
        id: e.id().into(),
        name: e.name().into(),
        version: e.package.version.clone(),
        category: category.into(),
        enabled: e.enabled,
        locked: false,
        runs: match e.package.side {
            Side::Server => "Only on the server you host. Players never download it.",
            Side::Shared => "Everyone in the game. Players download it when they join.",
            Side::Client => "Just you. Other players don't need it.",
        }
        .into(),
        description: info.description.clone(),
        authors: info.authors.join(", "),
        license: info.license.clone(),
        source: match info.source() {
            Some("original") | None => String::new(),
            Some(s) => format!("From {}", s.split(", sha256").next().unwrap_or(s)),
        },
        provides: kinds
            .iter()
            .map(|(k, n)| {
                if *n == 1 {
                    format!("1 {k}")
                } else {
                    format!("{n} {k}s")
                }
            })
            .collect(),
        needs: info
            .dependencies
            .iter()
            .map(|(id, requirement)| format!("{} {requirement}", name_of(id)))
            .collect(),
        needed_by: if e.enabled {
            library
                .dependents(e.id())
                .iter()
                .filter(|i| library.companion_of(i) != Some(e.id()))
                .map(|i| name_of(i))
                .collect()
        } else {
            vec![]
        },
        allowed: info
            .capabilities
            .iter()
            .map(|c| bri_package::capability::describe(c).map_or(c.clone(), str::to_string))
            .collect(),
        // Its companions' problems are its own.
        problems: std::iter::once(e)
            .chain(info.companions.iter().filter_map(|c| library.get(c)))
            .flat_map(|e| &e.problems)
            .map(|d| match &d.hint {
                Some(h) if d.severity == Severity::Error => format!("{} ({h})", d.message),
                _ => d.message.clone(),
            })
            .collect(),
        broken: std::iter::once(e)
            .chain(info.companions.iter().filter_map(|c| library.get(c)))
            .any(LibraryEntry::has_errors),
        importable: false,
        importing: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn palette_only_imports_belong_to_colorsets_but_mixed_content_stays_visible() {
        let root = std::env::temp_dir().join(format!("bri-palette-rows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("packages.json"),
            json!({"schema_version": 1, "packages": []}).to_string(),
        )
        .unwrap();
        for (id, extra_asset, client_code, palette) in [
            ("unfamiliar_palette", false, false, "255 0 0 255"),
            ("mixed_assets", true, false, "0 255 0 255"),
            ("with_client", false, true, "0 0 255 255"),
            ("broken_palette", false, false, "not a palette"),
        ] {
            let dir = root.join("addons").join(id);
            std::fs::create_dir_all(dir.join("assets")).unwrap();
            let mut info = json!({"schema_version": 1, "id": id, "version": "1.0.0",
                "api": 1, "name": id, "provides": [], "capabilities": []});
            if client_code {
                info["client"] = json!({"module": "ui.wasm"});
            }
            std::fs::write(dir.join("package.json"), info.to_string()).unwrap();
            std::fs::write(dir.join("colorSet.txt"), palette).unwrap();
            let mut content = vec![json!({"file": "colorSet.txt", "kind": "asset"})];
            if extra_asset {
                content.push(json!({"file": "model.dts", "kind": "asset"}));
            }
            std::fs::write(
                dir.join("assets/content.json"),
                json!({"schema_version": 1, "content": content}).to_string(),
            )
            .unwrap();
        }
        let rows = view(&root).rows;
        assert!(!rows.iter().any(|r| r.id == "unfamiliar_palette"));
        for id in ["mixed_assets", "with_client", "broken_palette"] {
            assert!(rows.iter().any(|r| r.id == id), "missing {id}");
        }
        let choices = crate::colorsets::discover(&root, &root.join("player"));
        assert!(choices.iter().any(|c| c.id == "addon:unfamiliar_palette"));
        std::fs::remove_dir_all(root).unwrap();
    }

    /// An import's host rules (its companion) are part of its row: no row
    /// or switch of their own, their problems shown on it, on and off with
    /// it, and refused when asked for alone.
    #[test]
    fn host_rules_are_part_of_their_add_ons_row() {
        let root = std::env::temp_dir().join(format!("bri-add-ons-rules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("packages.json"),
            json!({ "schema_version": 1, "packages": [] }).to_string(),
        )
        .unwrap();
        for (id, extra) in [
            ("tool_hook", json!({ "companions": ["tool_hook-rules"] })),
            (
                "tool_hook-rules",
                json!({ "dependencies": { "tool_hook": "=1.0.0" } }),
            ),
        ] {
            let dir = root.join("addons").join(id);
            std::fs::create_dir_all(&dir).unwrap();
            let kind = if id.ends_with("-rules") {
                "behaviour"
            } else {
                "weapons"
            };
            let mut manifest = json!({ "schema_version": 1, "id": id, "version": "1.0.0", "api": 1,
                "name": format!("The {id}"), "license": "CC0-1.0", "authors": ["Lab"],
                "capabilities": [], "dependencies": {},
                "provides": [{ "kind": kind, "id": format!("{id}:{kind}/a"), "file": "a.json" }] });
            for (k, v) in extra.as_object().unwrap() {
                manifest[k] = v.clone();
            }
            std::fs::write(dir.join("package.json"), manifest.to_string()).unwrap();
        }
        let ids = |v: &AddOnsView| -> Vec<(String, bool)> {
            v.rows
                .iter()
                .filter(|r| r.id != BASE_ROW)
                .map(|r| (r.id.clone(), r.enabled))
                .collect()
        };
        let hook = |v: &AddOnsView| v.rows.iter().find(|r| r.id == "tool_hook").unwrap().clone();
        let v = view(&root);
        assert_eq!(ids(&v), [("tool_hook".to_string(), false)]);
        let v = set_enabled(&root, "tool_hook", true).unwrap();
        assert_eq!(ids(&v), [("tool_hook".to_string(), true)]);
        // The notice speaks of the Add-On alone; the rules are part of it.
        let library = Library::scan(&root).unwrap();
        assert!(library.get("tool_hook-rules").unwrap().enabled);
        assert!(hook(&v).needed_by.is_empty(), "{:?}", hook(&v).needed_by);
        let refused = set_enabled(&root, "tool_hook-rules", false).unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("is part of The tool_hook and turns on and off with it"),
            "{refused}"
        );
        let v = set_enabled(&root, "tool_hook", false).unwrap();
        assert_eq!(ids(&v), [("tool_hook".to_string(), false)]);
        assert!(
            !Library::scan(&root)
                .unwrap()
                .get("tool_hook-rules")
                .unwrap()
                .enabled
        );
        // A problem with the rules shows on its Add-On's row.
        std::fs::write(
            root.join("addons/tool_hook-rules/package.json"),
            json!({ "schema_version": 1, "id": "tool_hook-rules", "version": "1.0.0", "api": 1,
                "dependencies": { "tool_hook": "=1.0.0", "not_installed": "*" }, "capabilities": [],
                "provides": [{ "kind": "behaviour", "id": "tool_hook-rules:behaviour/a", "file": "a.json" }] })
            .to_string(),
        )
        .unwrap();
        let v = view(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(ids(&v), [("tool_hook".to_string(), false)]);
        assert!(
            hook(&v)
                .problems
                .iter()
                .any(|p| p.contains("not_installed")),
            "{:?}",
            hook(&v)
        );
    }

    #[test]
    fn rows_group_explain_and_toggle() {
        let root = std::env::temp_dir().join(format!("bri-add-ons-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("ui-pack-004")).unwrap();
        std::fs::write(
            root.join("packages.json"),
            json!({ "schema_version": 1, "packages": [
                { "id": "v20-ui", "version": "3.0.0", "side": "client", "dir": "ui-pack-004", "role": "ui_pack" }
            ]})
            .to_string(),
        )
        .unwrap();
        for (dir, id, deps, kind, caps) in [
            ("mods/world", "lab-world", json!({}), "world", json!([])),
            (
                "mods/creeper",
                "creeper",
                json!({ "lab-world": "^1.0" }),
                "entity",
                json!(["entity", "damage"]),
            ),
        ] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(
                root.join(dir).join("package.json"),
                json!({ "schema_version": 1, "id": id, "version": "1.0.0", "api": 1,
                    "name": format!("The {id}"), "license": "CC0-1.0", "authors": ["Lab"],
                    "dependencies": deps, "capabilities": caps,
                    "provides": [{ "kind": kind, "id": format!("{id}:{kind}/a"), "file": "a.json" }] })
                .to_string(),
            )
            .unwrap();
        }
        let v = view(&root);
        let names: Vec<_> = v
            .rows
            .iter()
            .map(|r| (r.category.as_str(), r.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("Base Game", "Blockland v20"),
                ("Game Modes & Worlds", "The lab-world"),
                ("Gameplay", "The creeper"),
            ]
        );
        let creeper = &v.rows[2];
        assert_eq!(
            creeper.allowed,
            [
                "spawn and move its own creatures and objects",
                "hurt and heal players and break bricks"
            ]
        );
        assert_eq!(creeper.needs, ["The lab-world ^1.0"]);
        assert!(creeper.runs.starts_with("Only on the server"));

        let v = set_enabled(&root, "creeper", true).unwrap();
        assert_eq!(
            v.notice,
            "The creeper is on. Also turned on: The lab-world. Changes apply the next time you start a game."
        );
        assert_eq!(v.rows[1].needed_by, ["The creeper"]);
        assert!(set_enabled(&root, BASE_ROW, false).is_err());
        let v = defaults(&root).unwrap();
        assert!(v.rows.iter().all(|r| r.locked || !r.enabled), "{v:?}");
        assert!(v.notice.starts_with("Turned off 2 add-ons"), "{}", v.notice);
        // A refused join names add-ons as the player's list does.
        let m = mismatch(
            &root,
            "Joining: Your content does not match the server: server has lab-world 1.0.0 (aaaa), you do not",
        )
        .unwrap();
        assert_eq!(m.rows[0].name, "The lab-world");
        assert_eq!(
            (m.rows[0].server.as_str(), m.rows[0].yours.as_str()),
            ("1.0.0", "")
        );
        assert!(m.explanation.is_empty() && !m.base_game);
        // The same version with different files is said in words.
        let hash = |c: char| c.to_string().repeat(64);
        let m = mismatch(
            &root,
            &format!(
                "Your content does not match the server: server has v20-map-bundle 16.0.0 ({}), you have v20-map-bundle 16.0.0 ({})",
                hash('a'),
                hash('b')
            ),
        )
        .unwrap();
        assert!(m.base_game);
        assert!(
            m.explanation.starts_with(
                "v20-map-bundle has the same version on both computers but different files."
            ),
            "{}",
            m.explanation
        );
        assert!(mismatch(&root, "Timed out").is_none());
        // A classic Add-On dropped in Add-Ons shows as converting, last.
        std::fs::create_dir_all(root.join("Add-Ons")).unwrap();
        std::fs::write(root.join("Add-Ons/Weapon_Shotgun.zip"), b"PK").unwrap();
        let v = view(&root);
        let last = v.rows.last().unwrap();
        assert_eq!(
            (last.id.as_str(), last.category.as_str(), last.importing),
            ("legacy:Weapon_Shotgun", "Converting", true)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A stand-in importer: a weapons package named for the dropped file,
    /// or a refusal for one whose bytes say `bad`.
    fn fake_import(input: &Path, out: &Path, reference: &Path) -> Result<()> {
        assert!(
            reference.join("Add-Ons").is_dir(),
            "the drop folder is the reference"
        );
        anyhow::ensure!(std::fs::read(input)? != b"bad", "not a zip file");
        let name = input.file_stem().unwrap().to_string_lossy().to_string();
        let id = name.to_ascii_lowercase();
        std::fs::create_dir_all(out)?;
        std::fs::write(
            out.join("package.json"),
            json!({ "schema_version": 1, "id": id, "version": "1.0.0", "api": 1,
                "provides": [{ "kind": "weapons", "id": format!("{id}:weapons/w"), "file": "w.json" }],
                "provenance": { "source": format!("Blockland Add-On {name} (zip), sha256 00") } })
            .to_string(),
        )?;
        Ok(())
    }

    fn sync_now(root: &Path) -> Vec<SyncNote> {
        let steps = classic::plan(&Library::scan(root).unwrap(), &State::load(root));
        let (send, receive) = std::sync::mpsc::channel();
        let notice = sync(root, &fake_import, steps, &send);
        drop(send);
        let mut notes: Vec<SyncNote> = receive.iter().collect();
        notes.push(SyncNote {
            notice,
            finished: true,
        });
        notes
    }

    #[test]
    fn the_add_ons_folder_converts_reconverts_and_removes_by_itself() {
        let root = std::env::temp_dir().join(format!("bri-add-ons-sync-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let drop = classic::folder(&root);
        std::fs::create_dir_all(&drop).unwrap();
        std::fs::write(drop.join("Weapon_Gun.zip"), b"PK one").unwrap();
        std::fs::write(drop.join("Weapon_Bad.zip"), b"bad").unwrap();
        let notes = sync_now(&root);
        assert_eq!(
            notes.last().unwrap().notice,
            "Converted Weapon_Gun. It starts off. Could not convert Weapon_Bad."
        );
        assert!(notes.iter().any(|n| n.notice == "Converting Weapon_Gun..."));
        let v = view(&root);
        let gun = v.rows.iter().find(|r| r.id == "weapon_gun").unwrap();
        assert!(!gun.enabled);
        let bad = v.rows.iter().find(|r| r.id == "legacy:Weapon_Bad").unwrap();
        assert_eq!(
            (bad.category.as_str(), bad.importable, bad.importing),
            ("Could Not Convert", true, false)
        );
        assert_eq!(bad.problems, ["not a zip file"]);
        // Nothing changed: nothing to do, and the failure is not retried.
        assert_eq!(sync_now(&root).last().unwrap().notice, "");
        // Retry, after fixing it.
        std::fs::write(drop.join("Weapon_Bad.zip"), b"PK fixed").unwrap();
        retry(&root, "legacy:Weapon_Bad").unwrap();
        assert_eq!(
            sync_now(&root).last().unwrap().notice,
            "Converted Weapon_Bad. It starts off."
        );
        // A changed zip converts again and stays on if it was on.
        set_enabled(&root, "weapon_gun", true).unwrap();
        std::fs::write(drop.join("Weapon_Gun.zip"), b"PK two, longer").unwrap();
        assert_eq!(
            sync_now(&root).last().unwrap().notice,
            "Converted Weapon_Gun. It starts off."
        );
        let library = Library::scan(&root).unwrap();
        let gun = library.get("weapon_gun").unwrap();
        assert!(gun.enabled);
        assert_eq!(gun.package.dir, "addons/weapon_gun");
        // Taken out of the folder, its conversion goes.
        std::fs::remove_file(drop.join("Weapon_Gun.zip")).unwrap();
        assert_eq!(
            sync_now(&root).last().unwrap().notice,
            "Removed Weapon_Gun, no longer in the Add-Ons folder."
        );
        let library = Library::scan(&root).unwrap();
        assert!(library.get("weapon_gun").is_none());
        assert!(!root.join("addons/weapon_gun").exists());
        assert!(library.get("weapon_bad").is_some());
        let _ = std::fs::remove_dir_all(&root);
    }
}
