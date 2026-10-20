//! A map's decals ([`bri_content::decals`]): the triangles each decal's volume touches are found once when the map loads, and drawn again
//! every frame with the picture projected on them ([`crate::surface_projections`]).
use crate::projected_textures::ProjectionBlend;
use crate::scene::{AlphaMode, MaterialKind, SceneData};
use crate::surface_projections::{
    ProjectedSurfaces, SurfaceBatch, SurfaceImage, SurfaceProjection, SurfaceVertex,
};
use bri_content::decals::{Blend, Decal, DecalSet, Receivers};
use glam::{Mat4, Vec3};

/// The depth bias a decal's triangles are drawn with, in the renderer's depth units.
pub const DEPTH_BIAS: i32 = 8;

/// The triangles of `scene` a decal draws on, three vertices each. Translucent surfaces take none; ground is the terrain materials.
pub fn gather(scene: &SceneData, decal: &Decal) -> (Vec<SurfaceVertex>, Vec<u32>) {
    let forward = Vec3::from(decal.forward);
    let planes: Vec<(Vec3, f32)> = decal
        .planes
        .iter()
        .map(|p| (Vec3::new(p[0], p[1], p[2]), p[3]))
        .collect();
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for batch in &scene.batches {
        let Some(material) = scene.materials.get(batch.material) else {
            continue;
        };
        if !matches!(material.alpha, AlphaMode::Opaque | AlphaMode::Mask(_)) {
            continue;
        }
        let ground = matches!(
            material.kind,
            MaterialKind::Terrain | MaterialKind::LayerBlend
        );
        let rule: &Receivers = if ground {
            &decal.ground
        } else {
            &decal.surfaces
        };
        if !rule.enabled {
            continue;
        }
        let range = batch.indices.start as usize..batch.indices.end as usize;
        for tri in scene.indices[range].chunks_exact(3) {
            let v = [
                scene.vertices[tri[0] as usize],
                scene.vertices[tri[1] as usize],
                scene.vertices[tri[2] as usize],
            ];
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
    }
    (vertices, indices)
}

/// A map's decals as loaded: the set, its pictures in order and the triangles each decal draws on.
pub struct LoadedDecals {
    pub set: DecalSet,
    pub pictures: Vec<crate::scene::SceneImage>,
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

struct Placed {
    decal: Decal,
    image: SurfaceImage,
    vertices: Vec<SurfaceVertex>,
    indices: Vec<u32>,
}

/// The map's decals uploaded: the receiving triangles, the pictures and the surface pass that draws them.
pub struct DecalLayer {
    surfaces: ProjectedSurfaces,
    placed: Vec<Placed>,
    /// Every decal's vertices and indices end to end.
    vertices: Vec<SurfaceVertex>,
    ranges: Vec<std::ops::Range<usize>>,
    indices: Vec<Vec<u32>>,
}

impl DecalLayer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        samples: u32,
        loaded: &LoadedDecals,
    ) -> Self {
        let mut surfaces = ProjectedSurfaces::new(device, queue, target, depth, samples);
        let mut placed = Vec::new();
        for ((decal, picture), (vertices, indices)) in loaded
            .set
            .decals
            .iter()
            .zip(&loaded.pictures)
            .zip(&loaded.receivers)
        {
            let image = surfaces.add_image(
                device,
                queue,
                &picture.rgba,
                picture.width,
                picture.height,
                true,
            );
            placed.push(Placed {
                decal: decal.clone(),
                image,
                vertices: vertices.clone(),
                indices: indices.clone(),
            });
        }
        let mut vertices = Vec::new();
        let (mut ranges, mut indices) = (Vec::new(), Vec::new());
        for p in &placed {
            let base = vertices.len() as u32;
            let start = vertices.len();
            vertices.extend_from_slice(&p.vertices);
            ranges.push(start..vertices.len());
            indices.push(p.indices.iter().map(|i| i + base).collect());
        }
        Self {
            surfaces,
            placed,
            vertices,
            ranges,
            indices,
        }
    }

    /// Triangles each decal draws on, for the log.
    pub fn triangle_counts(&self) -> Vec<usize> {
        self.placed.iter().map(|p| p.indices.len() / 3).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Upload the frame's camera and the (unchanging) geometry.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        eye: Vec3,
        seconds: f32,
    ) {
        let projections: Vec<SurfaceProjection> = self
            .placed
            .iter()
            .map(|p| SurfaceProjection {
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
        let _ = &self.ranges;
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
