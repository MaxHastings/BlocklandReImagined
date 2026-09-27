use anyhow::Result;
use bri_client::weapon_effects::{HostRequest, WeaponEffects};
use bri_content::effects::*;
use bri_fx_runtime::{
    pack::{Composite, TextureImage},
    *,
};
use bri_sim::{
    presentation::{Cue, CueKind},
    session::WeaponView,
};
use bri_weapons::{ActorId, Pack, Projectile, ProjectileDef};
use glam::{Mat4, Vec3};
use std::{collections::BTreeMap, sync::Arc};

fn fixture(finite: bool) -> Arc<EffectsPack> {
    EffectsPack::from_parts(
        Library {
            schema_version: 1,
            textures: BTreeMap::from([("texture".into(), "texture.png".into())]),
            lights: vec![],
            particles: vec![Particle {
                id: "particle".into(),
                texture: "texture".into(),
                alpha_blend: true,
                lifetime: 2.,
                lifetime_variance: 0.,
                drag: 0.,
                wind: 0.,
                gravity: 0.,
                inherited_velocity: 0.,
                acceleration: 0.,
                spin_degrees: 0.,
                random_spin: [0.; 2],
                keys: vec![
                    ParticleKey {
                        time: 0.,
                        color: [1.; 4],
                        size: 1.,
                    },
                    ParticleKey {
                        time: 1.,
                        color: [1.; 4],
                        size: 1.,
                    },
                ],
            }],
            emitters: vec![Emitter {
                id: "v20/emitter/trail".into(),
                name: "Trail UI".into(),
                particles: vec!["particle".into()],
                period: 0.01,
                period_variance: 0.,
                speed: 0.,
                speed_variance: 0.,
                offset: 0.,
                offset_variance: 0.,
                theta_degrees: [0.; 2],
                phi_rate_degrees: 0.,
                phi_variance_degrees: 0.,
                lifetime: if finite { 0.05 } else { 0. },
                lifetime_variance: 0.,
                orient: false,
                orient_on_velocity: false,
                override_advance: false,
                use_emitter_colors: false,
                use_emitter_sizes: false,
                use_placement_velocity: false,
                node_time_scale: 1.,
                point_node_time_scale: 1.,
            }],
        },
        Manifest {
            schema_version: 1,
            library_sha256: String::new(),
            textures: BTreeMap::new(),
            emitter_alpha: BTreeMap::new(),
            bindings: vec![],
            composites: vec![Composite {
                id: "v20/explosion/hit".into(),
                lifetime: 0.05,
                emitters: vec![],
                light: None,
                burst: Some(("v20/emitter/trail".into(), 3, 0.)),
            }],
            unresolved: vec![],
        },
        vec![TextureImage {
            id: "texture".into(),
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        }],
    )
    .unwrap()
}
fn weapons() -> Arc<Pack> {
    let p = ProjectileDef {
        id: "projectile".into(),
        name: "Projectile".into(),
        model: String::new(),
        speed: 10.,
        inherit: 0.,
        gravity: 0.,
        lifetime_ticks: 120,
        fade_ticks: 120,
        arm_ticks: 0,
        ballistic: false,
        elasticity: 0.,
        friction: 0.,
        damage: 0.,
        damage_type: String::new(),
        radius_damage_type: String::new(),
        impulse: 0.,
        vertical: 0.,
        explode_player: false,
        explode_death: false,
        collide_players: false,
        explosion: Default::default(),
        brick: Default::default(),
        bounce_effect: String::new(),
        stick_effect: String::new(),
        blood_effect: String::new(),
        bounce_angle: 0.,
        min_stick_speed: 0.,
        trail: "Trail".into(),
        sound: String::new(),
        light_radius: 3.,
        light_color: [1., 0.2, 0.1],
        sport_image: None,
        rest_speed: 0.,
    };
    Arc::new(Pack {
        schema_version: bri_weapons::SCHEMA,
        id: "test".into(),
        items: BTreeMap::new(),
        images: BTreeMap::new(),
        projectiles: BTreeMap::from([("projectile".into(), p)]),
        damage_types: BTreeMap::new(),
        explosions: BTreeMap::new(),
        definitions: vec![],
        resources: vec![],
        diagnostics: vec![],
    })
}
fn view() -> WeaponView {
    WeaponView {
        projectiles: vec![Projectile {
            id: 1,
            definition: "projectile".into(),
            source: ActorId(1),
            position: Vec3::ZERO,
            velocity: Vec3::X * 10.,
            scale: 1.,
            age: 0,
            bounced: false,
            stuck: false,
            origin: Vec3::ZERO,
            was_thrown: false,
        }],
        ..Default::default()
    }
}
fn cue(id: u64, definition: &str, seconds: f32) -> Cue {
    // JSON permits forward-compatible additions to native cue pose metadata.
    serde_json::from_value(serde_json::json!({"id":id,"tick":1,"position":[0.,0.,0.],"kind":{"WeaponEffect":{
        "source":{"Actor":1},"definition":definition,"node":if seconds > 0. {"muzzleNode"} else {""},"seconds":seconds,
        "image":null,"hand":null,"direction":null,"scale":1.}}})).unwrap()
}
fn pose(_: &Cue) -> Option<SourceTransform> {
    Some(SourceTransform::default())
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
fn projectile_update_late_join_remove_and_reset() -> Result<()> {
    let mut fx = WeaponEffects::new(fixture(false), weapons(), EffectsLimits::default())?;
    let mut v = view();
    fx.sync(&v)?;
    v.projectiles[0].position = Vec3::X;
    fx.sync(&v)?;
    fx.advance(0.1, Vec3::ZERO, pose)?;
    let frame = fx.world().snapshot(&camera());
    assert_eq!(fx.attachment_count(), 2);
    assert_eq!(frame.lights[0].position, Vec3::X);
    assert_eq!(frame.lights[0].radius, 3.);
    assert!(
        frame
            .particles
            .iter()
            .any(|p| p.position.x > 0.1 && p.position.x < 0.9)
    );
    let mut late = WeaponEffects::new(fixture(false), weapons(), EffectsLimits::default())?;
    late.reset(50);
    late.sync(&v)?;
    late.cues(&[cue(49, "hit", 0.)], pose)?;
    assert_eq!(
        late.world().particle_count(),
        0,
        "late join replayed a historical burst"
    );
    fx.sync(&WeaponView::default())?;
    assert_eq!(fx.world().source_count(), 0);
    assert!(fx.world().particle_count() > 0);
    fx.advance(3., Vec3::ZERO, pose)?;
    assert_eq!(fx.world().particle_count(), 0);
    late.reset(0);
    assert_eq!(late.world().source_count(), 0);
    assert_eq!(late.attachment_count(), 0);
    Ok(())
}

#[test]
fn projectile_trails_emit_backward_and_impacts_use_received_normal() -> Result<()> {
    let mut pack = fixture(false);
    Arc::get_mut(&mut pack).unwrap().library.emitters[0].speed = 2.;
    let mut trail = WeaponEffects::new(pack.clone(), weapons(), EffectsLimits::default())?;
    trail.sync(&view())?;
    trail.advance(0.1, Vec3::ZERO, pose)?;
    assert!(
        trail
            .world()
            .snapshot(&camera())
            .particles
            .iter()
            .any(|p| p.position.x < -0.1)
    );
    let mut impact = WeaponEffects::new(pack, weapons(), EffectsLimits::default())?;
    let mut c = cue(1, "hit", 0.);
    if let CueKind::WeaponEffect { direction, .. } = &mut c.kind {
        *direction = Some([1., 0., 0.]);
    }
    impact.cues(&[c], |_| None)?;
    impact.advance(0.1, Vec3::ZERO, |_| None)?;
    assert!(
        impact
            .world()
            .snapshot(&camera())
            .particles
            .iter()
            .all(|p| p.position.x > 0.1)
    );
    Ok(())
}

#[test]
fn exact_once_finite_cues_long_frame_and_removed_pose() -> Result<()> {
    let mut fx = WeaponEffects::new(fixture(false), weapons(), EffectsLimits::default())?;
    let c = cue(1, "trail", 0.05);
    fx.cues(&[c.clone(), c.clone()], pose)?;
    fx.cues(std::slice::from_ref(&c), pose)?;
    assert_eq!(fx.timed_count(), 1);
    fx.advance(0.5, Vec3::ZERO, pose)?;
    assert_eq!(fx.world().source_count(), 0);
    assert_eq!(
        fx.world().particle_count(),
        5,
        "finite cap applied after frame, over-emitting"
    );
    fx.cues(&[c], pose)?;
    assert_eq!(fx.world().source_count(), 0);
    fx.cues(&[cue(2, "trail", 1.)], pose)?;
    fx.advance(0.03, Vec3::ZERO, pose)?;
    fx.advance(0., Vec3::ZERO, |_| None)?;
    assert_eq!(fx.world().source_count(), 0);
    assert!(fx.world().particle_count() > 0);
    assert_eq!(fx.diagnostics.accepted_cues, 2);
    assert_eq!(fx.diagnostics.duplicate_cues, 3);
    Ok(())
}

#[test]
fn finite_trails_do_not_restart_and_capacity_is_retried() -> Result<()> {
    let limits = EffectsLimits {
        sources: 1,
        ..Default::default()
    };
    let mut fx = WeaponEffects::new(fixture(true), weapons(), limits)?;
    fx.sync(&view())?;
    assert_eq!(fx.diagnostics.deferred_attachments, 1);
    fx.advance(0.1, Vec3::ZERO, pose)?;
    let emitted = fx.world().diagnostics().emitted;
    fx.sync(&view())?;
    fx.advance(0.1, Vec3::ZERO, pose)?;
    assert_eq!(fx.world().diagnostics().emitted, emitted);
    assert_eq!(
        fx.attachment_count(),
        2,
        "freed finite source should admit deferred light"
    );
    fx.sync(&WeaponView::default())?;
    fx.sync(&view())?;
    assert_eq!(
        fx.world().source_count(),
        1,
        "new identity lifecycle can start the trail"
    );
    Ok(())
}

#[test]
fn malformed_batch_is_atomic_and_missing_bindings_are_bounded() -> Result<()> {
    let mut fx = WeaponEffects::new(fixture(false), weapons(), EffectsLimits::default())?;
    let mut bad = cue(2, "hit", 0.);
    bad.position[0] = f32::NAN;
    assert!(fx.cues(&[cue(1, "hit", 0.), bad], pose).is_err());
    assert_eq!(fx.cue_cursor(), 0);
    assert_eq!(fx.world().particle_count(), 0);
    assert!(
        fx.cues(&[cue(2, "hit", 0.), cue(1, "hit", 0.)], pose)
            .is_err()
    );
    for i in 1..=200 {
        fx.cues(&[cue(i, &format!("missing{i}"), 0.)], pose)?;
    }
    assert_eq!(fx.diagnostics.messages.len(), 128);
    assert_eq!(fx.diagnostics.missing_bindings, 200);
    fx.cues(&[cue(201, "trail", 0.)], pose)?;
    assert_eq!(
        fx.world().source_count(),
        0,
        "infinite transient must reject"
    );
    fx.cues(&[cue(202, "trail", 1.)], |_| None)?;
    assert_eq!(fx.diagnostics.missing_poses, 1);
    Ok(())
}

#[test]
fn shell_animation_queue_is_exact_once_and_bounded() -> Result<()> {
    let mut fx = WeaponEffects::new(fixture(false), weapons(), EffectsLimits::default())?;
    let shell = Cue {
        id: 1,
        tick: 1,
        position: [0.; 3],
        kind: CueKind::WeaponShell {
            actor: 1,
            image: "gun".into(),
            hand: 1,
        },
    };
    fx.cues(std::slice::from_ref(&shell), pose)?;
    fx.cues(&[shell], pose)?;
    assert!(matches!(
        fx.take_host_requests().next(),
        Some(HostRequest::Shell(_))
    ));
    assert_eq!(fx.take_host_requests().count(), 0);
    let batch: Vec<_> = (2..=4097)
        .map(|id| Cue {
            id,
            tick: 1,
            position: [0.; 3],
            kind: CueKind::WeaponAnimation {
                actor: 1,
                thread: 0,
                sequence: "root".into(),
                image_hand: None,
            },
        })
        .collect();
    fx.cues(&batch, pose)?;
    fx.cues(
        &[Cue {
            id: 4098,
            tick: 1,
            position: [0.; 3],
            kind: CueKind::WeaponAnimation {
                actor: 1,
                thread: 0,
                sequence: "root".into(),
                image_hand: None,
            },
        }],
        pose,
    )?;
    assert_eq!(fx.diagnostics.host_queue_drops, 1);
    assert_eq!(fx.take_host_requests().count(), 4096);

    let shell = Cue {
        id: 4099,
        tick: 2,
        position: [0.; 3],
        kind: CueKind::WeaponShell {
            actor: 1,
            image: "gun".into(),
            hand: 0,
        },
    };
    let non_avatar = Cue {
        id: 4100,
        tick: 2,
        position: [0.; 3],
        kind: CueKind::WeaponAnimation {
            actor: 1,
            thread: 0,
            sequence: "root".into(),
            image_hand: None,
        },
    };
    let avatar = Cue {
        id: 4101,
        tick: 2,
        position: [0.; 3],
        kind: CueKind::WeaponAnimation {
            actor: 1,
            thread: 2,
            sequence: "fire".into(),
            image_hand: Some(0),
        },
    };
    fx.cues(&[shell.clone(), non_avatar.clone(), avatar.clone()], pose)?;
    assert_eq!(fx.take_avatar_animation_requests(), vec![avatar]);
    let retained: Vec<_> = fx.take_host_requests().collect();
    assert_eq!(retained.len(), 2);
    assert!(matches!(&retained[0], HostRequest::Shell(cue) if cue == &shell));
    assert!(matches!(&retained[1], HostRequest::Animation(cue) if cue == &non_avatar));
    Ok(())
}

#[test]
fn runtime_lifetime_cap_validates_and_never_extends_authored_lifetime() -> Result<()> {
    let mut world = EffectsWorld::new(fixture(true), EffectsLimits::default(), 3)?;
    let h = world.start_emitter(
        "v20/emitter/trail",
        SourceTransform::default(),
        SourceOptions::default(),
    )?;
    assert!(world.set_remaining_lifetime(h, f32::NAN).is_err());
    world.set_remaining_lifetime(h, 2.)?;
    world.advance(0.5, Vec3::ZERO)?;
    assert_eq!(world.particle_count(), 5);
    assert!(!world.is_active(h));
    assert!(world.set_remaining_lifetime(h, 1.).is_err());
    Ok(())
}

#[test]
#[ignore = "requires original converted packs; CPU only"]
fn actual_native_weapon_bindings_and_effects() -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pack = EffectsPack::load(root.join("content/effects-runtime-pack-002"))?;
    let weapons = Arc::new(Pack::from_json(&std::fs::read(
        root.join("content/weapons-pack-004/weapons.json"),
    )?)?);
    let mut fx = WeaponEffects::new(pack, weapons.clone(), EffectsLimits::default())?;
    fx.cues(
        &[cue(1, "gunExplosion", 0.), cue(2, "gunFlashEmitter", 0.05)],
        pose,
    )?;
    fx.advance(0.04, Vec3::ZERO, pose)?;
    assert_eq!(fx.diagnostics.accepted_cues, 2);
    assert!(fx.world().particle_count() > 0);
    assert_eq!(fx.diagnostics.missing_bindings, 0);
    fx.reset(2);
    assert_eq!(fx.world().particle_count(), 0);
    let mut next = 3;
    let mut tested = std::collections::BTreeSet::new();
    for p in weapons.projectiles.values() {
        let mut v = view();
        v.projectiles[0].definition = p.id.clone();
        fx.sync(&v)?;
        fx.advance(0.02, Vec3::ZERO, pose)?;
        fx.sync(&WeaponView::default())?;
        for name in [
            &p.explosion.effect,
            &p.bounce_effect,
            &p.stick_effect,
            &p.blood_effect,
        ] {
            if !name.is_empty() && tested.insert(name.clone()) {
                fx.cues(&[cue(next, name, 0.)], pose)?;
                next += 1;
            }
        }
    }
    for image in weapons.images.values() {
        for state in &image.states {
            if !state.emitter.is_empty() && tested.insert(state.emitter.clone()) {
                fx.cues(&[cue(next, &state.emitter, state.emitter_seconds)], pose)?;
                next += 1;
            }
        }
    }
    fx.advance(0.1, Vec3::ZERO, pose)?;
    assert_eq!(
        fx.diagnostics.missing_bindings, 0,
        "{:?}",
        fx.diagnostics.messages
    );
    assert_eq!(fx.diagnostics.capacity_rejections, 0);
    assert_eq!(fx.diagnostics.accepted_cues as usize, tested.len());
    assert!(fx.world().particle_count() > 0);
    eprintln!(
        "Native weapon FX: {} projectiles, {} unique transient definitions, {} particles, {} sources",
        weapons.projectiles.len(),
        tested.len(),
        fx.world().particle_count(),
        fx.world().source_count()
    );
    Ok(())
}
