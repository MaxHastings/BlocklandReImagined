//! The crash-recovery snapshot of the game this player hosts
//! (`bri_net::recovery`): one file in the client's state folder, never in
//! the saves Load Bricks lists. The host replaces it while the world
//! changes and deletes it when the game ends normally, so a file found here
//! means the last hosted game ended abnormally (the game crashed or was
//! killed, or the host failed). Only then is it offered back, once: kept as
//! an ordinary save, or discarded.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Where a hosted game keeps its recovery snapshot.
pub fn path(state_dir: &Path) -> PathBuf {
    state_dir.join("recovery").join("hosted-world.build")
}

/// What a left recovery snapshot holds, for the question that offers it.
pub struct Left {
    pub path: PathBuf,
    /// When it was last written, as Load Bricks shows dates.
    pub written: String,
    pub map: String,
    pub bricks: u64,
}

/// The snapshot a hosted game that ended abnormally left, if any.
pub fn left(state_dir: &Path, saves: &crate::saves::Store) -> Option<Left> {
    let path = path(state_dir);
    let written = std::fs::metadata(&path).ok()?.modified().ok()?;
    let seconds = written
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (map, bricks) = std::fs::read(&path)
        .ok()
        .and_then(|bytes| bri_world::build::decode_header(&bytes).ok())
        .map_or_else(
            || ("an unknown map".to_string(), 0),
            |(world, bricks)| (saves.map_name(&world.map_id), bricks),
        );
    Some(Left {
        path,
        written: crate::saves::modified_date(seconds),
        map,
        bricks,
    })
}

/// The question that offers a left snapshot back. `why` says how the game
/// ended, when this game saw it end.
pub fn question(left: &Left, why: Option<&str>) -> bri_ui::api::Question {
    let how = why.map_or_else(
        || "The last game you hosted did not close normally".to_string(),
        |why| format!("The game you were hosting stopped: {why}"),
    );
    bri_ui::api::Question {
        title: "Recover Unsaved Build?".into(),
        text: format!(
            "{how}.\n\nIts build on {} ({} bricks) from {} was kept. Keep it as a save you \
             can load with Load Bricks, or discard it?",
            left.map, left.bricks, left.written
        ),
        yes: "Keep".into(),
        no: "Discard".into(),
        on_yes: Box::new(bri_ui::api::UiAction::KeepRecoveredBuild),
        on_no: Some(Box::new(bri_ui::api::UiAction::DiscardRecoveredBuild)),
    }
}

/// Keep the left snapshot as an ordinary save ("Recovered <date>") in its
/// map's saves, then remove it. Returns the save's name.
pub fn keep(state_dir: &Path, saves: &crate::saves::Store) -> Result<String> {
    let left = left(state_dir, saves).context("There is no build to recover")?;
    let build = bri_world::build::decode(&std::fs::read(&left.path)?)
        .context("The recovered build is damaged")?;
    let stem = format!(
        "Recovered {}",
        left.written.trim_end_matches('Z').replace(':', ".")
    );
    let taken: Vec<String> = saves
        .list()?
        .into_iter()
        .map(|e| e.info.name.to_ascii_lowercase())
        .collect();
    let name = std::iter::once(format!("{stem}.world.json"))
        .chain((2..).map(|n| format!("{stem} ({n}).world.json")))
        .find(|n| !taken.contains(&n.to_ascii_lowercase()))
        .context("No free save name")?;
    saves.save(
        &name,
        "Kept after a game you hosted ended unexpectedly.",
        build,
        false,
    )?;
    discard(state_dir)?;
    Ok(name)
}

/// Remove the left snapshot.
pub fn discard(state_dir: &Path) -> Result<()> {
    match std::fs::remove_file(path(state_dir)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_left_snapshot_is_kept_once_as_a_save_or_discarded() -> Result<()> {
        let state = tempfile::tempdir()?;
        let saves = crate::saves::Store::for_tests(
            state.path().join("saves"),
            [("map/one".to_string(), "One".to_string())].into(),
            None,
        );
        assert!(left(state.path(), &saves).is_none());
        let world = bri_world::World::new("w".into(), "map/one".into(), vec![[1.0; 4]]);
        let mut build = bri_world::build::SavedBuild::new(world);
        build.minigame = Some(serde_json::json!({"teams": [{"name": "Red"}]}));
        let slot = path(state.path());
        std::fs::create_dir_all(slot.parent().unwrap())?;
        std::fs::write(&slot, bri_world::build::encode(&build)?)?;
        let found = left(state.path(), &saves).context("not found")?;
        assert_eq!(found.map, "One");
        let asked = question(&found, None);
        assert!(
            asked.text.contains("did not close normally"),
            "{}",
            asked.text
        );
        // Never listed among the saves while it waits.
        assert!(saves.list()?.is_empty());
        let name = keep(state.path(), &saves)?;
        assert!(name.starts_with("Recovered "), "{name}");
        assert!(!slot.exists());
        let kept = saves.load("One", &name)?;
        assert_eq!(kept.minigame, build.minigame);
        // Kept again (a second crash at the same minute): its own name.
        std::fs::write(&slot, bri_world::build::encode(&build)?)?;
        assert_ne!(keep(state.path(), &saves)?, name);
        std::fs::write(&slot, b"anything")?;
        discard(state.path())?;
        assert!(left(state.path(), &saves).is_none());
        Ok(())
    }
}
