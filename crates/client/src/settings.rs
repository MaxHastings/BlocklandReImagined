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
        save(&path, &s).unwrap();
        assert_eq!(load(&path).unwrap(), s);
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
