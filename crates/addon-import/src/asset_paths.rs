//! File paths for generated legacy Add-On assets. The source member remains
//! the identity and provenance. Changing its bytes does not choose a new path.
use crate::source::hash;
use bri_package::path;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(crate) struct AssetPaths {
    files: BTreeMap<String, String>,
    directories: BTreeSet<String>,
}

impl AssetPaths {
    pub(crate) fn member(&mut self, dir: &str, source: &str, extension: &str) -> String {
        let (folders, file) = source.rsplit_once('/').unwrap_or(("", source));
        let kind = dir.rsplit('/').next().expect("asset directory");
        let mut folders = folders
            .split('/')
            .filter(|part| !part.is_empty())
            .peekable();
        if folders
            .peek()
            .is_some_and(|part| part.eq_ignore_ascii_case(kind))
        {
            folders.next();
        }
        let mut parent = dir.to_owned();
        for folder in folders {
            parent.push('/');
            parent.push_str(&component(folder));
        }
        self.allocate(dir, &parent, source, file, extension)
    }

    /// Catalog bindings accept a bare mesh filename. Reserve the same name
    /// for the original conversion and its copy beside the catalog.
    pub(crate) fn brick(&mut self, source: &str) -> String {
        let file = source.rsplit('/').next().expect("source name");
        self.allocate("bricks", "bricks", source, file, "brick.json")
    }

    fn allocate(
        &mut self,
        dir: &str,
        parent: &str,
        source: &str,
        file: &str,
        extension: &str,
    ) -> String {
        let stem = component(file.rsplit_once('.').map_or(file, |(stem, _)| stem));
        let preferred = format!("{parent}/{stem}.{extension}");
        if self.claim(&preferred, source) {
            return preferred;
        }

        let digest = hash(source.to_ascii_lowercase().as_bytes());
        let suffix = &digest[..12];
        let collision = format!("{parent}/{stem}-{suffix}.{extension}");
        if self.claim(&collision, source) {
            return collision;
        }

        // Fall back to a short, flat path if the source is too deep or a
        // converted file has already claimed one of its directories.
        let stem = stem[..stem.len().min(64)].trim_end_matches([' ', '.']);
        let mut attempt = 1;
        loop {
            let name = if attempt == 1 {
                format!("{dir}/{stem}-{suffix}.{extension}")
            } else {
                format!("{dir}/{stem}-{suffix}-{attempt}.{extension}")
            };
            if self.claim(&name, source) {
                return name;
            }
            attempt += 1;
        }
    }

    fn claim(&mut self, name: &str, source: &str) -> bool {
        if let Some(prior) = self.files.get(name) {
            return prior == source;
        }
        let packaged = if let Some(brick) = name.strip_prefix("bricks/") {
            format!("assets/brick-catalog/{brick}")
        } else {
            format!("assets/{name}")
        };
        if path::problem(&packaged).is_some() || self.directories.contains(name) {
            return false;
        }
        let parents = name.match_indices('/').map(|(i, _)| &name[..i]);
        if parents.clone().any(|dir| self.files.contains_key(dir)) {
            return false;
        }
        self.directories.extend(parents.map(str::to_owned));
        self.files.insert(name.to_owned(), source.to_owned());
        true
    }
}

fn component(name: &str) -> String {
    // Lowercase portable characters keep directory spelling identical on
    // case-sensitive and case-insensitive filesystems. Reports keep the exact
    // source spelling, including characters that need replacing here.
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '-' | '_') {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim_matches([' ', '.']);
    if path::problem(name).is_some() {
        format!("_{name}")
    } else {
        name.to_owned()
    }
}
