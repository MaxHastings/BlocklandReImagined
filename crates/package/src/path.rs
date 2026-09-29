//! The one rule for file names inside a package, used by every reader: the
//! manifest's directory entries, a package's provided files, and the files
//! a client downloads. A name that passes is a plain relative path that
//! means the same file on every platform; [`inside`] then refuses links, so
//! a package reads only its own bytes.
use std::path::{Path, PathBuf};

/// Longest relative path, leaving room under Windows' 260-character limit
/// for the content root or cache directory.
pub const MAX_PATH: usize = 160;
/// Device names Windows resolves in any directory, with any extension.
const WINDOWS_DEVICES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9", "conin$",
    "conout$",
];

/// Why `path` is not a plain relative package path, or None. Forward
/// slashes only; no empty, `.` or `..` parts; no character Windows forbids
/// or gives a meaning (`:` names an alternate data stream); no part ending
/// in a space or dot (Windows drops them); no device names.
pub fn problem(path: &str) -> Option<String> {
    if path.is_empty() || path.len() > MAX_PATH {
        return Some(format!("path must be 1-{MAX_PATH} bytes"));
    }
    if path
        .chars()
        .any(|c| c.is_control() || matches!(c, '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Some("path has a character Windows forbids or a control character".into());
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Some("path must be relative, without empty, `.` or `..` parts".into());
        }
        if segment.ends_with([' ', '.']) {
            return Some("a path part may not end in a space or dot".into());
        }
        let stem = segment.split('.').next().unwrap_or_default();
        if WINDOWS_DEVICES.contains(&stem.to_ascii_lowercase().as_str()) {
            return Some(format!("`{segment}` is a Windows device name"));
        }
    }
    None
}

/// `root/path`, provided `path` passes [`problem`] and no part of it is a
/// link (a symlink, or a junction on Windows) that could lead outside
/// `root`. Parts that do not exist yet are fine.
pub fn inside(root: &Path, path: &str) -> Result<PathBuf, String> {
    if let Some(problem) = problem(path) {
        return Err(format!("`{path}`: {problem}"));
    }
    let mut at = root.to_path_buf();
    for segment in path.split('/') {
        at.push(segment);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "`{path}` goes through a link; packages may not contain links"
                ));
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    Ok(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_names_are_refused() {
        for path in [
            "",
            "../x",
            "a/../x",
            "/abs",
            "a\\b",
            "a//b",
            "./a",
            "main.rhai:stream",
            "C:/x",
            "trailing.",
            "space ",
            "con.json",
            "a/NUL",
            "bad\u{7}",
            &"x".repeat(MAX_PATH + 1),
        ] {
            assert!(problem(path).is_some(), "{path:?} accepted");
        }
        for path in ["main.rhai", "models/creeper.glb", "a.b/c-d_e.json"] {
            assert!(problem(path).is_none(), "{path:?} refused");
        }
    }

    #[cfg(unix)]
    #[test]
    fn links_are_refused() {
        let root = std::env::temp_dir().join(format!("bri-path-{}", std::process::id()));
        std::fs::create_dir_all(root.join("real")).unwrap();
        std::fs::write(root.join("real/f"), "x").unwrap();
        let _ = std::fs::remove_file(root.join("link"));
        std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();
        assert!(inside(&root, "real/f").is_ok());
        assert!(inside(&root, "link/f").is_err());
        assert!(inside(&root, "real/missing").is_ok());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
