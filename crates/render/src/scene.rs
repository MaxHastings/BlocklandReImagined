//! Persistent world-space rendering on the caller's device. The caller owns
//! the swapchain/offscreen attachment, encoder and submission, so UI passes can
//! follow this pass without another adapter/device or scene re-upload.
use anyhow::{Context, Result, ensure};
use glam::{Mat4, Vec3, Vec4};
use std::{ops::Range, sync::Arc};
use wgpu::util::DeviceExt;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Bytes of one `DrawIndexedIndirectArgs`.
const INDIRECT_ARGS_SIZE: u64 = 20;

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
    /// Every brick surface in one material, so a chunk draws its bricks in
    /// one batch instead of one per surface image: diffuse slots 0..4 hold
    /// TOP, SIDE, BOTTOMEDGE, BOTTOMLOOP and RAMP (each a `BrickOverlay`,
    /// SIDE clamped like `clamp_nearest`), slot 5 a white image for unprinted
    /// print faces (a `VertexLit` surface). Each vertex names its slot in
    /// `lightmap_uv.x`, which brick materials do not otherwise read.
    BrickSurfaces,
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
    /// Texture alpha neither blends nor discards. Torque draws DTS materials
    /// without the Translucent flag with blending and alpha test off
    /// (`TSMesh::setMaterial`), so texels whose alpha is zero still show their
    /// colour: the Sharp_Trees frond stems sample such texels.
    pub ignore_texture_alpha: bool,
    /// Kind-specific uniforms, required for water and terrain only.
    /// Water: flow/wave/opacity, distortion/depth flag, surface+shore
    /// tiling/reflection/parallax. Terrain: see `terrain_scene::parameters`.
    pub parameters: Option<[[f32; 4]; 4]>,
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
            ignore_texture_alpha: false,
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
        self.append_validated_brick_hiding(mesh, transform, paint, surface_materials, fx, 0)
    }
    /// As `append_validated_brick_with_fx`, leaving out the quads of the
    /// faces set in `hidden` (bit `i` for `bri_content::brick::FACES[i]`,
    /// top to west): faces neighbours cover (v20 BLB COVERAGE).
    pub fn append_validated_brick_hiding(
        &mut self,
        mesh: &bri_content::brick::Brick,
        transform: [f32; 16],
        paint: [f32; 4],
        surface_materials: [usize; 6],
        fx: BrickFx,
        hidden: u8,
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
                    MaterialKind::VertexLit
                        | MaterialKind::BrickOverlay
                        | MaterialKind::BrickSurfaces
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
        // v20 keeps brickTOP world-aligned: for TOP quads its grid-brick quad
        // emitter (0x52f7fc..0x52fc0f, texture slot 0 only) turns the datablock
        // UV by the brick's angle ID, (v,-u), (u,v), (-v,u), (-u,-v) for angles
        // 0..3. Without it neighbouring bricks at other angles disagree on
        // which bevel edges are lit. The angle is the placement's quarter turn
        // about the vertical (clockwise from above, as `quarter_turns`).
        let x_axis = placement.transform_vector3(Vec3::X);
        let angle = (x_axis.z.atan2(x_axis.x) / std::f32::consts::FRAC_PI_2)
            .round()
            .rem_euclid(4.0) as u8;
        let top_uv = |[u, v]: [f32; 2]| match angle {
            0 => [v, -u],
            1 => [u, v],
            2 => [-v, u],
            _ => [-u, -v],
        };
        let mut groups = std::collections::BTreeMap::<usize, Vec<u32>>::new();
        let mut blend_materials = std::collections::BTreeMap::new();
        let mut provisional_color = false;
        for quad in &mesh.quads {
            if hidden != 0 {
                use bri_content::brick::Face;
                let bit = match quad.face {
                    Face::Top => 1,
                    Face::Bottom => 2,
                    Face::North => 4,
                    Face::East => 8,
                    Face::South => 16,
                    Face::West => 32,
                    Face::Omni => 0,
                };
                if hidden & bit != 0 {
                    continue;
                }
            }
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
                    uv: if quad.surface == Surface::Top {
                        top_uv(vertex.uv)
                    } else {
                        vertex.uv
                    },
                    // The surface slot, for `MaterialKind::BrickSurfaces`.
                    lightmap_uv: [slot as f32, 0.],
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
            // Water and terrain need their uniforms; a temp brick may carry
            // its flash (`temp_brick_flash`); nothing else has any.
            let wants = matches!(material.kind, MaterialKind::Water | MaterialKind::Terrain);
            ensure!(
                (material.parameters.is_some() == wants
                    || (material.temp_brick_flash && !wants))
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
                                MaterialKind::VertexLit
                                    | MaterialKind::BrickOverlay
                                    | MaterialKind::BrickSurfaces
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
    /// A camera looking along `forward` with its own `up`, so a view at or
    /// past straight up or down keeps its roll (the player camera turns by
    /// yaw then pitch, as Torque's eye transform does).
    pub fn oriented(
        eye: [f32; 3],
        forward: [f32; 3],
        up: [f32; 3],
        aspect: f32,
        fov_y: f32,
        near: f32,
        far: f32,
    ) -> Self {
        let forward = Vec3::from(forward).normalize_or_zero();
        let up = Vec3::from(up).normalize_or_zero();
        if forward.length_squared() < 0.5
            || up.length_squared() < 0.5
            || forward.cross(up).length_squared() < 1e-6
        {
            let target = Vec3::from(eye) + forward;
            return Self::perspective(eye, target.to_array(), aspect, fov_y, near, far);
        }
        let view = glam::camera::rh::view::look_to_mat4(Vec3::from(eye), forward, up);
        let projection = glam::camera::rh::proj::directx::perspective(fov_y, aspect, near, far);
        Self {
            view_projection: (projection * view).to_cols_array(),
            ..Self::perspective(eye, [eye[0], eye[1], eye[2] - 1.0], aspect, fov_y, near, far)
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
    /// (blend, double sided, sky/cloud background, alpha mask, water plane)
    material_modes: Vec<(usize, bool, bool, bool, bool)>,
    /// World-space bounds of static chunk geometry; unbounded scenes always draw.
    bounds: Option<(Vec3, Vec3)>,
    /// A pooled chunk's place in its shared block (`crate::pool`); its
    /// indices are chunk-local, drawn from `first_index` at `base_vertex`.
    /// Zero for scenes with buffers of their own.
    slot: Option<Arc<crate::pool::Slot>>,
    base_vertex: i32,
    first_index: u32,
    /// A pooled chunk's translucent batches, in the translucent pool.
    translucent: Option<Box<GpuScene>>,
    /// Where the source `SceneData`'s vertices landed in this scene: runs of
    /// source vertices and the local vertex each starts at, for
    /// `hide_vertices`. Empty when every source vertex is at its own index.
    vertex_runs: Vec<(Range<u32>, u32)>,
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
    /// A batch's index range in the scene's (possibly shared) index buffer.
    fn index_range(&self, batch: &Range<u32>) -> Range<u32> {
        self.first_index + batch.start..self.first_index + batch.end
    }
    /// Stop drawing the source vertices `range` (one brick of a chunk) now,
    /// without a rebuild: they collapse to one point, so their triangles
    /// cover nothing, shadows included. A later upload of the scene draws
    /// them again.
    pub fn hide_vertices(&self, queue: &wgpu::Queue, range: Range<u32>) {
        const HIDDEN: SceneVertex = SceneVertex {
            position: [0.; 3],
            normal: [0.; 3],
            uv: [0.; 2],
            lightmap_uv: [0.; 2],
            color: [0.; 4],
            fx: [0.; 4],
        };
        let size = std::mem::size_of::<SceneVertex>() as u64;
        for local in local_spans(&self.vertex_runs, self.vertex_count as u32, range.clone()) {
            let first = self.base_vertex as u64 + u64::from(local.start);
            let hidden = vec![HIDDEN; local.len()];
            queue.write_buffer(&self.vertices, first * size, bytemuck::cast_slice(&hidden));
        }
        if let Some(translucent) = &self.translucent {
            translucent.hide_vertices(queue, range);
        }
    }
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
        // Every float of every vertex (fx included), as one flat slice: a
        // dynamic scene re-sends its vertices every frame.
        ensure!(
            bytemuck::cast_slice::<SceneVertex, f32>(vertices)
                .iter()
                .all(|x| x.is_finite())
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
            slot: self.slot.clone(),
            base_vertex: self.base_vertex,
            first_index: self.first_index,
            translucent: None,
            vertex_runs: self.vertex_runs.clone(),
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

/// A chunk's geometry split in two by `second`: each part keeps only the
/// vertices its batches use, re-indexed from zero.
/// Where source vertices `range` are in a scene with these `runs` (empty:
/// every source vertex at its own index, `count` of them): local spans.
fn local_spans(runs: &[(Range<u32>, u32)], count: u32, range: Range<u32>) -> Vec<Range<u32>> {
    let identity = [(0..count, 0)];
    let runs = if runs.is_empty() { &identity[..] } else { runs };
    runs.iter()
        .filter_map(|(run, local)| {
            let (start, end) = (range.start.max(run.start), range.end.min(run.end));
            (start < end).then(|| local + (start - run.start)..local + (end - run.start))
        })
        .collect()
}

/// One part of a split scene: vertices, indices, batches and vertex runs.
type SplitPart = (Vec<SceneVertex>, Vec<u32>, Vec<MeshBatch>, Vec<(Range<u32>, u32)>);

/// Split a scene's batches into two parts with vertices of their own. Each
/// part keeps its vertices in source order, so one brick's vertices stay
/// together; the runs map source vertices to the part's (see
/// `GpuScene::vertex_runs`).
fn split_batches(data: &SceneData, second: impl Fn(&MeshBatch) -> bool) -> [SplitPart; 2] {
    let mut parts: [SplitPart; 2] = Default::default();
    let mut remap = [
        vec![u32::MAX; data.vertices.len()],
        vec![u32::MAX; data.vertices.len()],
    ];
    let range = |batch: &MeshBatch| batch.indices.start as usize..batch.indices.end as usize;
    for batch in &data.batches {
        let p = usize::from(second(batch));
        for &index in &data.indices[range(batch)] {
            remap[p][index as usize] = 0;
        }
    }
    for (p, (vertices, _, _, runs)) in parts.iter_mut().enumerate() {
        for (source, mapped) in remap[p].iter_mut().enumerate() {
            if *mapped == u32::MAX {
                continue;
            }
            let (source, local) = (source as u32, vertices.len() as u32);
            *mapped = local;
            vertices.push(data.vertices[source as usize]);
            match runs.last_mut() {
                Some((run, start))
                    if run.end == source && *start + (run.end - run.start) == local =>
                {
                    run.end += 1;
                }
                _ => runs.push((source..source + 1, local)),
            }
        }
    }
    for batch in &data.batches {
        let p = usize::from(second(batch));
        let (_, indices, batches, _) = &mut parts[p];
        let start = indices.len() as u32;
        indices.extend(
            data.indices[range(batch)]
                .iter()
                .map(|&index| remap[p][index as usize]),
        );
        batches.push(MeshBatch {
            indices: start..indices.len() as u32,
            material: batch.material,
            center: batch.center,
        });
    }
    parts
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
/// Back-to-front order for translucent draws, each a centre and, for a
/// water surface, the height of its horizontal plane. Torque sorts water
/// blocks as planes (`SceneRenderImage::Plane`), not points: a plane is drawn
/// after everything beyond it and before everything on the camera's side,
/// and stacked planes go farthest first. So an ocean covers the sand layer
/// under it wherever their strips' centres happen to lie.
pub fn translucent_order(eye: Vec3, draws: &[(Vec3, Option<f32>)]) -> Vec<usize> {
    let mut planes: Vec<f32> = draws.iter().filter_map(|d| d.1).collect();
    planes.sort_by(|a, b| (a - eye.y).abs().total_cmp(&(b - eye.y).abs()));
    planes.dedup();
    // Planes between a point and the camera; a plane's rank counts itself.
    let level = |(center, plane): &(Vec3, Option<f32>)| match plane {
        Some(h) => planes.iter().position(|p| p == h).unwrap_or(0) * 2 + 1,
        None => {
            let behind = |p: &f32| (center.y - p) * (eye.y - p) < 0.0;
            match planes.iter().rposition(behind) {
                Some(i) => i * 2 + 2,
                None => 0,
            }
        }
    };
    let mut order: Vec<usize> = (0..draws.len()).collect();
    order.sort_by(|&a, &b| {
        level(&draws[b]).cmp(&level(&draws[a])).then_with(|| {
            draws[b]
                .0
                .distance_squared(eye)
                .total_cmp(&draws[a].0.distance_squared(eye))
        })
    });
    order
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
///
/// v20 reads Use Sharp Filter (`gUseGLNearest`, 0x8705e0) only in its two
/// brick draw paths (0x52ceb0, 0x4c7b40), for brick textures that are not
/// smooth-filtered: brickSIDE. Those always magnify nearest; sharp switches
/// their minification from `GL_NEAREST_MIPMAP_LINEAR` to `GL_NEAREST` with
/// anisotropy turned down. The texture manager, which filters interiors,
/// terrain, shapes and skies, never reads it, so map textures stay smooth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureFiltering {
    /// Blend between mip levels instead of snapping to the nearest one.
    pub trilinear: bool,
    /// `useGLNearest`: brickSIDE minifies to the nearest base-level texel.
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
    /// Diffuse sampler state: repeating and clamped images, then brickSIDE
    /// (which the shader snaps to texel centres for nearest magnification).
    /// Only brickSIDE follows `sharp`, sampling its nearest base-level texel.
    pub fn diffuse_samplers(self) -> [wgpu::SamplerDescriptor<'static>; 3] {
        // Anisotropy requires linear filtering throughout.
        let anisotropic = self.anisotropy > 1 && self.trilinear;
        let filtered = |address_mode| wgpu::SamplerDescriptor {
            address_mode_u: address_mode,
            address_mode_v: address_mode,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: if self.trilinear {
                wgpu::MipmapFilterMode::Linear
            } else {
                wgpu::MipmapFilterMode::Nearest
            },
            anisotropy_clamp: if anisotropic { self.anisotropy } else { 1 },
            ..Default::default()
        };
        let side = if self.sharp {
            wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                lod_max_clamp: 0.0,
                ..Default::default()
            }
        } else {
            filtered(wgpu::AddressMode::ClampToEdge)
        };
        [
            filtered(wgpu::AddressMode::Repeat),
            filtered(wgpu::AddressMode::ClampToEdge),
            side,
        ]
    }
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
    volume: &VolumeBinding,
) -> wgpu::BindGroup {
    let [tiled, clamped, side] = filtering.diffuse_samplers();
    let exact = |address_mode| wgpu::SamplerDescriptor {
        address_mode_u: address_mode,
        address_mode_v: address_mode,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    };
    let samplers = [
        tiled,
        clamped,
        exact(wgpu::AddressMode::Repeat),
        exact(wgpu::AddressMode::ClampToEdge),
    ]
    .map(|d| device.create_sampler(&d));
    let side = device.create_sampler(&side);
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
        wgpu::BindGroupEntry {
            binding: 10,
            resource: wgpu::BindingResource::TextureView(&volume.view),
        },
        wgpu::BindGroupEntry {
            binding: 11,
            resource: volume.parameters.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 12,
            resource: wgpu::BindingResource::Sampler(&side),
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

/// The light volume texture and its placement: origin and cell size, then
/// dimensions and 1 when enabled (an empty 1x1x1 volume is bound otherwise).
struct VolumeBinding {
    view: wgpu::TextureView,
    parameters: wgpu::Buffer,
}
impl VolumeBinding {
    fn new(
        device: &wgpu::Device,
        volume: Option<(&wgpu::Queue, &crate::light_volume::LightVolume)>,
    ) -> Self {
        let dims = volume.map_or([1; 3], |(_, v)| v.dims);
        let size = wgpu::Extent3d {
            width: dims[0],
            height: dims[1],
            depth_or_array_layers: dims[2],
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("light volume"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut parameters = [0f32; 8];
        if let Some((queue, volume)) = volume {
            queue.write_texture(
                texture.as_image_copy(),
                bytemuck::cast_slice(&volume.texels),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(dims[0] * 4),
                    rows_per_image: Some(dims[1]),
                },
                size,
            );
            parameters = [
                volume.origin[0],
                volume.origin[1],
                volume.origin[2],
                volume.cell,
                dims[0] as f32,
                dims[1] as f32,
                dims[2] as f32,
                1.0,
            ];
        }
        Self {
            view: texture.create_view(&Default::default()),
            parameters: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("light volume placement"),
                contents: bytemuck::cast_slice(&parameters),
                usage: wgpu::BufferUsages::UNIFORM,
            }),
        }
    }
}
/// Native unshadowed point illumination. Radius and RGB come from the effect clock.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PointLight {
    pub position_radius: [f32; 4],
    pub color: [f32; 4],
}

/// What the last frame's world and shadow passes recorded. Counts, unlike
/// times, do not change with the load on the machine, so they make stable
/// regression checks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct RenderStats {
    /// Indexed draws in the world pass.
    pub draws: u32,
    /// Pipeline, bind group and buffer binds in the world pass.
    pub binds: u32,
    /// Bounded scenes (chunks) drawn and skipped outside the view.
    pub scenes_drawn: u32,
    pub scenes_culled: u32,
    /// Indexed draws and binds across every shadow cascade.
    pub shadow_draws: u32,
    pub shadow_binds: u32,
    /// Triangles submitted by the world pass.
    pub triangles: u64,
    /// World-pass batches of blended geometry, sorted back to front.
    pub translucent_draws: u32,
    /// Chunk batches drawn through indirect multi-draws (counted in `draws`
    /// once per multi-draw), in the world and shadow passes.
    pub batched: u32,
    pub shadow_batched: u32,
}

/// The pass state last bound, so repeated binds are skipped.
#[derive(Default)]
struct Bound<'a> {
    pipeline: Option<&'a wgpu::RenderPipeline>,
    material: Option<&'a wgpu::BindGroup>,
    vertices: Option<&'a wgpu::Buffer>,
    instances: Option<&'a wgpu::Buffer>,
    indices: Option<&'a wgpu::Buffer>,
    binds: u32,
}
impl<'a> Bound<'a> {
    fn pipeline(&mut self, pass: &mut wgpu::RenderPass<'_>, pipeline: &'a wgpu::RenderPipeline) {
        if self.pipeline != Some(pipeline) {
            pass.set_pipeline(pipeline);
            self.pipeline = Some(pipeline);
            self.binds += 1;
        }
    }
    fn material(&mut self, pass: &mut wgpu::RenderPass<'_>, group: &'a wgpu::BindGroup) {
        if self.material != Some(group) {
            pass.set_bind_group(1, group, &[]);
            self.material = Some(group);
            self.binds += 1;
        }
    }
    fn geometry(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        vertices: &'a wgpu::Buffer,
        instances: &'a wgpu::Buffer,
        indices: &'a wgpu::Buffer,
    ) {
        if self.vertices != Some(vertices) {
            pass.set_vertex_buffer(0, vertices.slice(..));
            self.vertices = Some(vertices);
            self.binds += 1;
        }
        if self.instances != Some(instances) {
            pass.set_vertex_buffer(1, instances.slice(..));
            self.instances = Some(instances);
            self.binds += 1;
        }
        if self.indices != Some(indices) {
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            self.indices = Some(indices);
            self.binds += 1;
        }
    }
}

pub struct SceneRenderer {
    identity_instance: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    light_buffer: wgpu::Buffer,
    volume: VolumeBinding,
    camera_layout: wgpu::BindGroupLayout,
    camera_group: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    pipelines: Vec<wgpu::RenderPipeline>,
    filtering: TextureFiltering,
    samples: u32,
    shadows: crate::shadow::ShadowMaps,
    eye: Vec3,
    frustum: Option<[glam::Vec4; 6]>,
    stats: std::cell::Cell<RenderStats>,
    /// Shared buffers for static chunks (`upload_chunk`): opaque batches,
    /// and translucent ones apart.
    pool: crate::pool::GeometryPool,
    translucent_pool: crate::pool::GeometryPool,
    device: wgpu::Device,
    /// The queue from the last `update_camera`, for this frame's indirect
    /// draw arguments, and where in `indirect` the frame has written.
    queue: std::cell::RefCell<Option<wgpu::Queue>>,
    indirect: std::cell::RefCell<(Option<wgpu::Buffer>, u64)>,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 10,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 11,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                sampler_entry(12),
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
        let volume = VolumeBinding::new(device, None);
        let camera_group = camera_group(
            device,
            &camera_layout,
            &camera_buffer,
            &light_buffer,
            filtering,
            &shadows,
            &volume,
        );
        Self {
            identity_instance: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("identity scene instance"),
                contents: bytemuck::bytes_of(&SceneTransform::default().record()),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            camera_buffer,
            light_buffer,
            volume,
            camera_layout,
            camera_group,
            material_layout,
            pipelines,
            filtering,
            samples,
            shadows,
            eye: Vec3::ZERO,
            frustum: None,
            stats: Default::default(),
            pool: Default::default(),
            translucent_pool: Default::default(),
            device: device.clone(),
            queue: Default::default(),
            indirect: Default::default(),
        }
    }
    /// Before uploading many chunks at once (a load): room for all of them
    /// in one shared block per pool.
    pub fn reserve_chunks(&self, chunks: &[&SceneData]) -> Result<()> {
        let mut totals = [[0u64; 2]; 2];
        for chunk in chunks {
            for batch in &chunk.batches {
                let clear = matches!(
                    chunk.materials[batch.material].alpha,
                    AlphaMode::Blend | AlphaMode::Additive
                );
                let count = u64::from(batch.indices.end - batch.indices.start);
                // Quads: four vertices for six indices.
                totals[usize::from(clear)][0] += count * 2 / 3;
                totals[usize::from(clear)][1] += count;
            }
        }
        self.pool.reserve(&self.device, totals[0][0], totals[0][1])?;
        self.translucent_pool
            .reserve(&self.device, totals[1][0], totals[1][1])
    }
    /// Shared chunk geometry: blocks allocated and their bytes.
    pub fn pool_usage(&self) -> (usize, u64) {
        let (a, b) = (self.pool.usage(), self.translucent_pool.usage());
        (a.0 + b.0, a.1 + b.1)
    }
    /// Write indirect draw arguments for this frame; returns the buffer and
    /// the offset they start at, or None before any `update_camera`.
    fn upload_indirect(
        &self,
        args: &[wgpu::util::DrawIndexedIndirectArgs],
    ) -> Option<(wgpu::Buffer, u64)> {
        if args.is_empty() {
            return None;
        }
        let queue = self.queue.borrow().clone()?;
        let bytes: Vec<u8> = args.iter().flat_map(|a| a.as_bytes()).copied().collect();
        let mut indirect = self.indirect.borrow_mut();
        let (buffer, cursor) = &mut *indirect;
        let needed = *cursor + bytes.len() as u64;
        if buffer.as_ref().is_none_or(|b| b.size() < needed) {
            // Passes recorded earlier this frame keep the old buffer alive.
            *buffer = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("indirect draw arguments"),
                size: (bytes.len() as u64 * 2).next_power_of_two().max(1 << 16),
                usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            *cursor = 0;
        }
        let buffer = buffer.clone()?;
        let offset = *cursor;
        queue.write_buffer(&buffer, offset, &bytes);
        *cursor = (offset + bytes.len() as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
        Some((buffer, offset))
    }
    /// Counts from the passes recorded since the last `update_camera`.
    pub fn stats(&self) -> RenderStats {
        self.stats.get()
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
        // An alpha-tested image keeps its cut-out coverage at every mip level;
        // plain averaging thins leaves until distant crowns turn to sparse
        // stripes with only their blended soft edges left.
        let mut cutoffs = vec![None; data.images.len()];
        for material in &data.materials {
            if let AlphaMode::Mask(cutoff) = material.alpha {
                cutoffs[material.images[0]].get_or_insert(cutoff);
            }
        }
        for (image, cutoff) in data.images.iter().zip(cutoffs) {
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
            let levels = match cutoff {
                Some(cutoff) => crate::mipmap::chain_preserving_coverage(
                    image.width,
                    image.height,
                    &image.rgba,
                    image.srgb,
                    cutoff,
                ),
                None => crate::mipmap::chain(image.width, image.height, &image.rgba, image.srgb),
            };
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
            let mut parameters: [f32; 20] = [0.0; 20];
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
                    MaterialKind::BrickSurfaces => 9.0,
                },
                match material.alpha {
                    AlphaMode::Mask(c) => c,
                    _ => 0.0,
                },
                if material.clamp_nearest { 1.0 } else { 0.0 },
                // Flags: 1 temp-brick flash, 2 ignore texture alpha.
                f32::from(u8::from(material.temp_brick_flash) | u8::from(material.ignore_texture_alpha) << 1),
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
                        m.kind == MaterialKind::Water,
                    )
                })
                .collect(),
            bounds: None,
            slot: None,
            base_vertex: 0,
            first_index: 0,
            translucent: None,
            vertex_runs: Vec::new(),
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
        queue: &wgpu::Queue,
        data: &SceneData,
        palette: &GpuScene,
    ) -> Result<GpuScene> {
        self.upload_palette_geometry(device, Some(queue), data, palette)
    }
    /// A chunk goes into the shared pool (`queue` given) or buffers of its own.
    fn upload_palette_geometry(
        &self,
        device: &wgpu::Device,
        queue: Option<&wgpu::Queue>,
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
        let bounds = (!data.vertices.is_empty()).then_some((min - margin, max + margin));
        let Some(queue) = queue.filter(|_| !data.vertices.is_empty() && !data.indices.is_empty())
        else {
            let (vertices, indices) = geometry_buffers(device, &data.name, data);
            return Ok(self.palette_scene(
                palette,
                (vertices, indices, None),
                data.batches.clone(),
                bounds,
                (data.vertices.len(), data.indices.len()),
            ));
        };
        // Translucent batches live in a pool of their own: sorted back to
        // front across chunks, they draw in long runs only when they share
        // one buffer.
        let blended = |b: &MeshBatch| palette.material_modes[b.material].0 != 0;
        let [opaque, clear] = split_batches(data, blended);
        let mut parts = Vec::with_capacity(2);
        for (part, pool) in [(opaque, &self.pool), (clear, &self.translucent_pool)] {
            if part.1.is_empty() {
                continue;
            }
            let slot = pool.store(device, queue, &part.0, &part.1)?;
            let mut scene = self.palette_scene(
                palette,
                (
                    slot.block.vertices.clone(),
                    slot.block.indices.clone(),
                    Some(Arc::new(slot)),
                ),
                part.2,
                bounds,
                (part.0.len(), part.1.len()),
            );
            scene.vertex_runs = part.3;
            parts.push(scene);
        }
        let mut scene = parts.remove(0);
        scene.translucent = parts.pop().map(Box::new);
        Ok(scene)
    }
    /// A chunk scene on `palette`'s materials over the given buffers.
    fn palette_scene(
        &self,
        palette: &GpuScene,
        (vertices, indices, slot): (wgpu::Buffer, wgpu::Buffer, Option<Arc<crate::pool::Slot>>),
        batches: Vec<MeshBatch>,
        bounds: Option<(Vec3, Vec3)>,
        (vertex_count, index_count): (usize, usize),
    ) -> GpuScene {
        GpuScene {
            vertices,
            indices,
            base_vertex: slot.as_ref().map_or(0, |s| s.vertices.start as i32),
            first_index: slot.as_ref().map_or(0, |s| s.indices.start),
            slot,
            translucent: None,
            vertex_runs: Vec::new(),
            materials: palette.materials.clone(),
            material_modes: palette.material_modes.clone(),
            material_descriptors: palette.material_descriptors.clone(),
            image_signatures: palette.image_signatures.clone(),
            batches,
            bounds,
            vertex_count,
            index_count,
            image_count: palette.image_count,
        }
    }
    /// Upload one instanced model whose batches index `palette`'s materials:
    /// geometry only, like a chunk, but never culled by its model-space
    /// bounds, since its instances may be anywhere.
    pub fn upload_palette_model(
        &self,
        device: &wgpu::Device,
        data: &SceneData,
        palette: &GpuScene,
    ) -> Result<GpuScene> {
        let mut scene = self.upload_palette_geometry(device, None, data, palette)?;
        scene.bounds = None;
        Ok(scene)
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
        // A pose may hide every object (the spear's `fire` sequence while it
        // is thrown): the placeholder keeps the buffers non-empty, as upload
        // does, because wgpu panics on slicing an empty buffer.
        let (vertices, indices) = geometry_buffers(device, "shared-material posed vertices", data);
        Ok(GpuScene {
            vertices,
            indices,
            materials: base.materials.clone(),
            material_modes: base.material_modes.clone(),
            material_descriptors: base.material_descriptors.clone(),
            image_signatures: base.image_signatures.clone(),
            batches: data.batches.clone(),
            bounds: None,
            slot: None,
            base_vertex: 0,
            first_index: 0,
            translucent: None,
            vertex_runs: Vec::new(),
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
                &self.volume,
            );
        }
    }
    /// Baked interior light for vertex-lit surfaces (see `crate::light_volume`);
    /// None removes it. Rebuilds only the camera bind group.
    pub fn set_light_volume(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        volume: Option<&crate::light_volume::LightVolume>,
    ) -> Result<()> {
        if let Some(volume) = volume {
            ensure!(
                volume.dims.iter().all(|d| (1..=2048).contains(d))
                    && volume.texels.len()
                        == volume.dims.iter().map(|d| *d as usize).product::<usize>()
                    && volume.cell.is_finite()
                    && volume.cell > 0.0
                    && volume.origin.iter().all(|v| v.is_finite()),
                "Invalid light volume"
            );
        }
        self.volume = VolumeBinding::new(device, volume.map(|v| (queue, v)));
        self.camera_group = camera_group(
            device,
            &self.camera_layout,
            &self.camera_buffer,
            &self.light_buffer,
            self.filtering,
            &self.shadows,
            &self.volume,
        );
        Ok(())
    }
    /// Call once before encoding/submitting a frame. Multiple writes before a
    /// single submission would intentionally use the latest camera everywhere.
    pub fn update_camera(&mut self, queue: &wgpu::Queue, camera: &Camera) {
        self.eye = Vec3::new(camera.eye[0], camera.eye[1], camera.eye[2]);
        self.stats.set(RenderStats::default());
        // A new frame: its indirect arguments start over.
        *self.queue.borrow_mut() = Some(queue.clone());
        self.indirect.borrow_mut().1 = 0;
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
                // Everything this cascade draws, then recorded with repeated
                // binds skipped.
                let mut items: Vec<(&GpuScene, &wgpu::Buffer, Range<u32>)> = Vec::new();
                for &scene in casters.scenes {
                    items.push((scene, &self.identity_instance, 0..1));
                }
                for &(scene, instances) in casters.instances {
                    // Fading copies stop casting once they turn translucent.
                    let solid = instances.transforms.iter().all(|t| t.tint[3] == 1.);
                    if !instances.is_empty() && solid {
                        items.push((scene, &instances.buffer, 0..instances.len() as u32));
                    } else {
                        for (i, transform) in instances.transforms.iter().enumerate() {
                            if transform.tint[3] == 1. {
                                items.push((scene, &instances.buffer, i as u32..i as u32 + 1));
                            }
                        }
                    }
                }
                let mut bound = Bound::default();
                let mut draws = 0;
                // Opaque runs of pooled chunks, drawn per block with one
                // indirect multi-draw after the rest.
                let mut pooled: Vec<(&wgpu::Buffer, &wgpu::Buffer, wgpu::util::DrawIndexedIndirectArgs)> =
                    Vec::new();
                for (scene, buffer, range) in items {
                    // A pose can hide every object (the spear's `fire`
                    // sequence while it is thrown); wgpu panics on slicing
                    // the empty buffers.
                    if scene.vertex_count == 0 || scene.index_count == 0 {
                        continue;
                    }
                    if let Some(bounds) = scene.bounds
                        && !aabb_visible(&planes, bounds)
                    {
                        continue;
                    }
                    let indirect = scene.slot.is_some() && range == (0..1);
                    // Adjacent opaque batches (a chunk's coalesced materials)
                    // share one draw; masked batches bind their material.
                    let mut run: Option<Range<u32>> = None;
                    macro_rules! flush {
                        () => {
                            if let Some(indices) = run.take() {
                                if indirect {
                                    pooled.push((
                                        &scene.vertices,
                                        &scene.indices,
                                        wgpu::util::DrawIndexedIndirectArgs {
                                            index_count: indices.end - indices.start,
                                            instance_count: 1,
                                            first_index: scene.first_index + indices.start,
                                            base_vertex: scene.base_vertex,
                                            first_instance: 0,
                                        },
                                    ));
                                } else {
                                    bound.geometry(&mut pass, &scene.vertices, buffer, &scene.indices);
                                    bound.pipeline(&mut pass, &pipelines[0]);
                                    pass.draw_indexed(scene.index_range(&indices), scene.base_vertex, range.clone());
                                    draws += 1;
                                }
                            }
                        };
                    }
                    for batch in &scene.batches {
                        let (blend, _, background, masked, _) =
                            scene.material_modes[batch.material];
                        if blend != 0 || background {
                            continue;
                        }
                        if masked {
                            flush!();
                            bound.geometry(&mut pass, &scene.vertices, buffer, &scene.indices);
                            bound.pipeline(&mut pass, &pipelines[1]);
                            bound.material(&mut pass, &scene.materials[batch.material]);
                            pass.draw_indexed(scene.index_range(&batch.indices), scene.base_vertex, range.clone());
                            draws += 1;
                        } else if let Some(indices) =
                            run.as_mut().filter(|r| r.end == batch.indices.start)
                        {
                            indices.end = batch.indices.end;
                        } else {
                            flush!();
                            run = Some(batch.indices.clone());
                        }
                    }
                    flush!();
                }
                let mut batched = 0;
                if !pooled.is_empty() {
                    pooled.sort_by(|a, b| a.0.cmp(b.0));
                    let args: Vec<_> = pooled.iter().map(|(_, _, a)| *a).collect();
                    let uploaded = self.upload_indirect(&args);
                    let mut start = 0;
                    while start < pooled.len() {
                        let (vertices, indices, _) = pooled[start];
                        let end = start
                            + pooled[start..]
                                .iter()
                                .take_while(|(v, _, _)| *v == vertices)
                                .count();
                        bound.geometry(&mut pass, vertices, &self.identity_instance, indices);
                        bound.pipeline(&mut pass, &pipelines[0]);
                        match &uploaded {
                            Some((buffer, offset)) => {
                                pass.multi_draw_indexed_indirect(
                                    buffer,
                                    offset + start as u64 * INDIRECT_ARGS_SIZE,
                                    (end - start) as u32,
                                );
                                draws += 1;
                            }
                            None => {
                                for (_, _, a) in &pooled[start..end] {
                                    pass.draw_indexed(
                                        a.first_index..a.first_index + a.index_count,
                                        a.base_vertex,
                                        0..1,
                                    );
                                    draws += 1;
                                }
                            }
                        }
                        batched += (end - start) as u32;
                        start = end;
                    }
                }
                let mut stats = self.stats.get();
                stats.shadow_draws += draws;
                stats.shadow_binds += bound.binds;
                stats.shadow_batched += batched;
                self.stats.set(stats);
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
        fn pipeline_of(d: &Draw<'_>) -> usize {
            let (_, double_sided, background, _, _) = d.scene.material_modes[d.batch.material];
            usize::from(background) * 6 + d.blend * 2 + usize::from(double_sided)
        }
        /// What a pooled chunk batch binds; None for everything else.
        #[allow(clippy::type_complexity)]
        fn pooled_key<'a>(
            d: &Draw<'a>,
            identity: &wgpu::Buffer,
        ) -> Option<(usize, &'a wgpu::BindGroup, &'a wgpu::Buffer)> {
            (d.scene.slot.is_some() && d.range == (0..1) && d.buffer == identity).then(|| {
                (
                    pipeline_of(d),
                    &d.scene.materials[d.batch.material],
                    &d.scene.vertices,
                )
            })
        }
        let mut order = Vec::new();
        let mut stats = self.stats.get();
        // Unbounded scenes (the map, characters) keep their order and come
        // first, as before; chunks follow nearest first so the depth test
        // rejects hidden fragments early, and each chunk's buffers bind once.
        let mut visible: Vec<(f32, &GpuScene)> = Vec::with_capacity(scenes.len());
        for &scene in scenes {
            let Some(bounds) = scene.bounds else {
                visible.push((f32::NEG_INFINITY, scene));
                continue;
            };
            if let Some(frustum) = &self.frustum
                && !aabb_visible(frustum, bounds)
            {
                stats.scenes_culled += 1;
                continue;
            }
            stats.scenes_drawn += 1;
            let nearest = self.eye.clamp(bounds.0, bounds.1);
            visible.push((nearest.distance_squared(self.eye), scene));
        }
        visible.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, whole) in visible {
            for scene in std::iter::once(whole).chain(whole.translucent.as_deref()) {
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
            aa.cmp(&ba)
        });
        // Translucent draws go back to front; water surfaces sort as planes.
        let first = order
            .iter()
            .position(|d| d.blend != 0 && !d.scene.material_modes[d.batch.material].2)
            .unwrap_or(order.len());
        let translucent = order.split_off(first);
        // Opaque pooled chunk batches regroup by what they bind (nearest
        // first within a group), after everything else opaque, so each group
        // is one indirect multi-draw. Opaque order only matters for coplanar
        // faces, which already resolve arbitrarily between chunks.
        let identity = &self.identity_instance;
        let sky_end = order
            .iter()
            .position(|d| !d.scene.material_modes[d.batch.material].2)
            .unwrap_or(order.len());
        order[sky_end..].sort_by(|a, b| pooled_key(a, identity).cmp(&pooled_key(b, identity)));
        let keys: Vec<_> = translucent
            .iter()
            .map(|d| {
                (
                    d.center,
                    d.scene.material_modes[d.batch.material]
                        .4
                        .then_some(d.center.y),
                )
            })
            .collect();
        let mut translucent: Vec<_> = translucent.into_iter().map(Some).collect();
        order.extend(
            translucent_order(self.eye, &keys)
                .into_iter()
                .filter_map(|i| translucent[i].take()),
        );
        order.retain(|d| d.scene.vertex_count != 0 && d.scene.index_count != 0);
        // Runs of pooled chunk batches that bind the same things become one
        // indirect multi-draw each; their arguments upload together.
        let mut runs: Vec<(usize, usize)> = Vec::new();
        let mut args = Vec::new();
        let mut i = 0;
        while i < order.len() {
            let key = pooled_key(&order[i], identity);
            let mut end = i + 1;
            if key.is_some() {
                while end < order.len() && pooled_key(&order[end], identity) == key {
                    end += 1;
                }
            }
            if end - i > 1 {
                for d in &order[i..end] {
                    args.push(wgpu::util::DrawIndexedIndirectArgs {
                        index_count: d.batch.indices.end - d.batch.indices.start,
                        instance_count: 1,
                        first_index: d.scene.first_index + d.batch.indices.start,
                        base_vertex: d.scene.base_vertex,
                        first_instance: 0,
                    });
                }
            }
            runs.push((i, end));
            i = end;
        }
        let uploaded = self.upload_indirect(&args);
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
        let mut bound = Bound::default();
        let mut next_args = 0u64;
        for (start, end) in runs {
            let draw = &order[start];
            let (scene, batch) = (draw.scene, draw.batch);
            bound.pipeline(&mut pass, &self.pipelines[pipeline_of(draw)]);
            bound.material(&mut pass, &scene.materials[batch.material]);
            bound.geometry(&mut pass, &scene.vertices, draw.buffer, &scene.indices);
            for d in &order[start..end] {
                stats.translucent_draws += u32::from(d.blend != 0);
                stats.triangles += u64::from(d.batch.indices.end - d.batch.indices.start) / 3
                    * u64::from(d.range.end - d.range.start);
            }
            match (&uploaded, end - start > 1) {
                (Some((buffer, offset)), true) => {
                    pass.multi_draw_indexed_indirect(
                        buffer,
                        offset + next_args * INDIRECT_ARGS_SIZE,
                        (end - start) as u32,
                    );
                    next_args += (end - start) as u64;
                    stats.draws += 1;
                    stats.batched += (end - start) as u32;
                }
                _ => {
                    // No queue for indirect arguments (a renderer used
                    // before `update_camera`): one draw each.
                    if end - start > 1 {
                        next_args += (end - start) as u64;
                    }
                    for d in &order[start..end] {
                        pass.draw_indexed(
                            d.scene.index_range(&d.batch.indices),
                            d.scene.base_vertex,
                            d.range.clone(),
                        );
                        stats.draws += 1;
                    }
                }
            }
        }
        stats.binds += bound.binds;
        self.stats.set(stats);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex(n: u32) -> SceneVertex {
        SceneVertex {
            position: [n as f32, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0; 2],
            lightmap_uv: [0.0; 2],
            color: [1.0; 4],
            fx: [0.0; 4],
        }
    }

    /// Three bricks of four vertices each; the middle one is translucent.
    fn three_bricks() -> SceneData {
        let mut data = SceneData::default();
        for brick in 0..3u32 {
            let first = brick * 4;
            data.vertices.extend((first..first + 4).map(vertex));
            let start = data.indices.len() as u32;
            data.indices.extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
            data.batches.push(MeshBatch {
                indices: start..data.indices.len() as u32,
                material: usize::from(brick == 1),
                center: [0.0; 3],
            });
        }
        data
    }

    #[test]
    fn split_parts_keep_each_bricks_vertices_together_and_findable() {
        let data = three_bricks();
        let parts = split_batches(&data, |b| b.material == 1);
        for part in &parts {
            // Every index still draws the vertex it drew before.
            let mut source = Vec::new();
            for (run, local) in &part.3 {
                for (k, v) in run.clone().enumerate() {
                    assert_eq!(
                        part.0[*local as usize + k].position,
                        data.vertices[v as usize].position
                    );
                    source.push(v);
                }
            }
            assert_eq!(source.len(), part.0.len(), "runs cover the part");
        }
        let (opaque, clear) = (&parts[0], &parts[1]);
        assert_eq!(opaque.0.len(), 8);
        assert_eq!(clear.0.len(), 4);
        // The last brick is found where the split put it, in one span.
        assert_eq!(local_spans(&opaque.3, 8, 8..12), vec![4..8]);
        assert!(local_spans(&clear.3, 4, 8..12).is_empty());
        assert_eq!(local_spans(&clear.3, 4, 4..8), vec![0..4]);
        // A scene with vertices of its own maps them to themselves.
        assert_eq!(local_spans(&[], 12, 4..8), vec![4..8]);
        assert_eq!(local_spans(&[], 12, 10..20), vec![10..12]);
        // Indices still form the same triangles.
        for part in &parts {
            for &i in &part.1 {
                assert!((i as usize) < part.0.len());
            }
        }
        assert_eq!(opaque.1[6..], [4, 5, 6, 4, 6, 7]);
    }
}
