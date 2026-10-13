//! Offline import of Blockland's Steam release's Environment Add-Ons
//! (`Sky_*`, `Ground_*`, `Water_*`) into the environments pack
//! (`bri_content::environment_pack`). Follows v21's own scripts: a sky,
//! ground or water is offered when its file has a `-thumb.jpg` beside it
//! (`buildSkyBoxList`), and its values start from `setSkyBox`, `setGround`
//! and `setWater`'s defaults before its file's lines (`parseEnvironmentFile`).
//!
//! Usage: environment_pack <steam-blockland-dir|-> <out-dir>
//! With `-` (no Steam release on this machine) the pack is written empty.
use anyhow::{Context, Result, ensure};
use bri_console::Clamp;
use bri_content::{
    atmosphere::{
        DayCycle, FLARE_SIZE, MAX_DISTANCE, MIN_VISIBLE_DISTANCE, Settings, SunFlare, Vignette,
    },
    environment::{Cloud, Environment, Fog, Image},
    environment_pack::{self, Ground, Pack, Precipitation, Sky, Water},
};
use bri_convert::environment::{material_names, read_original};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    path::{Path, PathBuf},
};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 2,
        "Usage: environment_pack <steam-blockland-dir|-> <out-dir>"
    );
    let out = PathBuf::from(&args[1]);
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    std::fs::create_dir_all(&out)?;
    let pack = if args[0] == "-" {
        Pack {
            schema_version: environment_pack::SCHEMA,
            ..Default::default()
        }
    } else {
        import(&PathBuf::from(&args[0]).canonicalize()?, &out)?
    };
    pack.validate()?;
    std::fs::write(
        out.join(environment_pack::FILE),
        serde_json::to_vec_pretty(&pack)?,
    )?;
    println!(
        "environment pack: {} skies, {} grounds, {} waters",
        pack.skies.len(),
        pack.grounds.len(),
        pack.waters.len()
    );
    Ok(())
}

/// Every `<kind>_*` Add-On archive's files of `extension` with a
/// `-thumb.jpg` beside them, as `Add-Ons/<addon>/<file>`, in name order.
fn offered(root: &Path, kind: &str, extension: &str) -> Result<Vec<String>> {
    let mut archives: Vec<String> = std::fs::read_dir(root.join("Add-Ons"))?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            n.to_ascii_lowercase().starts_with(&format!("{kind}_"))
                && n.to_ascii_lowercase().ends_with(".zip")
        })
        .collect();
    archives.sort_by_key(|n| n.to_ascii_lowercase());
    let mut found = Vec::new();
    for archive in archives {
        let addon = &archive[..archive.len() - 4];
        let zip = zip::ZipArchive::new(File::open(root.join("Add-Ons").join(&archive))?)?;
        let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
        let mut files: Vec<&String> = names
            .iter()
            .filter(|n| !n.contains('/') && n.to_ascii_lowercase().ends_with(extension))
            .collect();
        files.sort_by_key(|n| n.to_ascii_lowercase());
        for file in files {
            let stem = &file[..file.len() - extension.len()];
            let thumb = format!("{stem}-thumb.jpg");
            if names.iter().any(|n| n.eq_ignore_ascii_case(&thumb)) {
                found.push(format!("Add-Ons/{addon}/{file}"));
            }
        }
    }
    Ok(found)
}

fn import(root: &Path, out: &Path) -> Result<Pack> {
    let mut pack = Pack {
        schema_version: environment_pack::SCHEMA,
        source: Some("Blockland (Steam)".into()),
        ..Default::default()
    };
    for path in offered(root, "sky", ".dml")? {
        pack.skies
            .push(sky(root, out, &path).with_context(|| format!("Sky {path}"))?);
    }
    for path in offered(root, "ground", ".ground")? {
        pack.grounds
            .push(ground(root, out, &path).with_context(|| format!("Ground {path}"))?);
    }
    for path in offered(root, "water", ".water")? {
        pack.waters
            .push(water(root, out, &path).with_context(|| format!("Water {path}"))?);
    }
    Ok(pack)
}

/// An original file copied into the pack under its hash.
fn store(out: &Path, source: &str, bytes: &[u8]) -> Result<Image> {
    let decoded = image::load_from_memory(bytes).with_context(|| format!("Decoding {source}"))?;
    let hash = format!("{:x}", Sha256::digest(bytes));
    let extension = source
        .rsplit('.')
        .next()
        .context("Image lacks extension")?
        .to_ascii_lowercase();
    let file = format!("{hash}.{extension}");
    std::fs::write(out.join(&file), bytes)?;
    Ok(Image {
        file,
        source: source.into(),
        sha256: hash,
        width: decoded.width(),
        height: decoded.height(),
    })
}
/// A one-pixel picture of `color` standing in for a missing `source`.
fn plain(out: &Path, source: &str, color: [f32; 4]) -> Result<Image> {
    let pixel =
        image::RgbaImage::from_pixel(1, 1, image::Rgba(color.map(|c| (c * 255.0).round() as u8)));
    let mut bytes = Vec::new();
    pixel.write_to(
        &mut std::io::Cursor::new(&mut bytes),
        image::ImageFormat::Png,
    )?;
    store(out, &format!("{source}.png"), &bytes)
}
/// `name` as given, or with `.png`/`.jpg` added, as Torque finds textures.
fn texture(root: &Path, out: &Path, name: &str) -> Result<Option<Image>> {
    let name = name.replace('\\', "/");
    for path in [
        name.clone(),
        format!("{name}.png"),
        format!("{name}.jpg"),
        format!("{name}.jpeg"),
    ] {
        if let Some(bytes) = read_original(root, &path)? {
            return store(out, &path, &bytes).map(Some);
        }
    }
    Ok(None)
}
fn thumbnail(root: &Path, out: &Path, path: &str) -> Result<Image> {
    let stem = path.rsplit_once('.').context("No extension")?.0;
    texture(root, out, &format!("{stem}-thumb.jpg"))?.context("Missing thumbnail")
}
fn id(path: &str) -> String {
    path.to_ascii_lowercase()
}
/// "Slate_Desert.dml" reads "Slate Desert", "defaultDayCycle.dml" "Default
/// Day Cycle", "TTcementBlue01.ground" (with `digits` false) "Cement Blue".
fn label(path: &str, digits: bool) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let stem = stem
        .strip_prefix("TT")
        .filter(|rest| rest.starts_with(|c: char| c.is_ascii_lowercase()))
        .unwrap_or(stem);
    let stem = if digits {
        stem
    } else {
        stem.trim_end_matches(|c: char| c.is_ascii_digit())
    };
    let mut words = String::new();
    let mut previous = ' ';
    for c in stem.chars() {
        let c = if c == '_' || c == '-' { ' ' } else { c };
        if c.is_ascii_uppercase() && (previous.is_ascii_lowercase() || previous.is_ascii_digit()) {
            words.push(' ');
        }
        words.push(c);
        previous = c;
    }
    let mut words: Vec<&str> = words.split_whitespace().collect();
    // "groundLava" is Lava in the Ground picker.
    if words.len() > 1
        && ["ground", "water", "sky"].contains(&words[0].to_ascii_lowercase().as_str())
    {
        words.remove(0);
    }
    words
        .into_iter()
        .map(|w| {
            let mut chars = w.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A `.atmosphere`, `.ground` or `.water` file: `$Name value...` lines,
/// names in lower case.
fn values(root: &Path, path: &str) -> Result<BTreeMap<String, String>> {
    let Some(bytes) = read_original(root, path)? else {
        return Ok(BTreeMap::new());
    };
    let text = String::from_utf8_lossy(&bytes);
    Ok(text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                return None;
            }
            let (label, value) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            Some((label.to_ascii_lowercase(), value.trim().to_owned()))
        })
        .collect())
}
fn number(v: &BTreeMap<String, String>, key: &str, default: f32, range: (f32, f32)) -> f32 {
    v.get(key)
        .and_then(|s| s.split_whitespace().next()?.parse::<f32>().ok())
        .filter(|x| x.is_finite())
        .unwrap_or(default)
        .clamped(range.0, range.1)
}
fn flag(v: &BTreeMap<String, String>, key: &str, default: bool) -> bool {
    v.get(key)
        .and_then(|s| s.split_whitespace().next()?.parse::<f32>().ok())
        .map_or(default, |x| x != 0.0)
}
/// v21's `getColorF`/`getColorI`: a 6 or 8 digit hex word, or numbers that
/// are 0 to 255 when any is above 1 and 0 to 1 otherwise; alpha defaults
/// to opaque. Straight RGBA, 0 to 1.
fn color(text: &str) -> Option<[f32; 4]> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if let [word] = words[..]
        && matches!(word.len(), 6 | 8)
        && word.bytes().all(|b| b.is_ascii_hexdigit())
    {
        let byte = |i: usize| {
            u8::from_str_radix(&word[i..i + 2], 16)
                .ok()
                .map(|b| b as f32 / 255.0)
        };
        return Some([
            byte(0)?,
            byte(2)?,
            byte(4)?,
            if word.len() == 8 { byte(6)? } else { 1.0 },
        ]);
    }
    let numbers: Vec<f32> = words
        .iter()
        .take(4)
        .map(|w| w.parse::<f32>().ok().filter(|x| x.is_finite()))
        .collect::<Option<_>>()?;
    if numbers.len() < 3 {
        return None;
    }
    let scale = if numbers.iter().any(|x| *x > 1.0) {
        255.0
    } else {
        1.0
    };
    let mut c = [1.0; 4];
    for (i, x) in numbers.iter().enumerate() {
        c[i] = (x / scale).clamped(0.0, 1.0);
    }
    Some(c)
}
fn color_of(v: &BTreeMap<String, String>, key: &str, default: [f32; 4]) -> [f32; 4] {
    v.get(key).and_then(|s| color(s)).unwrap_or(default)
}
fn rgb(c: [f32; 4]) -> [f32; 3] {
    [c[0], c[1], c[2]]
}
fn pair(v: &BTreeMap<String, String>, key: &str) -> [f32; 2] {
    let words: Vec<f32> = v
        .get(key)
        .map(|s| {
            s.split_whitespace()
                .filter_map(|w| w.parse::<f32>().ok())
                .filter(|x| x.is_finite())
                .map(|x| x.clamped(-100.0, 100.0))
                .collect()
        })
        .unwrap_or_default();
    [
        words.first().copied().unwrap_or(0.0),
        words.get(1).copied().unwrap_or(0.0),
    ]
}

fn sky(root: &Path, out: &Path, dml: &str) -> Result<Sky> {
    let folder = dml.rsplit_once('/').context("Sky path lacks parent")?.0;
    let bytes = read_original(root, dml)?.context("Missing material list")?;
    let names = material_names(&String::from_utf8_lossy(&bytes))?;
    let stem = dml.rsplit_once('.').context("No extension")?.0;
    let v = values(root, &format!("{stem}.atmosphere"))?;
    let mut images = Vec::new();
    let mut warnings = Vec::new();
    for name in &names {
        match texture(root, out, &format!("{folder}/{name}"))? {
            Some(image) => images.push(image),
            // Slate Desert's list names a floor its Add-On lacks; Torque
            // draws nothing there, so the fog shows.
            None => {
                warnings.push(format!("{name} is missing; drawn in the fog colour"));
                images.push(plain(
                    out,
                    &format!("{folder}/{name}"),
                    color_of(&v, "$sky::fogcolor", [1.0; 4]),
                )?);
            }
        }
    }
    // setSkyBox's defaults, then the file.
    let visible = number(&v, "$sky::visibledistance", 800.0, (0.0, 1000.0));
    let fog_start = number(&v, "$sky::fogdistance", 400.0, (0.0, 1000.0));
    let fog = color_of(&v, "$sky::fogcolor", [1.0; 4]);
    let wind: Vec<f32> = v
        .get("$sky::windvelocity")
        .map(|s| {
            s.split_whitespace()
                .filter_map(|w| w.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    let wind = glam::Vec2::new(
        wind.first().copied().unwrap_or(0.0),
        wind.get(1).copied().unwrap_or(0.0),
    );
    // Torque drifts clouds along the wind; with none set, along +X.
    let direction = if wind.length() > 0.0 {
        wind.normalize()
    } else {
        glam::Vec2::X
    };
    let face_count = images.len().min(6);
    let faces: Vec<Image> = images.drain(..face_count).collect();
    let reflection = (!images.is_empty()).then(|| images.remove(0));
    let clouds = images
        .into_iter()
        .enumerate()
        .map(|(i, image)| Cloud {
            image,
            center_height: number(&v, &format!("$sky::cloudheight{i}"), 0.0, (0.0, 100.0))
                .clamped(0.05, 1.0),
            velocity: (direction
                * (number(&v, &format!("$sky::cloudspeed{i}"), 0.0, (0.0, 10.0)) / 0.032))
                .to_array(),
        })
        .collect();
    let render_bans = flag(&v, "$sky::renderbans", true);
    let environment = Environment {
        schema_version: 2,
        source_materials: id(dml),
        source_sha256: format!("{:x}", Sha256::digest(&bytes)),
        faces,
        reflection,
        clouds,
        textures: true,
        // v21 draws the box's floor only where no ground hides it; the
        // client decides per map.
        bottom: false,
        horizon_band: render_bans,
        solid_color: [0.6; 3],
        fog: Fog {
            start: fog_start
                .min(visible.max(MIN_VISIBLE_DISTANCE) - 1.0)
                .max(0.0),
            end: visible.max(MIN_VISIBLE_DISTANCE),
            color: rgb(fog),
        },
        warnings,
    };
    let flare_color = color_of(&v, "$sky::sunflarecolor", [1.0; 4]);
    let flare_size = number(&v, "$sky::sunflaresize", 1.0, (0.0, 10.0));
    let azimuth = number(&v, "$sky::sunazimuth", 250.0, (-360.0, 360.0)).rem_euclid(360.0);
    let look = Settings {
        day_cycle: flag(&v, "$sky::daycycleenabled", false).then(|| DayCycle {
            length_seconds: number(&v, "$sky::daylength", 300.0, (10.0, 86_400.0)),
            time: 0.5,
            anchor_tick: 0,
        }),
        sun_azimuth: Some(azimuth),
        sun_elevation: Some(number(&v, "$sky::sunelevation", 35.0, (-90.0, 90.0))),
        direct_light: Some(rgb(color_of(&v, "$sky::directlightcolor", [1.0; 4]))),
        ambient_light: Some(rgb(color_of(
            &v,
            "$sky::ambientlightcolor",
            [0.5, 0.5, 0.5, 1.0],
        ))),
        shadow_color: Some(rgb(color_of(&v, "$sky::shadowcolor", [0.3, 0.3, 0.5, 1.0]))),
        sun_flare: Some(if flare_size <= 0.0 {
            SunFlare {
                color: [flare_color[0], flare_color[1], flare_color[2], 0.0],
                size: *FLARE_SIZE.start(),
            }
        } else {
            SunFlare {
                color: flare_color,
                size: flare_size.clamped(*FLARE_SIZE.start(), *FLARE_SIZE.end()),
            }
        }),
        visible_distance: Some(visible.clamped(MIN_VISIBLE_DISTANCE, MAX_DISTANCE)),
        fog_distance: Some(fog_start.clamped(0.0, MAX_DISTANCE)),
        fog_color: Some(rgb(fog)),
        sky_color: Some(rgb(color_of(&v, "$sky::skycolor", [1.0; 4]))),
        vignette: Some(Vignette {
            color: color_of(&v, "$sky::vignettecolor", [0.0, 0.0, 0.0, 0.2]),
            multiply: flag(&v, "$sky::vignettemultiply", false),
        }),
        sky: None,
        ..Default::default()
    };
    let precipitation = match v.get("$rain::droptexture") {
        // Spooky3 Snow's snow is `base/data/specialfx/snow.png`: a release
        // without it is not the whole of Steam's Blockland.
        Some(drop) => match Some(
            texture(root, out, drop)?
                .with_context(|| format!("{drop}, the rain or snow of {dml}, is missing"))?,
        ) {
            Some(drop) => Some(Precipitation {
                drop,
                splash: match v.get("$rain::splashtexture") {
                    Some(s) => texture(root, out, s)?,
                    None => None,
                },
                drop_size: number(&v, "$rain::dropsize", 0.5, (0.0, 10.0)),
                splash_size: number(&v, "$rain::splashsize", 1.0, (0.0, 10.0)),
                splash_ms: number(&v, "$rain::splashms", 100.0, (0.0, 1000.0)) as u32,
                true_billboards: flag(&v, "$rain::usetruebillboards", false),
                speed: [
                    number(&v, "$rain::minspeed", 1.5, (0.0, 10.0)),
                    number(&v, "$rain::maxspeed", 2.0, (0.0, 10.0)),
                ],
                mass: [
                    number(&v, "$rain::minmass", 0.75, (0.0, 10.0)),
                    number(&v, "$rain::maxmass", 0.85, (0.0, 10.0)),
                ],
                max_turbulence: number(&v, "$rain::maxturbulence", 0.1, (0.0, 10.0)),
                turbulence_speed: number(&v, "$rain::turbulencespeed", 0.2, (0.0, 10.0)),
                rotate_with_camera: flag(&v, "$rain::rotatewithcamvel", true),
                turbulence: flag(&v, "$rain::useturbulence", false),
                drops: number(&v, "$rain::numdrops", 1024.0, (1.0, 10_000.0)) as u32,
                box_width: number(&v, "$rain::boxwidth", 200.0, (0.0, 1000.0)),
                box_height: number(&v, "$rain::boxheight", 100.0, (0.0, 1000.0)),
                collision: flag(&v, "$rain::docollision", true),
            }),
            None => None,
        },
        None => None,
    };
    Ok(Sky {
        id: id(dml),
        label: label(dml, true),
        thumbnail: thumbnail(root, out, dml)?,
        environment,
        look,
        precipitation,
    })
}

fn ground(root: &Path, out: &Path, path: &str) -> Result<Ground> {
    let v = values(root, path)?;
    let top = v
        .get("$ground::toptexture")
        .context("No $Ground::topTexture")?;
    Ok(Ground {
        id: id(path),
        label: label(path, false),
        thumbnail: thumbnail(root, out, path)?,
        texture: texture(root, out, top)?.with_context(|| format!("Missing {top}"))?,
        loops_per_unit: number(&v, "$ground::loopsperunit", 1.0, (0.001, 10.0)),
        color: color_of(&v, "$ground::color", [1.0; 4]),
        color_multiply: flag(&v, "$ground::colormultiply", true),
        scroll: pair(&v, "$ground::scrollspeed"),
    })
}

fn water(root: &Path, out: &Path, path: &str) -> Result<Water> {
    let v = values(root, path)?;
    let top = v
        .get("$water::toptexture")
        .context("No $Water::topTexture")?;
    let top = texture(root, out, top)?.with_context(|| format!("Missing {top}"))?;
    let bottom = match v.get("$water::bottomtexture") {
        Some(b) => texture(root, out, b)?.unwrap_or_else(|| top.clone()),
        None => top.clone(),
    };
    Ok(Water {
        id: id(path),
        label: label(path, false),
        thumbnail: thumbnail(root, out, path)?,
        top,
        bottom,
        loops_per_unit: number(&v, "$water::loopsperunit", 1.0, (0.001, 10.0)),
        color: color_of(&v, "$water::color", [1.0, 1.0, 1.0, 128.0 / 255.0]),
        overlay_color: color_of(
            &v,
            "$water::overlaycolor",
            [0.0, 0.0, 128.0 / 255.0, 128.0 / 255.0],
        ),
        color_multiply: flag(&v, "$water::colormultiply", true),
        height: number(
            &v,
            "$water::height",
            5.0,
            (0.0, environment_pack::MAX_WATER_HEIGHT),
        ),
        kill: flag(&v, "$water::kill", false),
        scroll: pair(&v, "$water::scrollspeed"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colors_read_as_v21_reads_them() {
        assert_eq!(color("DED1C1").unwrap()[0], 222.0 / 255.0);
        assert_eq!(
            color("0.000 0.500 0.250 1.000"),
            Some([0.0, 0.5, 0.25, 1.0])
        );
        assert_eq!(color("240 97 17 190").unwrap()[3], 190.0 / 255.0);
        assert_eq!(color("1.0 1.0 1.0 0.5"), Some([1.0, 1.0, 1.0, 0.5]));
        assert_eq!(color("0.95 0.95 1.0"), Some([0.95, 0.95, 1.0, 1.0]));
        assert_eq!(color("0 0 0 100").unwrap()[3], 100.0 / 255.0);
        assert_eq!(color("949392").unwrap()[2], 146.0 / 255.0);
    }
    #[test]
    fn labels_read_like_names() {
        assert_eq!(
            label("Add-Ons/Sky_Slate_Desert/Slate_Desert.dml", true),
            "Slate Desert"
        );
        assert_eq!(
            label("Add-Ons/Sky_Blank/defaultDayCycle.dml", true),
            "Default Day Cycle"
        );
        assert_eq!(
            label("Add-Ons/Sky_Spooky3/Spooky3Snow.dml", true),
            "Spooky3 Snow"
        );
        assert_eq!(
            label("Add-Ons/Ground_Bedroom/TTcementBlue01.ground", false),
            "Cement Blue"
        );
        assert_eq!(
            label("Add-Ons/Ground_TT/gravelly-walk.ground", false),
            "Gravelly Walk"
        );
        assert_eq!(
            label("Add-Ons/Ground_Bedroom/carpet1.ground", false),
            "Carpet"
        );
        assert_eq!(
            label("Add-Ons/Ground_Lava/groundLava.ground", false),
            "Lava"
        );
    }
}
