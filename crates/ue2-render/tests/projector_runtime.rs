//! Projectors at run time: attach, move and detach become decal edits.
use bri_render::decals::DecalEdit;
use bri_ue2_render::projector_runtime::{Pose, ProjectorRuntime, ProjectorSettings};

fn settings(draw_scale: f32) -> ProjectorSettings {
    ProjectorSettings {
        texture: "T-FX.Shadow.fx_shadow_birch".into(),
        fov: 10,
        max_trace_distance: 200,
        draw_scale,
        project_bsp: false,
        ..ProjectorSettings::default()
    }
}

fn pose(x: f32, roll: i32) -> Pose {
    Pose {
        location: [x, 0.0, 0.0],
        rotation: [-9740, -25204, roll],
    }
}

#[test]
fn attach_waits_for_its_picture_then_moves_and_detaches() {
    let mut runtime = ProjectorRuntime::new(0.04);
    runtime.attach(7, settings(1.0), pose(100.0, 0));
    assert!(runtime.drain().is_empty(), "no picture yet");
    runtime.register_picture("T-FX.Shadow.fx_shadow_birch", (128, 128), "birch");
    let edits = runtime.drain();
    let [DecalEdit::Add { key: 7, decal }] = edits.as_slice() else {
        panic!("{edits:?}");
    };
    assert_eq!(decal.picture, "birch");
    assert!(decal.perspective);
    // BSP off but static meshes on: the map's walls and meshes are one class, so it is taken (and noted).
    assert!(decal.surfaces.enabled && decal.ground.enabled);
    assert!(runtime.notes.iter().any(|n| n.contains("differently")));
    let [lo, hi] = decal.bounds.unwrap();
    assert!(lo.iter().zip(hi).all(|(l, h)| *l < h));
    // Moving changes the mapping, not the receivers.
    runtime.set_pose(7, pose(110.0, 0));
    let edits = runtime.drain();
    assert!(matches!(edits.as_slice(), [DecalEdit::Move { key: 7, .. }]));
    runtime.set_pose(7, pose(110.0, 0));
    assert!(runtime.drain().is_empty(), "same pose, no edit");
    runtime.detach(7);
    assert_eq!(runtime.drain(), vec![DecalEdit::Remove { key: 7 }]);
    runtime.detach(7);
    assert!(runtime.drain().is_empty());
}

#[test]
fn a_known_picture_attaches_at_once_and_a_projector_without_one_is_noted() {
    let mut runtime = ProjectorRuntime::new(0.04);
    runtime.register_picture("T-FX.Shadow.fx_shadow_birch", (128, 128), "birch");
    runtime.attach(1, settings(1.0), pose(0.0, 0));
    assert!(matches!(
        runtime.drain().as_slice(),
        [DecalEdit::Add { key: 1, .. }]
    ));
    runtime.attach(2, ProjectorSettings::default(), pose(0.0, 0));
    assert!(runtime.notes.iter().any(|n| n.contains("no picture")));
    assert_eq!(runtime.attached(), 1);
}
