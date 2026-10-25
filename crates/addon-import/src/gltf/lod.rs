//! The lower detail a body draws at a distance: its meshes simplified by
//! collapsing edges in order of least shape error (Garland and Heckbert's
//! quadric error metric). A vertex collapses onto a neighbour and takes that
//! neighbour's own position, normal, UV and bone weights, so the result
//! needs no new attributes and keeps its skinning. A seam's copies of a
//! vertex move together, each along its own side, so seams stay closed;
//! open edges stay put, and no collapse crosses between parts that follow
//! different bones, so the low detail neither tears nor bends at a joint.

use bri_content::shape::{Influence, Mesh, Primitive, Skin};
use glam::{DMat4, DVec3, DVec4};
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
};

/// `mesh` with at most about `target` triangles (fewer when every remaining
/// collapse would open a border, bend a joint or fold a face over).
pub fn simplify(mesh: &Mesh, target: usize) -> Mesh {
    let mut triangles: Vec<([u32; 3], usize)> = mesh
        .primitives
        .iter()
        .enumerate()
        .flat_map(|(p, prim)| prim.triangles.iter().map(move |t| (*t, p)))
        .collect();
    let count = mesh.frame_vertices.min(mesh.positions.len());
    let position = |v: u32| {
        let p = mesh.positions[v as usize];
        DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
    };

    // Vertices at one position are one corner, split by a UV or material
    // seam into copies (only those the faces use: primitives may share one
    // vertex list). Collapses move whole corners, every copy along its own
    // side of the seam, so a seam stays closed.
    let mut used = vec![false; count];
    for (t, _) in &triangles {
        for v in t {
            used[*v as usize] = true;
        }
    }
    let mut by_position: HashMap<[u32; 3], u32> = HashMap::new();
    let mut corner = vec![0u32; count];
    let mut copies: Vec<Vec<u32>> = Vec::new();
    for v in 0..count {
        let next = copies.len() as u32;
        let c = *by_position
            .entry(mesh.positions[v].map(f32::to_bits))
            .or_insert(next);
        if c == next {
            copies.push(Vec::new());
        }
        corner[v] = c;
        if used[v] {
            copies[c as usize].push(v as u32);
        }
    }
    let corners = copies.len();
    let mut place = vec![DVec3::ZERO; corners];
    for v in 0..count {
        place[corner[v] as usize] = position(v as u32);
    }
    let at = |c: u32| place[c as usize];
    // An edge between corners only one face uses is an open border.
    let mut locked = vec![false; corners];
    let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
    for (t, _) in &triangles {
        for i in 0..3 {
            let (a, b) = (corner[t[i] as usize], corner[t[(i + 1) % 3] as usize]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    for ((a, b), uses) in &edges {
        if *uses == 1 {
            locked[*a as usize] = true;
            locked[*b as usize] = true;
        }
    }
    // The bone each corner mostly follows: a collapse never crosses from
    // one bone's part to another's.
    let mut bone = vec![usize::MAX; corners];
    if let Some(skin) = &mesh.skin {
        let mut best = vec![0.0f32; count];
        let mut vertex_bone = vec![usize::MAX; count];
        for i in &skin.influences {
            if i.vertex < count && i.weight > best[i.vertex] {
                best[i.vertex] = i.weight;
                vertex_bone[i.vertex] = i.bone;
            }
        }
        for (c, list) in copies.iter().enumerate() {
            if let Some(v) = list.first() {
                bone[c] = vertex_bone[*v as usize];
            }
            if list.iter().any(|v| vertex_bone[*v as usize] != bone[c]) {
                locked[c] = true;
            }
        }
    }

    // Each corner's error quadric: the planes of the faces around it.
    let mut quadric = vec![DMat4::ZERO; corners];
    let mut around: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (ti, (t, _)) in triangles.iter().enumerate() {
        for v in t {
            around[*v as usize].push(ti);
        }
        let [a, b, c] = t.map(position);
        let cross = (b - a).cross(c - a);
        let Some(n) = cross.try_normalize() else {
            continue;
        };
        let plane = DVec4::new(n.x, n.y, n.z, -n.dot(a));
        let q = outer(plane) * (cross.length() * 0.5).max(1e-12);
        for v in t {
            quadric[corner[*v as usize] as usize] += q;
        }
    }
    let mut alive = vec![true; triangles.len()];
    let mut live = triangles.len();
    let mut version = vec![0u32; corners];
    let mut removed = vec![false; corners];
    let mut heap = BinaryHeap::new();
    let neighbours = |around: &[Vec<usize>],
                      alive: &[bool],
                      triangles: &[([u32; 3], usize)],
                      copies: &[Vec<u32>],
                      c: u32| {
        let mut out: Vec<u32> = copies[c as usize]
            .iter()
            .flat_map(|v| around[*v as usize].iter())
            .filter(|t| alive[**t])
            .flat_map(|t| triangles[*t].0)
            .map(|v| corner[v as usize])
            .filter(|n| *n != c)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    };
    let push = |heap: &mut BinaryHeap<Candidate>,
                quadric: &[DMat4],
                version: &[u32],
                from: u32,
                to: u32| {
        if locked[from as usize] || bone[from as usize] != bone[to as usize] {
            return;
        }
        let p = at(to).extend(1.0);
        heap.push(Candidate {
            cost: p.dot((quadric[from as usize] + quadric[to as usize]) * p),
            from,
            to,
            version: version[from as usize],
        });
    };
    for c in 0..corners as u32 {
        for n in neighbours(&around, &alive, &triangles, &copies, c) {
            push(&mut heap, &quadric, &version, c, n);
        }
    }
    while live > target {
        let Some(c) = heap.pop() else { break };
        if c.version != version[c.from as usize] || removed[c.to as usize] {
            continue;
        }
        // Each copy of `from` moves to the copy of `to` it shares a face
        // with; a copy with none would tear its seam open.
        let mut moves = Vec::new();
        for &a in &copies[c.from as usize] {
            let b = around[a as usize]
                .iter()
                .filter(|t| alive[**t])
                .flat_map(|t| triangles[*t].0)
                .find(|v| corner[*v as usize] == c.to);
            match b {
                Some(b) => moves.push((a, b)),
                None => break,
            }
        }
        if moves.len() != copies[c.from as usize].len() {
            continue;
        }
        let target_of = |v: u32| moves.iter().find(|(a, _)| *a == v).map(|(_, b)| *b);
        // The faces it keeps must not fold over.
        let folds = moves.iter().any(|(a, _)| {
            around[*a as usize].iter().filter(|t| alive[**t]).any(|t| {
                let (tri, _) = triangles[*t];
                if tri.iter().any(|v| corner[*v as usize] == c.to) {
                    return false;
                }
                let before = normal(tri.map(position));
                let after = normal(tri.map(|v| position(target_of(v).unwrap_or(v))));
                match (before, after) {
                    (Some(b), Some(a)) => b.dot(a) < 0.2,
                    _ => true,
                }
            })
        });
        if folds {
            continue;
        }
        for &(a, b) in &moves {
            for t in std::mem::take(&mut around[a as usize]) {
                if !alive[t] {
                    continue;
                }
                if triangles[t].0.iter().any(|v| corner[*v as usize] == c.to) {
                    alive[t] = false;
                    live -= 1;
                    continue;
                }
                for v in &mut triangles[t].0 {
                    if *v == a {
                        *v = b;
                    }
                }
                around[b as usize].push(t);
            }
        }
        let q = quadric[c.from as usize];
        quadric[c.to as usize] += q;
        removed[c.from as usize] = true;
        version[c.from as usize] = u32::MAX;
        copies[c.from as usize].clear();
        let ring = neighbours(&around, &alive, &triangles, &copies, c.to);
        for &n in ring.iter().chain([&c.to]) {
            version[n as usize] = version[n as usize].wrapping_add(1) % (u32::MAX - 1);
            for m in neighbours(&around, &alive, &triangles, &copies, n) {
                push(&mut heap, &quadric, &version, n, m);
            }
        }
    }

    // Keep only the vertices the remaining faces use, in their old order.
    let mut new_index = vec![u32::MAX; count];
    let mut out = Mesh {
        frame_vertices: 0,
        positions: Vec::new(),
        normals: Vec::new(),
        uv: Vec::new(),
        primitives: mesh
            .primitives
            .iter()
            .map(|p| Primitive {
                zone: p.zone,
                material: p.material,
                triangles: Vec::new(),
            })
            .collect(),
        skin: None,
        billboard: mesh.billboard,
        billboard_y: mesh.billboard_y,
        uv1: Vec::new(),
        colors: Vec::new(),
        vertex_rig: None,
    };
    triangles.retain({
        let mut i = 0;
        move |_| {
            i += 1;
            alive[i - 1]
        }
    });
    for (t, _) in &triangles {
        for v in t {
            let v = *v as usize;
            if new_index[v] == u32::MAX {
                new_index[v] = out.positions.len() as u32;
                out.positions.push(mesh.positions[v]);
                out.normals
                    .push(mesh.normals.get(v).copied().unwrap_or([0.0, 1.0, 0.0]));
                out.uv.push(mesh.uv.get(v).copied().unwrap_or([0.0, 0.0]));
                if !mesh.uv1.is_empty() {
                    out.uv1.push(mesh.uv1.get(v).copied().unwrap_or([0.0, 0.0]));
                }
            }
        }
    }
    for (t, p) in triangles {
        out.primitives[p]
            .triangles
            .push(t.map(|v| new_index[v as usize]));
    }
    out.primitives.retain(|p| !p.triangles.is_empty());
    out.frame_vertices = out.positions.len();
    out.skin = mesh.skin.as_ref().map(|skin| Skin {
        inverse_bind: skin.inverse_bind.clone(),
        nodes: skin.nodes.clone(),
        influences: skin
            .influences
            .iter()
            .filter(|i| i.vertex < count && new_index[i.vertex] != u32::MAX)
            .map(|i| Influence {
                vertex: new_index[i.vertex] as usize,
                bone: i.bone,
                weight: i.weight,
            })
            .collect(),
    });
    out
}

/// A mesh's triangle count.
pub fn triangles(mesh: &Mesh) -> usize {
    mesh.primitives.iter().map(|p| p.triangles.len()).sum()
}

fn outer(p: DVec4) -> DMat4 {
    DMat4::from_cols(p * p.x, p * p.y, p * p.z, p * p.w)
}

fn normal([a, b, c]: [DVec3; 3]) -> Option<DVec3> {
    (b - a).cross(c - a).try_normalize()
}

struct Candidate {
    cost: f64,
    from: u32,
    to: u32,
    version: u32,
}
impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Candidate {}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Candidate {
    /// The cheapest collapse first (the heap is a max-heap).
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.from.cmp(&self.from))
            .then_with(|| other.to.cmp(&self.to))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf::testing::{cuboid, rounded_cuboid};
    use glam::Vec3;

    fn mesh(parts: (Vec<Vec3>, Vec<Vec3>, Vec<[f32; 2]>, Vec<u32>)) -> Mesh {
        let (p, n, uv, i) = parts;
        Mesh {
            frame_vertices: p.len(),
            positions: p.iter().map(|v| v.to_array()).collect(),
            normals: n.iter().map(|v| v.to_array()).collect(),
            uv,
            primitives: vec![Primitive {
                zone: None,
                material: Some(0),
                triangles: i.chunks(3).map(|t| [t[0], t[1], t[2]]).collect(),
            }],
            skin: None,
            billboard: false,
            billboard_y: false,
            uv1: Vec::new(),
            colors: Vec::new(),
            vertex_rig: None,
        }
    }

    #[test]
    fn a_rounded_box_simplifies_to_its_budget_and_stays_closed() {
        let full = mesh(rounded_cuboid(Vec3::ZERO, Vec3::ONE, 16));
        assert_eq!(triangles(&full), 6 * 2 * 16 * 16);
        let low = simplify(&full, 400);
        assert!(triangles(&low) <= 400, "{}", triangles(&low));
        assert!(triangles(&low) > 200, "{}", triangles(&low));
        // Still closed: each edge, by position, joins exactly two faces,
        // so the seams between the box's sides did not open.
        let mut edges: HashMap<([u32; 3], [u32; 3]), u32> = HashMap::new();
        for t in &low.primitives[0].triangles {
            for i in 0..3 {
                let a = low.positions[t[i] as usize].map(f32::to_bits);
                let b = low.positions[t[(i + 1) % 3] as usize].map(f32::to_bits);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(edges.values().all(|n| *n == 2), "a seam opened");
    }

    #[test]
    fn a_box_keeps_its_corners() {
        let full = mesh(cuboid(Vec3::ZERO, Vec3::ONE));
        assert_eq!(
            triangles(&simplify(&full, 4)),
            12,
            "any collapse folds a side"
        );
    }
}
