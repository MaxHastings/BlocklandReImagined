//! Legacy DTS object-visibility fades ride in each clip's `extras.bri_clip`,
//! and the shipped GLBs must carry them or the client leaves those objects at
//! full opacity for their whole life: the rocket's expanding sphere lingers as
//! a solid dome instead of fading out to its particle cloud, the bow's arrow
//! stays visible while it draws, and the spear never appears at the end of its
//! throw.

use bri_content::shape::{Animation, Shape};

fn lower(rel: &str) -> Option<Shape> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content")
        .join(rel);
    if !path.is_file() {
        eprintln!("skipping: no {} at {}", rel, path.display());
        return None;
    }
    let bytes = std::fs::read(&path).unwrap();
    Some(bri_gltf::lower(&bytes, "test.object-visibility").unwrap().shape)
}

fn clip<'a>(shape: &'a Shape, name: &str) -> Option<&'a Animation> {
    shape.animations.iter().find(|a| a.name == name)
}

/// A glTF loader's unique name back to the node it was named after: a
/// trailing `_N` is dropped.
fn base(name: &str) -> String {
    match name.rsplit_once('_') {
        Some((b, s)) if !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()) => b.to_string(),
        _ => name.to_string(),
    }
}

#[test]
fn the_rocket_explosion_sphere_fades_out() {
    let Some(shape) = lower("default-pack/weapon_rocket_launcher/assets/models/explosionSphere1.glb")
    else {
        return;
    };
    shape.validate().unwrap();
    let ambient = clip(&shape, "ambient").expect("the sphere keeps its ambient clip");
    assert_eq!(ambient.frames, 11);
    assert!(!ambient.objects.is_empty(), "the ambient clip carries its object track");
    let track = &ambient.objects[0];
    // v20's authored fade: full for two frames, then down to nothing.
    assert_eq!(
        track.visibility,
        vec![1.0f32, 1.0, 0.75, 0.5, 0.25, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
    );
}

#[test]
fn the_debris_rocket_sphere_keeps_its_fade() {
    let Some(shape) = lower("default-pack/weapon_debris_base/models/v20.weapon_debris.rocket_explosion_sphere.glb")
    else {
        return;
    };
    shape.validate().unwrap();
    let ambient = clip(&shape, "ambient").expect("the debris sphere keeps its ambient clip");
    assert_eq!(ambient.frames, 11);
    let track = &ambient.objects[0];
    assert_eq!(track.visibility.last(), Some(&0.0), "the fade ends invisible");
}

#[test]
fn the_bow_hides_its_arrow_while_it_draws() {
    let Some(shape) = lower("default-pack/weapon_bow/assets/models/bow.glb") else { return };
    shape.validate().unwrap();
    // The arrow is visible for two frames, gone for the draw.
    let fire = clip(&shape, "fire").expect("the bow keeps its fire clip");
    assert_eq!(fire.frames, 5);
    let track = &fire.objects[0];
    assert_eq!(track.visibility, vec![1.0f32, 1.0, 0.0, 0.0, 0.0]);
    assert!(
        base(&shape.objects[track.object].name).starts_with("arrow"),
        "the hidden object is the arrow (got {:?})",
        shape.objects[track.object].name
    );
    // The reload starts empty and the arrow returns at its end.
    let reload = clip(&shape, "reload").expect("the bow keeps its reload clip");
    assert_eq!(reload.frames, 7);
    assert_eq!(
        &reload.objects[0].visibility[..],
        &[0.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0]
    );
}

#[test]
fn the_spear_appears_at_the_end_of_its_throw() {
    let Some(shape) = lower("default-pack/weapon_spear/assets/models/spear.glb") else { return };
    shape.validate().unwrap();
    // The throw clip moves nothing; it only reveals the spear's objects.
    let fire = clip(&shape, "fire").expect("the spear keeps its throw clip");
    assert_eq!(fire.frames, 7);
    assert_eq!(fire.objects.len(), 2, "both drawn objects ride the reveal");
    for track in &fire.objects {
        assert_eq!(
            track.visibility,
            vec![0.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0],
            "object {} reveals at the clip's end",
            shape.objects[track.object].name
        );
    }
}
