//! `--kind brick`: a glTF model as a brick Add-On (docs/modding/bricks.md).
//!
//! The model becomes one native brick (`*.brick.json`, the format converted
//! BLBs use): its triangles drawn in the player's paint colour, its grid
//! size the model's bounds rounded up to whole studs and plates, and its
//! collision a box per `col_` mesh, or the whole grid box when there are
//! none. The Add-On's catalog lists it like any imported brick, so it is
//! placed from the brick menu, saved, sent to players and wired with
//! events as one.
use super::{Converted, Options, Report, title};
use anyhow::{Context, Result, ensure};
use bri_content::{
    animation,
    brick::{
        BRICK_SCHEMA, Brick, Catalog, CatalogEntry, CollisionBox, Face, PLATE, Quad, STUD, Surface,
        Vertex,
    },
    collision::{CollisionBody, CollisionLibrary, Part},
};
use glam::Vec3;
use serde_json::json;
use std::collections::BTreeMap;

/// Where Add-On bricks go in the brick menu unless `--category` says.
const CATEGORY: &str = "Special";
const SUBCATEGORY: &str = "Models";

pub(super) fn write(opts: &Options, id: &str, converted: Converted) -> Result<Report> {
    let Converted {
        shape, mut report, ..
    } = converted;
    let ns = crate::namespace_for(id)?;
    let name = opts.name.clone().unwrap_or_else(|| title(id));
    let pose = animation::sample(&shape, None, 0.0)?;
    let mut drawn = vec![];
    let mut collision: BTreeMap<usize, (Vec3, Vec3)> = BTreeMap::new();
    for (d, detail) in shape.details.iter().enumerate() {
        let triangles = animation::triangles(&shape, &pose, d, |_| true)?;
        if detail.collision {
            for t in &triangles {
                let bounds = collision
                    .entry(t.object)
                    .or_insert((Vec3::INFINITY, Vec3::NEG_INFINITY));
                for v in &t.vertices {
                    *bounds = (bounds.0.min(v.position), bounds.1.max(v.position));
                }
            }
        } else if drawn.is_empty() {
            // The finest visible detail; distant ones are the same model.
            drawn = triangles;
        }
    }
    ensure!(!drawn.is_empty(), "the model has no visible triangles");
    let (lo, hi) = drawn
        .iter()
        .flat_map(|t| t.vertices.iter().map(|v| v.position))
        .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p), hi.max(p))
        });
    // The grid box the model fills, in studs and plates, centred on it.
    let size = hi - lo;
    let cells = |extent: f32, unit: f32| ((extent / unit) - 1e-3).ceil().max(1.0) as u32;
    let [width, height, depth] = [
        cells(size.x, STUD),
        cells(size.y, PLATE),
        cells(size.z, STUD),
    ];
    ensure!(
        u64::from(width) * u64::from(depth) * u64::from(height) <= 2_000_000,
        "the model is {:.1} x {:.1} x {:.1} units, too big for one brick; check its scale (1 Blender metre is 1 unit, a stud is 0.5)",
        size.x,
        size.y,
        size.z
    );
    let offset = -(lo + hi) * 0.5;
    let quads: Vec<Quad> = drawn
        .iter()
        .map(|t| {
            let [a, b, c] = t.vertices.each_ref().map(|v| Vertex {
                position: (v.position + offset).to_array(),
                normal: v.normal.try_normalize().unwrap_or(Vec3::Y).to_array(),
                uv: v.uv,
            });
            Quad {
                face: Face::Omni,
                // A print surface with no print: the plain painted face.
                surface: Surface::Print,
                vertices: [a, b, c, c],
                colors: None,
            }
        })
        .collect();
    let half = Vec3::new(
        width as f32 * STUD,
        height as f32 * PLATE,
        depth as f32 * STUD,
    ) * 0.5;
    let boxes: Vec<CollisionBox> = if collision.is_empty() {
        vec![CollisionBox {
            center: [0.0; 3],
            size: (half * 2.0).to_array(),
        }]
    } else {
        collision
            .values()
            .map(|(lo, hi)| CollisionBox {
                center: ((*lo + *hi) * 0.5 + offset).to_array(),
                size: (*hi - *lo).max(Vec3::splat(0.01)).to_array(),
            })
            .collect()
    };
    let mesh_id = format!("{ns}:brick_geometry/{id}");
    let brick_id = format!("{ns}:brick/{id}");
    let mesh = Brick {
        schema_version: BRICK_SCHEMA,
        id: mesh_id.clone(),
        footprint_studs: [width, depth],
        height_plates: height,
        attachment_rows: attachment_rows(width, depth, height),
        collision_boxes: boxes.clone(),
        needs_external_collision: false,
        coverage: None,
        quads,
    };
    mesh.validate()?;
    let body = CollisionBody {
        id: brick_id.clone(),
        parts: boxes
            .iter()
            .map(|b| Part::Box {
                center: b.center,
                size: b.size,
            })
            .collect(),
    };
    body.validate()?;
    let entry = CatalogEntry {
        id: brick_id.clone(),
        display_name: name.clone(),
        category: CATEGORY.into(),
        subcategory: SUBCATEGORY.into(),
        mesh_id,
        collision_source: None,
        icon_source: String::new(),
        print_aspect_ratio: None,
        orientation_fix: 0,
        can_cover: false,
        indestructible: false,
        special_kind: None,
        other_properties: BTreeMap::new(),
        reflection: None,
        link: None,
        stretch: None,
        swap: None,
        bot: None,
    };
    let out = &opts.out;
    let mut put = |rel: &str, bytes: Vec<u8>| -> Result<()> {
        let path = out.join(rel);
        std::fs::create_dir_all(path.parent().context("output path")?)?;
        std::fs::write(path, bytes)?;
        report.files.push(rel.into());
        Ok(())
    };
    let dir = "assets/brick-catalog";
    let mesh_file = format!("{id}.brick.json");
    put(&format!("{dir}/{mesh_file}"), serde_json::to_vec(&mesh)?)?;
    put(
        &format!("{dir}/stock-catalog.json"),
        serde_json::to_vec_pretty(&Catalog {
            schema_version: 1,
            bricks: vec![entry],
        })?,
    )?;
    put(
        &format!("{dir}/catalog-audit.json"),
        serde_json::to_vec_pretty(
            &json!({ "resolved_meshes": [{ "id": brick_id, "native_mesh": mesh_file }] }),
        )?,
    )?;
    put(
        &format!("{dir}/native-collisions.json"),
        serde_json::to_vec_pretty(&CollisionLibrary {
            schema_version: 1,
            bodies: vec![body],
        })?,
    )?;
    let manifest = json!({
        "schema_version": 1,
        "id": ns,
        "version": opts.version,
        "api": 1,
        "name": name,
        "description": format!("{name}, a brick imported from {} by bri-import-addon gltf.", report.source),
        "authors": [],
        "license": "proprietary",
        "provenance": { "source": format!("glTF model {}", report.source), "notes": "Imported by bri-import-addon gltf. Set the licence of the original model." },
        "dependencies": {},
        "capabilities": [],
        "provides": [
            { "kind": "bricks", "id": format!("{ns}:bricks/main"), "file": format!("{dir}/stock-catalog.json") },
        ],
    });
    put("package.json", serde_json::to_vec_pretty(&manifest)?)?;
    report.id = ns;
    report.model = brick_id;
    if !shape.materials.is_empty() {
        report.warnings.push(
            "a brick draws in the player's paint colour: the model's textures and material colours are not used".into(),
        );
    }
    report.width = width as f32 * STUD;
    report.height = height as f32 * PLATE;
    if collision.is_empty() {
        report.warnings.push(format!(
            "no `col_` mesh: the brick collides as its whole {width} x {depth} stud, {height} plate box; add `col_` meshes for a closer fit"
        ));
    }
    Ok(report)
}

/// Studs on top, sockets underneath, solid between: a brick other bricks
/// plant on and under, as a converted v20 `BRICK` gets.
fn attachment_rows(width: u32, depth: u32, height: u32) -> Vec<String> {
    (0..depth)
        .flat_map(|_| {
            (0..height).map(move |y| {
                let cell = match (height, y) {
                    (1, _) => "b",
                    (_, 0) => "u",
                    (h, y) if y == h - 1 => "d",
                    _ => "x",
                };
                cell.repeat(width as usize)
            })
        })
        .collect()
}
