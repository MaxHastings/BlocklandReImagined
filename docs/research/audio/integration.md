# Audio integration handoff (for Astra)

`crates/audio` (runtime) and `crates/audio-import` (offline converter) are
standalone crates, each with its own `[workspace]` table and `Cargo.lock`, so
they build without touching root manifests. Integration needs four steps.

## 1. Root integration patch

Root has now added both crates to the real workspace and shared lockfile, and
verified Windows tests plus linking with cpal-output. Do not apply this historical
patch wholesale over subsequent root changes. Client audio dependency, settings
and gameplay trigger integration remain pending.

Apply [`root-integration.patch`](root-integration.patch) from the project root
(`git apply docs/research/audio/root-integration.patch`). It changes:

- `Cargo.toml`: adds `"crates/audio", "crates/audio-import"` to `members`.
- Both crate manifests: remove the standalone `[workspace]` table and inherit
  `version/edition/rust-version/publish`, `serde` and `serde_json` from the
  workspace.
- `crates/client/Cargo.toml`: adds
  `bri-audio = { path = "../audio", features = ["cpal-output"] }`.

Then:

```powershell
Remove-Item crates/audio/Cargo.lock, crates/audio-import/Cargo.lock, crates/audio/.gitignore, crates/audio-import/.gitignore
cargo check --workspace          # updates the root Cargo.lock once (new: symphonia, cpal, rtrb)
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The patch was checked by applying it to copies of the current manifests
(`patch --dry-run`: clean) and building `bri-audio`, `bri-audio-import` and a
stub `bri-client` that depends on `bri-audio` with `cpal-output`, in a workspace
with stub members and the real root `Cargo.lock` (Rust 1.93.1, Linux). The
`bri-audio` and `bri-audio-import` tests also pass in that workspace form. It
was **not** checked against the real client sources, which Astra owns.

New dependencies:

| Crate | Version | Why | Notes |
| --- | --- | --- | --- |
| `symphonia` | 0.6 (features `wav,pcm,ogg,vorbis`) | decode | pure Rust, MPL-2.0 |
| `cpal` | 0.18, optional | device output | Linux builds need `libasound2-dev` (ALSA headers); nothing extra on Windows or macOS |
| `rtrb` | 0.3 | lock-free command/event rings | |
| `sha2` 0.10, `zip` =6.0.0 | — | pack verification, converter | same versions the workspace already uses |

MSRV: all dependencies need Rust 1.85 or newer; the project uses 1.93.

## 2. Create the runtime (client startup)

```rust
use std::sync::Arc;
use bri_audio::*;

let pack = content_root.join("audio-pack-001");
let bank = Arc::new(SoundBank::load(&pack, &BankOptions::default())?);   // ~10 MB resident, hashes verified
let (mut audio, why) = AudioRuntime::open_or_null(bank.clone(), RuntimeConfig::default());
if let Some(e) = why { log::warn!("audio output unavailable, continuing silent: {e}"); }

// Seed from persisted prefs (fall back to the pack's stock defaults).
let d = bank.defaults();
audio.set_volume(VolumeControl::Master, prefs.f32_or("$pref::Audio::masterVolume", d.master_volume))?;
audio.set_volume(VolumeControl::Interface, prefs.f32_or("$pref::Audio::channelVolume1", d.channel_volumes[1]))?;
audio.set_volume(VolumeControl::Effects, prefs.f32_or("$pref::Audio::channelVolume2", d.channel_volumes[2]))?;
audio.set_music_enabled(prefs.bool_or("$Pref::Audio::PlayMusic", d.play_music))?;
```

A dedicated server needs no audio: skip the runtime, or use `OutputKind::Null`
if shared code expects one.

## 3. Per frame

```rust
audio.set_listener(Listener { position: cam.pos.to_array(), forward: cam.forward.to_array(), up: glam::Vec3::Y.to_array() })?;
for e in world.entities_with_attached_sounds() {           // players, vehicles, projectiles, music bricks
    audio.update_entity(EntityKey(e.id), e.position.to_array())?;
}
audio.update(dt);                                           // no-op for device output, mixes Null/Offline
for ev in audio.drain_events() { /* optional: telemetry/debug overlay */ }
```

Positions are native Y-up world coordinates (the same values the renderer
uses). The listener is the camera, as in vanilla, where the listener follows
the control camera.

## 4. Emit and stop

```rust
// 2D UI:
audio.play_trigger("ui.client_join", Placement::Listener)?;
// Fixed position (explosions, ServerPlay3D):
audio.play("rocketExplodeSound", Placement::World(pos))?;
// Attached (playAudio slots, image stateSound, projectile loops, music bricks):
let h = audio.play("sprayFireSound", Placement::Attached { entity: EntityKey(player_id), position })?;
audio.stop(h)?;                          // state machine left the firing state
audio.despawn(EntityKey(projectile_id))?; // object deleted: its loop stops
```

Sounds can be named by trigger key, stable id (`v20/sound/...`,
`v20/music/...`), or vanilla datablock name (e.g. the `stateSound[2]` value from
converted datablocks). Missing clips return `Err(SoundUnavailable)`. Log it and
continue.

**Network note:** vanilla servers tell clients to play sounds by datablock
(`ServerPlay3D`, `play2D`, `playAudio`, `stateSound`, explosion
`soundProfile`). The native protocol is not changed here. Where the client
already replicates the corresponding event (explosion, image state, projectile
spawn, brick plant), it should derive the sound locally from the same authored
datablock reference. No audio-specific messages are required for the stock
content.

## Trigger map (vanilla gameplay → native sound id)

Curated semantic keys to wire first. The complete generated map (255 rows,
including every `datablock:` field binding and `call:` script site) is in
[coverage.md](coverage.md#trigger-map) and in `manifest.json` → `triggers`.

| Trigger key | Sound id | Placement | Vanilla evidence |
| --- | --- | --- | --- |
| `player.jump` | `v20/sound/jumpsound` | attached | `PlayerStandardArmor.JumpSound` (`allGameScripts-Vanilla.cs:8815`) |
| `player.water.impact_easy/medium/hard` | `v20/sound/splash1sound` | attached | `impactWater*` (`:8816-8818`), velocity thresholds 10/20 (`:8801-8802`) |
| `player.water.exit` | `v20/sound/exitwatersound` | attached | `exitingWater` (`:8824`) |
| `player.pain_cry` / `player.death_cry` | `v20/sound/paincrysound` / `deathcrysound` | attached | `Player::playPain` `:9614`, `Player::playDeathCry` `:9597` (per-datablock `painSound`/`deathSound` override) |
| `player.mount` | `v20/sound/playermountsound` | world | `Armor::onMount` `:8933` |
| `player.light_on` / `_off` | `v20/sound/lightonsound` / `lightoffsound` | world | `serverCmdLight` `:4787/4774` |
| `player.spawn` / `player.body_remove` | `spawnexplosionsound` / `deathexplosionsound` | world | explosion `soundProfile` `:10247/10124` |
| `brick.plant/move/rotate/change/break` | `v20/sound/brick*` | world | engine-bound client profiles (`allClientScripts-Vanilla.cs:152-182`); gated by prefs below |
| `tool.hammer.hit` | `v20/sound/hammerhitsound` | world | `hammerImage::onHitObject` `:10539` |
| `tool.wrench.hit` / `.miss` | `wrenchhitsound` / `wrenchmisssound` | world | `wrenchImage::onHitObject` `:10868-10966` |
| `tool.spray.activate` / `.fire` | `sprayactivatesound` / `sprayfiresound` (loop) | attached | spray images `stateSound[0]/[2]` |
| `tool.printer.fire` | `printfiresound` | attached | `printGunImage.stateSound[2]` |
| `tool.wand.hit` | `wandhitsound` | world | `wandExplosion`/`AdminWandExplosion` |
| `item.weapon_switch` | `weaponswitchsound` | attached | weapon images `stateSound[0]` |
| `weapon.*` (gun, bow, rocket, spear, sword, broom) | see coverage | attached/world | add-on `stateSound`/explosions/projectile `sound` |
| `vehicle.impact_soft/hard` | `slowimpactsound` / `fastimpactsound` | attached | vehicle `softImpactSound`/`hardImpactSound` (launcher-patch profiles) |
| `vehicle.explosion` | `vehicleexplosionsound` | world | `vehicleExplosion`, `vehicleFinalExplosion` |
| `music-brick:<Name>` | `v20/music/<name>` | attached (brick) | music brick / `setMusic` event (looping AudioEmitter) |
| `ui.client_join` / `ui.client_drop` / `ui.admin` / `ui.brick_clear` / `ui.upload_*` / `ui.process_complete` / `ui.item_pickup` / `ui.error` | `v20/sound/...` | listener | client handlers (`allClientScripts-Vanilla.cs:187, 213, 7108-7441`) |
| `ui.menu_note.N` | `v20/sound/noteNsound` | listener | `EM_*`/`MM_*::onMouseEnter` (see generated `call:` rows for the exact control → note) |
| `game.reward` | `rewardsound` | listener | tutorial triggers, minigame end, `GameConnection::onDeath` |

Event and wrench lists: sounds with `lists` containing `event-param:Sound` are
exactly those the stock `playSound` event menus offer, labelled by file name.
`event-param:Music` / `wrench:Sound` are the music-brick lists, labelled by
`ui_name`. `bri_ui::DatablockMenus` (`"Sound"`, `"Music"`) can be filled from
`bank.manifest().sounds` with those filters.

## Settings bindings needed in the existing UI (`crates/ui/src/screens/options.rs`)

| Control (stock GUI) | Pref | Runtime / client action | Current native UI |
| --- | --- | --- | --- |
| `OptAudioVolumeMaster` | `$pref::Audio::masterVolume` | `apply_ui_volume("master", v)` | slider works; emits `UiAction::SetVolume{"master"}` on Done |
| `OptAudioVolumeShell` | `$pref::Audio::channelVolume1` | `apply_ui_volume("shell", v)` | works |
| `OptAudioVolumeSim` | `$pref::Audio::channelVolume2` | `apply_ui_volume("sim", v)` | works |
| "Play Music" checkbox | `$Pref::Audio::PlayMusic` | `set_music_enabled(b)` | **disabled**: add to `LOCAL_PREFS` and forward on commit |
| "Menu Sounds" | `$Pref::Audio::MenuSounds` | client skips `ui.menu_note.*` when false | **disabled** |
| "Play Brick Plant Sounds" | `$Pref::Audio::PlayBrickPlantSound` | client skips `brick.plant` when false | **disabled** |
| "Play Brick Movement Sounds" | `$Pref::Audio::PlayBrickMoveSound` | client skips `brick.move/rotate` when false | **disabled** |
| "Play Brick Plant Error Sound" | `$Pref::Audio::PlantErrorSound` | client plays `ui.error` on plant errors only when true | **disabled** |
| `OptAudioDriverList` / `OptAudioInfo` | `$pref::Audio::driver` | show `audio.output_format()` / backend name | shows "Native" |

Notes:

- The options screen seeds checkboxes with `prefs.bool_or(var, false)`. Stock
  defaults are **true** for PlayMusic, MenuSounds, PlayBrickPlantSound and
  PlayBrickMoveSound, and false for PlantErrorSound (`bank.defaults()`). Seed
  them before enabling the controls, or they will default to off.
- Vanilla applies volume while dragging (`altCommand`) and plays `lightOn.wav`
  as a test tone. Optional: call `apply_ui_volume` on drag too.
- Optional native addition: a music volume slider (`VolumeControl::Music`). The
  stock UI has none.
