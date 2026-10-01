//! Item icons drawn from the item's own model, on each player's machine.
//!
//! An Add-On item may ask for its icon to be rendered rather than shipped:
//! `<icon>.render.json` beside the icon it names (see
//! `docs/modding/README.md`). The icon is drawn from the item's model as
//! the game has it, posed and framed exactly like a stock item's icon
//! (`pose_like`): the pose is found by fitting that item's model to the
//! silhouette of its icon, so the new icon sits in the tool slots at the
//! same angle and size as the stock ones. Nothing is shipped but the
//! request; the picture is made from the player's own game files.
//!
//! The look is the model's own colour, lit, with an optional veined skin
//! over it: the Gravity Gun's alien shell (`gravity-gun-fx/client/
//! alien.wgsl`), ported here so the icon matches the gun as drawn in play.
//! All CPU, deterministic, and small: icons are 128 pixels or less.
use anyhow::{Context, Result, ensure};
use bri_render::scene::{SceneData, SceneImage};
use glam::{Quat, Vec2, Vec3};
use serde::Deserialize;

/// `<icon>.render.json`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub schema_version: u32,
    /// The stock item whose icon's pose and framing this one takes.
    pub pose_like: String,
    #[serde(default)]
    pub look: Look,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Look {
    /// The model's own colour (its image's tint), seen where no skin is.
    #[serde(default = "white")]
    pub base: [f32; 3],
    /// Each surface also takes its material's own colour (its texture's
    /// average), so a model of several materials, such as a dark handle
    /// and a steel blade, keeps them apart.
    #[serde(default)]
    pub materials: bool,
    #[serde(default)]
    pub skin: Option<Skin>,
}

/// A dark shell with an oil-slick sheen and glowing veins, puffed out a
/// little along the model's normals like the in-game skin.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Skin {
    pub shell: [f32; 3],
    pub veins: [f32; 3],
    #[serde(default = "puff")]
    pub puff: f32,
}

fn white() -> [f32; 3] {
    [1.0; 3]
}
fn puff() -> f32 {
    0.012
}

impl Spec {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let spec: Self = serde_json::from_slice(bytes)?;
        ensure!(spec.schema_version == 1, "unknown icon render schema {}", spec.schema_version);
        ensure!(!spec.pose_like.is_empty(), "pose_like names no item");
        let colours = spec
            .look
            .base
            .iter()
            .chain(spec.look.skin.iter().flat_map(|s| s.shell.iter().chain(&s.veins)));
        ensure!(
            colours.into_iter().all(|c| c.is_finite() && (0.0..=1.0).contains(c)),
            "icon render colours must be 0 to 1"
        );
        ensure!(
            spec.look.skin.as_ref().is_none_or(|s| s.puff.is_finite() && (0.0..=0.1).contains(&s.puff)),
            "skin puff must be 0 to 0.1"
        );
        Ok(spec)
    }
}

/// A model's triangles in its own space: positions and per-vertex normals.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub indices: Vec<u32>,
    /// Each vertex's material colour (`Look::materials`); empty is white.
    pub colours: Vec<Vec3>,
}

impl Mesh {
    pub fn from_scene(scene: &SceneData) -> Self {
        // A material's colour: its diffuse texture's average, times the
        // vertex colour, as the model is drawn.
        let average = |image: &SceneImage| -> Vec3 {
            let (mut sum, mut weight) = (Vec3::ZERO, 0.0);
            for px in image.rgba.chunks_exact(4) {
                let a = f32::from(px[3]) / 255.0;
                sum += Vec3::new(f32::from(px[0]), f32::from(px[1]), f32::from(px[2])) / 255.0 * a;
                weight += a;
            }
            if weight > 0.0 { sum / weight } else { Vec3::ONE }
        };
        let materials: Vec<Vec3> = scene
            .materials
            .iter()
            .map(|m| scene.images.get(m.images[0]).map_or(Vec3::ONE, average))
            .collect();
        let mut colours: Vec<Vec3> = scene
            .vertices
            .iter()
            .map(|v| Vec3::new(v.color[0], v.color[1], v.color[2]))
            .collect();
        let mut tinted = vec![false; colours.len()];
        for batch in &scene.batches {
            let colour = materials.get(batch.material).copied().unwrap_or(Vec3::ONE);
            let range = batch.indices.start as usize..batch.indices.end as usize;
            for &i in scene.indices.get(range).unwrap_or(&[]) {
                if let Some(c) = colours.get_mut(i as usize)
                    && !std::mem::replace(&mut tinted[i as usize], true)
                {
                    *c *= colour;
                }
            }
        }
        Self {
            positions: scene.vertices.iter().map(|v| Vec3::from(v.position)).collect(),
            normals: scene
                .vertices
                .iter()
                .map(|v| Vec3::from(v.normal).normalize_or_zero())
                .collect(),
            indices: scene.indices.clone(),
            colours,
        }
    }
    fn triangles(&self) -> impl Iterator<Item = [usize; 3]> + '_ {
        self.indices.chunks_exact(3).filter_map(|t| {
            let t = [t[0] as usize, t[1] as usize, t[2] as usize];
            t.iter().all(|&i| i < self.positions.len()).then_some(t)
        })
    }
}

/// How an icon shows its model: turned by `rotation`, then drawn looking
/// down -Z with `scale` pixels a unit, the model's origin at `centre`
/// (pixels from the top left, y down), on a `size` picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub rotation: Quat,
    pub scale: f32,
    pub centre: Vec2,
    pub size: [u32; 2],
}

impl Pose {
    fn project(&self, p: Vec3) -> Vec3 {
        let q = self.rotation * p;
        Vec3::new(self.centre.x + q.x * self.scale, self.centre.y - q.y * self.scale, q.z)
    }
}

fn euler(yaw: f32, pitch: f32, roll: f32) -> Quat {
    Quat::from_rotation_z(roll) * Quat::from_rotation_x(pitch) * Quat::from_rotation_y(yaw)
}

/// Which pixels of a `w` x `h` grid the model covers under `pose`
/// (scaled to that grid).
fn silhouette(mesh: &Mesh, pose: &Pose, w: usize, h: usize) -> Vec<bool> {
    let mut mask = vec![false; w * h];
    let sx = w as f32 / pose.size[0] as f32;
    let sy = h as f32 / pose.size[1] as f32;
    let projected: Vec<Vec2> = mesh
        .positions
        .iter()
        .map(|p| {
            let s = pose.project(*p);
            Vec2::new(s.x * sx, s.y * sy)
        })
        .collect();
    for [a, b, c] in mesh.triangles() {
        let (a, b, c) = (projected[a], projected[b], projected[c]);
        raster(a, b, c, w, h, |x, y, _| mask[y * w + x] = true);
    }
    mask
}

/// Every pixel centre inside the triangle, with its barycentric weights.
fn raster(a: Vec2, b: Vec2, c: Vec2, w: usize, h: usize, mut f: impl FnMut(usize, usize, Vec3)) {
    let area = (b - a).perp_dot(c - a);
    if area.abs() < 1e-9 || !area.is_finite() {
        return;
    }
    let lo = a.min(b).min(c).max(Vec2::ZERO);
    let hi = a.max(b).max(c).min(Vec2::new(w as f32, h as f32));
    if lo.x >= hi.x || lo.y >= hi.y {
        return;
    }
    for y in (lo.y.floor() as usize)..(hi.y.ceil() as usize).min(h) {
        for x in (lo.x.floor() as usize)..(hi.x.ceil() as usize).min(w) {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let wa = (c - b).perp_dot(p - b) / area;
            let wb = (a - c).perp_dot(p - c) / area;
            let wc = 1.0 - wa - wb;
            if wa >= 0.0 && wb >= 0.0 && wc >= 0.0 {
                f(x, y, Vec3::new(wa, wb, wc));
            }
        }
    }
}

/// The icon's covered pixels (alpha at least half), on a `w` x `h` grid.
fn icon_mask(icon: &SceneImage, w: usize, h: usize) -> Vec<bool> {
    let (iw, ih) = (icon.width as usize, icon.height as usize);
    let mut mask = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let ix = ((x as f32 + 0.5) * iw as f32 / w as f32) as usize;
            let iy = ((y as f32 + 0.5) * ih as f32 / h as f32) as usize;
            mask[y * w + x] = icon.rgba[(iy.min(ih - 1) * iw + ix.min(iw - 1)) * 4 + 3] >= 128;
        }
    }
    mask
}

fn bounds(mask: &[bool], w: usize) -> Option<(Vec2, Vec2)> {
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    for (i, _) in mask.iter().enumerate().filter(|(_, m)| **m) {
        let p = Vec2::new((i % w) as f32, (i / w) as f32);
        lo = lo.min(p);
        hi = hi.max(p + 1.0);
    }
    (lo.x <= hi.x).then_some((lo, hi))
}

fn overlap(a: &[bool], b: &[bool]) -> f32 {
    let both = a.iter().zip(b).filter(|(a, b)| **a && **b).count();
    let either = a.iter().zip(b).filter(|(a, b)| **a || **b).count();
    if either == 0 { 0.0 } else { both as f32 / either as f32 }
}

/// Scale and place the model turned by `rotation` so its outline's box
/// fills the icon's.
fn framed(mesh: &Mesh, rotation: Quat, target: (Vec2, Vec2), grid: usize, size: [u32; 2]) -> Option<Pose> {
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    for p in &mesh.positions {
        let q = rotation * *p;
        let s = Vec2::new(q.x, -q.y);
        lo = lo.min(s);
        hi = hi.max(s);
    }
    let extent = hi - lo;
    if !(extent.x > 1e-6 && extent.y > 1e-6) {
        return None;
    }
    // Grid pixels to icon pixels.
    let to_icon = Vec2::new(size[0] as f32, size[1] as f32) / grid as f32;
    let (tlo, thi) = (target.0 * to_icon, target.1 * to_icon);
    let want = thi - tlo;
    let scale = ((want.x / extent.x) * (want.y / extent.y)).sqrt();
    let centre = (tlo + thi) * 0.5 - (lo + hi) * 0.5 * scale;
    Some(Pose { rotation, scale, centre, size })
}

/// The pose that draws `mesh` over `icon`'s silhouette: searched over
/// every turn in coarse steps, then refined. `None` when nothing matches
/// well (a model and icon that are not the same thing).
pub fn fit_pose(mesh: &Mesh, icon: &SceneImage) -> Option<(Pose, f32)> {
    const GRID: usize = 48;
    if icon.width == 0 || icon.height == 0 || icon.rgba.len() < (icon.width * icon.height * 4) as usize {
        return None;
    }
    let size = [icon.width, icon.height];
    let target_mask = icon_mask(icon, GRID, GRID);
    let target = bounds(&target_mask, GRID)?;
    let score = |yaw: f32, pitch: f32, roll: f32| -> Option<(Pose, f32)> {
        let pose = framed(mesh, euler(yaw, pitch, roll), target, GRID, size)?;
        Some((pose, overlap(&silhouette(mesh, &pose, GRID, GRID), &target_mask)))
    };
    // The coarse search at a quarter of the pixels.
    const COARSE: usize = GRID / 2;
    let coarse_mask = icon_mask(icon, COARSE, COARSE);
    let coarse_target = bounds(&coarse_mask, COARSE)?;
    let rough = |yaw: f32, pitch: f32, roll: f32| -> Option<(Pose, f32)> {
        let pose = framed(mesh, euler(yaw, pitch, roll), coarse_target, COARSE, size)?;
        Some((pose, overlap(&silhouette(mesh, &pose, COARSE, COARSE), &coarse_mask)))
    };
    let step = 15f32.to_radians();
    // The best few coarse turns, each refined: an outline can look alike
    // from two far-apart turns, and only refining tells them apart.
    let mut coarse: Vec<([f32; 3], Pose, f32)> = Vec::new();
    for yi in 0..24 {
        for pi in 0..13 {
            for ri in 0..24 {
                let angles = [yi as f32 * step, (pi as f32 - 6.0) * step, ri as f32 * step];
                if let Some((pose, s)) = rough(angles[0], angles[1], angles[2]) {
                    coarse.push((angles, pose, s));
                }
            }
        }
    }
    coarse.sort_by(|a, b| b.2.total_cmp(&a.2));
    let mut best: Option<(Pose, f32)> = None;
    for (mut angles, _, _) in coarse.into_iter().take(8) {
        let Some((mut pose, mut s)) = score(angles[0], angles[1], angles[2]) else {
            continue;
        };
        let mut delta = 8f32.to_radians();
        while delta > 0.25f32.to_radians() {
            let mut moved = true;
            while moved {
                moved = false;
                for axis in 0..3 {
                    for sign in [-1.0, 1.0] {
                        let mut a = angles;
                        a[axis] += sign * delta;
                        if let Some((p, t)) = score(a[0], a[1], a[2])
                            && t > s
                        {
                            (angles, pose, s, moved) = (a, p, t, true);
                        }
                    }
                }
            }
            delta *= 0.5;
        }
        if best.as_ref().is_none_or(|b| s > b.1) {
            best = Some((pose, s));
        }
    }
    let (mut pose, mut s) = best?;
    // Then the framing itself: a clipped or padded outline's box is not
    // quite the model's.
    let fits = |p: &Pose| overlap(&silhouette(mesh, p, GRID, GRID), &target_mask);
    let mut delta = 1.0f32;
    while delta > 0.05 {
        let mut moved = true;
        while moved {
            moved = false;
            let unit = size[0] as f32 / GRID as f32 * delta;
            for change in [
                Vec3::new(unit, 0.0, 0.0),
                Vec3::new(-unit, 0.0, 0.0),
                Vec3::new(0.0, unit, 0.0),
                Vec3::new(0.0, -unit, 0.0),
                Vec3::new(0.0, 0.0, 0.04 * delta),
                Vec3::new(0.0, 0.0, -0.04 * delta),
            ] {
                let p = Pose {
                    centre: pose.centre + change.truncate(),
                    scale: pose.scale * (1.0 + change.z),
                    ..pose
                };
                let t = fits(&p);
                if t > s {
                    (pose, s, moved) = (p, t, true);
                }
            }
        }
        delta *= 0.5;
    }
    (s >= 0.6).then_some((pose, s))
}

/// Light for icons, in view space: from above, the left and the front.
const LIGHT: Vec3 = Vec3::new(-0.45, 0.65, 0.62);
const SAMPLES: usize = 3;
/// Icons are shot under a stronger light than play: the stock icons show
/// their faces clearly apart. The skin's near-black shell is brightened by
/// this much so its faces read the same way at icon size.
const EXPOSURE: f32 = 3.0;
/// How far behind the nearest surface an edge still shows, model units:
/// past the skin's puff, short of the far side.
const EDGE_DEPTH: f32 = 0.04;
/// Faces meeting at more than this, degrees, make a hard edge.
const CREASE_DEGREES: f32 = 35.0;

/// The model's hard edges: where faces meet at a sharp angle, or where
/// the surface ends. Positions are welded first, since a flat-shaded
/// model repeats each corner once per face.
fn creases(mesh: &Mesh) -> Vec<(Vec3, Vec3)> {
    use std::collections::BTreeMap;
    let key = |p: Vec3| (p * 1.0e4).round().to_array().map(|v| v as i64);
    type Edge = ([i64; 3], [i64; 3]);
    let mut faces: BTreeMap<Edge, (Vec3, Vec3, Vec<Vec3>)> = BTreeMap::new();
    for t in mesh.triangles() {
        let [a, b, c] = t.map(|i| mesh.positions[i]);
        let normal = (b - a).cross(c - a).normalize_or_zero();
        if normal == Vec3::ZERO {
            continue;
        }
        for (p, q) in [(a, b), (b, c), (c, a)] {
            let (kp, kq) = (key(p), key(q));
            let k = if kp <= kq { (kp, kq) } else { (kq, kp) };
            faces.entry(k).or_insert((p, q, Vec::new())).2.push(normal);
        }
    }
    let cos = CREASE_DEGREES.to_radians().cos();
    faces
        .into_values()
        .filter(|(_, _, normals)| normals.len() == 1 || normals.iter().any(|n| normals.iter().any(|m| n.dot(*m) < cos)))
        .map(|(p, q, _)| (p, q))
        .collect()
}

/// A pose turned by `rotation` whose drawing (with the skin's `puff`)
/// fills `target` (a box in icon pixels) as far as it can without
/// leaving it, centred in it.
pub fn frame(mesh: &Mesh, rotation: Quat, puff: f32, target: (Vec2, Vec2), size: [u32; 2]) -> Option<Pose> {
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    for (p, n) in mesh.positions.iter().zip(&mesh.normals) {
        let q = rotation * (*p + *n * puff);
        let s = Vec2::new(q.x, -q.y);
        lo = lo.min(s);
        hi = hi.max(s);
    }
    let extent = hi - lo;
    let room = target.1 - target.0;
    if !(extent.x > 1e-6 && extent.y > 1e-6 && room.x > 0.0 && room.y > 0.0) {
        return None;
    }
    let scale = (room.x / extent.x).min(room.y / extent.y);
    let centre = (target.0 + target.1) * 0.5 - (lo + hi) * 0.5 * scale;
    Some(Pose { rotation, scale, centre, size })
}

/// The box a stock icon's drawing fills, in its pixels, kept at least
/// `MARGIN` of the icon clear of every edge (stock icons drawn right to the
/// edge would leave a new one no clear border).
fn filled_box(icon: &SceneImage) -> Option<(Vec2, Vec2)> {
    let (w, h) = (icon.width as usize, icon.height as usize);
    let mask: Vec<bool> = icon.rgba.chunks_exact(4).map(|p| p[3] >= 128).collect();
    let (lo, hi) = bounds(&mask, w)?;
    let margin = Vec2::new(w as f32, h as f32) * MARGIN;
    let lo = lo.max(margin);
    let hi = hi.min(Vec2::new(w as f32, h as f32) - margin);
    (hi.x > lo.x && hi.y > lo.y).then_some((lo, hi))
}
/// The least clear border an icon keeps on each side, of its size.
const MARGIN: f32 = 0.06;

/// How many fully clear rows or columns an icon has at its top, right,
/// bottom and left.
pub fn clear_border(image: &SceneImage) -> [usize; 4] {
    let (w, h) = (image.width as usize, image.height as usize);
    let clear = |x: usize, y: usize| image.rgba[(y * w + x) * 4 + 3] == 0;
    let row = |y: usize| (0..w).all(|x| clear(x, y));
    let column = |x: usize| (0..h).all(|y| clear(x, y));
    [
        (0..h).take_while(|y| row(*y)).count(),
        (0..w).rev().take_while(|x| column(*x)).count(),
        (0..h).rev().take_while(|y| row(*y)).count(),
        (0..w).take_while(|x| column(*x)).count(),
    ]
}

/// The model drawn under `pose` with `look`, on a clear background.
pub fn render(mesh: &Mesh, pose: &Pose, look: &Look, label: &str) -> SceneImage {
    let [w, h] = pose.size.map(|v| v as usize);
    let (sw, sh) = (w * SAMPLES, h * SAMPLES);
    let mut fine = *pose;
    fine.scale *= SAMPLES as f32;
    fine.centre *= SAMPLES as f32;
    let mut depth = vec![f32::MIN; sw * sh];
    let mut colour = vec![None::<Vec3>; sw * sh];
    let light = LIGHT.normalize();
    let base = Vec3::from(look.base);
    let coloured = look.materials && mesh.colours.len() == mesh.positions.len();
    if coloured {
        let shade = |c: Vec3, n: Vec3| base * c * (0.45 + 0.6 * n.dot(light).max(0.0));
        draw_coloured(mesh, &fine, sw, sh, &mut depth, &mut colour, &shade, pose.rotation);
    }
    let mut draw = |positions: &[Vec3], shade: &dyn Fn(Vec3, Vec3) -> Vec3| {
        let projected: Vec<Vec3> = positions.iter().map(|p| fine.project(*p)).collect();
        for t in mesh.triangles() {
            let [a, b, c] = t.map(|i| projected[i]);
            raster(a.truncate(), b.truncate(), c.truncate(), sw, sh, |x, y, bary| {
                let z = a.z * bary.x + b.z * bary.y + c.z * bary.z;
                let i = y * sw + x;
                if z > depth[i] {
                    depth[i] = z;
                    let local = positions[t[0]] * bary.x + positions[t[1]] * bary.y + positions[t[2]] * bary.z;
                    let normal = (mesh.normals[t[0]] * bary.x + mesh.normals[t[1]] * bary.y + mesh.normals[t[2]] * bary.z)
                        .normalize_or_zero();
                    colour[i] = Some(shade(local, pose.rotation * normal));
                }
            });
        }
    };
    if !coloured {
        draw(&mesh.positions, &|_, n| base * (0.45 + 0.6 * n.dot(light).max(0.0)));
    }
    if let Some(skin) = &look.skin {
        // Puffed along its normals, as the in-game skin is drawn over the
        // model: at hard edges the model's own colour shows through.
        let puffed: Vec<Vec3> = mesh
            .positions
            .iter()
            .zip(&mesh.normals)
            .map(|(p, n)| *p + *n * skin.puff)
            .collect();
        let shell = Vec3::from(skin.shell);
        let veins = Vec3::from(skin.veins);
        let pixel = 1.0 / pose.scale.max(1e-3);
        draw(&puffed, &|local, n| veined(local, n, light, shell, veins, pixel));
    }
    if look.skin.is_some() {
        // The in-game skin is puffed along split normals, so the model's
        // own colour shows along every hard edge: at icon size those
        // cracks are thinner than a pixel, so they are drawn a pixel wide.
        let edge = base * (0.55 + 0.55 * Vec3::new(0.0, 0.3, 0.95).normalize().dot(light).max(0.0));
        let radius = 0.55 * SAMPLES as f32;
        for (a, b) in creases(mesh) {
            let (a, b) = (fine.project(a), fine.project(b));
            let steps = (a.truncate().distance(b.truncate()) / 0.5).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let p = a.lerp(b, k as f32 / steps as f32);
                let (x0, x1) = ((p.x - radius).floor().max(0.0) as usize, ((p.x + radius).ceil() as usize).min(sw));
                let (y0, y1) = ((p.y - radius).floor().max(0.0) as usize, ((p.y + radius).ceil() as usize).min(sh));
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = y * sw + x;
                        let centre = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                        // Only where the edge is the surface seen there.
                        if centre.distance(p.truncate()) <= radius && colour[i].is_some() && p.z >= depth[i] - EDGE_DEPTH {
                            colour[i] = Some(edge);
                        }
                    }
                }
            }
        }
    }
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let (mut sum, mut covered) = (Vec3::ZERO, 0usize);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    if let Some(c) = colour[(y * SAMPLES + sy) * sw + x * SAMPLES + sx] {
                        sum += c;
                        covered += 1;
                    }
                }
            }
            let c = if covered > 0 { sum / covered as f32 } else { Vec3::ZERO };
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            rgba.extend([byte(c.x), byte(c.y), byte(c.z)]);
            rgba.push(((covered * 255) as f32 / (SAMPLES * SAMPLES) as f32).round() as u8);
        }
    }
    SceneImage {
        label: label.into(),
        width: pose.size[0],
        height: pose.size[1],
        rgba,
        srgb: false,
    }
}

/// Draw `mesh` with each vertex's material colour interpolated across its
/// triangles into the depth and colour buffers `render` keeps.
#[allow(clippy::too_many_arguments)]
fn draw_coloured(
    mesh: &Mesh,
    fine: &Pose,
    sw: usize,
    sh: usize,
    depth: &mut [f32],
    colour: &mut [Option<Vec3>],
    shade: &dyn Fn(Vec3, Vec3) -> Vec3,
    rotation: Quat,
) {
    let projected: Vec<Vec3> = mesh.positions.iter().map(|p| fine.project(*p)).collect();
    for t in mesh.triangles() {
        let [a, b, c] = t.map(|i| projected[i]);
        raster(a.truncate(), b.truncate(), c.truncate(), sw, sh, |x, y, bary| {
            let z = a.z * bary.x + b.z * bary.y + c.z * bary.z;
            let i = y * sw + x;
            if z > depth[i] {
                depth[i] = z;
                let normal = (mesh.normals[t[0]] * bary.x + mesh.normals[t[1]] * bary.y + mesh.normals[t[2]] * bary.z)
                    .normalize_or_zero();
                let tint = mesh.colours[t[0]] * bary.x + mesh.colours[t[1]] * bary.y + mesh.colours[t[2]] * bary.z;
                colour[i] = Some(shade(tint, rotation * normal));
            }
        });
    }
}

/// `alien.wgsl`'s `fs_main` at rest, seen from the front (+Z in view
/// space) under the icon light.
///
/// `pixel` is one icon pixel in model units: the veins are drawn at least
/// about a pixel wide, as they are at any distance in play, or the icon's
/// few pixels would miss them.
fn veined(local: Vec3, n: Vec3, light: Vec3, shell: Vec3, vein: Vec3, pixel: f32) -> Vec3 {
    let view = Vec3::Z;
    let lit = shell * (Vec3::splat(0.55) * 1.2 + Vec3::ONE * n.dot(light).max(0.0)) * EXPOSURE;
    let edge = 1.0 - n.dot(view).abs();
    let hue = edge * 1.3 + local.dot(Vec3::new(0.6, 0.9, 0.4));
    let film = (Vec3::splat(hue) + Vec3::new(0.0, 0.33, 0.67)).map(|v| 0.5 + 0.5 * (std::f32::consts::TAU * v).cos());
    let cold = Vec3::new(0.25, 0.1, 0.55).lerp(Vec3::new(0.1, 0.75, 0.8), film.y) * (0.6 + 0.4 * film.z);
    let sheen = cold * edge.powi(3) * 0.4;
    let reflected = (-light) - 2.0 * (-light).dot(n) * n;
    let spec = reflected.dot(view).max(0.0).powf(48.0) * 0.8;
    let q = local * 3.2;
    let w = (q.x * 7.0 + 2.0 * (q.y * 5.0 + q.z * 3.0).sin()).sin()
        + (q.y * 6.0 + 2.0 * (q.z * 4.0 + q.x * 5.0).sin()).sin()
        + 0.7 * (q.z * 8.0 + 1.5 * (q.x * 4.0 + q.y * 2.0).sin()).sin();
    // The wave changes about 22 a unit across the model.
    let t = (w.abs() / (22.0 * 0.6 * pixel).max(0.16)).clamp(0.0, 1.0);
    let lines = 1.0 - t * t * (3.0 - 2.0 * t);
    let flow = 0.5 + 0.5 * (local.y * 9.0).sin();
    let glow = vein * lines * 0.35 * (0.45 + 0.55 * flow);
    lit + sheen + Vec3::splat(spec) + glow
}

/// Render the icon `spec` asks for: `mesh` posed like `reference` (the
/// stock item's model and its icon).
pub fn render_like(spec: &Spec, mesh: &Mesh, reference: (&Mesh, &SceneImage), label: &str) -> Result<SceneImage> {
    ensure!(!mesh.indices.is_empty(), "the item has no model to draw");
    let (fitted, _) = fit_pose(reference.0, reference.1)
        .with_context(|| format!("{}'s model does not match its icon", spec.pose_like))?;
    // The stock icon's angle; the framing is this model's own, filling the
    // box the stock drawing fills.
    let target = filled_box(reference.1).context("the stock icon is empty")?;
    let puff = spec.look.skin.as_ref().map_or(0.0, |s| s.puff);
    let pose = frame(mesh, fitted.rotation, puff, target, fitted.size).context("the model has no size")?;
    Ok(render(mesh, &pose, &spec.look, label))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gun of boxes, lopsided on every axis so only one pose fits.
    fn gun() -> Mesh {
        let mut mesh = Mesh::default();
        let mut add_box = |c: Vec3, h: Vec3| {
            let corners = [
                (Vec3::X, Vec3::Y, Vec3::Z),
                (Vec3::NEG_X, Vec3::Y, Vec3::NEG_Z),
                (Vec3::Y, Vec3::Z, Vec3::X),
                (Vec3::NEG_Y, Vec3::Z, Vec3::NEG_X),
                (Vec3::Z, Vec3::X, Vec3::Y),
                (Vec3::NEG_Z, Vec3::X, Vec3::NEG_Y),
            ];
            for (n, u, v) in corners {
                let base = mesh.positions.len() as u32;
                for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    mesh.positions.push(c + (n + u * a + v * b) * h);
                    mesh.normals.push(n);
                }
                mesh.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        };
        add_box(Vec3::new(0.0, 0.2, 0.0), Vec3::new(0.9, 0.25, 0.2));
        add_box(Vec3::new(0.95, 0.3, 0.05), Vec3::new(0.2, 0.4, 0.3));
        add_box(Vec3::new(-0.45, -0.45, 0.0), Vec3::new(0.15, 0.45, 0.15));
        mesh
    }

    fn picture(mesh: &Mesh, pose: &Pose) -> SceneImage {
        render(mesh, pose, &Look { base: [1.0; 3], materials: false, skin: None }, "reference")
    }

    /// The pose of a stock icon is recovered from its picture alone, so a
    /// new icon drawn with it lines up with the stock one.
    #[test]
    fn a_models_pose_is_recovered_from_its_icon() {
        let mesh = gun();
        let truth = Pose {
            rotation: euler(0.7, 0.35, 0.5),
            scale: 24.0,
            centre: Vec2::new(34.0, 30.0),
            size: [64, 64],
        };
        let icon = picture(&mesh, &truth);
        let (pose, score) = fit_pose(&mesh, &icon).expect("fits");
        assert!(score > 0.9, "outline overlap {score}");
        let redrawn = picture(&mesh, &pose);
        let covered = |img: &SceneImage| img.rgba.chunks_exact(4).map(|p| p[3] >= 128).collect::<Vec<_>>();
        let agree = overlap(&covered(&redrawn), &covered(&icon));
        assert!(agree > 0.9, "redrawn icon overlaps the stock one {agree}");
        assert_eq!([redrawn.width, redrawn.height], [64, 64]);
    }

    /// Something else entirely does not pass for the stock item.
    #[test]
    fn a_model_that_is_not_the_icon_does_not_fit() {
        let mesh = gun();
        let mut icon = SceneImage { label: "ring".into(), width: 64, height: 64, rgba: vec![0; 64 * 64 * 4], srgb: false };
        for y in 0..64 {
            for x in 0..64 {
                let r = ((x as f32 - 31.5).powi(2) + (y as f32 - 31.5).powi(2)).sqrt();
                if (20.0..28.0).contains(&r) {
                    icon.rgba[(y * 64 + x) * 4 + 3] = 255;
                }
            }
        }
        assert!(fit_pose(&mesh, &icon).is_none());
    }

    /// The skin is dark with light veins, the background stays clear, and
    /// the same request draws the same picture every time.
    #[test]
    fn the_veined_skin_draws_a_dark_shell_on_a_clear_background() {
        let mesh = gun();
        let pose = Pose { rotation: euler(0.7, 0.35, 0.5), scale: 30.0, centre: Vec2::new(34.0, 30.0), size: [64, 64] };
        let look = Look {
            base: [0.35, 1.0, 0.8],
            materials: false,
            skin: Some(Skin { shell: [0.035, 0.025, 0.05], veins: [0.3, 0.95, 1.0], puff: 0.012 }),
        };
        let image = render(&mesh, &pose, &look, "gun");
        assert_eq!(image.rgba[3], 0, "the corner is clear");
        let solid: Vec<_> = image.rgba.chunks_exact(4).filter(|p| p[3] == 255).collect();
        assert!(solid.len() > 300, "{} solid pixels", solid.len());
        let dark = solid.iter().filter(|p| p[0] < 60 && p[1] < 60 && p[2] < 70).count();
        assert!(dark * 2 > solid.len(), "mostly the dark shell: {dark} of {}", solid.len());
        assert!(solid.iter().any(|p| p[1] > 60 && p[2] > 60 && p[0] < p[1]), "teal veins show");
        assert_eq!(render(&mesh, &pose, &look, "gun").rgba, image.rgba);
    }

    /// With `materials`, each part keeps its own material's colour: a dark
    /// handle stays dark beside a light blade.
    #[test]
    fn material_colours_keep_the_parts_apart() {
        let mut mesh = gun();
        // The first box is dark, the rest light.
        mesh.colours = (0..mesh.positions.len())
            .map(|i| if i < 24 { Vec3::splat(0.1) } else { Vec3::splat(0.9) })
            .collect();
        let pose = Pose { rotation: euler(0.7, 0.35, 0.5), scale: 24.0, centre: Vec2::new(34.0, 30.0), size: [64, 64] };
        let look = Look { base: [1.0; 3], materials: true, skin: None };
        let image = render(&mesh, &pose, &look, "gun");
        let solid: Vec<_> = image.rgba.chunks_exact(4).filter(|p| p[3] == 255).collect();
        let dark = solid.iter().filter(|p| p[0] < 80).count();
        let light = solid.iter().filter(|p| p[0] > 120).count();
        assert!(dark > 50 && light > 50, "{dark} dark and {light} light pixels");
        // Without it the model is one colour.
        let plain = render(&mesh, &pose, &Look { materials: false, ..look }, "gun");
        assert!(plain.rgba.chunks_exact(4).filter(|p| p[3] == 255).all(|p| p[0] > 100));
    }

    /// A stock icon drawn right to its edges still gets a new icon with a
    /// clear border on every side, and the drawing fills the box inside it.
    #[test]
    fn the_icon_keeps_a_clear_margin_on_every_side() {
        let mesh = gun();
        let mut stock = SceneImage { label: "stock".into(), width: 96, height: 96, rgba: vec![0; 96 * 96 * 4], srgb: false };
        for y in 0..96 {
            for x in 0..96 {
                // A band corner to corner, touching all four edges.
                if (x as i32 - y as i32).abs() < 40 {
                    stock.rgba[(y * 96 + x) * 4 + 3] = 255;
                }
            }
        }
        let target = filled_box(&stock).expect("a box");
        let look = Look {
            base: [0.35, 1.0, 0.8],
            materials: false,
            skin: Some(Skin { shell: [0.035, 0.025, 0.05], veins: [0.3, 0.95, 1.0], puff: 0.012 }),
        };
        for rotation in [euler(0.7, 0.35, 0.5), euler(-1.2, 0.9, 2.4), Quat::IDENTITY] {
            let pose = frame(&mesh, rotation, 0.012, target, [96, 96]).expect("frames");
            let image = render(&mesh, &pose, &look, "gun");
            let border = clear_border(&image);
            assert!(border.iter().all(|b| *b >= 5), "clear rows and columns (top, right, bottom, left): {border:?}");
            // It fills the box one way or the other: no shrunken drawing.
            let [top, right, bottom, left] = border.map(|b| b as i32);
            let (w, h) = (96 - left - right, 96 - top - bottom);
            assert!(w >= 80 || h >= 80, "{w}x{h} drawn, border {border:?}");
        }
    }

    #[test]
    fn the_request_is_checked() {
        let ok = br#"{"schema_version": 1, "pose_like": "v20.weapon.printgun",
            "look": {"base": [0.35, 1, 0.8], "skin": {"shell": [0.035, 0.025, 0.05], "veins": [0.3, 0.95, 1]}}}"#;
        let spec = Spec::parse(ok).unwrap();
        assert_eq!(spec.look.skin.unwrap().puff, 0.012);
        for bad in [
            &br#"{"schema_version": 2, "pose_like": "x"}"#[..],
            br#"{"schema_version": 1, "pose_like": ""}"#,
            br#"{"schema_version": 1, "pose_like": "x", "look": {"base": [2, 0, 0]}}"#,
            br#"{"schema_version": 1, "pose_like": "x", "extra": 1}"#,
        ] {
            assert!(Spec::parse(bad).is_err(), "{}", String::from_utf8_lossy(bad));
        }
    }
}
