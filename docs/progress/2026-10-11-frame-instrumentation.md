# 2026-10-11 Frame spikes: session logs say where the time went

v0.2.9 release blocker 3 (brief items P0-04 and P1-01): make frame time
explainable, tell background pacing apart from GPU cost, and find the
100-390 ms spikes in the v0.2.8 logs.

## What the v0.2.8 evidence shows

From the log tail in `crash-20261010-214554.txt` (session
`20261010-210717`, Bedroom then Skylands, RTX 4070 SUPER, DX12):

- **The 20 fps minutes are the background timer.** `platform.rs`
  `about_to_wait` schedules the next tick 50 ms out while the window is
  unfocused (`next_tick = now + 50 ms`) and still draws each tick. Minutes
  such as "4813 frames, 80 fps average, median 6.1 ms, 1% slowest 50.5 ms,
  692 frames under 30 fps" are about 692 frames of exactly 50 ms (35 s)
  plus about 4100 frames at 6.1 ms (25 s): a minute partly alt-tabbed. The
  logs had no focus marker, so these read like a 20 fps GPU. Confirmed from
  the code; the runtime correlation now shows in new logs.
- **The three biggest spikes coincide with renderer rebuilds.** 390.6 ms and
  336.9 ms fall in the minutes that log "Compiled scene pipelines" and
  "Add-On code: GPU measured", which are only written when `gpu_ready`
  rebuilds every renderer (a Shadow Quality or anti-aliasing change, or a
  map change). 245.6 ms is the minute of the Skylands map change. Max was
  switching Shadow Quality for the Bedroom screenshots then. The 353.1 ms
  minute has no rebuild line and stays unattributed.
- **Across all eight v0.2.8 session logs** (read from Max's install, read
  only), 11 of the 12 frames of 245 ms or more fall in a minute with a
  renderer rebuild ("Compiled scene pipelines" after joining) or a map
  change: 707.8 ms (two Shadow Quality changes, `142458`), 466.8
  (`145646`), 390.6, 336.9, 245.6 (`210717`), 383.0, 344.1, 289.5, 276.8,
  258.2, 257.4 (`214615`). The twelfth is the 353.1 ms above. The minute
  after joining also carries one 97-423 ms frame in every session (the
  join's own first frames). Frames of 90-120 ms with no event otherwise
  appear about once per long session.
- **Add-On GPU calibration reads low at join.** The first measurement in
  each session is 3.7e8 to 1.4e9 shader operations per ms; the ones after a
  rebuild read 1.4e9 to 4.8e9, up to 13 times more, on the same RTX 4070
  SUPER. That fits a GPU still at idle clocks when the game enters (an
  inference, not measured). With calibration now once per adapter, a
  session keeps its join-time figure after a graphics change, where v0.2.8
  happened to re-measure; Add-On shader loop caps were already set from the
  join-time figure in every session without a rebuild. Fixed here (see
  "Calibration warms the GPU first" below).
- A rebuild does all of this on the main thread in the next frame: waits
  for the world's pipelines (`Building::wait`, 47 ms in that log, seconds
  with FXC), uploads the map and every brick chunk again (about 188k bricks
  in Max's Bedroom), uploads the light volume, rebuilds the effects atlas
  and Add-On layer pipelines, and measured the GPU for Add-On code again.
- The 20 fps Bedroom screenshot's overlay (FPS 20, GPU 90%) is not the
  game's; 90% GPU at 20 fps suggests real GPU cost (about 45 ms a frame),
  not only the background timer. Unproven either way until a new log's GPU
  line shows the passes at each Shadow Quality.

## What changed

- **`crate::frame_trace`** (client): named spans on the main thread and
  one-off notes, closed per frame by the platform loop into a
  `FrameRecord`. Spans cover the platform loop (interface, update, commands,
  acquire, draw, interface draw, submit, present, idle in the event loop)
  and the app's frame (network, files, local game, combat, background jobs,
  world presentation with avatars, brick debris, Add-On bodies, effects,
  weather; drawing with prepare, graphics rebuild, pipelines wait, world
  upload, light volume, chunk uploads, Add-On code, Add-On GPU calibration,
  record passes). Notes mark focus changes, window moves and resizes, a lost
  device, map setup, map and chunk uploads and Add-On calibration.
- **Session log** (`quality::FrameLog`, player sessions only):
  - The minute summary counts frames paced by the background timer apart:
    "Frame times over 60 s (25 s focused): ...; 692 more frames over 35 s
    with the window unfocused or hidden, paced at 20 fps on purpose and not
    counted here". The first part keeps its old wording for focused frames.
  - A focused frame whose work (less the event loop's idle wait, which a
    frame cap spends on purpose) is 50 ms or more gets its own line, at most
    one a second and 20 a minute; the summary counts the rest:
    "Long frame: 390.6 ms (usual 6.1 ms), window focused: draw 381.0 ms
    (world upload ..., chunk uploads ..., pipelines wait ...), ...;
    events: graphics rebuilt ...".
  - "Main thread per focused frame, average ms: ..." per top-level span.
  - "GPU time per drawn frame over 60 s: median, 1% slowest, worst; average
    ms per pass: sky exposure, sun shadows, lamp shadows, mirrors, world,
    occlusion, world blended, effects". Player sessions now time the world's
    passes every frame (the existing `GpuTimer`, timestamps between passes;
    shading output is unchanged). `GpuTimer::readings` lets the log count
    each read-back frame once.
- **Add-On GPU calibration runs once per adapter**, not after every
  renderer rebuild. It submits a dozen small passes and waits for the GPU
  to finish everything queued after each (`PollType::Wait`) on the main
  thread. `ClientCode::gpu_stopped` keeps the speed; a lost device
  forgets it. Regression test
  `the_gpu_is_measured_once_per_adapter_not_per_renderer` failed against the
  old `gpu_stopped` ("measured again on the same GPU") and passes now.
- **Calibration warms the GPU first.** `speed_from_passes` keeps timing
  passes until the GPU has been busy for 150 ms of GPU time and the fastest
  pass has held for three passes in a row (a pass 3% faster resets that),
  raising the loop cap whenever the GPU speeds up enough that a pass drops
  under 10 ms. It stops after 500 ms of GPU time or 64 passes at one cap.
  The empty-pass cost is timed again at the end, warm, which can only lower
  the speed. Cost: on a warm RTX 4070 SUPER the highest loop cap is a
  1.4 ms pass, so calibration now runs 64 short passes (about 90 ms of GPU
  time) where it ran about 6; once per adapter, at join, and only when
  client Add-On code is running. Regression test
  `a_gpu_at_idle_clocks_is_measured_once_it_has_sped_up` models the v0.2.8
  figures (3.7e8 cold, 4.8e9 warm, clocks rising over 120 ms of load, and
  a jump at 120 ms); against the old loop it measures 2.53e9, half the real
  speed, in 69 ms, and passes now within 5%.
  `a_warm_gpu_is_measured_in_about_the_warm_up` bounds the time on a GPU
  that is already warm.
- **`large_build_perf` benchmark**: `BRI_PERF_EYE`/`BRI_PERF_AT` add a view
  (a corner of the Bedroom), `BRI_PERF_SHADOWS=0,2` switches Shadow Quality
  as Options does and reports the change frame with its breakdown, then
  steady frames and per-pass GPU time at each level. Every view reports its
  slowest frame's breakdown.

## Windows evidence (Max's PC)

`large_build_perf` on Max's RTX 4070 SUPER (DX12) with his own settings
at 1920x1080, on Crazy Collage Madness (187,762 bricks, the game's own
`.world.json` save, which the benchmark now loads), at two fixed Bedroom
views agreed with the shadows thread (`roomA`, `roomB`). Runs A and D;
runs B and C were contaminated by other builds on the PC and were rerun
as B2 and C2 below.

Clean reruns on a quiet PC (warm pipeline cache; run A had a cold
42 s pipeline compile before measuring):

| Run | Settings | Steady frame p50 / max (ms) | GPU p50 (ms) | Pass p50 at the custom view (ms) |
| --- | --- | --- | --- | --- |
| B2 | Max's settings at 2560x1440 (Classic, render scale 50) | 3.4-6.6 / 8.5 | 1.4-2.4 | sun shadows 1.0, world 1.3 |
| C2 | Unified, AO and soft shading, MSAA 4, scale 100, 1080p | 4.4-12.3 / 16.0 | 1.8-6.4 | occlusion 1.2, sun shadows 1.3, world 3.3 |

Sun shadows cost 1.0-2.0 ms of GPU time per frame in every view and AO
(occlusion) up to 1.2 ms; the world pass is the largest at 3.3 ms in the
heaviest view. The Shadow Quality change frames were 673-679 ms (B2) and
899-907 ms (C2): chunk uploads 365-380 ms, world upload 154-279 ms,
prepare 24-44 ms, pipelines wait 7-30 ms, GPU wait 62-104 ms.

- **Outside renderer rebuilds there are no long frames.** The slowest
  steady frame in every view was 5-12 ms.
- **A Shadow Quality change takes one 700-962 ms frame**, for example
  "Shadow Quality 2: the change took a 764.2 ms frame: draw 695.9 ms (chunk
  uploads 414.4, world upload 182.3, prepare 33.4, pipelines wait 25.7,
  record passes 2.0, light volume 0.7), GPU wait 62.5 ms". Across the
  changes: chunk uploads 380-510 ms (316 chunks, 5.9M vertices), world
  upload 160-235 ms, prepare 28-48, pipelines wait 25-39, GPU wait 60-70.
  So the rebuild spikes in the v0.2.8 logs are the world being uploaded
  again, not the shader compile.

## Settings rebuilds keep the world uploaded

A rebuild for a graphics option (Shadow Quality, anti-aliasing, colour
vision) on the same device and map no longer drops the world's GPU scenes.
`render.rs` takes the map scene, terrain, brick palette and chunks, and the
renderer that uploaded them, out before `gpu_ready` and puts them back
after; the new renderer takes them over on its first world frame with
`SceneRenderer::adopt_scenes`. Material bind groups stay valid because both
renderers build the same material layout descriptor, which wgpu-core
deduplicates into one layout; the new renderer takes the old one's chunk
pools and its environment probe map (which Metal materials sample) and
rebuilds its camera bind groups. The light volume is bound again and the
environment probe redrawn, as before. A map change or a new device still
uploads everything; a failed adoption falls back to a full upload with a
console warning.

Regression test `graphics_rebuild.rs`
(`a_shadow_quality_change_keeps_the_world_uploaded`, made-up content; an
ignored variant on v20 content): a 864-brick block, Shadow Quality 0 to 3.
Without the fix it fails ("the map was uploaded again: ... world upload,
chunk uploads ...; events: ... uploaded the map"); with it, the change
frame has no world upload, the new renderer uploads no palette geometry,
and the settled frame matches one drawn after a fresh upload (0 of 76,800
pixels differ).

## Tests

- `cargo test -p bri-client --lib -- frame_trace quality client_code`:
  frame trace nesting, notes and descriptions; background frames never
  counted as slow (the v0.2.8 50.3 ms pattern); long-frame lines, rate limit
  and the frame-cap exclusion; GPU and main-thread averages; calibration
  once per adapter (on llvmpipe here).
- `cargo test -p bri-client-sandbox -- --include-ignored`: all pass,
  including the two calibration tests and the llvmpipe render tests.
- `cargo clippy -p bri-client -p bri-render -p bri-client-sandbox --all-targets -D warnings`.
- `cargo test -p bri-client --test graphics_rebuild` (fails without the
  keep, passes with it), and `cargo test -p bri-client --lib --
  render_scale save_picture world_pipelines frame_trace`.
- `cargo clippy -p bri-client -p bri-render --all-targets -D warnings`.
- `large_build_perf` needs content, a save and a real GPU; it ran on Max's
  PC (above), not in this container.

## Next

- Re-measure the Shadow Quality change frame on Max's PC with this branch.
- The pipelines wait (25-39 ms there, seconds with FXC or a software
  adapter) and the new shadow maps' first frames remain in the change frame;
  drawing the old renderer until the new one compiles would remove the
  first if it matters.
