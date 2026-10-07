# 2026-10-07 Slate "froze" on the loading screen: a silent shader wait

A friend of Max loading Slate on v0.2.4 or v0.2.5 sat on the loading screen
until they pressed Esc; Bedroom then loaded fine.

## What was checked

- Hosting Slate headless with a real offscreen GPU and a fresh state folder
  (a scratch probe driving `App::tick` and printing every loading status):
  - origin/main 498d7f58 on the main checkout's `content`: single player,
    LAN and internet hosts, and Classic, Unified and Dynamic lighting all
    entered the game (8 to 47 s on this PC while other builds ran).
  - v0.2.5 (tag, 2cf0b3a4b) on a copy of the v0.2.5 release's `content`:
    single player and internet hosts entered in 16 s.
  - v0.2.5: a guest with six fewer Add-Ons joined a Slate host over
    loopback, downloaded them, reloaded its Add-Ons, rejoined and entered.
- No stage on this PC stalls. The network stages already time out
  (`PEER_STALL`, `DOWNLOAD_IDLE`, the join's read timeouts).

## Cause (inferred; the friend's log was not available)

Entering waits for the world's scene pipelines, which compile on a worker
thread from the moment the GPU opens. When they are not ready yet, the
loading screen kept the last network stage up: `RECEIVING WORLD` with a full
bar, unchanged, for as long as the compile took. On 2026-10-06 a friend on an
AMD integrated GPU compiled them in 223 s (FXC), and their GPU then dropped
out, which restarts the renderer and compiles again
(`2026-10-06-lan-join-lost-gpu.md`). v0.2.5 falls back to FXC when the D3D12
runtime refuses DXC's shaders. That matches this report: the first map waits
minutes on a bar that looks finished, Esc leaves it, and the next map loads
at once because the compile finished meanwhile. Brick meshing after the
world arrives had the same silent bar.

## Change

- `bri_progress::Stage::CompilingShaders` ("COMPILING SHADERS"), a local
  stage. `App::show_entry_wait` names what entering still waits on once the
  network part is done: `BUILDING BRICKS` until the world's chunks are
  built, then `COMPILING SHADERS` until the pipelines are (also for a map
  change waiting on its restarted renderers, and again after a lost GPU).
- The session log records each loading stage with the map and how long the
  previous stage took, ending with `IN GAME (after N ms)`, so the next
  report's log says where a slow load sat.

## Evidence

- `a_game_entered_before_the_world_pipelines_compile_waits_on_the_loading_screen`
  now also asserts the held compile shows `COMPILING SHADERS`. Before the
  change it failed with `the held compile shows as "RECEIVING WORLD"`;
  after, it passes.
- `cargo test -p bri-progress`: 2 passed.
- `cargo test -p bri-client --test multiplayer -- --include-ignored`
  (BRI_CONTENT on the main checkout's content): both variants pass, the map
  change to Slate included.

## Next

- The friend's `logs` folder from the game folder would confirm the cause:
  look for `Shader compiler: FXC` and `Compiled scene pipelines in N ms`.
- A shader compile still has no upper bound: a slow GPU is not a failure,
  and Esc leaves the load.
