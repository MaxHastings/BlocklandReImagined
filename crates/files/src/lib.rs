//! Crash-safe writes for every file the game keeps for a player or host:
//! settings, saves, trust and pin lists, identities, host certificates and
//! administration state.
//!
//! The bytes go to a new temporary file beside the target, are flushed to
//! disk, and only then take the target's name (a rename, or a hard link when
//! the target must not already exist). The target is therefore either its old
//! contents or the complete new contents, never a torn mix. On Unix the
//! directory is flushed too, so the new name itself survives a power loss.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// The new contents replaced the target, but flushing the directory failed,
/// so a crash could still bring back the old contents. Callers that must not
/// act on an uncertain save (administration state) check [`is_uncertain`].
#[derive(Debug)]
struct Uncertain(String);
impl std::fmt::Display for Uncertain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Uncertain {}

/// Whether a write error means the new contents may or may not survive a crash.
pub fn is_uncertain(error: &io::Error) -> bool {
    error.get_ref().is_some_and(|inner| inner.is::<Uncertain>())
}

#[derive(Clone, Copy, PartialEq)]
enum Publish {
    Replace,
    CreateNew,
}

/// Atomically replace (or create) `path` with `bytes`.
pub fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write(path, bytes, Publish::Replace, false)
}

/// Like [`replace`], but readable only by the current user on Unix.
pub fn replace_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write(path, bytes, Publish::Replace, true)
}

/// Atomically create `path` with `bytes`; fails if it already exists.
pub fn create_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write(path, bytes, Publish::CreateNew, false)
}

/// Like [`create_new`], but readable only by the current user on Unix.
pub fn create_new_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write(path, bytes, Publish::CreateNew, true)
}

fn context(path: &Path, what: &str, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("Could not save {}: {what}: {error}", path.display()),
    )
}

fn write(path: &Path, bytes: &[u8], publish: Publish, private: bool) -> io::Result<()> {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let name = path
        .file_name()
        .ok_or_else(|| context(path, "no file name", io::ErrorKind::InvalidInput.into()))?;
    fs::create_dir_all(parent).map_err(|e| context(path, "creating its folder", e))?;
    let (staging, mut file) = staged(parent, &name.to_string_lossy(), private)
        .map_err(|e| context(path, "creating a temporary file", e))?;
    let written = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| context(path, "writing the temporary file", e));
    drop(file);
    let published = written.and_then(|()| match publish {
        Publish::Replace => fs::rename(&staging, path).map_err(|e| context(path, "replacing it", e)),
        Publish::CreateNew => {
            fs::hard_link(&staging, path).map_err(|e| context(path, "creating it", e))
        }
    });
    let _ = fs::remove_file(&staging);
    published?;
    sync_directory(parent).map_err(|e| {
        io::Error::new(
            e.kind(),
            Uncertain(format!(
                "Saved {} but could not flush its folder ({e}); a crash could undo the save",
                path.display()
            )),
        )
    })
}

/// A new, uniquely named file beside the target.
fn staged(parent: &Path, name: &str, private: bool) -> io::Result<(PathBuf, File)> {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    loop {
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{name}.{}.{serial}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        if private {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(not(unix))]
        let _ = private;
        match options.open(&candidate) {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

#[cfg(unix)]
fn sync_directory(parent: &Path) -> io::Result<()> {
    File::open(parent)?.sync_all()
}
#[cfg(not(unix))]
fn sync_directory(_: &Path) -> io::Result<()> {
    // Windows commits the rename with the file system's own metadata journal
    // and offers no portable directory flush.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bri-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn leftovers(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn replace_swaps_whole_contents_and_leaves_no_temporary_files() {
        let dir = folder("replace");
        let path = dir.join("settings.json");
        replace(&path, b"first").unwrap();
        replace(&path, b"second, longer").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second, longer");
        replace(&path, b"3").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"3");
        assert!(leftovers(&dir).is_empty());
        let nested = dir.join("new/folder/state.bin");
        replace(&nested, b"x").unwrap();
        assert_eq!(fs::read(nested).unwrap(), b"x");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn create_new_never_overwrites() {
        let dir = folder("create");
        let path = dir.join("world.json");
        create_new(&path, b"original").unwrap();
        let error = create_new(&path, b"other").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(error.to_string().contains("world.json"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert!(leftovers(&dir).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_failed_replace_keeps_the_old_contents() {
        let dir = folder("failed");
        let path = dir.join("pins.json");
        replace(&path, b"kept").unwrap();
        // A folder where the target should be cannot be replaced by a file.
        let blocked = dir.join("blocked");
        fs::create_dir(&blocked).unwrap();
        fs::write(blocked.join("inside"), b"x").unwrap();
        let error = replace(&blocked, b"new").unwrap_err();
        assert!(error.to_string().contains("Could not save"), "{error}");
        assert!(!is_uncertain(&error));
        assert_eq!(fs::read(&path).unwrap(), b"kept");
        assert!(leftovers(&dir).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn private_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = folder("private");
        let path = dir.join("identity");
        create_new_private(&path, b"secret").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        replace_private(&path, b"secret2").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        fs::remove_dir_all(dir).unwrap();
    }
}
