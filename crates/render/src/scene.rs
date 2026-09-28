//! Persistent world-space rendering on the caller's device. The caller owns
//! the swapchain/offscreen attachment, encoder and submission, so UI passes can
//! follow this pass without another adapter/device or scene re-upload.
use anyhow::{Context, Result, ensure};
use glam::{Mat4, Vec3, Vec4};
use std::ops::Range;
use wgpu::util::DeviceExt;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SceneVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub lightmap_uv: [f32; 2],
    /// Display-encoded original material/brick color, straight alpha.
    pub color: [f32; 4],
    /// Brick FX only: world brick centre and packed ids (`BrickFx::encode`).
    /// All zero for everything else, including bricks without FX.
    pub fx: [f32; 4],
}

/// v20 colour FX 0..6 (None, Pearl, Chrome, Glow, Blink, Swirl, Rainbow) and
/// shape FX 0..2 (None, Undulo, Water). The shader evaluates the per-vertex
/// equations of `blocklandv20.exe`'s quad emitter 0x52ed70; see
/// docs/audits/bricks.md.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BrickFx {
    pub color: u8,
    pub shape: u8,
}
/// Decoded per-vertex FX record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrickFxVertex {
    pub fx: BrickFx,
    /// v20 quad corner 0..3; swirl phases each corner by 250 ms.
    pub corner: u8,
    /// Datablock depth (brickSizeY) in studs, scaling pearl and chrome.
    pub depth_studs: u8,
    pub centre: [f32; 3],
}
impl BrickFx {
    pub fn new(color: u8, shape: u8) -> Result<Self> {
        ensure!(color <= 6 && shape <= 2, "Unsupported brick FX IDs");
        Ok(Self { color, shape })
    }
    /// `[centre, 1 + color + 8*shape + 32*corner + 128*depth]`, or zeros
    /// when the brick has no FX. Integers stay exact in f32.
    pub fn encode(self, centre: [f32; 3], corner: u8, depth_studs: u8) -> Result<[f32; 4]> {
        Self::new(self.color, self.shape)?;
        ensure!(
            corner < 4 && depth_studs > 0,
            "Invalid brick FX corner/depth"
        );
        if self == Self::default() {
            return Ok([0.; 4]);
        }
        let code = 1
            + u32::from(self.color)
            + 8 * u32::from(self.shape)
            + 32 * u32::from(corner)
            + 128 * u32::from(depth_studs);
        Ok([centre[0], centre[1], centre[2], code as f32])
    }
    pub fn decode(value: [f32; 4]) -> Option<BrickFxVertex> {
        if value == [0.; 4] {
            return Some(BrickFxVertex {
                fx: Self::default(),
                corner: 0,
                depth_studs: 1,
                centre: [0.; 3],
            });
        }
        if !value.iter().all(|v| v.is_finite()) || value[3] < 1. || value[3].fract() != 0. {
            return None;
        }
        let code = value[3] as u32 - 1;
        let fx = Self::new((code % 8) as u8, (code / 8 % 4) as u8).ok()?;
        let depth = code / 128;
        (fx != Self::default() && (1..=255).contains(&depth)).then_some(BrickFxVertex {
            fx,
            corner: (code / 32 % 4) as u8,
            depth_studs: depth as u8,
            centre: [value[0], value[1], value[2]],
        })
    }
    /// Conservative world-axis visual bounds; gameplay collision stays authored.
    /// Undulo moves each axis by at most 0.08; water only lowers, by up to 0.2.
    pub fn displacement_bounds(self) -> [f32; 3] {
        match self.shape {
            1 => [0.08; 3],
            2 => [0., 0.2, 0.],
            _ => [0.; 3],
        }
    }
}
/// A checked native interpretation; raw BLB values remain unchanged in content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrickVertexColor {
    pub rgba: [f32; 4],
    pub provisional: bool,
}
pub fn resolve_brick_vertex_color(
    paint: [f32; 4],
    authored: Option<[f32; 4]>,
) -> Result<BrickVertexColor> {
    ensure!(
        paint
            .iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
        "Invalid brick paint"
    );
    let Some(c) = authored else {
        return Ok(BrickVertexColor {
            rgba: paint,
            provisional: false,
        });
    };
    ensure!(
        c.iter().all(|v| v.is_finite()),
        "Nonfinite authored brick color"
    );
    if c[3] == -1. {
        // Stock negative-alpha encoding selects signed RGB paint offsets.
        // Retain the existing inherited-paint-alpha policy with its diagnostic.
        return Ok(BrickVertexColor {
            rgba: [
                (paint[0] + c[0]).clamp(0., 1.),
                (paint[1] + c[1]).clamp(0., 1.),
                (paint[2] + c[2]).clamp(0., 1.),
                paint[3],
            ],
            provisional: paint[3] != 1.,
        });
    }
    ensure!(
        (0. ..=1.).contains(&c[3]),
        "Unsupported authored brick alpha sentinel"
    );
    // Preserve unresolved literal RGB exactly. Stock pumpkin200/150 cannot
    // safely be classified as normalized floats, bytes or a special encoding
    // without an original reader/reference comparison. Do not guess here.
    Ok(BrickVertexColor {
        rgba: c,
        provisional: c[..3].iter().any(|v| !(0. ..=1.).contains(v)),
    })
}

#[derive(Clone, Debug)]
pub struct SceneImage {
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Diffuse images use sRGB; authored lightmaps and weights are raw UNORM.
    pub srgb: bool,
}
impl SceneImage {
    pub fn white() -> Self {
        Self {
            label: "white".into(),
            width: 1,
            height: 1,
            rgba: vec![255; 4],
            srgb: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialKind {
    Surface,
    Terrain,
    VertexLit,
    /// Original brick mask/print: display-space RGB overlay on paint. Texture
    /// alpha controls pigment coverage, not the geometry's transparency.
    BrickOverlay,
    /// Camera-relative authored sky; never writes world depth or receives fog.
    Sky,
    /// Camera-relative repeating cloud layer; normal.xy carries UV velocity.
    Cloud,
    Water,
    /// Display-space pigment coverage over vertex tint, without scene lighting.
    UnlitOverlay,
    /// Texture/tint multiplication and texture coverage, without scene lighting.
    Unlit,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlphaMode {
    Opaque,
    Mask(f32),
    Blend,
    /// Straight-alpha-weighted source RGB added to destination; no depth writes.
    Additive,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    /// Slots 0..8 diffuse layers, 8 lightmap, 9/10 RGBA weight maps,
    /// 11 terrain detail, 12 terrain emboss bump.
    /// A surface/VertexLit material uses diffuse slot 0; supply valid fallback
    /// indices in unused slots (normally the 1x1 white image).
    pub images: [usize; 13],
    pub kind: MaterialKind,
    pub alpha: AlphaMode,
    pub double_sided: bool,
    /// v20 `fxBrickBatcher` loads brickSIDE with `GL_CLAMP` and nearest
    /// magnification; every other brick surface repeats with linear filtering.
    pub clamp_nearest: bool,
    /// v20 temp-brick flash: opacity follows `$pref::HUD::tempBrickFlash*`
    /// (triangle wave 0.3..0.6 over 800 ms) instead of vertex alpha.
    pub temp_brick_flash: bool,
    /// Kind-specific uniforms, required for water and terrain only.
    /// Water: flow/wave/opacity, distortion/depth flag, surface+shore
    /// tiling/reflection/parallax. Terrain: see `terrain_scene::parameters`.
    pub parameters: Option<[[f32; 4]; 3]>,
}
impl Material {
    pub fn brick_overlay(name: impl Into<String>, diffuse: usize) -> Self {
        let mut material = Self::vertex_lit(name, diffuse);
        material.kind = MaterialKind::BrickOverlay;
        material
    }
    pub fn vertex_lit(name: impl Into<String>, diffuse: usize) -> Self {
        let mut material = Self::surface(name, diffuse, 0);
        material.kind = MaterialKind::VertexLit;
        material
    }
    pub fn surface(name: impl Into<String>, diffuse: usize, lightmap: usize) -> Self {
        let mut images = [0; 13];
        images[0] = diffuse;
        images[8] = lightmap;
        Self {
            name: name.into(),
            images,
            kind: MaterialKind::Surface,
            alpha: AlphaMode::Opaque,
            double_sided: false,
            clamp_nearest: false,
            temp_brick_flash: false,
            parameters: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MeshBatch {
    pub indices: Range<u32>,
    pub material: usize,
    /// World-space center used for back-to-front translucent batch ordering.
    pub center: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct SceneData {
    pub id: String,
    pub name: String,
    pub spawn: [f32; 3],
    pub vertices: Vec<SceneVertex>,
    pub indices: Vec<u32>,
    pub images: Vec<SceneImage>,
    pub materials: Vec<Material>,
    pub batches: Vec<MeshBatch>,
    /// Explicitly exposed conversion/render gaps; the host should retain them
    /// in logs and handoff evidence instead of calling the map fully supported.
    pub omissions: Vec<String>,
    /// Authored sunlight for non-lightmapped meshes, in native Y-up coordinates.
    pub sun_direction: [f32; 3],
    pub sun_color: [f32; 3],
    pub ambient: [f32; 3],
    pub fog: bri_content::environment::Fog,
    /// Authored fog backdrop below the sky horizon, or a diagnostic clear color.
    pub clear_color: [f32; 4],
}
impl Default for SceneData {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            spawn: [0.0; 3],
            vertices: vec![],
            indices: vec![],
            images: vec![SceneImage::white()],
            materials: vec![],
            batches: vec![],
            omissions: vec![],
            sun_direction: [0.3, -1.0, 0.4],
            sun_color: [0.7; 3],
            ambient: [0.35; 3],
            clear_color: [0.05, 0.08, 0.12, 1.0],
            fog: Default::default(),
        }
    }
}
impl SceneData {
    /// Append one native brick using an independent authored mesh identity.
    /// Surface materials are [Top, Side, BottomEdge, BottomLoop, Ramp, Print];
    /// the caller binds converted original images/print textures explicitly.
    /// This does not depend on simulation/physics handles, nor infer authority.
    /// Replace/upload the resulting brick SceneData separately from the map.
    pub fn append_brick(
        &mut self,
        mesh: &bri_content::brick::Brick,
        transform: [f32; 16],
        paint: [f32; 4],
        surface_materials: [usize; 6],
    ) -> Result<()> {
        self.append_brick_with_fx(
            mesh,
            transform,
            paint,
            surface_materials,
            BrickFx::default(),
        )
    }
    /// The real replicated-brick path uses this; default append remains unchanged
    /// for ghosts and unrelated mesh adapters. FX metadata never changes UV0.
    pub fn append_brick_with_fx(
        &mut self,
        mesh: &bri_content::brick::Brick,
        transform: [f32; 16],
        paint: [f32; 4],
        surface_materials: [usize; 6],
        fx: BrickFx,
    ) -> Result<()> {
        mesh.validate()?;
        self.append_validated_brick_with_fx(mesh, transform, paint, surface_materials, fx)
    }
    /// As `append_brick_with_fx`, for a mesh the caller already validated once
    /// (chunk builders place the same mesh thousands of times).
    pub fn append_validated_brick_with_fx(
        &mut self,
        mesh: &bri_content::brick::Brick,
        transform: [f32; 16],
        paint: [f32; 4],
        surface_materials: [usize; 6],
        fx: BrickFx,
    ) -> Result<()> {
        use bri_content::brick::Surface;
        let centre = [transform[12], transform[13], transform[14]];
        let depth_studs = mesh.footprint_studs[1].clamp(1, 255) as u8;
        fx.encode(centre, 0, depth_studs)?;
        resolve_brick_vertex_color(paint, None)?;
        // Validate all authored sentinels before publishing any geometry.
        for quad in &mesh.quads {
            if let Some(colors) = quad.colors {
                for color in colors {
                    resolve_brick_vertex_color(paint, Some(color))?;
                }
            }
        }
        ensure!(
            transform.iter().chain(&paint).all(|v| v.is_finite()),
            "Nonfinite brick transform/color"
        );
        ensure!(
            surface_materials.iter().all(|i| *i < self.materials.len()),
            "Brick surface material is unbound"
        );
        ensure!(
            self.vertices
                .len()
                .saturating_add(mesh.quads.len().saturating_mul(4))
                < u32::MAX as usize
                && self
                    .indices
                    .len()
                    .saturating_add(mesh.quads.len().saturating_mul(6))
                    < u32::MAX as usize,
            "Brick scene too large"
        );
        ensure!(
            fx == BrickFx::default()
                || surface_materials.iter().all(|i| matches!(
                    self.materials[*i].kind,
                    MaterialKind::VertexLit | MaterialKind::BrickOverlay
                )),
            "Brick FX require non-lightmapped brick materials"
        );
        let placement = Mat4::from_cols_array(&transform);
        ensure!(
            placement.determinant().abs() > 0.0000001,
            "Singular brick transform"
        );
        let normals = placement.inverse().transpose();
        let mirrored = placement.determinant() < 0.0;
        let mut groups = std::collections::BTreeMap::<usize, Vec<u32>>::new();
        let mut blend_materials = std::collections::BTreeMap::new();
        let mut provisional_color = false;
        for quad in &mesh.quads {
            let slot = match quad.surface {
                Surface::Top => 0,
                Surface::Side => 1,
                Surface::BottomEdge => 2,
                Surface::BottomLoop => 3,
                Surface::Ramp => 4,
                Surface::Print => 5,
            };
            let mut material = surface_materials[slot];
            let colors = quad.colors.map_or([paint; 4], |colors| {
                colors.map(|color| {
                    let resolved =
                        resolve_brick_vertex_color(paint, Some(color)).expect("prevalidated color");
                    provisional_color |= resolved.provisional;
                    resolved.rgba
                })
            });
            if colors.iter().any(|c| c[3] < 1.0)
                && self.materials[material].alpha != AlphaMode::Blend
            {
                material = *blend_materials.entry(material).or_insert_with(|| {
                    let mut copy = self.materials[material].clone();
                    copy.alpha = AlphaMode::Blend;
                    if let Some(index) = self.materials.iter().position(|m| m == &copy) {
                        return index;
                    }
                    let index = self.materials.len();
                    self.materials.push(copy);
                    index
                });
            }
            // v20 applies colour FX only to paint quads (negative authored
            // alpha); literal-colour quads keep shape FX alone.
            let quad_fx = if quad.colors.is_some_and(|c| c.iter().any(|c| c[3] >= 0.0)) {
                BrickFx { color: 0, ..fx }
            } else {
                fx
            };
            let base = self.vertices.len() as u32;
            // Native quads list v20's corners in order 0, 3, 2, 1.
            for ((vertex, color), corner) in quad.vertices.iter().zip(colors).zip([0, 3, 2, 1]) {
                self.vertices.push(SceneVertex {
                    position: placement
                        .transform_point3(Vec3::from(vertex.position))
                        .to_array(),
                    normal: normals
                        .transform_vector3(Vec3::from(vertex.normal))
                        .normalize_or_zero()
                        .to_array(),
                    uv: vertex.uv,
                    lightmap_uv: [0.; 2],
                    color,
                    fx: quad_fx.encode(centre, corner, depth_studs)?,
                });
            }
            let group = groups.entry(material).or_default();
            group.extend(if mirrored {
                [base, base + 2, base + 1, base, base + 3, base + 2]
            } else {
                [base, base + 1, base + 2, base, base + 2, base + 3]
            });
        }
        for (material, indices) in groups {
            let start = self.indices.len() as u32;
            self.indices.extend(indices);
            self.batches.push(MeshBatch {
                indices: start..self.indices.len() as u32,
                material,
                center: placement.transform_point3(Vec3::ZERO).to_array(),
            });
        }
        if provisional_color {
            self.omissions.push(format!("Brick mesh {} uses explicit native sentinel/opacity adaptation; exact transparent-paint or out-of-range literal input conversion remains unverified",mesh.id));
        }
        if fx != BrickFx::default() {
            self.omissions.push("Brick FX IDs are source-backed; reflection, phase and displacement equations are native visual approximations pending original-engine calibration".into());
        }
        Ok(())
    }

    /// Consolidate opaque geometry after appending many bricks. Per-vertex
    /// paint/UVs stay intact; translucent batches remain independently sortable.
    pub fn coalesce_opaque_batches(&mut self) -> Result<()> {
        self.validate_geometry()?;
        let mut opaque = std::collections::BTreeMap::<usize, Vec<u32>>::new();
        let mut translucent = Vec::new();
        for batch in &self.batches {
            let indices =
                self.indices[batch.indices.start as usize..batch.indices.end as usize].to_vec();
            if matches!(
                self.materials[batch.material].alpha,
                AlphaMode::Blend | AlphaMode::Additive
            ) {
                translucent.push((batch.material, batch.center, indices));
            } else {
                opaque.entry(batch.material).or_default().extend(indices);
            }
        }
        self.indices.clear();
        self.batches.clear();
        for (material, indices) in opaque {
            let start = self.indices.len() as u32;
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for &index in &indices {
                let p = Vec3::from(self.vertices[index as usize].position);
                min = min.min(p);
                max = max.max(p);
            }
            let center = if indices.is_empty() {
                [0.0; 3]
            } else {
                ((min + max) * 0.5).to_array()
            };
            self.indices.extend(indices);
            self.batches.push(MeshBatch {
                indices: start..self.indices.len() as u32,
                material,
                center,
            });
        }
        for (material, center, indices) in translucent {
            let start = self.indices.len() as u32;
            self.indices.extend(indices);
            self.batches.push(MeshBatch {
                indices: start..self.indices.len() as u32,
                material,
                center,
            });
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        self.validate_geometry()?;
        for image in &self.images {
            let bytes = u64::from(image.width)
                .checked_mul(u64::from(image.height))
                .and_then(|pixels| pixels.checked_mul(4));
            ensure!(
                image.width > 0 && image.height > 0 && bytes == Some(image.rgba.len() as u64),
                "Invalid scene image {}",
                image.label
            );
        }
        for material in &self.materials {
            ensure!(
                material.images.iter().all(|i| *i < self.images.len()),
                "Material {} references missing image",
                material.name
            );
        }
        Ok(())
    }
    /// Geometry, batches and material uniforms, without image resources. Chunk
    /// geometry bound to a shared material palette carries no images itself.
    pub fn validate_geometry(&self) -> Result<()> {
        self.fog.validate()?;
        ensure!(
            self.vertices.len() <= u32::MAX as usize && self.indices.len() <= u32::MAX as usize,
            "Scene exceeds indexed geometry limits"
        );
        ensure!(
            self.vertices.iter().all(|v| v
                .position
                .iter()
                .chain(&v.normal)
                .chain(&v.uv)
                .chain(&v.lightmap_uv)
                .chain(&v.color)
                .chain(&v.fx)
                .all(|f| f.is_finite())),
            "Non-finite scene vertex"
        );
        ensure!(
            self.indices
                .iter()
                .all(|i| (*i as usize) < self.vertices.len()),
            "Scene index out of range"
        );
        for material in &self.materials {
            ensure!(
                material.parameters.is_some()
                    == matches!(material.kind, MaterialKind::Water | MaterialKind::Terrain)
                    && material
                        .parameters
                        .as_ref()
                        .is_none_or(|p| p.iter().flatten().all(|x| x.is_finite())),
                "Invalid water/terrain material uniforms"
            );
            if let AlphaMode::Mask(cutoff) = material.alpha {
                ensure!(
                    cutoff.is_finite() && (0.0..=1.0).contains(&cutoff),
                    "Invalid alpha cutoff"
                );
            }
        }
        for batch in &self.batches {
            ensure!(
                batch.indices.start <= batch.indices.end
                    && batch.indices.end as usize <= self.indices.len()
                    && (batch.indices.end - batch.indices.start).is_multiple_of(3)
                    && batch.material < self.materials.len()
                    && batch.center.iter().all(|f| f.is_finite()),
                "Invalid scene batch"
            );
            for &index in &self.indices[batch.indices.start as usize..batch.indices.end as usize] {
                let fx = self.vertices[index as usize].fx;
                if fx != [0.; 4] {
                    ensure!(
                        BrickFx::decode(fx).is_some()
                            && matches!(
                                self.materials[batch.material].kind,
                                MaterialKind::VertexLit | MaterialKind::BrickOverlay
                            ),
                        "Invalid or non-brick FX marker"
                    );
                }
            }
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Camera {
    pub view_projection: [f32; 16],
    pub eye: [f32; 4],
    pub sun_direction: [f32; 4],
    pub sun_color: [f32; 4],
    pub ambient: [f32; 4],
    pub fog_color: [f32; 4],
    /// Fog start, visible distance, animation seconds, fog enabled.
    pub atmosphere: [f32; 4],
}
impl Camera {
    /// Native world uses Y up, right-handed coordinates and a 0..1 depth range.
    pub fn perspective(
        eye: [f32; 3],
        target: [f32; 3],
        aspect: f32,
        fov_y: f32,
        near: f32,
        far: f32,
    ) -> Self {
        let eye_v = Vec3::from(eye);
        let target_v = Vec3::from(target);
        let direction = (target_v - eye_v).normalize_or_zero();
        let target_v = if direction.length_squared() < 0.5 {
            eye_v + Vec3::NEG_Z
        } else {
            target_v
        };
        let up = if direction.dot(Vec3::Y).abs() > 0.9999 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let view = glam::camera::rh::view::look_at_mat4(eye_v, target_v, up);
        let projection = glam::camera::rh::proj::directx::perspective(fov_y, aspect, near, far);
        Self {
            view_projection: (projection * view).to_cols_array(),
            eye: [eye[0], eye[1], eye[2], 1.0],
            sun_direction: [0.3, -1.0, 0.4, 0.0],
            sun_color: [0.7, 0.7, 0.7, 0.0],
            ambient: [0.35, 0.35, 0.35, 0.0],
            fog_color: [0.0; 4],
            atmosphere: [0.0; 4],
        }
    }
    pub fn apply_environment(&mut self, scene: &SceneData) {
        self.sun_direction[..3].copy_from_slice(&scene.sun_direction);
        self.sun_color[..3].copy_from_slice(&scene.sun_color);
        self.ambient[..3].copy_from_slice(&scene.ambient);
        self.fog_color[..3].copy_from_slice(&scene.fog.color);
        self.atmosphere[0] = scene.fog.start;
        self.atmosphere[1] = scene.fog.end;
        self.atmosphere[3] = if scene.fog.end > 0.0 { 1.0 } else { 0.0 };
    }
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            view_projection: Mat4::IDENTITY.to_cols_array(),
            eye: [0.0, 0.0, 0.0, 1.0],
            sun_direction: [0.0, -1.0, 0.0, 0.0],
            sun_color: [0.7, 0.7, 0.7, 0.0],
            ambient: [0.3, 0.3, 0.3, 0.0],
            fog_color: [0.0; 4],
            atmosphere: [0.0; 4],
        }
    }
}

pub struct GpuScene {
    material_descriptors: Vec<Material>,
    image_signatures: Vec<([u32; 2], bool, [u8; 32])>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    materials: Vec<wgpu::BindGroup>,
    batches: Vec<MeshBatch>,
    /// Opaque/alpha/additive, double sided, background, alpha-masked.
    material_modes: Vec<(usize, bool, bool, bool)>,
    /// World-space bounds of static chunk geometry; unbounded scenes always draw.
    bounds: Option<(Vec3, Vec3)>,
    pub vertex_count: usize,
    pub index_count: usize,
    pub image_count: usize,
}
fn image_signatures(data: &SceneData) -> Vec<([u32; 2], bool, [u8; 32])> {
    use sha2::{Digest, Sha256};
    data.images
        .iter()
        .map(|image| {
            (
                [image.width, image.height],
                image.srgb,
                Sha256::digest(&image.rgba).into(),
            )
        })
        .collect()
}

/// Per-object state. Geometry, materials and textures remain in a shared GpuScene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneTransform {
    pub transform: Mat4,
    pub tint: [f32; 4],
}
impl Default for SceneTransform {
    fn default() -> Self {
        Self {
            transform: Mat4::IDENTITY,
            tint: [1.; 4],
        }
    }
}
impl SceneTransform {
    fn validate(&self) -> Result<()> {
        let m = self.transform;
        ensure!(
            m.is_finite()
                && m.determinant() > 1e-8
                && m.x_axis.w == 0.
                && m.y_axis.w == 0.
                && m.z_axis.w == 0.
                && m.w_axis.w == 1.
                && m.w_axis.truncate().abs().max_element() < 1e7
                && [m.x_axis, m.y_axis, m.z_axis]
                    .iter()
                    .all(|v| v.truncate().length() <= 1000.)
                && self
                    .tint
                    .iter()
                    .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
            "Invalid scene instance transform/tint"
        );
        Ok(())
    }
    fn record(&self) -> InstanceRecord {
        InstanceRecord {
            transform: self.transform.to_cols_array_2d(),
            tint: self.tint,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceRecord {
    transform: [[f32; 4]; 4],
    tint: [f32; 4],
}

/// One bounded persistent instance buffer per shared model group. Update at most
/// once per submission, like the camera. Empty updates remove every instance.
pub struct GpuInstances {
    buffer: wgpu::Buffer,
    transforms: Vec<SceneTransform>,
    capacity: usize,
}
impl GpuInstances {
    pub fn new(device: &wgpu::Device, capacity: usize) -> Result<Self> {
        ensure!(
            (1..=16384).contains(&capacity),
            "Scene instance capacity must be1..16384"
        );
        let size = capacity as u64 * std::mem::size_of::<InstanceRecord>() as u64;
        ensure!(
            size <= device.limits().max_buffer_size,
            "Instance buffer exceeds device limit"
        );
        Ok(Self {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shared model instances"),
                size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            transforms: Vec::new(),
            capacity,
        })
    }
    pub fn len(&self) -> usize {
        self.transforms.len()
    }
    pub fn is_empty(&self) -> bool {
        self.transforms.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    /// Validate everything before changing CPU/GPU state. Returns false when
    /// unchanged, allowing static world items to incur no per-frame upload.
    pub fn update(&mut self, queue: &wgpu::Queue, transforms: &[SceneTransform]) -> Result<bool> {
        ensure!(
            transforms.len() <= self.capacity,
            "Scene instance capacity exceeded"
        );
        for transform in transforms {
            transform.validate()?;
        }
        if self.transforms == transforms {
            return Ok(false);
        }
        if !transforms.is_empty() {
            let records: Vec<_> = transforms.iter().map(SceneTransform::record).collect();
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&records));
        }
        self.transforms = transforms.to_vec();
        Ok(true)
    }
}

impl GpuScene {
    /// Stop drawing every batch that lies inside one of `ranges` (index
    /// ranges of a smashed map shape). Upload the scene again to restore them.
    pub fn hide_indices(&mut self, ranges: &[Range<u32>]) {
        self.batches.retain(|b| {
            !ranges
                .iter()
                .any(|r| r.start <= b.indices.start && b.indices.end <= r.end)
        });
    }
    /// Update a posed model with unchanged topology/materials. The caller must
    /// re-upload if visibility, mesh frame topology or appearance bindings change.
    /// One update per scene per submission: GPU writes are not per-draw snapshots.
    pub fn update_vertices(
        &mut self,
        queue: &wgpu::Queue,
        vertices: &[SceneVertex],
        centers: &[[f32; 3]],
    ) -> Result<()> {
        ensure!(
            vertices.len() == self.vertex_count && centers.len() == self.batches.len(),
            "Dynamic scene topology changed"
        );
        ensure!(
            vertices.iter().all(|v| v
                .position
                .iter()
                .chain(&v.normal)
                .chain(&v.uv)
                .chain(&v.lightmap_uv)
                .chain(&v.color)
                .all(|x| x.is_finite()))
                && centers.iter().flatten().all(|x| x.is_finite()),
            "Non-finite dynamic scene"
        );
        if !vertices.is_empty() {
            queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(vertices));
        }
        for (batch, center) in self.batches.iter_mut().zip(centers) {
            batch.center = *center;
        }
        Ok(())
    }
}

impl GpuScene {
    /// A view drawing only `batch`, sharing this scene's GPU buffers, textures
    /// and bind groups. Lets one uploaded mesh set carry independently
    /// instanced parts (terrain tiles) without duplicating GPU resources.
    pub fn batch_view(&self, batch: usize) -> Result<GpuScene> {
        let selected = self
            .batches
            .get(batch)
            .context("Scene batch view out of range")?
            .clone();
        Ok(GpuScene {
            material_descriptors: self.material_descriptors.clone(),
            image_signatures: self.image_signatures.clone(),
            vertices: self.vertices.clone(),
            indices: self.indices.clone(),
            materials: self.materials.clone(),
            batches: vec![selected],
            material_modes: self.material_modes.clone(),
            bounds: self.bounds,
            vertex_count: self.vertex_count,
            index_count: self.index_count,
            image_count: self.image_count,
        })
    }
}

/// Vertex/index buffers never have zero size; empty scenes carry no batches.
fn geometry_buffers(
    device: &wgpu::Device,
    label: &str,
    data: &SceneData,
) -> (wgpu::Buffer, wgpu::Buffer) {
    let empty_vertex = [SceneVertex {
        position: [0.; 3],
        normal: [0.; 3],
        uv: [0.; 2],
        lightmap_uv: [0.; 2],
        color: [0.; 4],
        fx: [0.; 4],
    }];
    let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(if data.vertices.is_empty() {
            &empty_vertex
        } else {
            &data.vertices
        }),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    });
    let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("scene indices"),
        contents: bytemuck::cast_slice(if data.indices.is_empty() {
            &[0u32]
        } else {
            &data.indices
        }),
        usage: wgpu::BufferUsages::INDEX,
    });
    (vertices, indices)
}

/// Clip-space planes (a, b, c, d) with inside meaning ax+by+cz+d >= 0.
fn frustum_planes(view_projection: Mat4) -> [glam::Vec4; 6] {
    let (r0, r1, r2, r3) = (
        view_projection.row(0),
        view_projection.row(1),
        view_projection.row(2),
        view_projection.row(3),
    );
    // 0..1 depth: the near plane is row 2 alone.
    [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2]
}
fn aabb_visible(planes: &[glam::Vec4; 6], (min, max): (Vec3, Vec3)) -> bool {
    planes.iter().all(|plane| {
        let normal = plane.truncate();
        let farthest = Vec3::select(normal.cmpge(Vec3::ZERO), max, min);
        normal.dot(farthest) + plane.w >= 0.
    })
}

/// Diffuse texture filtering, from v20's Trilinear Filtering, Use Sharp
/// Filter and Anisotropy options. Lightmaps and weight maps always use plain
/// bilinear sampling of their base level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureFiltering {
    /// Blend between mip levels instead of snapping to the nearest one.
    pub trilinear: bool,
    /// Nearest-texel magnification and minification (`useGLNearest`).
    pub sharp: bool,
    /// Maximum anisotropic samples: 1, 2, 4, 8 or 16.
    pub anisotropy: u16,
}
impl Default for TextureFiltering {
    fn default() -> Self {
        Self {
            trilinear: true,
            sharp: false,
            anisotropy: 8,
        }
    }
}
impl TextureFiltering {
    /// v20 stores anisotropy as a 0..1 slider value.
    pub fn from_v20(trilinear: bool, sharp: bool, anisotropy: f32) -> Self {
        let samples = if anisotropy.is_finite() {
            1.0 + anisotropy.clamp(0.0, 1.0) * 15.0
        } else {
            1.0
        };
        Self {
            trilinear,
            sharp,
            anisotropy: [16, 8, 4, 2]
                .into_iter()
                .find(|n| samples >= f32::from(*n))
                .unwrap_or(1),
        }
    }
}
const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2,3=>Float32x2,4=>Float32x4,10=>Float32x4];
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 5] =
    wgpu::vertex_attr_array![5=>Float32x4,6=>Float32x4,7=>Float32x4,8=>Float32x4,9=>Float32x4];
/// Scene vertices plus per-instance model matrix and tint.
fn vertex_layouts() -> [Option<wgpu::VertexBufferLayout<'static>>; 2] {
    [
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SceneVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES,
        }),
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRecord>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRIBUTES,
        }),
    ]
}
fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}
/// Camera, lights and the four shared samplers: filtered repeat/clamp for
/// diffuse images, plain bilinear repeat/clamp for lightmaps and weights.
fn camera_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    camera: &wgpu::Buffer,
    lights: &wgpu::Buffer,
    filtering: TextureFiltering,
    shadows: &crate::shadow::ShadowMaps,
) -> wgpu::BindGroup {
    let filtered = |address_mode| {
        let filter = if filtering.sharp {
            wgpu::FilterMode::Nearest
        } else {
            wgpu::FilterMode::Linear
        };
        // Anisotropy requires linear filtering throughout.
        let anisotropic = filtering.anisotropy > 1 && !filtering.sharp && filtering.trilinear;
        device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: address_mode,
            address_mode_v: address_mode,
            mag_filter: filter,
            min_filter: filter,
            mipmap_filter: if filtering.trilinear {
                wgpu::MipmapFilterMode::Linear
            } else {
                wgpu::MipmapFilterMode::Nearest
            },
            anisotropy_clamp: if anisotropic { filtering.anisotropy } else { 1 },
            ..Default::default()
        })
    };
    let plain = |address_mode| {
        device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: address_mode,
            address_mode_v: address_mode,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        })
    };
    let samplers = [
        filtered(wgpu::AddressMode::Repeat),
        filtered(wgpu::AddressMode::ClampToEdge),
        plain(wgpu::AddressMode::Repeat),
        plain(wgpu::AddressMode::ClampToEdge),
    ];
    let mut entries = vec![
        wgpu::BindGroupEntry {
            binding: 0,
            resource: camera.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 1,
            resource: lights.as_entire_binding(),
        },
    ];
    for (i, sampler) in samplers.iter().enumerate() {
        entries.push(wgpu::BindGroupEntry {
            binding: 2 + i as u32,
            resource: wgpu::BindingResource::Sampler(sampler),
        });
    }
    entries.extend([
        wgpu::BindGroupEntry {
            binding: 6,
            resource: wgpu::BindingResource::TextureView(&shadows.array_view),
        },
        wgpu::BindGroupEntry {
            binding: 7,
            resource: wgpu::BindingResource::Sampler(&shadows.comparison),
        },
        wgpu::BindGroupEntry {
            binding: 8,
            resource: shadows.receiver.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 9,
            resource: wgpu::BindingResource::Sampler(&shadows.point),
        },
    ]);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("camera"),
        layout,
        entries: &entries,
    })
}

/// Scenes and instanced model groups that cast sun shadows.
#[derive(Clone, Copy, Default)]
pub struct ShadowCasters<'a> {
    pub scenes: &'a [&'a GpuScene],
    pub instances: &'a [(&'a GpuScene, &'a GpuInstances)],
}

pub const MAX_POINT_LIGHTS: usize = 256;
/// Native unshadowed point illumination. Radius and RGB come from the effect clock.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PointLight {
    pub position_radius: [f32; 4],
    pub color: [f32; 4],
}

pub struct SceneRenderer {
    identity_instance: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    light_buffer: wgpu::Buffer,
    camera_layout: wgpu::BindGroupLayout,
    camera_group: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    pipelines: Vec<wgpu::RenderPipeline>,
    filtering: TextureFiltering,
    samples: u32,
    shadows: crate::shadow::ShadowMaps,
    eye: Vec3,
    frustum: Option<[glam::Vec4; 6]>,
}

impl SceneRenderer {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        Self::with_samples(device, color_format, 1)
    }
    /// Pipelines for `samples`-per-pixel color and depth attachments (see
    /// `create_depth_samples`); the caller resolves the color attachment.
    pub fn with_samples(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        samples: u32,
    ) -> Self {
        Self::with_settings(device, color_format, samples, None)
    }
    /// As `with_samples`, with cascaded sun shadows (None disables them).
    /// Call `render_shadows` before the world pass each frame.
    pub fn with_settings(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        samples: u32,
        shadows: Option<crate::shadow::ShadowSettings>,
    ) -> Self {
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene camera"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                sampler_entry(2),
                sampler_entry(3),
                sampler_entry(4),
                sampler_entry(5),
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let mut entries = vec![];
        for binding in 0..13 {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 15,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene material"),
            entries: &entries,
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene"),
            bind_group_layouts: &[Some(&camera_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world-space scene"),
            source: wgpu::ShaderSource::Wgsl(
                crate::color::shader_source(include_str!("scene.wgsl")).into(),
            ),
        });
        let mut pipelines = vec![];
        for background in [false, true] {
            for blend in 0..3 {
                let blend_state = match blend {
                    0 => None,
                    1 => Some(wgpu::BlendState::ALPHA_BLENDING),
                    _ => Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                };
                for double_sided in [false, true] {
                    pipelines.push(device.create_render_pipeline(
                        &wgpu::RenderPipelineDescriptor {
                            label: Some("persistent scene"),
                            layout: Some(&layout),
                            vertex: wgpu::VertexState {
                                module: &shader,
                                entry_point: Some("vs_main"),
                                compilation_options: Default::default(),
                                buffers: &vertex_layouts(),
                            },
                            primitive: wgpu::PrimitiveState {
                                cull_mode: if double_sided {
                                    None
                                } else {
                                    Some(wgpu::Face::Back)
                                },
                                ..Default::default()
                            },
                            depth_stencil: Some(wgpu::DepthStencilState {
                                format: DEPTH_FORMAT,
                                depth_write_enabled: Some(blend == 0 && !background),
                                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                                stencil: Default::default(),
                                bias: Default::default(),
                            }),
                            multisample: wgpu::MultisampleState {
                                count: samples,
                                ..Default::default()
                            },
                            fragment: Some(wgpu::FragmentState {
                                module: &shader,
                                entry_point: Some("fs_main"),
                                compilation_options: wgpu::PipelineCompilationOptions {
                                    constants: &crate::color::output_constants(color_format),
                                    ..Default::default()
                                },
                                targets: &[Some(wgpu::ColorTargetState {
                                    format: color_format,
                                    blend: blend_state,
                                    write_mask: wgpu::ColorWrites::ALL,
                                })],
                            }),
                            multiview_mask: None,
                            cache: None,
                        },
                    ));
                }
            }
        }
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera uniform"),
            contents: bytemuck::bytes_of(&Camera::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let light_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("native point lights"),
            contents: &vec![0u8; 16 + MAX_POINT_LIGHTS * std::mem::size_of::<PointLight>()],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let filtering = TextureFiltering::default();
        let shadows =
            crate::shadow::ShadowMaps::new(device, shadows, &material_layout, &vertex_layouts());
        let camera_group = camera_group(
            device,
            &camera_layout,
            &camera_buffer,
            &light_buffer,
            filtering,
            &shadows,
        );
        Self {
            identity_instance: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("identity scene instance"),
                contents: bytemuck::bytes_of(&SceneTransform::default().record()),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            camera_buffer,
            light_buffer,
            camera_layout,
            camera_group,
            material_layout,
            pipelines,
            filtering,
            samples,
            shadows,
            eye: Vec3::ZERO,
            frustum: None,
        }
    }
    /// Upload once. Construct another GpuScene for dynamic bricks/characters;
    /// replacing that handle leaves the map buffers and textures untouched.
    pub fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &SceneData,
    ) -> Result<GpuScene> {
        data.validate()?;
        let limits = device.limits();
        ensure!(
            data.vertices.len() as u64 * std::mem::size_of::<SceneVertex>() as u64
                <= limits.max_buffer_size
                && data.indices.len() as u64 * 4 <= limits.max_buffer_size,
            "Scene buffer exceeds device limits"
        );
        let (vertices, indices) = geometry_buffers(device, &data.name, data);
        // Every image is mipmapped; lightmap and weight slots bind only the
        // base level so atlas sheets never blend neighbouring surfaces.
        let mut views = Vec::with_capacity(data.images.len());
        let mut base_views = Vec::with_capacity(data.images.len());
        for image in &data.images {
            ensure!(
                image.width <= limits.max_texture_dimension_2d
                    && image.height <= limits.max_texture_dimension_2d,
                "Texture {} exceeds device limits",
                image.label
            );
            let size = wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            };
            let levels = crate::mipmap::chain(image.width, image.height, &image.rgba, image.srgb);
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&image.label),
                size,
                mip_level_count: levels.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: if image.srgb {
                    wgpu::TextureFormat::Rgba8UnormSrgb
                } else {
                    wgpu::TextureFormat::Rgba8Unorm
                },
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for (level, (width, height, rgba)) in levels.iter().enumerate() {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level as u32,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    rgba.as_ref(),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(width * 4),
                        rows_per_image: Some(*height),
                    },
                    wgpu::Extent3d {
                        width: *width,
                        height: *height,
                        depth_or_array_layers: 1,
                    },
                );
            }
            views.push(texture.create_view(&Default::default()));
            base_views.push(texture.create_view(&wgpu::TextureViewDescriptor {
                mip_level_count: Some(1),
                ..Default::default()
            }));
        }
        let mut materials = vec![];
        for material in &data.materials {
            let mut parameters: [f32; 16] = [0.0; 16];
            parameters[..4].copy_from_slice(&[
                match material.kind {
                    MaterialKind::Surface => 0.0,
                    MaterialKind::Terrain => 1.0,
                    MaterialKind::VertexLit => 2.0,
                    MaterialKind::BrickOverlay => 3.0,
                    MaterialKind::Sky => 4.0,
                    MaterialKind::Cloud => 5.0,
                    MaterialKind::Water => 6.0,
                    MaterialKind::UnlitOverlay => 7.0,
                    MaterialKind::Unlit => 8.0,
                },
                match material.alpha {
                    AlphaMode::Mask(c) => c,
                    _ => 0.0,
                },
                if material.clamp_nearest { 1.0 } else { 0.0 },
                if material.temp_brick_flash { 1.0 } else { 0.0 },
            ]);
            if let Some(groups) = material.parameters {
                for (i, group) in groups.iter().enumerate() {
                    parameters[(i + 1) * 4..(i + 2) * 4].copy_from_slice(group);
                }
            }
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&material.name),
                contents: bytemuck::cast_slice(&parameters),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let mut entries: Vec<_> = material
                .images
                .iter()
                .enumerate()
                .map(|(i, image)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: wgpu::BindingResource::TextureView(if (8..=10).contains(&i) {
                        &base_views[*image]
                    } else {
                        &views[*image]
                    }),
                })
                .collect();
            entries.push(wgpu::BindGroupEntry {
                binding: 15,
                resource: buffer.as_entire_binding(),
            });
            materials.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&material.name),
                layout: &self.material_layout,
                entries: &entries,
            }));
        }
        Ok(GpuScene {
            material_descriptors: data.materials.clone(),
            image_signatures: image_signatures(data),
            vertices,
            indices,
            materials,
            batches: data.batches.clone(),
            material_modes: data
                .materials
                .iter()
                .map(|m| {
                    (
                        match m.alpha {
                            AlphaMode::Blend => 1,
                            AlphaMode::Additive => 2,
                            _ => 0,
                        },
                        m.double_sided,
                        matches!(m.kind, MaterialKind::Sky | MaterialKind::Cloud),
                        matches!(m.alpha, AlphaMode::Mask(_)),
                    )
                })
                .collect(),
            bounds: None,
            vertex_count: data.vertices.len(),
            index_count: data.indices.len(),
            image_count: data.images.len(),
        })
    }
    /// Upload one static world chunk whose batches index `palette`'s materials.
    /// Only geometry is created; textures and bind groups stay shared, and the
    /// chunk's bounds let the renderer skip it outside the view frustum.
    pub fn upload_chunk(
        &self,
        device: &wgpu::Device,
        data: &SceneData,
        palette: &GpuScene,
    ) -> Result<GpuScene> {
        data.validate_geometry()?;
        ensure!(
            data.materials == palette.material_descriptors,
            "Chunk geometry was built against a different material palette"
        );
        ensure!(
            data.vertices.len() as u64 * std::mem::size_of::<SceneVertex>() as u64
                <= device.limits().max_buffer_size
                && data.indices.len() as u64 * 4 <= device.limits().max_buffer_size,
            "Chunk buffer exceeds device limits"
        );
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in &data.vertices {
            min = min.min(Vec3::from(vertex.position));
            max = max.max(Vec3::from(vertex.position));
        }
        // Brick shape FX displace vertices in the shader by up to 0.1 units.
        let margin = Vec3::splat(0.2);
        let (vertices, indices) = geometry_buffers(device, &data.name, data);
        Ok(GpuScene {
            vertices,
            indices,
            materials: palette.materials.clone(),
            material_modes: palette.material_modes.clone(),
            material_descriptors: palette.material_descriptors.clone(),
            image_signatures: palette.image_signatures.clone(),
            batches: data.batches.clone(),
            bounds: (!data.vertices.is_empty()).then_some((min - margin, max + margin)),
            vertex_count: data.vertices.len(),
            index_count: data.indices.len(),
            image_count: palette.image_count,
        })
    }
    /// Upload changed animation geometry while sharing the original material
    /// bind groups and textures. A foreign/recolored binding table is rejected.
    pub fn upload_geometry_shared(
        &self,
        device: &wgpu::Device,
        data: &SceneData,
        base: &GpuScene,
    ) -> Result<GpuScene> {
        data.validate()?;
        ensure!(
            data.materials == base.material_descriptors
                && image_signatures(data) == base.image_signatures,
            "Shared geometry changed immutable material/image bindings"
        );
        ensure!(
            data.vertices.len() as u64 * std::mem::size_of::<SceneVertex>() as u64
                <= device.limits().max_buffer_size
                && data.indices.len() as u64 * 4 <= device.limits().max_buffer_size,
            "Shared scene buffer exceeds device limits"
        );
        Ok(GpuScene {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("shared-material posed vertices"),
                contents: bytemuck::cast_slice(&data.vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("shared-material posed indices"),
                contents: bytemuck::cast_slice(&data.indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            materials: base.materials.clone(),
            material_modes: base.material_modes.clone(),
            material_descriptors: base.material_descriptors.clone(),
            image_signatures: base.image_signatures.clone(),
            batches: data.batches.clone(),
            bounds: None,
            vertex_count: data.vertices.len(),
            index_count: data.indices.len(),
            image_count: base.image_count,
        })
    }
    pub fn samples(&self) -> u32 {
        self.samples
    }
    pub fn shadow_settings(&self) -> Option<crate::shadow::ShadowSettings> {
        self.shadows.settings
    }
    pub fn filtering(&self) -> TextureFiltering {
        self.filtering
    }
    /// Texture filtering is sampler state shared by every uploaded scene, so
    /// changing it rebuilds only the camera bind group.
    pub fn set_filtering(&mut self, device: &wgpu::Device, filtering: TextureFiltering) {
        if filtering != self.filtering {
            self.filtering = filtering;
            self.camera_group = camera_group(
                device,
                &self.camera_layout,
                &self.camera_buffer,
                &self.light_buffer,
                filtering,
                &self.shadows,
            );
        }
    }
    /// Call once before encoding/submitting a frame. Multiple writes before a
    /// single submission would intentionally use the latest camera everywhere.
    pub fn update_camera(&mut self, queue: &wgpu::Queue, camera: &Camera) {
        self.eye = Vec3::new(camera.eye[0], camera.eye[1], camera.eye[2]);
        let view_projection = Mat4::from_cols_array(&camera.view_projection);
        self.frustum = Some(frustum_planes(view_projection));
        self.shadows.update(
            queue,
            view_projection,
            self.eye,
            Vec4::from(camera.sun_direction).truncate(),
        );
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(camera));
    }
    /// Validate before writing, including an empty update to clear the previous frame.
    pub fn update_lights(&self, queue: &wgpu::Queue, lights: &[PointLight]) -> Result<()> {
        ensure!(
            lights.len() <= MAX_POINT_LIGHTS,
            "Point light budget exceeded"
        );
        for light in lights {
            ensure!(
                light
                    .position_radius
                    .iter()
                    .chain(light.color.iter())
                    .all(|v| v.is_finite())
                    && light.position_radius[3] >= 0.
                    && light.color.iter().all(|v| *v >= 0.),
                "Invalid point light"
            );
        }
        queue.write_buffer(
            &self.light_buffer,
            0,
            bytemuck::cast_slice(&[lights.len() as u32, 0, 0, 0]),
        );
        if !lights.is_empty() {
            queue.write_buffer(&self.light_buffer, 16, bytemuck::cast_slice(lights));
        }
        Ok(())
    }
    /// Render sun shadow casters (bricks, players, vehicles, items; never map
    /// interiors or terrain, see `crate::shadow`) for the camera last passed
    /// to `update_camera`. Only opaque and alpha-masked, non-background
    /// materials cast. Without shadows this records nothing.
    ///
    /// Occluders (bricks that do not cast, interiors, terrain) render into a
    /// separate map that only stops shadows from passing through them.
    pub fn render_shadows(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        casters: ShadowCasters<'_>,
        occluders: ShadowCasters<'_>,
    ) {
        let cascades = &self.shadows.cascades;
        for (group, casters) in [casters, occluders].iter().enumerate() {
            for (index, cascade) in cascades.iter().enumerate() {
                let planes = frustum_planes(cascade.view_projection);
                let layer = group * cascades.len() + index;
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sun shadow cascade"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.shadows.layer_views[layer],
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                // Occluders read this cascade's finished caster layer.
                let (bind_group, pipelines) = if group == 0 {
                    (&self.shadows.caster_group, &self.shadows.pipelines)
                } else {
                    (
                        &self.shadows.occluder_groups[index],
                        &self.shadows.occluder_pipelines,
                    )
                };
                pass.set_bind_group(
                    0,
                    bind_group,
                    &[crate::shadow::ShadowMaps::caster_offset(index)],
                );
                let mut draw = |scene: &GpuScene, buffer: &wgpu::Buffer, range: Range<u32>| {
                    if let Some(bounds) = scene.bounds
                        && !aabb_visible(&planes, bounds)
                    {
                        return;
                    }
                    pass.set_vertex_buffer(0, scene.vertices.slice(..));
                    pass.set_vertex_buffer(1, buffer.slice(..));
                    pass.set_index_buffer(scene.indices.slice(..), wgpu::IndexFormat::Uint32);
                    // Adjacent opaque batches (a chunk's coalesced materials)
                    // share one draw; masked batches bind their material.
                    let mut run: Option<Range<u32>> = None;
                    let flush = |pass: &mut wgpu::RenderPass<'_>, run: &mut Option<Range<u32>>| {
                        if let Some(indices) = run.take() {
                            pass.set_pipeline(&pipelines[0]);
                            pass.draw_indexed(indices, 0, range.clone());
                        }
                    };
                    for batch in &scene.batches {
                        let (blend, _, background, masked) = scene.material_modes[batch.material];
                        if blend != 0 || background {
                            continue;
                        }
                        if masked {
                            flush(&mut pass, &mut run);
                            pass.set_pipeline(&pipelines[1]);
                            pass.set_bind_group(1, &scene.materials[batch.material], &[]);
                            pass.draw_indexed(batch.indices.clone(), 0, range.clone());
                        } else if let Some(indices) =
                            run.as_mut().filter(|r| r.end == batch.indices.start)
                        {
                            indices.end = batch.indices.end;
                        } else {
                            flush(&mut pass, &mut run);
                            run = Some(batch.indices.clone());
                        }
                    }
                    flush(&mut pass, &mut run);
                };
                for &scene in casters.scenes {
                    draw(scene, &self.identity_instance, 0..1);
                }
                for &(scene, instances) in casters.instances {
                    // Fading copies stop casting once they turn translucent.
                    let solid = instances.transforms.iter().all(|t| t.tint[3] == 1.);
                    if !instances.is_empty() && solid {
                        draw(scene, &instances.buffer, 0..instances.len() as u32);
                    } else {
                        for (i, transform) in instances.transforms.iter().enumerate() {
                            if transform.tint[3] == 1. {
                                draw(scene, &instances.buffer, i as u32..i as u32 + 1);
                            }
                        }
                    }
                }
            }
        }
    }
    /// Clear starts a world frame; None loads existing color/depth for another
    /// scene pass. Native UI can render afterward with color LoadOp::Load.
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        scenes: &[&GpuScene],
        clear: Option<wgpu::Color>,
    ) {
        self.render_with_instances(encoder, color, depth, scenes, &[], clear);
    }
    /// Rigid objects share geometry/materials. Opaque groups use one instanced
    /// draw per mesh batch. Blended/fading objects are sorted with ordinary
    /// world batches using transformed centers, never just their model pivots.
    #[allow(clippy::too_many_arguments)] // same render attachments plus shared model groups
    pub fn render_with_instances(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        scenes: &[&GpuScene],
        instances: &[(&GpuScene, &GpuInstances)],
        clear: Option<wgpu::Color>,
    ) {
        struct Draw<'a> {
            scene: &'a GpuScene,
            batch: &'a MeshBatch,
            buffer: &'a wgpu::Buffer,
            range: Range<u32>,
            center: Vec3,
            blend: usize,
        }
        let mut order = Vec::new();
        for &scene in scenes {
            if let (Some(frustum), Some(bounds)) = (&self.frustum, scene.bounds)
                && !aabb_visible(frustum, bounds)
            {
                continue;
            }
            for batch in &scene.batches {
                order.push(Draw {
                    scene,
                    batch,
                    buffer: &self.identity_instance,
                    range: 0..1,
                    center: Vec3::from(batch.center),
                    blend: scene.material_modes[batch.material].0,
                });
            }
        }
        for &(scene, instances) in instances {
            if instances.is_empty() {
                continue;
            }
            let all_opaque = instances.transforms.iter().all(|t| t.tint[3] == 1.);
            for batch in &scene.batches {
                let blend = scene.material_modes[batch.material].0;
                if blend == 0 && all_opaque {
                    order.push(Draw {
                        scene,
                        batch,
                        buffer: &instances.buffer,
                        range: 0..instances.len() as u32,
                        center: Vec3::ZERO,
                        blend,
                    });
                } else {
                    for (i, transform) in instances.transforms.iter().enumerate() {
                        if transform.tint[3] == 0. {
                            continue;
                        }
                        order.push(Draw {
                            scene,
                            batch,
                            buffer: &instances.buffer,
                            range: i as u32..i as u32 + 1,
                            center: transform
                                .transform
                                .transform_point3(Vec3::from(batch.center)),
                            blend: if transform.tint[3] < 1. && blend == 0 {
                                1
                            } else {
                                blend
                            },
                        });
                    }
                }
            }
        }
        order.sort_by(|a, b| {
            let sky_a = a.scene.material_modes[a.batch.material].2;
            let sky_b = b.scene.material_modes[b.batch.material].2;
            if sky_a != sky_b {
                return sky_b.cmp(&sky_a);
            }
            if sky_a {
                return std::cmp::Ordering::Equal;
            } // authored sky/cloud/band order
            let aa = a.blend != 0;
            let ba = b.blend != 0;
            aa.cmp(&ba).then_with(|| {
                if aa {
                    b.center
                        .distance_squared(self.eye)
                        .total_cmp(&a.center.distance_squared(self.eye))
                } else {
                    std::cmp::Ordering::Equal
                }
            })
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("persistent world scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: clear.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: if clear.is_some() {
                        wgpu::LoadOp::Clear(1.0)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.camera_group, &[]);
        for draw in order {
            let (scene, batch) = (draw.scene, draw.batch);
            let (_, double_sided, background, _) = scene.material_modes[batch.material];
            pass.set_pipeline(
                &self.pipelines
                    [usize::from(background) * 6 + draw.blend * 2 + usize::from(double_sided)],
            );
            pass.set_bind_group(1, &scene.materials[batch.material], &[]);
            pass.set_vertex_buffer(0, scene.vertices.slice(..));
            pass.set_vertex_buffer(1, draw.buffer.slice(..));
            pass.set_index_buffer(scene.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(batch.indices.clone(), 0, draw.range);
        }
    }
}

/// Recreate only this attachment when the viewport changes.
pub fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    create_depth_samples(device, width, height, 1)
}
pub fn create_depth_samples(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    samples: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}
