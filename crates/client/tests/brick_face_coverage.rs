//! Coverage uses authored occupied face cells, not a decorative volume's AABB.
use bri_client::brick_cover::Covers;
use bri_content::brick::{Brick as Mesh, Coverage, Face, Surface};
use bri_net::protocol::PublicWorld;
use bri_sim::grid::{Bounds, Index};
use bri_world::{Brick, ContentRef};
use std::collections::{BTreeMap, BTreeSet};

fn block(id: &str, footprint: [u32; 2], height: u32) -> Mesh {
    let mut m =
        bri_content::testing::bricks::block(id, footprint, height, |_| (Surface::Side, None));
    m.coverage = Some(
        [
            footprint[0] * footprint[1],
            footprint[0] * footprint[1],
            footprint[0] * height,
            footprint[1] * height,
            footprint[0] * height,
            footprint[1] * height,
        ]
        .map(|area| Coverage {
            hides_adjacent: true,
            required_area: area as f32,
        }),
    );
    m
}
fn branch() -> Mesh {
    let mut m = block("fixture/renamed-vault", [6, 6], 22);
    m.coverage = Some(std::array::from_fn(|face| Coverage {
        hides_adjacent: face == 1,
        required_area: if face == 1 { 4. } else { 999. },
    }));
    m.attachment_rows = vec!["------".into(); 6 * 22];
    for z in [2, 3] {
        m.attachment_rows[z * 22 + 21] = "--dd--".into();
    }
    for quad in &mut m.quads {
        if quad.face == Face::Bottom {
            for v in &mut quad.vertices {
                v.position[0] /= 3.;
                v.position[2] /= 3.;
            }
        }
    }
    m
}
fn at(id: &str, position: [f32; 3]) -> Brick {
    Brick::new(ContentRef::Resolved(id.into()), position, 1)
}
fn hidden(meshes: &BTreeMap<String, Mesh>, bricks: Vec<Brick>, id: u64) -> u8 {
    let world = PublicWorld {
        name: "Unfamiliar structure".into(),
        map_id: "fixture/map".into(),
        palette: vec![[1.; 4]],
        bricks: bricks
            .into_iter()
            .enumerate()
            .map(|(i, b)| (i as u64 + 1, b))
            .collect(),
    };
    let mut index = Index::default();
    for (id, b) in &world.bricks {
        let ContentRef::Resolved(def) = &b.definition else {
            unreachable!()
        };
        index.insert(*id, Bounds::new(b, &meshes[def]).unwrap());
    }
    let empty = BTreeSet::new();
    let covers = Covers {
        index: &index,
        world: &world,
        meshes,
        left_out: &empty,
        invalid: &empty,
        face_proofs: Default::default(),
    };
    let brick = &world.bricks[&id];
    let ContentRef::Resolved(def) = &brick.definition else {
        unreachable!()
    };
    covers.hidden(id, brick, &meshes[def])
}
#[test]
fn decorative_bounds_cannot_hide_the_supporting_face_or_a_plate_outside_the_stem() {
    let meshes = [
        block("fixture/support", [4, 4], 1),
        block("fixture/small", [1, 1], 1),
        branch(),
    ]
    .into_iter()
    .map(|m| (m.id.clone(), m))
    .collect();
    for turn in 0..4 {
        let mut upper = at("fixture/renamed-vault", [0., 2.4, 0.]);
        upper.quarter_turns = turn;
        assert_eq!(
            hidden(
                &meshes,
                vec![at("fixture/support", [0., 0.1, 0.]), upper.clone()],
                1
            ) & 1,
            0,
            "only central2x2 studs occlude the4x4 support, turn{turn}"
        );
        assert_eq!(
            hidden(
                &meshes,
                vec![at("fixture/small", [1.25, 0.1, 0.25]), upper],
                1
            ) & 1,
            0,
            "a plate under decorative bounds but outside the stem remains visible"
        );
    }
}
#[test]
fn overlapping_neighbours_cannot_double_count_the_same_covered_cells() {
    let meshes = [
        block("fixture/support", [4, 4], 1),
        block("fixture/solid", [2, 2], 1),
    ]
    .into_iter()
    .map(|m| (m.id.clone(), m))
    .collect();
    let mut bricks = vec![at("fixture/support", [0., 0.1, 0.])];
    // Coverage must stay a set even for coincident replicated geometry.
    bricks.extend((0..4).map(|_| at("fixture/solid", [0., 0.3, 0.])));
    assert_eq!(hidden(&meshes, bricks, 1) & 1, 0);
}
#[test]
fn real_opaque_faces_still_cover_but_literal_alpha_and_off_plane_tags_do_not() {
    let mut top = block("fixture/upper", [2, 2], 1);
    let support = block("fixture/support", [2, 2], 1);
    let pair = vec![
        at("fixture/support", [0., 0.1, 0.]),
        at("fixture/upper", [0., 0.3, 0.]),
    ];
    let maps = |m: Mesh| {
        [support.clone(), m]
            .into_iter()
            .map(|m| (m.id.clone(), m))
            .collect()
    };
    assert_ne!(
        hidden(&maps(top.clone()), pair.clone(), 1) & 1,
        0,
        "ordinary full opaque neighbour still culls"
    );
    for q in &mut top.quads {
        if q.face == Face::Bottom {
            q.colors = Some([[1., 1., 1., 0.3]; 4]);
        }
    }
    assert_eq!(
        hidden(&maps(top.clone()), pair.clone(), 1) & 1,
        0,
        "authored alpha selects blended geometry despite opaque paint"
    );
    for q in &mut top.quads {
        if q.face == Face::Bottom {
            q.colors = None;
            for v in &mut q.vertices {
                v.position[1] += 0.2;
            }
        }
    }
    assert_eq!(
        hidden(&maps(top), pair, 1) & 1,
        0,
        "a tagged face away from the touching plane is not coverage"
    );
}
#[test]
#[ignore = "requires generated native content; CPU geometry only"]
fn native_pine_tree_and_renamed_geometry_do_not_erase_larger_support_faces() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("content");
    let defs = bri_sim::definitions::Definitions::load(
        &bri_package::testing::pack_dir(&root, "brick_catalog"),
        &bri_package::testing::pack_dir(&root, "geometry"),
    )
    .unwrap();
    let mut tree = defs.entries["v20/brick/brickpinetreedata"].mesh.clone();
    assert_eq!(tree.footprint_studs, [6, 6]);
    assert_eq!(tree.coverage.unwrap()[1].required_area, 4.);
    for identity in [
        "v20/brick/brickpinetreedata",
        "fixture/renamed-opaque-column",
    ] {
        tree.id = identity.into();
        let mut plate = defs.entries["v20/brick/brick4x4fdata"].mesh.clone();
        plate.id = "fixture/support".into();
        let meshes = [(identity.into(), tree.clone()), (plate.id.clone(), plate)].into();
        for turn in 0..4 {
            let mut upper = at(identity, [0., 2.4, 0.]);
            upper.quarter_turns = turn;
            assert_eq!(
                hidden(
                    &meshes,
                    vec![at("fixture/support", [0., 0.1, 0.]), upper],
                    1
                ) & 1,
                0,
                "native tagged underside must not cover unrelated face cells"
            );
        }
        let mut base = defs.entries["v20/brick/brick32x32fdata"].mesh.clone();
        base.id = "fixture/base".into();
        let meshes = [(identity.into(), tree.clone()), (base.id.clone(), base)].into();
        let mut bricks = vec![at("fixture/base", [0., 0.1, 0.])];
        for x in [-7.5, -4.5, -1.5, 1.5, 4.5, 7.5] {
            for z in [-7.5, -4.5, -1.5, 1.5, 4.5, 7.5] {
                bricks.push(at(identity, [x, 2.4, z]));
            }
        }
        assert_eq!(
            hidden(&meshes, bricks, 1) & 1,
            0,
            "canopy bounds must not collectively erase an exposed native32x32 base"
        );
    }
}

#[test]
fn complex_unfamiliar_meshes_retain_faces_without_unbounded_geometry_proofs() {
    let support = block("fixture/support", [2, 2], 1);
    let mut detailed = block("fixture/detailed", [2, 2], 1);
    detailed.quads.resize(100_000, detailed.quads[0].clone());
    let meshes = [support, detailed]
        .into_iter()
        .map(|m| (m.id.clone(), m))
        .collect();
    assert_eq!(
        hidden(
            &meshes,
            vec![
                at("fixture/support", [0., 0.1, 0.]),
                at("fixture/detailed", [0., 0.3, 0.])
            ],
            1
        ) & 1,
        0,
        "complex geometry uses conservative retained-face fallback"
    );
}

#[test]
fn a_sparse_target_face_is_not_hidden_by_a_neighbour_outside_its_actual_region() {
    let meshes = [branch(), block("fixture/solid", [2, 2], 1)]
        .into_iter()
        .map(|m| (m.id.clone(), m))
        .collect();
    for turn in 0..4 {
        let mut target = at("fixture/renamed-vault", [0., 2.4, 0.]);
        target.quarter_turns = turn;
        assert_eq!(
            hidden(
                &meshes,
                vec![target.clone(), at("fixture/solid", [1., 0.1, 1.])],
                1
            ) & 2,
            0
        );
        assert_ne!(
            hidden(&meshes, vec![target, at("fixture/solid", [0., 0.1, 0.])], 1) & 2,
            0,
            "the actual opaque central support still hides the sparse target underside"
        );
    }
}

// A faithful geometric shape of the native four trapezoid rims plus loop.
// Its outward skin, degenerate triangles and surface names are mechanisms;
// unfamiliar identities must receive exactly the same proof.
fn quilt(id: &str, inner: f32, centre: bool) -> Mesh {
    let mut mesh = block(id, [2, 2], 1);
    let template = mesh
        .quads
        .iter()
        .find(|q| q.face == Face::Bottom)
        .unwrap()
        .clone();
    mesh.quads.retain(|q| q.face != Face::Bottom);
    let outer = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
    let inside = [
        [-inner, -inner],
        [inner, -inner],
        [inner, inner],
        [-inner, inner],
    ];
    let quad = |points: [[f32; 2]; 4]| {
        let mut q = template.clone();
        for (vertex, point) in q.vertices.iter_mut().zip(points) {
            vertex.position = [point[0], -0.1, point[1]];
        }
        q
    };
    for i in 0..4 {
        let next = (i + 1) % 4;
        mesh.quads
            .push(quad([outer[i], outer[next], inside[next], inside[i]]));
    }
    if centre {
        mesh.quads.push(quad(inside));
    }
    let half = mesh.half_size();
    for quad in &mut mesh.quads {
        for vertex in &mut quad.vertices {
            for axis in 0..3 {
                if vertex.position[axis].abs() == half[axis] {
                    vertex.position[axis] += vertex.position[axis].signum() * 0.0012;
                }
            }
        }
    }
    mesh
}
#[test]
fn outward_grid_skin_and_complete_trapezoid_or_triangle_quilts_keep_native_culling() {
    for inner in [0., 0.25] {
        let upper = quilt("fixture/unknown-quilt", inner, inner != 0.);
        let lower = block("fixture/support", [2, 2], 1);
        let meshes = [upper, lower]
            .into_iter()
            .map(|m| (m.id.clone(), m))
            .collect();
        for turn in 0..4 {
            let mut top = at("fixture/unknown-quilt", [0., 0.3, 0.]);
            top.quarter_turns = turn;
            let pair = vec![at("fixture/support", [0., 0.1, 0.]), top];
            assert_ne!(
                hidden(&meshes, pair.clone(), 1) & 1,
                0,
                "real opaque quilt covers the lower face, including repeated triangle vertices"
            );
            assert_ne!(
                hidden(&meshes, pair, 2) & 2,
                0,
                "complete native-like underside is hidden by its support"
            );
            let mut right = at("fixture/unknown-quilt", [1., 0.1, 0.]);
            right.quarter_turns = turn;
            assert_eq!(
                (hidden(
                    &meshes,
                    vec![at("fixture/support", [0., 0.1, 0.]), right],
                    1
                ) & 0b111100)
                    .count_ones(),
                1,
                "outward skinned native side still covers its logical touching plane"
            );
        }
    }
}
#[test]
fn a_real_hole_in_a_trapezoid_quilt_cannot_cover_the_missing_region() {
    let meshes = [
        block("fixture/support", [2, 2], 1),
        quilt("fixture/gapped-quilt", 0.25, false),
    ]
    .into_iter()
    .map(|m| (m.id.clone(), m))
    .collect();
    assert_eq!(
        hidden(
            &meshes,
            vec![
                at("fixture/support", [0., 0.1, 0.]),
                at("fixture/gapped-quilt", [0., 0.3, 0.])
            ],
            1
        ) & 1,
        0
    );
}
#[test]
#[ignore = "requires generated native content; CPU geometry only"]
fn native_standard_and_ramp_bottom_quilts_cull_with_renamed_geometry() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("content");
    let definitions = bri_sim::definitions::Definitions::load(
        &bri_package::testing::pack_dir(&root, "brick_catalog"),
        &bri_package::testing::pack_dir(&root, "geometry"),
    )
    .unwrap();
    for (upper_id, support_id, xz) in [
        ("v20/brick/brick1x1data", "v20/brick/brick1x1fdata", 0.25),
        ("v20/brick/brick2x2data", "v20/brick/brick2x2fdata", 0.),
        ("v20/brick/brick2x2rampdata", "v20/brick/brick2x2fdata", 0.),
        (
            "v20/brick/brick2x2rampupdata",
            "v20/brick/brick2x2fdata",
            0.,
        ),
    ] {
        let mut upper = definitions.entries[upper_id].mesh.clone();
        let mut support = definitions.entries[support_id].mesh.clone();
        upper.id = "fixture/renamed-native-quilt".into();
        support.id = "fixture/renamed-support".into();
        let y = 0.2 + upper.half_size().y;
        let meshes = [upper, support]
            .into_iter()
            .map(|m| (m.id.clone(), m))
            .collect();
        for turn in 0..4 {
            let mut top = at("fixture/renamed-native-quilt", [xz, y, xz]);
            top.quarter_turns = turn;
            assert_ne!(
                hidden(
                    &meshes,
                    vec![at("fixture/renamed-support", [xz, 0.1, xz]), top],
                    2
                ) & 2,
                0,
                "actual native standard/ramp underside remains cullable: {upper_id}, turn{turn}"
            );
        }
    }
}

#[test]
fn outward_skin_never_moves_projected_vertices_into_an_authored_sliver_hole() {
    let mut top = block("fixture/skewed-quilt", [2, 2], 1);
    let template = top
        .quads
        .iter()
        .find(|q| q.face == Face::Bottom)
        .unwrap()
        .clone();
    top.quads.retain(|q| q.face != Face::Bottom);
    for points in [
        [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.45], [-0.5, 0.45]],
        [[-0.5, 0.45], [0.49, 0.45], [0.5, 0.5], [-0.5, 0.5]],
        [[0.495, 0.45], [0.5, 0.45], [0.5, 0.5], [0.5, 0.5]],
        [[0.5012, 0.5012], [0.49, 0.45], [0.495, 0.45], [0.495, 0.45]],
    ] {
        let mut quad = template.clone();
        for (vertex, point) in quad.vertices.iter_mut().zip(points) {
            vertex.position = [point[0], -0.1, point[1]];
        }
        top.quads.push(quad);
    }
    let meshes = [block("fixture/support", [2, 2], 1), top]
        .into_iter()
        .map(|m| (m.id.clone(), m))
        .collect();
    assert_eq!(
        hidden(
            &meshes,
            vec![
                at("fixture/support", [0., 0.1, 0.]),
                at("fixture/skewed-quilt", [0., 0.3, 0.])
            ],
            1
        ) & 1,
        0,
        "clamping the skew apex would invent coverage of an actual missing sliver"
    );
}
