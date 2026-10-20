> **Historical note. Written 2026-10-12 (the file's date in the cloud project folder, `perf/large-build-rendering-recommendation.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Large-build rendering: what to do for v0.2.11

Date 2026-10-12, main at `f4ce0ae` (v0.2.10). This was a code read plus the measurements already on file. Nothing new was run on a GPU for this note.

## Short answer

Don't put any of the six proposals into v0.2.11 as written. Max's lag view is already a benchmark view: it is `roomA` in `large_build_perf`. For that view, four of the six proposals cannot help. The two that can (fewer triangles and smaller vertices) are worth building only after one measurement shows whether the frame is limited by geometry or by pixels. v0.2.11 should carry that measurement. If it says geometry, it should also carry merged brick faces.

## Max's view is already measured

The screenshot shows Crazy Collage Madness from the high corner of the Bedroom at 3440x1440. The overlay reads FPS 83, GPU 94%, CPU 8%. That overlay is NVIDIA's, not the game's. The waste audit's custom view `roomA` (eye (-40, 405, 200) looking at (95, 330, 98)) is the same shot. On Max's PC at 2560x1440 with his settings (`perf/waste-audit-v0.2.10.md`, baseline table):

| | roomA | spawn view for comparison |
|---|---|---|
| frame p50 (p95) | 13.28 (15.67) ms | 9.90 ms |
| GPU p50 | 6.82 ms, of which world pass 5.08 | 6.16 |
| CPU record p50 | 5.62 ms (p95 7.8) | 2.77 |
| chunks drawn | 218 | 38 |
| triangles | 2.99 M | 744k |

In real play at 3440x1440, the same build in the Bedroom ran at GPU median 9.0 ms, of which the world pass was 5.75 ms (session 134836). The cost of this view is the world pass drawing about 3 M triangles from about 218 chunks, plus the CPU time to record those draws. Shadows (0.36 ms) and AO (0.71 ms) are small, and particles were fixed in v0.2.10 (PR #69).

The M1 Pro is roughly six times weaker than the 4070 SUPER on the GPU, and its Retina panel has more pixels (about 5.9 MP against 5.0). A 9 ms GPU frame on the PC is therefore plausibly 40-60 ms there. That figure is inferred, not measured. Render Scale (100/85/70/50) has been in the game since 2026-10-05 for exactly this case, and it is the immediate workaround on the Mac.

## Each proposal against main and against this view

| # | Proposal | Claim true on main? | Helps roomA? | Verdict for v0.2.11 |
|---|---|---|---|---|
| 1 | Distance LOD for chunks | True: one representation per chunk, no LOD code (`world_chunks.rs`, `world_scene.rs::append_world_brick`) | Yes for the far half of the room. A 1x1 brick at the far wall covers about 2 pixels at 3440 wide. | Later. It needs a simplifier, popping control and a second mesh per chunk. Do it after #6 if geometry still dominates. |
| 2 | Compact brick vertex (72 to about 32 bytes) | True: `SceneVertex` is 72 bytes (`render/src/scene.rs:40`). The 1.7 GB per million bricks comment is at `app/mod.rs:1796`. | Partly. It cuts vertex bandwidth, and that matters most on the M1's shared memory. Depth-only shadow passes also pull the full 72-byte stride. | Strong second candidate, especially for the Mac. A cheaper first step is a position-only stream for shadow and depth passes. |
| 3 | GPU geometry streaming with a VRAM budget | True: every built chunk stays resident (`gpu_chunks`). | No. Everything in this view is on screen, and 188k bricks is roughly 300 MB of vertices, which fits on both machines. | Not until builds approach a million bricks or low-VRAM GPUs are a target. |
| 4 | HZB occlusion culling | True: chunks are tested against the frustum only. | No. The corner overview sees the whole room, with nothing in front of it. | Not now. It would only pay off inside buildings. |
| 5 | Per-frame upload budget | True: `app/render.rs:143-182` uploads every pending chunk in one frame. | No. It affects hitches on load and edit, not the steady frame rate in a still view. | Small and safe whenever someone touches loading hitches, but it is not this symptom. |
| 6 | Adaptive clusters and coplanar face merging | True: hidden faces are dropped (`brick_cover.rs`), but the faces that remain are emitted per brick. | Merging, yes, for walls, floors and baseplates of plain same-colour bricks with no visual change. Sparse-chunk clustering, no: 218 draws are cheap with indirect multi-draw. | Best first structural step if the measurement says geometry. |

Every "true on main" claim in the pasted analysis checked out. Its Brickadia figures were not verified and do not matter here.

## What I would do instead, in order

1. **Measure geometry against pixels (v0.2.11, small).** Run `large_build_perf` on Collage roomA at 3440x1440 three ways: Render Scale 100% against 50%, MSAA on against off, and Dynamic against Unified. If the world pass barely moves at 50% scale, the frame is geometry-bound and steps 2-3 are right. If it roughly halves, it is pixel-bound, and the work is in the fragment path instead: the per-fragment lamp-shadow and cascade cost under MSAA, and whether the world pass needs a depth prepass. Ask Max for one Mac session log taken in that corner. The log's per-pass GPU line shows where the M1's time goes, and nothing in the project measures the Mac on this build yet.
2. **Merge coplanar same-material faces of plain cuboid bricks** in the chunk builder. This is the merging half of #6. It is exact, so nothing pops, and it reduces the triangles in every pass, shadows included. Keep prints, FX, BLB meshes and rotated special bricks on today's path.
3. **Compact the brick vertex (#2)**, starting with a position-only stream for shadow and depth passes.
4. **Distance LOD (#1)** only if steps 2 and 3 leave the far half of large rooms too heavy.
5. **Record-side CPU.** roomA spends 5.6 ms recording 218 chunks. After v0.2.10's fixes that has not been re-profiled. On Metal, wgpu turns `multi_draw_indexed_indirect` into a loop of separate draws, so on the Mac the batching saves less than it does on DX12. Re-profiling roomA's record pass belongs in step 1.

Streaming (#3), HZB (#4) and the upload budget (#5) wait until there is a million-brick test build or a view where something blocks the camera.
