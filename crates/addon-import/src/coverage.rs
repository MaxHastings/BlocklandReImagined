//! How much of each item a player can pick the import carries.
//!
//! The report's other lists are per file, per datablock and per script
//! function; a player meets an Add-On as the items in its menus. An item is
//! a datablock with a `uiName` (a weapon, vehicle, brick, player type, or a
//! projectile, emitter or sound offered to events), together with what it
//! names: an item's image, the image's projectile, that projectile's
//! explosion, their shapes and sounds, and the script callbacks of each.
//! Its coverage is the worst of theirs:
//!
//! - `preserved`: the original art, sounds and data, shown as v20 showed them;
//! - `ported`: script behaviour carried out by a verified native port;
//! - `approximate`: converted, but shown or behaving differently (a partial
//!   port, a datablock converted with gaps, a shape needing adaptation);
//! - `unsupported`: script behaviour with no port, or a datablock or asset
//!   with no native form;
//! - `missing_source`: a file, datablock or dependency it names was not
//!   found, so a stand-in is used.
use crate::report::{Location, Report};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// The coverage levels, best first.
pub const LEVELS: [&str; 5] = [
    "preserved",
    "ported",
    "approximate",
    "unsupported",
    "missing_source",
];

#[derive(Debug, Clone, Serialize)]
pub struct ItemCoverage {
    /// The datablock (`gunItem`).
    pub name: String,
    /// What the menu shows (`Gun`).
    pub ui_name: String,
    pub class: String,
    /// Native concept, as its datablock entry has it (`weapon`).
    pub kind: String,
    pub source: Location,
    /// One of [`LEVELS`].
    pub coverage: String,
    /// Each reason it is not `preserved`, worst first: `level: why`.
    pub reasons: Vec<String>,
    /// The datablocks it is made of, its own first; another Add-On's are
    /// marked with where they come from.
    pub uses: Vec<String>,
}

/// One datablock of the Add-On: its class and fields (merged, raw source).
pub type Datablocks = BTreeMap<String, (String, BTreeMap<String, String>)>;

/// The items of `datablocks`, menu items first, each with its coverage.
pub fn items(datablocks: &Datablocks, report: &Report) -> Vec<ItemCoverage> {
    let entries: BTreeMap<String, &crate::report::DatablockEntry> = report
        .datablocks
        .iter()
        .map(|e| (e.name.to_ascii_lowercase(), e))
        .collect();
    let mut items: Vec<ItemCoverage> = datablocks
        .iter()
        .filter_map(|(key, (class, fields))| {
            let ui_name = crate::literal(fields.get("uiname")?).trim().to_owned();
            if ui_name.is_empty() {
                return None;
            }
            Some(item(key, class, ui_name, datablocks, &entries, report))
        })
        .collect();
    items.sort_by_key(|i| (menu_rank(&i.class), i.ui_name.to_ascii_lowercase()));
    items
}

/// Inventory items, then vehicles, player types and bricks, then what
/// events offer.
fn menu_rank(class: &str) -> usize {
    let class = class.to_ascii_lowercase();
    match class.as_str() {
        "itemdata" => 0,
        c if c.ends_with("vehicledata") => 1,
        "playerdata" => 2,
        "fxdtsbrickdata" => 3,
        _ => 4,
    }
}

fn item(
    key: &str,
    class: &str,
    ui_name: String,
    datablocks: &Datablocks,
    entries: &BTreeMap<String, &crate::report::DatablockEntry>,
    report: &Report,
) -> ItemCoverage {
    // Everything it names, breadth first, its own datablocks only (another
    // Add-On's are that package's to cover).
    let mut reached = vec![key.to_owned()];
    let mut seen: BTreeSet<String> = reached.iter().cloned().collect();
    let mut foreign = BTreeMap::new();
    let mut files = BTreeSet::new();
    let mut i = 0;
    while let Some(name) = reached.get(i).cloned() {
        i += 1;
        let Some((_, fields)) = datablocks.get(&name) else {
            continue;
        };
        for (field, value) in fields {
            let base = field.split('[').next().unwrap_or(field);
            let v = crate::literal(value).trim();
            if crate::FILE_FIELDS.contains(&base) {
                if !v.is_empty() {
                    files.insert(v.to_ascii_lowercase());
                }
            } else if crate::REFERENCE_FIELDS.contains(&base) {
                for named in v.split_whitespace() {
                    let lower = named.to_ascii_lowercase();
                    if datablocks.contains_key(&lower) {
                        if seen.insert(lower.clone()) {
                            reached.push(lower);
                        }
                    } else {
                        foreign.entry(lower).or_insert_with(|| named.to_owned());
                    }
                }
            }
        }
    }
    let mut reasons: Vec<(usize, String)> = vec![];
    let mut note = |level: &str, why: String| {
        let rank = LEVELS.iter().position(|l| *l == level).unwrap_or(0);
        reasons.push((rank, format!("{level}: {why}")));
    };
    for name in &reached {
        let Some(e) = entries.get(name) else {
            continue;
        };
        match e.status.as_str() {
            "converted" | "consumed" => {}
            "ported" => note("ported", format!("{} is carried out by the port", e.name)),
            "converted_with_gaps" => {
                let gaps: Vec<&str> = e
                    .notes
                    .iter()
                    .map(String::as_str)
                    .filter(|n| !behaviour_note(n))
                    .collect();
                if !gaps.is_empty() {
                    note("approximate", format!("{}: {}", e.name, gaps.join("; ")));
                }
            }
            "external" => note(
                "missing_source",
                format!("{} downloads its file from a website at run time", e.name),
            ),
            status => note(
                "unsupported",
                format!(
                    "{} ({}) is {}{}",
                    e.name,
                    e.class,
                    status.replace('_', " "),
                    e.notes.first().map_or(String::new(), |n| format!(": {n}"))
                ),
            ),
        }
    }
    // Its script callbacks: a datablock's methods, or its class's.
    let namespaces: BTreeSet<String> = reached
        .iter()
        .cloned()
        .chain(reached.iter().filter_map(|n| {
            let class = datablocks.get(n)?.1.get("classname")?;
            Some(crate::literal(class).trim().to_ascii_lowercase())
        }))
        .collect();
    for b in &report.needs_behaviour {
        let Some((ns, _)) = b.function.split_once("::") else {
            continue;
        };
        if !namespaces.contains(&ns.to_ascii_lowercase()) {
            continue;
        }
        match &b.port {
            Some(p) if p.applied && p.status == "verified" => {
                note("ported", format!("{} by port {}", b.function, p.port));
            }
            Some(p) if p.applied => note(
                "approximate",
                format!("{} by port {}, which is {}", b.function, p.port, p.status),
            ),
            _ => note(
                "unsupported",
                format!("{} needs native behaviour: {}", b.function, b.summary),
            ),
        }
    }
    for f in &files {
        let Some(a) = asset(report, f) else {
            continue; // Another package's file; a missing one is ambiguous below.
        };
        match a.status.as_str() {
            "failed" | "unsupported" => note(
                "unsupported",
                format!("{} {}: {}", a.source, a.status, a.notes.join("; ")),
            ),
            _ => {}
        }
        for m in &a.missing {
            note("missing_source", format!("{}: {m}", a.source));
        }
        for g in &a.gaps {
            note("approximate", format!("{}: {g}", a.source));
        }
    }
    // A file or datablock it names that is found nowhere.
    for f in &report.ambiguous {
        let Some((field, value)) = f.what.split_once(" = ") else {
            continue;
        };
        let Some((owner, _)) = field.split_once('.') else {
            continue;
        };
        if !seen.contains(&owner.to_ascii_lowercase()) {
            continue;
        }
        if value.trim().starts_with("$DamageType::") {
            note("approximate", format!("{}: {}", f.what, f.detail));
        } else {
            note("missing_source", format!("{}: {}", f.what, f.detail));
        }
    }
    // Another Add-On's datablocks: present unless that Add-On is missing.
    let mut uses: Vec<String> = reached.iter().map(|n| datablock_name(entries, n)).collect();
    for named in foreign.values() {
        let dependency = report
            .dependencies
            .iter()
            .find(|d| d.uses.iter().any(|u| u.eq_ignore_ascii_case(named)));
        match dependency {
            Some(d) => {
                if d.status == "missing" {
                    note(
                        "missing_source",
                        format!("{named} comes from {}, which was not found", d.addon),
                    );
                }
                uses.push(format!("{named} ({})", d.addon));
            }
            None => uses.push(named.clone()),
        }
    }
    reasons.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    reasons.dedup();
    let entry = entries.get(key);
    ItemCoverage {
        name: datablock_name(entries, key),
        ui_name,
        class: class.to_owned(),
        kind: entry.map(|e| e.recognised_as.clone()).unwrap_or_default(),
        source: entry.map(|e| e.source.clone()).unwrap_or_default(),
        coverage: LEVELS[reasons.first().map_or(0, |r| r.0)].into(),
        reasons: reasons.into_iter().map(|(_, r)| r).collect(),
        uses,
    }
}

/// A datablock note about script callbacks, which the item's behaviour
/// covers from `needs_behaviour`.
fn behaviour_note(note: &str) -> bool {
    note.ends_with(" needs native behaviour")
        || crate::state_script(note).is_some()
        || note.contains(" ported by ")
        || note.contains(" runs the engine's own ")
}

fn datablock_name(entries: &BTreeMap<String, &crate::report::DatablockEntry>, key: &str) -> String {
    entries.get(key).map_or(key.to_owned(), |e| e.name.clone())
}

/// The asset a file field names, which Torque finds with or without its
/// extension.
fn asset<'a>(report: &'a Report, file: &str) -> Option<&'a crate::report::AssetEntry> {
    report.assets.iter().find(|a| {
        let source = a.source.to_ascii_lowercase();
        source == file
            || source
                .rsplit_once('.')
                .is_some_and(|(stem, _)| stem == file)
    })
}
