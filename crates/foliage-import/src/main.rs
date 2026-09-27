use anyhow::{Context, Result, ensure};
use bri_foliage::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 4,
        "usage: bri-foliage-import SOURCE_ROOT NATIVE_MAP_BUNDLE NEW_OUTPUT"
    );
    let source = Path::new(&args[1]);
    let bundle = Path::new(&args[2]);
    let out = Path::new(&args[3]);
    ensure!(!out.exists(), "output exists");
    let zip_path = source.join("Add-Ons/Map_Bedroom.zip");
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&zip_path)?)?;
    let mut mission = String::new();
    zip.by_name("bedroom.mis")?.read_to_string(&mut mission)?;
    let mut pack = FoliagePack {
        schema_version: 1,
        definitions: vec![],
        textures: vec![],
        source_bundle: args[2].clone(),
    };
    let mut images = vec![];
    let mut scenes: Vec<_> = std::fs::read_dir(bundle)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".scene.json"))
        .collect();
    scenes.sort();
    for scene_path in scenes {
        let scene: Value = serde_json::from_slice(&std::fs::read(scene_path)?)?;
        let scene_id = scene["id"].as_str().context("scene id")?;
        for (index, node) in scene["nodes"]
            .as_array()
            .context("nodes")?
            .iter()
            .enumerate()
        {
            if node["kind"] != "foliage" {
                continue;
            }
            ensure!(
                scene_id.ends_with("map_bedroom/bedroom.mis"),
                "new foliage map requires source provenance"
            );
            let f: BTreeMap<String, String> = serde_json::from_value(node["properties"].clone())?;
            ensure!(
                matches!(
                    f["source_class"].as_str(),
                    "fxgrassreplicator" | "fxfoliagereplicator"
                ),
                "unsupported foliage class"
            );
            ensure!(
                f.get("scale").is_none_or(|s| s == "1 1 1"),
                "scaled foliage placement requires a policy"
            );
            let grass = f["source_class"] == "fxgrassreplicator";
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
            let prefix = if grass { "grass" } else { "foliage" };
            let resource = raw(&format!("{prefix}file")).context("image resource")?;
            let rel = if Path::new(resource).extension().is_none() {
                format!("{resource}.png")
            } else {
                resource.to_string()
            };
            let relative = Path::new(&rel);
            ensure!(
                !relative.is_absolute()
                    && relative
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_))),
                "unsafe source resource"
            );
            let parent = source.join(relative.parent().unwrap());
            let file = std::fs::read_dir(parent)?
                .filter_map(|e| e.ok())
                .find(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(relative.file_name().unwrap().to_str().unwrap())
                })
                .context("missing original foliage image")?
                .path();
            let bytes = std::fs::read(file)?;
            let sha = hash(&bytes);
            let texture = pack.textures.len();
            pack.textures.push(Texture {
                id: format!("v20.foliage.texture.{prefix}"),
                path: format!("{sha}.png"),
                sha256: sha,
                source: rel,
            });
            images.push(bytes);
            let transform = node["transform"].as_array().context("native transform")?;
            let class = if grass {
                "fxGrassReplicator"
            } else {
                "fxFoliageReplicator"
            };
            let line = mission
                .lines()
                .position(|l| l.contains(&format!("new {class}(")))
                .context("source class declaration")?
                + 1;
            let mut adaptations=vec!["Pinned OpenMBG 9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7 engine-family foliage/LCG evidence; closed Blockland engine parity unverified".into(),"Native bounded resumable placement preserves retry ordering; absolute sway/light time advances culled instances instead of pausing their phase".into(),"Conservative native grid bounds replace source quadtree; typed placement ignores editor-only placementAreaHeight/debug flags".into()];
            if grass {
                adaptations.push("fxGrassReplicator implementation unavailable: seeded random fixed vertical plane rotation, square placement and vertex color are native interpretations of authored fields; base foliage algorithm reused".into());
            }
            let use_color = grass && flag("usecolour", false)?;
            let definition = Definition {
                id: format!("{scene_id}/foliage/{index}"),
                scene: scene_id.into(),
                node: index,
                texture,
                origin: [
                    transform[12].as_f64().unwrap() as f32,
                    transform[13].as_f64().unwrap() as f32,
                    transform[14].as_f64().unwrap() as f32,
                ],
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
                width: [number("minwidth", 1.)?, number("maxwidth", 1.)?],
                height: [number("minheight", 1.)?, number("maxheight", 1.)?],
                fixed_size: flag("fixsizetomax", false)?,
                fixed_aspect: flag("fixaspectratio", false)?,
                flip: flag("randomflip", false)?,
                billboard: !grass,
                random_rotation: grass && flag("israndomrot", false)?,
                rotation: number("rotationangle", 0.)?.to_radians(),
                sway: flag("swayon", false)?,
                sway_sync: flag("swaysync", false)?,
                sway_magnitude: [number("swaymagside", 0.)?, number("swaymagfront", 0.)?],
                sway_seconds: [number("minswaytime", 1.)?, number("maxswaytime", 1.)?],
                light: flag("lighton", false)?,
                light_sync: flag("lightsync", false)?,
                light_seconds: number("lighttime", 1.)?,
                luminance: [number("minluminance", 1.)?, number("maxluminance", 1.)?],
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
                evidence: Evidence {
                    source: "Add-Ons/Map_Bedroom.zip::bedroom.mis".into(),
                    sha256: hash(mission.as_bytes()),
                    line,
                    fields: f.clone(),
                    adaptations,
                },
            };
            definition.validate()?;
            ensure!(
                raw("surfacetype").is_none_or(|v| v == "Any"),
                "unsupported authored grass surface filter"
            );
            pack.definitions.push(definition);
        }
    }
    pack.validate()?;
    std::fs::create_dir(out)?;
    for (texture, bytes) in pack.textures.iter().zip(images) {
        std::fs::write(out.join(&texture.path), bytes)?;
    }
    std::fs::write(out.join("foliage.json"), serde_json::to_vec_pretty(&pack)?)?;
    pack.images(out)?;
    println!(
        "{} replicators / {} requested plants / {} original textures",
        pack.definitions.len(),
        pack.definitions.iter().map(|d| d.count).sum::<u32>(),
        pack.textures.len()
    );
    Ok(())
}
