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
    // Compare canonical paths on both sides: Windows canonical paths carry
    // a `\\?\` prefix that a plain root never starts with.
    let root = &root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut path = root.join(name);
    if !path.exists() {
        // Blockland ran on Windows: a mission's `sky_spooky3` is the
        // installation's `Sky_Spooky3`.
        if let Some(found) = case_insensitive(root, name) {
            path = found;
        }
    }
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
/// `name` under `root` matched one folder at a time ignoring case, when
/// exactly one entry matches each part.
fn case_insensitive(root: &Path, name: &str) -> Option<std::path::PathBuf> {
    let mut path = root.to_path_buf();
    for part in name.split('/') {
        let mut matches = std::fs::read_dir(&path)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part));
        let entry = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        path = entry.path();
    }
    Some(path)
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
    convert_with_sky_bundle(root, source, scene, output, None)
}

/// Use exact original sky resources, or their verified copies already in
/// an installed native map bundle. The incoming mission owns all sky policy.
pub fn convert_with_sky_bundle(
    root: &Path,
    source: &str,
    scene: &Scene,
    output: &Path,
    sky_bundle: Option<&Path>,
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
        .map(|v| v.replace('\\', "/"))
        .unwrap_or_default();
    let dml = if dml.is_empty() {
        dml
    } else {
        crate::mission::asset_path(source, &dml)?
    };
    let textures = flag(p, "useskytextures", true)?;
    // Destruct deliberately has no material list and no sky textures. Preserve
    // its fog-color backdrop rather than inventing replacement image resources.
    let (source_sha256, images) = if dml.is_empty() && !textures {
        (format!("{:x}", Sha256::digest([])), Vec::new())
    } else if let Some(bytes) = read_original(root, &dml)? {
        let names = material_names(std::str::from_utf8(&bytes)?)?;
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
        (format!("{:x}", Sha256::digest(&bytes)), images)
    } else {
        installed_sky(sky_bundle, &dml, output)?
            .with_context(|| format!("Missing sky material list {dml}; supply its original assets with --reference or install matching vanilla map content"))?
    };
    let mut images = images;
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
        source_sha256,
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
            ..Default::default()
        },
        lens_flare: None,
        warnings,
    };
    env.validate()?;
    Ok(Some(env))
}

fn installed_sky(
    bundle: Option<&Path>,
    dml: &str,
    output: &Path,
) -> Result<Option<(String, Vec<Image>)>> {
    let Some(bundle) = bundle else {
        return Ok(None);
    };
    let bytes =
        read_original(bundle, "bundle.json")?.context("Installed sky bundle has no bundle.json")?;
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)?;
    let mut selected: Option<(String, Vec<Image>)> = None;
    for value in manifest["environments"]
        .as_object()
        .into_iter()
        .flat_map(|m| m.values())
    {
        if !value["source_materials"]
            .as_str()
            .is_some_and(|p| p.eq_ignore_ascii_case(dml))
        {
            continue;
        }
        let env: Environment = serde_json::from_value(value.clone())?;
        env.validate()?;
        let images: Vec<_> = env
            .faces
            .into_iter()
            .chain(env.reflection)
            .chain(env.clouds.into_iter().map(|c| c.image))
            .collect();
        ensure!(
            (6..=10).contains(&images.len()),
            "Installed sky {dml} has no complete image list"
        );
        if let Some((hash, previous)) = &selected {
            let identity = |images: &[Image]| {
                images
                    .iter()
                    .map(|i| {
                        (
                            i.source.to_ascii_lowercase(),
                            i.sha256.clone(),
                            i.width,
                            i.height,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            ensure!(
                hash == &env.source_sha256 && identity(previous) == identity(&images),
                "Ambiguous installed sky resources for {dml}"
            );
        } else {
            selected = Some((env.source_sha256, images));
        }
    }
    let Some((hash, images)) = selected else {
        return Ok(None);
    };
    // Validate every resource before writing. The native bundle contains
    // original image bytes, so identity is independent of any map's fog.
    let mut verified = Vec::new();
    for image in images {
        let bytes = read_original(bundle, &image.file)?
            .with_context(|| format!("Missing installed sky image {}", image.file))?;
        ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == image.sha256,
            "Installed sky image checksum mismatch: {}",
            image.file
        );
        let dimensions = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()?
            .into_dimensions()?;
        ensure!(
            dimensions == (image.width, image.height),
            "Installed sky image dimensions differ: {}",
            image.file
        );
        ensure!(
            bri_content::brick_materials::safe_relative(&image.source),
            "Unsafe installed sky source"
        );
        // Use the verified hash for the output basename.
        let ext = Path::new(&image.file)
            .extension()
            .and_then(|s| s.to_str())
            .context("Installed sky image lacks extension")?;
        let file = format!("{}.{ext}", image.sha256);
        let mut image = image;
        image.file = file;
        verified.push((image, bytes));
    }
    let mut images = Vec::new();
    for (image, bytes) in verified {
        std::fs::write(output.join(&image.file), bytes)?;
        images.push(image);
    }
    Ok(Some((hash, images)))
}
#[cfg(test)]
mod tests {
    use super::*;

    struct SkyFixture(std::path::PathBuf);
    impl Drop for SkyFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn installed_fixture() -> Result<(SkyFixture, Environment)> {
        let root = std::env::temp_dir().join(format!(
            "bri-installed-sky-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let fixture = SkyFixture(root.clone());
        let source = root.join("original/base/data/skies/Shared");
        std::fs::create_dir_all(&source)?;
        std::fs::create_dir(root.join("native"))?;
        std::fs::create_dir(root.join("empty"))?;
        std::fs::create_dir(root.join("out"))?;
        for i in 0..10 {
            image::RgbImage::from_pixel(1, 1, image::Rgb([i * 20, 0, 0]))
                .save(source.join(format!("{i}.png")))?;
        }
        std::fs::write(
            source.join("resource.dml"),
            (0..10)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        )?;
        let mission = "Add-Ons/Map_Original/map.mis";
        let scene = crate::mission::read(
            "new Sky() { materialList = \"~/data/skies/Shared/resource.dml\"; };",
            mission,
        )?;
        let env = convert(
            &root.join("original"),
            mission,
            &scene,
            &root.join("native"),
        )?
        .unwrap();
        std::fs::write(
            root.join("native/bundle.json"),
            serde_json::to_vec(
                &serde_json::json!({"environments":{"original":env,"another":env}}),
            )?,
        )?;
        Ok((fixture, env))
    }

    #[test]
    fn installed_exact_sky_preserves_images_but_uses_the_incoming_mission_policy() -> Result<()> {
        let (fixture, original) = installed_fixture()?;
        let root = &fixture.0;
        let mission = "Add-Ons/Map_New/map.mis";
        let scene = crate::mission::read(
            "new Sky() { materialList = \"~/data/skies/shared/resource.dml\"; useSkyTextures = \"0\"; skySolidColor = \"0.1 0.2 0.3\"; fogDistance = \"33\"; visibleDistance = \"444\"; fogColor = \"0.2 0.3 0.4\"; windVelocity = \"0 1 0\"; cloudSpeed1 = \"0.064\"; cloudHeightPer[0] = \"0.25\"; };",
            mission,
        )?;
        assert!(
            convert(&root.join("empty"), mission, &scene, &root.join("out"))
                .unwrap_err()
                .to_string()
                .contains("Missing sky material list")
        );
        let converted = convert_with_sky_bundle(
            &root.join("empty"),
            mission,
            &scene,
            &root.join("out"),
            Some(&root.join("native")),
        )?
        .unwrap();
        assert_eq!(converted.source_sha256, original.source_sha256);
        assert!(!converted.textures);
        assert_eq!(converted.fog.start, 33.0);
        assert_eq!(converted.fog.end, 444.0);
        assert_eq!(converted.solid_color, [0.1, 0.2, 0.3]);
        assert_eq!(converted.clouds[0].velocity, [0.0, 2.0]);
        assert_eq!(converted.clouds[0].center_height, 0.25);
        assert_eq!(
            converted.reflection.as_ref().unwrap().source,
            original.reflection.as_ref().unwrap().source
        );
        for image in converted
            .faces
            .iter()
            .chain(converted.reflection.iter())
            .chain(converted.clouds.iter().map(|c| &c.image))
        {
            assert_eq!(
                std::fs::read(root.join("native").join(&image.file))?,
                std::fs::read(root.join("out").join(&image.file))?
            );
        }
        Ok(())
    }

    #[test]
    fn installed_sky_refuses_other_materials_corruption_traversal_and_ambiguous_identity()
    -> Result<()> {
        let (fixture, env) = installed_fixture()?;
        let root = &fixture.0;
        let bundle = root.join("native");
        let output = root.join("out");
        let dml = env.source_materials.clone();
        assert!(
            installed_sky(Some(&bundle), "base/data/skies/Other/resource.dml", &output)?.is_none()
        );
        let pristine = serde_json::to_value(&env)?;
        for field in ["hash", "dimensions", "traversal", "ambiguous"] {
            let mut modified = pristine.clone();
            match field {
                "hash" => modified["faces"][0]["sha256"] = serde_json::json!("0".repeat(64)),
                "dimensions" => modified["faces"][0]["width"] = serde_json::json!(2),
                "traversal" => modified["faces"][0]["file"] = serde_json::json!("../outside.png"),
                "ambiguous" => modified["source_sha256"] = serde_json::json!("f".repeat(64)),
                _ => unreachable!(),
            }
            let mut environments = serde_json::json!({"modified":modified});
            if field == "ambiguous" {
                environments["original"] = pristine.clone();
            }
            std::fs::write(
                bundle.join("bundle.json"),
                serde_json::to_vec(&serde_json::json!({"environments":environments}))?,
            )?;
            assert!(
                installed_sky(Some(&bundle), &dml, &output).is_err(),
                "accepted {field}"
            );
            assert_eq!(
                std::fs::read_dir(&output)?.count(),
                0,
                "refusal wrote partial resources: {field}"
            );
        }
        Ok(())
    }
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
