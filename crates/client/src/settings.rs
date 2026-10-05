//! Versioned client preferences, atomically replaced after a successful write.
use anyhow::{Context, Result, ensure};
use bri_ui::api::Settings;
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Read, path::Path};
const LIMIT: u64 = 2 * 1024 * 1024;

/// Only native user overrides select the launch mode. Imported v20 monitor
/// defaults describe another renderer/machine and are not startup instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupDisplay {
    pub size: (u32, u32),
    pub fullscreen: bool,
    pub vsync: bool,
    /// Frame-rate cap from Options (Max FPS), None for unlimited.
    pub max_fps: Option<u32>,
}

pub fn startup_display(settings: &Settings) -> StartupDisplay {
    let prefs = bri_ui::prefs::Prefs::new(&Default::default(), &settings.prefs);
    // The platform requests default wgpu device limits, including this bound.
    let limit = wgpu::Limits::default().max_texture_dimension_2d;
    let mut words = prefs
        .str_or("$pref::Video::resolution", "")
        .split_whitespace();
    let size = words
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .zip(words.next().and_then(|v| v.parse::<u32>().ok()))
        .filter(|&(w, h)| (640..=limit).contains(&w) && (480..=limit).contains(&h));
    StartupDisplay {
        size: size.unwrap_or((1280, 720)),
        // An invalid saved size must not request an arbitrary fullscreen mode.
        fullscreen: size.is_some() && prefs.bool_or("$pref::Video::fullScreen", false),
        vsync: !prefs.bool_or("$pref::Video::disableVerticalSync", false),
        max_fps: bri_ui::screens::options::max_fps(&prefs),
    }
}
#[derive(Serialize, Deserialize)]
struct Stored {
    schema_version: u32,
    settings: Settings,
}
const SCHEMA: u32 = 1;
pub fn load(path: &Path) -> Result<Settings> {
    let Some(bytes) = read(path)? else {
        return Ok(Settings::default());
    };
    let stored: Stored = serde_json::from_slice(&bytes)
        .context("Invalid native client settings; preserve the file for recovery")?;
    ensure!(
        stored.schema_version == SCHEMA,
        "Unsupported settings schema {}",
        stored.schema_version
    );
    Ok(stored.settings)
}
fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    match File::open(path) {
        Ok(file) => {
            file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    ensure!(
        bytes.len() as u64 <= LIMIT,
        "Settings file exceeds size limit"
    );
    Ok(Some(bytes))
}

/// Where a damaged state file (`settings.json`, `servers.json`) is kept
/// beside itself before anything replaces it: `<name>.damaged-<unix
/// seconds>.<extension>`.
pub(crate) fn damaged_copy(path: &Path) -> std::path::PathBuf {
    let stem = path
        .file_stem()
        .map_or_else(|| "state".into(), |s| s.to_string_lossy());
    let extension = path
        .extension()
        .map_or_else(|| "json".into(), |s| s.to_string_lossy());
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    path.with_file_name(format!("{stem}.damaged-{seconds}.{extension}"))
}

/// Settings for startup, and what the player should be told when the file
/// could not be used as it was.
#[derive(Debug)]
pub struct Recovered {
    pub settings: Settings,
    pub notice: Option<String>,
}

/// Like [`load`], but a damaged file never stops the game. The original is
/// copied next to it as `settings.damaged-<unix seconds>.json`, every section
/// that still reads is kept, the rest take their defaults, and the result is
/// written back so the next start is clean.
pub fn recover(path: &Path) -> Recovered {
    let error = match load(path) {
        Ok(settings) => {
            return Recovered {
                settings,
                notice: None,
            };
        }
        Err(error) => error,
    };
    let bytes = match read(path) {
        Ok(Some(bytes)) => bytes,
        // Unreadable (permissions, a folder in the way): nothing to keep or copy.
        _ => {
            bri_console::warn(format!("Settings could not be read: {error:#}"));
            return Recovered {
                settings: Settings::default(),
                notice: Some(format!(
                    "Your settings could not be read, so the game started with default settings. \
                     Changes may not save until this is fixed.\n\nFile: {}",
                    path.display()
                )),
            };
        }
    };
    let (settings, kept) = salvage(&bytes);
    let backup = damaged_copy(path);
    let backed_up = bri_files::create_new(&backup, &bytes).is_ok();
    bri_console::warn(format!(
        "Settings file was damaged ({error:#}); kept {kept} section(s){}",
        if backed_up {
            format!(", original copied to {}", backup.display())
        } else {
            String::new()
        }
    ));
    // Only overwrite the original once a copy of it exists.
    if backed_up && let Err(error) = save(path, &settings) {
        bri_console::warn(format!("Recovered settings could not be saved: {error:#}"));
    }
    let what = if kept > 0 {
        "Some of your settings could not be read. The rest were kept and the missing ones are back to their defaults."
    } else {
        "Your settings could not be read, so they are back to their defaults."
    };
    let copy = if backed_up {
        format!(
            "\n\nA copy of the old file was saved as {}",
            backup.display()
        )
    } else {
        String::new()
    };
    Recovered {
        settings,
        notice: Some(format!("{what}{copy}")),
    }
}

/// Keep every top-level section of the stored settings that still reads on its
/// own; the rest take their defaults. Returns the settings and how many
/// sections were kept.
fn salvage(bytes: &[u8]) -> (Settings, usize) {
    let stored = serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|v| v.get("settings").cloned());
    let Some(serde_json::Value::Object(fields)) = stored else {
        return (Settings::default(), 0);
    };
    let mut current = serde_json::to_value(Settings::default()).expect("settings serialize");
    let mut kept = 0;
    for (key, value) in fields {
        let mut candidate = current.clone();
        let Some(slot) = candidate.get_mut(&key) else {
            continue;
        };
        *slot = value;
        if serde_json::from_value::<Settings>(candidate.clone()).is_ok() {
            current = candidate;
            kept += 1;
        }
    }
    let settings = serde_json::from_value(current).unwrap_or_default();
    (settings, kept)
}
pub fn save(path: &Path, settings: &Settings) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(&Stored {
        schema_version: SCHEMA,
        settings: settings.clone(),
    })?;
    ensure!(bytes.len() as u64 <= LIMIT, "Settings exceed size limit");
    path.parent()
        .context("Settings file needs a parent directory")?;
    Ok(bri_files::replace(path, &bytes)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_display_restores_native_overrides_and_rejects_invalid_sizes() {
        let mut settings = Settings::default();
        assert_eq!(
            startup_display(&settings),
            StartupDisplay {
                size: (1280, 720),
                fullscreen: false,
                vsync: true,
                max_fps: None,
            }
        );
        settings
            .prefs
            .insert("$PREF::VIDEO::Resolution".into(), "1920 1080 32".into());
        settings
            .prefs
            .insert("$pref::Video::fullScreen".into(), "1".into());
        settings
            .prefs
            .insert("$pref::Video::disableVerticalSync".into(), "1".into());
        settings
            .prefs
            .insert("$pref::Video::MaxFps".into(), "144".into());
        assert_eq!(
            startup_display(&settings),
            StartupDisplay {
                size: (1920, 1080),
                fullscreen: true,
                vsync: false,
                max_fps: Some(144),
            }
        );
        for value in [
            "0 1080",
            "1920 0",
            "-1 720",
            "4294967295 720",
            "1920",
            "NaN 720",
        ] {
            settings
                .prefs
                .insert("$PREF::VIDEO::Resolution".into(), value.into());
            let display = startup_display(&settings);
            assert_eq!(display.size, (1280, 720), "{value}");
            assert!(!display.fullscreen, "{value}");
        }
    }
    #[test]
    fn preferences_roundtrip_replace_and_corruption_is_not_silently_reset() {
        let dir = std::env::temp_dir().join(format!(
            "bri-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("settings.json");
        let mut s = load(&path).unwrap();
        s.prefs.insert("test".into(), "one".into());
        s.prefs
            .insert("$pref::Video::resolution".into(), "1600 900 32".into());
        s.prefs
            .insert("$pref::Video::disableVerticalSync".into(), "1".into());
        save(&path, &s).unwrap();
        let restored = load(&path).unwrap();
        assert_eq!(restored, s);
        assert_eq!(
            startup_display(&restored),
            StartupDisplay {
                size: (1600, 900),
                fullscreen: false,
                vsync: false,
                max_fps: None,
            }
        );
        s.prefs.insert("test".into(), "two".into());
        save(&path, &s).unwrap();
        assert_eq!(load(&path).unwrap(), s);
        std::fs::write(&path, b"broken").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken");
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bri-settings-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn backups(dir: &Path) -> Vec<Vec<u8>> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("settings.damaged-")
            })
            .map(|p| std::fs::read(p).unwrap())
            .collect()
    }
    #[test]
    fn damaged_settings_start_with_defaults_keep_a_copy_and_tell_the_player() {
        let dir = scratch("damaged");
        let path = dir.join("settings.json");
        std::fs::write(&path, b"broken").unwrap();
        let recovered = recover(&path);
        assert_eq!(recovered.settings, Settings::default());
        let notice = recovered.notice.unwrap();
        assert!(notice.contains("back to their defaults"), "{notice}");
        assert!(notice.contains("settings.damaged-"), "{notice}");
        assert_eq!(backups(&dir), vec![b"broken".to_vec()]);
        // The replacement reads cleanly, so the next start says nothing.
        assert_eq!(load(&path).unwrap(), Settings::default());
        assert!(recover(&path).notice.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn a_bad_section_keeps_the_rest_and_missing_fields_take_defaults() {
        let dir = scratch("partial");
        let path = dir.join("settings.json");
        // `binds` has the wrong shape, `mouse_type` and others are missing, and
        // a field from a newer build is present.
        std::fs::write(
            &path,
            br#"{"schema_version":1,"settings":{"prefs":{"$pref::Input::MouseSensitivity":"1.5"},"binds":"nope","keyboard_type":1,"from_the_future":true}}"#,
        )
        .unwrap();
        let recovered = recover(&path);
        assert_eq!(
            recovered
                .settings
                .prefs
                .get("$pref::Input::MouseSensitivity")
                .map(String::as_str),
            Some("1.5")
        );
        assert_eq!(recovered.settings.keyboard_type, 1);
        assert_eq!(recovered.settings.binds, None);
        assert!(recovered.notice.unwrap().contains("The rest were kept"));
        assert_eq!(backups(&dir).len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn older_files_missing_newer_fields_load_without_a_notice() {
        let dir = scratch("older");
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            br#"{"schema_version":1,"settings":{"prefs":{"a":"b"}}}"#,
        )
        .unwrap();
        let recovered = recover(&path);
        assert!(recovered.notice.is_none());
        assert_eq!(
            recovered.settings.prefs.get("a").map(String::as_str),
            Some("b")
        );
        assert!(backups(&dir).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
