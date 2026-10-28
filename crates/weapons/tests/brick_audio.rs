use bri_weapons::{Pack, SCHEMA};
fn recorded() -> Pack {
    Pack::from_json(include_bytes!(
        "../../../packages/brick-destruction-audio/assets/weapons.json"
    ))
    .unwrap()
}
#[test]
fn recorded_policy_validates_and_all_profiles_are_declared() {
    let pack = recorded();
    let policy = pack.effects.brick_debris_audio.as_ref().unwrap();
    assert!(!pack.effects.is_empty());
    for r in policy.responses() {
        assert!(pack.sounds.contains_key(&r.profile));
    }
    assert!(
        pack.add_on_references()
            .iter()
            .any(|r| r.used_by.contains("brick debris audio"))
    );
    for fault in 0..5 {
        let mut p = policy.clone();
        match fault {
            0 => p.min_speed = f32::NAN,
            1 => p.pitch = [1.2, 0.9],
            2 => p.cooldown = 0.,
            3 => p.impact.as_mut().unwrap().radius = 1000.,
            _ => p.landing_min_members = 0,
        }
        assert!(p.validate().is_err());
    }
}
#[test]
fn conflicting_policy_providers_disable_the_slot_and_report_the_conflict() {
    let first = recorded();
    let mut second = Pack {
        schema_version: SCHEMA,
        id: "other".into(),
        ..Default::default()
    };
    let mut policy = first.effects.brick_debris_audio.clone().unwrap();
    policy.id = "other:audio/foley".into();
    second.effects.brick_debris_audio = Some(policy);
    let (merged, notes) = first.merge(vec![("addons/other/assets".into(), second)]);
    assert!(merged.effects.brick_debris_audio.is_none());
    assert!(
        notes
            .iter()
            .any(|n| n.line().contains("multiple providers"))
    );
    merged.validate().unwrap();
}
