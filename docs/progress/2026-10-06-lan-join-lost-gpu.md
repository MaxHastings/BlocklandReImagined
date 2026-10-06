# 2026-10-06 LAN join report: a lost GPU crashed the guest

Max reported multiplayer broken, even on LAN, in v0.2.4 and the dev build
of main c337cd7: a friend's join "hangs forever" on Loading Graphics.

## What the evidence shows

- Joining works. The dev build's `bri-server.exe` (c337cd7) was run on
  127.0.0.1 with its own `content`, and a headless App with a real GPU
  (a scratch probe calling `gpu_ready` and `render_scene` each tick) joined
  it on Slate (in game at 6 s) and Bedroom (in game at 27 s), and stayed in
  until the server stopped. Windows Firewall on the host PC allows UDP
  28000 and 28050 for any profile through the game's port rule.
- The friend's copy was a source build (`target\release`) with no
  dxcompiler.dll, on an AMD integrated GPU. Its log: `Shader compiler: FXC`
  and `Compiled scene pipelines in 222994 ms`. Since b55f4508 (v0.2.4) the
  join waits on Loading Graphics for that compile instead of freezing the
  first world frame, so the guest sat there for almost four minutes.
- Then it crashed (`crash-20261006-042036.txt`): `create_buffer_init` in
  `SceneRenderer::upload` from `GpuTerrain::upload` panicked with
  `MapRangeError ... Buffer with 'v20/add-ons/map_bedroom/bedroom.ter' label
  is invalid`. A material uniform buffer only comes back invalid when the
  device is lost: the integrated GPU dropped out, and wgpu's helper panics
  mapping the invalid buffer before the next frame's device-lost check
  (`recover_gpu`) can restart the renderer.

## Change

`bri_render::BufferInit::buffer_init` replaces wgpu's `create_buffer_init`
in the render, foliage and client-sandbox crates: same padding and
contents, but a buffer that cannot be mapped (a lost device) is returned
unfilled instead of panicking. The next frame sees the loss and restarts the
renderer, as `recover_gpu` already does for losses noticed between frames.
Its test destroys a device and shows wgpu's helper panicking where ours
does not.

`cargo test -p bri-render --lib buffer_init`: 2 passed.

## Next

- Source builds on Windows still run on FXC unless dxcompiler.dll (pinned in
  `tools/shader-compiler.json`) sits beside the exe. Players should use the
  packaged build; `bootstrap.py` could place the DLL beside `target/release`.
- Why the friend's integrated GPU was lost is unknown; their next log after
  this fix shows whether the restart holds or the loss repeats.
