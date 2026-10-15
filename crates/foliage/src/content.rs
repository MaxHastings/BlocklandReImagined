use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Texture {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub source: String,
    pub sha256: String,
    pub line: usize,
    pub fields: BTreeMap<String, String>,
    pub adaptations: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Definition {
    pub id: String,
    pub scene: String,
    pub node: usize,
    pub texture: usize,
    pub origin: [f32; 3],
    pub seed: u32,
    pub count: u32,
    pub retries: u32,
    pub inner: [f32; 2],
    pub outer: [f32; 2],
    pub square: bool,
    pub offset: f32,
    pub allowed_slope: f32,
    pub allow_terrain: bool,
    pub allow_interior: bool,
    pub allow_static: bool,
    pub allow_water: bool,
    pub water_surface: bool,
    pub width: [f32; 2],
    pub height: [f32; 2],
    pub fixed_size: bool,
    pub fixed_aspect: bool,
    pub flip: bool,
    pub billboard: bool,
    pub random_rotation: bool,
    pub rotation: f32,
    pub sway: bool,
    pub sway_sync: bool,
    pub sway_magnitude: [f32; 2],
    pub sway_seconds: [f32; 2],
    pub light: bool,
    pub light_sync: bool,
    pub light_seconds: f32,
    pub luminance: [f32; 2],
    pub color_top: [f32; 4],
    pub color_bottom: [f32; 4],
    pub ground_alpha: f32,
    pub alpha_cutoff: f32,
    pub closest: f32,
    pub distance: f32,
    pub fade_near: f32,
    pub fade_far: f32,
    pub cull_size: f32,
    pub culling: bool,
    pub hidden: bool,
    pub evidence: Evidence,
}
impl Definition {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.count <= 100000
                && self.retries > 0
                && self.retries <= 1000
                && self.seed > 0
                && self.seed < 2147483647,
            "invalid foliage allocation/seed"
        );
        ensure!(
            self.origin.iter().all(|v| v.is_finite() && v.abs() < 1e7)
                && self.offset.is_finite()
                && self.rotation.is_finite(),
            "invalid foliage transform"
        );
        for pair in [
            self.inner,
            self.outer,
            self.width,
            self.height,
            self.sway_seconds,
            self.luminance,
        ] {
            ensure!(
                pair.iter().all(|v| v.is_finite() && *v >= 0.),
                "invalid foliage range"
            );
        }
        ensure!(
            self.width[0] > 0.
                && self.width[1] >= self.width[0]
                && self.height[0] > 0.
                && self.height[1] >= self.height[0]
                && self.sway_seconds[0] > 0.
                && self.sway_seconds[1] >= self.sway_seconds[0]
                && self.luminance[1] >= self.luminance[0],
            "inverted foliage range"
        );
        ensure!(
            (0..2).all(|i| self.outer[i] > 0.
                && self.outer[i] <= 10000.
                && self.inner[i] <= self.outer[i]),
            "invalid placement ellipse"
        );
        ensure!(
            self.allowed_slope.is_finite()
                && (0. ..=90.).contains(&self.allowed_slope)
                && self.cull_size.is_finite()
                && (8. ..=1024.).contains(&self.cull_size),
            "invalid slope/culling"
        );
        ensure!(
            [
                self.closest,
                self.distance,
                self.fade_near,
                self.fade_far,
                self.light_seconds
            ]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.)
                && self.distance >= self.closest
                && self.distance + self.fade_far <= 10000.
                && self.light_seconds > 0.,
            "invalid foliage timing/view"
        );
        ensure!(
            self.sway_magnitude
                .iter()
                .all(|v| v.is_finite() && *v >= 0. && *v <= 100.)
                && self
                    .color_top
                    .iter()
                    .chain(&self.color_bottom)
                    .chain([&self.ground_alpha, &self.alpha_cutoff])
                    .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
            "invalid foliage appearance"
        );
        ensure!(
            self.width[1] <= 1024.
                && self.height[1] <= 1024.
                && self.sway_seconds[1] <= 86400.
                && self.light_seconds <= 86400.
                && self.luminance[1] <= 16.,
            "foliage size/timing exceeds native bounds"
        );
        Ok(())
    }
}
/// A grass (`fxGrassReplicator`) or foliage (`fxFoliageReplicator`) node's
/// image, as its scene node names it: the authored reference with `.png`
/// added when it has no extension, and whether it is grass.
pub fn replicator_image(properties: &BTreeMap<String, String>) -> Result<(String, bool)> {
    let grass = match properties.get("source_class").map(String::as_str) {
        Some("fxgrassreplicator") => true,
        Some("fxfoliagereplicator") => false,
        _ => anyhow::bail!("unsupported foliage class"),
    };
    let prefix = if grass { "grass" } else { "foliage" };
    let resource = properties
        .get(&format!("{prefix}file"))
        .context("image resource")?;
    let image = if Path::new(resource).extension().is_none() {
        format!("{resource}.png")
    } else {
        resource.clone()
    };
    Ok((image, grass))
}

impl Definition {
    /// A replicator scene node's plants: `properties` are the node's
    /// (lowercase keys), `origin` its native position, `texture` the pack
    /// index of its image.
    pub fn from_replicator(
        scene: &str,
        node: usize,
        origin: [f32; 3],
        properties: &BTreeMap<String, String>,
        texture: usize,
        mut evidence: Evidence,
    ) -> Result<Self> {
        let f = properties;
        let (_, grass) = replicator_image(f)?;
        ensure!(
            f.get("scale").is_none_or(|s| s == "1 1 1"),
            "scaled foliage placement requires a policy"
        );
        ensure!(
            f.get("surfacetype").is_none_or(|v| v == "Any"),
            "unsupported authored grass surface filter"
        );
        let raw = |key: &str| f.get(key).map(String::as_str);
        let number = |key: &str, default: f32| -> Result<f32> {
            Ok(raw(key).map(str::parse).transpose()?.unwrap_or(default))
        };
        let flag = |key: &str, default: bool| -> Result<bool> {
            match raw(key) {
                None => Ok(default),
                Some("1" | "true") => Ok(true),
                Some("0" | "false") => Ok(false),
                _ => anyhow::bail!("invalid flag {key}"),
            }
        };
        let integer = |key: &str| -> Result<u32> {
            raw(key)
                .context(format!("missing {key}"))?
                .parse()
                .map_err(Into::into)
        };
        let color = |key: &str| -> Result<[f32; 4]> {
            let v: Vec<f32> = raw(key)
                .unwrap_or("1 1 1 1")
                .split_whitespace()
                .map(str::parse)
                .collect::<std::result::Result<_, _>>()?;
            v.try_into().map_err(|_| anyhow::anyhow!("invalid color"))
        };
        // Torque draws between the two values in either order (Paradise
        // gives its sway 10 to 5 seconds).
        let range =
            |low: &str, low_default: f32, high: &str, high_default: f32| -> Result<[f32; 2]> {
                let (a, b) = (number(low, low_default)?, number(high, high_default)?);
                Ok([a.min(b), a.max(b)])
            };
        let prefix = if grass { "grass" } else { "foliage" };
        evidence.adaptations.extend([
            "Pinned OpenMBG 9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7 engine-family foliage/LCG evidence; closed Blockland engine parity unverified".to_string(),
            "Native bounded resumable placement preserves retry ordering; absolute sway/light time advances culled instances instead of pausing their phase".into(),
            "Conservative native grid bounds replace source quadtree; typed placement ignores editor-only placementAreaHeight/debug flags".into(),
        ]);
        if grass {
            evidence.adaptations.push("fxGrassReplicator implementation unavailable: seeded random fixed vertical plane rotation, square placement and vertex color are native interpretations of authored fields; base foliage algorithm reused".into());
        }
        let use_color = grass && flag("usecolour", false)?;
        let definition = Self {
            id: format!("{scene}/foliage/{node}"),
            scene: scene.into(),
            node,
            texture,
            origin,
            seed: integer("seed")?,
            count: integer(&format!("{prefix}count"))?,
            retries: integer(&format!("{prefix}retries"))?,
            inner: [number("innerradiusx", 0.)?, number("innerradiusy", 0.)?],
            outer: [number("outerradiusx", 100.)?, number("outerradiusy", 100.)?],
            square: flag("issquarearea", false)?,
            offset: number("offsetz", 0.)?,
            allowed_slope: number("allowedterrainslope", 90.)?,
            allow_terrain: flag("allowonterrain", true)?,
            allow_interior: flag("allowoninteriors", false)?,
            allow_static: flag("allowonstatics", false)?,
            allow_water: flag("allowonwater", false)?,
            water_surface: flag("allowwatersurface", false)?,
            width: range("minwidth", 1., "maxwidth", 1.)?,
            height: range("minheight", 1., "maxheight", 1.)?,
            fixed_size: flag("fixsizetomax", false)?,
            fixed_aspect: flag("fixaspectratio", false)?,
            flip: flag("randomflip", false)?,
            billboard: !grass,
            random_rotation: grass && flag("israndomrot", false)?,
            rotation: number("rotationangle", 0.)?.to_radians(),
            sway: flag("swayon", false)?,
            sway_sync: flag("swaysync", false)?,
            sway_magnitude: [number("swaymagside", 0.)?, number("swaymagfront", 0.)?],
            sway_seconds: range("minswaytime", 1., "maxswaytime", 1.)?,
            light: flag("lighton", false)?,
            light_sync: flag("lightsync", false)?,
            light_seconds: number("lighttime", 1.)?,
            luminance: range("minluminance", 1., "maxluminance", 1.)?,
            color_top: if use_color {
                color("foilagecolourtop")?
            } else {
                [1.; 4]
            },
            color_bottom: if use_color {
                color("foilagecolourbtm")?
            } else {
                [1.; 4]
            },
            ground_alpha: number("groundalpha", 1.)?,
            alpha_cutoff: number("alphacutoff", 0.5)?,
            closest: number("viewclosest", 1.)?,
            distance: number("viewdistance", 70.)?,
            fade_near: number("fadeoutregion", 1.)?,
            fade_far: number("fadeinregion", 20.)?,
            cull_size: number("cullresolution", 32.)?,
            culling: flag("useculling", true)?,
            hidden: flag(&format!("hide{prefix}"), false)?,
            evidence,
        };
        definition.validate()?;
        Ok(definition)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FoliagePack {
    pub schema_version: u32,
    pub definitions: Vec<Definition>,
    pub textures: Vec<Texture>,
    pub source_bundle: String,
}
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
impl FoliagePack {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1 && self.definitions.len() <= 128 && self.textures.len() <= 128,
            "invalid foliage schema/count"
        );
        let mut ids = std::collections::BTreeSet::new();
        for d in &self.definitions {
            d.validate()?;
            ensure!(
                ids.insert(&d.id) && d.texture < self.textures.len(),
                "duplicate definition/missing texture"
            );
        }
        Ok(())
    }
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        ensure!(
            std::fs::metadata(path.as_ref())?.len() < 16 * 1024 * 1024,
            "oversized pack file"
        );
        let bytes = std::fs::read(path)?;
        ensure!(bytes.len() < 16 * 1024 * 1024, "oversized pack");
        let p: Self = serde_json::from_slice(&bytes)?;
        p.validate()?;
        Ok(p)
    }
    pub fn images(&self, root: impl AsRef<Path>) -> Result<Vec<Image>> {
        self.validate()?;
        let mut decoded_bytes = 0u64;
        self.textures
            .iter()
            .map(|t| {
                let path = Path::new(&t.path);
                ensure!(
                    !path.is_absolute()
                        && path
                            .components()
                            .all(|c| matches!(c, std::path::Component::Normal(_))),
                    "unsafe native texture path"
                );
                ensure!(
                    std::fs::metadata(root.as_ref().join(path))?.len() <= 16 * 1024 * 1024,
                    "oversized image file"
                );
                let bytes =
                    std::fs::read(root.as_ref().join(path)).context("foliage image missing")?;
                ensure!(
                    format!("{:x}", Sha256::digest(&bytes)) == t.sha256,
                    "foliage image hash mismatch"
                );
                let dimensions = image::ImageReader::new(std::io::Cursor::new(&bytes))
                    .with_guessed_format()?
                    .into_dimensions()?;
                ensure!(
                    dimensions.0 > 0
                        && dimensions.0 <= 4096
                        && dimensions.1 > 0
                        && dimensions.1 <= 4096,
                    "oversized foliage image"
                );
                decoded_bytes += u64::from(dimensions.0) * u64::from(dimensions.1) * 4;
                ensure!(
                    decoded_bytes <= 128 << 20,
                    "decoded foliage images exceed128MiB"
                );
                let image = image::load_from_memory(&bytes)?.to_rgba8();
                ensure!(
                    image.width() > 0
                        && image.width() <= 4096
                        && image.height() > 0
                        && image.height() <= 4096,
                    "invalid foliage image dimensions"
                );
                Ok(Image {
                    width: image.width(),
                    height: image.height(),
                    rgba: image.into_raw(),
                })
            })
            .collect()
    }
}
