//! Held voices use the audio sample clock, never animation or render cadence.
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
fn asset() -> Arc<SoundAsset> {
    asset_at(OUT_RATE)
}
fn asset_at(rate: u32) -> Arc<SoundAsset> {
    let mut samples = vec![0.1; rate as usize];
    samples[..rate as usize / 10].fill(0.3);
    let mut asset = SoundAsset::world(
        "test:held",
        ClipData::Pcm(Arc::new(bri_audio::decode::PcmClip {
            sample_rate: rate,
            channels: 1,
            samples: samples.into_boxed_slice(),
        })),
        1.,
        60.,
    );
    asset.playback.looping = true;
    asset.sustain = Some(Sustain {
        loop_start_frame: rate as usize / 10,
        attack_seconds: 0.05,
        release_seconds: 0.15,
    });
    Arc::new(asset)
}

#[test]
fn fractional_resampling_never_interpolates_back_into_the_intro() {
    let mut rt = runtime();
    rt.play_asset(asset_at(44_100), Placement::Listener, 1.)
        .unwrap();
    rt.render_offline(OUT_RATE as usize / 5);
    let held = rt.render_offline(OUT_RATE as usize * 3);
    let full = held[0];
    assert!(held.iter().all(|sample| (*sample - full).abs() < 1e-6));
}

#[test]
fn hold_wraps_after_intro_and_feathering_resumes_one_voice() {
    let mut rt = runtime();
    let handle = rt.play_asset(asset(), Placement::Listener, 1.).unwrap();
    rt.render_offline(OUT_RATE as usize / 5);
    let held = rt.render_offline(OUT_RATE as usize * 3);
    let full = held[0];
    assert!(full > 0.01);
    assert!(
        held.iter().all(|v| (*v - full).abs() < 1e-6),
        "intro must never recur on wrap"
    );
    rt.set_sustain_gate(handle, false).unwrap();
    let release = rt.render_offline(OUT_RATE as usize / 20);
    assert!(release.last().unwrap() < &full);
    rt.set_sustain_gate(handle, true).unwrap();
    let resume = rt.render_offline(OUT_RATE as usize / 10);
    assert!((resume[0] - release.last().unwrap()).abs() < 1e-4);
    assert!((resume.last().unwrap() - full).abs() < 1e-6);
    assert_eq!(rt.stats().started, 1);
    rt.set_sustain_gate(handle, false).unwrap();
    rt.render_offline(OUT_RATE as usize * 15 / 100);
    let silent = rt.render_offline(256);
    assert!(silent.iter().all(|v| *v == 0.));
    assert_eq!(rt.stats().real_voices + rt.stats().virtual_voices, 0);
}

#[test]
fn envelope_does_not_depend_on_render_block_size_and_invalid_region_is_rejected() {
    let mut a = runtime();
    let mut b = runtime();
    a.play_asset(asset(), Placement::Listener, 1.).unwrap();
    b.play_asset(asset(), Placement::Listener, 1.).unwrap();
    let whole = a.render_offline(OUT_RATE as usize / 10);
    let pieces: Vec<_> = (0..100)
        .flat_map(|_| b.render_offline(OUT_RATE as usize / 1000))
        .collect();
    assert_eq!(whole, pieces);
    assert!(whole[0] < whole[whole.len() - 1] * 0.001);
    let mut bad = SoundAsset::world("test:bad", asset().data.clone(), 1., 60.);
    bad.playback.looping = true;
    bad.sustain = Some(Sustain {
        loop_start_frame: OUT_RATE as usize,
        attack_seconds: 0.05,
        release_seconds: 0.15,
    });
    a.stop_all().unwrap();
    a.render_offline(1024);
    a.play_asset(Arc::new(bad), Placement::Listener, 1.)
        .unwrap();
    a.render_offline(1024);
    assert_eq!(a.stats().real_voices + a.stats().virtual_voices, 0);
}

#[test]
fn inaudible_virtual_hold_still_releases() {
    let mut cfg = RuntimeConfig::default();
    cfg.engine.sample_rate = OUT_RATE;
    cfg.engine.voices.max_real_voices = 0;
    let mut rt = AudioRuntime::new(bank(), cfg, OutputKind::Offline).unwrap();
    let handle = rt
        .play_asset(asset(), Placement::World([1000., 0., 0.]), 1.)
        .unwrap();
    rt.render_offline(1024);
    assert_eq!(rt.stats().virtual_voices, 1);
    rt.set_sustain_gate(handle, false).unwrap();
    rt.render_offline(OUT_RATE as usize / 5);
    assert_eq!(rt.stats().real_voices + rt.stats().virtual_voices, 0);
}
