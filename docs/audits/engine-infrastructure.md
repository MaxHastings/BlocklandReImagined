# Engine infrastructure audit (2026-09-27)

Measured with `perf_probe` (headless: real map + saved build, 8 players, bots
in a minigame, 4 vehicles; offscreen GPU at 1080p on an RTX 4070 SUPER):

```powershell
cargo run --release -p bri-client --bin perf_probe -- content artifacts/perf/golden-gate.json "Golden Gate"
```

## Fixed

- Server tick overran its 8.3 ms budget on 47% of ticks on Golden Gate Bridge
  (mean 10.9 ms, max 82 ms). Bot sight rays queried the brick index with the
  whole ray's bounding box. Rays now walk index buckets in order: mean 1.5 ms,
  1 tick over budget in 20 s.
- Dark-map floor seams: interior lightmap coordinates reached the next
  surface's texel in the shared sheet. Coordinates are now remapped onto the
  texels centered inside each surface.

## Remaining, by payoff

1. Any brick change rebuilds the whole world mesh (Golden Gate: ~460 ms on a
   worker, 108 MB re-upload) and re-diffs every brick for collision on the
   frame thread (~50 ms hitch). Chunk the brick mesh and collision mirror
   (e.g. 32-unit chunks) so one plant touches one chunk.
2. No texture mipmaps or anisotropic filtering anywhere: distant carpet,
   terrain and brick tops shimmer. Generate mips at upload.
3. No shadows. Sun cascaded shadow maps (players, bricks, vehicles, items,
   interiors) with a quality option; point-light shadows optional later.
4. No MSAA: brick edges alias. 4x MSAA option.
5. Adapter uses the default power preference; laptops may pick the iGPU.
   Request HighPerformance. VSync off runs an unbounded busy loop; add a
   frame cap option.
6. Server authority loop is timer-driven by tokio at 120 Hz; Windows timer
   resolution (15.6 ms) makes pose sends bursty. Call timeBeginPeriod(1) or
   use a high-resolution waitable timer.
7. Pose datagrams are JSON, one per player per peer (800/s/peer at 16
   players, ~217 KB/s). Batch per peer and use a compact binary encoding
   (protocol change; coordinate with the netcode owner).
8. Late-join checkpoint is built and zstd-encoded on the authority loop
   (Golden Gate ~360 ms stall for everyone). Move encoding off the loop.
9. Audio: no default-device change handling (unplugging a headset silences
   the game until restart); 16 real voices is Torque's limit and will cut
   weapon/vehicle sounds in busy minigames. Raise to ~48 and reopen the
   stream on device loss.
10. rapier broadphase step with 44k static colliders costs ~0.8 ms/tick;
    acceptable now, watch it.
