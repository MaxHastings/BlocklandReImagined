//! A map Add-On's zones from `--mapdata DIR` (docs/modding/maps.md): the folder holds one `*-mapdata.json` (the zone table and the portals
//! between zones) and one `*-zone-sections.json` (for each primitive of the world mesh, which zone each run of its triangles lies in), both
//! in the glTF file's own frame and length unit. The importer splits the world mesh's primitives by zone, moves the portals into the native
//! frame and the import's scale as it does the map's meshes, and lists the zone graph ([`bri_content::zones`]) in the bundle's `zones` table.
//! Zone numbers are the map's region numbers. Anything that does not fit the mesh fails the import: a wrong zone hides geometry.
use super::map_scatter::Put;
use super::{Options, Report, point};
use anyhow::{Context, Result, bail, ensure};
use bri_content::zones::{Portal, Zone, ZoneGraph};
use glam::Vec3;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The world mesh's triangle runs: for each of its primitives, `(triangle count, zone)` in triangle order.
pub(super) struct Sections {
    primitives: Vec<Vec<(u32, u16)>>,
}

impl Sections {
    /// The runs of primitive `index`, which must have `triangles` triangles.
    pub(super) fn runs(&self, index: usize, triangles: usize) -> Result<&[(u32, u16)]> {
        let runs = self
            .primitives
            .get(index)
            .with_context(|| format!("The map data lists no zones for world primitive {index}"))?;
        let total: usize = runs.iter().map(|r| r.0 as usize).sum();
        ensure!(
            total == triangles,
            "World primitive {index} has {triangles} triangles but the map data zones {total}"
        );
        Ok(runs)
    }
}

/// The one file in `dir` whose name ends with `suffix`.
fn find(dir: &Path, suffix: &str) -> Result<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("Reading {}", dir.display()))? {
        let path = entry?.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(suffix))
        {
            found.push(path);
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        n => bail!(
            "{} must hold exactly one *{suffix}, found {n}",
            dir.display()
        ),
    }
}

fn read(path: &Path) -> Result<Value> {
    serde_json::from_slice(
        &std::fs::read(path).with_context(|| format!("Reading {}", path.display()))?,
    )
    .with_context(|| format!("Parsing {}", path.display()))
}

fn vec3(v: &Value, what: &str) -> Result<Vec3> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 3)
        .with_context(|| format!("{what} is not three numbers"))?;
    let n = |i: usize| {
        a[i].as_f64()
            .map(|f| f as f32)
            .with_context(|| format!("{what} has a non-number"))
    };
    Ok(Vec3::new(n(0)?, n(1)?, n(2)?))
}

fn number(v: &Value, what: &str) -> Result<u64> {
    v.as_u64()
        .with_context(|| format!("{what} is not a whole number"))
}

/// The world mesh's runs from `--mapdata`, when the import names it.
pub(super) fn sections(opts: &Options) -> Result<Option<Sections>> {
    let Some(dir) = &opts.mapdata else {
        return Ok(None);
    };
    let file = read(&find(dir, "-zone-sections.json")?)?;
    let rows = file["groups"]["world"]
        .as_array()
        .context("The zone sections have no world group")?;
    let mut primitives = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let mut runs = Vec::new();
        let mut at = 0;
        for run in row["runs"].as_array().context("A section has no runs")? {
            let run = run
                .as_array()
                .filter(|r| r.len() >= 3)
                .context("A run is not [first, count, zone, ..]")?;
            let (first, count) = (
                number(&run[0], "A run's start")?,
                number(&run[1], "A run's length")?,
            );
            ensure!(
                first == at,
                "World section {i}'s runs are not in triangle order"
            );
            let zone = u16::try_from(number(&run[2], "A run's zone")?)
                .context("A zone number is past 65535")?;
            runs.push((count as u32, zone));
            at += count;
        }
        ensure!(
            at == number(&row["triangles"], "A section's triangle count")?,
            "World section {i}'s runs do not cover its triangles"
        );
        primitives.push(runs);
    }
    Ok(Some(Sections { primitives }))
}

/// The zone graph and its file name below `map/`, when the import names map data.
pub(super) fn zones(
    opts: &Options,
    id: &str,
    put: Put<'_>,
    report: &mut Report,
) -> Result<Option<String>> {
    let Some(dir) = &opts.mapdata else {
        return Ok(None);
    };
    let scale = opts.scale.unwrap_or(1.0);
    let data = read(&find(dir, "-mapdata.json")?)?;
    let mut graph = ZoneGraph::default();
    for (i, zone) in data["zones"]
        .as_array()
        .context("The map data has no zones")?
        .iter()
        .enumerate()
    {
        ensure!(
            number(&zone["zone"], "A zone's number")? == i as u64,
            "Zone {i} is out of order"
        );
        let bounds = match &zone["bounds"] {
            Value::Null => None,
            b => {
                let (a, c) = (
                    point(vec3(&b["min"], "A zone's bounds")?) * scale,
                    point(vec3(&b["max"], "A zone's bounds")?) * scale,
                );
                Some([a.min(c).to_array(), a.max(c).to_array()])
            }
        };
        graph.zones.push(Zone {
            bounds,
            indoor: zone["indoor"]
                .as_bool()
                .context("A zone has no indoor flag")?,
        });
    }
    for portal in data["portals"]
        .as_array()
        .context("The map data has no portals")?
    {
        // Sky portals open onto the backdrop, not another zone.
        if portal["kind"] != "zone" {
            continue;
        }
        let points = portal["points"]
            .as_array()
            .context("A portal has no points")?
            .iter()
            .map(|p| Ok((point(vec3(p, "A portal point")?) * scale).to_array()))
            .collect::<Result<Vec<_>>>()?;
        let plane = portal["plane"]
            .as_array()
            .filter(|p| p.len() == 4)
            .context("A portal has no plane")?;
        let n = |i: usize| {
            plane[i]
                .as_f64()
                .map(|f| f as f32)
                .context("A portal plane has a non-number")
        };
        let normal = point(Vec3::new(n(0)?, n(1)?, n(2)?));
        graph.portals.push(Portal {
            points,
            plane: [normal.x, normal.y, normal.z, n(3)? * scale],
            front: u16::try_from(number(&portal["front_zone"], "A portal's front zone")?)?,
            back: u16::try_from(number(&portal["back_zone"], "A portal's back zone")?)?,
        });
    }
    graph.validate()?;
    let file = format!("{id}.zones.json");
    put(&format!("map/{file}"), &serde_json::to_vec(&graph)?, report)?;
    Ok(Some(file))
}
