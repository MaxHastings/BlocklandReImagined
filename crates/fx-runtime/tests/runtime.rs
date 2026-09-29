use bri_content::effects::*;
use bri_fx_runtime::{
    pack::{TextureImage, digest},
    *,
};
use glam::{Mat4, Vec3};
use std::{collections::BTreeMap, sync::Arc};
fn fixture(mut change: impl FnMut(&mut Library)) -> Arc<EffectsPack> {
    let mut library = Library {
        schema_version: 1,
        textures: BTreeMap::from([("original".into(), "texture.png".into())]),
        lights: vec![Light {
            id: "light".into(),
            name: "Light".into(),
            enabled: true,
            color: [1., 0.5, 0.],
            brightness: 2.,
            radius: 5.,
            color_curves: None,
            brightness_curve: None,
            radius_curve: None,
            flare: Some(Flare {
                texture: "original".into(),
                color: [1.; 3],
                third_person: true,
                constant_size: Some(1.),
                near_size: 1.,
                far_size: 0.5,
                near_distance: 0.,
                far_distance: 10.,
                fade_seconds: 0.5,
                blend_mode: 0,
                link_color: true,
                link_size: true,
            }),
        }],
        particles: vec![Particle {
            id: "particle".into(),
            texture: "original".into(),
            alpha_blend: true,
            lifetime: 2.,
            lifetime_variance: 0.,
            drag: 0.,
            wind: 0.,
            gravity: 0.,
            inherited_velocity: 0.,
            acceleration: 0.,
            spin_degrees: 90.,
            random_spin: [0., 0.],
            keys: vec![
                ParticleKey {
                    time: 0.,
                    color: [1., 0., 0., 1.],
                    size: 1.,
                },
                ParticleKey {
                    time: 1.,
                    color: [0., 0., 1., 0.],
                    size: 3.,
                },
            ],
        }],
        emitters: vec![Emitter {
            id: "emitter".into(),
            name: "Emitter".into(),
            particles: vec!["particle".into()],
            period: 0.1,
            period_variance: 0.,
            speed: 2.,
            speed_variance: 0.,
            offset: 0.,
            offset_variance: 0.,
            theta_degrees: [0., 0.],
            phi_rate_degrees: 0.,
            phi_variance_degrees: 0.,
            lifetime: 0.,
            lifetime_variance: 0.,
            orient: false,
            orient_on_velocity: true,
            override_advance: false,
            use_emitter_colors: false,
            use_emitter_sizes: false,
            use_placement_velocity: false,
            node_time_scale: 1.,
            point_node_time_scale: 1.,
        }],
    };
    change(&mut library);
    EffectsPack::from_parts(
        library,
        Manifest {
            schema_version: 1,
            library_sha256: String::new(),
            textures: BTreeMap::new(),
            emitter_alpha: BTreeMap::new(),
            bindings: Vec::new(),
            composites: Vec::new(),
            unresolved: Vec::new(),
        },
        vec![TextureImage {
            id: "original".into(),
            width: 1,
            height: 1,
            rgba: vec![120, 80, 20, 255],
        }],
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

#[test]
#[ignore = "requires locally converted original content; run explicitly"]
fn original_pack_all_emitters_lights_and_composites_execute() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/effects-runtime-pack-001");
    let pack = EffectsPack::load(root).unwrap();
    assert_eq!(pack.library.particles.len(), 119);
    assert_eq!(pack.library.emitters.len(), 120);
    assert_eq!(pack.textures.len(), 18);
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
