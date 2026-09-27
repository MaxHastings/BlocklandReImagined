use anyhow::Result;
use bri_client::actor_effects::{ActorEffects, Anchor};
use bri_content::effects::*;
use bri_fx_runtime::{pack::TextureImage, *};
use bri_sim::presentation::{Cue, CueKind};
use bri_weapons::{Image, Pack, State};
use glam::{Mat4, Vec3};
use std::{collections::BTreeMap, sync::Arc};

fn emitter(id: &str, lifetime: f32) -> Emitter {
    Emitter {
        id: id.into(),
        name: String::new(),
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
        lifetime,
        lifetime_variance: 0.,
        orient: false,
        orient_on_velocity: false,
        override_advance: false,
        use_emitter_colors: false,
        use_emitter_sizes: false,
        use_placement_velocity: false,
        node_time_scale: 1.,
        point_node_time_scale: 1.,
    }
}
fn effects() -> Arc<EffectsPack> {
    EffectsPack::from_parts(
        Library {
            schema_version: 1,
            textures: BTreeMap::from([("texture".into(), "texture.png".into())]),
            lights: vec![],
            particles: vec![Particle {
                id: "particle".into(),
                texture: "texture".into(),
                alpha_blend: true,
                lifetime: 0.5,
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
            emitters: vec![
                emitter("v20/emitter/loveemitter", 0.),
                emitter("v20/emitter/playerburnemitter", 0.),
                emitter("v20/emitter/playerjetemitter", 0.),
                emitter("v20/emitter/vehicleburnemitter", 0.),
                emitter("v20/emitter/vehiclesplashemitter", 0.1),
                emitter("v20/emitter/vehiclesplashmistemitter", 0.25),
            ],
        },
        Manifest {
            schema_version: 1,
            library_sha256: String::new(),
            textures: BTreeMap::new(),
            emitter_alpha: BTreeMap::new(),
            bindings: vec![],
            composites: vec![],
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
fn state(name: &str, ticks: u32, emitter: &str, seconds: f32, timeout: Option<usize>) -> State {
    State {
        name: name.into(),
        ticks,
        wait: true,
        allow_change: true,
        timeout,
        emitter: emitter.into(),
        emitter_seconds: seconds,
        ..Default::default()
    }
}
fn image(name: &str, states: Vec<State>) -> (String, Image) {
    let id = bri_weapons::native_id("image", name);
    (
        id.clone(),
        Image {
            id,
            name: name.into(),
            model: String::new(),
            projectile: None,
            mount_point: 5,
            offset: [0.; 3],
            eye_offset: [0.; 3],
            source_rotation_degrees: [0.; 3],
            correct_muzzle: false,
            melee: false,
            color: [1.; 4],
            color_shift: false,
            arm_ready: false,
            casing: String::new(),
            min_shot_ticks: 0,
            states,
        },
    )
}
/// The recovered LoveImage and PlayerBurnImage state tables.
fn weapons() -> Arc<Pack> {
    Arc::new(Pack {
        schema_version: bri_weapons::SCHEMA,
        id: "test".into(),
        items: BTreeMap::new(),
        images: BTreeMap::from([
            image(
                "LoveImage",
                vec![
                    state("Ready", 2, "", 0., Some(1)),
                    state("FireA", 42, "LoveEmitter", 0.35, Some(2)),
                    state("Done", 0, "", 0., None),
                ],
            ),
            image(
                "PlayerBurnImage",
                vec![
                    state("Ready", 2, "", 0., Some(1)),
                    state("FireA", 6, "PlayerBurnEmitter", 5., Some(2)),
                    state("FireB", 6, "PlayerBurnEmitter", 0.05, Some(1)),
                ],
            ),
        ]),
        projectiles: BTreeMap::new(),
        damage_types: BTreeMap::new(),
        explosions: BTreeMap::new(),
        definitions: vec![],
        resources: vec![],
        diagnostics: vec![],
    })
}
fn cue(id: u64, kind: CueKind) -> Cue {
    Cue {
        id,
        tick: id,
        kind,
        position: [0.; 3],
    }
}
fn head(anchor: Anchor) -> Option<Mat4> {
    matches!(anchor, Anchor::Actor { actor: 7, mount: 5 })
        .then(|| Mat4::from_translation(Vec3::new(1., 2., 3.)))
}

#[test]
fn emote_image_emits_for_its_state_time_then_unmounts() -> Result<()> {
    let mut fx = ActorEffects::new(effects(), weapons(), Default::default())?;
    fx.cue(&cue(
        1,
        CueKind::Emote {
            actor: 7,
            name: "love".into(),
        },
    ));
    // A replayed or older cue is ignored.
    fx.cue(&cue(
        1,
        CueKind::Emote {
            actor: 7,
            name: "love".into(),
        },
    ));
    assert_eq!(fx.image_count(), 1);
    fx.advance(0.1, head, &[], &[])?;
    assert!(fx.world().particle_count() > 0);
    for _ in 0..20 {
        fx.advance(0.1, head, &[], &[])?;
    }
    assert_eq!(fx.image_count(), 0, "Done unmounts after the emitter time");
    assert!(
        fx.diagnostics.messages.is_empty(),
        "{:?}",
        fx.diagnostics.messages
    );
    Ok(())
}

#[test]
fn burning_loops_until_cleared_and_emotes_replace_it() -> Result<()> {
    let mut fx = ActorEffects::new(effects(), weapons(), Default::default())?;
    fx.cue(&cue(
        1,
        CueKind::Burn {
            actor: 7,
            seconds: 1.0,
        },
    ));
    for _ in 0..8 {
        fx.advance(0.1, head, &[], &[])?;
    }
    assert_eq!(fx.image_count(), 1, "burning outlives its first state");
    for _ in 0..4 {
        fx.advance(0.1, head, &[], &[])?;
    }
    assert_eq!(fx.image_count(), 0, "clearBurn after the burn time");
    fx.cue(&cue(
        2,
        CueKind::Burn {
            actor: 7,
            seconds: 5.0,
        },
    ));
    fx.cue(&cue(
        3,
        CueKind::Emote {
            actor: 7,
            name: "love".into(),
        },
    ));
    assert_eq!(fx.image_count(), 1, "slot 3 holds one image");
    // A body that is no longer presented drops its image.
    fx.advance(0.1, |_| None, &[], &[])?;
    assert_eq!(fx.image_count(), 0);
    Ok(())
}

#[test]
fn jets_burning_vehicles_and_splashes_follow_their_sources() -> Result<()> {
    let mut fx = ActorEffects::new(effects(), weapons(), Default::default())?;
    let feet = [Mat4::IDENTITY, Mat4::from_translation(Vec3::X)];
    let wreck = Mat4::from_translation(Vec3::new(5., 0., 5.));
    fx.advance(0.1, head, &[(7, feet, Vec3::ZERO)], &[(9, wreck)])?;
    assert_eq!((fx.jet_count(), fx.burning_count()), (2, 1));
    assert!(fx.world().particle_count() > 0);
    fx.advance(0.1, head, &[], &[])?;
    assert_eq!((fx.jet_count(), fx.burning_count()), (0, 0));
    fx.cue(&cue(
        1,
        CueKind::VehicleEffect {
            vehicle: 9,
            effect: "vehicleSplash".into(),
            active: true,
        },
    ));
    let at_vehicle = |a| matches!(a, Anchor::Vehicle { vehicle: 9 }).then_some(wreck);
    fx.advance(0.05, at_vehicle, &[], &[])?;
    assert!(fx.world().source_count() >= 2);
    for _ in 0..10 {
        fx.advance(0.1, at_vehicle, &[], &[])?;
    }
    assert_eq!(fx.world().source_count(), 0, "splash emitters are finite");
    assert!(
        fx.diagnostics.messages.is_empty(),
        "{:?}",
        fx.diagnostics.messages
    );
    Ok(())
}

#[test]
#[ignore = "requires generated effects-runtime-pack-002 and weapons-pack-007; CPU only"]
fn original_emote_pain_burn_and_vehicle_images_resolve() -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let pack = EffectsPack::load(root.join("effects-runtime-pack-002"))?;
    let weapons = Arc::new(Pack::from_json(&std::fs::read(
        root.join("weapons-pack-007/weapons.json"),
    )?)?);
    let mut fx = ActorEffects::new(pack, weapons, Default::default())?;
    let mut id = 0;
    let mut next = |kind| {
        id += 1;
        cue(id, kind)
    };
    for name in ["love", "hate", "confusion"] {
        fx.cue(&next(CueKind::Emote {
            actor: 7,
            name: name.into(),
        }));
        fx.advance(0.05, head, &[], &[])?;
    }
    for level in [5., 30., 50.] {
        fx.cue(&next(CueKind::Pain {
            actor: 7,
            level,
            cry: true,
        }));
        fx.advance(0.05, head, &[], &[])?;
    }
    fx.cue(&next(CueKind::Burn {
        actor: 7,
        seconds: 2.,
    }));
    fx.cue(&next(CueKind::Water {
        actor: 7,
        entered: true,
        speed: 12.,
    }));
    let muzzle = |a| match a {
        Anchor::Actor { .. } => head(a),
        _ => Some(Mat4::IDENTITY),
    };
    for effect in [
        "TankSmokeImage",
        "CannonSmokeImage",
        "CannonFuseImage",
        "vehicleSplash",
    ] {
        fx.cue(&next(CueKind::VehicleEffect {
            vehicle: 9,
            effect: effect.into(),
            active: true,
        }));
    }
    fx.cue(&next(CueKind::VehicleEffect {
        vehicle: 9,
        effect: "CannonFuseImage".into(),
        active: false,
    }));
    fx.advance(
        0.1,
        muzzle,
        &[(7, [Mat4::IDENTITY; 2], Vec3::ZERO)],
        &[(9, Mat4::IDENTITY)],
    )?;
    assert!(fx.world().particle_count() > 0);
    assert!(
        fx.diagnostics.messages.is_empty(),
        "{:?}",
        fx.diagnostics.messages
    );
    assert_eq!(fx.diagnostics.capacity_rejections, 0);
    Ok(())
}
