//! Hardware-free runtime behaviour tests (offline/null output only).

mod common;

use bri_audio::*;
use common::*;

fn offline(cfg: impl FnOnce(&mut RuntimeConfig)) -> AudioRuntime {
    let mut c = RuntimeConfig::default();
    c.engine.sample_rate = OUT_RATE;
    cfg(&mut c);
    AudioRuntime::new(bank(), c, OutputKind::Offline).unwrap()
}

fn linear(c: &mut RuntimeConfig) {
    c.engine.gain_curve = GainCurve::Linear;
}

fn seconds(rt: &mut AudioRuntime, s: f32) -> Vec<f32> {
    rt.render_offline((s * OUT_RATE as f32) as usize)
}

fn assert_finite(v: &[f32]) {
    assert!(
        v.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
        "non-finite or out-of-range sample"
    );
}

#[test]
fn one_shot_plays_then_finishes_and_frees_its_voice() {
    let mut rt = offline(|_| {});
    let h = rt.play("UiSound", Placement::Listener).unwrap();
    let a = seconds(&mut rt, 0.1);
    assert!(rms(&a) > 0.01);
    assert_eq!(rt.stats().real_voices, 1);
    let b = seconds(&mut rt, 0.3); // clip is 0.25 s
    assert_finite(&b);
    let ev = rt.drain_events();
    assert!(ev.contains(&AudioEvent::Started {
        handle: h,
        real: true
    }));
    assert!(ev.contains(&AudioEvent::Finished { handle: h }));
    assert_eq!(rt.stats().real_voices, 0);
    let tail = seconds(&mut rt, 0.05);
    assert_eq!(rms(&tail), 0.0);
}

#[test]
fn resampled_duration_matches_source_duration() {
    let mut rt = offline(|_| {});
    rt.play("UiSound", Placement::Listener).unwrap(); // 0.25 s at 22.05 kHz
    let out = seconds(&mut rt, 0.5);
    let last_nonzero = out
        .chunks(2)
        .rposition(|f| f[0] != 0.0 || f[1] != 0.0)
        .unwrap();
    let played = (last_nonzero + 1) as f32 / OUT_RATE as f32;
    assert!((played - 0.25).abs() < 0.002, "played {played} s");
}

#[test]
fn loop_continues_until_stopped_and_stop_is_declicked() {
    let mut rt = offline(|_| {});
    let h = rt.play("Loop2dSound", Placement::Listener).unwrap();
    let a = seconds(&mut rt, 2.5); // clip is 1 s
    assert_finite(&a);
    assert!(
        rms(&a[a.len() - 4800..]) > 0.05,
        "still audible after wrapping twice"
    );
    rt.stop(h).unwrap();
    let b = seconds(&mut rt, 0.02);
    // 5 ms fade: first samples still non-zero, last 10 ms silent.
    assert!(b[..40].iter().any(|s| *s != 0.0));
    assert_eq!(rms(&b[b.len() - 960..]), 0.0);
    assert!(
        rt.drain_events()
            .contains(&AudioEvent::Stopped { handle: h })
    );
    assert_eq!(rt.stats().real_voices + rt.stats().virtual_voices, 0);
}

#[test]
fn attenuation_follows_torque_linear_rolloff() {
    // LoopSound: reference 5, max 30. Linear curve for exact ratios.
    let level = |d: f32| {
        let mut rt = offline(linear);
        rt.play("LoopSound", Placement::World([0.0, 0.0, -d]))
            .unwrap();
        rms(&seconds(&mut rt, 0.5))
    };
    let near = level(2.0);
    let refd = level(5.0);
    let half = level(17.5);
    let edge = level(30.0);
    let far = level(45.0);
    assert!(
        (near - refd).abs() < 1e-4,
        "full volume inside reference distance"
    );
    assert!(
        (half / refd - 0.5).abs() < 0.01,
        "half way => 0.5, got {}",
        half / refd
    );
    assert_eq!(edge, 0.0);
    assert_eq!(far, 0.0);

    // Default Torque gain table: monotonic and quieter than linear at half distance.
    let mut rt = offline(|_| {});
    rt.play("LoopSound", Placement::World([0.0, 0.0, -17.5]))
        .unwrap();
    let table_half = rms(&seconds(&mut rt, 0.5));
    assert!(table_half > 0.0 && table_half < half);
}

/// Max's video: a crash seen through a portal is heard as near as it
/// shows, and from that side, not from wherever it really is (here 100
/// units off, past LoopSound's 30 unit reach).
#[test]
fn a_sound_seen_through_a_window_is_heard_from_where_it_shows() {
    // A window ahead in the plane z = -2 leads 100 along x, the same way
    // round: the far face is at x = 100, facing -z.
    let window = Window {
        ear: Listener {
            position: [100.0, 0.0, 0.0],
            ..Listener::default()
        },
        centre: [100.0, 0.0, -2.0],
        normal: [0.0, 0.0, -1.0],
        u: [1.0, 0.0, 0.0],
        v: [0.0, 1.0, 0.0],
        half: [2.0, 2.0],
    };
    let level = |at: Vec3, windows: &[Window]| {
        let mut rt = offline(linear);
        rt.set_windows(windows).unwrap();
        rt.play("LoopSound", Placement::World(at)).unwrap();
        rms_lr(&seconds(&mut rt, 0.5)[9600..])
    };
    let (dl, dr) = level([1.0, 0.0, -8.0], &[]);
    // The same source past the far face, a little to the right.
    let (l, r) = level([101.0, 0.0, -8.0], &[window]);
    assert!(
        (l - dl).abs() < 0.02 * dl && (r - dr).abs() < 0.02 * dr,
        "through {l}/{r}, direct {dl}/{dr}"
    );
    assert!(r > l, "heard from its right");
    // Without the window, too far to hear.
    let (l, r) = level([101.0, 0.0, -8.0], &[]);
    assert_eq!(l + r, 0.0);
}

#[test]
fn panning_uses_listener_orientation_y_up() {
    let mut rt = offline(|_| {});
    rt.play("LoopSound", Placement::World([3.0, 0.0, 0.0]))
        .unwrap(); // +X = right of default listener
    let (l, r) = rms_lr(&seconds(&mut rt, 0.3));
    assert!(r > 10.0 * l.max(1e-6), "l={l} r={r}");
    // Turn to face +X (right becomes +Z): source is now straight ahead.
    rt.set_listener(Listener {
        forward: [1.0, 0.0, 0.0],
        ..Listener::default()
    })
    .unwrap();
    let (l, r) = rms_lr(&seconds(&mut rt, 0.3)[4800..]);
    assert!(
        (l - r).abs() / r < 0.02,
        "centred after turning: l={l} r={r}"
    );
}

#[test]
fn attached_sources_follow_entities_and_despawn_stops_them() {
    let mut rt = offline(linear);
    let car = EntityKey(7);
    let engine = rt
        .play(
            "LoopSound",
            Placement::Attached {
                entity: car,
                position: [0.0, 0.0, -2.0],
            },
        )
        .unwrap();
    let other = rt
        .play(
            "LoopSound",
            Placement::Attached {
                entity: EntityKey(8),
                position: [0.0, 0.0, -2.0],
            },
        )
        .unwrap();
    let close = rms(&seconds(&mut rt, 0.2));
    rt.update_entity(car, [0.0, 0.0, -17.5]).unwrap();
    let moved = rms(&seconds(&mut rt, 0.2)[2400..]);
    assert!(moved < close * 0.9, "moving one source away lowers the mix");
    rt.despawn(car).unwrap();
    seconds(&mut rt, 0.05);
    let ev = rt.drain_events();
    assert!(ev.contains(&AudioEvent::Stopped { handle: engine }));
    assert!(!ev.contains(&AudioEvent::Stopped { handle: other }));
    assert_eq!(rt.stats().real_voices, 1);
    rt.despawn(EntityKey(8)).unwrap();
    seconds(&mut rt, 0.05);
    assert_eq!(rt.stats().real_voices, 0);
}

#[test]
fn master_channel_and_music_gains() {
    let mut rt = offline(|_| {});
    rt.play("Loop2dSound", Placement::Listener).unwrap(); // effects (channel 2)
    let base = rms(&seconds(&mut rt, 0.2));
    assert!(base > 0.0);
    rt.set_volume(VolumeControl::Effects, 0.0).unwrap();
    assert_eq!(rms(&seconds(&mut rt, 0.2)[4800..]), 0.0);
    rt.set_volume(VolumeControl::Effects, 1.0).unwrap();
    assert!(rt.apply_ui_volume("master", 0.0).unwrap());
    assert_eq!(rms(&seconds(&mut rt, 0.2)[4800..]), 0.0);
    assert!(!rt.apply_ui_volume("bogus", 1.0).unwrap());
    rt.apply_ui_volume("master", 1.0).unwrap();
    rt.apply_ui_volume("shell", 0.0).unwrap(); // interface channel does not affect effects
    assert!(rms(&seconds(&mut rt, 0.2)[4800..]) > 0.0);

    let mut rt = offline(|_| {});
    rt.play("musicData_Test", Placement::Listener).unwrap();
    assert!(rms(&seconds(&mut rt, 0.2)) > 0.0);
    rt.set_volume(VolumeControl::Music, 0.0).unwrap();
    assert_eq!(rms(&seconds(&mut rt, 0.2)[4800..]), 0.0);
}

#[test]
fn voice_pressure_is_bounded_observable_and_leak_free() {
    let mut rt = offline(|c| c.command_capacity = 20_000);
    let mut handles = Vec::new();
    for i in 0..40 {
        // Increasing distance: later sounds are quieter.
        handles.push(
            rt.play("BoomSound", Placement::World([0.0, 0.0, -(i as f32)]))
                .unwrap(),
        );
    }
    seconds(&mut rt, 0.01);
    let s = rt.stats();
    assert_eq!(s.real_voices, 16);
    let culled = rt
        .drain_events()
        .iter()
        .filter(|e| {
            matches!(
                e,
                AudioEvent::Culled {
                    reason: CullReason::VoicePressure,
                    ..
                }
            )
        })
        .count();
    assert_eq!(
        culled, 24,
        "one-shots beyond the budget are culled, not queued"
    );

    // Sustained spam: 10,000 one-shots over 10 simulated seconds.
    for n in 0..10_000u32 {
        rt.play(
            "BoomSound",
            Placement::World([0.0, 0.0, -((n % 50) as f32)]),
        )
        .unwrap();
        if n % 100 == 0 {
            rt.update(0.1);
            let s = rt.stats();
            assert!(s.real_voices <= 16 && s.virtual_voices == 0);
            rt.drain_events();
        }
    }
    seconds(&mut rt, 1.0);
    let s = rt.stats();
    assert_eq!(
        s.real_voices + s.virtual_voices,
        0,
        "every one-shot retired"
    );
    let requested = 40 + 10_000;
    assert_eq!(
        s.started + s.rejected,
        requested,
        "every request started or was rejected"
    );
    assert_eq!(
        s.started,
        s.finished + s.stopped + s.culled,
        "every started voice retired"
    );
    assert_eq!(s.commands_dropped, 0);
    // 16 in-phase copies exceed full scale: output is clamped and the clipping is counted.
    assert!(s.peak.is_finite() && s.peak > 1.0 && s.clipped_samples > 0);
}

#[test]
fn loopers_virtualize_under_pressure_and_revive() {
    let mut rt = offline(|c| c.engine.voices.max_virtual_voices = 8);
    let mut near = Vec::new();
    for _ in 0..16 {
        near.push(
            rt.play("LoopSound", Placement::World([0.0, 0.0, -1.0]))
                .unwrap(),
        );
    }
    let far = rt
        .play("LoopSound", Placement::World([0.0, 0.0, -20.0]))
        .unwrap();
    seconds(&mut rt, 0.05);
    let s = rt.stats();
    assert_eq!((s.real_voices, s.virtual_voices), (16, 1));
    assert!(rt.drain_events().contains(&AudioEvent::Started {
        handle: far,
        real: false
    }));

    // Virtual list is bounded.
    for _ in 0..20 {
        rt.play("LoopSound", Placement::World([0.0, 0.0, -25.0]))
            .unwrap();
    }
    seconds(&mut rt, 0.05);
    assert_eq!(rt.stats().virtual_voices, 8);
    assert!(rt.drain_events().iter().any(|e| matches!(
        e,
        AudioEvent::Culled {
            reason: CullReason::VirtualLimit,
            ..
        }
    )));

    // Free a real voice: the loudest eligible virtual looper revives after the delay.
    rt.stop(near[0]).unwrap();
    seconds(&mut rt, 0.2);
    assert_eq!(
        rt.stats().real_voices,
        15,
        "not before the 500 ms uncull delay"
    );
    seconds(&mut rt, 0.5);
    assert_eq!(rt.stats().real_voices, 16);
    assert!(
        rt.drain_events()
            .contains(&AudioEvent::Revived { handle: far })
    );
    rt.stop_all().unwrap();
    seconds(&mut rt, 0.05);
    let s = rt.stats();
    assert_eq!(s.real_voices + s.virtual_voices, 0);
}

#[test]
fn inaudible_one_shots_are_not_started() {
    let mut rt = offline(|_| {});
    let h = rt
        .play("BoomSound", Placement::World([0.0, 0.0, -100.0]))
        .unwrap();
    seconds(&mut rt, 0.01);
    assert!(rt.drain_events().contains(&AudioEvent::Culled {
        handle: h,
        reason: CullReason::BelowMinimumGain
    }));
    assert_eq!(rt.stats().real_voices, 0);
}

#[test]
fn three_d_description_without_position_plays_2d() {
    let mut rt = offline(|_| {});
    rt.play("BoomSound", Placement::Listener).unwrap(); // like client.play2D(profile)
    let (l, r) = rms_lr(&seconds(&mut rt, 0.1));
    assert!(l > 0.01 && (l - r).abs() < 1e-6);
}

#[test]
fn missing_and_unknown_sounds_fail_cleanly() {
    let mut rt = offline(|_| {});
    assert!(matches!(
        rt.play("MissingSound", Placement::Listener),
        Err(AudioError::SoundUnavailable { .. })
    ));
    assert!(matches!(
        rt.play("NoSuchSound", Placement::Listener),
        Err(AudioError::UnknownSound(_))
    ));
    assert!(matches!(
        rt.play_trigger("no.such.trigger", Placement::Listener),
        Err(AudioError::UnknownSound(_))
    ));
    let h = rt
        .play_trigger("test.boom", Placement::World([0.0, 0.0, -1.0]))
        .unwrap();
    seconds(&mut rt, 0.01);
    assert!(rt.drain_events().contains(&AudioEvent::Started {
        handle: h,
        real: true
    }));
    // Invalid positions / listener are ignored rather than poisoning the mix.
    rt.set_listener(Listener {
        forward: [0.0, 1.0, 0.0],
        ..Listener::default()
    })
    .unwrap();
    rt.update_entity(EntityKey(1), [f32::NAN, 0.0, 0.0])
        .unwrap();
    rt.play("BoomSound", Placement::World([f32::INFINITY, 0.0, 0.0]))
        .unwrap();
    assert_finite(&seconds(&mut rt, 0.1));
}

#[test]
fn full_command_queue_reports_and_drops() {
    let mut rt = offline(|c| c.command_capacity = 16);
    let mut errors = 0;
    for _ in 0..40 {
        if matches!(
            rt.play("UiSound", Placement::Listener),
            Err(AudioError::QueueFull)
        ) {
            errors += 1;
        }
    }
    assert_eq!(errors, 24);
    assert_eq!(rt.stats().commands_dropped, 24);
    seconds(&mut rt, 0.01);
    assert!(
        rt.play("UiSound", Placement::Listener).is_ok(),
        "queue drains on render"
    );
}

#[test]
fn streamed_music_loops_seamlessly() {
    let mut rt = offline(linear);
    rt.play("musicData_Test", Placement::World([0.0, 0.0, -1.0]))
        .unwrap(); // 0.7 s streamed clip
    let out = seconds(&mut rt, 3.0);
    assert_finite(&out);
    // No dropout across loop boundaries: every 10 ms window has energy.
    for (i, w) in out.chunks(960).enumerate() {
        assert!(rms(w) > 0.05, "silent window {i}");
    }
    assert_eq!(rt.stats().real_voices, 1);
}

#[test]
fn music_disable_suspends_and_enable_restarts() {
    let mut rt = offline(|_| {});
    rt.play("musicData_Test", Placement::Listener).unwrap();
    rt.play("Loop2dSound", Placement::Listener).unwrap();
    seconds(&mut rt, 0.1);
    rt.set_music_enabled(false).unwrap();
    let with_effects_only = rms(&seconds(&mut rt, 0.2)[4800..]);
    let s = rt.stats();
    assert_eq!((s.real_voices, s.suspended_voices), (1, 1));
    assert!(
        with_effects_only > 0.0,
        "disabling music does not stop other sounds"
    );
    rt.set_music_enabled(true).unwrap();
    seconds(&mut rt, 0.1);
    assert_eq!(rt.stats().real_voices, 2);
    rt.set_music_enabled(false).unwrap();
    let h = rt.play("musicData_Test", Placement::Listener).unwrap();
    seconds(&mut rt, 0.01);
    assert!(rt.drain_events().contains(&AudioEvent::Started {
        handle: h,
        real: false
    }));
    assert_eq!(rt.stats().suspended_voices, 2);
}

#[test]
fn stereo_clips_are_not_spatialised() {
    let mut rt = offline(|_| {});
    rt.play("StereoSound", Placement::World([40.0, 0.0, 0.0]))
        .unwrap(); // far right, beyond reference
    let (l, r) = rms_lr(&seconds(&mut rt, 0.2));
    assert!(
        l > 0.05,
        "left channel carries the clip's left channel unattenuated"
    );
    assert_eq!(r, 0.0, "the clip's right channel is silent");
}

#[test]
fn mono_output_and_determinism() {
    let render = || {
        let mut rt = offline(|c| c.engine.channels = 1);
        rt.play("LoopSound", Placement::World([2.0, 0.0, -4.0]))
            .unwrap();
        rt.play("UiSound", Placement::Listener).unwrap();
        let mut out = seconds(&mut rt, 0.3);
        rt.update_entity(EntityKey(1), [0.0; 3]).unwrap();
        out.extend(seconds(&mut rt, 0.3));
        out
    };
    let a = render();
    let b = render();
    assert_eq!(a.len(), (0.6 * OUT_RATE as f32) as usize);
    assert!(rms(&a) > 0.0);
    assert!(a == b, "identical commands give bit-identical output");
}

#[test]
fn update_tick_advances_null_output_time() {
    let mut rt = AudioRuntime::new(bank(), RuntimeConfig::default(), OutputKind::Null).unwrap();
    let h = rt.play("UiSound", Placement::Listener).unwrap();
    for _ in 0..20 {
        rt.update(1.0 / 60.0);
    }
    assert!(
        rt.drain_events()
            .contains(&AudioEvent::Finished { handle: h })
    );
    assert!(rt.take_capture().is_empty(), "null output keeps nothing");
    let frames = rt.stats().frames_rendered;
    assert!(
        (frames as i64 - 16_000).abs() <= 1,
        "20 ticks of 1/60 s at 48 kHz, got {frames}"
    );
}

#[test]
fn device_output_is_explicit() {
    #[cfg(not(feature = "cpal-output"))]
    {
        let r = AudioRuntime::new(bank(), RuntimeConfig::default(), OutputKind::Device);
        assert!(matches!(r, Err(AudioError::Unsupported(_))));
        let (rt, why) = AudioRuntime::open_or_null(bank(), RuntimeConfig::default());
        assert!(why.is_some());
        assert_eq!(rt.output_format(), (48_000, 2));
    }
}
