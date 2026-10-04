//! Cascaded sun shadow maps. v20 drew per-shape projected shadows from
//! players, vehicles and items (quality from `setShadowResolution`), never
//! from bricks, and baked the map's own shadows into lightmaps. Here those
//! casters render into stabilized cascades instead; bricks are optional.
//! Each cascade fades into the next over the last part of its range, so the
//! step to coarser texels never shows as a line.
//!
//! Map geometry never casts. v20 lit bricks and players by the sun even
//! inside the Bedroom and Kitchen interiors, so map shadows on bricks would
//! darken nearly every indoor build (checked with Cottage and Town renders).
//! Lightmapped surfaces (which cannot separate their baked sun from other
//! light) darken by a bounded fixed share, as v20's projected shadows did.
//!
//! Bricks that do not cast (Brick Shadows off) still stop a shadow: they
//! render into a second, occluder depth map, and a caster's shadow is
//! dropped wherever an occluder lies between the caster and the receiving
//! surface. A player on a brick tower shades the tower top, not the floor
//! beneath it. Occluders write only past the caster along the sun (they
//! read the finished caster layer), so the map keeps the first surface
//! below the caster; a ceiling or overhang above the player would otherwise
//! hide the tower and let the shadow through.
//!
//! The map (interiors and terrain) is not an occluder either. Its shadows
//! are baked, as engines with baked static lighting treat static geometry
//! for movable casters: a beam or shelf between a build and a wall would
//! otherwise erase the build's shadow in the beam's silhouette while casting
//! none of its own, leaving lit cut-outs. Where the map really blocks the
//! sun, the receiver is in the map's shadow anyway, so the caster's shadow
//! continuing there is what a fully lit scene would show.
//!
//! Lamps: in the Unified lighting modes the nearest, strongest of a map's
//! recovered lights (`crate::map_lighting`) also cast live shadows, from a
//! cube of six perspective maps each, so a build on the Bedroom dresser or
//! a player by the Kitchen stove shades what the lamp lights. How many lamps
//! and how sharp follow the Shadow Quality setting. The same casters render
//! into them (the map's own lamp shadows are baked). The faces are tiles in
//! extra layers of the sun's map array, after the cascades' layers (a stage
//! may bind only 16 textures).
//!
//! Each lamp also keeps a coarse cube of the map's own surfaces (drawn once
//! when the lamp takes its slot), which says whether its light reaches a
//! point at all: on the map's surfaces, whose live lamp shadows take away
//! only light the lamp gave them, and on objects, which it lights. The map's
//! visibility volume is a few units coarse, and beside furniture its cells
//! can sit inside the geometry and hide a lamp from everything next to it
//! (the Bedroom desk lamp from a player on the dresser); it stays for the
//! lights without a shadow slot.
//!
//! The map's own sun shadows reach objects the same way the map's baked
//! sun reached its walls. In the Unified modes the map's opaque interior
//! surfaces render into a third layer per cascade (never a caster or an
//! occluder of the first two, so the map's baked look is untouched), which
//! only bricks, players, items and vehicles read: sun through a window
//! lands on a build with the same filtered edge as any live shadow, and an
//! object is sunlit exactly where the walls beside it are. That layer
//! reaches much farther toward the sun than the casters' (`MAP_REACH`):
//! the map is large and its walls stand far from the eye.
//!
//! Dynamic shades map surfaces and objects from the live sun, ambient and
//! recovered light parameters. Every recovered map light keeps a map-geometry
//! cube, populated 24 faces per frame. No legacy lightmap, visibility volume or
//! residual is sampled. Geometry changes invalidate the cubes. The nearest
//! quality-limited lamps additionally receive live brick and moving shadows;
//! sun cascades read current opaque map geometry including terrain and models.
//! Past the finite cascade range the sun is unshadowed rather than baked.
//!
//! Bricks hardly ever move, so their lamp faces are kept: a face is drawn
//! again only when its lamp or view changes, or when the static chunks
//! inside it change (a brick placed or removed there shows in its shadow
//! the same frame), plus one face a frame in turn as a backstop. Players,
//! vehicles and items draw every frame into their own, coarser faces; a
//! receiver is lit by the lamp only where neither shades it.
//! Inside a million-brick build this keeps lamp shadows to a few percent.
use crate::map_lighting::MAX_LIGHTS;
use crate::scene::{FAR_DEPTH, NEAR_DEPTH};
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3, Vec4};

pub const MAX_CASCADES: usize = 4;
/// Lamps that may cast live shadows at once (Best quality).
pub const MAX_LAMPS: usize = 4;
/// Cube faces, in the order the shader picks them: +X, -X, +Y, -Y, +Z, -Z.
const FACES: usize = 6;
/// Lamp shadows reach no nearer to their lamp than this.
pub(crate) const LAMP_NEAR: f32 = 0.05;
/// Lights reaching farther than this are the fit's broad fill (bounced
/// light spread over a room), not lamps: a shadow from one would be a long
/// smear across the room.
const LAMP_MAX_REACH: f32 = 200.0;
/// Extra texels each face covers past 90 degrees, so filter taps at a
/// face's edge stay inside it.
pub(crate) const LAMP_MARGIN_TEXELS: f32 = 3.0;
pub const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Cascade radii are multiples of this (world units).
const RADIUS_STEP: f32 = 0.25;
/// Relative rounding noise in a slice's measured radius as the camera
/// turns (it is 1e-4 at a 1000-unit far plane).
const RADIUS_NOISE: f32 = 1e-3;
/// Casters this far beyond a cascade toward the sun still cast into it.
pub(crate) const CASTER_REACH: f32 = 400.0;
/// Map surfaces this far beyond a cascade toward the sun still shade it
/// (the map layer): past the stock maps' extent.
const MAP_REACH: f32 = 10000.0;
/// Caster uniform stride; dynamic offsets must be 256-byte aligned.
const CASTER_STRIDE: u64 = 256;
/// Caster uniform: light matrix, then the occluder gap (padded to a vec4).
const CASTER_SIZE: u64 = 80;
/// Occluders must lie this many world units past a caster to stop its
/// shadow, so the brick a player stands on still receives it.
const OCCLUDER_GAP: f32 = 0.1;

/// Sun shadow quality: how many cascades, their square resolution, and how
/// far from the eye shadows reach before fading out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSettings {
    pub cascades: u32,
    pub resolution: u32,
    pub distance: f32,
    /// Map lamps casting live shadows in the Unified lighting modes, and
    /// the square resolution of each of their six faces (a divisor of
    /// `resolution`: faces are tiles of the sun's map layers).
    pub lamps: u32,
    pub lamp_resolution: u32,
    /// Dynamic: every recovered map light keeps a geometry-derived cube.
    /// Map surfaces and objects both read it; no legacy visibility channel.
    /// Cached faces are rebuilt when map geometry or light parameters change.
    pub light_cubes: bool,
}
impl ShadowSettings {
    pub const BEST: Self = Self {
        cascades: 4,
        resolution: 2048,
        distance: 320.0,
        lamps: 4,
        lamp_resolution: 1024,
        light_cubes: false,
    };
    pub const HIGH: Self = Self {
        cascades: 3,
        resolution: 2048,
        distance: 240.0,
        lamps: 2,
        lamp_resolution: 512,
        light_cubes: false,
    };
    pub const MEDIUM: Self = Self {
        cascades: 3,
        resolution: 1024,
        distance: 160.0,
        lamps: 1,
        lamp_resolution: 512,
        light_cubes: false,
    };
    pub const LOW: Self = Self {
        cascades: 2,
        resolution: 1024,
        distance: 100.0,
        lamps: 0,
        lamp_resolution: 256,
        light_cubes: false,
    };
    /// Faces of moving casters (players, vehicles, items): half the
    /// resolution of the kept brick faces.
    pub fn lamp_dynamic_resolution(&self) -> u32 {
        (self.lamp_resolution / 2).max(64)
    }
    /// Faces of `size` per row of a layer, and the layers all lamps' fill.
    fn tiles_of(&self, size: u32) -> u32 {
        (self.resolution / size.max(1)).max(1)
    }
    fn layers_of(&self, size: u32) -> u32 {
        let per_layer = self.tiles_of(size) * self.tiles_of(size);
        (self.lamps * FACES as u32).div_ceil(per_layer)
    }
    /// Each Dynamic map light face: moving-caster resolution, at least
    /// 256 (512 at Best). Finite MAX_LIGHTS x 6 faces in the shared atlas.
    pub fn cube_resolution(&self) -> u32 {
        self.lamp_dynamic_resolution().max(256).min(self.resolution)
    }
    /// The map lights' cubes' layers, after the lamps' (none unless
    /// `light_cubes`).
    pub fn cube_layers(&self) -> u32 {
        if !self.light_cubes {
            return 0;
        }
        let tiles = self.tiles_of(self.cube_resolution());
        (crate::map_lighting::MAX_LIGHTS as u32 * FACES as u32).div_ceil(tiles * tiles)
    }
    /// Map light `index / 6`'s cube face `index % 6`: layer and texel
    /// rectangle.
    pub(crate) fn cube_tile(&self, index: usize) -> (u32, [u32; 3]) {
        self.tile_in(
            self.cube_resolution(),
            self.cascade_layers() + self.lamp_layers(),
            index,
        )
    }
    /// Per cascade: casters, occluders and the map, before the lamps.
    pub fn cascade_layers(&self) -> u32 {
        self.cascades * 3
    }
    /// Brick faces' layers, then moving casters' layers, then the map's.
    pub fn lamp_layers(&self) -> u32 {
        self.layers_of(self.lamp_resolution) + 2 * self.layers_of(self.lamp_dynamic_resolution())
    }
    fn tile_in(&self, size: u32, first_layer: u32, index: usize) -> (u32, [u32; 3]) {
        let tiles = self.tiles_of(size);
        let (layer, tile) = (
            index as u32 / (tiles * tiles),
            index as u32 % (tiles * tiles),
        );
        (
            first_layer + layer,
            [(tile % tiles) * size, (tile / tiles) * size, size],
        )
    }
    /// A lamp face's kept brick tile: layer and texel rectangle (x, y, size).
    pub(crate) fn lamp_tile(&self, index: usize) -> (u32, [u32; 3]) {
        self.tile_in(self.lamp_resolution, self.cascade_layers(), index)
    }
    /// A lamp face's moving-caster tile.
    pub(crate) fn lamp_dynamic_tile(&self, index: usize) -> (u32, [u32; 3]) {
        let first = self.cascade_layers() + self.layers_of(self.lamp_resolution);
        self.tile_in(self.lamp_dynamic_resolution(), first, index)
    }
    /// A lamp face's kept map tile, the size of a moving-caster tile: the
    /// map only tells whether the lamp reaches a point at all.
    pub(crate) fn lamp_map_tile(&self, index: usize) -> (u32, [u32; 3]) {
        let dynamic = self.lamp_dynamic_resolution();
        let first =
            self.cascade_layers() + self.layers_of(self.lamp_resolution) + self.layers_of(dynamic);
        self.tile_in(dynamic, first, index)
    }
    pub fn validate(&self, device: &wgpu::Device) -> Result<()> {
        ensure!(
            (1..=MAX_CASCADES as u32).contains(&self.cascades)
                && (256..=device.limits().max_texture_dimension_2d).contains(&self.resolution)
                && self.cascade_layers() <= device.limits().max_texture_array_layers
                && self.distance.is_finite()
                && (10.0..=2000.0).contains(&self.distance)
                && self.lamps <= MAX_LAMPS as u32
                && (self.lamps == 0
                    || ((64..=self.resolution).contains(&self.lamp_resolution)
                        && self.resolution.is_multiple_of(self.lamp_resolution)
                        && self
                            .resolution
                            .is_multiple_of(self.lamp_dynamic_resolution())))
                && self.resolution.is_multiple_of(self.cube_resolution())
                && self.cascade_layers() + self.lamp_layers() + self.cube_layers()
                    <= device.limits().max_texture_array_layers,
            "Invalid shadow settings {self:?}"
        );
        Ok(())
    }
}

/// Receiver uniform: cascade matrices, far split distances, world size of
/// one texel per cascade, view forward + cascade count, and map resolution.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ShadowUniform {
    matrices: [[f32; 16]; MAX_CASCADES],
    splits: [f32; 4],
    texels: [f32; 4],
    forward_count: [f32; 4],
    params: [f32; 4],
    /// Shadow-map depth per world unit along the sun, per cascade.
    depth_scale: [f32; 4],
    /// The eye the cascades were fitted from: every view (a mirror's too)
    /// measures cascade distances from it.
    origin: [f32; 4],
    /// Per lamp slot, its six face matrices.
    lamp_faces: [[f32; 16]; MAX_LAMPS * FACES],
    /// Per shaded map light (as the shader indexes them), its lamp slot, or
    /// -1 without one.
    light_slots: [[f32; 4]; MAX_LIGHTS / 4],
    /// Lamps in use, face resolution, world texel size per unit of distance,
    /// and the eye distance lamp shadows fade out by.
    lamp_params: [f32; 4],
    /// Kept brick faces: first layer, faces per row, a face's share of a
    /// layer.
    lamp_atlas: [f32; 4],
    /// Moving casters' faces: the same, then their resolution.
    lamp_dynamic: [f32; 4],
    /// Per slot: lamp position and its shadow's reach.
    lamp_centers: [[f32; 4]; MAX_LAMPS],
    /// Per cascade, a caster depth `z` in the map layer is `z * map_scale +
    /// map_offset` (the map layer's longer reach toward the sun).
    map_scale: [f32; 4],
    map_offset: [f32; 4],
    /// x: 1 when the map layers (and the lamps' map faces) hold the map
    /// this frame; y: the lamps' first map layer (laid out as `lamp_dynamic`).
    map_params: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Cascade {
    pub view_projection: Mat4,
    /// The same map, reaching `MAP_REACH` toward the sun (the map layer).
    pub map_view_projection: Mat4,
    /// Caster depth to map-layer depth: `z * scale + offset`.
    pub map_depth: [f32; 2],
    pub far: f32,
    pub texel: f32,
    pub depth_scale: f32,
    /// Half the map's width in world units (a multiple of `RADIUS_STEP`).
    pub radius: f32,
    /// Light space (`light_rotation`): the map's centre in whole texels,
    /// its half width in texels, and its depth range along the sun, near
    /// then far (world units; the view looks down -z, so depth is -z).
    pub center_texels: [i64; 2],
    pub half_texels: i64,
    pub depth_range: [f32; 2],
}

/// Light space for sun direction `sun` (normalized): x and y across the
/// sun, looking down -z along it. Cascades snap to texels in it.
pub(crate) fn light_rotation(sun: Vec3) -> Mat4 {
    let up = if sun.dot(Vec3::Y).abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    glam::camera::rh::view::look_to_mat4(Vec3::ZERO, sun, up)
}

/// Split the view from its near plane to `settings.distance` and fit each
/// slice with a texel-snapped sphere in light space, so cascades do not
/// shimmer as the camera turns or moves.
#[cfg(test)]
pub(crate) fn cascades(
    view_projection: Mat4,
    eye: Vec3,
    sun_direction: Vec3,
    settings: &ShadowSettings,
) -> Option<(Vec<Cascade>, Vec3)> {
    cascades_after(view_projection, eye, sun_direction, settings, &[])
}
/// `cascades`, keeping each cascade's radius from the `previous` frame's
/// while the slice still fits it (within rounding noise) and needs no less
/// than a step below it.
/// A slice's bounding sphere does not change as the camera turns, but the
/// corners it is measured from carry rounding noise, which would flip the
/// quantized radius (and so the texel size and grid) between two steps as
/// the camera turns: shimmering edges, and kept brick layers
/// (`crate::kept_shadows`) that no longer line up.
pub(crate) fn cascades_after(
    view_projection: Mat4,
    eye: Vec3,
    sun_direction: Vec3,
    settings: &ShadowSettings,
    previous: &[Cascade],
) -> Option<(Vec<Cascade>, Vec3)> {
    let sun = sun_direction.normalize_or_zero();
    let inverse = view_projection.inverse();
    if sun == Vec3::ZERO || !inverse.is_finite() {
        return None;
    }
    let corner = |x: f32, y: f32, z: f32| inverse.project_point3(Vec3::new(x, y, z));
    let far: Vec<Vec3> = [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)]
        .iter()
        .map(|&(x, y)| corner(x, y, FAR_DEPTH))
        .collect();
    let near_center = corner(0.0, 0.0, NEAR_DEPTH);
    let far_center = corner(0.0, 0.0, FAR_DEPTH);
    let forward = (far_center - near_center).normalize_or_zero();
    let far_depth = (far_center - eye).dot(forward);
    let near_depth = (near_center - eye).dot(forward).max(0.01);
    if forward == Vec3::ZERO
        || far_depth.partial_cmp(&near_depth) != Some(std::cmp::Ordering::Greater)
    {
        return None;
    }
    let distance = settings.distance.min(far_depth);
    let count = settings.cascades as usize;
    // Practical split scheme: mostly logarithmic, partly uniform.
    let splits: Vec<f32> = (1..=count)
        .map(|i| {
            let t = i as f32 / count as f32;
            let log = near_depth * (distance / near_depth).powf(t);
            let uniform = near_depth + (distance - near_depth) * t;
            0.75 * log + 0.25 * uniform
        })
        .collect();
    let rotation = light_rotation(sun);
    // Each corner ray scaled by its own depth along the view: the far
    // corners come back from the inverse projection with rounding noise in
    // depth, which would otherwise skew the slice as the camera turns.
    let point_at = |ray: Vec3, depth: f32| eye + (ray - eye) * (depth / (ray - eye).dot(forward));
    let mut start = near_depth;
    let mut out = Vec::with_capacity(count);
    for split in splits {
        let slice: Vec<Vec3> = far
            .iter()
            .flat_map(|&ray| [point_at(ray, start), point_at(ray, split)])
            .collect();
        let center = slice.iter().copied().sum::<Vec3>() / slice.len() as f32;
        let radius = slice
            .iter()
            .map(|p| p.distance(center))
            .fold(0.0f32, f32::max);
        // Quantize the radius so the texel size (and snapping grid) is stable.
        let radius = match previous.get(out.len()).map(|c| c.radius) {
            Some(kept) if radius <= kept * (1.0 + RADIUS_NOISE) && radius > kept - RADIUS_STEP => {
                kept
            }
            _ => (radius / RADIUS_STEP).ceil() * RADIUS_STEP,
        };
        let texel = radius * 2.0 / settings.resolution as f32;
        let light = rotation.transform_point3(center);
        let (x, y) = (
            (light.x / texel).round() * texel,
            (light.y / texel).round() * texel,
        );
        // Right-handed view looks down -Z: depth along the sun is -z.
        let depth = -light.z;
        let projection = glam::camera::rh::proj::directx::orthographic(
            x - radius,
            x + radius,
            y - radius,
            y + radius,
            depth - radius - CASTER_REACH,
            depth + radius,
        );
        let map_projection = glam::camera::rh::proj::directx::orthographic(
            x - radius,
            x + radius,
            y - radius,
            y + radius,
            depth - radius - MAP_REACH,
            depth + radius,
        );
        let span = 2.0 * radius + CASTER_REACH;
        let map_span = 2.0 * radius + MAP_REACH;
        out.push(Cascade {
            view_projection: projection * rotation,
            map_view_projection: map_projection * rotation,
            map_depth: [span / map_span, (MAP_REACH - CASTER_REACH) / map_span],
            far: split,
            texel,
            depth_scale: 1.0 / (2.0 * radius + CASTER_REACH),
            radius,
            center_texels: [
                (light.x / texel).round() as i64,
                (light.y / texel).round() as i64,
            ],
            half_texels: i64::from(settings.resolution / 2),
            depth_range: [depth - radius - CASTER_REACH, depth + radius],
        });
        start = split;
    }
    Some((out, forward))
}

/// A map light that may cast lamp shadows (the shaded lights, in the order
/// the shader indexes them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LampLight {
    pub position: Vec3,
    pub color: Vec3,
    pub outer: f32,
}

/// A lamp chosen to cast this frame: its light and six face matrices.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lamp {
    pub light: usize,
    pub faces: [Mat4; FACES],
    /// Where the lamp stands and how far its shadow reaches.
    pub center: Vec3,
    pub reach: f32,
}

/// The lamps worth a shadow this frame: lights whose reach the view sees,
/// ranked by how much of their light reaches around the eye, past the map's
/// walls (`seen`: the share of the eye's surroundings the light reaches, so
/// a bulb shut inside its lamp shade never takes a slot from the light
/// falling on the player). Lamps already casting keep a lead, so the choice
/// does not flicker as the eye moves.
pub(crate) fn pick_lamps(
    lights: &[LampLight],
    eye: Vec3,
    budget: usize,
    previous: &[usize],
    in_view: impl Fn(Vec3, f32) -> bool,
    seen: impl Fn(usize) -> f32,
) -> Vec<usize> {
    let mut scored: Vec<(f32, usize)> = lights
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            (LAMP_NEAR..=LAMP_MAX_REACH).contains(&l.outer) && in_view(l.position, l.outer)
        })
        .filter_map(|(i, l)| {
            let brightness = l.color.dot(Vec3::new(0.2126, 0.7152, 0.0722));
            // Light near the eye: what the lamp gives the nearest things
            // the player looks at (the eye itself may sit past its reach).
            let reach = 1.0 - (l.position.distance(eye) - 8.0).max(0.0) / l.outer;
            let lead = if previous.contains(&i) { 1.5 } else { 1.0 };
            // The visibility volume is coarse (its cells can sit inside
            // furniture beside the eye), so a lamp it hides only ranks lower.
            let score = brightness * reach * lead * (0.25 + 0.75 * seen(i));
            (reach > 0.0 && score > 0.01).then_some((score, i))
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(budget).map(|(_, i)| i).collect()
}

/// Six perspective faces around `position` out to `outer`, each a little
/// wider than 90 degrees (see `LAMP_MARGIN_TEXELS`).
pub(crate) fn lamp_faces(position: Vec3, outer: f32, resolution: u32) -> [Mat4; FACES] {
    let half = 1.0 + 2.0 * LAMP_MARGIN_TEXELS / resolution as f32;
    let projection =
        glam::camera::rh::proj::directx::perspective(2.0 * half.atan(), 1.0, LAMP_NEAR, outer);
    let looks = [
        (Vec3::X, Vec3::NEG_Y),
        (Vec3::NEG_X, Vec3::NEG_Y),
        (Vec3::Y, Vec3::Z),
        (Vec3::NEG_Y, Vec3::NEG_Z),
        (Vec3::Z, Vec3::NEG_Y),
        (Vec3::NEG_Z, Vec3::NEG_Y),
    ];
    looks.map(|(forward, up)| {
        projection * glam::camera::rh::view::look_to_mat4(position, forward, up)
    })
}

/// Admission uses the same six-face construction as rendering, rather than a
/// coordinate envelope. All face axes are checked. Resolution changes only the
/// lateral margin (scale <= 1); the overflow-prone depth coefficients are the
/// same at every quality, so the standard cube faces cover that contract.
pub(crate) fn finite_lamp_faces(position: Vec3, outer: f32) -> bool {
    position.is_finite()
        && outer.is_finite()
        && outer > LAMP_NEAR
        && lamp_faces(position, outer, ShadowSettings::BEST.cube_resolution())
            .iter()
            .all(|face| face.to_cols_array().iter().all(|value| value.is_finite()))
}

/// Freshness is distinct from availability. A geometry change schedules new
/// faces but the previous runtime geometry cubes remain usable for the same
/// lamp projectors until their replacements are drawn. A source change has no
/// such history: its missing faces are explicitly unavailable.
#[derive(Default)]
struct CubeCache {
    key: Vec<usize>,
    projectors: Vec<Option<(Vec3, f32)>>,
    drawn: Vec<Option<Mat4>>,
    available: bool,
}
impl CubeCache {
    fn refresh(
        &mut self,
        settings: ShadowSettings,
        key: &[usize],
        lights: &[Option<(Vec3, f32)>],
        budget: usize,
    ) -> (Vec<(usize, usize, Mat4)>, bool) {
        if key.is_empty() {
            *self = Self::default();
            return (Vec::new(), false);
        }
        let lights = &lights[..lights.len().min(crate::map_lighting::MAX_LIGHTS)];
        if self.projectors.as_slice() != lights {
            self.drawn.clear();
            self.available = false;
            self.projectors = lights.to_vec();
        }
        if self.key.as_slice() != key {
            self.drawn.clear();
            self.key = key.to_vec();
        }
        self.drawn.resize(lights.len() * FACES, None);
        self.drawn.truncate(lights.len() * FACES);
        let mut stale = Vec::new();
        let mut fresh = true;
        for (light, cube) in lights.iter().enumerate() {
            let Some((position, reach)) = *cube else {
                continue;
            };
            let faces = lamp_faces(position, reach, settings.cube_resolution());
            for (face, matrix) in faces.iter().enumerate() {
                let index = light * FACES + face;
                if self.drawn[index] != Some(*matrix) {
                    if stale.len() < budget {
                        self.drawn[index] = Some(*matrix);
                        stale.push((light, face, *matrix));
                    } else {
                        fresh = false;
                    }
                }
            }
        }
        self.available |= fresh;
        (stale, self.available)
    }
}

/// Shadow map textures, uniforms and caster pipelines. Disabled shadows keep
/// a 1x1 map and a zero cascade count so receivers need no variant.
pub(crate) struct ShadowMaps {
    pub settings: Option<ShadowSettings>,
    pub array_view: wgpu::TextureView,
    pub layer_views: Vec<wgpu::TextureView>,
    pub comparison: wgpu::Sampler,
    /// Nearest sampling for gathering caster and occluder depths.
    pub point: wgpu::Sampler,
    pub receiver: wgpu::Buffer,
    caster: wgpu::Buffer,
    pub caster_group: wgpu::BindGroup,
    /// Per cascade: the caster group plus that cascade's caster depth, which
    /// occluders test against.
    pub occluder_groups: Vec<wgpu::BindGroup>,
    /// Opaque (depth only), alpha-masked and clip-plane-cut opaque caster
    /// pipelines.
    pub pipelines: [wgpu::RenderPipeline; 3],
    /// Opaque, alpha-masked and cut opaque occluder pipelines.
    pub occluder_pipelines: [wgpu::RenderPipeline; 3],
    pub cascades: Vec<Cascade>,
    /// The sun direction the cascades were fitted to (normalized).
    pub sun: Vec3,
    /// Per slot, the lamp casting there (a lamp keeps its slot while it
    /// casts, so its kept faces stay valid).
    pub lamps: Vec<Option<Lamp>>,
    /// Per kept brick face: the matrix it was last drawn with.
    drawn: Vec<Option<Mat4>>,
    /// Per kept brick face: draw it this frame.
    pub stale: Vec<bool>,
    /// The kept face refreshed last, in turn.
    refresh: usize,
    /// Per kept brick face: what it was last drawn from (`kept_casters`).
    kept_casters: std::cell::RefCell<Vec<Option<u64>>>,
    /// Clears one tile of a layer (depth 1) before a kept face redraws.
    pub clear_pipeline: wgpu::RenderPipeline,
    /// Per lamp face: the matrix its kept map face was drawn with, and the
    /// map it was drawn from. The map never changes while it is loaded, so
    /// a map face is drawn only when its lamp takes a slot.
    map_faces: std::cell::RefCell<(Vec<usize>, Vec<Option<Mat4>>)>,
    /// Per map light face (Dynamic mode): the matrix its cube face was
    /// drawn with, and the map it was drawn from.
    cubes: std::cell::RefCell<CubeCache>,
}
impl ShadowMaps {
    pub fn new(
        device: &wgpu::Device,
        settings: Option<ShadowSettings>,
        material_layout: &wgpu::BindGroupLayout,
        vertex_layouts: &[Option<wgpu::VertexBufferLayout<'_>>],
    ) -> Self {
        // Per cascade: caster depth, then (after all cascades) occluder
        // depth, then map depth, then the lamp faces' layers.
        let (size, layers) = settings.map_or((1, 3), |s| {
            (
                s.resolution,
                s.cascade_layers() + s.lamp_layers() + s.cube_layers(),
            )
        });
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sun shadow maps"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SHADOW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let array_view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let layer_views: Vec<_> = (0..layers)
            .map(|layer| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let comparison = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sun shadow comparison"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let point = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sun shadow depth gather"),
            ..Default::default()
        });
        use wgpu::util::DeviceExt;
        let receiver = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sun shadow receiver uniform"),
            contents: bytemuck::bytes_of(&ShadowUniform::zeroed_disabled()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let caster = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sun shadow caster matrices"),
            size: CASTER_STRIDE
                * (MAX_CASCADES * 3 + MAX_LAMPS * FACES + crate::map_lighting::MAX_LIGHTS * FACES)
                    as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let caster_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sun shadow caster"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(CASTER_SIZE),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let occluder_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sun shadow occluder"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(CASTER_SIZE),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let mask_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let caster_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sun shadow caster"),
            layout: &caster_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &caster,
                        offset: 0,
                        size: wgpu::BufferSize::new(CASTER_SIZE),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&mask_sampler),
                },
            ],
        });
        // Caster layers come first, so layer i is cascade i's caster depth.
        let cascade_count = settings.map_or(1, |s| s.cascades as usize);
        let occluder_groups = (0..cascade_count)
            .map(|cascade| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sun shadow occluder"),
                    layout: &occluder_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &caster,
                                offset: 0,
                                size: wgpu::BufferSize::new(CASTER_SIZE),
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&mask_sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&layer_views[cascade]),
                        },
                    ],
                })
            })
            .collect();
        // Opaque casters need no material, so whole chunks draw without
        // rebinding; masked casters sample their material's alpha.
        let opaque_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sun shadow casters"),
            bind_group_layouts: &[Some(&caster_layout)],
            immediate_size: 0,
        });
        let masked_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sun shadow masked casters"),
            bind_group_layouts: &[Some(&caster_layout), Some(material_layout)],
            immediate_size: 0,
        });
        let occluder_opaque_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sun shadow occluders"),
                bind_group_layouts: &[Some(&occluder_layout)],
                immediate_size: 0,
            });
        let occluder_masked_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sun shadow masked occluders"),
                bind_group_layouts: &[Some(&occluder_layout), Some(material_layout)],
                immediate_size: 0,
            });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sun shadow casters"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shadow.wgsl").into()),
        });
        let pipeline = |label: &str, layout: &wgpu::PipelineLayout, fragment: Option<&str>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: vertex_layouts,
                },
                // Single-sided map geometry must still block the sun.
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: SHADOW_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState {
                        constant: 2,
                        slope_scale: 2.0,
                        clamp: 0.0,
                    },
                }),
                multisample: Default::default(),
                fragment: fragment.map(|entry| wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let clear_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lamp face clear"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let clear_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lamp face clear"),
            layout: Some(&clear_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_clear"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        Self {
            settings,
            array_view,
            layer_views,
            comparison,
            point,
            receiver,
            caster,
            caster_group,
            occluder_groups,
            pipelines: [
                pipeline("sun shadow casters", &opaque_layout, None),
                pipeline(
                    "sun shadow masked casters",
                    &masked_layout,
                    Some("fs_masked"),
                ),
                pipeline("sun shadow cut casters", &opaque_layout, Some("fs_clipped")),
            ],
            occluder_pipelines: [
                pipeline(
                    "sun shadow occluders",
                    &occluder_opaque_layout,
                    Some("fs_occluder"),
                ),
                pipeline(
                    "sun shadow masked occluders",
                    &occluder_masked_layout,
                    Some("fs_occluder_masked"),
                ),
                pipeline(
                    "sun shadow cut occluders",
                    &occluder_opaque_layout,
                    Some("fs_occluder"),
                ),
            ],
            cascades: Vec::new(),
            sun: Vec3::ZERO,
            lamps: Vec::new(),
            drawn: Vec::new(),
            stale: Vec::new(),
            refresh: 0,
            kept_casters: Default::default(),
            clear_pipeline,
            map_faces: Default::default(),
            cubes: Default::default(),
        }
    }
    /// Fit cascades to this frame's camera, pick the lamps that cast (from
    /// `lamps`; none outside the Unified modes) and upload caster/receiver
    /// data.
    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        eye: Vec3,
        sun: Vec3,
        lamps: &[LampLight],
        seen: &[f32],
    ) {
        let fitted = self.settings.and_then(|settings| {
            cascades_after(view_projection, eye, sun, &settings, &self.cascades)
        });
        let mut uniform = ShadowUniform::zeroed_disabled();
        self.cascades.clear();
        self.sun = sun.normalize_or_zero();
        if let (Some(settings), Some((cascades, forward))) = (self.settings, fitted) {
            for (i, cascade) in cascades.iter().enumerate() {
                uniform.matrices[i] = cascade.view_projection.to_cols_array();
                uniform.splits[i] = cascade.far;
                uniform.texels[i] = cascade.texel;
                uniform.depth_scale[i] = cascade.depth_scale;
                let mut caster = [0.0f32; 20];
                caster[..16].copy_from_slice(&cascade.view_projection.to_cols_array());
                caster[16] = OCCLUDER_GAP * cascade.depth_scale;
                queue.write_buffer(
                    &self.caster,
                    i as u64 * CASTER_STRIDE,
                    bytemuck::bytes_of(&caster),
                );
                caster[..16].copy_from_slice(&cascade.map_view_projection.to_cols_array());
                caster[16] = 0.0;
                queue.write_buffer(
                    &self.caster,
                    Self::map_offset(i) as u64,
                    bytemuck::bytes_of(&caster),
                );
                uniform.map_scale[i] = cascade.map_depth[0];
                uniform.map_offset[i] = cascade.map_depth[1];
            }
            uniform.forward_count = forward.extend(cascades.len() as f32).to_array();
            uniform.origin = eye.extend(1.0).to_array();
            uniform.params = [
                settings.cascades as f32,
                settings.resolution as f32,
                0.0,
                0.0,
            ];
            self.cascades = cascades;
        }
        let previous: Vec<usize> = self.lamps.iter().flatten().map(|l| l.light).collect();
        let mut slots: Vec<Option<Lamp>> = Vec::new();
        self.stale.clear();
        if let Some(settings) = self.settings.filter(|s| s.lamps > 0) {
            let count = settings.lamps as usize;
            let planes = crate::scene::frustum_planes(view_projection);
            let in_view = |center: Vec3, radius: f32| {
                planes.iter().all(|p| {
                    let normal = p.truncate();
                    normal.dot(center) + p.w >= -radius * normal.length()
                })
            };
            let picked = pick_lamps(lamps, eye, count, &previous, in_view, |i| {
                seen.get(i).copied().unwrap_or(1.0)
            });
            // Lamps that stay keep their slots (and kept faces); new ones
            // take the free slots.
            let mut next: Vec<Option<usize>> = (0..count)
                .map(|slot| {
                    self.lamps
                        .get(slot)
                        .copied()
                        .flatten()
                        .map(|l| l.light)
                        .filter(|light| picked.contains(light))
                })
                .collect();
            for light in picked {
                if !next.contains(&Some(light))
                    && let Some(free) = next.iter_mut().find(|s| s.is_none())
                {
                    *free = Some(light);
                }
            }
            let dynamic = settings.lamp_dynamic_resolution();
            slots = next
                .iter()
                .map(|slot| {
                    slot.map(|light| {
                        let l = lamps[light];
                        Lamp {
                            light,
                            faces: lamp_faces(l.position, l.outer, dynamic),
                            center: l.position,
                            reach: l.outer,
                        }
                    })
                })
                .collect();
            self.drawn.resize(count * FACES, None);
            self.stale = vec![false; count * FACES];
            for (slot, lamp) in slots.iter().enumerate() {
                let Some(lamp) = lamp else {
                    self.drawn[slot * FACES..(slot + 1) * FACES].fill(None);
                    continue;
                };
                let l = lamps[lamp.light];
                for (face, matrix) in lamp.faces.iter().enumerate() {
                    let index = slot * FACES + face;
                    uniform.lamp_faces[index] = matrix.to_cols_array();
                    let mut caster = [0.0f32; 20];
                    caster[..16].copy_from_slice(&matrix.to_cols_array());
                    queue.write_buffer(
                        &self.caster,
                        Self::lamp_offset(slot, face) as u64,
                        bytemuck::bytes_of(&caster),
                    );
                    if self.drawn[index] != Some(*matrix) {
                        self.stale[index] = true;
                        self.drawn[index] = Some(*matrix);
                    }
                }
                uniform.light_slots[lamp.light / 4][lamp.light % 4] = slot as f32;
                uniform.lamp_centers[slot] = l.position.extend(l.outer).to_array();
            }
            // One kept face a frame is drawn again in turn, so a changed
            // build reaches its lamp shadows within a few dozen frames.
            let kept: Vec<usize> = (0..count * FACES)
                .filter(|&i| self.drawn[i].is_some())
                .collect();
            if !kept.is_empty() {
                self.refresh = (self.refresh + 1) % kept.len();
                self.stale[kept[self.refresh]] = true;
            }
            let half = 1.0 + 2.0 * LAMP_MARGIN_TEXELS / dynamic as f32;
            uniform.lamp_atlas = [
                settings.cascade_layers() as f32,
                settings.tiles_of(settings.lamp_resolution) as f32,
                settings.lamp_resolution as f32 / settings.resolution as f32,
                0.0,
            ];
            uniform.lamp_dynamic = [
                settings.lamp_dynamic_tile(0).0 as f32,
                settings.tiles_of(dynamic) as f32,
                dynamic as f32 / settings.resolution as f32,
                dynamic as f32,
            ];
            uniform.lamp_params = [
                count as f32,
                settings.lamp_resolution as f32,
                2.0 * half / dynamic as f32,
                settings.distance,
            ];
            uniform.origin = eye.extend(1.0).to_array();
        }
        self.lamps = slots;
        queue.write_buffer(&self.receiver, 0, bytemuck::bytes_of(&uniform));
    }
    pub fn caster_offset(cascade: usize) -> u32 {
        (cascade as u64 * CASTER_STRIDE) as u32
    }
    /// A lamp face's caster matrix, after the cascades'.
    pub fn lamp_offset(slot: usize, face: usize) -> u32 {
        ((MAX_CASCADES + slot * FACES + face) as u64 * CASTER_STRIDE) as u32
    }
    /// A cascade's map-layer matrix, after the lamps'.
    pub fn map_offset(cascade: usize) -> u32 {
        ((MAX_CASCADES + MAX_LAMPS * FACES + cascade) as u64 * CASTER_STRIDE) as u32
    }
    /// A map light's cube face matrix, after the map layers'.
    pub fn cube_offset(light: usize, face: usize) -> u32 {
        ((MAX_CASCADES * 2 + MAX_LAMPS * FACES + light * FACES + face) as u64 * CASTER_STRIDE)
            as u32
    }
    /// Writes a map light's cube face matrix for its draw.
    pub fn set_cube_matrix(&self, queue: &wgpu::Queue, light: usize, face: usize, matrix: Mat4) {
        let mut caster = [0.0f32; 20];
        caster[..16].copy_from_slice(&matrix.to_cols_array());
        queue.write_buffer(
            &self.caster,
            Self::cube_offset(light, face) as u64,
            bytemuck::bytes_of(&caster),
        );
    }
    /// The cascades' square resolution (1 while shadows are off).
    pub fn resolution(&self) -> u32 {
        self.settings.map_or(1, |s| s.resolution)
    }
    /// A cascade's kept brick layer matrix (`crate::kept_shadows`), after
    /// the map lights' cube faces.
    pub fn kept_offset(cascade: usize) -> u32 {
        ((MAX_CASCADES * 2 + MAX_LAMPS * FACES + crate::map_lighting::MAX_LIGHTS * FACES + cascade)
            as u64
            * CASTER_STRIDE) as u32
    }
    /// Writes a kept brick layer's matrix for its draws.
    pub fn set_kept_matrix(&self, queue: &wgpu::Queue, cascade: usize, matrix: Mat4) {
        let mut caster = [0.0f32; 20];
        caster[..16].copy_from_slice(&matrix.to_cols_array());
        queue.write_buffer(
            &self.caster,
            Self::kept_offset(cascade) as u64,
            bytemuck::bytes_of(&caster),
        );
    }
    /// Marks whether the map layers hold the map this frame (written after
    /// `update`, before the frame is submitted).
    pub fn set_map_drawn(&self, queue: &wgpu::Queue, drawn: bool) {
        let first = self.settings.map_or(0, |s| s.lamp_map_tile(0).0);
        let flag = [f32::from(u8::from(drawn)), first as f32, 0.0, 0.0];
        queue.write_buffer(
            &self.receiver,
            std::mem::offset_of!(ShadowUniform, map_params) as u64,
            bytemuck::bytes_of(&flag),
        );
    }
}
impl ShadowMaps {
    /// The lamp faces whose map tile must be drawn this frame from `map`
    /// (identified by `key`), marking them drawn; with no map, forgets them
    /// all.
    pub fn stale_map_faces(&self, key: &[usize]) -> Vec<usize> {
        let mut state = self.map_faces.borrow_mut();
        let (drawn_key, drawn) = &mut *state;
        if key.is_empty() || drawn_key.as_slice() != key {
            drawn.clear();
            *drawn_key = key.to_vec();
        }
        if key.is_empty() {
            return Vec::new();
        }
        drawn.resize(MAX_LAMPS * FACES, None);
        let mut stale = Vec::new();
        for (slot, lamp) in self.lamps.iter().enumerate() {
            let Some(lamp) = lamp else { continue };
            for (face, matrix) in lamp.faces.iter().enumerate() {
                let index = slot * FACES + face;
                if drawn[index] != Some(*matrix) {
                    drawn[index] = Some(*matrix);
                    stale.push(index);
                }
            }
        }
        stale
    }
    /// Whether kept brick face `index` must be drawn this frame: it is
    /// stale (a new lamp or view, or its turn to refresh), or the static
    /// casters inside it (identified by `casters`, from the chunks' own
    /// geometry) differ from those it was drawn with: a brick placed or
    /// removed there reaches the lamp's shadow the same frame. Marks the
    /// face drawn with them when it is.
    pub fn kept_face_due(&self, index: usize, casters: u64) -> bool {
        let mut drawn = self.kept_casters.borrow_mut();
        if drawn.len() <= index {
            drawn.resize(index + 1, None);
        }
        let due = self.stale.get(index).copied().unwrap_or(true) || drawn[index] != Some(casters);
        if due {
            drawn[index] = Some(casters);
        }
        due
    }
    /// Forgets the drawn map faces (a new map or new map lighting).
    pub fn forget_map_faces(&self) {
        self.map_faces.borrow_mut().1.clear();
        *self.cubes.borrow_mut() = CubeCache::default();
    }
    /// The map lights' cube faces to draw this frame from `map` (identified
    /// by `key`), at most `budget`, lights in order, marking them drawn; and
    /// whether a whole runtime cube cohort is available (including the last
    /// geometry version during a bounded refresh). `lights` are each light's position
    /// and reach, or `None` for an inactive light. Without cubes or a map, none.
    pub fn stale_cube_faces(
        &self,
        key: &[usize],
        lights: &[Option<(Vec3, f32)>],
        budget: usize,
    ) -> (Vec<(usize, usize, Mat4)>, bool) {
        let Some(settings) = self.settings.filter(|s| s.light_cubes) else {
            return (Vec::new(), false);
        };
        self.cubes
            .borrow_mut()
            .refresh(settings, key, lights, budget)
    }
}
impl ShadowUniform {
    fn zeroed_disabled() -> Self {
        Self {
            matrices: [Mat4::IDENTITY.to_cols_array(); MAX_CASCADES],
            splits: [0.0; 4],
            texels: [0.0; 4],
            forward_count: Vec4::ZERO.to_array(),
            params: [0.0, 1.0, 0.0, 0.0],
            depth_scale: [0.0; 4],
            origin: [0.0; 4],
            lamp_faces: [Mat4::IDENTITY.to_cols_array(); MAX_LAMPS * FACES],
            light_slots: [[-1.0; 4]; MAX_LIGHTS / 4],
            lamp_params: [0.0, 1.0, 0.0, 0.0],
            lamp_atlas: [0.0, 1.0, 1.0, 0.0],
            lamp_dynamic: [0.0, 1.0, 1.0, 1.0],
            lamp_centers: [[0.0; 4]; MAX_LAMPS],
            map_scale: [1.0; 4],
            map_offset: [0.0; 4],
            map_params: [0.0; 4],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera(eye: Vec3, target: Vec3) -> Mat4 {
        crate::scene::perspective(1.5, 16.0 / 9.0, 0.05, 4000.0)
            * glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y)
    }

    #[test]
    fn cascades_cover_the_view_and_every_slice_corner() {
        let eye = Vec3::new(10.0, 5.0, -20.0);
        let view = camera(eye, eye + Vec3::new(1.0, -0.2, -1.0));
        let sun = Vec3::new(0.3, -1.0, 0.4);
        let (cascades, forward) = cascades(view, eye, sun, &ShadowSettings::BEST).unwrap();
        assert_eq!(cascades.len(), 4);
        assert!((cascades[3].far - 320.0).abs() < 0.01);
        assert!(cascades.windows(2).all(|w| w[0].far < w[1].far));
        assert!(cascades.windows(2).all(|w| w[0].texel < w[1].texel));
        // A point on the view axis at each cascade's far split projects inside it.
        for cascade in &cascades {
            let point = eye + forward * (cascade.far * 0.99);
            let clip = cascade.view_projection.project_point3(point);
            assert!(clip.x.abs() < 1.0 && clip.y.abs() < 1.0, "{clip:?}");
            assert!((0.0..1.0).contains(&clip.z), "{clip:?}");
            // A caster 100 units toward the sun still lands in the map.
            let caster = cascade
                .view_projection
                .project_point3(point - sun.normalize() * 100.0);
            assert!((0.0..1.0).contains(&caster.z), "{caster:?}");
        }
    }

    #[test]
    fn cascades_snap_to_texels_as_the_eye_moves() {
        let sun = Vec3::new(0.3, -1.0, 0.4);
        let settings = ShadowSettings::LOW;
        let base = Vec3::new(3.0, 2.0, 1.0);
        let fit = |eye: Vec3| {
            cascades(camera(eye, eye + Vec3::NEG_Z), eye, sun, &settings)
                .unwrap()
                .0
        };
        let (a, b) = (fit(base), fit(base + Vec3::new(0.013, 0.0, 0.007)));
        for (a, b) in a.iter().zip(&b) {
            assert_eq!(a.texel, b.texel);
            // The same world point lands on the same sub-texel phase.
            let pa = a.view_projection.project_point3(Vec3::ZERO);
            let pb = b.view_projection.project_point3(Vec3::ZERO);
            let texels = (pa - pb).truncate() * settings.resolution as f32 / 2.0;
            assert!((texels - texels.round()).length() < 0.01, "{texels:?}");
        }
        assert!(cascades(Mat4::ZERO, base, sun, &settings).is_none());
        assert!(cascades(camera(base, Vec3::ZERO), base, Vec3::ZERO, &settings).is_none());
    }

    fn lamp(x: f32, brightness: f32, outer: f32) -> LampLight {
        LampLight {
            position: Vec3::new(x, 10.0, 0.0),
            color: Vec3::splat(brightness),
            outer,
        }
    }

    #[test]
    fn the_brightest_lamps_near_the_eye_cast_and_keep_casting() {
        let lights = [
            lamp(0.0, 0.5, 40.0),
            lamp(20.0, 1.0, 40.0),
            lamp(200.0, 1.0, 40.0),
            lamp(-10.0, 0.005, 40.0),
            lamp(5.0, 1.0, 40.0),
        ];
        let everywhere = |_: Vec3, _: f32| true;
        let all = |_: usize| 1.0;
        // Out of reach (200 away) and too dim never cast; the budget holds.
        let picked = pick_lamps(&lights, Vec3::ZERO, 4, &[], everywhere, all);
        assert_eq!(picked, vec![4, 1, 0]);
        assert_eq!(
            pick_lamps(&lights, Vec3::ZERO, 1, &[], everywhere, all),
            vec![4]
        );
        assert!(pick_lamps(&lights, Vec3::ZERO, 0, &[], everywhere, all).is_empty());
        // A lamp already casting keeps its slot against a slightly brighter one.
        let close = [lamp(0.0, 0.9, 40.0), lamp(0.0, 1.0, 40.0)];
        assert_eq!(
            pick_lamps(&close, Vec3::ZERO, 1, &[0], everywhere, all),
            vec![0]
        );
        assert_eq!(
            pick_lamps(&close, Vec3::ZERO, 1, &[], everywhere, all),
            vec![1]
        );
        // Lamps whose reach the view never sees do not cast.
        let ahead = |c: Vec3, r: f32| c.x + r > 55.0;
        assert_eq!(pick_lamps(&lights, Vec3::ZERO, 4, &[], ahead, all), vec![1]);
        // A bright lamp the map's walls hide from the eye's surroundings
        // (a bulb inside its shade) gives its slot to one that reaches it.
        let hidden = |i: usize| if i == 4 { 0.0 } else { 1.0 };
        assert_eq!(
            pick_lamps(&lights, Vec3::ZERO, 1, &[], everywhere, hidden),
            vec![1]
        );
    }

    #[test]
    fn validated_light_radii_have_finite_shadow_faces() {
        for outer in [0.001, LAMP_NEAR, 0.050001, 1.0, 50.0] {
            if crate::lighting_parameters::valid_radii(0.0, outer) {
                assert!(
                    lamp_faces(Vec3::new(3.0, 4.0, 5.0), outer, 512)
                        .iter()
                        .all(|face| face.to_cols_array().iter().all(|v| v.is_finite()))
                );
            } else {
                assert!(
                    outer <= LAMP_NEAR,
                    "tiny/equal inputs must be rejected before projection"
                );
            }
        }
    }

    #[test]
    fn geometry_cube_refresh_preserves_available_lighting_and_bounded_work() {
        let settings = ShadowSettings {
            light_cubes: true,
            ..ShadowSettings::BEST
        };
        let lights = vec![Some((Vec3::new(0.0, 12.0, 0.0), 40.0)); 24];
        let mut cache = CubeCache::default();
        for frame in 0..6 {
            let (drawn, available) = cache.refresh(settings, &[1], &lights, 24);
            assert_eq!(drawn.len(), 24);
            assert_eq!(
                available,
                frame == 5,
                "first-use cohort has no valid history"
            );
        }
        for _ in 0..6 {
            let (drawn, available) = cache.refresh(settings, &[2], &lights, 24);
            assert_eq!(drawn.len(), 24);
            assert!(available, "geometry invalidation must not zero every lamp");
        }
        assert!(cache.refresh(settings, &[2], &lights, 24).0.is_empty());
        let mut moved = lights.clone();
        moved[23] = Some((Vec3::new(1.0, 12.0, 0.0), 40.0));
        assert!(
            !cache.refresh(settings, &[2], &moved, 24).1,
            "new projector identities cannot reuse an old source's cohort"
        );
        assert!(!cache.refresh(settings, &[], &moved, 24).1);
    }

    #[test]
    fn finite_positions_can_overflow_shadow_depth_projection() {
        let position = Vec3::new(1e38, 0.0, 0.0);
        let outer = 0.050001;
        assert!(position.is_finite());
        assert!(crate::lighting_parameters::valid_radii(0.0, outer));
        for resolution in [256, 512] {
            assert!(
                lamp_faces(position, outer, resolution)
                    .iter()
                    .any(|face| face.to_cols_array().iter().any(|value| !value.is_finite()))
            );
        }
        assert!(!finite_lamp_faces(position, outer));
        // There is no invented position limit: the math itself decides.
        assert!(finite_lamp_faces(position, 50.0));
        assert!(finite_lamp_faces(Vec3::new(3.0, 4.0, 5.0), outer));
    }

    #[test]
    fn each_lamp_face_holds_its_axis_and_reaches_the_light_range() {
        let at = Vec3::new(3.0, 4.0, 5.0);
        let faces = lamp_faces(at, 50.0, 512);
        let axes = [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
        ];
        for (face, axis) in faces.iter().zip(axes) {
            // Straight along the axis lands in the face's centre.
            let centre = face.project_point3(at + axis * 20.0);
            assert!(centre.x.abs() < 1e-4 && centre.y.abs() < 1e-4, "{centre:?}");
            assert!((0.0..1.0).contains(&centre.z), "{centre:?}");
            // 45 degrees off (a cube edge) is still inside, with margin.
            let side = if axis.x != 0.0 { Vec3::Z } else { Vec3::X };
            let edge = face.project_point3(at + (axis + side) * 10.0);
            assert!(edge.x.abs().max(edge.y.abs()) < 1.0, "{edge:?}");
            assert!(edge.x.abs().max(edge.y.abs()) > 0.97, "{edge:?}");
            // Past the light's reach is past the far plane.
            assert!(face.project_point3(at + axis * 60.0).z > 1.0);
        }
        // Tiles pack into layers after the cascades' (casters, occluders
        // and the map: 12 at Best).
        let best = ShadowSettings::BEST;
        assert_eq!(best.lamp_layers(), 10);
        assert_eq!(best.lamp_tile(0), (12, [0, 0, 1024]));
        assert_eq!(best.lamp_tile(5), (13, [1024, 0, 1024]));
        assert_eq!(best.lamp_tile(16), (16, [0, 0, 1024]));
        // Moving casters' coarser faces follow in layers of their own.
        assert_eq!(best.lamp_dynamic_tile(0), (18, [0, 0, 512]));
        assert_eq!(best.lamp_dynamic_tile(9), (18, [512, 1024, 512]));
        // Then the map's faces, as coarse.
        assert_eq!(best.lamp_map_tile(0), (20, [0, 0, 512]));
        assert_eq!(best.lamp_map_tile(23), (21, [1536, 512, 512]));
        assert_eq!(ShadowSettings::MEDIUM.lamp_layers(), 4);
        assert_eq!(ShadowSettings::LOW.lamp_layers(), 0);
        // Dynamic: 24 lights' cubes after the lamps, 16 faces of 512 a layer.
        assert_eq!(best.cube_layers(), 0);
        let dynamic = ShadowSettings {
            light_cubes: true,
            ..best
        };
        assert_eq!((dynamic.cube_resolution(), dynamic.cube_layers()), (512, 9));
        assert_eq!(dynamic.cube_tile(0), (22, [0, 0, 512]));
        assert_eq!(dynamic.cube_tile(143), (30, [1536, 1536, 512]));
        let low = ShadowSettings {
            light_cubes: true,
            ..ShadowSettings::LOW
        };
        assert_eq!(
            (low.cube_resolution(), low.cube_layers(), low.cube_tile(0).0),
            (256, 9, 6)
        );
    }
}
