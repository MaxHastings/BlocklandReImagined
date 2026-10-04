# 2026-10-04 Video script rehearsal for v0.2.4

Max's v0.2.4 showcase video walks a long script: startup, Bedroom building
and tools, ACM City with vehicles and weapons, bots, Add-On weapons, the
Gravity Gun and ragdolls, Pong and the event editor, MoTE Mansion and the
Kitchen from day into night, the Slopes, Skylands, a big Badspot Block
Party save with the F3 overlay, and three-player credits with emotes. This
thread rehearsed those beats in a release-like build and fixed what would
look broken on camera. Portal, mirror, steel ball, bot soccer, teams and
MiniNuke belong to other lanes and were left to them.

## How it was rehearsed

`crates/client/tests/video_rehearsal.rs` (ignored; it returns at once
without `BRI_REHEARSAL_OUT`) drives the normal App offscreen at 1920x1080 on
the real GPU, with the player's own `settings.json`, and records for each
beat every frame's time (update, record and the GPU wait a synchronized
present makes), frames over 50 ms, a screenshot per moment and the
console's warnings and errors. It runs against a packaged build's content
(`tools/package_playtest.ps1` output), with Add-Ons dropped in the
content's Add-Ons folder converted by the package's own importer, so the
default and imported Add-Ons load as in a release.

    BRI_CONTENT=<package>/content BRI_REHEARSAL_OUT=<folder> \
    BRI_REHEARSAL_SETTINGS=%LOCALAPPDATA%/BlocklandReImagined/settings.json \
    BRI_REHEARSAL_SAVES=%LOCALAPPDATA%/BlocklandReImagined/saves \
    cargo test -p bri-client --release --test video_rehearsal -- --ignored --nocapture

Tests: `bedroom_building`, `city_vehicles_weapons`, `maps_lighting_big_build`,
`credits_three_players` (a LAN host and two headless guests on UDP 28000),
and `scene_pipeline_compile_time`.

## Found and fixed

- **An 18.7 s frozen window on the first map after launch.** The world's
  pipelines compile on a worker at launch; on Windows FXC takes about 20 s
  (23.7 s measured for the scene pipelines on an RTX 4070 SUPER; Max's
  v0.2.2 logs show 18.8 and 22.9 s). Starting a game sooner entered it
  anyway and the first world frame waited on the compile. Entering, and
  finishing a map change, now also wait for the pipelines, so the loading
  screen stays up and responsive and no frame blocks on them. New App test
  `a_game_entered_before_the_world_pipelines_compile_waits_on_the_loading_screen`
  holds the compile and fails on the old code.
- **Red "ready" lines in chat on every spawn.** Gravity Gun Effects and
  Steel Ball Sounds logged "... ready" from their client code. Removed; the
  showcase tests now require their first frame to log nothing (both failed
  on the old modules).
- **"2 Add-On problems" in red chat for every admin.** The bundled
  Tier+Tactical Tier 2 and Sniper Skins name Tier 1's `pistolTrailEmitter`
  bare, as v20's global datablock names allow; imported emitters only had
  namespaced ids, so those tracers drew nothing and the health check said
  so in chat. Add-On emitters and lights now also answer to their symbol,
  the first keeping a name, as Add-On explosions already did.
- **Windows build break on the integration branch** (`splash.rs` integer
  clamp), reported and fixed there.

## Checked and fine

Startup reaches the main menu in about 1 s of content load; spray paint and
FX cans, printer, hammer and wrench dialogs, light/emitter/item/music on
bricks, brick search, avatar Randomize, the New Duplicator's selection and
plant mode, ACM City (16,478 bricks loaded in 0.3 s), jeep spawning,
rockets, bots, sniper zoom, HE grenade, the Gravity Gun, the admin camera,
Pong, the Mansion garden and Kitchen through dusk into night, Slopes with
skis, Skylands, Badspot's Block Party (68,746 bricks in about 6 s, about
5.5 ms frames over the whole build), the F3 overlay, three players with
/love, /hug, /hate, /alarm, /confusion and /wtf, jumping, spray paint and
hammers. Map changes show a 150-380 ms frame as the new map first draws,
behind the change.

## Open

- The pipeline compile itself is still about 20 s with FXC. The Windows
  SDK's DXC (`dxcompiler.dll` 1.8.2502 beside the executable, which wgpu's
  default compiler choice already picks up) compiles the same pipelines in
  3.3 s (1.2 s without `dxil.dll`). Shipping it is a packaging decision for
  Max.
- The F3 overlay draws over the "Q = tools" hint in the top-right corner.
