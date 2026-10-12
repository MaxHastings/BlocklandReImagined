//! The skies, grounds and waters the Environment window offers, imported
//! from Blockland's Steam release (its `Sky_*`, `Ground_*` and `Water_*`
//! Add-Ons) by `bri-convert`'s `environment_pack`. A machine without that
//! release builds the pack empty, and the window offers the v20 maps' own
//! skies instead. Names are each Add-On file's path in lower case
//! (`add-ons/sky_blue2/blue2.dml`), the settings' stable IDs.
use crate::{
    atmosphere::{NO_WATER, Settings},
    environment::{Environment, Image},
    scene::Plate,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const SCHEMA: u32 = 1;
/// The pack's index file.
pub const FILE: &str = "environments.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub schema_version: u32,
    /// Where the Add-Ons came from ("Blockland (Steam)"), or None when no
    /// such release was given and the pack is empty.
    pub source: Option<String>,
    pub skies: Vec<Sky>,
    pub grounds: Vec<Ground>,
    pub waters: Vec<Water>,
}

/// A sky box with the look its `.atmosphere` file gives: picking it sets
/// both, as Steam's Environment window does.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sky {
    pub id: String,
    pub label: String,
    pub thumbnail: Image,
    pub environment: Environment,
    /// Sun, light, fog, sky colour, sun flare, vignette and day cycle.
    pub look: Settings,
    /// Rain or snow that comes with it (Slate Storm, Spooky3 Snow).
    pub precipitation: Option<Precipitation>,
}

/// v21's `$Rain::` values: an untextured `Precipitation` the sky brings.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Precipitation {
    pub drop: Image,
    pub splash: Option<Image>,
    pub drop_size: f32,
    pub splash_size: f32,
    pub splash_ms: u32,
    pub true_billboards: bool,
    pub speed: [f32; 2],
    pub mass: [f32; 2],
    pub max_turbulence: f32,
    pub turbulence_speed: f32,
    pub rotate_with_camera: bool,
    pub turbulence: bool,
    pub drops: u32,
    pub box_width: f32,
    pub box_height: f32,
    pub collision: bool,
}

/// A floor laid over a flat map's plate (v21's `groundPlane`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ground {
    pub id: String,
    pub label: String,
    pub thumbnail: Image,
    pub texture: Image,
    /// Texture repeats per world unit.
    pub loops_per_unit: f32,
    /// Straight RGBA, 0 to 1; alpha 0 hides the floor.
    pub color: [f32; 4],
    /// True: the texture times the colour. False: the texture's alpha lays
    /// it over the colour, as a brick's print does.
    pub color_multiply: bool,
    /// Texture repeats per second.
    pub scroll: [f32; 2],
}

/// A sea over a flat map's plate (v21's `WaterPlane` and its `WaterZone`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Water {
    pub id: String,
    pub label: String,
    pub thumbnail: Image,
    pub top: Image,
    pub bottom: Image,
    pub loops_per_unit: f32,
    /// The surface's colour; alpha below 1 lets the floor show through.
    pub color: [f32; 4],
    /// The colour over the screen under water.
    pub overlay_color: [f32; 4],
    pub color_multiply: bool,
    /// Above the plate, units.
    pub height: f32,
    /// Whoever goes in dies (Lava).
    pub kill: bool,
    pub scroll: [f32; 2],
}

/// v21's `WaterZone` reaches this far under the surface.
pub const SEA_DEPTH: f32 = 100.0;
/// How far a sea reaches past the plate's edges, so a swimmer who leaves
/// the plate still swims.
pub const SEA_MARGIN: f32 = 1000.0;

impl Water {
    /// This water over `plate`, with the height and underwater colour
    /// `settings` give: a still `PhysicalZone` of density 1 and viscosity
    /// 40 whose surface is the height above the plate's top.
    pub fn sea(&self, plate: &Plate, settings: &Settings) -> crate::water::Water {
        let surface = plate.top + settings.water_height.unwrap_or(self.height);
        let mut sea = crate::water::Water::volume(
            [
                plate.min[0] - SEA_MARGIN,
                surface - SEA_DEPTH,
                plate.min[1] - SEA_MARGIN,
            ],
            [
                plate.max[0] + SEA_MARGIN,
                surface,
                plate.max[1] + SEA_MARGIN,
            ],
        );
        sea.id = self.id.clone();
        sea.liquid_type = if self.kill { LAVA } else { "Water" }.into();
        sea.surface = self.top.clone();
        sea.shore = self.bottom.clone();
        sea.opacity = settings.water_color.unwrap_or(self.color)[3];
        sea.tint = Some(settings.underwater_color.unwrap_or(self.overlay_color));
        sea
    }
}
/// The liquid type of a water that kills (v21's `waterType` 8): whoever is
/// in it takes [`LAVA_DAMAGE`] every [`LAVA_PERIOD_MS`].
pub const LAVA: &str = "Lava";
pub const LAVA_DAMAGE: f32 = 20.0;
pub const LAVA_PERIOD_MS: u32 = 300;

/// Highest water and loops-per-unit values v21 takes.
pub const MAX_WATER_HEIGHT: f32 = 100.0;
pub const LOOPS_PER_UNIT: std::ops::RangeInclusive<f32> = 0.001..=10.0;

fn image(image: &Image) -> bool {
    crate::brick_materials::safe_relative(&image.file)
        && !image.file.contains('/')
        && image.sha256.len() == 64
        && image.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        && (1..=8192).contains(&image.width)
        && (1..=8192).contains(&image.height)
}
fn unit(values: &[f32]) -> bool {
    values
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}
fn id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= crate::atmosphere::MAX_CHOICE_ID
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"/_-. ".contains(&b))
}
fn scroll(s: [f32; 2]) -> bool {
    s.iter().all(|v| v.is_finite() && v.abs() <= 100.0)
}

impl Pack {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SCHEMA,
            "Unknown environment pack schema"
        );
        let mut ids = std::collections::BTreeSet::new();
        for sky in &self.skies {
            ensure!(
                id(&sky.id) && ids.insert(&sky.id) && !sky.label.is_empty(),
                "Invalid sky {}",
                sky.id
            );
            ensure!(image(&sky.thumbnail), "Invalid sky thumbnail {}", sky.id);
            sky.environment.validate()?;
            ensure!(sky.look.sky.is_none(), "A sky's look names no sky");
            sky.look.validate()?;
            if let Some(p) = &sky.precipitation {
                ensure!(
                    image(&p.drop)
                        && p.splash.as_ref().is_none_or(image)
                        && [
                            p.drop_size,
                            p.splash_size,
                            p.max_turbulence,
                            p.turbulence_speed
                        ]
                        .into_iter()
                        .chain(p.speed)
                        .chain(p.mass)
                        .all(|v| v.is_finite() && (0.0..=10.0).contains(&v))
                        && p.splash_ms <= 1000
                        && (1..=10_000).contains(&p.drops)
                        && [p.box_width, p.box_height]
                            .iter()
                            .all(|v| v.is_finite() && (0.0..=1000.0).contains(v)),
                    "Invalid precipitation of {}",
                    sky.id
                );
            }
        }
        for g in &self.grounds {
            ensure!(
                id(&g.id) && ids.insert(&g.id) && !g.label.is_empty(),
                "Invalid ground {}",
                g.id
            );
            ensure!(
                image(&g.thumbnail)
                    && image(&g.texture)
                    && LOOPS_PER_UNIT.contains(&g.loops_per_unit)
                    && unit(&g.color)
                    && scroll(g.scroll),
                "Invalid ground {}",
                g.id
            );
        }
        for w in &self.waters {
            ensure!(
                id(&w.id) && ids.insert(&w.id) && !w.label.is_empty(),
                "Invalid water {}",
                w.id
            );
            ensure!(
                image(&w.thumbnail)
                    && image(&w.top)
                    && image(&w.bottom)
                    && LOOPS_PER_UNIT.contains(&w.loops_per_unit)
                    && unit(&w.color)
                    && unit(&w.overlay_color)
                    && w.height.is_finite()
                    && (0.0..=MAX_WATER_HEIGHT).contains(&w.height)
                    && scroll(w.scroll),
                "Invalid water {}",
                w.id
            );
        }
        ensure!(
            self.source.is_some()
                || (self.skies.is_empty() && self.grounds.is_empty() && self.waters.is_empty()),
            "An environment pack without a source is empty"
        );
        Ok(())
    }
    pub fn sky(&self, id: &str) -> Option<&Sky> {
        self.skies.iter().find(|s| s.id == id)
    }
    pub fn ground(&self, id: &str) -> Option<&Ground> {
        self.grounds.iter().find(|g| g.id == id)
    }
    pub fn water(&self, id: &str) -> Option<&Water> {
        self.waters.iter().find(|w| w.id == id)
    }
    /// The water a flat map has under `settings`: the map's own (`own`)
    /// until one is picked, none for [`NO_WATER`], or the picked one as a
    /// sea over the whole plate. A map without a plate, or a water this
    /// pack lacks, keeps its own.
    pub fn waters(
        &self,
        plate: Option<&Plate>,
        settings: &Settings,
        own: &[crate::water::Water],
    ) -> Vec<crate::water::Water> {
        let (Some(plate), Some(id)) = (plate, settings.water.as_deref()) else {
            return own.to_vec();
        };
        if id == NO_WATER {
            return Vec::new();
        }
        let Some(water) = self.water(id) else {
            return own.to_vec();
        };
        vec![water.sea(plate, settings)]
    }
    /// The pack in `dir`, or an empty one when it has none (content from
    /// before this pack existed).
    pub fn load(dir: &std::path::Path) -> Result<Self> {
        let path = dir.join(FILE);
        if !path.is_file() {
            return Ok(Self {
                schema_version: SCHEMA,
                ..Default::default()
            });
        }
        let pack: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        pack.validate()?;
        Ok(pack)
    }
}
