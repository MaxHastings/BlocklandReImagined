//! Native brick materials and meshes for brick rendering tests: the
//! generated v20 packs, or a synthetic set made here (images drawn in code,
//! made-up geometry) that exercises the same loader and scene paths.
#![allow(dead_code)]

use super::files::{png, repo_root, scratch, sha256, write, write_json};
use anyhow::Result;
use bri_client::materials::BrickMaterials;
use bri_content::brick::{Brick as Mesh, Face, Quad, Surface, Vertex};
use bri_sim::definitions::Definitions;
use glam::Vec3;
use serde_json::json;
use std::collections::BTreeMap;

pub struct BrickFixture {
    pub materials: BrickMaterials,
    /// Render meshes by definition id.
    pub meshes: BTreeMap<String, Mesh>,
    /// A brick with a print surface.
    pub printable: String,
    /// A print name `materials` resolves.
    pub print: String,
    /// A brick whose authored vertex colours include literal RGB above 1
    /// (resolved as provisional literals, not paint offsets), facing -X.
    pub literal: String,
    /// Whether this is the generated v20 content.
    pub content: bool,
    _scratch: Option<tempfile::TempDir>,
}

impl BrickFixture {
    pub fn content() -> Result<Self> {
        let root = repo_root();
        let definitions = Definitions::load(
            &root.join("content/stock-catalog-004"),
            &root.join("content/maps-pass-008"),
        )?;
        Ok(Self {
            materials: BrickMaterials::load(&root.join("content/brick-materials-002"))?,
            meshes: definitions
                .entries
                .into_iter()
                .map(|(id, d)| (id, d.mesh))
                .collect(),
            printable: "v20/brick/brick2x2fprintdata".into(),
            print: "Letters/A".into(),
            literal: "v20/brick/brickpumpkinfacedata".into(),
            content: true,
            _scratch: None,
        })
    }

    /// Five surface images and one print, written as a native
    /// `brick-materials.json` pack and loaded through the real loader, and
    /// two made-up bricks: a printed 2x2 tile and a block with a literal
    /// coloured west face.
    pub fn synthetic() -> Result<Self> {
        let dir = scratch("brick-materials-")?;
        let root = dir.path();
        // Each surface a different grey checker whose alpha is pigment
        // coverage; the print a diagonal cross on clear.
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
            images.insert(
                name.into(),
                image(root, &format!("surfaces/{name}.png"), 8, &bytes)?,
            );
        }
        let print = png(16, 16, |x, y| {
            if x == y || x + y == 15 {
                [20, 20, 20, 255]
            } else {
                [255, 255, 255, 0]
            }
        })?;
        let icon = png(4, 4, |_, _| [20, 20, 20, 255])?;
        write_json(
            &root.join("brick-materials.json"),
            &json!({
                "schema_version": 1,
                "surfaces": images,
                "prints": [{
                    "id": "print/print_fixture/cross",
                    "name": "Cross",
                    "aspect": "Fixture",
                    "package": "Print_Fixture",
                    "aliases": ["Fixture/Cross"],
                    "diffuse": image(root, "prints/cross.png", 16, &print)?,
                    "icon": image(root, "prints/cross-icon.png", 4, &icon)?,
                }],
                "packages": [{
                    "name": "Print_Fixture",
                    "archive": "Add-Ons/Print_Fixture.zip",
                    "archive_sha256": sha256(b"synthetic print package"),
                    "default_list_line": 1,
                }],
                "evidence": [],
                "excluded_installed_packages": [],
                "warnings": [],
            }),
        )?;
        let printable = "fixture/brick/print-tile";
        let literal = "fixture/brick/literal-block";
        let tile = block(printable, [2, 2], 1, |face| match face {
            Face::Top => (Surface::Print, None),
            _ => (default_surface(face), None),
        });
        // Out-of-range literal RGB on the west face only.
        let block = block(literal, [2, 2], 3, |face| match face {
            Face::West => (Surface::Side, Some([[3.5, 2.0, 0.25, 1.0]; 4])),
            _ => (default_surface(face), None),
        });
        Ok(Self {
            materials: BrickMaterials::load(root)?,
            meshes: [tile, block].map(|m| (m.id.clone(), m)).into(),
            printable: printable.into(),
            print: "Fixture/Cross".into(),
            literal: literal.into(),
            content: false,
            _scratch: Some(dir),
        })
    }
}

/// One square native image entry, its file written under `root`.
fn image(root: &std::path::Path, path: &str, size: u32, bytes: &[u8]) -> Result<serde_json::Value> {
    let sha = write(&root.join(path), bytes)?;
    Ok(json!({
        "path": path, "width": size, "height": size, "sha256": sha,
        "source": { "path": path, "archive": null, "sha256": sha },
    }))
}

fn default_surface(face: Face) -> Surface {
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
