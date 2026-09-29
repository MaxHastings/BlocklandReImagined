//! Asset-dependent checks against a generated pack. Skipped (passes trivially)
//! unless `BRI_AUDIO_PACK` points at e.g. `content/audio-pack-002`, because
//! original game audio is never committed. Hardware-free: offline output only.

use std::sync::Arc;

use bri_audio::decode::{StreamDecoder, decode_all};
use bri_audio::schema::ClipFormat;
use bri_audio::*;

fn pack() -> Option<std::path::PathBuf> {
    std::env::var_os("BRI_AUDIO_PACK").map(Into::into)
}

#[test]
fn every_clip_decodes_and_streams_match_preloaded_decode() {
    let Some(dir) = pack() else {
        eprintln!("BRI_AUDIO_PACK not set; skipping");
        return;
    };
    let bank =
        SoundBank::load(&dir, &BankOptions::default()).expect("pack loads with hash verification");
    let m = bank.manifest();
    assert!(m.clips.len() >= 100);
    for c in &m.clips {
        let bytes = std::fs::read(dir.join(&c.file)).unwrap();
        let ext = if c.format == ClipFormat::Wav {
            "wav"
        } else {
            "ogg"
        };
        let (pcm, rep) = decode_all(&bytes, Some(ext)).unwrap_or_else(|e| panic!("{}: {e}", c.id));
        assert_eq!(
            (pcm.sample_rate, pcm.channels, pcm.frames() as u64),
            (c.sample_rate, c.channels, c.frames),
            "{}",
            c.id
        );
        assert_eq!(rep.corrupt_packets + rep.non_finite_samples, 0, "{}", c.id);
        if c.stream {
            let mut d = StreamDecoder::open(
                Arc::from(bytes.into_boxed_slice()),
                Some(ext),
                c.sample_rate,
                c.channels,
            )
            .unwrap();
            let mut all = Vec::new();
            while d.next_chunk(&mut all).unwrap() {}
            assert_eq!(all.len(), pcm.samples.len(), "{}: streamed length", c.id);
            assert!(
                all.iter().zip(pcm.samples.iter()).all(|(a, b)| a == b),
                "{}: streamed samples differ",
                c.id
            );
        }
    }
}

#[test]
fn every_ready_sound_plays_and_retires_offline() {
    let Some(dir) = pack() else { return };
    let bank = Arc::new(SoundBank::load(&dir, &BankOptions::default()).unwrap());
    let mut cfg = RuntimeConfig::default();
    cfg.engine.gain_curve = GainCurve::Linear;
    let mut rt = AudioRuntime::new(bank.clone(), cfg, OutputKind::Offline).unwrap();
    let mut played = 0;
    for s in bank.manifest().sounds.clone() {
        match rt.play(&s.id, Placement::World([1.0, 0.0, -1.0])) {
            Ok(_) => {
                assert!(s.is_ready());
                played += 1;
                let out = rt.render_offline(4800);
                assert!(out.iter().all(|v| v.is_finite()));
                rt.stop_all().unwrap();
                rt.render_offline(960);
                let st = rt.stats();
                assert_eq!(
                    st.real_voices + st.virtual_voices + st.suspended_voices,
                    0,
                    "{} retired",
                    s.id
                );
            }
            Err(AudioError::SoundUnavailable { .. }) => assert!(!s.is_ready(), "{}", s.id),
            Err(e) => panic!("{}: {e}", s.id),
        }
    }
    assert_eq!(played, bank.ready_count());
    // Trigger table resolves.
    for t in &bank.manifest().triggers {
        match rt.play_trigger(&t.key, Placement::Listener) {
            Ok(_) | Err(AudioError::SoundUnavailable { .. }) => {}
            Err(e) => panic!("trigger {}: {e}", t.key),
        }
        rt.render_offline(64);
        rt.stop_all().unwrap();
        rt.render_offline(512);
    }
}
