//! `bri-audio-evidence` — hardware-free evidence for a native audio pack.
//!
//! Loads the pack with hash verification, renders every ready sound through the
//! runtime (offline output, never a device), and renders representative scenes
//! to WAV files plus JSON diagnostics. Nothing is played.
//!
//! ```text
//! bri-audio-evidence --pack content/audio-pack-001 --out artifacts/native-audio
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use bri_audio::*;
use serde::Serialize;

const RATE: u32 = 48_000;

#[derive(Serialize)]
struct ChannelStats {
    peak: f32,
    rms: f32,
}

#[derive(Serialize)]
struct RenderReport {
    name: String,
    file: Option<String>,
    description: String,
    frames: usize,
    seconds: f32,
    left: ChannelStats,
    right: ChannelStats,
    all_finite: bool,
    clipped_samples_after_clamp: usize,
    stats: StatsOut,
    events: EventCounts,
    notes: Vec<String>,
}

#[derive(Serialize, Default)]
struct EventCounts {
    started_real: usize,
    started_virtual: usize,
    finished: usize,
    stopped: usize,
    culled: usize,
    virtualized: usize,
    revived: usize,
}

#[derive(Serialize)]
struct StatsOut {
    real_voices_at_end: u32,
    virtual_voices_at_end: u32,
    started: u64,
    finished: u64,
    stopped: u64,
    culled: u64,
    rejected: u64,
    clipped_samples: u64,
    non_finite_samples: u64,
    peak_before_clamp: f32,
    commands_dropped: u64,
    events_dropped: u64,
}

struct Scene {
    rt: AudioRuntime,
    events: EventCounts,
    notes: Vec<String>,
}

impl Scene {
    fn new(bank: &Arc<SoundBank>) -> Self {
        let mut cfg = RuntimeConfig::default();
        cfg.engine.sample_rate = RATE;
        cfg.command_capacity = 65_536;
        cfg.event_capacity = 65_536;
        Self {
            rt: AudioRuntime::new(bank.clone(), cfg, OutputKind::Offline).expect("offline"),
            events: EventCounts::default(),
            notes: vec![],
        }
    }
    fn run(&mut self, seconds: f32) {
        self.rt.render_offline((seconds * RATE as f32) as usize);
        for e in self.rt.drain_events() {
            match e {
                AudioEvent::Started { real: true, .. } => self.events.started_real += 1,
                AudioEvent::Started { real: false, .. } => self.events.started_virtual += 1,
                AudioEvent::Finished { .. } => self.events.finished += 1,
                AudioEvent::Stopped { .. } => self.events.stopped += 1,
                AudioEvent::Culled { .. } => self.events.culled += 1,
                AudioEvent::Virtualized { .. } => self.events.virtualized += 1,
                AudioEvent::Revived { .. } => self.events.revived += 1,
            }
        }
    }
    fn play(&mut self, sound: &str, p: Placement) -> Option<SoundHandle> {
        match self.rt.play(sound, p) {
            Ok(h) => Some(h),
            Err(e) => {
                self.notes.push(format!("{sound}: {e}"));
                None
            }
        }
    }
    fn finish(
        mut self,
        name: &str,
        description: &str,
        out: Option<&Path>,
    ) -> Result<RenderReport, String> {
        let samples = self.rt.take_capture();
        let file = match out {
            Some(dir) => {
                let f = format!("{name}.wav");
                std::fs::write(dir.join(&f), wav::encode_pcm16(RATE, 2, &samples))
                    .map_err(|e| e.to_string())?;
                Some(f)
            }
            None => None,
        };
        let ch = |c: usize| {
            let v: Vec<f32> = samples.iter().skip(c).step_by(2).copied().collect();
            let peak = v.iter().fold(0f32, |a, s| a.max(s.abs()));
            let rms = if v.is_empty() {
                0.0
            } else {
                (v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32).sqrt()
            };
            ChannelStats { peak, rms }
        };
        let s = self.rt.stats();
        Ok(RenderReport {
            name: name.into(),
            file,
            description: description.into(),
            frames: samples.len() / 2,
            seconds: samples.len() as f32 / 2.0 / RATE as f32,
            left: ch(0),
            right: ch(1),
            all_finite: samples.iter().all(|v| v.is_finite()),
            clipped_samples_after_clamp: samples.iter().filter(|v| v.abs() > 1.0).count(),
            stats: StatsOut {
                real_voices_at_end: s.real_voices,
                virtual_voices_at_end: s.virtual_voices,
                started: s.started,
                finished: s.finished,
                stopped: s.stopped,
                culled: s.culled,
                rejected: s.rejected,
                clipped_samples: s.clipped_samples,
                non_finite_samples: s.non_finite_samples,
                peak_before_clamp: s.peak,
                commands_dropped: s.commands_dropped,
                events_dropped: s.events_dropped,
            },
            events: self.events,
            notes: self.notes,
        })
    }
}

fn scenes(bank: &Arc<SoundBank>, dir: &Path) -> Result<Vec<RenderReport>, String> {
    let mut out = Vec::new();
    let e = EntityKey;

    // 1. Menu notes (2D, interface channel), as hovered in the main menu.
    let mut s = Scene::new(bank);
    for n in 0..12 {
        s.play(&format!("Note{n}Sound"), Placement::Listener);
        s.run(0.35);
    }
    s.run(0.5);
    out.push(s.finish(
        "01_menu_notes",
        "Note0..Note11Sound, 2D interface channel, 0.35 s apart",
        Some(dir),
    )?);

    // 2. Building: engine-bound brick sounds around the listener.
    let mut s = Scene::new(bank);
    for (i, snd) in [
        "BrickPlant",
        "BrickMove",
        "BrickRotate",
        "BrickChange",
        "BrickPlant",
        "BrickBreak",
    ]
    .iter()
    .enumerate()
    {
        let x = [-6.0, -3.0, 0.0, 3.0, 6.0, 0.0][i];
        s.play(snd, Placement::World([x, 0.0, -4.0]));
        s.run(0.4);
    }
    s.play("BrickClearSound", Placement::Listener);
    s.run(4.5);
    out.push(s.finish(
        "02_building",
        "Brick plant/move/rotate/change/break panned left->right at 4 units, then brick clear (2D)",
        Some(dir),
    )?);

    // 3. Player: sounds attached to a moving player entity.
    let mut s = Scene::new(bank);
    let player = e(1);
    let at = |x: f32| Placement::Attached {
        entity: player,
        position: [x, 0.0, -3.0],
    };
    s.play("JumpSound", at(-4.0));
    for i in 0..10 {
        s.rt.update_entity(player, [-4.0 + i as f32 * 0.8, 0.0, -3.0])
            .ok();
        s.run(0.05);
    }
    s.play("Splash1Sound", at(4.0));
    s.run(0.8);
    s.play("exitWaterSound", at(4.0));
    s.run(1.0);
    s.play("PainCrySound", at(2.0));
    s.run(0.6);
    s.play("DeathCrySound", at(2.0));
    s.run(0.8);
    s.play("deathExplosionSound", Placement::World([2.0, 0.0, -3.0]));
    s.run(1.0);
    s.play("spawnExplosionSound", Placement::World([0.0, 0.0, -2.0]));
    s.run(1.2);
    out.push(s.finish(
        "03_player",
        "jump (attached, moving L->R), splash, exit water, pain, death cry, body removal, spawn",
        Some(dir),
    )?);

    // 4. Distance attenuation: gun shots at increasing distance straight ahead.
    let mut s = Scene::new(bank);
    for d in [0.0f32, 10.0, 20.0, 35.0, 50.0, 59.0, 70.0] {
        s.play("gunShot1Sound", Placement::World([0.0, 0.0, -d]));
        s.run(0.6);
    }
    s.notes.push("AudioClose3d: reference 10, max 60; at 59 and 70 units the gain (<= 0.02) is at or below MIN_GAIN 0.05, so vanilla does not start them".into());
    out.push(s.finish(
        "04_gun_distance",
        "gunShot1Sound at 0,10,20,35,50,59,70 units ahead",
        Some(dir),
    )?);

    // 5. Rocket fly-by: attached loop moves across, explosion, despawn stops loop.
    let mut s = Scene::new(bank);
    let rocket = e(2);
    s.play("rocketFireSound", Placement::Listener);
    s.play(
        "rocketLoopSound",
        Placement::Attached {
            entity: rocket,
            position: [-40.0, 2.0, -5.0],
        },
    );
    for i in 0..=80 {
        s.rt.update_entity(rocket, [-40.0 + i as f32, 2.0, -5.0])
            .ok();
        s.run(1.0 / 40.0);
    }
    s.play("rocketExplodeSound", Placement::World([40.0, 2.0, -5.0]));
    s.rt.despawn(rocket).ok();
    s.run(2.5);
    out.push(s.finish("05_rocket_flyby", "rocket fire (2D) + rocketLoopSound attached to a projectile flying -40..+40 x, explosion at the end, despawn stops the loop", Some(dir))?);

    // 6. Loop start/stop: spray can.
    let mut s = Scene::new(bank);
    s.play(
        "sprayActivateSound",
        Placement::Attached {
            entity: e(3),
            position: [0.5, 0.0, -1.0],
        },
    );
    s.run(0.3);
    let h = s.play(
        "sprayFireSound",
        Placement::Attached {
            entity: e(3),
            position: [0.5, 0.0, -1.0],
        },
    );
    s.run(1.5);
    if let Some(h) = h {
        s.rt.stop(h).ok();
    }
    s.run(0.7);
    out.push(s.finish(
        "06_spray_loop_stop",
        "spray activate, firing loop for 1.5 s, stopped (5 ms declick)",
        Some(dir),
    )?);

    // 7. Music brick: streamed OGG, listener walks away; music toggled off/on.
    let mut s = Scene::new(bank);
    s.play(
        "musicData_Peaceful",
        Placement::Attached {
            entity: e(4),
            position: [0.0, 0.0, 0.0],
        },
    );
    for i in 0..=40 {
        s.rt.set_listener(Listener {
            position: [0.0, 0.0, i as f32],
            ..Listener::default()
        })
        .ok();
        s.run(0.1);
    }
    s.rt.set_listener(Listener {
        position: [0.0, 0.0, 5.0],
        ..Listener::default()
    })
    .ok();
    s.run(1.0);
    s.rt.set_music_enabled(false).ok();
    s.run(1.0);
    s.rt.set_music_enabled(true).ok();
    s.run(2.0);
    s.rt.despawn(e(4)).ok(); // brick removed
    s.run(0.1);
    s.notes.push("AudioMusicLooping3d: reference 10, max 30; silent beyond 30 units; disabling music suspends, enabling restarts from the top".into());
    out.push(s.finish("07_music_brick_walk", "musicData_Peaceful streamed at a brick; listener walks 0->40 units, returns to 5, music off 1 s, on", Some(dir))?);

    // 8. Voice pressure: 64 hammer hits in half a second.
    let mut s = Scene::new(bank);
    for i in 0..64 {
        let d = (i % 16) as f32;
        s.play(
            "hammerHitSound",
            Placement::World([d * 0.2, 0.0, -1.0 - d * 0.1]),
        );
        if i % 8 == 7 {
            s.run(0.0625);
        }
    }
    s.run(1.0);
    s.notes.push("16 real voices (TGE MAX_AUDIOSOURCES); quieter one-shots culled; every voice retired by the end".into());
    s.notes.push("16 simultaneous close hits exceed full scale: output is hard-clamped and counted (vanilla OpenAL had no limiter either)".into());
    out.push(s.finish("08_voice_pressure", "64 hammer hits in 0.5 s", Some(dir))?);

    // 9. Gain buses.
    let mut s = Scene::new(bank);
    let lp = s.play("sprayFireSound", Placement::World([0.0, 0.0, -2.0]));
    s.run(0.7);
    s.rt.apply_ui_volume("sim", 0.3).ok();
    s.run(0.7);
    s.rt.apply_ui_volume("sim", 1.0).ok();
    s.rt.apply_ui_volume("master", 0.0).ok();
    s.run(0.7);
    s.rt.apply_ui_volume("master", 0.9).ok();
    s.play("Note5Sound", Placement::Listener);
    s.rt.apply_ui_volume("shell", 0.0).ok();
    s.run(0.7);
    if let Some(h) = lp {
        s.rt.stop(h).ok();
    }
    s.run(0.2);
    out.push(s.finish(
        "09_gain_buses",
        "effects loop: sim 1.0 -> 0.3 -> 1.0, master 0 then 0.9; menu note with shell 0 (silent)",
        Some(dir),
    )?);
    Ok(out)
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let get = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
    };
    let pack = get("--pack").ok_or("usage: bri-audio-evidence --pack <dir> --out <dir>")?;
    let out = get("--out").ok_or("usage: bri-audio-evidence --pack <dir> --out <dir>")?;
    let pack_id = pack
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let wav_dir = out.join(format!("{pack_id}-renders"));
    std::fs::create_dir_all(&wav_dir).map_err(|e| e.to_string())?;

    let bank =
        Arc::new(SoundBank::load(&pack, &BankOptions::default()).map_err(|e| e.to_string())?);

    // Every ready sound once, 2D and at 5 units, through the real runtime.
    let mut per_sound = Vec::new();
    let manifest = bank.manifest().clone();
    for snd in manifest.sounds.iter() {
        let mut s = Scene::new(&bank);
        let ready = snd.is_ready();
        let secs = manifest
            .clips
            .iter()
            .find(|c| Some(&c.id) == snd.clip.as_ref())
            .map(|c| (c.duration_seconds as f32).min(4.0))
            .unwrap_or(0.1);
        s.play(&snd.id, Placement::World([3.0, 0.0, -4.0]));
        s.run(secs + 0.05);
        // Loops (and clips longer than the 4 s render cap) are stopped explicitly.
        s.rt.stop_all().ok();
        s.run(0.05);
        let mut r = s.finish(
            &snd.name,
            &format!(
                "{} ({})",
                snd.id,
                if ready { "ready" } else { "unavailable" }
            ),
            None,
        )?;
        r.notes.insert(
            0,
            format!(
                "looping={}, spatial={:?}",
                snd.playback.looping, snd.playback.spatial
            ),
        );
        per_sound.push(r);
    }
    let failures: Vec<&str> = per_sound
        .iter()
        .zip(&manifest.sounds)
        .filter(|(r, s)| {
            s.is_ready()
                && (!r.all_finite
                    || r.left.peak == 0.0
                    || r.stats.real_voices_at_end + r.stats.virtual_voices_at_end != 0)
        })
        .map(|(r, _)| r.name.as_str())
        .collect();

    let scenes = scenes(&bank, &wav_dir)?;
    let report = serde_json::json!({
        "pack": pack.display().to_string(),
        "output": "offline render only; no audio device was opened and nothing was played",
        "sample_rate": RATE,
        "channels": 2,
        "bank": {
            "sounds_ready": bank.ready_count(),
            "unavailable": bank.unavailable().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>(),
            "resident_bytes": bank.resident_bytes(),
            "streamed_clips": bank.streamed_clips(),
            "hashes_verified": true,
        },
        "per_sound_failures": failures,
        "scenes": scenes,
        "per_sound": per_sound,
    });
    std::fs::write(
        out.join(format!("{pack_id}-offline-renders.json")),
        serde_json::to_string_pretty(&report).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;
    println!(
        "{} sounds rendered ({} failures), {} scenes written to {}",
        manifest.sounds.len(),
        failures.len(),
        scenes.len(),
        wav_dir.display()
    );
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("render failures: {failures:?}"))
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bri-audio-evidence: {e}");
            ExitCode::FAILURE
        }
    }
}
