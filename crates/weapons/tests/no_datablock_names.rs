//! The game reads declared data ([`bri_weapons::Image::on_fire`],
//! [`bri_weapons::Image::sport`], [`bri_weapons::State::raised_arms`],
//! ...), never an image's, item's or projectile's name. A v20
//! compatibility case is a field the importer fills.

use std::path::{Path, PathBuf};

/// Crates that read Torque content or exist to test or measure the game:
/// they may name datablocks.
const NOT_RUNTIME: [&str; 14] = [
    "addon-import",
    "audio-import",
    "bls",
    "chaos",
    "client-sandbox",
    "convert",
    "events-import",
    "foliage-import",
    "fx-import",
    "stresslab",
    "ui-import",
    "vehicles-import",
    "weapons-import",
    "weather-import",
];

/// Choosing behaviour by a datablock's name. Compared without whitespace,
/// so a call split over lines still counts.
const BANNED: [&str; 6] = [
    "native_id(",
    "image.contains(",
    "image.to_ascii_lowercase()",
    "image.rsplit(",
    "projectile.contains(",
    "Stock::",
];

/// The weapons runtime also may not compare any name at all.
const BANNED_IN_WEAPONS_RUNTIME: [&str; 5] = [
    "name.contains(",
    "name==\"",
    "name.as_str()",
    "name.eq_ignore_ascii_case(\"",
    "format!(\"horse",
];

/// Known uses, each with why it stays. Entries marked v0.2.7 are old name
/// tables still to move into importer data.
const KNOWN: [(&str, &str, &str); 6] = [
    (
        "weapons/src/lib.rs",
        "native_id(",
        "the function itself, used by importers",
    ),
    (
        "weapons/src/references.rs",
        "native_id(",
        "a unit test building a base-game id",
    ),
    (
        "weapons/src/rotation.rs",
        "image.to_ascii_lowercase()",
        "looks a definition up by name while reading Torque fields",
    ),
    (
        "package-runtime/src/bot_objectives.rs",
        "image.contains(",
        "checks an id has a namespace (`:`), not a name",
    ),
    (
        "client/src/actor_effects.rs",
        "native_id(",
        "v0.2.7: an emote image by datablock name, and the teleport image built in code",
    ),
    (
        "sim/src/session/weapons.rs",
        "native_id(",
        "v0.2.7: emote, pain and burn images by datablock name",
    ),
];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_game_never_matches_datablock_names() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    let mut used = Vec::new();
    for entry in std::fs::read_dir(&crates).unwrap() {
        let dir = entry.unwrap().path();
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        if NOT_RUNTIME.contains(&name.as_str()) || !dir.join("src").is_dir() {
            continue;
        }
        let mut files = Vec::new();
        sources(&dir.join("src"), &mut files);
        for file in files {
            let relative = file
                .strip_prefix(&crates)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let source: String = std::fs::read_to_string(&file)
                .unwrap()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            let weapons_runtime = relative.starts_with("weapons/src/runtime");
            let banned = BANNED.iter().chain(
                weapons_runtime
                    .then_some(BANNED_IN_WEAPONS_RUNTIME.iter())
                    .into_iter()
                    .flatten(),
            );
            for pattern in banned {
                if !source.contains(pattern) {
                    continue;
                }
                if KNOWN.iter().any(|(f, p, _)| *f == relative && p == pattern) {
                    used.push((relative.clone(), *pattern));
                    continue;
                }
                found.push(format!("{relative}: `{pattern}`"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "matching a datablock name; declare the behaviour in pack data and fill it in the importer:\n{}",
        found.join("\n")
    );
    // A known use that is gone comes off the list.
    for (file, pattern, _) in KNOWN {
        assert!(
            used.iter().any(|(f, p)| f == file && *p == pattern),
            "{file} no longer uses `{pattern}`: remove it from KNOWN"
        );
    }
}
