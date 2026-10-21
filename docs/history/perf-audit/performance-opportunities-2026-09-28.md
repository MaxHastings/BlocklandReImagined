> **Historical note. Written 2026-09-29 (the file's date in the cloud project folder, `perf-audit/performance-opportunities-2026-09-28.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# Performance opportunities outside the open lanes (2026-09-28)

Audit of `main` at 9847029 (protocol 52). Scope: everything a player or host
feels except what the open lanes already own (brick chunk rendering, entity /
particle / vehicle / debris / water / avatar skinning, brick loading and
transfer). "Measured" means run on a 4-core Linux cloud box; everything else is
inferred from the code and marked so. The Windows game was not run.

Branch with the fixes below: `claude/project-thread-cw15he` at c388889a0 (current
main merged in). Clippy `-D warnings` is clean for bri-net and bri-client;
all bri-net tests pass; bri-client's 197 unit tests and its content-free
integration tests pass (only `brick_fx` and `item_rendering` fail here, both
listed as CI skip targets because they need content and a GPU).

## Done on the branch

| Change | What players get | Evidence |
|---|---|---|
| Client view published once per batch of received messages, and the host's trust key hashed once per connection | Less frame-time and less network-task CPU with many players moving. Before, every pose, vehicle and orb item in a datagram (up to ~21 per 1,100 B datagram of 52 B poses) deep-copied the whole replica view: every avatar's appearance maps, names, all chat lines, weapons, tools, vitals, vehicles, entities, package state, plus a SHA-256 of the certificate. Now one copy per burst. The UI still never sees a view older than the events it receives: the view is flushed before any reply, notice, cue batch or map change goes to the UI. | `crates/client/src/network.rs` `run`/`publish`; `crates/net/src/client.rs` `queued()` |
| 1 ms timer resolution while a session runs (Windows `timeBeginPeriod(1)`, released when the session ends) | Even host ticks and pose sends: without it Windows wakes tokio timers on a 15.6 ms clock, so the 120 Hz tick ran in bursts of two and 40 Hz poses left 15.6 or 31 ms apart instead of 25 ms; a guest's movement datagrams (14 ms gap) waited up to a full 15.6 ms period. Scoped to the server loop and each client connection, so menus idle at normal power. The frame cap already uses winit's high-resolution waitable timer. | `crates/net/src/timer_resolution.rs`, held in `server::run` and `Client` |
| Screenshots no longer stall the frame | The frame that takes a screenshot used to block on `device.poll(Wait)`, swizzle the pixels and encode the PNG on the main thread (tens to hundreds of ms at 1440p, inferred). Now the copy is polled on later frames and the PNG is written on a worker thread; the "Screenshot saved" message arrives when the file exists. | `crates/client/src/platform.rs` `Screenshots` |
| Remote player interpolation no longer allocates per player per frame | Small; removes a `Vec` per remote player per frame. | `crates/client/src/motion.rs` `sample` |
| Release build settings | See the measurement section below. | `Cargo.toml` `[profile.release]` |

## Round 2 and close-out (2026-09-29)

Branch `claude/project-thread-cw15he`. Round 2 went to Gate at `bc59037d0`
(protocol change: Gate numbers VERSION). The later commits on the branch are
listed below the table.

### Host upload for 32 moving players (measured, `crates/net/tests/bandwidth.rs`)

| Scene | Before | After |
|---|---|---|
| 32 players spread over the map | 2.10 MB/s | 0.41 MB/s |
| 32 players crowded at spawn | 2.02 MB/s | 0.99 MB/s |

Distance-tiered rates (40 Hz within 32 units, then 20, 10, 5 Hz), rank caps in
a crowd (12 closest moving players at their tier rate, the next 12 at most
half, the rest a quarter), a 39 B remote pose (was 52 B: centimetre
positions, a flags byte), and a per-remote adaptive interpolation delay on the
client so slower senders stay smooth and never rewind. Budget tests hold both
scenes under 0.8 and 1.4 MB/s.

### Listen-host memory per brick (measured, `crates/net/tests/memory_census.rs`, 100k bricks)

| Copy | Before | After |
|---|---|---|
| Authority world | 1070 B | 510 B |
| Host session adds (collision, spatial index) | 690 B | 690 B |
| Host player's replica | 1171 B | 611 B |
| Its prediction collision mirror | 788 B | 666 B |
| Total | 3719 B (about 3.7 GB at 1M) | 2477 B (about 2.5 GB at 1M) |

`Brick` went from 520 to 240 bytes inline: light, emitter, vehicle spawn and
block look are boxed, and an unresolved content reference boxes its two names
(encoding unchanged, pinned by a test). The client's build tool no longer
deep-copies every brick and the collision mirror no longer keeps per-brick
geometry copies: both hold a structurally shared `imbl` handle to the replica.
The build tool's copy is not in the census (it lives in the client crate);
it was a full clone of every brick. What remains is mostly rapier's own
per-collider cost, twice (host and mirror); cutting it means merging brick
colliders per chunk, which belongs with item 1.

### Every item on the list

| # | Item | Status |
|---|---|---|
| 1 | Brick removal stalls the tick (rapier full refit) | Handed to entity perf (brick removal and batched collapse are theirs) |
| 2 | Ghost brick rebuilt on every change | Handed to brick perf |
| 3 | Memory per brick | Done (table above); collider merging goes with item 1 |
| 4 | Changed bricks rescanned every tick | Handed to brick loading (change tracking is theirs) |
| 5 | Host upload grows with players squared | Done (table above) |
| 6 | Listen host's server shares the client's runtime | Done: a hosted game's server runs on its own two-thread runtime |
| 7 | Shadows fully redrawn every frame | Handed to brick perf (render crate and chunk passes) |
| 8 | Upload spikes on settings changes, CPU mips | Handed to brick perf (chunk uploads, renderer) |
| 9 | No culling or LOD for players, items, vehicles | Handed to entity perf (avatars; main already skips building bodies out of view) |
| 10 | Sounds and hit effects wait on the 20 Hz reliable update | Handed to entity perf (weapons and effects); needs a cue/state ordering design and a protocol change |
| 11 | Input queue never drains back | Done: a standing backlog is run off one extra input per tick after a half-second window; jitter that empties the queue is left alone; the above-six catch-up is unchanged |
| 12 | Per-player state rebuilt and compared at 20 Hz | Done for the costly parts: package views are rebuilt only when package state or the player set changes (revision counter), and updates copy only new chat lines. Names, vitals, tools and avatars stay as they are: each is one small map per player, not worth a revision scheme |
| 13 | Startup and map load are serial | Not done here: needs content on the PC to measure and verify ordering; left for a PC lane if Max wants faster startup |
| 14 | UI hitches | Done for icons: brick selector icons decode on a worker thread when the brick list arrives. An icon atlas is left to brick perf's GPU work. Chat relayout per frame is not worth it: a page of lines, and a safe cache needs a pack identity the UI does not have |
| 15 | No performance regression gate | Handed to brick perf (their planned regression check) |
| 16 | Smaller items | zstd contexts: handed to brick loading (owns the codec). Allocator: done, mimalloc in the game and dedicated server binaries (200k-brick world, Linux, best of 3: world build 83 to 69 ms, wire decode 414 to 333 ms, JSON load 371 to 311 ms, collider inserts 245 to 200 ms; probes keep the system allocator so lane baselines stay put). Low-latency present option: not worth it now; with vsync off the game already presents immediately, and with vsync on a shorter queue is a feel change Max would have to judge |

### Commits after the round 2 hand-off

- Run off a standing input backlog instead of keeping it (item 11)
- Run a hosted game's server on its own threads (item 6)
- Rebuild package state views only when package state changes (item 12)
- Box the entity probe's rare brick settings (build fix after the merge)
- Decode brick selector icons ahead of first use (item 14)
- Copy only new chat lines into each update (item 12)
- Use mimalloc in the game and dedicated server binaries (item 16)

## Ranked opportunities not done here

Highest player impact first. "Lane" says who should own it.

1. **Removing a brick stalls the tick in big worlds (measured).** Rapier 0.36's
   broad phase does a full BVH refit on the step after any collider removal
   (`rapier3d-0.36.0/src/geometry/broad_phase_bvh/update.rs:272`,
   `must_full_refit = ... || !removed_colliders.is_empty()`). Every hammer hit,
   wand, rocket knock-out and undo calls `detect_collisions` after removing a
   collider (`crates/sim/src/simulation.rs:373` and siblings), and the client's
   collision mirror does the same (`crates/sim/src/prediction.rs:194`).
   Measured with static cuboid colliders: remove + refresh costs **1.5–2 ms at
   100k bricks and 26 ms at 1M** (insert + refresh is only 30 µs at 1M). At 1M
   bricks each removal costs three 8.3 ms ticks on the host and a frame hitch on
   every client. Fix: batch all removals in a tick into one refresh (never one
   per brick), and patch rapier to refit only the removed leaves' ancestors (it
   already does a partial refit for inserts). Lane: tool-kill feel / entity
   perf (they own brick removal and batched support collapse).
2. **Ghost brick rebuilt from scratch on every shift, rotate and paint, and on
   every world edit (inferred, high for building).** `ghost_key` mixes
   `ghost_generation` with `query_generation` (`crates/client/src/app.rs:4711`),
   so any placed brick anywhere rebuilds the ghost. Each rebuild goes through
   `build_world_scene_materials`, clones the brick surface images
   (`crates/client/src/materials.rs:116`), generates their mip chains on the CPU
   and creates new textures and bind groups (`crates/render/src/scene.rs:1578`).
   Remote players' ghosts repeat it, and a duplicator ghost rebuilds the whole
   copied build per nudge. Fix: share the palette textures the chunk path
   already uploads, upload ghost geometry once and move it with a transform.
   Lane: brick perf (it touches the brick upload path it is reworking).
3. **Memory per brick blocks the 1M-brick goal (partly measured).**
   `size_of::<Brick>()` is 520 B; one brick in the `imbl::OrdMap` costs about
   1.1 KB (measured). A listen host keeps roughly five copies: authority world,
   server colliders, client replica, the building mirror's deep-cloned
   `BTreeMap` (`crates/client/src/building.rs:101`) and the collision mirror,
   plus four spatial indexes. Estimate ~3.9 GB at 1M bricks before GPU meshes
   (inferred). Fix: a hot fixed-size brick record (interned definition id,
   position, rotation, colour, flags, owner; ~40 B) with rare fields in side
   tables; let the building mirror share the replica instead of cloning. New;
   overlaps brick loading (compact save cache).
4. **Changed bricks are rescanned every tick until the next network publish
   (inferred).** The replication `dirty` set is only emptied at the 20 Hz
   publish, but events reinstall every brick in it every tick
   (`crates/sim/src/session.rs:2039`, `session/events.rs:272`), items reconcile
   it twice per tick (`session/items.rs:91`), vehicles and Add-On voxels scan it
   too. Each change is reprocessed up to six times, and during a streamed build
   load the set grows every tick. Fix: per-consumer change queues drained once.
   Lane: brick loading (owns change tracking); worth checking against the
   `session_chaos` slowdown that failed its gate.
5. **Every client gets every moving player at 40 Hz, so host upload grows with
   players squared (arithmetic).** At 52 B per remote pose, 16 moving players
   cost the host about 4 Mbit/s up, 32 about 16.5 Mbit/s, above many home
   uploads. Fix: per-client priority accumulator (distance, in view, recently
   changed) under a byte budget, with distance-based rates. New.
6. **The listen host's simulation shares a 2-worker tokio runtime with its own
   client networking (inferred).** `crates/client/src/app.rs:1392`;
   `session.step()`, delta building and zstd all run inside the server's
   `select!` loop (`crates/net/src/server.rs:1225`). A heavy tick delays the
   host player's own network task and every peer writer. Fix: run the
   simulation on its own thread with a high-resolution timer and talk to it
   through channels. New.
7. **Shadows are fully redrawn every frame (inferred, GPU).** Up to 8 depth
   passes at Best (`crates/render/src/shadow.rs:44`); the map's interiors have
   no bounds so they are drawn into every cascade (`scene.rs:1699`); avatars too.
   Cascades are already texel-snapped, so static occluders can be cached and
   re-rendered only when a cascade moves or a chunk changes. New, but in the
   render crate: coordinate with brick perf.
8. **Upload spikes (inferred).** Changing AA, shadows or colour-vision settings
   rebuilds every pipeline and re-uploads the whole map and all chunks in one
   frame (`app.rs:6725-6737`, `6874-6879`); mip chains are built on the CPU at
   upload, including for lightmaps whose mips are never sampled. Fix: rebuild
   only pipelines and targets on a settings change, precompute mips at import,
   and spread uploads over frames with a per-frame byte budget. Partly brick
   perf (chunk uploads); the settings rebuild and mips are new.
9. **Players, items and vehicles are never frustum-culled and have no LOD
   (inferred).** Avatars upload with `bounds: None` (`crates/client/src/avatar.rs:914`);
   instanced draws are not culled per instance (`scene.rs:2060`); the first DTS
   detail level is always used (`avatar.rs:77`); each avatar clones its face and
   decal images (`avatar.rs:310`). Overlaps entity perf (avatars).
10. **Sounds, hit effects and projectile spawns wait on the 20 Hz reliable
    update and one ordered stream (inferred).** They arrive 0–50 ms late, and a
    lost packet holds every later cue behind it (head-of-line blocking) plus any
    bulk frame queued ahead. Fix: one-shot cues on their own stream or
    unreliably with redundancy, flushed at the 40 Hz pose rate. New; protocol
    change.
11. **Server input queue never drains back (inferred).** Between 1 and 6 queued
    inputs are consumed one per tick, so a jitter burst leaves up to 50 ms of
    standing latency; above 6 the player simulates at 3× and visibly sprints
    (`crates/sim/src/session.rs:73`, `1886`). Fix: adaptive target of 1–2 with a
    gentle drain. New; feel change for Max to check.
12. **Per-player state rebuilt and compared at 20 Hz (inferred).** Package views
    walk packages × players × keys cloning every string for every peer
    (`crates/net/src/server.rs:975`); avatars, names, vitals, tools and the full
    chat list are rebuilt and compared each update. Fix: revision counters and
    dirty flags. New.
13. **Startup and map load are serial and repeat work (inferred).** About 15
    loaders run one after another before the window opens
    (`crates/client/src/app.rs:1274-1330`), several decoding PNGs eagerly; a
    hosted map is parsed by the client (`prepare_map`, `app.rs:252`) and again
    by the host (`crates/client/src/content.rs:380`), brick definitions are
    loaded twice, and interior geometry is JSON float arrays. Fix: open the
    window first, load in parallel, load a map once into a shared `Arc`, binary
    geometry. New; needs content on a PC to measure.
14. **UI hitches (inferred).** Brick selector icons are decoded on the main
    thread on first open with one texture per icon (`crates/ui/src/pack.rs:85`,
    `ui/src/gpu.rs:258`); chat is re-joined and re-wrapped every frame
    (`play.rs:434`). Fix: decode during map load into an atlas; cache chat
    layout by revision and width. New.
15. **No performance regression gate.** No criterion benches, no tracy/puffin
    spans, only a whole-frame GPU timestamp (`crates/client/src/perf.rs:131`),
    and `perf_probe` / `network_probe` / `build_load_probe` run in no gate. Fix:
    per-pass GPU timestamps, tracing spans behind a feature, and a release-mode
    probe on a fixed map with a p95 budget in the release workflow. New;
    overlaps brick perf's planned regression check.
16. **Smaller items.** zstd compressor and decompressor contexts are created per
    message (`crates/net/src/codec.rs`, brick loading owns the codec); no fast
    global allocator (mimalloc) for the allocation-heavy persistent maps; a
    low-latency option (frame latency 1, Mailbox) is not offered
    (`crates/client/src/platform.rs:249`).

## Already in good shape

Delta replication with per-owner changes; encode once for all peers;
quantized 52 B remote poses in MTU-sized datagrams; clients run the host's
projectile step; redundant movement input; BBR and bounded datagram buffers;
world transfer and autosave off the tick on O(1) snapshots; the server's
TickClock catch-up; no SipHash maps on hot paths; stabilized shadow cascades;
foliage cell culling; the UI vertex buffer reused; audio mixed on its own
thread with streamed music; map preparation and light baking off the frame
thread; the frame cap uses a high-resolution waitable timer.

## Release build settings measurement

A standalone release benchmark using the repo's own crates (`bri-world`,
`bri-net`, pinned `rapier3d`): a synthetic 200k-brick world, best of 3 runs,
4-core Linux. Build profile otherwise identical to `[profile.release]`.

| Workload (ms) | no LTO | thin LTO | fat LTO, 1 CGU |
|---|---|---|---|
| Wire encode (world checkpoint) | 300 | 263 | 218 |
| Wire decode | 575 | 500 | 486 |
| World JSON save | 275 | 167 | 152 |
| World JSON load | 520 | 464 | 445 |
| 200k colliders insert + refresh | 243 | 220 | 215 |
| World build (imbl inserts) | 190 | 183 | 188 |
| Relink after a one-file change (s) | 4 | 22 | 61 |

Chosen: thin LTO. It takes most of the gain for a 5x relink instead of 15x,
which matters because the brick perf lane rebuilds release binaries for every
benchmark. A separate `dist` profile (fat LTO, one codegen unit) for GitHub
releases would add another 5-15% on serialization-heavy work; it needs the
release workflow and packaging scripts to read `target/dist`, so it is left to
whoever owns packaging.

Raw rapier measurement for finding 1 (static cuboid colliders, no bodies
moving, dt 1e-6 refresh step): 100k colliders: insert + refresh 8-40 µs,
remove + refresh 1.5-2.0 ms. 1M colliders: insert + refresh 27-56 µs,
remove + refresh 26 ms (first after build 73 ms), first full refresh 1.2 s.
