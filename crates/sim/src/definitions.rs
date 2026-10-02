use anyhow::{Context, Result, ensure};
use bri_content::{
    brick::{Brick, Catalog},
    collision::{CollisionBody, CollisionLibrary},
};
use bri_world::{Brick as Placed, ContentRef};
use rapier3d::prelude::SharedShape;
use std::{collections::BTreeMap, path::Path};
/// Stock bricks whose behavior comes from their add-on script rather than
/// geometry alone; the session implements each natively.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Special {
    #[default]
    None,
    /// `isWaterBrick`: a swimmable, non-solid liquid volume.
    Water,
    Checkpoint,
    Teledoor,
    TreasureChest,
    /// The open chest shown briefly after a find.
    TreasureChestOpen,
    /// Uncarved pumpkin; a sword hit carves it.
    Pumpkin,
    /// `specialBrickType = "SpawnPoint"`: the base Spawn Point and every
    /// brick inheriting it (an Add-On's team spawn).
    SpawnPoint,
}
#[derive(Clone)]
pub struct Definition {
    pub mesh: Brick,
    pub collision: CollisionBody,
    pub shape: SharedShape,
    pub indestructible: bool,
    pub special: Special,
    /// Mirrored sides, drawn by each player's game (checked against `mesh`).
    pub reflection: Option<bri_content::brick::Reflection>,
    /// Sides open onto a linked brick (checked against `mesh`).
    pub link: Option<bri_content::brick::Link>,
    /// The glass a link's views took the place of, averaged (straight
    /// RGBA): what an unlinked opening shows.
    pub glass: [f32; 4],
}
/// World-space box of a placed brick's logical grid volume.
/// Every brick's catalog entry the game knows: the base game's in
/// `catalog_dir` and each Add-On's in `extras`, the same set
/// [`Definitions::load_with`] loads. Tool menus, the wrench and the host's
/// tool allowlist read it, so an Add-On brick is wrenched, named and
/// printed like a base one.
pub fn catalog_with(
    catalog_dir: &Path,
    extras: &[(String, std::path::PathBuf)],
) -> Result<Catalog> {
    let mut catalog: Catalog =
        serde_json::from_slice(&std::fs::read(catalog_dir.join("stock-catalog.json"))?)?;
    extend_catalog(&mut catalog, extras)?;
    Ok(catalog)
}
/// Add each Add-On's catalog entries in `extras` to `catalog` (see
/// [`catalog_with`]); brick ids are namespaced, so a duplicate is an error
/// naming the package.
pub fn extend_catalog(
    catalog: &mut Catalog,
    extras: &[(String, std::path::PathBuf)],
) -> Result<()> {
    let mut ids: std::collections::BTreeSet<String> =
        catalog.bricks.iter().map(|b| b.id.clone()).collect();
    for (dir, path) in extras {
        let extra: Catalog =
            serde_json::from_slice(&std::fs::read(path.join("stock-catalog.json"))?)
                .with_context(|| format!("{dir}: brick catalog"))?;
        ensure!(
            extra.schema_version == 1,
            "{dir}: unsupported brick catalog schema"
        );
        for entry in extra.bricks {
            ensure!(
                ids.insert(entry.id.clone()),
                "{dir}: brick {} is already defined",
                entry.id
            );
            catalog.bricks.push(entry);
        }
    }
    Ok(())
}
pub fn brick_box(brick: &Placed, mesh: &Brick) -> (glam::Vec3, glam::Vec3) {
    let (x, z) = if brick.quarter_turns % 2 == 1 {
        (mesh.footprint_studs[1], mesh.footprint_studs[0])
    } else {
        (mesh.footprint_studs[0], mesh.footprint_studs[1])
    };
    let half = glam::Vec3::new(
        x as f32 * 0.25,
        mesh.height_plates as f32 * 0.1,
        z as f32 * 0.25,
    );
    let center = glam::Vec3::from(brick.position);
    (center - half, center + half)
}
/// The swimmable volume of a water brick. Presentation comes from the brick's
/// own mesh; this is only the liquid players move through.
pub fn brick_water(brick: &Placed, definition: &Definition) -> Option<bri_content::water::Water> {
    if definition.special != Special::Water {
        return None;
    }
    // `createWaterZone` grows the brick's box by 0.1 in height, then sets its
    // centre 0.1 low: the zone reaches 0.15 below the brick and its surface
    // sits 0.05 under the brick's top.
    let (mut min, mut max) = brick_box(brick, &definition.mesh);
    min.y -= 0.15;
    max.y -= 0.05;
    let image = || bri_content::environment::Image {
        file: "brick-water".into(),
        source: String::new(),
        sha256: "0".repeat(64),
        width: 1,
        height: 1,
    };
    Some(bri_content::water::Water {
        schema_version: 1,
        node: 0,
        id: "brick-water".into(),
        min: min.to_array(),
        max: max.to_array(),
        repeat_period: None,
        liquid_type: "Water".into(),
        density: 1.0,
        viscosity: 40.0,
        surface: image(),
        shore: image(),
        reflection: None,
        opacity: 0.5,
        wave_amplitude: 0.0,
        flow: [0.0; 2],
        distortion: [0.0, 0.0, 1.0],
        tiles: [1.0; 2],
        depth_mask: false,
        depth_alpha: [0.0; 4],
        reflection_intensity: 0.0,
        parallax: 0.0,
        warnings: vec![],
        current: match definition.mesh.id.as_str() {
            // River and rapids zones push along the brick's forward vector
            // with 1000 and 3000 force on the 90-mass player.
            id if id.ends_with("8x river.blb") => {
                (brick.transform().transform_vector3(glam::Vec3::NEG_Z) * (1000.0 / 90.0))
                    .to_array()
            }
            id if id.ends_with("8x rapids.blb") => {
                (brick.transform().transform_vector3(glam::Vec3::NEG_Z) * (3000.0 / 90.0))
                    .to_array()
            }
            _ => [0.0; 3],
        },
    })
}
#[derive(Default, Clone)]
pub struct Definitions {
    pub entries: BTreeMap<String, Definition>,
}
impl Definitions {
    pub fn get(&self, brick: &Placed) -> Result<&Definition> {
        let ContentRef::Resolved(id) = &brick.definition else {
            anyhow::bail!("Unresolved brick definition")
        };
        self.by_id(id)
    }
    /// The definition with id `id`.
    pub fn by_id(&self, id: &str) -> Result<&Definition> {
        self.entries
            .get(id)
            .with_context(|| format!("Missing native definition {id}"))
    }
    /// [`Self::load`] plus other packages' brick catalogs, each a directory in
    /// the stock catalog layout holding its own meshes (as `bri-import-addon`
    /// writes to `assets/brick-catalog/`). A package brick whose mesh binding
    /// names no file reuses the shape (mesh and, unless the package bakes
    /// its own, collision) of a brick already loaded with the same `mesh_id`:
    /// an Add-On brick built on a base game brick, as a v20 datablock
    /// inheriting its parent's `brickFile`, ships no copy of that geometry.
    /// Brick ids are namespaced, so a duplicate is an error naming the package.
    pub fn load_with(
        catalog_dir: &Path,
        content: &Path,
        extras: &[(String, std::path::PathBuf)],
    ) -> Result<Self> {
        let mut out = Self::load(catalog_dir, content)?;
        for (dir, catalog) in extras {
            for (id, definition) in Self::load_on(catalog, catalog, &out)?.entries {
                ensure!(
                    !out.entries.contains_key(&id),
                    "{dir}: brick {id} is already defined"
                );
                out.entries.insert(id, definition);
            }
        }
        Ok(out)
    }
    pub fn load(catalog_dir: &Path, content: &Path) -> Result<Self> {
        Self::load_on(catalog_dir, content, &Self::default())
    }
    /// [`Self::load`], with bricks that name no mesh file taking theirs from
    /// `shared` (see [`Self::load_with`]).
    fn load_on(catalog_dir: &Path, content: &Path, shared: &Self) -> Result<Self> {
        let catalog: Catalog =
            serde_json::from_slice(&std::fs::read(catalog_dir.join("stock-catalog.json"))?)?;
        let audit: serde_json::Value =
            serde_json::from_slice(&std::fs::read(catalog_dir.join("catalog-audit.json"))?)?;
        let library: CollisionLibrary =
            serde_json::from_slice(&std::fs::read(catalog_dir.join("native-collisions.json"))?)?;
        ensure!(
            catalog.schema_version == 1 && library.schema_version == 1,
            "Unsupported catalog/collision schema"
        );
        let mut collisions: BTreeMap<_, _> = library
            .bodies
            .into_iter()
            .map(|b| (b.id.clone(), b))
            .collect();
        let mut out = Self::default();
        for entry in catalog.bricks {
            let resolved = audit["resolved_meshes"]
                .as_array()
                .context("Missing native mesh bindings")?
                .iter()
                .find(|r| r["id"].as_str() == Some(&entry.id))
                .context("Missing catalog mesh binding")?;
            let own = collisions.remove(&entry.id);
            let own_collision = own.is_some() && resolved.get("native_mesh").is_none();
            let (mesh, collision, shape) = match resolved.get("native_mesh") {
                Some(file) => {
                    let file = file.as_str().context("Missing mesh file")?;
                    ensure!(
                        !file.contains(['/', '\\', ':']),
                        "Invalid native mesh filename"
                    );
                    let mesh: Brick = serde_json::from_slice(&std::fs::read(content.join(file))?)?;
                    mesh.validate()?;
                    ensure!(mesh.id == entry.mesh_id, "Mesh identity mismatch");
                    let collision = own.context("Missing native collision recipe")?;
                    let shape = bri_physics::content::collider(&collision)?
                        .build()
                        .shared_shape()
                        .clone();
                    (mesh, collision, shape)
                }
                None => {
                    let base = shared
                        .entries
                        .values()
                        .find(|d| d.mesh.id == entry.mesh_id)
                        .with_context(|| {
                            format!(
                                "Brick {}: its shape {} is not a loaded brick's",
                                entry.id, entry.mesh_id
                            )
                        })?;
                    match own {
                        Some(collision) => {
                            let shape = bri_physics::content::collider(&collision)?
                                .build()
                                .shared_shape()
                                .clone();
                            (base.mesh.clone(), collision, shape)
                        }
                        None => (
                            base.mesh.clone(),
                            CollisionBody {
                                id: entry.id.clone(),
                                ..base.collision.clone()
                            },
                            base.shape.clone(),
                        ),
                    }
                }
            };
            // Another size of the shape: its own mesh identity, and the
            // shape's collision stretched the way its faces are, unless the
            // package bakes its own or bodies pass through its openings.
            let (mut mesh, collision, shape) = match entry.stretch {
                None => (mesh, collision, shape),
                Some(size) => {
                    let [w, d, h] = size;
                    let id = format!("{}#{w}x{d}x{h}", entry.mesh_id);
                    let context = || format!("Brick {}", entry.id);
                    let map = mesh.stretching(size).with_context(context)?;
                    let mesh = mesh.stretched(&id, size).with_context(context)?;
                    let passes = entry.link.as_ref().is_some_and(|l| l.pass);
                    if own_collision || passes {
                        (mesh, collision, shape)
                    } else {
                        let collision = collision.stretched(&entry.id, map);
                        collision.validate().with_context(context)?;
                        let shape = bri_physics::content::collider(&collision)?
                            .build()
                            .shared_shape()
                            .clone();
                        (mesh, collision, shape)
                    }
                }
            };
            let special = if entry
                .other_properties
                .get("iswaterbrick")
                .is_some_and(|v| v == "true" || v == "1")
            {
                Special::Water
            } else {
                match entry.id.as_str() {
                    "v20/brick/brickcheckpointdata" => Special::Checkpoint,
                    "v20/brick/brickteledoordata" => Special::Teledoor,
                    "v20/brick/bricktreasurechestdata" => Special::TreasureChest,
                    "v20/brick/bricktreasurechestopendata" => Special::TreasureChestOpen,
                    "v20/brick/brickpumpkinbasedata" => Special::Pumpkin,
                    "v20/brick/brickspawnpointdata" => Special::SpawnPoint,
                    _ if entry
                        .special_kind
                        .as_deref()
                        .is_some_and(|k| k.eq_ignore_ascii_case("SpawnPoint")) =>
                    {
                        Special::SpawnPoint
                    }
                    _ => Special::None,
                }
            };
            let mut glass = [0.8, 0.9, 1.0, 0.35];
            let (mut collision, mut shape) = (collision, shape);
            if let Some(link) = &entry.link {
                link.validate(&mesh)
                    .with_context(|| format!("Brick {}", entry.id))?;
                // The views take the place of the window's glass, as a
                // mirror does; an unlinked opening shows that glass again.
                let cover = bri_content::brick::Reflection {
                    faces: link.faces.clone(),
                    depth: link.depth,
                    inset: link.inset,
                    tint: [1.0; 3],
                    strength: 1.0,
                };
                let covered: Vec<bool> = mesh
                    .quads
                    .iter()
                    .map(|q| cover.replaces(&mesh, q))
                    .collect();
                let panes: Vec<[f32; 4]> = mesh
                    .quads
                    .iter()
                    .zip(&covered)
                    .filter(|(_, c)| **c)
                    .filter_map(|(q, _)| q.colors)
                    .flatten()
                    .collect();
                if !panes.is_empty() {
                    let sum = panes
                        .iter()
                        .fold(glam::Vec4::ZERO, |a, c| a + glam::Vec4::from(*c));
                    glass = (sum / panes.len() as f32)
                        .clamp(glam::Vec4::ZERO, glam::Vec4::ONE)
                        .to_array();
                }
                let mut covered = covered.into_iter();
                mesh.quads.retain(|_| !covered.next().unwrap_or(false));
                if link.pass {
                    // Bodies pass through: the brick is a frame around its
                    // openings.
                    collision = CollisionBody {
                        id: entry.id.clone(),
                        parts: link
                            .frame_boxes(&mesh)
                            .into_iter()
                            .map(|b| bri_content::collision::Part::Box {
                                center: b.center,
                                size: b.size,
                            })
                            .collect(),
                    };
                    shape = bri_physics::content::collider(&collision)?
                        .build()
                        .shared_shape()
                        .clone();
                }
            }
            if let Some(reflection) = &entry.reflection {
                reflection
                    .validate(&mesh)
                    .with_context(|| format!("Brick {}", entry.id))?;
                // What the mirror covers is not drawn: a borrowed window
                // shape loses its glass.
                let covered: Vec<bool> = mesh
                    .quads
                    .iter()
                    .map(|q| reflection.replaces(&mesh, q))
                    .collect();
                let mut covered = covered.into_iter();
                mesh.quads.retain(|_| !covered.next().unwrap_or(false));
            }
            ensure!(
                out.entries
                    .insert(
                        entry.id,
                        Definition {
                            mesh,
                            collision,
                            shape,
                            indestructible: entry.indestructible,
                            special,
                            reflection: entry.reflection,
                            link: entry.link,
                            glass,
                        }
                    )
                    .is_none(),
                "Duplicate brick definition"
            );
        }
        ensure!(collisions.is_empty(), "Unbound collision recipes");
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn catalog(
        dir: &Path,
        entries: &[serde_json::Value],
        meshes: &[(&str, serde_json::Value)],
        bodies: &[&str],
    ) {
        std::fs::create_dir_all(dir).unwrap();
        let write = |name: &str, value: serde_json::Value| {
            std::fs::write(dir.join(name), serde_json::to_vec(&value).unwrap()).unwrap()
        };
        write(
            "stock-catalog.json",
            json!({ "schema_version": 1, "bricks": entries }),
        );
        write(
            "catalog-audit.json",
            json!({ "resolved_meshes": meshes.iter().map(|(id, binding)| {
                let mut binding = binding.clone();
                binding["id"] = json!(id);
                binding
            }).collect::<Vec<_>>() }),
        );
        write(
            "native-collisions.json",
            json!({ "schema_version": 1, "bodies": bodies.iter().map(|id| json!({
                "id": id, "parts": [{ "type": "box", "center": [0.0, 0.0, 0.0], "size": [2.0, 0.6, 0.5] }]
            })).collect::<Vec<_>>() }),
        );
    }
    fn entry(id: &str, mesh: &str, reflection: Option<serde_json::Value>) -> serde_json::Value {
        json!({
            "id": id, "display_name": id, "category": "Special", "subcategory": "",
            "mesh_id": mesh, "collision_source": null, "icon_source": "",
            "print_aspect_ratio": null, "orientation_fix": 0, "can_cover": false,
            "indestructible": false, "special_kind": null, "other_properties": {},
            "reflection": reflection
        })
    }

    /// The host's tool allowlist and the wrench read the Add-Ons' bricks
    /// too: the Portal brick's Name pairs it, so it must be wrenchable.
    #[test]
    fn the_full_catalog_holds_add_on_bricks_once() {
        let base = std::env::temp_dir().join(format!("bri-catalog-with-{}", std::process::id()));
        catalog(&base, &[entry("plate", "plate", None)], &[], &[]);
        let portal = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/brick_portal/assets/brick-catalog");
        let full = catalog_with(&base, &[("brick_portal".into(), portal.clone())]).unwrap();
        let twice = catalog_with(&base, &[("a".into(), portal.clone()), ("b".into(), portal)]);
        std::fs::remove_dir_all(&base).unwrap();
        let ids: Vec<_> = full.bricks.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids[0], "plate");
        assert!(ids.contains(&"brick_portal:brick/brickportal1x14x10data"));
        assert!(twice.unwrap_err().to_string().contains("already defined"));
    }

    #[test]
    fn an_add_on_brick_reuses_a_base_bricks_shape_without_copying_it() {
        let root = std::env::temp_dir().join(format!("bri-shared-shape-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (base, addon) = (root.join("base"), root.join("addon"));
        catalog(
            &base,
            &[entry("v20/brick/window", "v20/window.blb", None)],
            &[(
                "v20/brick/window",
                json!({ "native_mesh": "window.brick.json" }),
            )],
            &["v20/brick/window"],
        );
        std::fs::write(
            base.join("window.brick.json"),
            serde_json::to_vec(&json!({
                "schema_version": 1, "id": "v20/window.blb", "footprint_studs": [4, 1],
                "height_plates": 3, "attachment_rows": ["bbbb", "bbbb", "bbbb"], "collision_boxes": [],
                "needs_external_collision": false, "coverage": null, "quads": [{
                    "face": "omni", "surface": "side", "colors": null,
                    "vertices": [
                        { "position": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0], "uv": [0.0, 0.0] },
                        { "position": [1.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0], "uv": [0.0, 0.0] },
                        { "position": [1.0, 1.0, 0.0], "normal": [0.0, 0.0, 1.0], "uv": [0.0, 0.0] },
                        { "position": [0.0, 1.0, 0.0], "normal": [0.0, 0.0, 1.0], "uv": [0.0, 0.0] }
                    ]
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        let mirror = json!({ "faces": ["north", "south"], "depth": 0.5 });
        catalog(
            &addon,
            &[entry(
                "mirror:brick/mirror",
                "v20/window.blb",
                Some(mirror.clone()),
            )],
            &[("mirror:brick/mirror", json!({}))],
            &[],
        );
        let extras = [("addons/mirror".to_string(), addon.clone())];
        let loaded = Definitions::load_with(&base, &base, &extras).unwrap();
        let (window, mirror_brick) = (
            &loaded.entries["v20/brick/window"],
            &loaded.entries["mirror:brick/mirror"],
        );
        assert_eq!(mirror_brick.mesh.id, window.mesh.id);
        assert_eq!(mirror_brick.collision.id, "mirror:brick/mirror");
        assert_eq!(
            mirror_brick.collision.parts.len(),
            window.collision.parts.len()
        );
        assert_eq!(mirror_brick.reflection.as_ref().unwrap().faces.len(), 2);
        assert!(window.reflection.is_none());
        // A shape nothing loaded is an error naming the brick.
        catalog(
            &addon,
            &[entry("mirror:brick/mirror", "v20/door.blb", Some(mirror))],
            &[("mirror:brick/mirror", json!({}))],
            &[],
        );
        let error = format!(
            "{:#}",
            Definitions::load_with(&base, &base, &extras).err().unwrap()
        );
        assert!(
            error.contains("mirror:brick/mirror") && error.contains("v20/door.blb"),
            "{error}"
        );
        // Another size of the shape: its own mesh, and a portal's frame
        // round the bigger opening.
        let mut portal = entry("portal:brick/big", "v20/window.blb", None);
        portal["stretch"] = json!([8, 1, 6]);
        portal["link"] = json!({ "faces": ["north", "south"], "depth": 0.5, "pass": true,
            "frame": 0.05, "name": "Portal" });
        let mut plain = entry("portal:brick/plain", "v20/window.blb", None);
        plain["stretch"] = json!([8, 1, 6]);
        catalog(&addon, &[portal], &[("portal:brick/big", json!({}))], &[]);
        let loaded = Definitions::load_with(&base, &base, &extras).unwrap();
        let big = &loaded.entries["portal:brick/big"];
        assert_eq!(big.mesh.id, "v20/window.blb#8x1x6");
        assert_eq!(
            (big.mesh.footprint_studs, big.mesh.height_plates),
            ([8, 1], 6)
        );
        assert_eq!(
            loaded.entries["v20/brick/window"].mesh.footprint_studs,
            [4, 1]
        );
        let aabb = big.shape.compute_local_aabb();
        assert!((aabb.maxs.x - 2.0).abs() < 1e-5 && (aabb.maxs.y - 0.6).abs() < 1e-5);
        // Without openings bodies pass, the shape's collision stretches
        // with it: the whole 4 by 1.2 by 0.5 brick.
        catalog(&addon, &[plain], &[("portal:brick/plain", json!({}))], &[]);
        let loaded = Definitions::load_with(&base, &base, &extras).unwrap();
        let plain = &loaded.entries["portal:brick/plain"];
        assert_eq!(plain.collision.id, "portal:brick/plain");
        let aabb = plain.shape.compute_local_aabb();
        assert!(
            glam::Vec3::new(aabb.maxs.x, aabb.maxs.y, aabb.maxs.z)
                .abs_diff_eq(glam::Vec3::new(2.0, 0.6, 0.25), 1e-5),
            "{aabb:?}"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
