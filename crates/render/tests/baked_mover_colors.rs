//! A lit moving body whose material holds the room's light in its vertex colours (a map's mover: `bri_actor_lit` with
//! `bri_vertex_light`) is built with those colours; without them it would take the object's white and draw full-bright.
use bri_content::animation;
use bri_content::testing::{material, plain, rigid_shape};
use bri_render::scene::{Material, MaterialKind, SceneData, SceneImage};
use bri_render::shape_scene::ShapeInstance;
use glam::{Mat4, Vec3};

fn door_vertices(baked: bool) -> Vec<[f32; 4]> {
    let mut shape = rigid_shape(
        "door",
        &[("root", None, [0.0; 3])],
        &[(0, [0.0; 3], [1.0; 3], plain(0))],
        vec![material("door", "opaque")],
    );
    // A dark room: the bake left the vertex colours near 0 (0.5 is neutral).
    let mesh = shape.meshes[0].as_mut().unwrap();
    mesh.colors = vec![[10, 12, 14, 255]; mesh.positions.len()];
    let pose = animation::sample(&shape, None, 0.0).unwrap();
    let mut scene = SceneData {
        images: vec![SceneImage {
            label: "grey".into(),
            width: 1,
            height: 1,
            rgba: vec![128; 4],
            srgb: false,
        }],
        ..Default::default()
    };
    let mut lit = Material::vertex_lit("door", 0);
    lit.kind = MaterialKind::ActorLit;
    lit.parameters = Some([
        [0.0, 0.0, f32::from(u8::from(baked)), 0.0],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
    ]);
    scene.materials.push(lit);
    scene
        .append_shape(
            ShapeInstance {
                shape: &shape,
                pose: &pose,
                detail: 0,
                transform: Mat4::from_scale(Vec3::ONE),
                materials: &[0],
                translucent_materials: None,
                unassigned_material: 0,
            },
            |_| Some([1.0; 4]),
        )
        .unwrap();
    scene.vertices.iter().map(|v| v.color).collect()
}

#[test]
fn a_baked_mover_keeps_its_dark_vertex_colours() {
    let colors = door_vertices(true);
    assert!(!colors.is_empty());
    for c in colors {
        assert!(
            (c[0] - 10.0 / 255.0).abs() < 1e-3 && (c[2] - 14.0 / 255.0).abs() < 1e-3,
            "{c:?}"
        );
    }
}

#[test]
fn a_free_body_ignores_vertex_colours() {
    for c in door_vertices(false) {
        assert_eq!(c, [1.0; 4]);
    }
}
