use anyhow::{Context, Result, ensure};
use bri_weather::*;
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};
const LIMIT: u64 = 64 << 20;
fn read(path: &Path) -> Result<Vec<u8>> {
    let f = fs::File::open(path)?;
    ensure!(
        f.metadata()?.is_file() && f.metadata()?.len() <= LIMIT,
        "Oversized weather source"
    );
    let mut b = Vec::new();
    f.take(LIMIT + 1).read_to_end(&mut b)?;
    ensure!(
        b.len() as u64 <= LIMIT,
        "Weather source exceeded read bound"
    );
    Ok(b)
}
fn source(root: &Path, name: &str) -> Result<Vec<u8>> {
    ensure!(
        !name.contains(['\\', ':'])
            && name
                .split('/')
                .all(|s| !s.is_empty() && s != "." && s != ".."),
        "Unsafe source name"
    );
    let mut parts = name.split('/');
    if parts.next().unwrap().eq_ignore_ascii_case("add-ons") {
        let package = parts.next().context("Missing source package")?;
        let member = parts.collect::<Vec<_>>().join("/");
        let wanted = format!("{package}.zip");
        let paths: Vec<_> = fs::read_dir(root.join("Add-Ons"))?
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&wanted)
            })
            .collect();
        ensure!(
            paths.len() == 1,
            "Missing/ambiguous source archive {package}"
        );
        let path = paths[0].path().canonicalize()?;
        ensure!(path.starts_with(root), "Archive alias escaped source root");
        let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
        ensure!(zip.len() <= 65536, "ZIP entry budget exceeded");
        let names: Vec<_> = zip
            .file_names()
            .filter(|n| n.eq_ignore_ascii_case(&member))
            .map(str::to_owned)
            .collect();
        ensure!(names.len() == 1, "Missing/ambiguous source member {member}");
        let file = zip.by_name(&names[0])?;
        ensure!(file.size() <= LIMIT, "Oversized ZIP member");
        let mut data = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut data)?;
        ensure!(data.len() as u64 <= LIMIT, "ZIP member read bound exceeded");
        Ok(data)
    } else {
        let mut current = root.to_path_buf();
        for part in name.split('/') {
            let paths: Vec<_> = fs::read_dir(&current)?
                .collect::<std::io::Result<Vec<_>>>()?
                .into_iter()
                .filter(|p| p.file_name().to_string_lossy().eq_ignore_ascii_case(part))
                .collect();
            ensure!(paths.len() == 1, "Missing/ambiguous source {name}");
            current = paths[0].path();
        }
        let path = current.canonicalize()?;
        ensure!(path.starts_with(root), "Resource alias escaped source root");
        read(&path)
    }
}
fn fields(body: &str) -> Result<BTreeMap<String, String>> {
    let regex = Regex::new(r#"(?m)(\w+(?:\[\d+\])?)\s*=\s*(?:"([^"]*)"|([^;\r\n]+))\s*;"#)?;
    let mut result = BTreeMap::new();
    for c in regex.captures_iter(body) {
        let value = c.get(2).or_else(|| c.get(3)).unwrap().as_str().trim();
        ensure!(
            !value.contains(['$', '%', '{', '}']),
            "Nonliteral weather field"
        );
        ensure!(
            result
                .insert(c[1].to_lowercase(), value.to_owned())
                .is_none(),
            "Duplicate weather field"
        );
    }
    Ok(result)
}
fn declaration(script: &str, name: &str) -> Result<BTreeMap<String, String>> {
    let regex = Regex::new(&format!(
        r"(?is)datablock\s+PrecipitationData\s*\(\s*{}\s*\)\s*\{{([^{{}}]*)\}}\s*;",
        regex::escape(name)
    ))?;
    let matches: Vec<_> = regex.captures_iter(script).collect();
    ensure!(
        matches.len() == 1,
        "Missing/ambiguous precipitation datatype {name}"
    );
    fields(&matches[0][1])
}
fn number(f: &BTreeMap<String, String>, name: &str, default: f32) -> Result<f32> {
    let n = f
        .get(name)
        .map(|s| s.parse::<f32>())
        .transpose()?
        .unwrap_or(default);
    ensure!(n.is_finite(), "Nonfinite weather field {name}");
    Ok(n)
}
fn boolean(f: &BTreeMap<String, String>, name: &str, default: bool) -> Result<bool> {
    match f.get(name).map(|s| s.to_lowercase()).as_deref() {
        None => Ok(default),
        Some("1" | "true") => Ok(true),
        Some("0" | "false") => Ok(false),
        _ => anyhow::bail!("Invalid weather bool {name}"),
    }
}
fn vector(f: &BTreeMap<String, String>, name: &str, default: [f32; 3]) -> Result<[f32; 3]> {
    let Some(s) = f.get(name) else {
        return Ok(default);
    };
    let n: Vec<f32> = s
        .split_whitespace()
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        n.len() == 3 && n.iter().all(|v| v.is_finite()),
        "Invalid weather vector"
    );
    Ok([n[0], n[2], -n[1]])
}
fn proof(
    manifest: &mut WeatherManifest,
    files: &mut BTreeMap<String, Vec<u8>>,
    path: &str,
    kind: &str,
    bytes: Vec<u8>,
) {
    let hash = sha256(&bytes);
    let file = format!("{hash}.source");
    manifest.sources.push(SourceRecord {
        path: path.into(),
        sha256: hash,
        bytes: bytes.len() as u64,
        kind: kind.into(),
        copy_file: file.clone(),
    });
    files.insert(file, bytes);
}
fn image_resource(
    root: &Path,
    id: &str,
    rgb_path: &str,
    alpha_path: Option<&str>,
    manifest: &mut WeatherManifest,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    let rgb = source(root, rgb_path)?;
    let dimensions = image::ImageReader::new(Cursor::new(&rgb))
        .with_guessed_format()?
        .into_dimensions()?;
    ensure!(
        dimensions.0 > 0 && dimensions.1 > 0 && dimensions.0 <= 4096 && dimensions.1 <= 4096,
        "Weather source dimensions exceed bounds"
    );
    let decoded = image::load_from_memory(&rgb)?;
    ensure!(
        decoded.width() <= 4096 && decoded.height() <= 4096,
        "Weather texture too large"
    );
    let mut image = decoded.to_rgba8();
    let mut paths = vec![rgb_path.to_owned()];
    proof(manifest, files, rgb_path, "primary_original", rgb.clone());
    if let Some(path) = alpha_path {
        let bytes = source(root, path)?;
        ensure!(
            image::ImageReader::new(Cursor::new(&bytes))
                .with_guessed_format()?
                .into_dimensions()?
                == dimensions,
            "Alpha companion dimensions mismatch"
        );
        let alpha = image::load_from_memory(&bytes)?;
        ensure!(
            alpha.color() == image::ColorType::L8
                && alpha.width() == image.width()
                && alpha.height() == image.height(),
            "Original alpha companion must be matching grayscale JPEG"
        );
        for (pixel, a) in image.pixels_mut().zip(alpha.to_luma8().pixels()) {
            pixel[3] = a[0];
        }
        proof(manifest, files, path, "primary_original", bytes);
        paths.push(path.into());
    }
    let mut native = Cursor::new(Vec::new());
    image.write_to(&mut native, image::ImageFormat::Png)?;
    let native = native.into_inner();
    let hash = sha256(&native);
    let filename = format!("{hash}.png");
    manifest.textures.insert(
        id.into(),
        TextureRecord {
            file: filename.clone(),
            sha256: hash,
            rgba_sha256: sha256(image.as_raw()),
            width: image.width(),
            height: image.height(),
            source_paths: paths,
            alpha_policy: if alpha_path.is_some() {
                "grayscale_companion_bytes_replace_alpha"
            } else {
                "original_rgba_alpha"
            }
            .into(),
        },
    );
    files.insert(filename, native);
    Ok(())
}
fn main() -> Result<()> {
    let a: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    ensure!(
        a.len() == 4,
        "Usage: bri-weather-import <primary-root> <recovered-core.cs> <map-bundle-dir> <new-output-dir>"
    );
    let root = a[0].canonicalize()?;
    let maps = a[2].canonicalize()?;
    ensure!(!a[3].exists(), "Weather output must be fresh");
    let parent = a[3]
        .parent()
        .context("Missing output parent")?
        .canonicalize()?;
    let normalized = |path: &Path| path.to_string_lossy().replace('\\', "/").to_lowercase();
    let source_name = normalized(&root);
    let output_parent = normalized(&parent);
    ensure!(
        output_parent != source_name && !output_parent.starts_with(&(source_name + "/")),
        "Refusing original-install output"
    );
    let output = parent.join(a[3].file_name().context("Output basename missing")?);
    let mut manifest=WeatherManifest {schema_version:1,legacy_tick_seconds:0.032,definitions:Vec::new(),placements:Vec::new(),textures:BTreeMap::new(),sources:Vec::new(),assumptions:vec!["Motion fields retain original distance-per-tick values. Conversion uses corroborating OpenMBU GameBase 32ms tick; exact closed v20 engine remains comparison reference.".into(),"SnowA2x2 atlas is an explicit texture-evidence adaptation, kept in weather-import/atlas-adaptations.json; modern family default4 would cut flake silhouettes.".into(),"HeavyRain4x4 drop atlas /2x2 splash atlas, followCamera=true and animated splashes follow corroborating later-family defaults plus actual source image cell structure.".into(),"Sky windEffectPrecipitation controls use_wind; reference_wind_velocity negates native-converted source wind and divides by .032. Runtime requires host-supplied environment; no hardcoded global wind.".into(),"Weather sound playback is host/audio-owned. Neither authored precipitation datatype declares soundProfile.".into()]};
    let mut files = BTreeMap::new();
    let core = read(&a[1])?;
    let rain = source(&root, "Add-Ons/Map_Slate_Storm_Revised/rain.cs")?;
    let adaptations: Value = serde_json::from_slice(include_bytes!("../atlas-adaptations.json"))?;
    ensure!(
        adaptations["schema_version"] == 1,
        "Unsupported weather adaptation schema"
    );
    let declarations = [
        (
            "SnowA",
            declaration(std::str::from_utf8(&core)?, "SnowA")?,
            "base/data/specialfx/snow",
            adaptations["snowa"]["drops_per_side"]
                .as_u64()
                .context("Snow atlas adaptation missing")?
                .try_into()?,
        ),
        (
            "HeavyRain",
            declaration(std::str::from_utf8(&rain)?, "HeavyRain")?,
            "add-ons/map_slate_storm_revised/rain",
            4u32,
        ),
    ];
    proof(
        &mut manifest,
        &mut files,
        "recovered/core/allGameScripts-Vanilla.cs",
        "recovered_source",
        core,
    );
    proof(
        &mut manifest,
        &mut files,
        "Add-Ons/Map_Slate_Storm_Revised/rain.cs",
        "primary_original",
        rain,
    );
    proof(
        &mut manifest,
        &mut files,
        "weather-import/atlas-adaptations.json",
        "manual_adaptation",
        // As LF, whatever line endings this checkout gave the file: the pack
        // copies and hashes it, and must be the same on every machine.
        String::from_utf8_lossy(include_bytes!("../atlas-adaptations.json"))
            .replace("\r\n", "\n")
            .into_bytes(),
    );
    image_resource(
        &root,
        "base/data/specialfx/snow",
        "base/data/specialfx/snow.png",
        None,
        &mut manifest,
        &mut files,
    )?;
    image_resource(
        &root,
        "add-ons/map_slate_storm_revised/rain",
        "Add-Ons/Map_Slate_Storm_Revised/rain.jpg",
        Some("Add-Ons/Map_Slate_Storm_Revised/rain.alpha.jpg"),
        &mut manifest,
        &mut files,
    )?;
    image_resource(
        &root,
        "add-ons/map_slate_storm_revised/water_splash",
        "Add-Ons/Map_Slate_Storm_Revised/water_splash.jpg",
        Some("Add-Ons/Map_Slate_Storm_Revised/water_splash.alpha.jpg"),
        &mut manifest,
        &mut files,
    )?;
    let mut original_datatypes = BTreeMap::new();
    for (name, f, texture, side) in declarations {
        let id = format!("v20/weather/{}", name.to_lowercase());
        ensure!(
            f.get("droptexture").map(String::as_str)
                == Some(if name == "SnowA" {
                    "~/data/specialfx/snow"
                } else {
                    "./rain"
                }),
            "Weather source texture changed; inspect new binding"
        );
        ensure!(
            f.get("splashtexture").map(String::as_str)
                == if name == "SnowA" {
                    None
                } else {
                    Some("./water_splash")
                },
            "Weather source splash texture changed"
        );
        manifest.definitions.push(Definition {
            id,
            drop_texture: texture.into(),
            splash_texture: f
                .get("splashtexture")
                .filter(|v| !v.is_empty())
                .map(|_| "add-ons/map_slate_storm_revised/water_splash".into()),
            drop_radius: number(&f, "dropsize", 0.5)?,
            splash_radius: number(&f, "splashsize", 0.5)?,
            true_billboards: boolean(&f, "usetruebillboards", true)?,
            splash_seconds: number(&f, "splashms", 250.)? / 1000.,
            drop_animation_seconds: number(&f, "dropanimatems", 0.)? / 1000.,
            animate_splashes: boolean(&f, "animatesplashes", true)?,
            drops_per_side: side,
            splashes_per_side: 2,
        });
        original_datatypes.insert(name.to_owned(), f);
    }
    let bundle_bytes = read(&maps.join("bundle.json"))?;
    let bundle: Value = serde_json::from_slice(&bundle_bytes)?;
    ensure!(
        bundle["schema_version"] == 1,
        "Unsupported native bundle version"
    );
    proof(
        &mut manifest,
        &mut files,
        "native/map-bundle/bundle.json",
        "native_input",
        bundle_bytes,
    );
    let original_node = Regex::new(r"(?is)new\s+Precipitation\s*\([^)]*\)\s*\{([^{}]*)\}\s*;")?;
    let sky_regex = Regex::new(r"(?is)new\s+Sky\s*\([^)]*\)\s*\{([^{}]*)\}\s*;")?;

    for map in bundle["maps"]
        .as_array()
        .context("Missing native map list")?
    {
        let id = map["id"].as_str().context("Missing native map ID")?;
        let filename = map["file"].as_str().context("Missing native scene")?;
        let bytes = safe_file(&maps, filename, LIMIT)?;
        let scene: Value = serde_json::from_slice(&bytes)?;
        ensure!(
            scene["schema_version"] == 1 && scene["id"] == id,
            "Unsupported/mismatched native scene"
        );
        let nodes = scene["nodes"]
            .as_array()
            .context("Missing native scene nodes")?;
        let weather: Vec<_> = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n["kind"] == "precipitation")
            .collect();
        if weather.is_empty() {
            continue;
        }
        ensure!(
            id.starts_with("v20/add-ons/"),
            "Unexpected weather map source"
        );
        let original_path = &id[4..];
        let mission = source(&root, original_path)?;
        let source_nodes: Vec<_> = original_node
            .captures_iter(std::str::from_utf8(&mission)?)
            .map(|c| fields(&c[1]))
            .collect::<Result<_>>()?;
        ensure!(
            source_nodes.len() == weather.len(),
            "Native/source precipitation count mismatch"
        );
        let sky = nodes
            .iter()
            .find(|n| n["kind"] == "sky")
            .context("Weather map sky missing")?;
        let sky: BTreeMap<String, String> = serde_json::from_value(sky["properties"].clone())?;
        let sky_sources: Vec<_> = sky_regex
            .captures_iter(std::str::from_utf8(&mission)?)
            .collect();
        ensure!(sky_sources.len() == 1, "Missing/ambiguous original sky");
        let sky_source = fields(&sky_sources[0][1])?;
        for key in ["windvelocity", "windeffectprecipitation"] {
            ensure!(
                sky.get(key) == sky_source.get(key),
                "Native sky wind differs from original {id}.{key}"
            );
        }
        let wind = vector(&sky, "windvelocity", [0.; 3])?;
        for ((index, node), source_fields) in weather.into_iter().zip(source_nodes) {
            let properties: BTreeMap<String, String> =
                serde_json::from_value(node["properties"].clone())?;
            for (k, v) in &source_fields {
                ensure!(
                    properties.get(k) == Some(v),
                    "Native weather field differs from original {id}.{k}"
                );
            }
            let definition = format!(
                "v20/weather/{}",
                properties
                    .get("datablock")
                    .context("Weather datatype missing")?
                    .to_lowercase()
            );
            ensure!(
                manifest.definitions.iter().any(|d| d.id == definition),
                "Unadapted weather datatype {definition}"
            );
            let drops = number(&properties, "numdrops", 1024.)?;
            ensure!(
                drops >= 0. && drops.fract() == 0. && drops <= 65536.,
                "Invalid authored integer numDrops"
            );
            let position = vector(&properties, "position", [0.; 3])?;
            let transform = node["transform"]
                .as_array()
                .context("Missing native weather transform")?;
            for i in 0..3 {
                ensure!(
                    (transform[12 + i]
                        .as_f64()
                        .context("Invalid transform component")? as f32
                        - position[i])
                        .abs()
                        < 0.001,
                    "Weather transform coordinate mismatch"
                );
            }
            manifest.placements.push(Placement {
                id: format!("{id}#weather-{index}"),
                map_id: id.into(),
                definition,
                position,
                drops: drops as u32,
                width: number(&properties, "boxwidth", 200.)?,
                height: number(&properties, "boxheight", 100.)?,
                speed_per_tick: [
                    number(&properties, "minspeed", 1.5)?,
                    number(&properties, "maxspeed", 2.)?,
                ],
                mass: [
                    number(&properties, "minmass", 0.75)?,
                    number(&properties, "maxmass", 0.85)?,
                ],
                turbulence_amplitude: number(&properties, "maxturbulence", 0.1)?,
                turbulence_radians_per_tick: number(&properties, "turbulencespeed", 0.2)?,
                use_turbulence: boolean(&properties, "useturbulence", false)?,
                rotate_with_camera_velocity: boolean(&properties, "rotatewithcamvel", true)?,
                collision: boolean(&properties, "docollision", true)?,
                follow_camera: boolean(&properties, "followcam", true)?,
                use_wind: boolean(&sky, "windeffectprecipitation", false)?,
                authored_sky_wind: wind,
                reference_wind_velocity: wind.map(|v| -v / manifest.legacy_tick_seconds),
                original_fields: properties,
            });
        }
        proof(
            &mut manifest,
            &mut files,
            original_path,
            "primary_original",
            mission,
        );
        proof(
            &mut manifest,
            &mut files,
            &format!("native/map-bundle/{filename}"),
            "native_input",
            bytes,
        );
    }
    ensure!(
        manifest.placements.len() == 2,
        "Reference weather placement count changed; inspect new scope"
    );
    manifest.validate()?;
    fs::create_dir(&output)?;
    for (file, bytes) in files {
        fs::write(output.join(file), bytes)?;
    }
    fs::write(
        output.join("weather.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    fs::write(
        output.join("datatype-fields.json"),
        serde_json::to_vec_pretty(&original_datatypes)?,
    )?;
    WeatherPack::load(&output)?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"definitions":manifest.definitions.len(),"placements":manifest.placements.len(),"textures":manifest.textures.len(),"source_records":manifest.sources.len(),"original_drops":manifest.placements.iter().map(|p|p.drops).sum::<u32>(),"output":output})
        )?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_types_reject_nonliteral_or_duplicate_fields() {
        let f=declaration("datablock PrecipitationData(SnowA) { dropSize = 0.25; useTrueBillboards = 1; splashMS = 250; };","SnowA").unwrap();
        assert_eq!(number(&f, "dropsize", 0.).unwrap(), 0.25);
        assert!(boolean(&f, "usetruebillboards", false).unwrap());
        assert!(fields("numDrops = $bad;").is_err());
        assert!(fields("a=1; a=2;").is_err());
    }
}
