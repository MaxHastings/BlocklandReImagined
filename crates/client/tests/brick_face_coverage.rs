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
