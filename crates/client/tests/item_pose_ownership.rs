//! Imported image skins and RGBA must retain authored behavior.
use anyhow::Result;
use bri_client::items::{moves_visible_detail, native_shape_scene};
use bri_content::{
    animation::sample,
    shape::{Animation, Influence, Skin},
    testing::{material, plain, rigid_shape},
};
use bri_render::scene::{AlphaMode, MaterialKind, SceneImage};
use glam::{Mat4, Quat};
fn shape() -> bri_content::shape::Shape {
    rigid_shape(
        "fixture",
        &[("root", None, [0.; 3]), ("bone", Some(0), [0.; 3])],
        &[(0, [0.; 3], [1.; 3], plain(0))],
        vec![material("texture", "alpha")],
    )
}
#[test]
fn an_image_skin_uses_animation_below_its_object_root() -> Result<()> {
    let mut shape = shape();
    let clip: Animation = serde_json::from_value(
        serde_json::json!({"name":"reload","frames":2,"duration":1.,"looping":false,"additive":false,"priority":1,"nodes":[{"node":"bone","rotations":[[0.,0.,0.,1.],Quat::from_rotation_x(0.8).to_array()],"translations":[],"scales":[],"scale_rotations":[]}],"objects":[],"ground_translations":[],"ground_rotations":[],"triggers":[]}),
    )?;
    assert!(
        !moves_visible_detail(&shape, &clip, true),
        "rigid object root is still"
    );
    let mesh = shape.meshes[0].as_mut().unwrap();
    mesh.skin = Some(Skin {
        nodes: vec![1],
        inverse_bind: vec![Mat4::IDENTITY.to_cols_array()],
        influences: (0..mesh.positions.len())
            .map(|vertex| Influence {
                vertex,
                bone: 0,
                weight: 1.,
            })
            .collect(),
    });
    assert!(
        moves_visible_detail(&shape, &clip, true),
        "vertices follow the animated skin bone"
    );
    Ok(())
}
#[test]
fn imported_rgba_is_not_a_legacy_node_paint_overlay() -> Result<()> {
    let mut shape = shape();
    let pose = sample(&shape, None, 0.)?;
    let image = SceneImage {
        label: "blue transparent art".into(),
        width: 1,
        height: 1,
        rgba: vec![0, 32, 100, 0],
        srgb: true,
    };
    let legacy = native_shape_scene(
        "legacy",
        &shape,
        &[&image],
        [1.; 4],
        true,
        Mat4::IDENTITY,
        &pose,
    )?;
    assert_eq!(legacy.materials[0].kind, MaterialKind::BrickOverlay);
    shape.materials[0].paint_overlay = Some(false);
    let imported = native_shape_scene(
        "imported",
        &shape,
        &[&image],
        [1.; 4],
        true,
        Mat4::IDENTITY,
        &pose,
    )?;
    assert_eq!(imported.materials[0].kind, MaterialKind::VertexLit);
    assert_eq!(imported.materials[0].alpha, AlphaMode::Blend);
    shape.materials[0].blend = "opaque".into();
    shape.materials[0].alpha_cutoff = Some(0.5);
    let cutout = native_shape_scene(
        "cutout",
        &shape,
        &[&image],
        [1.; 4],
        true,
        Mat4::IDENTITY,
        &pose,
    )?;
    assert_eq!(cutout.materials[0].alpha, AlphaMode::Mask(0.5));
    assert_ne!(cutout.materials[0].kind, MaterialKind::BrickOverlay);
    assert!(!cutout.materials[0].ignore_texture_alpha);
    Ok(())
}
