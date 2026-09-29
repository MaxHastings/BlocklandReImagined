//! Script effects with any arguments a rule can pass: whatever the
//! capability gate accepts must become a cue every client accepts, so a
//! rule can never make the host send what joiners refuse.
use bri_package_runtime::ops::{CAPABILITIES, FOV_RANGE, Op, authorize};
use bri_sim::presentation::{Cue, CueKind};
use proptest::{prelude::*, test_runner::RngSeed};

fn all() -> Vec<String> {
    CAPABILITIES.iter().map(|c| c.to_string()).collect()
}

/// Mostly ordinary values, with NaN, infinities, huge and negative ones.
fn float() -> impl Strategy<Value = f32> {
    prop_oneof![
        6 => -50.0f32..50.0,
        1 => Just(f32::NAN),
        1 => Just(f32::INFINITY),
        1 => Just(f32::NEG_INFINITY),
        1 => prop_oneof![Just(1e7f32), Just(-1e7), Just(2500.0), Just(0.0), Just(1e-9)],
    ]
}
fn point() -> impl Strategy<Value = [f32; 3]> {
    [float(), float(), float()]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 512,
        failure_persistence: None,
        rng_seed: RngSeed::Fixed(0xbea7),
        ..ProptestConfig::default()
    })]

    #[test]
    fn accepted_beams_make_cues_clients_accept(
        from in point(),
        to in point(),
        color in [-0.5f32..1.5, -0.5f32..1.5, -0.5f32..1.5, -0.5f32..1.5],
        width in float(),
        seconds in float(),
        muzzle in proptest::option::of(0u64..4),
    ) {
        let op = Op::Beam { from, to, color, width, seconds, muzzle };
        if authorize("fuzz", &all(), &op).is_ok() {
            let cue = Cue {
                id: 1,
                tick: 1,
                kind: CueKind::Beam { to, color, width, seconds, muzzle: muzzle.filter(|m| *m > 0) },
                position: from,
            };
            prop_assert!(cue.validate().is_ok(), "{op:?}");
        }
    }

    #[test]
    fn accepted_animations_make_cues_clients_accept(
        thread in any::<u8>(),
        sequence in prop_oneof![
            "[a-z0-9_]{1,8}",
            ".{0,80}",
            Just(String::new()),
        ],
    ) {
        let op = Op::PlayThread { player: 1, thread, sequence: sequence.clone() };
        if authorize("fuzz", &all(), &op).is_ok() {
            let cue = Cue {
                id: 1,
                tick: 1,
                kind: CueKind::WeaponAnimation { actor: 1, thread, sequence, image_hand: None },
                position: [0.0; 3],
            };
            prop_assert!(cue.validate().is_ok(), "{op:?}");
        }
    }

    #[test]
    fn only_camera_fovs_are_accepted(fov in proptest::option::of(float())) {
        let accepted = authorize("fuzz", &all(), &Op::SetFov { player: 1, fov }).is_ok();
        prop_assert_eq!(accepted, fov.is_none_or(|f| FOV_RANGE.contains(&f)));
    }
}
