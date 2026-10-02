use bri_content::effects::*;
use bri_fx_runtime::{pack::digest, *};
use glam::{Mat4, Vec3};
use std::sync::Arc;
fn fixture(change: impl FnMut(&mut Library)) -> Arc<EffectsPack> {
    bri_fx_runtime::testing::pack(change)
}
/// A converted pack under `content/`.
fn content_pack(name: &str) -> Arc<EffectsPack> {
    EffectsPack::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content")
            .join(name),
    )
    .unwrap()
}
fn camera() -> Camera {
    Camera {
        view_projection: Mat4::IDENTITY,
        position: Vec3::Z * 10.,
        right: Vec3::X,
        up: Vec3::Y,
    }
}
fn world(pack: Arc<EffectsPack>) -> EffectsWorld {
    EffectsWorld::new(pack, EffectsLimits::default(), 73).unwrap()
}
#[test]
fn moving_attachment_emits_along_path_and_detaches_particles() {
    let mut w = world(fixture(|l| l.emitters[0].speed = 0.));
    let h = w
        .start_emitter(
            "emitter",
            SourceTransform::default(),
            SourceOptions::default(),
        )
        .unwrap();
    w.update_source(
        h,
        SourceTransform {
            position: Vec3::X * 10.,
            ..Default::default()
        },
    )
    .unwrap();
    w.advance(1., Vec3::ZERO).unwrap();
    let f = w.snapshot(&camera());
    assert!(f.particles.len() >= 9);
    assert!(
        f.particles
            .iter()
            .any(|p| (p.position.x - 1.).abs() < 0.001)
    );
    assert!(
        f.particles
            .iter()
            .any(|p| (p.position.x - 9.).abs() < 0.001)
    );
    w.stop(h, StopMode::Drain);
    w.advance(0.1, Vec3::ZERO).unwrap();
    assert!(w.particle_count() > 0);
    assert!(w.update_source(h, SourceTransform::default()).is_err());
    w.advance(3., Vec3::ZERO).unwrap();
    assert_eq!(w.particle_count(), 0);
}
#[test]
fn curve_spin_and_closed_form_motion_are_frame_partition_stable() {
    let pack = fixture(|l| {
        let p = &mut l.particles[0];
        p.drag = 1.;
        p.gravity = 0.5;
        p.wind = 0.2;
        p.inherited_velocity = 0.5;
    });
    let mut a = world(pack.clone());
    let mut b = world(pack);
    let t = SourceTransform {
        velocity: Vec3::X * 4.,
        ..Default::default()
    };
    a.burst("emitter", t, SourceOptions::default(), 1).unwrap();
    b.burst("emitter", t, SourceOptions::default(), 1).unwrap();
    a.advance(0.5, Vec3::Z).unwrap();
    for _ in 0..50 {
        b.advance(0.01, Vec3::Z).unwrap();
    }
    let a = a.snapshot(&camera()).particles[0];
    let b = b.snapshot(&camera()).particles[0];
    assert!(a.position.distance(b.position) < 0.0001);
    assert!((a.position.x - 2. * (1. - (-0.5f32).exp())).abs() < 0.00001);
    assert!(a.position.z < 0.);
    assert!((a.size - 1.5).abs() < 0.00001);
    assert!((a.color.w - 0.75).abs() < 0.00001);
    assert!((a.spin - std::f32::consts::FRAC_PI_4).abs() < 0.00001);
}
#[test]
fn seeded_variance_stop_immediate_and_repeated_sources_stay_bounded() {
    let pack = fixture(|l| {
        l.emitters[0].period_variance = 0.05;
        l.emitters[0].theta_degrees = [0., 180.];
        l.particles[0].lifetime_variance = 1.;
    });
    let mut a = world(pack.clone());
    let mut b = world(pack);
    for w in [&mut a, &mut b] {
        w.start_emitter(
            "emitter",
            SourceTransform::default(),
            SourceOptions::default(),
        )
        .unwrap();
        w.advance(0.8, Vec3::ZERO).unwrap();
    }
    let af = a.snapshot(&camera());
    let bf = b.snapshot(&camera());
    assert_eq!(af.particles.len(), bf.particles.len());
    for (a, b) in af.particles.iter().zip(&bf.particles) {
        assert_eq!(a.position, b.position);
        assert_eq!(a.color, b.color);
    }
    a.teardown();
    for _ in 0..500 {
        let h = a
            .start_emitter(
                "emitter",
                SourceTransform::default(),
                SourceOptions::default(),
            )
            .unwrap();
        a.advance(0.1, Vec3::ZERO).unwrap();
        a.stop(h, StopMode::Immediate);
    }
    assert_eq!(a.source_count(), 0);
    assert_eq!(a.particle_count(), 0);
}
#[test]
fn large_elapsed_time_expires_particles_and_reports_bounded_work() {
    let mut w = EffectsWorld::new(
        fixture(|_| {}),
        EffectsLimits {
            sources: 2,
            particles: 4,
            lights: 1,
            emissions_per_advance: 8,
        },
        1,
    )
    .unwrap();
    let h = w
        .start_emitter(
            "emitter",
            SourceTransform::default(),
            SourceOptions::default(),
        )
        .unwrap();
    w.advance(0.9, Vec3::ZERO).unwrap();
    assert_eq!(w.particle_count(), 4);
    assert!(w.diagnostics().particle_capacity_drops > 0);
    w.advance(3600., Vec3::ZERO).unwrap();
    assert_eq!(w.particle_count(), 0);
    assert!(w.diagnostics().emission_budget_skips > 30000);
    assert!(w.is_active(h));
    assert!(w.advance(f32::NAN, Vec3::ZERO).is_err());
    assert!(
        w.burst(
            "missing",
            SourceTransform::default(),
            SourceOptions::default(),
            1
        )
        .is_err()
    );
}
#[test]
fn oriented_particles_and_runtime_override_keys_are_preserved() {
    let mut w = world(fixture(|l| {
        l.emitters[0].orient = true;
        l.emitters[0].use_emitter_colors = true;
        l.emitters[0].use_emitter_sizes = true;
    }));
    w.burst(
        "emitter",
        SourceTransform::default(),
        SourceOptions {
            colors: Some([[0., 1., 0., 0.5]; 4]),
            sizes: Some([7.; 4]),
            ..Default::default()
        },
        1,
    )
    .unwrap();
    w.advance(0.2, Vec3::ZERO).unwrap();
    let p = w.snapshot(&camera()).particles[0];
    assert_eq!(p.axis, Vec3::Y);
    assert_eq!(p.size, 7.);
    assert_eq!(p.color.to_array(), [0., 1., 0., 0.5]);
    assert_eq!(p.blend, BlendMode::Alpha);
}
#[test]
fn flares_use_authored_luminance_radius_fade_and_first_person_visibility() {
    let mut w = world(fixture(|_| {}));
    let h = w
        .start_light(
            "light",
            SourceTransform::default(),
            SourceOptions::default(),
        )
        .unwrap();
    let f = w.snapshot(&camera());
    assert_eq!(f.lights.len(), 1);
    assert_eq!(f.lights[0].color, Vec3::new(2., 1., 0.));
    let initial = f.particles[0].size;
    assert!((initial - 2. * (2. * 0.212671 + 0.715160)).abs() < 0.00001);
    assert!(!f.particles[0].depth_test);
    w.update_options(
        h,
        SourceOptions {
            flare_visibility: 0.,
            ..Default::default()
        },
    )
    .unwrap();
    w.advance(0.25, Vec3::ZERO).unwrap();
    assert!((w.snapshot(&camera()).particles[0].size - initial / 2.).abs() < 0.00001);
    w.update_options(
        h,
        SourceOptions {
            first_person_owner: true,
            ..Default::default()
        },
    )
    .unwrap();
    let f = w.snapshot(&camera());
    assert!(f.particles.is_empty());
    assert_eq!(f.lights.len(), 1);
    w.teardown();
    assert!(w.snapshot(&camera()).lights.is_empty());
}

#[test]
fn paused_source_updates_existing_particle_wind_visibility_and_paint() {
    let mut w = world(fixture(|l| {
        l.particles[0].wind = 1.;
        l.emitters[0].speed = 0.;
        l.emitters[0].use_emitter_colors = true;
    }));
    let h = w
        .start_emitter(
            "emitter",
            SourceTransform::default(),
            SourceOptions::default(),
        )
        .unwrap();
    w.advance(0.25, Vec3::ZERO).unwrap();
    let count = w.particle_count();
    assert_eq!(count, 2);
    w.update_options(
        h,
        SourceOptions {
            emitting: false,
            visible: false,
            wind: Vec3::X * 4.,
            ..Default::default()
        },
    )
    .unwrap();
    w.advance(0.1, Vec3::ZERO).unwrap();
    assert!(w.snapshot(&camera()).particles.is_empty());
    assert_eq!(w.particle_count(), count);
    w.update_options(
        h,
        SourceOptions {
            emitting: false,
            wind: Vec3::X * 4.,
            colors: Some([[0., 1., 0., 0.4]; 4]),
            ..Default::default()
        },
    )
    .unwrap();
    w.advance(0.1, Vec3::ZERO).unwrap();
    let frame = w.snapshot(&camera());
    assert_eq!(frame.particles.len(), count);
    assert!(
        frame
            .particles
            .iter()
            .all(|p| p.position.x < -0.07 && p.color.to_array() == [0., 1., 0., 0.4])
    );
}
#[test]
fn brick_directions_thresholds_and_fake_death_follow_recovered_script() {
    let pack = fixture(|l| {
        l.emitters[0].node_time_scale = 3.;
        l.emitters[0].point_node_time_scale = 2.;
    });
    assert!((brick_direction(2).unwrap() * Vec3::Y).distance(Vec3::Z) < 0.00001);
    assert!((brick_direction(3).unwrap() * Vec3::Y).distance(Vec3::NEG_X) < 0.00001);
    assert!(brick_direction(6).is_err());
    let (_, o) = brick_source(
        &pack,
        "emitter",
        &BrickAttachment {
            center: Vec3::ZERO,
            world_size: Vec3::new(0.5, 0.6, 0.5),
            stud_size: [1, 1, 3],
            direction: 0,
            paint: [1.; 4],
            fake_dead: true,
        },
    )
    .unwrap();
    assert_eq!(o.half_extents, Vec3::ZERO);
    assert_eq!(o.time_scale, 2.);
    assert!(!o.emitting);
}
#[test]
fn native_loader_rejects_texture_path_hash_and_dimension_tampering() {
    let p = fixture(|_| {});
    let dir = std::env::temp_dir().join(format!(
        "bri-fx-fixture-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let image = image::RgbaImage::from_raw(1, 1, vec![120, 80, 20, 255]).unwrap();
    image.save(dir.join("texture.png")).unwrap();
    let png = std::fs::read(dir.join("texture.png")).unwrap();
    let json = serde_json::to_vec(&p.library).unwrap();
    std::fs::write(dir.join("effects.json"), &json).unwrap();
    let mut m = p.manifest.clone();
    m.library_sha256 = digest(&json);
    m.textures.insert(
        "original".into(),
        TextureRecord {
            file: "texture.png".into(),
            sha256: digest(&png),
            width: 1,
            height: 1,
        },
    );
    let write = |m: &Manifest| {
        std::fs::write(dir.join("manifest.json"), serde_json::to_vec(m).unwrap()).unwrap()
    };
    write(&m);
    assert!(EffectsPack::load(&dir).is_ok());
    m.textures.get_mut("original").unwrap().width = 2;
    write(&m);
    assert!(
        EffectsPack::load(&dir)
            .unwrap_err_text()
            .contains("dimensions")
    );
    m.textures.get_mut("original").unwrap().width = 1;
    m.textures.get_mut("original").unwrap().sha256 = "bad".into();
    write(&m);
    assert!(
        EffectsPack::load(&dir)
            .unwrap_err_text()
            .contains("checksum")
    );
    let mut l = p.library.clone();
    l.textures.insert("original".into(), "../escape.png".into());
    let json = serde_json::to_vec(&l).unwrap();
    std::fs::write(dir.join("effects.json"), &json).unwrap();
    m.library_sha256 = digest(&json);
    write(&m);
    assert!(EffectsPack::load(&dir).is_err());
    // Leave tiny fixture files for post-failure inspection; no original installation is touched.
}
trait ErrorText {
    fn unwrap_err_text(self) -> String;
}
impl ErrorText for anyhow::Result<Arc<EffectsPack>> {
    fn unwrap_err_text(self) -> String {
        match self {
            Ok(_) => panic!("expected error"),
            Err(e) => e.to_string(),
        }
    }
}

fn all_emitters_lights_and_composites_execute(pack: Arc<EffectsPack>) {
    assert!(!pack.library.emitters.is_empty() && !pack.manifest.composites.is_empty());
    for e in &pack.library.emitters {
        let mut w = world(pack.clone());
        let h = w
            .start_emitter(&e.id, SourceTransform::default(), SourceOptions::default())
            .unwrap();
        w.advance(0.5, Vec3::new(1., 0., 0.)).unwrap();
        let f = w.snapshot(&camera());
        assert!(
            f.particles
                .iter()
                .all(|p| p.position.is_finite() && p.color.is_finite())
        );
        w.stop(h, StopMode::Immediate);
        assert_eq!(w.particle_count(), 0);
    }
    let mut w = world(pack.clone());
    for l in &pack.library.lights {
        w.start_light(&l.id, SourceTransform::default(), SourceOptions::default())
            .unwrap();
    }
    w.advance(0.11, Vec3::ZERO).unwrap();
    assert!(!w.snapshot(&camera()).lights.is_empty());
    w.teardown();
    for c in &pack.manifest.composites {
        let handles = w
            .play_composite(&c.id, SourceTransform::default(), SourceOptions::default())
            .unwrap();
        w.advance(0.01, Vec3::ZERO).unwrap();
        for h in handles {
            w.stop(h, StopMode::Immediate);
        }
        assert_eq!(w.particle_count(), 0);
        assert_eq!(w.source_count(), 0);
    }
}
#[test]
fn all_emitters_lights_and_composites_execute_synthetic() {
    all_emitters_lights_and_composites_execute(bri_fx_runtime::testing::showcase_pack());
}
#[test]
#[ignore = "requires generated v20 content"]
fn original_pack_all_emitters_lights_and_composites_execute() {
    all_emitters_lights_and_composites_execute(content_pack("effects-runtime-pack-005"));
}
#[test]
#[ignore = "requires generated v20 content"]
fn original_pack_counts() {
    let pack = content_pack("effects-runtime-pack-005");
    assert_eq!(pack.library.particles.len(), 132);
    assert_eq!(pack.library.emitters.len(), 133);
    assert_eq!(pack.textures.len(), 18);
}
#[test]
fn brick_paint_tints_rgb_but_keeps_authored_alpha_keys() {
    // Fog A's shape: a faint puff fading in and out, painted opaque white.
    let fog = |l: &mut Library| {
        l.emitters[0].speed = 0.;
        l.particles[0].keys = vec![
            ParticleKey {
                time: 0.,
                color: [1., 1., 1., 0.],
                size: 1.5,
            },
            ParticleKey {
                time: 0.2,
                color: [1., 1., 1., 0.5],
                size: 2.,
            },
            ParticleKey {
                time: 1.,
                color: [1., 1., 1., 0.],
                size: 1.6,
            },
        ];
    };
    let brick = |paint| BrickAttachment {
        center: Vec3::ZERO,
        world_size: Vec3::new(0.5, 0.6, 0.5),
        stud_size: [1, 1, 3],
        direction: 0,
        paint,
        fake_dead: false,
    };
    let pack = fixture(|l| {
        fog(l);
        l.emitters[0].use_emitter_colors = true;
    });
    let mut w = world(pack.clone());
    let (t, o) = brick_source(&pack, "emitter", &brick([0.9, 0.2, 0.1, 1.])).unwrap();
    let h = w.start_emitter("emitter", t, o).unwrap();
    w.advance(1.95, Vec3::ZERO).unwrap();
    let frame = w.snapshot(&camera());
    assert!(frame.particles.len() > 10);
    for p in &frame.particles {
        assert_eq!(p.color.truncate().to_array(), [0.9, 0.2, 0.1]);
        assert!(p.color.w <= 0.5 + 1e-6, "paint alpha replaced the keys");
    }
    assert!(frame.particles.iter().any(|p| p.color.w < 0.1));
    // Repainting retints live particles; the alpha keys still hold.
    let (_, o) = brick_source(&pack, "emitter", &brick([0., 0., 1., 1.])).unwrap();
    w.update_options(h, o).unwrap();
    w.advance(0.01, Vec3::ZERO).unwrap();
    for p in &w.snapshot(&camera()).particles {
        assert_eq!(p.color.truncate().to_array(), [0., 0., 1.]);
        assert!(p.color.w <= 0.5 + 1e-6);
    }
    // Without useEmitterColors the paint is ignored entirely.
    let pack = fixture(fog);
    let mut w = world(pack.clone());
    let (t, o) = brick_source(&pack, "emitter", &brick([0.9, 0.2, 0.1, 1.])).unwrap();
    w.start_emitter("emitter", t, o).unwrap();
    w.advance(1., Vec3::ZERO).unwrap();
    assert!(
        w.snapshot(&camera())
            .particles
            .iter()
            .all(|p| p.color.truncate().to_array() == [1.; 3] && p.color.w <= 0.5 + 1e-6)
    );
}
/// Every paint-taking emitter, on an opaque white brick, stays at or below
/// its particles' authored alpha peak; `expected` must all be among them.
fn painted_brick_emitters_never_exceed_authored_alpha(pack: Arc<EffectsPack>, expected: &[&str]) {
    let mut checked = Vec::new();
    for e in pack
        .library
        .emitters
        .iter()
        .filter(|e| e.use_emitter_colors)
    {
        let peak = pack
            .library
            .particles
            .iter()
            .filter(|p| e.particles.contains(&p.id))
            .flat_map(|p| p.keys.iter().map(|k| k.color[3]))
            .fold(0f32, f32::max);
        let mut w = world(pack.clone());
        let (t, o) = brick_source(
            &pack,
            &e.id,
            &BrickAttachment {
                center: Vec3::ZERO,
                world_size: Vec3::new(0.5, 0.6, 0.5),
                stud_size: [1, 1, 3],
                direction: 0,
                paint: [1.; 4],
                fake_dead: false,
            },
        )
        .unwrap();
        w.start_emitter(&e.id, t, o).unwrap();
        w.advance(2.5, Vec3::ZERO).unwrap();
        for p in &w.snapshot(&camera()).particles {
            assert!(
                p.color.w <= peak + 1e-5,
                "{} drew alpha {} above its authored peak {peak}",
                e.name,
                p.color.w
            );
        }
        checked.push(e.name.as_str());
    }
    for &name in expected {
        assert!(checked.contains(&name), "{name} missing: {checked:?}");
    }
}
#[test]
fn painted_brick_emitters_never_exceed_authored_alpha_synthetic() {
    painted_brick_emitters_never_exceed_authored_alpha(
        bri_fx_runtime::testing::showcase_pack(),
        &bri_fx_runtime::testing::PAINTED_EMITTERS,
    );
}
// Slate "Ice Palace.bls": 199 Fog A/B emitters on opaque white bricks.
#[test]
#[ignore = "requires generated v20 content"]
fn original_painted_brick_emitters_never_exceed_authored_alpha() {
    painted_brick_emitters_never_exceed_authored_alpha(
        content_pack("effects-runtime-pack-005"),
        &["Fog A", "Fog B", "Fog C"],
    );
}
#[test]
fn recolor_replaces_rgb_keeps_alpha_keys_and_overrides_blend() {
    let mut w = world(fixture(|l| l.emitters[0].use_emitter_colors = true));
    w.burst(
        "emitter",
        SourceTransform::default(),
        SourceOptions {
            recolor: Some(Recolor {
                rgb: [0.2, 0.6, 0.1],
                blend: Some(BlendMode::Additive),
            }),
            ..Default::default()
        },
        1,
    )
    .unwrap();
    w.advance(1., Vec3::ZERO).unwrap();
    let p = w.snapshot(&camera()).particles[0];
    assert!(
        p.color
            .abs_diff_eq(glam::Vec4::new(0.2, 0.6, 0.1, 0.5), 1e-6)
    );
    assert_eq!(p.blend, BlendMode::Additive);
}
#[test]
fn in_view_snapshot_leaves_out_only_sprites_the_camera_cannot_see() {
    let mut w = world(fixture(|l| l.emitters[0].speed = 0.));
    for x in [0., 40., -40.] {
        w.burst(
            "emitter",
            SourceTransform {
                position: Vec3::new(x, 0., -10.),
                ..Default::default()
            },
            SourceOptions::default(),
            1,
        )
        .unwrap();
    }
    // Looking down -Z with a 90 degree view: only the sprite ahead shows.
    let camera = Camera {
        view_projection: bri_render::scene::perspective(90f32.to_radians(), 1., 0.1, 100.)
            * glam::camera::rh::view::look_at_mat4(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y),
        position: Vec3::ZERO,
        right: Vec3::X,
        up: Vec3::Y,
    };
    assert_eq!(w.snapshot(&camera).particles.len(), 3);
    let seen = w.snapshot_in_view(&camera).particles;
    assert_eq!(seen.len(), 1);
    assert!(seen[0].position.x.abs() < 0.001);
}
