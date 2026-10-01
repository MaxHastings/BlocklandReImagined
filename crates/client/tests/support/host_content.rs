//! The bricks and weapons a bare `Session` host runs, for tests that build
//! their own server: the generated v20 stock catalog and weapons pack, or
//! `bri_sim::testing`'s bricks (drawn as boxes) and `bri_weapons::testing`'s
//! pack.
use anyhow::Result;
use bri_sim::definitions::Definitions;

pub struct HostContent {
    pub content: bool,
    /// A plain brick to plant and destroy.
    pub brick: String,
    pub hammer: String,
    pub rocket: String,
}

impl HostContent {
    pub fn content() -> Result<Self> {
        Ok(Self {
            content: true,
            brick: "v20/brick/brick2x2data".into(),
            hammer: "v20.weapon.hammeritem".into(),
            rocket: "v20.weapon.rocketlauncheritem".into(),
        })
    }

    pub fn synthetic() -> Result<Self> {
        Ok(Self {
            content: false,
            brick: bri_sim::testing::BRICK.into(),
            hammer: bri_weapons::testing::HAMMER.into(),
            rocket: bri_weapons::testing::ROCKET_ITEM.into(),
        })
    }

    /// The catalog, each made-up brick drawn as a box
    /// (`bri_content::testing::bricks::block`) so it can be meshed.
    pub fn definitions(&self) -> Result<Definitions> {
        if !self.content {
            use bri_content::testing::bricks;
            let mut definitions = bri_sim::testing::definitions();
            for d in definitions.entries.values_mut() {
                let m = &d.mesh;
                d.mesh.quads = bricks::block(&m.id, m.footprint_studs, m.height_plates, |face| {
                    (bricks::default_surface(face), None)
                })
                .quads;
            }
            return Ok(definitions);
        }
        let content = super::files::repo_root().join("content");
        Definitions::load(
            &content.join("stock-catalog-004"),
            &content.join("maps-pass-008"),
        )
    }

    pub fn weapons(&self) -> Result<bri_weapons::Pack> {
        if !self.content {
            return Ok(bri_weapons::testing::pack());
        }
        let pack =
            std::fs::read(super::files::repo_root().join("content/weapons-pack-009/weapons.json"))?;
        bri_weapons::Pack::from_json(&pack)
    }
}
