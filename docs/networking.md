# Native multiplayer foundation

`bri-net` runs the same `bri-sim::Session` used by local headless tests. Quinn
provides QUIC with a reliable ordered control stream and unreliable movement/
pose datagrams. This is implemented backend networking, not a finished join UI.
The runtime dependency graph contains no Torque readers.

## Authority and replication

- Protocol 7 carries the server-owned five-slot tool inventory and selected slot
  to reliable checkpoints/deltas, including late join. Equip commands contain
  only a slot; the established connection determines the actor. Invalid owner
  membership, duplicate item IDs and invalid selected slots reject a delta
  before world mutation. It also replicates mounted image/state, projectiles and
  dropped-item views, with reliable sound/effect/animation/shell cues. Native
  Gun firing and projectile late join are tested through real QUIC. Rendering
  those item views and full inventory HUD reconciliation remain integration work.
  Runtime content identity8 includes native weapon definitions/resource bytes;
  item presentation pack hashing will extend this separately.
- The host assigns owner IDs. Commands cannot supply positions for the player,
  privileges or owner identities. Build transforms remain validated against
  authoritative reach, geometry, support and permissions.
- Reliable messages carry actions, results, checkpoints and dirty-brick deltas.
  Native source records remain on the host; public brick state preserves
  supported gameplay properties and events.
- Wire format: one codec (`codec.rs`). Every message is MessagePack with named
  fields, strictly decoded (a buffer is exactly one message); server frames are
  zstd compressed. Datagrams (movement, poses, vehicles) are bounded to 1,100
  bytes so any encodable one fits a minimum-MTU QUIC path.
- World transfer: a Welcome or MapChanged checkpoint carries everything but the
  bricks, plus `world_bricks`, the count that follows as `WorldChunk` frames of
  at most 4,096 bricks. The authority loop takes only an O(1) snapshot of the
  persistent brick map (`bri_world::Bricks`, an `imbl::OrdMap`); stripping
  source records, chunking and compression run on a blocking thread, and the
  peer's writer sends the frames at that point in its ordered stream. Clients
  assemble with `WorldAssembly`, which accepts exactly the announced bricks
  (no duplicates, no overrun) before building the replica, and emit
  `ClientEvent::MapChanging` at a map change's head. There is no world size
  ceiling beyond `MAX_BRICKS`, and joins no longer stall other players.
- Congestion control is BBR: random loss (Wi-Fi, mobile) is not congestion,
  and loss-based Cubic stalled reliable replies for seconds at 5% loss in the
  soak test (`tests/soak.rs`, through `impair::ImpairedLink`).
- Replica worlds are the same persistent map: publishing a new world revision
  to the frame thread after an edit is O(1) (6 us on Golden Gate, was 12-21 ms
  of deep copy on the network worker that also sends movement).
- Protocol version 4 retains optional validated yaw/pitch snapshots on reliable
  actions. The native client captures body aim at dispatch, so look/fire/look
  sequences remain correct even when the movement channel only delivers the
  final look. Server-owned position, reach and permissions still apply. Action
  aim does not change body orientation, movement intent or movement acknowledgments.
- Avatar appearance is a reliable catalog-validated command owned by the sending
  connection. Checkpoints/deltas carry active outfits separately from movement
  datagrams; rejected choices preserve the previous complete appearance. Late
  join and same-process authenticated resume retain outfits. Preview never sends
  this command. Replicas validate bounds/owner membership before applying deltas.
- Movement datagrams carry separately sequenced intent. The motor runs at 120 Hz;
  snapshots and reliable world updates run at 20 Hz. Input expiry stops unattended
  movement after 60 simulation ticks.
- Clients reject replication gaps and invalid deltas before mutating their world.
  Remote pose history ignores older ticks and interpolates yaw across its seam.
- Local prediction reuses the player motor, retains at most240 unacknowledged
  inputs, restores an authoritative pose including jump-edge state, then replays
  pending inputs. It does not advance unrelated dynamic bodies. Wiring this into
  the windowed client, collision updates and visual correction is still required.
- TLS verifies an explicitly supplied host certificate. Random 256-bit reconnect
  credentials resume an existing owner; old numeric Blockland IDs confer no
  authority. Server lookup stores credential hashes. Credentials and certificates
  currently last for one server process; restart persistence remains required.
- Limits:64 peers,80 simultaneous connection/handshake tasks, bounded channels,
  64 KiB Hello,64 MiB command requests,16 MiB compressed frames,128 MiB decoded
  frames and bounded zstd window. A shared 128 MiB command-body admission budget
  is acquired before allocation and held through command dispatch; it measures
  wire body bytes, not all parsed/object memory. Body reads retain a 10-second
  deadline. These are working limits, not proven maximum-load capacity.

The clock uses actual monotonic elapsed time, retaining fractional ticks and
running up to 8 catch-up steps per wakeup. Coarse Windows timer wakeups previously
slowed gameplay (144 ticks in a two-second smoke); the corrected smoke reached241
ticks with zero dropped ticks. Longer stalls discard excess debt and record it
as `dropped_ticks`. Checkpoint preparation is off the authority loop (see
World transfer above).

## Hosting and joining

Joining needs only the game port. The host's certificate is pinned by the
client (saved pin, LAN listing, an invite's key, or trust on first use), and
protocol 34's `Challenge` carries the server listing so the join list and the
host's reachability check can probe a server over the game port without
joining. Internet hosts and non-loopback dedicated servers ask the router to
forward the port (UPnP, then NAT-PMP), take their public address from the
router, probe it, and tell the host in plain words whether friends can reach
them. No outside service is contacted. See
[architecture/hosting.md](architecture/hosting.md).

## Executable host

```powershell
cargo run -p bri-net --release --bin bri-server -- content content/worlds-pass-005/<world>.world.json artifacts/native-network/server-clock-smoke 127.0.0.1:0 2
```

The first argument is the content root. The server loads the packages its
`packages.json` lists (the base game's list, `crates/package/base-packages.json`,
when the root has none) and hashes each one into its environment
(`docs/architecture/packages.md`). A joining client sends its shared and
client packages; the join is refused when a shared package differs, and the
refusal names every differing package. Differences in client-only
(presentation) packages are allowed and told to the joining player in chat.
`host.json` records the environment.

The optional final duration bounds a headless smoke. Without it the host runs
until Ctrl+C. Public host metadata and certificate go to the supplied state
directory; shutdown publishes a new native save without replacing existing
revisions. Both saved source provenance and supported state survive the smoke;
only tick/revision advance. Map geometry, native water and clearance-checked spawn
candidates are loaded. Runtime content identity v8 added validated weapon definitions
and every declared converted native resource byte to the v7 foliage placement/textures
and audio/weather definitions/texture/clip bytes, as well as the complete effects
pack (definitions, bindings/composites and all declared original texture bytes),
and verifies their declared native checksums. Weapon resource hashes in the pack are
original-file provenance, so v8 fingerprints actual converted bytes without comparing
them to the original DTS hashes. Weapon loading bounds the manifest to 32 MiB,
resources to 4096 entries/64 MiB each, and total bytes to 512 MiB, with canonical
package containment. Host and join reject on-disk weapon catalog replacement during
an App lifetime; restart loads a new catalog. The host installs all 21 native item
choices and weapon definitions before accepting peers and resolves known imported
item display names while preserving unresolved references and source records.
Content identity v9 additionally verifies presentation schema2, the checksum-pinned
item-physics schema1 metadata, all21 item/model bounds and every declared native
presentation model/texture checksum. The host installs these authored bounds after
weapon definitions and before peers join; metadata reports initial static-item
count. This adds `item-presentation-dir` after `weapons-dir` in the dedicated CLI.
No renderer or original-file reader is required by the dedicated loader.
See [item startup integration](research/item-spawners/startup-integration.md) and
[checked physics startup](research/item-spawners/item-physics-startup.md).
The protocol wire version (35 at this writing) is independent of content identity version 9.
Content digest domains distinguish older packs before entering a session.
Pending scene objects and finite terrain coverage are disclosed in host metadata.

Content matching uses a versioned full fingerprint of native brick definitions,
collision/mesh bindings, the flat map bundle, the brick-material manifest and all
declared original surface/print/icon image bytes, plus the effect catalog and
referenced effect textures. Changed material/effect content rejects joining;
paths on disk do not contribute to the digest. The dedicated host installs the
same print/light/emitter tool catalog as the client-hosted server, including the
original Letters/A default. `fingerprint_runtime` adds the avatar rig, customization
tables and every declared face/decal/surface image in a V3 domain, checking hashes.
This does not yet include complete tool/UI/audio packages, dependency resolution
or automatic content delivery. Earlier fingerprints remain for diagnostic probes.

## Evidence

`cargo test -p bri-net --locked` exercises actual loopback QUIC clients: two-player
building, denied unauthorized edits, chat, exact late join, deletion replication,
disconnect/resume with ownership retained, wrong certificate/content/credential
rejection, movement acknowledgements and stale-input stopping. Codec/replica tests
cover truncation, frame bounds, invalid/gapped changes and reordered poses.
An additional loopback test pipelines opposite-facing tool inspections without
any movement datagrams; each hits its own target and leaves body aim/input
acknowledgments unchanged. Simulation tests reject invalid aim and replay while
preserving ownership checks.

A real QUIC test submits all 4,096 event rows (565,349 bytes), verifies live
replicas, late join and native save/reload, and rejects a 4,097-row edit atomically.
Oversized local serialization writes no partial frame and the connection remains
usable. A maximum escaped native event payload fits the larger command bound;
Hello stays small. Separate transport tests exercise admission/retained permits
and immediate oversized-length rejection before allocation. This verifies large
event edits, not relay scheduling, fair execution or bot-load performance.

Protocol 4 adds host capability authentication, administrator status in Welcome,
build snapshot/append commands and append-only palette deltas. Invalid palette
extensions and bricks reject together before replica mutation. Tests cover an
unprivileged first localhost connection, invalid host credential, host resume,
imported-owner isolation, exact colors/events, live clients and late join. A native
build request exceeding the old 16 MiB cap traverses real QUIC and exports with
all opaque source records intact. Reply serialization failure returns an error
instead of terminating the authority loop. Working save/load admission is four
requests per 120 ticks across the server; this is not a performance guarantee.
Host capabilities remain private in process, absent from public host metadata.

`build_load_probe` appends Demo House and Golden Gate Bridge into an already
running server, checks late join and native export, and reports acceptance time,
replication time and dropped ticks. It includes authored brick collision but no
map or render workload. It does not establish smooth multiplayer loading.

```powershell
cargo run -p bri-net --release --locked --bin build_load_probe -- content/stock-catalog-004 content/maps-pass-003 content/worlds-pass-004 artifacts/native-build-load/report.json
```

`cargo test -p bri-sim --test prediction --locked` exercises controlled 100 ms
correction delay and 20% input loss, final authoritative convergence, bounded
history and unchanged unrelated dynamics. It is not a WAN impairment test.

```powershell
cargo run -p bri-net --release --bin network_probe -- content/stock-catalog-004 content/maps-pass-003 content/worlds-pass-003 artifacts/native-network/integration.json
```

Release loopback results on Maxwell's Windows/Ryzen7800X3D machine:

| Native save | Bricks | Public checkpoint JSON / zstd bytes | First / late join |
| --- | ---: | ---: | ---: |
| Demo House |150|52,191 /2,397|3.45 /2.09 ms|
| Golden Gate Bridge |44,465|14,763,326 /561,476|103.29 /104.59 ms|

Both compare exact public world state and send chat after joining. These probes
omit map geometry and spawn players away from the build to isolate transport;
the separate host smoke loads the actual Bedroom map. Timings are single local
runs, not WAN, frame-rate,64-player or platform-portability evidence.

## Still required for the alpha

Windowed client integration; stable host identity and restart-safe ownership;
LAN discovery and direct-IP certificate onboarding; the UI action dispatcher;
native content packaging; sustained mixed-player/build workloads and WAN tests.
Action timeouts currently require client teardown/reconnect because their outcome
can be unknown. Imported provenance is retained server-side but unsupported rows
need an explicit UI metadata path. Full multiplayer acceptance remains unchecked.
