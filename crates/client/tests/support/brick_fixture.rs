//! Native brick materials and meshes for brick rendering tests: the
//! generated v20 packs, or a synthetic set (`bri_content::testing::bricks`:
//! images drawn in code, made-up geometry) that exercises the same loader
//! and scene paths.
#![allow(dead_code)]

use super::files::{repo_root, scratch};
use anyhow::Result;
use bri_client::materials::BrickMaterials;
use bri_content::brick::{Brick as Mesh, Face, Surface};
use bri_sim::definitions::Definitions;
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
            &bri_package::testing::pack_dir(&root.join("content"), "brick_catalog"),
            &bri_package::testing::pack_dir(&root.join("content"), "geometry"),
        )?;
        Ok(Self {
            materials: BrickMaterials::load(&bri_package::testing::pack_dir(
                &root.join("content"),
                "brick_materials",
            ))?,
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

    /// `bri_content::testing::bricks`' materials pack (five surface images
    /// and its prints), loaded through the real loader, and
    /// two made-up bricks: a printed 2x2 tile and a block with a literal
    /// coloured west face.
    pub fn synthetic() -> Result<Self> {
        let dir = scratch("brick-materials-")?;
        let root = dir.path();
        write_materials(root)?;
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
            print: bri_content::testing::bricks::PRINTS[0].3.into(),
            literal: literal.into(),
            content: false,
            _scratch: Some(dir),
        })
    }
}

pub use bri_content::testing::bricks::{block, default_surface, write_materials};
