//! One lighting model for maps and everything placed on them.
//!
//! A map's interior lightmaps hold three kinds of light at once: the lights
//! the map's author placed (baked into the interior's own lightmaps by the
//! map compiler; no light objects survive in the file or the mission), the
//! mission sun's ambient, and the sun itself with the shadows the interiors
//! cast (added by the mission lighting bake). Bricks, players and items saw
//! only the sun, so they never matched the walls beside them, and a live
//! brick shadow darkened a lightmap texel whose sun was already baked away.
//!
//! This module separates those parts, the way engines with stationary
//! lights do:
//!
//! - [`decompose_sheet`] splits each mission lightmap into its static part
//!   (authored lights plus ambient) and the baked sun visibility of each
//!   texel. The shader adds the sun back per pixel, `N.L` times the lesser of
//!   the baked and the live visibility, so live shadows only ever remove sun
//!   that was there. Without live shadows it reproduces the mission lightmap
//!   to within a level.
//! - [`fit`] recovers the authored lights (position, colour, falloff) from
//!   the interior's own lightmaps by inverse rendering against its geometry.
//! - [`Bake`] turns that into what dynamic objects need: the fitted lights,
//!   a visibility volume (sun and up to three light channels, from the map's
//!   geometry) and a residual irradiance volume for the light the fit does
//!   not explain. Bricks then shade with the same lights, falloff, sun and
//!   occlusion as the lightmapped surfaces around them.
//! - The Dynamic lighting mode goes further and lights the map's own
//!   surfaces live: [`DynamicSheet`]s keep, per lightmap texel, only the
//!   light no recovered light explains (bounced light, ambient, the fit's
//!   error), and the shader adds every light (through its light cube, see
//!   `crate::shadow`) and the sun on top, so switching, dimming or
//!   recolouring a light changes the walls completely, and shadows are as
//!   sharp as the shadow maps instead of the lightmaps' texels.
use crate::scene::SceneImage;
use glam::{Vec2, Vec3};

/// `SurfaceOutsideVisible`: the mission sun and its ambient light these.
pub const OUTSIDE_VISIBLE: u8 = 1 << 4;

/// Texels a triangle claims in a `width` x `height` sheet, in texel space:
/// centres inside it (`inside`) and, when `reach` > 0, centres within
/// `reach` texels outside its edges (their position is the plane
/// extrapolated). Calls `f(x, y, barycentric, inside)`.
pub fn raster(
    size: [u32; 2],
    uv: [[f32; 2]; 3],
    reach: f32,
    mut f: impl FnMut(u32, u32, [f32; 3], bool),
) {
    let p = uv.map(|t| Vec2::new(t[0] * size[0] as f32, t[1] * size[1] as f32));
    let area = (p[1] - p[0]).perp_dot(p[2] - p[0]);
    if area.abs() < 1e-9 {
        return;
    }
    let lo = p[0].min(p[1]).min(p[2]) - Vec2::splat(reach + 1.0);
    let hi = p[0].max(p[1]).max(p[2]) + Vec2::splat(reach + 1.0);
    let x0 = lo.x.floor().max(0.0) as u32;
    let y0 = lo.y.floor().max(0.0) as u32;
    let x1 = (hi.x.ceil().max(0.0) as u32).min(size[0]);
    let y1 = (hi.y.ceil().max(0.0) as u32).min(size[1]);
    // Signed distance of a point to each edge, positive inside.
    let edges: [(Vec2, Vec2, f32); 3] = std::array::from_fn(|k| {
        let (a, b) = (p[(k + 1) % 3], p[(k + 2) % 3]);
        let d = b - a;
        let n = Vec2::new(-d.y, d.x) * area.signum();
        (a, n / n.length().max(1e-12), d.length())
    });
    for y in y0..y1 {
        for x in x0..x1 {
            let c = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let dist = edges.map(|(a, n, _)| (c - a).dot(n));
            let outside = dist.iter().fold(0.0f32, |m, d| m.max(-d));
            let inside = outside <= 1e-4;
            if !inside && outside > reach {
                continue;
            }
            let w1 = (c - p[0]).perp_dot(p[2] - p[0]) / area;
            let w2 = (p[1] - p[0]).perp_dot(c - p[0]) / area;
            f(x, y, [1.0 - w1 - w2, w1, w2], inside);
        }
    }
}

/// One interior surface drawn from a lightmap sheet: its world triangles
/// with original (not inset) lightmap coordinates, its world normal and
/// whether the mission sun lit it.
pub struct SheetSurface {
    pub triangles: Vec<[([f32; 3], [f32; 2]); 3]>,
    pub normal: Vec3,
    pub outside: bool,
}

/// The mission sun as the lighting bake applied it (colour and ambient
/// clamped to 0..1, direction toward which light travels).
#[derive(Clone, Copy, Debug)]
pub struct BakeSun {
    pub direction: Vec3,
    pub color: Vec3,
    pub ambient: Vec3,
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Splits a mission lightmap into RGB: its static part (the interior's
/// authored light, plus the sun's ambient on outside-visible surfaces) and
/// A: the share of the sun the bake let reach each texel. `base` is the
/// interior's own lightmap; `mission` the bake's result (None when the
/// mission has no sun, so nothing was added). Texels no surface claims keep
/// the mission light and no sun, so they render exactly as before.
///
/// `min(1, static + sun * N.L * visibility)` is the bake's own saturating
/// sum, so it matches the mission lightmap to within a level per texel. The
/// shader keeps drawing the mission lightmap and uses this only to take away
/// the sun a live shadow removes: filtered separately, the saturating sum
/// would brighten texels next to saturated ones.
pub fn decompose_sheet(
    base: &SceneImage,
    mission: Option<&SceneImage>,
    surfaces: &[SheetSurface],
    sun: Option<BakeSun>,
) -> SceneImage {
    let (w, h) = (base.width, base.height);
    let source = mission.filter(|m| m.width == w && m.height == h).unwrap_or(base);
    let mut rgba = source.rgba.clone();
    for p in rgba.chunks_exact_mut(4) {
        p[3] = 0;
    }
    let Some(sun) = sun.filter(|_| mission.is_some_and(|m| m.width == w && m.height == h)) else {
        return SceneImage {
            label: format!("{} static", base.label),
            width: w,
            height: h,
            rgba,
            srgb: false,
        };
    };
    // Texels inside a surface win over another surface's extrapolated rim.
    let mut claimed = vec![0u8; (w * h) as usize];
    let texel = |image: &SceneImage, i: usize| {
        Vec3::new(
            image.rgba[i * 4] as f32,
            image.rgba[i * 4 + 1] as f32,
            image.rgba[i * 4 + 2] as f32,
        ) / 255.0
    };
    for surface in surfaces {
        let facing = surface.normal.dot(-sun.direction);
        for tri in &surface.triangles {
            raster([w, h], tri.map(|v| v.1), 1.5, |x, y, _, inside| {
                let i = (y * w + x) as usize;
                let rank = if inside { 2 } else { 1 };
                if claimed[i] > rank {
                    return;
                }
                claimed[i] = rank;
                let b = texel(base, i);
                let m = texel(source, i);
                let ambient = if surface.outside { sun.ambient } else { Vec3::ZERO };
                let fixed = (b + ambient).min(Vec3::ONE);
                // The sun share: from the least saturated channel the sun lit.
                let mut visibility = 0.0f32;
                if surface.outside && facing > 0.01 {
                    let mut best = -1.0f32;
                    for c in 0..3 {
                        let lit = sun.color[c] * facing;
                        if lit < 1.0 / 255.0 {
                            continue;
                        }
                        let saturated = m[c] >= 1.0 - 0.5 / 255.0;
                        // Prefer unsaturated channels, then the strongest.
                        let score = lit + if saturated { 0.0 } else { 2.0 };
                        if score > best {
                            best = score;
                            visibility = if saturated && fixed[c] + lit >= 1.0 {
                                1.0
                            } else {
                                ((m[c] - fixed[c]) / lit).clamp(0.0, 1.0)
                            };
                        }
                    }
                }
                rgba[i * 4] = byte(fixed.x);
                rgba[i * 4 + 1] = byte(fixed.y);
                rgba[i * 4 + 2] = byte(fixed.z);
                rgba[i * 4 + 3] = byte(visibility);
            });
        }
    }
    SceneImage {
        label: format!("{} static", base.label),
        width: w,
        height: h,
        rgba,
        srgb: false,
    }
}

/// What the shader reconstructs from a decomposed texel without live
/// shadows (a CPU mirror for tests and error reports).
pub fn reconstruct(texel: [u8; 4], sun: BakeSun, normal: Vec3) -> Vec3 {
    let fixed = Vec3::new(texel[0] as f32, texel[1] as f32, texel[2] as f32) / 255.0;
    let facing = normal.dot(-sun.direction).max(0.0);
    (fixed + sun.color * facing * (texel[3] as f32 / 255.0)).min(Vec3::ONE)
}

// --- Recovering the authored lights -----------------------------------------

/// Lights the fit may add, and lights the shader evaluates per pixel (those
/// given a visibility channel).
pub const MAX_LIGHTS: usize = 24;
/// Visibility channels for lights in the visibility volume (its first
/// channel is the sun). Lights whose ranges overlap need different ones; a
/// light that finds none stays baked (its light joins the residual).
pub const CHANNELS: usize = 7;
/// Lexels the fit samples; the light's shape is judged on these.
const SAMPLES: usize = 12_000;
/// Inner falloff radii and falloff spans the fit tries for each position.
const INNER: [f32; 7] = [0.0, 2.0, 5.0, 10.0, 20.0, 40.0, 80.0];
const SPAN: [f32; 10] = [5.0, 10.0, 20.0, 40.0, 60.0, 90.0, 130.0, 180.0, 250.0, 350.0];
/// A light must remove this share of what is left to be kept.
const MIN_GAIN: f64 = 0.002;
/// Offset off a surface before casting toward a light.
const LIFT: f32 = 0.05;
/// Texels this far outside a surface's edges (in texels) take its values in
/// the Dynamic mode's lightmaps: past a bilinear filter's reach.
const RIM: f32 = 1.5;
/// A rim texel's samples start this far (world units) inside its surface.
const RIM_INSET: f32 = 0.05;
/// Light channels one Dynamic lightmap carries: four to each of the six
/// spare material slots (1..=6).
pub const DYNAMIC_CHANNELS: usize = 24;
/// Leak cleanup (`Bake::leaks`): texels to each side a thin leak must be
/// brighter than, and by how much (authored light in luminance, baked sun
/// in its share).
const LEAK_STEP: i64 = 3;
const LEAK_LEVELS: f32 = 8.0 / 255.0;
const LEAK_SUN: f32 = 0.12;
/// Names the fit, the bake and the stored layout; change it with either.
const FORMAT: &[u8; 8] = b"BRIML\0\0\x09";

/// A light recovered from a map's lightmaps. The map compiler's point light:
/// full `color` out to `inner`, then falling linearly to nothing at `outer`,
/// on every surface that faces it (no cosine), stopped by the map's geometry.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MapLight {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub inner: f32,
    pub outer: f32,
    /// Visibility channel `0..CHANNELS`, or `None` when the light stays in
    /// the baked residual.
    pub channel: Option<u8>,
}
impl MapLight {
    /// Unshadowed light reaching a surface at `position` facing `normal`.
    pub fn shade(&self, position: Vec3, normal: Vec3) -> Vec3 {
        let delta = Vec3::from(self.position) - position;
        let distance = delta.length();
        let facing = facing_term((normal.dot(delta) / distance.max(1e-4)).max(0.0));
        Vec3::from(self.color) * falloff(distance, self.inner, self.outer) * facing
    }
}
/// A recovered light belongs to the light shapes (bulbs, tubes) nearest it,
/// up to this far from their centres. The fit places a fixture's lights
/// where their falloff fits the lightmaps best, not on the bulb: measured on
/// v20's maps, the Bedroom bulb's main light sits 19.9 units from it and the
/// Kitchen tubes' lights 8.8 to 15.9. Window and sun light, fitted farther
/// from any fixture, stays unowned.
pub const FIXTURE_REACH: f32 = 24.0;
/// Shapes up to this many times the nearest one's distance share a light:
/// the Kitchen's paired tubes fit as one light between them.
pub const FIXTURE_SHARE: f32 = 1.5;
/// Per light, the light shapes (`shapes`: an id and centre each) it belongs
/// to: switched off when all of them break, dimmed by the share broken.
pub fn fixture_owners<T: Copy>(lights: &[MapLight], shapes: &[(T, Vec3)]) -> Vec<Vec<(T, Vec3)>> {
    lights
        .iter()
        .map(|light| {
            let at = Vec3::from(light.position);
            let nearest = shapes.iter().map(|(_, c)| c.distance(at)).fold(f32::INFINITY, f32::min);
            let limit = FIXTURE_REACH.min(nearest * FIXTURE_SHARE);
            shapes.iter().copied().filter(|(_, c)| c.distance(at) <= limit).collect()
        })
        .collect()
}
fn falloff(distance: f32, inner: f32, outer: f32) -> f32 {
    ((outer - distance) / (outer - inner).max(1e-3)).clamp(0.0, 1.0)
}
/// The map compiler lit every surface facing a light fully, without the
/// cosine: fitted that way the stock maps' lit texels are 15 levels off on
/// Bedroom and 25 on Kitchen, against 25 and 29 with a cosine (squared or
/// smoothstep falloffs fit no better).
fn facing_term(cosine: f32) -> f32 {
    if cosine > 0.0 { 1.0 } else { 0.0 }
}

/// How well the lights reproduce the interiors' own lightmaps, in levels
/// (0..255) over every lightmap texel a surface covers.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FitReport {
    pub lexels: usize,
    /// Mean and RMS error with no lights at all (the lightmaps' own size).
    pub unlit_mean: f32,
    pub unlit_rms: f32,
    /// With every fitted light.
    pub mean: f32,
    pub rms: f32,
    /// Over texels with any light (above 2 levels).
    pub lit_lexels: usize,
    pub lit_mean: f32,
    /// Only the lights the shader evaluates (those with a channel); the rest
    /// of the light is the residual.
    pub channel_mean: f32,
    /// Lightmap texels whose leaked light was cleaned up (`Bake::leaks`).
    #[serde(default)]
    pub leak_texels: usize,
    pub seconds: f32,
}

/// A texel of an interior lightmap that a surface covers.
#[derive(Clone, Copy)]
struct Lexel {
    position: Vec3,
    normal: Vec3,
    base: Vec3,
    sheet: u32,
    index: u32,
}

/// Deterministic xorshift, so every machine fits the same lights.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// What the bake reads from a map scene; taken on the loading thread so the
/// bake can run on another.
pub struct Bake {
    bvh: crate::light_volume::Bvh,
    lexels: Vec<Lexel>,
    /// Scene image index and size of each decomposed sheet.
    sheets: Vec<(usize, u32, u32)>,
    bases: Vec<std::sync::Arc<SceneImage>>,
    /// Per sheet: the lightmap drawn (mission or original) and its
    /// decomposition (scene image index and pixels), which leak cleanup
    /// patches.
    drawn: Vec<SceneImage>,
    decomposed: Vec<(usize, SceneImage)>,
    /// Texels just outside the surfaces (within `RIM` texels of an edge)
    /// that no surface covers, at the nearest point of the surface beside
    /// them: bilinear filtering at a surface's edge reads them, so the
    /// Dynamic mode's lightmaps give them that surface's own values.
    rims: Vec<Lexel>,
    sun_direction: Vec3,
    /// The classic volume's input; its lightmaps become the residual.
    residual_input: crate::light_volume::Baker,
    key: [u8; 32],
}

/// Everything dynamic objects need to share the map's lighting.
#[derive(Clone, Debug, PartialEq)]
pub struct MapLighting {
    pub lights: Vec<MapLight>,
    pub report: FitReport,
    /// Per cell: sun, then light channels 0..CHANNELS visibility (0..255).
    pub visibility: VisibilityVolume,
    /// The light the channel lights leave unexplained, gathered like the
    /// classic light volume.
    pub residual: crate::light_volume::LightVolume,
    /// Lightmap texels to patch where the map compiler's light leaked
    /// through walls (`Bake::leaks`): the drawn lightmaps and their
    /// decompositions, in every lighting mode.
    pub leaks: Vec<TexelFix>,
    /// The Dynamic lighting mode's lightmaps, one per decomposed sheet:
    /// RGB the static light none of the recovered lights explain (bounced
    /// light, the mission ambient, the fit's error), A the baked sun share.
    /// The shader adds every light, and the sun, live on top.
    pub dynamic: Vec<DynamicSheet>,
    /// The residual volume without any recovered light, for objects in the
    /// Dynamic mode, where every light (not only those with a visibility
    /// channel) is live. The same as `residual` when every light has one.
    pub residual_all: crate::light_volume::LightVolume,
}

/// The Dynamic mode's residual volume, left to bake after the rest
/// (`Bake::bake_staged`).
pub struct ResidualBake(crate::light_volume::Baker);
impl ResidualBake {
    /// `MapLighting::residual_all`.
    pub fn bake(self, min_cell: f32, max_cells: usize) -> crate::light_volume::LightVolume {
        self.0.bake(min_cell, max_cells)
    }
}

/// One decomposed lightmap sheet in the Dynamic mode: the light no
/// recovered light explains (RGB; A the baked sun share), and, per light that
/// reaches the sheet (`lights`, indices into `MapLighting::lights`), the
/// share of it each texel receives past the map's walls, four lights to an
/// RGBA image. Both come from the same rays the map's own light was fitted
/// with, so at rest the sheet reproduces the decomposition; the shader
/// applies each light's colour and brightness now.
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicSheet {
    /// The decomposition's scene image (material slot 9) this belongs to.
    pub parts_image: u32,
    pub width: u32,
    pub height: u32,
    pub left: Vec<u8>,
    pub lights: Vec<u8>,
    pub visibility: Vec<Vec<u8>>,
}
/// `DynamicSheet` without its pixels, as a stored bake's header keeps it.
#[derive(serde::Serialize, serde::Deserialize)]
struct DynamicLayout {
    parts_image: u32,
    width: u32,
    height: u32,
    lights: Vec<u8>,
}
impl DynamicSheet {
    /// Gives a map scene the Dynamic mode's lightmaps: each sheet's images
    /// join the scene, and every decomposed material drawing from it reads
    /// them (slot 10 the leftover light, 1..=6 the visibility) with its
    /// light channels in its parameters (`scene.wgsl` `channel_light`).
    /// Returns whether anything changed; upload the scene again after.
    pub fn equip(sheets: &[DynamicSheet], scene: &mut crate::scene::SceneData) -> bool {
        use crate::scene::{DECOMPOSED_LIGHTMAP, MaterialKind};
        let mut changed = false;
        for sheet in sheets {
            let image = |label: String, rgba: &Vec<u8>| SceneImage {
                label,
                width: sheet.width,
                height: sheet.height,
                rgba: rgba.clone(),
                srgb: false,
            };
            let users: Vec<usize> = (0..scene.materials.len())
                .filter(|&m| {
                    let material = &scene.materials[m];
                    material.kind == MaterialKind::Surface
                        && material.images[9] == sheet.parts_image as usize
                        && material.parameters == Some(DECOMPOSED_LIGHTMAP)
                })
                .collect();
            let fits = scene.images.get(sheet.parts_image as usize).is_some_and(|p| {
                p.width == sheet.width && p.height == sheet.height
            }) && sheet.left.len() == (sheet.width * sheet.height * 4) as usize
                && sheet.lights.len() <= DYNAMIC_CHANNELS
                && sheet.visibility.len() == sheet.lights.len().div_ceil(4)
                && sheet.visibility.iter().all(|v| v.len() == sheet.left.len());
            if users.is_empty() || !fits {
                continue;
            }
            let label = scene.images[sheet.parts_image as usize].label.clone();
            let left = scene.images.len();
            scene.images.push(image(format!("{label} dynamic"), &sheet.left));
            let first = scene.images.len();
            for (k, v) in sheet.visibility.iter().enumerate() {
                scene.images.push(image(format!("{label} dynamic visibility {k}"), v));
            }
            let parameters = pack_channels(&sheet.lights);
            for m in users {
                let material = &mut scene.materials[m];
                material.images[10] = left;
                for k in 0..sheet.visibility.len() {
                    material.images[1 + k] = first + k;
                }
                material.parameters = Some(parameters);
            }
            changed = true;
        }
        changed
    }
}
/// A decomposed material's parameters with light channels: x 1 (decomposed),
/// y the channel count, then two light indices to a float (`a + 32 b`).
fn pack_channels(lights: &[u8]) -> [[f32; 4]; 4] {
    let mut p = crate::scene::DECOMPOSED_LIGHTMAP;
    p[0][1] = lights.len() as f32;
    for (c, &light) in lights.iter().enumerate() {
        let f = c / 2;
        p[1 + f / 4][f % 4] += f32::from(light) * if c % 2 == 0 { 1.0 } else { 32.0 };
    }
    p
}

/// One lightmap texel's cleaned-up value: scene image, texel, RGBA.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TexelFix {
    pub image: u32,
    pub index: u32,
    pub rgba: [u8; 4],
}
impl TexelFix {
    /// Applies `fixes` to a scene's images; returns the images changed.
    pub fn apply(fixes: &[TexelFix], images: &mut [SceneImage]) -> Vec<usize> {
        let mut changed = std::collections::BTreeSet::new();
        for fix in fixes {
            let Some(image) = images.get_mut(fix.image as usize) else { continue };
            let Some(texel) = image.rgba.get_mut(fix.index as usize * 4..fix.index as usize * 4 + 4) else {
                continue;
            };
            texel.copy_from_slice(&fix.rgba);
            changed.insert(fix.image as usize);
        }
        changed.into_iter().collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisibilityVolume {
    pub origin: [f32; 3],
    pub cell: f32,
    pub dims: [u32; 3],
    pub texels: Vec<[u8; 8]>,
}

impl Bake {
    /// Reads a map scene's decomposed interior lightmaps. None without any.
    pub fn new(scene: &crate::scene::SceneData) -> Option<Self> {
        use crate::scene::{AlphaMode, DECOMPOSED_LIGHTMAP, MaterialKind};
        if scene.lightmap_bases.is_empty() {
            return None;
        }
        let bases: std::collections::BTreeMap<usize, &std::sync::Arc<SceneImage>> =
            scene.lightmap_bases.iter().map(|(i, b)| (*i, b)).collect();
        let mut sheet_of = std::collections::BTreeMap::new();
        let mut sheets = vec![];
        let mut base_images = vec![];
        let mut drawn = vec![];
        let mut decomposed = vec![];
        let mut rims = vec![];
        let mut claimed: Vec<Vec<bool>> = vec![];
        let mut lexels = vec![];
        let mut occluders = vec![];
        for batch in &scene.batches {
            let material = scene.materials.get(batch.material)?;
            if material.kind != MaterialKind::Surface
                || matches!(material.alpha, AlphaMode::Blend | AlphaMode::Additive)
            {
                continue;
            }
            let lit = material.parameters == Some(DECOMPOSED_LIGHTMAP);
            debug_assert!(!crate::scene::decomposed_lightmap(material.parameters) || lit, "bake before equipping");
            let image = material.images[8];
            let sheet = match (lit, bases.get(&image)) {
                (true, Some(base)) => Some(*sheet_of.entry(image).or_insert_with(|| {
                    sheets.push((image, base.width, base.height));
                    base_images.push((*base).clone());
                    let parts = material.images[9];
                    drawn.push(scene.images[image].clone());
                    decomposed.push((parts, scene.images[parts].clone()));
                    claimed.push(vec![false; (base.width * base.height) as usize]);
                    sheets.len() - 1
                })),
                _ => None,
            };
            let range = batch.indices.start as usize..batch.indices.end as usize;
            for corner in scene.indices.get(range)?.chunks_exact(3) {
                let v = [0, 1, 2].map(|k| scene.vertices.get(corner[k] as usize));
                let [Some(a), Some(b), Some(c)] = v else {
                    return None;
                };
                let v = [a, b, c];
                let p = v.map(|v| Vec3::from(v.position));
                if (p[1] - p[0]).cross(p[2] - p[0]).length_squared() < 1e-12 {
                    continue;
                }
                occluders.push(crate::light_volume::Triangle::occluder(p[0], p[1], p[2]));
                let Some(sheet) = sheet else { continue };
                let base = &base_images[sheet];
                let normal = v
                    .iter()
                    .map(|v| Vec3::from(v.normal))
                    .sum::<Vec3>()
                    .normalize_or_zero();
                let (w, h) = (base.width, base.height);
                let centroid = (p[0] + p[1] + p[2]) / 3.0;
                raster([w, h], v.map(|v| v.lightmap_uv), RIM, |x, y, b, inside| {
                    let index = (y * w + x) as usize;
                    let t = &base.rgba[index * 4..index * 4 + 3];
                    let base = Vec3::new(t[0] as f32, t[1] as f32, t[2] as f32) / 255.0;
                    if !inside {
                        // The nearest point of this triangle, a little inside
                        // it, so its rays never start in the wall it meets.
                        let b = b.map(|w| w.max(0.0));
                        let sum = (b[0] + b[1] + b[2]).max(1e-6);
                        let edge = (p[0] * b[0] + p[1] * b[1] + p[2] * b[2]) / sum;
                        let inward = (centroid - edge).clamp_length_max(RIM_INSET);
                        rims.push(Lexel {
                            position: edge + inward,
                            normal,
                            base,
                            sheet: sheet as u32,
                            index: index as u32,
                        });
                        return;
                    }
                    if std::mem::replace(&mut claimed[sheet][index], true) {
                        return;
                    }
                    lexels.push(Lexel {
                        position: p[0] * b[0] + p[1] * b[1] + p[2] * b[2],
                        normal,
                        base,
                        sheet: sheet as u32,
                        index: index as u32,
                    });
                });
            }
        }
        if lexels.is_empty() || occluders.is_empty() {
            return None;
        }
        // Rim texels a surface covers are its own; the rest go to the first
        // surface that reaches them.
        let mut rimmed = claimed.clone();
        rims.retain(|r: &Lexel| !std::mem::replace(&mut rimmed[r.sheet as usize][r.index as usize], true));
        let sun_direction = Vec3::from(scene.sun_direction).normalize_or_zero();
        let key = {
            use sha2::{Digest, Sha256};
            let mut hash = Sha256::new();
            hash.update(FORMAT);
            for c in sun_direction.to_array() {
                hash.update(c.to_le_bytes());
            }
            for t in &occluders {
                for c in [t.a, t.e1, t.e2].iter().flat_map(|v: &Vec3| v.to_array()) {
                    hash.update(c.to_le_bytes());
                }
            }
            for l in &lexels {
                for c in [l.position, l.normal].iter().flat_map(|v| v.to_array()) {
                    hash.update(c.to_le_bytes());
                }
                hash.update(l.sheet.to_le_bytes());
                hash.update(l.index.to_le_bytes());
            }
            for b in base_images.iter().map(|b| &**b).chain(&drawn).chain(decomposed.iter().map(|d| &d.1)) {
                hash.update(b.width.to_le_bytes());
                hash.update(&b.rgba);
            }
            hash.finalize().into()
        };
        Some(Self {
            bvh: crate::light_volume::Bvh::new(occluders),
            lexels,
            sheets,
            bases: base_images,
            drawn,
            decomposed,
            rims,
            sun_direction,
            residual_input: crate::light_volume::Baker::new(scene)?,
            key,
        })
    }

    /// Equal keys bake equal results, so a stored bake can stand in.
    pub fn key(&self) -> [u8; 32] {
        self.key
    }

    /// Whether `from` sees `to` past the map's geometry.
    fn sees(&self, from: Vec3, normal: Vec3, to: Vec3) -> bool {
        let origin = from + normal * LIFT;
        let delta = to - origin;
        let distance = delta.length();
        distance < 1e-3 || !self.bvh.blocked(origin, delta / distance, distance - LIFT)
    }

    /// Fits lights to the interiors' own lightmaps, then bakes the volumes.
    /// `min_cell`/`max_cells` bound the residual volume like the classic
    /// one; the visibility volume uses cells of `vis_cell` units or more,
    /// at most `vis_cells`.
    pub fn bake(self, min_cell: f32, max_cells: usize, vis_cell: f32, vis_cells: usize) -> MapLighting {
        let (mut lighting, rest) = self.bake_staged(min_cell, max_cells, vis_cell, vis_cells);
        if let Some(rest) = rest {
            lighting.residual_all = rest.bake(min_cell, max_cells);
        }
        lighting
    }

    /// `bake`, less the residual volume for the Dynamic mode when it needs
    /// a bake of its own (some light has no channel): that is returned to
    /// bake next, and `residual_all` holds `residual` until then. The
    /// other modes need not wait for it.
    pub fn bake_staged(
        self,
        min_cell: f32,
        max_cells: usize,
        vis_cell: f32,
        vis_cells: usize,
    ) -> (MapLighting, Option<ResidualBake>) {
        let started = std::time::Instant::now();
        let mut lights = self.fit();
        assign_channels(&mut lights);
        // Every lexel's light from each light, for the report and residual.
        // Also what every light would give with no walls in the way.
        // Per lexel, too, which lights it sees (a bit each).
        let (per_lexel, seen): (Vec<(Vec3, Vec3, Vec3)>, Vec<u32>) =
            crate::light_volume::parallel(self.lexels.len(), |i| {
                let l = &self.lexels[i];
                let mut all = Vec3::ZERO;
                let mut channel = Vec3::ZERO;
                let mut open = Vec3::ZERO;
                let mut seen = 0u32;
                for (k, light) in lights.iter().enumerate() {
                    let shade = light.shade(l.position, l.normal);
                    if shade.max_element() <= 0.0 {
                        continue;
                    }
                    open += shade;
                    if !self.sees(l.position, l.normal, light.position.into()) {
                        continue;
                    }
                    seen |= 1 << k;
                    all += shade;
                    if light.channel.is_some() {
                        channel += shade;
                    }
                }
                ((all, channel, open), seen)
            })
            .into_iter()
            .unzip();
        let mut report = FitReport {
            lexels: self.lexels.len(),
            ..Default::default()
        };
        let (mut unlit, mut unlit2, mut err, mut err2, mut lit, mut lit_err, mut ch_err) =
            (0f64, 0f64, 0f64, 0f64, 0usize, 0f64, 0f64);
        let mut residual: Vec<SceneImage> = self.bases.iter().map(|b| (**b).clone()).collect();
        for (l, (all, channel, _)) in self.lexels.iter().zip(&per_lexel) {
            let e0 = (l.base * 255.0).element_sum() as f64 / 3.0;
            unlit += e0;
            unlit2 += (l.base * 255.0).length_squared() as f64 / 3.0;
            let d = (all.min(Vec3::ONE) - l.base) * 255.0;
            err += d.abs().element_sum() as f64 / 3.0;
            err2 += d.length_squared() as f64 / 3.0;
            ch_err += ((channel.min(Vec3::ONE) - l.base) * 255.0).abs().element_sum() as f64 / 3.0;
            if l.base.max_element() > 2.0 / 255.0 {
                lit += 1;
                lit_err += d.abs().element_sum() as f64 / 3.0;
            }
            let texel = &mut residual[l.sheet as usize].rgba[l.index as usize * 4..][..3];
            let left = (l.base - *channel).max(Vec3::ZERO);
            texel.copy_from_slice(&[byte(left.x), byte(left.y), byte(left.z)]);
        }
        let n = self.lexels.len().max(1) as f64;
        report.unlit_mean = (unlit / n) as f32;
        report.unlit_rms = (unlit2 / n).sqrt() as f32;
        report.mean = (err / n) as f32;
        report.rms = (err2 / n).sqrt() as f32;
        report.lit_lexels = lit;
        report.lit_mean = (lit_err / lit.max(1) as f64) as f32;
        report.channel_mean = (ch_err / n) as f32;
        let leaks = self.leaks(&per_lexel);
        report.leak_texels = leaks.len() / 2;
        let visibility = self.visibility(&lights, vis_cell, vis_cells);
        let dynamic = self.dynamic_sheets(&lights, &seen, &leaks);
        // Without any light: what objects add in the Dynamic mode.
        let every_light_live = lights.iter().all(|l| l.channel.is_some());
        let mut residual_all: Vec<SceneImage> = self.bases.iter().map(|b| (**b).clone()).collect();
        if !every_light_live {
            for (l, (all, _, _)) in self.lexels.iter().zip(&per_lexel) {
                let texel = &mut residual_all[l.sheet as usize].rgba[l.index as usize * 4..][..3];
                let left = (l.base - *all).max(Vec3::ZERO);
                texel.copy_from_slice(&[byte(left.x), byte(left.y), byte(left.z)]);
            }
        }
        let replaced = |images: Vec<SceneImage>, what: &str| -> std::collections::BTreeMap<usize, SceneImage> {
            self.sheets
                .iter()
                .zip(images)
                .map(|((image, _, _), mut r)| {
                    r.label = format!("{} {what}", r.label);
                    (*image, r)
                })
                .collect()
        };
        let rest = (!every_light_live).then(|| {
            ResidualBake(self.residual_input.clone().replace(&replaced(residual_all, "residual without lights")))
        });
        let residual = self.residual_input.replace(&replaced(residual, "residual")).bake(min_cell, max_cells);
        report.seconds = started.elapsed().as_secs_f32();
        let lighting = MapLighting {
            lights,
            report,
            visibility,
            residual_all: residual.clone(),
            residual,
            leaks,
            dynamic,
        };
        (lighting, rest)
    }

    /// The Dynamic mode's lightmaps (`DynamicSheet`), from each decomposed
    /// sheet with its leaks cleaned and the lights each texel sees (`seen`,
    /// a bit per light, from the fit's own rays; rim texels cast theirs).
    ///
    /// The interior's own lightmap is the truth about where each light
    /// arrived; the rays only say which light it most likely was. So a
    /// texel's authored light above the map compiler's ambient floor
    /// (`authored_floor`) goes first to the lights its rays see, as far as it
    /// holds them (where the compiler had a shadow the rays miss, their share
    /// drops), and what is left over to the lights in reach the rays say are
    /// hidden or the texel faces away from, when it is most of their light
    /// (the compiler let it through geometry the rays hit, or lit the
    /// outside of the Bedroom lamp's shade with the light inside). The floor and the mission sun's
    /// ambient never go to a light. Switching a light off then takes away
    /// exactly the light it baked: its baked shadows vanish with it instead
    /// of turning darker than the room, and nothing it lit stays lit.
    fn dynamic_sheets(&self, lights: &[MapLight], seen: &[u32], leaks: &[TexelFix]) -> Vec<DynamicSheet> {
        let rim_seen: Vec<u32> = crate::light_volume::parallel(self.rims.len(), |i| {
            let r = &self.rims[i];
            lights.iter().enumerate().fold(0u32, |mask, (k, light)| {
                let lit = light.shade(r.position, r.normal).max_element() > 0.0
                    && self.sees(r.position, r.normal, light.position.into());
                mask | (u32::from(lit) << k)
            })
        });
        let floor = self.authored_floor(seen);
        let lights = &lights[..lights.len().min(DYNAMIC_CHANNELS)];
        let mut by_sheet: Vec<Vec<(&Lexel, u32)>> = vec![Vec::new(); self.decomposed.len()];
        for (l, mask) in self.lexels.iter().zip(seen.iter().copied()).chain(self.rims.iter().zip(rim_seen)) {
            if let Some(sheet) = by_sheet.get_mut(l.sheet as usize) {
                sheet.push((l, mask));
            }
        }
        let luminance = |c: Vec3| c.dot(Vec3::new(0.2126, 0.7152, 0.0722));
        // Up to `held` of `given` (by luminance): the share each gets.
        let share = |held: Vec3, given: Vec3| {
            if luminance(given) <= 1e-6 {
                0.0
            } else {
                (luminance(held) / luminance(given)).clamp(0.0, 1.0)
            }
        };
        let mut sheets = Vec::with_capacity(self.decomposed.len());
        for (sheet, (parts_image, parts)) in self.decomposed.iter().enumerate() {
            let mut parts = parts.clone();
            for fix in leaks.iter().filter(|f| f.image as usize == *parts_image) {
                if let Some(t) = parts.rgba.get_mut(fix.index as usize * 4..fix.index as usize * 4 + 4) {
                    t.copy_from_slice(&fix.rgba);
                }
            }
            let texel_of = |i: usize| {
                parts.rgba.get(i * 4..i * 4 + 3).map(|t| Vec3::new(t[0] as f32, t[1] as f32, t[2] as f32) / 255.0)
            };
            // Per texel: the lights its rays see take its authored light
            // first, as far as it holds them; what is left over is a share
            // (`raw`) of the lights in reach the rays say are hidden.
            struct Split {
                index: usize,
                position: Vec3,
                normal: Vec3,
                texel: Vec3,
                given: Vec3,
                shares: Vec<f32>,
                seen: u32,
                hidden: u32,
                hidden_light: Vec3,
                raw: f32,
            }
            let mut splits: Vec<Split> = Vec::new();
            let mut split_at: Vec<Option<usize>> = vec![None; (parts.width * parts.height) as usize];
            for &(l, mask) in &by_sheet[sheet] {
                let i = l.index as usize;
                let Some(texel) = texel_of(i) else { continue };
                // The authored light (a cleaned leak holds less), above the
                // floor; the rest of the texel is the sun's ambient.
                let held = (l.base.min(texel) - floor).max(Vec3::ZERO);
                // Each light as the renderer gives a share of it
                // (`light_given`): its falloff, facing or not. The rays see
                // only lights the texel faces; the hidden ones include those
                // it faces away from, which the compiler could still have
                // lit it with, as the outside of the Bedroom lamp's shade
                // glows with the light inside it.
                let given_by: Vec<Vec3> = lights
                    .iter()
                    .map(|light| {
                        let distance = Vec3::from(light.position).distance(l.position);
                        Vec3::from(light.color) * falloff(distance, light.inner, light.outer)
                    })
                    .collect();
                let in_reach = |k: usize| given_by[k].max_element() > 0.0;
                let faces = |k: usize| lights[k].shade(l.position, l.normal).max_element() > 0.0;
                let seen = (0..lights.len()).filter(|&k| mask & (1 << k) != 0 && faces(k)).fold(0u32, |m, k| m | 1 << k);
                let hidden = (0..lights.len()).filter(|&k| seen & (1 << k) == 0 && in_reach(k)).fold(0u32, |m, k| m | 1 << k);
                let light_of = |set: u32| (0..lights.len()).filter(|&k| set & (1 << k) != 0).map(|k| given_by[k]).sum::<Vec3>();
                let seen_light = light_of(seen);
                let s = share(held, seen_light);
                let mut shares = vec![0.0f32; lights.len()];
                for k in (0..lights.len()).filter(|&k| seen & (1 << k) != 0) {
                    shares[k] = s;
                }
                let given = seen_light * s;
                let hidden_light = light_of(hidden);
                if let Some(at) = split_at.get_mut(i) {
                    *at = Some(splits.len());
                }
                splits.push(Split {
                    index: i,
                    position: l.position,
                    normal: l.normal,
                    texel,
                    given,
                    shares,
                    seen,
                    hidden,
                    hidden_light,
                    raw: share((held - given).max(Vec3::ZERO), hidden_light),
                });
            }
            // A remainder far short of the hidden lights' light (under a
            // tenth, fading out by a quarter) is the fit's error or a light
            // it never traced, and stays in the leftover. Unless a neighbour
            // on the same surface plainly holds one of those lights (its rays
            // see it, or a quarter of it is left over there): then this is a
            // shadow's edge, where the rays from the fitted light and the map
            // compiler's filtered shadow disagree by a texel or two, and
            // keeping it would leave a line of the light after it goes out.
            let (w, h) = (parts.width as i64, parts.height as i64);
            let edge_of_lit = |t: &Split| {
                let (x, y) = (t.index as i64 % w.max(1), t.index as i64 / w.max(1));
                (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (x + dx, y + dy))).any(|(nx, ny)| {
                    if nx < 0 || ny < 0 || nx >= w || ny >= h || (nx, ny) == (x, y) {
                        return false;
                    }
                    let Some(n) = split_at[(ny * w + nx) as usize].map(|k| &splits[k]) else { return false };
                    let apart = n.position - t.position;
                    let same_surface =
                        n.normal.dot(t.normal) > 0.95 && apart.dot(t.normal).abs() <= 0.1 * apart.length() + 1e-3;
                    same_surface && (n.seen & t.hidden != 0 || (n.raw >= 0.25 && n.hidden & t.hidden != 0))
                })
            };
            // Per texel: its index, its leftover light and each light's share.
            let mut shared: Vec<(usize, Vec3, Vec<f32>)> = Vec::with_capacity(splits.len());
            let mut reach = 0u32;
            for t in &splits {
                let mut shares = t.shares.clone();
                let mut given = t.given;
                let mut s = t.raw;
                if s < 0.25 && !edge_of_lit(t) {
                    let fade = ((s - 0.1) / 0.15).clamp(0.0, 1.0);
                    s *= fade * fade * (3.0 - 2.0 * fade);
                }
                if s > 0.0 {
                    for k in (0..lights.len()).filter(|&k| t.hidden & (1 << k) != 0) {
                        shares[k] = s;
                    }
                    given += t.hidden_light * s;
                }
                for (k, &share) in shares.iter().enumerate() {
                    if share > 0.0 {
                        reach |= 1 << k;
                    }
                }
                shared.push((t.index, (t.texel - given).max(Vec3::ZERO), shares));
            }
            let channels: Vec<u8> = (0..lights.len() as u8).filter(|&k| reach & (1 << k) != 0).collect();
            let texels = (parts.width * parts.height) as usize;
            let mut visibility = vec![vec![0u8; texels * 4]; channels.len().div_ceil(4)];
            let mut left = parts.rgba.clone();
            for (i, rest, shares) in shared {
                left[i * 4..i * 4 + 3].copy_from_slice(&[byte(rest.x), byte(rest.y), byte(rest.z)]);
                for (c, &k) in channels.iter().enumerate() {
                    visibility[c / 4][i * 4 + c % 4] = byte(shares[k as usize]);
                }
            }
            sheets.push(DynamicSheet {
                parts_image: *parts_image as u32,
                width: parts.width,
                height: parts.height,
                left,
                lights: channels,
                visibility,
            });
        }
        sheets
    }

    /// The ambient the map compiler added to every texel of the interiors'
    /// own lightmaps (its `ambient_color`): per channel, the level all but
    /// the darkest 5% of the texels no fitted light reaches (by the rays,
    /// `seen`) hold. Those include texels the compiler lit through geometry
    /// the rays hit, but mostly ones behind furniture and walls that only
    /// the ambient reaches. With too few of them to tell, none.
    fn authored_floor(&self, seen: &[u32]) -> Vec3 {
        let dark: Vec<Vec3> = self.lexels.iter().zip(seen).filter(|(_, s)| **s == 0).map(|(l, _)| l.base).collect();
        let mut floor = Vec3::ZERO;
        if dark.len() < 64.max(self.lexels.len() / 200) {
            return floor;
        }
        for c in 0..3 {
            let mut levels: Vec<f32> = dark.iter().map(|b| b[c]).collect();
            let at = levels.len() / 20;
            levels.select_nth_unstable_by(at, f32::total_cmp);
            floor[c] = levels[at];
        }
        floor
    }

    /// Greedy inverse rendering: each round seeds candidate positions above
    /// the brightest unexplained texels, keeps the one whose best colour and
    /// falloff remove the most error (visibility cast from every sample to
    /// it), refines it by pattern search, then refits every colour jointly.
    fn fit(&self) -> Vec<MapLight> {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let lit: Vec<usize> = (0..self.lexels.len())
            .filter(|&i| self.lexels[i].base.max_element() > 1.0 / 255.0)
            .collect();
        let dark: Vec<usize> = (0..self.lexels.len())
            .filter(|&i| self.lexels[i].base.max_element() <= 1.0 / 255.0)
            .collect();
        if lit.is_empty() {
            return vec![];
        }
        let mut pick = |pool: &[usize], n: usize| -> Vec<usize> {
            if pool.len() <= n {
                return pool.to_vec();
            }
            let mut chosen: Vec<usize> = (0..n)
                .map(|_| pool[(rng.next() % pool.len() as u64) as usize])
                .collect();
            chosen.sort_unstable();
            chosen.dedup();
            chosen
        };
        let mut samples = pick(&lit, SAMPLES * 2 / 3);
        samples.extend(pick(&dark, SAMPLES / 3));
        let samples: Vec<Lexel> = samples.iter().map(|&i| self.lexels[i]).collect();
        // Saturated samples only bound the light from below; leave them out.
        let usable: Vec<f32> = samples
            .iter()
            .map(|s| if s.base.max_element() >= 254.5 / 255.0 { 0.0 } else { 1.0 })
            .collect();
        let target: Vec<Vec3> = samples.iter().map(|s| s.base).collect();
        let energy = |residual: &[Vec3]| -> f64 {
            residual
                .iter()
                .zip(&usable)
                .map(|(r, u)| (r.length_squared() * u) as f64)
                .sum()
        };
        // Per light: its shape on the samples (falloff * N.L * visibility).
        let mut shapes: Vec<Vec<f32>> = vec![];
        let mut lights: Vec<MapLight> = vec![];
        let total = energy(&target);
        let mut residual = target.clone();
        for _ in 0..MAX_LIGHTS {
            let before = energy(&residual);
            let enough = |gain: f64| gain > MIN_GAIN * total.max(1e-9) && gain > MIN_GAIN * before;
            // Seeds above the brightest residual samples, spread apart. When
            // those find nothing worth keeping, seeds on the most strongly
            // coloured leftovers: coloured light (Kitchen's orange stove) is
            // dimmer than the white lights around it, but just as clearly
            // one light.
            let bright = |i: usize| residual[i].element_sum() * usable[i];
            let chroma =
                |i: usize| (residual[i].max_element() - residual[i].min_element().max(0.0)) * usable[i] - 0.1;
            let mut best: Option<Candidate> = None;
            for weight in [&bright as &dyn Fn(usize) -> f32, &chroma] {
                let mut order: Vec<usize> = (0..samples.len()).collect();
                order.sort_by(|&a, &b| weight(b).total_cmp(&weight(a)));
                let mut seeds: Vec<usize> = vec![];
                for &i in &order {
                    if seeds.len() >= 12 || weight(i) <= 0.0 {
                        break;
                    }
                    if seeds
                        .iter()
                        .all(|&s| samples[s].position.distance(samples[i].position) > 12.0)
                    {
                        seeds.push(i);
                    }
                }
                let mut candidates: Vec<(f64, Candidate)> = vec![];
                for &s in &seeds {
                    for height in [0.5, 2.0, 6.0, 16.0, 40.0] {
                        let at = samples[s].position + samples[s].normal * height;
                        let c = self.candidate(at, &samples, &residual, &usable);
                        candidates.push((c.gain, c));
                    }
                }
                candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
                for (_, start) in candidates.into_iter().take(3) {
                    let refined = self.refine(start, &samples, &residual, &usable);
                    if best.as_ref().is_none_or(|b| refined.gain > b.gain) {
                        best = Some(refined);
                    }
                }
                if best.as_ref().is_some_and(|b| enough(b.gain)) {
                    break;
                }
            }
            let Some(best) = best.filter(|b| enough(b.gain)) else {
                break;
            };
            shapes.push(best.shape);
            lights.push(MapLight {
                position: best.position.to_array(),
                color: best.color.to_array(),
                inner: best.inner,
                outer: best.outer,
                channel: None,
            });
            // Joint colour refit: non-negative least squares per channel.
            refit_colors(&mut lights, &shapes, &target, &usable);
            for (i, r) in residual.iter_mut().enumerate() {
                let predicted: Vec3 = lights
                    .iter()
                    .zip(&shapes)
                    .map(|(l, s)| Vec3::from(l.color) * s[i])
                    .sum();
                *r = target[i] - predicted.min(Vec3::ONE);
            }
        }
        lights
    }

    fn candidate(&self, at: Vec3, samples: &[Lexel], residual: &[Vec3], usable: &[f32]) -> Candidate {
        let geometry: Vec<(f32, f32)> = crate::light_volume::parallel(samples.len(), |i| {
            let s = &samples[i];
            let delta = at - s.position;
            let distance = delta.length();
            let facing = facing_term((s.normal.dot(delta) / distance.max(1e-4)).max(0.0));
            if facing <= 0.0 || usable[i] == 0.0 || !self.sees(s.position, s.normal, at) {
                (0.0, distance)
            } else {
                (facing, distance)
            }
        });
        let mut best = Candidate {
            position: at,
            gain: 0.0,
            color: Vec3::ZERO,
            inner: 0.0,
            outer: 1.0,
            shape: vec![],
        };
        let (mut bb, mut br) = ([[0f64; SPAN.len()]; INNER.len()], [[[0f64; 3]; SPAN.len()]; INNER.len()]);
        for ((facing, distance), r) in geometry.iter().zip(residual) {
            if *facing <= 0.0 {
                continue;
            }
            for (a, inner) in INNER.iter().enumerate() {
                for (b, span) in SPAN.iter().enumerate() {
                    let g = (facing * falloff(*distance, *inner, inner + span)) as f64;
                    if g <= 0.0 {
                        continue;
                    }
                    bb[a][b] += g * g;
                    for c in 0..3 {
                        br[a][b][c] += g * r[c] as f64;
                    }
                }
            }
        }
        for (a, inner) in INNER.iter().enumerate() {
            for (b, span) in SPAN.iter().enumerate() {
                if bb[a][b] <= 1e-12 {
                    continue;
                }
                let color = br[a][b].map(|v| (v / bb[a][b]).max(0.0));
                let gain: f64 = (0..3)
                    .map(|c| 2.0 * color[c] * br[a][b][c] - color[c] * color[c] * bb[a][b])
                    .sum();
                if gain > best.gain {
                    best.gain = gain;
                    best.color = Vec3::new(color[0] as f32, color[1] as f32, color[2] as f32);
                    best.inner = *inner;
                    best.outer = inner + span;
                }
            }
        }
        best.shape = geometry
            .iter()
            .map(|(facing, distance)| facing * falloff(*distance, best.inner, best.outer))
            .collect();
        best
    }

    fn refine(&self, mut best: Candidate, samples: &[Lexel], residual: &[Vec3], usable: &[f32]) -> Candidate {
        let mut step = 8.0f32;
        while step >= 0.5 {
            let mut improved = false;
            for axis in [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z] {
                let c = self.candidate(best.position + axis * step, samples, residual, usable);
                if c.gain > best.gain * (1.0 + 1e-4) {
                    best = c;
                    improved = true;
                }
            }
            if !improved {
                step *= 0.5;
            }
        }
        best
    }

    /// Light leaks: the map compiler sometimes let light through thin gaps
    /// and seams between brushes that are sealed in the geometry, leaving
    /// thin bright lines on the far side of a wall (a strip across the
    /// Bedroom floor). A texel is a leak when it is a thin ridge between the
    /// texels `LEAK_STEP` to both sides of it along one axis (on the same
    /// surface), which the map's own walls explain while its neighbours need
    /// no explaining. Authored light: the neighbours hold no more than the
    /// fitted lights give them past the walls, the texel holds more (by more
    /// than the neighbours' own light), and those lights would give it at
    /// least most of that extra with no walls in the way. Baked sun: the bake left the neighbours (nearly) unlit,
    /// and the walls hide the sun from the texel and both neighbours. Inside
    /// a lit patch or glow (a window's sun patch, a stove's panels) the
    /// neighbours are lit too, so nothing there changes. A leak takes the
    /// mean of those two neighbours; everything else stays exactly as baked.
    /// Returns fixes for the drawn lightmaps and their decompositions, in
    /// pairs.
    fn leaks(&self, per_lexel: &[(Vec3, Vec3, Vec3)]) -> Vec<TexelFix> {
        let lookup: Vec<Vec<u32>> = self
            .sheets
            .iter()
            .map(|(_, w, h)| vec![u32::MAX; (*w * *h) as usize])
            .collect();
        let mut lookup = lookup;
        for (i, l) in self.lexels.iter().enumerate() {
            lookup[l.sheet as usize][l.index as usize] = i as u32;
        }
        let luminance = |c: Vec3| c.dot(Vec3::new(0.2126, 0.7152, 0.0722));
        let reach = self
            .bvh
            .bounds_of_all()
            .map_or(1.0, |(min, max)| (max - min).length() * 2.0 + 100.0);
        let toward_sun = -self.sun_direction;
        let fixes: Vec<Option<[TexelFix; 2]>> = crate::light_volume::parallel(self.lexels.len(), |i| {
            let l = &self.lexels[i];
            let sheet = l.sheet as usize;
            let (image, w, h) = self.sheets[sheet];
            let (drawn, (parts_image, parts)) = (&self.drawn[sheet], &self.decomposed[sheet]);
            if drawn.width != w || drawn.height != h || parts.width != w || parts.height != h {
                return None;
            }
            let (x, y) = ((l.index % w) as i64, (l.index / w) as i64);
            let texel = |image: &SceneImage, i: usize| {
                let t = &image.rgba[i * 4..i * 4 + 4];
                (Vec3::new(t[0] as f32, t[1] as f32, t[2] as f32) / 255.0, t[3] as f32 / 255.0)
            };
            // The two neighbours along each axis, when both lie on this
            // surface with this one midway between them.
            let neighbour = |dx: i64, dy: i64| -> Option<usize> {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    return None;
                }
                let n = lookup[sheet][(ny * w as i64 + nx) as usize];
                (n != u32::MAX && self.lexels[n as usize].normal.dot(l.normal) > 0.99).then_some(n as usize)
            };
            let mut best: Option<(f32, usize, usize)> = None;
            let mut best_sun: Option<(f32, usize, usize)> = None;
            for (dx, dy) in [(LEAK_STEP, 0), (0, LEAK_STEP)] {
                let (Some(a), Some(c)) = (neighbour(-dx, -dy), neighbour(dx, dy)) else { continue };
                let (pa, pc) = (self.lexels[a].position, self.lexels[c].position);
                if (l.position - (pa + pc) * 0.5).length() > 0.25 * pa.distance(pc) {
                    continue;
                }
                // A leak is a thin ridge the lights past the walls leave
                // unexplained, between neighbours they explain. Inside a lit
                // patch or glow the fit misplaces (the sun patch through a
                // window, a stove's panels), the neighbours are unexplained
                // too, and nothing changes.
                // And the ridge must outshine its surroundings' own light: a
                // leak is light where there is (nearly) none, never detail in
                // a lit patch the fit happens to explain.
                let unexplained = |n: usize| luminance(self.lexels[n].base - per_lexel[n].0);
                let bright = |n: usize| luminance(self.lexels[n].base);
                let side = bright(a).max(bright(c));
                let ridge = bright(i) - side;
                if unexplained(a).max(unexplained(c)) < LEAK_LEVELS
                    && unexplained(i) >= 0.75 * ridge
                    && side < ridge
                    && ridge > best.map_or(LEAK_LEVELS, |b| b.0)
                {
                    best = Some((ridge, a, c));
                }
                // Baked sun: a thin ridge between neighbours the bake left
                // (nearly) unlit.
                let sun = |n: usize| texel(parts, self.lexels[n].index as usize).1;
                let side = sun(a).max(sun(c));
                let ridge = sun(i) - side;
                if side < 0.5 * LEAK_SUN && ridge > best_sun.map_or(LEAK_SUN, |b| b.0) {
                    best_sun = Some((ridge, a, c));
                }
            }
            // Authored light: the walls must hide at least most of the extra.
            let (visible, _, open) = per_lexel[i];
            let delta = best
                .filter(|(ridge, _, _)| luminance(open - visible) >= 0.75 * ridge)
                .map_or(Vec3::ZERO, |(_, a, c)| {
                    (l.base - (self.lexels[a].base + self.lexels[c].base) * 0.5).max(Vec3::ZERO)
                });
            // Baked sun: the walls hide the sun from this texel.
            let (fixed, share) = texel(parts, l.index as usize);
            let sun_blocked = |n: usize| {
                let l = &self.lexels[n];
                toward_sun.length_squared() > 0.5
                    && l.normal.dot(toward_sun) > 0.01
                    && self.bvh.blocked(l.position + l.normal * LIFT, toward_sun, reach)
            };
            // The walls hide the sun from it and from both neighbours.
            let new_share = best_sun.filter(|&(_, a, c)| sun_blocked(i) && sun_blocked(a) && sun_blocked(c)).map_or(share, |(_, a, c)| {
                (texel(parts, self.lexels[a].index as usize).1 + texel(parts, self.lexels[c].index as usize).1) * 0.5
            });
            if delta.max_element() < 0.5 / 255.0 && share - new_share < 0.5 / 255.0 {
                return None;
            }
            let (mission, _) = texel(drawn, l.index as usize);
            // The drawn lightmap is the static light plus the baked sun.
            let sun_part = (mission - fixed).max(Vec3::ZERO);
            let kept = if share > 0.0 { new_share / share } else { 1.0 };
            let mission = (mission - delta - sun_part * (1.0 - kept)).max(Vec3::ZERO);
            let fixed = (fixed - delta).max(Vec3::ZERO);
            let rgba = |c: Vec3, a: u8| [byte(c.x), byte(c.y), byte(c.z), a];
            let drawn_alpha = drawn.rgba[l.index as usize * 4 + 3];
            Some([
                TexelFix {
                    image: image as u32,
                    index: l.index,
                    rgba: rgba(mission, drawn_alpha),
                },
                TexelFix {
                    image: *parts_image as u32,
                    index: l.index,
                    rgba: rgba(fixed, byte(new_share)),
                },
            ])
        });
        fixes.into_iter().flatten().flatten().collect()
    }

    fn visibility(&self, lights: &[MapLight], min_cell: f32, max_cells: usize) -> VisibilityVolume {
        let (min, max) = self.bvh.bounds_of_all().unwrap_or((Vec3::ZERO, Vec3::ZERO));
        let extent = (max - min).max(Vec3::splat(min_cell));
        let mut cell = min_cell.max(1e-3);
        let dims = |cell: f32| (extent / cell).ceil().as_uvec3() + 2;
        while dims(cell).as_u64vec3().element_product() > max_cells as u64 {
            cell *= 1.1;
        }
        let dims = dims(cell);
        let origin = min - Vec3::splat(cell);
        let reach = extent.length() * 2.0 + 100.0;
        let toward_sun = -self.sun_direction;
        let count = dims.as_u64vec3().element_product() as usize;
        let texels = crate::light_volume::parallel(count, |index| {
            let x = index as u32 % dims.x;
            let y = index as u32 / dims.x % dims.y;
            let z = index as u32 / (dims.x * dims.y);
            let centre = origin + (glam::UVec3::new(x, y, z).as_vec3() + 0.5) * cell;
            let sun = toward_sun.length_squared() > 0.5
                && !self.bvh.blocked(centre, toward_sun, reach);
            let mut texel = [u8::from(sun) * 255, 0, 0, 0, 0, 0, 0, 0];
            for c in 0..CHANNELS {
                // Channels may be shared by lights whose ranges never meet.
                let seen = lights
                    .iter()
                    .filter(|l| l.channel == Some(c as u8))
                    .find(|l| centre.distance(Vec3::from(l.position)) < l.outer + cell)
                    .is_some_and(|l| {
                        let to = Vec3::from(l.position);
                        let d = to - centre;
                        let dist = d.length();
                        dist < 1e-3 || !self.bvh.blocked(centre, d / dist, dist - LIFT)
                    });
                texel[c + 1] = u8::from(seen) * 255;
            }
            texel
        });
        VisibilityVolume {
            origin: origin.to_array(),
            cell,
            dims: dims.to_array(),
            texels,
        }
    }
}

struct Candidate {
    position: Vec3,
    gain: f64,
    color: Vec3,
    inner: f32,
    outer: f32,
    shape: Vec<f32>,
}

// Colour channels index several arrays at once.
#[allow(clippy::needless_range_loop)]
fn refit_colors(lights: &mut [MapLight], shapes: &[Vec<f32>], target: &[Vec3], usable: &[f32]) {
    let k = lights.len();
    // Normal equations A c = b per channel, solved by projected
    // coordinate descent (non-negative colours).
    let mut a = vec![0f64; k * k];
    let mut b = vec![[0f64; 3]; k];
    for i in 0..target.len() {
        if usable[i] == 0.0 {
            continue;
        }
        for p in 0..k {
            let sp = shapes[p][i] as f64;
            if sp == 0.0 {
                continue;
            }
            for c in 0..3 {
                b[p][c] += sp * target[i][c] as f64;
            }
            for q in 0..k {
                a[p * k + q] += sp * shapes[q][i] as f64;
            }
        }
    }
    for c in 0..3 {
        let mut x: Vec<f64> = lights.iter().map(|l| l.color[c] as f64).collect();
        for _ in 0..200 {
            for p in 0..k {
                if a[p * k + p] <= 1e-12 {
                    continue;
                }
                let others: f64 = (0..k).filter(|&q| q != p).map(|q| a[p * k + q] * x[q]).sum();
                x[p] = ((b[p][c] - others) / a[p * k + p]).max(0.0);
            }
        }
        for (l, v) in lights.iter_mut().zip(x) {
            l.color[c] = v as f32;
        }
    }
}

/// Gives the strongest lights visibility channels so that lights whose
/// ranges overlap never share one.
fn assign_channels(lights: &mut [MapLight]) {
    let strength = |l: &MapLight| Vec3::from(l.color).element_sum() * l.outer * l.outer;
    let mut order: Vec<usize> = (0..lights.len()).collect();
    order.sort_by(|&a, &b| strength(&lights[b]).total_cmp(&strength(&lights[a])));
    for &i in &order {
        let used: Vec<u8> = lights
            .iter()
            .filter(|o| {
                o.channel.is_some()
                    && Vec3::from(o.position).distance(Vec3::from(lights[i].position))
                        < o.outer + lights[i].outer
            })
            .filter_map(|o| o.channel)
            .collect();
        lights[i].channel = (0..CHANNELS as u8).find(|c| !used.contains(c));
    }
}

impl MapLighting {
    /// A stored bake: `FORMAT`, the key, a JSON header, visibility texels,
    /// the two residual volumes (each after its length), then the Dynamic
    /// lightmaps' pixels.
    pub fn to_bytes(&self, key: [u8; 32]) -> Vec<u8> {
        let header = serde_json::json!({
            "lights": self.lights, "report": self.report,
            "origin": self.visibility.origin, "cell": self.visibility.cell,
            "dims": self.visibility.dims, "leaks": self.leaks,
            "dynamic": self.dynamic.iter().map(|d| DynamicLayout {
                parts_image: d.parts_image, width: d.width, height: d.height, lights: d.lights.clone(),
            }).collect::<Vec<_>>(),
        })
        .to_string();
        let mut out = FORMAT.to_vec();
        out.extend(key);
        out.extend((header.len() as u32).to_le_bytes());
        out.extend(header.as_bytes());
        out.extend(self.visibility.texels.as_flattened());
        for volume in [&self.residual, &self.residual_all] {
            let bytes = volume.to_bytes();
            out.extend((bytes.len() as u64).to_le_bytes());
            out.extend(bytes);
        }
        for sheet in &self.dynamic {
            out.extend(&sheet.left);
            for v in &sheet.visibility {
                out.extend(v);
            }
        }
        out
    }

    /// None unless `bytes` is a bake `to_bytes` wrote for `key`.
    pub fn from_bytes(bytes: &[u8], key: [u8; 32]) -> Option<Self> {
        let rest = bytes.strip_prefix(FORMAT.as_slice())?.strip_prefix(key.as_slice())?;
        let (len, rest) = rest.split_at_checked(4)?;
        let len = u32::from_le_bytes(len.try_into().ok()?) as usize;
        let (header, rest) = rest.split_at_checked(len)?;
        let header: serde_json::Value = serde_json::from_slice(header).ok()?;
        let dims: [u32; 3] = serde_json::from_value(header["dims"].clone()).ok()?;
        let count = dims.iter().try_fold(1usize, |n, d| n.checked_mul(*d as usize))?;
        let (texels, rest) = rest.split_at_checked(count.checked_mul(8)?)?;
        let lights: Vec<MapLight> = serde_json::from_value(header["lights"].clone()).ok()?;
        let cell: f32 = serde_json::from_value(header["cell"].clone()).ok()?;
        if lights.len() > MAX_LIGHTS || !(cell.is_finite() && cell > 0.0) || dims.contains(&0) {
            return None;
        }
        let mut rest = rest;
        let mut volume = || -> Option<crate::light_volume::LightVolume> {
            let (len, tail) = rest.split_at_checked(8)?;
            let len = usize::try_from(u64::from_le_bytes(len.try_into().ok()?)).ok()?;
            let (bytes, tail) = tail.split_at_checked(len)?;
            rest = tail;
            crate::light_volume::LightVolume::from_bytes(bytes)
        };
        let residual = volume()?;
        let residual_all = volume()?;
        let layouts: Vec<DynamicLayout> = serde_json::from_value(header["dynamic"].clone()).ok()?;
        let mut dynamic = Vec::with_capacity(layouts.len());
        for layout in layouts {
            if layout.lights.len() > DYNAMIC_CHANNELS {
                return None;
            }
            let len = (layout.width as usize).checked_mul(layout.height as usize)?.checked_mul(4)?;
            let mut image = || -> Option<Vec<u8>> {
                let (rgba, tail) = rest.split_at_checked(len)?;
                rest = tail;
                Some(rgba.to_vec())
            };
            let left = image()?;
            let visibility = (0..layout.lights.len().div_ceil(4)).map(|_| image()).collect::<Option<Vec<_>>>()?;
            dynamic.push(DynamicSheet {
                parts_image: layout.parts_image,
                width: layout.width,
                height: layout.height,
                left,
                lights: layout.lights,
                visibility,
            });
        }
        if !rest.is_empty() {
            return None;
        }
        Some(Self {
            lights,
            report: serde_json::from_value(header["report"].clone()).ok()?,
            visibility: VisibilityVolume {
                origin: serde_json::from_value(header["origin"].clone()).ok()?,
                cell,
                dims,
                texels: texels.chunks_exact(8).map(|t| t.try_into().expect("8 bytes")).collect(),
            },
            residual,
            residual_all,
            dynamic,
            leaks: serde_json::from_value(header["leaks"].clone()).ok()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A quad of `scene` with its own 32x32 lightmap (decomposed, no sun)
    /// holding `light` at each texel.
    fn lit_quad(scene: &mut crate::scene::SceneData, corner: impl Fn(f32, f32) -> Vec3, normal: Vec3, light: impl Fn(Vec3) -> Vec3) {
        use crate::scene::{DECOMPOSED_LIGHTMAP, Material, MeshBatch, SceneVertex};
        const SIZE: u32 = 32;
        let at = |t: u32| (t as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
        let base = image(SIZE, SIZE, |x, y| {
            let c = light(corner(at(x), at(y))).min(Vec3::ONE) * 255.0 + 0.5;
            [c.x as u8, c.y as u8, c.z as u8, 255]
        });
        let index = scene.images.len();
        scene.images.push(base.clone());
        let mut parts = base.clone();
        parts.rgba.chunks_exact_mut(4).for_each(|t| t[3] = 0);
        scene.images.push(parts);
        scene.lightmap_bases.push((index, std::sync::Arc::new(base)));
        let mut material = Material::surface("quad", 0, index);
        material.images[9] = index + 1;
        material.parameters = Some(DECOMPOSED_LIGHTMAP);
        let first = scene.vertices.len() as u32;
        for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            scene.vertices.push(SceneVertex {
                position: corner(a, b).to_array(),
                normal: normal.to_array(),
                uv: [0.0; 2],
                lightmap_uv: [(a + 1.0) * 0.5, (b + 1.0) * 0.5],
                color: [1.0; 4],
                fx: [0.0; 4],
            });
        }
        let start = scene.indices.len() as u32;
        scene.indices.extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
        scene.batches.push(MeshBatch {
            indices: start..start + 6,
            material: scene.materials.len(),
            center: [0.0; 3],
        });
        scene.materials.push(material);
    }

    /// A lamp's shade: a band of panels around a light, facing out, away
    /// from it, which the map compiler lit anyway (the Bedroom lamp's shade
    /// glows with the bulb inside). With the light given, as the fit found
    /// it on the Bedroom, the shade's glow is the light's, so it goes out
    /// with it instead of glowing in a dark room.
    #[test]
    fn a_shade_facing_away_from_its_light_goes_dark_with_it() {
        let light = MapLight {
            position: [0.0, 0.0, 0.0],
            color: [0.6, 0.5, 0.4],
            inner: 5.0,
            outer: 25.0,
            channel: Some(0),
        };
        let given = |p: Vec3| Vec3::from(light.color) * falloff(p.length(), light.inner, light.outer);
        let mut scene = crate::scene::SceneData {
            sun_direction: [0.0, -1.0, 0.0],
            ..Default::default()
        };
        for (axis, side) in [(0, -1.0f32), (0, 1.0), (2, -1.0), (2, 1.0)] {
            let mut out = Vec3::ZERO;
            out[axis] = side;
            let corner = move |a: f32, b: f32| {
                let mut p = Vec3::new(0.0, 0.5 * b, 0.0);
                p[axis] = 1.5 * side;
                p[2 - axis] = 1.5 * a;
                p
            };
            lit_quad(&mut scene, corner, out, given);
        }
        // A dark floor far below, out of reach: the compiler's ambient, none.
        lit_quad(&mut scene, |a, b| Vec3::new(40.0 * a, -40.0, 40.0 * b), Vec3::Y, |_| Vec3::ZERO);
        let bake = Bake::new(&scene).expect("lightmapped shade");
        let lights = [light];
        let seen: Vec<u32> = bake
            .lexels
            .iter()
            .map(|l| u32::from(light.shade(l.position, l.normal).max_element() > 0.0 && bake.sees(l.position, l.normal, light.position.into())))
            .collect();
        let sheets = bake.dynamic_sheets(&lights, &seen, &[]);
        for sheet in &sheets[..4] {
            let worst = sheet.left.chunks_exact(4).map(|t| t[..3].iter().copied().max().unwrap_or(0)).max().unwrap_or(0);
            assert!(worst <= 2, "a side of the shade keeps {worst} levels with its light off");
        }
    }

    fn image(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> SceneImage {
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgba.extend(f(x, y));
            }
        }
        SceneImage {
            label: "t".into(),
            width: w,
            height: h,
            rgba,
            srgb: false,
        }
    }

    #[test]
    fn raster_covers_inside_and_rim() {
        let mut inside = 0;
        let mut rim = 0;
        raster(
            [8, 8],
            [[0.0, 0.0], [0.5, 0.0], [0.0, 0.5]],
            1.5,
            |_, _, w, i| {
                assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-4);
                if i { inside += 1 } else { rim += 1 }
            },
        );
        // Centres strictly below the diagonal x + y < 4 of a 4x4 corner.
        assert_eq!(inside, 10);
        assert!(rim > 0);
    }

    #[test]
    fn decomposition_reconstructs_the_mission_lightmap() {
        let sun = BakeSun {
            direction: Vec3::new(0.0, -0.8, -0.6).normalize(),
            color: Vec3::new(0.7, 0.7, 0.6),
            ambient: Vec3::new(0.4, 0.4, 0.3),
        };
        let normal = Vec3::Y;
        let facing = normal.dot(-sun.direction);
        // Base light ramps across; the right half is in baked shadow.
        let base = image(16, 16, |x, _| [(x * 12) as u8, (x * 12) as u8, 40, 255]);
        let mission = image(16, 16, |x, _| {
            let b = Vec3::new((x * 12) as f32, (x * 12) as f32, 40.0) / 255.0;
            let lit = if x < 8 { 1.0 } else { 0.0 };
            let m = (b + sun.ambient).min(Vec3::ONE) + sun.color * facing * lit;
            [byte(m.x), byte(m.y), byte(m.z), 255]
        });
        let surface = SheetSurface {
            triangles: vec![
                [([0.0; 3], [0.0, 0.0]), ([0.0; 3], [1.0, 0.0]), ([0.0; 3], [1.0, 1.0])],
                [([0.0; 3], [0.0, 0.0]), ([0.0; 3], [1.0, 1.0]), ([0.0; 3], [0.0, 1.0])],
            ],
            normal,
            outside: true,
        };
        let out = decompose_sheet(&base, Some(&mission), &[surface], Some(sun));
        for i in 0..256 {
            let t: [u8; 4] = out.rgba[i * 4..i * 4 + 4].try_into().unwrap();
            let r = reconstruct(t, sun, normal) * 255.0;
            for c in 0..3 {
                assert!((r[c] - mission.rgba[i * 4 + c] as f32).abs() <= 1.01, "{i}");
            }
            let shadowed = (i % 16) >= 8;
            assert_eq!(t[3] < 8, shadowed, "texel {i} visibility {}", t[3]);
        }
    }

    #[test]
    fn no_sun_keeps_the_lightmap_static() {
        let base = image(4, 4, |_, _| [90, 80, 70, 255]);
        let out = decompose_sheet(&base, None, &[], None);
        assert!(out.rgba.chunks_exact(4).all(|p| p == [90, 80, 70, 0]));
    }
}
