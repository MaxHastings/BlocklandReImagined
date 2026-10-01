//! A made-up content root for tests that have no generated v20 content: a
//! `packages.json` naming the base game's packages, and every package the
//! server reads ([`crate::dedicated::load_packages`]) written into the
//! folder it names, through the same formats (and checks) the converted
//! packs go through. The client adds its own packages on top
//! (`bri_client::testing::content_root`).
//!
//! Every value is invented; nothing is read from, measured on, or copied
//! out of the original game's files. Ids follow what the engine looks up
//! (the base package ids, the `Letters/A` default print).
//!
//! - [`items`]: the weapons pack and its item presentation pack.
//! - [`vehicles`]: the vehicle pack with its models, clips and horse rig.
//! - [`write_root`]: the whole server root.

pub mod items;
pub mod vehicles;

use anyhow::{Context, Result};
use bri_content::brick::{Face, Surface};
use bri_content::testing::{bricks, map_bundle, write_file};
use bri_package::packages::{PackageEntry, PackageSet};
use bri_sim::definitions::Special;
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// The map a root written for the server alone offers, when the test
/// names none of its own.
pub const MAP: &str = "fixture/maps/room.mis";

/// Bricks the made-up catalog offers in the brick menu, beyond
/// `bri_sim::testing`'s: (id, name, studs, plates).
pub const MENU_BRICKS: [(&str, &str, [u8; 2], u16); 4] = [
    ("fixture/brick/1x1", "1x1 Fixture", [1, 1], 3),
    ("fixture/brick/1x2", "1x2 Fixture", [1, 2], 3),
    ("fixture/brick/2x2", "2x2 Fixture", [2, 2], 3),
    ("fixture/brick/2x2f", "2x2F Fixture", [2, 2], 1),
];

/// The music loop the audio pack offers music bricks.
pub const MUSIC: &str = "Fixture_Tune";

/// The base packages the server reads, by role.
pub const SERVER_ROLES: [&str; 12] = [
    "map_bundle",
    "brick_catalog",
    "geometry",
    "effects",
    "brick_materials",
    "avatar",
    "audio",
    "weapons",
    "item_presentation",
    "vehicles",
    "events",
    "worlds",
];

/// What a written root holds that the client's root builds on.
pub struct ServerRoot {
    /// Each catalog brick's id and its icon's UI image name.
    pub brick_icons: Vec<(String, String)>,
    /// Each print's icon UI image name.
    pub print_icons: Vec<String>,
}

/// The base game's package for `role`, in a folder named for the role.
pub fn package(role: &str) -> Result<PackageEntry> {
    let mut entry = PackageSet::base().role(role)?.clone();
    entry.dir = role.replace('_', "-");
    Ok(entry)
}

/// The folder `role`'s package fills under `root`, made if missing.
pub fn role_dir(root: &Path, role: &str) -> Result<PathBuf> {
    let dir = root.join(package(role)?.dir);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Write `root/packages.json` naming the base packages for `roles`.
pub fn write_packages(root: &Path, roles: &[&str]) -> Result<PackageSet> {
    let set = PackageSet {
        schema_version: 1,
        packages: roles.iter().map(|r| package(r)).collect::<Result<_>>()?,
    };
    set.validate().into_result()?;
    write_file(
        root,
        bri_package::packages::PACKAGES_FILE,
        &serde_json::to_vec_pretty(&set)?,
    )?;
    Ok(set)
}

/// Write `packages.json` and every package of [`SERVER_ROLES`] into `root`,
/// the map bundle offering one lit room for each id in `maps`.
pub fn write_root(root: &Path, maps: &[&str]) -> Result<ServerRoot> {
    let dir = |role: &str| role_dir(root, role);
    map_bundle::write_bundle(&dir("map_bundle")?, &map_bundle::rooms_for(maps))?;
    let brick_icons = write_bricks(&dir("brick_catalog")?, &dir("geometry")?)?;
    write_effects(&dir("effects")?)?;
    let print_icons = bricks::write_materials(&dir("brick_materials")?)?;
    bri_content::testing::avatar::write(&dir("avatar")?)?;
    write_audio(&dir("audio")?)?;
    items::write(&dir("item_presentation")?, &dir("weapons")?)?;
    vehicles::write_pack(&dir("vehicles")?)?;
    write_file(
        &dir("events")?,
        "catalog.json",
        &serde_json::to_vec_pretty(&bri_events::testing::catalog())?,
    )?;
    write_file(
        &dir("worlds")?,
        "report.json",
        &serde_json::to_vec_pretty(&json!({ "schema_version": 1, "saves": [] }))?,
    )?;
    write_packages(root, &SERVER_ROLES)?;
    Ok(ServerRoot {
        brick_icons,
        print_icons,
    })
}

/// `bri_sim::testing`'s bricks plus [`MENU_BRICKS`], each drawn as a box
/// ([`bricks::block`]), written as a native catalog: the catalog, its mesh
/// bindings and collision bodies, and one mesh file per brick. Returns each
/// brick's id and icon.
fn write_bricks(catalog_dir: &Path, geometry_dir: &Path) -> Result<Vec<(String, String)>> {
    let mut definitions: Vec<_> = bri_sim::testing::definitions()
        .entries
        .into_values()
        .collect();
    for (id, _, studs, plates) in MENU_BRICKS {
        definitions.push(bri_sim::testing::definition(
            id,
            studs,
            plates,
            Special::None,
            false,
        ));
    }
    let names: BTreeMap<&str, &str> = MENU_BRICKS.iter().map(|(id, n, ..)| (*id, *n)).collect();
    let look =
        |face: Face| -> (Surface, Option<[[f32; 4]; 4]>) { (bricks::default_surface(face), None) };
    let (mut entries, mut bindings, mut bodies, mut icons) = (vec![], vec![], vec![], vec![]);
    for d in definitions {
        let mut mesh = d.mesh.clone();
        mesh.quads = bricks::block(&mesh.id, mesh.footprint_studs, mesh.height_plates, look).quads;
        let name = mesh.id.rsplit('/').next().unwrap_or(&mesh.id).to_string();
        let file = format!("{}.brick.json", name.replace(['/', ':', '#'], "-"));
        write_file(geometry_dir, &file, &serde_json::to_vec(&mesh)?)?;
        bindings.push(json!({ "id": mesh.id, "native_mesh": file }));
        bodies.push(d.collision.clone());
        let display = names
            .get(mesh.id.as_str())
            .map(|n| n.to_string())
            .unwrap_or_else(|| format!("Fixture {name}"));
        let icon = format!("fixture/bricks/{name}");
        let mut other = serde_json::Map::new();
        if d.special == Special::Water {
            other.insert("iswaterbrick".into(), json!("1"));
        }
        entries.push(json!({
            "id": mesh.id, "display_name": display, "category": "Bricks",
            "subcategory": if mesh.height_plates == 1 { "Plates" } else { "Basic" },
            "mesh_id": mesh.id, "collision_source": null, "icon_source": icon,
            "print_aspect_ratio": null, "orientation_fix": 0, "can_cover": false,
            "indestructible": d.indestructible, "special_kind": null,
            "other_properties": other,
        }));
        icons.push((mesh.id.clone(), icon));
    }
    let json = |dir: &Path, file: &str, value: serde_json::Value| -> Result<()> {
        write_file(dir, file, &serde_json::to_vec_pretty(&value)?)?;
        Ok(())
    };
    json(
        catalog_dir,
        "stock-catalog.json",
        json!({ "schema_version": 1, "bricks": entries }),
    )?;
    json(
        catalog_dir,
        "catalog-audit.json",
        json!({ "resolved_meshes": bindings }),
    )?;
    json(
        catalog_dir,
        "native-collisions.json",
        json!({ "schema_version": 1, "bodies": bodies }),
    )?;
    Ok(icons)
}

/// An effects library with no lights, particles or emitters: the server
/// offers none. (The client's root writes a full one.)
fn write_effects(dir: &Path) -> Result<()> {
    let library = bri_content::effects::Library {
        schema_version: 1,
        lights: vec![],
        particles: vec![],
        emitters: vec![],
        textures: BTreeMap::new(),
    };
    library.validate()?;
    write_file(dir, "effects.json", &serde_json::to_vec(&library)?)?;
    Ok(())
}

/// The audio manifest as far as the server reads it: one music loop
/// ([`MUSIC`]) and one sound events may play. (The client's root writes the
/// full pack, clips and all, with the same two.)
fn write_audio(dir: &Path) -> Result<()> {
    let manifest = json!({
        "sounds": [{ "id": "fixture/sound/event", "name": "FixtureEventSound",
            "lists": ["event-param:Sound"] }],
        "triggers": [{ "key": format!("music-brick:{MUSIC}"), "sound": "fixture/music/tune" }],
    });
    write_file(dir, "manifest.json", &serde_json::to_vec_pretty(&manifest)?)?;
    Ok(())
}

/// A written root in a scratch folder, removed when dropped.
pub struct ScratchRoot {
    pub root: ServerRoot,
    pub scratch: bri_content::testing::ScratchDir,
}

impl ScratchRoot {
    pub fn new() -> Result<Self> {
        let scratch = bri_content::testing::ScratchDir::new("server-root")?;
        let root = write_root(scratch.path(), &[MAP]).context("writing the made-up server root")?;
        Ok(Self { root, scratch })
    }
    pub fn path(&self) -> &Path {
        self.scratch.path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server hosts the written root: every package it reads loads,
    /// the map has spawns, and the catalog offers the menu bricks.
    #[test]
    fn the_server_hosts_the_made_up_root() -> Result<()> {
        let root = ScratchRoot::new()?;
        let set = PackageSet::load_root(root.path())?;
        let world = bri_world::World::new("Fixture".into(), MAP.into(), vec![[1.0; 4]]);
        let host = crate::dedicated::load_packages(root.path(), &set, world)?;
        assert_eq!(host.environment.packages.len(), SERVER_ROLES.len());
        assert!(!host.spawn_points.is_empty());
        let definitions = &host.session.simulation().definitions.entries;
        for (id, ..) in MENU_BRICKS {
            assert!(definitions.contains_key(id), "{id} is not in the catalog");
        }
        Ok(())
    }
}
