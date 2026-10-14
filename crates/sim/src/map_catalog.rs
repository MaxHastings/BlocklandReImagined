//! Which maps exist and where each one's native files are: the base game's
//! converted maps and every enabled Add-On's `map` content, in one list.
//!
//! A map's files are a native map bundle (`bundle.json`, its scene and the
//! assets it names) in one directory. The base game's bundle holds every
//! converted v20 map; a map Add-On's holds its own maps, each id in the
//! Add-On's namespace (`<package>:map/<name>`). Everything that loads a map
//! (collision, rendering, Change Map, a dedicated server) asks this list for
//! the map's directory instead of assuming the base bundle
//! (docs/architecture/map-packages.md).
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Largest `bundle.json` a map Add-On may ship.
pub const MAX_BUNDLE_INDEX_BYTES: u64 = 1024 * 1024;

/// One map and the directory its native bundle is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapSource {
    pub id: String,
    pub name: String,
    /// The directory holding the map's `bundle.json`.
    pub root: PathBuf,
    /// The Add-On that provides it; `None` for the base game's maps.
    pub package: Option<String>,
}

/// An Add-On's declared `map` content: the provided id and the bundle
/// index file it names (`content_identity::map_providers`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapProvider {
    pub package: String,
    pub id: String,
    pub bundle: PathBuf,
}

/// Every map a game can be played on, base maps first in bundle order,
/// then Add-On maps in package order. Cheap to clone.
#[derive(Clone, Debug, Default)]
pub struct MapCatalog {
    maps: Arc<Vec<MapSource>>,
}

impl MapCatalog {
    /// The base bundle's integrated maps ([`crate::map::LOADABLE_MAPS`])
    /// and each provider's map. A provider whose bundle does not hold
    /// exactly its declared map, or a map id given twice, is an error.
    pub fn new(base: &Path, providers: &[MapProvider]) -> Result<Self> {
        let mut maps = Vec::new();
        for (id, name) in bundle_index(&base.join("bundle.json"))? {
            if crate::map::LOADABLE_MAPS.contains(&id.as_str()) {
                maps.push(MapSource {
                    id,
                    name,
                    root: base.to_path_buf(),
                    package: None,
                });
            }
        }
        for provider in providers {
            let prefix = format!(
                "{}:map/",
                bri_package::id::content_namespace(&provider.package)
            );
            ensure!(
                provider
                    .id
                    .strip_prefix(&prefix)
                    .is_some_and(|name| !name.is_empty()),
                "Add-On {}: a map's id must be {prefix}<name>, not {}",
                provider.package,
                provider.id
            );
            let meta = std::fs::metadata(&provider.bundle)
                .with_context(|| format!("Add-On {}: map file is missing", provider.package))?;
            ensure!(
                meta.is_file() && meta.len() <= MAX_BUNDLE_INDEX_BYTES,
                "Add-On {}: a map's bundle.json must be a file of at most 1 MiB",
                provider.package
            );
            ensure!(
                provider
                    .bundle
                    .file_name()
                    .is_some_and(|n| n == "bundle.json"),
                "Add-On {}: a map's file must be its bundle.json",
                provider.package
            );
            let root = provider
                .bundle
                .parent()
                .context("Map bundle has no directory")?
                .to_path_buf();
            let index = bundle_index(&provider.bundle)
                .with_context(|| format!("Add-On {}: reading its map", provider.package))?;
            let [(id, name)] = <[_; 1]>::try_from(index).map_err(|_| {
                anyhow::anyhow!(
                    "Add-On {}: a map's bundle.json lists exactly one map",
                    provider.package
                )
            })?;
            ensure!(
                id == provider.id,
                "Add-On {}: its bundle.json holds map {id}, but it provides {}",
                provider.package,
                provider.id
            );
            maps.push(MapSource {
                id,
                name,
                root,
                package: Some(provider.package.clone()),
            });
        }
        let mut seen = BTreeSet::new();
        for map in &maps {
            ensure!(seen.insert(&map.id), "Map {} is provided twice", map.id);
        }
        Ok(Self {
            maps: Arc::new(maps),
        })
    }

    pub fn get(&self, id: &str) -> Option<&MapSource> {
        self.maps.iter().find(|m| m.id == id)
    }
    pub fn contains(&self, id: &str) -> bool {
        self.get(id).is_some()
    }
    /// The directory of `id`'s native bundle.
    pub fn root(&self, id: &str) -> Result<&Path> {
        Ok(&self
            .get(id)
            .with_context(|| format!("Map {id} is not installed or not integrated yet"))?
            .root)
    }
    pub fn iter(&self) -> impl Iterator<Item = &MapSource> {
        self.maps.iter()
    }
}

/// The `(id, name)` of every map a bundle index lists, in order.
fn bundle_index(path: &Path) -> Result<Vec<(String, String)>> {
    let bundle: serde_json::Value = serde_json::from_slice(
        &std::fs::read(path).with_context(|| format!("Reading {}", path.display()))?,
    )?;
    ensure!(
        bundle
            .get("schema_version")
            .is_none_or(|v| v.as_u64() == Some(1)),
        "Unsupported native map bundle schema"
    );
    bundle["maps"]
        .as_array()
        .context("Missing native map index")?
        .iter()
        .map(|m| {
            let id = m["id"].as_str().context("Map without an id")?;
            ensure!(!id.is_empty(), "Map without an id");
            let name = m["name"].as_str().map_or_else(|| short(id), str::to_owned);
            Ok((id.to_owned(), name))
        })
        .collect()
}

/// `slate` for `v20/add-ons/map_slate/slate.mis`; `main` for
/// `my-map:map/main`.
pub fn short(id: &str) -> String {
    let file = id.rsplit(['/', ':']).next().unwrap_or(id);
    file.strip_suffix(".mis")
        .unwrap_or(file)
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(dir: &Path, maps: &[(&str, &str)]) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let maps: Vec<_> = maps
            .iter()
            .map(|(id, name)| serde_json::json!({"id": id, "name": name, "file": "scene.json"}))
            .collect();
        let path = dir.join("bundle.json");
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({"schema_version": 1, "maps": maps})).unwrap(),
        )
        .unwrap();
        path
    }
    fn provider(package: &str, id: &str, bundle: PathBuf) -> MapProvider {
        MapProvider {
            package: package.into(),
            id: id.into(),
            bundle,
        }
    }

    #[test]
    fn lists_integrated_base_maps_then_add_on_maps() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base");
        bundle(
            &base,
            &[
                (crate::map::LOADABLE_MAPS[0], "Bedroom"),
                ("v20/add-ons/map_other/other.mis", "Not integrated"),
            ],
        );
        let added = bundle(&dir.path().join("hall"), &[("hall:map/main", "Hall")]);
        let catalog = MapCatalog::new(&base, &[provider("hall", "hall:map/main", added)]).unwrap();
        let ids: Vec<_> = catalog.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, [crate::map::LOADABLE_MAPS[0], "hall:map/main"]);
        assert_eq!(
            catalog.root("hall:map/main").unwrap(),
            dir.path().join("hall")
        );
        assert_eq!(
            catalog.get("hall:map/main").unwrap().package.as_deref(),
            Some("hall")
        );
        assert!(!catalog.contains("v20/add-ons/map_other/other.mis"));
        assert!(catalog.root("missing").is_err());
    }

    #[test]
    fn refuses_maps_outside_the_add_ons_namespace_or_its_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base");
        bundle(&base, &[]);
        let foreign = bundle(
            &dir.path().join("a"),
            &[("v20/add-ons/map_slate/slate.mis", "x")],
        );
        let error = MapCatalog::new(
            &base,
            &[provider("hall", "v20/add-ons/map_slate/slate.mis", foreign)],
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("hall:map/<name>"));
        let other = bundle(&dir.path().join("b"), &[("hall:map/other", "x")]);
        let error =
            MapCatalog::new(&base, &[provider("hall", "hall:map/main", other)]).unwrap_err();
        assert!(format!("{error:#}").contains("holds map hall:map/other"));
        let two = bundle(
            &dir.path().join("c"),
            &[("hall:map/main", "x"), ("hall:map/two", "y")],
        );
        assert!(MapCatalog::new(&base, &[provider("hall", "hall:map/main", two)]).is_err());
    }

    #[test]
    fn refuses_a_map_given_twice() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base");
        bundle(&base, &[]);
        let a = bundle(&dir.path().join("a"), &[("hall:map/main", "x")]);
        let b = bundle(&dir.path().join("b"), &[("hall:map/main", "y")]);
        let error = MapCatalog::new(
            &base,
            &[
                provider("hall", "hall:map/main", a),
                provider("hall", "hall:map/main", b),
            ],
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("provided twice"));
    }

    #[test]
    fn short_names() {
        assert_eq!(short("v20/add-ons/map_slate/slate.mis"), "slate");
        assert_eq!(short("hall:map/Main"), "main");
    }
}
