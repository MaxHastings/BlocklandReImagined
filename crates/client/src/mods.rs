//! Add-On packages this client downloaded from a server. Joining a server
//! whose shared packages this client lacks downloads them into the package
//! cache (`bri_net::client::Client::connect_fetching`), loads them here and
//! joins again with the server's package list. The catalog holds this
//! client's own packages with the downloaded ones in place of those of the
//! same id. Only data is loaded: models, HUD panels and other declarative
//! kinds. Base game content cannot be
//! swapped while the game runs, so a server running different base content
//! is refused with that reason rather than joined with content this client
//! does not actually use.
use anyhow::{Result, bail};
use bri_net::packages::Fetched;
use bri_package::{
    environment::PackageRef,
    packages::{PackageEntry, PackageSet},
};
use bri_package_runtime::{Catalog, manifest::MANIFEST_FILE};

/// Load downloaded packages and return the catalog with the package list
/// to join with: this client's own packages (`set` under `root`, whose
/// references are `local`), with every package the server sent in place of
/// the local one of the same id, and without the `dropped` ones the server
/// does not run.
pub fn load_fetched(
    root: &std::path::Path,
    set: &PackageSet,
    local: &[PackageRef],
    fetched: &[Fetched],
    dropped: &[PackageRef],
) -> Result<(Catalog, Vec<PackageRef>)> {
    let left_out =
        |id: &str| fetched.iter().any(|f| f.package.id == id) || dropped.iter().any(|d| d.id == id);
    let mut dirs = Vec::new();
    for entry in &set.packages {
        if entry.role.is_some() || left_out(&entry.id) {
            continue;
        }
        dirs.push((
            bri_package::packages::package_dir(root, entry)?,
            entry.clone(),
        ));
    }
    for f in fetched {
        let differs = !local.contains(&f.package);
        if differs && !f.dir.join(MANIFEST_FILE).is_file() {
            bail!(
                "The server runs different base game content ({}); this game cannot load it while running",
                f.package
            );
        }
        dirs.push((
            f.dir.clone(),
            PackageEntry {
                id: f.package.id.clone(),
                version: f.package.version.clone(),
                side: f.package.side,
                dir: f.dir.to_string_lossy().into_owned(),
                role: None,
            },
        ));
    }
    // A downloaded HUD may need the server's own rules (a server-only
    // package the client never loads). The server runs them; list them as
    // server-side so the client-side checks skip them, as they do for a
    // host's own server packages.
    let mut needs = Vec::new();
    for f in fetched {
        let Ok(text) = std::fs::read(f.dir.join(MANIFEST_FILE)) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&text) else {
            continue;
        };
        if let Some(dependencies) = manifest.get("dependencies").and_then(|d| d.as_object()) {
            needs.extend(dependencies.keys().cloned());
        }
    }
    for id in needs {
        if dirs
            .iter()
            .any(|(_, e): &(std::path::PathBuf, PackageEntry)| e.id == id)
        {
            continue;
        }
        dirs.push((
            root.to_path_buf(),
            PackageEntry {
                id: id.clone(),
                version: "0.0.0".into(),
                side: bri_package::packages::Side::Server,
                dir: id,
                role: None,
            },
        ));
    }
    // A downloaded Add-On that does not load here (a HUD key clash with one
    // of this player's, say) is left out of what this client shows, named in
    // the console; the server still runs it and the join goes ahead.
    let (catalog, problems) = Catalog::load_dirs_skipping(&dirs, false);
    for problem in &problems {
        bri_console::warn(format!("Add-On left out on this PC: {problem}"));
    }
    let mut packages: Vec<PackageRef> =
        local.iter().filter(|p| !left_out(&p.id)).cloned().collect();
    packages.extend(fetched.iter().map(|f| f.package.clone()));
    Ok((catalog, packages))
}

/// The package list a joined game runs with: this client's own list with
/// every package the server sent in place of the local one of the same id,
/// less the `dropped` ones the server does not run. Downloads must lie
/// under `root` (the cache is `root/.downloads`), so their bricks, weapons
/// and vehicles load like any other Add-On's.
pub fn joined_set(
    root: &std::path::Path,
    set: &PackageSet,
    fetched: &[Fetched],
    dropped: &[PackageRef],
) -> Result<PackageSet> {
    let root = root.canonicalize()?;
    let mut packages: Vec<PackageEntry> = set
        .packages
        .iter()
        .filter(|e| {
            !fetched.iter().any(|f| f.package.id == e.id) && !dropped.iter().any(|d| d.id == e.id)
        })
        .cloned()
        .collect();
    for f in fetched {
        let dir = f.dir.canonicalize()?;
        let Ok(relative) = dir.strip_prefix(&root) else {
            bail!("{} was downloaded outside the game folder", f.package);
        };
        let relative = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        packages.push(PackageEntry {
            id: f.package.id.clone(),
            version: f.package.version.clone(),
            side: f.package.side,
            dir: relative,
            role: None,
        });
    }
    Ok(PackageSet {
        schema_version: set.schema_version,
        packages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_package::packages::Side;

    fn scratch(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("bri-mods-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn package(root: &std::path::Path, id: &str, manifest: bool) -> Fetched {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        if manifest {
            std::fs::write(
                dir.join(MANIFEST_FILE),
                serde_json::json!({
                    "schema_version": 1, "id": id, "version": "1.0.0", "api": 1,
                    "name": id, "license": "CC0-1.0", "capabilities": [],
                    "provides": [{ "kind": "model", "id": format!("{id}:model/cube"), "file": "cube.json" }],
                })
                .to_string(),
            )
            .unwrap();
            std::fs::write(
                dir.join("cube.json"),
                r#"{ "schema_version": 1, "boxes": [{ "center": [0.0, 0.5, 0.0], "size": [1.0, 1.0, 1.0], "color": [0.4, 0.8, 0.3, 1.0] }] }"#,
            )
            .unwrap();
        }
        let (hash, size) = bri_package::environment::hash_dir(&dir).unwrap();
        Fetched {
            package: PackageRef {
                id: id.into(),
                version: "1.0.0".into(),
                side: Side::Shared,
                hash,
                size,
            },
            dir,
            downloaded: size,
        }
    }

    #[test]
    fn downloaded_add_ons_load_and_replace_the_local_list() {
        let root = scratch("load");
        let blocks = package(&root, "blocks", true);
        let base = PackageRef {
            id: "v20-bricks".into(),
            version: "1.0.0".into(),
            side: Side::Shared,
            hash: "ab".repeat(32),
            size: 1,
        };
        let (catalog, packages) = load_fetched(
            &root,
            &PackageSet {
                schema_version: 1,
                packages: Vec::new(),
            },
            std::slice::from_ref(&base),
            std::slice::from_ref(&blocks),
            &[],
        )
        .unwrap();
        assert!(catalog.model("blocks:model/cube").is_some());
        assert_eq!(packages, [base, blocks.package]);
    }

    #[test]
    fn different_base_content_is_refused_not_pretended() {
        let root = scratch("base");
        let other_base = package(&root, "v20-bricks", false);
        let error = load_fetched(
            &root,
            &PackageSet {
                schema_version: 1,
                packages: Vec::new(),
            },
            &[],
            &[other_base],
            &[],
        )
        .unwrap_err();
        assert!(
            format!("{error:#}").contains("base game content"),
            "{error:#}"
        );
    }
}
