use anyhow::{Context, Result, ensure};
use bri_admin::{DurableState, MAX_SAVE_BYTES};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

/// Caller-owned persistent ban/auto-role state. Mutations are staged beside the
/// target and atomically replace it before the in-memory authority is published.
pub(super) struct AdminStore {
    path: PathBuf,
    #[cfg(test)]
    fail_after_commit_for_test: bool,
}

impl AdminStore {
    /// Open the state at `path`. A damaged file (unreadable contents, over
    /// the size limit) is set aside beside it as `<name>.damaged-<seconds>`
    /// and the host starts with empty bans and ranks, logged, rather than
    /// refusing to host at all.
    pub(super) fn open(path: impl AsRef<Path>) -> Result<(Self, DurableState)> {
        let path = path.as_ref().to_path_buf();
        ensure!(path.is_absolute(), "Admin state path must be absolute");
        let parent = path.parent().context("Admin state path has no parent")?;
        fs::create_dir_all(parent).context("Create admin state directory")?;
        let state = match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                // A link or folder in its place is not ours to move.
                ensure!(
                    metadata.file_type().is_file(),
                    "Admin state is not a regular file"
                );
                match Self::read(&path, metadata.len()) {
                    Ok(state) => state,
                    Err(error) => {
                        let aside = Self::set_aside(&path)?;
                        eprintln!(
                            "Admin state {} is damaged ({error:#}); moved it to {} and started with no bans or saved ranks",
                            path.display(),
                            aside.display()
                        );
                        DurableState::default()
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => DurableState::default(),
            Err(error) => return Err(error).context("Inspect admin state"),
        };
        let mut store = Self {
            path,
            #[cfg(test)]
            fail_after_commit_for_test: false,
        };
        // Verify the caller's directory is writable before the server accepts peers.
        if !store.path.exists() {
            store.persist(&state)?;
        }
        Ok((store, state))
    }

    fn read(path: &Path, len: u64) -> Result<DurableState> {
        ensure!(len <= MAX_SAVE_BYTES as u64, "Admin state exceeds limit");
        let file = File::open(path).context("Open admin state")?;
        let mut state = Vec::with_capacity(len as usize);
        file.take(MAX_SAVE_BYTES as u64 + 1)
            .read_to_end(&mut state)?;
        ensure!(state.len() <= MAX_SAVE_BYTES, "Admin state exceeds limit");
        DurableState::read(state.as_slice()).context("Validate admin state")
    }

    /// Move a damaged state file out of the way, keeping it for recovery.
    fn set_aside(path: &Path) -> Result<PathBuf> {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let mut name = path
            .file_name()
            .context("Admin state has no file name")?
            .to_os_string();
        name.push(format!(".damaged-{seconds}"));
        let aside = path.with_file_name(name);
        fs::rename(path, &aside).context("Set aside damaged admin state")?;
        Ok(aside)
    }

    /// Write `state`. When the new file is in place but the disk could not
    /// confirm it would survive a power loss, the change is kept and
    /// logged: the next change rewrites the whole file anyway, and stopping
    /// the host would cost every player more than the one change at risk.
    pub(super) fn persist(&mut self, state: &DurableState) -> Result<()> {
        state.validate()?;
        if let Ok(metadata) = fs::symlink_metadata(&self.path) {
            ensure!(
                metadata.file_type().is_file(),
                "Admin state target is not a regular file"
            );
        }
        let mut bytes = Vec::new();
        state.write(&mut bytes)?;
        if let Err(error) = bri_files::replace_private(&self.path, &bytes) {
            if bri_files::is_uncertain(&error) {
                eprintln!("Admin state saved, but the disk did not confirm it ({error}); kept");
                return Ok(());
            }
            return Err(error).context("Atomically replace admin state");
        }
        #[cfg(test)]
        if self.fail_after_commit_for_test {
            eprintln!("Admin state saved, but the disk did not confirm it (injected); kept");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_store_is_initialized_and_a_damaged_one_is_set_aside() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("admin.json");
        let (store, state) = AdminStore::open(&path).unwrap();
        assert!(path.is_file());
        assert_eq!(state.bans.len(), 0);
        drop(store);
        fs::write(&path, b"{not valid state").unwrap();
        let (_store, state) = AdminStore::open(&path).unwrap();
        assert_eq!(state.bans.len(), 0);
        // The damaged file is kept beside a fresh one.
        let aside: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.starts_with("admin.json.damaged-"))
            .collect();
        assert_eq!(aside.len(), 1, "{aside:?}");
        assert_eq!(
            fs::read(directory.path().join(&aside[0])).unwrap(),
            b"{not valid state"
        );
        DurableState::read(File::open(&path).unwrap()).unwrap();
        // A folder in its place is still refused: it is not ours to move.
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(AdminStore::open(&path).is_err());
    }

    #[test]
    fn precommit_disk_failure_is_reported_and_the_store_keeps_working() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("state");
        fs::create_dir(&parent).unwrap();
        let path = parent.join("admin.json");
        let (mut store, mut state) = AdminStore::open(&path).unwrap();
        // A folder now sits where the state file was: nothing is replaced.
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(store.persist(&state).is_err());
        fs::remove_dir(&path).unwrap();
        state.next_ban_id = 3;
        store.persist(&state).unwrap();
        assert_eq!(
            DurableState::read(File::open(&path).unwrap())
                .unwrap()
                .next_ban_id,
            3
        );
    }

    #[test]
    fn unconfirmed_durability_keeps_the_change_and_the_host() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("admin.json");
        let (mut store, mut state) = AdminStore::open(&path).unwrap();
        state.next_ban_id = 2;
        store.fail_after_commit_for_test = true;
        store.persist(&state).unwrap();
        let disk = DurableState::read(File::open(&path).unwrap()).unwrap();
        assert_eq!(disk.next_ban_id, 2);
        state.next_ban_id = 4;
        store.persist(&state).unwrap();
        let disk = DurableState::read(File::open(path).unwrap()).unwrap();
        assert_eq!(disk.next_ban_id, 4);
    }
}
