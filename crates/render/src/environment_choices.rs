//! The skies and grounds the Environment window offers over a map's own
//! (`bri_content::atmosphere::Settings::sky` and `ground`). Both come from
//! the converted map bundle, so nothing new is imported: every stock map's
//! sky box, and every texture a map's terrain is painted with. Picking one
//! changes only what is drawn; the map's fog, light and collision stay.
use crate::{
    scene::{MaterialKind, SceneData, SceneImage},
    scene_loader::{decode, file, read},
    terrain_scene::TerrainScene,
};
use anyhow::{Context, Result, ensure};
use bri_content::{
    atmosphere::PLAIN_SKY,
    environment::{Environment, Image},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

/// Thumbnails are this many pixels square.
pub const THUMBNAIL: u32 = 64;

/// A sky box to pick: a stock map's sky, or the plain one.
#[derive(Clone, Debug)]
pub struct SkyChoice {
    /// Its name in the settings: the sky's material list in lower case.
    pub id: String,
    pub label: String,
    /// None for the plain sky.
    environment: Option<Environment>,
}
/// A texture to lay over a map's terrain.
#[derive(Clone, Debug)]
pub struct GroundChoice {
    /// Its name in the settings: the texture's source in lower case.
    pub id: String,
    pub label: String,
    file: String,
}

/// What a map bundle offers, and each map's own sky.
#[derive(Clone, Debug)]
pub struct Choices {
    root: PathBuf,
    pub skies: Vec<SkyChoice>,
    pub grounds: Vec<GroundChoice>,
    /// Each map's own sky, by map ID.
    maps: BTreeMap<String, Environment>,
}

/// A sky's name: its material list, lower case with forward slashes.
pub fn sky_id(environment: &Environment) -> String {
    environment
        .source_materials
        .replace('\\', "/")
        .to_lowercase()
}
/// "base/data/skies/Sky_Blue2/resource.dml" reads "Blue2",
/// "Add-Ons/Map_Slate_Desert/cloudy02.dml" "Slate Desert": the folder the
/// sky lives in, without its kind.
fn sky_label(source: &str) -> String {
    let path = source.replace('\\', "/");
    let mut parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    parts.pop();
    let folder = parts.last().copied().unwrap_or("Sky");
    let lower = folder.to_lowercase();
    let name = ["sky_", "map_"]
        .iter()
        .find(|prefix| lower.starts_with(*prefix))
        .map_or(folder, |prefix| &folder[prefix.len()..]);
    let name = name.replace('_', " ");
    let mut chars = name.chars();
    chars.next().map_or_else(
        || "Sky".into(),
        |first| first.to_uppercase().chain(chars).collect(),
    )
}
/// "Add-Ons/Map_Slopes/TTicefrosty1" reads "TTicefrosty1".
fn ground_label(source: &str) -> String {
    let path = source.replace('\\', "/");
    let name = path.rsplit('/').next().unwrap_or(&path);
    name.rsplit_once('.')
        .map_or(name, |(stem, _)| stem)
        .to_string()
}

impl Choices {
    /// The skies and grounds of the map bundle at `root`, in its maps'
    /// order, each once.
    pub fn load(root: &Path) -> Result<Self> {
        let root = root.canonicalize().context("Opening native map bundle")?;
        let bundle: Value = serde_json::from_slice(&read(&root, "bundle.json")?)?;
        Self::from_bundle(root, &bundle)
    }
    fn from_bundle(root: PathBuf, bundle: &Value) -> Result<Self> {
        let mut maps = BTreeMap::new();
        if let Some(environments) = bundle.get("environments").and_then(Value::as_object) {
            for (map, environment) in environments {
                let environment: Environment = serde_json::from_value(environment.clone())
                    .with_context(|| format!("Sky of {map}"))?;
                maps.insert(map.clone(), environment);
            }
        }
        let mut skies: Vec<SkyChoice> = Vec::new();
        for environment in maps.values() {
            // A map that draws its sky in one colour shows nothing of the
            // box's pictures; the plain sky stands for those.
            if !environment.textures || environment.faces.len() != 6 {
                continue;
            }
            let id = sky_id(environment);
            if skies.iter().any(|s| s.id == id) {
                continue;
            }
            skies.push(SkyChoice {
                label: sky_label(&environment.source_materials),
                id,
                environment: Some(environment.clone()),
            });
        }
        skies.push(SkyChoice {
            id: PLAIN_SKY.into(),
            label: "Plain".into(),
            environment: None,
        });
        let mut grounds: Vec<GroundChoice> = Vec::new();
        for binding in bundle
            .get("bindings")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|b| b.get("terrain_layer").is_some())
        {
            let (Some(source), Some(texture)) =
                (binding["source"].as_str(), binding["texture"].as_str())
            else {
                continue;
            };
            // Maps share textures under their own folders; one is enough.
            if grounds.iter().any(|g| g.file == texture) {
                continue;
            }
            grounds.push(GroundChoice {
                id: source.replace('\\', "/").to_lowercase(),
                label: ground_label(source),
                file: texture.into(),
            });
        }
        Ok(Self {
            root,
            skies,
            grounds,
            maps,
        })
    }
    /// The sky `map` draws on its own, by name; None when it has no sky box.
    pub fn map_sky(&self, map: &str) -> Option<String> {
        self.maps
            .get(map)
            .filter(|e| e.textures && e.faces.len() == 6)
            .map(sky_id)
    }
    pub fn sky(&self, id: &str) -> Option<&SkyChoice> {
        self.skies.iter().find(|s| s.id == id)
    }
    pub fn ground(&self, id: &str) -> Option<&GroundChoice> {
        self.grounds.iter().find(|g| g.id == id)
    }
    /// A small picture of a sky: the box's first side under its first
    /// cloud layer (a storm is mostly cloud), or plain white.
    pub fn sky_thumbnail(&self, choice: &SkyChoice) -> Result<SceneImage> {
        let Some(environment) = &choice.environment else {
            return Ok(SceneImage {
                label: choice.id.clone(),
                width: THUMBNAIL,
                height: THUMBNAIL,
                rgba: vec![255; (THUMBNAIL * THUMBNAIL * 4) as usize],
                srgb: true,
            });
        };
        let face = environment.faces.first().context("Sky has no sides")?;
        let mut picture = thumbnail(&image(&self.root, face)?);
        if let Some(cloud) = environment.clouds.first() {
            let cloud = thumbnail(&image(&self.root, &cloud.image)?);
            for (under, over) in picture
                .rgba
                .chunks_exact_mut(4)
                .zip(cloud.rgba.chunks_exact(4))
            {
                let a = u32::from(over[3]);
                for c in 0..3 {
                    under[c] =
                        ((u32::from(over[c]) * a + u32::from(under[c]) * (255 - a)) / 255) as u8;
                }
            }
        }
        Ok(picture)
    }
    pub fn ground_thumbnail(&self, choice: &GroundChoice) -> Result<SceneImage> {
        Ok(thumbnail(&decode(
            &read(&self.root, &choice.file)?,
            &choice.id,
            true,
        )?))
    }
    /// Draw sky `id` in `scene` (map `map`'s) in place of the sky it has:
    /// None, or a sky this bundle lacks, gives the map its own back. The
    /// map's fog stays, and the box keeps the map's size.
    pub fn set_sky(&self, scene: &mut SceneData, map: &str, id: Option<&str>) -> Result<()> {
        let picked = id.and_then(|id| self.sky(id));
        let Some(own) = self.maps.get(map) else {
            // A map without a sky keeps none unless one is picked.
            strip_sky(scene);
            return match picked {
                Some(choice) => self.append_choice(scene, choice),
                None => Ok(()),
            };
        };
        strip_sky(scene);
        match picked {
            Some(choice) => self.append_choice(scene, choice),
            None => append_sky(&self.root, scene, own),
        }
    }
    fn append_choice(&self, scene: &mut SceneData, choice: &SkyChoice) -> Result<()> {
        let mut environment = match &choice.environment {
            Some(environment) => environment.clone(),
            None => plain_sky(),
        };
        // Keep the map's fog (and so the size of its box).
        if scene.fog.end > 0.0 {
            environment.fog = scene.fog;
        }
        environment.warnings.clear();
        if choice.environment.is_some() {
            append_sky(&self.root, scene, &environment)
        } else {
            let white = scene.images.len();
            scene.images.push(SceneImage::white());
            crate::environment_scene::append(scene, &environment, &[white; 6], &[])
        }
    }
    /// `terrain` painted all over with ground `id`, without its detail and
    /// bump passes; None when this bundle lacks that ground.
    pub fn grounded(&self, terrain: &TerrainScene, id: &str) -> Result<Option<TerrainScene>> {
        let Some(choice) = self.ground(id) else {
            return Ok(None);
        };
        let image = decode(&read(&self.root, &choice.file)?, &choice.id, true)?;
        terrain.with_ground(image).map(Some)
    }
}

/// The plain sky: six white sides, which Sky Color tints.
fn plain_sky() -> Environment {
    let side = Image {
        file: "plain.png".into(),
        source: PLAIN_SKY.into(),
        sha256: "0".repeat(64),
        width: 1,
        height: 1,
    };
    Environment {
        schema_version: 2,
        source_materials: PLAIN_SKY.into(),
        source_sha256: "0".repeat(64),
        faces: vec![side; 6],
        reflection: None,
        clouds: Vec::new(),
        textures: true,
        bottom: true,
        horizon_band: false,
        solid_color: [1.0; 3],
        fog: bri_content::environment::Fog {
            start: 500.0,
            end: 600.0,
            color: [1.0; 3],
        },
        warnings: Vec::new(),
    }
}

/// One of a sky's pictures, checked against its recorded checksum.
fn image(root: &Path, image: &Image) -> Result<SceneImage> {
    use sha2::{Digest, Sha256};
    let mut bytes = Vec::new();
    std::fs::File::open(file(root, &image.file)?)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 32 * 1024 * 1024 && format!("{:x}", Sha256::digest(&bytes)) == image.sha256,
        "Sky image checksum mismatch: {}",
        image.source
    );
    let decoded = decode(&bytes, &image.source, true)?;
    ensure!(
        decoded.width == image.width && decoded.height == image.height,
        "Sky image dimensions changed"
    );
    Ok(decoded)
}

/// Add `environment`'s sky box, clouds and fog to `out`.
pub(crate) fn append_sky(
    root: &Path,
    out: &mut SceneData,
    environment: &Environment,
) -> Result<()> {
    environment.validate()?;
    let mut images = Vec::new();
    for (i, picture) in environment
        .faces
        .iter()
        .chain(environment.clouds.iter().map(|c| &c.image))
        .enumerate()
    {
        images.push(out.images.len());
        // A box without a bottom never draws or samples its sixth side.
        out.images.push(if i == 5 && !environment.bottom {
            SceneImage::white()
        } else {
            image(root, picture)?
        });
    }
    let faces = environment.faces.len();
    crate::environment_scene::append(out, environment, &images[..faces], &images[faces..])
}

/// Stop drawing `scene`'s sky box and clouds. Their pictures, which nothing
/// else draws, shrink to one pixel; the indices of everything else stay.
fn strip_sky(scene: &mut SceneData) {
    let sky = |kind: MaterialKind| matches!(kind, MaterialKind::Sky | MaterialKind::Cloud);
    let materials = &scene.materials;
    scene.batches.retain(|b| !sky(materials[b.material].kind));
    let mut drawn = vec![false; scene.images.len()];
    let mut sky_images = vec![false; scene.images.len()];
    for material in &scene.materials {
        let marks = if sky(material.kind) {
            &mut sky_images
        } else {
            &mut drawn
        };
        for &image in &material.images {
            if let Some(mark) = marks.get_mut(image) {
                *mark = true;
            }
        }
    }
    for (image, (sky, drawn)) in scene
        .images
        .iter_mut()
        .zip(sky_images.into_iter().zip(drawn))
    {
        if sky && !drawn && image.width * image.height > 1 {
            *image = SceneImage {
                label: image.label.clone(),
                ..SceneImage::white()
            };
        }
    }
}

/// `image` box-filtered to a [`THUMBNAIL`] square (stretched, as v21's
/// picker shows them).
fn thumbnail(image: &SceneImage) -> SceneImage {
    let mut rgba = Vec::with_capacity((THUMBNAIL * THUMBNAIL * 4) as usize);
    for y in 0..THUMBNAIL {
        let (y0, y1) = span(y, image.height);
        for x in 0..THUMBNAIL {
            let (x0, x1) = span(x, image.width);
            let mut sum = [0u64; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let at = ((sy * image.width + sx) * 4) as usize;
                    for (s, &c) in sum.iter_mut().zip(&image.rgba[at..at + 4]) {
                        *s += u64::from(c);
                    }
                }
            }
            let count = u64::from((y1 - y0) * (x1 - x0)).max(1);
            rgba.extend(sum.map(|s| (s / count) as u8));
        }
    }
    SceneImage {
        label: image.label.clone(),
        width: THUMBNAIL,
        height: THUMBNAIL,
        rgba,
        srgb: image.srgb,
    }
}
/// The source pixels thumbnail pixel `i` covers, at least one.
fn span(i: u32, size: u32) -> (u32, u32) {
    let start = (u64::from(i) * u64::from(size) / u64::from(THUMBNAIL)) as u32;
    let end = (u64::from(i + 1) * u64::from(size) / u64::from(THUMBNAIL)) as u32;
    (start.min(size - 1), end.max(start + 1).min(size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Material, MeshBatch};
    use serde_json::json;

    fn picture(dir: &Path, name: &str, color: [u8; 4], size: u32) -> Value {
        use sha2::{Digest, Sha256};
        let image = image::RgbaImage::from_pixel(size, size, image::Rgba(color));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        std::fs::write(dir.join(name), &bytes).unwrap();
        json!({
            "file": name,
            "source": name,
            "sha256": format!("{:x}", Sha256::digest(&bytes)),
            "width": size,
            "height": size,
        })
    }
    fn sky(dir: &Path, source: &str, color: [u8; 4], textures: bool) -> Value {
        let faces: Vec<Value> = (0..6)
            .map(|i| picture(dir, &format!("{}-{i}.png", color[0]), color, 4))
            .collect();
        json!({
            "schema_version": 2,
            "source_materials": source,
            "source_sha256": "0".repeat(64),
            "faces": faces,
            "reflection": null,
            "clouds": [],
            "textures": textures,
            "bottom": false,
            "horizon_band": true,
            "solid_color": [0.0, 0.0, 0.0],
            "fog": {"start": 100.0, "end": 400.0, "color": [0.5, 0.5, 0.5]},
            "warnings": [],
        })
    }
    /// Three maps, two sharing a sky (one drawing it in a plain colour) and
    /// two sharing a ground texture.
    fn bundle(dir: &Path) -> Choices {
        let grass = picture(dir, "grass.png", [0, 200, 0, 255], 8);
        let sand = picture(dir, "sand.png", [200, 200, 0, 255], 8);
        let bundle = json!({
            "environments": {
                "v20/a.mis": sky(dir, "base/data/skies/Sky_Blue2/resource.dml", [10, 20, 200, 255], true),
                "v20/b.mis": sky(dir, "base/data/skies/Sky_Blue2/resource.dml", [10, 20, 200, 255], false),
                "v20/c.mis": sky(dir, "Add-Ons/Map_Slate_Desert/cloudy02.dml", [200, 150, 90, 255], true),
            },
            "bindings": [
                {"asset": "a.ter", "source": "Add-Ons/Map_A/TTgrass01", "terrain_layer": 0, "texture": grass["file"]},
                {"asset": "b.ter", "source": "Add-Ons/Map_B/TTgrass01", "terrain_layer": 0, "texture": grass["file"]},
                {"asset": "b.ter", "source": "Add-Ons/Map_B/sand1", "terrain_layer": 1, "texture": sand["file"]},
                {"asset": "a.dif", "source": "Add-Ons/Map_A/carpet", "texture": sand["file"]},
            ],
        });
        Choices::from_bundle(dir.canonicalize().unwrap(), &bundle).unwrap()
    }

    #[test]
    fn every_stock_sky_and_terrain_texture_is_offered_once() {
        let dir = tempfile::tempdir().unwrap();
        let choices = bundle(dir.path());
        let skies: Vec<_> = choices
            .skies
            .iter()
            .map(|s| (s.id.as_str(), s.label.as_str()))
            .collect();
        assert_eq!(
            skies,
            [
                ("base/data/skies/sky_blue2/resource.dml", "Blue2"),
                ("add-ons/map_slate_desert/cloudy02.dml", "Slate Desert"),
                ("plain", "Plain"),
            ]
        );
        let grounds: Vec<_> = choices
            .grounds
            .iter()
            .map(|g| (g.id.as_str(), g.label.as_str()))
            .collect();
        assert_eq!(
            grounds,
            [
                ("add-ons/map_a/ttgrass01", "TTgrass01"),
                ("add-ons/map_b/sand1", "sand1")
            ]
        );
        // A map drawing its box in one colour has no sky of its own to show.
        assert_eq!(
            choices.map_sky("v20/a.mis").as_deref(),
            Some("base/data/skies/sky_blue2/resource.dml")
        );
        assert_eq!(choices.map_sky("v20/b.mis"), None);
        for sky in &choices.skies {
            let t = choices.sky_thumbnail(sky).unwrap();
            assert_eq!((t.width, t.height, t.rgba.len()), (64, 64, 64 * 64 * 4));
        }
        let t = choices.ground_thumbnail(&choices.grounds[1]).unwrap();
        assert_eq!(&t.rgba[..4], &[200, 200, 0, 255]);
        let picked = choices
            .sky("base/data/skies/sky_blue2/resource.dml")
            .unwrap();
        assert_eq!(
            &choices.sky_thumbnail(picked).unwrap().rgba[..4],
            &[10, 20, 200, 255]
        );
    }

    /// A map scene with one ordinary surface drawing image 0, then its sky.
    fn map(choices: &Choices) -> SceneData {
        let mut scene = SceneData::default();
        let wall = scene.images.len();
        scene.images.push(SceneImage {
            label: "wall".into(),
            width: 2,
            height: 2,
            rgba: vec![90; 16],
            srgb: true,
        });
        scene.materials.push(Material::surface("wall", wall, 0));
        scene
            .vertices
            .extend(
                [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]].map(|position| {
                    crate::scene::SceneVertex {
                        position,
                        normal: [0.0, 0.0, 1.0],
                        uv: [0.0; 2],
                        lightmap_uv: [0.0; 2],
                        color: [1.0; 4],
                        fx: [0.0; 4],
                    }
                }),
            );
        scene.indices.extend([0, 1, 2]);
        scene.batches.push(MeshBatch {
            indices: 0..3,
            material: 0,
            center: [0.0; 3],
        });
        append_sky(&choices.root, &mut scene, &choices.maps["v20/a.mis"]).unwrap();
        scene
    }
    fn drawn_sky_colors(scene: &SceneData) -> Vec<[u8; 4]> {
        scene
            .batches
            .iter()
            .map(|b| &scene.materials[b.material])
            .filter(|m| m.kind == MaterialKind::Sky && m.parameters.is_none())
            .map(|m| {
                let rgba = &scene.images[m.images[0]].rgba;
                [rgba[0], rgba[1], rgba[2], rgba[3]]
            })
            .collect()
    }

    #[test]
    fn a_picked_sky_replaces_the_maps_and_keeps_its_fog_and_surfaces() {
        let dir = tempfile::tempdir().unwrap();
        let choices = bundle(dir.path());
        let mut scene = map(&choices);
        let wall = scene.materials[0].images[0];
        let fog = scene.fog;
        assert_eq!(drawn_sky_colors(&scene), vec![[10, 20, 200, 255]; 5]);
        choices
            .set_sky(
                &mut scene,
                "v20/a.mis",
                Some("add-ons/map_slate_desert/cloudy02.dml"),
            )
            .unwrap();
        assert_eq!(drawn_sky_colors(&scene), vec![[200, 150, 90, 255]; 5]);
        assert_eq!((scene.fog.start, scene.fog.end), (fog.start, fog.end));
        // The wall still draws its picture; the old sky's are one pixel.
        assert_eq!(scene.batches[0].material, 0);
        assert_eq!(scene.images[wall].rgba, vec![90; 16]);
        assert!(
            scene
                .images
                .iter()
                .filter(|i| i.label.starts_with("10-"))
                .all(|i| i.width * i.height == 1)
        );
        // Plain: white sides for Sky Color to tint.
        choices
            .set_sky(&mut scene, "v20/a.mis", Some(PLAIN_SKY))
            .unwrap();
        assert_eq!(drawn_sky_colors(&scene), vec![[255; 4]; 6]);
        // Unknown, or none: the map's own again.
        for id in [Some("add-ons/map_nowhere/sky.dml"), None] {
            choices.set_sky(&mut scene, "v20/a.mis", id).unwrap();
            assert_eq!(drawn_sky_colors(&scene), vec![[10, 20, 200, 255]; 5]);
        }
        scene.validate().unwrap();
        assert_eq!(scene.batches.len(), 1 + 6 + 5);
    }

    #[test]
    fn sky_labels_name_the_folder_without_its_kind() {
        for (source, label) in [
            ("base/data/skies/Sky_Spooky1/resource.dml", "Spooky1"),
            ("base/data/skies/sunset/resource.dml", "Sunset"),
            ("Add-Ons/Map_Skylands/resource.dml", "Skylands"),
            (
                "Add-Ons/Map_Slate_Storm_Revised/storm.dml",
                "Slate Storm Revised",
            ),
            ("resource.dml", "Sky"),
        ] {
            assert_eq!(sky_label(source), label);
        }
        assert_eq!(
            ground_label("Add-Ons/Map_Slopes/TTicefrosty1"),
            "TTicefrosty1"
        );
        assert_eq!(ground_label("paper05.jpg"), "paper05");
    }
}
