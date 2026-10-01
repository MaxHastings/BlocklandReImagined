use anyhow::Result;
use bri_client::actor_effects::{ActorEffects, Anchor, PlayerLight};
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
            lights: vec![Light {
                id: "v20/light/playerlight".into(),
                name: "Player's Light".into(),
                enabled: true,
                color: [1.; 3],
                brightness: 5.,
                radius: 10.,
                color_curves: None,
                brightness_curve: None,
                radius_curve: None,
                flare: Some(Flare {
                    texture: "texture".into(),
                    color: [1.; 3],
                    third_person: true,
                    constant_size: Some(1.),
                    near_size: 3.,
                    far_size: 0.5,
                    near_distance: 10.,
                    far_distance: 30.,
                    fade_seconds: 0.1,
                    blend_mode: 0,
                    link_color: true,
                    link_size: false,
                }),
            }],
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
                emitter("v20/emitter/playerjetgroundemitter", 0.),
                emitter("v20/emitter/vehicleburnemitter", 0.),
                emitter("v20/emitter/vehiclesplashemitter", 0.1),
                emitter("v20/emitter/vehiclesplashmistemitter", 0.25),
                emitter("v20/emitter/playerfoamdropletsemitter", 0.),
                emitter("v20/emitter/playerfoamemitter", 0.),
                emitter("v20/emitter/playerbubbleemitter", 0.),
                emitter("v20/emitter/playersplash", 0.3),
                emitter("v20/emitter/cameraemittera", 0.),
                emitter("v20/emitter/playerteleportemittera", 0.),
                emitter("v20/emitter/playerteleportemitterb", 0.),
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
            command: None,
            commands: Default::default(),
            shot: None,
            eye_rotation: [0.0; 3],
            zoom: None,
            crosshair: true,
            follow_arm: false,
            paint_tint: false,
            rope: None,
            scripts: Default::default(),
        },
    )
}
/// The recovered LoveImage and PlayerBurnImage state tables.
fn weapons() -> Arc<Pack> {
    Arc::new(Pack {
        effects: Default::default(),
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
        sounds: Default::default(),
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
    fx.advance(0.1, head, &[], &[], &[])?;
    assert!(fx.world().particle_count() > 0);
    for _ in 0..20 {
        fx.advance(0.1, head, &[], &[], &[])?;
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
        fx.advance(0.1, head, &[], &[], &[])?;
    }
    assert_eq!(fx.image_count(), 1, "burning outlives its first state");
    for _ in 0..4 {
        fx.advance(0.1, head, &[], &[], &[])?;
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
    fx.advance(0.1, |_| None, &[], &[], &[])?;
    assert_eq!(fx.image_count(), 0);
    Ok(())
}

#[test]
fn jets_burning_vehicles_and_splashes_follow_their_sources() -> Result<()> {
    let mut fx = ActorEffects::new(effects(), weapons(), Default::default())?;
    let feet = [Mat4::IDENTITY, Mat4::from_translation(Vec3::X)];
    let wreck = Mat4::from_translation(Vec3::new(5., 0., 5.));
    let fire = "v20/emitter/vehicleburnemitter".to_string();
    fx.advance(
        0.1,
        head,
        &[(7, feet, Vec3::ZERO)],
        &[(9, fire, wreck)],
        &[],
    )?;
    assert_eq!((fx.jet_count(), fx.burning_count()), (2, 1));
    assert!(fx.world().particle_count() > 0);
    fx.advance(0.1, head, &[], &[], &[])?;
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
    fx.advance(0.05, at_vehicle, &[], &[], &[])?;
    assert!(fx.world().source_count() >= 2);
    for _ in 0..10 {
        fx.advance(0.1, at_vehicle, &[], &[], &[])?;
    }
    assert_eq!(fx.world().source_count(), 0, "splash emitters are finite");
    assert!(
        fx.diagnostics.messages.is_empty(),
        "{:?}",
        fx.diagnostics.messages
    );
    Ok(())
}

/// `PlayerStandardArmor.jetGroundDistance = 4` and the 0.1 lift of
/// `Player::updateJetEffects` (blocklandv20.exe 0x5ad1b0, 0x711fb0): dust at
/// full rate on the ground, fading linearly to none 4 units up.
#[test]
fn jet_dust_kicks_up_below_four_units_and_fades_with_height() -> Result<()> {
    use bri_client::actor_effects::{JET_GROUND_DISTANCE, JET_GROUND_LIFT, jet_dust};
    assert_eq!((JET_GROUND_DISTANCE, JET_GROUND_LIFT), (4.0, 0.1));
    let at = Vec3::new(3., 10., 0.);
    let dust = |foot, height: f32| jet_dust(7, foot, at, Vec3::NEG_Y, Some((height, Vec3::Y)));
    let low = dust(0, 0.5).expect("dust half a unit up");
    assert!(low.position.distance(Vec3::new(3., 9.6, 0.)) < 1e-6);
    assert_eq!(low.normal, Vec3::Y);
    assert!((low.rate - (4. - 0.4) / 4.).abs() < 1e-6);
    assert!((dust(0, 2.1).unwrap().rate - 0.5).abs() < 1e-6);
    assert!(dust(0, 4.1).is_none(), "the lifted point is past the cast");
    assert!(jet_dust(7, 0, at, Vec3::NEG_Y, None).is_none());
    // A slope ejects along its own normal.
    let slope = jet_dust(7, 1, at, Vec3::NEG_Y, Some((1., Vec3::new(1., 1., 0.)))).unwrap();
    assert!((slope.normal.length() - 1.).abs() < 1e-6 && slope.normal.x > 0.);

    let mut fx = ActorEffects::new(effects(), weapons(), Default::default())?;
    fx.update_jet_dust(&[low, dust(1, 1.).unwrap()])?;
    fx.advance(0.1, head, &[], &[], &[])?;
    assert_eq!(fx.jet_dust_count(), 2, "one source per foot");
    assert!(fx.world().particle_count() > 0);
    fx.update_jet_dust(&[low])?;
    assert_eq!(fx.jet_dust_count(), 1);
    fx.update_jet_dust(&[])?;
    assert_eq!(fx.jet_dust_count(), 0);
    assert!(
        fx.diagnostics.messages.is_empty(),
        "{:?}",
        fx.diagnostics.messages
    );
    Ok(())
}

#[test]
fn froth_follows_the_surface_and_bubbles_follow_a_splash() -> Result<()> {
    use bri_client::actor_effects::Swimmer;
    let mut fx = ActorEffects::new(effects(), weapons(), Default::default())?;
    let water = bri_content::water::Water::volume([-8., -4., -8.], [8., 1., 8.]);
    fx.set_liquids(
        vec![bri_sim::water::TintedWater {
            water: water.clone(),
            color: [0., 0., 1., 0.75],
            brick: true,
        }]
        .into(),
        vec![water].into(),
    );
    let swimmer = |feet: Vec3, speed: f32| Swimmer {
        actor: 7,
        feet,
        height: 2.65,
        velocity: Vec3::X * speed,
    };
    // Standing still in the shallows makes no froth.
    fx.update_water(0.1, &[swimmer(Vec3::ZERO, 0.)])?;
    fx.advance(0.1, head, &[], &[], &[])?;
    assert_eq!(fx.world().source_count(), 0);
    // Wading makes both foam emitters run at the surface.
    fx.update_water(0.1, &[swimmer(Vec3::ZERO, 5.)])?;
    fx.advance(0.1, head, &[], &[], &[])?;
    assert_eq!(fx.world().source_count(), 2);
    assert!(fx.world().particle_count() > 0);
    // Fully under or out of the water, the froth drains.
    fx.update_water(0.1, &[swimmer(Vec3::new(0., -3.9, 0.), 5.)])?;
    fx.advance(0.1, head, &[], &[], &[])?;
    assert_eq!(fx.world().source_count(), 0);
    // A splash adds its ring and bubbles for `bubbleEmitTime` only.
    fx.cue(&cue(
        1,
        CueKind::Water {
            actor: 7,
            entered: true,
            speed: 12.,
        },
    ));
    fx.update_water(0.05, &[swimmer(Vec3::new(0., -3.9, 0.), 0.)])?;
    fx.advance(0.05, head, &[], &[], &[])?;
    assert_eq!(fx.world().source_count(), 2, "ring and bubbles");
    fx.update_water(0.06, &[swimmer(Vec3::new(0., -3.9, 0.), 0.)])?;
    fx.advance(0.2, head, &[], &[], &[])?;
    fx.advance(0.2, head, &[], &[], &[])?;
    assert_eq!(fx.world().source_count(), 0);
    // A player who leaves the view takes their emitters along.
    fx.update_water(0.1, &[swimmer(Vec3::ZERO, 5.)])?;
    fx.update_water(0.1, &[])?;
    fx.advance(0.1, head, &[], &[], &[])?;
    assert_eq!(fx.world().source_count(), 0);
    assert!(
        fx.diagnostics.messages.is_empty(),
        "{:?}",
        fx.diagnostics.messages
    );
    Ok(())
}

#[test]
#[ignore = "requires generated effects-runtime-pack-005 and weapons-pack-009; CPU only"]
fn original_emote_pain_burn_and_vehicle_images_resolve() -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let pack = EffectsPack::load(root.join("effects-runtime-pack-005"))?;
    let weapons = Arc::new(Pack::from_json(&std::fs::read(
        root.join("weapons-pack-009/weapons.json"),
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
        fx.advance(0.05, head, &[], &[], &[])?;
    }
    for level in [5., 30., 50.] {
        fx.cue(&next(CueKind::Pain {
            actor: 7,
            level,
            cry: true,
        }));
        fx.advance(0.05, head, &[], &[], &[])?;
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
        &[(9, "v20/emitter/vehicleburnemitter".into(), Mat4::IDENTITY)],
        &[],
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

#[test]
fn player_lights_shine_and_flare_at_the_hand_until_switched_off() -> Result<()> {
    let mut fx = ActorEffects::new(effects(), weapons(), Default::default())?;
    let hand = Vec3::new(1., 2., 3.);
    let light = |flare_visible| PlayerLight {
        actor: 7,
        position: hand,
        flare_visible,
    };
    let camera = bri_fx_runtime::Camera {
        view_projection: Mat4::IDENTITY,
        position: Vec3::new(1., 2., 10.),
        right: Vec3::X,
        up: Vec3::Y,
    };
    fx.advance(0.2, head, &[], &[], &[light(true)])?;
    assert_eq!(fx.light_count(), 1);
    let frame = fx.world().snapshot(&camera);
    assert_eq!(frame.lights.len(), 1);
    assert_eq!(frame.lights[0].position, hand);
    assert_eq!(frame.lights[0].radius, 10.);
    let flare = &frame.particles[0];
    assert_eq!(flare.position, hand);
    assert!(!flare.depth_test, "flares draw over the scene like fxLight");
    // `ConstantSize = 1`: a quad reaching one unit either side of the hand.
    assert!((flare.size - 2.).abs() < 1e-5, "{}", flare.size);
    // Linked flare colour is the light colour at full strength, not 5x.
    assert_eq!(flare.color, glam::Vec4::ONE);
    // Blocked sight fades the flare out over `FadeTime`; the light stays.
    fx.advance(0.2, head, &[], &[], &[light(false)])?;
    let frame = fx.world().snapshot(&camera);
    assert_eq!(frame.lights.len(), 1);
    assert!(frame.particles.iter().all(|p| p.size <= 0.));
    fx.advance(0.1, head, &[], &[], &[])?;
    assert_eq!(fx.light_count(), 0);
    assert!(fx.world().snapshot(&camera).lights.is_empty());
    assert!(
        fx.diagnostics.messages.is_empty(),
        "{:?}",
        fx.diagnostics.messages
    );
    Ok(())
}

#[test]
fn teleports_sparkle_briefly_and_camera_orbs_follow_the_stream() -> Result<()> {
    let mut fx = ActorEffects::new(effects(), weapons(), Default::default())?;
    let back = |a| matches!(a, Anchor::Actor { actor: 7, mount: 2 }).then_some(Mat4::IDENTITY);
    fx.cue(&cue(
        1,
        CueKind::Teleport {
            actor: 7,
            scale: 1.0,
            player: true,
        },
    ));
    assert_eq!(
        fx.image_count(),
        1,
        "PlayerTeleportImage takes the emote slot"
    );
    fx.advance(0.1, back, &[], &[], &[])?;
    assert!(fx.world().particle_count() > 0);
    assert!(fx.world().source_count() >= 1);
    for _ in 0..28 {
        fx.advance(0.1, back, &[], &[], &[])?;
    }
    assert_eq!(fx.image_count(), 1, "the sparkle lasts 3 seconds");
    for _ in 0..3 {
        fx.advance(0.1, back, &[], &[], &[])?;
    }
    assert_eq!(fx.image_count(), 0, "PlayerTeleportImage::onDone unmounts");
    assert_eq!(fx.world().source_count(), 0, "the 150 ms burst is finite");
    // A vehicle bursts without the image.
    fx.cue(&cue(
        2,
        CueKind::Teleport {
            actor: 7,
            scale: 3.0,
            player: false,
        },
    ));
    assert_eq!(fx.image_count(), 0);
    fx.set_orbs(vec![(8, Vec3::new(1., 2., 3.))]);
    fx.advance(0.1, back, &[], &[], &[])?;
    fx.advance(0.1, back, &[], &[], &[])?;
    assert_eq!(fx.orb_count(), 1);
    fx.set_orbs(Vec::new());
    fx.advance(0.1, back, &[], &[], &[])?;
    assert_eq!(fx.orb_count(), 0, "the orb goes with the camera");
    assert!(
        fx.diagnostics.messages.is_empty(),
        "{:?}",
        fx.diagnostics.messages
    );
    Ok(())
}

#[test]
fn image_emitters_eject_along_the_image_forward_axis() {
    // `ShapeBase::updateImageState` emits along column 1 (source +Y): a
    // head-slot emote sprays forward (native -Z), and HateImage's
    // `rotation = "1 0 0 -90"` turns its steam upward.
    let axis = |image: &Image| {
        let m = bri_client::actor_effects::image_emitter(Mat4::IDENTITY, image);
        m.transform_vector3(Vec3::Y)
    };
    let (_, mut love) = image("LoveImage", Vec::new());
    assert!(axis(&love).abs_diff_eq(Vec3::NEG_Z, 1e-5));
    love.source_rotation_degrees = [-90., 0., 0.];
    assert!(axis(&love).abs_diff_eq(Vec3::Y, 1e-5));
}

#[test]
fn hard_landings_shake_the_camera_by_speed_past_ten() -> Result<()> {
    let peak = |speed: f32, min_impact: f32| -> Result<f32> {
        let mut fx = ActorEffects::new(effects(), weapons(), Default::default())?;
        fx.ground_impact(speed, min_impact, 7);
        let mut peak = 0f32;
        for _ in 0..40 {
            fx.advance(0.02, head, &[], &[], &[])?;
            peak = peak.max(
                fx.camera_shake(Vec3::new(500.0, 0.0, 0.0))
                    .abs()
                    .max_element(),
            );
        }
        Ok(peak)
    };
    assert_eq!(peak(9.5, 30.0)?, 0.0, "below groundImpactMinSpeed");
    let standard = peak(40.0, 30.0)?;
    assert!(standard > 0.1, "a 40 m/s landing shakes: {standard}");
    // The horse's minImpactSpeed of 250 makes the same landing a tremor.
    assert!(peak(40.0, 250.0)? < standard * 0.2);
    // Not an explosion: the shake follows the camera wherever it is.
    Ok(())
}
