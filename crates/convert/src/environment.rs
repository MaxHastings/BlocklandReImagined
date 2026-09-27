//! Offline sky/material-list conversion with byte-identical image resources.
use anyhow::{Context, Result, ensure};
use bri_content::{
    environment::{Cloud, Environment, Fog, Image},
    scene::{Kind, Scene},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};
const LIMIT: u64 = 32 * 1024 * 1024;
/// Read a bounded original resource without extracting archives or executing code.
pub fn read_original(root: &Path, name: &str) -> Result<Option<Vec<u8>>> {
    ensure!(
        bri_content::brick_materials::safe_relative(name),
        "Unsafe original environment path"
    );
    let path = root.join(name);
    if path.is_file() {
        let path = path.canonicalize()?;
        ensure!(
            path.starts_with(root),
            "Original environment path escapes installation"
        );
        let mut bytes = vec![];
        File::open(path)?.take(LIMIT + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() as u64 <= LIMIT, "Oversized original resource");
        return Ok(Some(bytes));
    }
    if let Some(rest) = name
        .get(8..)
        .filter(|_| name[..8].eq_ignore_ascii_case("Add-Ons/"))
    {
        let (addon, member) = rest.split_once('/').context("Invalid add-on path")?;
        let path = root.join("Add-Ons").join(format!("{addon}.zip"));
        if path.is_file() {
            let path = path.canonicalize()?;
            ensure!(
                path.starts_with(root),
                "Original archive escapes installation"
            );
            let mut zip = zip::ZipArchive::new(File::open(path)?)?;
            let matches: Vec<_> = zip
                .file_names()
                .filter(|n| n.eq_ignore_ascii_case(member))
                .map(str::to_owned)
                .collect();
            ensure!(matches.len() <= 1, "Ambiguous environment archive member");
            if let Some(name) = matches.first() {
                let mut bytes = vec![];
                zip.by_name(name)?.take(LIMIT + 1).read_to_end(&mut bytes)?;
                ensure!(
                    bytes.len() as u64 <= LIMIT,
                    "Oversized environment archive member"
                );
                return Ok(Some(bytes));
            }
        }
    }
    Ok(None)
}
fn vector<const N: usize>(
    p: &BTreeMap<String, String>,
    key: &str,
    default: [f32; N],
) -> Result<[f32; N]> {
    let Some(value) = p.get(key) else {
        return Ok(default);
    };
    let values = value
        .split_whitespace()
        .take(N)
        .map(str::parse::<f32>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        values.len() == N && values.iter().all(|x| x.is_finite()),
        "Invalid environment field {key}"
    );
    Ok(values.try_into().unwrap())
}
fn flag(p: &BTreeMap<String, String>, key: &str, default: bool) -> Result<bool> {
    match p.get(key).map(|x| x.to_ascii_lowercase()).as_deref() {
        None => Ok(default),
        Some("1" | "true") => Ok(true),
        Some("0" | "false") => Ok(false),
        _ => anyhow::bail!("Invalid sky flag {key}"),
    }
}
pub fn material_names(text: &str) -> Result<Vec<String>> {
    let names: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    ensure!(
        (6..=10).contains(&names.len())
            && names
                .iter()
                .all(|s| bri_content::brick_materials::safe_relative(s)),
        "Sky requires six faces, optional reflection map and up to three clouds"
    );
    Ok(names)
}
pub fn convert(
    root: &Path,
    source: &str,
    scene: &Scene,
    output: &Path,
) -> Result<Option<Environment>> {
    let skies: Vec<_> = scene
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, Kind::Sky))
        .collect();
    ensure!(
        skies.len() <= 1,
        "Multiple skies need explicit scene adaptation"
    );
    let Some(sky) = skies.first() else {
        return Ok(None);
    };
    let p = &sky.properties;
    let dml = p
        .get("materiallist")
        .context("Sky lacks materials")?
        .replace('\\', "/");
    let dml = if let Some(local) = dml.strip_prefix("./") {
        format!(
            "{}/{local}",
            source.rsplit_once('/').context("Mission lacks parent")?.0
        )
    } else if let Some(base) = dml.strip_prefix("~/") {
        format!(
            "{}/{base}",
            source.split_once('/').context("Mission lacks mod root")?.0
        )
    } else {
        dml
    };
    let textures = flag(p, "useskytextures", true)?;
    // Destruct deliberately has no material list and no sky textures. Preserve
    // its fog-color backdrop rather than inventing replacement image resources.
    let (bytes, names) = if dml.is_empty() && !textures {
        (Vec::new(), Vec::new())
    } else {
        let bytes = read_original(root, &dml)?
            .with_context(|| format!("Missing sky material list {dml}"))?;
        let names = material_names(std::str::from_utf8(&bytes)?)?;
        (bytes, names)
    };
    let mut images = vec![];
    for name in names {
        let stem = format!(
            "{}/{name}",
            dml.rsplit_once('/').context("Sky path lacks parent")?.0
        );
        let mut found = None;
        for path in [
            stem.clone(),
            format!("{stem}.png"),
            format!("{stem}.jpg"),
            format!("{stem}.jpeg"),
        ] {
            if let Some(bytes) = read_original(root, &path)? {
                found = Some((path, bytes));
                break;
            }
        }
        let (source, bytes) = found.with_context(|| format!("Missing sky image {stem}"))?;
        let decoded = image::load_from_memory(&bytes)?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let file = format!(
            "{hash}.{}",
            source.rsplit('.').next().context("Image lacks extension")?
        );
        std::fs::write(output.join(&file), bytes)?;
        images.push(Image {
            file,
            source,
            sha256: hash,
            width: decoded.width(),
            height: decoded.height(),
        });
    }
    let face_count = images.len().min(6);
    let faces = images.drain(..face_count).collect();
    let reflection = if images.is_empty() {
        None
    } else {
        Some(images.remove(0))
    };
    let wind = vector(p, "windvelocity", [0.0; 3])?;
    let direction = glam::Vec2::new(wind[0], wind[1]).normalize_or_zero();
    let mut clouds = vec![];
    for (i, image) in images.into_iter().enumerate() {
        let speed = vector(p, &format!("cloudspeed{}", i + 1), [0.0001])?[0];
        clouds.push(Cloud {
            image,
            center_height: vector(p, &format!("cloudheightper[{i}]"), [0.5])?[0],
            velocity: (direction * (speed / 0.032)).to_array(),
        });
    }
    let mut warnings = vec![];
    if reflection.is_some() {
        warnings.push("Original sky reflection texture retained; water/material reflection binding remains required".into());
    }
    for i in 1..=3 {
        let v = vector(p, &format!("fogvolume{i}"), [0.0; 3])?;
        if v[0] > 0.0 {
            warnings.push(format!("Active fog volume {i} is retained in scene properties; volume/storm rendering remains required"));
        }
    }
    let env = Environment {
        schema_version: 2,
        source_materials: dml,
        source_sha256: format!("{:x}", Sha256::digest(&bytes)),
        faces,
        reflection,
        clouds,
        textures,
        bottom: flag(p, "renderbottomtexture", false)?,
        horizon_band: !flag(p, "norenderbans", false)?,
        solid_color: vector(p, "skysolidcolor", [0.6; 3])?,
        fog: Fog {
            start: vector(p, "fogdistance", [250.0])?[0],
            end: vector(p, "visibledistance", [500.0])?[0],
            color: vector(p, "fogcolor", [0.5; 3])?,
        },
        warnings,
    };
    env.validate()?;
    Ok(Some(env))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reflection_slot_is_not_a_cloud_and_empty_skies_need_no_images() -> Result<()> {
        struct Fixture(std::path::PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let fixture = Fixture(std::env::temp_dir().join(format!(
                "bri-sky-import-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            )));
        std::fs::create_dir(&fixture.0)?;
        let root = fixture.0.canonicalize()?;
        let source = root.join("source");
        let output = root.join("output");
        std::fs::create_dir(&source)?;
        std::fs::create_dir(&output)?;
        for i in 0..10 {
            image::RgbaImage::from_pixel(1, 1, image::Rgba([i * 20, 0, 0, 255]))
                .save(source.join(format!("{i}.png")))?;
        }
        for (count, expected_clouds) in [(7, 0), (10, 3)] {
            let names = (0..count)
                .map(|i| format!("{i}.png"))
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(source.join("sky.dml"), names)?;
            let scene = crate::mission::read(
                "new Sky(Sky) { materialList=\"source/sky.dml\"; windVelocity=\"1 0 0\"; cloudSpeed1=\"0.032\"; };",
                "Add-Ons/Map_Test/map.mis",
            )?;
            let environment = convert(&root, "Add-Ons/Map_Test/map.mis", &scene, &output)?.unwrap();
            assert_eq!(environment.faces.len(), 6);
            assert_eq!(
                environment.reflection.as_ref().unwrap().source,
                "source/6.png"
            );
            assert_eq!(environment.clouds.len(), expected_clouds);
            if expected_clouds > 0 {
                assert_eq!(environment.clouds[0].image.source, "source/7.png");
                assert_eq!(environment.clouds[0].velocity, [1.0, 0.0]);
                assert_eq!(environment.clouds[2].image.source, "source/9.png");
            }
            for image in environment
                .faces
                .iter()
                .chain(environment.reflection.iter())
                .chain(environment.clouds.iter().map(|c| &c.image))
            {
                assert_eq!(
                    std::fs::read(root.join(&image.source))?,
                    std::fs::read(output.join(&image.file))?
                );
            }
        }
        let scene = crate::mission::read(
            "new Sky(Sky) { materialList=\"\"; useSkyTextures=\"0\"; fogColor=\"0 0 0 1\"; };",
            "Add-Ons/Map_Test/map.mis",
        )?;
        let empty = convert(&root, "Add-Ons/Map_Test/map.mis", &scene, &output)?.unwrap();
        assert!(empty.faces.is_empty() && empty.reflection.is_none() && empty.clouds.is_empty());
        assert_eq!(empty.fog.color, [0.0; 3]);
        Ok(())
    }
    #[test]
    fn rejects_incomplete_or_escaping_sky_lists() {
        assert_eq!(
            material_names("1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n\r\n")
                .unwrap()
                .len(),
            7
        );
        assert!(material_names("1\n2\n3\n4\n5").is_err());
        assert!(material_names("1\n2\n3\n4\n5\n../escape").is_err());
    }
}
