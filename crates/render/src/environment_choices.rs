//! The skies the Environment window offers over a map's own
//! (`bri_content::atmosphere::Settings::sky`): Steam's Sky Add-Ons from the
//! environment pack (each with the look its `.atmosphere` gives), then
//! every stock map's sky box Steam has no copy of. Without the pack, a
//! plain one stands in for Steam's White. Picking one changes only what is
//! drawn; the look is the settings', and collision stays.
use crate::{
    scene::{MaterialKind, SceneData, SceneImage},
    scene_loader::{decode, file, read},
};
use anyhow::{Context, Result, ensure};
use bri_content::{
    atmosphere::{PLAIN_SKY, Settings},
    environment::{Environment, Image},
    environment_pack::{Pack, Precipitation},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Thumbnails are this many pixels square.
pub const THUMBNAIL: u32 = 64;

/// A sky box to pick: a Steam sky, a stock map's, or the plain one.
#[derive(Clone, Debug)]
pub struct SkyChoice {
    /// Its name in the settings: the sky's material list in lower case.
    pub id: String,
    pub label: String,
    /// None for the plain sky.
    environment: Option<Environment>,
    /// Where its pictures are: the map bundle or the environment pack.
    root: PathBuf,
    /// A Steam sky's own picture for the picker.
    thumbnail: Option<Image>,
    /// What a Steam sky brings when picked (empty for the others).
    pub look: Settings,
    /// The rain or snow a Steam sky brings.
    pub precipitation: Option<Precipitation>,
}

/// What a map bundle and the environment pack offer, and each map's own sky.
#[derive(Clone, Debug)]
pub struct Choices {
    root: PathBuf,
    pub skies: Vec<SkyChoice>,
    /// Each map's own sky, by map ID.
    maps: BTreeMap<String, Environment>,
    /// The Steam skies, grounds and waters (empty without them), and where
    /// their pictures are.
    pub pack: Arc<Pack>,
    pack_root: Option<PathBuf>,
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

impl Choices {
    /// The skies of the environment pack at `pack` (when there is one),
    /// then those of the map bundle at `root` Steam lacks, in its maps'
    /// order, each once.
    pub fn load(root: &Path, pack: Option<&Path>) -> Result<Self> {
        let root = root.canonicalize().context("Opening native map bundle")?;
        let bundle: Value = serde_json::from_slice(&read(&root, "bundle.json")?)?;
        let mut choices = Self::from_bundle(root, &bundle)?;
        if let Some(dir) = pack {
            let dir = dir.canonicalize().context("Opening the environment pack")?;
            let pack = Pack::load(&dir)?;
            choices.add_pack(dir, pack);
        }
        Ok(choices)
    }
    /// Steam's skies first; a stock sky Steam has a copy of (the same first
    /// picture) is offered once, as Steam's, and its White stands for Plain.
    fn add_pack(&mut self, dir: PathBuf, pack: Pack) {
        if pack.skies.is_empty() {
            self.pack = Arc::new(pack);
            self.pack_root = Some(dir);
            return;
        }
        let steam: Vec<SkyChoice> = pack
            .skies
            .iter()
            .map(|s| SkyChoice {
                id: s.id.clone(),
                label: s.label.clone(),
                environment: Some(s.environment.clone()),
                root: dir.clone(),
                thumbnail: Some(s.thumbnail.clone()),
                look: s.look.clone(),
                precipitation: s.precipitation.clone(),
            })
            .collect();
        let first = |c: &SkyChoice| {
            c.environment
                .as_ref()
                .and_then(|e| e.faces.first())
                .map(|f| f.sha256.clone())
        };
        let copies: std::collections::BTreeSet<String> = steam.iter().filter_map(first).collect();
        let stock = std::mem::take(&mut self.skies)
            .into_iter()
            .filter(|c| c.environment.is_some() && first(c).is_none_or(|f| !copies.contains(&f)));
        self.skies = steam.into_iter().chain(stock).collect();
        self.pack = Arc::new(pack);
        self.pack_root = Some(dir);
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
                root: root.clone(),
                thumbnail: None,
                look: Settings::default(),
                precipitation: None,
            });
        }
        skies.push(SkyChoice {
            id: PLAIN_SKY.into(),
            label: "Plain".into(),
            environment: None,
            root: root.clone(),
            thumbnail: None,
            look: Settings::default(),
            precipitation: None,
        });
        Ok(Self {
            root,
            skies,
            maps,
            pack: Arc::new(Pack {
                schema_version: bri_content::environment_pack::SCHEMA,
                ..Default::default()
            }),
            pack_root: None,
        })
    }
    /// One of the environment pack's pictures (a ground's or water's
    /// texture, or a thumbnail), checked against its checksum.
    pub fn picture(&self, picture: &Image) -> Result<SceneImage> {
        image(
            self.pack_root.as_deref().context("No environment pack")?,
            picture,
        )
    }
    /// One of the pack's pictures at the picker's size.
    pub fn pack_thumbnail(&self, picture: &Image) -> Result<SceneImage> {
        Ok(thumbnail(&self.picture(picture)?))
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
    /// A small picture of a sky: the box's first side under its first
    /// cloud layer (a storm is mostly cloud), or plain white.
    pub fn sky_thumbnail(&self, choice: &SkyChoice) -> Result<SceneImage> {
        if let Some(picture) = &choice.thumbnail {
            return Ok(thumbnail(&image(&choice.root, picture)?));
        }
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
        let mut picture = thumbnail(&image(&choice.root, face)?);
        if let Some(cloud) = environment.clouds.first() {
            let cloud = thumbnail(&image(&choice.root, &cloud.image)?);
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
    /// Draw sky `id` in `scene` (map `map`'s) in place of the sky it has:
    /// None, or a sky this content lacks, gives the map its own back. The
    /// map's fog stays, and the box keeps the map's size. A Steam sky draws
    /// its floor unless `ground` (a picked ground that shows) hides it.
    pub fn set_sky(
        &self,
        scene: &mut SceneData,
        map: &str,
        id: Option<&str>,
        ground: bool,
    ) -> Result<()> {
        self.set_sky_for(scene, map, None, id, ground)
    }
    /// [`Self::set_sky`] for a map that may live in a bundle of its own (a
    /// map Add-On's, at `map_root`): its own sky comes from there when the
    /// base bundle does not have it.
    pub fn set_sky_for(
        &self,
        scene: &mut SceneData,
        map: &str,
        map_root: Option<&Path>,
        id: Option<&str>,
        ground: bool,
    ) -> Result<()> {
        let picked = id.and_then(|id| self.sky(id));
        let own = match self.maps.get(map) {
            Some(own) => Some((self.root.clone(), own.clone())),
            None => match map_root {
                Some(root) if root != self.root => own_sky(root, map)?,
                _ => None,
            },
        };
        strip_sky(scene);
        match (picked, own) {
            (Some(choice), _) => self.append_choice(scene, choice, ground),
            (None, Some((root, own))) => append_sky(&root, scene, &own),
            // A map without a sky keeps none unless one is picked.
            (None, None) => Ok(()),
        }
    }
    fn append_choice(&self, scene: &mut SceneData, choice: &SkyChoice, ground: bool) -> Result<()> {
        let mut environment = match &choice.environment {
            Some(environment) => environment.clone(),
            None => plain_sky(),
        };
        // Keep the map's fog (and so the size of its box).
        if scene.fog.end > 0.0 {
            environment.fog = scene.fog;
        }
        environment.warnings.clear();
        if choice.thumbnail.is_some() {
            environment.bottom = !ground;
        }
        if choice.environment.is_some() {
            append_sky(&choice.root, scene, &environment)
        } else {
            let white = scene.images.len();
            scene.images.push(SceneImage::white());
            crate::environment_scene::append(scene, &environment, &[white; 6], &[])
        }
    }
}

/// The sky a map bundle at `root` gives `map`, and where its pictures are.
fn own_sky(root: &Path, map: &str) -> Result<Option<(PathBuf, Environment)>> {
    let root = root.canonicalize().context("Opening the map's bundle")?;
    let bundle: Value = serde_json::from_slice(&read(&root, "bundle.json")?)?;
    bundle["environments"]
        .get(map)
        .map(|e| -> Result<_> { Ok((root.clone(), serde_json::from_value(e.clone())?)) })
        .transpose()
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
    /// Three maps, two sharing a sky (one drawing it in a plain colour).
    fn bundle(dir: &Path) -> Choices {
        let bundle = json!({
            "environments": {
                "v20/a.mis": sky(dir, "base/data/skies/Sky_Blue2/resource.dml", [10, 20, 200, 255], true),
                "v20/b.mis": sky(dir, "base/data/skies/Sky_Blue2/resource.dml", [10, 20, 200, 255], false),
                "v20/c.mis": sky(dir, "Add-Ons/Map_Slate_Desert/cloudy02.dml", [200, 150, 90, 255], true),
            },
        });
        Choices::from_bundle(dir.canonicalize().unwrap(), &bundle).unwrap()
    }

    #[test]
    fn every_stock_sky_is_offered_once() {
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

    /// A map Add-On's own sky lives in its own bundle: unpicking a sky
    /// draws that again, not nothing.
    #[test]
    fn a_map_add_on_gets_its_own_sky_back() {
        let dir = tempfile::tempdir().unwrap();
        let choices = bundle(dir.path());
        let addon = tempfile::tempdir().unwrap();
        let own = sky(addon.path(), "plain colour", [40, 160, 60, 255], true);
        std::fs::write(
            addon.path().join("bundle.json"),
            serde_json::to_vec(&json!({ "environments": { "pond:map/pond": own } })).unwrap(),
        )
        .unwrap();
        let mut scene = map(&choices);
        for id in [Some(PLAIN_SKY), None] {
            choices
                .set_sky_for(&mut scene, "pond:map/pond", Some(addon.path()), id, false)
                .unwrap();
        }
        assert_eq!(drawn_sky_colors(&scene), vec![[40, 160, 60, 255]; 5]);
        // Without its bundle it keeps none, as before.
        choices
            .set_sky(&mut scene, "pond:map/pond", None, false)
            .unwrap();
        assert!(drawn_sky_colors(&scene).is_empty());
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
                false,
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
            .set_sky(&mut scene, "v20/a.mis", Some(PLAIN_SKY), false)
            .unwrap();
        assert_eq!(drawn_sky_colors(&scene), vec![[255; 4]; 6]);
        // Unknown, or none: the map's own again.
        for id in [Some("add-ons/map_nowhere/sky.dml"), None] {
            choices.set_sky(&mut scene, "v20/a.mis", id, false).unwrap();
            assert_eq!(drawn_sky_colors(&scene), vec![[10, 20, 200, 255]; 5]);
        }
        scene.validate().unwrap();
        assert_eq!(scene.batches.len(), 1 + 6 + 5);
    }

    #[test]
    fn steam_skies_come_first_and_stand_for_their_stock_copies() {
        let dir = tempfile::tempdir().unwrap();
        let mut choices = bundle(dir.path());
        let pack_dir = dir.path().join("pack");
        std::fs::create_dir(&pack_dir).unwrap();
        // Steam's Blue2 is the stock one's pictures; its Storm is new.
        let blue: Environment = serde_json::from_value(sky(
            &pack_dir,
            "Add-Ons/Sky_Blue2/blue2.dml",
            [10, 20, 200, 255],
            true,
        ))
        .unwrap();
        let storm: Environment = serde_json::from_value(sky(
            &pack_dir,
            "Add-Ons/Sky_Slate_Storm/slate_storm.dml",
            [60, 60, 70, 255],
            true,
        ))
        .unwrap();
        let thumb: Image =
            serde_json::from_value(picture(&pack_dir, "thumb.png", [1, 2, 3, 255], 8)).unwrap();
        let steam =
            |id: &str, label: &str, environment: Environment| bri_content::environment_pack::Sky {
                id: id.into(),
                label: label.into(),
                thumbnail: thumb.clone(),
                environment,
                look: Settings {
                    fog_color: Some([0.3; 3]),
                    ..Default::default()
                },
                precipitation: None,
            };
        choices.add_pack(
            pack_dir.canonicalize().unwrap(),
            Pack {
                schema_version: bri_content::environment_pack::SCHEMA,
                source: Some("test".into()),
                skies: vec![
                    steam("add-ons/sky_blue2/blue2.dml", "Blue2", blue),
                    steam(
                        "add-ons/sky_slate_storm/slate_storm.dml",
                        "Slate Storm",
                        storm,
                    ),
                ],
                ..Default::default()
            },
        );
        let labels: Vec<_> = choices.skies.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["Blue2", "Slate Storm", "Slate Desert"]);
        assert_eq!(choices.skies[1].look.fog_color, Some([0.3; 3]));
        // Its own picture from the pack.
        let t = choices.sky_thumbnail(&choices.skies[0]).unwrap();
        assert_eq!(&t.rgba[..4], &[1, 2, 3, 255]);
        // Drawn from the pack, with a floor unless a ground hides it.
        let mut scene = map(&choices);
        let storm = "add-ons/sky_slate_storm/slate_storm.dml";
        choices
            .set_sky(&mut scene, "v20/a.mis", Some(storm), false)
            .unwrap();
        assert_eq!(drawn_sky_colors(&scene), vec![[60, 60, 70, 255]; 6]);
        choices
            .set_sky(&mut scene, "v20/a.mis", Some(storm), true)
            .unwrap();
        assert_eq!(drawn_sky_colors(&scene), vec![[60, 60, 70, 255]; 5]);
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
    }
}
