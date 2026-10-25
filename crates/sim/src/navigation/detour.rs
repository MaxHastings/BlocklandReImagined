//! Ownership boundary around Detour. Validated polygon arrays build an owned
//! tile; native memory images are never accepted from content.
use anyhow::Result;
use bri_content::navigation::{WalkingMesh, WalkingProfile};
use glam::Vec3;
use recastnavigation_rs::detour as dt;
use std::collections::VecDeque;

pub(super) struct Mesh {
    // Rust drops fields in declaration order: query before its owning mesh.
    query: dt::DtNavMeshQuery,
    _nav: dt::DtNavMesh,
}

// The sole owner is behind Guidance's mutex. Detour's mutable query pool and
// owning mesh never escape; every query executes under that lock.
unsafe impl Send for Mesh {}
impl Mesh {
    pub(super) fn attachment(&self, at: Vec3, step: f32) -> Option<(dt::DtPolyRef, [f32; 3])> {
        self.query
            .find_nearest_poly_1(
                &at.to_array(),
                &[3., step + 0.1, 3.],
                &dt::DtQueryFilter::default(),
            )
            .ok()
    }
    pub(super) fn new(mesh: &WalkingMesh, profile: &WalkingProfile) -> Result<Self> {
        mesh.validate()?;
        let count = mesh.polygons.len();
        let mut polys = vec![u16::MAX; count * 12];
        let mut detail_meshes = Vec::with_capacity(count);
        let mut detail_vertices = Vec::new();
        let mut detail_triangles = Vec::new();
        for (i, poly) in mesh.polygons.iter().enumerate() {
            detail_meshes.push([
                detail_vertices.len() as u32,
                poly.len() as u32,
                detail_triangles.len() as u32,
                (poly.len() - 2) as u32,
            ]);
            for (j, vertex) in poly.iter().enumerate() {
                polys[i * 12 + j] = *vertex;
                polys[i * 12 + 6 + j] = mesh.neighbours[i][j].unwrap_or(u16::MAX);
                let v = mesh.vertices[usize::from(*vertex)];
                detail_vertices.push([
                    mesh.min[0] + f32::from(v[0]) * mesh.cell_size,
                    mesh.min[1] + f32::from(v[1]) * mesh.cell_height,
                    mesh.min[2] + f32::from(v[2]) * mesh.cell_size,
                ]);
            }
            // Coarse polygon heights guide global routing only. Native motor
            // geometry resolves standing support for every local walking leg.
            for j in 2..poly.len() {
                // A fan's middle edge is always an outer boundary. Its first
                // and final spokes are boundaries only at the fan ends.
                // Detour's nearest-point query requires those boundary flags.
                let edges =
                    4 | if j == 2 { 1 } else { 0 } | if j + 1 == poly.len() { 16 } else { 0 };
                detail_triangles.push([0, (j - 1) as u8, j as u8, edges]);
            }
        }
        let flags = vec![1; count];
        let areas = vec![0; count];
        let mut params = dt::DtNavMeshCreateParams {
            verts: Some(&mesh.vertices),
            polys: Some(&polys),
            poly_flags: Some(&flags),
            poly_areas: Some(&areas),
            nvp: 6,
            detail_meshes: Some(&detail_meshes),
            detail_verts: Some(&detail_vertices),
            detail_tris: Some(&detail_triangles),
            bmin: mesh.min,
            bmax: mesh.max,
            cs: mesh.cell_size,
            ch: mesh.cell_height,
            walkable_height: profile.height,
            walkable_radius: profile.width * 0.5,
            walkable_climb: profile.step,
            build_bv_tree: true,
            ..Default::default()
        };
        let nav = dt::DtNavMesh::with_data(dt::dt_create_nav_mesh_data(&mut params)?)?;
        let query = dt::DtNavMeshQuery::with_mesh(&nav, 32768)?;
        Ok(Self { query, _nav: nav })
    }
    pub(super) fn route(&mut self, from: Vec3, goal: Vec3, step: f32) -> Option<VecDeque<Vec3>> {
        if !from.is_finite() || !goal.is_finite() {
            return None;
        }
        let filter = dt::DtQueryFilter::default();
        let extents = [3., step + 0.1, 3.];
        let (start, near_start) = self.attachment(from, step)?;
        let (end, near_end) = self.attachment(goal, step)?;
        if start.0 == 0 || end.0 == 0 {
            return None;
        }
        // Detour's BV-tree search quantizes/clamps to the tile bounds. Its
        // nearest result is not a promise that the requested vertical extent
        // was respected, especially above/below a flat tile. Enforce the actual
        // attachment bounds before accepting either end of a route.
        for (requested, nearest) in [(from, near_start), (goal, near_end)] {
            let delta = (requested - Vec3::from(nearest)).abs();
            if delta.x > extents[0] || delta.y > extents[1] || delta.z > extents[2] {
                return None;
            }
        }
        let mut corridor = vec![dt::DtPolyRef::default(); 4096];
        let count = self
            .query
            .find_path(start, end, &near_start, &near_end, &filter, &mut corridor)
            .ok()?;
        // Bindings return count rather than detail bits. End-reference and
        // capacity checks independently reject incomplete/truncated corridors.
        if count == 0 || count == corridor.len() || corridor[count - 1] != end {
            return None;
        }
        let mut points = vec![[0.; 3]; 4096];
        let length = self
            .query
            .find_straight_path(
                &near_start,
                &near_end,
                &corridor[..count],
                &mut points,
                None,
                None,
                // Keep height changes on stairs as intermediate steering points.
                dt::DT_STRAIGHTPATH_ALL_CROSSINGS,
            )
            .ok()?;
        if length == 0
            || length == points.len()
            || Vec3::from(points[length - 1]).distance(Vec3::from(near_end)) > 0.01
        {
            return None;
        }
        Some(points[..length].iter().copied().map(Vec3::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (WalkingMesh, WalkingProfile) {
        let mesh = WalkingMesh {
            vertices: vec![
                [0, 0, 0],
                [4, 0, 0],
                [10, 0, 0],
                [10, 0, 4],
                [4, 0, 4],
                [4, 0, 10],
                [0, 0, 10],
                [0, 0, 4],
            ],
            polygons: vec![vec![0, 7, 4, 1], vec![1, 4, 3, 2], vec![7, 6, 5, 4]],
            neighbours: vec![
                vec![None, Some(2), Some(1), None],
                vec![Some(0), None, None, None],
                vec![None, None, None, Some(0)],
            ],
            min: [0.; 3],
            max: [10., 0., 10.],
            cell_size: 1.,
            cell_height: 1.,
        };
        let profile = WalkingProfile {
            round: false,
            wall_step: None,
            width: 1.,
            height: 2.,
            crouch_height: 1.,
            step: 1.,
            crawl_step: 1.,
            drop: 1.,
            floor_cos: 0.7,
        };
        (mesh, profile)
    }
    #[test]
    fn nearest_boundary_is_safe_and_routes_follow_connected_polygons() {
        let (data, profile) = fixture();
        let mut mesh = Mesh::new(&data, &profile).unwrap();
        // Starting just outside the mesh exercises boundary projection. Missing
        // detail-edge flags previously dereferenced null in Detour here.
        let from = Vec3::new(10.2, 0., 2.);
        let goal = Vec3::new(2., 0., 8.);
        let route = mesh.route(from, goal, 1.).expect("connected L corridor");
        assert!((route.front().unwrap().x - 10.).abs() < 0.01);
        assert!(
            route
                .iter()
                .any(|p| p.distance(Vec3::new(4., 0., 4.)) < 0.01),
            "must turn at the L corridor corner: {route:?}"
        );
        assert!(route.back().unwrap().distance(goal) < 0.01);
        assert!(
            mesh.route(from, goal + Vec3::Y * 10., 1.).is_none(),
            "cannot attach through another floor"
        );
        let mut disconnected = data;
        disconnected.neighbours[0][2] = None;
        disconnected.neighbours[1][0] = None;
        assert!(
            Mesh::new(&disconnected, &profile)
                .unwrap()
                .route(from, goal, 1.)
                .is_none(),
            "a partial corridor is not reaching the destination"
        );
    }
    #[test]
    fn invalid_portable_topology_is_rejected_before_native_construction() {
        let (mut data, profile) = fixture();
        data.polygons[1].clear();
        assert!(Mesh::new(&data, &profile).is_err());
        let (mut data, _) = fixture();
        data.neighbours[0][1] = Some(u16::MAX);
        assert!(Mesh::new(&data, &profile).is_err());
    }
}
