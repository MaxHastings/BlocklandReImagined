# Progress and evidence

## Current state
2026-09-26: full vanilla playable-alpha goal active; not ready for Maxwell's
interactive playtest. All 14 reference maps have native geometry/collision and
baseline environment rendering. Stock building, original surfaces/77 prints,
customizable Blockheads, native UI, authority/QUIC/save/load, brick emitters/lights
and initial audio cues are connected to the development client. Storm rain and
Slopes snow now use actual client geometry/water, settings and shared GPU rendering.
Weather/audio integration evidence is in artifacts/native-client-weather and
artifacts/native-client-flow. Default workspace checkpoint: 352 passed, zero failed,
48 ignored; explicit actual-pack/client/GPU tests are recorded separately below.
Local prediction exists as a subsystem but is not yet connected to normal play.
Weapons, vehicles and minigames have substantial isolated implementations awaiting
Session/client/UI/network integration. Bedroom foliage now uses classified static
collision, background placement and the actual client renderer. Full events,
remaining brick FX/sentinels, shadows/lighting,
water/terrain fidelity and large-world performance remain mandatory work.
No visible window, desktop/game input or audible playback has been used.
The expanded requirements in alpha-contract.md supersede the narrow initial goal.

## Verified environment
- Workspace was not a Git repository at start; contained research/docs/tools only.
- Rust 1.93.1, Cargo 1.93.1, stable x86_64-pc-windows-msvc available.
- CMake, Ninja and Visual Studio discovery utility available; native dependency integration not yet verified.

## Decisions
- Maxwell authorized a new private GitHub repository, commit and push once the
  complete definition of done and playtest package are ready. This is a final
  handoff requirement; no early publication. Existing original-content exclusions
  still apply. Repository creation and verified push remain pending.
- Maxwell reauthorized GPT-6 Astra parallel work after the solo period. Active
  assignment: native item/weapon presentation assets. Full events, brick FX and
  item-spawner-property handoffs are complete; gameplay integration remains. Opus
  finished its handoff and receives no further work. Root owns shared integration.
- Maxwell clarified that modding support and modding-system decisions are
  excluded from alpha completion. Keep sensible internal boundaries; revisit
  the mod API/language/distribution after the complete vanilla base is accepted.
  Vanilla wrench events and agreed event QoL remain in scope.
- Added `event-modernization.md` and alpha gates from creator feedback: larger
  event lists, reliable zero-delay relays without forced 33 ms per-hop waits,
  explicit budgets/loop diagnostics, and measured eight-player event/bot load.
  Parties/quests/RPG progression are motivating later modes, not alpha features.
- Maxwell reaffirmed that recorded rendering fidelity gaps are mandatory before
  handoff: sky/fog, decorations, water/snow, terrain detail/streaming, original
  brick surfaces/prints/FX and legacy color sentinel behavior. Added an explicit
  unchecked acceptance item; recording a gap does not waive it.
- One-time conversion into native content, not permanent Torque compatibility.
- No interactive computer/game operation by the assistant.
- No subagent delegation unless explicitly authorized by the user.
- Original content/recovered scripts remain ignored local research inputs.
- First work: executable technical proofs and a bounded, provenance-preserving content pipeline.
- Maxwell clarified Jolt is optional; use suitable Rust physics. Rapier selected
  as the initial implementation (see `physics-decision.md`). No C++ bridge needed.
- Maxwell authorizes common-sense technical pivots; prior implementation choices
  are assumptions, not constraints. Preserve the product outcome and explain
  meaningful changes without unnecessary permission loops.

## Implemented and verified
- Initialized local Git repository; original game/research/generated assets ignored.
- Cargo workspace: `bri-render` and `bri-physics`, dependencies pinned by Cargo.lock.
- wgpu 30.0.1 offscreen WGSL render, texture copy/readback and pixel assertions
  pass on RTX 4070 SUPER / Vulkan / NVIDIA 616.92. No visible window or input.
- Rapier 0.36.0 headless tests pass: dynamic body settles on floor, character
  collision stops motion against wall, four-wheel suspension vehicle drives.
- Fixed 120 Hz synthetic simulation. Repeat identical drop scene matches exactly
  on this machine; this does not establish cross-platform deterministic lockstep.
- Commands: `cargo run -p bri-render --bin gpu_probe`,
  `cargo test -p bri-physics` (3 passed),
  `cargo run -p bri-physics --bin physics_probe`.
- Local artifacts: `artifacts/preflight/gpu-probe.png`, `gpu-probe.json`,
  `physics-probe.json`. These are integration evidence, not game screenshots.

## Limits
- Controllers use synthetic shapes/default settings, not the original avatar or
  Jeep dimensions/tuning. No claim of player feel, vehicle steering/braking,
  slopes/stairs, map collision correctness, network behavior or performance yet.
- Windows verified only; macOS/Linux architecture retained but not tested.
- No original assets have entered a game runtime yet. Terrain data conversion
  now exists; full map/material/collision integration does not.

## Terrain conversion evidence
- Added `bri-content` native schema and separate `bri-convert` offline reader.
- 13 original TER v3 files converted, including Bedroom, Kitchen and Slopes.
- Independent Python verifier reconstructs all 13 original files byte-for-byte
  from native elevations/blend layers plus preserved provenance. Source SHA-256
  values match. No original installation file is written.
- `content/terrain-pass-001/manifest.json` records source paths, hashes, outputs,
  warnings and scan errors. Outputs remain ignored local content.
- One scan error: Map_BiomeRacing.zip is actually RAR, unsupported by the ZIP
  reader. Converter deliberately exits nonzero with a manifest; it does not
  claim complete corpus conversion. Format-aware fallback remains work.
- 3 converter tests pass: blend/authoring preservation, malformed/truncated/
  oversized/version rejection, and unsafe archive-path rejection.
- Command: `python tools/verify_terrain_conversion.py <v20-root> content/terrain-pass-001`.
- Material references, mission transforms/repetition and high material flag
  semantics remain unresolved and visible in diagnostics.
- Formatting and workspace Clippy (all targets, warnings denied) pass.
- Final `cargo test --workspace --locked`: all 6 tests pass.

## Brick and catalog implementation (following goal turn)
- Previous goal turn classified as progress; current turn adds native geometry,
  provenance, catalog extraction and actual asset render evidence.
- `bri-content::brick`: native schema for mesh quads, material channels, authored
  vertex colors, placement grid, face coverage and collision boxes.
- `bri-convert::brick`: parses BRICK/SPECIAL/SPECIALBRICK with bounded counts,
  explicit source adaptations and original text preservation outside runtime.
- Latest corpus: `content/bricks-pass-007`: 173 standalone BLBs + 14 terrains
  converted, 0 archive scan errors. RAR renamed .zip is now read through a
  bounded 7z pipe; all 14 terrains reconstruct byte-for-byte independently.
- One failed file remains: 1x1x5spike.blb is a headerless mesh fragment, not a
  standalone definition. The converter still reports failure rather than hiding
  it. None of the 136 stock catalog entries reference that fragment.
- Independent Python comparison verifies all 173 bricks: 80 generated standard
  bricks and 3,190 authored quads across the other 93. Checks source hashes,
  provenance, corner transforms/winding, UVs, normals, colors, dimensions/grid.
- Native-only wgpu contact sheet loads/renders 8 representative shapes. Saved
  `artifacts/native-bricks/native-bricks.png` and JSON; viewed through image tool.
  Stock textures/prints and sorted transparency are not implemented in this check.
- Corrected the old C++ importer's normal assumption based on corpus evidence:
  BLB normals already use world proportions and need axis rotation only. Unit
  regression checks a tall-ramp normal stays perpendicular to its native face.
- Hash-scoped stock repairs preserve all originals, require exact before-lines,
  and emit reasons in manifests. Some UV/grid repairs require fidelity review.
- Stock catalog: 136 original entries, all 136 native meshes resolved, zero
  missing. Preserves UI/category names, special types, print ratio, orientation
  flags, cover/indestructible flags and source references. 46 entries still need
  external DTS collision shapes. Catalog generated at `content/stock-catalog-001`.
- Catalog tokenizer never executes scripts; required dynamic fields/cyclic
  inheritance are rejected. General add-on scripting remains outside scope.
- Final checks this turn: `cargo fmt --all`, workspace Clippy with warnings
  denied, and `cargo test --workspace --locked` all pass (13 tests). No visible
  game launched and no user input automation performed.

## Model, animation and collision implementation (following goal turn)
- Added native shape/clip/collision schemas and bounded DTS/DSQ v24 conversion.
  Runtime animation/render/physics crates have no Torque-reader dependency.
- Latest `content/shapes-pass-003`: 397 conversions = 149 DTS + 61 DSQ + 173 BLB
  + 14 TER; 0 scan errors. Six failures remain explicit: four EmeraldIsles4
  tree11 variants use unsupported sorted meshes, octahedron marker uses DTS18,
  and the previously reported headerless spike BLB fragment. Converter correctly
  exits nonzero; this is not complete corpus support or complete map conversion.
- Original player, tools, Jeep and all stock brick collision models decode.
  Preserve hierarchy, named avatar objects, LOD/collision selection, mesh frames,
  material references, skin weights/binds, node/object tracks, ground motion and
  triggers. Some decal/IFL and material features remain diagnostic/provenance only.
- Verified Torque quaternion conjugation/basis, skin shared-array layout and
  billboard bits against pinned Torque3D v1.1 source and corpus. Eight apparent
  model failures were explicit empty mesh slots; now represented as native nulls.
- `bri-content::animation`: hierarchy, loop/clamp interpolation, additive tracks,
  object animation and CPU posed geometry. Production mixer/gameplay integration
  still pending. Native validation covers ranges/finite channels/unit rotations.
- `artifacts/native-models/native-models.png` and JSON: offscreen original player
  bind/run/crouch + Jeep/tire geometry, viewed twice. Run max vertex motion 0.4167.
  Debug colors only; no stock textures, face print, blended states or wheel rig.
- All 40 player DSQs sampled at six times. Four old unused files refer to missing
  bones (armready/boot/jump/visorup), recorded rather than aliased speculatively.
  Decompiled mDts constructor references 36 unique files through 39 sequence names;
  all those used files bind. Jump correctly uses standjump, not the old jump DSQ.
- `content/stock-catalog-002`: 136/136 meshes and collision bodies resolved,
  0 collision dependencies pending. Native boxes/convex recipes preserve separate
  collision details/objects, including trees and multi-part crest corners.
  Converter rejects evidence of concavity before requesting a convex hull.
- `bri-physics::content` builds Rapier solver colliders. Headless probe found GJK
  raycast edge tolerance; direct triangle queries also missed a shared seam.
  Added convex-plane targeting against authored surfaces, with a seam regression.
  This is an evidence-based query implementation change, not a change of physics
  library or source geometry. Runtime broad-phase/cooked caching remains work.
- `artifacts/native-collision/stock-collision.json`: 49,368 independent surface
  comparisons across all 136 stock bodies, maximum error 0.0002248 world units.
  Reports 21 GJK coverage differences separately. Actual dynamic ball settled on
  imported 1x1 round at Y=0.3999313 (expected 0.4). Probe took 115 ms on this machine;
  this is a small headless correctness check, not a game performance benchmark.
- Final workspace tests: 21 passed, including DSQ every-prefix truncation,
  quaternion handedness, three buffer guards, animation hierarchy and precise
  targeting. Workspace Clippy (all targets, warnings denied) passes. Brick preview
  rerun after sharing offscreen renderer; all eight cells still pass.
- No visible game window, gameplay input automation or original-install writes.

## Interior, mission and map material implementation (following goal turn)
- Previous goal turn was progress; this turn adds the native map pipeline and
  tests real architecture/placements rather than repeating existing probes.
- Added DIF resource44/interior0 reader with complete input consumption, bounded
  arrays, both TGE/TGEA surface layouts, embedded PNGs, all detail/subobject meshes,
  original diffuse/lightmap UVs, render surfaces, collision fan masks, null-surface
  collision boundaries, convex pieces and specialized vehicle collision geometry.
  Optional original BSP/zone/light-state metadata is indexed into a separate
  source archive; runtime adaptation of those features remains work.
- Native schema lives in bri-content::interior; the runtime doesn't read DIF.
  Pinned Torque3D v1.1 interiorIO/interiorCollision source provides format evidence
  alongside actual v20 files. Null collision surfaces were excluded in the old
  Blockland2 importer; the new conversion retains those authored boundaries.
- DIF's shared version number does not distinguish surface layouts. Testing the
  entire detail on a cloned reader before choosing a layout fixes a single-face
  map that the earlier surface-only heuristic misidentified. Regression fixture
  verifies axes, UV/lightmap coordinates, fan collision and every truncated prefix.
- Latest conversion `content/maps-pass-002`: 492 succeeded: 173 BLB, 149 DTS,
  73 DIF, 61 DSQ, 22 MIS and 14 TER. 15 explicit failures, zero scan errors:
  prior six unsupported/malformed shapes plus three Paradise DIF extensions and
  six community/tutorial missions requiring declarations/script adaptation.
- New literal mission reader handles nested groups, indexed fields, original
  absolute placement, nonuniform scale and conjugated Torque axis-angle rotation.
  Scene nodes hold native transforms/asset IDs and retained environment properties.
  Scripts are never executed; unknown object kinds remain explicit unadapted nodes.
  Bedroom/Kitchen/Slopes all parse. Native environment behavior is not yet built.
- `map_bundle` packages any supplied native mission IDs, direct converted geometry,
  interior diffuse and terrain layer textures. `content/map-bundle-002` contains
  the three required maps, 13 direct native assets and 52 original image resources,
  with zero unresolved interior/terrain texture bindings. Source JPEGs are copied
  byte-for-byte, not recompressed. Static-model materials, sky and datablock-driven
  props are explicitly pending; this is not a complete distributable game bundle.
- Offscreen textured wgpu renderer draws original diffuse plus embedded lightmaps
  with depth/culling and texture-pair batching. `artifacts/native-interiors/`
  contains a viewed Bedroom/Kitchen contact sheet and rendering report. It draws
  35,963 triangles in 348 batches and decodes 338 images on RTX4070 SUPER.
  Measured 875 ms includes device creation/upload/readback; not a game frame time.
- Visual check found incomplete lighting: embedded maps alone leave dark regions.
  Cached mission lighting, environment illumination, color-space/orientation
  fidelity and dynamic lights need investigation/integration. Do not call the
  current image visually accepted. Terrain/sky/decorations are absent in this probe.
- Native Rapier architectural adapter uses authored collision triangles, welded
  vertices and internal-edge suppression. Before suppression, a dropped ball on
  Bedroom's flat floor drifted about 1.87 units; afterward drift is zero in the
  same 600-tick check. Both Bedroom/Kitchen now assert drift below 0.001.
- `artifacts/native-interiors/collision.json`: original spawn floor Y=286.3120
  (Bedroom) and 119.7840 (Kitchen), with correct upward normals and resting ball
  heights. Covers transformed 4,827/25,375 collision triangles respectively.
  Does not prove traversal, doorways, player feel, vehicle or Slopes collision.
- Final `cargo fmt --all`, workspace Clippy with warnings denied, and all 23
  workspace tests pass after the adapter refinement. Both native map collision
  probes pass again using map-bundle-002. No gameplay input or game window.

## Cached lighting and terrain implementation (following goal turn)
- Added bounded cache v16 reader and offline map lighting bake. Bedroom has 105
  additive interior maps, Kitchen 190; unchanged slots retain original embedded
  lightmaps. All three maps also have terrain cache images. Ambiguous instance
  association is rejected; unusable resource CRCs and unverified mission CRCs are
  reported, with full cache/SHA-256 retained separately in provenance.
- Corpus inspection caught terrain cache RGB values capped at 31. Classic TGE
  source confirms five-bit BGR packed into PNG, unlike newer Torque3D lighting.
  Corrected ordering/scale in conversion, with saturation, truncated-cache and
  encoding regression tests. The first Slopes render was wrongly dark/brown;
  the corrected native conversion restores blue-white snow. No exposure hack.
- Map shading now preserves authored display-domain baked modulation rather than
  applying legacy coefficients directly to linear diffuse samples. It still uses
  floating-point blending and sRGB output; full visual acceptance is pending.
- Related classic source also corrects a later-engine tiling assumption: 32
  diffuse repeats per terrain block, not 64. Reference OpenMBG commit
  9c5673f9a1c26348da445bb1dbfd88bf1ed3c3b7; OpenMBU commit
  3d6516e1c9cb43e61aead3369d1f7210d08b83ef for related terrain behavior.
  Downloaded source stays ignored and is format evidence, not copied runtime.
- Corrected native terrain placement to centered 256-cell periods using square
  size, preserving serialized properties. Default 8 gives Slopes origin
  [-1024,0,1024]; Bedroom/Kitchen size16 gives [-2048,0,2048]. Regression test
  covers default/explicit spacing and retaining the original position field.
- Added shared native terrain mesh/height implementation: negative and positive
  periodic coordinates, closing border vertices, checkerboard diagonals, normals
  and triangle interpolation. Rendering and Rapier use the same geometry.
- `content/maps-pass-003`: 492 conversions, same 15 explicit failures, no scan
  errors. Latest `content/map-bundle-005`: three maps, 13 direct native assets,
  52 original diffuse resources, zero unresolved interior/terrain image bindings,
  plus 295 composed interior and three decoded terrain lighting bindings.
- Viewed final offscreen `artifacts/native-interiors-lit/interiors.png` and
  `artifacts/native-terrain/slopes.png`. Slopes renders all six weighted layers
  over 294,912 triangles per view, including periodic terrain. Original map
  thumbnail used as additional visual reference without launching the game.
- `artifacts/native-terrain/collision.json`: 3,721 rays across negative/repeated
  borders match native height queries; maximum error 0.0001831 world units.
  Slopes spawn Y571.371, floor Y569.4233; constrained vertical ball settles at
  Y569.9310 vs plane-normal expectation569.93195. A free ball should roll on
  slopes, so the flat-interior zero-drift assertion is deliberately not reused.
- Workspace tests: 28 pass; formatting and all-target Clippy warnings-denied
  pass. Original installation remains unchanged; no visible window or gameplay
  input automation. This is native-content evidence, not a playable-alpha handoff.
- Remaining map work: sky/fog/water/snow, static/decorative model materials,
  dynamic lighting, mipmaps/filtering, terrain LOD/streaming and emptySquares.

## Native world, events and original save import (following goal turn)
- Previous turn classified as progress: map conversion and executable evidence
  advanced. This turn moves into the shared authoritative state layer.
- New `bri-world` crate has no renderer, physics-handle, network or converter
  dependency. Versioned world state includes stable monotonically allocated brick
  IDs, exact native positions/quarter turns, palette, ownership, properties,
  event definitions and pending actions. Source metadata is opaque and retained.
- Authority implements validated editing/removal and ID/owner assignment on
  planting. Server construction of Actor is mandatory; old BLS owner numbers
  never confer authority. Planting requires an external physical/rules validator,
  which is not yet integrated. Rejected changes leave state and counters intact.
- Event scheduler implements activate/touch, delays, self/owner-scoped named
  targets, color/rendering/collision/raycast/light/emitter and color-FX state.
  Stable equal-time order, ceil-to-120 Hz delays, bounded fanout/queue, deletion
  cancellation and pending-state persistence are tested. Contact/reach validation
  and resulting physics/render effects are still integration work.
- New offline BLS converter preserves all palette entries, description, placements,
  flags, prints, names, light/emitter state and exact original extension text.
  Unsupported inputs/outputs/attachments stay diagnostic. Full original bytes
  and SHA-256 are retained. Runtime has no BLS-reader dependency.
- First corpus pass rejected 33/35 saves due to eight-bit degree symbols in ramp
  names. Added explicit Latin-1 decoding for the observed encoding, recording it;
  ambiguous C1 byte encodings still fail rather than silently replace text.
- Second pass imported 276,612 bricks but found 88 missing Large Cubes definitions.
  Added explicit add-on declaration extraction with relative paths resolved at
  the script's folder. `content/stock-catalog-003` now has 145 definitions, meshes
  and native collision bodies: core136 plus nine Brick_Large_Cubes entries.
  Water-brick callbacks are not executed or claimed adapted by catalog parsing.
- `content/worlds-pass-003`: all 35 saves, 276,612 bricks, zero parse errors and
  zero missing brick definitions. 302 original events adapted; unsupported ones
  remain retained. Print/light/emitter resource binding remains explicit pending
  work; item/music/vehicle source extensions are retained for later adaptation.
- Every world survives exact Rust native serialize/load equality. Independent
  `tools/verify_world_conversion.py` passes across all 35 worlds, 276,612 bricks
  and 279,272 source records. Compares original bytes/hashes, all palettes,
  transforms/flags/definitions/records and light/emitter properties. Report is
  `content/worlds-pass-003/independent-verification.json`.
- Native save publication stages and flushes before atomic no-clobber hard-link
  publication. Existing revisions stay intact; test verifies both reload and
  overwrite rejection. Hard-link-capable filesystem required; unsupported
  filesystems need a later adapter, not a silent unsafe overwrite fallback.
- Expanded collision probe requires exact catalog identity coverage when an
  add-on catalog is supplied. `artifacts/native-collision/stock-plus-cubes.json`
  passes 52,635 surface comparisons over 145 bodies, max error 0.0002248. Existing
  21 GJK edge-coverage differences remain reported. Water gameplay not tested.
- Workspace tests:35 pass; all-target Clippy warnings denied passes. No original
  files changed, game launched or input automation used. This is backend progress;
  none of the full playable-alpha acceptance items is being declared complete.

## Building simulation integration, 2026-09-26
- Previous conversational turn was coordination only (Claude audit handoff), not
  implementation progress or a verified running worker. Resumed the available
  physical simulation work; no Claude process or completion is assumed.
- Added `bri-sim`: shared native template cooking, integer stud/plate occupancy,
  rotated authored attachment cells, spatial index, camera-relative ghost shifts,
  authority-backed placement/edit/removal, map occlusion and precise brick targets.
- Placement checks support, owner permissions, reach, map embedding and moving
  entity obstruction. Collision events now update actual solver contacts. Separate
  raycast/visible/collision flags are verified, including a resting body dropping
  after a delayed collision event.
- First dynamic-body test exposed untagged bodies being mistaken for map geometry.
  Fixed with a distinct map identity outside the brick-ID range.
- Native map loader builds original interior/terrain collision. It exposes pending
  static/datablock/water objects and uses an explicit finite terrain region.
- Corpus integration:145 definitions,35 saves,276,612 placements load into grid and
  collider indexes. All three original spawn areas pass plant/target/overlap/
  remove/replant. Report: `artifacts/native-building/integration.json`.
- Catalog placement exposed the original pine hull extending0.014544 below its
  logical build volume. Added a measured map-floor-only allowance without changing
  the hull or general entity collision rules. Regression verifies the distinction.
- 564 catalog plant/remove cases pass (141 definitions x four rotations). Four
  water definitions explicitly await behavior adapters. No complete catalog
  gameplay acceptance is claimed.
- Release headless corpus/map pass2.42s;44,465-brick Golden Gate load/index~118 ms
  on this machine. Warm-cache single-run CPU measurements, not game performance.
- `cargo test --workspace --locked`:42 pass. `cargo clippy --workspace --all-targets
  --locked -- -D warnings` and formatting pass. No game launch/input automation.
- See `docs/building-simulation.md` for rules, command, evidence and remaining
  fidelity assumptions. In particular rotation anchoring, rooted support/removal,
  water/environment adapters, terrain streaming and actual player integration
  remain work. The full playable-alpha acceptance contract is unchanged.

## Player motor and server sessions, 2026-09-26
- Previous goal turn was verified implementation progress: building simulation,
  native corpus/map checks and 42 passing workspace tests. No active worker/process
  was assumed from that turn.
- Added fixed120 Hz player motor with bounded intent-only inputs, original scripted
  directional/crouch speeds, jump, air control, unlimited standard jets, crouch
  clearance, step traversal, wall sliding and swept third-person camera.
- Tuning assumptions remain explicit: dimensions, gravity, jet equations and
  detailed camera behavior are not proven equivalent to the original engine.
- Tests exposed gradual sinking when applying tiny downward gravity every grounded
  tick. Corrected grounded updates;2,400-tick regression verifies idle stability
  and one touch-entry event. Final physical shape remains an axis-aligned box.
- Added server-owned player/session command layer: authenticated-connection owner
  context (transport still pending), authoritative reach/target/ownership checks,
  command sequencing, rates, stale-input timeout and bounded chat.
- In-process two-player checks cover planting/editing/removal, ownership denial,
  exact serialized late-join state, converging players, rejected position/owner
  injection, replay, disconnect and fresh-owner rejoin. Fresh rejoin does not yet
  resume old ownership; that requires authenticated reconnect tokens/persistence.
- Physical touch-entry now feeds the existing delayed event scheduler; an actual
  standing visitor triggers a brick's delayed color event once.
- `artifacts/native-player/integration.json`: all three original map spawns pass
  settle, jump/land, crouch movement, jet rise and bounded camera checks. Command:
  `cargo run -p bri-sim --release --bin player_probe -- content/map-bundle-005
  artifacts/native-player/integration.json`.
- `cargo test --workspace --locked`:52 tests pass. All-target Clippy with warnings
  denied passes. No original installation edits or visible game/input automation.
- Details/assumptions: `docs/player-simulation.md`. No playable-client, movement-
  fidelity or complete multiplayer acceptance is claimed by this backend work.

## Networking and audit follow-up, 2026-09-26
- Previous conversational turn provided a concrete Claude UI assignment but made
  no implementation change; classified as coordination/no goal progress. Resumed
  available engine work; no external Claude process is assumed running.
- Added `bri-net`: actual QUIC/TLS headless host and clients, native compressed
  checkpoints/dirty deltas, movement/pose datagrams, bounded queues/frames, content
  fingerprinting, host certificate verification and secret-based same-process
  ownership resumption. Tests use real sockets for build/edit/delete/chat, late
  join, disconnect/rejoin and rejected ownership/content/certificate/credentials.
- Added shared prediction/reconciliation and remote pose interpolation. Controlled
  delayed/lost-input tests converge while leaving unrelated dynamics unchanged.
  Renderer/client wiring, persistent identity and LAN discovery remain work.
- Release native-save transfer: Golden Gate44,465 bricks,14,763,326 JSON bytes
  compressed to 561,476; first/late joins 103.29/104.59 ms. Demo150 bricks joins
  3.45/2.09 ms. Exact public-world equality and post-join chat pass. Local loopback
  timings only; see `artifacts/native-network/integration.json` and `networking.md`.
- Headless native Bedroom host smoke preserves all original world/provenance
  fields except advancing tick/revision. First check assumed revision stayed
  fixed and failed; source confirms each tick advances both, and exact remaining
  fields match. Timer smoke exposed 144 ticks in two seconds: fixed scheduling
  now accumulates monotonic elapsed time and bounds catch-up. Corrected smoke
  reaches 241 ticks, zero dropped, no notices; `server-clock-smoke/last-run.json`.
- Added `content/stock-catalog-004`:170 definitions/meshes/collision bodies,
  zero missing. Four hidden states leave166 selector entries. The duplicate
  Treasure Chest save name resolves to the final/closed declaration, as v20 does;
  alias and hidden states are recorded. Tests cover empty icons and alias loading.
- Expanded building probe passes 644 ordinary plant/remove cases, all 35 worlds /
  276,612 bricks and all three map spawn areas. Nine behavior-dependent definitions
  are explicit pending work. Independent BLS verification passes against the
  expanded catalog with 279,272 preserved records and 302 adapted events.
- Expanded collision probe passes 61,710 authored-surface comparisons over 170
  bodies (max error 0.0002248), with 22 GJK coverage differences separately reported.
  `tools/convert_stock_catalog.ps1` reads stock enabled-add-on declarations as data;
  a second output at `stock-catalog-005` matches 004 byte-for-byte. Use004 as the
  canonical referenced catalog;005 is reproducibility evidence, not a changed schema.
- Direct effects inspection found literal includes, inherited light definitions
  and add-ons which only rename core emitters. Recorded the required typed-native
  conversion behavior in `effects-conversion.md`; runtime effects are still pending.
- Audit corrections: tool reaches split to 5/5.5 hammer,10 wrench/printer and 5
  activation; ghost input must use body facing. Rotation anchoring remains an
  explicit mismatch to resolve with original engine snapping. LAN ownership is a
  deliberate change from stock. Original files and external UI-owned folders untouched.
- `docs/creator-direction.md` records the shared creator-platform guidance without
  expanding/reducing alpha scope or committing to a scripting dependency.
- Final 61 workspace tests pass, formatting and all-target Clippy warnings denied
  pass. No game window or desktop/game input automation. Full alpha unchecked.

## Complete vanilla scope expansion and effect foundation, 2026-09-26
- Previous goal turn was verified progress: default stock catalog expansion,
  server clock correction, tests and executable corpus evidence.
- Maxwell explicitly expanded handoff to all vanilla v20 content and gameplay:
  every stock vehicle/weapon/item/player type, prints/effects, full brick events,
  minigames and remaining vanilla workflows. Updated `alpha-contract.md` and
  implementation plan. This supersedes older one-weapon/Jeep/event-subset text
  in the persistent objective; do not narrow acceptance back to that old text.
- Added `vanilla-coverage.md` requiring per-entry provenance, conversion, native
  behavior, UI/network integration and verification. Required unsupported content
  remains open; placeholder/disabled UI is not acceptance. Missing stock Tutorial
  and other verified vanilla gaps must be resolved or explicitly excepted by Maxwell.
- `tools/inventory_vanilla.py` indexes54 default-enabled packages, all present,
  9 core event inputs/65 outputs and4 add-on registration candidates. Generated
  `vanilla-inventory.json` is an initial coverage index, not proof of all vanilla
  content or registration activation. Shipped-but-disabled packages, core native
  engine behavior and workflows still need reconciliation.
- `ui-scope-update.md` gives Claude's expanded interface requirements without
  editing its owned audit/crates or assuming an external worker is running.
- Began `bri-content::effects` typed light/flare/particle/emitter schemas and curve
  sampling, plus `bri-convert::effect_script` literal declarations/inheritance/
  indexed fields/naming overrides. These are a foundation in progress: no full
  effect converter, texture bundle or runtime render adapter is claimed yet.
- Read pinned OpenMBU/OpenMBG particle and fxLight sources into ignored research
  for defaults/curve evidence. Their engine versions are references, not proof of
  Blockland's modified behavior. Native samples do not copy engine implementation.
- Targeted content/converter library tests pass29 cases including2 new effect
  tests; formatting passes. Last whole-workspace gate remains61 tests from the
  preceding milestone. No game or desktop input was used.

## Effects conversion and UI transfer, 2026-09-26
- Added offline literal effect conversion with bounded includes, inheritance,
  indexed keys, supported naming overrides and explicit conditional/unknown-field
  diagnostics. Native runtime content has no Torque reader dependency.
- `content/effects-pass-004`:13 lights,119 particles,120 emitters (102 named for
  selection),18 original texture files,76 source hashes,zero conversion errors.
  Native curves preserve authored HDR/additive values, including glow alpha2.
  The first conversion rejected that value; corrected validation retains it.
- `content/worlds-pass-004`:all35 saves/276,612 bricks;453 light/emitter references
  resolved,zero unresolved effect references. `tools/verify_effect_bindings.py`
  independently compares every other world field and original BLS bytes against
  pass003, and checks18 texture hashes. Report `effect-binding-verification.json`.
  Rendering/simulation/attachments and dynamic script-generated effects remain work.
- Full engine workspace gate before UI integration:66 tests, fmt and all-target
  Clippy with warnings denied pass. No game window or desktop input.
- Claude's source/content/renders have now arrived locally. Both UI crates joined
  the root workspace; their existing25 UI +9 converter tests pass here. Initial
  screen manager now compiles from the draft. Menus/input integration is in progress.
- Maxwell explicitly authorized GPT-6 Astra High parallel workers. Delegated
  options/remapping, brick/print selectors and full-install UI conversion verification
  to isolated file owners; root retains screen manager and integration.

## Native UI integration and verification, 2026-09-26
- Completed the transferred screen manager and native screens for menus,
  hosting/joining/loading/pause, options/remapping, HUD/chat, brick and print
  selection, wrench/events, avatar/palette, save/load and the player list.
  All three explicitly authorized Astra High workers completed their assignments.
- Added typed requests/results, modal input release and repeat cancellation,
  transactional preferences, inventory bounds checks and visual-only server slot
  updates. Session request tokens reject late responses from cancelled connections;
  a future transport adapter must use the documented session-scoped update API.
- Hardened the UI converter's output containment, archive bounds and cached-font
  PNG parsing. Independent provenance checks cover the full installed UI corpus.
  Canonical `content/ui-pack-003` contains 558 images, 39 fonts on 41 sheets,
  seven skins, 114 styles and 68 layouts. All 166 selectable stock brick icons
  resolve, including 30 recovered from default add-on catalog entries.
- `artifacts/native-ui-pack-003-verification.json` verifies 660 asset inputs,
  five metadata inputs and 600 outputs plus the manifest checksum. Its 26 map
  entries describe installed content, not a certified vanilla allowlist. The
  23 conversion warnings remain documented in `ui-conversion.md`.
- `artifacts/native-ui-runtime/report.json` records 48 offscreen frames across
  three viewport/scale combinations, no missing textures, native menu request
  flows and a nonsquare external-texture UV check. Separate authored-screen GPU
  tests cover options/save/player, selector, avatar and wrench/event dialogs.
- Review corrected stale-slot panics, authoritative selection request echoes,
  cancelled-connection races, duplicate category casing, stale ghosting HUD art
  and chat contrast. No desktop/game input or visible window was used.
- Final checks: `cargo test --workspace --locked` passes 145 tests, four ignored;
  `cargo fmt --all` and all-target workspace Clippy with warnings denied pass.
- Tutorial's mission entry is present in both the transferred and full UI packs,
  correcting the earlier audit's absence claim. Stock provenance, dependencies
  and playable behavior still need verification.
- These screens expose typed actions, not implemented gameplay adapters. Real
  avatar previews, complete event/minigame behavior, trust/admin/add-on/music and
  server configuration workflows, remaining graphics/audio controls and the
  windowed client remain required. Full vanilla alpha acceptance is unchanged.

## Native application, persistent rendering and transport integration, 2026-09-26
- Classified the preceding goal turn as verified progress: native UI integration,
  full-install pack verification and 145 passing workspace tests. The expanded
  complete-vanilla contract continues to supersede the older narrow goal text.
- Added `bri-client`, separating platform/device ownership, native content,
  asynchronous transport, preferences and UI request dispatch. winit 0.30.13 and
  wgpu 30.0.1 share one device and compositor. Tests never launch its window.
- Real hosting loads Bedroom/Kitchen/Slopes off-thread, starts the authoritative
  server and connects a QUIC client. Solo is loopback/one player; LAN uses port
  28000 and enforces the selected limit. Direct-IP requires explicit certificate
  pins. Passwords, discovery and host/admin identity remain unimplemented, with
  explicit rejection rather than ignored protection settings.
- Connected movement/jet/crouch/jump intentions, held zoom/free look, activation,
  authoritative chat/player views, display acknowledgment and atomic versioned
  settings. Split network request/reply dispatch so commands do not stall poses.
  Added bounded queues/deadlines and cancellation-safe session/job ownership.
- Persistent map renderer uploads once, updates camera uniforms and composes
  with original UI on the same target. Architecture uses original diffuse/baked
  lighting; terrain retains all eight layers. Native replicated brick meshes
  are rebuilt off-thread with paint/visibility/rotation/transparency; opaque
  batches coalesce. Triangle-budget overflow rejects explicitly. Original brick
  textures/prints/FX are still unbound and are reported as omissions.
- `artifacts/native-client-content/integration.json`: three empty-map simulation
  loads plus Demo House's 150-brick lazy load; startup indexes 35 saves without
  loading all payloads. Native catalogs expose 166 selector bricks, 36 default
  colors, 13 lights and 102 emitters. Joined-world palette indices are preserved.
- `artifacts/persistent-scene/`: six real-map frames, one upload per map and two
  camera views. Bedroom 300,891 triangles, Kitchen 324,896, Slopes 294,912.
  GPU tests also verify depth, blending, matrix-only camera movement and odd-size
  resize. Sky/fog/decorations/water/snow/detail/terrain streaming remain explicit.
- `artifacts/native-client-flow/`: real App → QUIC host → replica → scene/HUD,
  active-loader cancellation, batched host/cancel/rehost, authoritative horizontal
  movement, chat delivery/escaping, settings rewrite/reload and disconnect. The
  final run passed in 2.21 seconds on RTX 4070 SUPER/Vulkan with no missing UI
  textures. This verifies the connection, not subjective control feel or a
  complete play session.
- Review fixed stale cancels clearing newer tokens, private-use chat formatting
  injection, stalled writes, late world update rendering, queued-job/resource
  cleanup, initial HUD hiding and late catalog dimension changes. Explicitly
  retained native window lifecycle and cross-platform execution as unverified.
- `cargo test --workspace --locked`: 168 pass, zero failures, seven ignored;
  detailed output in `artifacts/workspace-tests-latest.txt`. Full workspace
  formatting and all-target Clippy with warnings denied pass.
- Windows release executable builds; its non-windowed `--help` path passes.
  `artifacts/native-client-flow/build.json` records its size and SHA-256. This
  development executable is not the packaged complete-vanilla handoff.
- No alpha acceptance item is checked by these foundations. Client building/
  tools/events/save workflows, original avatar/animations/audio, all vanilla
  vehicles/weapons/minigames, prediction/camera polish, remaining map fidelity,
  normal LAN trust/discovery/admin flows and release packaging remain required.

## Original brick materials and native building/tool integration
- Converted five original brick surface overlays, 77 print images and 77 icons
  from six verified default packages: 159 byte-identical PNGs, zero warnings.
  Independent source/native hash and dimension verification is recorded in
  `artifacts/brick-materials-verification.json`. Stable print IDs retain BLS aliases
  and original print aspect/Letters compatibility. Historical shipped-but-disabled
  vanilla scope still requires final inventory verification.
- Native material loading checks manifest/resource containment, hashes, dimensions
  and byte/decoded-memory budgets. Synthetic tests cover Windows junction escape,
  corruption, oversized/growing reads, deduplication and blank-print paint.
  Original overlays use pigment alpha separately from geometry opacity. A real
  GPU test independently verifies coverage 0/46/255 and opaque output alpha.
- `artifacts/brick-materials-gallery/` renders all 77 prints on actual compatible
  meshes and paints original chest/gravestone/pumpkin meshes in three colors.
  An initial occupancy assertion was too high for the sparse special-brick grid;
  reframed the camera, inspected output and used a 10,000-pixel nonempty bound.
  Every print must also own an actually drawn mesh batch. No visible window.
- Confirmed negative-alpha signed paint offsets using original corpus and pinned
  BLB exporter author evidence. Applied the observed -1 encoding; preserve literal
  colors. Translucent-paint alpha interaction is provisional and reported; pumpkin
  out-of-range RGB and other encodings remain unresolved fidelity requirements.
- Added local inventory/equipment, incremental authored-collision queries, anchored
  brick ghosts, body-facing numpad shifts, native planting and tool dispatch.
  Server owns target/reach/ownership/catalog validation, atomic wrench properties,
  print compatibility, inspection identity and a 512-entry planting undo history.
  Race tests reject concurrent changes; no client-selected object bypasses reach.
- Native UI adapters bind print icons/catalogs, ordinary wrench properties and the
  implemented two-input/seven-output event subset. Imported print aliases resolve
  without rewriting source state. Unsupported source/event rows remain read-only;
  nested wrench/events dialogs retain concurrency protection. Unsupported special
  item/sound/vehicle behavior is rejected, not silently discarded.
- Expanded release App flow passes in 2.69 seconds on the final transport build: native Bedroom ghost with
  Letters/A, authoritative planting, material-world replacement, printer change
  to Letters/B, event save followed by base-wrench property save, replicated
  planting undo, canceled inspection, cancellation/rehost, chat and settings.
  Evidence/screenshots in `artifacts/native-client-flow/`; zero missing UI textures.
  The test explicitly waits for authoritative aim and does not establish rapid
  look/use ordering or interactive feel. It creates no visible window or OS input.
- Integration exposed non-grid Bedroom carpet placement snapping into the map;
  corrected map-floor deployment to the first nonpenetrating grid plane without
  weakening server collision validation. Exact original snapping remains open.
  Ghost opacity now applies after authored vertex colors so literal-color parts
  do not become opaque. Regression tests cover both corrections.
- Current full network content identity covers map/mesh bindings, materials and
  effects. Tests detect changed print bytes, effect texture/brightness and mesh
  mappings. Client hosting and dedicated hosting share the same native ToolCatalog.
  Actual dedicated Demo House run reached 121 ticks and saved successfully;
  evidence in `artifacts/native-dedicated-content/` includes 77 prints, seven
  printable definitions, 13 lights and 102 named emitters.
- Event QoL foundation: raised native per-brick admission from 256 to 4,096 rows;
  a full zero-delay list saves/reloads and executes property writes in authored
  order in one event phase; over-limit edit rejects atomically. UI adapter also
  round-trips 4,096 rows. Relay execution, fair work budgets, loop diagnostics and
  eight-client bot/event load evidence remain mandatory work, not delivered here.
- Fixed the 64 KiB transport bottleneck for large event edits. Commands now admit
  up to 16 MiB, Hello remains 64 KiB and the server reserves a shared 32 MiB wire
  body budget before allocation, retaining it through dispatch. Serialization is
  bounded while growing; rejection writes no partial frame. The worst escaped
  schema-valid event request is 16,371,851 bytes and fits the new bound.
  A real QUIC test submits 4,096 ordered rows (565,349 bytes), checks both live
  clients, late join and native save/reload; 4,097 rejects atomically. Additional
  tests check admission/permit lifetime and oversized-prefix rejection.
- Final default workspace gate: 207 passed, zero failures, ten ignored with
  separate asset/GPU runs. `artifacts/workspace-tests-latest.txt` records results.
  Workspace formatting and all-target Clippy with warnings denied pass.
- Windows release client build and non-windowed `--help` pass; refreshed executable
  size/SHA-256 in `artifacts/native-client-flow/build.json`. The final release App
  flow was rerun after transport changes. This is a development build, not the
  complete vanilla package or Maxwell's interactive playtest handoff.
- Remaining tool limitations include exact legacy ghost anchoring/terrain bias,
  projectile flight/muzzle offsets, equipment/minigame/trust rules, animation/audio,
  paint/FX/print undo, original printer map-mask semantics and last-print history.
  Rapid look/use currently has an independent movement/action transport ordering
  gap; fix with a validated action-aim snapshot, not a sleep.

## Reliable action aim and third-person collision
- Continuing solo after all three UI agents finished; no new delegation.
- Protocol version 2 captures validated body yaw/pitch with each reliable action.
  Tool rays use the click's aim even if movement datagrams contain an older or
  later look. Server position, reach and permissions remain authoritative; this
  does not rewind motion or change body orientation/input acknowledgments.
- Simulation and real QUIC tests cover opposite-facing inspections without any
  movement datagrams, invalid aim, replay and ownership. The release App flow
  now looks toward a brick, fires and looks away in one UI batch, successfully
  opening its printer. This replaces the previous aim-wait workaround.
- Connected third-person sphere sweeps to original map collision and replicated
  brick shapes. A separate broadphase indexes authored collision bounds, including
  overhangs beyond placement footprints. Collision remains independent of brick
  visibility/raycast flags. Rotation, removal, toggles and starting overlap pass.
- Original-content camera test: 170 definitions, four orientations and six axes,
  4,080 sweeps, 4,013 hits, zero unsupported queries. Clear paths through authored
  gaps are allowed. Evidence: `artifacts/native-camera/stock-shapes.json`.
- Release offscreen App flow passes in 2.70 seconds, including original Bedroom
  floor camera obstruction and a third-person render. No visible window or OS
  input. Evidence: `artifacts/native-client-flow/report.json` and screenshots.
- Default workspace gate: 211 passed, zero failed, eleven ignored; formatting,
  all-target Clippy with warnings denied and Windows release build pass.
  Non-windowed `--help` passes; refreshed executable hash in the build artifact.
- Still required: client movement prediction, avatars/animation, dynamic actor
  camera obstruction, camera smoothing/original offsets and full vanilla fidelity.
  These tests do not establish interactive feel or complete alpha readiness.

## Native avatar rig and layered pose rendering
- Assembled `content/avatar-rig-001` from converted native geometry and the stock
  `mDts` declarations: 82 nodes, 44 objects, all 39 gameplay sequence aliases.
  Preserves reused source clips under distinct names, source/native hashes and
  constructor line references. Declarative converter rejects malformed/ambiguous
  bindings and writes only to a new output outside the original/native inputs.
- Added weighted absolute-channel and additive-local animation composition.
  Partial arm poses preserve running legs; missing channels preserve the prior
  pose. Native model geometry now feeds the persistent scene renderer with
  explicit named-part colors/material bindings. Stable-topology vertex updates
  preserve uploaded textures/indices and update transparent batch centers.
- All 39 stock aliases sampled at six times (234 poses); finite geometry passes.
  Three original Blockhead combined poses render offscreen using one GPU upload.
  Tests verify that tool pose preserves legs, successive poses change pixels and
  invalid dynamic updates preserve the previous frame. Visually inspected all
  three diagnostic renders. Evidence: `artifacts/native-avatar/`.
- Explicit release asset/GPU gate also reran the existing Bedroom/Kitchen/Slopes
  persistent-map test after the shared renderer change; both ignored tests pass
  together in 1.09 seconds. No visible window or desktop input.
- Default workspace gate: 215 passed, zero failed, twelve ignored; formatting and
  workspace all-target Clippy with warnings denied pass. Runtime crates remain
  independent of legacy readers. See `docs/avatar-pipeline.md` for reproduction.
- This is a model pipeline milestone, not live player integration. Original face/
  decal/material bindings, full outfit customization, gameplay state/transition
  selection, avatar preview, player rendering/replication and avatar content
  identity remain next. Diagnostic paints do not satisfy original-asset fidelity.

## Original avatar materials, live rendering and appearance replication
- Continuing solo. Previous turn was concrete progress (native rig/mixer/renderer);
  this turn connects it to normal client rendering and the avatar editor.
- Converted `content/avatar-pack-001`: 27 faces, 28 decals and eight model surface
  textures, 63 byte-identical original PNGs with source/hash/dimension provenance.
  Runtime verifies contained native paths, hashes, dimensions and decoded budget.
  The original installation remains unchanged. Package classification retains the
  inventory's existing provenance limits rather than treating installation as
  proof every face/decal package shipped with vanilla.
- Added native outfit resolution from original preferences and part tables:
  skirt replaces legs, skirt trims use leg colors, hat/accent restrictions apply,
  packs raise the head. Body colors retain stock opacity/quantization; accents
  preserve transparency using separate material bindings. All 108 part/accent/
  image binding cases pass; evidence in `artifacts/native-avatar/outfits.json`.
- The client now renders local third-person and remote original Blockheads from
  authoritative poses, with initial locomotion/crouch/jump/fall/look selection.
  First person hides the local body; persistent buffers update across poses.
  Exact animation timing/transitions, velocity matching, strafe direction, tools/
  emotes/footsteps, death/spawn, interpolation and alternative player types remain.
- Avatar editor now receives a 3D preview, with separate camera resources so its
  camera cannot overwrite the world camera in one GPU submission. Uses a portrait
  target, original backdrop and authored lighting/FOV fields; exact orbit/FOV
  parity remains an adaptation. Preview does not publish or save an outfit.
- Protocol 3 replicates accepted appearances reliably, separately from movement.
  Server validates catalog choices and sending-owner identity. Real QUIC test
  verifies another client, late join, invalid edit atomicity, disconnect removal
  and authenticated same-process resume. Bounds tests reject malformed avatar
  deltas before changing world state. In-process Snapshot schema is now 2.
- Runtime identity V3 includes avatar rig, customization tables and all declared
  textures. Explicit test proves catalog edits change identity, corrupt image
  bytes fail validation and isolated-copy testing leaves the source unchanged.
  Dedicated host now takes the avatar package path and installs the same catalog;
  headless native Demo House smoke reached 120 ticks with zero dropped ticks and
  saved successfully (`artifacts/native-dedicated-avatar/`).
- Visual inspection exposed two real preview integration faults: wrong aspect
  and sRGB texture sampling by the display-encoded UI. Corrected portrait/FOV and
  exposed encoded preview bytes through a UNORM view. Also corrected the main
  scene shader's transfer for non-sRGB output attachments, which had darkened
  client maps/models. A GPU test compares sRGB and UNORM output and verifies
  expected display colors within one byte. Current preview was visually inspected.
- Release App → QUIC → model/material/preview flow passes (2.80 seconds), including
  a pirate face/suit decal and proof previewing another outfit leaves server state
  unchanged. Current evidence/screenshots: `artifacts/native-client-flow/`.
- Full default gate: 218 passed, zero failed, fifteen ignored with explicit asset
  runs. Workspace formatting and all-target Clippy with warnings denied pass.
  This remains a development client, not the full vanilla alpha handoff. No visible
  game window or desktop input. See `docs/avatar-pipeline.md` for remaining limits.

## Native local Save/Load, authenticated hosting and bulk append
- Continuing solo. The prior turn supplied a status update and compile evidence;
  this turn implements the missing Save/Load application integration. Full vanilla
  alpha scope remains unchanged, including all fidelity and gameplay requirements.
- Connected native Save/Load dialogs to authoritative snapshots, local PC storage,
  converted original templates and append into a running session. Background file
  work is serialized/bounded; late file reads cannot target a new session after
  disconnect/rehost. Saves no longer advertise host-controlled file storage.
- Versioned native build wrapper records an opaque ownership scope. Same-session
  loads preserve native owners; other-session/imported numbers map to reserved
  unclaimed owners, or to the loader when ownership is unchecked. Subsequent joins
  cannot claim reserved numbers. Dedicated startup accepts wrapped build saves as
  well as earlier world/checkpoint files. Persistent cross-restart identity and
  normal trust/admin management remain unfinished.
- Exact palette merging remaps brick and implemented event color parameters.
  Palette extensions replicate atomically with bricks and refresh client paint.
  Found/fixed the client query mirror's obsolete 64-color rejection; native world
  state admits up to 256 colors. Over-budget merges reject without approximation.
  All definitions/footprints preflight before authority/index/collider publication;
  existing players, bodies and builds remain intact. Bulk restore deliberately
  bypasses hand-placement reach/support checks, not catalog/grid validation.
- Save snapshots omit scheduled pending actions while retaining authored events,
  print/effect references and opaque source records. Options strip excluded event/
  ownership records too. Round-trip testing found a serde untagged-enum numeric-map
  key decoding failure on populated saves; direct typed JSON parsing fixes it.
- Local files publish from flushed staging files. Confirmed overwrites retain the
  prior revision under `.history/`. Templates stay byte-identical and a same-name
  save shadows them locally. Tests cover no-clobber, backups, path/device-name
  rejection, case-insensitive names and dedicated startup decoding. Filesystem
  hard-link support is currently required. Dates are sortable UTC timestamps.
- Protocol 4 adds private in-process host capability authentication and explicit
  administrator status. First/local-IP peers receive no automatic authority;
  invalid credentials reject and authenticated resume retains the host role.
  Credentials never enter public host metadata. This is an intentional change
  from stock v20's trusted LAN policy.
- Client requests now admit 64 MiB with a shared 128 MiB body budget; native build
  files admit 63 MiB to leave envelope room. Bounded serialization rejects before
  oversize publication. Large native save reply encoding errors return an error
  instead of terminating the server. Working admission: four server save/load
  requests per 120 ticks; eight local file jobs; 1,000 saves/512 MiB per list scan.
  A corrupt local save currently errors the listing; these are working limits,
  not a measured full-content capacity guarantee.
- Real QUIC tests cover host/guest authority, exact palette and event remapping,
  snapshots, append, imported-owner isolation, live replicas, late join and host
  resume. Another request exceeds the former 16 MiB cap and round-trips all opaque
  records. Replica tests reject malformed palette/brick combinations atomically.
- Release native-content load probe: Demo House 150 bricks accepted in 1.63 ms,
  replicated in 56.57 ms, zero dropped ticks. Golden Gate Bridge 44,465 bricks,
  18,476,986 build bytes, accepted in 247.73 ms, replicated in 394.48 ms, nine
  dropped ticks. Both late joins match and native export preserves source records.
  `artifacts/native-build-load/report.json` records this Windows loopback workload
  on the Ryzen 7800X3D host. Authored brick collision is included, map/render/WAN
  workloads are excluded. This explicitly does not prove smooth live loading.
- Offscreen native App flow now covers save/undo/reload with print/events/owner,
  confirmed overwrite, load-dialog rendering and pending-load cancellation across
  rehost. Visual inspection caught a missing map-preview binding; connected the
  original preview art in both save/load dialogs. No visible game window, OS input
  or original-install changes. Final release App flow passes in 3.90 seconds;
  visually inspected the corrected dialog with its original Bedroom preview.
- Final default workspace gate: 227 passed, zero failed, 15 ignored with separate
  asset/offscreen runs. Workspace formatting and all-target Clippy with warnings
  denied pass. Windows release build and non-windowed `--help` pass; refreshed
  executable size/SHA-256 in `artifacts/native-client-flow/build.json`. These prove
  this integration milestone, not full vanilla alpha readiness or interactive feel.
- Remaining save/load work includes streaming/chunking and removing authority-loop
  serialization/planning stalls, larger-world frame/memory limits, clearer corrupt
  save recovery, save previews beyond the map image, persistence of host identity,
  and full vanilla behavior/state restoration as those gameplay systems land.

## 2026-09-26 — designated vanilla source, Slate and native atmosphere

- Maxwell supplied `E:\Downloads\B4v21Launcher\versions\Blockland v20` as the
  read-only vanilla reference. Updated AGENTS, contract and coverage docs. New
  `vanilla-reference.md` and compact inventory identify all 14 map missions,
  including Slate and Tutorial. Historical claims that Tutorial was missing are
  superseded; original tutorial behavior remains required.
- Added reproducible read-only ZIP-member/loose-file hash comparison. All 2,843
  reference assets match the earlier installation exactly; 79 packages, no new
  reference read errors or duplicate virtual paths. All 54 stock-default packages
  are present. The older source has 452 extra indexed paths plus two problematic
  community archives. Six map archives add only cached mission lighting. Detailed
  evidence is under `artifacts/reference-audit/`; no original scripts/art copied
  into tracked documentation.
- Fresh `maps-pass-004` conversion: 383 succeeded, four failed, no scan errors.
  Literal mission reader still rejects Tutorial/Slate Storm constructs; spike BLB
  and DTS v18 editor marker remain explicit failures. Existing shared converted
  bytes remain valid, but complete conversion/runtime coverage is not claimed.
- Source correction exposes a pipeline dependency: the new reference has no
  `.ml` caches used by the previous terrain/interior lighting conversion. Retain
  existing native lighting as explicitly secondary derived evidence; independent
  validation/reproducible baking remains required. Do not silently replace it
  with white fallback lighting or treat cached-light CRCs as verified.
- Added native environment schema, offline DML/image conversion and renderer
  bindings. Original face/cloud bytes, hashes and authored fog/wind/height flags
  are retained. Camera-relative far-depth backgrounds avoid parallax and world
  depth writes; cloud UV motion uses the camera uniform. Surfaces/terrain receive
  nonlinear distance haze with correct display/output transfer. Bounded native
  image reads and declared aggregate memory validation protect sky loading.
- `map-bundle-006` includes Bedroom, Kitchen, Slopes and Slate with 52 original
  base textures and resolved sky/cloud resources. Slate now appears in native
  map selection and its reference saves can load. Native collider loading passes
  for all four maps (Slate has one collider). Slate has no visible interior
  surfaces; corrected an overbroad surface assertion and the previous test/warning
  that assumed Slate must remain unavailable.
- Offscreen tests cover sky cardinal direction, translation invariance, depth,
  fog magnitude, wind motion/calm and wrapping without reupload. Four-map native
  offscreen render passes; inspected Slate/Slopes images. Native index/lazy-map
  test passes. No visible window, audio playback or OS input.
- Workspace gate: 230 passed, zero failed, 15 ignored; all-target Clippy with
  warnings denied passes. Formatting applied. Asset-dependent runs are separate;
  these results are not acceptance of all vanilla gameplay or subjective feel.
- Release offscreen App/QUIC/UI flow also passes in 4.38 seconds with the updated
  environment/client code. This exercises the normal typed application boundary
  without opening a window. No refreshed release handoff package is claimed.
- `environment-pipeline.md` records engine-family evidence and remaining fog
  volumes/storms/settings, water/snow/decorations, lighting/shadows, detail/streaming,
  long-running cloud phase and the other ten maps. All remain alpha requirements.
- At Maxwell's request, prepared `coordination/opus-vanilla-audio.md` as a bounded
  task brief he can give Opus: complete original audio inventory/native pack,
  isolated Rust runtime, hardware-free tests and concrete integration handoff.
  No agent was spawned/reactivated or messaged; Astra remains solo.

## 2026-09-26 — all reference map architectures and parallel gameplay work

- Prior turn was progress: source audit, native atmosphere and Slate integration
  changed code and yielded reproducible evidence. Continued the full contract;
  no scope reduction or completion claim.
- Mission reader now honors exact editor object-export boundaries, preserving
  surrounding scripts as pending native behavior with source line ranges/hashes.
  Scripts never execute. Missing/duplicate/reversed boundaries reject. Corrected
  `~/` references to the mission's mod root. Tutorial and Slate Storm declarations
  now convert; source setup/trigger behavior is still required.
- `maps-pass-006`: 385 conversions, two explicit failures (spike BLB fragment and
  DTS v18 editor marker), zero archive/scan errors. `map-bundle-007` failed on the
  tilde reference and 008 exposed the sky-material offset error; both are retained
  as incomplete historical outputs, never selected by the client.
- Corrected a fidelity error from the prior sky pass: the seventh DML image is a
  reflection map, not a cloud. Pinned OpenMBG sky.h confirms EnvMapMaterialOffset=6
  and CloudMaterialOffset=7. Native atmosphere schema 2 rejects old schema 1.
  The reflection texture is retained for later water/material binding. Actual
  moving cloud layers in this reference occur in Slate Storm (three), not Slate.
  Destruct's authored empty material list produces its black fog backdrop.
- Added explicit secondary lighting cache input. Primary original hashes must
  match native conversion provenance; secondary mission and referenced geometry
  must match primary bytes. Mission CRC is independently checked. Multi-interior
  mapping requires one unique complete assignment by slots/image dimensions;
  ambiguous mappings reject. Tutorial's two interior caches now bind. Sentinel
  resource CRCs cannot independently establish generated-lighting correctness;
  source-only baking and visual lighting acceptance remain work.
- `map-bundle-009`: all 14 reference maps, 19 direct native assets, 66 original
  architecture textures, zero unresolved texture bindings. Six missions use
  verified-input secondary caches. Client default selects this bundle and all
  14 maps; native index/lazy collision loading passes in 7.15 seconds. All map
  previews bind original UI pack images. Tutorial behavior is explicitly pending.
- All-14-map offscreen render passes in 1.82 seconds. Inspected corrected Slate,
  Tutorial and Slate Storm images; false oversized cloud/moon overlays are gone.
  Destruct's all-black scene is authored rather than mistaken for failed rendering.
  Exact horizon behavior, water/weather/static objects and other fidelity gaps
  are still required. No visible window or desktop input.
- Source-backed converter regression covers reflection/cloud separation, byte
  identity, empty skies, script boundaries, CRC and unique/ambiguous association.
  Workspace and Clippy gates pass; release App/QUIC/UI flow passes in 4.64 seconds.
- Maxwell explicitly reauthorized GPT-6 Astra agents. Updated AGENTS. Three are
  running: vanilla_weapons owns new weapons/import crates; vanilla_vehicles owns
  vehicles/import crates; reused completed ui_pack_audit agent now owns ONLY new
  fx-runtime/fx-import crates. Existing engine/client/network/root manifests stay
  root-owned. Opus independently owns audio/music. All obey the same no-visible-
  game/no-input boundary, and their work still requires integrated acceptance.
- Weapon/vehicle API boundary agreed: 120Hz host-driven logic, distinct entity IDs,
  shared PhysicsWorld (vehicles never step a separate world), typed intents and
  host permissions. Vehicle firing references weapon projectile IDs including
  tankShellProjectile/cannonBallProjectile. Root continues environment and shared
  integration while agents implement substantial isolated subsystems.

## 2026-09-26 — original static decorations and map fixtures

- Native map rendering now includes TSStatic trees/stove burners and adapted
  StaticShape glass, light fixtures and LCD clocks. Source-parent texture lookup
  resolves the tree resources; authored skin substitution selects the green LCD
  material where requested. Initial time0/blink poses come from original clips.
  Runtime depends only on converted shapes/images and native scene metadata.
- Bundle 011 covers all 14 maps, 24 direct native assets and 76 original textures,
  with zero unresolved bindings. The four Bedroom/Kitchen variants contain 30
  static model and 52 datablock-model placements. Bundle 010 was the intermediate
  TSStatic-only pass; client defaults now select 011. Literal StaticShapeData
  reading is opt-in and does not broaden the default effect-class parser.
- Physics uses authored collision details, including visually hidden collision
  objects. A regression checks transformed collision versus larger visual meshes
  and confirms that models without authored collision stay decorative. Source
  reads now share bounded/canonical-path validation; material parent search stops
  at the containing mod root. Original installations remain untouched.
- All-14-map offscreen rendering passes in 1.85 seconds; native index/lazy collision
  loading passes in 7.41 seconds. Release App/QUIC/UI flow passes in 4.48 seconds.
  Inspected Kitchen fixtures in context. Default workspace: 235 passed, zero
  failed, 15 ignored; all-target Clippy with warnings denied and formatting pass.
  A new parser regression initially assumed shape paths were normalized there;
  corrected the assertion to preserve the parser/adapter boundary. Clippy caught
  two needless borrows in the converter; fixed and reran successfully.
- Hardened converter reproduced all 14 maps under target/map-bundle-static-repro-001
  with the same asset/texture counts and zero unresolved resources. Full source
  comparison and earlier secondary-lighting provenance requirements still apply.
  All 849 non-manifest output files match bundle 011 byte-for-byte; bundle.json
  reflects normalized source-path provenance from the shared resource reader.
- Clock ticking/blinking/explosions, breakable fixtures, repair, distance LOD,
  replicated grass and finer translucent sorting remain work. Drawing initial
  models does not complete their behavior. Water/weather, terrain detail/streaming
  and the other explicitly required fidelity gaps remain open.
- Parallel work continues in three isolated agent areas: weapons, vehicles and
  effects. Vehicle agent reports shared-Rapier headless tests; effects has native
  runtime/GPU APIs; weapons has native item/projectile state graphs. These are
  progress reports, not integrated acceptance. Root integration and Opus audio
  remain separate ownership areas. No visible game window, OS input or playback.
- Started water/weather integration review in map-water-weather.md: nine water
  placements, two precipitation systems and two Bedroom foliage replicators.
  Engine-family sources confirm water surface differs from its source position;
  depth/coverage behavior remains to be resolved. Older precipitation source does
  not match SnowA, so the later family implementation is kept as qualified evidence.

## 2026-09-26 — native water, liquid forces and valid map spawns

- Previous goal turn was progress: static map objects/collision and evidence were
  completed. This turn continues the full vanilla contract; no scope reduction.
- Added versioned native Water records and offline resource conversion for all
  nine authored placements across Desert, Sea, Storm, Slopes and Tutorial. Typed
  native bounds distinguish object origin from the top surface, retain density,
  viscosity, repetition and visual controls, and verify original image hashes.
  Legacy core/map tilde paths are adapted only in the converter.
- Bundle 012 failed on Tutorial's nonexistent, disabled reflection reference.
  Disabled reflections now skip resource loading; enabled missing resources fail.
  A regression covers that distinction, original byte identity and converted
  water height/bounds. Bundle 013 exposed Desert's missing default repetition in
  the render check; bundle 014 corrects it using qualified engine-family evidence.
  The client and all-map render tests select 014. All 14 maps package, with nine
  water records and no unresolved required resources. Earlier incomplete packs
  remain ignored historical outputs, never current defaults.
- Shared rendering now draws original surface/shore resources with GPU wave and
  UV motion, combined surface alpha passes, terrain-derived depth/shore masks and
  a provisional original-resource reflection mapping. Water stays world-relative,
  receives fog, and is sorted in strips. Shader/unit evidence checks visible
  water, dry terrain masking and time motion without scene reupload. Visually
  inspected Desert's continuous sand and Sea's water/sand layers. These are not
  claims of exact legacy specular/reflection, wet-edge or underwater fidelity.
- Host simulation loads the same liquid records. Player motor applies authored
  density/drag and underwater speeds with source-family coverage thresholds;
  tests cover buoyancy, movement, drag, stable floating and removal of forces on
  exit. Full fluid movement acceleration and bounded-volume edge handling remain
  declared native adaptations needing feel/fidelity checks. Player prediction,
  vehicles, projectile crossings, liquid events and underwater audio/visuals still
  need the shared environment inputs.
- The actual Sea-session test exposed an existing launch bug: its spawn marker
  touches map collision, so using the marker center directly rejected the player.
  New shared spawn service checks body clearance and traces floors for obstructed
  centers, returning up to 64 separated candidates within authored regions. Host
  joins/resumes can try alternatives. Exact legacy weighted random selection is
  still an adapter; no user/game input was used to find or fix this issue.
  Regression covers floor contact and fully blocked regions. The real Sea session
  now joins and floats at its authored 9-unit surface.
- All-14-map offscreen rendering passes in 1.91 seconds; native index/lazy loading
  plus the real Sea host/motor check pass in 8.22 seconds. Release App/QUIC/UI flow
  passes in 5.66 seconds. Expanded default workspace: 257 passed, zero failed,
  34 ignored; all-target Clippy with warnings denied and formatting pass.
- Accepted agent handoffs: weapons-pack-003 (17 items,31 images,25 projectiles,
  117 resources); vehicles-pack-006 (11 definitions including skis/tumble,77
  assets); effects-runtime-pack-001 (120 emitters,119 particles,34 lights,41
  composites,208 bindings,18 textures). Detailed evidence/gaps remain in their
  research folders and READMEs. Added weapons/import and fx-runtime/import to the
  root workspace/shared lockfile. Re-ran all 21 weapon runtime tests and all 11
  effects runtime/GPU tests, including actual converted content: pass. Workspace
  membership proves build compatibility, not normal gameplay integration.
- Reassigned the three explicitly authorized agents: minigames owns NEW isolated
  minigames/import paths; weather owns NEW isolated weather/import paths; vehicles
  retains its own paths for checkpoint/restore, scaling and source-backed remaining
  flight/hover rules. Root owns finished weapons/effects and shared integration.
  No additional child agents, original-install edits, visible windows, OS input
  or audio playback. Full alpha goal remains active; no playtest handoff yet.

## 2026-09-26 — replicated brick effects and verified audio/vehicle handoffs

- Connected effects-runtime-pack-001 to the actual client: native pack loading,
  replicated brick attachment reconciliation, original-texture particles/flares
  and shared-renderer point illumination on terrain, architecture and bricks.
  Unrelated deltas retain effect clocks; removal drains particles; disconnect
  tears down all sources. Hidden bricks retain intentional emitters. Nearby
  cosmetic sources receive bounded budgets with explicit deferred counts.
- Flares query static map and visible-brick occlusion independently of tool-ray
  and physical collision flags. Own source bricks are excluded. A new visibility
  test initially used an off-grid fixture; corrected to a valid stud/plate center.
  Long diagonal queries exposed a potential empty-volume scan: spatial queries
  now choose occupied-bucket traversal when cheaper. Sparse-large-query regression
  and the full sim suite pass after this correction.
- Point-light uniforms update/clear without geometry reupload; GPU regression
  proves illumination, distance falloff, color change, removal and invalid input
  rejection. Actual App -> loopback QUIC -> wrench -> replica -> scene test now
  selects Red Light/Player Jet, observes particles, renders, saves/reloads and
  undoes/removes sources. Final release run passes in 4.79 seconds. Inspected
  offscreen bedroom-scene.png. Added evidence to native-client-flow/report.json.
  One extra field exceeded the existing large json! macro's recursion limit;
  moved that field to a normal post-construction insertion and reran successfully.
- Runtime content identity v4 includes full effects manifest/library/original
  image bytes and checksums. Regression covers changed binding metadata and
  corrupt library/texture bytes. Host/join and dedicated executable agree.
  Dedicated server also now uses native waters and clearance-checked spawns,
  matching the local host. Two-second Slate Sea smoke: 241 ticks, native save
  published; evidence artifacts/native-network/server-sea-effects.
- Before new handoffs, default workspace gates passed 260 tests, zero failures,
  35 ignored (workspace-brick-effects-tests.log). Integrated released vehicles
  and Opus audio/import into the main workspace/shared lockfile. Expanded suite
  passed 321 tests, zero failures, 35 ignored with BRI_AUDIO_PACK explicitly set
  (workspace-effects-audio-vehicles-tests.log). Subsequent sparse-index regression
  and complete sim suite pass separately. Final all-target workspace Clippy with
  warnings denied and formatting pass. No claim that ignored gates all ran here.
- Opus finished; Maxwell explicitly requested no more Opus work. Read its handoff
  and integration docs. All audio runtime Windows tests, including both real-pack
  decode/offline mix tests, pass. cpal-output backend plus tests compile/link on
  Windows with --no-run; feature-enabled all-target Clippy passes. No device opened
  or audio played. Both missing profiles remain explicit. Keep requested missing
  title music unbound; no Ambient Deep substitution. Client audio/settings/trigger
  wiring is still pending. Historical integration patch must not be applied over
  newer root changes; root workspace membership is already complete.
- Vehicle follow-up accepted: pack007/schema2, 11 definitions; uniform instance
  scale, atomic checkpoint restore, source-family Carpet hover/energy and corrected
  rear-wheel drive defaults. Root reran all 25 runtime/2 importer tests and Clippy
  in its workspace. Session/client/network adapters remain pending. Reassigned
  the vehicle agent to NEW foliage/import paths for original Bedroom grass and
  beargrass; vehicle paths released to root. Weather agent still owns weather
  paths and is finishing texture-array GPU batching and evidence.
- Minigames agent released standalone runtime/import and pack002: 11 player
  types/21 items, 20 reported passing tests, typed lifecycle/permissions/scoring,
  unlimited lives/manual respawn and persistence. Its isolated eight-player rules
  benchmark is not the contract's combined AI/events/physics/network load test.
  Root integration/reverification is next; no new task assigned to that agent yet.
  Clarified contract's generic lives/respawn wording to source-backed unlimited
  vanilla lives, without inventing an additional gamemode feature.
- Exact light falloff/shadows, transparency/material-aware occlusion and sorting,
  water illumination, map wind, fake-kill state, cross-client effect phases and
  player/weapon/vehicle/transient dispatch remain fidelity work (runtime-effects.md).
  No visible window, desktop/game input, playback or alpha handoff. Goal active.

## 2026-09-26 — client audio, reliable cues and authored weather
- Previous user-requested status turn only inspected/restated state; classified as
  no progress. Revalidated current workspace and continued safe implementation.
- Finished client audio adapter: original missing defaults are seeded without
  replacing saved case variants; UI notes/settings and ghost cues are connected.
  Actual jump/plant/break/tool operations generate ordered server presentation
  cues. Protocol 5 delivers them reliably outside coalesced pose snapshots, with
  checkpoint cursors, late-join suppression, validation and bounded diagnostics.
  Two-peer/late-join/rejected-action tests and offline listener-before-culling
  tests pass. Actual App flow silently exercises those cues. Full gameplay audio
  dispatch remains incomplete; runtime-audio.md enumerates remaining bindings.
- All-target Clippy exposed an enlarged network incoming enum; boxed reliable
  messages and retained explicit handling for unexpected post-welcome messages.
  Moved the new audio test module after implementation. All-target workspace
  Clippy with warnings denied and formatting subsequently pass.
- Integrated weather/import into the root workspace and normal client. Authored
  Storm/Slopes placements, settings, first/third-person camera, collision revision,
  native water and shared GPU lifecycle are connected. Physical roof queries use
  colliding geometry independently of tool-ray flags. Committed original
  precipitation setting disables/re-enables weather; disconnect clears all state.
- The initial integrated run caught the peer identity reader using manifest.json
  instead of delivered weather.json. Corrected reader and regression fixture,
  then reran both release actual-App tests: 2 passed in 11.65 seconds. Storm
  renders 5000 drops, Slopes 500, Bedroom zero. Frozen on/off renders change
  12604/595/0 pixels; no invalid/pending queries or capacity clipping at capture.
  Inspected Storm and Slopes PNGs. Water impacts/splashes and actual native-water
  nearest-roof/repeated-footprint checks pass. See runtime-weather.md and
  artifacts/native-client-weather/report.json; visual fidelity remains unaccepted.
- Runtime identity v5 includes original audio clip bytes and all bindings;
  v6 additionally includes weather.json and all declared PNGs. Client host/join
  and dedicated executable agree. Regressions verify changed bindings/authored
  values and reject corrupt/escaping resources. Dedicated two-second Slate Sea
  smoke saves at tick 241, with report in server-audio-weather. No decoder/device
  is used by the host's resource hashing. Server reports cosmetic cue drops too.
- Default workspace checkpoint: 345 passed, zero failed, 39 ignored in
  artifacts/workspace-audio-weather-tests.log (BRI_AUDIO_PACK set). The actual
  App/offscreen runs and converted-water test ran separately; subsequent content
  identity tests pass after the filename correction. Final lint/format checks
  pass. Ignored tests are not counted as completed by the default suite.
- Active parallel work: ui_pack_audit implements full vanilla events/catalog and
  fair zero-delay scheduler; vanilla_vehicles now owns brick color/shape FX and
  sentinel rendering in client world/material and render scene/shader files.
  Approved explicit brick-only metadata representation without changing public
  vertex layouts, subject to original-material and bounds regressions. Opus has
  no new assignments. Root owns ongoing host/client integration.
- Weather is connected, but foliage, weapons, vehicles, minigames, complete
  events/audio/transient effects and remaining fidelity/performance gates are
  still open. No window, desktop input, audible playback or alpha handoff.

## 2026-09-26 — native Bedroom foliage client integration
- Previous goal turn classified as progress: client audio/cue/weather changes
  and successful integrated evidence. Revalidated current files before continuing.
- Adopted foliage/import as root members. NativeMap adds explicit Terrain,
  Interior and Static query tags; authoritative Simulation still overwrites map
  tags as before. Client static-surface queries return the nearest class, never
  trace past a forbidden roof. Existing weather/camera/tool queries remain intact.
- Actual host/join map loaders build seeded foliage in their background worker,
  1024 query chunks and explicit total load budget. Prepared fields travel with
  the matching map. Current original Bedroom placed all 40000 grass/1000 beargrass
  with zero rejects and 59200 queries. Root client CPU test samples positions on
  permitted original terrain; prohibited-roof regression passes.
- Connected original-image foliage to the host GPU/depth pass before particles,
  weather and UI. Spatial culling updates indices, not meshes/textures. CPU fields
  survive GPU recreation; disconnect/map replacement clears state. F64 map time
  and per-plant phase rebasing remove the 24-hour limitation without wrapping
  distinct sway/light rates together. Rebase upload bytes are explicit in stats.
- All 10 foliage tests pass with private/GPU cases explicitly included. Marked
  the 8 pack-dependent cases ignored by default rather than making source-only
  tests implicitly depend on proprietary data. Seven-day phase and GPU checks,
  original alpha, depth, culling, placement and bounded retries pass.
- Actual release App/QUIC/UI/offscreen flow and weather-map integration pass:
  2 tests in 10.67 seconds. Bedroom placement measured 38.9537 ms there; 41000
  uploaded sources, GPU recreation without re-placement and disconnect teardown
  are asserted. Counts are source-backed, not an interactive fidelity claim.
- Added native map + actual client foliage compositor. Separate release test
  passes in 1.79 seconds; inspected bedroom-grass.png. Its view has 935 visible
  plants, 2 draws, 3852 bytes/frame and 5,248,000 resident instance bytes; changing
  foliage alone affects 278643 pixels. Placement measured 33.4888 ms in this run.
  These are bounded view measurements; general traversal/stress remain open.
- Runtime identity v7 now includes foliage placement/definitions and verified
  original PNG bytes on local host, remote join and the dedicated executable.
  Added changed-placement/corrupt-texture regression and updated host arguments.
  Dedicated two-second Slate Sea smoke saves at tick 241 with zero dropped ticks
  or cues (artifacts/native-network/server-foliage). Details and remaining fidelity
  are in runtime-foliage.md.
- Default workspace checkpoint passes 352 tests, zero failed, 48 ignored in
  artifacts/workspace-foliage-tests.log. The new original-map compositor ran
  separately afterward. Scoped all-target Clippy with warnings denied passes for
  client/foliage/import/net/sim. Agent renderer files remain agent-owned and active.
- Brick-FX agent proposed pre-light clamping of unexplained pumpkin RGB200/150;
  root challenged the unsupported visual change. Agent agreed to retain raw
  finite values and diagnostics, produce comparison evidence and leave exact
  interpretation unresolved. Renderer compatibility is not proof of fidelity.
- Full events and brick FX continue in parallel. Weapons/vehicles/minigames,
  complete audio/events, shadows/lighting/terrain, networking performance and
  packaged vanilla handoff remain open. No desktop input, visible window or
  audible playback. Goal active.

## 2026-09-26 — shared tool inventory and reliable equipment selection
- Previous goal turn was the user-requested status update only: no progress.
  Revalidated contract, progress, current source and live agents before edits.
- Unified native weapon/core-tool inventory without phantom reservations or a
  second weapon inventory. Stock Hammer/Wrench/Printer occupy slots0/1/2;
  native weapons use remaining slots and reuse a dropped core-tool slot. Four
  explicit core IDs delegate their actions to building authority. Pack-defined
  weapon state machines retain switching restrictions. Weapon saves write schema2
  (read1/2), reject duplicate inventory items, preserve core drops and selections.
- Session now owns inventories; setup-only weapon pack API and internal grants
  expose no remote Give command. Equip commands retain connection-derived owners,
  rate/replay checks and reject empty/out-of-range slots atomically. Disconnect
  removes the actor; authenticated resume creates the normal stock inventory.
  Session snapshot schema3 includes inventory. Dedicated persistence of complete
  weapon state is still required.
- Protocol6 checkpoints/deltas include inventory, selected slot and connected
  ownership. Replicas validate all fields before world changes. Normal client
  tool/brick/paint selection sends reliable equip/unequip. Item metadata/HUD
  reconciliation and equipment gates on tool-use remain pending, as do actual
  dropped/spawned-item interaction and weapon fixed-tick/presentation adapters.
  Details and explicit remaining integration: runtime-inventory.md.
- New isolated core-tool and actual native mixed-inventory/drop/checkpoint tests
  pass. Session suite7 passes; initial new fixture incorrectly spawned on the
  floor boundary, correctly rejected by clearance; corrected to a clear spawn.
  Replica suite7 and new two-peer/late-join inventory QUIC test pass. Client library
  checkpoint47 passed/9 ignored. Actual release App/QUIC/UI/offscreen + weather
  integration2 passed in4.86s, including new authoritative Printer selection.
- Final scoped gates: full net suite26 passed/2 explicitly ignored; all23 weapon
  runtime tests passed with private-asset cases enabled. All-target Clippy with
  warnings denied passed for sim/net/client/weapons. Root-edited Rust files were
  formatted. These checks do not assert completion of pending gameplay adapters.
- Accepted item-spawner handoff: persistent native properties, BLS item records,
  validated21-item catalog adapters, alias resolver and seconds-correct wrench
  respawn UI. Agent reported world7/convert37/sim-tools11/ToolUi8/UI-wrench10
  tests and native21-choice smoke passing, plus scoped Clippy. Root still must
  connect catalog installation, world alias resolution, spawn/pickup/respawn.
- Accepted full-events handoff: native events-pack-002 has16 inputs/65 outputs;
  20 Rust/3 importer tests reported passing and independent source comparison.
  Final isolated8-area benchmarks:64-row p95 0.7507ms;4096-row overload p95
  15.5098ms/max31.9385ms, maximum overdue75ms, no silently discarded work.
  These are not combined game/bot/network acceptance. Root must bind host
  intents/input, editor/network and migrate existing pending jobs explicitly.
- Brick-FX handoff finished: all8 vanilla effects bound to normal rendering;
  approximation parameters and unresolved legacy sentinel interpretation remain
  fidelity requirements. Reassigned that agent to new native item presentation
  assets only (original model textures/icons/mount helpers and offscreen evidence).
  Original source hashes are not native-output hashes; a typed native presentation
  pack will carry correct hashes and metadata without parsing provenance at runtime.
  Renderer additive blending and unlit overlays were also delegated to that
  agent after actual stock projectile materials exposed those missing modes.
- No visible game window, input automation, audio playback or alpha handoff.
  Expanded vanilla contract remains active and incomplete.

## 2026-09-26 — native weapon startup, host ticks and projectile replication
- Previous turn was progress: shared authoritative inventory, reliable equipment
  selection and passing integration evidence. Revalidated source/handoffs.
- Catalog startup agent completed and released paths. App now installs all21
  item choices before catalog menus. Host/join/dedicated load weapons-pack-003;
  content identity8 hashes actual native bytes with bounded reads/containment
  (never compares converted JSON against an original DTS hash). Known item_ui
  aliases resolve during map/reference-world/dedicated startup without rewriting
  source records. Later native build append alias resolution remains work.
  Agent verified actual null-audio/no-GPU App hosting and dedicated240tick smoke;
  dedicated metadata reports21 items/identity8/zero dropped ticks/cues.
- Root added native collision adapter and Session120Hz weapon stepping. Actor
  frames use authoritative position/velocity, captured trigger aim and a provisional
  eye-origin muzzle. Quick down/up edges are preserved across ticks; expiry,
  queue capacity and switch/disconnect cancellation are explicit. Source-exit
  collision, thin-map sweeps, closest-bounds radius and blast occlusion pass a
  focused physics test; original grace/epsilon/muzzle fidelity remains open.
- Protocol7 + Session snapshot4 replicate mounted image/state, projectiles and
  drops. Reliable cues now carry weapon sound/effect/animation/shell information;
  audio consumes original named profiles, other presentation bindings remain work.
  Replica rejects duplicate identities/malformed cues before any state mutation.
- Setup-only host loadouts preserve empty slots; default still3 core tools.
  Unknown/duplicate/live changes reject. Added runtime give_at for authored slots;
  ordinary give/pickup still fills first empty slot. Weapon clock aligns with an
  existing world's tick. Free-build player damage remains denied; unimplemented
  gameplay intentions are counted/reported as adapter gaps, including in dedicated
  last-run diagnostics. This is not completion of combat/minigames/brick damage.
- Native Gun Session test passed: quick tap, perpendicular captured aim, one
  projectile, original sound and disconnect cleanup. Real QUIC native weapon test
  passed: configured loadout, equip/fire/sound, projectile evolution and late join.
  Initial test tried pre-populating an actor before server ownership-scope setup;
  protection correctly rejected it. Switched test to validated pre-join loadout;
  did not weaken identity protection. Initial Clippy fixture-style warning fixed.
- Root gates: sim37 passed/2ignored plus new loadout test passed afterward;
  net30 passed/4ignored, with the native Gun/QUIC tests explicitly run separately.
  All-target Clippy sim/net/client/weapons passed. Latest release actual App/QUIC/
  UI/offscreen + weather tests2 passed in5.43s. No visible game/input/audio use.
- Item presentation/additive/unlit renderer agent remains active. Next integration
  includes original item world spawning/pickup/drop, authoritative HUD/tool mapping,
  actual mounted/projectile rendering, proper muzzle poses, transient effects,
  full gameplay intents and minigames. See runtime-weapons-host.md for limits.
- Maxwell added final private GitHub repository/commit/push after full playtest
  readiness; recorded in contract and decisions. Publication has not started.
  Full expanded vanilla goal remains active and incomplete.

## 2026-09-26 — world item contacts and explicit administration scope
- Root added authored-box static item placement, host contact pickups, first-empty
  inventory grants, duplicate/full-slot behavior and per-brick respawn clocks.
  Item placement uses world direction selectors and pivot correction; unchanged
  properties do not reset a running timer. Spatial buckets bound normal contact
  queries, with oversized-shape fallback and preflighted4096 static-item capacity.
- DropTool uses connection-owned inventory/current pose, original script20-unit
  throw/10s lifetime, body yaw and scale.58-tick thrower exclusion derives from
  engine-family480ms evidence, explicitly not exact closed-v20 proof. Other
  players can collect immediately. Weapon save3 reads legacy1/2 safely;
  protocol8/Session snapshot5 carry static items and drop transforms.
- Agent delivered checked presentation pack003 authored bounds for all21 items,
  offline DTS header proof, reproducible outputs and offscreen tests. Startup
  agent integrated item physics in normal host/join/dedicated and identity9;
  reported checked all21-item dedicated fixture240ticks/no dropped ticks,
  alias/source-record preservation, native startup tests and strict Clippy.
- Root evidence: five item tests pass; sim suite checkpoint42 passed/2ignored
  before adding the fifth item test; weapon23 tests passed with native cases
  enabled, then new drop migration/timeout test passed. Expanded real-QUIC
  inventory/drop/late-join test passes. Initial fourth client exhausted the
  fixture's three spawn positions; disconnected an observer before late join,
  preserving the real spawn clearance rules. Scoped sim/net/weapons Clippy passes.
- Full net suite after startup integration:33 passed/5 explicitly ignored;
  native item-physics/startup cases were run separately by the startup agent.
- Maxwell asked explicitly about Admin/SuperAdmin menus. Added separate contract
  acceptance for original flows and server-enforced roles, including denied
  forged requests. Agent assigned source audit/typed authority foundation;
  early evidence says host auto-SA and one permission-sensitive Admin menu,
  not a separate SA dialog. Weapon-effects and inventory/HUD integration run
  in separate agents; root retains shared server integration.
- Remaining item behavior/rendering limitations are explicit in
  runtime-world-items.md. No visible window, input automation or audible playback.
  Full expanded vanilla contract and final private GitHub publication remain open.

## 2026-09-26 — equipment authority follows inventory
- Previous goal turn classified as progress: implemented/tested world-item host
  integration and assigned independent inventory, effects and administration work.
- Root now requires the selected authoritative Hammer/Wrench/Printer for their
  corresponding tool requests and direct edit/remove commands. Paint/FX require
  no selected inventory tool; explicit server paint-image state remains work.
  Host equip and successful selected-tool drop revoke stale inspection capability;
  same-slot use preserves the open dialog. Source callbacks originate from mounted
  images; v20 source selection is at core5278 and Wrench/Printer hits10824/23992.
- New remote-request regression covers unequipped hammer rejection, switching
  away/back invalidating inspection, dropped wrench rejection, raw edit/remove
  bypass attempts and successful authorized hammer. The world remains unchanged
  throughout denied attempts. Existing ray/ownership tests now set up their tools
  through the trusted host API rather than bypassing equipment implicitly.
- Focused sim session/tools checks pass, including the new regression. Real QUIC
  suite8 passed/2 asset cases explicitly ignored after selecting tools in fixtures.
  Initial failures were expected missing fixture equipment, corrected without
  weakening runtime checks. Root has not launched a visible game or audio device.
- Parallel reports: item HUD/input agent has compiling native icons, replicated
  slot mapping and14 focused tests; effects agent has expanded native pack002
  with missing Horse Ray/sports/Tank/cannon resources and passing adapter tests;
  admin agent has standalone roles/commands/bans/config foundation. These reports
  do not establish normal-game integration or alpha acceptance of those systems.

## 2026-09-26 — subagent model change
- Maxwell instructed that all further subagent work use GPT-6 Luna. Root
  interrupted the three running Astra subagents (administration screens,
  inventory UI, world item rendering), preserving their workspace changes.
  Replacement Luna agents continue from those files and recorded handoffs;
  previous Astra assignments must not be resumed.

## 2026-09-26 — item HUD and selection handoff verification
- Continued the existing item HUD/building integration from its recorded README.
  The HUD builds the complete 21-entry native catalog from pack003, retains
  original icons for17 entries, and resolves the four sports ball items through
  the original `print_letters_default/icons/b` fallback. App keeps shared item
  assets alive for renderer registration; no runtime source reader was added.
- Re-ran `cargo test -p bri-client --lib building::tests`:14 passed,1 ignored
  (separate converted stock-camera fixture),0 failed. Re-ran actual-pack icon
  catalog and two-pass offscreen GPU/reset test with
  `cargo test -p bri-client --lib item_ui::tests -- --ignored --nocapture`:
  2 passed,0 failed. No window or audio was used.
- Re-ran `cargo test -p bri-client --lib app::tests::native_weapon_catalog_startup_and_headless_host -- --ignored --nocapture`:
  passed, including normal UI selection/drop, authoritative inventory/drop,
  and disconnect cleanup.
- `cargo clippy -p bri-client --all-targets -- -D warnings` currently stops on
  two warnings in concurrent, unowned `crates/ui/src/models/admin.rs` and
  `crates/ui/src/screens/admin.rs`; no item HUD/building diagnostic was reached.
  Root owns those UI changes and can rerun Clippy after their cleanup.
- Remaining item work is the shared gameplay integration documented above:
  wand destruction, sports alternate actions/rules, mounted-item animation and
  model-driven muzzle positions; automated HUD checks do not establish play feel.

## 2026-09-26 — shared model rendering and client integration
- Previous goal turn classified as progress: changed the collaboration policy,
  stopped Astra assignments and continued their preserved work with Luna only.
  Maxwell reaffirmed maximum Luna parallel work; current assignments are weapon
  effects App integration, authored avatar weapon poses, and admin Session/QUIC.
- Root added persistent affine instance buffers, shared material/texture bindings
  for posed geometry, inverse-transpose normals, transformed transparency sorting
  and alpha fading in the main scene renderer. Ordinary geometry uses identity
  instances. Independent CPU-geometry pixel comparisons and invalid/overflow
  buffer update tests passed; posed shared-binding equality/mismatch tests passed.
  Scoped render/sim strict Clippy passed before subsequent client integration.
- AvatarMesh now exposes original sampled world-space nodes. Native Mount0/Mount1
  and body-transform test passed. Root moved avatar CPU sampling into App tick,
  including the hidden first-person body, and connected WorldItems sync/upload/
  instanced draws/device reset/disconnect. Initial normal client cargo check passed.
  This does not yet prove correct held-arm poses or complete weapon effects;
  these are the next coordinated integration assignments.
- Root hardened trigger release: unequipped late releases are harmless; full
  trigger queues accept release, cancel pending down edges and report the count.
  Native queue regression and core-switch regression passed. Equip sound remains
  legitimate; the no-shot assertion specifically checks gunshot/projectile output.
- Luna inventory handoff passed actual App selection/drop/disconnect tests and
  native icon checks. Luna world-item handoff passed three adapter checks. Root
  review found obsolete cached models/pose slots could permanently consume limits,
  and alphabetical model grouping could displace local held items. Reclaim now
  releases unused models/slots, retains active uploads, and processes local/near
  groups first. Five adapter tests pass, including the new capacity regression
  and actual native Gun pixels compared with independently CPU-transformed
  geometry, identical post-reset rendering, and empty rendering after teardown.
  Full persistent renderer suite now passes10 tests/2 explicitly ignored assets.
  Asset-dependent tests are explicitly ignored by default and run with converted
  packs for native verification; generic fade tests require no content.
- Luna admin UI handoff reports 63 library and four admin tests passing, scoped
  strict Clippy and actual-pack offscreen render success. Root assigned subsequent
  authority integration; UI availability alone does not establish server support.
- Root animation review rejected an initial arm-only filtering proposal: original
  additive action clips intentionally carry body channels. It also caught an
  absolute held-arm layer ordered after additive look/headup, which the native
  mixer rejects. Luna is correcting both against the preserved clip flags.
- Client Clippy/combined checks temporarily encounter the in-flight admin module;
  no completed combined gate is claimed until that agent's module is buildable.
- Subsequent review caught core-tool readiness looking in the weapon-only item
  table (which excludes the core tools), stale action poses on direct equipment
  changes, and raw Eye-node orientation lacking pitch. Luna assignments now
  address these. Eye rotation must combine authored animated position with view
  orientation; recovered engine-family getEyeTransform supplies that evidence.
  The initial concern about Eye's180-degree local bind rotation alone was not a
  proven defect: parent bind rotations cancel it. Missing pitch is the real issue.
- Admin agent reached a passing bri-net check and continues role/disconnect
  replication/tests; FX agent is adapting client events. Subsequent in-flight
  edits still prevent a final shared client Clippy claim. Work remains active.
- Full expanded contract remains incomplete. No visible window, desktop input,
  audible output, original-install mutation or GitHub publication occurred.

## 2026-09-26 — administration client boundary and continued integration
- Previous goal turn classified as progress: inspected the existing finite terrain
  implementation to give Maxwell a nonoverlapping substantial Opus brief. Terrain
  delegation was proposed, not assumed to have started. Root continues shared
  integration while all three Luna assignments remain active.
- Root added client/admin_ui.rs mapping authenticated snapshots, stable connection
  targets, supported actions and request-correlated replies. It never transmits a
  claimed acting role/host flag. Unsupported operations reject; wrong passwords and
  wrong reply variants cannot become successful UI acknowledgements. Three focused
  client adapter tests pass. Root subsequently connected normal App dispatch,
  authenticated state, request-correlated replies and disconnect teardown.
- Owner0 is a valid unowned brick group. UI group validation now accepts it while
  retaining uniqueness and keeping player/ban ID validation separate. Highlighting
  has its own capability, so enabling brick deletion does not advertise an absent
  highlight handler. Buttons requiring a group remain inactive without a selection.
  Admin screen/model tests pass5, with1 native render test explicitly ignored.
- Shared client now compiles. Root scoped `cargo clippy -p bri-client --test
  world_items -- -D warnings` passes after replacing a redundant GPU unwrap with
  an explicit matching borrow. Agent App/animation/QUIC checks continue separately.
- Root wired StartGame Admin/SuperAdmin credentials through the validated trusted
  prejoin setup API. Join password still rejects explicitly. The expanded native
  App/QUIC host test passes: host SuperAdmin snapshot, original Admin menu, group
  query, password change, normal tool/drop lifecycle and admin cache reset.
  It also verifies image-state transitions preserve an active avatar action,
  while switching the actual mounted image clears it. Root removed image state
  from equipment identity and drains redundant thread2 FX requests after App
  takes ownership in its separately bounded avatar queue.
- Root client library suite passes59 tests/16 explicitly ignored native cases;
  native host and FX cases and original avatar cases were run separately as
  recorded above/by agents. No aggregate alpha completion is implied.
- Continued Luna assignments now cover persistent native identity/ban durability,
  original weapon casing/debris conversion/runtime, and normal-client offscreen
  first-/third-person item presentation. Root reviews protocol/security and source
  assumptions. Terrain work was proposed for Opus; initiation is not yet verified.
- Session and protocol-v9 QUIC administration authority now use server-owned
  connection IDs, host-credential-bound roles, and host bits retained in opaque
  server-issued resume tickets. Role/password changes replicate authoritative
  snapshots and update existing Session permissions; kick and password lockout
  close the authenticated peer, and clear operations preflight before mutation.
  Session setup supports validated Admin/SuperAdmin passwords only before the
  first client joins. Focused admin/sim/net tests pass (10 authority, 47 sim,
  10 QUIC; 6 asset-dependent tests ignored across sim/net), as does targeted
  Clippy with `-D warnings`. Five asset-dependent tests were ignored. Ban/unban remain unavailable until a host-verified
  durable identity and persistent ban store exist; join passwords and remaining
  host menu actions still lack adapters. App binding remains in progress.

## 2026-09-26 — persistent administration client integration
- Previous goal turn was coordination-only: Opus terrain work was proposed but
  has not been confirmed started. Root resumed concrete client work while three
  Luna agents own identity/ban authority, weapon debris and minigame UI.
- Root connected App host/join to the new proof-of-possession identity API and
  host startup to persistent administration storage while retaining the selected
  player limit. Identity IO runs off the UI thread, uses the supplied state
  directory, and headless tests retain their isolated temporary directories.
- Normal executable defaults now select per-user platform application data rather
  than the working directory. Explicit state directories remain supported; no
  existing user files are migrated or removed.
- Ban/unban/list client mappings now preserve stable IDs and use the host's
  timestamp for remaining duration. List replies cannot overwrite a newer
  administration revision. Two additional adapter regressions and an expanded
  normal App host test are written; combined execution is pending the agents'
  shared compile milestone and is not yet claimed passing.
- Root inspected the earlier normal App Wrench first-person render and third-
  person hand crop. The previous offscreen test passed with visible core tools,
  deselection and device recreation. Its later third-person zoom-only test edit
  still requires a rerun; earlier pixel counts do not verify that edited revision.
  Lighting/overall visual fidelity and actual interactive feel remain open.
- Maxwell subsequently confirmed Opus is actively implementing terrain fidelity,
  CDLOD and collision streaming in its cloud workspace. It owns terrain-specific
  files and will deliver reviewed patches for shared App/content/simulation/server
  files. Its proposed software-Vulkan verification does not establish Windows GPU
  correctness. RepeatTerrain defaults for omitted fields remain an open source
  question, not a resolved assumption.

## 2026-09-26 — first-playtest scope and stabilization
- Maxwell explicitly requested wrapping up with necessities only and selected
  "Core building playtest first; clearly list unfinished features." The current
  gate is now docs/playtest-contract.md. Full vanilla combat, vehicles,
  minigames, events and remaining fidelity remain roadmap work; they no longer
  block this first handoff. No full-alpha completion is claimed.
- Root froze all Luna feature expansion and requested coherent, tested checkpoints.
  Focus is launch, maps, movement, building/tools, saves and basic multiplayer,
  then a self-contained Windows package and useful playtest instructions.
- Persistent identity App host test now passes with private temporary state,
  authenticated host role, correlated ban-list query and stable identity reload.
  Client admin adapter tests pass5; admin UI tests pass6/1native-render ignored.
  Final offscreen/combined checks continue after transient UI edits settle.
- Subsequent checks pass: client library64/19asset-dependent ignored, transport1;
  actual native App flow2 (core building/save-load/HUD plus weather settings),
  actual native core-item offscreen1 including the corrected third-person zoom.
  The ignored normal App identity/admin startup was also run explicitly and passes.
- Maxwell then required waiting for Opus terrain completion before the final
  build/package, and authorized small Luna fixes/nice-to-haves while waiting.
  Packaging assembly is held. Luna scopes now focus on package/launch/trust-pin
  helpers, bounded settings/input usability, and clear multiplayer errors after
  finishing the identity checkpoint. No broader gameplay expansion was resumed.
- Combined client Clippy now reaches Opus's newly arriving terrain implementation
  and reports two terrain-owned physics lints (nonminimal bool and redundant
  float cast). Root is not modifying those in-flight files; the final combined
  gate waits for Opus's completed handoff. Earlier passing tests are checkpoints,
  not evidence of a frozen final package.

## 2026-09-27 — packaged first building playtest
- Opus delivered only terrain data/conversion/collision components, without
  renderer/runtime integration or terrain render evidence. Root preserved this
  work, corrected its strict-Clippy lints, and explicitly selected the already
  tested finite map-bundle-014 path. Unintegrated 015 is excluded from the package.
  Streaming/fidelity remain unfinished and are disclosed in KNOWN-ISSUES.md.
- Final multiplayer gate exposed ghost administration rows from failed player
  spawn retries. Luna added rollback on failed join/resume and a regression for
  live rows and monotonic connection IDs. Root reviewed it. A stale-snapshot
  race in the persistent-ban test was fixed by requiring a newer revision.
  Full QUIC loopback: 12 pass, 2 ignored. No diagnostics remain from the fix.
- Final root gates pass: strict Clippy for client/content/physics/converter all
  targets; both native App flow tests, including host/cancel/rehost, building,
  tools/events, save/load, chat, menus/HUD and weather settings. Earlier final
  checkpoints also pass: client library64, transport1, core-item offscreen1,
  explicit native identity/admin startup1, client admin adapter5 and UI admin6.
  Asset-dependent ignored tests were run explicitly where listed; ignored tests
  elsewhere are not represented as passing. No visible window, desktop input,
  audio playback or original-install writes occurred.
- Built locked optimized x86_64-pc-windows-msvc client with static CRT. PE import
  inspection confirms no VCRUNTIME/MSVCP DLL dependency. Added --check for silent
  startup validation using the same App/content loader as the game.
- Local release: dist/BlocklandReImagined-building-playtest-2026-09-27-01.
  Includes 14 selected native packs / 3147 content files, normalized selection,
  executable, launch/log and certificate helpers, PLAYTEST.md, KNOWN-ISSUES.md,
  and a SHA-256 manifest. Verified every one of 3155 payload entries. The packaged
  executable loaded its own content successfully: 14 maps, 170 brick definitions,
  from outside the repository root with isolated fresh state and no window/audio.
- ZIP is 74,283,235 bytes; every archived payload was also checked against the
  manifest. ZIP SHA-256:
  9640b6a5e1cf38d50ccaa0329dbd7b28072e0b77783087b4b82201f549dc486e.
  Executable SHA-256:
  a23f67289471afab300c163f3a82ca8685aecfc4fde2522bd098be4a226e855f.
- This satisfies packaging for the explicitly narrowed first building playtest,
  not the full vanilla contract. Maxwell's interactive feel/audio/visual review
  remains necessary. Complete vanilla alpha goal remains active.

## 2026-09-27 — post-handoff startup preferences fix
- Previous goal turn made concrete progress: packaged/verified Windows playtest,
  committed source and pushed new private MaxHastings/BlocklandReImagined repo.
  Release tag playtest-2026-09-27-01 points to 3c90a6e. GitHub privacy and matching
  local/remote main were verified. Original content is local only.
- Follow-up source audit confirmed device-open failure already falls back to
  silent audio. Its warning was retained only in memory; startup now emits it
  into the launcher's stderr log. No audio device was opened for this audit.
- Found and fixed an actual restart gap: main ignored saved display preferences
  and always passed 1280x720/windowed/VSync to the platform. Startup now reads
  native user overrides with case-insensitive keys, validates resolution against
  default GPU limits, and restores size/fullscreen/VSync. Invalid saved sizes
  fall back to windowed 1280x720 without modifying the saved file.
- Startup fullscreen now selects the saved exact-resolution monitor mode like
  the Options screen does. If that monitor mode is no longer available, launch
  falls back to windowed and reports why. Unsupported saved no-VSync modes fall
  back to FIFO at startup; interactive unsupported changes still reject normally.
- Settings parsing and on-disk persistence regressions pass; client all-target
  strict Clippy passes. These verify configuration and compilation, not actual
  monitor switching. Maxwell remains responsible for interactive verification.
- The delivered folder/ZIP/tag remain unchanged. In that release, display
  settings take effect in-session but startup still uses the defaults. This
  follow-up source correction is for the next build, not silently substituted
  into the already verified package.

## 2026-09-27 — user playtest feedback and full continuation handoff
- Maxwell reports a crash around crouch/jump, then clarifies it may be jetting;
  the exact input combination is uncertain. He also reports movement/jetting
  feel is off. These reopen core-playtest blockers; packaging success was not
  interactive acceptance. Do not claim his actual crash is proven fixed yet.
- Inspected release logs are empty. Root reproduced a launcher defect under
  Windows PowerShell with a console-only fixture: redirected native stderr plus
  ErrorActionPreference Stop swallowed the error and replaced native exit7 with1.
  Source launcher now uses direct process stream redirection. New
  Test-PlaytestLauncher.ps1 passes both exit0-with-stderr and exit7, preserving
  stdout/stderr and exit status. No game/window/audio was used for this test.
- Rig metadata identifies a concrete animation ordering problem: additive jump
  priority8 could precede absolute armReady14/crouch20; the sampler rejects this.
  Local avatar assembler now orders absolute base layers before additive layers.
  Initial CPU movement regression passed after the change. Luna is completing
  the expanded original-content GPU/held-tool/jet transition test checkpoint.
  This candidate and corrected launcher are NOT in the shipped 01 package.
- Maxwell requested a full handoff. docs/ALPHA-HANDOFF.md captures product intent,
  original expanded vanilla scope versus early playtest scope, actual released
  source/package identities, existing systems/evidence, terrain handoff limits,
  live fixes/feedback, commands, next work and ownership boundaries.
- Maxwell confirms another agent is actively editing movement/prediction in the
  same workspace. Root did not author or review those concurrent sim changes and
  leaves them intact. Receiving agent must coordinate before integration/build/
  commits; prior green tests do not certify the moving working tree.

## 2026-09-27 — alpha push: movement, combat, vehicles, events, special bricks
- Movement: client prediction replays unacknowledged inputs against a collision
  mirror built exactly like the server's (`bri_sim::prediction`). The server
  consumes one queued input per 120 Hz tick, holds on short gaps, catches up on
  backlog and acknowledges the processed sequence in each pose. Remote players
  interpolate nine ticks behind. Poses go out at 40 Hz; movement datagrams carry
  the last six inputs. Protocol version 11.
- Combat and minigames run on `bri-minigames`: health, falling damage, kills with
  v20 death messages and icons, respawn timers, team chat, minigame invites and
  settings. Vehicles run on `bri-vehicles` with spawn bricks, seats, driving,
  damage and respawn. Bots are server-side peers spawned from bot bricks that
  fight inside the brick owner's minigame. LAN hosting advertises on UDP 28050
  with a persistent host certificate and trust-on-first-use pins.
- Wrench events: the world's seven-output stub is gone. `bri-events` is the only
  event system: 16 inputs, 65 outputs, typed rows, named targets, relays, delays,
  cancel and row toggles. World schema 2 stores `bri_events::Row`s. The host
  (`crates/sim/src/session/events.rs`) fires onActivate, onPlayerTouch/onBotTouch,
  onProjectileHit, onBlownUp, onRespawn, onTeledoorEnter/Exit, OnKeyMatch/
  Mismatch, onTouchdown and onBallHit. It applies brick, player, client and
  minigame outputs; harmful player outputs need a shared minigame. Projectile
  outputs, burning, player scale and onToolBreak remain gaps (known issues).
- Explosions and heavy hits knock out bricks within `max_volume` when the
  shooter's minigame allows brick damage. The bricks respawn after the
  minigame's brick respawn time.
- Saves: `bind_world_events` types the vanilla `+-EVENT` records against the
  catalog and binds `+-VEHICLE`/`+-AUDIOEMITTER` spawn bricks:
  `cargo run --release -p bri-convert --bin bind_world_events -- content/worlds-pass-004 content/events-pack-002/catalog.json content/audio-pack-001/manifest.json content/weapons-pack-003/weapons.json content/vehicles-pack-007/vehicles.json content/effects-pass-004/effects.json content/worlds-pass-005`
  Result: 35 worlds, 1491 rows, 1483 runnable. The eight others are empty
  source lines, kept as preserved rows. 8 vehicle and 7 music bricks were bound.
  `events_native` (ignored, needs content) installs every evented save and runs
  its activations for ten seconds. The only diagnostics are sandbox permission
  rejections (AddHealth, BurnPlayer outside a minigame).
- Special bricks follow their add-on scripts: checkpoints set the respawn point
  (`/clearCheckpoint`), consecutive teledoors pair and carry players through,
  treasure chests open for two seconds once per player with found counts
  (`/treasureStatus`), swords carve pumpkins, and water bricks are swimmable
  liquid volumes (server and prediction). `special_bricks` (ignored, needs
  content) covers the first four behaviors plus water.
- Streamed terrain (from the terrain session) replaced the finite region. The
  packager and its fixture test no longer carry `terrain_region`, and they ship
  `events-pack-002`.
- Work is now split across parallel sessions (terrain, effects/audio, UI,
  jetting, tools and held items, vehicles, brick debris). This session remains
  the integrator for packaging, docs and the release build.
- Control objects (a1 playtest report: the F7 camera turned the body and Spy
  did nothing). Each peer now has one replicated `ControlObject` (Player,
  Camera, Spy(target) or Corpse) in `sim/src/session/control.rs`, modelled
  on Torque's `setControlObject`. The step feeds the walking or seated body
  only while it is in control, so the free camera, spy orbit and death
  camera leave it still and unturned. Spy uses v20's `Corpse` orbit (8 units)
  and clicking returns to the body. F7 and Spy go through the admin authority,
  and F8 drops the player at the camera. Z free look now turns only the head:
  a replicated `head_yaw` drives the original `headside` pose, clamped to
  `maxFreelookAngle` = 3. F zoom eases in and out. Tests: `session` (camera,
  spy and corpse control) and client `controls`/`avatar`.

- 2026-09-27 Internet hosting (a5). Start Game's Internet option was
  greyed out by design (`menus.rs`, "public legacy services are excluded").
  It now hosts like LAN on `0.0.0.0:28000` (UDP, QUIC) and still answers the
  certificate query on UDP 28050, which Connect to IP uses for its first
  join before pinning. Internet hosts turn `$Server::LAN` off, so brick
  damage uses v20 `miniGameCanDamage` (ownership outside minigames); LAN and
  single-player keep the looser rule. Connect to IP accepts a bare address
  (port 28000). Query Internet stays disabled: no master server exists.
  Remote hosts must allow UDP 28000 and 28050 through Windows Firewall and,
  for a public IP, forward both on the router; Tailscale/ZeroTier only need
  the firewall rule. Not yet tested over a real remote link. Packaged
  `dist/BlocklandReImagined-alpha-2026-09-27-a5`: `-VerifyPackage` verified
  3236 files; `--check` passed (14 maps, 170 brick definitions).

- 2026-09-27 UPnP for Internet hosts (a6). `bri-net::upnp` (igd-next, sync,
  no hyper) forwards UDP 28000/28050 from the router with a 1 h lease renewed
  every 20 min (falls back to a permanent lease if required) and removes
  them when the host stops. The search binds to the default-route interface
  because Hyper-V/WSL adapters can misroute SSDP. The host gets one chat line:
  public address, a double/carrier NAT warning, or manual-forwarding advice.
  Maxwell's own router (192.168.88.1) answers no UPnP search, confirmed by
  Windows' HNetCfg.NATUPnP too, so it shows the manual advice. Packaged
  `dist/BlocklandReImagined-alpha-2026-09-27-a6`: `-VerifyPackage` 3236 files;
  `--check` passed.

- 2026-09-27 Player step-up beneath ceilings (v20 clearance). Players stopped at
  a plate or brick step under a ceiling that v20 players walk beneath. The
  player box matched v20 (1.25 x 2.65, crouched 1.0, confirmed in the v20
  executable's PlayerData offsets and the 0.25 box scale), and brick and
  interior collision are exact boxes and hull faces as in Torque. The cause was
  step-up: Rapier's autostep needs `step_height` (1.0) of room above the head,
  while v20 `Player::step` only needs the player's own height above the new
  step. `bri-sim::player::v20_step` now implements the v20 rule. New tests in
  `crates/sim/tests/player.rs` cover a plate under a five-brick ceiling, a brick
  step under 3.4, a crouched plate under 1.4, a one-plate-short ceiling that
  still blocks, and jumping under a 2.8 lintel. `cargo test -p bri-sim` passed.
- 2026-09-27 Spray can particles take the selected colour. Every colour can
  showed the blue can's navy mist and splash. v20 `setSprayCanColor` copies
  the `bluePaint*` explosion/droplet particles per palette index as
  `color<N>Paint*` (palette RGB, 8/255 floor and additive blend when
  translucent), and the nozzle's `bluePaintEmitter` has `useEmitterColors`,
  fed by the image's colour shift. The weapons runtime now names the can's
  effects `color<N>PaintEmitter`/`color<N>PaintExplosion`; the client resolves
  them to the blue originals with an `fx-runtime` `Recolor` (palette RGB,
  authored alpha keys). The mist keeping its authored 0.5 to 0 alpha is
  inferred: the engine's emitter-colour path is closed source. No protocol
  change. Evidence: `cargo test -p bri-client --test spray_paint_render --
  --ignored` renders `artifacts/spray-paint/spray-paint-colors.png` (red,
  green, yellow, white, translucent blue and black) and checks every particle's
  RGB; sim `spray_cans_mount_in_hand_and_paint_by_projectile` checks the cues.

- 2026-09-27 Diagonal walk animation. Cause: at 45 degrees the picker
  compared |right| > |forward| on the render-interpolated body (live mouse
  yaw, lerped tick velocity), so ulp-level noise flipped `run` (legs swing)
  and `side` (hip bob only) every frame and restarted the clip each time; an
  offscreen capture measured right-leg jumps of 77 deg between 60 Hz frames.
  Now `avatar::locomotion` follows v20 `pickActionAnimation` as read from
  blocklandv20.exe by the quirks thread: re-pick every frame (no
  `delayTicks` hold), root below 0.4 object-space speed, then run/back/side
  with curMax 0.1, strict `>` (ties keep run/back), side reversed for right,
  crouch maps to the crouch clips, fixed 1.0 time scale. `setActionThread`
  semantics: same action never restarts (reverse flag stays stale), a change
  restarts at pos 0 (1 if reversed) with a 0.25 s transition (0.15 s jumps)
  from the frozen pose (`animation::sample_layers_with_transition`). The pick
  reads the latest simulated tick (`Motion::ticked`), where rotation and
  velocity agree, as Torque's tick-time `mWorldToObj` does; `PICK_TIE` (1e-4)
  stands in for Torque's bit-exact 45 degree tie. Not ported: water coverage
  gate, first-person side-to-run swap. Verified:
  `cargo test -p bri-client --test avatar_diagonal_render --release -- --ignored`
  renders forward, slow-turning and wobbling 45 degree walks offscreen; both
  diagonals match a straight run's leg motion within 0.03 deg/frame
  (sheets in `artifacts/avatar-diagonal/`).

- 2026-09-27 Spraying players (v20 `Player::SetTempColor`). A colour spray
  can hitting a player recolours the body band at the impact height above the
  feet: legs (<0.63), hip and hands (<1.04), torso and arms plus no decal
  (<1.72), worn packs (<1.98), head (<2.35), worn hat and accent above. The
  colour is the palette RGB at full alpha, with no trust check because
  `PlayerStandardArmor` is not `paintable`. 2000 ms after the latest hit
  `ClearTempColor` restores the avatar and plays `color<N>PaintExplosion` at
  scale 2 at the player's centre; respawning also restores it. The session
  overlays the temporary colours on the replicated appearance, so clients
  rebuild the avatar through the existing appearance path. No protocol
  change. Not covered: bots (AIPlayer) and v20's skirt-trim leg nodes.
  Evidence: sim `spray_paint_temporarily_recolours_the_body_band_it_hits`
  and the `spray::tests` band test; `cargo test -p bri-sim` passed.

- 2026-09-27 Torque and v20 player quirks audit (`docs/audits/torque-quirks.md`).
  The evidence is the TGE 1.x `player.cc` (MBG reference) plus a read-only
  disassembly of blocklandv20.exe: pickActionAnimation, setActionThread,
  canJump, and the crouch and jet parts of updateMove. Changes:
  - **Humping quirk.** Crouch is now v20's crouch pose thread. Crouching sinks
    over the 0.2 s clip, and releasing reverses from the current pose.
    Re-pressing crouch while rising restarts from standing, so tapping pumps
    the hips. The first-person eye follows the same thread.
  - **Jump rules.** Held jump rehops after jumpDelay (3 Torque ticks). A late
    jump works up to 224 ms after leaving a surface. Surfaces up to 80 degrees
    are jumpable. The impulse adds along the surface normal to the current
    velocity, fades from rise speed 20 to 30, and Blockland's rising guard
    applies. `PlayerState.jump` replaces `jump_held`, with net VERSION 16
    (approved by Max).
  - **Crouch jets.** Thrust goes flat along the facing.

  The picker gates (0.4 3D root threshold, per-frame re-pick, 0.25 s
  transitions) went to the diagonal walk animation thread.

  Verification: `cargo test -p bri-sim -p bri-client -p bri-net` passed, and
  clippy is clean on those crates. The offscreen capture
  `cargo test -p bri-client --test crouch_capture -- --ignored` wrote
  `artifacts/torque-quirks/crouch-tap.png` and the hip log. The hip rises from
  0.2445 to 0.5434, snaps to 0.6660 on the re-press, then sinks again.

- 2026-09-27 Chunked brick meshes. A planted brick rebuilt and re-uploaded
  the whole world mesh (Golden Gate: 964k triangles, 108 MB). Bricks now
  live in 32-unit 3D chunks (`client::world_chunks`) sharing one uploaded
  material palette (surfaces, all 77 prints, blended copies); the network
  bridge keeps a bounded `WorldLog` of changed brick ids per replica
  revision, so the chunk builder, the build-tool query mirror and the
  prediction collision mirror touch only changed bricks. Chunks carry
  bounds and are frustum culled. `perf_probe`
  on Golden Gate (machine under other builds, so absolute times are noisy):
  one plant rebuilds one chunk in ~24 ms off-thread (whole world in the same
  run: 360-1200 ms) and uploads 3.6 MB in ~2 ms; building query sync 20 ms
  to 0.04 ms; collision mirror 17-60 ms to 2 ms; frames p50 spawn 6.2 to
  1.9-4.5 ms, overview 20 to 4-6 ms. Chunked renders match the whole-world
  render (0 and 9 of 2.07M pixels differ; PNGs beside the probe report).

- 2026-09-27 Burning players turn black. v20 `Player::burn` calls
  `SetTempColor("0 0 0 1", %time)` without a position: every node black and
  no decal, restored by `ClearTempColor` when the burn ends. The explosion
  burn path (`WeaponEvent::Burn`, which already starts the flames) now also
  sets that overlay through `session/spray.rs`. A burn restore plays no paint
  explosion, and a later spray hit takes over the timer as in v20. Evidence:
  `spray::tests::burning_blackens_every_slot_until_the_burn_ends`;
  `cargo test -p bri-sim` passed.

- 2026-09-27 Builder animations (protocol 19). v20 plays thread 3 on the
  builder for `ServerCmdShiftBrick`/`SuperShiftBrick` (shiftUp/Down/Left/
  Right/Away/TO by first nonzero z, y, x), `ServerCmdRotateBrick` (rotCW/CCW),
  plant, undo (only when something was undone) and `serverCmdActivateStuff`
  (activate, or activate2 once five repeats land within 320 ms each), and
  mounts `brickImage` (`armReady`) while bricks are in hand. The ghost stays
  client-side, so `Command::BuildGesture` reports shifts and rotations; the
  server emits the thread-3 `WeaponAnimation` cue for every client, and
  `Vitals::brick_in_hand` raises the right arm (armReadyRight). The client
  holds thread 3 per player (`AvatarAnimationInput::gesture`) until the next
  one or `root`. Rebuilt outfits (spray paint) keep the running action thread
  (`AvatarMesh::continue_animation`). Not done: the grey brick model in hand
  (brickWeapon.dts is not in the weapons pack). Verified: sim test
  `builder_animations_play_on_thread_three_and_bricks_raise_the_arm`;
  `cargo test -p bri-client --test avatar_animation_render --release -- --ignored`
  renders every builder clip over the raised arm (`artifacts/avatar-animation/builder-sheet.png`).

- 2026-09-27 Chat talk animation. `serverCmdMessageSent` and
  `serverCmdTeamMessageSent` play thread 3 `talk` (the looping hip bob) and
  schedule `playThread(3, root)` after `strlen(%text) * 50` ms. The session
  now emits the `talk` cue on chat and team chat (team chat talks even when
  the sender has no team, as v20 does) and a `root` cue when each message's
  own timer runs out, even if a builder animation replaced talk by then.
  No protocol change. Verified: sim test
  `chat_talks_on_thread_three_for_fifty_ms_per_character`; the talk row of
  `artifacts/avatar-animation/builder-sheet.png`.

- 2026-09-27 Mipmaps, texture filtering and MSAA. Every scene image now
  uploads a CPU mip chain (`bri_render::mipmap`: sRGB averaged in linear
  light, colour weighted by alpha so overlays keep their pigment coverage);
  lightmap and terrain weight slots bind only the base level, so atlas
  sheets never blend neighbouring surfaces. Samplers moved to the camera
  group and follow v20's own Graphics prefs: `$pref::OpenGL::textureTrilinear`,
  `useGLNearest` ("Use Sharp Filter") and `anisotropy` (0..1 slider to
  1-16x); defaults trilinear, 8x. World-pass MSAA 4x (`$pref::Video::AntiAliasing`,
  native pref, default on; v20 had no control) covers the scene, foliage,
  effects and weather passes and resolves before the UI. Golden Gate
  1080p: MSAA costs about 0.1 ms p50. Distant brick tops and road textures
  no longer sparkle (`artifacts/perf/mips-compare.png`, `msaa-compare.png`).
  Offscreen GPU tests brick_fx, brick_material_gallery, foliage_scene,
  spray_paint_render, vehicle_render and world_items pass. `app_flow`'s
  wrench step times out on main before these changes too.

- 2026-09-27 Sun shadows (`bri_render::shadow`). v20 had no sun shadows on
  bricks or players, so these are designed for this game: 1-4 stabilized
  cascades (texel-snapped spheres), 3x3 PCF, fade over the last tenth of
  the distance, driven by v20's `$pref::ShadowQuality` radios (0 Best: 4 x
  2048 to 320 units; 1 High; 2 Medium; 3 Low; 4 Minimum = off; v20
  default 0). Only bricks, players (including the unseen first-person body),
  vehicles, items, shells and debris cast. A first version also let map
  interiors cast; Cottage (Bedroom) and Town (Kitchen) renders showed that
  darkens nearly every indoor build, because v20 sun-lit bricks even
  indoors, so map geometry no longer casts. Lightmapped interiors and
  terrain darken to at most the mission ambient, so baked shadows are never
  darkened twice; baked lighting is otherwise pixel-identical. Opaque
  batches draw per chunk without rebinding. Golden Gate 1080p MSAA: Best
  adds ~1.2 ms p50 (casters ~1.2 ms), Low ~0.4 ms. Evidence: probe
  `quality_variants` PNGs and `artifacts/native-client-weather/slopes.png`
  (first-person body shadow on Slopes terrain); client GPU tests brick_fx,
  foliage_scene, world_items, vehicle_render and app_flow weather pass.

- 2026-09-27 Grass mipmaps. The foliage replicator's textures now upload
  mip chains whose alpha is rescaled per level to keep the alpha-tested
  coverage of level 0 (`mipmap::chain_preserving_coverage`, cutoff 0.5
  from the pack), so distant grass stops shimmering without thinning out.
  Bedroom `foliage_scene` render: grass coverage per screen band is
  unchanged (0.562/1.0/1.0/0.996 before and after); 3.9% of pixels differ,
  at blade edges.

- 2026-09-27 Shadow casters follow v20's projected shape shadows (Max's
  direction): players (including the first-person body), vehicles and
  held/dropped items cast; bricks cast only with the native
  `$pref::Video::BrickShadows` (default off). Baked surfaces now darken by a
  fixed share under a caster (mission ambient/(ambient+sun), bounded
  0.4-0.7) instead of down to ambient, because the Bedroom carpet's baked
  light is already at ambient and hid player shadows entirely
  (`artifacts/native-client-flow/bedroom-third-person.png` now shows one).
  Also fixed the flaky app_flow wrench/printer timeout: a cancelled click's
  late inspection notice reopened the wrench; notices now open only if no
  tool switch/cancel/close happened since the click (70 runs, no wrench or
  printer failure; one unrelated rehost "Connection changed while reading
  the build" timeout seen once).

- 2026-09-27 Seats, field of view and dismount (Max's reports). From
  blocklandv20.exe: the FOV is horizontal, so the 90 degree default had been
  rendered about 121 degrees across on 16:9 (the dizzy turning, on foot and
  in vehicles); fixed at the projection. `setLookLimits` clamps only the arm
  look thread (0x5a53b0), not the view; `Player::processTick` (0x5b2cad)
  gives a mounted rider fire, jet and pitch, so every rider uses tools (the
  Tank turret and cannon packages excepted), and jet dismounts (0x5b03d8,
  Tutorial "press Jet") while jump brakes wheeled vehicles. Every rider,
  passengers too, faces the seat; the mouse only tilts the view. v20 has no
  dismount sound. Evidence: `docs/audits/vehicles.md` rows 1, 25, 31-34;
  `cargo test -p bri-sim --test vehicles -- --ignored` and
  `cargo test -p bri-client --lib`.

- 2026-09-27 Shadows stop at the first surface (Max: his shadow showed both
  on the brick building he stood on and on the Bedroom floor beneath it).
  Non-casting bricks, interiors and terrain render into an occluder layer
  per cascade; the receiver filter gathers caster and occluder depths and
  drops a caster's shadow where an occluder sits more than 0.1 units
  between caster and receiver (smooth 3x3 PCF from a 4x4 gather). With
  Brick Shadows on, bricks cast and only map geometry occludes. Evidence:
  `bri-render --test shadow_occluders` (tower top shaded, floor below
  unchanged, and shaded without occluders), Slopes first-person shadow
  still lands on terrain. Brick Shadows checkbox added under Anti-Aliasing
  in Options > Graphics (the authored Shadow Quality box clips a sixth row).

- 2026-09-27 Vehicles thread paused (session limit). Landed on main: horizontal
  FOV (879b923), v20 seats with tools while riding and jet to leave (31d944a),
  riders flush with tilted seats (f8f1e5f), Vehicle Mouse Invert (cc155c3).
  The rider chase camera already follows the rider's look plus `cameraTilt`
  as v20's `Player::getCameraTransform` does, so it was left alone. Next, not
  started: for every WheeledVehicle (Jeep, Tank, Flying Wheeled Jeep, ball,
  skis, tumble), (1) linear damping drag/mass and angular damping
  rotationalDrag + drag (world.rs `linear_damping`, today only FlyingWheeled);
  (2) wheel steer -(s*|s|) instead of -s (`w.steering`); (3) mouse-steering
  auto-return by `1 - rate * min(|throttle|, maxSpeed) / maxSpeed` per 32 ms
  tick when yaw is 0 (0x570c4a, verified), for skis as for the Flying Wheeled
  Jeep, and no return for the FlyingVehicle carpet (drop
  STEERING_RETURN_PER_TICK). Each needs a test in crates/vehicles.

## 2026-09-28 — mod platform stress campaign (cloud, PR #1)

Goal: make the game easy to modify at its core by building modes nothing
like the Stress Lab, red-teaming the package sandbox, and fixing each class
of weakness with the smallest general seam. Record:
`docs/stress-lab/weakness-ledger.md`; handoff section "Beyond the Stress
Lab" in `docs/stress-lab/HANDOFF.md`.

- 29 experiments and two red-team rounds, tagged by Max's nine seam
  families; 15 classes (W1 to W15). All fixed except W14 (closed engine
  kinds: movement datablocks and control objects), whose two tests are
  ignored and registered in `tools/gate-known-failures.toml`.
- New package seams: `on_death`, `player` operations, `place_brick`,
  entity variables at spawn, policy points (`allow_respawn`,
  `allow_build`), state `visible` audiences with per-client views,
  scoreboard bindings, per-origin shares of script work, world edits, chat,
  entity slots and state bytes, and a start-of-tick view shared by calls.
- Platform: one package path rule, load-time conflict checks, verified
  package cache, byte-bounded join chunks, storage budgets and reliable
  outbox. Protocol 35 (hosting's listing took 34 first).
- Evidence: `cargo test -p bri-sim --test unlike_modes --test
  hardening_packages --test packages`, `cargo test -p bri-package-runtime`,
  `cargo test -p bri-net`, clippy clean on Linux; Windows CI on PR #1.
- Not saturated yet: the last round (E26 to E29) found no new class, but
  W14 is open and package state does not replicate to clients yet.
- E30, player archetypes (Max: "custom player controller models beyond
  just everyone being a Blockhead"). `PlayerState.datablock` (the closed
  `PlayerType` enum) became `archetype`, an index into an `Archetypes`
  table the checkpoint carries: v20's eight datablocks first, then
  packages' `archetype` files (a base plus the constants they change). An
  archetype is movement constants, collision body (`box`, `ball`), steering
  model (`strafe`, `turn`), health, riding rules, model and camera distance.
  Packages assign one with `set_archetype(player, id)` (kept across
  respawns); mini-games may pick named ones. The client predicts from the
  host's table. W14 is fixed for archetypes (E22 now passes); control
  targets (E27) stay open. Architecture: `docs/player-simulation.md`
  "Player archetypes". Evidence: `cargo test -p bri-motor`, `cargo test -p
  bri-sim` (all content-free targets), `cargo test -p bri-net`, `cargo
  test -p bri-client --lib`, clippy `-D warnings` clean.
- Merged main's join package check (81604e1). E31: a refused join now
  names the differing packages as a typed `PackagesDiffer`, and
  `Client::connect_fetching` downloads the server's packages and joins
  again (`package_sync::a_refused_join_downloads_the_missing_packages_and_joins`).
  The game client still joins without it until it can load a downloaded
  package. Observed once under a loaded machine, not reproduced in 14 runs
  (12 in parallel): two `package_sync` tests lost the server's refusal
  frame ("closed by peer") or found the per-address download slots still
  held, both consistent with the server's 1 s wait for the client to read
  its last frame.
- The game client now joins remote servers through `connect_fetching`:
  downloads go to `<state>/package-cache`, `bri_client::mods::load_fetched`
  loads the client's own packages with the downloaded ones in their place,
  and the HUD and entity models draw with that catalog. A server running
  different base game content is refused with that reason.
- Merged the Stress Lab (main da5668e). Package state
  now replicates per client: the welcome carries
  `Session::package_state_for(viewer)` and the server sends each client
  `Message::PackageState` when its own view changes (it left `Delta`). The
  miner's purse keys are `"visible": "owner"`; the loopback test asserts
  another client never receives them, and its world-convergence wait is now
  bounded in time rather than 40 messages (alice no longer waits on bob's
  purse, so she had not caught up). `stresslab test`: strata.rhai generates
  25 chunks (10,934 voxels) with 0 diagnostics inside the 400k Generate
  budget.
- E27 passes, closing the rest of W14: a package hands a player one of its
  entities (`control(player, entity)`, `release(player)`, capability
  `player`, `ControlObject::Entity`), which moves by its kind's
  `archetype` (a kart turns) while the avatar stays behind. The client
  orbits the entity and parks its avatar prediction as when seated; it
  does not predict the entity. Removed its `tools/gate-known-failures.toml`
  entry. Package-authored controllers are recorded as the tier-2 sandbox
  case (HANDOFF, ledger W14).
- Players whose archetype's look is a package box model draw as it; third
  person uses the archetype's camera distance.
- E32 (Max's Minecraft-like cube): `texture` (PNG) and `block` content
  kinds; a block has per-face textures or flipbooks and named states.
  Materials of a generated world may name a block, and its voxels carry
  `Brick::look` (block, state), which replicates and saves with the brick.
  `set_block_state(brick, state)` (capability `world.edit`) and `aim()`'s
  `block`/`state` let a dig tool crack a block through states before
  digging it out (`cargo test -p bri-sim --test blocks`). Found W14 again
  (closed brick looks). The world renderer does not draw block faces yet.

## Longer-term next actions (after first playtest)
1. Finish building fidelity and large-world loading/rendering performance.
   Integrate local prediction, remote interpolation and remaining camera presentation.
   Finish persistent host/reconnect identity, LAN discovery and trust/admin flow.
2. Bind default add-on lights/emitters/sounds and native behavior adapters.
   Integrate sky/environment/decorations, remaining brick color/shape FX, model materials,
   animation state selection/customization, measured movement and all stock vehicle/item
   behavior. Complete full event targets and minigame authority as shared systems.
3. Complete the full alpha contract and package a coherent game for Maxwell;
   technical probes are not the requested handoff.

- 2026-09-27 v20 bug sweep (`docs/audits/bug-sweep.md`). Fixed explosion
  splash to match `ProjectileData::onExplode` (no cover test, centre
  distance, quadratic falloff, projectile scale, grounded push flattening)
  and made splash honour minigame `selfDamage`. Minigame reset now respawns
  the owners' vehicles and restores their items, and event-driven resets
  announce themselves. Ranked open items (host stops on any step error, LAN
  and public-brick trust, duplicate loadout items, private PlaySound, Horse
  Ray, join/leave messages, shell casings) went to the coordinator.

- 2026-09-27 map tree/shrub leaves (Kitchen palms, Bedroom maple/oak/pines).
  Checked the Kitchen, Kitchen Dark and Bedroom TSStatic trees against the
  v20 missions: DTS models, node/quaternion conventions, scale, rotation,
  placement and count all match (the two Kitchen palm05 "shrubs" are sunk
  about 20 units into the sand in the original mission). No replicators apply.
  Defect: leaf textures are soft-edged cutouts (~40-53% clear, 28-48% solid),
  so the loader made them Blend with no depth writes, and each tree's leaves
  drew as one unsorted batch in mesh order; back fronds painted over front
  fronds and the trunk showed through near fronds. Static-model soft cutouts
  now draw solid texels in a Mask(0.5) depth pass plus a blended soft-edge
  twin batch (`scene_loader::load_static_shape`). Glass stays Blend; the stove
  burner texture also qualifies. Renderer-only; no pack regeneration.
  `scene_snapshot` gained an eye position and streamed terrain. Evidence:
  `artifacts/native-static-foliage/foliage_before_after.jpg`; `cargo test
  --release -p bri-render` (incl. ignored GPU cases) passes. v20's own
  translucent depth behaviour is closed-engine and not verified.

- 2026-09-27 vehicle audit and fixes (`docs/audits/vehicles.md`). Seated
  input now follows a per-seat role: drivers face the seat and passengers turn
  freely (the driver spun with the mouse before), the view turns with the
  vehicle, the tank gunner aims relative to the hull (the sign was reversed),
  the Tank Turret is spawnable, boarding needs a landing 0.2 above the origin
  and takes the first free seat. Mouse steering per Torque `mSteering` for the
  Magic Carpet (plus FlyingVehicle's unit-sphere inertia, which is why it only
  flew straight), Flying Wheeled Jeep and skis. Skis and tumbling are wired end
  to end. Run over, click-to-flip, turret hit routing, passenger protection,
  `VehicleDamageScale`, `impulseVertical` and minigame vehicle cleanup/eject
  are applied. Horse/rowboat/cannon/turret move with a kinematic player motor
  (horse follow-ups handed to Gameplay leftovers). Content: vehicles-pack-010
  (schema 4) and weapons-pack-008 (schema 3; same as 007 apart from the new
  fields and main's bae2359 importer fix). Evidence: `cargo test -p
  bri-vehicles` (28 native tests), `cargo test -p bri-sim --test vehicles --
  --ignored` (7 of 7).
  Follow-up the same day closed the rest: barrels pose their look clip,
  seated look limits, chase camera offset/tilt/lag (vehicles-pack-011, schema
  5), the ski-crash whiteout and clearing the owner's event projectiles on
  minigame change.
- 2026-09-27 save/load sounds, streamed loads, chat HUD, Tutorial gates
  (net protocol VERSION 17). Server chat lines carry their v20 message type
  (`MessageTag`: MsgUploadStart, MsgUploadEnd, MsgProcessComplete,
  MsgClearBricks) and every client plays the matching `addMessageCallback`
  sound (uploadStart, processComplete, brickClear). A loaded save is checked
  whole up front (definitions, owners, colors, item bounds), announced with
  "Loading bricks. Please wait.", then published in batches every 100 ms
  (25 bricks minimum, about 80 batches for large saves) and closed with
  v20's "N / M bricks created in 0:03.27". One load at a time, as in v20.
  Clear All / Clear Group post v20's clear messages. Chat HUD follows the
  authored NewChatHud: text at (2, 20), the Say/Team box right under the last
  line (`newMessageHud::updatePosition`) with `\c0SAY:` / `\c1TEAM:`, the VVV
  page indicator, and the who's-typing line (`MsgStartTalking` via a
  replicated `Vitals.talking`, started by the first typed non-`/` character).
  `/wand` is wired; the Tutorial keeps `/wand` for the wand room and the
  spray/FX cans for after the spray room, silently like
  `TutorialParentingPackage`. Unverified: GuiMLTextCtrl `lineSpacing = 12`
  is not applied (Torque's exact use of it is not in the references).
  Evidence: `cargo test` for world/sim/ui/net/client and strict clippy pass;
  `ui_runtime_probe` renders hud-chat-say/team/scrolled.
- 2026-09-27 Protocol 18 (Max approved): inventories may repeat an item like
  v20 (duplicate minigame loadouts no longer break respawn), and the
  Client > PlaySound event output is a private 2D `Notice::Sound`. Saves
  store fake-dead bricks as they will respawn.

- 2026-09-27 held items, balls and item physics (Gameplay leftovers group).
  Balls: `passBallCheck` catching (same minigame or both outside, alive, empty
  hands), walking into rolling/resting ball projectiles, football/soccer
  `onRest` items that mount on touch, `armor::onDisabled` drops the ball, the
  minigame StartBall (a ball in loadout slot 0), `weaponSwitchSound` on every
  pickup and `CatchFootballMessage` prints plus the in-memory record. Tool
  pickups and drops play the client `ItemPickup` sound (`MsgItemPickup`) as a
  private `Notice::Sound`. Cause of
  "the ball is not in my hand": basketballImage mounts on Mount8, which the
  player shape lacks; Torque then uses the player transform, and the client
  now does the same for every missing mount node. Script-driven arm threads
  (`playThread(1, armReady*)`) raise both arms for LeftHandedGunImage (the
  "left gun points down" report) and pose the balls. Dropped items fall and
  rest on their authored box (Rapier cuboid cast, terrain by lowest corner)
  instead of their origin. Gun casings (weapon-debris-pack-003) are finally
  simulated and drawn. The player light shines from Mount1 (left hand; Max's
  recollection, closed-engine attach point not verified). Wrench `BurnPlayer`
  and `ClearBurn` apply flames and the spray thread's burn colours;
  `onToolBreak` runs before the hammer/wand kill. Akimbo timing was checked
  against the scripts: press fires the right gun, release fires the left (0.09 s
  fire, 0.01 s smoke), matching v20. Evidence: `cargo test -p bri-sim --test
  sports --test items --test tools -- --include-ignored`; offscreen App probe
  `cargo test -p bri-client --test held_items_render --release -- --ignored`
  (frames in `artifacts/held-items/`). Open: player types, setPlayerScale,
  Horse Ray (one protocol bump).
- 2026-09-27 app_flow `native_host_cancel` timed out at the wrench. Cause:
  the client drops a Fire press while its previous trigger is still held
  until the host acknowledges the tool switch, so clicking right after
  switching tools did nothing. A click after a newer switch now always
  reaches the new image. The test also now expects the host's
  `wrenchHitSound` profile (the `WrenchHit` cue is no longer emitted).

- 2026-09-27 Options graphics controls. The Advanced tab now shows v20's
  Trilinear Filtering (default on), Use Sharp Filter and the Anisotropy slider
  (0-1). The Graphics tab shows the Shadow Quality radios (0 = Best through
  4 = Minimum, default Best) in their authored place, and a new Anti-Aliasing
  checkbox (4x MSAA, default on; no v20 control) under Resolution. All save
  on Done/Apply for the renderer's SaveSettings path. Row closing now keeps
  controls in the same column apart, which fixes Fullscreen drawn over
  Disable Vsync. Evidence: options unit tests and `ui_runtime_probe` renders
  of all four tabs.

- 2026-09-27 Brick visuals audit against v20 (`docs/audits/bricks.md`). From
  exe disassembly and emulation: bricks now decal the surface overlay after
  lighting (GL_DECAL), brickSIDE is clamped with nearest magnification, the
  ghost brick is v20's pulsing two-shell temp brick, and generated `BRICK`
  meshes match the exe's generator exactly (TOP studs were rotated 180°, SIDE
  rim UVs were wrong). Geometry pack is now `maps-pass-007` (maps-pass-003
  with 80 regenerated meshes via `regenerate_bricks`). Evidence: converter
  test against emulated output, `brick_audit` render vs
  `tools/brick_reference.py` independent v20 reference (1.1/255 mean
  difference). Open: port the exact v20 colour/shape FX equations.

- 2026-09-27 Engine foundations audit (`docs/audits/engine-foundations.md`),
  protocol VERSION 21. Fixed: a hostile movement sequence could panic a
  dev-built host; any unencodable Update/MapChanged/Notice/admin snapshot
  stopped the host for everyone (now disconnects only the affected peers);
  LAN discovery was a ~100x UDP amplifier on the UPnP-forwarded port (queries
  now padded to 1,200 bytes, replies capped at 3x) and crashed the browser on
  non-ASCII certificate text; resume tickets filled after 4,096 joins and
  refused everyone (now evicts the oldest disconnected ticket); frames that ran
  over 6 prediction ticks dropped inputs (rubber-banding after hitches);
  movement floods are rate-limited per peer; audio reopens the default device
  after a headset unplug or default-output change. The wire is one strict
  MessagePack codec for frames and datagrams (1,100-byte datagram bound).
  Golden Gate state: raw 19.2 to 13.7 MB, client decode about 30% faster.
  Open, ranked in the audit: replica world deep-clone per edit (12-21 ms at
  44k bricks), monolithic checkpoint on the authority loop, unified atomic
  writes for host identity and pins. Evidence: `cargo test -p bri-net -p
  bri-audio`, `cargo test -p bri-client --lib --test transport`, two-client
  loopback `cargo test -p bri-client --test multiplayer --release -- --ignored`,
  `wire_benchmark` (ignored, BRI_BENCH_WORLD).

- 2026-09-27 Protocol 22: v20 player types on one motor. The player motor
  moved to a new `bri-motor` crate. `PlayerState` carries its datablock, scale
  and jet energy, so the host and client prediction run the same constants
  (prediction now adopts a changed datablock). Types: Standard, No-Jet,
  Fuel-Jet, Jump-Jet, Leap-Jet, Quake-Like and Horse (plus the hidden
  BallShootPlayer), from the stock Player_* and Vehicle_Horse datablocks:
  `canJet`, `minJetEnergy`/`jetEnergyDrain`/`rechargeRate`, speeds, `runForce`,
  `jumpForce`, `jumpDelay`, surface angles, boxes and `maxDamage`. Applied by
  mini-game player type (spawn and live update), `ChangeDataBlock`,
  `setPlayerScale` (box and eye scale, speeds unchanged) and the Horse Ray.
  Horse players and spawned horses draw horse.dts with its sequences
  (`ApplyBodyColors`: body in the chest colour, head black); horse.dts copies
  of `h_root.dsq` for Blockhead-only sequences pose nothing. Horse, rowboat,
  cannon and turret mounts adopt their kinematic body into the same motor with
  constants from their PlayerData, replacing the vehicle actor motor;
  `VehiclesWorld::pre_step` takes the map liquids. `HUD_EnergyBar` draws as
  Torque's GuiHealthBarHud for Fuel-Jet and Leap-Jet. Evidence: `cargo test
  -p bri-sim --test player_types --test vehicles -- --include-ignored`,
  `cargo test -p bri-vehicles`, offscreen App probe `cargo test -p bri-client
  --test player_types_render --release -- --ignored` (frames in
  `artifacts/player-types/`). Open: horse `cameraVerticalOffset` (the camera
  pivots at the horse's eye), energy carried across a datablock change is
  clamped like v20 rather than refilled.

- 2026-09-27 Exact v20 brick colour/shape FX. Pearl, chrome, glow, blink,
  swirl, rainbow, undulo and water now follow the per-vertex equations of the
  exe's quad emitter (0x52ed70; `docs/audits/bricks.md`). FX data moved from
  the lightmap UV hack to a new `SceneVertex::fx` attribute (location 10:
  brick centre + packed ids). Blink no longer makes bricks translucent.
  Evidence: `brick_fx` and `item_rendering` GPU tests, `brick_audit` FX scene
  vs the reference renderer (1.3/255 mean difference).
- 2026-09-27 Undo parity with v20 (`serverCmdUndoBrick`, `%client.undoStack`).
  One mixed per-owner stack (`crates/sim/src/session/undo.rs`) now records
  plants, spray paint (`COLOR`), colour FX (`COLORFX`), shape FX (`SHAPEFX`)
  and prints (`PRINT`), each only when the value changed. Undoing a plant is a
  `killBrick`, so the brick breaks with the hammer's break sound and debris
  instead of vanishing. Kept v20 quirks: each press pops one entry even when
  its brick is gone; `New_QueueSO(512)` holds 511 entries; paint/print undo
  needs Full trust and its refusal prints a blank group name; the undo
  animation plays whenever the entry's brick exists. Not ported: `COLORGENERIC`
  (we don't paint bots or vehicles) and the chain-kill undo trust check (our
  bricks never chain-kill). `ToolAction::UndoPlant` is now `UndoBrick`, so the
  protocol version moved to 23. Evidence: `cargo test -p bri-sim` including
  `undo_reverts_paint_and_print_then_breaks_the_plant`.
- 2026-09-27 Held translucent spray cans draw their clear body. v20
  `setSprayCanColor` gives a colour with alpha <= 0.99 (and `jelloSprayCanImage`)
  `transspraycan.dts` with `colorShiftColor` alpha clamped to at least 10/255,
  mounted like `blueSprayCanImage` (mountPoint 0, same eyeOffset). That model
  flags only its `blank` body translucent; blank.png is white with alpha 0, so
  the body is the node colour at the colour's alpha. The item renderer drew
  that material as texture x colour with texture alpha, i.e. invisible, leaving
  only the rim and cap. `native_shape_scene` now takes `node_color`: for
  item/image colour shift, translucent materials use the overlay combine at the
  colour's alpha and opaque materials stay solid (vehicles and explosion shapes
  pass false and are unchanged). Solid trim over a clear body is inferred from
  the model's per-material flags, not the disassembly. Evidence: `cargo test -p
  bri-client --test item_rendering` (renders
  `artifacts/spray-paint/held-spray-cans.png`) and `--lib items`.
- 2026-09-27 Field of view. The reference v20 install (B4v21 patch) has an
  `SliderFOV` in Advanced Graphics Options below Anisotropy, range 70-140,
  whole degrees, writing `$pref::Player::defaultFov` (default 90); stock v20
  only had the pref. Options now shows that row, and the camera uses the pref
  in first and third person through main's horizontal-to-vertical conversion.
  Zoom now starts from the saved `$Pref::player::CurrentFOV` (v20 default 10,
  previously a hard-coded 45), wheel steps of 5 within 5-85 glide instead of
  snapping, and mouse look scales by current FOV / 90 as in v20. Our renderer
  honours the full 140, where Torque clamped at the datablock's 120. The FOV
  applies on Done, not while dragging. Evidence: `cargo test -p bri-ui --lib
  options`, `cargo test -p bri-client --lib controls`, offscreen
  `authored_options_save_players_offscreen` (options-AdvGraphics.png).
- 2026-09-27 Mouse wheel audit. v20's only wheel bind is `moveMap zaxis ->
  scrollInventory` (reference `config/client/config.cs`, mouse types 2/3).
  It is ignored on LoadingGui and while any dialog beyond PlayGui and
  NewChatHud is open (FrameOverlay/NetGraph excepted); with zoom held it steps
  the zoom FOV by 5 within 5-85; otherwise wheel down is +1 and it scrolls the
  brick bar (reopening the current slot if the bar is closed), the paint
  column's swatches or the tool slots, and with nothing selected it opens the
  bricks (tools when building is disabled). There is no wheel camera zoom;
  the engine only names the axis, and over menus GuiScrollCtrl panes take it.
  All of this was already ported (`scrollInventory` in `hud.rs`) and passes
  headlessly, including through the real App (`crates/client/tests/wheel_flow.rs`).
  The divergence was notch handling: Windows reports a high-resolution wheel
  (this machine has a Logitech Bolt receiver) as fractions of a notch, and
  gameplay stepped once per report, so one notch could lap the whole bar back
  to the same slot. Gameplay now steps once per whole notch like v20's 120-unit
  DirectInput notches, sharing the menu accumulator; reversing drops the
  unfinished notch. Evidence: `cargo test -p bri-ui --test runtime_input
  wheel_scrolls`, `cargo test -p bri-client --test wheel_flow --release --
  --ignored` (BRI_CONTENT = main checkout content).
- 2026-09-27 Leftovers group (tests, fx, content pipeline, Linux).
  - The eight tool tests paused after the v20 tool images merge are rewritten
    (identity fixtures carry the core tools as pack items; loopback swings a
    synthetic stock-layout tool pack over QUIC; the aim test uses Activate).
    `edit_brick` now validates event rows like the wrench path. Two sweep
    tests: weapons pack counts 21 items; the transition-budget test uses a
    two-state cycle because a zero-tick self-loop is the wands' timed sparkle.
  - bot_brick: a player body inserted mid-step lost its island because
    `detect_collisions` consumed its pending change without an island manager
    (Rapier debug assert). New player bodies are re-queued. Vehicles'
    checkpoint re-insert has the same shape; reported to its owner.
    brick_material_gallery matched stale omission wording; lan_discovery binds
    a free port through `Server::advertise_on`.
  - fx daa5bc5: axis-angle image rotations (HateImage "1 0 0 -90" was read as
    Euler [1,0,0]) and the PlayerSplash ring emitter, in weapons-pack-008,
    item-presentation-pack-009 and effects-runtime-pack-004.
  - Mission lighting is baked by `map_bundle` (`scene_lighting.rs`); `.ml`
    caches are no longer read and `--lighting-cache-root` is gone. Sun from
    azimuth/elevation; terrain sweep within 0.07-4.5/255 of the engine's caches;
    interiors 2.8-3.3 after finding the 10-texel lightmap border. map-bundle-016
    is in use; `terrain_bundle` is removed (map_bundle emits terrain itself).
    Evidence: `lighting_compare` against map-bundle-015, offscreen
    interior_preview of 015 vs the new bake.
  - `tools/regenerate_content.py` rebuilds all 18 ContentConfig packs from a v20
    folder (docs/content-regeneration.md). A run from an empty checkout
    against the reference install passed `bri-client --check`; eight packs came
    out byte-identical, the re-decompile matched `.research/v20-dso` exactly.
  - Linux: `cargo check`/`build --target x86_64-unknown-linux-gnu` of bri-client
    is warning-free; linked against an Ubuntu 24.04 sysroot, the binary passes
    `--check` under WSL. `tools/package_playtest.sh` and `launch_playtest.sh`
    package it. Window, input, audio and GPU on a Linux desktop remain unverified.
- 2026-09-27 Printer audit against v20 (`loadPrintedBrickTextures`,
  `clientCmdOpenPrintSelectorDlg`, `serverCmdSetPrint`). v20 ships five print
  packs plus Letters: 1x2F 10 prints, 2x2F 7, 2x2R (both 45° print ramps) 7,
  Letters 53. The 1x1, 1x1F and 1x4x4 print bricks have no print pack and take
  letters only. All 77 are converted and every set reaches the selector for
  its aspect (`cargo test -p bri-client --test print_selector -- --ignored`
  opens the real menu on each aspect and counts the shown buttons). Like v20,
  the dialog remembers the last tab, so after using Letters the other prints
  are under the Prints tab. Fixed divergences: the player's last print per
  aspect (`%client.lastPrint[%ar]`) now goes to the next planted brick of that
  aspect on the server and to the ghost on the client, instead of always
  Letters/A. Letter hotkeys work on either tab, because v20 registers every
  print button's accelerator when the dialog is pushed, hidden scrollers
  included. No wire change. Evidence:
  `next_brick_of_the_aspect_takes_the_players_last_print_like_v20`,
  `last_print_updates_the_ghost_and_later_bricks_of_its_aspect`,
  `letter_shortcuts_work_on_the_prints_tab`.
- 2026-09-27 Mouse capture, display modes and menu scaling. Windows locks the
  cursor where it sits, so after Alt+Tab or a taskbar restore it was locked
  outside the window; like v20's setMouseClipping (exe 0x607ce0, called from
  WM_ACTIVATE) the platform parks it inside and re-clips on focus, resize,
  move, restore and a 1 s watchdog, and drops the click that activates the
  window. Fullscreen is now borderless at the monitor's size (winit's
  exclusive mode changed the desktop mode, never restored it on Alt+Tab and
  asserts on failure); Options lists the monitor's real modes, filtered by
  the fullscreen toggle as v20's `OptGraphicsResolutionMenu::init` did.
  Alt+Enter and startup corrections are recorded in the video prefs; windowed
  changes un-maximize first; DPI changes keep the chosen pixel size; the
  swapchain reconfigures whenever it disagrees with the window. Relative/
  relative GUI controls scale by one factor so menu text keeps its aspect,
  and full-screen backgrounds crop instead of stretching (a deliberate change
  from v20, which stretched both). Dropped: fullscreen below native
  resolution. Evidence: `cargo test -p bri-ui -p bri-client` (content-backed
  tests need `content/`), `ui_gallery` MainMenuGui at 1920x1080, a 20 s
  windowed startup. Needs Max's playtest: Alt+Tab, restore, Alt+Enter, Apply.


- 2026-09-27 Flying Wheeled Jeep flies like v20. Decoded Blockland's flying
  forces on `WheeledVehicle` from blocklandv20.exe (`updateForces` 0x5746a0,
  `updateMove` 0x570be0, move split in `Player::processTick` 0x5b2cad) and
  replaced the invented model: thrust below `maxForwardVel`, lift of 100 ×
  nose speed capped at 4000, control surfaces and torques scaled by the
  stall bite, squared mouse steering, v20's throttle-dependent steering
  return, `rotationalDrag` + `drag`, a 200 speed cap and no jets. Details in
  `docs/audits/vehicles.md`. Evidence: `cargo test -p bri-vehicles` (new
  `tests/flying_jeep.rs`), `cargo clippy -p bri-vehicles --tests`. Open, not
  changed (shared with the jeep, tank and skis): v20 applies
  `rotationalDrag` + `drag` damping, quadratic wheel steering and the same
  steering return to every WheeledVehicle.
- 2026-09-27 held brick (protocol 24). Bricks in hand now mount v20's
  `brickImage` (`base/data/shapes/brickWeapon.dts`, mount0, offset
  `0 -0.05 0`, rotation/eyeRotation `0 180 0`, eyeOffset `0.7 1.2 -0.8`,
  colour shift 0.647 grey, armReady) as a real server image, so it replicates
  like any tool and raises the arm through the image. `Vitals.brick_in_hand`
  is gone. The weapons importer now roots `brickImage`, which also fills
  `horseBrickImage`'s inherited model; HorseArmor players hold that one on
  mount3 and a datablock change swaps the held brick (`onNewDataBlock`). A
  new loadout (respawn, leaving a minigame) re-mounts the brick while the
  client still has it selected. Content: weapons-pack-009 (008 plus
  brickImage, brickDeployProjectile/Explosion and the brick trail emitter;
  008 reproduces byte-exact from the same importer before the change) and
  item-presentation-pack-010 built against it. Effects-runtime and
  weapon-debris packs are unchanged; the image's Fire state (click-to-deploy
  swing and `brickTrailEmitter`) is not driven yet because ghost deploy stays
  client-side. Evidence: `cargo test --release --workspace`; `bri-sim --test
  session -- --ignored bricks_in_hand`; `bri-client --test app_item_render --
  --ignored bricks_in_hand` (first person 5548, third person 7001 changed
  pixels; renders in artifacts/native-held-brick). Pre-existing failure:
  `bri-weapons --test runtime inventory_drop_pickup_and_disconnect` fails on
  weapons-pack-008 too.


- 2026-09-27 Platform principles and door-closer audit (docs only, no code
  changes). `docs/architecture/platform-principles.md` records the north star,
  the mechanism/policy boundary, fifteen principles, the alpha scope rule (no
  migrations until the first beta), the definition of done for vanilla, the
  modding spikes and a phased roadmap; awaiting Max's approval.
  `docs/audits/platform-door-closers.md` grades the codebase against Max's
  twelve platform contracts with file references. P0 items, all about shape:
  owner identity by principal in saved worlds, avatar part names instead of
  indices, one content id grammar with importer-set namespaces, a package
  manifest read by client and server with a precise join mismatch report, and
  partial build loads that report missing definitions instead of refusing the
  whole build. The earlier save-format fix request was withdrawn in favour of
  this review.

- 2026-09-27 The `~` console is back (v20 `ConsoleDlg`). The authored window,
  GuiConsole log (Lucida Console 12; normal, grey warning and red error
  colours) and entry line come from the UI pack; `~` toggles it with v20's
  100 ms debounce, it shows the cursor, keeps gameplay binds off while open
  and survives content changes (its own canvas layer, like
  `pushDialog(ConsoleDlg, 99)`). Entry echoes `==>line`, keeps 20 lines of
  history (Up/Down), pages the log (PgUp/PgDn), completes names, cvars,
  `$pref::` keys, players and maps (Tab), and closes on Escape. No script VM:
  a new `bri-console` crate holds a process-wide log (mirrored to stderr) and
  a typed command/cvar registry any layer can register into. Cvars are typed
  views of `$pref::` values that persist through SaveSettings; `$pref::X =
  value` and `name(args);` work for v20 habits. Commands: help, cvars, prefs,
  echo, cls, quit, connect, disconnect, say, players, maps, netgraph,
  screenshot, admin, adminlogin, kick, ban, clearbricks, changemap, and any
  chat `/command`; the client app adds `stats` (frame, network, world, audio,
  effects) and `version` (build, protocol, content packages). Shared-state
  commands send the existing admin/chat/join requests, so host trust checks
  apply unchanged; passwords are redacted from the echo and never enter
  history. Also fixed for every text field: the caret now draws with the
  fallback profile font, and long input scrolls to keep the caret visible.
  Evidence: `cargo test -p bri-console`, `cargo test -p bri-ui --test
  console`, `cargo test -p bri-ui`, `cargo test -p bri-client --lib console`,
  `ui_runtime_probe` console renders at 1024x768, 1920x1080 and 2x.

- 2026-09-27 Player light corona. v20's `serverCmdLight` attaches the
  `PlayerLight` fxLight (radius 10, brightness 5, white, no animation) to the
  player. Its flare is `base/lighting/corona` (128x128 RGB, additive
  `SRC_ALPHA, ONE`, no depth test), `ConstantSize 1` (a 2x2 unit billboard),
  faded over `FadeTime 0.1` by a camera-to-light ray, hidden beyond 75 units,
  and linked flare colour divided by its largest channel (exe `fxLight::
  renderObject` 0x54d820). The attached position is the player's
  `getRenderMountTransform(1)` (exe 0x54c1c1 casts to Player and calls vtable
  0x160 = 0x5cc930 with mount 1), confirming the left hand (`Mount1`).
  The client now starts `v20/light/playerlight` from the effects runtime per
  lit player, so everyone sees each light and corona; the old hard-coded
  point light (radius 12, colour 1) is gone. Open: v20's flare ray is also
  blocked by players and vehicles; ours tests map and bricks only.
- 2026-09-27 One-command setup from a fresh clone. A Mac setup took a long
  detective hunt: the shipped `content/` matched an older commit, and nothing
  said which packs HEAD needed or how to build them. `tools/bootstrap.py
  --v20 <folder>` now checks the toolchain (printing the exact install command
  per OS), then runs `regenerate_content.py`, builds the client and runs
  `--check`. Regeneration stamps each pack with a hash of its inputs (importer
  sources and local path deps, upstream stamps, decompiler pins, v20 listing)
  and rebuilds missing, interrupted or stale packs plus their dependents;
  unstamped packs (copied from a package) are kept. It gained `--content`,
  `--plan`, `--rebuild` and `--keep-stale`; moves an old `client-content.json`
  override aside; builds the research importers with `--locked` (both
  lockfiles had drifted); always builds one cargo package set (a narrower set
  re-unified features and recompiled the client for 6 minutes); skips
  non-stock add-on geometry failures; and builds dso-sharp off Windows with
  `dotnet build -p:PublishAot=false -p:RollForward=Major` at the pinned 2.1.0
  commit. `build_presentation.py` read `content/ui-pack-003` and
  `avatar-pack-001` by hard-coded name; it now takes `--ui`/`--avatar`.
  `bri-client --check` names every missing pack and the bootstrap command.
  Fixed stale tests: 17 -> 21 weapons-pack items, and the weapon-debris tests
  load the configured pack instead of pack-001.
  Evidence: a fresh Windows clone ran bootstrap to a passing `--check` in 22
  min (17 of them the first release build); the same eight packs as before came
  out byte-identical to the shipped ones; a rerun is an 8 s no-op; editing
  `bri-weapons-import` plans only weapons, debris, runtime effects, item
  presentation, worlds and tutorial. Not run: macOS and Linux (WSL could not
  start its VM, and Maxwell deferred those platforms).
- 2026-09-27 Build output and a shared compile cache. C: was 99% full: 1.56 TB
  of cargo `target/` folders across ~50 worktrees. `tools/clean_targets.py`
  (dry run by default, `--apply`, `--keep`) deleted 35 finished worktrees'
  folders and freed 981 GB (C: 50 GB -> 909 GB free); it spares the main
  checkout, locked folders and ones active in the last 30 minutes. sccache
  0.18 is installed and set machine-wide (`~/.cargo/config.toml`,
  40 GiB cap). Measured: a second checkout at a different path gets 8 of 11
  cache hits building `bri-content` (the misses are workspace crates);
  setting `CARGO_TARGET_DIR` drops that to 0 because the variable is hashed.

- 2026-09-27 Brick owners follow the player's identity (door-closer P0).
  Brick owner numbers were per-session counters; a saved build carried an
  opaque `ownership_scope`, and after a restart returning players were new
  numbers who no longer owned their builds. The world now carries an owner
  table (`World.owners`: number to principal and last name), joins map a
  known principal back to its number (a second live connection of the same
  principal gets a fresh number), the build format drops `ownership_scope`
  (`SavedBuild` schema 2), and loading gives each recorded builder's bricks
  to that principal's number on the target server. Unclaimed owners are
  still remapped to fresh numbers. Trust levels now include offline owners
  from the table. World saves without owners serialize unchanged, so the
  generated packs are unaffected. Evidence: `cargo test -p bri-world -p
  bri-sim -p bri-net` (new `returning_players_get_their_bricks_back_after_a_restart`).
- 2026-09-27 breakable map glass (protocol 29), re-ported from `breakables`
  (ab6a16a) onto the streamed world transfer. v20's only breakable map
  objects are the `Glass`-class StaticShapes: `glassA` (the 4 Bedroom and
  BedroomDark windows), `lightBulbA` (the Bedroom lamp bulb) and
  `fluorescentLight` (the 4 Kitchen and KitchenDark ceiling lights). The 7
  Kitchen windows are `glassA` with `indestructable = "1"` and never break.
  Rule (`Armor::onImpact`): any player collision whose speed into the surface
  exceeds `minImpactSpeed` 30 calls `StaticShape::explode` and skips that
  impact's falling damage (`Player::updatePos` 0x5B18DD gates it, server
  only). Corpses count; projectiles, explosions, the hammer and vehicles do
  not. `explode` plays `glassExplosion` at object-box center + position,
  stops drawing the shape (`renderWhenDestroyed = 0`), plays
  `explosionSound` (only `glassA` has one) and hides it 100 ms later, which
  removes its collision. Nothing repairs it until the mission reloads.
  Implementation: `NativeMap::breakables`, per-collision hits from the motor,
  `session/breakables.rs`, `Checkpoint.broken_shapes` beside `world_bricks`
  and `Delta.broken_shapes`, client prediction/build-ray mirrors disable the
  colliders and `GpuScene::hide_indices` stops drawing it. Evidence:
  `bri-sim --test breakables`, `--test breakables_native -- --ignored` against
  map-bundle-016 (every Bedroom window and the bulb take a >30 impact from a
  thrown player), `bri-net --test replication`. The motor hits still come from
  Rapier contacts; they move to the Torque contacts when slides lands.
- 2026-09-27 Spike (c), importing community Add-Ons. The new `bri-import-addon`
  (`crates/addon-import`) takes one Add-On zip or folder and writes a package
  directory: `package.json`, native weapons/vehicles/bricks data, converted
  DTS and BLB files, copied textures and sounds, and `import-report.json` plus
  `IMPORT-REPORT.md`. The report covers assets, datablocks, ids in
  `namespace:kind/name`, dependencies, unsupported and ambiguous findings, and
  "needs behaviour" entries (hook, operations with lines, capabilities present
  and missing). Nothing is executed. Supporting changes:
  - `bri_convert::tscript`: a static reader for datablocks, functions, packages
    and calls.
  - `bri_vehicles_import::lower`: vehicle lowering is now a library function;
    the vanilla output is byte-identical (`diff -r` against `maps-pass-007`).
  - `catalog::read_with_parents`.
  - `bri_vehicles::Pack::validate` accepts namespaced vehicle ids.
  Evidence: the Sawn-off Shotgun (Ephialtes), Blocko Car (Kaje) and Bot_Zombie
  (Rotondo) import against the E: reference. The imported shotgun fires in
  `WeaponsWorld` and the imported car drives in `VehiclesWorld`, both headless.
  253 of 255 archive zips import (the other two are a non-archive and a
  member-budget refusal). Tests: `cargo test -p bri-addon-import` runs a CC0
  synthetic fixture everywhere and the real samples where the archive exists.
  Open: a hosted game cannot load an imported package yet, because each system
  reads one pack per role. That seam and 19 others are in
  `docs/audits/spike-addon-import.md`.

- 2026-09-27 Content loads through `packages.json`; joins name differing
  packages (door-closer P0, protocol 31). `ContentConfig` (18 fixed pack
  fields and `client-content.json`) is gone: the client and `bri-server` read
  `packages.json` from the content root, falling back to
  `crates/package/base-packages.json`, and resolve each engine role to its
  package directory. `bri-server` now takes `<content-root> <world.json>
  <state-dir> <listen> [seconds]`. Hosting and joining hash every listed
  package into an environment; `Hello` carries the client's shared and client
  packages instead of one opaque content id, and the server refuses a join
  whose shared packages differ with a message naming each one (server has X,
  you have Y / you do not / the server does not). Client-only differences
  join and are told in chat. The fingerprint chain in `content_identity.rs`
  (and its tests) is removed; the weapon and item-physics startup snapshots
  stay. The regeneration script, both packagers and the launchers use the
  package list. Measured: the base environment is 333 MiB over 18 packages;
  `bri-server` on the real content ran a 2 s smoke and published it.
  Evidence: `cargo test -p bri-package -p bri-net -p bri-world -p bri-sim`,
  new loopback `join_refusal_names_each_differing_shared_package`,
  `tools/tests/Test-PlaytestPackaging.ps1`, `Test-PlaytestLauncher.ps1`.

- 2026-09-28 Client sandbox: Add-Ons may send joining players sandboxed code
  (Maxwell's decision; principle 10 is now trust tiers: data without asking,
  sandboxed WebAssembly and WGSL after a per-server trust prompt, elevated
  capabilities after a separate per-Add-On choice; native plugins are tier 3
  with a typed confirmation, designed but not built). Crate
  `bri-client-sandbox`: Wasmtime 45 with fuel, epoch deadlines and store limits;
  capability-gated host functions (render layer, shaders, audio, focused
  input, messages to the Add-On's server script); naga validation of Add-On
  WGSL with every loop rewritten to draw on one per-invocation allowance; a
  wgpu layer renderer; the trust prompt model and `addon-trust.json` store.
  Sample `packages/samples/spinning-cube` draws a cube with an animated
  shader. Red team round 1 found and fixed two issues: the wall-clock
  deadline stopped advancing once the `Sandbox` was dropped (an endless loop
  then ran forever), and modules shaped to compile slowly took 5 s (now
  bounded at load; worst allowed 0.5 to 0.7 s on 4 cores). Open: compiling
  on a worker thread with a cache. Design and findings: `docs/architecture/client-sandbox.md`.
  Evidence: `cargo test -p bri-client-sandbox` (23 tests; the two GPU tests
  are ignored without an adapter and passed on llvmpipe),
  `bri-addon-preview packages/samples/spinning-cube <out>`.
  In the client (`client_code.rs`): enabled Add-Ons' code starts when the
  player enters a game they host, and on other servers only for code
  `addon-trust.json` grants (the join-screen prompt is next, with PR #4);
  layers draw in the world's last pass. Not yet seen in a real session:
  the cloud container has no v20 content; verify on the PC.
  Evidence: `cargo test -p bri-client --lib client_code`.
  GPU budget fix after the verifier measured Maxwell's RTX 4070 SUPER (no
  reset, but the heaviest allowed shader took about 2.4 s a frame and the
  4 ms x 30-strike rule tolerated about 75 s of that). The loop allowance
  is now set per frame (`bri_frame.limits.x`): 16 until the GPU is measured,
  then fitted by `gpu::calibrate` (a small timed offscreen pass, once per
  device) to the GPU's speed, the screen size and the shader's cost, and
  halved after every frame whose timestamps show it over 4 ms. One frame
  over 100 ms stops the Add-On at once; 20 slow frames in a row stop it.
  Shaders whose helpers fan out (no loop, a million times the work) are
  refused by an expanded-cost limit. A device loss stops every Add-On's code
  until the next join. The client requests timestamp features where the GPU
  has them. On llvmpipe the endless-loop shader over 512x512 went from 0.96 s
  to 23, 5.8, then 4.6 ms a frame. Evidence: `cargo test -p
  bri-client-sandbox` (25 tests; the 4 ignored GPU tests passed on llvmpipe with
  `--ignored`), `cargo test -p bri-client --lib client_code` (4 tests),
  clippy `-D warnings`. Next: re-measure on the PC.
## 2026-09-28 Ramp slides: v20 player collision

- player collision is now v20's own `updatePos`/`findContact`/`step`
  (`crates/motor/src/torque.rs`, from the exe at 0x5B0714/0x5AA570/0x5A9FD0),
  replacing Rapier's character controller, which stood players still on the
  74.5 degree face of every "72 degree" ramp. Gravity always applies; slopes
  past runSurfaceAngle slide; the crease rule carries riders down V lanes.
  Per-tick epsilons are rescaled for 120 Hz (docs/player-simulation.md).
  Evidence on "Mr.Block's Slides": 524/524 ramp faces release the player;
  889/893 lane rides reach the end of their leg, one from the tower top to
  the ground (`cargo test --release -p bri-sim --test slides -- --ignored`).
  Open: canJump's post-ceiling-hit refusal and the hard-landing recover
  state need new PlayerState fields (protocol bump).

## 2026-09-28 Stress Lab: gameplay from packages (protocol 32)

- Package-defined gameplay seams: `bri-package-runtime` (mod package loading,
  Rhai sandbox, `ops::authorize` capability gate), `session/packages.rs`
  (package commands, namespaced durable state, entities, chunked world
  provider, `Session::explode`), `Checkpoint`/`Delta` `entities` and
  `package_state`, the UI's `hud.overlay` slot and package keys, client box
  models, hosting package worlds from Start Game. Design:
  `docs/architecture/package-runtime.md`.
- 2026-09-27 Hammer, wand and Destructo Wand rules match v20. The admin
  Destructo Wand (`/magicWand`) never worked: its 500-unit reach exceeded the
  150-unit brick-targeting limit, so every swing errored. Targeting now allows
  `Simulation::MAX_TARGET_DISTANCE` (2000). The hammer now follows
  `hammerImage::onHitObject`: it silently refuses any brick for which
  `willCauseChainKill` is true, before the trust check. Decoded from the exe
  (console method at 0x6df8f0 -> 0x540720, distance-to-ground invalidation at
  0x540180): true when removing the brick leaves any brick joined to it by
  studs, up or down, with no stud path to a grounded brick. A brick on top
  that is also held up elsewhere does not block the hammer.
  `Simulation::stranded_by`/`will_cause_chain_kill` implement this. The root
  test approximates v20's plant-time probe (ray from the brick top to 0.1
  below its bottom finds map floor, or terrain) until the placement audit's
  cached `grounded()` lands; the Tutorial's layouts sit 0.006 into the floor. `killBrick` (wands, admin wand, undo) now
  chain-kills: stranded bricks break with it, all at once rather than v20's
  staggered wave, and indestructible bricks are skipped. This is vanilla brick
  destruction, which the contract keeps. Undoing a plant that would strand
  bricks runs v20's `undoTrustCheck` (Full group trust with every up/down
  neighbour, no admin bypass). Not changed: `Actor::trusted`'s administrator
  bypass (v20's `getTrustLevel` has none, so v20 admins cannot hammer or wand
  strangers' bricks online) and `stackBL_ID` (the stack starter may hammer or
  wand bricks others placed on their stack). Evidence: `cargo test -p bri-sim`
  (`hammer_only_breaks_bricks_that_hold_nothing_up`,
  `hammer_breaks_a_brick_whose_load_is_still_held_up_elsewhere`,
  `wand_breaks_anywhere_and_the_stranded_bricks_above_die_with_it`,
  `undoing_a_plant_that_holds_up_untrusting_bricks_is_refused`,
  `admin_destructo_wand_breaks_bricks_from_afar`), `cargo test -p bri-client
  --lib tutorial_hammer -- --ignored` (every Tutorial layout brick can be
  hammered, top first) and `cargo test -p bri-client --test app_flow --
  --ignored`. app_flow now undoes twice: since the undo parity work, the first
  Ctrl+Z reverts the Letters/B print and the second breaks the plant, as in v20.
- The Stress Lab (`packages/stresslab`, five CC0 packages: generated world,
  creeper, creeper model, mining economy, miner HUD) uses only those seams.
- Evidence: `bri-package-runtime` tests, `bri-sim --test packages`,
  `bri-stresslab` loopback and workflow tests, `bri-client --test
  stresslab_flow -- --ignored` (offscreen, also against a packaged release),
  soak `docs/stress-lab/soak-8x120.json` (8 clients, 120 s, 0 dropped ticks,
  replicas agree). Handoff and labels: `docs/stress-lab/HANDOFF.md`.
- Open: join mismatch naming waits on door-closers' Hello wiring; see the
  handoff's next steps.

- 2026-09-27 Avatars are saved by part name (door-closer P0). `Appearance`
  stored parts as positions in the avatar pack's lists, and the settings
  file's `$pref::Avatar::*` values did the same, so adding or reordering a
  part changed everyone's avatar. Parts are now named (`hat: "helmet"`,
  `accent: "visor"`) in the wire `Appearance`, replicated avatars and the
  settings file; the editor maps names to its list positions only for
  display. v20 keeps an accent's position when the hat changes, and so do we.
  The avatar pack file is unchanged: its defaults stay positions in its own
  lists and are named on load. v20 prefs are named once on import. A saved
  part the pack no longer has falls back to the pack default. Protocol 33.
  Evidence: `cargo test -p bri-ui --lib avatar -- --include-ignored`,
  `cargo test -p bri-client --lib avatar -- --include-ignored`,
  `cargo test -p bri-net --test loopback avatar -- --include-ignored`.

- 2026-09-27 Missing brick definitions no longer refuse a whole world or
  build (door-closer P0). `Simulation::new` and `preflight_load` failed on the
  first brick with an unknown or unresolved definition, so a v20 save with
  one add-on brick, or a world after a package was removed, did not load at
  all. Such bricks now move to `World.unloaded`: not placed or replicated,
  kept exactly, saved again with the world and with builds, and offered again
  when a build is loaded on a server that has them. Loading says what it
  skipped ("1 bricks were not loaded because this server does not have their
  definitions: 1 missing. They are kept and saved with the world.") in chat,
  in map-load pending objects and in `bri-server`'s log. Geometry errors in
  placeable bricks still refuse the load atomically. No protocol change.
  Evidence: `cargo test -p bri-world -p bri-sim` (updated
  `build_load_keeps_unknown_bricks_aside_and_preserves_existing_players`).
- 2026-09-28 In-game Add-Ons manager, first slice (`docs/architecture/mod-manager.md`,
  player research in `docs/research/mod-manager-expectations.md`). Players see
  one word, "Add-Ons"; "package" stays internal. `bri_package::library` scans
  the content root: `packages.json` is the enabled list, a disabled package's
  exact entry moves to `packages-disabled.json`, and unlisted directories with
  a `package.json` are discovered as disabled. Enabling pulls in dependencies
  first, disabling takes dependents, base `v20-*` packages stay on, and
  refusals (missing or wrong-version dependency, newer API, role conflict,
  unreadable manifest, missing folder) are named diagnostics. The main menu
  gains an Add-Ons button opening a native dialog (grouped list, search,
  details with what it adds, where it runs, what it needs and what it may do,
  Enabled box, Defaults). A native join screen shows a server's missing
  add-ons with byte progress and Cancel
  (`ConnectionState::DownloadingPackages`). No wire change. Evidence:
  `cargo test -p bri-package library`, `cargo test -p bri-ui --lib addons`,
  `cargo test -p bri-client --lib add_ons`, all content-free. Open: toggles
  take effect once multi-pack loading and door-closers' join wiring read
  `packages.json`; the join screen needs PR #1's `fetch_missing` call.
- 2026-09-28 Content from several packages (steps a to e of the multi-package
  proposal in `docs/audits/spike-addon-import.md`).
  - Weapons (with item presentation and drop bounds), vehicles and brick
    catalogs merge from every role-less package in `packages.json` that
    provides them, onto the base packages.
  - This happens in the dedicated host (`bri_net::dedicated`, now shared with
    `bri-server`), in a client-hosted game and in joining clients.
  - `bri-import-addon` writes the presentation, drop bounds and a loadable
    brick catalog, and declares the new runtime kinds `weapons`, `vehicles`
    and `bricks`.
  - Without extra packages, loading and the weapon fingerprint are unchanged.
  Evidence:
  - `crates/addon-import/tests/hosted.rs`: the real Sawn-off Shotgun and
    Blocko Car hosted beside the base game on Slate. The shotgun and the
    vanilla gun both fire, and a player mounts the spawned car and drives it
    more than 5 units. An imported brick loads into a hosted world.
  - `crates/client/tests/addon_packages.rs`: the client-side load of the same
    packages, including the shotgun model and the car assets.
  Imported bricks also show in the brick menu under the category they
  declare, with icons the importer stores in `brick-catalog/brick-icons.json`
  (`crates/client/tests/addon_packages.rs`). Open: sounds and effects, and
  Maxwell's interactive playtest.
- 2026-09-28 Add-Ons: Import. Old Blockland zips or folders dropped into
  `content/Add-Ons/` show under "Not Imported Yet" with an Import button; the
  client runs `bri-import-addon` as a separate program into
  `content/addons/<name>`, then the new add-on is listed (off) and turns on
  like any other. Evidence: library, UI and client unit tests, plus a manual
  end-to-end import of the CC0 `Weapon_Synthetic_Blaster` fixture that ended
  with it in `packages.json`. Open: packaging must ship
  `bri-import-addon.exe` next to the client.
## 2026-09-28 Game modes in Start Game

Max asked for easy in-game Add-On management; the coordinator scheduled the
game mode picker once the Stress Lab landed (da5668e).

- New server content kind `mode` (`bri_package_runtime::content::GameMode`):
  name, description, optional map, `add_ons`. Checks `set.mode.add_on`,
  `set.mode.map`; `Catalog::for_mode`, `Catalog::for_world` pick what a hosted
  game runs, and the one-world check moved from load to hosting, so several
  world Add-Ons can be turned on together.
- UI: Start Game gets a **Mode:** button above Start opening the Game Mode
  screen (`crates/ui/src/screens/modes.rs`); `UiUpdate::GameModes`,
  `HostGame.game_mode`. Custom keeps today's behaviour.
- Client: `packages::hosted` resolves mode, map, base map and save key;
  `packages::modes` feeds the list.
- `packages/stresslab/stresslab-mode` adds the Stress Lab mode (added to
  `bri_stresslab::PACKAGES`).
- Evidence: `cargo test -p bri-package-runtime -p bri-ui -p bri-stresslab`,
  `-p bri-sim --test packages`, `-p bri-client --lib` all pass; clippy
  `-D warnings` clean on those crates. New tests:
  `the_stress_lab_mode_runs_its_world_and_rules`,
  `a_package_world_without_a_mode_runs_every_add_on_that_fits_it`,
  `a_mode_may_only_run_add_ons_its_package_depends_on`,
  `hosting_runs_the_chosen_mode_or_the_plain_base_game`,
  `start_game_hosts_the_chosen_game_mode_on_its_map`.
- Not interactively checked; Max's playtest covers the screen's look.
- Follow-up: the Add-Ons screen (PR #4) must list `mode` as a server kind.
## 2026-09-28 — Add-On author guide and samples

- `docs/modding/README.md`: the Add-On author guide (manifest, sides,
  scripts, state, capabilities, content kinds, HUD panels, weapons, headless
  testing, importing v20 Add-Ons). It describes only what is on `main` and
  lists in-flight work (multi-pack loading, state visibility, the `players`
  capability, player archetypes, sandboxed client code with its trust
  prompt, downloads on join) as coming soon.
- `packages/samples/`: Survival Points (server rule: timer, public state,
  a cooldown command, an admin-only command, chat), its HUD panel, and the
  Bubble Blaster weapon (hand-written in the Import Add-On weapons format).
- Evidence: `cargo test -p bri-package-runtime --test samples` (loads on
  server and client, compiles, HUD binds only public keys and real
  commands), `cargo test -p bri-sim --test samples` (a session awards
  points to living players, greets on join, runs the leaderboard, refuses
  a non-admin reset and allows the host's), `cargo test -p bri-weapons
  --test sample_addon` (the bubble fires, harmless and shoving).
## 2026-09-28 — first-impressions audit and three robustness fixes

- `docs/audits/first-impressions.md` ranks 20 things a new player would find
  missing, rough or fragile on main `1599ef2`, with evidence for each.
- Settings: a damaged or older `settings.json` no longer stops startup. Missing
  fields take defaults; a damaged file is copied to
  `settings.damaged-<time>.json`, readable sections are kept, and the player is
  told in plain words (`settings::recover`).
- Saves: one unreadable save no longer empties the Save and Load lists. It is
  listed as damaged (`SaveFileInfo::damaged`) and can be saved over.
- Messages: the Connection Failed dialog explains transport and join failures
  in plain words (`bri_ui::models::disconnect::explain`); kicks and bans close
  with the reason and ban length, and a banned rejoin is told how long is left.
  No protocol change: the close reason is free text.
- Evidence: `cargo test -p bri-ui --lib`, `-p bri-sim --lib --test session`,
  `-p bri-client --lib`, `-p bri-net --lib`; clippy `-D warnings` on those
  four crates with `--all-targets`.

## 2026-09-28 — autosave and unsaved-changes prompt for hosted games

- Single-player, LAN and Internet games hosted from the client autosave every
  60 s into the map's own save folder (`autosave-<unix ms>.world.json`, newest
  three kept), so they appear in Load Bricks as "Autosave" with their date. An
  interval with no world change writes nothing, so idle play never pushes out
  older autosaves (`saves::Store::autosaver`).
- When a hosted game ends (Disconnect, Quit, the window's close button, a
  crash of the host loop) its final world is kept the same way. Quitting waits
  up to 15 s for the host to stop and write it (`App` drop, `Worker::finish`).
- Leaving or quitting a hosted game whose world changed since it was last
  saved under a name asks first ("Unsaved Changes"); loads and map changes do
  not count until they settle. Closing the window asks too; a second close
  quits.
- The server-side mechanism is PR #1's `ServerOptions::autosave` /
  `server::Autosave` and `bri_world::persistence::autosave`, ported unchanged
  (plus `autosave_bytes` and `is_autosave`) so PR #1 can drop its copy.
  `bri-server` autosaves as PR #1 had it.
- Evidence: `cargo test -p bri-world --lib`, `-p bri-net --lib --test
  loopback`, `-p bri-ui --lib --tests`, `-p bri-client --lib --test transport`,
  `-p bri-sim --lib --test session`; clippy `-D warnings --all-targets` on those
  crates. New tests: `hosted_games_autosave_changes_into_the_load_list`,
  `a_host_autosaves_on_its_timer_and_returns_its_final_world`,
  `leaving_a_host_with_unsaved_changes_asks_about_them_first`, and the
  transport test now checks the final world is kept.
## 2026-09-28 — modplatform package draft folded into main

- Reviewed the uncommitted modplatform `bri-package` draft (PR #6,
  `wip/modplatform-package/`). Main already covers its id grammar,
  diagnostics and environment. Its archive, store and luau kinds are
  superseded by directory hashing, the package sync cache and the Rhai
  runtime. Kept:
  - `bri_package::capability`: the one capability list, with the plain
    words players read. `ops::CAPABILITIES` re-exports it.
  - Strict manifests: unknown `package.json` fields are errors.
  - `bri-addon-check <folder> [--json]`: checks an Add-On and its
    dependencies found beside it the way the game loads them, and prints
    the side, provides, capabilities and needs.
- Evidence: `cargo test -p bri-package-runtime --test check` (a HUD
  checked with its rules; a misspelt field, a missing dependency, a private
  HUD binding and a script syntax error are each named; sides follow
  kinds), plus the bri-package, bri-package-runtime and bri-addon-import
  tests and clippy `-D warnings`.
## 2026-09-28 Settings players expect

- Options gains what players look for today, on top of v20's rebinding,
  sensitivity, resolution, fullscreen and volumes. Graphics: a Quality menu
  (Low, Medium, High, Ultra; Custom while hand-set values match none) that
  sets shadows, anti-aliasing, brick shadows, anisotropy and precipitation,
  and a Max FPS menu (30 to 240 or Unlimited, `$pref::Video::MaxFps`,
  default Unlimited). High equals the renderer's defaults, so new players
  see High. Audio: Shell and Sim volumes read Interface and Effects, a new
  Music volume (`$pref::Audio::musicVolume`, the runtime's existing music
  bus), live volume preview while dragging (undone if the dialog closes
  without Done), and Mute when in background
  (`$pref::Audio::MuteInBackground`, default off). Every slider shows its
  value (percent, sensitivity, anisotropy as Off/2x..16x, FOV degrees).
- The frame cap paces the focused loop by deadline (`PlatformCommand::
  FrameLimit`, `PlatformConfig::max_fps`); without it the loop still runs
  flat out, paced by VSync. Console cvars `maxfps`, `musicvolume`,
  `mutebackground`.
- Fix: with no saved anisotropy the slider showed 0 while the renderer drew
  8x; it now shows the renderer's value, and Done always stores the preset
  options so a stock default the renderer ignores cannot hide a choice.
- Evidence: `cargo test -p bri-ui --lib` (new options tests for presets,
  Max FPS, music volume and readouts, mute), `cargo test -p bri-client
  --lib`, clippy `-D warnings` on both. Not yet seen on the authored layout:
  the new rows are placed relative to Resolution and the Sim volume row;
  needs the offscreen options render on a PC with content.

- 2026-09-28 Modern hosting (first-impressions item 5, Max's "make hosting
  first class"). Joining needs only the game port: door-closers' first-use
  pinning, host names, typed join errors and remembered address (from
  859c012, item 5 parts only) plus `bri://host:port/<key>` invites whose key
  (first 128 bits of the certificate's SHA-256) verifies a first join. One TLS
  verifier handles every `HostPin`. Protocol 34: `Challenge` carries the
  server `Listing`, so `client::probe` reads name, map and players over the
  game port without joining; a client on another version is told which side
  must update (that refusal was dropped unsent before). Internet hosts and
  non-loopback dedicated servers run `reach::open_and_check`: UPnP IGD, then
  NAT-PMP (RFC 6886), the public address as the router reports it (no
  outside service is contacted), a self-probe of that address, and one plain verdict (reachable, likely, shared address,
  needs forward, unknown) with the invite put on the clipboard and `/invite`
  to copy it again. UDP 28050 is no longer forwarded. Windows hosts read the
  firewall rules for the game (PowerShell NetSecurity, per active profile) and
  offer a one-prompt fix (`bri-client --allow-firewall <port>`, elevated,
  removes the program's inbound rules and keeps one allow rule for the game
  and discovery UDP ports, so new build folders need no second prompt). Hosts
  whose router gives no public address still get a home network invite. Join Server searches the LAN
  and probes saved servers on open; Query Internet became Favorite
  (`servers.json`: 64 favourites, 10 recent). Join codes with hole punching
  and a relay were built and tested against simulated routers, then shelved
  because they need a hosted service (Max: direct IP only, no relay, no
  third-party services); the patch is kept outside the repository. Evidence: `cargo test -p bri-net --lib`
  (invite, natpmp against a fake router, reach verdicts), `--test
  loopback` (`invites_pin_the_host_key_and_probes_read_the_listing`,
  `a_different_version_is_told_which_side_to_update`,
  `first_join_needs_only_the_game_port_and_errors_are_plain`), `cargo test -p
  bri-client --lib` (servers, firewall decisions), `cargo test -p bri-ui`
  (Favorite button, Confirm dialog, list query on open), clippy `-D warnings`
  on Linux and `--target x86_64-pc-windows-gnu`. Not yet exercised: a real
  router, the Windows Firewall helper on
  Windows, and a remote friend joining.

- 2026-09-28 Rejoining keeps your bricks; a dropped connection rejoins
  (first-impressions item 6). A player who left and joined the same running
  game again got a new owner number, because the returning check skipped
  numbers still held by dropped connections, so their own bricks were no
  longer theirs. A fresh join by the same principal now takes the dropped
  number back and replaces the stale resume entry. When the network drops
  (QUIC timeout or reset), the client's close reason is
  `bri_net::client::CONNECTION_LOST` instead of the server's close frame, and
  a joined remote game rejoins the same address automatically, up to three
  times ("Connection lost. Reconnecting to ..."), before the failure dialog
  shows; kicks, shutdowns and other closes the server chose keep their
  message and are not rejoined. Evidence: `cargo test -p bri-sim --test
  session` (`leaving_and_rejoining_keeps_the_same_owner_number`), `cargo test
  -p bri-net`, clippy on bri-net, bri-client and bri-sim. Not tested against
  a real network drop.
## 2026-09-28: stress campaign merged onto the combined landing (protocol 35)

- PR #1 merged main 47dcf2a (Add-Ons tab, game modes, first impressions,
  settings, hosting, client sandbox, slides). Protocol 35: hosting's
  `Challenge` listing is 34.
- The campaign's autosave copy is gone; `bri-server` and the windowed host
  use main's (#5).
- Joins keep hosting's `HostPin`s and `JoinError`s; `connect_fetching`
  takes the join's pin and downloads over `client::connect_quic`, so a
  download reaches the host the join trusts. `PackagesDiffer` prints as
  `environment::refusal`, which the Add-Ons screen parses into rows.
- `JoinBegin.purpose` defaults to a join, so an older client still hears
  which side must update.
- Admin disconnects keep the campaign's publish-before-reply order and
  carry #5's close messages.
- The `player` capability has plain words on the Add-Ons screen. The
  survival-points sample and the modding guide use `visible` instead of
  `public`; the guide lists archetypes, textures, blocks and the `player`
  operations.
- `bri-client-sandbox` moved from the Windows-only dependency table to the
  client's dependencies (the client did not build on other targets).
- E22's moon jump now settles 120 ticks first: under main's v20 contact
  port a player spawned 5 cm up is still airborne after 10 ticks, so the
  jump was ignored.
- Evidence: clippy `-D warnings` on the workspace; `cargo test` for bri-net,
  bri-package, bri-package-runtime, bri-world, bri-progress, bri-stresslab,
  bri-sim (content-free targets; `tools` needs generated content) and
  `bri-client --lib --test transport`.
