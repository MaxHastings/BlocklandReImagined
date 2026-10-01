//! Made-up brick meshes and a brick materials pack: box bricks of any size
//! ([`block`]) and a `brick-materials.json` pack ([`write_materials`]) with
//! five surface images and two prints, drawn in code.
use super::{png, sha256, write_file};
use crate::brick::{Brick as Mesh, Face, Quad, Surface, Vertex};
use anyhow::{Context, Result};
use glam::Vec3;
use serde_json::json;
use std::path::Path;

/// The made-up print package and its archive.
pub const PRINT_PACKAGE: (&str, &str) = ("Print_Fixture", "Add-Ons/Print_Fixture.zip");
/// The prints: (id, name, aspect, alias). The tool catalog gives new
/// printed bricks the universal `Letters/A`.
pub const PRINTS: [(&str, &str, &str, &str); 2] = [
    (
        "print/print_fixture/cross",
        "Cross",
        "Fixture",
        "Fixture/Cross",
    ),
    ("print/print_fixture/letters_a", "A", "Letters", "Letters/A"),
];

/// The surface a plain brick shows on `face`.
pub fn default_surface(face: Face) -> Surface {
    match face {
        Face::Top => Surface::Top,
        Face::Bottom => Surface::BottomLoop,
        _ => Surface::Side,
    }
}

/// A box brick `studs` wide/deep and `plates` tall (half a unit per stud, a
/// fifth per plate), one quad per side; `look` gives each side's surface and
/// authored colours.
pub fn block(
    id: &str,
    studs: [u32; 2],
    plates: u32,
    look: impl Fn(Face) -> (Surface, Option<[[f32; 4]; 4]>),
) -> Mesh {
    let half = Vec3::new(
        studs[0] as f32 * 0.25,
        plates as f32 * 0.1,
        studs[1] as f32 * 0.25,
    );
    // Each side's outward normal and in-plane axes, u x v = normal, so the
    // corners run counterclockwise seen from outside.
    let sides = [
        (Face::Top, Vec3::Y, Vec3::X, Vec3::NEG_Z),
        (Face::Bottom, Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Face::North, Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
        (Face::South, Vec3::Z, Vec3::X, Vec3::Y),
        (Face::East, Vec3::X, Vec3::NEG_Z, Vec3::Y),
        (Face::West, Vec3::NEG_X, Vec3::Z, Vec3::Y),
    ];
    let quads = sides
        .into_iter()
        .map(|(face, n, u, v)| {
            let (surface, colors) = look(face);
            let centre = n * half;
            let (du, dv) = (u * (u.abs() * half).length(), v * (v.abs() * half).length());
            let corners = [
                (centre - du - dv, [0.0, 1.0]),
                (centre + du - dv, [1.0, 1.0]),
                (centre + du + dv, [1.0, 0.0]),
                (centre - du + dv, [0.0, 0.0]),
            ];
            Quad {
                face,
                surface,
                colors,
                vertices: corners.map(|(p, uv)| Vertex {
                    position: p.to_array(),
                    normal: n.to_array(),
                    uv,
                }),
            }
        })
        .collect();
    let cell = if plates == 1 { "b" } else { "x" };
    Mesh {
        schema_version: 1,
        id: id.into(),
        footprint_studs: studs,
        height_plates: plates,
        attachment_rows: (0..studs[1] * plates)
            .map(|_| cell.repeat(studs[0] as usize))
            .collect(),
        collision_boxes: vec![],
        needs_external_collision: false,
        coverage: None,
        quads,
    }
}

/// One square native image entry, its file written under `root`, and
/// where it came from (`source`, `archive`).
fn image(
    root: &Path,
    path: &str,
    size: u32,
    bytes: &[u8],
    source: (&str, Option<&str>),
) -> Result<serde_json::Value> {
    let sha = write_file(root, path, bytes)?;
    Ok(json!({
        "path": path, "width": size, "height": size, "sha256": sha,
        "source": { "path": source.0, "archive": source.1, "sha256": sha },
    }))
}

/// Writes a native `brick-materials.json` pack into `root`: five surface
/// images (each a grey checker whose alpha is pigment coverage) and the
/// [`PRINTS`] (a diagonal cross on clear), their icons sourced from the
/// print package as the print menu reads them. Returns each print icon's
/// UI image name (`<package archive stem>/icons/<name>`, lowercase).
pub fn write_materials(root: &Path) -> Result<Vec<String>> {
    let surfaces = [
        ("top", 0.9),
        ("side", 0.8),
        ("bottom_edge", 0.6),
        ("bottom_loop", 0.5),
        ("ramp", 0.7),
    ];
    let mut images = serde_json::Map::new();
    for (name, grey) in surfaces {
        let bytes = png(8, 8, |x, y| {
            let shade = if (x / 2 + y / 2) % 2 == 0 {
                grey
            } else {
                grey * 0.8
            };
            let v = (shade * 255.0) as u8;
            [v, v, v, if (x + y) % 3 == 0 { 96 } else { 32 }]
        })?;
        let path = format!("surfaces/{name}.png");
        images.insert(name.into(), image(root, &path, 8, &bytes, (&path, None))?);
    }
    let print = png(16, 16, |x, y| {
        if x == y || x + y == 15 {
            [20, 20, 20, 255]
        } else {
            [255, 255, 255, 0]
        }
    })?;
    let icon = png(4, 4, |_, _| [20, 20, 20, 255])?;
    let (package, archive) = PRINT_PACKAGE;
    let stem = archive.strip_suffix(".zip").context("print package")?;
    let mut prints = vec![];
    let mut icons = vec![];
    for (id, name, aspect, alias) in PRINTS {
        let file = name.to_ascii_lowercase();
        let diffuse = format!("prints/{aspect}_{file}.png").to_ascii_lowercase();
        let small = format!("prints/{aspect}_{file}-icon.png").to_ascii_lowercase();
        let source = format!("icons/{aspect}_{name}.png");
        prints.push(json!({
            "id": id, "name": name, "aspect": aspect, "package": package,
            "aliases": [alias],
            "diffuse": image(root, &diffuse, 16, &print, (&diffuse, None))?,
            "icon": image(root, &small, 4, &icon, (&source, Some(archive)))?,
        }));
        icons.push(format!("{stem}/{}", source.trim_end_matches(".png")).to_ascii_lowercase());
    }
    let pack = json!({
        "schema_version": 1,
        "surfaces": images,
        "prints": prints,
        "packages": [{
            "name": package,
            "archive": archive,
            "archive_sha256": sha256(b"synthetic print package"),
            "default_list_line": 1,
        }],
        "evidence": [],
        "excluded_installed_packages": [],
        "warnings": [],
    });
    write_file(
        root,
        "brick-materials.json",
        &serde_json::to_vec_pretty(&pack)?,
    )?;
    Ok(icons)
}
