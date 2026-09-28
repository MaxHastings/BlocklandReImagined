//! The Add-Ons screen's host side: reads the package library under the
//! content root, turns it into display rows, and applies the player's
//! changes. Players only ever see "Add-Ons"; "package" is the code's word.
//! The mechanism (lists, dependencies, refusals) is `bri_package::library`;
//! the words and grouping here are presentation only.
use anyhow::Result;
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
    ("Vehicles", &["vehicles", "vehicle"]),
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

/// What each capability lets a package do, in a player's words.
const ALLOWED: &[(&str, &str)] = &[
    ("world.edit", "change the world's bricks"),
    ("damage", "hurt players and break bricks"),
    ("entity", "spawn and move its own creatures and objects"),
    ("chat", "send chat messages"),
    ("players", "move and respawn players"),
];

pub fn view(root: &Path) -> AddOnsView {
    match Library::scan(root) {
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
    Some(bri_ui::api::AddOnMismatch {
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

/// Turn off every add-on outside the base game (v20's "Default").
pub fn defaults(root: &Path) -> Result<AddOnsView> {
    let mut library = Library::scan(root)?;
    let mut count = 0;
    // Dependents go with the package they need, so one pass settles it.
    while let Some(id) = library
        .entries
        .iter()
        .find(|e| e.enabled && !e.required)
        .map(|e| e.id().to_string())
    {
        let plan = library.plan(&id, false);
        count += 1 + plan.also.len();
        library.apply(&plan)?;
    }
    Ok(AddOnsView {
        rows: rows(&library),
        notice: match count {
            0 => "Only the base game is on.".into(),
            n => format!(
                "Turned off {n} add-on{}. Changes apply the next time you start a game.",
                if n == 1 { "" } else { "s" }
            ),
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
            provides: vec![format!("{} base packages", base.len())],
            broken: base.iter().any(|e| e.has_errors()),
            problems,
            ..Default::default()
        });
    }
    for e in library.entries.iter().filter(|e| !e.required) {
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
    out
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
                .map(|i| name_of(i))
                .collect()
        } else {
            vec![]
        },
        allowed: info
            .capabilities
            .iter()
            .map(|c| {
                ALLOWED
                    .iter()
                    .find(|(k, _)| k == c)
                    .map_or(c.clone(), |(_, words)| words.to_string())
            })
            .collect(),
        problems: e
            .problems
            .iter()
            .map(|d| match &d.hint {
                Some(h) if d.severity == Severity::Error => format!("{} ({h})", d.message),
                _ => d.message.clone(),
            })
            .collect(),
        broken: e.has_errors(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rows_group_explain_and_toggle() {
        let root = std::env::temp_dir().join(format!("bri-add-ons-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("ui-pack-003")).unwrap();
        std::fs::write(
            root.join("packages.json"),
            json!({ "schema_version": 1, "packages": [
                { "id": "v20-ui", "version": "3.0.0", "side": "client", "dir": "ui-pack-003", "role": "ui_pack" }
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
                "hurt players and break bricks"
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
        assert!(mismatch(&root, "Timed out").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
