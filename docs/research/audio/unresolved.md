# Unresolved audio behaviour

These items need a decision, engine evidence we don't have, or Maxwell's
listening judgment. None of them blocks integration. Each has a documented
default in the runtime or pack.

## Missing or ambiguous source content

1. **Title music is absent.** `TitleMusic` (`allClientScripts-Vanilla.cs:137`)
   names `~/data/sound/music/Ambient Deep.ogg`. The reference has no
   `base/data/sound/music/` directory, so vanilla v20 as installed plays nothing
   in `MainMenuGui::PlayMusic` (`:14148`). A byte-for-byte candidate exists:
   `Add-Ons/Music/Ambient_Deep.ogg` (`v20/music/ambient_deep`). The pack keeps
   `v20/sound/titlemusic` *unavailable* and does not substitute on its own.
   **Maxwell decides:** silent menu (faithful to this install) or bind
   `ui.title_music` to `v20/music/ambient_deep`. If bound, it should play 2D on
   the interface channel at volume 0.8 (`AudioBGMusic2D`).
2. **`AudioButtonOver` is absent.** `~/data/sound/buttonOver.wav` does not exist
   and no GUI profile references `AudioButtonOver`. Stock menus play
   `Note0..Note11Sound` on hover instead (`EM_*::onMouseEnter`,
   `MM_*::onMouseEnter`). Treat it as unused.
3. **Unreferenced files**, preserved in the pack but named by no profile:
   - `base/data/sound/banned.wav`
   - `base/data/sound/clickSuperMove.wav`: the name suggests super-shift brick
     moves, but neither the scripts nor the executable strings mention it.
   - `Add-Ons/Projectile_Pong/pongPaddleHit.wav`
   - `Add-Ons/Projectile_GravityRocket/rocketLoop.wav`: identical bytes are used
     through Weapon_Rocket_Launcher.
4. **GravityRocket fallback path.** Its `isObject`-guarded `rocketLoopSound`
   names `./sound/rocketLoop.wav`, which does not exist. It only matters if
   Weapon_Rocket_Launcher is disabled or loads later; in the default set the
   Rocket Launcher definition wins. The two `rocketExplodeSound` definitions
   point at identical bytes.
5. **Dead script references.** `serverCmdLight` calls
   `%player.playAudio(0, lightOff)` and `(0, LightOn)`
   (`allGameScripts-Vanilla.cs:4775, 4788`). Neither profile exists, so vanilla
   plays only the `ServerPlay3D(lightOn/OffSound)` next to them. Do not
   "fix" this into a double sound.
6. **Unbound profiles:** `ArmorMoveBubblesSound` and `WaterBreathMaleSound`
   (underwater loops) are defined but never bound. They are kept, but vanilla
   never played them.

## Engine behaviour not verified against the Blockland binary

7. **Gain curve.** The TGE `linearToDB` table is the default. At master 0.9 it
   produces about -7.7 dB. A/B against `GainCurve::Linear` while listening
   ([runtime model](runtime-model.md#gain-curve-engine-family-medium-confidence)).
8. **Panning law.** Equal-power stereo panning approximates OpenAL's generic
   software panning. There is no HRTF and no Doppler: TGE-family code sets no
   velocities, so `Listener` has no velocity field.
9. **Engine-played brick sounds:** where `BrickPlant/Move/Rotate/Change` are
   emitted (ghost brick or player), and whether `BrickBreak` plays at the brick.
   The runtime accepts any position; the trigger table marks them `world`.
10. **Voice culling details.** TGE scores real 3D sources with `ref/dist` and
    restarts culled loopers. The runtime uses linear attenuation scores and
    keeps loopers in phase. Adjust `VoicePolicy` if a playtest shows audible
    differences, e.g. `max_real_voices` above 16 for large servers.
11. **Assumed add-on load order** (only matters for duplicate datablocks, which
    currently all resolve to identical bytes): base, launcher patch, then RTB
    first and the rest alphabetically. `findFirstFile` order is filesystem
    dependent.
12. **Dedicated servers:** `$SimAudioType` is defined client-side only, so a
    dedicated v20 server would network channel 0 for sim sounds. The pack
    assumes the listen-server value (2), so the Sim slider works.

## Behaviour owned by gameplay integration (Astra)

13. Which gameplay code emits each trigger: image state machine `stateSound`,
    projectile loops, explosion `soundProfile`, PlayerData water/jump fields,
    vehicle `softImpactSound`/`hardImpactSound` thresholds, and the Item_Sports,
    Item_Skis and Tutorial scripts. The trigger table in `coverage.md` names the
    datablock/field or function for every one.
14. Event outputs `fxDTSBrick.playSound` / `GameConnection.playSound` and the
    wrench sound brick menu. Membership is precomputed in `SoundEntry.lists`
    (`event-param:Sound`, `event-param:Music`, `wrench:Sound`), but the
    event/wrench plumbing is Astra's.
15. `$Pref::Audio::MenuSounds`, `$Pref::Audio::PlantErrorSound`,
    `PlayBrickMoveSound` and `PlayBrickPlantSound` gate *whether the client
    emits* a sound. They are not runtime volumes. See the settings table in
    [integration.md](integration.md).
16. Launcher-patch profiles `fastImpactSound`/`slowImpactSound` ("made default
    in v21", `base/server/scripts/allGameScripts.cs:28`) are used by the stock
    vehicles. They are part of the designated reference install, and are marked
    `layer: launcher-patch`.
17. Return to Blockland UI sounds (6 profiles) are inventoried and playable, but
    RTB services are not an alpha requirement (`docs/vanilla-reference.md`).

## Not claimed

No sound has been listened to. No device was opened, and no game window or
playtest was run. Timing and feel in real gameplay remain for Maxwell's
playtest after Astra integrates the runtime.
