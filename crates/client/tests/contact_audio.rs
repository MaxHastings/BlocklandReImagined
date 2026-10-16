//! Invented fixtures; these values do not encode original repeat/promotion policy.
use bri_client::weapon_effects::WeaponEffects;
use bri_content::effects::{ContactAudio, ParticleContact};
use bri_fx_runtime::{ContactHit, EffectsLimits, SourceTransform};
use glam::{Quat, Vec3};
use std::{collections::BTreeMap, sync::Arc};

#[test]
fn contact_audio_local_adapter_resolves_exact_hit_once_and_resets_without_replay() {
    let pack = bri_fx_runtime::testing::pack(|l| {
        l.particles[0].contact = Some(ParticleContact {
            restitution: 0.,
            friction: 1.,
            rest_speed: 0.1,
            die_on_contact: true,
            die_at_rest: false,
            stop_animation_at_rest: false,
        });
        l.particles[0].contact_audio = Some(ContactAudio {
            surfaces: BTreeMap::from([("invented_surface".into(), "invented_sound".into())]),
            fallback: None,
            min_inward_speed: 0.,
            cooldown_seconds: 0.,
            max_impacts: 1,
            max_lateness_seconds: 0.2,
        });
    });
    let weapons = serde_json::from_value(serde_json::json!({
        "schema_version": bri_weapons::SCHEMA, "id": "test"
    }))
    .unwrap();
    let mut fx = WeaponEffects::new(pack, Arc::new(weapons), EffectsLimits::default()).unwrap();
    for known in [true, false] {
        fx.burst_at(
            "emitter",
            SourceTransform {
                position: Vec3::Y * 0.05,
                rotation: Quat::from_rotation_z(std::f32::consts::PI),
                ..Default::default()
            },
        )
        .unwrap();
        let mut sweeps = Vec::new();
        fx.advance_with_contacts(
            0.1,
            Vec3::ZERO,
            |_| None,
            |a, b| {
                if a.y > 0. && b.y <= 0. {
                    sweeps.push((a, b));
                    Some(ContactHit {
                        fraction: a.y / (a.y - b.y),
                        normal: Vec3::Y,
                    })
                } else {
                    None
                }
            },
        )
        .unwrap();
        let sounds = fx.take_contact_audio(|hit| {
            assert!(sweeps.contains(&(hit.from, hit.to)));
            assert!(hit.point.y.abs() < 1e-6);
            Some(if known { "invented_surface" } else { "unknown" }.into())
        });
        assert_eq!(sounds.len(), usize::from(known));
        if known {
            assert_eq!(sounds[0].0, "invented_sound");
            assert!(sounds[0].1[1].abs() < 1e-6);
        }
        assert!(
            fx.take_contact_audio(|_| panic!("already consumed"))
                .is_empty()
        );
    }
    assert_eq!(fx.diagnostics.contact_mapping_misses, 1);
    fx.burst_at(
        "emitter",
        SourceTransform {
            position: Vec3::Y * 0.05,
            rotation: Quat::from_rotation_z(std::f32::consts::PI),
            ..Default::default()
        },
    )
    .unwrap();
    fx.advance_with_contacts(
        0.1,
        Vec3::ZERO,
        |_| None,
        |a, b| {
            (a.y > 0. && b.y <= 0.).then(|| ContactHit {
                fraction: a.y / (a.y - b.y),
                normal: Vec3::Y,
            })
        },
    )
    .unwrap();
    fx.reset(0);
    assert!(
        fx.take_contact_audio(|_| panic!("session reset"))
            .is_empty()
    );
}

#[test]
fn contact_audio_surface_resolution_rejects_nearer_occluder_and_different_face() {
    use bri_client::{building::Building, weapon_effects::contact_surface};
    use bri_fx_runtime::{ContactImpact, EffectHandle};
    use rapier3d::prelude::*;
    let make = |occluded| {
        let mut colliders =
            vec![ColliderBuilder::cuboid(5., 0.5, 5.).translation(Vector::new(0., -0.5, 0.))];
        if occluded {
            colliders
                .push(ColliderBuilder::cuboid(5., 0.1, 5.).translation(Vector::new(0., 0.5, 0.)));
        }
        Building::new(
            bri_sim::definitions::Definitions {
                entries: BTreeMap::new(),
            },
            colliders,
        )
        .unwrap()
    };
    let mut impact = ContactImpact {
        source: EffectHandle(1),
        particle_id: 1,
        ordinal: 1,
        definition: 0,
        from: Vec3::Y,
        to: Vec3::NEG_Y,
        fraction: 0.5,
        point: Vec3::ZERO,
        normal: Vec3::Y,
        incoming_velocity: Vec3::NEG_Y,
        lateness: 0.,
    };
    assert_eq!(
        contact_surface(&make(false), &impact).as_deref(),
        Some("default")
    );
    assert_eq!(contact_surface(&make(true), &impact), None);
    impact.normal = Vec3::NEG_Y;
    assert_eq!(contact_surface(&make(false), &impact), None);
    impact.from.x = 20.;
    impact.to.x = 20.;
    assert_eq!(contact_surface(&make(false), &impact), None);
}
