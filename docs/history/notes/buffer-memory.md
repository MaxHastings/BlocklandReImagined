> **Historical note. Written 2026-10-14 (the file's date in the cloud project folder, `notes/buffer-memory.md`); copied here unchanged on 2026-10-20 so it is kept in git. It describes the project as of that date and may no longer match the code.**

# How Blockland ReImagined allocates buffer memory

Read from the code on 2026-10-14 (repo main checkout). Paths are relative to the repo root.

## The short version

The game draws through **wgpu**, which hands out GPU memory in chunks called *buffers*. There is no general-purpose memory manager; each kind of data has its own simple rule:

| What | Rule | Freed when |
|---|---|---|
| The built world (bricks) | A few big shared "blocks", each chunk rents a slice | Slice returns on chunk unload; blocks themselves stay |
| Individual models (players, items, vehicles) | Their own buffer pair, filled once on upload | When the model is dropped |
| Copies of one model (instances) | One fixed-size buffer per model group, rewritten only on change | With the group |
| Per-frame lists (draw commands, UI, particles, weather) | Buffer kept and reused; replaced with a bigger one (power of two) only when it overflows | Never shrinks; replaced on growth |
| Camera, lights, settings | Small fixed buffers made once at startup | On renderer restart |

Think of it like a web app that keeps one big array and reuses it rather than doing `new Array()` every frame, and only reallocates when it runs out of room.

## 1. The world: a pooled allocator (`crates/render/src/pool.rs`)

The biggest consumer. Static brick chunks don't each get their own buffer; they share large **blocks** (one vertex buffer + one index buffer each), so the GPU can draw many chunks in one go.

- First block holds 65,536 vertices (~4.5 MB); each new block is double the last, capped at ~1M vertices (~72 MB) unless one chunk needs more. `pool.rs:14-20`, growth at `pool.rs:197-206`.
- Indices get 2 per vertex reserved (quads are 4 verts / 6 indices). `pool.rs:21`.
- Inside a block, space is handed out with a **first-fit free list** that is kept sorted and merges neighbouring gaps back together. `pool.rs:23-62`.
- A chunk's slice is a `Slot`; when the chunk is dropped, `Drop` returns its range to the free list. `pool.rs:100-114`.
- Before a big load, `reserve` makes one block big enough for the whole load (geometric growth), so a map doesn't end up spread across many blocks. `pool.rs:126-153`, called from `scene.rs:3045-3048`.
- Data is copied in with `queue.write_buffer` at the slot's offset. `pool.rs:220-229`.
- Blocks never grow, move or get freed while the renderer lives: freed space is reused, but the pool doesn't shrink. `pool.rs:1-6`.
- There are two pools: opaque bricks and translucent bricks, kept apart so sorted see-through bricks draw in long runs. `scene.rs:2643-2646`, split at `scene.rs:3430-3440`.
- `pool_usage()` reports block count and bytes. `scene.rs:3050-3053`.

## 2. Everything else that has geometry

- Models and non-pooled chunks get their own vertex + index buffer, created already filled. Empty ones get a 1-vertex/1-index placeholder because zero-size buffers aren't allowed. `scene.rs:1505-1537` (`geometry_buffers`), used at `scene.rs:3225`, `3425`, `3536`.
- All "create already filled" calls go through the project's own `buffer_init` (`crates/render/src/buffer_init.rs`), which pads to 4-byte copy units and survives a lost GPU instead of crashing. A clippy rule bans wgpu's stock helper (`clippy.toml:9`).

## 3. Instances (`GpuInstances`, `scene.rs:1210-1290`)

When the same model appears many times (e.g. items), one buffer holds a transform per copy. Capacity is fixed at creation (1 to 16,384) and checked against the device limit (`scene.rs:1223-1243`). Updates compare against the CPU copy first and skip the upload if nothing changed, so static things cost nothing per frame (`scene.rs:1262-1290`).

## 4. Per-frame "grow but never shrink" buffers

- **Indirect draw arguments** (the list of "draw this slice" commands): one buffer, a cursor reset to 0 each frame (`scene.rs:3737`), written sequentially. If it overflows it is replaced by one sized to the next power of two of 2x the request, at least 64 KB. `scene.rs:3057-3083`.
- **UI vertices**: same pattern, next power of two, minimum 64 bytes. `crates/ui/src/gpu.rs:719-734`.
- **Particles**: per view, at least 256, power of two, capped at the player's budget; rebuilt only when too small. `crates/fx-runtime/src/gpu.rs:316-334`.
- **Weather (rain/snow)**: the same scheme. `crates/weather/src/gpu.rs:236-255`.

## 5. Fixed small buffers made once

Camera uniform per view (views are kept once made, so mirrors don't rebuild each frame) `scene.rs:2955-2962`; point lights `scene.rs:2871-2877`; light grid `scene.rs:2879-2884`; identity instance `scene.rs:2891`. Updated in place with `write_buffer`.

## 6. When it's all freed

wgpu frees a buffer when its last handle is dropped. Buffers in use by an already-recorded pass stay alive until that frame is done (`scene.rs:3069`). If the GPU is lost (driver reset, out of memory), the renderer is torn down and rebuilt, which releases everything, including the pool blocks (`crates/client/src/app/mod.rs:1725`, `crates/client/src/platform.rs:435`).

## Other buffers (CPU side, brief)

- **Network**: QUIC via `quinn`. Outgoing datagrams are capped at 1,100 bytes (`crates/net/src/protocol.rs:105`) and packed into a `Vec::with_capacity(MAX_DATAGRAM)` (`crates/net/src/codec.rs:88`); quinn manages its own socket buffers.
- **Audio**: decoded clips are shared read-only (`Arc<[u8]>`, `crates/audio/src/bank.rs:24`); the mixer is built not to allocate in steady state except a small 8,192-sample refill buffer for streamed music (`crates/audio/src/engine.rs:1-4`, `704`); the output device uses a 4,096-frame mix buffer (`crates/audio/src/device.rs:204`). Voices are capped at 16 audible / 128 virtual, like the original engine.
