//! Versioned client preferences, atomically replaced after a successful write.
use anyhow::{Context, Result, ensure};
use bri_ui::api::Settings;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
const LIMIT: u64 = 2 * 1024 * 1024;

/// Only native user overrides select the launch mode. Imported v20 monitor
/// defaults describe another renderer/machine and are not startup instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupDisplay {
    pub size: (u32, u32),
    pub fullscreen: bool,
    pub vsync: bool,
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
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    schema_version: u32,
    settings: Settings,
}
pub fn load(path: &Path) -> Result<Settings> {
    let mut bytes = Vec::new();
    match File::open(path) {
        Ok(file) => {
            file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(e) => return Err(e.into()),
    }
    ensure!(
        bytes.len() as u64 <= LIMIT,
        "Settings file exceeds size limit"
    );
    let stored: Stored = serde_json::from_slice(&bytes)
        .context("Invalid native client settings; preserve the file for recovery")?;
    ensure!(
        stored.schema_version == 1,
        "Unsupported settings schema {}",
        stored.schema_version
    );
    Ok(stored.settings)
}
pub fn save(path: &Path, settings: &Settings) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(&Stored {
        schema_version: 1,
        settings: settings.clone(),
    })?;
    ensure!(bytes.len() as u64 <= LIMIT, "Settings exceed size limit");
    let parent = path
        .parent()
        .context("Settings file needs a parent directory")?;
    std::fs::create_dir_all(parent)?;
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let staging = parent.join(format!(
        ".settings-{}-{}.tmp",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        Ok(())
    })();
    drop(file);
    let result = result.and_then(|()| std::fs::rename(&staging, path).map_err(Into::into));
    if result.is_err() {
        let _ = std::fs::remove_file(&staging);
    }
    result
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
        assert_eq!(
            startup_display(&settings),
            StartupDisplay {
                size: (1920, 1080),
                fullscreen: true,
                vsync: false,
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
}
