//! Which triangles a decal's volume draws on, and the picture turning about its centre.
use bri_content::decals::{Blend, Decal, Receivers};
use bri_render::decals::{gather, spun};
use bri_render::scene::{Material, MeshBatch, SceneData, SceneVertex};
use glam::{Mat4, Vec4};

fn vertex(position: [f32; 3], normal: [f32; 3]) -> SceneVertex {
    SceneVertex {
        position,
        normal,
        uv: [0.0; 2],
        lightmap_uv: [0.0; 2],
        color: [1.0; 4],
        fx: [0.0; 4],
    }
}

/// Two triangles: one on the floor (y = 0, normal up, near the origin) and one far away.
fn scene() -> SceneData {
    let up = [0.0, 1.0, 0.0];
    let mut scene = SceneData::default();
    for (x, z) in [(0.0, 0.0), (100.0, 100.0)] {
        scene.vertices.extend([
            vertex([x, 0.0, z], up),
            vertex([x, 0.0, z + 1.0], up),
            vertex([x + 1.0, 0.0, z], up),
        ]);
    }
    scene.indices = (0..6).collect();
    scene.materials.push(Material::vertex_lit("floor", 0));
    scene.batches.push(MeshBatch {
        zone: None,
        indices: 0..6,
        material: 0,
        center: [0.0; 3],
    });
    scene
}

/// A volume 10 units around the origin looking straight down (-y).
fn decal(limit: f32) -> Decal {
    Decal {
        picture: "p.png".into(),
        world_to_texture: Mat4::IDENTITY.to_cols_array(),
        perspective: false,
        planes: [
            [0.0, -1.0, 0.0, -5.0],
            [1.0, 0.0, 0.0, -10.0],
            [-1.0, 0.0, 0.0, -10.0],
            [0.0, 0.0, 1.0, -10.0],
            [0.0, 0.0, -1.0, -10.0],
            [0.0, 1.0, 0.0, -20.0],
        ],
        forward: [0.0, -1.0, 0.0],
        bounds: None,
        blend: Blend::Modulate2x,
        wrap: [false, false],
        two_sided: false,
        spin_degrees_per_second: 0.0,
        surfaces: Receivers {
            enabled: true,
            facing_limit: limit,
            fade_by_facing: false,
        },
        ground: Receivers {
            enabled: true,
            facing_limit: 2.0,
            fade_by_facing: true,
        },
    }
}

#[test]
fn only_triangles_in_the_volume_and_facing_it_receive() {
    let (vertices, indices) = gather(&scene(), &decal(-0.0001));
    assert_eq!(indices.len(), 3, "the far triangle is outside the volume");
    assert_eq!(vertices.len(), 3);
    assert!(vertices.iter().all(|v| v.fade == 1.0));
    // The floor faces up, the decal looks down: a limit that wants the surface facing away takes nothing.
    let none = gather(&scene(), &decal(-1.5)).1;
    assert!(none.is_empty());
}

#[test]
fn ground_fades_with_its_facing_and_a_disabled_class_takes_nothing() {
    let mut d = decal(-0.0001);
    d.surfaces.enabled = false;
    assert!(gather(&scene(), &d).1.is_empty());
    let mut ground = scene();
    ground.materials[0].kind = bri_render::scene::MaterialKind::Terrain;
    let (vertices, indices) = gather(&ground, &d);
    assert_eq!(indices.len(), 3);
    assert!(
        vertices.iter().all(|v| (v.fade - 1.0).abs() < 1e-6),
        "facing straight on"
    );
}

#[test]
fn a_quarter_turn_moves_the_picture_corner_about_its_centre() {
    // An orthographic matrix mapping (x, y) to (s, t) = (x, y).
    let m = Mat4::IDENTITY;
    let turned = spun(m, false, 90.0);
    let corner = turned * Vec4::new(1.0, 0.5, 0.0, 1.0);
    // (1, 0.5) is a half step right of the centre; a quarter turn puts it a half step along t.
    assert!(
        (corner.x - 0.5).abs() < 1e-5 && (corner.y - 1.0).abs() < 1e-5,
        "{corner}"
    );
    assert_eq!(spun(m, false, 0.0), m);
}

#[test]
fn the_index_finds_what_a_full_read_finds() {
    use bri_render::decals::{ReceiverIndex, gather_indexed};
    let scene = scene();
    let index = ReceiverIndex::build(&scene);
    let mut d = decal(-0.0001);
    d.bounds = Some([[-10.0, -1.0, -10.0], [10.0, 5.0, 10.0]]);
    let indexed = gather_indexed(&scene, &index, &d);
    assert_eq!(indexed.1.len(), gather(&scene, &d).1.len());
    // A box away from both triangles reads none.
    d.bounds = Some([[500.0, 0.0, 500.0], [510.0, 5.0, 510.0]]);
    assert!(gather_indexed(&scene, &index, &d).1.is_empty());
}

#[test]
fn glass_takes_a_decal_but_an_additive_surface_does_not() {
    use bri_render::scene::AlphaMode;
    let mut glass = scene();
    glass.materials[0].alpha = AlphaMode::Blend;
    assert_eq!(gather(&glass, &decal(-0.0001)).1.len(), 3);
    glass.materials[0].alpha = AlphaMode::Additive;
    assert!(gather(&glass, &decal(-0.0001)).1.is_empty());
}
