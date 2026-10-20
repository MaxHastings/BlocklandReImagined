//! A map Add-On's regions from `--regions FILE.json` (docs/modding/maps.md): a [`bri_content::regions::RegionMap`] written in the glTF
//! file's own frame and length unit. The importer moves the planes into the native frame and the import's scale exactly as it does the
//! map's meshes, scales the fog distances, and lists the file in the bundle's `regions` table.
use super::map_scatter::Put;
use super::{Options, Report, point};
use anyhow::{Context, Result};
use bri_content::regions::RegionMap;

/// The converted map and its file name below `map/`, when the import names one.
pub(super) fn regions(
    opts: &Options,
    id: &str,
    put: Put<'_>,
    report: &mut Report,
) -> Result<Option<String>> {
    let Some(path) = &opts.regions else {
        return Ok(None);
    };
    let scale = opts.scale.unwrap_or(1.0);
    let text =
        std::fs::read(path).with_context(|| format!("Reading regions {}", path.display()))?;
    let mut map: RegionMap = serde_json::from_slice(&text).context("Reading the map's regions")?;
    map.validate()?;
    for plane in &mut map.planes {
        let n = point(glam::Vec3::new(plane[0], plane[1], plane[2]));
        *plane = [n.x, n.y, n.z, plane[3] * scale];
    }
    for region in &mut map.regions {
        if let Some(fog) = &mut region.fog {
            fog.start *= scale;
            fog.end *= scale;
        }
        for sun in &mut region.suns {
            let d = point(glam::Vec3::from(sun.direction));
            sun.direction = d.to_array();
            sun.position = (point(glam::Vec3::from(sun.position)) * scale).to_array();
        }
    }
    map.validate()?;
    let file = format!("{id}.regions.json");
    put(&format!("map/{file}"), &serde_json::to_vec(&map)?, report)?;
    Ok(Some(file))
}
