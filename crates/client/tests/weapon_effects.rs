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
        max_bounces: 0,
        children: Vec::new(),
        aura: None,
        slow: None,
        fixed_damage: false,
    };
    Arc::new(Pack {
        effects: Default::default(),
        schema_version: bri_weapons::SCHEMA,
        id: "test".into(),
        items: BTreeMap::new(),
        images: BTreeMap::new(),
        projectiles: BTreeMap::from([("projectile".into(), p)]),
        external_projectiles: Default::default(),
        damage_types: BTreeMap::new(),
        explosions: BTreeMap::new(),
        sounds: Default::default(),
        definitions: vec![],
        resources: vec![],
        diagnostics: vec![],
        bindings: vec![],
    })
}
fn view() -> WeaponView {
    WeaponView {
        projectiles: vec![Projectile {
            paint: None,
            heading: None,
            bounces: 0,
            spawned: 0,
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
    let pack = EffectsPack::load(bri_package::testing::pack_dir(
        &root.join("content"),
        "effects_runtime",
    ))?;
    let weapons = Arc::new(Pack::from_json(&std::fs::read(
        bri_package::testing::pack_dir(&root.join("content"), "weapons").join("weapons.json"),
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

/// An Add-On explosion with a light (the Mini-Nuke's: its light is named
/// after it) is played by its explosion's name, its emitters and all. The
/// light took the name once Add-On lights were bound by datablock name, so
/// the explosion cue started only the light's "missing duration" note.
#[test]
fn an_add_on_explosion_with_a_light_plays_by_its_name() -> Result<()> {
    let base = fixture(false);
    let particle = Particle {
        id: "kit:particle/spark".into(),
        ..base.library.particles[0].clone()
    };
    let emitter = Emitter {
        id: "kit:emitter/flash".into(),
        name: String::new(),
        particles: vec![particle.id.clone()],
        ..base.library.emitters[0].clone()
    };
    let light = Light {
        id: "kit:explosion-light/boom".into(),
        name: String::new(),
        enabled: true,
        color: [1.0, 0.8, 0.2],
        brightness: 1.0,
        radius: 60.0,
        color_curves: None,
        brightness_curve: None,
        radius_curve: None,
        flare: None,
    };
    let mut pack = (*weapons()).clone();
    pack.effects = bri_weapons::PackEffects {
        particles: vec![particle],
        emitters: vec![emitter],
        lights: vec![light.clone()],
        explosions: vec![bri_weapons::ExplosionEffect {
            id: "kit:explosion/boom".into(),
            lifetime: 0.25,
            emitters: vec!["kit:emitter/flash".into()],
            light: Some(light.id.clone()),
            burst: Some(("kit:emitter/flash".into(), 10, 0.2)),
        }],
    };
    pack.validate()?;
    let mut fx = WeaponEffects::new(base, Arc::new(pack), EffectsLimits::default())?;
    fx.cues(&[cue(1, "boom", 0.)], pose)?;
    fx.advance(0.1, Vec3::ZERO, pose)?;
    assert_eq!(
        fx.diagnostics.missing_bindings, 0,
        "{:?}",
        fx.diagnostics.messages
    );
    assert!(
        fx.world().particle_count() >= 10,
        "{} particles",
        fx.world().particle_count()
    );
    Ok(())
}

#[test]
fn an_add_on_pack_brings_its_own_emitters_and_explosions() -> Result<()> {
    let base = fixture(false);
    let particle = Particle {
        id: "kit:particle/spark".into(),
        ..base.library.particles[0].clone()
    };
    let emitter = Emitter {
        id: "kit:emitter/flash".into(),
        name: String::new(),
        particles: vec![particle.id.clone()],
        ..base.library.emitters[0].clone()
    };
    let mut lost = particle.clone();
    lost.id = "kit:particle/lost".into();
    lost.texture = "not-in-the-base-game".into();
    let mut pack = (*weapons()).clone();
    pack.effects = bri_weapons::PackEffects {
        particles: vec![particle, lost.clone()],
        emitters: vec![
            emitter,
            Emitter {
                id: "kit:emitter/lost".into(),
                particles: vec![lost.id.clone()],
                ..base.library.emitters[0].clone()
            },
        ],
        lights: vec![],
        explosions: vec![bri_weapons::ExplosionEffect {
            id: "kit:explosion/boom".into(),
            lifetime: 0.2,
            emitters: vec!["kit:emitter/flash".into(), "kit:emitter/lost".into()],
            light: None,
            burst: Some(("kit:emitter/flash".into(), 4, 0.5)),
        }],
    };
    pack.validate()?;
    let fx = WeaponEffects::new(base, Arc::new(pack), EffectsLimits::default())?;
    // States and trails name the emitter by id; the explosion is found by
    // its explosion's name, as the base game's are.
    assert!(fx.resolves("kit:emitter/flash"));
    // Another Add-On names it by its datablock name, as v20's names are
    // global (Tier 2's tracers trail Tier 1's pistolTrailEmitter); one left
    // out answers to no name.
    assert!(fx.resolves("Flash"));
    assert!(!fx.resolves("lost"));
    assert!(fx.resolves("boom"));
    assert!(fx.resolves("kit:explosion/boom"));
    // A particle drawing a texture the game lacks is left out, with the
    // emitter using it, and said so.
    assert!(!fx.resolves("kit:emitter/lost"));
    assert!(
        fx.diagnostics
            .messages
            .iter()
            .any(|m| m.contains("kit:particle/lost")),
        "{:?}",
        fx.diagnostics.messages
    );
    // The base game's names still win.
    assert!(fx.resolves("hit"));
    Ok(())
}

/// v20 draws the game's cloud for a particle whose texture does not load
/// (`ParticleData` preload, 0x558c60), so an explosion naming a texture the
/// game lacks (the Mini-Nuke's `base/data/particles/star`) keeps every part.
#[test]
fn a_particle_whose_texture_is_missing_draws_the_cloud() -> Result<()> {
    let cloud = bri_client::weapon_effects::MISSING_PARTICLE_TEXTURE;
    let fixture = fixture(false);
    let mut library = fixture.library.clone();
    library.textures.insert(cloud.into(), "cloud.png".into());
    let mut textures: Vec<_> = fixture
        .textures
        .iter()
        .map(|t| TextureImage {
            id: t.id.clone(),
            width: t.width,
            height: t.height,
            rgba: t.rgba.clone(),
        })
        .collect();
    textures.push(TextureImage {
        id: cloud.into(),
        width: 1,
        height: 1,
        rgba: vec![128; 4],
    });
    let base = EffectsPack::from_parts(library, fixture.manifest.clone(), textures)?;
    let lost = Particle {
        id: "kit:particle/star".into(),
        texture: "base/data/particles/star".into(),
        ..base.library.particles[0].clone()
    };
    let mut pack = (*weapons()).clone();
    pack.effects = bri_weapons::PackEffects {
        particles: vec![lost.clone()],
        emitters: vec![Emitter {
            id: "kit:emitter/star".into(),
            name: String::new(),
            particles: vec![lost.id.clone()],
            ..base.library.emitters[0].clone()
        }],
        lights: vec![],
        explosions: vec![bri_weapons::ExplosionEffect {
            id: "kit:explosion/nuke".into(),
            lifetime: 0.2,
            emitters: vec!["kit:emitter/star".into()],
            light: None,
            burst: Some(("kit:emitter/star".into(), 4, 0.5)),
        }],
    };
    pack.validate()?;
    let fx = WeaponEffects::new(base, Arc::new(pack), EffectsLimits::default())?;
    assert!(
        fx.resolves("kit:emitter/star"),
        "{:?}",
        fx.diagnostics.messages
    );
    let drawn = fx
        .world()
        .pack()
        .library
        .particles
        .iter()
        .find(|p| p.id == lost.id)
        .expect("the particle is kept");
    assert_eq!(drawn.texture, cloud);
    let explosion = fx
        .world()
        .pack()
        .manifest
        .composites
        .iter()
        .find(|c| c.id == "kit:explosion/nuke")
        .unwrap();
    assert_eq!(explosion.emitters, ["kit:emitter/star"]);
    assert!(explosion.burst.is_some());
    assert!(
        !fx.diagnostics
            .messages
            .iter()
            .any(|m| m.contains("draws nothing") || m.contains("shows less")),
        "{:?}",
        fx.diagnostics.messages
    );
    Ok(())
}

/// An Add-On particle may draw the Add-On's own texture: the effects take
/// it from the item presentation's images, fitted within the Add-On limit.
#[test]
fn an_add_on_particle_draws_its_own_texture() -> Result<()> {
    let base = fixture(false);
    let key = "add-ons/weapon_kit/spark.png";
    let particle = Particle {
        id: "kit:particle/spark".into(),
        texture: key.into(),
        ..base.library.particles[0].clone()
    };
    let mut pack = (*weapons()).clone();
    pack.effects = bri_weapons::PackEffects {
        particles: vec![particle.clone()],
        emitters: vec![Emitter {
            id: "kit:emitter/spark".into(),
            name: String::new(),
            particles: vec![particle.id.clone()],
            ..base.library.emitters[0].clone()
        }],
        lights: vec![],
        explosions: vec![],
    };
    pack.validate()?;
    let pack = Arc::new(pack);
    // Without the image the particle is left out.
    let without = WeaponEffects::new(base.clone(), pack.clone(), EffectsLimits::default())?;
    assert!(!without.resolves("kit:emitter/spark"));
    let image = bri_render::scene::SceneImage {
        label: key.into(),
        width: 512,
        height: 300,
        rgba: vec![200; 512 * 300 * 4],
        srgb: false,
    };
    let fx = WeaponEffects::with_textures(base, pack, EffectsLimits::default(), |k| {
        (k == key).then_some(&image)
    })?;
    assert!(fx.resolves("kit:emitter/spark"));
    let texture = fx
        .world()
        .pack()
        .textures
        .iter()
        .find(|t| t.id == key)
        .expect("the Add-On's texture joins the effects");
    let side = bri_client::weapon_effects::ADD_ON_TEXTURE_SIDE;
    assert_eq!((texture.width, texture.height), (side, 150));
    assert_eq!(texture.rgba.len(), (side * 150 * 4) as usize);
    Ok(())
}

/// A particle may draw the game's own interface art (`base/client/ui/...`,
/// as the Duplorcator's brick icon sparks do): it comes from the UI pack,
/// whatever the case the Add-On wrote, and the particle draws.
#[test]
fn an_add_on_particle_draws_an_interface_picture() -> Result<()> {
    let base = fixture(false);
    let named = "base/client/ui/brickIcons/1x1";
    let particle = Particle {
        id: "kit:particle/icon".into(),
        texture: named.into(),
        ..base.library.particles[0].clone()
    };
    let mut pack = (*weapons()).clone();
    pack.effects = bri_weapons::PackEffects {
        particles: vec![particle.clone()],
        emitters: vec![Emitter {
            id: "kit:emitter/icon".into(),
            name: String::new(),
            particles: vec![particle.id.clone()],
            ..base.library.emitters[0].clone()
        }],
        lights: vec![],
        explosions: vec![],
    };
    pack.validate()?;
    let dir = tempfile::tempdir()?;
    image::RgbaImage::from_pixel(4, 2, image::Rgba([10, 20, 30, 255]))
        .save(dir.path().join("1x1.png"))?;
    let mut ui = bri_ui::schema::UiPack::default();
    ui.images.insert(
        "base/client/ui/brickicons/1x1".into(),
        bri_ui::schema::ImageEntry {
            file: "1x1.png".into(),
            width: 4,
            height: 2,
            sha256: String::new(),
            source: "base/client/ui/brickIcons/1x1.png".into(),
        },
    );
    let ui = bri_ui::pack::Pack::from_parts(ui, dir.path().to_path_buf());
    let interface = bri_client::weapon_effects::interface_textures(&pack.effects, &ui);
    let fx = WeaponEffects::with_textures(base, Arc::new(pack), EffectsLimits::default(), |k| {
        interface.get(k)
    })?;
    assert!(
        fx.resolves("kit:emitter/icon"),
        "{:?}",
        fx.diagnostics.messages
    );
    assert!(
        !fx.diagnostics
            .messages
            .iter()
            .any(|m| m.contains("draws nothing")),
        "{:?}",
        fx.diagnostics.messages
    );
    let texture = fx
        .world()
        .pack()
        .textures
        .iter()
        .find(|t| t.id == named)
        .unwrap();
    assert_eq!((texture.width, texture.height), (4, 2));
    Ok(())
}

/// A held image's rope (`Image::rope`) is drawn by its projectile's trail
/// swept along the whole rope each frame, as densely as that projectile
/// flying it would lay it, and stops when the rope goes.
#[test]
fn held_ropes_lay_their_trail_along_the_rope() -> Result<()> {
    use bri_client::weapon_effects::HeldRope;
    let mut pack = (*weapons()).clone();
    pack.images.insert(
        "rope-image".into(),
        bri_weapons::Image {
            id: "rope-image".into(),
            rope: Some(bri_weapons::Rope {
                projectile: "projectile".into(),
                speed: 10.,
            }),
            ..Default::default()
        },
    );
    let mut fx = WeaponEffects::new(fixture(false), Arc::new(pack), EffectsLimits::default())?;
    let rope = HeldRope {
        owner: 1,
        image: "rope-image".into(),
        from: Vec3::ZERO,
        to: Vec3::X * 10.,
    };
    // A frame of 0.1 s: the projectile at 10 a second would cross the
    // 10-long rope in 1 s, emitting every 0.01 s: 100 particles along it.
    fx.sync_ropes(std::slice::from_ref(&rope), 0.1)?;
    fx.advance(0.1, Vec3::ZERO, |_| None)?;
    let particles = fx.world().snapshot(&camera()).particles;
    assert!((90..=110).contains(&particles.len()), "{}", particles.len());
    let xs: Vec<f32> = particles.iter().map(|p| p.position.x).collect();
    assert!(xs.iter().any(|x| *x < 1.) && xs.iter().any(|x| *x > 9.));
    assert!(
        particles.iter().all(|p| p.position.y.abs() < 0.01
            && p.position.z.abs() < 0.01
            && p.position.x > -0.01
            && p.position.x < 10.01),
        "on the rope"
    );
    // The next frame sweeps back along it.
    fx.sync_ropes(std::slice::from_ref(&rope), 0.1)?;
    fx.advance(0.1, Vec3::ZERO, |_| None)?;
    assert!(fx.world().snapshot(&camera()).particles.len() > 150);
    // An image without a rope, or no rope: nothing more is laid.
    let plain = HeldRope {
        image: "other".into(),
        ..rope
    };
    fx.sync_ropes(&[plain], 0.1)?;
    assert_eq!(fx.world().source_count(), 0);
    Ok(())
}

#[test]
fn a_trail_carried_through_a_portal_does_not_streak_between_the_two() -> Result<()> {
    use bri_content::passage::{Passage, Passages};
    // In through x = 0.5 going +x, out twenty units on.
    let passages = Passages {
        list: vec![Passage {
            brick: 1,
            centre: Vec3::new(0.5, 0.0, 0.0),
            normal: Vec3::NEG_X,
            u: Vec3::Z,
            v: Vec3::Y,
            half: glam::Vec2::new(1.0, 1.5),
            carry: glam::Affine3A::from_translation(Vec3::X * 20.0),
        }],
        closed: vec![],
    };
    let mut fx = WeaponEffects::new(fixture(false), weapons(), EffectsLimits::default())?;
    fx.set_passages(&passages);
    let mut v = view();
    fx.sync(&v)?;
    fx.advance(0.05, Vec3::ZERO, pose)?;
    v.projectiles[0].position = Vec3::X * 21.0;
    fx.sync(&v)?;
    fx.advance(0.05, Vec3::ZERO, pose)?;
    let particles = fx.world().snapshot(&camera()).particles;
    assert!(particles.iter().any(|p| p.position.x < 0.5));
    assert!(particles.iter().any(|p| p.position.x > 20.5));
    assert!(
        particles
            .iter()
            .all(|p| p.position.x < 1.0 || p.position.x > 20.0),
        "streaked between the portals"
    );
    Ok(())
}

/// v20's image light (`hasLight`, `ConstantLight`): a worn image lights
/// the world around it, in the paint colour it is worn in, and goes out
/// with it.
#[test]
fn a_worn_image_with_a_light_glows_in_its_paint_and_goes_out_with_it() -> Result<()> {
    let mut pack = (*weapons()).clone();
    pack.images.insert(
        "flag".into(),
        bri_weapons::Image {
            id: "flag".into(),
            paint_tint: true,
            light: Some(bri_weapons::ImageLight {
                radius: 20.,
                color: [1.; 3],
            }),
            ..Default::default()
        },
    );
    let mut fx = WeaponEffects::new(fixture(false), Arc::new(pack), EffectsLimits::default())?;
    fx.set_palette(&[[1., 1., 1., 1.], [0., 0., 1., 1.]]);
    let worn = |paint| WeaponView {
        images: BTreeMap::from([(
            7,
            vec![bri_sim::session::MountedImage {
                image: "flag".into(),
                state: "Idle".into(),
                hand: 3,
                paint,
            }],
        )]),
        ..Default::default()
    };
    let at = Vec3::new(2., 1., 0.);
    fx.sync_image_lights(&worn(Some(1)), |owner, hand| {
        (owner == 7 && hand == 3).then_some(at)
    })?;
    fx.advance(0.01, Vec3::ZERO, pose)?;
    let lights = fx.world().snapshot(&camera()).lights;
    let [light] = &lights[..] else {
        panic!("one light: {}", lights.len());
    };
    assert_eq!(light.position, at);
    assert_eq!(light.radius, 20.);
    assert_eq!(light.color, Vec3::new(0., 0., 1.), "blue, as it is worn");
    fx.sync_image_lights(&WeaponView::default(), |_, _| Some(at))?;
    fx.advance(0.01, Vec3::ZERO, pose)?;
    assert!(fx.world().snapshot(&camera()).lights.is_empty());
    Ok(())
}
