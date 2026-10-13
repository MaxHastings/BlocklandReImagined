# 2026-10-13 Mid-match freezes: body uploads, raw mouse input, tracing

Thread "Mid-match freezes", branch `claude/match-freezes-8yyv6k`. Max's
v0.2.11 Slayer and flag matches with bots logged main-thread freezes of
400 to 800 ms (`session-20261013-020835.log`, tail in
`crash-20261013-030638.txt`): a 477.6 ms frame with "avatar meshes 442.1",
a 787.0 ms frame with "commands 372.9" and "untraced 405.7", 418 and 486 ms
frames almost all "untraced", and 248 to 272 ms "present" frames.

## Causes found

- **Avatar meshes.** Every new body or outfit (Slayer dresses each player
  on spawn, so a round start renews them all in one frame) made a fresh
  `GpuScene` with its own copy of every texture. The Blockhead rig binds
  11 materials over 10 images: face and decal (256x256), four 512x512
  brick surfaces, a 128x128 and three 16x16 (read from Max's
  `avatar-pack-002`). Each upload mipmapped them on the CPU, hashed them
  with SHA-256 and wrote about 6 MB (4.6 MB plus mips) to new textures.
  The CPU part alone measured 11.4 ms per body in release (container
  CPU, 10 runs), before DX12 texture creation and copies, so a round's
  worth of bots respawning in one frame reaches the 442 ms.
- **Commands and untraced.** Every raw mouse motion event (`DeviceEvent`,
  at the mouse's polling rate) ran the whole interface pump, and the
  cursor was re-centred with `SetCursorPos` every second while playing.
  Time spent in winit and Windows between the game's handlers, in input
  handling and in the periodic cursor sync was not in any span, which is
  what "untraced" was. The logs cannot say which of these the 400 ms
  stalls were; the new spans below will.
- **Present.** `present` blocks when the GPU or driver is two frames behind
  (`desired_maximum_frame_latency: 2`). The host simulation runs on its own
  tokio runtime, so these stalls freeze drawing and input, not the match.
  GPU pass times in the same minutes peak at 56 to 108 ms, so the rest is
  driver-side work our timestamps do not see. Not changed here.

## What changed

- `bri-render`: `SceneRenderer::upload_cached` with a `TextureCache` keyed
  by image label, size, colour space and alpha cutoff. Texture creation
  moved into `image_texture`, which also keeps the image's signature so a
  cached image is never hashed again.
- Bodies upload through `AvatarMesh::upload_sharing` with one cache per body
  model (`Avatars::body_textures`, cleared when the device goes). A second
  body in an outfit uploads no texture; a new face uploads only that face.
- Whole body uploads in a frame stop after 4 ms (one always goes); the rest
  draw from the next frames, the player's own body first. A frame that
  spends 8 ms or defers any notes "uploaded N new bodies in X ms, M wait".
- Raw mouse motion is summed in `device_event` and handed to the interface
  once per frame before the tick (or before any other input, so order
  holds). It no longer pumps per event.
- The once-a-second cursor re-assert only re-clips (winit calls ClipCursor
  only when the clip was lost); it no longer moves the cursor.
- Tracing: a child span's own parts of 1 ms or more are listed in brackets
  ("world presentation 21.0 [avatars 18.2]"); new spans "input",
  "cursor" and "event loop" (time between the game's handlers); an
  interface action taking 20 ms or more notes its name and time.

## Checks

- `new_outfits_bind_the_textures_bodies_already_uploaded` (synthetic,
  lavapipe): texture counts across a repeated outfit, a new face and a
  restructure.
- `a_slow_child_names_its_own_slow_parts` for the bracketed spans.

## Not done, and next

- The cold start (`session-20261013-030712.log`: avatar preview pipelines
  8.8 s, scene pipelines 13.3 s, "Add-On draw setup" 6.1 s in the first
  frame) is DX12 pipeline compilation. wgpu 30's `PipelineCache` is
  Vulkan-only, so the client cannot keep its own DX12 cache; NVIDIA's
  driver cache is written at a clean exit, and both v0.2.11 sessions that
  night ended in crashes. "Add-On draw setup" is GPU calibration and the
  Add-On layer pipelines, made on the first in-game frame; moving them to a
  worker like the scene pipelines is a separate change.
- Sky exposure's per-minute cost follows building: any brick chunk change
  replans the sky field, redrawing the whole map into nine directions over
  the next frames, and every moving caster is drawn into all nine layers.
  It is not an unbounded growth in the code (casings and debris are
  capped); making the field update only changed chunks is a separate change.
- The 20 to 45 ms "world presentation" frames in the same minute are now
  broken down by the bracketed spans; read them in the next match log.
