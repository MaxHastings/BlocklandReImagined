//! Offline water resource/path adaptation. No legacy paths reach the runtime.
use anyhow::{Context, Result, ensure};
use bri_content::{
    environment::{Environment, Image},
    scene::{Kind, Scene},
    water::Water,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
fn number(p: &BTreeMap<String, String>, key: &str, default: f32) -> Result<f32> {
    let v = p
        .get(key)
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(default);
    ensure!(v.is_finite(), "Invalid water field {key}");
    Ok(v)
}
fn flag(p: &BTreeMap<String, String>, key: &str, default: bool) -> Result<bool> {
    match p.get(key).map(String::as_str) {
        None => Ok(default),
        Some("1" | "true") => Ok(true),
        Some("0" | "false") => Ok(false),
        _ => anyhow::bail!("Invalid water flag {key}"),
    }
}
pub fn resource(root: &Path, source: &str, reference: &str, output: &Path) -> Result<Image> {
    let reference = reference.replace('\\', "/");
    // Authored v20 missions retain both pre-v9 map paths and core ~/data paths.
    let path = if let Some(rest) = reference.strip_prefix("./") {
        // Beside the mission, as a map Add-On's own water texture is.
        let folder = source.rsplit_once('/').map_or("", |(folder, _)| folder);
        format!("{folder}/{rest}")
    } else if let Some(rest) = reference.strip_prefix("~/data/") {
        format!("base/data/{rest}")
    } else if let Some(rest) = reference.strip_prefix("~/Map_") {
        format!("Add-Ons/Map_{rest}")
    } else {
        reference.to_string()
    };
    let candidates = if Path::new(&path).extension().is_some() {
        vec![path]
    } else {
        vec![
            format!("{path}.png"),
            format!("{path}.jpg"),
            format!("{path}.jpeg"),
        ]
    };
    for source in candidates {
        if let Some(bytes) = crate::environment::read_original(root, &source)? {
            let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
                .with_guessed_format()?
                .into_dimensions()?;
            ensure!(
                width > 0 && height > 0 && width <= 8192 && height <= 8192,
                "Oversized water image"
            );
            let sha256 = format!("{:x}", Sha256::digest(&bytes));
            let file = format!("{sha256}.{}", source.rsplit('.').next().unwrap());
            std::fs::write(output.join(&file), bytes)?;
            return Ok(Image {
                file,
                source,
                sha256,
                width,
                height,
            });
        }
    }
    anyhow::bail!("Missing original water image {reference}")
}
/// Where a water block that does not repeat lies along one axis. The
/// classic engine keeps water in the terrain block's own space, wrapping its
/// position into the block, so a block authored at `352` on a terrain from
/// `-1024` to `1024` covers the terrain from its far edge round to its near
/// one. A block as wide as the terrain covers all of it; a narrower one
/// moves whole periods to sit inside it, and the second flag says whether
/// part of it is still past the edge (the classic engine drew that part on
/// the near side).
pub fn terrain_wrapped(min: f32, max: f32, start: f32, period: f32) -> (f32, f32, bool) {
    let width = max - min;
    if width >= period - 1e-3 {
        return (start, start + period, false);
    }
    let centre = min + width * 0.5;
    let shift = start + (centre - start).rem_euclid(period) - centre;
    let (min, max) = (min + shift, max + shift);
    (min, max, min < start - 1e-3 || max > start + period + 1e-3)
}

/// The terrain block's native start along x and z and its width, as
/// `terrain_wrapped` takes them.
pub fn terrain_block(scene: &Scene) -> Option<([f32; 2], f32)> {
    let node = scene
        .nodes
        .iter()
        .find(|n| matches!(n.kind, Kind::Terrain))?;
    let spacing = node
        .properties
        .get("squaresize")
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(8.0);
    let period = spacing * 256.0;
    let origin = glam::Mat4::from_cols_array(&node.transform).w_axis;
    // Native z runs against Torque y: the block spans z from origin - period.
    Some(([origin.x, origin.z - period], period))
}

pub fn convert(
    root: &Path,
    source: &str,
    scene: &Scene,
    environment: Option<&Environment>,
    output: &Path,
) -> Result<Vec<Water>> {
    let mut waters = vec![];
    for (node, n) in scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.kind, Kind::Water))
    {
        let p = &n.properties;
        let t = glam::Mat4::from_cols_array(&n.transform);
        let (scale, _, origin) = t.to_scale_rotation_translation();
        ensure!(
            t.is_finite() && scale.min_element() > 0.,
            "Invalid water placement"
        );
        let surface = resource(
            root,
            source,
            p.get("surfacetexture")
                .context("Water surface texture absent")?,
            output,
        )?;
        let shore = resource(
            root,
            source,
            p.get("shoretexture")
                .context("Water shore texture absent")?,
            output,
        )?;
        let reflection_intensity = number(p, "envmapintensity", 0.4)?;
        let mut missing_reflection = None;
        let reflection = if reflection_intensity <= 0.0 {
            None
        } else if let Some(path) = p
            .get("envmapovertexture")
            .or_else(|| p.get("envmaptexture"))
        {
            // Community maps name reflection maps from other games
            // (`common/data/skies/...`): fall back to the sky's reflection map.
            match resource(root, source, path, output) {
                Ok(image) => Some(image),
                Err(error) => {
                    // Never silently: only onto the sky's own reflection map.
                    let sky = environment.and_then(|e| e.reflection.clone());
                    if sky.is_none() {
                        return Err(error);
                    }
                    missing_reflection = Some(path.clone());
                    sky
                }
            }
        } else {
            environment.and_then(|e| e.reflection.clone())
        };
        let angle = number(p, "flowangle", 0.)?.to_radians();
        let rate = number(p, "flowrate", 0.)?;
        let repeat_period = flag(p, "repeatterrain", true)?.then_some(2048.);
        let mut warnings=vec!["Water shader adaptation still needs original multi-pass reflection/specular/shore fidelity and underwater composition".into(),"Water coverage is a native bounded volume; original terrain accept masks/edge snapping and container overlap need fidelity review".into()];
        if repeat_period.is_some() {
            warnings.push("Authored repeatTerrain uses the classic 2048-unit period; camera-following rendering/LOD remains required".into());
        }
        if let Some(path) = missing_reflection {
            warnings.push(format!(
                "Reflection map {path} is not in the installation; the sky's is used"
            ));
        }
        if p.contains_key("watercolor") {
            warnings.push("Authored waterColor retained in source scene; classic water texture rendering does not infer a new blue tint".into());
        }
        let mut min = [origin.x, origin.y, origin.z - scale.z];
        let mut max = [origin.x + scale.x, origin.y + scale.y, origin.z];
        if let (None, Some((start, period))) = (repeat_period, terrain_block(scene)) {
            let mut past = false;
            for (axis, start) in [(0, start[0]), (2, start[1])] {
                let wrapped = terrain_wrapped(min[axis], max[axis], start, period);
                (min[axis], max[axis]) = (wrapped.0, wrapped.1);
                past |= wrapped.2;
            }
            if past {
                warnings.push("Part of this water lies past the terrain's edge, where the classic engine wrapped it to the near side; that part is left out".into());
            }
        }
        let w = Water {
            schema_version: 1,
            node,
            id: format!("{}/water/{node}", scene.id),
            min,
            max,
            repeat_period,
            liquid_type: p
                .get("liquidtype")
                .cloned()
                .unwrap_or_else(|| "OceanWater".into()),
            density: number(p, "density", 1.)?,
            viscosity: number(p, "viscosity", 15.)?,
            surface,
            shore,
            reflection,
            opacity: number(p, "surfaceopacity", 0.75)?,
            wave_amplitude: number(p, "wavemagnitude", 1.)?,
            // Portable sines keep imports identical across machines.
            flow: [rate * libm::cosf(angle), rate * libm::sinf(angle)],
            distortion: [
                number(p, "distortgridscale", 0.1)?,
                number(p, "distortmag", 0.05)?,
                number(p, "distorttime", 0.5)?,
            ],
            tiles: [number(p, "tesssurface", 50.)?, number(p, "tessshore", 60.)?],
            depth_mask: flag(p, "usedepthmask", true)?,
            depth_alpha: [
                number(p, "minalpha", 0.03)?,
                number(p, "maxalpha", 1.)?,
                number(p, "shoredepth", 20.)?,
                number(p, "depthgradient", 1.)?,
            ],
            reflection_intensity,
            parallax: number(p, "surfaceparallax", 0.5)?,
            warnings,
            tint: None,
            current: [0.0; 3],
        };
        w.validate()?;
        waters.push(w);
    }
    Ok(waters)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn water_wraps_into_the_terrain_block() {
        // Map_Creek: a terrain-wide block authored at 352 covers the terrain.
        assert_eq!(
            terrain_wrapped(352.0, 2400.0, -1024.0, 2048.0),
            (-1024.0, 1024.0, false)
        );
        // A pond past the far edge comes back onto the terrain.
        assert_eq!(
            terrain_wrapped(1500.0, 1600.0, -1024.0, 2048.0),
            (-548.0, -448.0, false)
        );
        // Inside already: left alone. Across the edge: said so.
        assert_eq!(
            terrain_wrapped(-100.0, 100.0, -1024.0, 2048.0),
            (-100.0, 100.0, false)
        );
        assert!(terrain_wrapped(1000.0, 1100.0, -1024.0, 2048.0).2);
    }
    #[test]
    fn legacy_core_paths_and_disabled_missing_reflection_are_explicit() -> Result<()> {
        let parent = std::env::temp_dir().canonicalize()?;
        let root = parent.join(format!(
            "bri-water-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("base/data/water"))?;
        std::fs::create_dir(root.join("out"))?;
        let root = root.canonicalize()?;
        let result = (|| -> Result<()> {
            image::RgbaImage::from_pixel(1, 1, image::Rgba([20, 100, 180, 255]))
                .save(root.join("base/data/water/source.png"))?;
            let mut scene = Scene {
                schema_version: 1,
                id: "fixture".into(),
                name: "fixture".into(),
                pending_scripts: vec![],
                nodes: vec![bri_content::scene::Node {
                    name: "water".into(),
                    parent: None,
                    kind: Kind::Water,
                    transform: glam::Mat4::from_scale_rotation_translation(
                        glam::Vec3::new(32., 100., 64.),
                        glam::Quat::IDENTITY,
                        glam::Vec3::new(2., -91., 3.),
                    )
                    .to_cols_array(),
                    asset: None,
                    properties: [
                        ("surfacetexture".into(), "~/data/water/source".into()),
                        ("shoretexture".into(), "base/data/water/source.png".into()),
                        ("envmaptexture".into(), "base/data/skies/missing".into()),
                        ("envmapintensity".into(), "0".into()),
                    ]
                    .into(),
                }],
            };
            let water = convert(
                &root,
                "base/data/missions/fixture.mis",
                &scene,
                None,
                &root.join("out"),
            )?
            .remove(0);
            assert_eq!(water.min, [2., -91., -61.]);
            assert_eq!(water.max, [34., 9., 3.]);
            assert_eq!(water.repeat_period, Some(2048.));
            assert!(water.reflection.is_none());
            assert_eq!(
                std::fs::read(root.join("base/data/water/source.png"))?,
                std::fs::read(root.join("out").join(&water.surface.file))?
            );
            scene.nodes[0]
                .properties
                .insert("envmapintensity".into(), "0.4".into());
            assert!(
                convert(
                    &root,
                    "base/data/missions/fixture.mis",
                    &scene,
                    None,
                    &root.join("out")
                )
                .is_err(),
                "Required reflection must not silently fall back"
            );
            Ok(())
        })();
        ensure!(
            root.starts_with(&parent)
                && root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("bri-water-"),
            "Fixture cleanup escaped temp directory"
        );
        std::fs::remove_dir_all(root)?;
        result
    }
}
