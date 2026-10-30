//! Decoded/mixed signal evidence for ambient profile distances and shelter.
mod common;
use bri_audio::*;
use common::*;
use std::sync::Arc;

fn runtime() -> AudioRuntime {
    let mut cfg = RuntimeConfig::default();
    cfg.engine.sample_rate = OUT_RATE;
    cfg.engine.gain_curve = GainCurve::Linear;
    AudioRuntime::new(bank(), cfg, OutputKind::Offline).unwrap()
}

fn decoded() -> SoundAsset {
    let mut signal = sine(OUT_RATE, 1, 1.0, 200.0, 0.15);
    for (a, b) in signal.iter_mut().zip(sine(OUT_RATE, 1, 1.0, 8000.0, 0.15)) {
        *a += b;
    }
    let bytes = wav::encode_pcm16(OUT_RATE, 1, &signal);
    SoundAsset::decoded("environment-test", &bytes, "wav", 10.0, 60.0).unwrap()
}

#[test]
fn explicit_profile_distances_override_ambient_range_and_legacy_bank_ranges_do_not() {
    let energy = |authored: bool, distance: f32| {
        let mut rt = runtime();
        let mut asset = decoded();
        if authored {
            asset.authored_spatial = Some(schema::Spatial {
                reference_distance: 150.0,
                max_distance: 300.0,
                falloff: schema::Falloff::Linear,
            });
        }
        rt.play_asset(
            Arc::new(asset.as_ambient(Some(200.0))),
            Placement::World([0.0, 0.0, -distance]),
            1.0,
        )
        .unwrap();
        rms(&rt.render_offline(OUT_RATE as usize / 2))
    };
    let legacy = energy(false, 100.0) / energy(false, 0.0);
    let authored = energy(true, 100.0) / energy(true, 0.0);
    assert!((legacy - 2.0 / 3.0).abs() < 0.01, "{legacy}");
    assert!((authored - 1.0).abs() < 0.01, "{authored}");
    assert_eq!(energy(false, 250.0), 0.0);
    assert!(energy(true, 250.0) > 0.001);
}

fn amplitude(samples: &[f32], hz: f32) -> f32 {
    let count = samples.len() / 2;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, frame) in samples.chunks_exact(2).enumerate() {
        let phase = i as f32 * std::f32::consts::TAU * hz / OUT_RATE as f32;
        re += frame[0] * phase.cos();
        im += frame[0] * phase.sin();
    }
    (re * re + im * im).sqrt() / count as f32
}

#[test]
fn acoustic_commands_muffle_actual_decoded_audio_smoothly_without_restarting_the_loop() {
    let mut rt = runtime();
    let h = rt
        .play_asset(
            Arc::new(decoded().as_ambient(None)),
            Placement::Listener,
            1.0,
        )
        .unwrap();
    let clear = rt.render_offline(OUT_RATE as usize);
    rt.set_voice_acoustics(
        h,
        Acoustics {
            gain: 0.3,
            muffle: 1.0,
        },
    )
    .unwrap();
    let transition = rt.render_offline(OUT_RATE as usize * 2);
    let blocked = rt.render_offline(OUT_RATE as usize);
    let low = amplitude(&blocked, 200.0) / amplitude(&clear, 200.0);
    let high = amplitude(&blocked, 8000.0) / amplitude(&clear, 8000.0);
    assert!(low > 0.25 && low < 0.32, "{low}");
    assert!(high < low * 0.2, "low={low} high={high}");
    // The first samples retain the previous gain; no command-edge hard mute.
    assert!(rms(&transition[..480]) > rms(&blocked[..480]) * 2.0);
    rt.set_voice_acoustics(h, Acoustics::CLEAR).unwrap();
    rt.render_offline(OUT_RATE as usize * 3);
    let restored = rt.render_offline(OUT_RATE as usize);
    assert!((rms(&restored) / rms(&clear) - 1.0).abs() < 0.01);
    let events = rt.drain_events();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, AudioEvent::Started { .. }))
            .count(),
        1
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AudioEvent::Finished { .. } | AudioEvent::Stopped { .. }))
    );
}

#[test]
fn streaming_bed_keeps_its_handle_and_recovers_after_shelter() {
    let mut rt = runtime();
    let bed = Arc::new(
        rt.bank()
            .resolve("musicData_Test")
            .unwrap()
            .as_ambient(None),
    );
    assert!(matches!(bed.data, ClipData::Stream(_)));
    let h = rt.play_asset(bed, Placement::Listener, 1.0).unwrap();
    let clear = rms(&rt.render_offline(OUT_RATE as usize));
    rt.set_voice_acoustics(
        h,
        Acoustics {
            gain: 0.2,
            muffle: 0.5,
        },
    )
    .unwrap();
    rt.render_offline(OUT_RATE as usize * 2);
    let sheltered = rms(&rt.render_offline(OUT_RATE as usize));
    assert!(sheltered < clear * 0.25 && sheltered > 0.0);
    rt.set_voice_acoustics(h, Acoustics::CLEAR).unwrap();
    rt.render_offline(OUT_RATE as usize * 3);
    assert!((rms(&rt.render_offline(OUT_RATE as usize)) / clear - 1.0).abs() < 0.01);
    assert_eq!(
        rt.drain_events()
            .iter()
            .filter(|e| matches!(e, AudioEvent::Started { .. }))
            .count(),
        1
    );
}
