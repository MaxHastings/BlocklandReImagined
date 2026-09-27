//! How the native client drives the audio runtime. Runs headless by default
//! (offline output, nothing is played) so it is safe in CI:
//!
//! ```text
//! cargo run --example client_integration -- content/audio-pack-001
//! ```
//!
//! Maxwell-only, audible (requires `--features cpal-output`):
//! `cargo run --features cpal-output --example client_integration -- content/audio-pack-001 --device`

use std::sync::Arc;

use bri_audio::*;

/// Stand-ins for the client's own types.
struct Camera {
    position: [f32; 3],
    forward: [f32; 3],
    up: [f32; 3],
}
struct PlayerView {
    entity: u64,
    position: [f32; 3],
}
enum GameEvent {
    Jumped {
        entity: u64,
    },
    ProjectileSpawned {
        entity: u64,
        loop_sound: &'static str,
    },
    ProjectileMoved {
        entity: u64,
        position: [f32; 3],
    },
    Exploded {
        entity: u64,
        sound: &'static str,
        position: [f32; 3],
    },
    BrickPlanted {
        position: [f32; 3],
    },
    MusicBrick {
        brick: u64,
        track: &'static str,
        position: [f32; 3],
    },
    BrickRemoved {
        brick: u64,
    },
    UiSetVolume {
        channel: &'static str,
        value: f32,
    },
    UiMusicPref(bool),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let pack = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("content/audio-pack-001");
    let want_device = args.iter().any(|a| a == "--device");

    // 1. Load once at startup (verifies hashes, preloads PCM, keeps music streamable).
    let bank = Arc::new(SoundBank::load(pack, &BankOptions::default())?);

    // 2. Create the runtime. Device output is explicit; fall back to Null so a
    //    missing/broken device never stops the game.
    let config = RuntimeConfig::default();
    let mut audio = if want_device {
        let (rt, why) = AudioRuntime::open_or_null(bank.clone(), config);
        if let Some(e) = why {
            eprintln!("audio disabled: {e}");
        }
        rt
    } else {
        AudioRuntime::new(bank.clone(), config, OutputKind::Offline)?
    };

    // 3. Apply persisted prefs (bri_ui Settings / $pref::Audio::*).
    let d = bank.defaults().clone();
    audio.set_volume(VolumeControl::Master, d.master_volume)?;
    audio.set_volume(VolumeControl::Interface, d.channel_volumes[1])?;
    audio.set_volume(VolumeControl::Effects, d.channel_volumes[2])?;
    audio.set_music_enabled(d.play_music)?;

    // UI sounds: 2D.
    audio.play_trigger("ui.client_join", Placement::Listener)?;

    let mut camera = Camera {
        position: [0.0, 2.0, 0.0],
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
    };
    let player = PlayerView {
        entity: 1,
        position: [0.0, 0.0, -2.0],
    };
    let events = [
        GameEvent::Jumped {
            entity: player.entity,
        },
        GameEvent::BrickPlanted {
            position: [1.0, 0.0, -3.0],
        },
        GameEvent::ProjectileSpawned {
            entity: 50,
            loop_sound: "rocketLoopSound",
        },
        GameEvent::ProjectileMoved {
            entity: 50,
            position: [0.0, 1.0, -10.0],
        },
        GameEvent::Exploded {
            entity: 50,
            sound: "rocketExplodeSound",
            position: [0.0, 1.0, -20.0],
        },
        GameEvent::MusicBrick {
            brick: 900,
            track: "musicData_Peaceful",
            position: [5.0, 0.0, -5.0],
        },
        GameEvent::UiSetVolume {
            channel: "sim",
            value: 0.8,
        },
        GameEvent::UiMusicPref(true),
        GameEvent::BrickRemoved { brick: 900 },
    ];

    let dt = 1.0 / 60.0;
    for (frame, ev) in events.into_iter().enumerate() {
        // 4. Every frame: listener = camera; attached sources follow their entities.
        camera.position[2] -= 0.05;
        audio.set_listener(Listener {
            position: camera.position,
            forward: camera.forward,
            up: camera.up,
        })?;
        audio.update_entity(EntityKey(player.entity), player.position)?;

        // 5. Map gameplay events to stable sound ids / trigger keys.
        let r = match ev {
            GameEvent::Jumped { entity } => audio
                .play_trigger(
                    "player.jump",
                    Placement::Attached {
                        entity: EntityKey(entity),
                        position: player.position,
                    },
                )
                .map(drop),
            GameEvent::BrickPlanted { position } => audio
                .play_trigger("brick.plant", Placement::World(position))
                .map(drop),
            GameEvent::ProjectileSpawned { entity, loop_sound } => audio
                .play(
                    loop_sound,
                    Placement::Attached {
                        entity: EntityKey(entity),
                        position: camera.position,
                    },
                )
                .map(drop),
            GameEvent::ProjectileMoved { entity, position } => {
                audio.update_entity(EntityKey(entity), position)
            }
            GameEvent::Exploded {
                entity,
                sound,
                position,
            } => {
                audio.despawn(EntityKey(entity))?; // stops the projectile loop
                audio.play(sound, Placement::World(position)).map(drop)
            }
            GameEvent::MusicBrick {
                brick,
                track,
                position,
            } => audio
                .play(
                    track,
                    Placement::Attached {
                        entity: EntityKey(brick),
                        position,
                    },
                )
                .map(drop),
            GameEvent::BrickRemoved { brick } => audio.despawn(EntityKey(brick)),
            // bri_ui::UiAction::SetVolume { channel, value } maps directly.
            GameEvent::UiSetVolume { channel, value } => {
                audio.apply_ui_volume(channel, value).map(drop)
            }
            GameEvent::UiMusicPref(on) => audio.set_music_enabled(on),
        };
        // Missing clips / full queues are reported, never fatal.
        if let Err(e) = r {
            eprintln!("frame {frame}: {e}");
        }

        // 6. Tick (mixes inline for Null/Offline; no-op for device output).
        audio.update(dt);
        for e in audio.drain_events() {
            if let AudioEvent::Culled { handle, reason } = e {
                eprintln!("culled {handle:?}: {reason:?}");
            }
        }
    }
    audio.update(0.5);
    let s = audio.stats();
    println!(
        "started {} finished {} stopped {} rejected {} | voices real {} virtual {} | peak {:.3}",
        s.started, s.finished, s.stopped, s.rejected, s.real_voices, s.virtual_voices, s.peak
    );
    Ok(())
}
