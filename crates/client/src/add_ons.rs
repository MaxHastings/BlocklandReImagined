//! The Add-Ons screen's host side: reads the package library under the
//! content root, turns it into display rows, and applies the player's
//! changes. Players only ever see "Add-Ons"; "package" is the code's word.
//! The mechanism (lists, dependencies, refusals) is `bri_package::library`;
//! the words and grouping here are presentation only.
use anyhow::{Context, Result};
use bri_package::classic::Discovery;
use bri_package::defaults;
use bri_package::diag::Severity;
use bri_package::library::{Library, LibraryEntry};
use bri_package::packages::Side;
use bri_ui::api::{AddOnRow, AddOnsView};
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

/// Where this game looks for the player's classic Add-Ons: the machine's
/// Steam and v20 installs, found once.
pub fn machine() -> &'static Discovery {
    static MACHINE: std::sync::OnceLock<Discovery> = std::sync::OnceLock::new();
    MACHINE.get_or_init(Discovery::machine)
}

pub fn view(root: &Path, discovery: &Discovery) -> AddOnsView {
    match Library::scan_with(root, discovery) {
        Ok(library) => AddOnsView {
            rows: rows(&library),
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
pub fn set_enabled(
    root: &Path,
    discovery: &Discovery,
    id: &str,
    enabled: bool,
) -> Result<AddOnsView> {
    let mut library = Library::scan_with(root, discovery)?;
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
        rows: rows(&library),
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
pub fn defaults(root: &Path, discovery: &Discovery) -> Result<AddOnsView> {
    let mut library = Library::scan_with(root, discovery)?;
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
        rows: rows(&library),
        notice: match (off, on) {
            (0, 0) => notice,
            _ => format!("{notice} Changes apply the next time you start a game."),
        },
    })
}

pub fn rows(library: &Library) -> Vec<AddOnRow> {
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
    for e in library
        .entries
        .iter()
        .filter(|e| !e.required && library.companion_of(e.id()).is_none())
    {
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
    // Old add-ons waiting to be imported come last.
    for l in library.legacy.iter().filter(|l| l.imported_as.is_none()) {
        out.push(AddOnRow {
            id: format!("{LEGACY}{}", l.name),
            name: l.name.clone(),
            category: LEGACY_CATEGORY.into(),
            description: format!("An old Blockland add-on from {}. Import converts your copy into an add-on this game can load; it starts off, and players who join you download it from you. Its scripts are never run: the game's own rewrites of them come with the import, and its report lists anything still missing.", l.origin.label()),
            importable: true,
            ..Default::default()
        });
    }
    out
}

/// Row ids of old add-ons waiting in the drop folder.
pub const LEGACY: &str = "legacy:";
const LEGACY_CATEGORY: &str = "Not Imported Yet";

/// Show `id` as being imported.
pub fn mark_importing(view: &mut AddOnsView, id: &str) {
    for r in view.rows.iter_mut().filter(|r| r.id == id) {
        r.importing = true;
    }
}

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

/// Start importing the old add-on behind row `id` on a worker thread. The
/// receiver yields the notice to show when it finishes.
pub fn start_import(
    root: &Path,
    discovery: &Discovery,
    id: &str,
    importer: &Path,
) -> Result<std::sync::mpsc::Receiver<Result<String>>> {
    let library = Library::scan_with(root, discovery)?;
    let name = id
        .strip_prefix(LEGACY)
        .context("That add-on is already imported.")?;
    let legacy = library
        .legacy
        .iter()
        .find(|l| l.name == name)
        .with_context(|| format!("{name} is no longer in the Add-Ons folder."))?;
    anyhow::ensure!(legacy.imported_as.is_none(), "{name} is already imported.");
    anyhow::ensure!(
        importer.is_file(),
        "The add-on importer is not installed ({}).",
        importer.display()
    );
    let dir = library.import_dir(name);
    let out = root.join(&dir);
    let input = legacy.path.clone();
    // Base bricks, sounds and the rest an Add-On builds on come from the
    // game's own converted content.
    let installed = root.to_path_buf();
    let importer = importer.to_path_buf();
    let name = name.to_string();
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut command = std::process::Command::new(&importer);
        command
            .arg(&input)
            .arg(&out)
            .arg("--installed")
            .arg(&installed)
            .arg("--json");
        let result = command
            .stdin(std::process::Stdio::null())
            .output()
            .with_context(|| format!("Running {}", importer.display()))
            .and_then(|o| {
                if o.status.success() {
                    Ok(format!(
                        "Imported {name}. It starts off; its report is {dir}/IMPORT-REPORT.md."
                    ))
                } else {
                    let err = String::from_utf8_lossy(&o.stderr);
                    let reason = err
                        .lines()
                        .rev()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("no details");
                    // Leave no half-written package behind.
                    let _ = std::fs::remove_dir_all(&out);
                    Err(anyhow::anyhow!("{name} could not be imported: {reason}"))
                }
            });
        let _ = send.send(result);
    });
    Ok(receive)
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
        let discovery = Discovery::root_only();
        let v = view(&root, &discovery);
        assert_eq!(ids(&v), [("tool_hook".to_string(), false)]);
        let v = set_enabled(&root, &discovery, "tool_hook", true).unwrap();
        assert_eq!(ids(&v), [("tool_hook".to_string(), true)]);
        // The notice speaks of the Add-On alone; the rules are part of it.
        let library = Library::scan(&root).unwrap();
        assert!(library.get("tool_hook-rules").unwrap().enabled);
        assert!(hook(&v).needed_by.is_empty(), "{:?}", hook(&v).needed_by);
        let refused = set_enabled(&root, &discovery, "tool_hook-rules", false).unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("is part of The tool_hook and turns on and off with it"),
            "{refused}"
        );
        let v = set_enabled(&root, &discovery, "tool_hook", false).unwrap();
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
        let v = view(&root, &discovery);
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
        let v = view(&root, &Discovery::root_only());
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

        let v = set_enabled(&root, &Discovery::root_only(), "creeper", true).unwrap();
        assert_eq!(
            v.notice,
            "The creeper is on. Also turned on: The lab-world. Changes apply the next time you start a game."
        );
        assert_eq!(v.rows[1].needed_by, ["The creeper"]);
        assert!(set_enabled(&root, &Discovery::root_only(), BASE_ROW, false).is_err());
        let v = defaults(&root, &Discovery::root_only()).unwrap();
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
        // An old add-on dropped in Add-Ons is offered for import, last.
        std::fs::create_dir_all(root.join("Add-Ons")).unwrap();
        std::fs::write(root.join("Add-Ons/Weapon_Shotgun.zip"), b"PK").unwrap();
        let mut v = view(&root, &Discovery::root_only());
        let last = v.rows.last().unwrap();
        assert_eq!(
            (last.id.as_str(), last.category.as_str(), last.importable),
            ("legacy:Weapon_Shotgun", "Not Imported Yet", true)
        );
        mark_importing(&mut v, "legacy:Weapon_Shotgun");
        assert!(v.rows.last().unwrap().importing);
        let missing = start_import(
            &root,
            &Discovery::root_only(),
            "legacy:Weapon_Shotgun",
            &root.join("no-importer"),
        )
        .unwrap_err();
        assert!(
            format!("{missing}").contains("importer is not installed"),
            "{missing}"
        );
        assert!(start_import(&root, &Discovery::root_only(), "creeper", &root.join("x")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
