//! Native brick overlays/print resources. Image alpha is pigment coverage.
use anyhow::{Context, Result, ensure};
use bri_content::brick_materials::{Bundle, Image, SURFACES};
use bri_render::scene::{Material, MaterialKind, SceneData, SceneImage};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};
pub struct BrickMaterials {
    pub bundle: Bundle,
    images: BTreeMap<String, SceneImage>,
}
impl BrickMaterials {
    pub fn load(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let bytes = read_resource(&root, "brick-materials.json", 8 * 1024 * 1024)?;
        let bundle: Bundle = serde_json::from_slice(&bytes)?;
        bundle.validate()?;
        ensure!(
            bundle
                .images()
                .map(|image| u64::from(image.width) * u64::from(image.height) * 4)
                .sum::<u64>()
                <= 512 * 1024 * 1024,
            "Decoded brick materials exceed 512 MiB budget"
        );
        let mut images = BTreeMap::new();
        for entry in bundle.images() {
            images.insert(entry.path.clone(), load_image(&root, entry)?);
        }
        Ok(Self { bundle, images })
    }
    /// Plain 1x1 surfaces and one `Letters/A` print, held in memory, for
    /// tests of textured brick scenes.
    #[cfg(test)]
    pub(crate) fn in_memory() -> Self {
        use bri_content::brick_materials::{Package, Print, Source};
        let image = |name: &str| Image {
            path: format!("{name}.png"),
            width: 1,
            height: 1,
            sha256: "0".repeat(64),
            source: Source {
                path: format!("{name}.png"),
                archive: None,
                sha256: "0".repeat(64),
            },
        };
        let bundle = Bundle {
            schema_version: 1,
            surfaces: SURFACES.into_iter().map(|s| (s.into(), image(s))).collect(),
            prints: vec![Print {
                id: "print/print_letters_default/a".into(),
                name: "A".into(),
                aspect: "Letters".into(),
                package: "Print_Letters_Default".into(),
                aliases: vec!["Letters/A".into()],
                diffuse: image("letter-a"),
                icon: image("icon-a"),
            }],
            packages: vec![Package {
                name: "Print_Letters_Default".into(),
                archive: "Add-Ons/Print_Letters_Default.zip".into(),
                archive_sha256: "a".repeat(64),
                default_list_line: 1,
            }],
            evidence: vec![],
            excluded_installed_packages: vec![],
            warnings: vec![],
        };
        let images = bundle
            .images()
            .map(|entry| {
                (
                    entry.path.clone(),
                    SceneImage {
                        label: entry.path.clone(),
                        width: 1,
                        height: 1,
                        rgba: vec![255; 4],
                        srgb: false,
                    },
                )
            })
            .collect();
        Self { bundle, images }
    }
    /// One material for every brick surface (`MaterialKind::BrickSurfaces`),
    /// so a chunk's bricks draw in one batch, not one per surface image.
    /// Slots follow `SURFACES` (the shader clamps brickSIDE, v20
    /// fxBrickBatcher's only `GL_CLAMP`, nearest-magnified surface,
    /// 0x531f94); slot 5 is white: a print-less surface is still painted,
    /// not an arbitrary letter. The server assigns original default
    /// Letters/A when appropriate.
    pub fn surface_materials(&self, scene: &mut SceneData) -> [usize; 6] {
        const NAME: &str = "native-brick-surfaces";
        if let Some(index) = scene.materials.iter().position(|m| m.name == NAME) {
            return [index; 6];
        }
        let white = scene
            .images
            .iter()
            .position(|i| i.width == 1 && i.height == 1 && i.rgba == [255; 4])
            .unwrap_or_else(|| {
                scene.images.push(SceneImage::white());
                scene.images.len() - 1
            });
        let mut material = Material::vertex_lit(NAME, white);
        material.kind = MaterialKind::BrickSurfaces;
        material.images = [white; 13];
        for (slot, name) in SURFACES.iter().enumerate() {
            material.images[slot] = self.image(scene, &self.bundle.surfaces[*name]);
        }
        scene.materials.push(material);
        [scene.materials.len() - 1; 6]
    }
    pub fn print_material(&self, scene: &mut SceneData, id: &str) -> Result<usize> {
        let print = self
            .bundle
            .resolve(id)
            .with_context(|| format!("Unresolved native print {id}"))?;
        Ok(self.append(scene, &print.diffuse))
    }
    /// The scene's copy of a native image, added once.
    fn image(&self, scene: &mut SceneData, image: &Image) -> usize {
        let native = &self.images[&image.path];
        if let Some(index) = scene
            .images
            .iter()
            .position(|i| i.label == native.label && i.rgba == native.rgba)
        {
            return index;
        }
        scene.images.push(native.clone());
        scene.images.len() - 1
    }
    fn append(&self, scene: &mut SceneData, image: &Image) -> usize {
        let material_name = format!("native-overlay/{}", image.path);
        if let Some(index) = scene.materials.iter().position(|m| m.name == material_name) {
            return index;
        }
        let image_index = scene.images.len();
        scene.images.push(self.images[&image.path].clone());
        let index = scene.materials.len();
        scene
            .materials
            .push(Material::brick_overlay(material_name, image_index));
        index
    }
}
fn load_image(root: &Path, entry: &Image) -> Result<SceneImage> {
    let bytes = read_resource(root, &entry.path, 32 * 1024 * 1024)?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == entry.sha256,
        "Native brick image checksum mismatch: {}",
        entry.path
    );
    let dimensions =
        image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Png)
            .into_dimensions()?;
    ensure!(
        dimensions == (entry.width, entry.height),
        "Native brick image dimension mismatch"
    );
    let pixels = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)?.into_rgba8();
    Ok(SceneImage {
        label: entry.path.clone(),
        width: entry.width,
        height: entry.height,
        rgba: pixels.into_raw(),
        srgb: false,
    })
}
pub(crate) fn read_resource(root: &Path, relative: &str, limit: u64) -> Result<Vec<u8>> {
    ensure!(
        bri_content::brick_materials::safe_relative(relative),
        "Unsafe native brick resource path"
    );
    let path = root
        .join(relative)
        .canonicalize()
        .with_context(|| format!("Missing native brick resource {relative}"))?;
    // Compared in canonical form on both sides: on Windows a canonical path
    // carries the `\\?\` prefix a caller's root may not.
    let root = root
        .canonicalize()
        .with_context(|| format!("Missing native package {}", root.display()))?;
    ensure!(
        path.starts_with(&root),
        "Brick resource escapes native package: {relative}"
    );
    let file = std::fs::File::open(&path)?;
    ensure!(
        file.metadata()?.len() <= limit,
        "Oversized native brick resource {relative}: limit {limit} bytes"
    );
    read_limited(file, limit).with_context(|| format!("Reading native brick resource {relative}"))
}
fn read_limited(reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Native brick resource exceeds byte limit {limit}"
    );
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_content::{
        brick::{Brick, Face, Quad, Surface, Vertex},
        brick_materials::{Package, Print, Source},
    };
    use bri_render::scene::{AlphaMode, MaterialKind};
    use std::{io::Cursor, path::PathBuf};

    struct Fixture {
        root: PathBuf,
        bundle: Bundle,
        pixels: Vec<u8>,
    }
    impl Fixture {
        fn new() -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "bri-material-loader-{}-{stamp}",
                std::process::id()
            ));
            std::fs::create_dir(&root).unwrap();
            let pixels = vec![17, 31, 63, 46, 200, 150, 100, 0];
            let png = image::RgbaImage::from_raw(2, 1, pixels.clone()).unwrap();
            let mut encoded = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(png)
                .write_to(&mut encoded, image::ImageFormat::Png)
                .unwrap();
            let bytes = encoded.into_inner();
            let sha256 = format!("{:x}", Sha256::digest(&bytes));
            let image = |name: &str| Image {
                path: format!("{name}.png"),
                width: 2,
                height: 1,
                sha256: sha256.clone(),
                source: Source {
                    path: format!("{name}.png"),
                    archive: None,
                    sha256: sha256.clone(),
                },
            };
            let bundle = Bundle {
                schema_version: 1,
                surfaces: SURFACES.into_iter().map(|s| (s.into(), image(s))).collect(),
                prints: vec![Print {
                    id: "print/print_letters_default/a".into(),
                    name: "A".into(),
                    aspect: "Letters".into(),
                    package: "Print_Letters_Default".into(),
                    aliases: vec!["Letters/A".into()],
                    diffuse: image("letter-a"),
                    icon: image("icon-a"),
                }],
                packages: vec![Package {
                    name: "Print_Letters_Default".into(),
                    archive: "Add-Ons/Print_Letters_Default.zip".into(),
                    archive_sha256: "a".repeat(64),
                    default_list_line: 1,
                }],
                evidence: vec![],
                excluded_installed_packages: vec![],
                warnings: vec![],
            };
            for image in bundle.images() {
                std::fs::write(root.join(&image.path), &bytes).unwrap();
            }
            let fixture = Self {
                root,
                bundle,
                pixels,
            };
            fixture.save();
            fixture
        }
        fn save(&self) {
            std::fs::write(
                self.root.join("brick-materials.json"),
                serde_json::to_vec(&self.bundle).unwrap(),
            )
            .unwrap();
        }
        fn error(&self) -> String {
            match BrickMaterials::load(&self.root) {
                Ok(_) => panic!("invalid fixture accepted"),
                Err(error) => format!("{error:#}"),
            }
        }
    }

    #[test]
    fn aliases_deduplicate_and_blank_print_surface_preserves_paint() {
        let fixture = Fixture::new();
        let loaded = BrickMaterials::load(&fixture.root).unwrap();
        assert_eq!(loaded.images.len(), 7);
        for image in loaded.images.values() {
            assert_eq!(image.rgba, fixture.pixels);
            assert!(!image.srgb);
            assert_eq!((image.width, image.height), (2, 1));
        }
        let mut scene = SceneData::default();
        let slots = loaded.surface_materials(&mut scene);
        let index = loaded.print_material(&mut scene, "Letters/A").unwrap();
        assert_eq!(
            index,
            loaded.print_material(&mut scene, "letters/a").unwrap()
        );
        assert_eq!(
            index,
            loaded
                .print_material(&mut scene, "print/print_letters_default/a")
                .unwrap()
        );
        // White, the five surfaces and the print; one surfaces material
        // and the print's.
        assert_eq!((scene.images.len(), scene.materials.len()), (7, 2));
        assert_eq!(scene.materials[index].kind, MaterialKind::BrickOverlay);
        assert!(
            loaded
                .print_material(&mut scene, "Letters/Missing")
                .is_err()
        );
        assert_eq!((scene.images.len(), scene.materials.len()), (7, 2));
        assert!(slots.iter().all(|s| *s == slots[0]));
        assert_eq!(scene.materials[slots[5]].kind, MaterialKind::BrickSurfaces);
        assert_eq!(
            scene.images[scene.materials[slots[5]].images[5]].rgba,
            vec![255; 4]
        );
        assert_eq!(loaded.surface_materials(&mut scene), slots);
        let mesh = Brick {
            schema_version: 1,
            id: "print-face".into(),
            footprint_studs: [1, 1],
            height_plates: 1,
            attachment_rows: vec!["b".into()],
            collision_boxes: vec![],
            needs_external_collision: false,
            coverage: None,
            quads: vec![Quad {
                face: Face::Omni,
                surface: Surface::Print,
                vertices: [[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]].map(
                    |position| Vertex {
                        position,
                        normal: [0., 0., 1.],
                        uv: [0., 0.],
                    },
                ),
                colors: None,
            }],
        };
        let paint = [0.2, 0.4, 0.6, 0.5];
        scene
            .append_brick(&mesh, glam::Mat4::IDENTITY.to_cols_array(), paint, slots)
            .unwrap();
        assert!(scene.vertices.iter().all(|v| v.color == paint));
        // The print face names the white slot.
        assert!(scene.vertices.iter().all(|v| v.lightmap_uv == [5., 0.]));
        let actual = &scene.materials[scene.batches[0].material];
        assert_eq!(actual.kind, MaterialKind::BrickSurfaces);
        assert_eq!(actual.alpha, AlphaMode::Blend);
        assert_eq!(actual.images[5], 0);
    }
    #[test]
    fn rejects_changed_bytes_wrong_dimensions_and_truncated_png() {
        let mut fixture = Fixture::new();
        std::fs::write(fixture.root.join("top.png"), b"changed").unwrap();
        assert!(fixture.error().contains("checksum mismatch"));
        let other = Fixture::new();
        let original = std::fs::read(other.root.join("top.png")).unwrap();
        std::fs::write(fixture.root.join("top.png"), &original).unwrap();
        fixture.bundle.surfaces.get_mut("top").unwrap().width = 3;
        fixture.save();
        assert!(fixture.error().contains("dimension mismatch"));
        let image = fixture.bundle.surfaces.get_mut("top").unwrap();
        image.width = 2;
        let truncated = &original[..original.len() / 2];
        image.sha256 = format!("{:x}", Sha256::digest(truncated));
        image.source.sha256 = image.sha256.clone();
        fixture.save();
        std::fs::write(fixture.root.join("top.png"), truncated).unwrap();
        assert!(!fixture.error().contains("checksum mismatch"));
    }
    #[test]
    fn rejects_traversal_and_aggregate_decoded_budget_before_image_reads() {
        let mut fixture = Fixture::new();
        fixture.bundle.prints[0].icon.path = "../outside.png".into();
        fixture.save();
        assert!(fixture.error().contains("output path"));
        let mut fixture = Fixture::new();
        let mut extra = fixture.bundle.prints[0].clone();
        extra.id.push('b');
        extra.name = "B".into();
        extra.aliases = vec!["Letters/B".into()];
        extra.diffuse.path = "letter-b.png".into();
        extra.icon.path = "icon-b.png".into();
        fixture.bundle.prints.push(extra);
        for image in fixture.bundle.surfaces.values_mut() {
            image.width = 4096;
            image.height = 4096;
        }
        for print in &mut fixture.bundle.prints {
            for image in [&mut print.diffuse, &mut print.icon] {
                image.width = 4096;
                image.height = 4096;
            }
        }
        fixture.save();
        assert!(fixture.error().contains("512 MiB budget"));
    }
    #[test]
    fn bounded_reads_stop_growth_and_oversized_files_before_decode() {
        let mut reader = Cursor::new(vec![0; 100]);
        assert!(read_limited(&mut reader, 7).is_err());
        assert_eq!(reader.position(), 8);
        assert_eq!(
            read_limited(Cursor::new(vec![1; 7]), 7).unwrap(),
            vec![1; 7]
        );
        let fixture = Fixture::new();
        std::fs::File::create(fixture.root.join("top.png"))
            .unwrap()
            .set_len(32 * 1024 * 1024 + 1)
            .unwrap();
        assert!(
            fixture
                .error()
                .contains("Oversized native brick resource top.png")
        );
        let fixture = Fixture::new();
        std::fs::File::create(fixture.root.join("brick-materials.json"))
            .unwrap()
            .set_len(8 * 1024 * 1024 + 1)
            .unwrap();
        assert!(
            fixture
                .error()
                .contains("Oversized native brick resource brick-materials.json")
        );
    }
    #[test]
    fn canonical_containment_rejects_directory_alias_escape() {
        let mut fixture = Fixture::new();
        let outside = Fixture::new();
        let link = fixture.root.join("outside");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let output = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(&outside.root)
                .creation_flags(0x08000000)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "Cannot create isolated test junction: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside.root, &link).unwrap();
        fixture.bundle.surfaces.get_mut("top").unwrap().path = "outside/top.png".into();
        fixture.save();
        assert!(fixture.error().contains("escapes native package"));
        // The same gate protects manifest reads through aliases, without needing
        // a privileged Windows file symlink for this regression test.
        let root = fixture.root.canonicalize().unwrap();
        assert!(
            read_resource(&root, "outside/brick-materials.json", 8 * 1024 * 1024)
                .unwrap_err()
                .to_string()
                .contains("escapes native package")
        );
    }
}
