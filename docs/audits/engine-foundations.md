# Engine foundations audit (2026-09-27)

Scope: the layers every feature stands on. Netcode (transport, codec,
replication, discovery), the client frame loop and prediction, audio output,
file and content IO, and threading. The question for each layer is Max's bar:
a clean, consistent, first-principles substrate that stays up under hostile
or unlucky input, and is fast because it is simple. Companion to
`engine-infrastructure.md` (measured frame, tick and GPU costs); items it
already covers are referenced, not repeated.

Method: read every file in `crates/net`, the client network worker
(`network.rs`), prediction (`motion.rs`, `sim/prediction.rs`), the platform
frame loop, the audio device and engine, save and settings IO, and grepped
all crates for unbounded channels, thread spawns, non-atomic writes and panics
on untrusted input. Costs were measured on a real converted world (Golden
Gate Bridge, 44,465 bricks) with `crates/net/tests/wire_benchmark.rs`.

## Ranked findings

Risk = likelihood x blast radius. "Fixed" items landed with this audit.

| # | Layer | Finding | Risk | Status |
|---|---|---|---|---|
| 1 | Net | Any peer could crash a dev-built host: a movement datagram with `newest = u64::MAX` overflowed `newest + 1` in `Movement::sequenced` and panicked the server task (release builds wrapped instead and poisoned that peer's input sequence). | High | Fixed |
| 2 | Net | Many `?` inside the server loop turned one failure into the host stopping for everyone: an Update, MapChanged or Notice that did not encode (a large world or paste exceeding the frame budget), an admin snapshot error, a datagram encode. | High | Fixed |
| 3 | Net | LAN discovery answered a 13-byte query with an ~1.5 KB listing, and UPnP forwards that port to the Internet: a ~100x UDP reflection amplifier for spoofed floods. | High | Fixed |
| 4 | Net | `Beacon::certificate_der` sliced untrusted text at byte offsets: a LAN packet with a multi-byte character crashed the server browser. | High | Fixed |
| 5 | Net | Resume tickets were never forgotten; after 4,096 fresh joins in one host's lifetime every new player was refused ("Server identity capacity reached"). | Medium | Fixed |
| 6 | Prediction | A frame that ran more than 6 prediction ticks (under 20 fps, or any hitch; up to 12 run) sent only the newest 6 inputs. The server never saw the rest, so the local player rubber-banded after every hitch. | Medium | Fixed |
| 7 | Net | Movement datagrams from one peer were unbounded; a flood filled the shared event queue that every player's commands and joins pass through. | Medium | Fixed |
| 8 | Audio | An unplugged headset or a changed default output silenced the game until restart (the stream error was only logged). | Medium | Fixed |
| 9 | Net | Wire format was JSON everywhere, datagrams included, with each path's own ad-hoc parsing and size checks. `MAX_DATAGRAM` (1,200) exceeded what a minimum-MTU QUIC path can carry, and pose datagrams had no size check at all: a grown `PlayerState` would silently vanish. | Medium | Fixed |
| 10 | Replication | Every world-changing delta deep-clones the whole replica world on the network worker (Golden Gate: 12 to 21 ms per edit, linear in bricks; ~0.5 s at the 1 M brick cap). The same worker sends movement, so building on a big map delays input. | Medium | Fixed (persistent map, 6 us) |
| 11 | Replication | The join/map-change checkpoint is one monolithic frame built and encoded on the authority loop (Golden Gate: ~70 ms encode; the infrastructure audit measured a ~360 ms stall). Worlds above ~128 MB of encoded state cannot be joined at all. | Medium | Fixed (streamed chunks, off the loop) |
| 12 | Files | Four different "atomic" write implementations (settings, saves, trust list, admin store) and two plain writes of security state: the host certificate and key pair (torn write: host cannot start, or its identity changes and every friend's pin breaks) and the joined-server pin list. | Medium | Fixed: `bri-files` (temp file, fsync, rename or no-clobber link, Unix directory fsync) now writes settings, saves, trust and pin lists, identities, admin state and the host certificate, which is one file |
| 13 | Net | `Session::adopt` failing during Change Map still stops the host (`?`): the old session is moved in and lost on error. Its only failure is an invariant (a fresh map has no players), so this is latent. | Low | Open |
| 14 | Net | Request body budget: one peer may reserve up to 64 MB of the shared 128 MB and trickle the body for 10 s, delaying other players' large requests (build loads). | Low | Open |
| 15 | Client | `network::publish` clones every replicated map (names, avatars, poses, chat, vehicles) on each pose datagram (up to 64 players x 40 Hz) and the UI clones the View every frame. | Low | Open |
| 16 | Net | Discovery receive errors looped without yielding (Windows reports ICMP port-unreachable as a receive error). | Low | Fixed |
| 17 | Audio | Music streams open their decoder inside the real-time callback (allocation and header parsing on the audio thread). | Low | Open |
| 18 | Render | GPU device loss and surface loss are handled (`platform.rs`); nothing fragile found. Rendering costs are tracked in `engine-infrastructure.md`. | - | OK |

What is already sound and should be kept: the codec bounds every length
before allocating and reserves request bytes before reading bodies; the
client network worker never blocks the frame thread; reliable backlogs
disconnect instead of buffering; the server tick clock bounds catch-up;
audio commands cross threads on lock-free ring buffers; saves and settings
write through fsynced temp files; replicas validate every incoming value.

## What changed

**Wire codec (one format).** `codec.rs` is now the only place bytes become
messages. Every message, reliable or datagram, is MessagePack with named
fields; decoding is strict (the buffer must be exactly one message).
MessagePack was chosen over schema-ordered formats (postcard, bincode)
because the shared command types are adjacently tagged serde enums, which
only self-describing formats round-trip; a test pins that. Datagrams go
through `encode_datagram`/`decode_datagram`, bounded to 1,100 bytes so any
encodable datagram fits a 1,200-byte-MTU QUIC path. Worst cases: movement
(6 inputs) 502 B, pose 232 B, vehicle (16 wheels) 359 B. Golden Gate state:

| | JSON (before) | MessagePack (now) |
|---|---|---|
| Raw | 19.2 MB | 13.7 MB (-28%) |
| zstd | 575 KB | 560 KB |
| Decode (client) | 144-209 ms | 99-154 ms (about -30%) |
| Encode (host) | 56-87 ms | 70-108 ms (noisy, shared machine) |

Honest reading: the format is a consistency and decode win, not a
transformative one. The dominant cost is the brick representation itself
(~430 JSON bytes per brick) and the monolithic checkpoint (items 10, 11).
The worst native event request shrank from over 8 MB to 2.4 MB.

**Server loop.** Reliable sends go through `Peer::send`, `send_message` and
`broadcast`: a slow peer is disconnected, an unencodable message disconnects
the peers it was for, and nothing short of admin-store durability loss or a
map-change invariant stops the host. Resume tickets are a bounded `Tickets`
table that forgets the oldest disconnected player's ticket. Movement
datagrams pass a per-peer token bucket (240/s, burst 60) before reaching the
shared queue. Sequence arithmetic cannot overflow.

**Discovery.** Queries are padded to 1,200 bytes and replies are capped at
3x the query (QUIC's anti-amplification rule), so the forwarded port cannot
amplify. Certificates decode as bytes. Receive errors back off.

**Input under hitches.** `Motion::advance` hands the transport every input
the frame produced (at least the 6-input redundancy window); `Client::movement`
splits batches into datagrams oldest first.

**Audio device loss.** The engine is shared with the device callback behind a
mutex the callback only `try_lock`s (never contended while a stream runs).
cpal reports a lost device or changed default output; the device thread
drops the stream and reopens the default device with the same engine,
preferring the engine's sample rate (`Engine::set_sample_rate` keeps the
clock continuous if the new device needs another rate), retrying every 2 s
while no device exists.

## Second pass: world snapshots and streaming

`bri_world::Bricks` (an `imbl::OrdMap`) now backs both the authoritative
`World` and the replicated `PublicWorld`. A copy is O(1) and an edit copies
O(log n) nodes, so every consumer holding a snapshot (frame thread, collision
mirror, music, checkpoint) shares structure instead of deep-cloning. The
serialized form is unchanged, so saves and converted content are untouched.
Golden Gate (`wire_benchmark`): replica edit plus snapshot 0.006 ms (was
12-21 ms); a full iteration of 44k bricks 1.9 ms.

Joins and map changes stream: the checkpoint head announces `world_bricks`,
then `WorldChunk` frames of 4,096 bricks follow. The authority loop only
snapshots; a blocking task strips source records, chunks and compresses, and
the peer's writer emits the frames in order (a `Frame::Pending` entry keeps the
peer's later frames behind the transfer). The client assembles exactly the
announced bricks and signals `MapChanging` at the head so a loading screen can
come up before the bricks arrive.

The reported `fourth_failed_admin_password` flake could not be reproduced
(64 of 64 passes, 16 in parallel) and its path is deterministic, so it is
dropped. `unread_pose_datagrams_never_block_reliable_delivery` pins the one
plausible mechanism checked while looking (an undrained client being kicked).

## Weaknesses of owning the engine (third pass)

Owning the engine means owning what a commercial engine hands you for free.
Ranked by impact on getting problems fixed, with status:

1. **Crash capture: done.** `bri-crash` (installed by `bri-client` and
   `bri-server` before anything else) writes `logs/` next to the game, falling
   back to the per-user state folder when the install is read-only:
   `session-<UTC time>.log` holds everything the game printed (stderr is teed,
   so a terminal still shows it); a Rust panic adds `crash-<time>.txt` with
   message, location, thread, backtrace and the end of the session log; a
   native crash (access violation, driver fault) adds `crash-<time>.dmp`, a
   minidump, plus a `.txt` with the exception code. The newest 20 session logs
   and 10 crash reports are kept. Release builds now carry line tables so
   backtraces and dumps name files and lines (open a dump against the
   matching build's `bri_client.pdb`). Tested by real crashes in child
   processes: `cargo test -p bri-crash`.
2. **Platform-layer robustness: GPU part done.** The client opens the
   high-performance adapter on DX12 first, falls back to Vulkan when DX12's
   driver or device fails, and to WARP software rendering as a last resort so
   a player still reaches the menus; `WGPU_BACKEND` forces a backend. The
   chosen GPU, backend and driver go to the session log, with the reasons for
   any fallback. A lost GPU device (driver reset or update, TDR) now rebuilds
   the renderer the way a resume does instead of ending the game; three losses
   within a minute still fail with the reason. Audio device loss was fixed in
   the first pass. Window modes, DPI, focus and mouse capture belong to the
   Window thread.
3. An in-game frame profiler and debug overlays (frame time, CPU and GPU
   spans, net stats) as console commands.
4. **Recorded-input playback: done.** `bri_client::playback` records what
   the platform delivered, frame by frame (each tick's elapsed time and the
   input before it), as JSON lines: run the game with
   `BRI_RECORD_INPUT=<file>`. `replay` feeds a recording to the real App in
   the platform's order (input, UI clock, tick, window commands) with no
   window, GPU or OS input; `Script` authors the same frames from control
   names (`click`, `hold`, `press`, `wait`). `tests/input_playback.rs` goes
   from a first launch through the default-controls dialog, Start Game and
   the mission list to walking with W and opening the Escape menu using only
   clicks and keys, then round-trips its own recording.
   Also landed from the first-impressions audit: the game opens on
   double-click with the content beside it, release builds have no console,
   startup and fatal errors show a dialog with an Open logs folder choice, a
   main-thread panic says so before closing, and a crash nobody saw is
   reported once on the next launch (`tests/launch.rs`). Loading feedback: the
   loading screen shows the real stage and bar from `bri-progress` (loading
   the map, starting the server, connecting, receiving the world brick by
   brick, loading graphics) for hosting, joining (from the moment the host
   names its map) and map changes; `input_playback` asserts the stages seen.
   Still open there: a splash while content loads before the window opens.
5. **Net soak tests: done, and they found a real problem.**
   `bri_net::impair::ImpairedLink` is a UDP relay that adds latency, jitter
   (so reordering), loss and duplication both ways, with a seeded pattern so a
   failure replays (`BRI_SOAK_SEED`). `crates/net/tests/soak.rs` runs two
   real clients through a bad Wi-Fi link (60 ms + up to 40 ms each way, 5%
   loss, 1% duplicates) moving every tick and chatting, for `BRI_SOAK_SECONDS`
   (default 10). It exposed that Quinn's default Cubic congestion control
   reads random loss as congestion: reliable replies stalled 3 to 8 s and
   replicas fell seconds behind. The transport now uses BBR; the same link
   answers every command within about 0.3 to 0.6 s across seeds. Other threads
   can wrap any loopback test's host address in an `ImpairedLink`.
6. A benchmark map with a frame-time budget checked on each build.
7. A dependency-aware content build that rebuilds stale packs.

## Next, by payoff

1. Items 10 and 11: done (second pass above).
2. Item 12: done (`crates/files`).
3. Item 9 follow-up: a compact wire form for bricks (ids and packed
   position/rotation/color instead of named fields) would cut checkpoint bytes
   several-fold; do it together with item 2.
4. Items 14, 15, 17 as the owners of those areas touch them.

## Evidence

```powershell
cargo test -p bri-net            # codec, discovery, server unit tests; loopback and replication suites
cargo test -p bri-audio          # engine sample-rate continuity plus existing suites
cargo build -p bri-audio --features cpal-output
$env:BRI_BENCH_WORLD="content/worlds-pass-005/3ee5411a....world.json"
cargo test --release -p bri-net --test wire_benchmark -- --ignored --nocapture
```
