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
    poisoned: bool,
    #[cfg(test)]
    fail_after_commit_for_test: bool,
}

impl AdminStore {
    pub(super) fn open(path: impl AsRef<Path>) -> Result<(Self, DurableState)> {
        let path = path.as_ref().to_path_buf();
        ensure!(path.is_absolute(), "Admin state path must be absolute");
        let parent = path.parent().context("Admin state path has no parent")?;
        fs::create_dir_all(parent).context("Create admin state directory")?;
        let state = match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                ensure!(metadata.file_type().is_file(), "Admin state is not a regular file");
                ensure!(metadata.len() <= MAX_SAVE_BYTES as u64, "Admin state exceeds limit");
                let file = File::open(&path).context("Open admin state")?;
                let mut state = Vec::with_capacity(metadata.len() as usize);
                file.take(MAX_SAVE_BYTES as u64 + 1).read_to_end(&mut state)?;
                ensure!(state.len() <= MAX_SAVE_BYTES, "Admin state exceeds limit");
                DurableState::read(state.as_slice()).context("Validate admin state")?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => DurableState::default(),
            Err(error) => return Err(error).context("Inspect admin state"),
        };
        let mut store = Self {
            path,
            poisoned: false,
            #[cfg(test)]
            fail_after_commit_for_test: false,
        };
        // Verify the caller's directory is writable before the server accepts peers.
        if !store.path.exists() {
            store.persist(&state)?;
        }
        Ok((store, state))
    }

    pub(super) fn persist(&mut self, state: &DurableState) -> Result<()> {
        ensure!(!self.poisoned, "Admin store durability is uncertain; restart required");
        state.validate()?;
        if let Ok(metadata) = fs::symlink_metadata(&self.path) {
            ensure!(metadata.file_type().is_file(), "Admin state target is not a regular file");
        }
        let parent = self.path.parent().context("Admin state path has no parent")?;
        let mut staged = tempfile::NamedTempFile::new_in(parent)
            .context("Create admin state staging file")?;
        state.write(&mut staged)?;
        staged.as_file().sync_all().context("Flush staged admin state")?;
        staged
            .persist(&self.path)
            .map_err(|error| error.error)
            .context("Atomically replace admin state")?;
        #[cfg(test)]
        if self.fail_after_commit_for_test {
            self.poisoned = true;
            anyhow::bail!("injected post-rename durability uncertainty");
        }
        #[cfg(unix)]
        if let Err(error) = File::open(parent).and_then(|directory| directory.sync_all()) {
            // Rename already committed; do not publish the candidate or accept
            // another durable mutation while crash durability is ambiguous.
            self.poisoned = true;
            return Err(error).context("Sync admin state directory; restart required");
        }
        Ok(())
    }

    pub(super) fn poisoned(&self) -> bool {
        self.poisoned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_store_is_initialized_and_corrupt_store_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("admin.json");
        let (store, state) = AdminStore::open(&path).unwrap();
        assert!(path.is_file());
        assert_eq!(state.bans.len(), 0);
        drop(store);
        fs::write(&path, b"{not valid state").unwrap();
        assert!(AdminStore::open(&path).is_err());
    }

    #[test]
    fn precommit_disk_failure_does_not_poison_store() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("state");
        fs::create_dir(&parent).unwrap();
        let path = parent.join("admin.json");
        let (mut store, state) = AdminStore::open(&path).unwrap();
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&parent).unwrap();
        assert!(store.persist(&state).is_err());
        assert!(!store.poisoned());
    }

    #[test]
    fn postrename_uncertainty_poison_requires_host_stop() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("admin.json");
        let (mut store, mut state) = AdminStore::open(&path).unwrap();
        state.next_ban_id = 2;
        store.fail_after_commit_for_test = true;
        assert!(store.persist(&state).is_err());
        assert!(store.poisoned());
        let disk = DurableState::read(File::open(path).unwrap()).unwrap();
        assert_eq!(disk.next_ban_id, 2);
    }
}
