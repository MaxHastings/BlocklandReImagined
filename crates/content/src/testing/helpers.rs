//! Generic made-up content builders: files, PNGs, scratch folders, box
//! meshes and rigid shapes with invented values.
use crate::shape::{Material, Mesh, Primitive};
use anyhow::{Context, Result};
use glam::Vec3;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The lowercase hex SHA-256 of `bytes`.
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Write `bytes` at `dir/relative` (making the folders) and return their
/// SHA-256.
pub fn write_file(dir: &Path, relative: &str, bytes: &[u8]) -> Result<String> {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().context("fixture file has no folder")?)?;
    std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok(sha256(bytes))
}

/// A PNG of `width` x `height` whose pixel (x, y) is `pixel(x, y)`.
pub fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Result<Vec<u8>> {
    let image = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba(pixel(x, y)));
    let mut bytes = Vec::new();
    image.write_to(
        &mut std::io::Cursor::new(&mut bytes),
        image::ImageFormat::Png,
    )?;
    Ok(bytes)
}

/// A fresh, empty folder under the system temporary folder, removed when
/// the handle drops.
pub struct ScratchDir(PathBuf);
impl ScratchDir {
    pub fn new(label: &str) -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "bri-{label}-{}-{nanos}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The materials of a [`cuboid`]: its front (-Z) face's, and the rest's.
#[derive(Clone, Copy, Debug)]
pub struct Paint {
    pub front: usize,
    pub rest: usize,
}
/// One material all round.
pub const fn plain(material: usize) -> Paint {
    Paint {
        front: material,
        rest: material,
    }
}

/// A box: 4 vertices per face with its own normal and uv, two
/// triangles per face. The -Z face (front) gets `paint.front`.
pub fn cuboid(centre: Vec3, half: Vec3, paint: Paint) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uv = Vec::new();
    let mut front = Vec::new();
    let mut rest = Vec::new();
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let mut n = Vec3::ZERO;
            n[axis] = sign;
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let base = positions.len() as u32;
            for (cu, cv) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut p = n;
                p[u] = cu;
                p[v] = cv;
                positions.push((centre + p * half).to_array());
                normals.push(n.to_array());
                uv.push([(cu + 1.0) * 0.5, (1.0 - cv) * 0.5]);
            }
            // Wind counter-clockwise seen from outside.
            let quad = if sign > 0.0 {
                [[base, base + 1, base + 2], [base, base + 2, base + 3]]
            } else {
                [[base, base + 2, base + 1], [base, base + 3, base + 2]]
            };
            if axis == 2 && sign < 0.0 {
                front.extend(quad);
            } else {
                rest.extend(quad);
            }
        }
    }
    let mut primitives = vec![Primitive {
        material: Some(paint.rest),
        triangles: rest,
    }];
    primitives.push(Primitive {
        material: Some(paint.front),
        triangles: front,
    });
    Mesh {
        frame_vertices: positions.len(),
        positions,
        normals,
        uv,
        primitives,
        skin: None,
        billboard: false,
        billboard_y: false,
    }
}

/// A plain shape material: `blend` is `"none"`, `"alpha"`, ...
pub fn material(name: &str, blend: &str) -> Material {
    Material {
        name: name.into(),
        wrap_u: true,
        wrap_v: true,
        blend: blend.into(),
        unlit: false,
        environment: false,
        mipmaps: true,
        detail_map: None,
        bump_map: None,
        reflectance_map: None,
        detail_scale: 1.0,
        reflectance: 0.0,
        metal: None,
    }
}

/// A box of a [`rigid_shape`]: node index, centre and half size in the
/// node's frame, paint.
pub type BoxPart = (usize, [f32; 3], [f32; 3], Paint);

/// A rigid model: nodes (name, parent index, translation), one box object
/// per part (node index, centre and half size in the node's frame, paint),
/// one visible detail, no animations.
pub fn rigid_shape(
    id: &str,
    nodes: &[(&str, Option<usize>, [f32; 3])],
    parts: &[BoxPart],
    materials: Vec<Material>,
) -> crate::shape::Shape {
    use crate::shape::{Detail, Node, Object, Shape};
    Shape {
        schema_version: 1,
        id: id.into(),
        nodes: nodes
            .iter()
            .map(|(name, parent, t)| Node {
                name: (*name).into(),
                parent: *parent,
                translation: *t,
                rotation: [0.0, 0.0, 0.0, 1.0],
            })
            .collect(),
        objects: parts
            .iter()
            .enumerate()
            .map(|(i, (node, ..))| Object {
                name: format!("part{i}"),
                node: Some(*node),
                meshes: vec![i],
                visibility: 1.0,
                frame: 0,
                material_frame: 0,
            })
            .collect(),
        details: vec![Detail {
            name: "detail1".into(),
            pixel_threshold: 1.0,
            object_start: 0,
            object_count: parts.len(),
            mesh_offset: 0,
            collision: false,
        }],
        meshes: parts
            .iter()
            .map(|(_, c, h, paint)| Some(cuboid(Vec3::from(*c), Vec3::from(*h), *paint)))
            .collect(),
        materials,
        animations: Vec::new(),
    }
}
