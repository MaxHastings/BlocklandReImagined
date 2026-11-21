//! The rocket's fireball, end to end on the CPU: the base game's fx runtime
//! pack must load from `content/default-pack/fx_runtime`, and playing its
//! `v20/explosion/rocketexplosion` composite must start every group it
//! authors — the ring flash emitter, the dynamic light, and the cloud burst
//! that is the explosion's soft fireball. A missing reference used to abort
//! playback before the burst could emit, leaving only a small flash where
//! v20 shows a full fireball.

use bri_fx_runtime::{EffectsLimits, EffectsPack, EffectsWorld, SourceOptions, SourceTransform};
use glam::Vec3;

fn fx_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/default-pack/fx_runtime")
}

#[test]
fn rocket_explosion_composite_plays_its_fireball() {
    let pack = EffectsPack::load(fx_root()).expect("the base fx runtime pack loads");
    let mut world =
        EffectsWorld::new(pack.clone(), EffectsLimits::default(), 1).unwrap();

    let transform = SourceTransform::default();
    let plan = world
        .prepare_composite(
            "v20/explosion/rocketexplosion",
            transform,
            SourceOptions::default(),
            None,
        )
        .expect("the rocket composite prepares");
    let handles = world
        .play_prepared_composite(plan)
        .expect("the rocket composite plays without aborting");

    // Ring emitter + light + the cloud burst: every authored group started
    // a source (the burst's own tail is removed once it has emitted).
    assert_eq!(handles.len(), 3, "every authored group starts a source");
    let live = handles.iter().filter(|h| world.is_active(**h)).count();
    assert!(live >= 2, "ring emitter and light stay active (got {live})");

    // The burst emits its ten cloud sprites synchronously; the ring flash
    // adds more as it runs. After half a second of play there must be live
    // particles — the fireball itself.
    let at_play = world.particle_count();
    for _ in 0..30 {
        world.advance(1.0 / 60.0, Vec3::ZERO).unwrap();
    }
    let after = world.particle_count();
    eprintln!(
        "particles right after play: {at_play}; after 0.5 s: {after}; sources: {}",
        world.source_count()
    );
    assert!(
        at_play >= 10,
        "the cloud burst emits its authored count immediately (got {at_play})"
    );
    assert!(
        after > at_play / 2,
        "cloud particles survive into the second half of their life (got {after})"
    );

    let d = world.diagnostics();
    eprintln!("diagnostics: {d:?}");

    // The fireball itself, as the renderer would draw it this frame: sprites
    // near the blast at v20's authored sizes (10 to 15), on the cloud texture,
    // already fading toward their end colour. A world that emits nothing here
    // draws only the shape's dome — what the rocket looked like before its
    // fireball was restored.
    let camera = bri_fx_runtime::Camera {
        view_projection: glam::Mat4::IDENTITY,
        position: Vec3::ZERO,
        right: glam::Vec3::X,
        up: glam::Vec3::Y,
    };
    let frame = world.snapshot(&camera);
    let cloud = pack
        .library
        .textures
        .keys()
        .enumerate()
        .find(|(_, id)| *id == "base/data/particles/cloud")
        .map_or(0, |(i, _)| i as u32);
    let clouds: Vec<_> = frame.particles.iter().filter(|p| p.texture == cloud).collect();
    eprintln!(
        "frame: {} sprites ({} cloud), sizes {:?}..{:?}, alphas {:?}..{:?}",
        frame.particles.len(),
        clouds.len(),
        frame.particles.iter().map(|p| p.size).reduce(f32::min),
        frame.particles.iter().map(|p| p.size).reduce(f32::max),
        frame.particles.iter().map(|p| p.color.w).reduce(f32::min),
        frame.particles.iter().map(|p| p.color.w).reduce(f32::max),
    );
    assert!(!frame.particles.is_empty(), "the frame draws sprites");
    assert!(
        !clouds.is_empty(),
        "the cloud burst still has live sprites at half a second (got {})",
        clouds.len()
    );
    for p in &clouds {
        // Authored 10 to 15 over the life, before any per-particle scale.
        assert!(p.size >= 9.0 && p.size <= 16.0, "cloud size {}", p.size);
        // The authored alpha only ever falls from its start of 0.9.
        assert!(p.color.w <= 0.9 + 1e-3, "cloud alpha {}", p.color.w);
    }
}
