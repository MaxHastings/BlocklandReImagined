//! Torque (TGE 1.x, as built into Blockland v20) player collision.
//!
//! v20 players never touch a physics solver: `Player::updatePos` sweeps the
//! axis-aligned player box through the polygons of nearby convexes
//! (`ExtrudedPolyList`), stops at the first face it would cross, removes the
//! velocity into that polygon's plane and retries. `findContact` picks the
//! flattest polygon in a thin slab under the feet, and `Player::step` lifts the
//! box onto the highest vertex it can clear. Collision normals are always the
//! hit polygon's own plane, never a separating axis, so the hidden top of one
//! ramp under the next never reads as ground and brick seams do not catch.
//!
//! Evidence: the TGE `player.cc`/`extrudedPolyList.cc` sources, checked
//! against a read-only disassembly of `blocklandv20.exe` (`updatePos`
//! 0x5B0714, `step` 0x5A9FD0, `findContact` 0x5AA570). Torque's z axis is our
//! y axis.
use glam::Vec3;
use rapier3d::prelude::*;

/// `sTractionDistance` (0x775C04): contact slab depth under the feet.
pub const TRACTION: f32 = 0.013;
/// `sMinFaceDistance` (0x775C00).
pub const MIN_FACE_DISTANCE: f32 = 0.01;
/// `sNormalElasticity` (0x775C08): the speed a hit leaves the player moving
/// away from the polygon. A speed, so it holds at any tick rate.
pub const NORMAL_ELASTICITY: f32 = 0.01;
/// The distance `updatePos` backs off after each hit (0x70F984).
pub const BACK_OFF: f32 = 0.01;
/// `sMoveRetryCount` (0x775C0C).
pub const MOVE_RETRIES: usize = 5;
/// `sVerticalStepDot`.
pub const VERTICAL_STEP_DOT: f32 = 0.05;
/// `ExtrudedPolyList` equality epsilon, against a 32 ms Torque tick's move.
pub const EQUAL_EPSILON: f32 = 0.0001;
/// How far inside a face's swept volume a polygon must reach to count.
const SIDE_TOLERANCE: f32 = 1e-5;
/// `CollisionList::MaxCollisions`.
const MAX_COLLISIONS: usize = 64;

/// What a polygon belongs to, standing in for Torque object type masks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Bricks and map architecture (`StaticObjectType`).
    Static,
    /// Terrain: stepping never triggers on it (`TerrainObjectType`).
    Terrain,
    /// Players, vehicles and other moving bodies.
    Actor,
}

#[derive(Debug, Clone, Copy)]
pub struct Poly {
    first: u32,
    count: u32,
    pub normal: Vec3,
    pub kind: Kind,
    /// The owning collider's `user_data`.
    pub tag: u128,
    /// The owning collider.
    pub collider: ColliderHandle,
    min: Vec3,
    max: Vec3,
}

/// Axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box3 {
    pub min: Vec3,
    pub max: Vec3,
}
impl Box3 {
    pub fn overlaps(&self, min: Vec3, max: Vec3) -> bool {
        self.min.cmple(max).all() && min.cmple(self.max).all()
    }
    pub fn union(self, other: Box3) -> Box3 {
        Box3 {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
    pub fn expanded(self, by: Vec3) -> Box3 {
        Box3 {
            min: self.min - by,
            max: self.max + by,
        }
    }
    /// Clip planes pointing out of the box: (normal, offset), outside when
    /// `normal . p - offset` is positive.
    fn planes(&self) -> [(Vec3, f32); 6] {
        [
            (Vec3::NEG_X, -self.min.x),
            (Vec3::X, self.max.x),
            (Vec3::NEG_Y, -self.min.y),
            (Vec3::Y, self.max.y),
            (Vec3::NEG_Z, -self.min.z),
            (Vec3::Z, self.max.z),
        ]
    }
}

fn v3(v: Vector) -> Vec3 {
    Vec3::from_array(v.to_array())
}
fn rv(v: Vec3) -> Vector {
    Vector::from_array(v.to_array())
}

/// The polygons of every solid collider around a player, gathered once per
/// tick (Torque's convex working list). Points are stored relative to
/// `origin` (the player's feet), so sweeps a fraction of a millimetre long
/// keep full float precision far from the world origin; the public functions
/// take and return world positions.
#[derive(Default)]
pub struct Soup {
    pub origin: Vec3,
    /// The collider whose shape is being added.
    current: ColliderHandle,
    points: Vec<Vec3>,
    pub polys: Vec<Poly>,
}
impl Soup {
    pub fn gather(
        query: &rapier3d::pipeline::QueryPipeline<'_>,
        bodies: &RigidBodySet,
        region: Box3,
        origin: Vec3,
    ) -> Soup {
        let mut soup = Soup {
            origin,
            ..Default::default()
        };
        let aabb = Aabb::new(rv(region.min), rv(region.max));
        let mut found: Vec<_> = query.intersect_aabb_conservative(aabb).collect();
        // Rapier's traversal order depends on insertion history; collide in
        // tag (brick id) order so client and server resolve ties identically.
        found.sort_by_key(|(h, c)| (c.user_data, h.into_raw_parts()));
        for (handle, collider) in found {
            soup.current = handle;
            let kind = if collider.shape().as_heightfield().is_some() {
                Kind::Terrain
            } else if collider
                .parent()
                .and_then(|b| bodies.get(b))
                .is_some_and(|b| !b.is_fixed())
            {
                Kind::Actor
            } else {
                Kind::Static
            };
            soup.add_shape(
                collider.shape(),
                collider.position(),
                kind,
                collider.user_data,
                &region,
            );
        }
        soup
    }
    fn add_shape(&mut self, shape: &dyn Shape, pose: &Pose, kind: Kind, tag: u128, region: &Box3) {
        // Relative to the origin before adding the small local offset.
        let (rotation, shift) = (pose.rotation, v3(pose.translation) - self.origin);
        let to = move |p: Vector| v3(rotation * p) + shift;
        if let Some(c) = shape.as_cuboid() {
            let h = v3(c.half_extents);
            let corner = |x: f32, y: f32, z: f32| to(rv(h * Vec3::new(x, y, z)));
            let faces = [
                [(1., -1., -1.), (1., 1., -1.), (1., 1., 1.), (1., -1., 1.)],
                [
                    (-1., -1., 1.),
                    (-1., 1., 1.),
                    (-1., 1., -1.),
                    (-1., -1., -1.),
                ],
                [(-1., 1., -1.), (-1., 1., 1.), (1., 1., 1.), (1., 1., -1.)],
                [
                    (-1., -1., 1.),
                    (-1., -1., -1.),
                    (1., -1., -1.),
                    (1., -1., 1.),
                ],
                [(-1., -1., 1.), (1., -1., 1.), (1., 1., 1.), (-1., 1., 1.)],
                [
                    (1., -1., -1.),
                    (-1., -1., -1.),
                    (-1., 1., -1.),
                    (1., 1., -1.),
                ],
            ];
            for face in faces {
                let verts = face.map(|(x, y, z)| corner(x, y, z));
                self.push(&verts, None, kind, tag);
            }
        } else if let Some(c) = shape.as_convex_polyhedron() {
            let points: Vec<Vec3> = c.points().iter().map(|p| to(*p)).collect();
            let adjacent = c.vertices_adj_to_face();
            for face in c.faces() {
                let normal = v3(pose.rotation * face.normal);
                let ids = &adjacent[face.first_vertex_or_edge as usize
                    ..(face.first_vertex_or_edge + face.num_vertices_or_edges) as usize];
                let mut verts: Vec<Vec3> = ids.iter().map(|&i| points[i as usize]).collect();
                wind(&mut verts, normal);
                self.push(&verts, Some(normal), kind, tag);
            }
        } else if let Some(c) = shape.as_compound() {
            for (sub, part) in c.shapes() {
                self.add_shape(part.as_ref(), &(*pose * *sub), kind, tag, region);
            }
        } else if let Some(mesh) = shape.as_trimesh() {
            let local = Aabb::new(rv(region.min), rv(region.max)).transform_by(&pose.inverse());
            for i in mesh.bvh().intersect_aabb(&local) {
                let t = mesh.triangle(i);
                let verts = [t.a, t.b, t.c].map(to);
                self.push(&verts, None, kind, tag);
            }
        } else if let Some(field) = shape.as_heightfield() {
            let local = Aabb::new(rv(region.min), rv(region.max)).transform_by(&pose.inverse());
            field.map_elements_in_local_aabb(&local, &mut |_, t| {
                let verts = [t.a, t.b, t.c].map(to);
                self.push(&verts, None, kind, tag);
            });
        } else if let Some(t) = shape.as_triangle() {
            let verts = [t.a, t.b, t.c].map(to);
            self.push(&verts, None, kind, tag);
        } else {
            // Rounded shapes (vehicle wheels): their bounding box.
            let aabb = shape.compute_local_aabb();
            let half = (aabb.maxs - aabb.mins) * 0.5;
            let center = (aabb.maxs + aabb.mins) * 0.5;
            let boxed = Cuboid::new(half);
            self.add_shape(
                &boxed,
                &(*pose * Pose::from_translation(center)),
                kind,
                tag,
                region,
            );
        }
    }
    fn push(&mut self, verts: &[Vec3], normal: Option<Vec3>, kind: Kind, tag: u128) {
        let normal = match normal {
            Some(n) => n,
            None => {
                let n = (verts[1] - verts[0]).cross(verts[2] - verts[0]);
                match n.try_normalize() {
                    Some(n) => n,
                    None => return,
                }
            }
        };
        let first = self.points.len() as u32;
        self.points.extend_from_slice(verts);
        let (min, max) = verts
            .iter()
            .fold((Vec3::MAX, Vec3::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        self.polys.push(Poly {
            first,
            count: verts.len() as u32,
            normal,
            kind,
            tag,
            collider: self.current,
            min,
            max,
        });
    }
    pub fn verts(&self, poly: &Poly) -> &[Vec3] {
        &self.points[poly.first as usize..(poly.first + poly.count) as usize]
    }
    /// `ClippedPolyList`: the polygons clipped to `bounds`, as (poly, vertices).
    fn clipped<'a>(
        &'a self,
        bounds: Box3,
        accept: impl Fn(&Poly) -> bool + 'a,
    ) -> impl Iterator<Item = (&'a Poly, Vec<Vec3>)> + 'a {
        let planes = bounds.planes();
        self.polys.iter().filter_map(move |poly| {
            if !accept(poly) || !bounds.overlaps(poly.min, poly.max) {
                return None;
            }
            let mut verts: Vec<(Vec3, bool)> =
                self.verts(poly).iter().map(|p| (*p, false)).collect();
            for (normal, offset) in planes {
                verts = clip(&verts, normal, offset, false)?;
            }
            (verts.len() >= 3).then(|| (poly, verts.into_iter().map(|(p, _)| p).collect()))
        })
    }
}

/// Order a convex face's vertices counter-clockwise around its normal.
fn wind(verts: &mut [Vec3], normal: Vec3) {
    let center = verts.iter().copied().sum::<Vec3>() / verts.len() as f32;
    let u = (verts[0] - center).normalize_or_zero();
    let w = normal.cross(u);
    verts.sort_by(|a, b| {
        let angle = |p: &Vec3| {
            let d = *p - center;
            d.dot(w).atan2(d.dot(u))
        };
        angle(a).total_cmp(&angle(b))
    });
}

/// Keep the part of a polygon inside `normal . p < offset`. Torque marks an
/// original vertex lying exactly on a plane as outside (`>= 0`, the extruded
/// list) or inside (`> 0`, clipped lists); generated vertices use `> 0`.
/// Returns `None` when nothing remains.
fn clip(
    verts: &[(Vec3, bool)],
    normal: Vec3,
    offset: f32,
    on_plane_outside: bool,
) -> Option<Vec<(Vec3, bool)>> {
    let outside = |(p, generated): &(Vec3, bool)| {
        let d = normal.dot(*p) - offset;
        if on_plane_outside && !generated {
            d >= 0.0
        } else {
            d > 0.0
        }
    };
    let flags: Vec<bool> = verts.iter().map(outside).collect();
    if flags.iter().all(|o| *o) {
        return None;
    }
    if !flags.iter().any(|o| *o) {
        return Some(verts.to_vec());
    }
    let mut out = Vec::with_capacity(verts.len() + 2);
    let mut i1 = verts.len() - 1;
    for i2 in 0..verts.len() {
        if flags[i1] != flags[i2] {
            let (a, b) = (verts[i1].0, verts[i2].0);
            let vv = b - a;
            let t = -(normal.dot(a) - offset) / normal.dot(vv);
            out.push((a + vv * t, true));
        }
        if !flags[i2] {
            out.push(verts[i2]);
        }
        i1 = i2;
    }
    (out.len() >= 3).then_some(out)
}

#[derive(Debug, Clone, Copy)]
pub struct Collision {
    pub normal: Vec3,
    pub face_dot: f32,
    pub point: Vec3,
    pub kind: Kind,
    pub tag: u128,
    pub collider: ColliderHandle,
}

#[derive(Debug)]
pub struct CollisionList {
    /// Fraction of the swept vector at the first contact; 2 when none.
    pub t: f32,
    pub max_height: f32,
    pub hits: Vec<Collision>,
}

/// `EqualEpsilon` as this motor's tick sees it. Torque compares against moves
/// of one 32 ms tick; a 120 Hz move is 3.84 times shorter, so the same
/// absolute distances are a smaller threshold for a face to lead the move and
/// a larger fraction of the move for two hits to tie. Unscaled, float noise
/// splits the tie between a slope and a hidden brick end face along its edge.
#[derive(Debug, Clone, Copy)]
pub struct Epsilon {
    /// Least distance a box face must travel to lead the move.
    pub face: f32,
    /// Fractions of the move closer than this are simultaneous hits.
    pub tie: f32,
}
impl Epsilon {
    pub fn at(torque_ticks: f32) -> Self {
        Self {
            face: EQUAL_EPSILON * torque_ticks,
            tie: EQUAL_EPSILON / torque_ticks,
        }
    }
}

/// One face of the moving box, extruded along the move (`ExtrudedFace`).
struct Face {
    normal: Vec3,
    offset: f32,
    max_distance: f32,
    /// Side planes through the face's edges, parallel to the move.
    sides: [(Vec3, f32); 4],
}

/// `ExtrudedPolyList`: the first polygons the box crosses moving by `vector`.
fn collide(
    soup: &Soup,
    bounds: Box3,
    vector: Vec3,
    epsilon: Epsilon,
    accept: impl Fn(&Poly) -> bool,
) -> CollisionList {
    let mut list = CollisionList {
        t: 2.0,
        max_height: f32::MIN,
        hits: Vec::new(),
    };
    let Some(heading) = vector.try_normalize() else {
        return list;
    };
    let faces = extrude(bounds, vector, epsilon.face);
    let mut found = Vec::new();
    let swept = bounds.union(Box3 {
        min: bounds.min + vector,
        max: bounds.max + vector,
    });
    for poly in &soup.polys {
        if !accept(poly) || !swept.overlaps(poly.min, poly.max) || poly.normal.dot(heading) > 0.0 {
            continue;
        }
        let verts = soup.verts(poly);
        // Faces meeting the polygon head-on first; edge contacts otherwise.
        let mut best: Option<(f32, f32, f32, Vec3)> = None;
        for edge in [false, true] {
            for face in &faces {
                let face_dot = -face.normal.dot(poly.normal);
                if (face_dot > 0.0) == edge {
                    continue;
                }
                if let Some((time, height, point)) = test_poly(face, verts)
                    && best.is_none_or(|(dot, ..)| face_dot > dot)
                {
                    best = Some((face_dot, time, height, point));
                }
            }
            if best.is_some() {
                break;
            }
        }
        let Some((face_dot, time, height, point)) = best else {
            continue;
        };
        if time < 1.0 {
            found.push((
                time,
                height,
                Collision {
                    normal: poly.normal,
                    face_dot,
                    point,
                    kind: poly.kind,
                    tag: poly.tag,
                    collider: poly.collider,
                },
            ));
        }
    }
    // Every hit within the tie of the earliest counts as simultaneous. Torque
    // compares each hit against the running earliest, so which near-ties
    // survive depends on polygon order; keeping all of them is its exact-
    // arithmetic result, and lets the most parallel face win as intended.
    if let Some(first) = found.iter().map(|(t, ..)| *t).min_by(f32::total_cmp) {
        list.t = first;
        for (time, height, hit) in found {
            if time <= first + epsilon.tie && list.hits.len() < MAX_COLLISIONS {
                list.max_height = list.max_height.max(height);
                list.hits.push(hit);
            }
        }
    }
    list
}

fn extrude(bounds: Box3, vector: Vec3, epsilon: f32) -> Vec<Face> {
    let (lo, hi) = (bounds.min, bounds.max);
    let mut faces = Vec::with_capacity(3);
    for axis in 0..3 {
        for sign in [1.0_f32, -1.0] {
            let mut normal = Vec3::ZERO;
            normal[axis] = sign;
            let max_distance = normal.dot(vector);
            if max_distance <= epsilon {
                continue;
            }
            let offset = if sign > 0.0 { hi[axis] } else { -lo[axis] };
            // The face's rectangle in the other two axes.
            let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
            let plane = if sign > 0.0 { hi[axis] } else { lo[axis] };
            let corner = |ua: f32, ub: f32| {
                let mut p = Vec3::ZERO;
                p[axis] = plane;
                p[a] = ua;
                p[b] = ub;
                p
            };
            let quad = [
                corner(lo[a], lo[b]),
                corner(hi[a], lo[b]),
                corner(hi[a], hi[b]),
                corner(lo[a], hi[b]),
            ];
            let center = (quad[0] + quad[2]) * 0.5;
            let sides = std::array::from_fn(|i| {
                let (p1, p2) = (quad[i], quad[(i + 1) % 4]);
                let mut n = (p2 - p1).cross(vector).normalize();
                if n.dot(center - p1) > 0.0 {
                    n = -n;
                }
                (n, n.dot(p1))
            });
            faces.push(Face {
                normal,
                offset,
                max_distance,
                sides,
            });
        }
    }
    faces
}

/// `ExtrudedPolyList::testPoly`: clip the polygon to the face's swept volume
/// and find when the face reaches its nearest remaining point. Returns
/// (time as a fraction of the move, highest point reached, contact point).
fn test_poly(face: &Face, verts: &[Vec3]) -> Option<(f32, f32, Vec3)> {
    let mut clipped: Vec<(Vec3, bool)> = verts.iter().map(|p| (*p, false)).collect();
    // In front of the face (the inverted face plane), then the sides.
    clipped = clip(&clipped, -face.normal, -face.offset, true)?;
    // Torque counts a polygon merely touching the swept volume's side as a
    // miss; a hair of tolerance keeps float noise from turning a brick's end
    // face, whose edge runs exactly under the box's corner along a slope, into
    // a head-on hit that stops a rider dead.
    for (normal, offset) in face.sides {
        clipped = clip(&clipped, normal, offset - SIDE_TOLERANCE, true)?;
    }
    let mut bd = 1e30_f32;
    let mut bp = Vec3::ZERO;
    let mut height = -1e30_f32;
    for (p, _) in &clipped {
        let dist = face.normal.dot(*p) - face.offset;
        if dist <= bd {
            bd = dist.max(0.0);
            bp = *p;
        }
        if p.y > height && dist < face.max_distance {
            height = p.y;
        }
    }
    (bd < face.max_distance).then(|| (bd / face.max_distance, height, bp))
}

/// `Player::findContact`: the flattest polygon within `TRACTION` of the feet.
#[derive(Debug, Clone, Copy, Default)]
pub struct Contact {
    pub run: bool,
    pub jump: bool,
    /// The flattest polygon's plane normal, when anything was touched.
    pub normal: Option<Vec3>,
    pub tag: Option<u128>,
}
pub fn find_contact(
    soup: &Soup,
    feet: Vec3,
    half_width: f32,
    run_cos: f32,
    jump_cos: f32,
) -> Contact {
    contact_local(soup, feet - soup.origin, half_width, run_cos, jump_cos)
}
fn contact_local(soup: &Soup, feet: Vec3, half_width: f32, run_cos: f32, jump_cos: f32) -> Contact {
    let slab = Box3 {
        min: Vec3::new(feet.x - half_width, feet.y - TRACTION, feet.z - half_width),
        max: Vec3::new(feet.x + half_width, feet.y + TRACTION, feet.z + half_width),
    };
    let mut best = -1.0_f32;
    let mut contact = Contact::default();
    for (poly, _) in soup.clipped(slab, |_| true) {
        if poly.normal.y > best {
            best = poly.normal.y;
            contact.normal = Some(poly.normal);
            contact.tag = Some(poly.tag);
        }
    }
    contact.run = best > run_cos;
    contact.jump = best > jump_cos;
    contact
}

/// v20 `Player::step` (0x5A9FD0): at the move's destination, the highest
/// static vertex under `max_step` with none within the player's own height
/// above it. Only that height must be clear, not `maxStepHeight` more.
fn step(
    soup: &Soup,
    feet: &mut Vec3,
    max_step: &mut f32,
    offset: Vec3,
    contact_y: f32,
    m: &Mover,
) -> bool {
    let (half_width, height, step_reach) = (m.half_width, m.height, m.step_reach);
    let at = *feet + offset;
    let bounds = Box3 {
        min: Vec3::new(at.x - half_width, at.y, at.z - half_width),
        max: Vec3::new(
            at.x + half_width,
            at.y + height + step_reach + 2.0 * MIN_FACE_DISTANCE,
            at.z + half_width,
        ),
    };
    let heights: Vec<f32> = soup
        .clipped(bounds, |p| p.kind != Kind::Actor)
        .flat_map(|(_, verts)| verts.into_iter().map(|p| p.y + MIN_FACE_DISTANCE))
        .collect();
    let mut best = feet.y - MIN_FACE_DISTANCE;
    for &h in &heights {
        if h > best
            && h - feet.y < *max_step
            && !heights.iter().any(|&o| {
                let d = o - h;
                d > 0.0 && d < height
            })
        {
            best = h;
        }
    }
    let rise = best - feet.y;
    // v20 accepts a zero step (`>=`), TGE only a rise (`>`). With the 0.01
    // back-off equal to sMinFaceDistance, the floor a fall just reached sits at
    // the feet and qualifies only by float noise; stepping onto it loops the
    // move to its retry limit and cancels the landing's impact. Measured from
    // the contact point (before the back-off, which is scaled here), only
    // vertices above the feet count, as in TGE.
    if best >= feet.y && best > contact_y + MIN_FACE_DISTANCE && rise < *max_step {
        feet.y = best;
        *max_step -= rise;
        true
    } else {
        false
    }
}

/// `EarlyOutPolyList`: whether any polygon reaches into `bounds`.
fn any_in(soup: &Soup, bounds: Box3) -> bool {
    soup.clipped(bounds, |_| true).next().is_some()
}

/// The player's collision constants for one `update_pos`.
pub struct Mover {
    pub half_width: f32,
    pub height: f32,
    pub run_cos: f32,
    pub jump_cos: f32,
    /// `maxStepHeight`: the step budget for one move.
    pub max_step: f32,
    /// `maxStepHeight * scale`: how high a collision may be and still be
    /// stepped over, and how far above the head step polygons are gathered.
    pub step_reach: f32,
    /// `sNormalElasticity`.
    pub elasticity: f32,
    /// The 0.01 back-off after each hit, scaled to this motor's tick: every
    /// hit backs off, so at 120 Hz an unscaled back-off costs 3.84 times the
    /// distance per second and stalls a player grinding along a lane.
    pub back_off: f32,
    pub epsilon: Epsilon,
}

pub struct Moved {
    pub feet: Vec3,
    /// Tags of every polygon the box hit.
    pub touched: Vec<u128>,
    /// Each blocking hit's collider and the speed into its surface before
    /// the hit stopped it (`bd`, what v20 passes to `onImpact`), in order.
    pub hit: Vec<(ColliderHandle, f32)>,
    /// The last blocking polygon faced straight down (v20 0x8A2, which
    /// `canJump` refuses on).
    pub ceiling: bool,
}

/// v20 `Player::updatePos` (0x5B0714): move `feet` by `velocity * time`,
/// stopping at each first polygon hit, backing off 0.01, stepping up where
/// allowed, and otherwise removing the velocity into the polygon plus a little
/// elasticity. The second hit re-aims along the crease; five hits in one move
/// give up and stop dead.
pub fn update_pos(soup: &Soup, m: &Mover, feet: Vec3, velocity: &mut Vec3, time: f32) -> Moved {
    let mut moved = update_local(soup, m, feet - soup.origin, velocity, time);
    moved.feet += soup.origin;
    moved
}
fn update_local(soup: &Soup, m: &Mover, feet: Vec3, velocity: &mut Vec3, time: f32) -> Moved {
    let half = m.half_width;
    let body = |at: Vec3| Box3 {
        min: Vec3::new(at.x - half, at.y, at.z - half),
        max: Vec3::new(at.x + half, at.y + m.height, at.z + half),
    };
    let initial = feet;
    let mut start = feet;
    let mut time = time;
    let mut max_step = m.max_step;
    let mut first_normal = Vec3::ZERO;
    let mut touched = Vec::new();
    let mut colliders = Vec::new();
    let mut ceiling = false;
    let mut count = 0;
    while count < MOVE_RETRIES {
        let speed = velocity.length();
        if speed == 0.0 {
            break;
        }
        let end = start + *velocity * time;
        let distance = end - start;
        // Moves shorter than the box end freely when nothing is at the end.
        if distance.x.abs() < 2.0 * half
            && distance.y.abs() < m.height
            && distance.z.abs() < 2.0 * half
            && !any_in(soup, body(end))
        {
            start = end;
            break;
        }
        let list = collide(soup, body(start), distance, m.epsilon, |_| true);
        if list.hits.is_empty() || list.t >= 1.0 {
            start = end;
            break;
        }
        let dt = time * list.t.min(1.0);
        start += *velocity * dt;
        time -= dt;
        // Back off 0.01 (per Torque tick) along the move.
        let backed = *velocity * (m.back_off / speed).min(dt);
        start -= backed;
        let contact_y = start.y + backed.y;
        // v20 steps only from a run surface, over hits low enough, off walls
        // or walkable slopes, and never off terrain.
        if contact_local(soup, start, half, m.run_cos, m.jump_cos).run
            && list.max_height < start.y + m.step_reach
            && list.hits.iter().any(|c| {
                c.kind != Kind::Terrain
                    && (c.normal.y.abs() < VERTICAL_STEP_DOT || c.normal.y > m.run_cos)
            })
            && step(
                soup,
                &mut start,
                &mut max_step,
                // The step probe looks from the contact, not the backed-off
                // box: at 120 Hz a player pushing off a riser from rest moves
                // less per tick than the back-off, so probing from behind it
                // never reaches the tread and v20's stair climb stalls.
                *velocity * time + backed,
                contact_y,
                m,
            )
        {
            count += 1;
            continue;
        }
        ceiling = list.hits.iter().any(|c| c.normal.y <= -0.99);
        // The hit most parallel to the face that struck it.
        let hit = list.hits.iter().fold(list.hits[0], |best, c| {
            if c.face_dot > best.face_dot { *c } else { best }
        });
        touched.extend(list.hits.iter().map(|c| c.tag));
        let into = -velocity.dot(hit.normal);
        colliders.push((hit.collider, into));
        let dv = hit.normal * (into + m.elasticity);
        *velocity += dv;
        if count == 0 {
            first_normal = hit.normal;
        } else if count == 1 && dv.dot(first_normal) < 0.0 && hit.normal.dot(first_normal) < 0.0 {
            // Re-aim along the crease between the two planes.
            let crease = hit.normal.cross(first_normal);
            let mut length = crease.length();
            if length > 0.0 {
                if crease.dot(*velocity) < 0.0 {
                    length = -length;
                }
                *velocity = crease * (velocity.length() / length);
            }
        }
        count += 1;
    }
    if count == MOVE_RETRIES {
        start = initial;
        *velocity = Vec3::ZERO;
    }
    Moved {
        feet: start,
        touched,
        hit: colliders,
        ceiling,
    }
}
