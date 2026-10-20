//! Projectors at run time: attach, move and detach become decal edits.
use bri_effects_render::projector_runtime::{
    Placement, ProjectorReceivers, ProjectorRuntime, ProjectorSpec,
};
use bri_render::decals::DecalEdit;

fn spec(width: f32) -> ProjectorSpec {
    ProjectorSpec {
        picture: "shadow/birch".into(),
        fov_degrees: 10.0,
        reach: 8.0,
        width,
        receivers: ProjectorReceivers {
            terrain: false,
            ..ProjectorReceivers::default()
        },
        ..ProjectorSpec::default()
    }
}

fn placement(x: f32, roll: f32) -> Placement {
    Placement {
        position: [x, 5.0, 0.0],
        direction: [0.0, -1.0, 0.0],
        roll_degrees: roll,
    }
}

#[test]
fn attach_waits_for_its_picture_then_moves_and_detaches() {
    let mut runtime = ProjectorRuntime::new();
    runtime.attach(7, spec(2.0), placement(1.0, 0.0));
    assert!(runtime.drain().is_empty(), "no picture yet");
    runtime.register_picture("shadow/birch", (128, 128), "birch");
    let edits = runtime.drain();
    let [DecalEdit::Add { key: 7, decal }] = edits.as_slice() else {
        panic!("{edits:?}");
    };
    assert_eq!(decal.picture, "birch");
    assert!(decal.perspective);
    assert!(decal.surfaces.enabled && !decal.ground.enabled);
    let [lo, hi] = decal.bounds.unwrap();
    assert!(lo.iter().zip(hi).all(|(l, h)| *l < h));
    // It throws along -y: the volume reaches below it.
    assert!(lo[1] < 5.0 - 7.0 && hi[1] <= 5.0 + 0.1, "{lo:?} {hi:?}");
    // Moving changes the mapping, not the receivers.
    runtime.set_placement(7, placement(1.5, 0.0));
    let edits = runtime.drain();
    assert!(matches!(edits.as_slice(), [DecalEdit::Move { key: 7, .. }]));
    runtime.set_placement(7, placement(1.5, 0.0));
    assert!(runtime.drain().is_empty(), "same placement, no edit");
    runtime.detach(7);
    assert_eq!(runtime.drain(), vec![DecalEdit::Remove { key: 7 }]);
    runtime.detach(7);
    assert!(runtime.drain().is_empty());
}

#[test]
fn a_known_picture_attaches_at_once_and_a_projector_without_one_is_noted() {
    let mut runtime = ProjectorRuntime::new();
    runtime.register_picture("shadow/birch", (128, 128), "birch");
    runtime.attach(1, spec(2.0), placement(0.0, 0.0));
    assert!(matches!(
        runtime.drain().as_slice(),
        [DecalEdit::Add { key: 1, .. }]
    ));
    runtime.attach(2, ProjectorSpec::default(), placement(0.0, 0.0));
    assert!(runtime.notes.iter().any(|n| n.contains("no picture")));
    assert_eq!(runtime.attached(), 1);
}
