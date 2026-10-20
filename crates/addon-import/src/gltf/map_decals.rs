//! A map Add-On's projector pictures from `--projector-pictures FILE.json` (docs/modding/maps.md): a [`bri_content::decals::DecalSet`] already in the native frame,
//! with each decal's `picture` a PNG beside the file. The pictures and the set are copied into the map's bundle and listed in its `decals`
//! table.
use super::map_scatter::Put;
use super::{Options, Report};
use anyhow::{Context, Result, ensure};
use bri_content::decals::DecalSet;
use std::collections::BTreeMap;

/// The decals file's name below `map/` and the pictures it needs listed as textures.
pub(super) struct Built {
    pub file: Option<String>,
    pub textures: BTreeMap<String, String>,
}

pub(super) fn decals(opts: &Options, id: &str, put: Put<'_>, report: &mut Report) -> Result<Built> {
    let mut built = Built {
        file: None,
        textures: BTreeMap::new(),
    };
    let Some(path) = &opts.projector_pictures else {
        return Ok(built);
    };
    let text = std::fs::read(path).with_context(|| format!("Reading decals {}", path.display()))?;
    let set: DecalSet = serde_json::from_slice(&text).context("Reading the map's decals")?;
    set.validate()?;
    ensure!(
        set.decals.is_empty(),
        "--projector-pictures carries pictures only; a game's projectors attach at run time and are not baked into the map"
    );
    let dir = path.parent().unwrap_or(std::path::Path::new("."));
    let mut copied = BTreeMap::new();
    for decal in &set.decals {
        if copied.contains_key(&decal.picture) {
            continue;
        }
        let bytes = std::fs::read(dir.join(&decal.picture))
            .with_context(|| format!("Reading decal picture {}", decal.picture))?;
        ensure!(
            bytes.starts_with(b"\x89PNG"),
            "Decal picture {} is not a PNG",
            decal.picture
        );
        let name = format!("{id}_decal_{}", decal.picture);
        put(&format!("map/{name}"), &bytes, report)?;
        built.textures.insert(name.clone(), name.clone());
        copied.insert(decal.picture.clone(), name);
    }
    let mut set = set;
    for decal in &mut set.decals {
        decal.picture = copied[&decal.picture].clone();
    }
    for picture in &mut set.pictures {
        let name = match copied.get(&picture.file) {
            Some(name) => name.clone(),
            None => {
                let bytes = std::fs::read(dir.join(&picture.file))
                    .with_context(|| format!("Reading decal picture {}", picture.file))?;
                ensure!(
                    bytes.starts_with(b"\x89PNG"),
                    "Decal picture {} is not a PNG",
                    picture.file
                );
                let name = format!("{id}_decal_{}", picture.file);
                put(&format!("map/{name}"), &bytes, report)?;
                built.textures.insert(name.clone(), name.clone());
                copied.insert(picture.file.clone(), name.clone());
                name
            }
        };
        picture.file = name;
    }
    report
        .warnings
        .extend(set.notes.iter().map(|n| format!("decals: {n}")));
    let file = format!("{id}.decals.json");
    put(&format!("map/{file}"), &serde_json::to_vec(&set)?, report)?;
    built.file = Some(file);
    Ok(built)
}
