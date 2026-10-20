use bri_emitters::projector_geometry::*;
use glam::{Mat4, Vec2, Vec3, Vec4};

const INSIDE: Vec4 = Vec4::new(0.0, 0.0, 1.0, -1000.0);

#[test]
fn a_bsp_record_reverses_the_polygon_and_maps_the_mask() {
    let poly = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(64.0, 0.0, 0.0),
        Vec3::new(64.0, 32.0, 0.0),
    ];
    let mapping = BspMapping {
        base: Vec3::ZERO,
        u_axis: Vec3::X,
        v_axis: Vec3::Y,
        mask_size: Some((64.0, 32.0)),
    };
    let rec = bsp_record(&poly, &mapping, None);
    assert_eq!(rec.len(), 3);
    assert_eq!(rec[0].position, poly[2]);
    assert_eq!(rec[2].position, poly[0]);
    assert_eq!(rec[0].uv, Vec2::new(1.0, 1.0));
    assert_eq!(rec[1].uv, Vec2::new(1.0, 0.0));
    assert!(rec.iter().all(|v| v.fade == 1.0));
    assert_eq!(fan_indices(4, 10), vec![10, 11, 12, 10, 12, 13]);
}

#[test]
fn the_bsp_clip_replaces_crossing_vertices_as_read() {
    // A square straddling x = 0; the plane keeps x >= 0. Reversed order is
    // (-1,1) (1,1) (1,-1) (-1,-1).
    let poly = [
        Vec3::new(-1.0, -1.0, 0.0),
        Vec3::new(1.0, -1.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(-1.0, 1.0, 0.0),
    ];
    let mapping = BspMapping {
        base: Vec3::ZERO,
        u_axis: Vec3::X,
        v_axis: Vec3::Y,
        mask_size: Some((2.0, 2.0)),
    };
    let planes = [
        Vec4::new(1.0, 0.0, 0.0, 0.0),
        INSIDE,
        INSIDE,
        INSIDE,
        INSIDE,
        INSIDE,
    ];
    let rec = bsp_record(&poly, &mapping, Some(&planes));
    let pos: Vec<Vec3> = rec.iter().map(|v| v.position).collect();
    // The corner (1, 1) is lost: the first inside vertex after the outside one is replaced
    // by the crossing instead of being kept, so the polygon comes out as a triangle.
    assert_eq!(
        pos,
        vec![
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
        ]
    );
}

fn grid(nx: usize, ny: usize) -> (Vec<Vec3>, Vec<Vec3>) {
    let mut p = Vec::new();
    for y in 0..ny {
        for x in 0..nx {
            p.push(Vec3::new(x as f32 * 10.0, y as f32 * 10.0, 0.0));
        }
    }
    let n = vec![Vec3::Z; nx * ny];
    (p, n)
}

#[test]
fn terrain_triangles_are_culled_by_outcodes_and_faded_by_gradient_and_normal() {
    let (positions, normals) = grid(3, 3);
    let all = |_: usize, _: usize| true;
    let diagonal = |_: usize, _: usize| false;
    let input = |planes: [Vec4; 6], gradient: bool, forward: Vec3| TerrainInput {
        positions: &positions,
        normals: &normals,
        stride: 3,
        origin: (0, 0),
        tiles: (2, 2),
        quad_visible: &all,
        diagonal: &diagonal,
        planes,
        projector_location: Vec3::new(0.0, 0.0, 100.0),
        forward,
        max_trace_distance: 200.0,
        project_on_backfaces: false,
        gradient,
    };
    let (v, idx) = terrain_receivers(&input([INSIDE; 6], false, Vec3::NEG_Z));
    assert_eq!(v.len(), 9);
    assert_eq!(idx.len(), 2 * 2 * 2 * 3);
    assert!(v.iter().all(|r| (r.fade - 1.0).abs() < 1e-6));
    // The projector points up at a floor facing up: no light. With a gradient and
    // direction down, g = 1 - 100 / 200 = 0.5 at z = 0.
    let (v, _) = terrain_receivers(&input([INSIDE; 6], false, Vec3::Z));
    assert!(v.iter().all(|r| r.fade == 0.0));
    let (v, _) = terrain_receivers(&input([INSIDE; 6], true, Vec3::NEG_Z));
    assert!(v.iter().all(|r| (r.fade - 0.5).abs() < 1e-6));
    // Everything outside the first plane: every triangle shares that bit and is dropped.
    let mut outside = [INSIDE; 6];
    outside[0] = Vec4::new(0.0, 0.0, 1.0, 1000.0);
    let (_, idx) = terrain_receivers(&input(outside, false, Vec3::NEG_Z));
    assert!(idx.is_empty());
}

#[test]
fn static_mesh_triangles_in_the_frustum_are_kept_and_faded_by_facing() {
    // A projector at the origin looking along +X; its matrix gives
    // (0.5 y + 0.5 x, 0.5 z + 0.5 x, x), the shape of the perspective form.
    let projector = Mat4::from_cols_array(&[
        0.5, 0.5, 1.0, 0.0, //
        0.5, 0.0, 0.0, 0.0, //
        0.0, 0.5, 0.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]);
    let positions = [
        Vec3::new(10.0, 0.0, 0.0),
        Vec3::new(10.0, 4.0, 0.0),
        Vec3::new(10.0, 0.0, 4.0),
        Vec3::new(-10.0, 0.0, 0.0),
    ];
    let normals = [Vec3::NEG_X, Vec3::NEG_X, Vec3::X, Vec3::NEG_X];
    let uvs = [Vec2::ZERO; 4];
    let triangles = [[0, 1, 2], [3, 3, 3]];
    let input = StaticMeshInput {
        positions: &positions,
        normals: &normals,
        uvs: &uvs,
        triangles: &triangles,
        to_world: Mat4::IDENTITY,
        projector,
        near: 0.0,
        far: 100.0,
        forward: Vec3::X,
        project_on_backfaces: false,
    };
    let (v, idx) = static_mesh_receivers(&input);
    // The first triangle is inside; the second sits wholly behind the projector (z < near)
    // and is rejected.
    assert_eq!(idx, vec![0, 1, 2]);
    // Facing: normals toward the projector (-X against forward +X) take full alpha,
    // the +X normal none.
    assert_eq!(v[0].fade, 1.0);
    assert_eq!(v[2].fade, 0.0);
}
