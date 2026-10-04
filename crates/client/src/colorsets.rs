//! Local host colorset files. The resulting palette belongs to the new world;
//! joining clients receive that world palette through ordinary replication.
use anyhow::{Context, Result, ensure};
use bri_console::Clamp;
use bri_ui::api::{HostColorset, PaintDivision};
use std::{fs, io::Read, path::Path};

const MAX_BYTES: u64 = 65_536;
const MAX_SETS: usize = 128;

pub fn catalog(content: &Path, state: &Path, default: &[PaintDivision]) -> Vec<HostColorset> {
    let mut choices = vec![HostColorset {
        id: String::new(),
        name: "Default (v20)".into(),
        divisions: default.to_vec(),
    }];
    choices.extend(discover(content, state));
    choices
}

pub fn parse(text: &str) -> Result<Vec<PaintDivision>> {
    ensure!(text.len() as u64 <= MAX_BYTES, "Colorset exceeds 64 KiB");
    let mut divisions = Vec::new();
    let mut colors = Vec::new();
    let mut count = 0;
    let flush = |name: &str, colors: &mut Vec<_>, divisions: &mut Vec<_>| {
        for (i, chunk) in colors.chunks(9).enumerate() {
            divisions.push(PaintDivision {
                name: if i == 0 {
                    name.to_owned()
                } else {
                    format!("{name} {}", i + 1)
                },
                colors: chunk.to_vec(),
            });
        }
        colors.clear();
    };
    for (line, raw) in text.lines().enumerate() {
        let raw = raw.trim_start_matches('\u{feff}');
        let raw = raw.split_once("//").map_or(raw, |(s, _)| s).trim();
        if raw.is_empty() || raw.starts_with('#') {
            continue;
        }
        if let Some(name) = raw.strip_prefix("DIV:") {
            ensure!(
                name.len() <= 128,
                "Colorset division name is too long at line {}",
                line + 1
            );
            flush(name.trim(), &mut colors, &mut divisions);
            continue;
        }
        let values = raw
            .split_whitespace()
            .map(str::parse::<f32>)
            .collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| format!("Invalid colorset number at line {}", line + 1))?;
        ensure!(
            values.len() == 4,
            "Colorset needs RGBA at line {}",
            line + 1
        );
        ensure!(
            values
                .iter()
                .all(|v| v.is_finite() && *v >= 0.0 && *v <= 255.0),
            "Invalid colorset color at line {}",
            line + 1
        );
        // Match v20's whole-row integer/float decision. In particular,
        // "1 1 1 255" is near-black byte RGB, not white with byte alpha.
        let fractional = values.iter().any(|v| v.floor() != *v);
        let scale = if fractional || values.iter().all(|v| *v <= 1.0) {
            1.0
        } else {
            255.0
        };
        colors.push([
            (values[0] / scale).min(1.0),
            (values[1] / scale).min(1.0),
            (values[2] / scale).min(1.0),
            (values[3] / scale).clamped(1.0 / 255.0, 1.0),
        ]);
        count += 1;
        ensure!(count <= 256, "Colorset has more than 256 colors");
    }
    flush("Colors", &mut colors, &mut divisions);
    ensure!(count > 0, "Colorset is empty");
    Ok(divisions)
}

pub(super) fn read(path: &Path) -> Result<Vec<PaintDivision>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_BYTES, "Colorset exceeds 64 KiB");
    parse(std::str::from_utf8(&bytes).context("Colorset is not UTF-8")?)
}

/// Only catalog IDs can be selected. Caller-provided strings are never paths.
pub fn discover(content: &Path, state: &Path) -> Vec<HostColorset> {
    let mut sets = Vec::new();
    for (prefix, root, plain) in [
        ("user", state.join("colorsets"), true),
        ("addon", content.join("addons"), false),
    ] {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().take(512).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries.into_iter().take(512) {
            if sets.len() >= MAX_SETS {
                break;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            // IDs must preserve filename identity. Lossy conversion can merge
            // distinct Unix filenames into the same selectable catalog entry.
            let Ok(name) = entry.file_name().into_string() else {
                bri_console::warn(format!(
                    "Colorset filename is not UTF-8: {}",
                    entry.path().display()
                ));
                continue;
            };
            let path = if kind.is_dir() {
                entry.path().join("colorSet.txt")
            } else if plain
                && kind.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("txt"))
            {
                entry.path()
            } else {
                continue;
            };
            if !path.is_file() {
                continue;
            }
            match read(&path) {
                Ok(divisions) => sets.push(HostColorset {
                    id: format!("{prefix}:{name}"),
                    name: if prefix == "addon" {
                        addon_name(&entry.path()).unwrap_or_else(|| name.replace('_', " "))
                    } else if kind.is_dir() {
                        name.replace('_', " ")
                    } else {
                        Path::new(&name)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .replace('_', " ")
                    },
                    divisions,
                }),
                Err(error) => bri_console::warn(format!("Colorset {}: {error:#}", path.display())),
            }
        }
    }
    sets.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    sets
}

fn addon_name(folder: &Path) -> Option<String> {
    let file = fs::File::open(folder.join("package.json")).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    let manifest: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let name = manifest["name"].as_str()?;
    (!name.is_empty() && name.len() <= 128).then(|| name.to_owned())
}

pub fn selected(content: &Path, state: &Path, id: &str) -> Result<Option<Vec<[f32; 4]>>> {
    if id.is_empty() {
        return Ok(None);
    }
    let choice = discover(content, state)
        .into_iter()
        .find(|set| set.id == id)
        .context("Selected colorset is unavailable or invalid; choose another in Start Game")?;
    Ok(Some(
        choice
            .divisions
            .into_iter()
            .flat_map(|d| d.colors)
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::content_root::ContentRoot;
    crate::testing::synthetic_and_content!(ContentRoot: selected_colors_initialize_a_fresh_authoritative_world);

    fn selected_colors_initialize_a_fresh_authoritative_world(f: &ContentRoot) -> Result<()> {
        let content = crate::content::ClientContent::load(&f.root)?;
        let colors = vec![[0.2, 0.4, 0.6, 1.0], [0.8, 0.3, 0.1, 0.5]];
        let map = "v20/add-ons/map_slate/slate.mis";
        let loaded = content
            .paths
            .load_map_with_palette(map, None, Some(&colors))?;
        assert_eq!(loaded.simulation.state().palette, colors);
        assert!(
            content
                .paths
                .load_map_with_palette(map, Some("saved"), Some(&colors))
                .is_err()
        );
        assert!(
            content
                .paths
                .load_map_with_palette(map, None, Some(&[[f32::NAN, 0., 0., 1.]]))
                .is_err()
        );
        let default = content.paths.load_map(map, None)?;
        assert_eq!(
            default.simulation.state().palette,
            content
                .paint
                .iter()
                .flat_map(|d| d.colors.iter().copied())
                .collect::<Vec<_>>()
        );
        Ok(())
    }
    #[test]
    fn classic_divisions_float_byte_alpha_and_invalid_files() {
        let d = parse("1 0 0 1\n0 255 0 128\nDIV:First\n0 0 1 0.5\nDIV:Second\n").unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].name, "First");
        assert_eq!(d[0].colors[0], [1., 0., 0., 1.]);
        assert_eq!(d[0].colors[1], [0., 1., 0., 128. / 255.]);
        assert_eq!(d[1].colors[0], [0., 0., 1., 0.5]);
        let byte = parse("1 1 1 255").unwrap();
        assert_eq!(byte[0].colors[0], [1. / 255., 1. / 255., 1. / 255., 1.]);
        assert_eq!(parse("1 1 1 1").unwrap()[0].colors[0], [1.; 4]);
        for bad in ["", "nan 0 0 1", "-1 0 0 1", "256 0 0 1", "1 0 0", "garbage"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert!(parse(&"1 0 0 1\n".repeat(257)).is_err());
    }
    #[test]
    fn discovery_is_bounded_and_selection_never_accepts_arbitrary_paths() {
        let root = tempfile::tempdir().unwrap();
        let state = root.path().join("state");
        let content = root.path().join("content");
        fs::create_dir_all(state.join("colorsets")).unwrap();
        fs::create_dir_all(content.join("addons/ColorSet_Other")).unwrap();
        fs::write(state.join("colorsets/My_Set.txt"), "1 0 0 1\nDIV:Red").unwrap();
        fs::write(
            content.join("addons/ColorSet_Other/colorSet.txt"),
            "0 1 0 1",
        )
        .unwrap();
        assert_eq!(discover(&content, &state).len(), 2);
        assert_eq!(
            selected(&content, &state, "user:My_Set.txt").unwrap(),
            Some(vec![[1., 0., 0., 1.]])
        );
        assert!(selected(&content, &state, "../../outside").is_err());
        fs::write(state.join("colorsets/My_Set.txt"), "not a palette").unwrap();
        assert!(
            selected(&content, &state, "user:My_Set.txt").is_err(),
            "fresh launch rejects changed files"
        );
        assert_eq!(selected(&content, &state, "").unwrap(), None);
    }

    // macOS rejects these filenames at creation (EILSEQ); Linux accepts them
    // and exercises the real discovery collision rather than a mock path.
    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_filenames_cannot_alias_a_selectable_palette() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("colorsets");
        fs::create_dir(&folder).unwrap();
        for byte in [0x80, 0x81] {
            let filename = OsString::from_vec(vec![b'a', byte, b'.', b't', b'x', b't']);
            fs::write(folder.join(filename), "1 0 0 1").unwrap();
        }
        fs::write(folder.join("a\u{fffd}.txt"), "0 1 0 1").unwrap();
        let sets = discover(root.path(), root.path());
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].id, "user:a\u{fffd}.txt");
        assert_eq!(
            selected(root.path(), root.path(), &sets[0].id).unwrap(),
            Some(vec![[0., 1., 0., 1.]])
        );
    }
}
