use bri_content::effects::*;
use bri_fx_runtime::{pack::digest, *};
use glam::{Mat4, Vec3};
use std::sync::Arc;
fn fixture(change: impl FnMut(&mut Library)) -> Arc<EffectsPack> {
    bri_fx_runtime::testing::pack(change)
}
/// A converted pack under `content/`.
fn content_pack() -> Arc<EffectsPack> {
    EffectsPack::load(bri_package::testing::pack_dir(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"),
        "effects_runtime",
    ))
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

#[test]
fn planar_particles_preserve_source_roll_across_views_and_attachment_carry() {
    let mut w = world(fixture(|l| {
        l.particles[0].orientation = Some(ParticleOrientation::Plane);
        l.emitters[0].speed = 0.;
        l.emitters[0].orient_on_velocity = false;
    }));
    let initial = SourceTransform {
        rotation: glam::Quat::from_euler(glam::EulerRot::XYZ, 0.3, 0.7, -0.6),
        ..Default::default()
    };
    let h = w
        .burst(
            "emitter",
            initial,
            SourceOptions {
                follow_source: true,
                ..Default::default()
            },
            1,
        )
        .unwrap();
    let a = w.snapshot(&camera()).particles[0];
    let b = w
        .snapshot(&Camera {
            right: Vec3::Y,
            up: Vec3::NEG_X,
            ..camera()
        })
        .particles[0];
    assert!(a.axis.abs_diff_eq(initial.rotation * Vec3::Y, 1e-6));
    assert!(a.plane_right.abs_diff_eq(initial.rotation * Vec3::X, 1e-6));
    assert_eq!(a.plane_right, b.plane_right);
    assert_eq!(a.axis, b.axis);
    let current = SourceTransform {
        position: Vec3::new(3., 2., 1.),
        rotation: glam::Quat::from_euler(glam::EulerRot::XYZ, -0.4, -0.2, 0.8),
        ..Default::default()
    };
    w.carry_attachment(h, initial, current).unwrap();
    let moved = w.snapshot(&camera()).particles[0];
    assert!(moved.axis.abs_diff_eq(current.rotation * Vec3::Y, 1e-6));
    assert!(
        moved
            .plane_right
            .abs_diff_eq(current.rotation * Vec3::X, 1e-6)
    );
    assert!(moved.position.abs_diff_eq(current.position, 1e-6));
}

#[test]
fn directional_coronas_follow_the_source_in_each_view_without_point_light() {
    let mut w = world(fixture(|l| {
        let light = &mut l.lights[0];
        light.radius = 0.;
        let flare = light.flare.as_mut().unwrap();
        flare.angles = Some([0.4, 0.8]);
        flare.depth_test = true;
    }));
    let h = w
        .start_light(
            "light",
            SourceTransform::default(),
            SourceOptions::default(),
        )
        .unwrap();
    let mut front = camera();
    front.position = Vec3::Y * 5.;
    let mut rear = camera();
    rear.position = Vec3::NEG_Y * 5.;
    let frame = w.snapshot(&front);
    assert!(
        frame.lights.is_empty(),
        "a corona does not add a duplicate point light"
    );
    assert_eq!(frame.particles.len(), 1);
    assert!(frame.particles[0].depth_test);
    assert!(w.snapshot(&rear).particles.is_empty());
    w.update_source(
        h,
        SourceTransform {
            rotation: glam::Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(w.snapshot(&front).particles.is_empty());
    let mut side = camera();
    side.position = Vec3::X * 5.;
    assert_eq!(w.snapshot(&side).particles.len(), 1);
}
fn world(pack: Arc<EffectsPack>) -> EffectsWorld {
    EffectsWorld::new(pack, EffectsLimits::default(), 73).unwrap()
}

#[test]
fn authored_particle_frames_advance_without_wrapping_and_survive_split_frames() {
    let mut library = bri_fx_runtime::testing::library();
    let frames = ["frame0", "frame1", "frame2"];
    for id in frames {
        library.textures.insert(id.into(), format!("{id}.png"));
    }
    library.particles[0].animation = Some(ParticleAnimation {
        layouts: [(
            "frame1".into(),
            SpriteLayout {
                scale: [0.5, 1.],
                pivot: [0.25, 0.75],
            },
        )]
        .into(),
        frames: frames.map(str::to_owned).to_vec(),
        rate: [10.; 2],
        looping: false,
        random_start: false,
        reverse: false,
    });
    let textures = library
        .textures
        .keys()
        .enumerate()
        .map(|(i, id)| pack::TextureImage {
            id: id.clone(),
            width: 1,
            height: 1,
            rgba: vec![i as u8, 0, 0, 255],
        })
        .collect();
    let pack =
        EffectsPack::from_parts(library, bri_fx_runtime::testing::manifest(), textures).unwrap();
    let index = |id: &str| pack.textures.iter().position(|t| t.id == id).unwrap() as u32;
    let mut a = world(pack.clone());
    let mut b = world(pack.clone());
    for w in [&mut a, &mut b] {
        w.burst(
            "emitter",
            SourceTransform::default(),
            SourceOptions::default(),
            1,
        )
        .unwrap();
    }
    assert_eq!(a.snapshot(&camera()).particles[0].texture, index("frame0"));
    assert_eq!(
        a.snapshot(&camera()).particles[0].layout,
        SpriteLayout::default()
    );
    a.advance(0.15, Vec3::ZERO).unwrap();
    for _ in 0..3 {
        b.advance(0.05, Vec3::ZERO).unwrap();
    }
    assert_eq!(a.snapshot(&camera()).particles[0].texture, index("frame1"));
    assert_eq!(
        a.snapshot(&camera()).particles[0].layout,
        SpriteLayout {
            scale: [0.5, 1.],
            pivot: [0.25, 0.75]
        }
    );
    assert_eq!(
        a.snapshot(&camera()).particles[0].texture,
        b.snapshot(&camera()).particles[0].texture
    );
    a.advance(0.85, Vec3::ZERO).unwrap();
    assert_eq!(
        a.snapshot(&camera()).particles[0].texture,
        index("frame2"),
        "last image holds"
    );
    let mut invalid = pack.library.clone();
    invalid.particles[0]
        .animation
        .as_mut()
        .unwrap()
        .frames
        .push("missing".into());
    assert!(invalid.validate().is_err());
    invalid.particles[0]
        .animation
        .as_mut()
        .unwrap()
        .frames
        .pop();
    invalid.particles[0].animation.as_mut().unwrap().rate[0] = f32::NAN;
    assert!(invalid.validate().is_err());
}

#[test]
fn composite_particle_groups_share_one_budget_and_reject_atomically() {
    let library = bri_fx_runtime::testing::library();
    let mut manifest = bri_fx_runtime::testing::manifest();
    manifest.composites.push(pack::Composite {
        id: "contact".into(),
        lifetime: 0.1,
        emitters: vec![],
        light: None,
        burst: Some(("emitter".into(), 2, 0.)),
        bursts: [3, 4]
            .into_iter()
            .map(|n| Burst {
                emitter: "emitter".into(),
                count: [n, n],
                ..Default::default()
            })
            .collect(),
    });
    let pack = bri_fx_runtime::testing::pack_from(library, manifest);
    let mut w = world(pack.clone());
    w.play_composite(
        "contact",
        SourceTransform::default(),
        SourceOptions::default(),
    )
    .unwrap();
    assert_eq!(w.particle_count(), 9);
    let limits = EffectsLimits {
        emissions_per_advance: 8,
        ..Default::default()
    };
    let mut w = EffectsWorld::new(pack, limits, 73).unwrap();
    assert!(
        w.play_composite(
            "contact",
            SourceTransform::default(),
            SourceOptions::default()
        )
        .is_err()
    );
    assert_eq!(
        w.particle_count(),
        0,
        "a rejected effect never starts half its groups"
    );
    assert_eq!(w.source_count(), 0);
}

#[test]
fn hsv_tints_are_sampled_once_without_changing_emission_or_alpha() {
    let make = |mode| {
        world(fixture(|l| {
            for key in &mut l.particles[0].keys {
                key.color = [1.; 4];
            }
            l.particles[0].variation = Some(ParticleVariation {
                color: [[1., 0., 0., 0.2], [0., 0., 1., 0.8]],
                color_interpolation: mode,
                size: [1.; 2],
                initial_spin_degrees: [0.; 2],
            });
        }))
    };
    let mut rgb = make(TintInterpolation::Rgb);
    let mut hsv = make(TintInterpolation::HsvShort);
    for w in [&mut rgb, &mut hsv] {
        w.burst(
            "emitter",
            SourceTransform::default(),
            SourceOptions::default(),
            32,
        )
        .unwrap();
    }
    let before = hsv.snapshot(&camera());
    for (a, b) in rgb
        .snapshot(&camera())
        .particles
        .iter()
        .zip(&before.particles)
    {
        assert_eq!(a.position, b.position);
        assert_eq!(a.color.w, b.color.w);
        assert_eq!(a.spin, b.spin);
        assert!(b.color.x.max(b.color.z) > 0.99999);
        assert_eq!(b.color.y, 0.);
        assert!(a.color.x.max(a.color.z) < b.color.x.max(b.color.z));
    }
    hsv.advance(0.01, Vec3::ZERO).unwrap();
    let after = hsv.snapshot(&camera());
    // Particle velocities can change depth order; compare tint multisets.
    let tints = |f: FrameEffects| {
        let mut colors: Vec<_> = f
            .particles
            .iter()
            .map(|p| p.color.to_array().map(f32::to_bits))
            .collect();
        colors.sort();
        colors
    };
    assert_eq!(tints(before), tints(after));
}

#[test]
fn authored_groups_keep_seeded_counts_placement_tints_and_sizes() {
    let mut library = bri_fx_runtime::testing::library();
    library.emitters[0].speed = 0.;
    let particle = &mut library.particles[0];
    for key in &mut particle.keys {
        key.color = [1.; 4];
        key.size = 1.;
    }
    particle.variation = Some(ParticleVariation {
        color_interpolation: Default::default(),
        color: [[0.25, 0.5, 0.75, 1.], [0.5, 0.75, 1., 0.8]],
        size: [0.1, 0.3],
        initial_spin_degrees: [90., 180.],
    });
    let mut manifest = bri_fx_runtime::testing::manifest();
    manifest.composites.push(pack::Composite {
        id: "contact".into(),
        lifetime: 0.1,
        emitters: vec![],
        light: None,
        burst: None,
        bursts: vec![Burst {
            emitter: "emitter".into(),
            count: [7, 11],
            radius: [0.2, 0.4],
            offset: [10., 0., 0.],
            ..Default::default()
        }],
    });
    let pack = bri_fx_runtime::testing::pack_from(library, manifest);
    let mut a = world(pack.clone());
    let mut b = world(pack);
    for w in [&mut a, &mut b] {
        w.play_composite(
            "contact",
            SourceTransform {
                position: Vec3::X * 20.,
                ..Default::default()
            },
            SourceOptions::default(),
        )
        .unwrap();
    }
    let a = a.snapshot(&camera());
    let b = b.snapshot(&camera());
    assert!((7..=11).contains(&a.particles.len()));
    assert_eq!(a.particles.len(), b.particles.len());
    for (a, b) in a.particles.iter().zip(&b.particles) {
        assert_eq!(a.position, b.position);
        assert_eq!(a.color, b.color);
        assert_eq!(a.size, b.size);
        assert_eq!(a.spin, b.spin);
        assert!((std::f32::consts::FRAC_PI_2..=std::f32::consts::PI).contains(&a.spin));
        assert!((0.1999..=0.4001).contains(&a.position.distance(Vec3::X * 30.)));
        assert!((0.1..=0.3).contains(&a.size));
        assert!((0.25..=0.5).contains(&a.color.x));
        assert!((0.8..=1.).contains(&a.color.w));
    }
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
    all_emitters_lights_and_composites_execute(content_pack());
}
#[test]
#[ignore = "requires generated v20 content"]
fn original_pack_counts() {
    let pack = content_pack();
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
        content_pack(),
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

/// Two still emitters 200 units apart, in two worlds alike but for
/// [`EffectsWorld::cull_sources`], and a camera on each.
fn far_apart() -> (EffectsWorld, EffectsWorld, Camera, Camera) {
    let pack = fixture(|l| {
        l.emitters[0].period = 0.05;
        l.emitters[0].period_variance = 0.02;
        l.emitters[0].theta_degrees = [0., 60.];
        l.particles[0].lifetime_variance = 0.5;
        l.particles[0].drag = 0.4;
        l.particles[0].gravity = 0.2;
    });
    let start = |w: &mut EffectsWorld| {
        for x in [0., 200.] {
            w.start_emitter(
                "emitter",
                SourceTransform {
                    position: Vec3::new(x, 0., -10.),
                    ..Default::default()
                },
                SourceOptions::default(),
            )
            .unwrap();
        }
    };
    let (mut culled, mut reference) = (world(pack.clone()), world(pack));
    start(&mut culled);
    start(&mut reference);
    let camera = |x: f32| Camera {
        view_projection: bri_render::scene::perspective(70f32.to_radians(), 1., 0.1, 500.)
            * glam::camera::rh::view::look_at_mat4(
                Vec3::new(x, 1., 0.),
                Vec3::new(x, 1., -10.),
                Vec3::Y,
            ),
        position: Vec3::new(x, 1., 0.),
        right: Vec3::X,
        up: Vec3::Y,
    };
    (culled, reference, camera(0.), camera(200.))
}

#[test]
fn unseen_emitters_sleep_and_what_is_seen_stays_identical() {
    let (mut culled, mut reference, near, _) = far_apart();
    for _ in 0..300 {
        culled.advance(1. / 100., Vec3::ZERO).unwrap();
        reference.advance(1. / 100., Vec3::ZERO).unwrap();
        culled.cull_sources(&[near.view_projection]);
    }
    // The far emitter is stepped no more: it has no particles at all.
    assert_eq!(culled.dormant_count(), 1);
    assert_eq!(reference.dormant_count(), 0);
    let near_only = culled.particle_count();
    assert!(near_only > 20 && near_only * 3 < reference.particle_count() * 2);
    // What the camera sees is the very same sprites, in the same order.
    let (a, b) = (
        culled.snapshot_in_view(&near).particles,
        reference.snapshot_in_view(&near).particles,
    );
    assert!(!a.is_empty());
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(&b) {
        assert_eq!(a.position, b.position);
        assert_eq!(a.color, b.color);
        assert_eq!(a.size, b.size);
    }
}

#[test]
fn a_waking_emitter_refills_its_cloud_before_it_is_seen() {
    let (mut culled, mut reference, near, far) = far_apart();
    for _ in 0..300 {
        culled.advance(1. / 100., Vec3::ZERO).unwrap();
        reference.advance(1. / 100., Vec3::ZERO).unwrap();
        culled.cull_sources(&[near.view_projection]);
    }
    // Turning to the far emitter wakes it with a full cloud in the same
    // frame: as many sprites show as from one that never slept.
    culled.cull_sources(&[far.view_projection]);
    assert_eq!(culled.dormant_count(), 0);
    assert_eq!(culled.diagnostics().wakes, 1);
    let woken = culled.snapshot_in_view(&far).particles.len() as f32;
    let steady = reference.snapshot_in_view(&far).particles.len() as f32;
    assert!(steady > 20., "{steady}");
    assert!(
        (woken - steady).abs() <= steady * 0.15,
        "{woken} vs {steady}"
    );
    // And it keeps emitting from there, with no burst.
    for _ in 0..50 {
        culled.advance(1. / 100., Vec3::ZERO).unwrap();
        reference.advance(1. / 100., Vec3::ZERO).unwrap();
        culled.cull_sources(&[far.view_projection, near.view_projection]);
    }
    let woken = culled.snapshot_in_view(&far).particles.len() as f32;
    let steady = reference.snapshot_in_view(&far).particles.len() as f32;
    assert!(
        (woken - steady).abs() <= steady * 0.15,
        "{woken} vs {steady}"
    );
}

#[test]
fn lights_moving_sources_and_portal_worlds_never_sleep() {
    let mut w = world(fixture(|_| {}));
    let away = SourceTransform {
        position: Vec3::new(500., 0., 500.),
        ..Default::default()
    };
    w.start_light("light", away, SourceOptions::default())
        .unwrap();
    w.start_emitter(
        "emitter",
        SourceTransform {
            velocity: Vec3::X,
            ..away
        },
        SourceOptions::default(),
    )
    .unwrap();
    let nowhere = bri_render::scene::perspective(70f32.to_radians(), 1., 0.1, 10.)
        * glam::camera::rh::view::look_at_mat4(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
    for _ in 0..200 {
        w.advance(0.01, Vec3::ZERO).unwrap();
        w.cull_sources(&[nowhere]);
    }
    assert_eq!(w.dormant_count(), 0);
}

#[test]
fn swept_fragments_bounce_without_crossing_the_floor_and_keep_the_remaining_step() {
    let mut w = world(fixture(|l| {
        l.particles[0].contact = Some(ParticleContact {
            restitution: 0.5,
            friction: 0.0,
            rest_speed: 0.0,
            die_on_contact: false,
            die_at_rest: false,
            stop_animation_at_rest: false,
        });
    }));
    w.burst(
        "emitter",
        SourceTransform {
            position: Vec3::Y * 0.2,
            rotation: glam::Quat::from_rotation_z(std::f32::consts::PI),
            ..Default::default()
        },
        SourceOptions::default(),
        1,
    )
    .unwrap();
    let mut hits = 0;
    w.advance_with_contacts(0.2, Vec3::ZERO, |a, b| {
        if a.y > 0.0 && b.y <= 0.0 {
            hits += 1;
            Some(ContactHit {
                fraction: a.y / (a.y - b.y),
                normal: Vec3::Y,
            })
        } else {
            None
        }
    })
    .unwrap();
    let f = w.snapshot(&camera());
    assert_eq!(hits, 1);
    assert_eq!(f.particles.len(), 1);
    assert!(
        (f.particles[0].position.y - 0.101).abs() < 0.002,
        "{:?}",
        f.particles[0].position
    );
    // It now travels upward, never carrying on through the floor.
    w.advance_with_contacts(0.1, Vec3::ZERO, |_, _| None)
        .unwrap();
    assert!((w.snapshot(&camera()).particles[0].position.y - 0.201).abs() < 0.002);
}

#[test]
fn cosmetic_contact_death_and_rest_are_distinct_authored_behaviors() {
    for die_on_contact in [false, true] {
        let mut library = bri_fx_runtime::testing::library();
        {
            let l = &mut library;
            for name in ["rest0", "rest1", "rest2", "rest3"] {
                l.textures.insert(name.into(), format!("{name}.png"));
            }
            l.particles[0].animation = Some(ParticleAnimation {
                frames: ["rest0", "rest1", "rest2", "rest3"]
                    .map(str::to_owned)
                    .to_vec(),
                layouts: Default::default(),
                rate: [10.; 2],
                looping: true,
                random_start: false,
                reverse: false,
            });
            l.particles[0].gravity = 1.0;
            l.particles[0].contact = Some(ParticleContact {
                restitution: 0.0,
                friction: 1.0,
                rest_speed: 0.1,
                die_on_contact,
                die_at_rest: false,
                stop_animation_at_rest: true,
            });
        }
        let textures = library
            .textures
            .keys()
            .map(|id| pack::TextureImage {
                id: id.clone(),
                width: 1,
                height: 1,
                rgba: vec![255; 4],
            })
            .collect();
        let mut w = world(
            EffectsPack::from_parts(library, bri_fx_runtime::testing::manifest(), textures)
                .unwrap(),
        );
        w.burst(
            "emitter",
            SourceTransform {
                position: Vec3::Y * 0.05,
                rotation: glam::Quat::from_rotation_z(std::f32::consts::PI),
                ..Default::default()
            },
            SourceOptions::default(),
            1,
        )
        .unwrap();
        let floor = |a: Vec3, b: Vec3| {
            if a.y > 0.0 && b.y <= 0.0 {
                Some(ContactHit {
                    fraction: a.y / (a.y - b.y),
                    normal: Vec3::Y,
                })
            } else {
                None
            }
        };
        w.advance_with_contacts(0.2, Vec3::ZERO, floor).unwrap();
        let f = w.snapshot(&camera());
        assert_eq!(f.particles.len(), usize::from(!die_on_contact));
        if !die_on_contact {
            let at = f.particles[0].position;
            let texture = f.particles[0].texture;
            w.advance_with_contacts(0.15, Vec3::ZERO, floor).unwrap();
            assert_eq!(
                w.snapshot(&camera()).particles[0].texture,
                texture,
                "the original frame freezes at contact rest"
            );
            assert_eq!(w.snapshot(&camera()).particles[0].position, at);
            assert!((at.y - 0.001).abs() < 0.0001);
            w.advance_with_contacts(0.1, Vec3::ZERO, |_, _| None)
                .unwrap();
            assert!(
                w.snapshot(&camera()).particles[0].position.y < at.y - 0.01,
                "removing support resumes falling instead of leaving a floating casing"
            );
        }
    }
    let mut l = bri_fx_runtime::testing::library();
    l.particles[0].contact = Some(ParticleContact {
        restitution: f32::NAN,
        friction: 0.0,
        rest_speed: 0.1,
        die_on_contact: false,
        die_at_rest: false,
        stop_animation_at_rest: false,
    });
    assert!(l.validate().is_err());
}

#[test]
fn contact_queries_share_a_frame_budget_without_unchecked_motion() {
    let mut w = world(fixture(|l| {
        l.particles[0].contact = Some(ParticleContact {
            restitution: 0.0,
            friction: 0.0,
            rest_speed: 0.0,
            die_on_contact: false,
            die_at_rest: false,
            stop_animation_at_rest: false,
        });
    }));
    w.burst(
        "emitter",
        SourceTransform::default(),
        SourceOptions::default(),
        300,
    )
    .unwrap();
    let mut queries = 0;
    w.advance_with_contacts(0.1, Vec3::ZERO, |_, _| {
        queries += 1;
        Some(ContactHit {
            fraction: 0.0,
            normal: Vec3::X,
        })
    })
    .unwrap();
    assert_eq!(queries, 32768);
    assert!(
        w.snapshot(&camera())
            .particles
            .iter()
            .all(|p| p.position.y.abs() < 0.0001),
        "budget exhaustion must wait, never advance through an unqueried wall"
    );
}

#[test]
fn attached_flashes_do_not_inherit_the_source_translation_twice() {
    for follow_source in [false, true] {
        let mut w = world(fixture(|l| {
            l.emitters[0].speed = 0.0;
            l.particles[0].inherited_velocity = 1.0;
        }));
        let before = SourceTransform {
            velocity: Vec3::X * 20.0,
            ..Default::default()
        };
        let after = SourceTransform {
            position: Vec3::X * 0.2,
            ..before
        };
        let handle = w
            .burst(
                "emitter",
                before,
                SourceOptions {
                    follow_source,
                    ..Default::default()
                },
                1,
            )
            .unwrap();
        w.carry_attachment(handle, before, after).unwrap();
        w.advance(0.01, Vec3::ZERO).unwrap();
        let at = w.snapshot(&camera()).particles[0].position;
        assert!(
            at.abs_diff_eq(Vec3::X * 0.2, 1e-5),
            "follow_source={follow_source}: {at:?}"
        );
    }
}

// Invented policies and material IDs exercise mechanisms, not original-game rules.
fn contact_audio_policy() -> ContactAudio {
    ContactAudio {
        surfaces: std::collections::BTreeMap::from([("test_surface".into(), "test_hit".into())]),
        fallback: None,
        min_inward_speed: 0.,
        cooldown_seconds: 0.,
        max_impacts: 100,
        max_lateness_seconds: 1.,
    }
}
fn audio_world(audio: Option<ContactAudio>, die: bool) -> EffectsWorld {
    world(fixture(|l| {
        l.emitters[0].speed = 2.;
        l.particles[0].gravity = 1.;
        l.particles[0].contact = Some(ParticleContact {
            restitution: 0.,
            friction: 1.,
            rest_speed: 0.1,
            die_on_contact: die,
            die_at_rest: false,
            stop_animation_at_rest: false,
        });
        l.particles[0].contact_audio = audio.clone();
    }))
}
fn audio_burst(w: &mut EffectsWorld, count: usize) {
    w.burst(
        "emitter",
        SourceTransform {
            position: Vec3::Y * 0.05,
            rotation: glam::Quat::from_rotation_z(std::f32::consts::PI),
            ..Default::default()
        },
        SourceOptions::default(),
        count,
    )
    .unwrap();
}
fn audio_floor(a: Vec3, b: Vec3) -> Option<ContactHit> {
    (a.y > 0. && b.y <= 0.).then(|| ContactHit {
        fraction: a.y / (a.y - b.y),
        normal: Vec3::Y,
    })
}
#[test]
fn contact_audio_is_opt_in_preserves_visual_rng_and_reports_death_before_removal() {
    let mut silent = audio_world(None, false);
    let mut audible = audio_world(Some(contact_audio_policy()), false);
    for w in [&mut silent, &mut audible] {
        audio_burst(w, 8);
        w.advance_with_contacts(0.1, Vec3::ZERO, audio_floor)
            .unwrap();
    }
    assert!(silent.take_contact_impacts().is_empty());
    let impacts = audible.take_contact_impacts();
    assert_eq!(impacts.len(), 8);
    for (a, b) in silent
        .snapshot(&camera())
        .particles
        .iter()
        .zip(audible.snapshot(&camera()).particles)
    {
        assert_eq!(a.position, b.position);
        assert_eq!(a.color, b.color);
        assert_eq!(a.size, b.size);
    }
    for hit in &impacts {
        assert!(hit.incoming_velocity.dot(hit.normal) < 0.);
        assert!(
            hit.point
                .abs_diff_eq(hit.from.lerp(hit.to, hit.fraction), 1e-6)
        );
        assert!(hit.point.y.abs() < 1e-6);
        assert_eq!(
            audible.contact_profile(hit, "test_surface"),
            Some("test_hit")
        );
        assert_eq!(audible.contact_profile(hit, "unknown"), None);
        let fallback = world(fixture(|l| {
            l.particles[0].contact = Some(ParticleContact {
                restitution: 0.,
                friction: 0.,
                rest_speed: 0.,
                die_on_contact: false,
                die_at_rest: false,
                stop_animation_at_rest: false,
            });
            let mut policy = contact_audio_policy();
            policy.fallback = Some("explicit_fallback".into());
            l.particles[0].contact_audio = Some(policy);
        }));
        assert_eq!(
            fallback.contact_profile(hit, "unknown"),
            Some("explicit_fallback")
        );
    }
    assert_ne!(impacts[0].sound_seed(), impacts[1].sound_seed());
    assert!(audible.take_contact_impacts().is_empty());
    let mut dying = audio_world(Some(contact_audio_policy()), true);
    audio_burst(&mut dying, 1);
    dying
        .advance_with_contacts(0.1, Vec3::ZERO, audio_floor)
        .unwrap();
    assert!(dying.snapshot(&camera()).particles.is_empty());
    assert_eq!(dying.take_contact_impacts().len(), 1);
}
#[test]
fn contact_audio_support_separation_expiry_and_authored_eligibility_are_silent() {
    let mut w = audio_world(Some(contact_audio_policy()), false);
    audio_burst(&mut w, 1);
    w.advance_with_contacts(0.1, Vec3::ZERO, audio_floor)
        .unwrap();
    assert_eq!(w.take_contact_impacts().len(), 1);
    w.advance_with_contacts(0.1, Vec3::ZERO, audio_floor)
        .unwrap();
    assert!(
        w.take_contact_impacts().is_empty(),
        "support probe is silent"
    );
    w.advance_with_contacts(0.1, Vec3::ZERO, |_, _| None)
        .unwrap();
    assert!(w.take_contact_impacts().is_empty());
    w.advance_with_contacts(2., Vec3::ZERO, audio_floor)
        .unwrap();
    assert!(
        w.take_contact_impacts().is_empty(),
        "expired particles do not replay contacts"
    );
    for change in [0, 1] {
        let mut policy = contact_audio_policy();
        if change == 0 {
            policy.min_inward_speed = 100.;
        } else {
            policy.max_lateness_seconds = 0.;
        }
        let mut w = audio_world(Some(policy), false);
        audio_burst(&mut w, 1);
        w.advance_with_contacts(0.2, Vec3::ZERO, audio_floor)
            .unwrap();
        assert!(w.take_contact_impacts().is_empty());
    }
    let mut w = audio_world(Some(contact_audio_policy()), false);
    audio_burst(&mut w, 1);
    w.advance_with_contacts(0.1, Vec3::ZERO, |_, _| {
        Some(ContactHit {
            fraction: 0.5,
            normal: Vec3::NEG_Y,
        })
    })
    .unwrap();
    assert!(
        w.take_contact_impacts().is_empty(),
        "outward hits never request sound"
    );
}
#[test]
fn contact_audio_reports_are_bounded_discarded_and_stable_across_view_sampling() {
    let mut w = audio_world(Some(contact_audio_policy()), true);
    audio_burst(&mut w, 1000);
    w.advance_with_contacts(0.1, Vec3::ZERO, audio_floor)
        .unwrap();
    w.snapshot(&camera());
    w.snapshot(&camera());
    let hits = w.take_contact_impacts();
    assert_eq!(hits.len(), 64);
    assert_eq!(w.diagnostics().contact_report_drops, 936);
    w.advance(0.1, Vec3::ZERO).unwrap();
    assert!(w.take_contact_impacts().is_empty());
    let mut other = audio_world(Some(contact_audio_policy()), true);
    audio_burst(&mut other, 1000);
    other
        .advance_with_contacts(0.1, Vec3::ZERO, audio_floor)
        .unwrap();
    assert_eq!(
        hits.iter()
            .map(ContactImpact::sound_seed)
            .collect::<Vec<_>>(),
        other
            .take_contact_impacts()
            .iter()
            .map(ContactImpact::sound_seed)
            .collect::<Vec<_>>()
    );
}
#[test]
fn contact_audio_policy_rejects_invalid_or_implicit_rules() {
    let mut l = bri_fx_runtime::testing::library();
    l.particles[0].contact_audio = Some(contact_audio_policy());
    assert!(l.validate().is_err(), "audio requires contact geometry");
    for bad in [f32::NAN, -1., 2.] {
        let mut p = contact_audio_policy();
        p.max_lateness_seconds = bad;
        assert!(!p.valid());
    }
    assert!(
        serde_json::from_str::<ContactAudio>(r#"{"surfaces":{}}"#).is_err(),
        "eligibility is required"
    );
}

#[test]
fn contact_audio_repeat_limits_are_authored_and_do_not_change_motion() {
    let make = |cooldown, cap| {
        world(fixture(|l| {
            l.emitters[0].speed = 2.;
            l.particles[0].gravity = 0.;
            l.particles[0].contact = Some(ParticleContact {
                restitution: 1.,
                friction: 0.,
                rest_speed: 0.,
                die_on_contact: false,
                die_at_rest: false,
                stop_animation_at_rest: false,
            });
            let mut policy = contact_audio_policy();
            policy.cooldown_seconds = cooldown;
            policy.max_impacts = cap;
            l.particles[0].contact_audio = Some(policy);
        }))
    };
    let walls = |a: Vec3, b: Vec3| {
        if b.y <= 0. && a.y > 0. {
            audio_floor(a, b)
        } else if b.y >= 0.1 && a.y < 0.1 {
            Some(ContactHit {
                fraction: (0.1 - a.y) / (b.y - a.y),
                normal: Vec3::NEG_Y,
            })
        } else {
            None
        }
    };
    let mut all = make(0., 100);
    let mut cooldown = make(0.12, 100);
    let mut capped = make(0., 2);
    for w in [&mut all, &mut cooldown, &mut capped] {
        audio_burst(w, 1);
        w.advance_with_contacts(0.4, Vec3::ZERO, walls).unwrap();
    }
    let every = all.take_contact_impacts();
    let spaced = cooldown.take_contact_impacts();
    let limited = capped.take_contact_impacts();
    assert!(every.len() >= 6);
    assert!(spaced.len() < every.len());
    assert!(
        spaced
            .windows(2)
            .all(|h| h[0].lateness - h[1].lateness >= 0.12)
    );
    assert_eq!(
        limited.iter().map(|h| h.ordinal).collect::<Vec<_>>(),
        vec![1, 2]
    );
    for w in [&cooldown, &capped] {
        assert_eq!(
            all.snapshot(&camera()).particles[0].position,
            w.snapshot(&camera()).particles[0].position
        );
    }
}
#[test]
fn contact_audio_overdue_emissions_reject_stale_preaged_impacts() {
    let mut w = world(fixture(|l| {
        l.emitters[0].speed = 2.;
        l.emitters[0].period = 0.05;
        l.particles[0].contact = Some(ParticleContact {
            restitution: 0.,
            friction: 1.,
            rest_speed: 0.,
            die_on_contact: true,
            die_at_rest: false,
            stop_animation_at_rest: false,
        });
        let mut p = contact_audio_policy();
        p.max_lateness_seconds = 0.04;
        l.particles[0].contact_audio = Some(p);
    }));
    w.start_emitter(
        "emitter",
        SourceTransform {
            position: Vec3::Y * 0.05,
            rotation: glam::Quat::from_rotation_z(std::f32::consts::PI),
            ..Default::default()
        },
        SourceOptions::default(),
    )
    .unwrap();
    w.advance_with_contacts(0.4, Vec3::ZERO, audio_floor)
        .unwrap();
    let hits = w.take_contact_impacts();
    assert_eq!(
        hits.len(),
        1,
        "historical emissions must not burst into sound"
    );
    assert!(hits[0].lateness <= 0.04);
}

#[test]
fn contact_audio_seed_survives_particle_vector_compaction_and_undrained_replacement() {
    let mut compacted = audio_world(Some(contact_audio_policy()), true);
    let mut retained = audio_world(Some(contact_audio_policy()), true);
    for w in [&mut compacted, &mut retained] {
        w.burst(
            "emitter",
            SourceTransform {
                position: Vec3::Y * 100.,
                ..Default::default()
            },
            SourceOptions::default(),
            1,
        )
        .unwrap();
        audio_burst(w, 1);
    }
    compacted.stop(EffectHandle(1), StopMode::Immediate);
    for w in [&mut compacted, &mut retained] {
        w.advance_with_contacts(0.1, Vec3::ZERO, audio_floor)
            .unwrap();
    }
    let hits = compacted.take_contact_impacts();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].sound_seed(),
        retained.take_contact_impacts()[0].sound_seed()
    );
    audio_burst(&mut compacted, 1);
    compacted
        .advance_with_contacts(0.1, Vec3::ZERO, audio_floor)
        .unwrap();
    compacted.advance(0., Vec3::ZERO).unwrap();
    assert!(
        compacted.take_contact_impacts().is_empty(),
        "the next advance discards unconsumed reports"
    );
    audio_burst(&mut compacted, 1);
    compacted
        .advance_with_contacts(0.1, Vec3::ZERO, audio_floor)
        .unwrap();
    compacted.teardown();
    assert!(compacted.take_contact_impacts().is_empty());
}
