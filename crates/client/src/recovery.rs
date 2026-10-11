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

/// What [`make_room`] did with a snapshot an earlier game left.
#[derive(Debug, PartialEq)]
pub enum Room {
    /// Nothing was left.
    Free,
    /// It was kept as a save with this name.
    Kept(String),
    /// It does not load, so it can never be kept: it was moved to this
    /// path, out of the slot, so that this game is still protected.
    SetAside(PathBuf),
}

/// Free the slot for a game about to be hosted. A snapshot an earlier game
/// left is kept as a save first, never overwritten; one that is damaged
/// beyond loading is set aside beside the slot (the latest damaged one
/// only). An error (the save could not be written) leaves the slot taken,
/// and the new game then keeps no snapshot rather than lose the old one.
pub fn make_room(state_dir: &Path, saves: &crate::saves::Store) -> Result<Room> {
    let Some(left) = left(state_dir, saves) else {
        return Ok(Room::Free);
    };
    if bri_world::build::decode(&std::fs::read(&left.path)?).is_err() {
        let aside = left.path.with_file_name("hosted-world.damaged.build");
        std::fs::rename(&left.path, &aside)?;
        return Ok(Room::SetAside(aside));
    }
    keep(state_dir, saves).map(Room::Kept)
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

    /// A snapshot cut short (a disk that lost its tail, a copy cut off):
    /// it is still offered, but Keep refuses it whole and leaves it for
    /// Discard; a new hosted game sets it aside instead of running
    /// unprotected because the slot is taken.
    #[test]
    fn a_damaged_snapshot_is_offered_refused_whole_and_set_aside_for_a_new_game() -> Result<()> {
        let state = tempfile::tempdir()?;
        let saves = crate::saves::Store::for_tests(
            state.path().join("saves"),
            [("map/one".to_string(), "One".to_string())].into(),
            None,
        );
        let mut world = bri_world::World::new("w".into(), "map/one".into(), vec![[1.0; 4]]);
        for id in 1..=200 {
            world.bricks.insert(
                id,
                bri_world::Brick::new(
                    bri_world::ContentRef::Resolved("plate".into()),
                    [id as f32, 0.1, 0.0],
                    1,
                ),
            );
        }
        world.next_brick_id = 201;
        let whole = bri_world::build::encode(&bri_world::build::SavedBuild::new(world))?;
        let slot = path(state.path());
        std::fs::create_dir_all(slot.parent().unwrap())?;
        assert_eq!(make_room(state.path(), &saves)?, Room::Free);
        // Cut early (no head) and late (head intact, body short).
        for cut in [whole.len() / 50, whole.len() - 7] {
            std::fs::write(&slot, &whole[..cut])?;
            let found = left(state.path(), &saves).context("a damaged slot is not offered")?;
            let asked = question(&found, None);
            assert!(
                asked.text.contains("did not close normally"),
                "{}",
                asked.text
            );
            if found.bricks != 0 {
                assert_eq!((found.map.as_str(), found.bricks), ("One", 200));
            } else {
                assert_eq!(found.map, "an unknown map");
            }
            let refused = keep(state.path(), &saves).expect_err("a damaged build was kept");
            assert!(format!("{refused:#}").contains("damaged"), "{refused:#}");
            assert!(slot.exists(), "a refused build was removed");
            assert!(saves.list()?.is_empty(), "a damaged build became a save");
        }
        // Hosting again: the slot is freed for the new game's snapshot,
        // and the damaged file is kept beside it, not deleted.
        let Room::SetAside(aside) = make_room(state.path(), &saves)? else {
            panic!("the damaged snapshot was not set aside");
        };
        assert!(!slot.exists());
        assert_eq!(std::fs::read(&aside)?, &whole[..whole.len() - 7]);
        assert!(left(state.path(), &saves).is_none());
        // An undamaged one is still kept as a save, as before.
        std::fs::write(&slot, &whole)?;
        let Room::Kept(name) = make_room(state.path(), &saves)? else {
            panic!("a whole snapshot was not kept");
        };
        assert_eq!(saves.load("One", &name)?.world.bricks.len(), 200);
        // Discard always clears a damaged one.
        std::fs::write(&slot, &whole[..10])?;
        discard(state.path())?;
        assert!(left(state.path(), &saves).is_none());
        Ok(())
    }
}
