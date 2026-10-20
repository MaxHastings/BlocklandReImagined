//! Decals ([`bri_content::decals`]): the triangles a decal's volume touches are found when it is added, and drawn again every frame with
//! the picture projected on them ([`crate::surface_projections`]). A map lists its decals up front; a game adds, moves and removes
//! them as it runs ([`DecalEdit`]).
use crate::projected_textures::ProjectionBlend;
use crate::scene::{AlphaMode, MaterialKind, SceneData, SceneImage};
use crate::surface_projections::{
    ProjectedSurfaces, SurfaceBatch, SurfaceImage, SurfaceProjection, SurfaceVertex,
};
use bri_content::decals::{Blend, Decal, Receivers};
use glam::{Mat4, Vec3};
use std::collections::{BTreeMap, HashMap};

/// The depth bias a decal's triangles are drawn with, in the renderer's depth units.
pub const DEPTH_BIAS: i32 = 8;

/// Keys at or above this are the map's own decals (their index added to it); a game's start below.
pub const MAP_KEY_BASE: u64 = 1 << 62;

/// One change to the decals on screen.
#[derive(Clone, Debug, PartialEq)]
pub enum DecalEdit {
    /// A decal appeared; its receivers are found now.
    Add {
        key: u64,
        decal: Decal,
    },
    /// The decal moved or turned: its picture mapping and direction change, its receivers stay.
    Move {
        key: u64,
        world_to_texture: [f32; 16],
        forward: [f32; 3],
    },
    Remove {
        key: u64,
    },
}

const OPAQUE_SURFACE: u8 = 1;
const OPAQUE_GROUND: u8 = 2;

/// The scene's triangles in a grid of cells, so a decal finds the few it touches without reading them all.
pub struct ReceiverIndex {
    cell: f32,
    cells: HashMap<u64, Vec<u32>>,
    /// Triangles too large for the grid, tested by box.
    big: Vec<u32>,
    /// Per triangle: 0 takes no decal (translucent), 1 flat surface, 2 ground.
    class: Vec<u8>,
}

fn pack(x: i32, y: i32, z: i32) -> u64 {
    (u64::from(x as u32 & 0x1f_ffff))
        | (u64::from(y as u32 & 0x1f_ffff) << 21)
        | (u64::from(z as u32 & 0x1f_ffff) << 42)
}

impl ReceiverIndex {
    /// Cell edge in scene units, and the most cells a triangle may span before it is kept apart.
    const CELL: f32 = 8.0;
    const MAX_SPAN: i32 = 64;

    pub fn build(scene: &SceneData) -> Self {
        let triangles = scene.indices.len() / 3;
        let mut index = Self {
            cell: Self::CELL,
            cells: HashMap::new(),
            big: Vec::new(),
            class: vec![0; triangles],
        };
        for batch in &scene.batches {
            let Some(material) = scene.materials.get(batch.material) else {
                continue;
            };
            if !matches!(material.alpha, AlphaMode::Opaque | AlphaMode::Mask(_)) {
                continue;
            }
            let class = if matches!(
                material.kind,
                MaterialKind::Terrain | MaterialKind::LayerBlend
            ) {
                OPAQUE_GROUND
            } else {
                OPAQUE_SURFACE
            };
            for t in (batch.indices.start / 3)..(batch.indices.end / 3) {
                let t = t as usize;
                index.class[t] = class;
                let p = [0, 1, 2].map(|k| {
                    Vec3::from(scene.vertices[scene.indices[t * 3 + k] as usize].position)
                });
                let (lo, hi) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
                let (a, b) = (index.coords(lo), index.coords(hi));
                let span = (b.0 - a.0 + 1) * (b.1 - a.1 + 1) * (b.2 - a.2 + 1);
                if span > Self::MAX_SPAN {
                    index.big.push(t as u32);
                    continue;
                }
                for x in a.0..=b.0 {
                    for y in a.1..=b.1 {
                        for z in a.2..=b.2 {
                            index.cells.entry(pack(x, y, z)).or_default().push(t as u32);
                        }
                    }
                }
            }
        }
        index
    }

    fn coords(&self, p: Vec3) -> (i32, i32, i32) {
        (
            (p.x / self.cell).floor() as i32,
            (p.y / self.cell).floor() as i32,
            (p.z / self.cell).floor() as i32,
        )
    }

    /// The triangles whose cells the box touches, plus every large one, each once and in order.
    pub fn candidates(&self, scene: &SceneData, lo: Vec3, hi: Vec3) -> Vec<u32> {
        let (a, b) = (self.coords(lo), self.coords(hi));
        let mut out = Vec::new();
        let span = i64::from(b.0 - a.0 + 1) * i64::from(b.1 - a.1 + 1) * i64::from(b.2 - a.2 + 1);
        if span > 1 << 20 {
            out.extend((0..self.class.len() as u32).filter(|t| self.class[*t as usize] != 0));
            return out;
        }
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                for z in a.2..=b.2 {
                    if let Some(list) = self.cells.get(&pack(x, y, z)) {
                        out.extend_from_slice(list);
                    }
                }
            }
        }
        for &t in &self.big {
            let p = [0, 1, 2].map(|k| {
                Vec3::from(scene.vertices[scene.indices[t as usize * 3 + k] as usize].position)
            });
            let (tl, th) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
            if tl.cmple(hi).all() && th.cmpge(lo).all() {
                out.push(t);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// The triangles of `scene` a decal draws on, three vertices each. Translucent surfaces take none; ground is the terrain materials.
/// Reads every triangle; [`gather_indexed`] reads the few near the decal.
pub fn gather(scene: &SceneData, decal: &Decal) -> (Vec<SurfaceVertex>, Vec<u32>) {
    let index = ReceiverIndex::build(scene);
    let all: Vec<u32> = (0..index.class.len() as u32).collect();
    gather_from(scene, &index, decal, &all)
}

/// As [`gather`], using the decal's `bounds` and the index when it has them.
pub fn gather_indexed(
    scene: &SceneData,
    index: &ReceiverIndex,
    decal: &Decal,
) -> (Vec<SurfaceVertex>, Vec<u32>) {
    let candidates = match decal.bounds {
        Some([lo, hi]) => index.candidates(scene, Vec3::from(lo), Vec3::from(hi)),
        None => (0..index.class.len() as u32).collect(),
    };
    gather_from(scene, index, decal, &candidates)
}

fn gather_from(
    scene: &SceneData,
    index: &ReceiverIndex,
    decal: &Decal,
    candidates: &[u32],
) -> (Vec<SurfaceVertex>, Vec<u32>) {
    let forward = Vec3::from(decal.forward);
    let planes: Vec<(Vec3, f32)> = decal
        .planes
        .iter()
        .map(|p| (Vec3::new(p[0], p[1], p[2]), p[3]))
        .collect();
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for &t in candidates {
        let t = t as usize;
        let rule: &Receivers = match index.class[t] {
            OPAQUE_SURFACE => &decal.surfaces,
            OPAQUE_GROUND => &decal.ground,
            _ => continue,
        };
        if !rule.enabled {
            continue;
        }
        let v = [0, 1, 2].map(|k| scene.vertices[scene.indices[t * 3 + k] as usize]);
        let p = v.map(|v| Vec3::from(v.position));
        let outside = p.map(|p| {
            planes.iter().enumerate().fold(0u8, |bits, (k, (n, w))| {
                bits | (u8::from(n.dot(p) - w < 0.0) << k)
            })
        });
        if outside[0] & outside[1] & outside[2] != 0 {
            continue;
        }
        let face = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        if face.dot(forward) >= rule.facing_limit {
            continue;
        }
        let base = vertices.len() as u32;
        for (k, v) in v.iter().enumerate() {
            let fade = if rule.fade_by_facing {
                (-Vec3::from(v.normal).dot(forward)).max(0.0)
            } else {
                1.0
            };
            vertices.push(SurfaceVertex {
                position: p[k].to_array(),
                normal: v.normal,
                uv: v.uv,
                fade,
            });
        }
        indices.extend([base, base + 1, base + 2]);
    }
    (vertices, indices)
}

/// A map's decals as loaded: the set, its pictures in order and the triangles each decal draws on.
pub struct LoadedDecals {
    pub set: bri_content::decals::DecalSet,
    pub pictures: Vec<SceneImage>,
    /// The set's named pictures (`set.pictures`), decoded in order.
    pub named: Vec<SceneImage>,
    pub receivers: Vec<(Vec<SurfaceVertex>, Vec<u32>)>,
}

/// `matrix` with the picture turned by `degrees` about its centre (0.5, 0.5). Perspective matrices output `(s q, t q, q)`, so the centre
/// offset scales with the third component; orthographic ones with the fourth.
pub fn spun(matrix: Mat4, perspective: bool, degrees: f32) -> Mat4 {
    if degrees == 0.0 {
        return matrix;
    }
    let (sn, c) = degrees.to_radians().sin_cos();
    let rows = matrix.transpose().to_cols_array_2d();
    let (s, t) = (glam::Vec4::from(rows[0]), glam::Vec4::from(rows[1]));
    let offset = if perspective {
        glam::Vec4::from(rows[2])
    } else {
        glam::Vec4::from(rows[3])
    };
    let s2 = s * c - t * sn + offset * (0.5 * (1.0 - c + sn));
    let t2 = s * sn + t * c + offset * (0.5 * (1.0 - sn - c));
    Mat4::from_cols(s2, t2, glam::Vec4::from(rows[2]), glam::Vec4::from(rows[3])).transpose()
}

/// What a game has asked of the decal layer since the last frame: pictures to hold and edits to apply.
#[derive(Default)]
pub struct DecalFeed {
    pub pictures: Vec<(String, SceneImage)>,
    pub edits: Vec<DecalEdit>,
}

impl DecalFeed {
    pub fn add_picture(&mut self, name: impl Into<String>, image: SceneImage) {
        self.pictures.push((name.into(), image));
    }
    pub fn submit(&mut self, edits: impl IntoIterator<Item = DecalEdit>) {
        self.edits.extend(edits);
    }
    pub fn is_empty(&self) -> bool {
        self.pictures.is_empty() && self.edits.is_empty()
    }
}

struct Placed {
    decal: Decal,
    image: SurfaceImage,
    vertices: Vec<SurfaceVertex>,
    indices: Vec<u32>,
}

/// The decals on screen: their pictures, receiving triangles and the surface pass that draws them.
pub struct DecalLayer {
    surfaces: ProjectedSurfaces,
    pictures: HashMap<String, SurfaceImage>,
    placed: BTreeMap<u64, Placed>,
    /// Every decal's vertices end to end, rebuilt when the set changes.
    vertices: Vec<SurfaceVertex>,
    indices: Vec<Vec<u32>>,
    keys: Vec<u64>,
    dirty: bool,
}

impl DecalLayer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        samples: u32,
    ) -> Self {
        Self {
            surfaces: ProjectedSurfaces::new(device, queue, target, depth, samples),
            pictures: HashMap::new(),
            placed: BTreeMap::new(),
            vertices: Vec::new(),
            indices: Vec::new(),
            keys: Vec::new(),
            dirty: false,
        }
    }

    /// A picture decals name (`Decal::picture`). sRGB, straight alpha.
    pub fn add_picture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        name: &str,
        image: &SceneImage,
    ) {
        let id =
            self.surfaces
                .add_image(device, queue, &image.rgba, image.width, image.height, true);
        self.pictures.insert(name.into(), id);
    }

    pub fn has_picture(&self, name: &str) -> bool {
        self.pictures.contains_key(name)
    }

    /// The map's own decals, keyed from [`MAP_KEY_BASE`].
    pub fn load(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, loaded: &LoadedDecals) {
        for (decal, picture) in loaded.set.decals.iter().zip(&loaded.pictures) {
            if !self.has_picture(&decal.picture) {
                self.add_picture(device, queue, &decal.picture, picture);
            }
        }
        for (picture, image) in loaded.set.pictures.iter().zip(&loaded.named) {
            if !self.has_picture(&picture.file) {
                self.add_picture(device, queue, &picture.file, image);
            }
        }
        for (i, (decal, (vertices, indices))) in
            loaded.set.decals.iter().zip(&loaded.receivers).enumerate()
        {
            if let Some(&image) = self.pictures.get(&decal.picture) {
                self.placed.insert(
                    MAP_KEY_BASE + i as u64,
                    Placed {
                        decal: decal.clone(),
                        image,
                        vertices: vertices.clone(),
                        indices: indices.clone(),
                    },
                );
            }
        }
        self.dirty = true;
    }

    /// Apply a game's edits. A decal whose picture the layer does not hold is skipped and named in the result.
    pub fn apply(
        &mut self,
        edits: Vec<DecalEdit>,
        scene: &SceneData,
        index: &ReceiverIndex,
    ) -> Vec<String> {
        let mut missing = Vec::new();
        for edit in edits {
            match edit {
                DecalEdit::Add { key, decal } => {
                    let Some(&image) = self.pictures.get(&decal.picture) else {
                        missing.push(format!("decal picture {} is not loaded", decal.picture));
                        continue;
                    };
                    let (vertices, indices) = gather_indexed(scene, index, &decal);
                    self.placed.insert(
                        key,
                        Placed {
                            decal,
                            image,
                            vertices,
                            indices,
                        },
                    );
                    self.dirty = true;
                }
                DecalEdit::Move {
                    key,
                    world_to_texture,
                    forward,
                } => {
                    if let Some(p) = self.placed.get_mut(&key) {
                        p.decal.world_to_texture = world_to_texture;
                        p.decal.forward = forward;
                    }
                }
                DecalEdit::Remove { key } => {
                    self.dirty |= self.placed.remove(&key).is_some();
                }
            }
        }
        missing
    }

    /// Triangles each decal draws on, for the log.
    pub fn triangle_counts(&self) -> Vec<usize> {
        self.placed.values().map(|p| p.indices.len() / 3).collect()
    }

    pub fn len(&self) -> usize {
        self.placed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.placed.is_empty()
    }

    fn rebuild(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.keys.clear();
        for (key, p) in &self.placed {
            let base = self.vertices.len() as u32;
            self.vertices.extend_from_slice(&p.vertices);
            self.indices
                .push(p.indices.iter().map(|i| i + base).collect());
            self.keys.push(*key);
        }
        self.dirty = false;
    }

    /// Upload the frame's camera and the decals' mappings.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        eye: Vec3,
        seconds: f32,
    ) {
        if self.dirty {
            self.rebuild();
        }
        let projections: Vec<SurfaceProjection> = self
            .keys
            .iter()
            .map(|key| {
                let p = &self.placed[key];
                SurfaceProjection {
                    image: p.image,
                    world_to_texture: spun(
                        Mat4::from_cols_array(&p.decal.world_to_texture),
                        p.decal.perspective,
                        p.decal.spin_degrees_per_second * seconds,
                    ),
                    divide: p.decal.perspective,
                    blend: match p.decal.blend {
                        Blend::Modulate2x => ProjectionBlend::Modulate2X,
                        Blend::Alpha => ProjectionBlend::Alpha,
                        Blend::Add => ProjectionBlend::Add,
                    },
                    wrap: p.decal.wrap,
                    gradient: None,
                    mask: None,
                    base: None,
                    reflection: None,
                    normal_fade: None,
                    vertex_fade: true,
                    two_sided: p.decal.two_sided,
                    depth_bias: DEPTH_BIAS,
                    fog: None,
                }
            })
            .collect();
        let batches: Vec<SurfaceBatch<'_>> = self
            .indices
            .iter()
            .zip(&projections)
            .map(|(indices, projection)| SurfaceBatch {
                indices,
                projection: *projection,
            })
            .collect();
        self.surfaces.prepare(
            device,
            queue,
            view_projection,
            eye,
            &self.vertices,
            &batches,
        );
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.surfaces.render(pass);
    }
}
