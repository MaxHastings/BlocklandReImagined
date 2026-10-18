use bri_fx_runtime::*;
use glam::{Mat4, Vec3};
use std::sync::Arc;
fn affinity(fov: u32) -> Arc<LocalViewAffinity> {
    Arc::new(LocalViewAffinity {
        owner: 1,
        slot: 0,
        image: "probe:image/gun".into(),
        policy: [fov, 1, 2, 3, 0],
    })
}
fn camera() -> Camera {
    Camera {
        view_projection: Mat4::IDENTITY,
        position: Vec3::Z,
        right: Vec3::X,
        up: Vec3::Y,
    }
}
#[test]
fn detached_particles_keep_affinity_after_source_expiry_and_never_leak_to_world()
-> anyhow::Result<()> {
    let pack = testing::pack(|l| {
        l.emitters[0].period = 0.01;
        l.emitters[0].lifetime = 0.03;
        l.emitters[0].speed = 0.;
        l.particles[0].lifetime = 1.;
    });
    let mut world = EffectsWorld::new(pack, EffectsLimits::default(), 1)?;
    let original = affinity(11);
    let options = SourceOptions {
        local_view: Some(original.clone()),
        ..Default::default()
    };
    let handle = world.start_emitter("emitter", SourceTransform::default(), options.clone())?;
    let light = world.start_light("light", SourceTransform::default(), options.clone())?;
    let mut changed = options;
    changed.local_view = Some(affinity(12));
    assert!(
        world.update_options(handle, changed).is_err(),
        "affinity immutable while source exists"
    );
    world.advance(0.1, Vec3::ZERO)?;
    assert!(
        !world.attachment_alive(handle),
        "detached particles outlive both source and Timed membership"
    );
    assert!(world.particle_count() > 0);
    let main = world.snapshot_in_view(&camera());
    assert!(
        main.particles.is_empty(),
        "neither detached local sprites nor local flare may leak into world"
    );
    assert_eq!(
        main.lights.iter().filter(|l| l.handle == light).count(),
        1,
        "point lights remain world exactly once"
    );
    let local = world.snapshot_in_local_view(&camera(), &original);
    assert!(!local.particles.is_empty());
    assert!(local.lights.is_empty());
    assert!(world.snapshot_in_other_view(&camera()).particles.is_empty());
    assert!(
        world
            .snapshot_in_local_view(&camera(), &affinity(12))
            .particles
            .is_empty(),
        "new policy cannot borrow lingering particles"
    );
    assert!(world.has_mismatched_local_policy(&[affinity(12)]));
    assert!(
        !world.has_mismatched_local_policy(&[]),
        "holster suppresses without inventing mismatched binding"
    );
    let before = world.particle_count();
    world.advance(0.1, Vec3::ZERO)?;
    assert_eq!(
        world.particle_count(),
        before,
        "hidden interval does not restart effect lifetime"
    );
    assert!(
        !world
            .snapshot_in_local_view(&camera(), &original)
            .particles
            .is_empty(),
        "re-equip same binding can see still-live particles"
    );
    world.teardown();
    assert_eq!(world.particle_count(), 0);
    assert!(
        world
            .snapshot_in_local_view(&camera(), &original)
            .particles
            .is_empty()
    );
    Ok(())
}
